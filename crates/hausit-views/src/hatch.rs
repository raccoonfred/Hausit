//! Cut patterns of materials: hatch lines clipped to a region, sized on
//! paper so they read the same at every view scale.

use crate::{DisplayList, Prim, Stroke};
use hausit_core::geom::{clip_polygon_2d, perp, point_in_polygon};
use hausit_core::DVec2;
use hausit_model::materials::FillPattern;
use hausit_model::{Document, ElementId};

/// Spacing of hatch lines on paper, mm.
const SPACING: f64 = 1.2;

/// One region of a compound layer: its outline, the direction the layer
/// runs in, and the material that fills it.
pub struct Band<'a> {
    pub outline: Vec<DVec2>,
    pub along: DVec2,
    pub material: &'a str,
}

/// Splits a cut region into the layers of a compound type. `origin` is a
/// point on the reference line, `along` the run direction and `out` the
/// unit normal pointing to the first layer's face, which lies `half` from
/// the reference line. `layers` are (material, thickness) from that face.
/// Also returns the boundary lines between layers.
pub fn layer_bands<'a>(
    outline: &[DVec2],
    origin: DVec2,
    along: DVec2,
    out: DVec2,
    half: f64,
    layers: impl IntoIterator<Item = (&'a str, f64)>,
) -> (Vec<Band<'a>>, Vec<(DVec2, DVec2)>) {
    let base = origin.dot(out);
    let mut depth = 0.0;
    let mut bands = Vec::new();
    let mut lines = Vec::new();
    let mut layers = layers.into_iter().peekable();
    while let Some((material, t)) = layers.next() {
        let (d0, d1) = (depth, depth + t);
        depth = d1;
        // Keep base + half - d1 <= s <= base + half - d0; the outer faces
        // are left open so joins and pieces keep their own shape.
        let mut poly = outline.to_vec();
        if d0 > 0.0 {
            poly = clip_polygon_2d(&poly, out, base + half - d0);
        }
        if layers.peek().is_some() {
            poly = clip_polygon_2d(&poly, -out, -(base + half - d1));
            lines.extend(clip_line(outline, out * (base + half - d1), perp(out)));
        }
        if poly.len() >= 3 {
            bands.push(Band {
                outline: poly,
                along,
                material,
            });
        }
    }
    (bands, lines)
}

/// Draws a band's material fill and pattern.
pub fn draw_band(dl: &mut DisplayList, doc: &Document, band: &Band, scale: f64, element: Option<ElementId>) {
    let (pattern, color) = match doc.material(band.material) {
        Some(m) => (m.cut_pattern, m.pattern_color),
        None => (FillPattern::None, [0, 0, 0]),
    };
    let rgba = [color[0], color[1], color[2], 255];
    let fill = if pattern == FillPattern::Solid {
        rgba
    } else {
        [255, 255, 255, 255]
    };
    dl.push(Prim::Polygon(band.outline.clone()), None, Some(fill), element);
    let stroke = Stroke::new(1.0, rgba);
    for (a, b) in pattern_lines(&band.outline, pattern, band.along, scale) {
        dl.line(a, b, stroke, None);
    }
}

/// The hatch lines of `pattern` inside `poly` at a view scale of 1:`scale`.
pub fn pattern_lines(poly: &[DVec2], pattern: FillPattern, along: DVec2, scale: f64) -> Vec<(DVec2, DVec2)> {
    let sp = SPACING * scale;
    let along = along.try_normalize().unwrap_or(DVec2::X);
    let diag = DVec2::new(1.0, 1.0).normalize();
    match pattern {
        FillPattern::None | FillPattern::Solid => vec![],
        FillPattern::Diagonal => parallel_lines(poly, diag, sp),
        FillPattern::Crosshatch => {
            let mut l = parallel_lines(poly, diag, sp);
            l.extend(parallel_lines(poly, perp(diag), sp));
            l
        }
        FillPattern::Parallel => parallel_lines(poly, along, sp * 0.5),
        FillPattern::Concrete => concrete(poly, sp),
        FillPattern::Insulation => insulation(poly, along),
    }
}

/// Lines in direction `dir`, `spacing` apart, clipped to `poly`.
fn parallel_lines(poly: &[DVec2], dir: DVec2, spacing: f64) -> Vec<(DVec2, DVec2)> {
    let n = perp(dir);
    let (lo, hi) = range(poly, n);
    let mut out = Vec::new();
    if spacing <= 0.0 || (hi - lo) / spacing > 5000.0 {
        return out;
    }
    let mut k = (lo / spacing).ceil();
    while k * spacing <= hi {
        out.extend(clip_line(poly, n * (k * spacing), dir));
        k += 1.0;
    }
    out
}

/// Small triangles and dots on a jittered grid, as concrete aggregate.
fn concrete(poly: &[DVec2], spacing: f64) -> Vec<(DVec2, DVec2)> {
    let (x0, x1) = range(poly, DVec2::X);
    let (y0, y1) = range(poly, DVec2::Y);
    let step = spacing * 1.2;
    let mut out = Vec::new();
    if step <= 0.0 || ((x1 - x0) / step) * ((y1 - y0) / step) > 50_000.0 {
        return out;
    }
    let size = spacing * 0.25;
    let mut j = (y0 / step).floor() as i64;
    while j as f64 * step <= y1 {
        let mut i = (x0 / step).floor() as i64;
        while i as f64 * step <= x1 {
            let h = hash(i, j);
            let jitter = DVec2::new((h & 0xff) as f64 / 255.0, ((h >> 8) & 0xff) as f64 / 255.0);
            let c = (DVec2::new(i as f64, j as f64) + jitter * 0.8) * step;
            if inside_with_margin(c, poly, size) {
                if h & 0x10000 != 0 {
                    let a = ((h >> 17) & 0xff) as f64 / 255.0 * std::f64::consts::TAU;
                    let pts: Vec<DVec2> = (0..3).map(|k| c + DVec2::from_angle(a + k as f64 * 2.1) * size).collect();
                    for k in 0..3 {
                        out.push((pts[k], pts[(k + 1) % 3]));
                    }
                } else {
                    let d = DVec2::new(size * 0.15, 0.0);
                    out.push((c - d, c + d));
                }
            }
            i += 1;
        }
        j += 1;
    }
    out
}

