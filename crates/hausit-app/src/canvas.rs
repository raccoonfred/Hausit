//! The drawing area: view tabs, painting display lists, pan/zoom/orbit,
//! selection, tool input and previews, and the view control bar.

use crate::app::{Camera2D, HausitApp};
use crate::theme::{c32, CANVAS_BG, HOVER, PREVIEW, SELECT, SNAP};
use crate::tools::{self, DrawShape, SnapKind, Tool};
use egui::{pos2, vec2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui};
use hausit_core::geom::{distance_to_segment, perp, point_in_polygon, triangulate};
use hausit_core::{DVec2, ElementId};
use hausit_model::{Category, DetailLevel, ViewKind, VisualStyle};
use hausit_views::{Anchor, Dash, DisplayList, Item, Prim};

/// Screen ⇄ view-coordinate transform for one frame.
#[derive(Clone, Copy)]
pub struct Xform {
    pub rect: Rect,
    pub cam: Camera2D,
}

impl Xform {
    pub fn to_screen(self, p: DVec2) -> Pos2 {
        let c = self.rect.center();
        let d = (p - self.cam.center) * self.cam.zoom;
        pos2(c.x + d.x as f32, c.y - d.y as f32)
    }

    pub fn to_view(self, p: Pos2) -> DVec2 {
        let c = self.rect.center();
        self.cam.center + DVec2::new((p.x - c.x) as f64, -(p.y - c.y) as f64) / self.cam.zoom
    }

    /// Model length of `px` screen pixels.
    pub fn px(self, px: f32) -> f64 {
        px as f64 / self.cam.zoom
    }
}

pub fn show(app: &mut HausitApp, ui: &mut Ui) {
    view_tabs(app, ui);
    let Some(view_id) = app.active_view else {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("No view is open.\nDouble-click a view in the Project Browser.").color(Color32::GRAY));
        });
        return;
    };
    let Some(view) = app.doc.view(view_id) else { return };
    if let ViewKind::Schedule(_) = view.kind {
        crate::schedule_view::show(app, ui, view_id);
        return;
    }

    let control_h = 26.0;
    let full = ui.available_rect_before_wrap();
    let rect = Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - control_h));
    let response = ui.allocate_rect(rect, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, CANVAS_BG);

    let is_3d = matches!(view.kind, ViewKind::ThreeD);
    let is_plan = matches!(view.kind, ViewKind::FloorPlan { .. });

    // Graphics and camera.
    let bounds = app.active_graphics().and_then(|d| d.bounds());
    let cam = app.cameras.entry(view_id).or_default();
    if !cam.fitted {
        if let Some((min, max)) = bounds {
            let size = (max - min).max(DVec2::splat(1000.0));
            cam.zoom = ((rect.width() as f64 * 0.9) / size.x).min((rect.height() as f64 * 0.9) / size.y);
            cam.center = (min + max) * 0.5;
        }
        cam.fitted = true;
    }
    navigate(app, ui, &response, rect, view_id, is_3d);
    let xf = Xform {
        rect,
        cam: app.cameras[&view_id],
    };
    let dl = app.active_graphics().cloned().unwrap_or_default();

    paint_list(&painter, &xf, &dl, None);

    // Hover / pre-selection.
    let pointer = response.hover_pos();
    app.hover = None;
    if let (Some(p), Tool::Modify) = (pointer, &app.tool) {
        // Tab cycles through overlapping elements, as in Revit.
        let candidates = hit_candidates(&dl, &xf, p);
        if candidates != app.pick_candidates {
            app.pick_candidates = candidates;
            app.pick_cycle = 0;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Tab)) && !app.pick_candidates.is_empty() {
            app.pick_cycle += 1;
        }
        if !app.pick_candidates.is_empty() {
            app.hover = Some(app.pick_candidates[app.pick_cycle % app.pick_candidates.len()]);
        }
    }
    if !app.selection.is_empty() {
        paint_list(&painter, &xf, &dl, Some((&app.selection, SELECT)));
    }
    if let Some(h) = app.hover {
        if !app.selection.contains(&h) {
            paint_list(&painter, &xf, &dl, Some((&[h], HOVER)));
        }
    }

    // Tool input.
    let shift = ui.input(|i| i.modifiers.shift);
    let ctrl = ui.input(|i| i.modifiers.command);
    let raw = pointer.map(|p| xf.to_view(p));
    let mut snapped = raw;
    let mut snap_kind = None;
    if let Some(p) = raw {
        if is_plan && !matches!(app.tool, Tool::Modify | Tool::Door | Tool::Window) {
            if let Some((q, k)) = tools::snap(app, p, xf.px(10.0)) {
                snapped = Some(q);
                snap_kind = Some(k);
            }
            if let (Some(anchor), None | Some(SnapKind::Nearest)) = (app.tool.anchor(), snap_kind) {
                let rect_mode = app.options.shape == DrawShape::Rectangle;
                if !rect_mode || !matches!(app.tool, Tool::Wall { .. } | Tool::Floor { .. } | Tool::Roof { .. }) {
                    snapped = Some(tools::align(anchor, snapped.unwrap(), xf.px(8.0), shift));
                }
            }
        }
    }

    if response.clicked_by(egui::PointerButton::Primary) {
        if let Some(p) = snapped {
            if app.tool == Tool::Modify {
                select_click(app, app.hover, ctrl, shift);
            } else if app.tool.is_sketch() || is_plan || matches!(app.tool, Tool::Level) {
                tools::click(app, p);
            }
        }
    }
    if response.double_clicked() {
        if let Some(id) = pointer.and_then(|sp| hit_test(&dl, &xf, sp)) {
            if app.doc.view(id).is_some() {
                app.open_view(id);
            }
        }
    }
    if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !app.options.typed.is_empty() {
        if let Some(c) = snapped {
            tools::apply_typed_length(app, c);
        }
    }

    // Window / crossing selection.
    box_select(app, ui, &response, &painter, &dl, &xf);

    // Previews.
    if let Some(p) = snapped {
        preview(app, &painter, &xf, p);
        if let Some(k) = snap_kind {
            paint_snap(&painter, xf.to_screen(p), k);
        }
    }
    if let Some(p) = pointer {
        if app.tool != Tool::Modify && is_plan {
            let cursor = egui::CursorIcon::Crosshair;
            ui.ctx().set_cursor_icon(cursor);
            let _ = p;
        }
    }

    context_menu(app, &response);
    if is_3d {
        view_cube(app, ui, rect, view_id);
    }
    view_control_bar(app, ui, Rect::from_min_max(pos2(full.min.x, rect.max.y), full.max), view_id);
}

