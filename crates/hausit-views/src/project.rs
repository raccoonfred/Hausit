//! Orthographic projection of model solids for elevations, sections and 3D
//! views. Faces are ordered exactly with a BSP tree and painted far to near,
//! so filled faces hide what is behind them (hidden line / shaded).

use crate::bsp::{Bsp, Poly};
use crate::frame::ViewFrame;
use crate::hatch::{clip_segment, draw_band, layer_bands, parallel_lines, structural_material, Band};
use crate::{DisplayList, Prim, Rgba, Stroke, BLACK, CUT_FILL};
use hausit_core::geom::{clip_polygon_2d, distance_to_segment, perp, polygon_area};
use hausit_core::{DVec2, DVec3, ElementId, Plane};
use hausit_model::geometry::{model_geometry, Material, Shape};
use hausit_model::materials::{MaterialDef, SurfacePattern};
use hausit_model::{Category, Document, ElementData, View, ViewKind, VisualStyle};

/// Base colour of each category in shaded views.
pub fn category_color(cat: Category, material: Material) -> Rgba {
    match material {
        Material::Glass => return [150, 196, 214, 140],
        Material::Panel => return [186, 152, 112, 255],
        Material::Default => {}
    }
    match cat {
        // Revit's default materials shade to light neutral greys.
        Category::Walls => [236, 236, 234, 255],
        Category::Floors => [214, 214, 212, 255],
        Category::Roofs => [196, 192, 186, 255],
        Category::Doors => [205, 205, 205, 255],
        Category::Windows => [235, 235, 235, 255],
        _ => [200, 200, 200, 255],
    }
}

#[derive(Clone)]
struct FaceData {
    fill: Option<Rgba>,
    id: ElementId,
    /// The face's model pattern (brick courses…), drawn over its fill.
    pattern: SurfacePattern,
}

/// The material showing on a face: for layered walls, floors and roofs,
/// the outer layer on each side and the structure on the edges; otherwise
/// the type's material.
fn face_material(doc: &Document, id: ElementId, n: DVec3) -> Option<&MaterialDef> {
    let t = doc.type_of(id)?;
    if !t.layers.is_empty() {
        let side = match &doc.element(id)?.data {
            ElementData::Wall(w) => DVec2::new(n.x, n.y).dot(perp(w.direction())),
            ElementData::Floor(_) | ElementData::Roof(_) | ElementData::Ceiling(_) => n.z,
            _ => 0.0,
        };
        let roof = matches!(doc.element(id)?.data, ElementData::Roof(_));
        // Roof edges (fascias) show the roof covering.
        let layer = if side > 0.3 || (roof && side > -0.3 && n != DVec3::ZERO) {
            t.layers.first()
        } else if side < -0.3 {
            t.layers.last()
        } else {
            t.layers
                .iter()
                .find(|l| l.function == hausit_model::materials::LayerFunction::Structure)
                .or(t.layers.first())
        };
        return doc.material(&layer?.material);
    }
    ["Material", "Structural Material"].iter().find_map(|k| match t.params.get(*k) {
        Some(hausit_model::ParamValue::Text(name)) => doc.material(name),
        _ => None,
    })
}

