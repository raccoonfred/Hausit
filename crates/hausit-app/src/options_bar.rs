//! The Options Bar under the ribbon: settings for the active tool.

use crate::app::HausitApp;
use crate::tools::{DrawShape, Tool};
use egui::{Color32, RichText, Ui};
use hausit_core::{format_length, parse_length, ElementId};
use hausit_model::{LocationLine, RoofShape};

pub fn length_edit(ui: &mut Ui, id: &str, value: &mut f64, unit: hausit_core::LengthUnit) -> bool {
    let key = egui::Id::unique(("len_edit", id));
    let mut text = ui
        .ctx()
        .data(|d| d.get_temp::<String>(key))
        .unwrap_or_else(|| format_length(*value, unit));
    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(70.0));
    let mut changed = false;
    if r.has_focus() {
        ui.ctx().data_mut(|d| d.insert_temp(key, text.clone()));
    }
    if r.lost_focus() {
        if let Some(v) = parse_length(&text, unit) {
            changed = (*value - v).abs() > 1e-9;
            *value = v;
        }
        ui.ctx().data_mut(|d| d.remove::<String>(key));
    }
    changed
}

pub fn show(app: &mut HausitApp, ui: &mut Ui) {
    egui::Panel::top("options_bar")
        .exact_size(28.0)
        .frame(egui::Frame::new().fill(Color32::from_rgb(236, 238, 241)).inner_margin(egui::Margin::symmetric(8, 3)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(app.tool.context_title()).strong());
                ui.separator();
                let unit = app.doc.length_unit;
                match app.tool.clone() {
                    Tool::Wall { .. } => {
                        ui.label("Height:");
                        let levels = app.doc.levels();
                        let current = if app.options.wall_top.is_valid() { app.doc.level_name(app.options.wall_top) } else { "Unconnected".into() };
                        egui::ComboBox::from_id_salt("wall_top").selected_text(current).width(110.0).show_ui(ui, |ui| {
                            ui.selectable_value(&mut app.options.wall_top, ElementId::INVALID, "Unconnected");
                            for (id, l) in &levels {
                                ui.selectable_value(&mut app.options.wall_top, *id, format!("Up to: {}", l.name));
                            }
                        });
                        if !app.options.wall_top.is_valid() {
                            length_edit(ui, "wall_h", &mut app.options.wall_height, unit);
                        }
                        ui.separator();
                        ui.label("Location Line:");
                        egui::ComboBox::from_id_salt("locline").selected_text(app.options.location_line.label()).width(150.0).show_ui(ui, |ui| {
                            for l in LocationLine::ALL {
                                ui.selectable_value(&mut app.options.location_line, l, l.label());
                            }
                        });
                        ui.checkbox(&mut app.options.chain, "Chain");
                        draw_shape(app, ui);
                    }
                    Tool::Floor { .. } => {
                        draw_shape(app, ui);
                        finish_cancel(app, ui);
                    }
                    Tool::Roof { .. } => {
                        ui.label("Slope:");
                        ui.add(egui::DragValue::new(&mut app.options.roof_slope).range(0.0..=75.0).suffix("°").speed(0.5));
                        ui.label("Overhang:");
                        length_edit(ui, "roof_ov", &mut app.options.roof_overhang, unit);
                        egui::ComboBox::from_id_salt("roof_shape").selected_text(app.options.roof_shape.label()).width(200.0).show_ui(ui, |ui| {
                            for s in RoofShape::ALL {
                                ui.selectable_value(&mut app.options.roof_shape, s, s.label());
                            }
                        });
                        draw_shape(app, ui);
                        finish_cancel(app, ui);
                    }
                    Tool::Door | Tool::Window => {
                        ui.label(RichText::new("Hover over a wall in plan. The side of the wall you click sets the swing side.").color(Color32::DARK_GRAY));
                    }
                    Tool::Move { .. } | Tool::Copy { .. } => {
                        ui.label(RichText::new("Hold Shift to constrain to horizontal/vertical.").color(Color32::DARK_GRAY));
                    }
                    Tool::Modify => {
                        if app.selection.is_empty() {
                            ui.label(RichText::new("Shortcuts: WA wall · DR door · WN window · FL floor · RF roof · GR grid · LL level · MV move · CO copy · ZF zoom to fit").color(Color32::DARK_GRAY));
                        } else {
                            ui.label(format!("{} selected", app.selection.len()));
                        }
                    }
                    _ => {}
                }
                if !app.options.typed.is_empty() {
                    ui.separator();
                    ui.label(RichText::new(format!("Length: {}", app.options.typed)).strong().color(Color32::from_rgb(20, 60, 140)));
                }
            });
        });
}

fn draw_shape(app: &mut HausitApp, ui: &mut Ui) {
    ui.separator();
    ui.label("Draw:");
    ui.selectable_value(&mut app.options.shape, DrawShape::Line, "╱ Line");
    ui.selectable_value(&mut app.options.shape, DrawShape::Rectangle, "▭ Rectangle");
}

fn finish_cancel(app: &mut HausitApp, ui: &mut Ui) {
    ui.separator();
    if ui.button(RichText::new("✔ Finish").color(Color32::from_rgb(30, 130, 50))).clicked() {
        crate::tools::finish_sketch(app);
    }
    if ui.button(RichText::new("✖ Cancel").color(Color32::from_rgb(190, 40, 40))).clicked() {
        app.tool.cancel();
        app.status = app.tool.prompt().into();
    }
}
