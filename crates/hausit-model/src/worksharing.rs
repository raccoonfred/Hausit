//! Worksets and design options.
//!
//! Worksets split a project's elements into named groups that can be
//! closed (unloaded) and shown or hidden per view, as in Revit once
//! worksharing is enabled. New elements go on the active workset.
//!
//! Design options hold alternatives: an option set has options, one of
//! them primary. Elements in an option show only in views that display
//! that option (by default the primary one, or the option being edited).
//! Accepting the primary merges it into the main model and deletes the
//! other options.

use crate::category::Category;
use crate::document::{Document, ModelError};
use crate::view::View;
use hausit_core::ElementId;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Name of the workset levels and grids go on.
pub const SHARED_LEVELS_AND_GRIDS: &str = "Shared Levels and Grids";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workset {
    pub id: ElementId,
    pub name: String,
    /// Closed worksets are not loaded: their elements show nowhere.
    #[serde(default = "yes")]
    pub open: bool,
    /// "Visible in all views": the default for views that use the global
    /// setting.
    #[serde(default = "yes")]
    pub visible_by_default: bool,
}

fn yes() -> bool {
    true
}

/// How a view shows a workset (Visibility/Graphics ▸ Worksets).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorksetVisibility {
    #[default]
    UseGlobal,
    Show,
    Hide,
}

impl WorksetVisibility {
    pub const ALL: [WorksetVisibility; 3] = [Self::UseGlobal, Self::Show, Self::Hide];

