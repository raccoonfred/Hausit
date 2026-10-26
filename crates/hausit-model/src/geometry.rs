//! Element geometry: turns parametric elements into solids.
//!
//! Most elements are unions of vertical [`Prism`]s (a plan polygon extruded
//! between two heights), which views can cut exactly. Roofs add sloped
//! [`Face`]s.

use crate::category::Category;
use crate::document::Document;
use crate::element::*;
use hausit_core::geom::{clip_polygon_2d, cross2, line_intersection, perp, polygon_area, polygon_normal};
use hausit_core::{Aabb, DVec2, DVec3, ElementId, EPS, JOIN_TOLERANCE};

/// A plan polygon extruded from `z0` to `z1`. Footprint is counter-clockwise.
#[derive(Clone, Debug, PartialEq)]
pub struct Prism {
    pub footprint: Vec<DVec2>,
    pub z0: f64,
    pub z1: f64,
}

impl Prism {
    pub fn new(mut footprint: Vec<DVec2>, z0: f64, z1: f64) -> Self {
        if polygon_area(&footprint) < 0.0 {
            footprint.reverse();
        }
        Prism {
            footprint,
            z0: z0.min(z1),
            z1: z0.max(z1),
        }
    }

    pub fn faces(&self) -> Vec<Face> {
        let n = self.footprint.len();
        let mut faces = Vec::with_capacity(n + 2);
        let at = |p: DVec2, z: f64| DVec3::new(p.x, p.y, z);
        faces.push(Face {
            pts: self.footprint.iter().rev().map(|p| at(*p, self.z0)).collect(),
        });
        faces.push(Face {
            pts: self.footprint.iter().map(|p| at(*p, self.z1)).collect(),
        });
        for i in 0..n {
            let (a, b) = (self.footprint[i], self.footprint[(i + 1) % n]);
            faces.push(Face {
                pts: vec![at(a, self.z0), at(b, self.z0), at(b, self.z1), at(a, self.z1)],
            });
        }
        faces
    }

    pub fn volume(&self) -> f64 {
        polygon_area(&self.footprint).abs() * (self.z1 - self.z0)
    }
}

/// A planar polygon, counter-clockwise seen from outside the solid.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub pts: Vec<DVec3>,
}

impl Face {
    pub fn normal(&self) -> DVec3 {
        polygon_normal(&self.pts).normalize_or_zero()
    }

    /// Reverses the face if it does not point along `dir`.
    fn oriented(mut self, dir: DVec3) -> Self {
        if polygon_normal(&self.pts).dot(dir) < 0.0 {
            self.pts.reverse();
        }
        self
    }
}

/// Rendering hint for a part of an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    Default,
    Glass,
    Panel,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Prism(Prism),
    Faces(Vec<Face>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub material: Material,
    pub shape: Shape,
}

impl Part {
    fn prism(material: Material, p: Prism) -> Self {
        Part {
            material,
            shape: Shape::Prism(p),
        }
    }

