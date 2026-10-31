//! Model groups. A group type (a family type in the Model Groups
//! category) has any number of instances; each instance owns copies of
//! the member elements, placed at its origin and rotation. Editing one
//! instance and finishing the edit copies the change to every other
//! instance of the type, as in Revit.

use crate::category::Category;
use crate::document::{Document, ModelError};
use crate::element::ElementData;
use crate::geometry::element_geometry;
use hausit_core::{DVec2, ElementId};
use serde::{Deserialize, Serialize};

/// Family name of model group types.
pub const MODEL_GROUP: &str = "Model Group";

/// An instance of a model group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelGroup {
    pub type_id: ElementId,
    /// The level it is placed on.
    pub level: ElementId,
    /// The group origin in plan.
    pub origin: DVec2,
    /// Rotation in degrees, counter-clockwise, relative to the type.
    #[serde(default)]
    pub rotation: f64,
    pub members: Vec<ElementId>,
}

/// Categories that cannot go in a model group.
fn groupable(cat: Category) -> bool {
    cat.is_model() && cat != Category::ModelGroups || matches!(cat, Category::Rooms | Category::RoomSeparation)
}

impl Document {
    pub fn group(&self, id: ElementId) -> Option<&ModelGroup> {
        match &self.element(id)?.data {
            ElementData::Group(g) => Some(g),
            _ => None,
        }
    }

    /// The group an element belongs to.
    pub fn group_of(&self, id: ElementId) -> Option<ElementId> {
        self.index().group_of(id)
    }

    /// `ids` with every group's members added.
    pub fn with_group_members(&self, ids: &[ElementId]) -> Vec<ElementId> {
        let mut out = ids.to_vec();
        for id in ids {
            if let Some(g) = self.group(*id) {
                out.extend(g.members.iter().copied().filter(|m| !ids.contains(m)));
            }
        }
        out
    }

    /// Instances of a group type.
    pub fn group_instances(&self, type_id: ElementId) -> Vec<ElementId> {
        self.elements_of(Category::ModelGroups)
            .filter(|e| matches!(&e.data, ElementData::Group(g) if g.type_id == type_id))
            .map(|e| e.id)
            .collect()
    }

    /// Create Group (GP): groups model elements under a new group type.
    /// Doors and windows in grouped walls come along.
    pub fn create_group(&mut self, ids: &[ElementId], name: &str) -> Result<ElementId, ModelError> {
        let mut members: Vec<ElementId> = ids
            .iter()
            .copied()
            .filter(|id| self.element(*id).is_some_and(|e| groupable(e.category())))
            .collect();
        for id in members.clone() {
            members.extend(self.openings_in(id).map(|(e, _)| e.id).filter(|o| !ids.contains(o)));
        }
        if members.is_empty() {
            return Err(ModelError::Rejected("Select model elements to group.".into()));
        }
        if members.iter().any(|m| self.group_of(*m).is_some()) {
            return Err(ModelError::Rejected(
                "An element can only be in one group. Ungroup it first.".into(),
            ));
        }
        if self.types_of(Category::ModelGroups).any(|t| t.name == name) {
            return Err(ModelError::Rejected(format!("A group named \"{name}\" already exists.")));
        }
        let origin = self.members_center(&members);
        let level = self
            .common_level(&members)
            .unwrap_or_else(|| self.levels().first().map(|l| l.0).unwrap_or_default());
        let type_id = self.add_type(Category::ModelGroups, MODEL_GROUP, name, Default::default());
        Ok(self.add_element(ElementData::Group(ModelGroup {
            type_id,
            level,
            origin,
            rotation: 0.0,
            members,
        })))
    }

    /// Next free group name ("Group 1", "Group 2", …).
    pub fn next_group_name(&self) -> String {
        (1..)
            .map(|i| format!("Group {i}"))
            .find(|n| !self.types_of(Category::ModelGroups).any(|t| t.name == *n))
            .unwrap()
    }

    /// The plan centre of elements' geometry (a group's origin).
    fn members_center(&self, ids: &[ElementId]) -> DVec2 {
        let mut lo = DVec2::splat(f64::MAX);
        let mut hi = DVec2::splat(f64::MIN);
        for id in ids {
            if let Some(g) = element_geometry(self, *id) {
                let b = g.bounds();
                lo = lo.min(b.min.truncate());
                hi = hi.max(b.max.truncate());
            }
        }
        if lo.x > hi.x {
            DVec2::ZERO
        } else {
            ((lo + hi) / 2.0).round()
        }
    }

    /// Ungroup: the members stay as ordinary elements.
    pub fn ungroup(&mut self, id: ElementId) -> Vec<ElementId> {
        let members = self.group(id).map(|g| g.members.clone()).unwrap_or_default();
        self.remove_element_only(id);
        members
    }

