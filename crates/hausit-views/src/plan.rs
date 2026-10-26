//! Floor plan graphics: elements cut at the view's cut plane drawn heavy and
//! filled, elements below drawn in projection, plus door swings, window
//! symbols, grids, and section/elevation marks.

use crate::annotate::{grid_bubble, view_marks};
use crate::{DisplayList, Prim, Rgba, Stroke, CUT_FILL, HEAVY, MEDIUM, THIN};
use hausit_core::geom::perp;
use hausit_core::{DVec2, ElementId};
use hausit_model::geometry::{element_geometry, opening_frame, roof_plan_lines, Material, Shape};
use hausit_model::{Category, Document, ElementData, View};

const GLASS: Rgba = [80, 150, 220, 255];

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
                Some(THIN),
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
                Some(MEDIUM),
                None,
                Some(e.id),
            );
            for (a, b) in roof_plan_lines(r) {
                dl.line(a, b, THIN, Some(e.id));
            }
        }
    }

    // Walls (and door/window frames): projection below the cut, then cut.
    let mut cut_edges: Vec<(DVec2, DVec2, ElementId)> = Vec::new();
    for e in doc.elements_of(Category::Walls) {
        let Some(g) = element_geometry(doc, e.id) else { continue };
        for part in &g.parts {
            let Shape::Prism(p) = &part.shape else { continue };
            if p.z0 <= cut_z && cut_z < p.z1 {
                dl.push(Prim::Polygon(p.footprint.clone()), None, Some(CUT_FILL_LIGHT), Some(e.id));
                let n = p.footprint.len();
                for i in 0..n {
                    cut_edges.push((p.footprint[i], p.footprint[(i + 1) % n], e.id));
                }
            } else if p.z1 <= cut_z && p.z1 > elev - 1.0 {
                dl.push(
                    Prim::Polyline {
                        pts: p.footprint.clone(),
                        closed: true,
                    },
                    Some(THIN),
                    None,
                    Some(e.id),
                );
            }
        }
    }
    // Stroke only cut edges that are not shared by two pieces, so mitred
    // corners and wall segments read as one continuous wall.
    let shared = |a: DVec2, b: DVec2| {
        cut_edges
            .iter()
            .filter(|(c, d, _)| (a.distance(*d) < 0.5 && b.distance(*c) < 0.5) || (a.distance(*c) < 0.5 && b.distance(*d) < 0.5))
            .count()
            > 1
    };
    for (a, b, id) in &cut_edges {
        if !shared(*a, *b) {
            dl.line(*a, *b, HEAVY, Some(*id));
        }
    }

    // Doors and windows hosted by walls on this level.
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
                    Some(MEDIUM),
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
                    Some(THIN),
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
                dl.line(a + n * off, b + n * off, THIN, Some(e.id));
            }
            for off in [25.0, -25.0] {
                dl.line(a + n * off, b + n * off, Stroke::new(1.4, GLASS), Some(e.id));
            }
            dl.line(a + n * half_t, a - n * half_t, MEDIUM, Some(e.id));
            dl.line(b + n * half_t, b - n * half_t, MEDIUM, Some(e.id));
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

    // Grids.
    for e in doc.elements_of(Category::Grids) {
        let ElementData::Grid(g) = &e.data else { continue };
        grid_bubble(&mut dl, g.start, g.end, &g.name, scale, e.id);
    }

    view_marks(doc, view, &mut dl);
    dl
}

const CUT_FILL_LIGHT: Rgba = [CUT_FILL[0] + 104, CUT_FILL[1] + 104, CUT_FILL[2] + 104, 255];