    pub fn faces(&self) -> Vec<Face> {
        match &self.shape {
            Shape::Prism(p) => p.faces(),
            Shape::Faces(f) => f.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ElementGeometry {
    pub id: ElementId,
    pub category: Category,
    pub parts: Vec<Part>,
}

impl ElementGeometry {
    pub fn bounds(&self) -> Aabb {
        let mut b = Aabb::EMPTY;
        for part in &self.parts {
            match &part.shape {
                Shape::Prism(p) => {
                    for q in &p.footprint {
                        b.include(DVec3::new(q.x, q.y, p.z0));
                        b.include(DVec3::new(q.x, q.y, p.z1));
                    }
                }
                Shape::Faces(fs) => fs.iter().flat_map(|f| &f.pts).for_each(|p| b.include(*p)),
            }
        }
        b
    }
}

/// Geometry of every 3D element in the document.
pub fn model_geometry(doc: &Document) -> Vec<ElementGeometry> {
    doc.elements().filter_map(|e| element_geometry(doc, e.id)).collect()
}

/// Bounding box of all model geometry (and grids), or `None` if empty.
pub fn model_bounds(doc: &Document) -> Option<Aabb> {
    let mut b = Aabb::EMPTY;
    for g in model_geometry(doc) {
        b = b.union(g.bounds());
    }
    for e in doc.elements_of(Category::Grids) {
        if let ElementData::Grid(g) = &e.data {
            b.include(DVec3::new(g.start.x, g.start.y, 0.0));
            b.include(DVec3::new(g.end.x, g.end.y, 0.0));
        }
    }
    (!b.is_empty()).then_some(b)
}

pub fn element_geometry(doc: &Document, id: ElementId) -> Option<ElementGeometry> {
    let e = doc.element(id)?;
    let parts = match &e.data {
        ElementData::Wall(_) => wall_parts(doc, id),
        ElementData::Floor(f) => {
            let t = doc.type_of(id).map(|t| t.thickness()).unwrap_or(200.0);
            let top = doc.level_elevation(f.level) + f.height_offset;
            if f.boundary.len() < 3 {
                return None;
            }
            vec![Part::prism(Material::Default, Prism::new(f.boundary.clone(), top - t, top))]
        }
        ElementData::Roof(r) => roof_parts(doc, id, r),
        ElementData::Door(o) => opening_parts(doc, id, o, true),
        ElementData::Window(o) => opening_parts(doc, id, o, false),
        ElementData::Level(_) | ElementData::Grid(_) => return None,
    };
    if parts.is_empty() {
        return None;
    }
    Some(ElementGeometry {
        id,
        category: e.category(),
        parts,
    })
}

// ----- walls ----------------------------------------------------------------

/// Plan outline of a wall after joins: `[start_left, end_left, end_right,
/// start_right]`, where left is the exterior side.
pub fn wall_outline(doc: &Document, id: ElementId) -> Option<[DVec2; 4]> {
    let w = doc.element(id)?.as_wall()?;
    let h = doc.type_of(id).map(|t| t.thickness()).unwrap_or(200.0) * 0.5;
    let dir = w.direction();
    if dir == DVec2::ZERO {
        return None;
    }
    let n = perp(dir);
    let mut out = [w.start + n * h, w.end + n * h, w.end - n * h, w.start - n * h];
    // start: away = dir, so +n_away is our left.
    if let Some((pl, pr)) = join_end(doc, id, w.start, dir, h) {
        out[0] = pl;
        out[3] = pr;
    }
    // end: away = -dir, so +n_away is our right.
    if let Some((pr, pl)) = join_end(doc, id, w.end, -dir, h) {
        out[1] = pl;
        out[2] = pr;
    }
    Some(out)
}

/// Computes the cleaned-up corner points for one wall end. Returns the
/// points on the `+perp(away)` and `-perp(away)` faces.
fn join_end(doc: &Document, me: ElementId, p: DVec2, away: DVec2, h: f64) -> Option<(DVec2, DVec2)> {
    let my_base = doc.element(me)?.as_wall()?.base_level;
    let n = perp(away);
    let mut corner = None;
    let mut corner_count = 0;
    let mut tee = None;
    for e in doc.elements_of(Category::Walls) {
        if e.id == me {
            continue;
        }
        let w = e.as_wall().unwrap();
        if w.base_level != my_base {
            continue;
        }
        let oh = doc.type_of(e.id).map(|t| t.thickness()).unwrap_or(200.0) * 0.5;
        let od = w.direction();
        if od == DVec2::ZERO {
            continue;
        }
        if w.start.distance(p) < JOIN_TOLERANCE {
            corner = Some((od, oh));
            corner_count += 1;
        } else if w.end.distance(p) < JOIN_TOLERANCE {
            corner = Some((-od, oh));
            corner_count += 1;
        } else {
            // T-junction: our end touches the other wall's interior.
            let (q, t) = hausit_core::geom::closest_on_segment(p, w.start, w.end);
            if t > 0.0 && t < 1.0 && q.distance(p) <= oh + JOIN_TOLERANCE && cross2(away, od).abs() > 0.1 {
                tee = Some((w.start, od, oh));
            }
        }
    }
    if corner_count == 1 {
        let (b, oh) = corner.unwrap();
        if cross2(away, b).abs() < 1e-3 {
            return None; // collinear: plain butt join
        }
        let m = perp(b);
        let left = line_intersection(p + n * h, away, p - m * oh, b)?;
        let right = line_intersection(p - n * h, away, p + m * oh, b)?;
        // Guard against extreme miters at very sharp angles.
        if left.distance(p) > 10.0 * h.max(oh) || right.distance(p) > 10.0 * h.max(oh) {
            return None;
        }
        return Some((left, right));
    }
    if corner_count == 0 {
        if let Some((q0, od, oh)) = tee {
            let m = perp(od);
            // Face of the other wall on our side.
            let side = if (p - q0).dot(m) + away.dot(m) * oh >= 0.0 { 1.0 } else { -1.0 };
            let face_p = q0 + m * oh * side;
            let left = line_intersection(p + n * h, away, face_p, od)?;
            let right = line_intersection(p - n * h, away, face_p, od)?;
            return Some((left, right));
        }
    }
    None
}

/// Openings of a wall as (axis start, axis end, z bottom, z top).
fn wall_holes(doc: &Document, id: ElementId, w: &Wall) -> Vec<(f64, f64, f64, f64)> {
    let zb = doc.wall_base_z(w);
    doc.openings_in(id)
        .filter_map(|(e, o)| {
            let t = doc.type_of(e.id)?;
            let (half, hgt) = (t.width() * 0.5, t.height());
            let z0 = doc.level_elevation(w.base_level) + o.sill_height;
            Some((o.offset - half, o.offset + half, z0.max(zb), z0 + hgt))
        })
        .collect()
}

fn wall_parts(doc: &Document, id: ElementId) -> Vec<Part> {
    let Some(w) = doc.element(id).and_then(|e| e.as_wall()) else {
        return vec![];
    };
    let Some(outline) = wall_outline(doc, id) else { return vec![] };
    let (zb, zt) = (doc.wall_base_z(w), doc.wall_top_z(w));
    if zt - zb < EPS {
        return vec![];
    }
    let dir = w.direction();
    let base = dir.dot(w.start);
    let holes = wall_holes(doc, id, w);

    // Split the wall along its axis at every opening edge.
    let mut cuts: Vec<f64> = holes
        .iter()
        .flat_map(|h| [h.0, h.1])
        .filter(|s| *s > EPS && *s < w.length() - EPS)
        .collect();
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < EPS);
    let mut bounds = vec![f64::NEG_INFINITY];
    bounds.extend(cuts);
    bounds.push(f64::INFINITY);

    let mut parts = Vec::new();
    for pair in bounds.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let mut poly = outline.to_vec();
        if a.is_finite() {
            poly = clip_polygon_2d(&poly, -dir, -(base + a));
        }
        if b.is_finite() {
            poly = clip_polygon_2d(&poly, dir, base + b);
        }
        if poly.len() < 3 || polygon_area(&poly).abs() < EPS {
            continue;
        }
        let mid = if a.is_finite() && b.is_finite() {
            (a + b) * 0.5
        } else if a.is_finite() {
            a + 1.0
        } else {
            b - 1.0
        };
        // Vertical intervals removed by openings covering this segment.
        let mut gaps: Vec<(f64, f64)> = holes.iter().filter(|h| h.0 < mid && mid < h.1).map(|h| (h.2, h.3)).collect();
        gaps.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut z = zb;
        for (g0, g1) in gaps {
            if g0 > z + EPS {
                parts.push(Part::prism(Material::Default, Prism::new(poly.clone(), z, g0.min(zt))));
            }
            z = z.max(g1);
            if z >= zt {
                break;
            }
        }
        if zt > z + EPS {
            parts.push(Part::prism(Material::Default, Prism::new(poly, z, zt)));
        }
    }
    parts
}

// ----- doors & windows ------------------------------------------------------

/// Placement frame of a hosted opening: centre point on the wall axis,
/// wall direction, wall thickness, bottom z, width, height.
pub struct OpeningFrame {
    pub center: DVec2,
    pub dir: DVec2,
    pub wall_thickness: f64,
    pub z0: f64,
    pub width: f64,
    pub height: f64,
    pub thickness: f64,
}

pub fn opening_frame(doc: &Document, id: ElementId) -> Option<OpeningFrame> {
    let o = doc.element(id)?.as_opening()?;
    let w = doc.element(o.host)?.as_wall()?;
    let t = doc.type_of(id)?;
    Some(OpeningFrame {
        center: w.point_at(o.offset),
        dir: w.direction(),
        wall_thickness: doc.type_of(o.host).map(|t| t.thickness()).unwrap_or(200.0),
        z0: doc.level_elevation(w.base_level) + o.sill_height,
        width: t.width(),
        height: t.height(),
        thickness: t.thickness(),
    })
}

fn rect(center: DVec2, dir: DVec2, along: (f64, f64), across: (f64, f64)) -> Vec<DVec2> {
    let n = perp(dir);
    vec![
        center + dir * along.0 + n * across.0,
        center + dir * along.1 + n * across.0,
        center + dir * along.1 + n * across.1,
        center + dir * along.0 + n * across.1,
    ]
}

fn opening_parts(doc: &Document, id: ElementId, _o: &HostedOpening, door: bool) -> Vec<Part> {
    let Some(f) = opening_frame(doc, id) else { return vec![] };
    let (hw, ht) = (f.width * 0.5, f.thickness * 0.5);
    let frame_w = 50.0;
    let frame_d = (f.wall_thickness * 0.5).min(60.0);
    let mut parts = Vec::new();
    // Frame: two jambs and a head (plus a sill for windows).
    let jamb = |a: f64, b: f64| rect(f.center, f.dir, (a, b), (-frame_d, frame_d));
    parts.push(Part::prism(
        Material::Default,
        Prism::new(jamb(-hw, -hw + frame_w), f.z0, f.z0 + f.height),
    ));
    parts.push(Part::prism(
        Material::Default,
        Prism::new(jamb(hw - frame_w, hw), f.z0, f.z0 + f.height),
    ));
    parts.push(Part::prism(
        Material::Default,
        Prism::new(jamb(-hw, hw), f.z0 + f.height - frame_w, f.z0 + f.height),
    ));
    if door {
        let leaves = doc.type_of(id).and_then(|t| match t.params.get("Leaves") {
            Some(crate::ParamValue::Integer(n)) => Some((*n).clamp(1, 2) as usize),
            _ => None,
        });
        let inner = hw - frame_w;
        let z1 = f.z0 + f.height - frame_w;
        if leaves == Some(2) {
            parts.push(Part::prism(
                Material::Panel,
                Prism::new(rect(f.center, f.dir, (-inner, -2.0), (-ht, ht)), f.z0, z1),
            ));
            parts.push(Part::prism(
                Material::Panel,
                Prism::new(rect(f.center, f.dir, (2.0, inner), (-ht, ht)), f.z0, z1),
            ));
        } else {
            parts.push(Part::prism(
                Material::Panel,
                Prism::new(rect(f.center, f.dir, (-inner, inner), (-ht, ht)), f.z0, z1),
            ));
        }
    } else {
        parts.push(Part::prism(Material::Default, Prism::new(jamb(-hw, hw), f.z0, f.z0 + frame_w)));
        let glass = rect(f.center, f.dir, (-hw + frame_w, hw - frame_w), (-6.0, 6.0));
        parts.push(Part::prism(
            Material::Glass,
            Prism::new(glass, f.z0 + frame_w, f.z0 + f.height - frame_w),
        ));
    }
    parts
}

// ----- roofs ----------------------------------------------------------------

/// Local frame of a rectangular footprint: origin corner, long axis, short
/// axis, length, width. `None` if the footprint is not a rectangle.
fn rectangle_frame(fp: &[DVec2]) -> Option<(DVec2, DVec2, DVec2, f64, f64)> {
    if fp.len() != 4 {
        return None;
    }
    let e: Vec<DVec2> = (0..4).map(|i| fp[(i + 1) % 4] - fp[i]).collect();
    for i in 0..4 {
        let (a, b) = (e[i].normalize_or_zero(), e[(i + 1) % 4].normalize_or_zero());
        if a.dot(b).abs() > 1e-6 || e[i].length() < EPS {
            return None;
        }
    }
    let (l0, l1) = (e[0].length(), e[1].length());
    if l0 >= l1 {
        Some((fp[0], e[0] / l0, e[1] / l1, l0, l1))
    } else {
        Some((fp[1], e[1] / l1, -e[0] / l0, l1, l0))
    }
}

/// Lines drawn in plan for a roof: hips and ridge (eaves are the outline).
pub fn roof_plan_lines(r: &Roof) -> Vec<(DVec2, DVec2)> {
    let Some((o, ux, uy, l, w)) = rectangle_frame(&r.footprint) else {
        return vec![];
    };
    if r.slope <= 0.0 {
        return vec![];
    }
    let p = |x: f64, y: f64| o + ux * x + uy * y;
    match r.shape {
        RoofShape::Flat => vec![],
        RoofShape::Gable => vec![(p(0.0, w / 2.0), p(l, w / 2.0))],
        RoofShape::Hip => {
            let h = w / 2.0;
            let (r0, r1) = (p(h, h), p(l - h, h));
            vec![(p(0.0, 0.0), r0), (p(0.0, w), r0), (p(l, 0.0), r1), (p(l, w), r1), (r0, r1)]
        }
    }
}

fn roof_parts(doc: &Document, id: ElementId, r: &Roof) -> Vec<Part> {
    if r.footprint.len() < 3 {
        return vec![];
    }
    let t = doc.type_of(id).map(|t| t.thickness()).unwrap_or(200.0);
    let zb = doc.level_elevation(r.level) + r.base_offset;
    let frame = rectangle_frame(&r.footprint);
    let (Some((o, ux, uy, l, w)), true) = (frame, r.shape != RoofShape::Flat && r.slope > 0.0) else {
        return vec![Part::prism(Material::Default, Prism::new(r.footprint.clone(), zb, zb + t))];
    };
    let tan = r.slope.to_radians().tan();
    let hv = t / r.slope.to_radians().cos();
    let rise = w / 2.0 * tan;
    let p = |x: f64, y: f64, z: f64| {
        let q = o + ux * x + uy * y;
        DVec3::new(q.x, q.y, z)
    };
    let up = DVec3::Z;
    let mut faces = Vec::new();
    let ridge_x = if r.shape == RoofShape::Hip { w / 2.0 } else { 0.0 };
    let (zr, ze) = (zb + rise, zb);
    for (dz, dir) in [(hv, up), (0.0, -up)] {
        let e = [p(0.0, 0.0, ze + dz), p(l, 0.0, ze + dz), p(l, w, ze + dz), p(0.0, w, ze + dz)];
        let r0 = p(ridge_x, w / 2.0, zr + dz);
        let r1 = p(l - ridge_x, w / 2.0, zr + dz);
        faces.push(
            Face {
                pts: vec![e[0], e[1], r1, r0],
            }
            .oriented(dir),
        );
        faces.push(
            Face {
                pts: vec![e[2], e[3], r0, r1],
            }
            .oriented(dir),
        );
        if r.shape == RoofShape::Hip {
            faces.push(Face { pts: vec![e[1], e[2], r1] }.oriented(dir));
            faces.push(Face { pts: vec![e[3], e[0], r0] }.oriented(dir));
        }
    }
    let center = p(l / 2.0, w / 2.0, zb);
    // Fascia along sloped eaves.
    let eave_edges: Vec<((f64, f64), (f64, f64))> = match r.shape {
        RoofShape::Hip => vec![
            ((0.0, 0.0), (l, 0.0)),
            ((l, 0.0), (l, w)),
            ((l, w), (0.0, w)),
            ((0.0, w), (0.0, 0.0)),
        ],
        _ => vec![((0.0, 0.0), (l, 0.0)), ((l, w), (0.0, w))],
    };
    for (a, b) in eave_edges {
        let f = Face {
            pts: vec![p(a.0, a.1, zb), p(b.0, b.1, zb), p(b.0, b.1, zb + hv), p(a.0, a.1, zb + hv)],
        };
        let mid = (f.pts[0] + f.pts[1]) * 0.5;
        faces.push(f.oriented(mid - center));
    }
    if r.shape == RoofShape::Gable {
        for x in [0.0, l] {
            let f = Face {
                pts: vec![
                    p(x, 0.0, zb),
                    p(x, w / 2.0, zb + rise),
                    p(x, w, zb),
                    p(x, w, zb + hv),
                    p(x, w / 2.0, zb + rise + hv),
                    p(x, 0.0, zb + hv),
                ],
            };
            let out = p(x, w / 2.0, zb) - center;
            faces.push(f.oriented(out));
        }
    }
    vec![Part {
        material: Material::Default,
        shape: Shape::Faces(faces),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::sample_project;

    #[test]
    fn corner_walls_are_mitred() {
        let doc = sample_project();
        let wall = doc.elements_of(Category::Walls).next().unwrap().id;
        let o = wall_outline(&doc, wall).unwrap();
        // First wall goes (0,0)→(0,8000), 300 thick. Its exterior (left,
        // x<0) face should extend to the outer corners at y=-150 / 8150.
        assert!((o[0] - DVec2::new(-150.0, -150.0)).length() < 1e-6, "{o:?}");
        assert!((o[1] - DVec2::new(-150.0, 8150.0)).length() < 1e-6, "{o:?}");
        assert!((o[3] - DVec2::new(150.0, 150.0)).length() < 1e-6, "{o:?}");
    }

    #[test]
    fn openings_cut_walls() {
        let doc = sample_project();
        // South wall hosts a door and a window.
        let south = doc.elements_of(Category::Walls).nth(3).unwrap().id;
        let g = element_geometry(&doc, south).unwrap();
        let w = doc.element(south).unwrap().as_wall().unwrap();
        let vol: f64 = g
            .parts
            .iter()
            .map(|p| if let Shape::Prism(p) = &p.shape { p.volume() } else { 0.0 })
            .sum();
        let gross = w.length() * 300.0 * 4000.0; // a square mitre keeps centreline length × thickness
        let holes = 915.0 * 2134.0 * 300.0 + 1200.0 * 1500.0 * 300.0;
        assert!((vol - (gross - holes)).abs() < 1.0, "vol {vol} expected {}", gross - holes);
    }

    #[test]
    fn hip_roof_faces() {
        let doc = sample_project();
        let roof = doc.elements_of(Category::Roofs).next().unwrap().id;
        let g = element_geometry(&doc, roof).unwrap();
        let faces = g.parts[0].faces();
        assert_eq!(faces.len(), 4 + 4 + 4);
        for f in &faces[..4] {
            assert!(f.normal().z > 0.0);
        }
    }
}
