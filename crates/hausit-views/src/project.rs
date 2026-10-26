//! Orthographic projection of model solids for elevations, sections and 3D
//! views. Faces are ordered exactly with a BSP tree and painted far to near,
//! so filled faces hide what is behind them (hidden line / shaded).

use crate::bsp::{Bsp, Poly};
use crate::frame::ViewFrame;
use crate::{DisplayList, Prim, Rgba, Stroke, BLACK, CUT_FILL};
use hausit_core::geom::{clip_polygon_2d, polygon_area};
use hausit_core::{DVec2, DVec3, ElementId, Plane};
use hausit_model::geometry::{model_geometry, Material, Shape};
use hausit_model::{Category, Document, View, VisualStyle};

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

    let geometry = model_geometry(doc);
    let mut polys = Vec::new();
    for g in &geometry {
        let faces: Vec<(Material, hausit_model::geometry::Face)> = g
            .parts
            .iter()
            .flat_map(|p| p.faces().into_iter().map(move |f| (p.material, f)))
            .collect();
        let inside = internal_faces(&faces);
        for (fi, (material, face)) in faces.iter().enumerate() {
            if inside[fi] {
                continue;
            }
            let part = Part { material: *material };
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
                {
                    (pts, edges) = clip_with_edges(&pts, &edges, &plane);
                }
                if pts.len() < 3 {
                    continue;
                }
                let base = category_color(g.category, part.material);
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
                    data: FaceData { fill, id: g.id },
                });
            }
        }
    }

    split_t_junctions(&mut polys);
    hide_coplanar_seams(&mut polys);

    let edge = Stroke::new(1.0, if style == VisualStyle::Shaded { [40, 40, 40, 255] } else { BLACK });
    let mut dl = DisplayList::default();
    let bsp = Bsp::new(polys);
    for p in bsp.back_to_front(frame.forward) {
        let pts: Vec<DVec2> = p.pts.iter().map(|q| frame.to_view(*q).0).collect();
        if polygon_area(&pts).abs() < 1e-3 && p.data.fill.is_some() {
            continue;
        }
        if p.data.fill.is_some() {
            dl.push(Prim::Polygon(pts.clone()), None, p.data.fill, Some(p.data.id));
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

/// Stand-in for the geometry part a face came from.
struct Part {
    material: Material,
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
/// reads as one surface, as in Revit.
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
                if pa != pb && normals[pa].dot(normals[pb]) > 0.9999 {
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
                        dl.push(
                            Prim::Polygon(rect),
                            Some(crate::pen(doc, g.category, true, view.scale)),
                            Some(fill),
                            Some(g.id),
                        );
                    }
                }
                Shape::Faces(faces) => {
                    let plane = Plane::from_point_normal(frame.origin, frame.forward);
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
                            dl.line(hits[0], hits[1], crate::pen(doc, g.category, true, view.scale), Some(g.id));
                        }
                    }
                }
            }
        }
    }
}
