//! A BSP tree over 3D polygons, giving an exact back-to-front order for any
//! view direction. Polygons that straddle a splitting plane are cut; the
//! cut edges are flagged so outlines are only drawn along real edges.

use hausit_core::{DVec3, Plane};

const EPS: f64 = 1e-3;

#[derive(Clone, Debug)]
pub struct Poly<T: Clone> {
    pub pts: Vec<DVec3>,
    /// `edges[i]` is true when edge `pts[i] → pts[i+1]` is an original edge.
    pub edges: Vec<bool>,
    pub data: T,
}

struct Node<T: Clone> {
    plane: Plane,
    polys: Vec<Poly<T>>,
    front: Option<Box<Node<T>>>,
    back: Option<Box<Node<T>>>,
}

pub struct Bsp<T: Clone> {
    root: Option<Box<Node<T>>>,
}

fn plane_of(pts: &[DVec3]) -> Option<Plane> {
    let n = hausit_core::geom::polygon_normal(pts);
    let len = n.length();
    if len < 1e-9 {
        return None;
    }
    let c = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
    Some(Plane::from_point_normal(c, n / len))
}

enum Side<T: Clone> {
    Coplanar(Poly<T>),
    Front(Poly<T>),
    Back(Poly<T>),
    Split(Poly<T>, Poly<T>),
}

fn classify<T: Clone>(plane: &Plane, p: Poly<T>) -> Side<T> {
    let d: Vec<f64> = p.pts.iter().map(|q| plane.signed_distance(*q)).collect();
    let front = d.iter().any(|v| *v > EPS);
    let back = d.iter().any(|v| *v < -EPS);
    match (front, back) {
        (false, false) => Side::Coplanar(p),
        (true, false) => Side::Front(p),
        (false, true) => Side::Back(p),
        (true, true) => {
            let n = p.pts.len();
            let (mut f, mut fe, mut b, mut be) = (vec![], vec![], vec![], vec![]);
            for i in 0..n {
                let j = (i + 1) % n;
                let (a, c) = (p.pts[i], p.pts[j]);
                let (da, dc) = (d[i], d[j]);
                let orig = p.edges[i];
                // A vertex on the plane goes to both sides; its outgoing edge
                // is only original on the side the edge actually runs into.
                let on_plane = da.abs() <= EPS;
                if da >= -EPS {
                    f.push(a);
                    fe.push(orig && (!on_plane || dc >= -EPS));
                }
                if da <= EPS {
                    b.push(a);
                    be.push(orig && (!on_plane || dc <= EPS));
                }
                if (da > EPS && dc < -EPS) || (da < -EPS && dc > EPS) {
                    let x = a + (c - a) * (da / (da - dc));
                    // The new vertex starts the remainder of the cut edge on
                    // both sides; the edge leading into it is marked as a cut
                    // edge on the side we are leaving.
                    if da > 0.0 {
                        f.push(x);
                        fe.push(false);
                        b.push(x);
                        be.push(orig);
                    } else {
                        b.push(x);
                        be.push(false);
                        f.push(x);
                        fe.push(orig);
                    }
                }
            }
            let mk = |pts: Vec<DVec3>, edges: Vec<bool>| Poly {
                pts,
                edges,
                data: p.data.clone(),
            };
            Side::Split(mk(f, fe), mk(b, be))
        }
    }
}