fn view_tabs(app: &mut HausitApp, ui: &mut Ui) {
    let mut activate = None;
    let mut close = None;
    egui::Frame::new()
        .fill(crate::theme::TAB_BAR_BG)
        .inner_margin(egui::Margin::symmetric(4, 2))
        .show(ui, |ui| {
            egui::ScrollArea::horizontal().id_salt("view_tabs").show(ui, |ui| {
                ui.horizontal(|ui| {
                    for id in app.open_views.clone() {
                        let Some(v) = app.doc.view(id) else { continue };
                        let active = app.active_view == Some(id);
                        let frame = egui::Frame::new()
                            .fill(if active { Color32::WHITE } else { Color32::from_gray(218) })
                            .stroke(Stroke::new(1.0, Color32::from_gray(190)))
                            .inner_margin(egui::Margin::symmetric(8, 3));
                        frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let r = ui.add(egui::Label::new(&v.name).sense(Sense::click()));
                                if r.clicked() {
                                    activate = Some(id);
                                }
                                if ui.add(egui::Button::new("×").frame(false).small()).on_hover_text("Close").clicked() {
                                    close = Some(id);
                                }
                            });
                        });
                    }
                });
            });
        });
    if let Some(id) = activate {
        app.open_view(id);
    }
    if let Some(id) = close {
        app.close_view(id);
    }
}

