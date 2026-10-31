//! View-specific 2D detailing: detail lines, filled regions and spot
//! elevations. Like text notes, they belong to one view and are drawn in
//! its coordinates (model x and y in plans, the view's own axes in
//! elevations and sections).

use crate::category::Category;
use crate::document::Document;
use crate::element::ElementData;
use crate::geometry::element_geometry;
use crate::materials::FillPattern;
use hausit_core::geom::{point_in_polygon, polygon_normal};
use hausit_core::{DVec2, ElementId};
use serde::{Deserialize, Serialize};

/// Line styles for detail lines and region borders.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineStyle {
    #[default]
    Thin,
    Medium,
    Wide,
    Hidden,
    /// No line (region borders only).
    Invisible,
}

impl LineStyle {
    pub const ALL: [LineStyle; 5] = [
        LineStyle::Thin,
        LineStyle::Medium,
        LineStyle::Wide,
        LineStyle::Hidden,
        LineStyle::Invisible,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LineStyle::Thin => "Thin Lines",
            LineStyle::Medium => "Medium Lines",
            LineStyle::Wide => "Wide Lines",
            LineStyle::Hidden => "Hidden Lines",
            LineStyle::Invisible => "<Invisible lines>",
        }
    }
}

/// A detail line: a straight line drawn in one view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DetailLine {
    pub view: ElementId,
    pub start: DVec2,
    pub end: DVec2,
    #[serde(default)]
    pub style: LineStyle,
}

/// A filled region: a closed boundary filled with a pattern or a solid
/// colour. A masking region is filled white, hiding what is under it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FilledRegion {
    pub view: ElementId,
    pub boundary: Vec<DVec2>,
    pub pattern: FillPattern,
    /// Fill colour (solid fills and pattern lines).
    pub color: [u8; 3],
    #[serde(default)]
    pub masking: bool,
    #[serde(default)]
    pub border: LineStyle,
}

/// A spot elevation: the height of the point it marks, with a leader to
/// its label.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpotElevation {
    pub view: ElementId,
    /// The marked point, in view coordinates.
    pub point: DVec2,
    /// Where the label sits.
    pub label_at: DVec2,
}

/// Fill patterns a filled region can use, by name.
pub const REGION_PATTERNS: [(&str, FillPattern); 5] = [
    ("Solid fill", FillPattern::Solid),
    ("Diagonal", FillPattern::Diagonal),
    ("Crosshatch", FillPattern::Crosshatch),
    ("Concrete", FillPattern::Concrete),
    ("Insulation", FillPattern::RigidInsulation),
];

/// Categories the top of which a spot elevation reads in plan.
const SPOT_CATEGORIES: [Category; 5] = [
    Category::Floors,
    Category::Roofs,
    Category::Stairs,
    Category::StructuralFoundations,
    Category::GenericModels,
];

impl Document {
    /// The height a spot elevation shows. In plans: the highest upward
    /// face over the point (floors, roofs, stairs…), or the view's level
    /// when there is none. In elevations and sections the point's own
    /// height.
    pub fn spot_height(&self, s: &SpotElevation) -> f64 {
        let Some(view) = self.view(s.view) else { return 0.0 };
        match &view.kind {
            crate::ViewKind::FloorPlan { level } | crate::ViewKind::CeilingPlan { level } => {
                let mut best: Option<f64> = None;
                for cat in SPOT_CATEGORIES {
                    for e in self.elements_of(cat) {
                        if self.view_hidden(view).contains(&e.id) {
                            continue;
                        }
                        let Some(g) = element_geometry(self, e.id) else { continue };
                        for part in &g.parts {
                            for f in part.faces() {
                                let n = polygon_normal(&f.pts).normalize_or_zero();
                                if n.z < 0.05 {
                                    continue;
                                }
                                let flat: Vec<DVec2> = f.pts.iter().map(|p| p.truncate()).collect();
                                if !point_in_polygon(s.point, &flat) {
                                    continue;
                                }
                                // The face's plane at the point.
                                let o = f.pts[0];
                                let z = o.z - (n.x * (s.point.x - o.x) + n.y * (s.point.y - o.y)) / n.z;
                                best = Some(best.map_or(z, |b: f64| b.max(z)));
                            }
                        }
                    }
                }
                best.unwrap_or_else(|| self.level_elevation(*level))
            }
            _ => s.point.y,
        }
    }

    /// Detailing owned by a view (for deleting and duplicating it).
    pub fn is_detail_of(&self, data: &ElementData, view: ElementId) -> bool {
        match data {
            ElementData::DetailLine(l) => l.view == view,
            ElementData::FilledRegion(r) => r.view == view,
            ElementData::SpotElevation(s) => s.view == view,
            ElementData::RevisionCloud(c) => c.view == view,
            _ => false,
        }
    }
}

/// Moves a detail element's points into another view (Duplicate with
/// Detailing).
pub fn set_view(data: &mut ElementData, view: ElementId) {
    match data {
        ElementData::DetailLine(l) => l.view = view,
        ElementData::FilledRegion(r) => r.view = view,
        ElementData::SpotElevation(s) => s.view = view,
        ElementData::RevisionCloud(c) => c.view = view,
        _ => {}
    }
}

/// Applies a plan transform to a detail element's points.
pub fn map_points(data: &mut ElementData, f: &dyn Fn(DVec2) -> DVec2) {
    match data {
        ElementData::DetailLine(l) => {
            l.start = f(l.start);
            l.end = f(l.end);
        }
        ElementData::FilledRegion(r) => r.boundary.iter_mut().for_each(|p| *p = f(*p)),
        ElementData::SpotElevation(s) => {
            s.point = f(s.point);
            s.label_at = f(s.label_at);
        }
        ElementData::RevisionCloud(c) => {
            c.boundary.iter_mut().for_each(|p| *p = f(*p));
            c.tag = c.tag.map(f);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::sample_project;

    #[test]
    fn spot_elevation_reads_floor_top() {
        let mut doc = sample_project();
        let plan = doc
            .views()
            .find(|v| matches!(v.kind, crate::ViewKind::FloorPlan { .. }) && v.name == "Level 1")
            .unwrap()
            .id;
        let floor = doc.elements_of(Category::Floors).next().unwrap().clone();
        let ElementData::Floor(f) = &floor.data else { unreachable!() };
        let inside = f.boundary.iter().copied().sum::<DVec2>() / f.boundary.len() as f64;
        let top = doc.level_elevation(f.level) + f.height_offset;
        let s = SpotElevation {
            view: plan,
            point: inside,
            label_at: inside + DVec2::new(1000.0, 1000.0),
        };
        assert!((doc.spot_height(&s) - top).abs() < 1e-6, "{} vs {top}", doc.spot_height(&s));
        // Outside the building: the level's height.
        let out = SpotElevation {
            point: DVec2::new(-50_000.0, 0.0),
            ..s.clone()
        };
        assert_eq!(doc.spot_height(&out), 0.0);
        // Deleting the view deletes its detailing.
        let id = doc.add_element(ElementData::SpotElevation(s));
        doc.delete(&[plan]).unwrap();
        assert!(doc.element(id).is_none());
    }
}
