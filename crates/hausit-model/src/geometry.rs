//! Element geometry: turns parametric elements into solids.
//!
//! Most elements are unions of vertical [`Prism`]s (a plan polygon extruded
//! between two heights), which views can cut exactly. Roofs add sloped
//! [`Face`]s.

use crate::category::Category;
use crate::document::Document;
use crate::element::*;
use hausit_core::geom::{
    clip_polygon_2d, cross2, distance_to_segment, line_intersection, perp, point_in_polygon, polygon_area, polygon_normal,
};
use hausit_core::skeleton::{straight_skeleton, Skeleton};
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
    /// A named material for this part, overriding the element's (used
    /// by loadable families).
    pub material_name: Option<String>,
    pub shape: Shape,
}

impl Part {
    fn prism(material: Material, p: Prism) -> Self {
        Part {
            material_name: None,
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
    /// Plan view of a vertical solid: its footprint, base height and the
    /// lowest point of its top (a sloped top can be higher elsewhere).
    /// `None` for solids that are not vertical extrusions.
    pub fn vertical_extent(&self) -> Option<(Vec<DVec2>, f64, f64)> {
        match &self.shape {
            Shape::Prism(p) => Some((p.footprint.clone(), p.z0, p.z1)),
            Shape::Faces(faces) => {
                let bottom = faces.iter().find(|f| f.normal().z < -0.999)?;
                let z0 = bottom.pts.first()?.z;
                let top = faces
                    .iter()
                    .filter(|f| f.normal().z > 1e-6)
                    .flat_map(|f| f.pts.iter().map(|p| p.z))
                    .fold(f64::INFINITY, f64::min);
                top.is_finite()
                    .then(|| (bottom.pts.iter().rev().map(|p| p.truncate()).collect(), z0, top))
            }
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
        ElementData::Column(c) => {
            let (z0, z1) = column_extent(doc, c);
            if z1 - z0 < EPS {
                return None;
            }
            vec![Part::prism(Material::Default, Prism::new(column_outline(doc, id)?, z0, z1))]
        }
        ElementData::Framing(f) => {
            let (a, b) = framing_axis(doc, id, f);
            if a.distance(b) < EPS {
                return None;
            }
            let profile = section_profile(doc.type_of(id));
            vec![Part {
                material_name: None,
                material: Material::Default,
                shape: Shape::Faces(sweep(&profile, a, b)),
            }]
        }
        ElementData::Footing(_) | ElementData::WallFooting(_) => {
            let (outline, z0, z1) = foundation_extent(doc, id)?;
            vec![Part::prism(Material::Default, Prism::new(outline, z0, z1))]
        }
        ElementData::Stair(_) => stair_parts(doc, id)?,
        ElementData::Ceiling(c) => {
            let t = doc.type_of(id).map(|t| t.thickness()).unwrap_or(25.0);
            let z = doc.level_elevation(c.level) + c.height_offset;
            if c.boundary.len() < 3 {
                return None;
            }
            vec![Part::prism(Material::Default, Prism::new(c.boundary.clone(), z, z + t))]
        }
        ElementData::Railing(r) => railing_parts(doc, id, r),
        ElementData::Component(c) => crate::component::component_parts(doc, c),
        ElementData::DirectShape(d) => d
            .solids
            .iter()
            .map(|s| Part {
                material: match &s.material {
                    Some(m) if m.to_ascii_lowercase().contains("glass") => Material::Glass,
                    _ => Material::Default,
                },
                material_name: s.material.clone(),
                shape: match &s.geometry {
                    ShapeGeometry::Prism { footprint, z0, z1 } => Shape::Prism(Prism::new(footprint.clone(), *z0, *z1)),
                    ShapeGeometry::Faces(faces) => Shape::Faces(faces.iter().map(|f| Face { pts: f.clone() }).collect()),
                },
            })
            .collect(),
        ElementData::Level(_)
        | ElementData::Grid(_)
        | ElementData::Room(_)
        | ElementData::RoomSeparation(_)
        | ElementData::Dimension(_)
        | ElementData::TextNote(_)
        | ElementData::Tag(_) => return None,
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
    for id in doc.index().walls_near(my_base, p) {
        let Some(e) = doc.element(id) else { continue };
        if e.id == me {
            continue;
        }
        let Some(w) = e.as_wall() else { continue };
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
    let (zb, mut zt) = (doc.wall_base_z(w), doc.wall_top_z(w));
    // Attached tops: the straight part stops at the lowest point of the
    // surface above; sloped pieces fill up to it.
    let holes = wall_holes(doc, id, w);
    let opening_cuts: Vec<f64> = holes.iter().flat_map(|h| [h.0, h.1]).collect();
    let attached = attached_top_parts(doc, w, &outline, zt, &opening_cuts);
    if let Some((zmin, _, _)) = &attached {
        zt = zmin.max(zb);
    }
    if zt - zb < EPS && attached.is_none() {
        return vec![];
    }
    let dir = w.direction();
    let base = dir.dot(w.start);

    // Split the wall along its axis at every opening edge, and where the
    // attached pieces above are split, so their shared edges match (and
    // read as one face in 3D).
    let mut cuts: Vec<f64> = opening_cuts
        .iter()
        .copied()
        .chain(attached.iter().flat_map(|a| a.2.iter().copied()))
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
        match &attached {
            // Attached: one solid from here up to the sloped top, so the
            // wall faces have no seam at the straight top's height.
            Some((_, slices, _)) => {
                for (sp, tops) in slices {
                    let c = sp.iter().copied().sum::<DVec2>() / sp.len() as f64;
                    let u = dir.dot(c) - base;
                    if (!a.is_finite() || u > a) && (!b.is_finite() || u < b) {
                        parts.push(solid_under(sp, z, tops));
                    }
                }
            }
            _ if zt > z + EPS => parts.push(Part::prism(Material::Default, Prism::new(poly, z, zt))),
            _ => {}
        }
    }
    parts
}

/// A vertical solid over `poly` from `z0` up to per-corner heights.
fn solid_under(poly: &[DVec2], z0: f64, tops: &[f64]) -> Part {
    let (mut poly, mut tops) = (poly.to_vec(), tops.to_vec());
    if polygon_area(&poly) < 0.0 {
        poly.reverse();
        tops.reverse();
    }
    let n = poly.len();
    let at = |i: usize, z: f64| DVec3::new(poly[i].x, poly[i].y, z);
    let mut faces = vec![
        Face {
            pts: (0..n).rev().map(|i| at(i, z0)).collect(),
        },
        Face {
            pts: (0..n).map(|i| at(i, tops[i])).collect(),
        },
    ];
    for i in 0..n {
        let j = (i + 1) % n;
        let mut pts = vec![at(i, z0), at(j, z0)];
        if tops[j] - z0 > EPS {
            pts.push(at(j, tops[j]));
        }
        if tops[i] - z0 > EPS {
            pts.push(at(i, tops[i]));
        }
        if pts.len() >= 3 {
            faces.push(Face { pts });
        }
    }
    Part {
        material_name: None,
        material: Material::Default,
        shape: Shape::Faces(faces),
    }
}

/// Planar pieces of the surfaces a wall top can attach to: plan polygon
/// and the plane `z = a·x + b·y + c` of the underside over it.
fn attach_surfaces(doc: &Document, target: ElementId) -> Vec<(Vec<DVec2>, DVec3)> {
    let Some(e) = doc.element(target) else { return vec![] };
    match &e.data {
        ElementData::Roof(r) => {
            let zb = doc.level_elevation(r.level) + r.base_offset;
            match roof_skeleton(r) {
                Some(sk) => {
                    let tan = r.slope.to_radians().tan();
                    let slopes = r.edge_slopes();
                    sk.faces
                        .iter()
                        .filter(|f| slopes[f.edge])
                        .filter_map(|f| {
                            let pts: Vec<DVec3> = f.pts.iter().map(|p| DVec3::new(p.x, p.y, zb + p.z * tan)).collect();
                            plane_of(&pts).map(|pl| (pts.iter().map(|p| p.truncate()).collect(), pl))
                        })
                        .collect()
                }
                None => vec![(r.footprint.clone(), DVec3::new(0.0, 0.0, zb))],
            }
        }
        ElementData::Floor(f) => {
            let t = doc.type_of(target).map(|t| t.thickness()).unwrap_or(200.0);
            let top = doc.level_elevation(f.level) + f.height_offset;
            vec![(f.boundary.clone(), DVec3::new(0.0, 0.0, top - t))]
        }
        // Imported roofs: the faces looking down.
        ElementData::DirectShape(d) => d
            .solids
            .iter()
            .flat_map(|s| match &s.geometry {
                ShapeGeometry::Faces(f) => f.clone(),
                ShapeGeometry::Prism { footprint, z0, .. } => {
                    let mut f: Vec<DVec3> = footprint.iter().map(|p| DVec3::new(p.x, p.y, *z0)).collect();
                    if polygon_area(footprint) > 0.0 {
                        f.reverse();
                    }
                    vec![f]
                }
            })
            .filter(|f| f.len() >= 3 && polygon_normal(f).normalize_or_zero().z < -0.05)
            .filter_map(|f| plane_of(&f).map(|pl| (f.iter().map(|p| p.truncate()).collect(), pl)))
            .collect(),
        _ => vec![],
    }
}

/// `(a, b, c)` with z = a·x + b·y + c through a non-vertical planar polygon.
fn plane_of(pts: &[DVec3]) -> Option<DVec3> {
    let n = polygon_normal(pts);
    if n.z.abs() < 1e-9 {
        return None;
    }
    let p = pts[0];
    Some(DVec3::new(-n.x / n.z, -n.y / n.z, p.z + (n.x * p.x + n.y * p.y) / n.z))
}

/// The top of a wall attached to roofs or floors: the wall outline cut
/// into slices (along the wall, wherever the surface changes plane or
/// `extra` asks) with the surface height at each slice corner. Returns
/// the lowest attached height, the slices and the cut positions.
#[allow(clippy::type_complexity)]
fn attached_top_parts(
    doc: &Document,
    w: &Wall,
    outline: &[DVec2; 4],
    zt: f64,
    extra: &[f64],
) -> Option<(f64, Vec<(Vec<DVec2>, Vec<f64>)>, Vec<f64>)> {
    let surfaces: Vec<(Vec<DVec2>, DVec3)> = w.attached_top.iter().flat_map(|t| attach_surfaces(doc, *t)).collect();
    if surfaces.is_empty() {
        return None;
    }
    let dir = w.direction();
    let base = dir.dot(w.start);
    let height_at = |p: DVec2| -> Option<f64> {
        surfaces
            .iter()
            .filter(|(poly, _)| point_in_polygon(p, poly))
            .map(|(_, pl)| pl.x * p.x + pl.y * p.y + pl.z)
            .min_by(f64::total_cmp)
    };
    // Slice wherever a surface edge crosses the wall.
    let faces = [(outline[0], outline[1]), (outline[3], outline[2])];
    let mut us: Vec<f64> = outline.iter().map(|p| dir.dot(*p) - base).collect();
    for (poly, _) in &surfaces {
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            for (p, q) in faces {
                if let Some(x) = line_intersection(p, q - p, a, b - a) {
                    let (on_face, on_edge) = (
                        (x - p).dot(q - p) / (q - p).length_squared(),
                        (x - a).dot(b - a) / (b - a).length_squared(),
                    );
                    if (-1e-9..=1.0 + 1e-9).contains(&on_face) && (-1e-9..=1.0 + 1e-9).contains(&on_edge) {
                        us.push(dir.dot(x) - base);
                    }
                }
            }
        }
    }
    us.extend(extra.iter().copied().filter(|u| *u > 0.0 && *u < w.length()));
    us.sort_by(f64::total_cmp);
    us.dedup_by(|a, b| (*a - *b).abs() < 1.0);

    let mut slices: Vec<(Vec<DVec2>, Vec<f64>)> = Vec::new();
    for pair in us.windows(2) {
        let mut poly = clip_polygon_2d(outline, -dir, -(base + pair[0]));
        poly = clip_polygon_2d(&poly, dir, base + pair[1]);
        if poly.len() < 3 || polygon_area(&poly).abs() < EPS {
            continue;
        }
        let c = poly.iter().copied().sum::<DVec2>() / poly.len() as f64;
        // The plane over the slice's centre, evaluated at its corners.
        let plane = surfaces
            .iter()
            .filter(|(sp, _)| point_in_polygon(c, sp))
            .map(|(_, pl)| *pl)
            .min_by(|a, b| (a.x * c.x + a.y * c.y + a.z).total_cmp(&(b.x * c.x + b.y * c.y + b.z)));
        let tops: Vec<f64> = match plane {
            Some(pl) => poly.iter().map(|p| pl.x * p.x + pl.y * p.y + pl.z).collect(),
            None => poly.iter().map(|p| height_at(*p).unwrap_or(zt)).collect(),
        };
        slices.push((poly, tops));
    }
    let zmin = slices.iter().flat_map(|s| s.1.iter().copied()).fold(f64::INFINITY, f64::min);
    if !zmin.is_finite() {
        return None;
    }
    Some((zmin, slices, us))
}

// ----- columns --------------------------------------------------------------

/// Bottom and top heights of a structural column.
pub fn column_extent(doc: &Document, c: &Column) -> (f64, f64) {
    let z0 = doc.level_elevation(c.base_level) + c.base_offset;
    let z1 = if doc.level(c.top_level).is_some() {
        doc.level_elevation(c.top_level) + c.top_offset
    } else {
        z0 + 3000.0 + c.top_offset
    };
    (z0, z1)
}

/// Plan outline of a column: its type's section ("b" across, "h" deep for
/// rectangles), rotated about its point.
pub fn column_outline(doc: &Document, id: ElementId) -> Option<Vec<DVec2>> {
    let ElementData::Column(c) = &doc.element(id)?.data else {
        return None;
    };
    let (s, co) = c.rotation.to_radians().sin_cos();
    let (u, v) = (DVec2::new(co, s), DVec2::new(-s, co));
    let profile = section_profile(doc.type_of(id));
    Some(profile.iter().map(|p| c.point + u * p.x + v * p.y).collect())
}

// ----- structural sections, framing and foundations -------------------------

/// The cross-section of a column or framing type, centred on the origin:
/// x across the width, y along the depth, counter-clockwise. "Section
/// Shape" picks I (d, bf, tw, tf), Angle (d, bf, tw) or a rectangle (b, h).
pub fn section_profile(t: Option<&crate::family::FamilyType>) -> Vec<DVec2> {
    let len = |k: &str, d: f64| t.map(|t| t.length_or(k, d)).unwrap_or(d);
    let shape = t
        .and_then(|t| match t.params.get("Section Shape") {
            Some(crate::params::ParamValue::Text(s)) => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or("Rectangular");
    let p = DVec2::new;
    match shape {
        "I" => {
            let (hw, hd) = (len("bf", 150.0) / 2.0, len("d", 300.0) / 2.0);
            let (htw, tf) = (len("tw", 7.0) / 2.0, len("tf", 11.0).min(hd));
            vec![
                p(-hw, -hd),
                p(hw, -hd),
                p(hw, -hd + tf),
                p(htw, -hd + tf),
                p(htw, hd - tf),
                p(hw, hd - tf),
                p(hw, hd),
                p(-hw, hd),
                p(-hw, hd - tf),
                p(-htw, hd - tf),
                p(-htw, -hd + tf),
                p(-hw, -hd + tf),
            ]
        }
        "Angle" => {
            let (hw, hd) = (len("bf", 100.0) / 2.0, len("d", 100.0) / 2.0);
            let t = len("tw", 10.0).min(hw).min(hd);
            vec![
                p(-hw, -hd),
                p(hw, -hd),
                p(hw, -hd + t),
                p(-hw + t, -hd + t),
                p(-hw + t, hd),
                p(-hw, hd),
            ]
        }
        _ => {
            let (hb, hh) = (len("b", 300.0) / 2.0, len("h", 300.0) / 2.0);
            vec![p(-hb, -hh), p(hb, -hh), p(hb, hh), p(-hb, hh)]
        }
    }
}

/// Depth of a section (its extent along y).
pub fn section_depth(t: Option<&crate::family::FamilyType>) -> f64 {
    let ys = section_profile(t)
        .iter()
        .map(|p| p.y)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), y| (a.min(y), b.max(y)));
    ys.1 - ys.0
}

/// The centreline of a framing member in 3D. A beam's reference line is
/// its top, so its centreline sits half its depth lower.
pub fn framing_axis(doc: &Document, id: ElementId, f: &Framing) -> (DVec3, DVec3) {
    let z = doc.level_elevation(f.level);
    let drop = match f.kind {
        FramingKind::Beam => section_depth(doc.type_of(id)) / 2.0,
        FramingKind::Brace => 0.0,
    };
    (
        DVec3::new(f.start.x, f.start.y, z + f.start_offset - drop),
        DVec3::new(f.end.x, f.end.y, z + f.end_offset - drop),
    )
}

/// Sweeps a closed profile along the segment `a`–`b`. Profile x runs to
/// the left of the member (seen from above), y upwards.
pub fn sweep(profile: &[DVec2], a: DVec3, b: DVec3) -> Vec<Face> {
    let d = (b - a).normalize();
    let across = DVec3::Z.cross(d).try_normalize().unwrap_or(DVec3::Y);
    let up = d.cross(across);
    let at = |p: DVec2, base: DVec3| base + across * p.x + up * p.y;
    let mut faces = vec![
        Face {
            pts: profile.iter().map(|p| at(*p, a)).collect(),
        }
        .oriented(-d),
        Face {
            pts: profile.iter().map(|p| at(*p, b)).collect(),
        }
        .oriented(d),
    ];
    let n = profile.len();
    let ccw = polygon_area(profile) > 0.0;
    for i in 0..n {
        let (p, q) = (profile[i], profile[(i + 1) % n]);
        let e = q - p;
        let out2 = if ccw { DVec2::new(e.y, -e.x) } else { DVec2::new(-e.y, e.x) };
        let out = across * out2.x + up * out2.y;
        faces.push(
            Face {
                pts: vec![at(p, a), at(q, a), at(q, b), at(p, b)],
            }
            .oriented(out),
        );
    }
    faces
}

// ----- stairs and railings --------------------------------------------------

fn stair_parts(doc: &Document, id: ElementId) -> Option<Vec<Part>> {
    let l = crate::stairs::stair_layout(doc, id)?;
    let rules = crate::stairs::StairRules::of(doc.type_of(id));
    let mut parts = Vec::new();
    let dir = |p: &crate::stairs::StairPiece| (p.outline[1] - p.outline[0]).normalize_or_zero();
    let mut i = 0;
    while i < l.pieces.len() {
        let p = &l.pieces[i];
        if p.landing {
            let z0 = p.z_top - rules.landing_thickness.max(1.0);
            parts.push(Part::prism(Material::Default, Prism::new(p.outline.clone(), z0, p.z_top)));
            i += 1;
            continue;
        }
        // A run: consecutive treads going the same way.
        let mut j = i + 1;
        while j < l.pieces.len() && !l.pieces[j].landing && dir(&l.pieces[j]).dot(dir(p)) > 0.999 {
            j += 1;
        }
        run_parts(&l, &rules, &l.pieces[i..j], l.pieces.get(j).is_none(), &mut parts);
        i = j;
    }
    Some(parts)
}

/// The parts of one straight run. Profiles are drawn in the run's
/// vertical plane (s along the run from the first nosing, z up) and swept
/// across its width.
fn run_parts(
    l: &crate::stairs::StairLayout,
    rules: &crate::stairs::StairRules,
    run: &[crate::stairs::StairPiece],
    last: bool,
    parts: &mut Vec<Part>,
) {
    let first = &run[0];
    let tread = first.outline[0].distance(first.outline[1]);
    if tread < EPS {
        return;
    }
    let r = (first.outline[1] - first.outline[0]) / tread;
    let w = DVec2::new(r.y, -r.x);
    let (mut fa, mut fb) = first.front;
    if (fb - fa).dot(w) < 0.0 {
        std::mem::swap(&mut fa, &mut fb);
    }
    let riser = l.riser;
    let slope = riser / tread;
    let z_start = first.z_top - riser;
    let s_end = tread * run.len() as f64;
    let z_last = run[run.len() - 1].z_top;
    let mut add = |profile: Vec<DVec2>, a: DVec2, b: DVec2| {
        if b.distance(a) > EPS && polygon_area(&profile).abs() > EPS {
            parts.push(Part {
                material_name: None,
                material: Material::Default,
                shape: Shape::Faces(sweep(&profile, DVec3::new(a.x, a.y, 0.0), DVec3::new(b.x, b.y, 0.0))),
            });
        }
    };
    let rect = |s0: f64, s1: f64, z0: f64, z1: f64| vec![DVec2::new(s0, z0), DVec2::new(s1, z0), DVec2::new(s1, z1), DVec2::new(s0, z1)];
    if rules.monolithic {
        // Stepped top over a waist slab of the structural depth, measured
        // square to the pitch, bearing on the floor or landing below.
        let waist = rules.structural_depth.max(1.0) * (1.0 + slope * slope).sqrt();
        let mut pts = vec![DVec2::new(0.0, z_start)];
        for (k, p) in run.iter().enumerate() {
            pts.push(DVec2::new(k as f64 * tread, p.z_top));
            pts.push(DVec2::new((k + 1) as f64 * tread, p.z_top));
        }
        if last {
            pts.push(DVec2::new(s_end, l.z1));
        }
        let soffit_end = z_start + slope * s_end - waist;
        if soffit_end > z_start + EPS {
            pts.push(DVec2::new(s_end, soffit_end));
            pts.push(DVec2::new(waist / slope, z_start));
        } else {
            pts.push(DVec2::new(s_end, z_start));
        }
        add(pts, fa, fb);
        return;
    }
    // Assembled: stringers on both sides carry the treads (with nosings)
    // and, when closed, risers.
    let sw = rules.stringer_width.min((fb - fa).length() / 4.0);
    let (ta, tb) = (fa + w * sw, fb - w * sw);
    let tt = rules.structural_depth.clamp(1.0, riser);
    for (k, p) in run.iter().enumerate() {
        let s0 = k as f64 * tread;
        add(rect(s0 - rules.nosing, s0 + tread, p.z_top - tt, p.z_top), ta, tb);
        if rules.closed_risers {
            let z0 = p.z_top - riser;
            add(rect(s0, s0 + rules.riser_thickness, z0, p.z_top - tt), ta, tb);
        }
    }
    if last && rules.closed_risers {
        add(rect(s_end, s_end + rules.riser_thickness, z_last, l.z1 - tt), ta, tb);
    }
    if sw > EPS && rules.stringer_depth > EPS {
        // Along the nosing line, standing 50 above it, from the floor below
        // to the floor above (or into the landing).
        let nose = |s: f64| first.z_top + slope * s;
        let (s0, s1) = (-tread, if last { s_end } else { s_end - tread * 0.5 });
        let above = 50.0f64.min(rules.stringer_depth / 2.0);
        let h = sw / 2.0;
        let profile = vec![
            DVec2::new(-h, above - rules.stringer_depth),
            DVec2::new(h, above - rules.stringer_depth),
            DVec2::new(h, above),
            DVec2::new(-h, above),
        ];
        for c in [fa + w * h, fb - w * h] {
            let (a, b) = (c + r * s0, c + r * s1);
            parts.push(Part {
                material_name: None,
                material: Material::Default,
                shape: Shape::Faces(sweep(&profile, DVec3::new(a.x, a.y, nose(s0)), DVec3::new(b.x, b.y, nose(s1)))),
            });
        }
    }
}

fn railing_parts(doc: &Document, id: ElementId, r: &Railing) -> Vec<Part> {
    use crate::stairs::{RailingBase, RailingRules};
    let rules = RailingRules::of(doc.type_of(id));
    let base = RailingBase::of(doc, r);
    let mut parts = Vec::new();
    // Sample the path so rails follow the stair's pitch through turns.
    let mut pts: Vec<DVec2> = Vec::new();
    for w in r.path.windows(2) {
        let n = (w[0].distance(w[1]) / 250.0).ceil().max(1.0) as usize;
        for k in 0..n {
            pts.push(w[0].lerp(w[1], k as f64 / n as f64));
        }
    }
    pts.extend(r.path.last());
    let rail = |size: f64, frac: f64, parts: &mut Vec<Part>| {
        let h = size / 2.0;
        let profile = [DVec2::new(-h, -h), DVec2::new(h, -h), DVec2::new(h, h), DVec2::new(-h, h)];
        for w in pts.windows(2) {
            let z = |p: DVec2| base.pitch(p) + rules.height * frac - h;
            let (a, b) = (DVec3::new(w[0].x, w[0].y, z(w[0])), DVec3::new(w[1].x, w[1].y, z(w[1])));
            if a.distance(b) > EPS {
                parts.push(Part {
                    material_name: None,
                    material: Material::Default,
                    shape: Shape::Faces(sweep(&profile, a, b)),
                });
            }
        }
    };
    rail(rules.rail, 1.0, &mut parts);
    for k in 1..=rules.mid_rails {
        rail(rules.rail * 0.6, k as f64 / (rules.mid_rails + 1) as f64, &mut parts);
    }
    // Balusters (posts at the ends), standing on the walking surface.
    let total: f64 = r.path.windows(2).map(|w| w[0].distance(w[1])).sum();
    let count = (total / rules.spacing).floor() as usize;
    let mut stations: Vec<f64> = (0..=count).map(|k| k as f64 * total / count.max(1) as f64).collect();
    stations.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    for s in stations {
        let Some(p) = point_along(&r.path, s) else { continue };
        let b = rules.baluster / 2.0;
        let sq = vec![
            p + DVec2::new(-b, -b),
            p + DVec2::new(b, -b),
            p + DVec2::new(b, b),
            p + DVec2::new(-b, b),
        ];
        let (z0, z1) = (base.surface(p), base.pitch(p) + rules.height - rules.rail);
        if z1 - z0 > EPS {
            parts.push(Part::prism(Material::Default, Prism::new(sq, z0, z1)));
        }
    }
    parts
}

/// The point `s` along a polyline.
pub fn point_along(path: &[DVec2], mut s: f64) -> Option<DVec2> {
    for w in path.windows(2) {
        let len = w[0].distance(w[1]);
        if s <= len + 1e-6 {
            return Some(if len > 0.0 {
                w[0].lerp(w[1], (s / len).clamp(0.0, 1.0))
            } else {
                w[0]
            });
        }
        s -= len;
    }
    path.last().copied()
}

/// Plan outline and height range of an isolated or wall foundation. Both
/// hang below their reference: a pad below its level (plus offset), a
/// strip below the bottom of its wall.
pub fn foundation_extent(doc: &Document, id: ElementId) -> Option<(Vec<DVec2>, f64, f64)> {
    let e = doc.element(id)?;
    let t = doc.type_of(id);
    let len = |k: &str, d: f64| t.map(|t| t.length_or(k, d)).unwrap_or(d);
    let thickness = len("Thickness", 500.0);
    match &e.data {
        ElementData::Footing(f) => {
            let (hw, hl) = (len("Width", 1200.0) / 2.0, len("Length", 1200.0) / 2.0);
            let (s, co) = f.rotation.to_radians().sin_cos();
            let (u, v) = (DVec2::new(co, s), DVec2::new(-s, co));
            let top = doc.level_elevation(f.level) + f.offset;
            let outline = vec![
                f.point - u * hw - v * hl,
                f.point + u * hw - v * hl,
                f.point + u * hw + v * hl,
                f.point - u * hw + v * hl,
            ];
            Some((outline, top - thickness, top))
        }
        ElementData::WallFooting(f) => {
            let w = doc.element(f.wall)?.as_wall()?;
            let hw = len("Width", 600.0) / 2.0;
            let dir = w.direction();
            if dir == DVec2::ZERO {
                return None;
            }
            // Strips run past the wall ends by half their width, so
            // footings meeting at a corner overlap into a full square.
            let n = perp(dir);
            let (a, b) = (w.start - dir * hw, w.end + dir * hw);
            let top = doc.level_elevation(w.base_level) + w.base_offset;
            Some((vec![a - n * hw, b - n * hw, b + n * hw, a + n * hw], top - thickness, top))
        }
        _ => None,
    }
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
    // Doors and windows from a loaded family; the built-in shapes below
    // are for types without one.
    if let Some(parts) = crate::component::hosted_parts(doc, id) {
        return parts;
    }
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

fn roof_skeleton(r: &Roof) -> Option<Skeleton> {
    if !r.is_sloped() || r.footprint.len() < 3 {
        return None;
    }
    let w: Vec<f64> = r.edge_slopes().iter().map(|s| if *s { 1.0 } else { 0.0 }).collect();
    straight_skeleton(&r.footprint, &w)
}

/// Lines drawn in plan for a roof: hips, valleys and ridges (eaves are
/// the outline).
pub fn roof_plan_lines(r: &Roof) -> Vec<(DVec2, DVec2)> {
    let Some(sk) = roof_skeleton(r) else { return vec![] };
    let n = r.footprint.len();
    let on_outline = |a: DVec2, b: DVec2| {
        (0..n).any(|i| {
            let (p, q) = (r.footprint[i], r.footprint[(i + 1) % n]);
            distance_to_segment(a, p, q) < 1.0 && distance_to_segment(b, p, q) < 1.0
        })
    };
    let mut out: Vec<(DVec2, DVec2)> = Vec::new();
    for (a, b, _, _) in &sk.arcs {
        let (a, b) = (a.truncate(), b.truncate());
        if a.distance(b) < 1.0 || on_outline(a, b) {
            continue;
        }
        let dup = out
            .iter()
            .any(|(p, q)| (p.distance(a) < 1.0 && q.distance(b) < 1.0) || (p.distance(b) < 1.0 && q.distance(a) < 1.0));
        if !dup {
            out.push((a, b));
        }
    }
    out
}

fn roof_parts(doc: &Document, id: ElementId, r: &Roof) -> Vec<Part> {
    if r.footprint.len() < 3 {
        return vec![];
    }
    let t = doc.type_of(id).map(|t| t.thickness()).unwrap_or(200.0);
    let zb = doc.level_elevation(r.level) + r.base_offset;
    let Some(sk) = roof_skeleton(r) else {
        return vec![Part::prism(Material::Default, Prism::new(r.footprint.clone(), zb, zb + t))];
    };
    let tan = r.slope.to_radians().tan();
    // Plumb rafter cut: the vertical thickness of a sloped slab.
    let hv = t / r.slope.to_radians().cos();
    let at = |p: DVec3, dz: f64| DVec3::new(p.x, p.y, zb + p.z * tan + dz);
    let slopes = r.edge_slopes();
    let n = r.footprint.len();
    let ccw = polygon_area(&r.footprint) > 0.0;
    let mut faces = Vec::new();
    for f in &sk.faces {
        let i = f.edge;
        let (a, b) = (r.footprint[i], r.footprint[(i + 1) % n]);
        let d = (b - a).normalize_or_zero();
        let out = if ccw {
            DVec3::new(d.y, -d.x, 0.0)
        } else {
            DVec3::new(-d.y, d.x, 0.0)
        };
        if slopes[i] {
            faces.push(
                Face {
                    pts: f.pts.iter().map(|p| at(*p, 0.0)).collect(),
                }
                .oriented(-DVec3::Z),
            );
            faces.push(
                Face {
                    pts: f.pts.iter().map(|p| at(*p, hv)).collect(),
                }
                .oriented(DVec3::Z),
            );
            let (a3, b3) = (f.pts[0], f.pts[1]);
            faces.push(
                Face {
                    pts: vec![at(a3, 0.0), at(b3, 0.0), at(b3, hv), at(a3, hv)],
                }
                .oriented(out),
            );
        } else {
            // Gable end: the band between the underside and the top,
            // following the roof profile from the edge's end back to its
            // start.
            let profile: Vec<DVec3> = f.pts[1..].iter().chain(std::iter::once(&f.pts[0])).copied().collect();
            let mut pts: Vec<DVec3> = profile.iter().map(|p| at(*p, 0.0)).collect();
            pts.extend(profile.iter().rev().map(|p| at(*p, hv)));
            faces.push(Face { pts }.oriented(out));
        }
    }
    vec![Part {
        material_name: None,
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
        // First wall goes (0,0)→(0,8000), 350 thick. Its exterior (left,
        // x<0) face should extend to the outer corners at y=-175 / 8175.
        assert!((o[0] - DVec2::new(-175.0, -175.0)).length() < 1e-6, "{o:?}");
        assert!((o[1] - DVec2::new(-175.0, 8175.0)).length() < 1e-6, "{o:?}");
        assert!((o[3] - DVec2::new(175.0, 175.0)).length() < 1e-6, "{o:?}");
    }

    #[test]
    fn openings_cut_walls() {
        let mut doc = sample_project();
        // South wall hosts a door and a window. Detached, it is a plain
        // prism from 425 below Level 1 up to Level 2.
        let south = doc.elements_of(Category::Walls).nth(3).unwrap().id;
        if let Some(ElementData::Wall(w)) = doc.element_mut(south).map(|e| &mut e.data) {
            w.attached_top.clear();
        }
        let g = element_geometry(&doc, south).unwrap();
        let w = doc.element(south).unwrap().as_wall().unwrap();
        let vol: f64 = g
            .parts
            .iter()
            .map(|p| if let Shape::Prism(p) = &p.shape { p.volume() } else { 0.0 })
            .sum();
        let gross = w.length() * 350.0 * 4425.0; // a square mitre keeps centreline length × thickness
        let holes = 926.0 * 2040.0 * 350.0 + 1200.0 * 1500.0 * 350.0;
        assert!((vol - (gross - holes)).abs() < 1.0, "vol {vol} expected {}", gross - holes);
    }

    #[test]
    fn hip_roof_faces() {
        let doc = sample_project();
        let roof = doc.elements_of(Category::Roofs).next().unwrap().id;
        let g = element_geometry(&doc, roof).unwrap();
        let faces = g.parts[0].faces();
        // Top, underside and fascia for each of the four sloped edges.
        assert_eq!(faces.len(), 4 + 4 + 4);
        assert_eq!(faces.iter().filter(|f| f.normal().z > 0.3).count(), 4);
        assert_eq!(faces.iter().filter(|f| f.normal().z < -0.3).count(), 4);
        assert_eq!(faces.iter().filter(|f| f.normal().z.abs() < 1e-9).count(), 4);
    }

    /// Walls attached to a gable roof fill the triangle under it.
    #[test]
    fn walls_fill_gable() {
        for overhang in [750.0, 150.0, 0.0] {
            let mut doc = sample_project();
            let roof = doc.elements_of(Category::Roofs).next().unwrap().id;
            let walls: Vec<ElementId> = doc.elements_of(Category::Walls).map(|e| e.id).take(4).collect();
            if let Some(ElementData::Roof(r)) = doc.element_mut(roof).map(|e| &mut e.data) {
                r.defines_slope = vec![true, false, true, false];
                let (a, b) = (-overhang, overhang);
                r.footprint = vec![
                    DVec2::new(a, a),
                    DVec2::new(12000.0 + b, a),
                    DVec2::new(12000.0 + b, 8000.0 + b),
                    DVec2::new(a, 8000.0 + b),
                ];
            }
            for w in &walls {
                if let Some(ElementData::Wall(w)) = doc.element_mut(*w).map(|e| &mut e.data) {
                    w.attached_top = vec![roof];
                }
            }
            let roof_z = doc.elements_of(Category::Levels).map(|e| e.id).collect::<Vec<_>>();
            let base = doc.level_elevation(roof_z[1]);
            let top = |id: ElementId| {
                element_geometry(&doc, id)
                    .unwrap()
                    .parts
                    .iter()
                    .flat_map(|p| p.faces())
                    .flat_map(|f| f.pts)
                    .map(|p| p.z)
                    .fold(f64::NEG_INFINITY, f64::max)
            };
            // West wall (x = 0) is a gable end: it rises close to the ridge.
            let ridge = base + (4000.0 + overhang) * 30f64.to_radians().tan();
            let west = top(walls[0]);
            assert!(west > ridge - 600.0, "overhang {overhang}: west wall top {west}, ridge {ridge}");
        }
    }

    #[test]
    fn framing_and_foundations() {
        use crate::template::industrial_sample;
        let doc = industrial_sample();
        // A beam's volume is its section area times its length, and it
        // hangs below its reference line.
        let beam = doc
            .elements_of(Category::StructuralFraming)
            .find(|e| matches!(&e.data, ElementData::Framing(f) if f.kind == FramingKind::Beam && f.start_offset == f.end_offset))
            .unwrap();
        let ElementData::Framing(f) = &beam.data else { unreachable!() };
        let g = element_geometry(&doc, beam.id).unwrap();
        let faces = g.parts[0].faces();
        let area = polygon_area(&section_profile(doc.type_of(beam.id))).abs();
        assert_eq!(faces.len(), 2 + 12);
        let top = faces
            .iter()
            .flat_map(|f| f.pts.iter().map(|p| p.z))
            .fold(f64::NEG_INFINITY, f64::max);
        assert!((top - 6000.0).abs() < 1e-6, "beam top {top}");
        // Divergence theorem: volume from the closed faces.
        let vol: f64 = faces
            .iter()
            .map(|f| {
                let o = f.pts[0];
                (1..f.pts.len() - 1)
                    .map(|i| o.dot((f.pts[i] - o).cross(f.pts[i + 1] - o)) / 6.0)
                    .sum::<f64>()
            })
            .sum();
        assert!(
            (vol - area * f.length()).abs() / vol < 1e-9,
            "vol {vol}, expected {}",
            area * f.length()
        );
        // Pads sit under columns with their top 250 below Level 1.
        let pads: Vec<_> = doc.elements_of(Category::StructuralFoundations).collect();
        assert_eq!(pads.len(), 12);
        let (outline, z0, z1) = foundation_extent(&doc, pads[0].id).unwrap();
        assert_eq!((z0, z1), (-850.0, -250.0));
        assert!((polygon_area(&outline).abs() - 1800.0 * 1800.0).abs() < 1e-6);
    }

    #[test]
    fn wall_footings_follow_walls() {
        let doc = crate::template::sample_project();
        let footing = doc
            .elements_of(Category::StructuralFoundations)
            .find(|e| matches!(e.data, ElementData::WallFooting(_)))
            .unwrap();
        let ElementData::WallFooting(f) = &footing.data else {
            unreachable!()
        };
        let w = doc.element(f.wall).unwrap().as_wall().unwrap();
        let (outline, z0, z1) = foundation_extent(&doc, footing.id).unwrap();
        // 600 wide, 300 deep, under the wall's base 425 below Level 1, and
        // half its width past each end of the wall.
        assert_eq!((z0, z1), (-725.0, -425.0));
        assert!((polygon_area(&outline).abs() - 600.0 * (w.length() + 600.0)).abs() < 1e-6);
    }

    #[test]
    fn stair_and_railings() {
        let doc = crate::template::industrial_sample();
        let stair = doc.elements_of(Category::Stairs).next().unwrap().id;
        let g = element_geometry(&doc, stair).unwrap();
        // 3000 mm at 190 max: 16 risers, so 15 treads (open risers) and
        // two stringers; the last tread is at the mezzanine less one riser.
        assert_eq!(g.parts.len(), 17);
        let tops: Vec<f64> = g
            .parts
            .iter()
            .map(|p| p.faces().iter().flat_map(|f| f.pts.iter().map(|q| q.z)).fold(f64::MIN, f64::max))
            .collect();
        assert!((tops[14] - 3000.0 * 15.0 / 16.0).abs() < 1e-6, "{tops:?}");
        // The stringers reach the mezzanine, 50 above the nosing line.
        assert!((tops[15] - 3050.0).abs() < 30.0, "{tops:?}");
        // Its railings rise with it: the top rail is 1100 above the pitch
        // line at both ends.
        let rail = doc
            .elements_of(Category::Railings)
            .find(|e| matches!(&e.data, ElementData::Railing(r) if r.host == stair))
            .unwrap();
        let g = element_geometry(&doc, rail.id).unwrap();
        let zmax = g
            .parts
            .iter()
            .flat_map(|p| p.faces())
            .flat_map(|f| f.pts)
            .map(|p| p.z)
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(zmax > 3000.0 + 1000.0 && zmax < 3000.0 + 1200.0, "top rail reaches {zmax}");
    }
}
