//! The ViewCube: Revit's orientation widget in the top-right corner of 3D
//! views. Faces, edges and corners snap the view to that direction, the
//! compass letters turn to that side, the house goes Home, the arrows roll
//! to the neighbouring face, and dragging anywhere on it orbits.

use crate::app::HausitApp;
use egui::epaint::{Mesh, TessellationOptions, Tessellator, TextShape};
use egui::{pos2, vec2, Color32, FontId, Id, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use hausit_core::{DVec3, ElementId};
use hausit_model::view::OrbitCamera;

/// Half the cube's edge, in points.
const S: f32 = 32.0;
/// Width of the edge and corner bands, as a fraction of the half edge.
const BAND: f64 = 0.3;
const RING_IN: f64 = 1.55;
const RING_OUT: f64 = 1.95;
const HOME: (f64, f64) = (-45.0, 30.0);
const ANIM_SECS: f64 = 0.35;

struct Face {
    n: DVec3,
    u: DVec3,
    v: DVec3,
    label: &'static str,
}

const FACES: [Face; 6] = [
    Face {
        n: DVec3::NEG_Y,
        u: DVec3::X,
        v: DVec3::Z,
        label: "FRONT",
    },
    Face {
        n: DVec3::X,
        u: DVec3::Y,
        v: DVec3::Z,
        label: "RIGHT",
    },
    Face {
        n: DVec3::Y,
        u: DVec3::NEG_X,
        v: DVec3::Z,
        label: "BACK",
    },
    Face {
        n: DVec3::NEG_X,
        u: DVec3::NEG_Y,
        v: DVec3::Z,
        label: "LEFT",
    },
    Face {
        n: DVec3::Z,
        u: DVec3::X,
        v: DVec3::Y,
        label: "TOP",
    },
    Face {
        n: DVec3::NEG_Z,
        u: DVec3::X,
        v: DVec3::NEG_Y,
        label: "BOTTOM",
    },
];

/// What the pointer is over.
#[derive(Clone, Copy, PartialEq)]
enum Hit {
    /// A face, edge or corner: the direction the viewer moves to.
    Dir(DVec3),
    /// A compass letter: the side the viewer moves to, pitch kept.
    Compass(DVec3),
    Home,
    /// A roll arrow around a face-aligned cube: the new viewer direction.
    Arrow(DVec3),
}

#[derive(Clone, Copy)]
struct Anim {
    from: (f64, f64),
    to: (f64, f64),
    t0: f64,
}

/// Screen projection of cube space for the current camera.
#[derive(Clone, Copy)]
struct Proj {
    c: Pos2,
    right: DVec3,
    up: DVec3,
    forward: DVec3,
}

impl Proj {
    fn vec(self, d: DVec3) -> Vec2 {
        vec2(d.dot(self.right) as f32 * S, -(d.dot(self.up) as f32) * S)
    }
    fn pt(self, p: DVec3) -> Pos2 {
        self.c + self.vec(p)
    }
}

/// Camera that looks at the model from direction `d` (towards the origin).
fn orientation(d: DVec3, current_yaw: f64) -> (f64, f64) {
    let f = -d.normalize();
    let pitch = (-f.z).clamp(-1.0, 1.0).asin().to_degrees();
    let yaw = if f.x.abs() + f.y.abs() < 1e-6 {
        // Straight up or down: keep the nearest square rotation, as Revit does.
        (current_yaw / 90.0).round() * 90.0
    } else {
        f.x.atan2(f.y).to_degrees()
    };
    (yaw, pitch)
}

fn band(t: f64) -> f64 {
    if t < -1.0 + 2.0 * BAND {
        -1.0
    } else if t > 1.0 - 2.0 * BAND {
        1.0
    } else {
        0.0
    }
}

fn band_range(r: f64) -> (f64, f64) {
    let e = 1.0 - 2.0 * BAND;
    match r as i32 {
        -1 => (-1.0, -e),
        1 => (e, 1.0),
        _ => (-e, e),
    }
}

fn visible(face: &Face, p: Proj) -> bool {
    face.n.dot(p.forward) < -0.02
}

/// Face-local coordinates of screen point `q` on `face`, if inside it.
fn face_uv(face: &Face, p: Proj, q: Pos2) -> Option<(f64, f64)> {
    let (a, b) = (p.vec(face.u), p.vec(face.v));
    let d = q - p.pt(face.n);
    let det = a.x * b.y - a.y * b.x;
    if det.abs() < 1e-3 {
        return None;
    }
    let u = ((d.x * b.y - d.y * b.x) / det) as f64;
    let v = ((a.x * d.y - a.y * d.x) / det) as f64;
    (u.abs() <= 1.0 && v.abs() <= 1.0).then_some((u, v))
}

fn compass_letters() -> [(&'static str, DVec3); 4] {
    [("N", DVec3::Y), ("E", DVec3::X), ("S", DVec3::NEG_Y), ("W", DVec3::NEG_X)]
}

fn compass_pos(p: Proj, dir: DVec3) -> Pos2 {
    p.pt(dir * ((RING_IN + RING_OUT) / 2.0) - DVec3::Z)
}

/// The view direction when the camera looks straight at a face.
fn aligned_face(p: Proj) -> Option<DVec3> {
    let d = -p.forward;
    let r = d.round();
    (r.length_squared() == 1.0 && d.dot(r) > 0.9999).then_some(r)
}

fn arrows(p: Proj) -> [(DVec3, Pos2, Vec2); 4] {
    let o = S * 1.55;
    let snap = |v: DVec3| v.round();
    [
        (snap(p.up), p.c + vec2(0.0, -o), vec2(0.0, -1.0)),
        (snap(-p.up), p.c + vec2(0.0, o), vec2(0.0, 1.0)),
        (snap(-p.right), p.c + vec2(-o, 0.0), vec2(-1.0, 0.0)),
        (snap(p.right), p.c + vec2(o, 0.0), vec2(1.0, 0.0)),
    ]
}

fn home_pos(area: Rect) -> Pos2 {
    area.left_top() + vec2(12.0, 12.0)
}

fn hit(p: Proj, area: Rect, q: Pos2) -> Option<Hit> {
    if q.distance(home_pos(area)) < 9.0 {
        return Some(Hit::Home);
    }
    if aligned_face(p).is_some() {
        for (d, at, _) in arrows(p) {
            if q.distance(at) < 8.0 {
                return Some(Hit::Arrow(d));
            }
        }
    }
    for face in FACES.iter().filter(|f| visible(f, p)) {
        if let Some((u, v)) = face_uv(face, p, q) {
            return Some(Hit::Dir(face.n + face.u * band(u) + face.v * band(v)));
        }
    }
    compass_letters()
        .into_iter()
        .find(|(_, d)| q.distance(compass_pos(p, *d)) < 9.0)
        .map(|(_, d)| Hit::Compass(d))
}

/// The highlighted region of `face` for direction `d`, if `d` touches it.
fn region(face: &Face, d: DVec3) -> Option<[DVec3; 4]> {
    if (d.dot(face.n) - 1.0).abs() > 1e-6 {
        return None;
    }
    let (u0, u1) = band_range(d.dot(face.u));
    let (v0, v1) = band_range(d.dot(face.v));
    let at = |u: f64, v: f64| face.n + face.u * u + face.v * v;
    Some([at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1)])
}

fn alpha(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a)
}

