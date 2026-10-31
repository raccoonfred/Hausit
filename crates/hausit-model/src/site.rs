//! Site: toposolids (terrain with a thickness, shaped by points), their
//! contour lines, and property lines.
//!
//! A toposolid is sketched as a boundary on a level, like a floor, then
//! shaped with points: each point has a height above the level. Boundary
//! corners with no point stay at the level. Its top is triangulated so
//! that the boundary edges are kept; its bottom is flat, one type
//! thickness below the lowest point of the top.

use crate::category::Category;
use crate::document::{Document, ModelError};
use crate::element::ElementData;
use crate::family::TypeParams;
use crate::params::ParamValue;
use hausit_core::geom::{distance_to_segment, point_in_polygon, polygon_area};
use hausit_core::{DVec2, DVec3, ElementId};
use serde::{Deserialize, Serialize};

pub const TOPOSOLID: &str = "Toposolid";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Toposolid {
    pub type_id: ElementId,
    pub level: ElementId,
    pub boundary: Vec<DVec2>,
    /// Shape points: plan position, and height above the level (plus
    /// the offset).
    #[serde(default)]
    pub points: Vec<DVec3>,
    #[serde(default)]
    pub height_offset: f64,
}

/// A property line: the legal boundary of the site, drawn on a level.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PropertyLine {
    pub level: ElementId,
    pub points: Vec<DVec2>,
    #[serde(default = "yes")]
    pub closed: bool,
}

fn yes() -> bool {
    true
}

/// Project-wide site settings (Massing & Site ▸ Site Settings).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SiteSettings {
    /// Contour interval, mm. Zero hides contours.
    pub contour_interval: f64,
    /// Every n-th contour is a primary (heavier) one.
    pub primary_every: u32,
}

impl Default for SiteSettings {
    fn default() -> Self {
        SiteSettings {
            contour_interval: 500.0,
            primary_every: 5,
        }
    }
}

pub fn toposolid_type_params(thickness: f64, material: &str) -> TypeParams {
    let mut p = TypeParams::new();
    p.insert("Thickness".into(), ParamValue::Length(thickness));
    p.insert("Material".into(), ParamValue::Text(material.into()));
    p.insert("Type Mark".into(), ParamValue::Text(String::new()));
    p
}

fn cross(a: DVec2, b: DVec2) -> f64 {
    a.x * b.y - a.y * b.x
}

/// Whether `d` is inside the circumcircle of the counter-clockwise
/// triangle `a b c`.
fn in_circle(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let (ad, bd, cd) = (a - d, b - d, c - d);
    let det = (ad.length_squared()) * cross(bd, cd) - (bd.length_squared()) * cross(ad, cd) + (cd.length_squared()) * cross(ad, bd);
    det > 1e-6
}

