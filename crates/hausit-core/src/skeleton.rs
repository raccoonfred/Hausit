//! Weighted straight skeleton of a simple polygon: the solver behind hip
//! and gable roofs on any footprint, like Revit's Roof by Footprint.
//!
//! Every edge moves inwards at its weight (1 for an edge that defines a
//! slope, 0 for a gable end). The skeleton is the path traced by the
//! wavefront's vertices; each edge sweeps one roof face. Heights are the
//! offset distance of a weight-1 edge, so a roof of slope `a` has
//! z = height × tan(a).

use glam::{DVec2, DVec3};

/// The face swept by input edge `edge`. `pts` starts with the edge's two
/// endpoints (at height 0) and climbs back over the skeleton.
#[derive(Clone, Debug)]
pub struct SkeletonFace {
    pub edge: usize,
    pub pts: Vec<DVec3>,
}

#[derive(Clone, Debug)]
pub struct Skeleton {
    pub faces: Vec<SkeletonFace>,
    /// Skeleton segments: hips, valleys and ridges, with the two input
    /// edges on either side of each.
    pub arcs: Vec<(DVec3, DVec3, usize, usize)>,
}

#[derive(Clone)]
struct Edge {
    a: DVec2,
    dir: DVec2,
    /// Inward normal.
    n: DVec2,
    w: f64,
}

#[derive(Clone)]
struct Vert {
    pos: DVec2,
    v: DVec2,
    e_in: usize,
    e_out: usize,
    origin: DVec3,
}

enum Event {
    Edge { l: usize, i: usize },
    Split { l: usize, r: usize, k: usize },
}

const EPS: f64 = 1e-9;

fn cross(a: DVec2, b: DVec2) -> f64 {
    a.x * b.y - a.y * b.x
}

fn velocity(edges: &[Edge], e_in: usize, e_out: usize) -> DVec2 {
    let (a, b) = (&edges[e_in], &edges[e_out]);
    if e_in == e_out {
        return a.n * a.w;
    }
    let det = cross(a.n, b.n);
    if det.abs() < 1e-9 {
        // Collinear edges keep moving together; opposite ones have met in
        // a ridge and stop.
        return if a.n.dot(b.n) > 0.0 { a.n * (a.w + b.w) * 0.5 } else { DVec2::ZERO };
    }
    DVec2::new((a.w * b.n.y - a.n.y * b.w) / det, (a.n.x * b.w - a.w * b.n.x) / det)
}

fn is_reflex(edges: &[Edge], v: &Vert) -> bool {
    cross(edges[v.e_in].dir, edges[v.e_out].dir) < -EPS
}