/// Model pattern lines of a face, in model coordinates. Anchored to the
/// model's axes so courses line up from face to face.
fn surface_lines(pts: &[DVec3], pattern: SurfacePattern) -> Vec<(DVec3, DVec3)> {
    let (course, joint, stagger) = pattern.layout();
    if pts.len() < 3 || (course <= 0.0 && joint <= 0.0) {
        return vec![];
    }
    let n = hausit_core::geom::polygon_normal(pts).normalize_or_zero();
    let u = DVec3::Z.cross(n).try_normalize().unwrap_or(DVec3::X);
    let mut v = n.cross(u);
    if v.z < 0.0 || (v.z.abs() < 1e-9 && v.y < 0.0) {
        v = -v;
    }
    let d = n.dot(pts[0]);
    let poly: Vec<DVec2> = pts.iter().map(|p| DVec2::new(p.dot(u), p.dot(v))).collect();
    let lift = |q: DVec2| u * q.x + v * q.y + n * d;
    let (lo, hi) = poly
        .iter()
        .fold((DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)), |(a, b), p| {
            (a.min(*p), b.max(*p))
        });
    let size = hi - lo;
    let mut out2 = Vec::new();
    if course > 0.0 {
        if size.y / course > 2000.0 {
            return vec![];
        }
        out2.extend(parallel_lines(&poly, DVec2::X, course));
        if joint > 0.0 && size.x / joint * size.y / course < 20_000.0 {
            let mut k = (lo.y / course).floor();
            while k * course < hi.y {
                let shift = if stagger && (k as i64).rem_euclid(2) == 1 {
                    joint / 2.0
                } else {
                    0.0
                };
                let mut j = ((lo.x - shift) / joint).floor();
                while j * joint + shift < hi.x {
                    let x = j * joint + shift;
                    out2.extend(clip_segment(&poly, DVec2::new(x, k * course), DVec2::new(x, (k + 1.0) * course)));
                    j += 1.0;
                }
                k += 1.0;
            }
        }
    } else if size.x / joint < 2000.0 {
        out2.extend(parallel_lines(&poly, DVec2::Y, joint));
    }
    out2.into_iter().map(|(a, b)| (lift(a), lift(b))).collect()
}

