//! Small geometry toolkit: polygons, clipping, triangulation, planes.

use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

/// Axis aligned bounding box.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Aabb {
    pub min: DVec3,
    pub max: DVec3,
}

impl Aabb {
    pub const EMPTY: Aabb = Aabb {
        min: DVec3::splat(f64::INFINITY),
        max: DVec3::splat(f64::NEG_INFINITY),
    };

    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x
    }

    pub fn include(&mut self, p: DVec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    pub fn union(mut self, o: Aabb) -> Aabb {
        if !o.is_empty() {
            self.include(o.min);
            self.include(o.max);
        }
        self
    }

    pub fn center(&self) -> DVec3 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> DVec3 {
        self.max - self.min
    }

    pub fn corners(&self) -> [DVec3; 8] {
        let (a, b) = (self.min, self.max);
        [
            DVec3::new(a.x, a.y, a.z),
            DVec3::new(b.x, a.y, a.z),
            DVec3::new(b.x, b.y, a.z),
            DVec3::new(a.x, b.y, a.z),
            DVec3::new(a.x, a.y, b.z),
            DVec3::new(b.x, a.y, b.z),
            DVec3::new(b.x, b.y, b.z),
            DVec3::new(a.x, b.y, b.z),
        ]
    }
}

/// An oriented plane `normal · p = d`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Plane {
    pub normal: DVec3,
    pub d: f64,
}

impl Plane {
    pub fn from_point_normal(p: DVec3, n: DVec3) -> Self {
        let n = n.normalize();
        Plane { normal: n, d: n.dot(p) }
    }

    pub fn signed_distance(&self, p: DVec3) -> f64 {
        self.normal.dot(p) - self.d
    }
}

/// 2D cross product (z of the 3D cross product).
pub fn cross2(a: DVec2, b: DVec2) -> f64 {
    a.x * b.y - a.y * b.x
}

/// Left-hand perpendicular of a 2D vector.
pub fn perp(v: DVec2) -> DVec2 {
    DVec2::new(-v.y, v.x)
}

/// Signed area of a polygon (positive when counter-clockwise).
pub fn polygon_area(pts: &[DVec2]) -> f64 {
    let n = pts.len();
    let mut a = 0.0;
    for i in 0..n {
        a += cross2(pts[i], pts[(i + 1) % n]);
    }
    a * 0.5
}

pub fn polygon_perimeter(pts: &[DVec2]) -> f64 {
    let n = pts.len();
    (0..n).map(|i| pts[i].distance(pts[(i + 1) % n])).sum()
}

/// Intersection of two infinite lines `p + t*d`. `None` if parallel.
pub fn line_intersection(p1: DVec2, d1: DVec2, p2: DVec2, d2: DVec2) -> Option<DVec2> {
    let den = cross2(d1, d2);
    if den.abs() < 1e-12 {
        return None;
    }
    let t = cross2(p2 - p1, d2) / den;
    Some(p1 + d1 * t)
}

/// Closest point on segment `ab` to `p` and its parameter t in [0,1].
pub fn closest_on_segment(p: DVec2, a: DVec2, b: DVec2) -> (DVec2, f64) {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 < 1e-18 {
        return (a, 0.0);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (a + ab * t, t)
}

pub fn distance_to_segment(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    closest_on_segment(p, a, b).0.distance(p)
}

pub fn point_in_polygon(p: DVec2, poly: &[DVec2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Clips a polygon by the half plane `dot(n, p) <= d` (Sutherland–Hodgman).
pub fn clip_polygon_2d(poly: &[DVec2], n: DVec2, d: f64) -> Vec<DVec2> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    let len = poly.len();
    for i in 0..len {
        let a = poly[i];
        let b = poly[(i + 1) % len];
        let da = n.dot(a) - d;
        let db = n.dot(b) - d;
        if da <= 0.0 {
            out.push(a);
        }
        if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
            let t = da / (da - db);
            out.push(a + (b - a) * t);
        }
    }
    out
}

/// Clips a 3D polygon keeping the part with `plane.signed_distance(p) >= 0`.
pub fn clip_polygon_3d(poly: &[DVec3], plane: &Plane) -> Vec<DVec3> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    let len = poly.len();
    for i in 0..len {
        let a = poly[i];
        let b = poly[(i + 1) % len];
        let da = plane.signed_distance(a);
        let db = plane.signed_distance(b);
        if da >= 0.0 {
            out.push(a);
        }
        if (da > 0.0 && db < 0.0) || (da < 0.0 && db > 0.0) {
            let t = da / (da - db);
            out.push(a + (b - a) * t);
        }
    }
    out
}

