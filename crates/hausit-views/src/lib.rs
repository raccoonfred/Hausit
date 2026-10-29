//! View graphics.
//!
//! Every graphical view (floor plan, elevation, section, 3D) is turned into
//! a [`DisplayList`]: 2D primitives in *view coordinates* (millimetres, y
//! up), tagged with the element they belong to. The UI only has to pan,
//! zoom, paint and hit-test display lists, so all views share one canvas.

mod annotate;
mod bsp;
mod dims;
pub mod frame;
mod hatch;
pub mod pdf;
mod plan;
mod project;
pub mod sheet;

pub use dims::draw_dimension;
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

/// Screen pixels per paper millimetre for line weights.
pub const PX_PER_MM: f64 = 5.2;

/// The stroke a category draws with in a view: its object style's line
/// weight at the view's scale and colour, with the view's
/// Visibility/Graphics overrides.
pub fn pen(doc: &Document, cat: hausit_model::Category, cut: bool, view: &View) -> Stroke {
    let style = doc.object_styles.get(cat);
    let vg = view.category_graphics(cat);
    let (weight, color) = if cut {
        (vg.cut_weight.unwrap_or(style.cut), vg.cut_color.unwrap_or(style.color))
    } else {
        (
            vg.projection_weight.unwrap_or(style.projection),
            vg.projection_color.unwrap_or(style.color),
        )
    };
    let mm = doc.line_weights.width(weight, view.scale);
    let [r, g, b] = color;
    Stroke::new((mm * PX_PER_MM).max(1.0) as f32, [r, g, b, 255])
}

/// Halftone: half way to white, as Revit draws it.
pub fn halftone(c: Rgba) -> Rgba {
    let h = |v: u8| ((v as u16 + 255) / 2) as u8;
    [h(c[0]), h(c[1]), h(c[2]), c[3]]
}