/// Triangulates a polygon with points inside it, keeping the polygon's
/// edges: ear clipping, then inserting each point into the triangle that
/// holds it, then flipping interior edges until the mesh is Delaunay.
/// Returns the vertices (polygon first, then points) and counter-clockwise
/// index triples.
pub fn constrained_mesh(boundary: &[DVec2], inner: &[DVec2]) -> (Vec<DVec2>, Vec<[usize; 3]>) {
    let mut verts: Vec<DVec2> = boundary.to_vec();
    let mut tris: Vec<[usize; 3]> = hausit_core::geom::triangulate(boundary);
    let ccw = |t: [usize; 3], v: &[DVec2]| {
        if cross(v[t[1]] - v[t[0]], v[t[2]] - v[t[0]]) < 0.0 {
            [t[0], t[2], t[1]]
        } else {
            t
        }
    };
    tris = tris.into_iter().map(|t| ccw(t, &verts)).collect();
    let n_boundary = verts.len();
    let is_boundary_edge =
        |a: usize, b: usize| a < n_boundary && b < n_boundary && ((a + 1) % n_boundary == b || (b + 1) % n_boundary == a);
    for &p in inner {
        if verts.iter().any(|v| v.distance(p) < 1.0) {
            continue;
        }
        // Every triangle holding the point: one, or two when it sits on
        // a shared edge.
        let inside = |t: &[usize; 3]| {
            let (a, b, c) = (verts[t[0]], verts[t[1]], verts[t[2]]);
            let tol = -1e-6 * (b - a).length().max(1.0);
            cross(b - a, p - a) / (b - a).length().max(1e-9) >= tol
                && cross(c - b, p - b) / (c - b).length().max(1e-9) >= tol
                && cross(a - c, p - c) / (a - c).length().max(1e-9) >= tol
        };
        let holding: Vec<[usize; 3]> = tris.iter().filter(|t| inside(t)).copied().collect();
        if holding.is_empty() {
            continue;
        }
        tris.retain(|t| !inside(t));
        let i = verts.len();
        verts.push(p);
        for t in holding {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                // Skip the sliver when the point sits on an edge.
                if cross(verts[b] - verts[a], p - verts[a]).abs() / verts[a].distance(verts[b]).max(1e-9) > 1e-3 {
                    tris.push([a, b, i]);
                }
            }
        }
    }
    // Lawson flips.
    for _ in 0..verts.len() * 4 {
        let mut flipped = false;
        'outer: for x in 0..tris.len() {
            for e in 0..3 {
                let (a, b) = (tris[x][e], tris[x][(e + 1) % 3]);
                if is_boundary_edge(a, b) {
                    continue;
                }
                let c = tris[x][(e + 2) % 3];
                let Some((y, d)) = tris.iter().enumerate().find_map(|(y, t)| {
                    (y != x)
                        .then(|| (0..3).find(|f| t[*f] == b && t[(f + 1) % 3] == a).map(|f| (y, t[(f + 2) % 3])))
                        .flatten()
                }) else {
                    continue;
                };
                if !in_circle(verts[a], verts[b], verts[c], verts[d]) {
                    continue;
                }
                // The quad a d b c must be convex for the flip.
                let (pa, pb, pc, pd) = (verts[a], verts[b], verts[c], verts[d]);
                if cross(pd - pc, pa - pc) <= 1e-9 || cross(pb - pc, pd - pc) <= 1e-9 {
                    continue;
                }
                tris[x] = [c, a, d];
                tris[y] = [d, b, c];
                flipped = true;
                break 'outer;
            }
        }
        if !flipped {
            break;
        }
    }
    (verts, tris)
}

impl Toposolid {
    /// Height above the level of a boundary corner or point at `p`.
    fn height_of(&self, p: DVec2) -> f64 {
        self.points
            .iter()
            .find(|q| q.truncate().distance(p) < 1.0)
            .map(|q| q.z)
            .unwrap_or(0.0)
    }

    /// The top surface as triangles, heights relative to the level (with
    /// the offset).
    pub fn surface(&self) -> Vec<[DVec3; 3]> {
        if self.boundary.len() < 3 {
            return Vec::new();
        }
        let inner: Vec<DVec2> = self
            .points
            .iter()
            .map(|p| p.truncate())
            .filter(|p| point_in_polygon(*p, &self.boundary))
            .filter(|p| {
                let n = self.boundary.len();
                (0..n).all(|i| distance_to_segment(*p, self.boundary[i], self.boundary[(i + 1) % n]) > 1.0)
            })
            .collect();
        let (verts, tris) = constrained_mesh(&self.boundary, &inner);
        let z: Vec<f64> = verts.iter().map(|v| self.height_of(*v) + self.height_offset).collect();
        tris.iter().map(|t| t.map(|i| DVec3::new(verts[i].x, verts[i].y, z[i]))).collect()
    }