/// Wheel zooms about the cursor; middle (or right) drag pans; in 3D,
/// Shift+middle or right drag orbits.
fn navigate(app: &mut HausitApp, ui: &Ui, response: &egui::Response, rect: Rect, view_id: ElementId, is_3d: bool) {
    let (scroll, zoom_delta, shift) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta(), i.modifiers.shift));
    let pointer = response.hover_pos();
    let cam = app.cameras.get_mut(&view_id).unwrap();
    if let Some(p) = pointer {
        let factor = (scroll as f64 / 300.0).exp() * zoom_delta as f64;
        if (factor - 1.0).abs() > 1e-6 {
            let xf = Xform { rect, cam: *cam };
            let before = xf.to_view(p);
            cam.zoom = (cam.zoom * factor).clamp(1e-4, 50.0);
            let xf = Xform { rect, cam: *cam };
            cam.center += before - xf.to_view(p);
        }
    }
    let delta = response.drag_delta();
    let orbit =
        is_3d && (response.dragged_by(egui::PointerButton::Secondary) || (shift && response.dragged_by(egui::PointerButton::Middle)));
    if orbit {
        if let Some(v) = app.doc.view_mut(view_id) {
            v.camera.yaw += delta.x as f64 * 0.4;
            v.camera.pitch = (v.camera.pitch + delta.y as f64 * 0.4).clamp(-89.0, 89.0);
        }
    } else if response.dragged_by(egui::PointerButton::Middle) || response.dragged_by(egui::PointerButton::Secondary) {
        cam.center -= DVec2::new(delta.x as f64, -delta.y as f64) / cam.zoom;
    }
}

// ----- painting -------------------------------------------------------------

fn dash_shapes(pts: &[Pos2], stroke: Stroke, dash: Dash, out: &mut Vec<Shape>) {
    match dash {
        Dash::Solid => out.push(Shape::line(pts.to_vec(), stroke)),
        Dash::Dashed => out.extend(Shape::dashed_line(pts, stroke, 8.0, 4.0)),
        Dash::DashDot => out.extend(Shape::dashed_line_with_offset(pts, stroke, &[14.0, 2.0], &[3.0, 3.0], 0.0)),
    }
}

fn fill_polygon(pts: &[Pos2], color: Color32, out: &mut Vec<Shape>) {
    let p2: Vec<DVec2> = pts.iter().map(|p| DVec2::new(p.x as f64, p.y as f64)).collect();
    let mut mesh = egui::Mesh::default();
    for p in pts {
        mesh.colored_vertex(*p, color);
    }
    for t in triangulate(&p2) {
        mesh.add_triangle(t[0] as u32, t[1] as u32, t[2] as u32);
    }
    out.push(Shape::mesh(mesh));
}

fn arc_points(xf: &Xform, center: DVec2, r: f64, a0: f64, a1: f64) -> Vec<Pos2> {
    let n = ((a1 - a0).abs() * 12.0).ceil().max(4.0) as usize;
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f64 / n as f64;
            xf.to_screen(center + DVec2::new(a.cos(), a.sin()) * r)
        })
        .collect()
}