pub fn project(doc: &Document, view: &View, frame: &ViewFrame) -> DisplayList {
    let style = view.visual_style;
    let light = (frame.up * 0.5 - frame.forward * 0.6 - frame.right * 0.45).normalize();
    let near = frame
        .depth_clip
        .map(|(near, _)| Plane::from_point_normal(frame.origin + frame.forward * near, frame.forward));
    let far = frame
        .depth_clip
        .map(|(_, far)| Plane::from_point_normal(frame.origin + frame.forward * far, -frame.forward));
    let u_planes = frame.u_clip.map(|(u0, u1)| {
        (
            Plane::from_point_normal(frame.origin + frame.right * u0, frame.right),
            Plane::from_point_normal(frame.origin + frame.right * u1, -frame.right),
        )
    });

    // A 3D view's section box: keep the inside of all six faces.
    let section_box = match view.kind {
        ViewKind::ThreeD => view.section_box,
        _ => None,
    };
    let box_planes: Vec<Plane> = section_box
        .map(|b| b.planes().iter().map(|(p, n)| Plane::from_point_normal(*p, -*n)).collect())
        .unwrap_or_default();

    let geometry = model_geometry(doc);
    let mut polys = Vec::new();
    for g in &geometry {
        let mut faces: Vec<(Material, hausit_model::geometry::Face)> = Vec::new();
        let mut names: Vec<Option<&str>> = Vec::new();
        for p in &g.parts {
            for f in p.faces() {
                faces.push((p.material, f));
                names.push(p.material_name.as_deref());
            }
        }
        let inside = internal_faces(&faces);
        for (fi, (material, face)) in faces.iter().enumerate() {
            if inside[fi] {
                continue;
            }
            let part = Part {
                material: *material,
                material_name: names[fi],
            };
            {
                let n = face.normal();
                let facing = n.dot(frame.forward);
                if style != VisualStyle::Wireframe && facing > 1e-9 {
                    continue; // back face
                }
                // Edge-on faces contribute outlines only (e.g. roof slopes
                // seen end-on in a section).
                let edge_on = facing.abs() <= 1e-9;
                let mut pts: Vec<DVec3> = face.pts.clone();
                let mut edges = vec![true; pts.len()];
                for plane in [near, far]
                    .into_iter()
                    .flatten()
                    .chain(u_planes.into_iter().flat_map(|(a, b)| [a, b]))
                    .chain(box_planes.iter().copied())
                {
                    (pts, edges) = clip_with_edges(&pts, &edges, &plane);
                }
                if pts.len() < 3 {
                    continue;
                }
                let material = match part.material_name {
                    Some(name) => doc.material(name),
                    None => (part.material == Material::Default).then(|| face_material(doc, g.id, n)).flatten(),
                };
                let base = match material {
                    Some(m) => [m.color[0], m.color[1], m.color[2], 255],
                    None => category_color(g.category, part.material),
                };
                let pattern = match style {
                    VisualStyle::Wireframe => SurfacePattern::None,
                    _ if edge_on => SurfacePattern::None,
                    _ => material.map(|m| m.surface_pattern).unwrap_or_default(),
                };
                let fill = match style {
                    _ if edge_on => None,
                    VisualStyle::Wireframe => None,
                    VisualStyle::HiddenLine => Some(if part.material == Material::Glass {
                        [225, 238, 248, 255]
                    } else {
                        [255, 255, 255, 255]
                    }),
                    VisualStyle::ConsistentColors => Some(base),
                    VisualStyle::Shaded => {
                        let k = 0.74 + 0.26 * n.dot(light).max(0.0);
                        Some([
                            (base[0] as f64 * k) as u8,
                            (base[1] as f64 * k) as u8,
                            (base[2] as f64 * k) as u8,
                            base[3],
                        ])
                    }
                };
                polys.push(Poly {
                    pts,
                    edges,
                    data: FaceData { fill, id: g.id, pattern },
                });
            }
        }
    }

    if let Some(b) = section_box {
        box_caps(doc, &geometry, &b, &box_planes, frame, style, &mut polys);
    }

    split_t_junctions(&mut polys);
    hide_coplanar_seams(&mut polys);

    let edge = Stroke::new(1.0, if style == VisualStyle::Shaded { [40, 40, 40, 255] } else { BLACK });
    let pattern_pen = Stroke::new(
        0.6,
        if style == VisualStyle::HiddenLine {
            [150, 150, 150, 255]
        } else {
            [70, 70, 70, 255]
        },
    );
    let mut dl = DisplayList::default();
    let bsp = Bsp::new(polys);
    for p in bsp.back_to_front(frame.forward) {
        let pts: Vec<DVec2> = p.pts.iter().map(|q| frame.to_view(*q).0).collect();
        if polygon_area(&pts).abs() < 1e-3 && p.data.fill.is_some() {
            continue;
        }
        if p.data.fill.is_some() {
            dl.push(Prim::Polygon(pts.clone()), None, p.data.fill, Some(p.data.id));
            for (a, b) in surface_lines(&p.pts, p.data.pattern) {
                dl.line(frame.to_view(a).0, frame.to_view(b).0, pattern_pen, None);
            }
        }
        let n = pts.len();
        for i in 0..n {
            if p.edges[i] {
                dl.line(pts[i], pts[(i + 1) % n], edge, Some(p.data.id));
            }
        }
    }

    // Section: cut caps where the section plane slices prisms.
    if frame.depth_clip.is_some() {
        section_caps(doc, view, &geometry, frame, &mut dl);
    }
    dl
}