    /// Height of the top above the level at `p`, if inside.
    pub fn height_at(&self, p: DVec2) -> Option<f64> {
        for [a, b, c] in self.surface() {
            let (a2, b2, c2) = (a.truncate(), b.truncate(), c.truncate());
            let area = cross(b2 - a2, c2 - a2);
            if area.abs() < 1e-9 {
                continue;
            }
            let u = cross(b2 - p, c2 - p) / area;
            let v = cross(c2 - p, a2 - p) / area;
            let w = 1.0 - u - v;
            if u >= -1e-9 && v >= -1e-9 && w >= -1e-9 {
                return Some(u * a.z + v * b.z + w * c.z);
            }
        }
        None
    }

    /// Plan contour segments of the top at every `interval` (absolute
    /// height, from `base`, the level's elevation), each with whether it
    /// is a primary contour.
    pub fn contours(&self, base: f64, interval: f64, primary_every: u32) -> Vec<(DVec2, DVec2, bool)> {
        let mut out = Vec::new();
        if interval <= 0.0 {
            return out;
        }
        for tri in self.surface() {
            let zs = tri.map(|p| p.z + base);
            let (lo, hi) = (
                zs.iter().cloned().fold(f64::MAX, f64::min),
                zs.iter().cloned().fold(f64::MIN, f64::max),
            );
            if hi - lo < 1e-6 {
                continue;
            }
            let mut k = (lo / interval).ceil() as i64;
            while (k as f64) * interval < hi {
                let h = k as f64 * interval;
                let mut pts = Vec::new();
                for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                    let (za, zb) = (zs[i], zs[j]);
                    if (za - h) * (zb - h) < 0.0 || (za == h && zb != h) {
                        let t = (h - za) / (zb - za);
                        pts.push(tri[i].truncate().lerp(tri[j].truncate(), t));
                    }
                }
                if pts.len() >= 2 && pts[0].distance(pts[1]) > 1e-6 {
                    let primary = primary_every > 0 && k.rem_euclid(primary_every as i64) == 0;
                    out.push((pts[0], pts[1], primary));
                }
                k += 1;
            }
        }
        out
    }
}

impl Document {
    /// The toposolid type to use, added (with its materials) to projects
    /// made before toposolids.
    pub fn ensure_toposolid_type(&mut self) -> ElementId {
        if let Some(t) = self.types_of(Category::Toposolid).next() {
            return t.id;
        }
        for name in ["Earth", "Grass"] {
            if self.material(name).is_none() {
                if let Some(m) = crate::materials::default_materials().into_iter().find(|m| m.name == name) {
                    self.materials.push(m);
                }
            }
        }
        self.add_type(
            Category::Toposolid,
            TOPOSOLID,
            "Toposolid - 300mm",
            toposolid_type_params(300.0, "Earth"),
        )
    }

    /// Adds or moves a shape point of a toposolid: `at` in plan, `height`
    /// above its level. Points outside the boundary are rejected.
    pub fn set_topo_point(&mut self, id: ElementId, at: DVec2, height: f64) -> Result<(), ModelError> {
        let Some(ElementData::Toposolid(t)) = self.element_mut(id).map(|e| &mut e.data) else {
            return Err(ModelError::NotFound(id));
        };
        let on_boundary = t.boundary.iter().any(|b| b.distance(at) < 1.0);
        if !on_boundary && !point_in_polygon(at, &t.boundary) {
            return Err(ModelError::Rejected("Points go inside the toposolid's boundary.".into()));
        }
        let h = height - t.height_offset;
        match t.points.iter_mut().find(|p| p.truncate().distance(at) < 1.0) {
            Some(p) => p.z = h,
            None => t.points.push(DVec3::new(at.x, at.y, h)),
        }
        self.touch();
        Ok(())
    }

    /// The toposolid under a plan point on a level, if any.
    pub fn toposolid_at(&self, level: ElementId, p: DVec2) -> Option<ElementId> {
        self.elements_of(Category::Toposolid)
            .find(|e| match &e.data {
                ElementData::Toposolid(t) => t.level == level && point_in_polygon(p, &t.boundary),
                _ => false,
            })
            .map(|e| e.id)
    }

