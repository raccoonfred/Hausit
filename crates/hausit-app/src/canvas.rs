//! The drawing area: view tabs, painting display lists, pan/zoom/orbit,
//! selection, tool input and previews, and the view control bar.

use crate::app::{Camera2D, HausitApp};
use crate::theme::{c32, CANVAS_BG, HOVER, PREVIEW, SELECT, SKETCH, SNAP};
use crate::tools::{self, DrawShape, SnapKind, Tool};
use egui::{pos2, vec2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui};
use hausit_core::geom::{distance_to_segment, perp, point_in_polygon, triangulate};
use hausit_core::{DVec2, ElementId};
use hausit_model::{Category, DetailLevel, ElementData, ViewKind, VisualStyle};
use hausit_views::{Anchor, Dash, DisplayList, Item, Prim};

/// Screen ⇄ view-coordinate transform for one frame.
#[derive(Clone, Copy)]
pub struct Xform {
    pub rect: Rect,
    pub cam: Camera2D,
    /// Multiplies line widths. 1 in model views; on sheets, lines are
    /// drawn at their printed width, so they thin out as you zoom out.
    pub line_scale: f32,
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
    let is_sheet = matches!(view.kind, ViewKind::Sheet(_));
    // Sheets sit on a grey desk, so the paper's edge shows.
    painter.rect_filled(rect, 0.0, if is_sheet { Color32::from_gray(200) } else { CANVAS_BG });

    let is_3d = matches!(view.kind, ViewKind::ThreeD);
    let is_plan = view.kind.plan_level().is_some();

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
    let cam = app.cameras[&view_id];
    let xf = Xform {
        rect,
        cam,
        line_scale: if is_sheet {
            (cam.zoom as f32 / hausit_views::PX_PER_MM as f32).clamp(0.15, 4.0)
        } else {
            1.0
        },
    };
    let dl = app.active_graphics().cloned().unwrap_or_default();

    paint_list(&painter, &xf, &dl, None);

