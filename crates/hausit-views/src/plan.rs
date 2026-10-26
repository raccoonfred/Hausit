//! Floor plan graphics: elements cut at the view's cut plane drawn heavy and
//! filled, elements below drawn in projection, plus door swings, window
//! symbols, grids, and section/elevation marks.

use crate::annotate::{grid_bubble, view_marks};
use crate::{pen, Anchor, DisplayList, Prim, Rgba, Stroke, THIN};
use hausit_core::geom::perp;
use hausit_core::{DVec2, ElementId, LengthUnit};
use hausit_model::geometry::{column_extent, column_outline, element_geometry, opening_frame, roof_plan_lines, Material, Shape};
use hausit_model::rooms::room_info;
use hausit_model::{Category, Document, ElementData, ParamValue, View};

/// Paper height of tag text, mm.
const TAG_TEXT: f64 = 2.5;

/// Revit draws glazing in plan as thin black lines.
const GLASS: Rgba = [0, 0, 0, 255];

pub fn floor_plan(doc: &Document, view: &View, level: ElementId) -> DisplayList {
    let mut dl = DisplayList::default();
    let elev = doc.level_elevation(level);
    let cut_z = elev + view.cut_height;
    let top_z = doc.level_above(level).map(|l| doc.level_elevation(l)).unwrap_or(f64::INFINITY);
    let scale = view.scale as f64;

    // Floors on this level, in projection.
    for e in doc.elements_of(Category::Floors) {
        let ElementData::Floor(f) = &e.data else { continue };
        if f.level == level {
            dl.push(
                Prim::Polyline {
                    pts: f.boundary.clone(),
                    closed: true,
                },
                Some(pen(doc, Category::Floors, false, view.scale)),
                None,
                Some(e.id),
            );
        }
    }

    // Roofs based between this level and the next, in projection.
    for e in doc.elements_of(Category::Roofs) {
        let ElementData::Roof(r) = &e.data else { continue };
        let z = doc.level_elevation(r.level) + r.base_offset;
        if z >= elev - 1.0 && z < top_z {
            dl.push(
                Prim::Polyline {
                    pts: r.footprint.clone(),
                    closed: true,
                },
                Some(pen(doc, Category::Roofs, false, view.scale)),
                None,
                Some(e.id),
            );
            for (a, b) in roof_plan_lines(r) {
                dl.line(a, b, pen(doc, Category::Roofs, false, view.scale), Some(e.id));
            }
        }
    }

    // Rooms: an invisible region that can be picked, plus a room tag
    // (name, number and area, as Revit's "Room Tag With Area").
    for e in doc.elements_of(Category::Rooms) {
        let ElementData::Room(r) = &e.data else { continue };
        if r.level != level {
            continue;
        }
        let info = room_info(doc, r);
        if let Some(b) = &info.boundary {
            dl.push(Prim::Polygon(b.clone()), None, Some([0, 0, 0, 0]), Some(e.id));
        }
        let h = TAG_TEXT * scale;
        let area = match &info.boundary {
            Some(_) => format_area(info.area, doc.length_unit),
            None => "Not Enclosed".into(),
        };
        for (i, (text, k)) in [(r.name.clone(), 1.0), (r.number.clone(), 1.0), (area, 0.85)]
            .into_iter()
            .enumerate()
        {
            dl.push(
                Prim::Text {
                    pos: r.point + DVec2::new(0.0, h * (1.3 - 1.3 * i as f64)),
                    text,
                    height: h * k,
                    anchor: Anchor::Center,
                    angle: 0.0,
                },
                None,
                None,
                Some(e.id),
            );
        }
    }
    for e in doc.elements_of(Category::RoomSeparation) {
        let ElementData::RoomSeparation(l) = &e.data else { continue };
        if l.level == level {
            dl.line(l.start, l.end, pen(doc, Category::RoomSeparation, false, view.scale), Some(e.id));
        }
    }

    // Walls (and door/window frames): projection below the cut, then cut.
    let mut cut_edges: Vec<(DVec2, DVec2, ElementId)> = Vec::new();
    for e in doc.elements_of(Category::Walls) {
        let Some(g) = element_geometry(doc, e.id) else { continue };
        for part in &g.parts {
            // Straight pieces and pieces attached to a roof above.
            let Some((footprint, z0, z1)) = part.vertical_extent() else {
                continue;
            };
            if z0 <= cut_z && cut_z < z1 {
                dl.push(Prim::Polygon(footprint.clone()), None, Some(CUT_FILL_LIGHT), Some(e.id));
                let n = footprint.len();
                for i in 0..n {
                    cut_edges.push((footprint[i], footprint[(i + 1) % n], e.id));
                }
            } else if z1 <= cut_z && z1 > elev - 1.0 {
                dl.push(
                    Prim::Polyline {
                        pts: footprint.clone(),
                        closed: true,
                    },
                    Some(pen(doc, Category::Walls, false, view.scale)),
                    None,
                    Some(e.id),
                );
            }
        }
    }
    // Stroke only cut edges that are not shared by two pieces, so mitred
    // corners and wall segments read as one continuous wall.
    let key = |a: DVec2, b: DVec2| {
        let q = |p: DVec2| ((p.x * 2.0).round() as i64, (p.y * 2.0).round() as i64);
        let (qa, qb) = (q(a), q(b));
        if qa < qb {
            (qa, qb)
        } else {
            (qb, qa)
        }
    };
    let mut edge_count: std::collections::HashMap<_, usize> = std::collections::HashMap::new();
    for (a, b, _) in &cut_edges {
        *edge_count.entry(key(*a, *b)).or_default() += 1;
    }
    let shared = |a: DVec2, b: DVec2| edge_count.get(&key(a, b)).copied().unwrap_or(0) > 1;
    for (a, b, id) in &cut_edges {
        if !shared(*a, *b) {
            dl.line(*a, *b, pen(doc, Category::Walls, true, view.scale), Some(*id));
        }
    }

    // Structural columns: cut ones filled with a heavy outline, columns
    // ending below the cut plane in projection.
    for e in doc.elements_of(Category::StructuralColumns) {
        let ElementData::Column(c) = &e.data else { continue };
        let (z0, z1) = column_extent(doc, c);
        let Some(outline) = column_outline(doc, e.id) else { continue };
        if z0 <= cut_z && cut_z < z1 {
            dl.push(
                Prim::Polygon(outline),
                Some(pen(doc, Category::StructuralColumns, true, view.scale)),
                Some(COLUMN_FILL),
                Some(e.id),
            );
        } else if z1 <= cut_z && z1 > elev - 1.0 {
            dl.push(
                Prim::Polyline {
                    pts: outline,
                    closed: true,
                },
                Some(pen(doc, Category::StructuralColumns, false, view.scale)),
                None,
                Some(e.id),
            );
        }
    }

    // Doors and windows hosted by walls on this level.
    let mut tags: Vec<(Vec<DVec2>, String, DVec2, ElementId)> = Vec::new();
    for e in doc.elements() {
        let is_door = match e.category() {
            Category::Doors => true,
            Category::Windows => false,
            _ => continue,
        };
        let Some(o) = e.as_opening() else { continue };
        let Some(host) = doc.element(o.host).and_then(|h| h.as_wall()) else {
            continue;
        };
        if host.base_level != level {
            continue;
        }
        let Some(f) = opening_frame(doc, e.id) else { continue };
        if f.z0 > cut_z || f.z0 + f.height < cut_z {
            continue;
        }
        let n = perp(f.dir);
        let hw = f.width / 2.0;
        let half_t = f.wall_thickness / 2.0;
        if is_door {
            let leaves = doc
                .type_of(e.id)
                .and_then(|t| match t.params.get("Leaves") {
                    Some(hausit_model::ParamValue::Integer(n)) => Some(*n),
                    _ => None,
                })
                .unwrap_or(1);
            // Doors swing to the interior (right of the wall) by default.
            let facing = if o.flip_facing { n } else { -n };
            let hinge_sign = if o.flip_hand { 1.0 } else { -1.0 };
            let face = f.center + facing * half_t;
            let mut leaf = |hinge: DVec2, to_closed: DVec2, width: f64| {
                let open_end = hinge + facing * width;
                let t = f.thickness.min(width * 0.1);
                let side = to_closed * t;
                dl.push(
                    Prim::Polygon(vec![hinge, open_end, open_end + side, hinge + side]),
                    Some(pen(doc, Category::Doors, true, view.scale)),
                    Some([255, 255, 255, 255]),
                    Some(e.id),
                );
                let a0 = facing.y.atan2(facing.x);
                let a1 = to_closed.y.atan2(to_closed.x);
                let mut sweep = a1 - a0;
                while sweep > std::f64::consts::PI {
                    sweep -= std::f64::consts::TAU;
                }
                while sweep < -std::f64::consts::PI {
                    sweep += std::f64::consts::TAU;
                }
                dl.push(
                    Prim::Arc {
                        center: hinge,
                        radius: width,
                        start: a0,
                        end: a0 + sweep,
                    },
                    Some(pen(doc, Category::Doors, false, view.scale)),
                    None,
                    Some(e.id),
                );
            };
            if leaves >= 2 {
                leaf(face - f.dir * hw, f.dir, hw);
                leaf(face + f.dir * hw, -f.dir, hw);
            } else {
                let hinge = face + f.dir * hw * hinge_sign;
                leaf(hinge, -f.dir * hinge_sign, f.width);
            }
        } else {
            // Window: frame outline and double glass line, as in Revit.
            let a = f.center - f.dir * hw;
            let b = f.center + f.dir * hw;
            for off in [half_t, -half_t] {
                dl.line(a + n * off, b + n * off, pen(doc, Category::Windows, false, view.scale), Some(e.id));
            }
            for off in [25.0, -25.0] {
                dl.line(a + n * off, b + n * off, Stroke::new(1.0, GLASS), Some(e.id));
            }
            dl.line(
                a + n * half_t,
                a - n * half_t,
                pen(doc, Category::Windows, true, view.scale),
                Some(e.id),
            );
            dl.line(
                b + n * half_t,
                b - n * half_t,
                pen(doc, Category::Windows, true, view.scale),
                Some(e.id),
            );
        }
        // Tags: doors show their Mark, windows their Type Mark.
        let h = TAG_TEXT * scale;
        let (label, at) = if is_door {
            let facing = if o.flip_facing { n } else { -n };
            (e.mark.clone(), f.center + facing * (half_t + h * 2.2))
        } else {
            let tm = doc.type_of(e.id).and_then(|t| match t.params.get("Type Mark") {
                Some(ParamValue::Text(s)) => Some(s.clone()),
                _ => None,
            });
            (tm.unwrap_or_default(), f.center + n * (half_t + h * 2.2))
        };
        if !label.is_empty() {
            let (hw, hh) = (h * (0.5 + 0.35 * label.chars().count() as f64), h * 0.75);
            let shape = if is_door {
                vec![
                    at + DVec2::new(-hw, -hh),
                    at + DVec2::new(hw, -hh),
                    at + DVec2::new(hw, hh),
                    at + DVec2::new(-hw, hh),
                ]
            } else {
                vec![
                    at + DVec2::new(-hw - hh * 0.6, 0.0),
                    at + DVec2::new(-hw, -hh),
                    at + DVec2::new(hw, -hh),
                    at + DVec2::new(hw + hh * 0.6, 0.0),
                    at + DVec2::new(hw, hh),
                    at + DVec2::new(-hw, hh),
                ]
            };
            tags.push((shape, label, at, e.id));
        }
        // Cut frame pieces (jambs) drawn as part of the opening.
        if let Some(g) = element_geometry(doc, e.id) {
            for part in &g.parts {
                if let (Shape::Prism(p), Material::Default) = (&part.shape, part.material) {
                    if p.z0 <= cut_z && cut_z < p.z1 && is_door {
                        dl.push(
                            Prim::Polygon(p.footprint.clone()),
                            Some(THIN),
                            Some([255, 255, 255, 255]),
                            Some(e.id),
                        );
                    }
                }
            }
        }
    }

    for (shape, label, at, id) in tags {
        dl.push(Prim::Polygon(shape), Some(THIN), Some([255, 255, 255, 255]), Some(id));
        dl.push(
            Prim::Text {
                pos: at,
                text: label,
                height: TAG_TEXT * scale,
                anchor: Anchor::Center,
                angle: 0.0,
            },
            None,
            None,
            Some(id),
        );
    }

    // Dimensions placed in this view.
    for e in doc.elements_of(Category::Dimensions) {
        let ElementData::Dimension(d) = &e.data else { continue };
        if d.view != view.id {
            continue;
        }
        if let Some(l) = hausit_model::dimensions::layout(doc, d) {
            crate::draw_dimension(&mut dl, &l, scale, doc.length_unit, Some(e.id));
        }
    }

    // Grids.
    for e in doc.elements_of(Category::Grids) {
        let ElementData::Grid(g) = &e.data else { continue };
        grid_bubble(&mut dl, g.start, g.end, &g.name, scale, e.id);
    }

    view_marks(doc, view, &mut dl);
    dl
}

/// Areas as Revit's default tag shows them.
fn format_area(mm2: f64, unit: LengthUnit) -> String {
    match unit {
        LengthUnit::FeetInches => format!("{:.0} SF", mm2 / (304.8 * 304.8)),
        _ => format!("{:.2} m²", mm2 / 1e6),
    }
}

/// Concrete columns cut in plan.
const COLUMN_FILL: Rgba = [150, 150, 150, 255];

/// Generic walls have no cut pattern in Revit: white inside heavy outlines.
const CUT_FILL_LIGHT: Rgba = [255, 255, 255, 255];