/// Cut faces where a section box slices solids, filled like section cuts.
fn box_caps(
    doc: &Document,
    geometry: &[hausit_model::geometry::ElementGeometry],
    b: &hausit_model::SectionBox,
    keep: &[Plane],
    frame: &ViewFrame,
    style: VisualStyle,
    polys: &mut Vec<Poly<FaceData>>,
) {
    for (i, (p0, n)) in b.planes().iter().enumerate() {
        if n.dot(frame.forward) > -1e-9 {
            continue; // faces away from the camera
        }
        let plane = Plane::from_point_normal(*p0, *n);
        let e1 = n.cross(DVec3::Z).try_normalize().unwrap_or(DVec3::X);
        let e2 = n.cross(e1);
        let to2 = |p: DVec3| DVec2::new((p - *p0).dot(e1), (p - *p0).dot(e2));
        let to3 = |q: DVec2| *p0 + e1 * q.x + e2 * q.y;
        for g in geometry {
            let mut segs = Vec::new();
            for part in &g.parts {
                for face in part.faces() {
                    let m = face.pts.len();
                    let mut hits = Vec::new();
                    for k in 0..m {
                        let (a, c) = (face.pts[k], face.pts[(k + 1) % m]);
                        let (da, dc) = (plane.signed_distance(a), plane.signed_distance(c));
                        if (da < 0.0) != (dc < 0.0) {
                            hits.push(to2(a + (c - a) * (da / (da - dc))));
                        }
                    }
                    if hits.len() == 2 {
                        segs.push((hits[0], hits[1]));
                    }
                }
            }
            for l in chain_loops(segs) {
                let mut pts: Vec<DVec3> = l.into_iter().map(to3).collect();
                let mut edges = vec![true; pts.len()];
                for (j, k) in keep.iter().enumerate() {
                    if j != i {
                        (pts, edges) = clip_with_edges(&pts, &edges, k);
                    }
                }
                if pts.len() < 3 {
                    continue;
                }
                // Face the camera, as the caps' outward normals do.
                if hausit_core::geom::polygon_normal(&pts).dot(*n) < 0.0 {
                    pts.reverse();
                    edges.reverse();
                    edges.rotate_left(1);
                }
                let fill = match style {
                    VisualStyle::Wireframe => None,
                    VisualStyle::HiddenLine => Some(CUT_FILL),
                    _ => {
                        let c = face_material(doc, g.id, DVec3::ZERO)
                            .map(|m| [m.color[0], m.color[1], m.color[2], 255])
                            .unwrap_or(category_color(g.category, Material::Default));
                        Some([
                            (c[0] as f64 * 0.62) as u8,
                            (c[1] as f64 * 0.62) as u8,
                            (c[2] as f64 * 0.62) as u8,
                            255,
                        ])
                    }
                };
                polys.push(Poly {
                    pts,
                    edges,
                    data: FaceData {
                        fill,
                        id: g.id,
                        pattern: SurfacePattern::None,
                    },
                });
            }
        }
    }
}

/// Stand-in for the geometry part a face came from.
struct Part<'a> {
    material: Material,
    material_name: Option<&'a str>,
}

/// Faces inside an element's solid: where two pieces of one element touch
/// (wall pieces either side of a window edge, the straight wall under an
/// attached top), each piece has a face against the other. Those faces
/// are hidden by the solid in reality, but drawing them leaves seams.
fn internal_faces(faces: &[(Material, hausit_model::geometry::Face)]) -> Vec<bool> {
    let data: Vec<(DVec3, DVec3)> = faces
        .iter()
        .map(|(_, f)| {
            let c = f.pts.iter().copied().sum::<DVec3>() / f.pts.len().max(1) as f64;
            (f.normal(), c)
        })
        .collect();
    let to2d = |n: DVec3, pts: &[DVec3]| -> Vec<DVec2> {
        let u = if n.z.abs() < 0.9 { n.cross(DVec3::Z) } else { n.cross(DVec3::X) }.normalize();
        let v = n.cross(u);
        pts.iter().map(|p| DVec2::new(p.dot(u), p.dot(v))).collect()
    };
    (0..faces.len())
        .map(|i| {
            let (n, c) = data[i];
            n != DVec3::ZERO
                && (0..faces.len()).any(|j| {
                    let (m, d) = data[j];
                    if j == i || n.dot(m) > -0.9999 || n.dot(d - c).abs() > 0.1 {
                        return false;
                    }
                    let g = to2d(n, &faces[j].1.pts);
                    hausit_core::geom::point_in_polygon(to2d(n, &[c])[0], &g)
                })
        })
        .collect()
}