/// The zigzag batt symbol, spanning the layer across its thickness.
fn insulation(poly: &[DVec2], along: DVec2) -> Vec<(DVec2, DVec2)> {
    let n = perp(along);
    let (lo, hi) = range(poly, n);
    let (a0, a1) = range(poly, along);
    let t = hi - lo;
    if t <= 0.0 || (a1 - a0) / t > 5000.0 {
        return vec![];
    }
    let inset = t * 0.08;
    let step = t * 0.5;
    let mut out = Vec::new();
    let mut s = (a0 / step).floor();
    while s * step < a1 {
        let side = |k: f64| if (k as i64).rem_euclid(2) == 0 { lo + inset } else { hi - inset };
        let p = along * (s * step) + n * side(s);
        let q = along * ((s + 1.0) * step) + n * side(s + 1.0);
        out.extend(clip_segment(poly, p, q));
        s += 1.0;
    }
    out
}

fn inside_with_margin(c: DVec2, poly: &[DVec2], margin: f64) -> bool {
    point_in_polygon(c, poly)
        && (0..poly.len()).all(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            hausit_core::geom::distance_to_segment(c, a, b) > margin
        })
}

fn hash(i: i64, j: i64) -> u64 {
    let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (j as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^ (h >> 32)
}

fn range(poly: &[DVec2], axis: DVec2) -> (f64, f64) {
    poly.iter()
        .map(|p| p.dot(axis))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)))
}

/// The parts of the infinite line `p + t·dir` inside `poly` (even-odd).
fn clip_line(poly: &[DVec2], p: DVec2, dir: DVec2) -> Vec<(DVec2, DVec2)> {
    let mut ts = crossings(poly, p, dir);
    ts.sort_by(f64::total_cmp);
    ts.as_chunks::<2>()
        .0
        .iter()
        .filter(|[a, b]| b - a > 1e-6)
        .map(|[a, b]| (p + dir * *a, p + dir * *b))
        .collect()
}

/// The parts of segment `a`–`b` inside `poly`.
fn clip_segment(poly: &[DVec2], a: DVec2, b: DVec2) -> Vec<(DVec2, DVec2)> {
    let d = b - a;
    let mut ts: Vec<f64> = crossings(poly, a, d).into_iter().filter(|t| *t > 0.0 && *t < 1.0).collect();
    ts.push(0.0);
    ts.push(1.0);
    ts.sort_by(f64::total_cmp);
    ts.windows(2)
        .filter(|w| w[1] - w[0] > 1e-9 && point_in_polygon(a + d * ((w[0] + w[1]) * 0.5), poly))
        .map(|w| (a + d * w[0], a + d * w[1]))
        .collect()
}

/// Parameters where the line `p + t·d` crosses the edges of `poly`.
fn crossings(poly: &[DVec2], p: DVec2, d: DVec2) -> Vec<f64> {
    let mut ts = Vec::new();
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let e = b - a;
        let den = d.perp_dot(e);
        if den.abs() < 1e-12 {
            continue;
        }
        let t = (a - p).perp_dot(e) / den;
        let u = (a - p).perp_dot(d) / den;
        // Half-open on the edge so a vertex counts once.
        if (0.0..1.0).contains(&u) {
            ts.push(t);
        }
    }
    ts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(s: f64) -> Vec<DVec2> {
        vec![DVec2::ZERO, DVec2::new(s, 0.0), DVec2::new(s, s), DVec2::new(0.0, s)]
    }

    #[test]
    fn hatch_lines_stay_inside() {
        let sq = square(1000.0);
        for pattern in FillPattern::ALL {
            for (a, b) in pattern_lines(&sq, pattern, DVec2::X, 50.0) {
                for p in [a, b, (a + b) * 0.5] {
                    assert!(
                        p.x > -1e-6 && p.x < 1000.0 + 1e-6 && p.y > -1e-6 && p.y < 1000.0 + 1e-6,
                        "{pattern:?} {p}"
                    );
                }
            }
        }
        assert!(!pattern_lines(&sq, FillPattern::Diagonal, DVec2::X, 50.0).is_empty());
    }

    #[test]
    fn bands_split_by_thickness() {
        // A 300 thick wall along +x, exterior (left, +y) face at y = 150.
        let outline = vec![
            DVec2::new(0.0, -150.0),
            DVec2::new(1000.0, -150.0),
            DVec2::new(1000.0, 150.0),
            DVec2::new(0.0, 150.0),
        ];
        let (bands, lines) = layer_bands(&outline, DVec2::ZERO, DVec2::X, DVec2::Y, 150.0, [("A", 100.0), ("B", 200.0)]);
        assert_eq!(bands.len(), 2);
        assert_eq!(lines.len(), 1);
        let ys = |b: &Band| range(&b.outline, DVec2::Y);
        assert_eq!(ys(&bands[0]), (50.0, 150.0));
        assert_eq!(ys(&bands[1]), (-150.0, 50.0));
    }
}