    /// Places another instance of a group type with its origin at `at`.
    pub fn place_group(&mut self, type_id: ElementId, at: DVec2) -> Result<ElementId, ModelError> {
        let template = self
            .group_instances(type_id)
            .first()
            .and_then(|i| self.group(*i).cloned())
            .ok_or_else(|| ModelError::Rejected("This group has no instance left to copy.".into()))?;
        let members = self.copy(&template.members, at - template.origin);
        Ok(self.add_element(ElementData::Group(ModelGroup {
            members,
            origin: at,
            ..template
        })))
    }

    /// Finish editing a group: every other instance of its type is
    /// rebuilt from this one's members, at its own origin and rotation.
    pub fn sync_group(&mut self, id: ElementId) {
        let Some(src) = self.group(id).cloned() else { return };
        // Members deleted while editing drop out.
        let alive: Vec<ElementId> = src.members.iter().copied().filter(|m| self.element(*m).is_some()).collect();
        if let Some(ElementData::Group(g)) = self.element_mut(id).map(|e| &mut e.data) {
            g.members = alive.clone();
        }
        for other in self.group_instances(src.type_id) {
            if other == id {
                continue;
            }
            let Some(g) = self.group(other).cloned() else { continue };
            let old: Vec<ElementId> = g.members.iter().copied().filter(|m| self.element(*m).is_some()).collect();
            for m in old {
                self.remove_element_only(m);
            }
            let new = self.copy(&alive, g.origin - src.origin);
            let turn = g.rotation - src.rotation;
            if turn.abs() > 1e-9 {
                self.rotate_members(&new, g.origin, turn);
            }
            if let Some(ElementData::Group(g)) = self.element_mut(other).map(|e| &mut e.data) {
                g.members = new;
            }
        }
        self.touch();
    }

    /// Adds elements to a group (Edit Group > Add).
    pub fn add_to_group(&mut self, id: ElementId, ids: &[ElementId]) -> Result<(), ModelError> {
        let add: Vec<ElementId> = ids
            .iter()
            .copied()
            .filter(|i| *i != id && self.group_of(*i).is_none() && self.element(*i).is_some_and(|e| groupable(e.category())))
            .collect();
        let Some(ElementData::Group(g)) = self.element_mut(id).map(|e| &mut e.data) else {
            return Err(ModelError::Rejected("Not a group.".into()));
        };
        g.members.extend(add);
        self.touch();
        Ok(())
    }

    /// Removes elements from a group (Edit Group > Remove); they stay in
    /// the model.
    pub fn remove_from_group(&mut self, id: ElementId, ids: &[ElementId]) {
        if let Some(ElementData::Group(g)) = self.element_mut(id).map(|e| &mut e.data) {
            g.members.retain(|m| !ids.contains(m));
        }
        self.touch();
    }

    fn rotate_members(&mut self, ids: &[ElementId], center: DVec2, degrees: f64) {
        self.rotate(ids, center, degrees);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::sample_project;

    #[test]
    fn groups_copy_and_sync() {
        let mut doc = sample_project();
        let furniture: Vec<ElementId> = doc.elements_of(Category::Furniture).map(|e| e.id).take(2).collect();
        assert_eq!(furniture.len(), 2);
        let name = doc.next_group_name();
        let g = doc.create_group(&furniture, &name).unwrap();
        assert_eq!(doc.group_of(furniture[0]), Some(g));
        // Copying the group copies its members into a new instance.
        let copies = doc.copy(&[g], DVec2::new(20_000.0, 0.0));
        assert_eq!(copies.len(), 1);
        let g2 = copies[0];
        let t = doc.group(g).unwrap().type_id;
        assert_eq!(doc.group(g2).unwrap().type_id, t);
        assert_eq!(doc.group(g2).unwrap().members.len(), 2);
        assert_eq!(doc.group_instances(t).len(), 2);
        // Moving a group moves its members.
        let m = doc.group(g2).unwrap().members[0];
        let before = crate::geometry::element_geometry(&doc, m).unwrap().bounds().min;
        doc.translate(&[g2], DVec2::new(0.0, 1000.0));
        let after = crate::geometry::element_geometry(&doc, m).unwrap().bounds().min;
        assert!((after.y - before.y - 1000.0).abs() < 1e-6);
        // Edit the first instance (delete a member), finish: the copy follows.
        doc.delete(&[furniture[1]]).unwrap();
        doc.sync_group(g);
        assert_eq!(doc.group(g2).unwrap().members.len(), 1);
        // Deleting a group deletes its members; ungrouping keeps them.
        let kept = doc.group(g).unwrap().members.clone();
        doc.ungroup(g);
        assert!(doc.element(kept[0]).is_some() && doc.group_of(kept[0]).is_none());
        let gone = doc.group(g2).unwrap().members.clone();
        doc.delete(&[g2]).unwrap();
        assert!(doc.element(gone[0]).is_none());
    }
}