/// Adds the corners of coplanar neighbours that lie along a face's edges
/// (T-junctions, e.g. a full-height wall piece beside the pieces under
/// and over a window), so partly shared edges become exactly shared and
/// their seams can be hidden.
fn split_t_junctions(polys: &mut [Poly<FaceData>]) {
    use std::collections::HashMap;
    // Per element and plane: seams between pieces of one element.
    let mut groups: HashMap<(ElementId, i64, i64, i64, i64), Vec<usize>> = HashMap::new();
    for (i, p) in polys.iter().enumerate() {
        let n = hausit_core::geom::polygon_normal(&p.pts).normalize_or_zero();
        if n == DVec3::ZERO {
            continue;
        }
        let key = (
            p.data.id,
            (n.x * 1000.0).round() as i64,
            (n.y * 1000.0).round() as i64,
            (n.z * 1000.0).round() as i64,
            n.dot(p.pts[0]).round() as i64,
        );
        groups.entry(key).or_default().push(i);
    }
    for ids in groups.values().filter(|g| g.len() > 1) {
        let verts: Vec<DVec3> = ids.iter().flat_map(|i| polys[*i].pts.iter().copied()).collect();
        for &i in ids {
            let p = &polys[i];
            let n = p.pts.len();
            let (mut pts, mut edges) = (Vec::with_capacity(n), Vec::with_capacity(n));
            for k in 0..n {
                let (a, b) = (p.pts[k], p.pts[(k + 1) % n]);
                pts.push(a);
                edges.push(p.edges[k]);
                let d = b - a;
                let len2 = d.length_squared();
                if len2 < 1e-6 {
                    continue;
                }
                let mut inner: Vec<(f64, DVec3)> = verts
                    .iter()
                    .filter_map(|v| {
                        let t = (*v - a).dot(d) / len2;
                        let off = (a + d * t).distance(*v);
                        (t > 1e-6 && t < 1.0 - 1e-6 && off < 0.05 && v.distance(a) > 0.05 && v.distance(b) > 0.05).then_some((t, *v))
                    })
                    .collect();
                inner.sort_by(|x, y| x.0.total_cmp(&y.0));
                inner.dedup_by(|x, y| x.1.distance(y.1) < 0.05);
                for (_, v) in inner {
                    pts.push(v);
                    edges.push(p.edges[k]);
                }
            }
            polys[i].pts = pts;
            polys[i].edges = edges;
        }
    }
}

/// Hides edges shared by two coplanar faces (e.g. wall pieces either side
/// of a window, or the mitre on top of a wall corner), so joined geometry
/// reads as one surface, as in Revit. Triangles of a mesh that bend by
/// less than about 10° count as one smooth surface.
fn hide_coplanar_seams(polys: &mut [Poly<FaceData>]) {
    use std::collections::HashMap;
    let q = |p: DVec3| {
        (
            (p.x * 10.0).round() as i64,
            (p.y * 10.0).round() as i64,
            (p.z * 10.0).round() as i64,
        )
    };
    let normals: Vec<DVec3> = polys
        .iter()
        .map(|p| hausit_core::geom::polygon_normal(&p.pts).normalize_or_zero())
        .collect();
    type Key = (i64, i64, i64);
    let mut map: HashMap<(Key, Key), Vec<(usize, usize)>> = HashMap::new();
    for (pi, p) in polys.iter().enumerate() {
        let n = p.pts.len();
        for i in 0..n {
            let (a, b) = (q(p.pts[i]), q(p.pts[(i + 1) % n]));
            let key = if a < b { (a, b) } else { (b, a) };
            map.entry(key).or_default().push((pi, i));
        }
    }
    for uses in map.values() {
        for (k, &(pa, ea)) in uses.iter().enumerate() {
            for &(pb, eb) in &uses[k + 1..] {
                // Imported meshes approximate curved surfaces with
                // triangles: hide the seams between gently bending ones too.
                let mesh = polys[pa].pts.len() == 3 || polys[pb].pts.len() == 3;
                let limit = if mesh { 0.985 } else { 0.9999 };
                if pa != pb && normals[pa].dot(normals[pb]) > limit {
                    polys[pa].edges[ea] = false;
                    polys[pb].edges[eb] = false;
                }
            }
        }
    }
}