/// Computes the skeleton of `poly` (either orientation, no repeated
/// points) with one weight per edge (edge `i` runs from `poly[i]` to
/// `poly[i + 1]`). Returns `None` for degenerate input or when the faces
/// cannot be assembled.
pub fn straight_skeleton(poly: &[DVec2], weights: &[f64]) -> Option<Skeleton> {
    let n = poly.len();
    if n < 3 || weights.len() != n {
        return None;
    }
    let area: f64 = (0..n).map(|i| cross(poly[i], poly[(i + 1) % n])).sum::<f64>() / 2.0;
    if area.abs() < EPS {
        return None;
    }
    // Work counter-clockwise; `map[j]` is the caller's index of edge j.
    let (pts, w, map): (Vec<DVec2>, Vec<f64>, Vec<usize>) = if area > 0.0 {
        (poly.to_vec(), weights.to_vec(), (0..n).collect())
    } else {
        let pts: Vec<DVec2> = poly.iter().rev().copied().collect();
        // Reversed edge j runs pts[j] → pts[j+1] = poly[n-1-j] → poly[n-2-j],
        // which is caller edge n-2-j.
        let map: Vec<usize> = (0..n).map(|j| (2 * n - 2 - j) % n).collect();
        let w = map.iter().map(|&i| weights[i]).collect();
        (pts, w, map)
    };
    let edges: Vec<Edge> = (0..n)
        .map(|i| {
            let d = (pts[(i + 1) % n] - pts[i]).normalize_or_zero();
            Edge {
                a: pts[i],
                dir: d,
                n: DVec2::new(-d.y, d.x),
                w: w[i],
            }
        })
        .collect();
    if edges.iter().any(|e| e.dir == DVec2::ZERO) {
        return None;
    }
    let first: Vec<Vert> = (0..n)
        .map(|i| {
            let (e_in, e_out) = ((i + n - 1) % n, i);
            Vert {
                pos: pts[i],
                v: velocity(&edges, e_in, e_out),
                e_in,
                e_out,
                origin: pts[i].extend(0.0),
            }
        })
        .collect();

    let mut loops = vec![first];
    let mut arcs: Vec<(DVec3, DVec3, usize, usize)> = Vec::new();
    let mut t = 0.0;
    let arc = |arcs: &mut Vec<_>, a: DVec3, b: DVec3, e: (usize, usize)| {
        if a.distance(b) > 1e-9 {
            arcs.push((a, b, e.0, e.1));
        }
    };

    for _ in 0..10 * n * n + 100 {
        // Find the next event.
        let mut best: Option<(f64, Event)> = None;
        let mut offer = |dt: f64, ev: Event| {
            if dt > -1e-7 && best.as_ref().map(|b| dt < b.0 - 1e-12).unwrap_or(true) {
                best = Some((dt.max(0.0), ev));
            }
        };
        for (l, lp) in loops.iter().enumerate() {
            let m = lp.len();
            for i in 0..m {
                let (a, b) = (&lp[i], &lp[(i + 1) % m]);
                let u = edges[a.e_out].dir;
                let len = (b.pos - a.pos).dot(u);
                let rate = (a.v - b.v).dot(u);
                if len.abs() < 1e-9 {
                    offer(0.0, Event::Edge { l, i });
                } else if rate > 1e-12 {
                    offer(len / rate, Event::Edge { l, i });
                }
            }
            for r in 0..m {
                let rv = &lp[r];
                if !is_reflex(&edges, rv) {
                    continue;
                }
                for k in 0..m {
                    let k1 = (k + 1) % m;
                    if k == r || k1 == r {
                        continue;
                    }
                    let e = &edges[lp[k].e_out];
                    let denom = rv.v.dot(e.n) - e.w;
                    if denom.abs() < 1e-12 {
                        continue;
                    }
                    let dt = (e.w * t - (rv.pos - e.a).dot(e.n)) / denom;
                    if dt < -1e-7 {
                        continue;
                    }
                    let x = rv.pos + rv.v * dt;
                    let (pa, pb) = (lp[k].pos + lp[k].v * dt, lp[k1].pos + lp[k1].v * dt);
                    let span = (pb - pa).dot(e.dir);
                    let s = (x - pa).dot(e.dir);
                    if span > 1e-9 && s > 1e-7 && s < span - 1e-7 {
                        offer(dt, Event::Split { l, r, k });
                    }
                }
            }
        }
        let Some((dt, ev)) = best else { break };

        t += dt;
        for lp in &mut loops {
            for v in lp.iter_mut() {
                v.pos += v.v * dt;
            }
        }

        match ev {
            Event::Edge { l, i } => {
                let lp = loops.swap_remove(l);
                let m = lp.len();
                let (a, b) = (&lp[i], &lp[(i + 1) % m]);
                let x = ((a.pos + b.pos) * 0.5).extend(t);
                arc(&mut arcs, a.origin, x, (a.e_in, a.e_out));
                arc(&mut arcs, b.origin, x, (b.e_in, b.e_out));
                let nv = Vert {
                    pos: x.truncate(),
                    v: velocity(&edges, a.e_in, b.e_out),
                    e_in: a.e_in,
                    e_out: b.e_out,
                    origin: x,
                };
                let out: Vec<Vert> = lp
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != (i + 1) % m)
                    .map(|(j, v)| if j == i { nv.clone() } else { v.clone() })
                    .collect();
                settle(&edges, out, t, &mut arcs, &mut loops);
            }
            Event::Split { l, r, k } => {
                let lp = loops.swap_remove(l);
                let m = lp.len();
                let rv = &lp[r];
                let x = rv.pos.extend(t);
                arc(&mut arcs, rv.origin, x, (rv.e_in, rv.e_out));
                let e = lp[k].e_out;
                let v1 = Vert {
                    pos: rv.pos,
                    v: velocity(&edges, rv.e_in, e),
                    e_in: rv.e_in,
                    e_out: e,
                    origin: x,
                };
                let v2 = Vert {
                    pos: rv.pos,
                    v: velocity(&edges, e, rv.e_out),
                    e_in: e,
                    e_out: rv.e_out,
                    origin: x,
                };
                let walk = |from: usize, to: usize| -> Vec<Vert> {
                    let mut out = Vec::new();
                    let mut j = from;
                    loop {
                        out.push(lp[j].clone());
                        if j == to {
                            break;
                        }
                        j = (j + 1) % m;
                    }
                    out
                };
                let mut l1 = vec![v1];
                l1.extend(walk((k + 1) % m, (r + m - 1) % m));
                let mut l2 = vec![v2];
                l2.extend(walk((r + 1) % m, k));
                settle(&edges, l1, t, &mut arcs, &mut loops);
                settle(&edges, l2, t, &mut arcs, &mut loops);
            }
        }
    }
    if !loops.is_empty() {
        return None;
    }

    let faces = (0..n)
        .map(|i| assemble_face(i, &pts, &arcs).map(|pts| SkeletonFace { edge: map[i], pts }))
        .collect::<Option<Vec<_>>>()?;
    let arcs = arcs.into_iter().map(|(a, b, e, f)| (a, b, map[e], map[f])).collect();
    Some(Skeleton { faces, arcs })
}