    /// Height of the ground above a level at a plan point: the top of the
    /// toposolid there.
    pub fn topo_height_at(&self, level: ElementId, p: DVec2) -> Option<f64> {
        let id = self.toposolid_at(level, p)?;
        match &self.element(id)?.data {
            ElementData::Toposolid(t) => t.height_at(p),
            _ => None,
        }
    }

    /// Area of a property line (closed ones).
    pub fn property_area(&self, l: &PropertyLine) -> f64 {
        if l.closed && l.points.len() >= 3 {
            polygon_area(&l.points).abs()
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toposolid_surface_and_contours() {
        let sq = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(10000.0, 0.0),
            DVec2::new(10000.0, 10000.0),
            DVec2::new(0.0, 10000.0),
        ];
        let mut t = Toposolid {
            type_id: ElementId::INVALID,
            level: ElementId::INVALID,
            boundary: sq.clone(),
            points: vec![DVec3::new(5000.0, 5000.0, 2000.0)],
            height_offset: 0.0,
        };
        let s = t.surface();
        assert_eq!(s.len(), 4, "a centre point makes a fan of four");
        let area: f64 = s
            .iter()
            .map(|[a, b, c]| cross(b.truncate() - a.truncate(), c.truncate() - a.truncate()) / 2.0)
            .sum();
        assert!((area - 1e8).abs() < 1.0, "covers the boundary, counter-clockwise: {area}");
        assert!((t.height_at(DVec2::new(5000.0, 5000.0)).unwrap() - 2000.0).abs() < 1e-6);
        assert!((t.height_at(DVec2::new(2500.0, 5000.0)).unwrap() - 1000.0).abs() < 1e-6);
        // Contours at 500 mm: 500, 1000, 1500 — closed squares round the peak.
        let c = t.contours(0.0, 500.0, 2);
        assert_eq!(c.len(), 12);
        assert_eq!(c.iter().filter(|s| s.2).count(), 4, "1000 is primary");
        // A raised corner reshapes the mesh but keeps the boundary edges.
        t.points.push(DVec3::new(10000.0, 10000.0, 1500.0));
        t.points.push(DVec3::new(2000.0, 8000.0, 500.0));
        t.points.push(DVec3::new(7000.0, 3000.0, 800.0));
        let s = t.surface();
        let area: f64 = s
            .iter()
            .map(|[a, b, c]| cross(b.truncate() - a.truncate(), c.truncate() - a.truncate()) / 2.0)
            .sum();
        assert!((area - 1e8).abs() < 1.0);
        assert!(s
            .iter()
            .all(|[a, b, c]| cross(b.truncate() - a.truncate(), c.truncate() - a.truncate()) > 0.0));
        assert!((t.height_at(DVec2::new(10000.0, 10000.0)).unwrap() - 1500.0).abs() < 1e-6);
    }

    #[test]
    fn concave_boundary_keeps_its_edges() {
        let l = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(6000.0, 0.0),
            DVec2::new(6000.0, 2000.0),
            DVec2::new(2000.0, 2000.0),
            DVec2::new(2000.0, 6000.0),
            DVec2::new(0.0, 6000.0),
        ];
        let inner = vec![DVec2::new(1000.0, 1000.0), DVec2::new(1000.0, 4000.0), DVec2::new(4000.0, 1000.0)];
        let (v, tris) = constrained_mesh(&l, &inner);
        let area: f64 = tris.iter().map(|t| cross(v[t[1]] - v[t[0]], v[t[2]] - v[t[0]]) / 2.0).sum();
        assert!((area - polygon_area(&l).abs()).abs() < 1.0);
        // No triangle spills into the notch.
        for t in &tris {
            let c = (v[t[0]] + v[t[1]] + v[t[2]]) / 3.0;
            assert!(point_in_polygon(c, &l));
        }
    }
}
