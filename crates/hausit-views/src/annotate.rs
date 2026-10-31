//! Datum and view annotations: grid bubbles, level heads, section and
//! elevation marks.

use crate::frame::ViewFrame;
use crate::{Anchor, Dash, DisplayList, Prim, Stroke, GRID_COLOR, HEAVY, MEDIUM, THIN};
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
            ViewKind::Elevation { direction, at } => {
                let out = direction.outward();
                let r = BUBBLE_R * scale * 1.2;
                let p = match at {
                    // A placed mark points away from where it looks.
                    Some(at) => *at,
                    None => {
                        let Some(b) = model_bounds(doc) else { continue };
                        let c = b.center();
                        let half = match direction {
                            CardinalDirection::North | CardinalDirection::South => b.size().y / 2.0,
                            _ => b.size().x / 2.0,
                        };
                        DVec2::new(c.x, c.y) + out * (half + 4.0 * r)
                    }
                };

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

/// Text notes and tags placed in this view. Plans use model x and y;
/// elevations and sections their own view coordinates, with tags pointing
/// at their element's projection.
pub fn notes_and_tags(doc: &Document, view: &View, frame: Option<&ViewFrame>, dl: &mut DisplayList) {
    let scale = view.scale as f64;
    for e in doc.elements_of(Category::TextNotes) {
        let ElementData::TextNote(t) = &e.data else { continue };
        if t.view == view.id {
            text_note(dl, t, scale, e.id);
        }
    }
    for e in doc.elements_of(Category::Tags) {
        let ElementData::Tag(t) = &e.data else { continue };
        if t.view != view.id {
            continue;
        }
        let Some(host) = doc.element(t.host) else { continue };
        let anchor = hausit_model::annotation::anchor(doc, t.host).map(|p| match frame {
            None => p.truncate(),
            Some(f) => f.to_view(p).0,
        });
        let label = hausit_model::annotation::tag_label(doc, t);
        tag(
            dl,
            t.position,
            &label,
            host.category(),
            if t.leader { anchor } else { None },
            scale,
            e.id,
        );
    }
}

/// Detail lines, filled regions and spot elevations placed in this view.
/// Regions go first so lines and labels draw over them.
pub fn detailing(doc: &Document, view: &View, dl: &mut DisplayList) {
    use hausit_model::detail::LineStyle;
    let scale = view.scale as f64;
    let stroke = |style: LineStyle| match style {
        LineStyle::Thin => Some(THIN),
        LineStyle::Medium => Some(MEDIUM),
        LineStyle::Wide => Some(HEAVY),
        LineStyle::Hidden => Some(THIN.dashed(Dash::Dashed)),
        LineStyle::Invisible => None,
    };
    for e in doc.elements_of(Category::DetailItems) {
        let ElementData::FilledRegion(r) = &e.data else { continue };
        if r.view != view.id || r.boundary.len() < 3 {
            continue;
        }
        let [cr, cg, cb] = r.color;
        let fill = if r.masking {
            [255, 255, 255, 255]
        } else if r.pattern == hausit_model::materials::FillPattern::Solid {
            [cr, cg, cb, 255]
        } else {
            // Transparent, but still pickable.
            [255, 255, 255, 1]
        };
        dl.push(Prim::Polygon(r.boundary.clone()), None, Some(fill), Some(e.id));
        if !r.masking && r.pattern != hausit_model::materials::FillPattern::Solid {
            let pen = Stroke::new(0.8, [cr, cg, cb, 255]);
            for (a, b) in crate::hatch::pattern_lines(&r.boundary, r.pattern, DVec2::X, scale) {
                dl.line(a, b, pen, Some(e.id));
            }
        }
        if let Some(s) = stroke(r.border) {
            dl.push(
                Prim::Polyline {
                    pts: r.boundary.clone(),
                    closed: true,
                },
                Some(s),
                None,
                Some(e.id),
            );
        }
    }
    for e in doc.elements_of(Category::Lines) {
        let ElementData::DetailLine(l) = &e.data else { continue };
        if l.view == view.id {
            dl.line(l.start, l.end, stroke(l.style).unwrap_or(THIN), Some(e.id));
        }
    }
    for e in doc.elements_of(Category::RevisionClouds) {
        let ElementData::RevisionCloud(c) = &e.data else { continue };
        if c.view != view.id || c.boundary.len() < 2 {
            continue;
        }
        revision_cloud(doc, c, scale, e.id, dl);
    }
    for e in doc.elements_of(Category::SpotElevations) {
        let ElementData::SpotElevation(s) = &e.data else { continue };
        if s.view != view.id {
            continue;
        }
        let z = doc.spot_height(s);
        let text = format!(
            "{}{}",
            if z >= 0.5 {
                "+"
            } else if z <= -0.5 {
                ""
            } else {
                "±"
            },
            hausit_core::format_length(z, doc.length_unit)
        );
        spot(dl, s.point, s.label_at, &text, scale, e.id);
    }
}

/// Revision cloud colour.
const CLOUD: [u8; 4] = [200, 30, 30, 255];

/// A revision cloud: arcs bulging out along its boundary, and its tag, a
/// triangle with the revision number.
pub fn revision_cloud(doc: &Document, c: &hausit_model::RevisionCloud, scale: f64, id: ElementId, dl: &mut DisplayList) {
    let pen = Stroke::new(1.2, CLOUD);
    let outline = hausit_model::revisions::cloud_outline(&c.boundary, hausit_model::revisions::CLOUD_ARC * scale);
    // Invisible fill so a click inside picks the cloud.
    dl.push(Prim::Polygon(c.boundary.clone()), None, Some([255, 255, 255, 0]), Some(id));
    dl.push(
        Prim::Polyline {
            pts: outline,
            closed: true,
        },
        Some(pen),
        None,
        Some(id),
    );
    if let Some(t) = c.tag {
        let s = 4.5 * scale;
        let tri = vec![
            t + DVec2::new(-s * 0.8, -s * 0.45),
            t + DVec2::new(s * 0.8, -s * 0.45),
            t + DVec2::new(0.0, s * 0.95),
        ];
        dl.push(Prim::Polygon(tri), Some(pen), Some([255, 255, 255, 255]), Some(id));
        dl.push(
            Prim::Text {
                pos: t,
                text: doc.revision_number(c.revision),
                height: 2.2 * scale,
                anchor: Anchor::Center,
                angle: 0.0,
            },
            None,
            None,
            Some(id),
        );
    }
}

/// The colour fill legend of a view's colour scheme, with its top left
/// corner at the view's legend point.
pub fn color_legend(doc: &Document, view: &View, dl: &mut DisplayList) {
    let (Some(at), Some(by)) = (view.color_legend, view.color_scheme) else {
        return;
    };
    let (_, legend) = doc.color_fill(view.id);
    let scale = view.scale as f64;
    let h = 2.5 * scale;
    let what = if view.area_scheme.is_some() { "Area" } else { "Room" };
    dl.push(
        Prim::Text {
            pos: at,
            text: format!("{what} Legend: {}", by.label()),
            height: h * 1.2,
            anchor: Anchor::Left,
            angle: 0.0,
        },
        None,
        None,
        None,
    );
    for (i, entry) in legend.iter().enumerate() {
        let y = at.y - h * 2.2 * (i as f64 + 1.0);
        let [r, g, b] = entry.color;
        let sw = vec![
            DVec2::new(at.x, y - h * 0.6),
            DVec2::new(at.x + h * 2.0, y - h * 0.6),
            DVec2::new(at.x + h * 2.0, y + h * 0.6),
            DVec2::new(at.x, y + h * 0.6),
        ];
        dl.push(Prim::Polygon(sw), Some(THIN), Some([r, g, b, 255]), None);
        dl.push(
            Prim::Text {
                pos: DVec2::new(at.x + h * 2.6, y),
                text: entry.value.clone(),
                height: h,
                anchor: Anchor::Left,
                angle: 0.0,
            },
            None,
            None,
            None,
        );
    }
}

/// A spot elevation: a small target at the point, a leader with a
/// shoulder, and the height over the shoulder.
fn spot(dl: &mut DisplayList, at: DVec2, label: DVec2, text: &str, scale: f64, id: ElementId) {
    let r = 1.2 * scale;
    dl.push(
        Prim::Circle { center: at, radius: r },
        Some(THIN),
        Some([255, 255, 255, 255]),
        Some(id),
    );
    dl.push(
        Prim::Polygon(vec![at, at + DVec2::new(r, 0.0), at + DVec2::new(0.0, r)]),
        None,
        Some([0, 0, 0, 255]),
        Some(id),
    );
    dl.push(
        Prim::Polygon(vec![at, at - DVec2::new(r, 0.0), at - DVec2::new(0.0, r)]),
        None,
        Some([0, 0, 0, 255]),
        Some(id),
    );
    let h = 2.5 * scale;
    let width = text_width(text, h) + h * 0.4;
    let toward = if label.x >= at.x { 1.0 } else { -1.0 };
    let end = label + DVec2::new(width * toward, 0.0);
    let d = (label - at).normalize_or_zero();
    dl.line(at + d * r, label, THIN, Some(id));
    dl.line(label, end, THIN, Some(id));
    dl.push(
        Prim::Text {
            pos: (label + end) * 0.5 + DVec2::new(0.0, h * 0.7),
            text: text.to_string(),
            height: h,
            anchor: Anchor::Center,
            angle: 0.0,
        },
        None,
        None,
        Some(id),
    );
}

/// Rough width of text on paper, for boxes and picking.
fn text_width(text: &str, height: f64) -> f64 {
    text.chars().count() as f64 * height * 0.6
}

fn text_note(dl: &mut DisplayList, t: &hausit_model::TextNote, scale: f64, id: ElementId) {
    let h = t.size * scale;
    let lines: Vec<&str> = t.text.lines().collect();
    let lines = if lines.is_empty() { vec![""] } else { lines };
    let line_h = h * 1.5;
    let width = lines.iter().map(|l| text_width(l, h)).fold(h, f64::max);
    // An invisible box so the whole note can be picked.
    let top = t.position;
    let bottom = top - DVec2::new(0.0, line_h * lines.len() as f64);
    dl.push(
        Prim::Polygon(vec![
            DVec2::new(top.x, bottom.y),
            DVec2::new(top.x + width, bottom.y),
            DVec2::new(top.x + width, top.y),
            top,
        ]),
        None,
        Some([0, 0, 0, 0]),
        Some(id),
    );
    for (i, line) in lines.iter().enumerate() {
        dl.push(
            Prim::Text {
                pos: top - DVec2::new(0.0, line_h * (i as f64 + 0.5)),
                text: line.to_string(),
                height: h,
                anchor: Anchor::Left,
                angle: 0.0,
            },
            None,
            None,
            Some(id),
        );
    }
    if let Some(end) = t.leader {
        // From the nearer side of the first line to the point, with an
        // arrowhead.
        let first = top - DVec2::new(0.0, line_h * 0.5);
        let start = if end.x < top.x + width / 2.0 {
            first - DVec2::new(h * 0.3, 0.0)
        } else {
            first + DVec2::new(width + h * 0.3, 0.0)
        };
        arrow(dl, start, end, scale, id);
    }
}

/// A leader line ending in a filled arrowhead.
fn arrow(dl: &mut DisplayList, from: DVec2, to: DVec2, scale: f64, id: ElementId) {
    dl.line(from, to, THIN, Some(id));
    let d = (to - from).normalize_or_zero();
    if d == DVec2::ZERO {
        return;
    }
    let len = 2.0 * scale;
    let n = perp(d) * len * 0.3;
    dl.push(
        Prim::Polygon(vec![to, to - d * len + n, to - d * len - n]),
        Some(THIN),
        Some([0, 0, 0, 255]),
        Some(id),
    );
}

/// A tag: its label in a box shaped by category (a rectangle for doors, a
/// hexagon for windows, rounded ends for other elements), with an
/// optional leader to its element.
fn tag(dl: &mut DisplayList, at: DVec2, label: &str, cat: Category, leader: Option<DVec2>, scale: f64, id: ElementId) {
    let h = hausit_model::annotation::TAG_TEXT * scale;
    let (hw, hh) = (h * 0.5 + text_width(label, h) / 2.0, h * 0.75);
    let corner = |x: f64, y: f64| at + DVec2::new(x, y);
    let shape = match cat {
        Category::Doors => vec![corner(-hw, -hh), corner(hw, -hh), corner(hw, hh), corner(-hw, hh)],
        Category::Windows => vec![
            corner(-hw - hh * 0.6, 0.0),
            corner(-hw, -hh),
            corner(hw, -hh),
            corner(hw + hh * 0.6, 0.0),
            corner(hw, hh),
            corner(-hw, hh),
        ],
        _ => {
            // Rounded ends.
            let mut pts = Vec::new();
            for (cx, a0) in [(hw, -90.0_f64), (-hw, 90.0)] {
                for k in 0..=6 {
                    let a = (a0 + 30.0 * k as f64).to_radians();
                    pts.push(corner(cx + hh * a.cos(), hh * a.sin()));
                }
            }
            pts
        }
    };
    if let Some(p) = leader {
        // From the box edge nearest the element.
        let start = if (p.x - at.x).abs() * hh > (p.y - at.y).abs() * hw {
            at + DVec2::new(hw.copysign(p.x - at.x), 0.0)
        } else {
            at + DVec2::new(0.0, hh.copysign(p.y - at.y))
        };
        dl.line(start, p, THIN, Some(id));
    }
    dl.push(Prim::Polygon(shape), Some(THIN), Some([255, 255, 255, 255]), Some(id));
    dl.push(
        Prim::Text {
            pos: at,
            text: label.to_string(),
            height: h,
            anchor: Anchor::Center,
            angle: 0.0,
        },
        None,
        None,
        Some(id),
    );
}