/// Clips a polygon to the positive side of `plane`, keeping edge flags.
/// Edges created along the plane are not original.
fn clip_with_edges(pts: &[DVec3], edges: &[bool], plane: &Plane) -> (Vec<DVec3>, Vec<bool>) {
    let n = pts.len();
    let (mut out, mut flags) = (Vec::new(), Vec::new());
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let (da, db) = (plane.signed_distance(a), plane.signed_distance(b));
        if da >= 0.0 {
            out.push(a);
            flags.push(edges[i]);
        }
        if (da >= 0.0) != (db >= 0.0) {
            let x = a + (b - a) * (da / (da - db));
            out.push(x);
            // Entering the kept side: x→b is part of the original edge.
            // Leaving it: x→(next kept vertex) runs along the clip plane.
            flags.push(da < 0.0 && edges[i]);
        }
    }
    (out, flags)
}

/// Draws the cut faces of prisms intersected by the section plane (heavy
/// outline, solid fill), and cut lines through sloped faces.
fn section_caps(
    doc: &Document,
    view: &View,
    geometry: &[hausit_model::geometry::ElementGeometry],
    frame: &ViewFrame,
    dl: &mut DisplayList,
) {
    let o = DVec2::new(frame.origin.x, frame.origin.y);
    let f2 = DVec2::new(frame.forward.x, frame.forward.y);
    let r2 = DVec2::new(frame.right.x, frame.right.y);
    let (u0, u1) = frame.u_clip.unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
    for g in geometry {
        for part in &g.parts {
            match &part.shape {
                Shape::Prism(p) => {
                    // Intersections of the footprint with the cut line.
                    let n = p.footprint.len();
                    let mut us = Vec::new();
                    for i in 0..n {
                        let (a, b) = (p.footprint[i], p.footprint[(i + 1) % n]);
                        let (da, db) = ((a - o).dot(f2), (b - o).dot(f2));
                        if (da < 0.0) != (db < 0.0) {
                            let t = da / (da - db);
                            us.push((a + (b - a) * t - o).dot(r2));
                        }
                    }
                    us.sort_by(f64::total_cmp);
                    for pair in us.chunks(2) {
                        if pair.len() < 2 {
                            continue;
                        }
                        let (a, b) = (pair[0].max(u0), pair[1].min(u1));
                        if b - a < 1e-6 {
                            continue;
                        }
                        let fill = if part.material == Material::Glass {
                            [150, 200, 235, 255]
                        } else {
                            CUT_FILL
                        };
                        let rect = vec![DVec2::new(a, p.z0), DVec2::new(b, p.z0), DVec2::new(b, p.z1), DVec2::new(a, p.z1)];
                        let rect = clip_polygon_2d(&rect, DVec2::X, u1);
                        let edge = crate::pen(doc, g.category, true, view);
                        let (mut bands, lines) = if part.material == Material::Glass {
                            (vec![], vec![])
                        } else {
                            layer_polys(doc, view, g, &rect, o, r2)
                        };
                        if bands.is_empty() {
                            bands.extend(material_band(doc, view, g.id, &rect));
                        }
                        if bands.is_empty() {
                            dl.push(Prim::Polygon(rect), Some(edge), Some(fill), Some(g.id));
                            continue;
                        }
                        draw_layers(dl, doc, view, g.id, rect, &bands, &lines, edge);
                    }
                }
                Shape::Faces(faces) => {
                    let plane = Plane::from_point_normal(frame.origin, frame.forward);
                    let mut segs = Vec::new();
                    // Cuts through upward-facing faces, with their normal
                    // in view coordinates: the top surface of a roof.
                    let mut tops = Vec::new();
                    for face in faces {
                        let mut hits = Vec::new();
                        let m = face.pts.len();
                        for i in 0..m {
                            let (a, b) = (face.pts[i], face.pts[(i + 1) % m]);
                            let (da, db) = (plane.signed_distance(a), plane.signed_distance(b));
                            if (da < 0.0) != (db < 0.0) {
                                let p: DVec3 = a + (b - a) * (da / (da - db));
                                hits.push(frame.to_view(p).0);
                            }
                        }
                        if hits.len() == 2 && hits.iter().all(|h| h.x >= u0 - 1e-6 && h.x <= u1 + 1e-6) {
                            segs.push((hits[0], hits[1]));
                            let n = face.normal();
                            let n2 = DVec2::new(n.dot(frame.right), n.dot(frame.up));
                            if n.z > 0.05 && n2.length() > 1e-6 {
                                tops.push((hits[0], hits[1], n2.normalize()));
                            }
                        }
                    }
                    // Every closed cut is filled; layered types get their layers.
                    let edge = crate::pen(doc, g.category, true, view);
                    let loops = chain_loops(segs.clone());
                    if loops.is_empty() {
                        for (a, b) in segs {
                            dl.line(a, b, edge, Some(g.id));
                        }
                    }
                    for l in loops {
                        let (mut bands, mut lines) = layer_polys(doc, view, g, &l, o, r2);
                        if bands.is_empty() {
                            (bands, lines) = roof_layers(doc, view, g.id, &l, &tops);
                        }
                        if bands.is_empty() {
                            bands.extend(material_band(doc, view, g.id, &l));
                        }
                        if bands.is_empty() {
                            dl.push(Prim::Polygon(l), Some(edge), Some(CUT_FILL), Some(g.id));
                        } else {
                            draw_layers(dl, doc, view, g.id, l, &bands, &lines, edge);
                        }
                    }
                }
            }
        }
    }
}

