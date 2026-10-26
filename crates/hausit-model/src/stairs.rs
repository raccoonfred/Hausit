//! Stair layout (risers, treads and landings along the run path) and the
//! heights railings follow.

use crate::document::Document;
use crate::element::{ElementData, Railing, Stair};
use crate::family::FamilyType;
use crate::params::ParamValue;
use hausit_core::geom::{line_intersection, perp};
use hausit_core::{DVec2, ElementId};

/// Stair type settings.
#[derive(Clone, Copy, Debug)]
pub struct StairRules {
    pub max_riser: f64,
    pub tread: f64,
    pub structural_depth: f64,
    pub landing_thickness: f64,
    /// Monolithic stairs are cast as one waist slab below the steps;
    /// assembled stairs have treads carried by stringers.
    pub monolithic: bool,
    /// How far an assembled tread overhangs the riser below.
    pub nosing: f64,
    pub closed_risers: bool,
    pub riser_thickness: f64,
    pub stringer_width: f64,
    pub stringer_depth: f64,
}

impl StairRules {
    pub fn of(t: Option<&FamilyType>) -> Self {
        let len = |k: &str, d: f64| t.map(|t| t.length_or(k, d)).unwrap_or(d);
        StairRules {
            max_riser: len("Maximum Riser Height", 180.0).max(10.0),
            tread: len("Minimum Tread Depth", 280.0).max(10.0),
            structural_depth: len("Structural Depth", 150.0),
            landing_thickness: len("Landing Thickness", 200.0),
            monolithic: !matches!(t.and_then(|t| t.params.get("Monolithic")), Some(ParamValue::Bool(false))),
            nosing: len("Nosing Length", 25.0).max(0.0),
            closed_risers: matches!(t.and_then(|t| t.params.get("Closed Risers")), Some(ParamValue::Bool(true))),
            riser_thickness: len("Riser Thickness", 20.0).max(1.0),
            stringer_width: len("Stringer Width", 50.0).max(0.0),
            stringer_depth: len("Stringer Depth", 250.0).max(0.0),
        }
    }
}