    // Hover / pre-selection.
    let pointer = response.hover_pos();
    app.hover = None;
    app.options.pick_tol = xf.px(8.0);
    let picking = matches!(
        app.tool,
        Tool::Modify
            | Tool::Mirror { draw: false, .. }
            | Tool::Trim { .. }
            | Tool::Split
            | Tool::CurtainGrid
            | Tool::Attach { .. }
            | Tool::Tag
            | Tool::Demolish
            | Tool::Align { .. }
            | Tool::Offset
    );
    if let (Some(p), true) = (pointer, picking) {
        // Tab cycles through overlapping elements, as in Revit.
        let mut candidates = hit_candidates(&dl, &xf, p);
        candidates.retain(|id| {
            app.doc
                .element(*id)
                .map(|e| app.tool.picks(e.category()))
                .unwrap_or(app.tool == Tool::Modify)
        });
        // Model elements win over datums and view marks drawn over them.
        candidates.sort_by_key(|id| match app.doc.element(*id).map(|e| e.category()) {
            Some(Category::Grids | Category::Levels) => 1,
            None => 2,
            _ => 0,
        });
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
    let mut snap: Option<tools::Snap> = None;
    if let Some(p) = raw {
        let mode = if app.tool.is_sketch() {
            app.options.sketch_mode
        } else {
            app.options.shape
        };
        let drawing = !matches!(
            app.tool,
            Tool::Modify
                | Tool::Door
                | Tool::Window
                | Tool::Room
                | Tool::Dimension { .. }
                | Tool::Trim { .. }
                | Tool::Split
                | Tool::Attach { .. }
                | Tool::Mirror { draw: false, .. }
        );
        if is_plan && drawing && !(app.tool.is_sketch() && mode == DrawShape::PickWalls) {
            snap = tools::snap(app, p, xf.px(10.0));
            let weak = snap
                .as_ref()
                .map(|s| matches!(s.kind, SnapKind::Nearest | SnapKind::Alignment))
                .unwrap_or(true);
            let free_angle = (mode == DrawShape::Rectangle
                && matches!(app.tool, Tool::Wall { .. } | Tool::Floor { .. } | Tool::Roof { .. }))
                || (mode == DrawShape::Arc && app.options.arc_end.is_some());
            if let (Some(anchor), true, false) = (app.tool.anchor(), weak, free_angle) {
                if let Some(c) = tools::constrain(anchor, p, xf.px(1.0), shift) {
                    snap = Some(c);
                }
            }
            snapped = snap.as_ref().map(|s| s.point).or(raw);
        }
    }

    let flipped = is_plan && flip_controls(app, &response, &painter, &xf, pointer);
    if response.clicked_by(egui::PointerButton::Primary) && !flipped {
        if let Some(p) = snapped {
            if app.tool == Tool::Modify {
                select_click(app, app.hover, ctrl, shift);
            } else if app.tool.is_sketch()
                || is_plan
                || matches!(
                    app.tool,
                    Tool::Level
                        | Tool::Attach { .. }
                        | Tool::Callout { .. }
                        | Tool::CurtainGrid
                        | Tool::DetailLine { .. }
                        | Tool::FilledRegion { .. }
                        | Tool::SpotElevation { .. }
                )
            {
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

    // Dragging selected elements and wall end grips, then window /
    // crossing selection.
    if !is_3d && !is_sheet {
        crop_region(app, ui, &response, &painter, &xf, view_id);
    }
    if is_plan {
        direct_drag(app, ui, &response, &painter, &dl, &xf);
    }
    if is_3d {
        section_box_grips(app, ui, &response, &painter, &xf, view_id);
    }
    if is_sheet {
        viewport_drag(app, ui, &response, &painter, &dl, &xf, view_id);
    }
    box_select(app, ui, &response, &painter, &dl, &xf);

    // Previews.
    if let Some(p) = snapped {
        preview(app, &painter, &xf, p);
        if let Some(s) = &snap {
            paint_snap(&painter, &xf, s);
        }
    }
    if let Some(p) = pointer {
        if app.tool != Tool::Modify && is_plan {
            let cursor = egui::CursorIcon::Crosshair;
            ui.ctx().set_cursor_icon(cursor);
            let _ = p;
        }
    }

    if is_plan {
        paint_sketch(app, &painter, &xf);
    }
    if response.secondary_clicked() {
        if let Some(p) = raw {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("context_menu_at"), p));
        }
    }
    context_menu(app, &response);
    if is_3d {
        crate::viewcube::show(app, ui, rect, view_id);
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
                                // Plans and ceiling plans share their level's name.
                                let label = match v.kind {
                                    ViewKind::CeilingPlan { .. } => format!("{} (Ceiling)", v.name),
                                    _ => v.name.clone(),
                                };
                                let r = ui.add(egui::Label::new(label).sense(Sense::click()));
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
            let xf = Xform {
                rect,
                cam: *cam,
                line_scale: 1.0,
            };
            let before = xf.to_view(p);
            cam.zoom = (cam.zoom * factor).clamp(1e-4, 50.0);
            let xf = Xform {
                rect,
                cam: *cam,
                line_scale: 1.0,
            };
            cam.center += before - xf.to_view(p);
        }
    }
    let delta = response.drag_delta();
    let orbit =
        is_3d && (response.dragged_by(egui::PointerButton::Secondary) || (shift && response.dragged_by(egui::PointerButton::Middle)));
    if orbit {
        // Orbit about the model's centre, keeping it where it is on
        // screen (Revit orbits about the model or selection).
        let pivot = hausit_model::geometry::model_bounds(&app.doc).map(|b| b.center());
        let at = |app: &HausitApp| {
            let v = app.doc.view(view_id)?;
            let f = hausit_views::ViewFrame::for_view(&app.doc, v)?;
            Some(f.to_view(pivot?).0)
        };
        let before = at(app);
        if let Some(v) = app.doc.view_mut(view_id) {
            v.camera.yaw += delta.x as f64 * 0.4;
            v.camera.pitch = (v.camera.pitch + delta.y as f64 * 0.4).clamp(-89.0, 89.0);
        }
        if let (Some(b), Some(a)) = (before, at(app)) {
            app.cameras.get_mut(&view_id).unwrap().center += a - b;
        }
    } else if response.dragged_by(egui::PointerButton::Middle) || response.dragged_by(egui::PointerButton::Secondary) {
        cam.center -= DVec2::new(delta.x as f64, -delta.y as f64) / cam.zoom;
    }
}

// ----- painting -------------------------------------------------------------

fn dash_shapes(pts: &[Pos2], stroke: Stroke, dash: Dash, out: &mut Vec<Shape>) {
    match dash {
        // Separate segments: a joined path mitres sharp turns, and the
        // near-reversals of thin slivers seen edge-on became huge spikes.
        Dash::Solid => out.extend(pts.windows(2).map(|w| Shape::line_segment([w[0], w[1]], stroke))),
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
                item.stroke
                    .map(|s| Stroke::new(s.width * xf.line_scale + 1.0, c))
                    .or(Some(Stroke::new(1.5, c))),
                item.fill.map(|_| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 50)),
            ),
            None => (
                item.stroke.map(|s| Stroke::new((s.width * xf.line_scale).max(0.5), c32(s.color))),
                item.fill.map(c32),
            ),
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
            Prim::Text {
                pos,
                text,
                height,
                anchor,
                angle,
            } => {
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
                texts.push((xf.to_screen(*pos), align, text, px, -*angle as f32));
            }
        }
    }
    painter.extend(shapes);
    // Text last, so it sits on top of bubble and tag fills.
    for (pos, align, text, px, angle) in texts {
        if angle.abs() < 1e-4 {
            painter.text(pos, align, text, FontId::proportional(px), Color32::BLACK);
        } else {
            // Rotated text (vertical dimensions) is centred on `pos`.
            let galley = painter.layout_no_wrap(text.clone(), FontId::proportional(px), Color32::BLACK);
            let half = galley.size() / 2.0;
            let corner = pos - egui::emath::Rot2::from_angle(angle) * half;
            painter.add(egui::epaint::TextShape::new(corner, galley, Color32::BLACK).with_angle(angle));
        }
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
    if ui.ctx().data(|d| {
        d.get_temp::<DirectDrag>(egui::Id::unique("direct_drag")).is_some()
            || d.get_temp::<(usize, f32)>(egui::Id::unique("box_grip")).is_some()
            || d.get_temp::<DVec2>(egui::Id::unique("viewport_drag")).is_some()
            || d.get_temp::<usize>(egui::Id::unique("crop_edge")).is_some()
    }) {
        return;
    }
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

/// The section box of a 3D view: its edges, and an arrow on each face that
/// drags that face in or out, as in Revit.
fn section_box_grips(app: &mut HausitApp, ui: &Ui, response: &egui::Response, painter: &egui::Painter, xf: &Xform, view_id: ElementId) {
    let Some(view) = app.doc.view(view_id) else { return };
    let Some(b) = view.section_box else { return };
    let Some(frame) = hausit_views::ViewFrame::for_view(&app.doc, view) else {
        return;
    };
    let scr = |p: glam::DVec3| xf.to_screen(frame.to_view(p).0);
    let blue = Color32::from_rgb(40, 110, 230);
    let corner = |i: usize| {
        let pick = |bit: usize, lo: f64, hi: f64| if i & bit == 0 { lo } else { hi };
        b.to_model(glam::DVec3::new(
            pick(1, b.min.x, b.max.x),
            pick(2, b.min.y, b.max.y),
            pick(4, b.min.z, b.max.z),
        ))
    };
    for i in 0..8 {
        for bit in [1, 2, 4] {
            if i & bit == 0 {
                painter.line_segment([scr(corner(i)), scr(corner(i | bit))], Stroke::new(1.0, blue));
            }
        }
    }
    // Face centres and outward directions, in the box's local axes.
    let mid = (b.min + b.max) * 0.5;
    let faces: Vec<(glam::DVec3, glam::DVec3)> = (0..6)
        .map(|k| {
            let axis = k / 2;
            let mut c = mid;
            let mut n = glam::DVec3::ZERO;
            c[axis] = if k % 2 == 0 { b.min[axis] } else { b.max[axis] };
            n[axis] = if k % 2 == 0 { -1.0 } else { 1.0 };
            (c, n)
        })
        .collect();
    let mut grips = Vec::new();
    for (k, (c, n)) in faces.iter().enumerate() {
        let p = scr(b.to_model(*c));
        let q = scr(b.to_model(*c + *n * 1000.0));
        let d = q - p;
        if d.length() < 4.0 {
            continue; // faces the camera: no arrow to drag
        }
        let u = d.normalized();
        let side = vec2(-u.y, u.x);
        painter.add(Shape::convex_polygon(
            vec![p + u * 14.0, p + side * 6.0, p - side * 6.0],
            blue,
            Stroke::new(1.0, Color32::WHITE),
        ));
        grips.push((k, p, u, d.length() / 1000.0));
    }
    let id = egui::Id::unique("box_grip");
    if response.drag_started_by(egui::PointerButton::Primary) {
        if let Some(sp) = ui.input(|i| i.pointer.press_origin()) {
            if let Some(g) = grips.iter().find(|g| (g.1 + g.2 * 6.0).distance(sp) < 12.0) {
                ui.ctx().data_mut(|d| d.insert_temp(id, (g.0, 0.0f32)));
            }
        }
    }
    let Some((k, _)) = ui.ctx().data(|d| d.get_temp::<(usize, f32)>(id)) else {
        return;
    };
    let Some(&(_, p, u, px_per_mm)) = grips.iter().find(|g| g.0 == k) else {
        ui.ctx().data_mut(|d| d.remove::<(usize, f32)>(id));
        return;
    };
    let (Some(start), Some(cur)) = (ui.input(|i| i.pointer.press_origin()), ui.input(|i| i.pointer.latest_pos())) else {
        return;
    };
    let mm = ((cur - start).dot(u) / px_per_mm) as f64;
    painter.line_segment([p, p + u * (cur - start).dot(u)], Stroke::new(2.0, PREVIEW));
    label_at(
        painter,
        p + u * (cur - start).dot(u) + vec2(0.0, -14.0),
        &hausit_core::format_length(mm, app.doc.length_unit),
    );
    if response.drag_stopped() {
        ui.ctx().data_mut(|d| d.remove::<(usize, f32)>(id));
        let mut nb = b;
        let axis = k / 2;
        if k % 2 == 0 {
            nb.min[axis] = (b.min[axis] - mm).min(b.max[axis] - 100.0);
        } else {
            nb.max[axis] = (b.max[axis] + mm).max(b.min[axis] + 100.0);
        }
        app.transact("Resize Section Box", |doc| {
            if let Some(v) = doc.view_mut(view_id) {
                v.section_box = Some(nb);
            }
            Ok(())
        });
    }
}

/// On a sheet, dragging a placed view moves it on the paper.
fn viewport_drag(
    app: &mut HausitApp,
    ui: &Ui,
    response: &egui::Response,
    painter: &egui::Painter,
    dl: &DisplayList,
    xf: &Xform,
    sheet: ElementId,
) {
    if app.tool != Tool::Modify {
        return;
    }
    let id = egui::Id::unique("viewport_drag");
    let on_sheet = |app: &HausitApp, v: ElementId| match app.doc.view(sheet).map(|s| &s.kind) {
        Some(ViewKind::Sheet(def)) => def.viewports.iter().any(|p| p.view == v),
        _ => false,
    };
    if response.drag_started_by(egui::PointerButton::Primary) {
        if let (Some(sp), Some(h)) = (ui.input(|i| i.pointer.press_origin()), app.hover) {
            if on_sheet(app, h) {
                if !app.selection.contains(&h) {
                    app.selection = vec![h];
                }
                ui.ctx().data_mut(|d| d.insert_temp(id, xf.to_view(sp)));
            }
        }
    }
    let Some(from) = ui.ctx().data(|d| d.get_temp::<DVec2>(id)) else {
        return;
    };
    let Some(cur) = ui.input(|i| i.pointer.latest_pos()).map(|p| xf.to_view(p)) else {
        return;
    };
    let moved = Xform {
        rect: xf.rect,
        line_scale: xf.line_scale,
        cam: Camera2D {
            center: xf.cam.center - (cur - from),
            ..xf.cam
        },
    };
    paint_list(painter, &moved, dl, Some((&app.selection, PREVIEW)));
    if response.drag_stopped() {
        ui.ctx().data_mut(|d| d.remove::<DVec2>(id));
        let views: Vec<ElementId> = app.selection.iter().copied().filter(|v| on_sheet(app, *v)).collect();
        if cur.distance(from) > xf.px(2.0) {
            app.transact("Move Viewport", |doc| {
                for v in &views {
                    doc.move_viewport(sheet, *v, cur - from);
                }
                Ok(())
            });
        }
    }
}

/// A drag on the model in Modify: moving the selection, or a wall end.
#[derive(Clone, Copy)]
enum DirectDrag {
    Move { from: DVec2 },
    WallEnd { wall: ElementId, end: bool },
}

/// A flip control: what clicking it flips.
#[derive(Clone, Copy, PartialEq)]
enum Flip {
    Wall,
    Hand,
    Facing,
}

/// The flip controls of the selected element in plan: the double arrow
/// on a wall's exterior side, and a door or window's hand and facing
/// arrows. Returns true when this click hit one.
fn flip_controls(app: &mut HausitApp, response: &egui::Response, painter: &egui::Painter, xf: &Xform, pointer: Option<Pos2>) -> bool {
    if app.tool != Tool::Modify || app.selection.len() != 1 {
        return false;
    }
    let id = app.selection[0];
    let mut controls: Vec<(Flip, Pos2, egui::Vec2)> = Vec::new();
    let screen_dir = |d: DVec2| {
        let v = xf.to_screen(d) - xf.to_screen(DVec2::ZERO);
        v.normalized()
    };
    match app.doc.element(id).map(|e| &e.data) {
        Some(ElementData::Wall(w)) => {
            let t = app.doc.type_of(id).map(|t| t.thickness()).unwrap_or(200.0) / 2.0;
            let n = perp(w.direction());
            let mid = (w.start + w.end) * 0.5;
            let c = xf.to_screen(mid + n * t) + screen_dir(n) * 16.0;
            controls.push((Flip::Wall, c, screen_dir(n)));
        }
        Some(ElementData::Door(o) | ElementData::Window(o)) => {
            let Some(w) = app.doc.element(o.host).and_then(|e| e.as_wall()) else {
                return false;
            };
            let t = app.doc.type_of(o.host).map(|t| t.thickness()).unwrap_or(200.0) / 2.0;
            let width = app.doc.type_of(id).map(|t| t.width()).unwrap_or(900.0);
            let (d, n) = (w.direction(), perp(w.direction()));
            let c = w.point_at(o.offset);
            let side = if o.flip_facing { n } else { -n };
            controls.push((Flip::Facing, xf.to_screen(c + side * t) + screen_dir(side) * 16.0, screen_dir(n)));
            let hinge = if o.flip_hand { d } else { -d };
            controls.push((
                Flip::Hand,
                xf.to_screen(c + hinge * width / 2.0 + side * t) + screen_dir(side) * 16.0,
                screen_dir(d),
            ));
        }
        _ => return false,
    }
    let blue = Color32::from_rgb(40, 110, 230);
    for (_, c, u) in &controls {
        double_arrow(painter, *c, *u, blue);
    }
    if !response.clicked_by(egui::PointerButton::Primary) {
        return false;
    }
    let Some(sp) = pointer else { return false };
    let Some((flip, ..)) = controls.iter().find(|(_, c, _)| c.distance(sp) < 10.0).copied() else {
        return false;
    };
    match flip {
        Flip::Wall => app.transact("Flip Wall", |doc| doc.flip_wall(id)),
        Flip::Hand => app.transact("Flip Hand", |doc| doc.flip_opening(id, true)),
        Flip::Facing => app.transact("Flip Facing", |doc| doc.flip_opening(id, false)),
    };
    true
}

/// A small double-headed arrow along `u`, centred at `c`.
fn double_arrow(painter: &egui::Painter, c: Pos2, u: egui::Vec2, color: Color32) {
    let v = egui::vec2(-u.y, u.x);
    let (a, b) = (c - u * 8.0, c + u * 8.0);
    painter.circle_filled(c, 10.0, Color32::from_white_alpha(200));
    painter.line_segment([a, b], Stroke::new(1.5, color));
    for (tip, dir) in [(a, -u), (b, u)] {
        let base = tip - dir * 4.5;
        painter.add(Shape::convex_polygon(
            vec![tip, base + v * 3.5, base - v * 3.5],
            color,
            Stroke::NONE,
        ));
    }
}

/// Revit-style direct editing in plan: selected walls show blue end
/// grips that drag their ends; dragging a selected (or hovered) element
/// moves it, with snapping.
fn direct_drag(app: &mut HausitApp, ui: &Ui, response: &egui::Response, painter: &egui::Painter, dl: &DisplayList, xf: &Xform) {
    if app.tool != Tool::Modify {
        return;
    }
    let id = egui::Id::unique("direct_drag");
    let grip_fill = Color32::from_rgb(40, 110, 230);
    let grips: Vec<(ElementId, bool, DVec2)> = app
        .selection
        .iter()
        .filter_map(|s| app.doc.element(*s).and_then(|e| e.as_wall()).map(|w| (*s, w.start, w.end)))
        .flat_map(|(s, a, b)| [(s, false, a), (s, true, b)])
        .collect();
    for (_, _, p) in &grips {
        let c = xf.to_screen(*p);
        painter.circle(c, 4.5, grip_fill, Stroke::new(1.0, Color32::WHITE));
    }

    let cropping = ui.ctx().data(|d| d.get_temp::<usize>(egui::Id::unique("crop_edge")).is_some());
    if response.drag_started_by(egui::PointerButton::Primary) && !cropping {
        // Where the button went down, not where the drag threshold was crossed.
        if let Some(sp) = ui.input(|i| i.pointer.press_origin()) {
            let grip = grips.iter().find(|g| xf.to_screen(g.2).distance(sp) < 8.0);
            let op = match (grip, app.hover) {
                (Some(g), _) => Some(DirectDrag::WallEnd { wall: g.0, end: g.1 }),
                (None, Some(h)) if app.doc.element(h).is_some() => {
                    if !app.selection.contains(&h) {
                        app.selection = vec![h];
                    }
                    Some(DirectDrag::Move { from: xf.to_view(sp) })
                }
                _ => None,
            };
            if let Some(op) = op {
                ui.ctx().data_mut(|d| d.insert_temp(id, op));
            }
        }
    }
    let Some(op) = ui.ctx().data(|d| d.get_temp::<DirectDrag>(id)) else {
        return;
    };
    let Some(cur) = ui.input(|i| i.pointer.latest_pos()).map(|p| xf.to_view(p)) else {
        return;
    };
    let snap = tools::snap(app, cur, xf.px(10.0));
    let target = snap.as_ref().map(|s| s.point).unwrap_or(cur);
    if let Some(s) = &snap {
        paint_snap(painter, xf, s);
    }
    match op {
        DirectDrag::Move { from } => {
            let moved = Xform {
                rect: xf.rect,
                line_scale: xf.line_scale,
                cam: Camera2D {
                    center: xf.cam.center - (target - from),
                    ..xf.cam
                },
            };
            paint_list(painter, &moved, dl, Some((&app.selection, PREVIEW)));
        }
        DirectDrag::WallEnd { wall, end } => {
            if let Some(w) = app.doc.element(wall).and_then(|e| e.as_wall()) {
                let fixed = if end { w.start } else { w.end };
                painter.line_segment([xf.to_screen(fixed), xf.to_screen(target)], Stroke::new(2.0, PREVIEW));
                label_at(
                    painter,
                    xf.to_screen((fixed + target) * 0.5) + vec2(0.0, -14.0),
                    &hausit_core::format_length(fixed.distance(target), app.doc.length_unit),
                );
            }
        }
    }
    if response.drag_stopped() {
        ui.ctx().data_mut(|d| d.remove::<DirectDrag>(id));
        match op {
            DirectDrag::Move { from } if target.distance(from) > xf.px(2.0) => {
                let ids = app.selection.clone();
                app.transact("Drag", |doc| {
                    doc.check_unpinned(&ids)?;
                    doc.translate(&ids, target - from);
                    Ok(())
                });
            }
            DirectDrag::WallEnd { wall, end } => {
                app.transact("Drag Wall End", |doc| {
                    if let Some(hausit_model::ElementData::Wall(w)) = doc.element_mut(wall).map(|e| &mut e.data) {
                        let fixed = if end { w.start } else { w.end };
                        if fixed.distance(target) > 10.0 {
                            if end {
                                w.end = target;
                            } else {
                                w.start = target;
                            }
                        }
                    }
                    doc.touch();
                    Ok(())
                });
            }
            _ => {}
        }
    }
}

// ----- previews -------------------------------------------------------------

fn paint_snap(painter: &egui::Painter, xf: &Xform, snap: &tools::Snap) {
    let guide = Stroke::new(1.0, Color32::from_rgb(70, 130, 220));
    for (a, b) in &snap.guides {
        painter.extend(Shape::dashed_line(&[xf.to_screen(*a), xf.to_screen(*b)], guide, 6.0, 4.0));
    }
    let p = xf.to_screen(snap.point);
    let s = Stroke::new(1.6, SNAP);
    let r = 5.0;
    match snap.kind {
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
        SnapKind::Perpendicular => {
            painter.line_segment([p + vec2(-r, r), p + vec2(r, r)], s);
            painter.line_segment([p + vec2(0.0, r), p + vec2(0.0, -r)], s);
        }
        SnapKind::Nearest => {
            painter.add(Shape::closed_line(
                vec![p + vec2(-r, -r), p + vec2(r, r), p + vec2(-r, r), p + vec2(r, -r)],
                s,
            ));
        }
        SnapKind::Alignment | SnapKind::Increment => {}
    }
    // Revit names the snap in a tooltip by the cursor.
    let label = snap.kind.label();
    if !label.is_empty() {
        let galley = painter.layout_no_wrap(label.to_string(), FontId::proportional(11.5), Color32::from_gray(30));
        let r = Rect::from_min_size(p + vec2(14.0, 12.0), galley.size() + vec2(8.0, 4.0));
        painter.rect(
            r,
            1.0,
            Color32::from_rgb(255, 255, 225),
            Stroke::new(1.0, Color32::from_gray(120)),
            egui::StrokeKind::Inside,
        );
        painter.galley(r.min + vec2(4.0, 2.0), galley, Color32::from_gray(30));
    }
}

/// Sketch lines in magenta, like Revit's sketch mode, with the slope
/// symbol on roof lines that define slope.
fn paint_sketch(app: &HausitApp, painter: &egui::Painter, xf: &Xform) {
    let Some(sk) = app.tool.sketch() else { return };
    let roof = matches!(app.tool, Tool::Roof { .. });
    let magenta = Stroke::new(2.0, SKETCH);
    // Picked walls show at their overhang, trimmed at the corners once the
    // loop closes.
    let shown: Vec<(DVec2, DVec2, bool)> = match sk.closed_loop() {
        Ok(c) if c.iter().any(|l| l.1.overhang != 0.0) => {
            let pts: Vec<DVec2> = c.iter().map(|l| l.0).collect();
            let off = tools::offset_outwards(&pts, &c.iter().map(|l| l.1.overhang).collect::<Vec<_>>());
            (0..off.len()).map(|i| (off[i], off[(i + 1) % off.len()], c[i].1.slope)).collect()
        }
        _ => sk
            .lines
            .iter()
            .map(|l| {
                let d = (l.b - l.a).normalize_or_zero();
                let n = DVec2::new(-d.y, d.x) * l.overhang;
                (l.a + n, l.b + n, l.slope)
            })
            .collect(),
    };
    for (a, b, slope) in shown {
        let (a, b) = (xf.to_screen(a), xf.to_screen(b));
        painter.line_segment([a, b], magenta);
        if roof && slope && a.distance(b) > 30.0 {
            let d = (b - a).normalized();
            let n = vec2(d.y, -d.x);
            let m = a + (b - a) * 0.5 + n * 10.0;
            let tri = vec![m - d * 7.0, m + d * 7.0, m + d * 7.0 + n * 7.0];
            painter.add(Shape::closed_line(tri, Stroke::new(1.2, SKETCH)));
        }
    }
    if let Some((t, h)) = sk.arrow {
        slope_arrow(painter, xf.to_screen(t), xf.to_screen(h), SKETCH);
    }
}

/// A slope arrow: a line from tail to head with a filled arrowhead.
fn slope_arrow(painter: &egui::Painter, tail: Pos2, head: Pos2, color: Color32) {
    painter.line_segment([tail, head], Stroke::new(1.5, color));
    if tail.distance(head) < 1.0 {
        return;
    }
    let d = (head - tail).normalized();
    let n = vec2(-d.y, d.x);
    let base = head - d * 12.0;
    painter.add(Shape::convex_polygon(
        vec![head, base + n * 5.0, base - n * 5.0],
        color,
        Stroke::NONE,
    ));
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
        // Angle from horizontal, as Revit's listening dimensions show.
        let d = b - a;
        if d.length() > xf.px(40.0) {
            let ang = d.y.atan2(d.x).to_degrees();
            let at = xf.to_screen(a) + vec2(36.0, if d.y > 0.0 { 16.0 } else { -16.0 });
            label_at(painter, at, &format!("{:.2}°", ang.abs()));
        }
    };
    let rect_pts = |a: DVec2, b: DVec2| {
        let (min, max) = (a.min(b), a.max(b));
        vec![min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)]
    };
    match &app.tool {
        Tool::Wall { start: Some(a), .. } if app.options.shape == DrawShape::Arc => {
            let pts = match app.options.arc_end.and_then(|b| hausit_model::ArcCurve::through(*a, p, b)) {
                Some(arc) => arc.points(hausit_model::ARC_STEP / 2.0),
                None => vec![*a, app.options.arc_end.unwrap_or(p)],
            };
            let sp: Vec<Pos2> = pts.iter().map(|q| xf.to_screen(*q)).collect();
            painter.add(Shape::line(sp, s));
            if let Some(arc) = app.options.arc_end.and_then(|b| hausit_model::ArcCurve::through(*a, p, b)) {
                label_at(
                    painter,
                    xf.to_screen(p) + vec2(40.0, 14.0),
                    &format!("R {}", hausit_core::format_length(arc.radius, unit)),
                );
            }
        }
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
        Tool::Callout { start: Some(a) } => {
            let r = Rect::from_two_pos(xf.to_screen(*a), xf.to_screen(p));
            painter.rect_stroke(r, 6.0, Stroke::new(1.5, PREVIEW), egui::StrokeKind::Middle);
        }
        Tool::Grid { start: Some(a) }
        | Tool::Section { start: Some(a) }
        | Tool::RoomSeparation { start: Some(a) }
        | Tool::Beam { start: Some(a), .. } => seg(*a, p),
        Tool::Mirror {
            draw: true,
            start: Some(a),
        } => {
            let d = (p - *a).normalize_or_zero() * xf.px(4000.0);
            painter.add(Shape::dashed_line(
                &[xf.to_screen(*a - d), xf.to_screen(p + d)],
                Stroke::new(1.0, PREVIEW),
                10.0,
                4.0,
            ));
        }
        Tool::Room => {
            let Some(level) = app.active_level() else { return };
            let segs = hausit_model::rooms::bounding_segments(&app.doc, level);
            let region = hausit_model::rooms::regions(&segs)
                .into_iter()
                .filter(|f| hausit_core::geom::point_in_polygon(p, f))
                .min_by(|a, b| hausit_core::geom::polygon_area(a).total_cmp(&hausit_core::geom::polygon_area(b)));
            match region {
                Some(r) => {
                    let sp: Vec<Pos2> = r.iter().map(|q| xf.to_screen(*q)).collect();
                    let mut shapes = Vec::new();
                    fill_polygon(&sp, Color32::from_rgba_unmultiplied(0, 120, 230, 40), &mut shapes);
                    painter.extend(shapes);
                    painter.add(Shape::closed_line(sp, Stroke::new(2.0, PREVIEW)));
                    let area = hausit_core::geom::polygon_area(&r).abs() / 1e6;
                    label_at(painter, xf.to_screen(p) + vec2(0.0, -18.0), &format!("Room  {area:.2} m²"));
                }
                None => label_at(painter, xf.to_screen(p) + vec2(0.0, -18.0), "Not enclosed"),
            }
        }
        Tool::Stair { pts } if !pts.is_empty() => {
            let mut path = pts.clone();
            path.push(p);
            if let Some(l) = tools::stair_preview(app, &path) {
                for piece in &l.pieces {
                    let r: Vec<Pos2> = piece.outline.iter().map(|q| xf.to_screen(*q)).collect();
                    painter.add(Shape::closed_line(r, Stroke::new(1.0, PREVIEW)));
                }
                let line: Vec<Pos2> = l.path.iter().map(|q| xf.to_screen(*q)).collect();
                painter.add(Shape::line(line, Stroke::new(1.5, PREVIEW)));
                label_at(
                    painter,
                    xf.to_screen(p) + vec2(0.0, -18.0),
                    &format!("{} risers created, {} remaining", l.placed, l.risers - l.placed),
                );
            }
        }
        Tool::Ceiling => {
            if let Some(r) = tools::ceiling_region(app, p) {
                let sp: Vec<Pos2> = r.iter().map(|q| xf.to_screen(*q)).collect();
                painter.add(Shape::closed_line(sp, Stroke::new(2.0, PREVIEW)));
            }
        }
        Tool::Railing { pts } if !pts.is_empty() => {
            let mut line: Vec<Pos2> = pts.iter().map(|q| xf.to_screen(*q)).collect();
            line.push(xf.to_screen(p));
            painter.add(Shape::line(line, Stroke::new(1.5, PREVIEW)));
        }
        Tool::Component { category } => {
            let category = *category;
            let Some(type_id) = app.type_for(category) else { return };
            let c = hausit_model::Component {
                type_id,
                category,
                level: app.active_level().unwrap_or(ElementId::INVALID),
                point: p,
                rotation: app.options.component_rotation,
                offset: 0.0,
            };
            let lines: Vec<(DVec2, DVec2)> = match hausit_model::component::component_plan(&app.doc, &c) {
                Some(lines) => lines,
                None => hausit_model::component::component_parts(&app.doc, &c)
                    .iter()
                    .filter_map(|part| part.vertical_extent())
                    .flat_map(|(o, _, _)| {
                        let n = o.len();
                        (0..n).map(move |i| (o[i], o[(i + 1) % n])).collect::<Vec<_>>()
                    })
                    .collect(),
            };
            for (a, b) in lines {
                painter.line_segment([xf.to_screen(a), xf.to_screen(b)], Stroke::new(1.5, PREVIEW));
            }
        }
        Tool::Column | Tool::Footing => {
            let cat = app.tool.category().unwrap_or(Category::StructuralColumns);
            let Some(t) = app.type_for(cat).and_then(|t| app.doc.family_type(t)) else {
                return;
            };
            let outline: Vec<DVec2> = if cat == Category::StructuralFoundations {
                let (b, h) = (t.length_or("Width", 1200.0) / 2.0, t.length_or("Length", 1200.0) / 2.0);
                vec![DVec2::new(-b, -h), DVec2::new(b, -h), DVec2::new(b, h), DVec2::new(-b, h)]
            } else {
                hausit_model::geometry::section_profile(Some(t))
            };
            let rot = |v: DVec2| if app.options.flip { DVec2::new(-v.y, v.x) } else { v };
            let r: Vec<Pos2> = outline.iter().map(|v| xf.to_screen(p + rot(*v))).collect();
            let mut shapes = Vec::new();
            fill_polygon(&r, Color32::from_rgba_unmultiplied(0, 120, 230, 60), &mut shapes);
            painter.extend(shapes);
            painter.add(Shape::closed_line(r, s));
        }
        Tool::DetailLine { start: Some(a) } => seg(*a, p),
        Tool::FilledRegion { pts } if !pts.is_empty() => {
            let mut sp: Vec<Pos2> = pts.iter().map(|q| xf.to_screen(*q)).collect();
            sp.push(xf.to_screen(p));
            painter.add(Shape::line(sp, Stroke::new(1.5, SKETCH)));
            painter.circle_stroke(xf.to_screen(pts[0]), 5.0, Stroke::new(1.0, SKETCH));
        }
        Tool::SpotElevation { point: Some(a) } => seg(*a, p),
        Tool::LinearDimension { pts } => {
            for q in pts {
                painter.circle_filled(xf.to_screen(*q), 3.5, SELECT);
            }
            if pts.len() == 2 {
                let view = app.active_view.unwrap_or(ElementId::INVALID);
                let d = hausit_model::Dimension {
                    view,
                    refs: pts.iter().map(|q| hausit_model::DimRef::Point { point: *q }).collect(),
                    line_point: p,
                    measure: Some(tools::linear_direction(pts[0], pts[1], p)),
                };
                if let Some(l) = hausit_model::dimensions::layout(&app.doc, &d) {
                    let scale = app.doc.view(view).map(|v| v.scale as f64).unwrap_or(100.0);
                    let mut dl = DisplayList::default();
                    hausit_views::draw_dimension(&mut dl, &l, scale, unit, None);
                    paint_list(painter, xf, &dl, None);
                }
            } else if let Some(a) = pts.first() {
                painter.line_segment([xf.to_screen(*a), xf.to_screen(p)], Stroke::new(1.0, PREVIEW));
            }
        }
        Tool::Dimension { refs } => {
            let hovered = tools::pick_dim_ref(app, p, app.options.pick_tol);
            if let Some((_, (a, b))) = hovered {
                painter.line_segment([xf.to_screen(a), xf.to_screen(b)], Stroke::new(3.0, PREVIEW));
            }
            for r in refs {
                if let Some((a, b)) = hausit_model::dimensions::ref_segment(&app.doc, r) {
                    painter.line_segment([xf.to_screen(a), xf.to_screen(b)], Stroke::new(2.0, SELECT));
                }
            }
            if refs.len() >= 2 && hovered.is_none() {
                let view = app.active_view.unwrap_or(ElementId::INVALID);
                let d = hausit_model::Dimension {
                    view,
                    refs: refs.clone(),
                    line_point: p,
                    measure: None,
                };
                if let Some(l) = hausit_model::dimensions::layout(&app.doc, &d) {
                    let scale = app.doc.view(view).map(|v| v.scale as f64).unwrap_or(100.0);
                    let mut dl = DisplayList::default();
                    hausit_views::draw_dimension(&mut dl, &l, scale, unit, None);
                    paint_list(painter, xf, &dl, None);
                }
            }
        }
        Tool::Rotate { center, from } => {
            let c = xf.to_screen(*center);
            painter.circle(c, 5.0, Color32::WHITE, Stroke::new(1.5, PREVIEW));
            painter.circle_filled(c, 2.0, PREVIEW);
            if let Some(f) = from {
                painter.line_segment([c, xf.to_screen(*f)], s);
                painter.line_segment([c, xf.to_screen(p)], s);
                let (u, v) = (*f - *center, p - *center);
                let angle = (u.x * v.y - u.y * v.x).atan2(u.dot(v));
                let mut ids = app.selection.clone();
                ids.retain(|id| app.doc.element(*id).is_some());
                label_at(painter, xf.to_screen(p) + vec2(0.0, -18.0), &format!("{:.2}°", angle.to_degrees()));
                // Ghost of the rotated selection.
                let mut ghost = app.doc.clone();
                ghost.rotate(&ids, *center, angle.to_degrees());
                if let Some(v) = app.active_view.and_then(|v| app.doc.view(v)) {
                    let dl = hausit_views::build(&ghost, v);
                    paint_list(painter, xf, &dl, Some((&ids, PREVIEW)));
                }
            } else {
                painter.line_segment([c, xf.to_screen(p)], s);
            }
        }
        Tool::Trim { first: Some((w, _)) } => {
            let dl = app.cache.list.clone();
            paint_list(painter, xf, &dl, Some((&[*w], SELECT)));
        }
        Tool::Move { base: Some(a) } | Tool::Copy { base: Some(a) } => {
            seg(*a, p);
            let dl = app.cache.list.clone();
            let moved = Xform {
                rect: xf.rect,
                line_scale: xf.line_scale,
                cam: Camera2D {
                    center: xf.cam.center - (p - *a),
                    ..xf.cam
                },
            };
            paint_list(painter, &moved, &dl, Some((&app.selection, PREVIEW)));
        }
        Tool::Floor { sketch, .. } | Tool::Roof { sketch } => {
            let roof = matches!(app.tool, Tool::Roof { .. });
            match (app.options.sketch_mode, sketch.start) {
                (DrawShape::PickWalls, _) => {
                    if let Some(l) = tools::pick_wall_face(app, p, roof) {
                        painter.line_segment([xf.to_screen(l.a), xf.to_screen(l.b)], Stroke::new(3.0, PREVIEW));
                    }
                }
                (DrawShape::Rectangle, Some(a)) => {
                    let r = rect_pts(a, p);
                    let sp: Vec<Pos2> = r.iter().map(|q| xf.to_screen(*q)).collect();
                    painter.add(Shape::closed_line(sp, Stroke::new(1.5, SKETCH)));
                }
                (DrawShape::Line, Some(a)) => seg(a, p),
                (DrawShape::SlopeArrow, Some(a)) => slope_arrow(painter, xf.to_screen(a), xf.to_screen(p), PREVIEW),
                _ => {}
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
            ui.menu_button("Hide in View", |ui| {
                if ui.button("Elements (EH)").clicked() {
                    crate::hide_isolate::hide_in_view(app);
                    ui.close();
                }
                if ui.button("Category (VH)").clicked() {
                    hide_category_in_view(app);
                    ui.close();
                }
            });
            if ui.button("Copy to Clipboard (Ctrl+C)").clicked() {
                crate::hide_isolate::copy_to_clipboard(app);
                ui.close();
            }
            if let Some((wall, end, allowed)) = wall_end_under_menu(app, ui) {
                let label = if allowed { "Disallow Join" } else { "Allow Join" };
                if ui.button(label).clicked() {
                    app.transact(label, |doc| doc.set_join_allowed(wall, end, !allowed));
                    ui.close();
                }
            }
            if app.selection.len() == 1
                && app.doc.element(app.selection[0]).and_then(|e| e.as_wall()).is_some()
                && ui.button("Flip (Space)").clicked()
            {
                let id = app.selection[0];
                app.transact("Flip Wall", |doc| doc.flip_wall(id));
                ui.close();
            }
            let pinned = app.selection.iter().any(|s| app.doc.element(*s).is_some_and(|e| e.pinned));
            if ui.button(if pinned { "Unpin (UP)" } else { "Pin (PN)" }).clicked() {
                crate::hide_isolate::pin(app, !pinned);
                ui.close();
            }
            ui.separator();
        }
        if !app.clipboard.is_empty() && ui.button("Paste (Ctrl+V)").clicked() {
            crate::hide_isolate::paste(app);
            ui.close();
        }
        if ui.button("Zoom To Fit").clicked() {
            if let Some(c) = app.active_view.and_then(|v| app.cameras.get_mut(&v)) {
                c.fitted = false;
            }
            ui.close();
        }
    });
}