fn quad(pts: [Pos2; 4], fill: Color32) -> Shape {
    let mut mesh = Mesh::default();
    for p in pts {
        mesh.colored_vertex(p, fill);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    Shape::mesh(mesh)
}

/// Text laid flat on a face: the glyph mesh is mapped through the face's
/// projected axes, so labels skew with the cube like Revit's.
fn face_label(ui: &Ui, p: Proj, face: &Face, color: Color32) -> Option<Shape> {
    let font = FontId::proportional(20.0);
    let galley = ui.painter().layout_no_wrap(face.label.to_string(), font, color);
    let size = galley.size();
    let k = (1.5 / size.x as f64).min(0.42 / size.y as f64);
    let ctx = ui.ctx();
    let mut tess = Tessellator::new(
        ctx.pixels_per_point(),
        TessellationOptions::default(),
        ctx.fonts(|f| f.font_image_size()),
        vec![],
    );
    let mut mesh = Mesh::default();
    tess.tessellate_text(&TextShape::new(pos2(-size.x / 2.0, -size.y / 2.0), galley, color), &mut mesh);
    let (a, b, o) = (p.vec(face.u), p.vec(face.v), p.pt(face.n));
    for v in &mut mesh.vertices {
        let (u, w) = (v.pos.x as f64 * k, -v.pos.y as f64 * k);
        v.pos = o + a * u as f32 + b * w as f32;
    }
    Some(Shape::mesh(mesh))
}

fn set_camera(app: &mut HausitApp, view_id: ElementId, yaw: f64, pitch: f64) {
    if let Some(v) = app.doc.view_mut(view_id) {
        v.camera.yaw = yaw;
        v.camera.pitch = pitch;
    }
    // Revit fits the model to the view when the ViewCube changes the view.
    if let Some(c) = app.cameras.get_mut(&view_id) {
        c.fitted = false;
    }
}

fn animate_to(ui: &Ui, id: Id, cam: OrbitCamera, to: (f64, f64)) {
    let now = ui.input(|i| i.time);
    let anim = Anim {
        from: (cam.yaw, cam.pitch),
        to,
        t0: now,
    };
    ui.ctx().data_mut(|d| d.insert_temp(id, anim));
}

fn step_animation(app: &mut HausitApp, ui: &Ui, id: Id, view_id: ElementId) {
    let Some(anim) = ui.ctx().data(|d| d.get_temp::<Anim>(id)) else {
        return;
    };
    let t = ((ui.input(|i| i.time) - anim.t0) / ANIM_SECS).clamp(0.0, 1.0);
    let s = t * t * (3.0 - 2.0 * t);
    // Turn the short way round.
    let dyaw = (anim.to.0 - anim.from.0 + 540.0).rem_euclid(360.0) - 180.0;
    let yaw = anim.from.0 + dyaw * s;
    let pitch = anim.from.1 + (anim.to.1 - anim.from.1) * s;
    set_camera(app, view_id, yaw, pitch);
    if t >= 1.0 {
        ui.ctx().data_mut(|d| d.remove::<Anim>(id));
    } else {
        ui.ctx().request_repaint();
    }
}

pub fn show(app: &mut HausitApp, ui: &mut Ui, canvas: Rect, view_id: ElementId) {
    let id = Id::unique(("viewcube", view_id));
    step_animation(app, ui, id, view_id);
    let Some(cam) = app.doc.view(view_id).map(|v| v.camera) else {
        return;
    };

    let area = Rect::from_min_size(pos2(canvas.right() - 152.0, canvas.top() + 6.0), vec2(146.0, 146.0));
    let response = ui.interact(area, id, Sense::click_and_drag());
    let (right, up, forward) = cam.basis();
    let p = Proj {
        c: area.center() + vec2(0.0, -4.0),
        right,
        up,
        forward,
    };
    let pointer = response.hover_pos();
    let over = pointer.and_then(|q| hit(p, area, q));
    let active = response.hovered() || response.dragged();
    // Like Revit, the cube is faded until the pointer comes near it.
    let a = if active { 1.0 } else { 0.8 };

    let painter = ui.painter_at(area);
    let edge = alpha(Color32::from_gray(120), a);
    let blue = Color32::from_rgb(126, 186, 245);

    // Compass ring on the ground plane, under the cube.
    let n = 64;
    let ring = |r: f64| -> Vec<Pos2> {
        (0..n)
            .map(|i| {
                let t = i as f64 / n as f64 * std::f64::consts::TAU;
                p.pt(DVec3::new(t.cos() * r, t.sin() * r, -1.0))
            })
            .collect()
    };
    let (inner, outer) = (ring(RING_IN), ring(RING_OUT));
    let mut mesh = Mesh::default();
    let ring_fill = alpha(Color32::from_rgb(214, 218, 224), a);
    for i in 0..n {
        mesh.colored_vertex(inner[i], ring_fill);
        mesh.colored_vertex(outer[i], ring_fill);
    }
    for i in 0..n {
        let (j, k) = (2 * i as u32, 2 * ((i + 1) % n) as u32);
        mesh.add_triangle(j, j + 1, k + 1);
        mesh.add_triangle(j, k + 1, k);
    }
    painter.add(Shape::mesh(mesh));
    painter.add(Shape::closed_line(inner, Stroke::new(1.0, edge)));
    painter.add(Shape::closed_line(outer, Stroke::new(1.0, edge)));
    for (label, d) in compass_letters() {
        let at = compass_pos(p, d);
        let hot = over == Some(Hit::Compass(d));
        if hot {
            painter.circle_filled(at, 8.0, alpha(blue, a));
        }
        painter.text(
            at,
            egui::Align2::CENTER_CENTER,
            label,
            FontId::proportional(11.0),
            alpha(Color32::from_gray(50), a),
        );
    }

    // Cube faces, lit from the upper front left.
    let light = DVec3::new(-0.4, -0.6, 0.7).normalize();
    for face in FACES.iter().filter(|f| visible(f, p)) {
        let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(u, v)| p.pt(face.n + face.u * u + face.v * v));
        let shade = (226.0 + 24.0 * face.n.dot(light).max(-0.3)) as u8;
        painter.add(quad(corners, alpha(Color32::from_gray(shade), a)));
        if let Some(Hit::Dir(d)) = over {
            if let Some(r) = region(face, d) {
                painter.add(quad(r.map(|q| p.pt(q)), alpha(blue, a)));
            }
        }
        if let Some(label) = face_label(ui, p, face, alpha(Color32::from_gray(70), a)) {
            painter.add(label);
        }
        painter.add(Shape::closed_line(corners.to_vec(), Stroke::new(1.0, edge)));
    }

    // Roll arrows around a face-aligned cube.
    if aligned_face(p).is_some() {
        for (d, at, dir) in arrows(p) {
            let side = vec2(-dir.y, dir.x) * 6.0;
            let tri = vec![at + dir * 5.0, at - dir * 3.0 + side, at - dir * 3.0 - side];
            let fill = if over == Some(Hit::Arrow(d)) {
                blue
            } else {
                Color32::from_gray(150)
            };
            painter.add(Shape::convex_polygon(tri, alpha(fill, a), Stroke::new(1.0, edge)));
        }
    }

    // Home.
    let h = home_pos(area);
    if over == Some(Hit::Home) {
        painter.circle_filled(h, 9.0, alpha(blue, a));
    }
    let house = vec![
        h + vec2(0.0, -6.0),
        h + vec2(6.0, 0.0),
        h + vec2(4.0, 0.0),
        h + vec2(4.0, 5.0),
        h + vec2(-4.0, 5.0),
        h + vec2(-4.0, 0.0),
        h + vec2(-6.0, 0.0),
    ];
    painter.add(Shape::closed_line(house, Stroke::new(1.2, alpha(Color32::from_gray(70), a))));

    // Input.
    if response.dragged_by(egui::PointerButton::Primary) {
        ui.ctx().data_mut(|d| d.remove::<Anim>(id));
        let delta = response.drag_delta();
        if let Some(v) = app.doc.view_mut(view_id) {
            v.camera.yaw += delta.x as f64 * 0.5;
            v.camera.pitch = (v.camera.pitch + delta.y as f64 * 0.5).clamp(-90.0, 90.0);
        }
    } else if response.clicked() {
        let to = match over {
            Some(Hit::Dir(d)) | Some(Hit::Arrow(d)) => Some(orientation(d, cam.yaw)),
            Some(Hit::Compass(d)) => Some((orientation(d, cam.yaw).0, cam.pitch)),
            Some(Hit::Home) => Some(HOME),
            None => None,
        };
        if let Some(to) = to {
            animate_to(ui, id, cam, to);
        }
    }
    let tip = match over {
        Some(Hit::Home) => Some("Home"),
        Some(Hit::Compass(_)) => Some("Turn to this side"),
        Some(Hit::Arrow(_)) => Some("Rotate to the next face"),
        Some(Hit::Dir(_)) => Some("Click to orient · drag to orbit"),
        None => None,
    };
    let response = match tip {
        Some(t) if !response.dragged() => response.on_hover_text(t),
        _ => response,
    };
    response.context_menu(|ui| {
        if ui.button("Go Home").clicked() {
            animate_to(ui, id, cam, HOME);
            ui.close();
        }
        ui.menu_button("Orient to a Direction", |ui| {
            let dirs: [(&str, DVec3); 10] = [
                ("Top", DVec3::Z),
                ("Bottom", DVec3::NEG_Z),
                ("Front", DVec3::NEG_Y),
                ("Back", DVec3::Y),
                ("Left", DVec3::NEG_X),
                ("Right", DVec3::X),
                ("Southeast Isometric", DVec3::new(1.0, -1.0, 1.0)),
                ("Southwest Isometric", DVec3::new(-1.0, -1.0, 1.0)),
                ("Northeast Isometric", DVec3::new(1.0, 1.0, 1.0)),
                ("Northwest Isometric", DVec3::new(-1.0, 1.0, 1.0)),
            ];
            for (label, d) in dirs {
                if ui.button(label).clicked() {
                    animate_to(ui, id, cam, orientation(d, cam.yaw));
                    ui.close();
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_map_to_revit_views() {
        let close = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9;
        assert!(close(orientation(DVec3::NEG_Y, 0.0), (0.0, 0.0)));
        assert!(close(orientation(DVec3::X, 0.0), (-90.0, 0.0)));
        assert!(close(orientation(DVec3::Z, 10.0), (0.0, 90.0)));
        let (yaw, pitch) = orientation(DVec3::new(1.0, -1.0, 1.0), 0.0);
        assert!((yaw + 45.0).abs() < 1e-9 && (pitch - 35.264).abs() < 1e-3);
    }

    #[test]
    fn edge_regions_touch_two_faces() {
        let d = DVec3::new(1.0, -1.0, 0.0);
        assert_eq!(FACES.iter().filter(|f| region(f, d).is_some()).count(), 2);
        let corner = DVec3::new(1.0, -1.0, 1.0);
        assert_eq!(FACES.iter().filter(|f| region(f, corner).is_some()).count(), 3);
    }
}
