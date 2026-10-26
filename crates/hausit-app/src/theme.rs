//! Revit-like light theme.

use egui::{Color32, Context, CornerRadius, Stroke, Visuals};

pub const RIBBON_BG: Color32 = Color32::from_rgb(245, 246, 247);
pub const TAB_BAR_BG: Color32 = Color32::from_rgb(232, 234, 237);
pub const PANEL_BG: Color32 = Color32::from_rgb(250, 250, 250);
pub const FILE_TAB: Color32 = Color32::from_rgb(38, 94, 160);
pub const CONTEXT_TAB: Color32 = Color32::from_rgb(96, 168, 74);
pub const SELECT: Color32 = Color32::from_rgb(0, 90, 255);
pub const HOVER: Color32 = Color32::from_rgb(40, 140, 255);
pub const PREVIEW: Color32 = Color32::from_rgb(0, 120, 230);
/// Revit draws sketch lines in magenta.
pub const SKETCH: Color32 = Color32::from_rgb(200, 0, 200);
pub const SNAP: Color32 = Color32::from_rgb(220, 120, 0);
pub const CANVAS_BG: Color32 = Color32::WHITE;
pub const HEADER_BG: Color32 = Color32::from_rgb(222, 226, 231);

pub fn apply(ctx: &Context) {
    let mut v = Visuals::light();
    v.panel_fill = PANEL_BG;
    v.window_fill = Color32::from_rgb(248, 248, 248);
    v.extreme_bg_color = Color32::WHITE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_gray(205));
    v.selection.bg_fill = Color32::from_rgb(196, 220, 252);
    v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(20, 70, 160));
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(2);
    }
    v.window_corner_radius = CornerRadius::same(4);
    ctx.set_theme(egui::Theme::Light);
    ctx.set_visuals_of(egui::Theme::Light, v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(6.0, 2.0);
    });
}

pub fn c32(c: hausit_views::Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}