/// The selected wall end the context menu was opened on (within 600 mm
/// of the right-click), with whether it may join: Revit offers Allow or
/// Disallow Join on a wall end's control.
fn wall_end_under_menu(app: &HausitApp, ui: &Ui) -> Option<(ElementId, usize, bool)> {
    let at = ui.ctx().data(|d| d.get_temp::<DVec2>(egui::Id::unique("context_menu_at")))?;
    app.selection
        .iter()
        .filter_map(|s| app.doc.element(*s).and_then(|e| e.as_wall()).map(|w| (*s, w)))
        .flat_map(|(s, w)| [(s, 0usize, w.start, w), (s, 1usize, w.end, w)])
        .filter(|(_, _, p, _)| p.distance(at) < 600.0)
        .min_by(|a, b| a.2.distance(at).total_cmp(&b.2.distance(at)))
        .map(|(s, end, _, w)| (s, end, !w.disallow_join[end]))
}

/// Simplified ViewCube: preset orientations for the 3D view.
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
                    if let ViewKind::Sheet(def) = &view.kind {
                        let p = def.size.paper();
                        ui.label(format!("{}  ·  {} x {} mm", def.size.title_block(), p.x, p.y));
                        if ui.button("⛶ Zoom to Fit").on_hover_text("Zoom To Fit (ZF)").clicked() {
                            if let Some(c) = app.cameras.get_mut(&view_id) {
                                c.fitted = false;
                            }
                        }
                        if ui.button("Place View…").clicked() {
                            app.dialogs.place_view = Some((view_id, None));
                        }
                        if ui.button("Export PDF…").clicked() {
                            crate::sheets::start_export(app);
                        }
                        ui.label(
                            egui::RichText::new("Drag a view to move it · double-click to open it · Delete removes it from the sheet")
                                .color(Color32::GRAY)
                                .size(11.0),
                        );
                        return;
                    }
                    // Settings a view template controls show its values,
                    // greyed out.
                    let tpl = view.template.and_then(|t| app.doc.view_template(t)).cloned();
                    let eff = app.doc.effective_view(&view);
                    let free = |f: fn(&hausit_model::ViewTemplate) -> bool| tpl.as_ref().is_none_or(|t| !f(t));
                    ui.add_enabled_ui(free(|t| t.scale.is_some()), |ui| {
                        egui::ComboBox::from_id_salt("scale")
                            .selected_text(format!("1 : {}", eff.scale))
                            .width(70.0)
                            .show_ui(ui, |ui| {
                                for s in [1, 5, 10, 20, 25, 50, 100, 200, 500, 1000] {
                                    ui.selectable_value(&mut v.scale, s, format!("1 : {s}"));
                                }
                            });
                    });
                    ui.add_enabled_ui(free(|t| t.detail_level.is_some()), |ui| {
                        egui::ComboBox::from_id_salt("detail")
                            .selected_text(eff.detail_level.label())
                            .width(70.0)
                            .show_ui(ui, |ui| {
                                for d in DetailLevel::ALL {
                                    ui.selectable_value(&mut v.detail_level, d, d.label());
                                }
                            });
                    });
                    if v.kind.plan_level().is_none() {
                        ui.add_enabled_ui(free(|t| t.visual_style.is_some()), |ui| {
                            egui::ComboBox::from_id_salt("style")
                                .selected_text(eff.visual_style.label())
                                .width(110.0)
                                .show_ui(ui, |ui| {
                                    for s in VisualStyle::ALL {
                                        ui.selectable_value(&mut v.visual_style, s, s.label());
                                    }
                                });
                        });
                    }
                    if let Some(t) = &tpl {
                        ui.label(egui::RichText::new(format!("Template: {}", t.name)).color(Color32::GRAY).size(11.0));
                    }
                    if ui.button("⛶ Zoom to Fit").on_hover_text("Zoom To Fit (ZF)").clicked() {
                        if let Some(c) = app.cameras.get_mut(&view_id) {
                            c.fitted = false;
                        }
                    }
                    crate::hide_isolate::control_bar_menu(app, ui, view.temp_hide.is_some());
                    if v != view {
                        if let Some(target) = app.doc.view_mut(view_id) {
                            *target = v;
                        }
                    }
                    if let ViewKind::Section { .. } = view.kind {
                        if ui
                            .button("🧊 3D Section")
                            .on_hover_text("Open this section as a 3D view, cut by a section box on the section line")
                            .clicked()
                        {
                            app.section_in_3d(view_id);
                        }
                    }
                    if matches!(view.kind, ViewKind::ThreeD) {
                        let mut on = view.section_box.is_some();
                        if ui
                            .checkbox(&mut on, "Section Box")
                            .on_hover_text("Cut the model to a box; drag the box's arrows to resize it")
                            .changed()
                        {
                            let b = if on { crate::app::default_section_box(&app.doc) } else { None };
                            app.transact("Section Box", |doc| {
                                if let Some(t) = doc.view_mut(view_id) {
                                    t.section_box = b;
                                }
                                Ok(())
                            });
                        }
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

/// Hide in View ▸ Category (VH): turns the selection's categories off in
/// the view's Visibility/Graphics.
pub fn hide_category_in_view(app: &mut HausitApp) {
    let Some(view_id) = app.active_view else { return };
    let cats: Vec<Category> = app
        .selection
        .iter()
        .filter_map(|s| app.doc.element(*s))
        .map(|e| e.category())
        .collect();
    if cats.is_empty() {
        return;
    }
    app.transact("Hide Category in View", |doc| {
        if let Some(v) = doc.view_mut(view_id) {
            for c in &cats {
                v.graphics.entry(*c).or_default().visible = false;
            }
        }
        Ok(())
    });
    app.selection.clear();
}

/// A crop region around what a view shows now, with a margin: what Crop
/// View starts from.
pub fn crop_around_view(app: &HausitApp, view_id: ElementId) -> Option<(DVec2, DVec2)> {
    let mut v = app.doc.view(view_id)?.clone();
    v.crop = None;
    let (min, max) = hausit_views::build(&app.doc, &v).bounds()?;
    let pad = DVec2::splat(v.scale as f64 * 5.0);
    Some((min - pad, max + pad))
}

/// Crop Region Visible: the crop boundary, whose edges drag in or out.
fn crop_region(app: &mut HausitApp, ui: &Ui, response: &egui::Response, painter: &egui::Painter, xf: &Xform, view_id: ElementId) {
    let Some(view) = app.doc.view(view_id) else { return };
    let (Some((lo, hi)), true) = (view.crop, view.crop_visible) else {
        return;
    };
    if app.tool != Tool::Modify {
        let r = Rect::from_two_pos(xf.to_screen(lo), xf.to_screen(hi));
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_gray(90)), egui::StrokeKind::Middle);
        return;
    }
    let id = egui::Id::unique("crop_edge");
    let edges = |lo: DVec2, hi: DVec2| -> [(Pos2, Pos2); 4] {
        let (a, b, c, d) = (
            xf.to_screen(lo),
            xf.to_screen(DVec2::new(hi.x, lo.y)),
            xf.to_screen(hi),
            xf.to_screen(DVec2::new(lo.x, hi.y)),
        );
        // Left, right, bottom, top.
        [(a, d), (b, c), (a, b), (d, c)]
    };
    let dragging = ui.ctx().data(|d| d.get_temp::<usize>(id));
    let cur = ui.input(|i| i.pointer.latest_pos());
    let (mut lo2, mut hi2) = (lo, hi);
    if let (Some(k), Some(c)) = (dragging, cur) {
        let p = xf.to_view(c);
        match k {
            0 => lo2.x = p.x.min(hi.x - 100.0),
            1 => hi2.x = p.x.max(lo.x + 100.0),
            2 => lo2.y = p.y.min(hi.y - 100.0),
            _ => hi2.y = p.y.max(lo.y + 100.0),
        }
    }
    let near = |p: Pos2| {
        edges(lo, hi).iter().position(|(a, b)| {
            distance_to_segment(
                DVec2::new(p.x as f64, p.y as f64),
                DVec2::new(a.x as f64, a.y as f64),
                DVec2::new(b.x as f64, b.y as f64),
            ) < 6.0
        })
    };
    let hot = cur.and_then(near);
    for (k, (a, b)) in edges(lo2, hi2).iter().enumerate() {
        let on = dragging == Some(k) || (dragging.is_none() && hot == Some(k));
        let s = if on {
            Stroke::new(2.5, SELECT)
        } else {
            Stroke::new(1.0, Color32::from_gray(90))
        };
        painter.line_segment([*a, *b], s);
    }
    if hot.is_some() || dragging.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
    }
    if response.drag_started_by(egui::PointerButton::Primary) {
        if let Some(k) = ui.input(|i| i.pointer.press_origin()).and_then(near) {
            ui.ctx().data_mut(|d| d.insert_temp(id, k));
        }
    }
    if dragging.is_some() && response.drag_stopped() {
        ui.ctx().data_mut(|d| d.remove::<usize>(id));
        app.transact("Resize Crop Region", |doc| {
            if let Some(v) = doc.view_mut(view_id) {
                v.crop = Some((lo2, hi2));
            }
            Ok(())
        });
    }
}