/// A sloped roof cut in section: its layers follow the top surface. The
/// cut is split where roof planes meet (on the bisector, as at a ridge),
/// and each piece is layered down from its own top line.
fn roof_layers<'a>(
    doc: &'a Document,
    view: &View,
    id: ElementId,
    outline: &[DVec2],
    tops: &[(DVec2, DVec2, DVec2)],
) -> (Vec<Band<'a>>, Vec<(DVec2, DVec2)>) {
    let none = (vec![], vec![]);
    if view.detail_level == hausit_model::DetailLevel::Coarse {
        return none;
    }
    let Some(t) = doc.type_of(id).filter(|t| !t.layers.is_empty()) else {
        return none;
    };
    if !matches!(doc.element(id).map(|e| &e.data), Some(ElementData::Roof(_))) {
        return none;
    }
    let on_outline = |p: DVec2| (0..outline.len()).any(|i| distance_to_segment(p, outline[i], outline[(i + 1) % outline.len()]) < 1.0);
    let tops: Vec<_> = tops.iter().filter(|(a, b, _)| on_outline((*a + *b) * 0.5)).collect();
    let (mut bands, mut lines) = (Vec::new(), Vec::new());
    for (i, (a, b, n)) in tops.iter().enumerate() {
        let mut piece = outline.to_vec();
        let mid = (*a + *b) * 0.5;
        for (j, (c, d, m)) in tops.iter().enumerate() {
            if i == j || (*n - *m).length() < 1e-6 {
                continue;
            }
            let Some(v) = [*a, *b].into_iter().find(|p| p.distance(*c) < 1.0 || p.distance(*d) < 1.0) else {
                continue;
            };
            // Keep the side of the bisector at `v` that holds this plane.
            let dir = -(*n + *m);
            let k = perp(dir.try_normalize().unwrap_or(DVec2::Y));
            let k = if k.dot(mid - v) > 0.0 { -k } else { k };
            piece = clip_polygon_2d(&piece, k, k.dot(v));
        }
        if piece.len() < 3 {
            continue;
        }
        let along = (*b - *a).try_normalize().unwrap_or(DVec2::X);
        let layers = t.layers.iter().map(|l| (l.material.as_str(), l.thickness));
        let (bs, ls) = layer_bands(&piece, *a, along, *n, 0.0, layers);
        bands.extend(bs);
        lines.extend(ls);
    }
    (bands, lines)
}

