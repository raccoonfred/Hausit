//! Room boundaries, as Revit computes them: the region around a room's
//! point enclosed by room-bounding walls (at their finish faces),
//! structural columns and room separation lines.

use crate::category::Category;
use crate::document::Document;
use crate::element::{ElementData, Room};
use crate::geometry::{column_extent, column_outline, wall_outline};
use hausit_core::geom::{line_intersection, point_in_polygon, polygon_area, polygon_perimeter};
use hausit_core::{DVec2, ElementId};

/// Height above the level at which boundaries are found (Revit's
/// computation height).
const COMPUTATION_HEIGHT: f64 = 1.0;
const TOL: f64 = 0.5;

/// Boundary segments on a level.
pub fn bounding_segments(doc: &Document, level: ElementId) -> Vec<(DVec2, DVec2)> {
    let z = doc.level_elevation(level) + COMPUTATION_HEIGHT;
    let mut segs = Vec::new();
    let poly = |pts: &[DVec2], segs: &mut Vec<(DVec2, DVec2)>| {
        for i in 0..pts.len() {
            segs.push((pts[i], pts[(i + 1) % pts.len()]));
        }
    };
    for e in doc.elements() {
        match &e.data {
            ElementData::Wall(w) if w.room_bounding && doc.wall_base_z(w) <= z && doc.wall_top_z(w) > z => {
                if let Some(o) = wall_outline(doc, e.id) {
                    poly(&o, &mut segs);
                }
            }
            ElementData::Column(c) => {
                let (z0, z1) = column_extent(doc, c);
                if z0 <= z && z1 > z {
                    if let Some(o) = column_outline(doc, e.id) {
                        poly(&o, &mut segs);
                    }
                }
            }
            ElementData::RoomSeparation(l) if l.level == level => segs.push((l.start, l.end)),
            _ => {}
        }
    }
    segs
}

/// Closed regions of the plan arrangement formed by `segs`, each
/// counter-clockwise.
pub fn regions(segs: &[(DVec2, DVec2)]) -> Vec<Vec<DVec2>> {
    // 1. Split every segment where others cross or touch it.
    let mut verts: Vec<DVec2> = Vec::new();
    let vid = |p: DVec2, verts: &mut Vec<DVec2>| -> usize {
        match verts.iter().position(|q| q.distance(p) < TOL) {
            Some(i) => i,
            None => {
                verts.push(p);
                verts.len() - 1
            }
        }
    };
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (i, &(a, b)) in segs.iter().enumerate() {
        let d = b - a;
        let len2 = d.length_squared();
        if len2 < TOL * TOL {
            continue;
        }
        let mut ts = vec![0.0, 1.0];
        for (j, &(c, e)) in segs.iter().enumerate() {
            if i == j {
                continue;
            }
            for p in [c, e] {
                let t = (p - a).dot(d) / len2;
                if t > 0.0 && t < 1.0 && (a + d * t).distance(p) < TOL {
                    ts.push(t);
                }
            }
            if let Some(x) = line_intersection(a, d, c, e - c) {
                let t = (x - a).dot(d) / len2;
                let u = (x - c).dot(e - c) / (e - c).length_squared().max(1e-12);
                if t > 0.0 && t < 1.0 && (-1e-9..=1.0 + 1e-9).contains(&u) {
                    ts.push(t);
                }
            }
        }
        ts.sort_by(f64::total_cmp);
        for w in ts.windows(2) {
            let (p, q) = (a + d * w[0], a + d * w[1]);
            if p.distance(q) < TOL {
                continue;
            }
            let (u, v) = (vid(p, &mut verts), vid(q, &mut verts));
            if u != v && !edges.contains(&(u, v)) && !edges.contains(&(v, u)) {
                edges.push((u, v));
            }
        }
    }

    // 2. Trace faces: at each vertex turn to the next edge clockwise
    //    from the one we arrived on, which keeps the face on the left.
    let mut out_edges: Vec<Vec<usize>> = vec![Vec::new(); verts.len()];
    for &(u, v) in &edges {
        out_edges[u].push(v);
        out_edges[v].push(u);
    }
    let angle = |u: usize, v: usize| {
        let d = verts[v] - verts[u];
        d.y.atan2(d.x)
    };
    for (u, list) in out_edges.iter_mut().enumerate() {
        list.sort_by(|a, b| angle(u, *a).total_cmp(&angle(u, *b)));
    }
    let mut used = std::collections::HashSet::new();
    let mut faces = Vec::new();
    for &(a, b) in &edges {
        for (s, t) in [(a, b), (b, a)] {
            if used.contains(&(s, t)) {
                continue;
            }
            let mut face = Vec::new();
            let (mut u, mut v) = (s, t);
            for _ in 0..edges.len() * 2 + 2 {
                if !used.insert((u, v)) {
                    break;
                }
                face.push(verts[u]);
                let list = &out_edges[v];
                let k = list.iter().position(|w| *w == u).unwrap_or(0);
                let next = list[(k + list.len() - 1) % list.len()];
                u = v;
                v = next;
                if (u, v) == (s, t) {
                    break;
                }
            }
            if face.len() >= 3 && polygon_area(&face) > 1.0 {
                faces.push(face);
            }
        }
    }
    faces
}