/// Paints a display list. With `highlight`, paints only the items of those
/// elements, recoloured, on top.
pub fn paint_list(painter: &egui::Painter, xf: &Xform, dl: &DisplayList, highlight: Option<(&[ElementId], Color32)>) {
    let mut shapes = Vec::new();
    let mut texts = Vec::new();
    let visible = painter.clip_rect().expand(50.0);
    for item in &dl.items {
        if let Some((ids, _)) = highlight {
            if !item.element.map(|e| ids.contains(&e)).unwrap_or(false) {
                continue;
            }
        }
        let (stroke, fill) = match highlight {
            Some((_, c)) => (
                item.stroke.map(|s| Stroke::new(s.width + 1.0, c)).or(Some(Stroke::new(1.5, c))),
                item.fill.map(|_| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 50)),
            ),
            None => (item.stroke.map(|s| Stroke::new(s.width, c32(s.color))), item.fill.map(c32)),
        };
        let dash = item.stroke.map(|s| s.dash).unwrap_or(Dash::Solid);
        match &item.prim {
            Prim::Line(a, b) => {
                let (a, b) = (xf.to_screen(*a), xf.to_screen(*b));
                if !visible.intersects(Rect::from_two_pos(a, b)) {
                    continue;
                }
                if let Some(s) = stroke {
                    dash_shapes(&[a, b], s, dash, &mut shapes);
                }
            }
            Prim::Polyline { pts, closed } => {
                let mut p: Vec<Pos2> = pts.iter().map(|q| xf.to_screen(*q)).collect();
                if *closed && !p.is_empty() {
                    p.push(p[0]);
                }
                if let Some(s) = stroke {
                    dash_shapes(&p, s, dash, &mut shapes);
                }
            }
            Prim::Polygon(pts) => {
                let mut p: Vec<Pos2> = pts.iter().map(|q| xf.to_screen(*q)).collect();
                if !visible.intersects(Rect::from_points(&p)) {
                    continue;
                }
                if let Some(f) = fill {
                    fill_polygon(&p, f, &mut shapes);
                }
                if let Some(s) = stroke {
                    p.push(p[0]);
                    dash_shapes(&p, s, dash, &mut shapes);
                }
            }
            Prim::Arc {
                center,
                radius,
                start,
                end,
            } => {
                if let Some(s) = stroke {
                    dash_shapes(&arc_points(xf, *center, *radius, *start, *end), s, dash, &mut shapes);
                }
            }
            Prim::Circle { center, radius } => {
                let c = xf.to_screen(*center);
                let r = (*radius * xf.cam.zoom) as f32;
                if let Some(f) = fill {
                    shapes.push(Shape::circle_filled(c, r, f));
                }
                if let Some(s) = stroke {
                    shapes.push(Shape::circle_stroke(c, r, s));
                }
            }
            Prim::Text { pos, text, height, anchor } => {
                let px = (*height * xf.cam.zoom) as f32;
                if px < 5.0 || highlight.is_some() {
                    continue;
                }
                let align = match anchor {
                    Anchor::Center => egui::Align2::CENTER_CENTER,
                    Anchor::Left => egui::Align2::LEFT_CENTER,
                    Anchor::Right => egui::Align2::RIGHT_CENTER,
                    Anchor::BottomLeft => egui::Align2::LEFT_BOTTOM,
                    Anchor::BottomRight => egui::Align2::RIGHT_BOTTOM,
                };
                texts.push((xf.to_screen(*pos), align, text, px));
            }
        }
    }
    painter.extend(shapes);
    // Text last, so it sits on top of bubble and tag fills.
    for (pos, align, text, px) in texts {
        painter.text(pos, align, text, FontId::proportional(px), Color32::BLACK);
    }
}

// ----- picking --------------------------------------------------------------

fn item_hit(item: &Item, xf: &Xform, p: Pos2) -> bool {
    let tol = 5.0f64;
    let pv = DVec2::new(p.x as f64, p.y as f64);
    let s = |q: DVec2| {
        let s = xf.to_screen(q);
        DVec2::new(s.x as f64, s.y as f64)
    };
    match &item.prim {
        Prim::Line(a, b) => distance_to_segment(pv, s(*a), s(*b)) < tol,
        Prim::Polyline { pts, closed } => {
            let n = pts.len();
            let m = if *closed { n } else { n.saturating_sub(1) };
            (0..m).any(|i| distance_to_segment(pv, s(pts[i]), s(pts[(i + 1) % n])) < tol)
        }
        Prim::Polygon(pts) => {
            let sp: Vec<DVec2> = pts.iter().map(|q| s(*q)).collect();
            (item.fill.is_some() && point_in_polygon(pv, &sp))
                || (0..sp.len()).any(|i| distance_to_segment(pv, sp[i], sp[(i + 1) % sp.len()]) < tol)
        }
        Prim::Circle { center, radius } => s(*center).distance(pv) < radius * xf.cam.zoom + tol,
        Prim::Arc { center, radius, .. } => (s(*center).distance(pv) - radius * xf.cam.zoom).abs() < tol,
        Prim::Text { pos, height, .. } => s(*pos).distance(pv) < height * xf.cam.zoom,
    }
}

/// Elements under the cursor, topmost first. Stops at the first opaque
/// fill, which hides everything behind it.
pub fn hit_candidates(dl: &DisplayList, xf: &Xform, p: Pos2) -> Vec<ElementId> {
    let mut out: Vec<ElementId> = Vec::new();
    for item in dl.items.iter().rev() {
        let Some(id) = item.element else { continue };
        if !item_hit(item, xf, p) {
            continue;
        }
        if !out.contains(&id) {
            out.push(id);
        }
        let opaque_fill = matches!(item.prim, Prim::Polygon(_)) && item.fill.map(|f| f[3] == 255).unwrap_or(false);
        if opaque_fill {
            break;
        }
    }
    out
}