/// Keeps a loop that can still shrink, or closes off one that has
/// collapsed (fewer than three vertices, or no area left). A collapsed
/// loop is a ridge traced back and forth: each of its segments bounds the
/// face of the edge it lies on.
fn settle(edges: &[Edge], mut lp: Vec<Vert>, t: f64, arcs: &mut Vec<(DVec3, DVec3, usize, usize)>, loops: &mut Vec<Vec<Vert>>) {
    zip(edges, &mut lp, t, arcs);
    let m = lp.len();
    let area: f64 = (0..m).map(|i| cross(lp[i].pos, lp[(i + 1) % m].pos)).sum::<f64>() / 2.0;
    let perimeter: f64 = (0..m).map(|i| lp[i].pos.distance(lp[(i + 1) % m].pos)).sum();
    if m >= 3 && area > 1e-7 * perimeter * perimeter {
        loops.push(lp);
        return;
    }
    for i in 0..m {
        let v = &lp[i];
        let p = v.pos.extend(t);
        if v.origin.distance(p) > 1e-9 {
            arcs.push((v.origin, p, v.e_in, v.e_out));
        }
        let q = lp[(i + 1) % m].pos.extend(t);
        if m > 1 && p.distance(q) > 1e-9 {
            arcs.push((p, q, v.e_out, v.e_out));
        }
    }
}

/// A vertex between two opposite edges means the strip between them has
/// just closed: the two edges now run along the same line. Zip them
/// together into a ridge up to the nearer neighbour, repeatedly.
fn zip(edges: &[Edge], lp: &mut Vec<Vert>, t: f64, arcs: &mut Vec<(DVec3, DVec3, usize, usize)>) {
    let mut push = |a: DVec3, b: DVec3, e: (usize, usize)| {
        if a.distance(b) > 1e-9 {
            arcs.push((a, b, e.0, e.1));
        }
    };
    for _ in 0..4 * lp.len() + 4 {
        let m = lp.len();
        if m < 3 {
            return;
        }
        let opposite = |v: &Vert| v.e_in != v.e_out && edges[v.e_in].n.dot(edges[v.e_out].n) < -1.0 + 1e-9;
        let Some(i) = (0..m).find(|&i| opposite(&lp[i])) else { return };
        let (pi, ni) = ((i + m - 1) % m, (i + 1) % m);
        let (v, p, n) = (lp[i].clone(), lp[pi].clone(), lp[ni].clone());
        let (dp, dn) = (v.pos.distance(p.pos), v.pos.distance(n.pos));
        let vt = v.pos.extend(t);
        push(v.origin, vt, (v.e_in, v.e_out));
        let (remove, w) = if (dp - dn).abs() < 1e-6 {
            let x = n.pos.extend(t);
            push(vt, x, (v.e_in, v.e_out));
            push(p.origin, x, (p.e_in, p.e_out));
            push(n.origin, x, (n.e_in, n.e_out));
            (vec![pi, ni], (p.e_in, n.e_out, n.pos))
        } else if dn < dp {
            let x = n.pos.extend(t);
            push(vt, x, (v.e_in, v.e_out));
            push(n.origin, x, (n.e_in, n.e_out));
            (vec![ni], (v.e_in, n.e_out, n.pos))
        } else {
            let x = p.pos.extend(t);
            push(vt, x, (v.e_in, v.e_out));
            push(p.origin, x, (p.e_in, p.e_out));
            (vec![pi], (p.e_in, v.e_out, p.pos))
        };
        let nv = Vert {
            pos: w.2,
            v: velocity(edges, w.0, w.1),
            e_in: w.0,
            e_out: w.1,
            origin: w.2.extend(t),
        };
        *lp = (0..m)
            .filter(|j| !remove.contains(j))
            .map(|j| if j == i { nv.clone() } else { lp[j].clone() })
            .collect();
    }
}