fn build<T: Clone>(mut polys: Vec<Poly<T>>) -> Option<Box<Node<T>>> {
    polys.retain(|p| p.pts.len() >= 3 && plane_of(&p.pts).is_some());
    if polys.is_empty() {
        return None;
    }
    // Choose a splitter among a few candidates, minimising cuts.
    let mut best = 0;
    let mut best_cost = usize::MAX;
    let step = (polys.len() / 8).max(1);
    for i in (0..polys.len()).step_by(step).take(8) {
        let plane = plane_of(&polys[i].pts).unwrap();
        let cost = polys
            .iter()
            .filter(|p| {
                let d = p.pts.iter().map(|q| plane.signed_distance(*q));
                let (mut f, mut b) = (false, false);
                for v in d {
                    f |= v > EPS;
                    b |= v < -EPS;
                }
                f && b
            })
            .count();
        if cost < best_cost {
            best_cost = cost;
            best = i;
        }
    }
    let splitter = polys.swap_remove(best);
    let plane = plane_of(&splitter.pts).unwrap();
    let mut here = vec![splitter];
    let (mut front, mut back) = (Vec::new(), Vec::new());
    for p in polys {
        match classify(&plane, p) {
            Side::Coplanar(p) => here.push(p),
            Side::Front(p) => front.push(p),
            Side::Back(p) => back.push(p),
            Side::Split(f, b) => {
                front.push(f);
                back.push(b);
            }
        }
    }
    Some(Box::new(Node {
        plane,
        polys: here,
        front: build(front),
        back: build(back),
    }))
}

impl<T: Clone> Bsp<T> {
    pub fn new(polys: Vec<Poly<T>>) -> Self {
        Bsp { root: build(polys) }
    }

    /// Polygons ordered far to near for an orthographic view looking along
    /// `forward`.
    pub fn back_to_front(&self, forward: DVec3) -> Vec<&Poly<T>> {
        let mut out = Vec::new();
        fn walk<'a, T: Clone>(n: &'a Node<T>, fwd: DVec3, out: &mut Vec<&'a Poly<T>>) {
            // The viewer sits at -forward·∞, i.e. on the back side when the
            // plane normal points along the view direction.
            let viewer_in_front = n.plane.normal.dot(fwd) < 0.0;
            let (far, near) = if viewer_in_front {
                (&n.back, &n.front)
            } else {
                (&n.front, &n.back)
            };
            if let Some(f) = far {
                walk(f, fwd, out);
            }
            out.extend(n.polys.iter());
            if let Some(f) = near {
                walk(f, fwd, out);
            }
        }
        if let Some(r) = &self.root {
            walk(r, forward, &mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(z: f64, x0: f64, x1: f64) -> Poly<u32> {
        Poly {
            pts: vec![
                DVec3::new(x0, 0.0, z),
                DVec3::new(x1, 0.0, z),
                DVec3::new(x1, 1.0, z),
                DVec3::new(x0, 1.0, z),
            ],
            edges: vec![true; 4],
            data: z as u32,
        }
    }

    #[test]
    fn orders_parallel_planes() {
        let bsp = Bsp::new(vec![quad(5.0, 0.0, 1.0), quad(1.0, 0.0, 1.0), quad(3.0, 0.0, 1.0)]);
        let down: Vec<u32> = bsp.back_to_front(-DVec3::Z).iter().map(|p| p.data).collect();
        assert_eq!(down, vec![1, 3, 5]);
        let up: Vec<u32> = bsp.back_to_front(DVec3::Z).iter().map(|p| p.data).collect();
        assert_eq!(up, vec![5, 3, 1]);
    }

    #[test]
    fn split_marks_cut_edges() {
        // A vertical quad crossing the plane x = 0.5 of another vertical quad.
        let splitter = Poly {
            pts: vec![
                DVec3::new(0.5, -1.0, -1.0),
                DVec3::new(0.5, 2.0, -1.0),
                DVec3::new(0.5, 2.0, 1.0),
                DVec3::new(0.5, -1.0, 1.0),
            ],
            edges: vec![true; 4],
            data: 0u32,
        };
        let plane = plane_of(&splitter.pts).unwrap();
        match classify(&plane, quad(0.0, 0.0, 1.0)) {
            Side::Split(f, b) => {
                assert_eq!(f.edges.iter().filter(|e| !**e).count(), 1);
                assert_eq!(b.edges.iter().filter(|e| !**e).count(), 1);
            }
            _ => panic!("expected split"),
        }
    }
}
