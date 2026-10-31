//! Revisions, as Revit tracks them: the project's revision list (Sheet
//! Issues/Revisions), revision clouds drawn in views, and the revisions
//! each sheet carries in its title block.
//!
//! A sheet shows a revision when a cloud of it is drawn in one of the
//! sheet's views, or when the revision is ticked in the sheet's
//! "Revisions on Sheet". Once a revision is issued, its clouds are locked.

use crate::category::Category;
use crate::document::{Document, ModelError};
use crate::element::ElementData;
use crate::view::ViewKind;
use hausit_core::geom::polygon_area;
use hausit_core::{DVec2, ElementId};
use serde::{Deserialize, Serialize};

/// How a revision is numbered on sheets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Numbering {
    #[default]
    Numeric,
    Alphabetic,
    /// Not numbered (and not listed in title blocks).
    None,
}

impl Numbering {
    pub const ALL: [Numbering; 3] = [Numbering::Numeric, Numbering::Alphabetic, Numbering::None];

    pub fn label(self) -> &'static str {
        match self {
            Numbering::Numeric => "Numeric",
            Numbering::Alphabetic => "Alphabetic",
            Numbering::None => "None",
        }
    }
}

/// One revision of the project, in sequence order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    pub id: ElementId,
    #[serde(default)]
    pub numbering: Numbering,
    pub date: String,
    pub description: String,
    #[serde(default)]
    pub issued: bool,
    #[serde(default)]
    pub issued_to: String,
    #[serde(default)]
    pub issued_by: String,
}

/// A revision cloud: a cloud drawn around changed work in one view,
/// assigned to a revision, with an optional revision tag.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RevisionCloud {
    pub view: ElementId,
    pub boundary: Vec<DVec2>,
    pub revision: ElementId,
    /// Where the revision tag sits, if tagged.
    #[serde(default)]
    pub tag: Option<DVec2>,
}