    pub fn label(self) -> &'static str {
        match self {
            Self::UseGlobal => "Using Global Setting",
            Self::Show => "Show",
            Self::Hide => "Hide",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesignOption {
    pub id: ElementId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptionSet {
    pub id: ElementId,
    pub name: String,
    pub options: Vec<DesignOption>,
    pub primary: ElementId,
}

impl Document {
    pub fn worksharing(&self) -> bool {
        !self.worksets.is_empty()
    }

    pub fn workset(&self, id: ElementId) -> Option<&Workset> {
        self.worksets.iter().find(|w| w.id == id)
    }

    pub fn workset_name(&self, id: Option<ElementId>) -> String {
        id.and_then(|w| self.workset(w)).map(|w| w.name.clone()).unwrap_or_default()
    }

    /// Enables worksharing: levels and grids go on "Shared Levels and
    /// Grids", everything else on "Workset1", which becomes active.
    pub fn enable_worksharing(&mut self) -> Result<(), ModelError> {
        if self.worksharing() {
            return Err(ModelError::Rejected("Worksharing is already enabled.".into()));
        }
        let shared = self.new_id();
        let main = self.new_id();
        self.worksets = vec![
            Workset {
                id: shared,
                name: SHARED_LEVELS_AND_GRIDS.into(),
                open: true,
                visible_by_default: true,
            },
            Workset {
                id: main,
                name: "Workset1".into(),
                open: true,
                visible_by_default: true,
            },
        ];
        let ids: Vec<ElementId> = self.elements().map(|e| e.id).collect();
        for id in ids {
            if let Some(e) = self.element_mut(id) {
                let datum = matches!(e.category(), Category::Levels | Category::Grids);
                e.workset = Some(if datum { shared } else { main });
            }
        }
        self.active_workset = Some(main);
        Ok(())
    }

    pub fn add_workset(&mut self, name: &str) -> Result<ElementId, ModelError> {
        if !self.worksharing() {
            return Err(ModelError::Rejected("Enable worksharing first.".into()));
        }
        let name = name.trim();
        if name.is_empty() || self.worksets.iter().any(|w| w.name == name) {
            return Err(ModelError::Rejected("Workset names must be unique and not empty.".into()));
        }
        let id = self.new_id();
        self.worksets.push(Workset {
            id,
            name: name.into(),
            open: true,
            visible_by_default: true,
        });
        Ok(id)
    }

    /// Deletes a workset, moving its elements to `move_to`.
    pub fn delete_workset(&mut self, id: ElementId, move_to: ElementId) -> Result<(), ModelError> {
        if id == move_to || self.workset(move_to).is_none() {
            return Err(ModelError::Rejected("Pick another workset to move the elements to.".into()));
        }
        if self.workset(id).is_some_and(|w| w.name == SHARED_LEVELS_AND_GRIDS) {
            return Err(ModelError::Rejected("Shared Levels and Grids can't be deleted.".into()));
        }
        let ids: Vec<ElementId> = self.elements().filter(|e| e.workset == Some(id)).map(|e| e.id).collect();
        for e in ids {
            if let Some(e) = self.element_mut(e) {
                e.workset = Some(move_to);
            }
        }
        self.worksets.retain(|w| w.id != id);
        if self.active_workset == Some(id) {
            self.active_workset = Some(move_to);
        }
        for v in self.views().map(|v| v.id).collect::<Vec<_>>() {
            if let Some(v) = self.view_mut(v) {
                v.workset_visibility.retain(|(w, _)| *w != id);
            }
        }
        Ok(())
    }

    /// Whether a workset shows in a view.
    pub fn workset_visible(&self, view: &View, id: ElementId) -> bool {
        let Some(w) = self.workset(id) else { return true };
        if !w.open {
            return false;
        }
        match view
            .workset_visibility
            .iter()
            .find(|(x, _)| *x == id)
            .map(|v| v.1)
            .unwrap_or_default()
        {
            WorksetVisibility::UseGlobal => w.visible_by_default,
            WorksetVisibility::Show => true,
            WorksetVisibility::Hide => false,
        }
    }

    // ----- design options -------------------------------------------------

    pub fn option_set(&self, id: ElementId) -> Option<&OptionSet> {
        self.option_sets.iter().find(|s| s.id == id)
    }

    /// The set an option belongs to.
    pub fn set_of_option(&self, option: ElementId) -> Option<&OptionSet> {
        self.option_sets.iter().find(|s| s.options.iter().any(|o| o.id == option))
    }

    /// "Garage : Option 1 (primary)".
    pub fn option_label(&self, option: Option<ElementId>) -> String {
        let Some(o) = option else { return "Main Model".into() };
        let Some(set) = self.set_of_option(o) else {
            return "Main Model".into();
        };
        let name = set.options.iter().find(|x| x.id == o).map(|x| x.name.as_str()).unwrap_or("");
        let primary = if set.primary == o { " (primary)" } else { "" };
        format!("{} : {}{}", set.name, name, primary)
    }

    /// Adds an option set with its primary option.
    pub fn add_option_set(&mut self, name: &str) -> (ElementId, ElementId) {
        let id = self.new_id();
        let primary = self.new_id();
        self.option_sets.push(OptionSet {
            id,
            name: name.into(),
            options: vec![DesignOption {
                id: primary,
                name: "Option 1".into(),
            }],
            primary,
        });
        (id, primary)
    }

    pub fn add_option(&mut self, set: ElementId) -> Result<ElementId, ModelError> {
        let id = self.new_id();
        let s = self
            .option_sets
            .iter_mut()
            .find(|s| s.id == set)
            .ok_or_else(|| ModelError::Rejected("No such option set.".into()))?;
        let n = (1..)
            .find(|n| !s.options.iter().any(|o| o.name == format!("Option {n}")))
            .unwrap_or(1);
        s.options.push(DesignOption {
            id,
            name: format!("Option {n}"),
        });
        Ok(id)
    }

    pub fn make_primary(&mut self, option: ElementId) {
        if let Some(s) = self.option_sets.iter_mut().find(|s| s.options.iter().any(|o| o.id == option)) {
            s.primary = option;
        }
    }

    /// Deletes a secondary option and the elements in it.
    pub fn delete_option(&mut self, option: ElementId) -> Result<(), ModelError> {
        let set = self
            .set_of_option(option)
            .ok_or_else(|| ModelError::Rejected("No such option.".into()))?;
        if set.primary == option {
            return Err(ModelError::Rejected(
                "The primary option can't be deleted. Make another option primary first.".into(),
            ));
        }
        let ids: Vec<ElementId> = self.elements().filter(|e| e.design_option == Some(option)).map(|e| e.id).collect();
        self.delete(&ids)?;
        for s in &mut self.option_sets {
            s.options.retain(|o| o.id != option);
        }
        if self.editing_option == Some(option) {
            self.editing_option = None;
        }
        self.forget_option_choices(&[option]);
        Ok(())
    }

    /// Accept Primary: the primary option joins the main model; the other
    /// options and their elements are deleted, and the set goes.
    pub fn accept_primary(&mut self, set: ElementId) -> Result<(), ModelError> {
        let s = self
            .option_set(set)
            .cloned()
            .ok_or_else(|| ModelError::Rejected("No such option set.".into()))?;
        let others: Vec<ElementId> = s.options.iter().map(|o| o.id).filter(|o| *o != s.primary).collect();
        let ids: Vec<ElementId> = self
            .elements()
            .filter(|e| e.design_option.is_some_and(|o| others.contains(&o)))
            .map(|e| e.id)
            .collect();
        self.delete(&ids)?;
        let keep: Vec<ElementId> = self
            .elements()
            .filter(|e| e.design_option == Some(s.primary))
            .map(|e| e.id)
            .collect();
        for id in keep {
            if let Some(e) = self.element_mut(id) {
                e.design_option = None;
            }
        }
        self.option_sets.retain(|x| x.id != set);
        if self.editing_option.is_some_and(|o| s.options.iter().any(|x| x.id == o)) {
            self.editing_option = None;
        }
        let all: Vec<ElementId> = s.options.iter().map(|o| o.id).collect();
        self.forget_option_choices(&all);
        Ok(())
    }

    fn forget_option_choices(&mut self, options: &[ElementId]) {
        for v in self.views().map(|v| v.id).collect::<Vec<_>>() {
            if let Some(v) = self.view_mut(v) {
                v.option_choices.retain(|(_, o)| !options.contains(o));
            }
        }
    }

    /// Add to Set: main-model elements move into the first of `options`;
    /// each other option gets its own copy.
    pub fn add_to_set(&mut self, ids: &[ElementId], options: &[ElementId]) -> Result<(), ModelError> {
        if options.is_empty() || options.iter().any(|o| self.set_of_option(*o).is_none()) {
            return Err(ModelError::Rejected("Pick the options to add the elements to.".into()));
        }
        let set = self.set_of_option(options[0]).map(|s| s.id);
        if options.iter().any(|o| self.set_of_option(*o).map(|s| s.id) != set) {
            return Err(ModelError::Rejected("The options must be in one option set.".into()));
        }
        if ids.iter().any(|id| self.element(*id).is_some_and(|e| e.design_option.is_some())) {
            return Err(ModelError::Rejected("Only main model elements can be added to a set.".into()));
        }
        // Openings go with their walls.
        let mut ids = ids.to_vec();
        let hosted: Vec<ElementId> = self
            .elements()
            .filter(|e| e.as_opening().is_some_and(|o| ids.contains(&o.host)) && !ids.contains(&e.id))
            .map(|e| e.id)
            .collect();
        ids.extend(hosted);
        for o in &options[1..] {
            let copies = self.copy(&ids, hausit_core::DVec2::ZERO);
            for c in self.with_group_members(&copies) {
                if let Some(e) = self.element_mut(c) {
                    e.design_option = Some(*o);
                }
            }
        }
        for id in ids {
            if let Some(e) = self.element_mut(id) {
                e.design_option = Some(options[0]);
            }
        }
        Ok(())
    }

    /// The option of a set a view shows: its own choice, else the option
    /// being edited, else the primary.
    pub fn shown_option(&self, view: &View, set: &OptionSet) -> ElementId {
        if let Some((_, o)) = view.option_choices.iter().find(|(s, _)| *s == set.id) {
            return *o;
        }
        if let Some(o) = self.editing_option.filter(|o| set.options.iter().any(|x| x.id == *o)) {
            return o;
        }
        set.primary
    }

    /// Elements a view leaves out for worksets and design options.
    pub fn worksharing_hidden(&self, view: &View) -> HashSet<ElementId> {
        let shown: Vec<ElementId> = self.option_sets.iter().map(|s| self.shown_option(view, s)).collect();
        let closed_or_hidden: Vec<ElementId> = self
            .worksets
            .iter()
            .filter(|w| !self.workset_visible(view, w.id))
            .map(|w| w.id)
            .collect();
        if shown.is_empty() && closed_or_hidden.is_empty() {
            return HashSet::new();
        }
        self.elements()
            .filter(|e| e.design_option.is_some_and(|o| !shown.contains(&o)) || e.workset.is_some_and(|w| closed_or_hidden.contains(&w)))
            .map(|e| e.id)
            .collect()
    }

    /// Whether an element can be picked: secondary options are only
    /// editable while editing them; while editing an option, the main
    /// model is not.
    pub fn option_editable(&self, id: ElementId) -> bool {
        let Some(e) = self.element(id) else { return true };
        match (self.editing_option, e.design_option) {
            (Some(editing), Some(o)) => o == editing,
            (Some(_), None) => matches!(e.category(), Category::Levels | Category::Grids),
            (None, Some(o)) => self.set_of_option(o).is_some_and(|s| s.primary == o),
            (None, None) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::sample_project;
    use crate::{ElementData, ViewKind};

    #[test]
    fn worksets_hide_and_close() {
        let mut doc = sample_project();
        doc.enable_worksharing().unwrap();
        let shared = doc.worksets[0].id;
        let main = doc.worksets[1].id;
        let level = doc.elements_of(Category::Levels).next().unwrap().id;
        assert_eq!(doc.element(level).unwrap().workset, Some(shared));
        let furniture = doc.add_workset("Furniture").unwrap();
        assert!(doc.add_workset("Furniture").is_err());
        doc.active_workset = Some(furniture);
        let wall = doc.elements_of(Category::Walls).next().unwrap().clone();
        let id = doc.add_element(wall.data.clone());
        assert_eq!(doc.element(id).unwrap().workset, Some(furniture));
        let plan = doc.views().find(|v| matches!(v.kind, ViewKind::FloorPlan { .. })).unwrap().clone();
        assert!(!doc.view_hidden(&plan).contains(&id));
        doc.worksets[2].open = false;
        assert!(doc.view_hidden(&plan).contains(&id));
        doc.worksets[2].open = true;
        let mut v = plan.clone();
        v.workset_visibility.push((furniture, WorksetVisibility::Hide));
        assert!(doc.view_hidden(&v).contains(&id));
        doc.delete_workset(furniture, main).unwrap();
        assert_eq!(doc.element(id).unwrap().workset, Some(main));
        assert_eq!(doc.active_workset, Some(main));
    }

    #[test]
    fn design_options_show_accept_and_delete() {
        let mut doc = sample_project();
        let plan = doc.views().find(|v| matches!(v.kind, ViewKind::FloorPlan { .. })).unwrap().clone();
        let (set, primary) = doc.add_option_set("Partition");
        let second = doc.add_option(set).unwrap();
        // The partition wall (with its door) goes into the set.
        let walls: Vec<_> = doc.elements_of(Category::Walls).map(|e| e.id).collect();
        let part = walls[4];
        let doors_before = doc.elements_of(Category::Doors).count();
        doc.add_to_set(&[part], &[primary, second]).unwrap();
        assert_eq!(doc.element(part).unwrap().design_option, Some(primary));
        let copy = doc
            .elements_of(Category::Walls)
            .find(|e| e.design_option == Some(second))
            .unwrap()
            .id;
        // Primary shows by default; the second when the view picks it.
        let hidden = doc.view_hidden(&plan);
        assert!(!hidden.contains(&part) && hidden.contains(&copy));
        let mut v = plan.clone();
        v.option_choices.push((set, second));
        let hidden = doc.view_hidden(&v);
        assert!(hidden.contains(&part) && !hidden.contains(&copy));
        assert!(doc.option_editable(part) && !doc.option_editable(copy));
        doc.editing_option = Some(second);
        assert!(doc.option_editable(copy) && !doc.option_editable(walls[0]));
        doc.editing_option = None;

        assert!(doc.delete_option(primary).is_err());
        doc.make_primary(second);
        doc.accept_primary(set).unwrap();
        assert!(doc.element(part).is_none(), "the old primary went");
        assert_eq!(doc.element(copy).unwrap().design_option, None);
        assert!(doc.option_sets.is_empty());
        assert_eq!(doc.elements_of(Category::Doors).count(), doors_before);
        assert!(doc
            .elements_of(Category::Doors)
            .any(|e| matches!(&e.data, ElementData::Door(o) if o.host == copy)));
    }
}
