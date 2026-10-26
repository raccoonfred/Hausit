//! View graphics.
//!
//! Every graphical view (floor plan, elevation, section, 3D) is turned into
//! a [`DisplayList`]: 2D primitives in *view coordinates* (millimetres, y
//! up), tagged with the element they belong to. The UI only has to pan,
//! zoom, paint and hit-test display lists, so all views share one canvas.

mod annotate;
mod bsp;
pub mod frame;
mod plan;
mod project;

pub use frame::ViewFrame;

use hausit_core::{DVec2, ElementId};
use hausit_model::{Document, View, ViewKind};

pub type Rgba = [u8; 4];

pub const BLACK: Rgba = [0, 0, 0, 255];
/// Section cut caps: a light grey, close to Revit's unpatterned cuts.
pub const CUT_FILL: Rgba = [214, 214, 214, 255];
pub const GRID_COLOR: Rgba = [40, 40, 40, 255];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dash {
    Solid,
    Dashed,
    DashDot,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    /// Screen pixels; lineweights do not scale with zoom (as in Revit's
    /// "thin lines" off mode at typical zoom levels).
    pub width: f32,
    pub color: Rgba,
    pub dash: Dash,
}

impl Stroke {
    pub const fn new(width: f32, color: Rgba) -> Self {
        Stroke {
            width,
            color,
            dash: Dash::Solid,
        }
    }
    pub const fn dashed(mut self, dash: Dash) -> Self {
        self.dash = dash;
        self
    }
}

pub const THIN: Stroke = Stroke::new(1.0, BLACK);
pub const MEDIUM: Stroke = Stroke::new(1.6, BLACK);
pub const HEAVY: Stroke = Stroke::new(2.6, BLACK);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Center,
    Left,
    Right,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    Line(DVec2, DVec2),
    Polyline {
        pts: Vec<DVec2>,
        closed: bool,
    },
    /// Filled simple polygon (may be concave), optionally stroked.
    Polygon(Vec<DVec2>),
    /// Angles in radians, counter-clockwise from +x.
    Arc {
        center: DVec2,
        radius: f64,
        start: f64,
        end: f64,
    },
    Circle {
        center: DVec2,
        radius: f64,
    },
    /// `height` is in model millimetres (already multiplied by the scale).
    Text {
        pos: DVec2,
        text: String,
        height: f64,
        anchor: Anchor,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub prim: Prim,
    pub stroke: Option<Stroke>,
    pub fill: Option<Rgba>,
    /// The element (or view, for section/elevation marks) this belongs to.
    pub element: Option<ElementId>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    pub items: Vec<Item>,
}

impl DisplayList {
    pub fn push(&mut self, prim: Prim, stroke: Option<Stroke>, fill: Option<Rgba>, element: Option<ElementId>) {
        self.items.push(Item {
            prim,
            stroke,
            fill,
            element,
        });
    }

    pub fn line(&mut self, a: DVec2, b: DVec2, stroke: Stroke, element: Option<ElementId>) {
        self.push(Prim::Line(a, b), Some(stroke), None, element);
    }

    /// Bounds of all geometry (texts count by their anchor point).
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let mut min = DVec2::splat(f64::INFINITY);
        let mut max = DVec2::splat(f64::NEG_INFINITY);
        let mut add = |p: DVec2| {
            min = min.min(p);
            max = max.max(p);
        };
        for it in &self.items {
            match &it.prim {
                Prim::Line(a, b) => {
                    add(*a);
                    add(*b)
                }
                Prim::Polyline { pts, .. } | Prim::Polygon(pts) => pts.iter().for_each(|p| add(*p)),
                Prim::Arc { center, radius, .. } | Prim::Circle { center, radius } => {
                    add(*center - DVec2::splat(*radius));
                    add(*center + DVec2::splat(*radius));
                }
                Prim::Text { pos, .. } => add(*pos),
            }
        }
        (min.x <= max.x).then_some((min, max))
    }
}

/// Builds the graphics of a view. Schedules have none.
pub fn build(doc: &Document, view: &View) -> DisplayList {
    match &view.kind {
        ViewKind::FloorPlan { level } => plan::floor_plan(doc, view, *level),
        ViewKind::ThreeD | ViewKind::Elevation { .. } | ViewKind::Section { .. } => {
            let Some(frame) = ViewFrame::for_view(doc, view) else {
                return DisplayList::default();
            };
            let mut dl = project::project(doc, view, &frame);
            if !matches!(view.kind, ViewKind::ThreeD) {
                annotate::levels_and_grids(doc, view, &frame, &mut dl);
            }
            dl
        }
        ViewKind::Schedule(_) => DisplayList::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hausit_model::template::sample_project;

    #[test]
    fn every_view_builds() {
        let doc = sample_project();
        for v in doc.views() {
            let dl = build(&doc, v);
            if v.kind.is_graphical() {
                assert!(!dl.items.is_empty(), "view {} is empty", v.name);
                assert!(dl.bounds().is_some());
            }
        }
    }
}