pub fn hit_test(dl: &DisplayList, xf: &Xform, p: Pos2) -> Option<ElementId> {
    hit_candidates(dl, xf, p).first().copied()
}

fn select_click(app: &mut HausitApp, hit: Option<ElementId>, ctrl: bool, shift: bool) {
    match hit {
        Some(id) if shift => app.selection.retain(|s| *s != id),
        Some(id) if ctrl => {
            if !app.selection.contains(&id) {
                app.selection.push(id);
            }
        }
        Some(id) => app.selection = vec![id],
        None if !ctrl && !shift => app.selection.clear(),
        None => {}
    }
    if let Some(id) = app.selection.last() {
        app.status = app.doc.describe(*id);
    }
}

fn box_select(app: &mut HausitApp, ui: &Ui, response: &egui::Response, painter: &egui::Painter, dl: &DisplayList, xf: &Xform) {
    if app.tool != Tool::Modify {
        return;
    }
    let id = egui::Id::unique("box_select_start");
    if response.drag_started_by(egui::PointerButton::Primary) {
        if let Some(p) = response.interact_pointer_pos() {
            ui.ctx().data_mut(|d| d.insert_temp(id, p));
        }
    }
    let Some(start) = ui.ctx().data(|d| d.get_temp::<Pos2>(id)) else {
        return;
    };
    let Some(cur) = ui.input(|i| i.pointer.latest_pos()) else { return };
    let r = Rect::from_two_pos(start, cur);
    let crossing = cur.x < start.x;
    let stroke = Stroke::new(1.0, SELECT);
    painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(0, 90, 255, 20));
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    if crossing {
        painter.extend(Shape::dashed_line(&pts, stroke, 5.0, 3.0));
    } else {
        painter.add(Shape::line(pts.to_vec(), stroke));
    }
    if response.drag_stopped() {
        ui.ctx().data_mut(|d| d.remove::<Pos2>(id));
        // Per-element screen bounds.
        let mut boxes: std::collections::HashMap<ElementId, Rect> = Default::default();
        for item in &dl.items {
            let Some(e) = item.element else { continue };
            let pts: Vec<DVec2> = match &item.prim {
                Prim::Line(a, b) => vec![*a, *b],
                Prim::Polyline { pts, .. } | Prim::Polygon(pts) => pts.clone(),
                Prim::Circle { center, radius } | Prim::Arc { center, radius, .. } => {
                    vec![*center - DVec2::splat(*radius), *center + DVec2::splat(*radius)]
                }
                Prim::Text { .. } => continue,
            };
            let sp: Vec<Pos2> = pts.iter().map(|q| xf.to_screen(*q)).collect();
            let b = Rect::from_points(&sp);
            boxes.entry(e).and_modify(|r| *r = r.union(b)).or_insert(b);
        }
        let shift = ui.input(|i| i.modifiers.shift);
        let ctrl = ui.input(|i| i.modifiers.command);
        if !ctrl && !shift {
            app.selection.clear();
        }
        for (e, b) in boxes {
            let hit = if crossing { r.intersects(b) } else { r.contains_rect(b) };
            // Views (section/elevation marks) are only picked by clicking.
            if hit && app.doc.element(e).is_some() {
                if shift {
                    app.selection.retain(|s| *s != e);
                } else if !app.selection.contains(&e) {
                    app.selection.push(e);
                }
            }
        }
        app.status = format!("{} selected", app.selection.len());
    }
}

// ----- previews -------------------------------------------------------------

fn paint_snap(painter: &egui::Painter, p: Pos2, kind: SnapKind) {
    let s = Stroke::new(1.5, SNAP);
    let r = 5.0;
    match kind {
        SnapKind::Endpoint => {
            painter.rect_stroke(Rect::from_center_size(p, vec2(2.0 * r, 2.0 * r)), 0.0, s, egui::StrokeKind::Middle);
        }
        SnapKind::Midpoint => {
            painter.add(Shape::closed_line(vec![p + vec2(-r, r), p + vec2(r, r), p + vec2(0.0, -r)], s));
        }
        SnapKind::Intersection => {
            painter.line_segment([p + vec2(-r, -r), p + vec2(r, r)], s);
            painter.line_segment([p + vec2(-r, r), p + vec2(r, -r)], s);
        }
        SnapKind::Nearest => {
            painter.add(Shape::closed_line(
                vec![p + vec2(-r, -r), p + vec2(r, r), p + vec2(-r, r), p + vec2(r, -r)],
                s,
            ));
        }
    }
}