/// Whether segments `a`-`b` and `c`-`d` cross at a point inside both
/// (touching at an end does not count).
pub fn segments_cross(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let d1 = cross2(b - a, c - a);
    let d2 = cross2(b - a, d - a);
    let d3 = cross2(d - c, a - c);
    let d4 = cross2(d - c, b - c);
    let eps = 1e-9 * (1.0 + (b - a).length() * (d - c).length());
    ((d1 > eps && d2 < -eps) || (d1 < -eps && d2 > eps)) && ((d3 > eps && d4 < -eps) || (d3 < -eps && d4 > eps))
}

/// Joins holes into an outer polygon with bridges ("keyholes"), giving
/// one polygon that [`triangulate`] can fill. The result is
/// counter-clockwise; each hole runs clockwise inside it.
pub fn merge_holes(outer: &[DVec2], holes: &[Vec<DVec2>]) -> Vec<DVec2> {
    let mut poly: Vec<DVec2> = outer.to_vec();
    if polygon_area(&poly) < 0.0 {
        poly.reverse();
    }
    let mut holes: Vec<Vec<DVec2>> = holes
        .iter()
        .filter(|h| h.len() >= 3)
        .map(|h| {
            let mut h = h.clone();
            if polygon_area(&h) > 0.0 {
                h.reverse();
            }
            h
        })
        .collect();
    // Rightmost holes first, so later bridges can pass to the right.
    holes.sort_by(|a, b| {
        let mx = |h: &Vec<DVec2>| h.iter().map(|p| p.x).fold(f64::MIN, f64::max);
        mx(b).total_cmp(&mx(a))
    });
    for (hi, hole) in holes.iter().enumerate() {
        let others: Vec<&Vec<DVec2>> = holes[hi + 1..].iter().collect();
        let crosses = |m: DVec2, v: DVec2| {
            let ring = |r: &[DVec2]| (0..r.len()).any(|i| segments_cross(m, v, r[i], r[(i + 1) % r.len()]));
            ring(&poly) || ring(hole) || others.iter().any(|o| ring(o))
        };
        // The hole vertex and polygon vertex with the shortest bridge
        // that crosses nothing.
        let mut best: Option<(f64, usize, usize)> = None;
        for (mi, m) in hole.iter().enumerate() {
            for (vi, v) in poly.iter().enumerate() {
                let d = m.distance_squared(*v);
                if best.is_some_and(|b| b.0 <= d) || crosses(*m, *v) {
                    continue;
                }
                best = Some((d, mi, vi));
            }
        }
        let Some((_, mi, vi)) = best else { continue };
        let mut merged = Vec::with_capacity(poly.len() + hole.len() + 2);
        merged.extend_from_slice(&poly[..=vi]);
        for k in 0..=hole.len() {
            merged.push(hole[(mi + k) % hole.len()]);
        }
        merged.push(poly[vi]);
        merged.extend_from_slice(&poly[vi + 1..]);
        poly = merged;
    }
    poly
}

/// Triangles filling `outer` minus `holes`.
pub fn triangulate_with_holes(outer: &[DVec2], holes: &[Vec<DVec2>]) -> Vec<[DVec2; 3]> {
    let merged = merge_holes(outer, holes);
    triangulate(&merged)
        .into_iter()
        .map(|[a, b, c]| [merged[a], merged[b], merged[c]])
        .collect()
}