/// A tread or landing in plan, with the height of its top.
#[derive(Clone, Debug, PartialEq)]
pub struct StairPiece {
    pub outline: Vec<DVec2>,
    pub z_top: f64,
    /// The front (nosing) edge; for landings, the edge the run arrives at.
    pub front: (DVec2, DVec2),
    pub landing: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StairLayout {
    pub z0: f64,
    pub z1: f64,
    pub risers: usize,
    pub riser: f64,
    pub pieces: Vec<StairPiece>,
    /// The run centreline, the last run extended to the top riser.
    pub path: Vec<DVec2>,
    /// Risers placed by the path's runs (less than `risers` while the
    /// path is too short for the last run, which then grows).
    pub placed: usize,
    /// Height along the centreline: (distance along `path`, nosing height).
    pub pitch: Vec<(f64, f64)>,
}

/// Number of risers for a height: as few as the maximum riser allows.
pub fn riser_count(height: f64, max_riser: f64) -> usize {
    ((height / max_riser) - 1e-9).ceil().max(1.0) as usize
}

/// Lays out risers along `path` (at least two points). Every run but the
/// last holds as many treads as fit; the last one takes the rest, running
/// past its end point when needed. Landings sit at each turn.
pub fn layout(rules: StairRules, z0: f64, z1: f64, width: f64, path: &[DVec2]) -> StairLayout {
    let risers = riser_count((z1 - z0).max(1.0), rules.max_riser);
    let riser = (z1 - z0) / risers as f64;
    let g = rules.tread;
    let hw = width / 2.0;
    let mut pieces = Vec::new();
    let mut pitch = Vec::new();
    let mut out_path = vec![path[0]];
    let mut remaining = risers - 1; // the top riser lands on the floor above
    let mut done = 0usize;
    let mut placed = 0usize;
    let mut s_base = 0.0; // distance along the path at the segment start
    let n = path.len() - 1;
    for i in 0..n {
        let (a, b) = (path[i], path[i + 1]);
        let len = a.distance(b);
        let d = (b - a).try_normalize().unwrap_or(DVec2::X);
        let side = perp(d) * hw;
        let last = i + 1 == n;
        let start = if i > 0 { hw } else { 0.0 };
        let end = if last { 0.0 } else { hw };
        let fit = ((len - start - end) / g + 1e-6).floor().max(0.0) as usize;
        let count = if last { remaining } else { fit.min(remaining) };
        if last {
            placed += fit.min(remaining) + 1;
        } else {
            placed += count;
        }
        for k in 0..count {
            let s0 = start + k as f64 * g;
            let (p, q) = (a + d * s0, a + d * (s0 + g));
            done += 1;
            let z = z0 + done as f64 * riser;
            pitch.push((s_base + s0, z));
            pieces.push(StairPiece {
                outline: vec![p - side, q - side, q + side, p + side],
                z_top: z,
                front: (p - side, p + side),
                landing: false,
            });
        }
        remaining -= count;
        let run_end = start + count as f64 * g;
        if last {
            let top = a + d * run_end;
            out_path.push(top);
            pitch.push((s_base + run_end, z1));
        } else {
            // A square landing centred on the turn.
            let z = z0 + done as f64 * riser;
            let c = b;
            let (u, v) = (d * hw, side);
            pieces.push(StairPiece {
                outline: vec![c - u - v, c + u - v, c + u + v, c - u + v],
                z_top: z,
                front: (c - u - v, c - u + v),
                landing: true,
            });
            pitch.push((s_base + len - hw, z));
            pitch.push((s_base + len + hw, z));
            out_path.push(b);
        }
        s_base += len;
    }
    StairLayout {
        z0,
        z1,
        risers,
        riser,
        pieces,
        path: out_path,
        placed: placed.min(risers),
        pitch,
    }
}

/// Base and top heights of a stair.
pub fn stair_heights(doc: &Document, s: &Stair) -> (f64, f64) {
    let z0 = doc.level_elevation(s.base_level) + s.base_offset;
    let z1 = if doc.level(s.top_level).is_some() {
        doc.level_elevation(s.top_level) + s.top_offset
    } else {
        z0 + 3000.0
    };
    (z0, z1.max(z0 + 1.0))
}

pub fn stair_layout(doc: &Document, id: ElementId) -> Option<StairLayout> {
    let ElementData::Stair(s) = &doc.element(id)?.data else {
        return None;
    };
    if s.path.len() < 2 {
        return None;
    }
    let (z0, z1) = stair_heights(doc, s);
    Some(layout(StairRules::of(doc.type_of(id)), z0, z1, s.width, &s.path))
}

impl StairLayout {
    /// Distance along the centreline of the point nearest `p`.
    pub fn station(&self, p: DVec2) -> f64 {
        let mut best = (f64::INFINITY, 0.0);
        let mut s = 0.0;
        for w in self.path.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = a.distance(b);
            let t = if len > 0.0 {
                ((p - a).dot(b - a) / (len * len)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let d = p.distance(a + (b - a) * t);
            if d < best.0 - 1e-9 {
                best = (d, s + t * len);
            }
            s += len;
        }
        best.1
    }

    /// The pitch line (through the nosings) at a station.
    pub fn pitch_height(&self, s: f64) -> f64 {
        let Some(first) = self.pitch.first() else { return self.z0 };
        if s <= first.0 {
            return first.1 - (first.0 - s) / self.pitch_run().max(1e-9) * self.riser;
        }
        for w in self.pitch.windows(2) {
            let ((s0, z0), (s1, z1)) = (w[0], w[1]);
            if s <= s1 {
                return if s1 - s0 < 1e-9 {
                    z1
                } else {
                    z0 + (z1 - z0) * (s - s0) / (s1 - s0)
                };
            }
        }
        self.z1
    }

    /// Height of the walking surface (tread or landing top) at a station.
    pub fn step_height(&self, s: f64) -> f64 {
        let mut z = self.z0;
        for (st, h) in &self.pitch {
            if *st <= s + 1e-6 {
                z = *h;
            }
        }
        z
    }

    fn pitch_run(&self) -> f64 {
        match self.pitch.as_slice() {
            [a, b, ..] => b.0 - a.0,
            _ => 1.0,
        }
    }
}

/// Railing paths along both sides of a stair, `inset` in from its edges.
pub fn stair_railing_paths(l: &StairLayout, width: f64, inset: f64) -> [Vec<DVec2>; 2] {
    let off = width / 2.0 - inset;
    let side = |sign: f64| offset_polyline(&l.path, sign * off);
    [side(1.0), side(-1.0)]
}

/// Offsets a polyline to its left by `d` (right when negative), joining
/// segments at their intersections.
pub fn offset_polyline(path: &[DVec2], d: f64) -> Vec<DVec2> {
    let n = path.len();
    if n < 2 {
        return path.to_vec();
    }
    let dir = |i: usize| (path[i + 1] - path[i]).try_normalize().unwrap_or(DVec2::X);
    let mut out = vec![path[0] + perp(dir(0)) * d];
    for (i, v) in path.iter().enumerate().take(n - 1).skip(1) {
        let (d0, d1) = (dir(i - 1), dir(i));
        let (p0, p1) = (*v + perp(d0) * d, *v + perp(d1) * d);
        out.push(line_intersection(p0, d0, p1, d1).unwrap_or(p0));
    }
    out.push(path[n - 1] + perp(dir(n - 2)) * d);
    out
}

/// Railing settings.
#[derive(Clone, Copy, Debug)]
pub struct RailingRules {
    pub height: f64,
    pub spacing: f64,
    pub baluster: f64,
    pub rail: f64,
    pub mid_rails: i64,
}

impl RailingRules {
    pub fn of(t: Option<&FamilyType>) -> Self {
        let len = |k: &str, d: f64| t.map(|t| t.length_or(k, d)).unwrap_or(d);
        let mid_rails = match t.and_then(|t| t.params.get("Intermediate Rails")) {
            Some(ParamValue::Integer(n)) => (*n).clamp(0, 10),
            _ => 0,
        };
        RailingRules {
            height: len("Railing Height", 1000.0),
            spacing: len("Baluster Spacing", 120.0).max(50.0),
            baluster: len("Baluster Size", 25.0),
            rail: len("Rail Size", 50.0),
            mid_rails,
        }
    }
}

/// Heights along a railing: the surface it stands on and its pitch line.
pub struct RailingBase {
    stair: Option<StairLayout>,
    z: f64,
}

impl RailingBase {
    pub fn of(doc: &Document, r: &Railing) -> Self {
        let stair = if r.host.is_valid() { stair_layout(doc, r.host) } else { None };
        RailingBase {
            stair,
            z: doc.level_elevation(r.level) + r.base_offset,
        }
    }

