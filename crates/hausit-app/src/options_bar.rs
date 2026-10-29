//! The Options Bar under the ribbon: settings for the active tool.

use crate::app::HausitApp;
use crate::tools::{DrawShape, Tool};
use egui::{Color32, RichText, Ui};
use hausit_core::{format_length, parse_length, ElementId};
use hausit_model::LocationLine;

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
                        ui.separator();
                        ui.label(RichText::new("Space flips the exterior side").color(Color32::DARK_GRAY));
                    }
                    Tool::Floor { .. } => {
                        if app.options.sketch_mode == DrawShape::Line {
                            ui.checkbox(&mut app.options.chain, "Chain");
                        }
                        finish_cancel(app, ui);
                    }
                    Tool::Roof { .. } => {
                        ui.checkbox(&mut app.options.defines_slope, "Defines slope");
                        ui.label("Slope:");
                        ui.add(egui::DragValue::new(&mut app.options.roof_slope).range(0.0..=75.0).suffix("°").speed(0.5));
                        if app.options.sketch_mode == DrawShape::PickWalls {
                            ui.label("Overhang:");
                            length_edit(ui, "roof_ov", &mut app.options.roof_overhang, unit);
                        } else if app.options.sketch_mode == DrawShape::Line {
                            ui.checkbox(&mut app.options.chain, "Chain");
                        }
                        finish_cancel(app, ui);
                    }
                    Tool::Text | Tool::Tag => {
                        ui.checkbox(&mut app.options.leader, "Leader");
                    }
                    Tool::Door | Tool::Window => {
                        ui.checkbox(&mut app.options.tag_on_placement, "Tag on Placement");
                        ui.separator();
                        ui.label(RichText::new("Hover over a wall in plan. The side you click sets the swing side; Space flips the hand.").color(Color32::DARK_GRAY));
                    }
                    Tool::Rotate { .. } | Tool::Mirror { .. } => {
                        ui.checkbox(&mut app.options.copy, "Copy");
                        ui.separator();
                        let tip = if matches!(app.tool, Tool::Rotate { .. }) {
                            "Rotates about the centre of the selection. Type an angle (counter-clockwise) and press Enter."
                        } else {
                            "MM picks an axis, DM draws one."
                        };
                        ui.label(RichText::new(tip).color(Color32::DARK_GRAY));
                    }
                    Tool::Component { .. } => {
                        ui.label("Rotation");
                        ui.add(
                            egui::DragValue::new(&mut app.options.component_rotation)
                                .range(-360.0..=360.0)
                                .suffix("°"),
                        );
                        ui.separator();
                        if ui.button("Load Family…").clicked() {
                            crate::families::load_from_file(app);
                        }
                        if ui.button("Library…").clicked() {
                            crate::families::open_library(app);
                        }
                        ui.separator();
                        ui.label(RichText::new("Space rotates 90°.").color(Color32::DARK_GRAY));
                    }
                    Tool::Column => {
                        if ui.button("At Grids").on_hover_text("Place a column at every grid intersection").clicked() {
                            crate::tools::columns_at_grids(app);
                        }
                        ui.separator();
                        ui.label(RichText::new("Space rotates the column 90°.").color(Color32::DARK_GRAY));
                    }
                    Tool::RoomSeparation { .. } => {
                        ui.checkbox(&mut app.options.chain, "Chain");
                    }
                    Tool::Beam { brace: false, .. } => {
                        ui.checkbox(&mut app.options.chain, "Chain");
                        ui.separator();
                        ui.label(RichText::new("The beam's top is at this level.").color(Color32::DARK_GRAY));
                    }
                    Tool::Beam { .. } => {
                        ui.label(RichText::new("Braces run from this level to the level above.").color(Color32::DARK_GRAY));
                    }
                    Tool::Stair { pts } => {
                        let pts = pts.clone();
                        let msg = match crate::tools::stair_preview(app, &pts) {
                            Some(l) => format!("{} risers created, {} remaining", l.placed, l.risers - l.placed),
                            None if app.active_level().and_then(|l| app.doc.level_above(l)).is_none() => {
                                "A stair needs a level above this one.".to_string()
                            }
                            None => "Runs go from the first riser to the last.".to_string(),
                        };
                        ui.label(RichText::new(msg).color(Color32::DARK_GRAY));
                        path_finish(app, ui);
                    }
                    Tool::Railing { .. } => path_finish(app, ui),
                    Tool::Ceiling => {
                        ui.label("Height Offset:");
                        length_edit(ui, "ceiling_h", &mut app.options.ceiling_height, unit);
                    }
                    Tool::Footing => {
                        if ui.button("At Columns").on_hover_text("Place a foundation under every column on this level").clicked() {
                            crate::tools::footings_at_columns(app);
                        }
                        ui.separator();
                        ui.label(RichText::new("Space rotates the foundation 90°.").color(Color32::DARK_GRAY));
                    }
                    Tool::Move { .. } | Tool::Copy { .. } => {
                        ui.label(RichText::new("Hold Shift to constrain to horizontal/vertical.").color(Color32::DARK_GRAY));
                    }
                    Tool::Modify => {
                        if app.selection.is_empty() {
                            ui.label(RichText::new("Shortcuts: WA wall · DR door · WN window · FL floor · RF roof · GR grid · LL level · RM room · DI dimension · TX text · TG tag · CL column · BM beam · BR brace · FT wall foundation · MV move · CO copy · RO rotate · MM mirror · BX selection box · TR trim · SL split · ZF zoom to fit").color(Color32::DARK_GRAY));
                        } else {
                            ui.label(format!("{} selected", app.selection.len()));
                        }
                    }
                    _ => {}
                }
                if !app.options.typed.is_empty() {
                    ui.separator();
                    let what = if matches!(app.tool, Tool::Rotate { .. }) { "Angle" } else { "Length" };
                    ui.label(RichText::new(format!("{what}: {}", app.options.typed)).strong().color(Color32::from_rgb(20, 60, 140)));
                }
            });
        });
}

fn path_finish(app: &mut HausitApp, ui: &mut Ui) {
    ui.separator();
    if ui.button(RichText::new("✔ Finish").color(Color32::from_rgb(30, 130, 50))).clicked() {
        crate::tools::finish_path_tool(app);
    }
    if ui.button(RichText::new("✖ Cancel").color(Color32::from_rgb(190, 40, 40))).clicked() {
        app.tool.cancel_keep_kind();
        app.status = app.tool.prompt().into();
    }
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