/// A column, beam or foundation cut in section: one band of its type's
/// structural material (Medium and Fine detail).
fn material_band<'a>(doc: &'a Document, view: &View, id: ElementId, poly: &[DVec2]) -> Option<Band<'a>> {
    if view.detail_level == hausit_model::DetailLevel::Coarse {
        return None;
    }
    Some(Band {
        outline: poly.to_vec(),
        along: DVec2::X,
        material: structural_material(doc, id)?,
    })
}

/// A layered wall or floor cut in section: `poly` (view coordinates) split
/// into its layers, and the thin lines between them. Empty when the
/// element's type has no layers, the view is Coarse, or the cut runs along
/// a wall.
fn layer_polys<'a>(
    doc: &'a Document,
    view: &View,
    g: &hausit_model::geometry::ElementGeometry,
    poly: &[DVec2],
    o: DVec2,
    r2: DVec2,
) -> (Vec<Band<'a>>, Vec<(DVec2, DVec2)>) {
    let none = (vec![], vec![]);
    if view.detail_level == hausit_model::DetailLevel::Coarse {
        return none;
    }
    let Some(t) = doc.type_of(g.id).filter(|t| !t.layers.is_empty()) else {
        return none;
    };
    let total: f64 = t.layers.iter().map(|l| l.thickness).sum();
    let layers = t.layers.iter().map(|l| (l.material.as_str(), l.thickness));
    match doc.element(g.id).map(|e| &e.data) {
        Some(ElementData::Wall(w)) => {
            // Distance s from the wall line towards the exterior is linear
            // along the cut: s(u) = s0 + k·u, so layers are vertical bands.
            let nrm = perp(w.direction());
            let k = r2.dot(nrm);
            if k.abs() < 1e-6 {
                return none;
            }
            let s0 = (o - w.start).dot(nrm);
            // In u, the exterior face is at (total/2 - s0)/k, running
            // towards decreasing s.
            let out = DVec2::new(k.signum(), 0.0);
            let origin = DVec2::new(-s0 / k, 0.0);
            let half = total / 2.0 / k.abs();
            let scaled: Vec<(&str, f64)> = layers.map(|(m, d)| (m, d / k.abs())).collect();
            layer_bands(poly, origin, DVec2::Y, out, half, scaled)
        }
        Some(ElementData::Floor(_) | ElementData::Ceiling(_)) => {
            let top = poly.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
            layer_bands(poly, DVec2::new(0.0, top), DVec2::X, DVec2::Y, 0.0, layers)
        }
        _ => none,
    }
}

/// Joins cut segments end to end into closed loops.
fn chain_loops(mut segs: Vec<(DVec2, DVec2)>) -> Vec<Vec<DVec2>> {
    let mut loops = Vec::new();
    while let Some((a, b)) = segs.pop() {
        let mut pts = vec![a, b];
        loop {
            let end = *pts.last().unwrap();
            if end.distance(pts[0]) < 0.5 && pts.len() > 2 {
                pts.pop();
                break;
            }
            let Some(i) = segs.iter().position(|(p, q)| p.distance(end) < 0.5 || q.distance(end) < 0.5) else {
                break;
            };
            let (p, q) = segs.swap_remove(i);
            pts.push(if p.distance(end) < 0.5 { q } else { p });
        }
        if pts.len() >= 3 {
            loops.push(pts);
        }
    }
    loops
}

/// Fills a cut region layer by layer, with thin lines between layers and
/// the heavy cut outline on top.
#[allow(clippy::too_many_arguments)]
fn draw_layers(
    dl: &mut DisplayList,
    doc: &Document,
    view: &View,
    id: ElementId,
    outline: Vec<DVec2>,
    bands: &[Band],
    lines: &[(DVec2, DVec2)],
    edge: Stroke,
) {
    dl.push(Prim::Polygon(outline.clone()), None, Some(CUT_FILL), Some(id));
    for band in bands {
        draw_band(dl, doc, band, view.scale as f64, None);
    }
    for (a, b) in lines {
        dl.line(*a, *b, crate::THIN, None);
    }
    dl.push(
        Prim::Polyline {
            pts: outline,
            closed: true,
        },
        Some(edge),
        None,
        Some(id),
    );
}