fn label_at(painter: &egui::Painter, p: Pos2, text: &str) {
    let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(13.0), Color32::from_rgb(20, 60, 140));
    let r = Rect::from_center_size(p, galley.size() + vec2(8.0, 4.0));
    painter.rect(
        r,
        2.0,
        Color32::from_rgba_unmultiplied(255, 255, 255, 230),
        Stroke::new(1.0, PREVIEW),
        egui::StrokeKind::Inside,
    );
    painter.galley(r.min + vec2(4.0, 2.0), galley, Color32::BLACK);
}

fn preview(app: &mut HausitApp, painter: &egui::Painter, xf: &Xform, p: DVec2) {
    let s = Stroke::new(1.5, PREVIEW);
    let unit = app.doc.length_unit;
    let wall_t = app
        .type_for(Category::Walls)
        .and_then(|t| app.doc.family_type(t))
        .map(|t| t.thickness())
        .unwrap_or(200.0);
    let typed = app.options.typed.clone();
    let seg = |a: DVec2, b: DVec2| {
        painter.line_segment([xf.to_screen(a), xf.to_screen(b)], s);
        // Temporary dimension.
        let mid = xf.to_screen((a + b) * 0.5) + vec2(0.0, -14.0);
        let text = if typed.is_empty() {
            hausit_core::format_length(a.distance(b), unit)
        } else {
            format!("{typed}▏")
        };
        label_at(painter, mid, &text);
    };
    let rect_pts = |a: DVec2, b: DVec2| {
        let (min, max) = (a.min(b), a.max(b));
        vec![min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)]
    };
    match &app.tool {
        Tool::Wall { start: Some(a), .. } => {
            if app.options.shape == DrawShape::Rectangle {
                let r = rect_pts(*a, p);
                for i in 0..4 {
                    painter.line_segment([xf.to_screen(r[i]), xf.to_screen(r[(i + 1) % 4])], s);
                }
                label_at(
                    painter,
                    xf.to_screen(p) + vec2(40.0, 14.0),
                    &format!(
                        "{} × {}",
                        hausit_core::format_length((p.x - a.x).abs(), unit),
                        hausit_core::format_length((p.y - a.y).abs(), unit)
                    ),
                );
            } else {
                // Show wall thickness as a ghost outline.
                let t = wall_t;
                let n = perp((p - *a).normalize_or_zero());
                let shift = match app.options.location_line {
                    hausit_model::LocationLine::WallCenterline => 0.0,
                    hausit_model::LocationLine::FinishFaceExterior => -t / 2.0,
                    hausit_model::LocationLine::FinishFaceInterior => t / 2.0,
                };
                let (a2, b2) = (*a + n * shift, p + n * shift);
                let ghost: Vec<Pos2> = [a2 + n * t / 2.0, b2 + n * t / 2.0, b2 - n * t / 2.0, a2 - n * t / 2.0]
                    .iter()
                    .map(|q| xf.to_screen(*q))
                    .collect();
                painter.add(Shape::closed_line(
                    ghost,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 120, 230, 120)),
                ));
                seg(*a, p);
            }
        }
        Tool::Grid { start: Some(a) } | Tool::Section { start: Some(a) } => seg(*a, p),
        Tool::Move { base: Some(a) } | Tool::Copy { base: Some(a) } => {
            seg(*a, p);
            let dl = app.cache.list.clone();
            let moved = Xform {
                rect: xf.rect,
                cam: Camera2D {
                    center: xf.cam.center - (p - *a),
                    ..xf.cam
                },
            };
            paint_list(painter, &moved, &dl, Some((&app.selection, PREVIEW)));
        }
        Tool::Floor { pts, .. } | Tool::Roof { pts } => {
            if app.options.shape == DrawShape::Rectangle {
                if let Some(a) = pts.first() {
                    let r = rect_pts(*a, p);
                    let sp: Vec<Pos2> = r.iter().map(|q| xf.to_screen(*q)).collect();
                    painter.add(Shape::closed_line(sp, Stroke::new(2.0, Color32::from_rgb(200, 0, 160))));
                }
            } else {
                let mut sp: Vec<Pos2> = pts.iter().map(|q| xf.to_screen(*q)).collect();
                if !sp.is_empty() {
                    painter.add(Shape::line(sp.clone(), Stroke::new(2.0, Color32::from_rgb(200, 0, 160))));
                    sp.push(xf.to_screen(p));
                    let n = sp.len();
                    painter.line_segment([sp[n - 2], sp[n - 1]], s);
                    if pts.len() >= 2 {
                        painter.add(Shape::dashed_line(&[sp[n - 1], sp[0]], Stroke::new(1.0, PREVIEW), 4.0, 4.0));
                    }
                    seg(*pts.last().unwrap(), p);
                }
            }
        }
        Tool::Door | Tool::Window => {
            let cat = if app.tool == Tool::Door {
                Category::Doors
            } else {
                Category::Windows
            };
            let Some(t) = app.type_for(cat).and_then(|t| app.doc.family_type(t)) else {
                return;
            };
            let w = t.width();
            if let Some(h) = tools::pick_host(app, p, w, xf.px(12.0).max(300.0)) {
                let wall = app.doc.element(h.wall).and_then(|e| e.as_wall()).unwrap();
                let th = app.doc.type_of(h.wall).map(|t| t.thickness()).unwrap_or(200.0);
                let c = wall.point_at(h.offset);
                let d = wall.direction();
                let n = perp(d);
                let corners: Vec<Pos2> = [
                    c - d * w / 2.0 + n * th / 2.0,
                    c + d * w / 2.0 + n * th / 2.0,
                    c + d * w / 2.0 - n * th / 2.0,
                    c - d * w / 2.0 - n * th / 2.0,
                ]
                .iter()
                .map(|q| xf.to_screen(*q))
                .collect();
                painter.add(Shape::convex_polygon(corners, Color32::from_rgba_unmultiplied(0, 120, 230, 60), s));
                if cat == Category::Doors {
                    let side = if h.flip_facing { n } else { -n };
                    let tip = c + side * (th / 2.0 + w * 0.6);
                    painter.arrow(xf.to_screen(c), xf.to_screen(tip) - xf.to_screen(c), s);
                }
                // Temporary dimensions to the wall ends.
                label_at(
                    painter,
                    xf.to_screen(wall.start + d * (h.offset - w / 2.0) * 0.5) + vec2(0.0, -16.0),
                    &hausit_core::format_length(h.offset - w / 2.0, unit),
                );
                label_at(
                    painter,
                    xf.to_screen(wall.end - d * (wall.length() - h.offset - w / 2.0) * 0.5) + vec2(0.0, -16.0),
                    &hausit_core::format_length(wall.length() - h.offset - w / 2.0, unit),
                );
            }
        }
        Tool::Level => {
            let y = xf.to_screen(p).y;
            let r = painter.clip_rect();
            painter.add(Shape::dashed_line(&[pos2(r.left(), y), pos2(r.right(), y)], s, 8.0, 4.0));
            label_at(
                painter,
                xf.to_screen(p) + vec2(0.0, -14.0),
                &format!("{}  {}", app.doc.next_level_name(), hausit_core::format_length(p.y.round(), unit)),
            );
        }
        _ => {}
    }
}

