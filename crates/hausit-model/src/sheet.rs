//! Sheets: drawing sheets with a title block and views placed on them as
//! viewports, as in Revit. A sheet is a view of the document (in the
//! Project Browser under "Sheets (all)"); its paper space is millimetres
//! with the origin at the bottom-left corner of the paper.

use crate::view::ViewKind;
use crate::{Document, ModelError};
use hausit_core::{DVec2, ElementId};
use serde::{Deserialize, Serialize};

/// ISO A paper sizes, landscape, each with its own title block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SheetSize {
    A0,
    #[default]
    A1,
    A2,
    A3,
}

impl SheetSize {
    pub const ALL: [SheetSize; 4] = [Self::A0, Self::A1, Self::A2, Self::A3];

    /// Paper width and height in mm (landscape).
    pub fn paper(self) -> DVec2 {
        match self {
            Self::A0 => DVec2::new(1189.0, 841.0),
            Self::A1 => DVec2::new(841.0, 594.0),
            Self::A2 => DVec2::new(594.0, 420.0),
            Self::A3 => DVec2::new(420.0, 297.0),
        }
    }

    /// The title block's name, as listed in the New Sheet dialog.
    pub fn title_block(self) -> &'static str {
        match self {
            Self::A0 => "A0 Landscape",
            Self::A1 => "A1 Landscape",
            Self::A2 => "A2 Landscape",
            Self::A3 => "A3 Landscape",
        }
    }
}

/// A view placed on a sheet. `position` is where the centre of the view's
/// extents sits on the paper; for a schedule, its top-left corner.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub view: ElementId,
    pub position: DVec2,
    /// Number shown in the viewport title and in section heads.
    pub detail_number: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SheetDef {
    pub number: String,
    pub size: SheetSize,
    pub viewports: Vec<Viewport>,
    pub drawn_by: String,
    pub checked_by: String,
    pub issue_date: String,
}

impl SheetDef {
    pub fn new(number: &str, size: SheetSize) -> Self {
        SheetDef {
            number: number.to_string(),
            size,
            ..Default::default()
        }
    }

    /// The next free detail number on this sheet.
    pub fn next_detail_number(&self) -> String {
        let max = self.viewports.iter().filter_map(|v| v.detail_number.parse::<u32>().ok()).max();
        (max.unwrap_or(0) + 1).to_string()
    }

    /// Where the drawing area ends: the title block strip runs down the
    /// right-hand side of the paper.
    pub fn drawing_area(&self) -> (DVec2, DVec2) {
        let (min, max) = border(self.size);
        (min, DVec2::new(max.x - strip_width(self.size), max.y))
    }
}

/// Inner border of the title block: 20 mm binding margin on the left,
/// 10 mm elsewhere (7 mm on A3).
pub fn border(size: SheetSize) -> (DVec2, DVec2) {
    let p = size.paper();
    let m = if size == SheetSize::A3 { 7.0 } else { 10.0 };
    (DVec2::new(20.0, m), DVec2::new(p.x - m, p.y - m))
}

/// Width of the title strip on the right of the sheet.
pub fn strip_width(size: SheetSize) -> f64 {
    match size {
        SheetSize::A0 | SheetSize::A1 => 180.0,
        SheetSize::A2 => 150.0,
        SheetSize::A3 => 110.0,
    }
}

/// Whether a view can go on a sheet: any view except another sheet.
pub fn placeable(kind: &ViewKind) -> bool {
    !matches!(kind, ViewKind::Sheet(_))
}

impl Document {
    pub fn sheets(&self) -> impl Iterator<Item = (&crate::View, &SheetDef)> {
        self.views().filter_map(|v| match &v.kind {
            ViewKind::Sheet(s) => Some((v, s)),
            _ => None,
        })
    }

    /// The sheet a view is placed on, with its viewport. Schedules can sit
    /// on several sheets; this returns the first.
    pub fn sheet_of(&self, view: ElementId) -> Option<(&crate::View, &Viewport)> {
        self.sheets()
            .find_map(|(v, s)| s.viewports.iter().find(|p| p.view == view).map(|p| (v, p)))
    }