/// The boundary of a room, or `None` if it is not enclosed.
pub fn room_boundary(doc: &Document, room: &Room) -> Option<Vec<DVec2>> {
    let segs = bounding_segments(doc, room.level);
    regions(&segs)
        .into_iter()
        .filter(|f| point_in_polygon(room.point, f))
        .min_by(|a, b| polygon_area(a).total_cmp(&polygon_area(b)))
}

/// Computed values of a room.
#[derive(Clone, Debug, PartialEq)]
pub struct RoomInfo {
    /// `None` when the room is not enclosed ("Not Enclosed" in Revit).
    pub boundary: Option<Vec<DVec2>>,
    pub area: f64,
    pub perimeter: f64,
    pub height: f64,
    pub volume: f64,
}

pub fn room_info(doc: &Document, room: &Room) -> RoomInfo {
    let boundary = room_boundary(doc, room);
    let area = boundary.as_ref().map(|b| polygon_area(b).abs()).unwrap_or(0.0);
    let perimeter = boundary.as_ref().map(|b| polygon_perimeter(b)).unwrap_or(0.0);
    let base = doc.level_elevation(room.level);
    let upper = if doc.level(room.upper_limit).is_some() {
        room.upper_limit
    } else {
        room.level
    };
    let height = (doc.level_elevation(upper) + room.limit_offset - base).max(0.0);
    RoomInfo {
        boundary,
        area,
        perimeter,
        height,
        volume: area * height,
    }
}

/// Rooms whose point is inside the same enclosed region as `p` on `level`.
pub fn rooms_at(doc: &Document, level: ElementId, p: DVec2) -> Vec<ElementId> {
    let segs = bounding_segments(doc, level);
    let Some(region) = regions(&segs)
        .into_iter()
        .filter(|f| point_in_polygon(p, f))
        .min_by(|a, b| polygon_area(a).total_cmp(&polygon_area(b)))
    else {
        return vec![];
    };
    doc.elements_of(Category::Rooms)
        .filter_map(|e| match &e.data {
            ElementData::Room(r) if r.level == level && point_in_polygon(r.point, &region) => Some(e.id),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::sample_project;

    #[test]
    fn sample_rooms_are_enclosed() {
        let doc = sample_project();
        let level = doc.levels()[0].0;
        let segs = bounding_segments(&doc, level);
        let faces = regions(&segs);
        // The house's two rooms, at the walls' interior finish faces.
        let p = DVec2::new(2000.0, 4000.0);
        let room = faces
            .iter()
            .filter(|f| point_in_polygon(p, f))
            .min_by(|a, b| polygon_area(a).total_cmp(&polygon_area(b)));
        let room = room.expect("room around the left half");
        // 300 mm exterior walls, 135 mm partition at x = 6000.
        let a = polygon_area(room) / 1e6;
        let expect = (6000.0 - 67.5 - 150.0) * (8000.0 - 300.0) / 1e6;
        assert!((a - expect).abs() < 1e-6, "area {a}, expected {expect}");
    }
}