    /// Where balusters stand.
    pub fn surface(&self, p: DVec2) -> f64 {
        match &self.stair {
            Some(l) => l.step_height(l.station(p)),
            None => self.z,
        }
    }

    /// Where rails run (parallel to the stair's pitch).
    pub fn pitch(&self, p: DVec2) -> f64 {
        match &self.stair {
            Some(l) => l.pitch_height(l.station(p)),
            None => self.z,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> StairRules {
        StairRules {
            max_riser: 180.0,
            tread: 280.0,
            structural_depth: 150.0,
            monolithic: true,
            ..StairRules::of(None)
        }
    }

    #[test]
    fn straight_run() {
        // 3000 mm needs 17 risers (176.5 each) and 16 treads.
        let l = layout(rules(), 0.0, 3000.0, 1000.0, &[DVec2::ZERO, DVec2::new(1000.0, 0.0)]);
        assert_eq!(l.risers, 17);
        assert!((l.riser - 3000.0 / 17.0).abs() < 1e-9);
        assert_eq!(l.pieces.len(), 16);
        // The run grows past the clicked end to fit every tread.
        assert!((l.path[1].x - 16.0 * 280.0).abs() < 1e-9);
        assert_eq!(l.placed, 4);
        assert!((l.pieces[15].z_top - 16.0 * l.riser).abs() < 1e-9);
        assert!((l.pitch_height(8.0 * 280.0) - 9.0 * l.riser).abs() < 1e-6);
    }

    #[test]
    fn u_shaped_with_landing() {
        // First run holds 8 treads (2240 + half the landing), then turns.
        let path = [
            DVec2::ZERO,
            DVec2::new(2740.0, 0.0),
            DVec2::new(2740.0, 1000.0),
            DVec2::new(0.0, 1000.0),
        ];
        let l = layout(rules(), 0.0, 3000.0, 1000.0, &path);
        let landings: Vec<_> = l.pieces.iter().filter(|p| p.landing).collect();
        assert_eq!(landings.len(), 2);
        let treads = l.pieces.iter().filter(|p| !p.landing).count();
        assert_eq!(treads, 16);
        assert!((landings[0].z_top - 8.0 * l.riser).abs() < 1e-9);
        // The railings' surface is flat across the landing.
        let s = l.station(DVec2::new(2740.0, 500.0));
        assert!((l.step_height(s) - 8.0 * l.riser).abs() < 1e-9);
    }
}