/// "A", "B", … "Z", "AA", "AB", … for the n-th (1-based) letter number.
fn letters(mut n: usize) -> String {
    let mut s = Vec::new();
    while n > 0 {
        n -= 1;
        s.push(b'A' + (n % 26) as u8);
        n /= 26;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// Paper length of one cloud arc, mm.
pub const CLOUD_ARC: f64 = 8.0;

/// The scalloped outline of a cloud: arcs along each edge, bulging out of
/// the boundary. `chord` is the arc length in model units.
pub fn cloud_outline(boundary: &[DVec2], chord: f64) -> Vec<DVec2> {
    let n = boundary.len();
    if n < 2 || chord <= 0.0 {
        return boundary.to_vec();
    }
    // Outward is to the right of a counter-clockwise boundary.
    let ccw = polygon_area(boundary) > 0.0;
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (boundary[i], boundary[(i + 1) % n]);
        let len = a.distance(b);
        if len < 1e-6 {
            continue;
        }
        let d = (b - a) / len;
        let normal = if ccw { DVec2::new(d.y, -d.x) } else { DVec2::new(-d.y, d.x) };
        let count = (len / chord).round().max(1.0) as usize;
        let step = len / count as f64;
        for k in 0..count {
            let s = a + d * (step * k as f64);
            // A half circle-ish bump: six points per arc.
            for j in 0..6 {
                let t = j as f64 / 6.0;
                let bulge = (std::f64::consts::PI * t).sin() * step * 0.35;
                out.push(s + d * (step * t) + normal * bulge);
            }
        }
    }
    out
}

/// Where a new revision tag goes: just outside the cloud's top-right
/// corner.
pub fn default_tag_point(boundary: &[DVec2]) -> DVec2 {
    boundary
        .iter()
        .copied()
        .max_by(|a, b| (a.x + a.y).total_cmp(&(b.x + b.y)))
        .unwrap_or_default()
}

impl Document {
    /// A new project's single revision, as Revit starts with.
    pub fn ensure_revisions(&mut self) {
        if self.revisions.is_empty() {
            let id = self.new_id();
            self.revisions.push(Revision {
                id,
                numbering: Numbering::Numeric,
                date: "Date 1".into(),
                description: "Revision 1".into(),
                issued: false,
                issued_to: String::new(),
                issued_by: String::new(),
            });
        }
    }

    pub fn revision_def(&self, id: ElementId) -> Option<&Revision> {
        self.revisions.iter().find(|r| r.id == id)
    }

    /// 1-based position in the revision list.
    pub fn revision_sequence(&self, id: ElementId) -> usize {
        self.revisions.iter().position(|r| r.id == id).map_or(0, |i| i + 1)
    }

    /// The revision's number: numeric and alphabetic revisions count up
    /// separately in sequence order, as with Revit's per-project numbering.
    pub fn revision_number(&self, id: ElementId) -> String {
        let Some(i) = self.revisions.iter().position(|r| r.id == id) else {
            return String::new();
        };
        let numbering = self.revisions[i].numbering;
        let n = self.revisions[..=i].iter().filter(|r| r.numbering == numbering).count();
        match numbering {
            Numbering::Numeric => n.to_string(),
            Numbering::Alphabetic => letters(n),
            Numbering::None => String::new(),
        }
    }

    /// "Seq. 2 - Revision 2", for pick lists.
    pub fn revision_label(&self, id: ElementId) -> String {
        match self.revision_def(id) {
            Some(r) => format!("Seq. {} - {}", self.revision_sequence(id), r.description),
            None => "<none>".into(),
        }
    }

    /// Adds a revision at the end of the list.
    pub fn add_revision(&mut self) -> ElementId {
        let id = self.new_id();
        let n = self.revisions.len() + 1;
        let numbering = self.revisions.last().map(|r| r.numbering).unwrap_or_default();
        self.revisions.push(Revision {
            id,
            numbering,
            date: format!("Date {n}"),
            description: format!("Revision {n}"),
            issued: false,
            issued_to: String::new(),
            issued_by: String::new(),
        });
        id
    }

    /// Clouds of a revision.
    pub fn revision_clouds(&self, revision: ElementId) -> impl Iterator<Item = (ElementId, &RevisionCloud)> {
        self.elements_of(Category::RevisionClouds).filter_map(move |e| match &e.data {
            ElementData::RevisionCloud(c) if c.revision == revision => Some((e.id, c)),
            _ => None,
        })
    }

    /// Deletes a revision. Issued revisions, revisions with clouds or on
    /// sheets, and the last revision stay.
    pub fn delete_revision(&mut self, id: ElementId) -> Result<(), ModelError> {
        let r = self
            .revision_def(id)
            .ok_or_else(|| ModelError::Rejected("No such revision.".into()))?;
        if r.issued {
            return Err(ModelError::Rejected("An issued revision can't be deleted.".into()));
        }
        if self.revisions.len() == 1 {
            return Err(ModelError::Rejected("A project keeps at least one revision.".into()));
        }
        if self.revision_clouds(id).next().is_some() {
            return Err(ModelError::Rejected(
                "Revision clouds use this revision. Delete them or assign them to another revision first.".into(),
            ));
        }
        self.revisions.retain(|r| r.id != id);
        let sheets: Vec<ElementId> = self.sheets().map(|(v, _)| v.id).collect();
        for sheet in sheets {
            if let Some(ViewKind::Sheet(s)) = self.view_mut(sheet).map(|v| &mut v.kind) {
                s.revisions.retain(|r| *r != id);
            }
        }
        Ok(())
    }

    /// Moves a revision one place up (earlier) or down in the sequence.
    pub fn move_revision(&mut self, id: ElementId, up: bool) {
        let Some(i) = self.revisions.iter().position(|r| r.id == id) else {
            return;
        };
        let j = if up {
            i.checked_sub(1)
        } else {
            Some(i + 1).filter(|j| *j < self.revisions.len())
        };
        if let Some(j) = j {
            self.revisions.swap(i, j);
        }
    }

    /// Whether a cloud belongs to an issued revision (and is locked).
    pub fn cloud_locked(&self, id: ElementId) -> bool {
        match self.element(id).map(|e| &e.data) {
            Some(ElementData::RevisionCloud(c)) => self.revision_def(c.revision).is_some_and(|r| r.issued),
            _ => false,
        }
    }

    /// Revisions a sheet carries, in sequence order: those with clouds on
    /// the sheet or in its views, plus those ticked on the sheet.
    pub fn sheet_revisions(&self, sheet: ElementId) -> Vec<ElementId> {
        let Some(ViewKind::Sheet(def)) = self.view(sheet).map(|v| &v.kind) else {
            return Vec::new();
        };
        let views: Vec<ElementId> = std::iter::once(sheet).chain(def.viewports.iter().map(|vp| vp.view)).collect();
        self.revisions
            .iter()
            .filter(|r| def.revisions.contains(&r.id) || self.revision_clouds(r.id).any(|(_, c)| views.contains(&c.view)))
            .map(|r| r.id)
            .collect()
    }

    /// The sheet's current revision: its latest numbered one.
    pub fn sheet_current_revision(&self, sheet: ElementId) -> Option<ElementId> {
        self.sheet_revisions(sheet)
            .into_iter()
            .rev()
            .find(|r| self.revision_def(*r).is_some_and(|r| r.numbering != Numbering::None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::sample_project;

    #[test]
    fn numbering_and_sheet_revisions() {
        let mut doc = sample_project();
        doc.ensure_revisions();
        assert_eq!(doc.revisions.len(), 1);
        let r1 = doc.revisions[0].id;
        let r2 = doc.add_revision();
        let r3 = doc.add_revision();
        doc.revisions[2].numbering = Numbering::Alphabetic;
        assert_eq!(doc.revision_number(r1), "1");
        assert_eq!(doc.revision_number(r2), "2");
        assert_eq!(doc.revision_number(r3), "A");
        assert_eq!(letters(27), "AA");

        // A sheet with the Level 1 plan on it.
        let plan = doc
            .views()
            .find(|v| matches!(v.kind, ViewKind::FloorPlan { .. }) && v.name == "Level 1")
            .unwrap()
            .id;
        let number = doc.next_sheet_number();
        let sheet = doc.add_view("Plans", ViewKind::Sheet(crate::SheetDef::new(&number, crate::SheetSize::A1)));
        if let Some(ViewKind::Sheet(s)) = doc.view_mut(sheet).map(|v| &mut v.kind) {
            s.viewports.push(crate::Viewport {
                view: plan,
                position: DVec2::new(300.0, 300.0),
                detail_number: "1".into(),
            });
        }
        assert!(doc.sheet_revisions(sheet).is_empty());
        let cloud = doc.add_element(ElementData::RevisionCloud(RevisionCloud {
            view: plan,
            boundary: vec![DVec2::ZERO, DVec2::new(3000.0, 0.0), DVec2::new(3000.0, 2000.0)],
            revision: r2,
            tag: None,
        }));
        if let Some(ViewKind::Sheet(s)) = doc.view_mut(sheet).map(|v| &mut v.kind) {
            s.revisions.push(r1);
        }
        assert_eq!(doc.sheet_revisions(sheet), vec![r1, r2]);
        assert_eq!(doc.sheet_current_revision(sheet), Some(r2));

        // In use: can't delete. Issued: its clouds lock.
        assert!(doc.delete_revision(r2).is_err());
        doc.revisions[1].issued = true;
        assert!(doc.cloud_locked(cloud));
        assert!(doc.delete(&[cloud]).is_err());
        doc.delete_revision(r3).unwrap();
        doc.move_revision(r2, true);
        assert_eq!(doc.revision_sequence(r2), 1);

        // Outline bulges outward on every edge.
        let sq = [
            DVec2::ZERO,
            DVec2::new(100.0, 0.0),
            DVec2::new(100.0, 100.0),
            DVec2::new(0.0, 100.0),
        ];
        let o = cloud_outline(&sq, 25.0);
        assert!(o.iter().all(|p| p.x >= -1e-9 - 9.0 && p.x <= 109.0));
        assert!(o.iter().any(|p| p.y < -1.0) && o.iter().any(|p| p.x > 101.0));
    }
}