/// Applies a view's Visibility/Graphics: drops hidden categories and
/// draws halftone ones lighter.
fn apply_graphics(doc: &Document, view: &View, dl: &mut DisplayList) {
    if view.graphics.is_empty() {
        return;
    }
    let cat_of = |id: Option<ElementId>| id.and_then(|id| doc.element(id)).map(|e| e.category());
    dl.items
        .retain(|it| cat_of(it.element).is_none_or(|c| view.category_graphics(c).visible));
    for it in &mut dl.items {
        if cat_of(it.element).is_some_and(|c| view.category_graphics(c).halftone) {
            if let Some(s) = &mut it.stroke {
                s.color = halftone(s.color);
            }
            if let Some(f) = &mut it.fill {
                *f = halftone(*f);
            }
        }
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
        /// Counter-clockwise rotation in view coordinates, radians.
        angle: f64,
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
    // The view as its template sets it.
    let effective = doc.effective_view(view);
    let view = &effective;
    let mut dl = match &view.kind {
        ViewKind::FloorPlan { level } | ViewKind::CeilingPlan { level } => {
            let mut dl = plan::floor_plan(doc, view, *level);
            annotate::notes_and_tags(doc, view, None, &mut dl);
            dl
        }
        ViewKind::ThreeD | ViewKind::Elevation { .. } | ViewKind::Section { .. } => {
            let Some(frame) = ViewFrame::for_view(doc, view) else {
                return DisplayList::default();
            };
            let mut dl = project::project(doc, view, &frame);
            if !matches!(view.kind, ViewKind::ThreeD) {
                annotate::levels_and_grids(doc, view, &frame, &mut dl);
                annotate::notes_and_tags(doc, view, Some(&frame), &mut dl);
            }
            dl
        }
        ViewKind::Schedule(_) => DisplayList::default(),
        ViewKind::Sheet(def) => return sheet::build(doc, view, def),
    };
    annotate::callout_marks(doc, view, &mut dl);
    apply_graphics(doc, view, &mut dl);
    if let Some((lo, hi)) = view.crop {
        dl = crop(dl, lo, hi);
    }
    dl
}

/// Keeps what lies inside the rectangle `lo`–`hi`: lines and fills are
/// clipped to it; circles, arcs and text are kept when their centre is.
pub fn crop(dl: DisplayList, lo: DVec2, hi: DVec2) -> DisplayList {
    use hausit_core::geom::clip_polygon_2d;
    let inside = |p: DVec2| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y;
    let clip = |a: DVec2, b: DVec2| -> Option<(DVec2, DVec2)> {
        // Liang–Barsky.
        let d = b - a;
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        for (p, q) in [(-d.x, a.x - lo.x), (d.x, hi.x - a.x), (-d.y, a.y - lo.y), (d.y, hi.y - a.y)] {
            if p.abs() < 1e-12 {
                if q < 0.0 {
                    return None;
                }
            } else {
                let r = q / p;
                if p < 0.0 {
                    t0 = t0.max(r);
                } else {
                    t1 = t1.min(r);
                }
            }
        }
        (t0 < t1).then(|| (a + d * t0, a + d * t1))
    };
    let mut out = DisplayList::default();
    for item in dl.items {
        match &item.prim {
            Prim::Line(a, b) => {
                if let Some((a, b)) = clip(*a, *b) {
                    out.push(Prim::Line(a, b), item.stroke, item.fill, item.element);
                }
            }
            Prim::Polyline { pts, closed } => {
                let n = pts.len();
                let m = if *closed { n } else { n.saturating_sub(1) };
                for i in 0..m {
                    if let Some((a, b)) = clip(pts[i], pts[(i + 1) % n]) {
                        out.push(Prim::Line(a, b), item.stroke, None, item.element);
                    }
                }
            }
            Prim::Polygon(pts) => {
                let mut p = pts.clone();
                for (n, d) in [(DVec2::X, hi.x), (-DVec2::X, -lo.x), (DVec2::Y, hi.y), (-DVec2::Y, -lo.y)] {
                    p = clip_polygon_2d(&p, n, d);
                }
                if p.len() >= 3 {
                    out.push(Prim::Polygon(p), item.stroke, item.fill, item.element);
                }
            }
            Prim::Circle { center, .. } | Prim::Arc { center, .. } | Prim::Text { pos: center, .. } => {
                if inside(*center) {
                    out.items.push(item);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hausit_model::template::sample_project;

    #[test]
    fn view_templates_and_visibility() {
        let mut doc = sample_project();
        let plan = doc
            .views()
            .find(|v| matches!(v.kind, ViewKind::FloorPlan { .. }))
            .map(|v| v.id)
            .unwrap();
        let shows = |doc: &Document, cat: hausit_model::Category| {
            build(doc, doc.view(plan).unwrap())
                .items
                .iter()
                .any(|it| it.element.and_then(|e| doc.element(e)).is_some_and(|e| e.category() == cat))
        };
        assert!(shows(&doc, hausit_model::Category::Furniture));
        // The Structural Plan template hides furniture.
        let t = doc.view_templates.iter().find(|t| t.name == "Structural Plan").unwrap().id;
        doc.view_mut(plan).unwrap().template = Some(t);
        assert!(!shows(&doc, hausit_model::Category::Furniture));
        assert!(shows(&doc, hausit_model::Category::Walls));
        assert_eq!(
            doc.effective_view(doc.view(plan).unwrap()).detail_level,
            hausit_model::DetailLevel::Coarse
        );
        // Deleting the template leaves the view as it looked.
        doc.delete_view_template(t);
        assert!(doc.view(plan).unwrap().template.is_none());
        assert!(!shows(&doc, hausit_model::Category::Furniture));
        // A colour override reaches the pen.
        let v = doc.view_mut(plan).unwrap();
        v.graphics.insert(
            hausit_model::Category::Walls,
            hausit_model::CategoryGraphics {
                cut_color: Some([200, 0, 0]),
                ..Default::default()
            },
        );
        let v = doc.view(plan).unwrap().clone();
        assert_eq!(pen(&doc, hausit_model::Category::Walls, true, &v).color, [200, 0, 0, 255]);
    }

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

#[cfg(test)]
mod plan_tests {
    use super::*;
    use hausit_model::template::sample_project;
    use hausit_model::Category;

    /// Every wall is cut in the Level 1 plan, including walls attached to
    /// the roof.
    #[test]
    fn all_walls_cut_in_plan() {
        let doc = sample_project();
        let plan = doc.views().find(|v| matches!(v.kind, ViewKind::FloorPlan { .. })).unwrap();
        let dl = build(&doc, plan);
        for w in doc.elements_of(Category::Walls) {
            let cut = dl
                .items
                .iter()
                .any(|it| it.element == Some(w.id) && matches!(it.prim, Prim::Polygon(_)) && it.fill.is_some());
            assert!(cut, "wall {} is not cut", w.id);
        }
    }

    #[test]
    fn ceiling_plan_shows_ceilings_not_doors() {
        let doc = hausit_model::template::sample_project();
        let rcp = doc.views().find(|v| matches!(v.kind, ViewKind::CeilingPlan { .. })).unwrap();
        let dl = build(&doc, rcp);
        let ids: std::collections::HashSet<_> = dl.items.iter().filter_map(|i| i.element).collect();
        let cat = |c: Category| doc.elements_of(c).any(|e| ids.contains(&e.id));
        assert!(cat(Category::Ceilings));
        assert!(cat(Category::Walls));
        assert!(!cat(Category::Doors));
        assert!(!cat(Category::Floors));
    }
}

#[cfg(test)]
mod orbit_tests {
    use super::*;
    use hausit_model::template::sample_project;
    use hausit_model::{Category, ElementData};

    #[test]
    fn orbit_stays_bounded() {
        let mut doc = sample_project();
        let roof = doc.elements_of(Category::Roofs).next().unwrap().id;
        if let Some(ElementData::Roof(r)) = doc.element_mut(roof).map(|e| &mut e.data) {
            r.defines_slope = vec![true, false, true, false];
        }
        let v3 = doc.views().find(|v| matches!(v.kind, ViewKind::ThreeD)).unwrap().id;
        let mut bad = vec![];
        for yaw in (0..360).step_by(5) {
            for pitch in (-80..=80).step_by(5) {
                let v = doc.view_mut(v3).unwrap();
                v.camera.yaw = yaw as f64;
                v.camera.pitch = pitch as f64;
                let v = doc.view(v3).unwrap().clone();
                let dl = build(&doc, &v);
                for it in &dl.items {
                    if let Prim::Polygon(pts) = &it.prim {
                        if pts.iter().any(|p| p.length() > 60000.0 || !p.is_finite()) {
                            bad.push((yaw, pitch, pts.len()));
                            break;
                        }
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{} bad views, e.g. {:?}", bad.len(), &bad[..bad.len().min(5)]);
    }
}

#[cfg(test)]
mod crop_tests {
    use super::*;
    use hausit_model::template::sample_project;

    #[test]
    fn callouts_crop_their_parent() {
        let mut doc = sample_project();
        let section = doc.views().find(|v| matches!(v.kind, ViewKind::Section { .. })).unwrap().id;
        let (lo, hi) = (DVec2::new(-500.0, 2500.0), DVec2::new(1500.0, 5000.0));
        let c = doc.add_callout(section, hi, lo).unwrap();
        let callout = doc.view(c).unwrap().clone();
        assert_eq!((callout.scale, callout.crop), (20, Some((lo, hi))));
        let dl = build(&doc, &callout);
        let (min, max) = dl.bounds().unwrap();
        assert!(min.cmpge(lo - 1e-6).all() && max.cmple(hi + 1e-6).all(), "{min} {max}");
        // The parent shows the callout's boundary, tagged to open it.
        let parent = build(&doc, doc.view(section).unwrap());
        assert!(parent.items.iter().any(|i| i.element == Some(c)));
        // Deleting the parent deletes its callouts.
        doc.delete_view(section);
        assert!(doc.view(c).is_none());
    }

    #[test]
    fn section_box_cuts_the_model() {
        let mut doc = sample_project();
        let v3 = doc.views().find(|v| matches!(v.kind, ViewKind::ThreeD)).unwrap().id;
        let whole = build(&doc, doc.view(v3).unwrap());
        let b = hausit_model::geometry::model_bounds(&doc).unwrap();
        let mut cut = hausit_model::SectionBox::around(b.min, b.max);
        cut.max.x = (b.min.x + b.max.x) / 2.0;
        doc.view_mut(v3).unwrap().section_box = Some(cut);
        let half = build(&doc, doc.view(v3).unwrap());
        let width = |d: &DisplayList| {
            let (lo, hi) = d.bounds().unwrap();
            hi.x - lo.x
        };
        assert!(width(&half) < width(&whole) * 0.8, "{} vs {}", width(&half), width(&whole));
    }
}