// ----- context menu, view cube, view control bar ----------------------------

fn context_menu(app: &mut HausitApp, response: &egui::Response) {
    response.context_menu(|ui| {
        if app.tool != Tool::Modify {
            if ui.button("Cancel").clicked() {
                app.tool.cancel();
                app.status = app.tool.prompt().into();
                ui.close();
            }
            if app.tool.is_sketch() && ui.button("Finish Edit Mode").clicked() {
                tools::finish_sketch(app);
                ui.close();
            }
            ui.separator();
        }
        if !app.selection.is_empty() {
            if ui.button("Delete").clicked() {
                app.delete_selection();
                ui.close();
            }
            if ui.button("Move").clicked() {
                tools::start(app, Tool::Move { base: None });
                ui.close();
            }
            if ui.button("Copy").clicked() {
                tools::start(app, Tool::Copy { base: None });
                ui.close();
            }
            if ui.button("Select All Instances (in project)").clicked() {
                if let Some(t) = app.selection.first().and_then(|s| app.doc.element(*s)).and_then(|e| e.type_id()) {
                    app.selection = app.doc.elements().filter(|e| e.type_id() == Some(t)).map(|e| e.id).collect();
                }
                ui.close();
            }
            ui.separator();
        }
        if ui.button("Zoom To Fit").clicked() {
            if let Some(c) = app.active_view.and_then(|v| app.cameras.get_mut(&v)) {
                c.fitted = false;
            }
            ui.close();
        }
    });
}