    /// "A101" → "A102": one more than the highest sheet number.
    pub fn next_sheet_number(&self) -> String {
        let numbers: Vec<&str> = self.sheets().map(|(_, s)| s.number.as_str()).collect();
        match numbers.iter().max_by_key(|n| (n.len(), n.to_string())) {
            Some(n) => crate::document::increment_name(n),
            None => "A101".into(),
        }
    }

    pub fn add_sheet(&mut self, number: &str, name: &str, size: SheetSize) -> ElementId {
        self.add_view(name, ViewKind::Sheet(SheetDef::new(number, size)))
    }

    /// Places `view` on `sheet` at `position` (see [`Viewport`]). A model view
    /// can only be on one sheet; schedules can be on several.
    pub fn place_view(&mut self, sheet: ElementId, view: ElementId, position: DVec2) -> Result<(), ModelError> {
        let kind = &self.view(view).ok_or(ModelError::NotFound(view))?.kind;
        if !placeable(kind) {
            return Err(ModelError::Rejected("A sheet cannot be placed on another sheet.".into()));
        }
        let is_schedule = matches!(kind, ViewKind::Schedule(_));
        if let Some((s, _)) = self.sheet_of(view).filter(|_| !is_schedule) {
            return Err(ModelError::Rejected(format!("This view is already placed on sheet {}.", s.name)));
        }
        let Some(ViewKind::Sheet(def)) = self.view_mut(sheet).map(|v| &mut v.kind) else {
            return Err(ModelError::NotFound(sheet));
        };
        if def.viewports.iter().any(|p| p.view == view) {
            return Err(ModelError::Rejected("This view is already on the sheet.".into()));
        }
        let detail_number = def.next_detail_number();
        def.viewports.push(Viewport {
            view,
            position,
            detail_number,
        });
        Ok(())
    }

    /// Removes a view's viewport from a sheet (the view itself stays).
    pub fn remove_viewport(&mut self, sheet: ElementId, view: ElementId) {
        if let Some(ViewKind::Sheet(def)) = self.view_mut(sheet).map(|v| &mut v.kind) {
            def.viewports.retain(|p| p.view != view);
        }
    }

    pub fn move_viewport(&mut self, sheet: ElementId, view: ElementId, delta: DVec2) {
        if let Some(ViewKind::Sheet(def)) = self.view_mut(sheet).map(|v| &mut v.kind) {
            for p in def.viewports.iter_mut().filter(|p| p.view == view) {
                p.position += delta;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::sample_project;

    #[test]
    fn views_go_on_one_sheet_only() {
        let mut doc = sample_project();
        assert_eq!(doc.next_sheet_number(), "A101");
        let first = doc.add_sheet("A101", "Plans", SheetSize::A1);
        let plan = doc.views().find(|v| v.name == "Level 1").unwrap().id;
        doc.place_view(first, plan, DVec2::ZERO).unwrap();
        assert_eq!(doc.sheet_of(plan).unwrap().0.id, first);
        let n = doc.next_sheet_number();
        assert_eq!(n, "A102");
        let a = doc.add_sheet(&n, "Extra", SheetSize::A3);
        // A plan goes on one sheet only, and sheets go on none.
        assert!(doc.place_view(a, plan, DVec2::ZERO).is_err());
        assert!(doc.place_view(a, first, DVec2::ZERO).is_err());
        let sheets = [first];
        let schedule = doc.views().find(|v| matches!(v.kind, ViewKind::Schedule(_))).unwrap().id;
        doc.place_view(a, schedule, DVec2::ZERO).unwrap();
        doc.place_view(sheets[0], schedule, DVec2::ZERO).ok();
        // Deleting a view takes it off its sheets.
        doc.delete_view(plan);
        assert!(doc.sheet_of(plan).is_none());
        doc.remove_viewport(a, schedule);
        assert!(doc.sheets().find(|(v, _)| v.id == a).unwrap().1.viewports.is_empty());
    }
}