/// Triangulates a simple polygon by ear clipping. Returns index triples.
/// Works for either winding.
pub fn triangulate(pts: &[DVec2]) -> Vec<[usize; 3]> {
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    let ccw = polygon_area(pts) >= 0.0;
    let mut idx: Vec<usize> = if ccw { (0..n).collect() } else { (0..n).rev().collect() };
    let mut tris = Vec::with_capacity(n - 2);
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let (ia, ib, ic) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            let (a, b, c) = (pts[ia], pts[ib], pts[ic]);
            if cross2(b - a, c - b) <= 1e-12 {
                continue; // reflex or degenerate
            }
            // Points at the ear's corners (the doubled bridge points of a
            // polygon with holes) do not block it.
            let corner = |q: DVec2| q.distance_squared(a) < 1e-12 || q.distance_squared(b) < 1e-12 || q.distance_squared(c) < 1e-12;
            let contains = idx
                .iter()
                .any(|&k| k != ia && k != ib && k != ic && !corner(pts[k]) && point_in_triangle(pts[k], a, b, c));
            if contains {
                continue;
            }
            tris.push([ia, ib, ic]);
            idx.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            // Nearly degenerate input (slivers rounded to screen pixels):
            // clip the smallest convex corner anyway. A fan over the rest
            // could cover far more than the polygon.
            let tri_area = |i: usize| {
                let (a, b, c) = (pts[idx[(i + m - 1) % m]], pts[idx[i]], pts[idx[(i + 1) % m]]);
                cross2(b - a, c - b)
            };
            let Some(i) = (0..m)
                .filter(|&i| tri_area(i) > 0.0)
                .min_by(|&x, &y| tri_area(x).total_cmp(&tri_area(y)))
                .or_else(|| (0..m).min_by(|&x, &y| tri_area(x).abs().total_cmp(&tri_area(y).abs())))
            else {
                return tris;
            };
            if tri_area(i) > 0.0 {
                tris.push([idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]]);
            }
            idx.remove(i);
        }
    }
    if idx.len() == 3 {
        tris.push([idx[0], idx[1], idx[2]]);
    }
    tris
}

#[cfg(test)]
mod hole_tests {
    use super::*;

    fn area(tris: &[[DVec2; 3]]) -> f64 {
        tris.iter().map(|t| polygon_area(t).abs()).sum()
    }

    #[test]
    fn square_with_hole() {
        let sq = |x0: f64, y0: f64, s: f64| {
            vec![
                DVec2::new(x0, y0),
                DVec2::new(x0 + s, y0),
                DVec2::new(x0 + s, y0 + s),
                DVec2::new(x0, y0 + s),
            ]
        };
        let tris = triangulate_with_holes(&sq(0.0, 0.0, 10.0), &[sq(3.0, 3.0, 2.0)]);
        assert!((area(&tris) - 96.0).abs() < 1e-6, "{}", area(&tris));
        // Two holes.
        let tris = triangulate_with_holes(&sq(0.0, 0.0, 10.0), &[sq(1.0, 1.0, 2.0), sq(6.0, 6.0, 3.0)]);
        assert!((area(&tris) - (100.0 - 4.0 - 9.0)).abs() < 1e-6, "{}", area(&tris));
        // L-shaped outer, hole in one arm.
        let l = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 4.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(4.0, 10.0),
            DVec2::new(0.0, 10.0),
        ];
        let tris = triangulate_with_holes(&l, &[sq(1.0, 6.0, 2.0)]);
        assert!((area(&tris) - (64.0 - 4.0)).abs() < 1e-6, "{}", area(&tris));
    }
}

fn point_in_triangle(p: DVec2, a: DVec2, b: DVec2, c: DVec2) -> bool {
    let d1 = cross2(b - a, p - a);
    let d2 = cross2(c - b, p - b);
    let d3 = cross2(a - c, p - c);
    d1 >= 0.0 && d2 >= 0.0 && d3 >= 0.0
}

/// Newell normal of a planar 3D polygon (not normalised).
pub fn polygon_normal(pts: &[DVec3]) -> DVec3 {
    let mut n = DVec3::ZERO;
    let len = pts.len();
    for i in 0..len {
        let a = pts[i];
        let b = pts[(i + 1) % len];
        n.x += (a.y - b.y) * (a.z + b.z);
        n.y += (a.z - b.z) * (a.x + b.x);
        n.z += (a.x - b.x) * (a.y + b.y);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_and_triangulation() {
        // L-shape, 3 triangles needed at least.
        let l = [
            DVec2::new(0.0, 0.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(2.0, 1.0),
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 2.0),
            DVec2::new(0.0, 2.0),
        ];
        assert!((polygon_area(&l) - 3.0).abs() < 1e-9);
        let tris = triangulate(&l);
        assert_eq!(tris.len(), 4);
        let sum: f64 = tris.iter().map(|t| polygon_area(&[l[t[0]], l[t[1]], l[t[2]]]).abs()).sum();
        assert!((sum - 3.0).abs() < 1e-9);
    }

    #[test]
    fn clip_square() {
        let sq = [
            DVec2::new(0.0, 0.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(2.0, 2.0),
            DVec2::new(0.0, 2.0),
        ];
        let c = clip_polygon_2d(&sq, DVec2::X, 1.0);
        assert!((polygon_area(&c) - 2.0).abs() < 1e-9);
    }
}
