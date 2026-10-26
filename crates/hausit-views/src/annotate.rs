//! Datum and view annotations: grid bubbles, level heads, section and
//! elevation marks.

use crate::frame::ViewFrame;
use crate::{Anchor, Dash, DisplayList, Prim, Stroke, GRID_COLOR, THIN};
use hausit_core::geom::perp;
use hausit_core::{DVec2, DVec3, ElementId};
use hausit_model::geometry::model_bounds;
use hausit_model::{CardinalDirection, Category, Document, ElementData, View, ViewKind};

/// Paper size of a grid bubble radius, mm.
const BUBBLE_R: f64 = 4.0;
const TEXT_H: f64 = 3.0;

fn grid_stroke() -> Stroke {
    Stroke::new(1.0, GRID_COLOR).dashed(Dash::DashDot)
}

/// Grid line with bubbles at both ends.
pub fn grid_bubble(dl: &mut DisplayList, a: DVec2, b: DVec2, name: &str, scale: f64, id: ElementId) {
    let d = (b - a).normalize_or_zero();
    let r = BUBBLE_R * scale;
    dl.line(a, b, grid_stroke(), Some(id));
    for (p, dir) in [(a, -d), (b, d)] {
        let c = p + dir * r;
        dl.push(
            Prim::Circle { center: c, radius: r },
            Some(THIN),
            Some([255, 255, 255, 255]),
            Some(id),
        );
        dl.push(
            Prim::Text {
                pos: c,
                text: name.to_string(),
                height: TEXT_H * scale,
                anchor: Anchor::Center,
                angle: 0.0,
            },
            None,
            None,
            Some(id),
        );
    }
}

/// Section lines and elevation markers shown in plan views. Tagged with the
/// target view's id, so double-clicking them can open the view.
pub fn view_marks(doc: &Document, plan: &View, dl: &mut DisplayList) {
    let scale = plan.scale as f64;
    // Callouts are shown in their parent view, not as marks of their own.
    for v in doc.views().filter(|v| v.callout_of.is_none()) {
        match &v.kind {
            ViewKind::Section { start, end, .. } => {
                let s = Stroke::new(2.0, [0, 0, 0, 255]).dashed(Dash::Dashed);
                dl.line(*start, *end, s, Some(v.id));
                let d = (*end - *start).normalize_or_zero();
                let look = DVec2::new(d.y, -d.x);
                let r = BUBBLE_R * scale;
                for p in [*start, *end] {
                    let tip = p + look * r * 1.8;
                    dl.push(
                        Prim::Polygon(vec![p + d * r * 0.6, tip, p - d * r * 0.6]),
                        Some(THIN),
                        Some([0, 0, 0, 255]),
                        Some(v.id),
                    );
                }
                let c = *start - d * r;
                dl.push(
                    Prim::Circle { center: c, radius: r },
                    Some(THIN),
                    Some([255, 255, 255, 255]),
                    Some(v.id),
                );
                // Once on a sheet, the head reads detail number over sheet
                // number, as a view reference.
                let on_sheet = doc.sheet_of(v.id).and_then(|(sv, vp)| match &sv.kind {
                    ViewKind::Sheet(def) => Some((vp.detail_number.clone(), def.number.clone())),
                    _ => None,
                });
                let label = |dl: &mut DisplayList, pos: DVec2, text: String, k: f64| {
                    dl.push(
                        Prim::Text {
                            pos,
                            text,
                            height: TEXT_H * scale * k,
                            anchor: Anchor::Center,
                            angle: 0.0,
                        },
                        None,
                        None,
                        Some(v.id),
                    )
                };
                match on_sheet {
                    Some((detail, sheet)) => {
                        dl.line(c - DVec2::new(r, 0.0), c + DVec2::new(r, 0.0), THIN, Some(v.id));
                        label(dl, c + DVec2::new(0.0, r * 0.42), detail, 0.6);
                        label(dl, c - DVec2::new(0.0, r * 0.42), sheet, 0.5);
                    }
                    None => label(dl, c, v.name.trim_start_matches("Section ").to_string(), 0.8),
                }
            }
            ViewKind::Elevation { direction } => {
                let Some(b) = model_bounds(doc) else { continue };
                let c = b.center();
                let out = direction.outward();
                let half = match direction {
                    CardinalDirection::North | CardinalDirection::South => b.size().y / 2.0,
                    _ => b.size().x / 2.0,
                };
                let r = BUBBLE_R * scale * 1.2;
                let p = DVec2::new(c.x, c.y) + out * (half + 4.0 * r);
                dl.push(
                    Prim::Circle { center: p, radius: r },
                    Some(THIN),
                    Some([255, 255, 255, 255]),
                    Some(v.id),
                );
                let n = perp(out);
                dl.push(
                    Prim::Polygon(vec![p + n * r, p - out * r * 1.6, p - n * r]),
                    Some(THIN),
                    Some([0, 0, 0, 255]),
                    Some(v.id),
                );
            }
            _ => {}
        }
    }
}