/// Walks the skeleton arcs bounding edge `i`'s face, from the edge's end
/// back to its start.
fn assemble_face(i: usize, pts: &[DVec2], arcs: &[(DVec3, DVec3, usize, usize)]) -> Option<Vec<DVec3>> {
    let n = pts.len();
    let (start, end) = (pts[i].extend(0.0), pts[(i + 1) % n].extend(0.0));
    let mine: Vec<(DVec3, DVec3)> = arcs.iter().filter(|a| a.2 == i || a.3 == i).map(|a| (a.0, a.1)).collect();
    let mut used = vec![false; mine.len()];
    let mut out = vec![start, end];
    let mut cur = end;
    let tol = 1e-3;
    for _ in 0..=mine.len() {
        let next = (0..mine.len())
            .filter(|&j| !used[j])
            .filter_map(|j| {
                let (a, b) = mine[j];
                let (da, db) = (a.distance(cur), b.distance(cur));
                if da.min(db) > tol {
                    None
                } else if da <= db {
                    Some((j, b, da))
                } else {
                    Some((j, a, db))
                }
            })
            .min_by(|x, y| x.2.total_cmp(&y.2));
        let (j, other, _) = next?;
        used[j] = true;
        if other.distance(start) < tol {
            return Some(out);
        }
        out.push(other);
        cur = other;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(l: f64, w: f64) -> Vec<DVec2> {
        vec![DVec2::new(0.0, 0.0), DVec2::new(l, 0.0), DVec2::new(l, w), DVec2::new(0.0, w)]
    }

    fn area_xy(f: &SkeletonFace) -> f64 {
        let p = &f.pts;
        (0..p.len())
            .map(|i| cross(p[i].truncate(), p[(i + 1) % p.len()].truncate()))
            .sum::<f64>()
            .abs()
            / 2.0
    }

    #[test]
    fn rectangle_hip() {
        let s = straight_skeleton(&rect(12000.0, 8000.0), &[1.0; 4]).unwrap();
        assert_eq!(s.faces.len(), 4);
        let total: f64 = s.faces.iter().map(area_xy).sum();
        assert!((total - 96e6).abs() < 1.0, "{total}");
        let top = s.faces.iter().flat_map(|f| f.pts.iter()).map(|p| p.z).fold(0.0, f64::max);
        assert!((top - 4000.0).abs() < 1e-6);
        // Long sides are trapezoids, short ends triangles.
        assert_eq!(s.faces[0].pts.len(), 4);
        assert_eq!(s.faces[1].pts.len(), 3);
    }

    #[test]
    fn clockwise_input_keeps_edge_indices() {
        let mut r = rect(12000.0, 8000.0);
        r.reverse();
        let s = straight_skeleton(&r, &[1.0; 4]).unwrap();
        for f in &s.faces {
            let a = r[f.edge];
            let b = r[(f.edge + 1) % 4];
            let has = |p: DVec2| f.pts.iter().any(|q| q.truncate().distance(p) < 1e-6);
            assert!(has(a) && has(b), "face {} misses its edge", f.edge);
        }
    }

    #[test]
    fn rectangle_gable() {
        let s = straight_skeleton(&rect(12000.0, 8000.0), &[1.0, 0.0, 1.0, 0.0]).unwrap();
        // Gable ends are vertical: zero plan area, apex at the ridge.
        assert!(area_xy(&s.faces[1]) < 1e-6);
        assert!(s.faces[1].pts.iter().any(|p| (p.z - 4000.0).abs() < 1e-6));
        assert!((area_xy(&s.faces[0]) - 48e6).abs() < 1.0);
    }

    #[test]
    fn l_shape_hip() {
        let l = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(10000.0, 0.0),
            DVec2::new(10000.0, 4000.0),
            DVec2::new(4000.0, 4000.0),
            DVec2::new(4000.0, 10000.0),
            DVec2::new(0.0, 10000.0),
        ];
        let s = straight_skeleton(&l, &[1.0; 6]).unwrap();
        let total: f64 = s.faces.iter().map(area_xy).sum();
        assert!((total - 64e6).abs() < 1.0, "{total}");
        let top = s.faces.iter().flat_map(|f| f.pts.iter()).map(|p| p.z).fold(0.0, f64::max);
        assert!((top - 2000.0).abs() < 1e-6);
    }

    #[test]
    fn t_shape_and_uneven_widths() {
        let t = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(3000.0, 0.0),
            DVec2::new(3000.0, 6000.0),
            DVec2::new(9000.0, 6000.0),
            DVec2::new(9000.0, 10000.0),
            DVec2::new(-6000.0, 10000.0),
            DVec2::new(-6000.0, 6000.0),
            DVec2::new(0.0, 6000.0),
        ];
        let s = straight_skeleton(&t, &[1.0; 8]).unwrap();
        let total: f64 = s.faces.iter().map(area_xy).sum();
        assert!((total - (18e6 + 60e6)).abs() < 1.0, "{total}");
    }

    fn check_area(poly: &[DVec2], w: &[f64]) {
        let s = straight_skeleton(poly, w).unwrap_or_else(|| panic!("no skeleton for {poly:?}"));
        let n = poly.len();
        let expect = (0..n).map(|i| cross(poly[i], poly[(i + 1) % n])).sum::<f64>().abs() / 2.0;
        let total: f64 = s.faces.iter().map(area_xy).sum();
        assert!((total - expect).abs() < 1.0, "{total} vs {expect} for {poly:?}");
    }

    #[test]
    fn more_shapes() {
        let p = |v: &[(f64, f64)]| v.iter().map(|(x, y)| DVec2::new(*x, *y)).collect::<Vec<_>>();
        // Uneven L.
        check_area(
            &p(&[
                (0.0, 0.0),
                (12000.0, 0.0),
                (12000.0, 5000.0),
                (3000.0, 5000.0),
                (3000.0, 9000.0),
                (0.0, 9000.0),
            ]),
            &[1.0; 6],
        );
        // U.
        check_area(
            &p(&[
                (0.0, 0.0),
                (10000.0, 0.0),
                (10000.0, 8000.0),
                (7000.0, 8000.0),
                (7000.0, 3000.0),
                (3000.0, 3000.0),
                (3000.0, 8000.0),
                (0.0, 8000.0),
            ]),
            &[1.0; 8],
        );
        // Skewed quadrilateral and a pentagon.
        check_area(&p(&[(0.0, 0.0), (9000.0, 1000.0), (8000.0, 7000.0), (1000.0, 6000.0)]), &[1.0; 4]);
        check_area(
            &p(&[(0.0, 0.0), (8000.0, 0.0), (10000.0, 5000.0), (4000.0, 9000.0), (-2000.0, 5000.0)]),
            &[1.0; 5],
        );
        // L with a gable end.
        check_area(
            &p(&[
                (0.0, 0.0),
                (12000.0, 0.0),
                (12000.0, 5000.0),
                (3000.0, 5000.0),
                (3000.0, 9000.0),
                (0.0, 9000.0),
            ]),
            &[1.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        );
    }
}