/// Simplified ViewCube: preset orientations for the 3D view.
fn view_cube(app: &mut HausitApp, ui: &mut Ui, rect: Rect, view_id: ElementId) {
    let area = Rect::from_min_size(pos2(rect.right() - 168.0, rect.top() + 8.0), vec2(160.0, 84.0));
    let mut set = None;
    ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(240, 242, 245, 230))
            .stroke(Stroke::new(1.0, Color32::from_gray(180)))
            .inner_margin(4)
            .corner_radius(4)
            .show(ui, |ui| {
                egui::Grid::new("viewcube").spacing(vec2(2.0, 2.0)).show(ui, |ui| {
                    let mut b = |ui: &mut Ui, label: &str, yaw: f64, pitch: f64| {
                        if ui
                            .add(egui::Button::new(egui::RichText::new(label).size(11.0)).min_size(vec2(44.0, 18.0)))
                            .clicked()
                        {
                            set = Some((yaw, pitch));
                        }
                    };
                    b(ui, "Top", 0.0, 89.0);
                    b(ui, "Front", 0.0, 0.0);
                    b(ui, "Right", -90.0, 0.0);
                    ui.end_row();
                    b(ui, "Back", 180.0, 0.0);
                    b(ui, "Left", 90.0, 0.0);
                    b(ui, "⌂ Home", -45.0, 30.0);
                    ui.end_row();
                });
            });
    });
    if let Some((yaw, pitch)) = set {
        if let Some(v) = app.doc.view_mut(view_id) {
            v.camera.yaw = yaw;
            v.camera.pitch = pitch;
        }
        if let Some(c) = app.cameras.get_mut(&view_id) {
            c.fitted = false;
        }
    }
}

fn view_control_bar(app: &mut HausitApp, ui: &mut Ui, rect: Rect, view_id: ElementId) {
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        egui::Frame::new()
            .fill(crate::theme::TAB_BAR_BG)
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.set_min_size(rect.size());
                ui.horizontal(|ui| {
                    let Some(view) = app.doc.view(view_id).cloned() else { return };
                    let mut v = view.clone();
                    egui::ComboBox::from_id_salt("scale")
                        .selected_text(format!("1 : {}", v.scale))
                        .width(70.0)
                        .show_ui(ui, |ui| {
                            for s in [1, 5, 10, 20, 25, 50, 100, 200, 500, 1000] {
                                ui.selectable_value(&mut v.scale, s, format!("1 : {s}"));
                            }
                        });
                    egui::ComboBox::from_id_salt("detail")
                        .selected_text(v.detail_level.label())
                        .width(70.0)
                        .show_ui(ui, |ui| {
                            for d in DetailLevel::ALL {
                                ui.selectable_value(&mut v.detail_level, d, d.label());
                            }
                        });
                    if !matches!(v.kind, ViewKind::FloorPlan { .. }) {
                        egui::ComboBox::from_id_salt("style")
                            .selected_text(v.visual_style.label())
                            .width(110.0)
                            .show_ui(ui, |ui| {
                                for s in VisualStyle::ALL {
                                    ui.selectable_value(&mut v.visual_style, s, s.label());
                                }
                            });
                    }
                    if ui.button("⛶ Zoom to Fit").on_hover_text("Zoom To Fit (ZF)").clicked() {
                        if let Some(c) = app.cameras.get_mut(&view_id) {
                            c.fitted = false;
                        }
                    }
                    if v != view {
                        if let Some(target) = app.doc.view_mut(view_id) {
                            *target = v;
                        }
                    }
                    if matches!(view.kind, ViewKind::ThreeD) {
                        ui.label(
                            egui::RichText::new("Orbit: right-drag or Shift+middle-drag · Pan: middle-drag · Zoom: wheel")
                                .color(Color32::GRAY)
                                .size(11.0),
                        );
                    }
                });
            });
    });
}