/// Level lines with heads, and grid lines, in elevations and sections.
pub fn levels_and_grids(doc: &Document, view: &View, frame: &ViewFrame, dl: &mut DisplayList) {
    let scale = view.scale as f64;
    let (u0, u1) = match (frame.u_clip, dl.bounds()) {
        (Some(c), _) => c,
        (None, Some((min, max))) => (min.x, max.x),
        (None, None) => (-5000.0, 5000.0),
    };
    let margin = 1500.0;
    let (u0, u1) = (u0 - margin, u1 + margin);
    let levels = doc.levels();
    let r = BUBBLE_R * scale * 0.5;
    for (id, l) in &levels {
        let v = l.elevation;
        let s = Stroke::new(1.0, [0, 0, 0, 255]).dashed(Dash::Dashed);
        dl.line(DVec2::new(u0, v), DVec2::new(u1, v), s, Some(*id));
        // Level head: target symbol, name and elevation (right end).
        let c = DVec2::new(u1 + r, v);
        dl.push(
            Prim::Circle { center: c, radius: r },
            Some(THIN),
            Some([255, 255, 255, 255]),
            Some(*id),
        );
        dl.push(
            Prim::Polygon(vec![c, c + DVec2::new(r, 0.0), c + DVec2::new(0.0, r)]),
            None,
            Some([0, 0, 0, 255]),
            Some(*id),
        );
        dl.push(
            Prim::Polygon(vec![c, c - DVec2::new(r, 0.0), c - DVec2::new(0.0, r)]),
            None,
            Some([0, 0, 0, 255]),
            Some(*id),
        );
        let tx = c + DVec2::new(r * 1.5, r * 0.3);
        dl.push(
            Prim::Text {
                pos: tx,
                text: l.name.clone(),
                height: TEXT_H * scale * 0.8,
                anchor: Anchor::BottomLeft,
                angle: 0.0,
            },
            None,
            None,
            Some(*id),
        );
        let el = hausit_core::format_length(l.elevation, doc.length_unit);
        dl.push(
            Prim::Text {
                pos: tx - DVec2::new(0.0, TEXT_H * scale * 1.1),
                text: el,
                height: TEXT_H * scale * 0.7,
                anchor: Anchor::BottomLeft,
                angle: 0.0,
            },
            None,
            None,
            Some(*id),
        );
    }
    let zmin = levels.first().map(|l| l.1.elevation).unwrap_or(0.0) - 1500.0;
    let zmax = dl
        .bounds()
        .map(|b| b.1.y)
        .unwrap_or(0.0)
        .max(levels.last().map(|l| l.1.elevation).unwrap_or(0.0))
        + 1500.0;
    for e in doc.elements_of(Category::Grids) {
        let ElementData::Grid(g) = &e.data else { continue };
        let d = (g.end - g.start).normalize_or_zero();
        // Only grids roughly perpendicular to the view plane appear.
        if DVec3::new(d.x, d.y, 0.0).dot(frame.forward).abs() < 0.9 {
            continue;
        }
        let (uv, _) = frame.to_view(DVec3::new(g.start.x, g.start.y, 0.0));
        if let Some((a, b)) = frame.u_clip {
            if uv.x < a || uv.x > b {
                continue;
            }
        }
        grid_bubble(dl, DVec2::new(uv.x, zmin), DVec2::new(uv.x, zmax), &g.name, scale, e.id);
    }
}

/// Callout boundaries drawn in the view they were made from: the crop
/// rectangle, and a leader to a bubble with the callout's number. Tagged
/// with the callout view, so double-clicking opens it.
pub fn callout_marks(doc: &Document, view: &View, dl: &mut DisplayList) {
    let scale = view.scale as f64;
    for c in doc.views().filter(|c| c.callout_of == Some(view.id)) {
        let Some((lo, hi)) = c.crop else { continue };
        let id = Some(c.id);
        let s = Stroke::new(1.6, [0, 0, 0, 255]);
        let r = BUBBLE_R * scale;
        // Rounded corners, as callouts are drawn.
        let k = (r * 0.8).min((hi - lo).min_element() / 4.0);
        let mut pts = Vec::new();
        for (corner, a0) in [
            (DVec2::new(hi.x - k, lo.y + k), -std::f64::consts::FRAC_PI_2),
            (DVec2::new(hi.x - k, hi.y - k), 0.0),
            (DVec2::new(lo.x + k, hi.y - k), std::f64::consts::FRAC_PI_2),
            (DVec2::new(lo.x + k, lo.y + k), std::f64::consts::PI),
        ] {
            for i in 0..=4 {
                pts.push(corner + DVec2::from_angle(a0 + i as f64 / 4.0 * std::f64::consts::FRAC_PI_2) * k);
            }
        }
        dl.push(Prim::Polyline { pts, closed: true }, Some(s), None, id);
        let from = DVec2::new(hi.x, hi.y - k);
        let bubble = from + DVec2::new(r * 2.5, r * 2.5);
        dl.line(from, bubble - DVec2::new(r, 0.0), THIN, id);
        dl.push(
            Prim::Circle { center: bubble, radius: r },
            Some(THIN),
            Some([255, 255, 255, 255]),
            id,
        );
        // Numbered in the order they were drawn in this view.
        let number = doc
            .views()
            .filter(|v| v.callout_of == Some(view.id))
            .position(|v| v.id == c.id)
            .map(|i| (i + 1).to_string())
            .unwrap_or_default();
        dl.push(
            Prim::Text {
                pos: bubble,
                text: if number.is_empty() { "C".into() } else { number },
                height: TEXT_H * scale * 0.8,
                anchor: Anchor::Center,
                angle: 0.0,
            },
            None,
            None,
            id,
        );
    }
}
