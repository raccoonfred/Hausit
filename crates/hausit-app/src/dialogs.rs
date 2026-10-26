//! Dialogs and the Home screen.

use crate::app::HausitApp;
use crate::icons::{self, Icon};
use crate::options_bar::length_edit;
use egui::{vec2, Color32, Context, RichText, Sense, Ui};
use hausit_core::{ElementId, LengthUnit};
use hausit_model::materials::{FillPattern, Layer, LayerFunction, MaterialDef};
use hausit_model::params::set_type_parameter;
use hausit_model::{Category, Document, ParamValue, ScheduleDef, TypeParams, ViewKind};
use std::path::PathBuf;

pub struct RevitFileDialog {
    pub path: PathBuf,
    pub info: hausit_rvt::RevitFileInfo,
}

pub struct TypeProps {
    pub type_id: ElementId,
    pub params: TypeParams,
    /// Compound structure, exterior (walls) or top (floors, roofs) first.
    pub layers: Vec<Layer>,
}

/// Manage ▸ Materials: (name when opened, material) pairs and the row shown.
pub struct MaterialsDialog {
    pub rows: Vec<(String, MaterialDef)>,
    pub selected: usize,
}

pub struct NewSchedule {
    pub category: Category,
    pub name: String,
}

pub struct Rename {
    pub target: ElementId,
    pub text: String,
    pub is_type: bool,
}

#[derive(Default)]
pub struct Dialogs {
    pub error: Option<String>,
    pub about: bool,
    pub revit_file: Option<RevitFileDialog>,
    pub type_props: Option<TypeProps>,
    pub new_schedule: Option<NewSchedule>,
    pub rename: Option<Rename>,
    pub project_info: bool,
    pub units: bool,
    pub confirm_close: bool,
    pub confirm_home: bool,
    pub allow_close: bool,
    /// Lowest Level Notice for a new roof: (current level, level above).
    pub roof_level: Option<(ElementId, ElementId)>,
    /// After creating a roof: attach these walls to it?
    pub attach_walls: Option<(ElementId, Vec<ElementId>)>,
    /// Object Styles dialog: the settings being edited.
    pub object_styles: Option<(hausit_model::styles::ObjectStyles, hausit_model::styles::LineWeights)>,
    pub materials: Option<MaterialsDialog>,
}

impl Dialogs {
    pub fn open_type_properties(&mut self, doc: &Document, type_id: ElementId) {
        if let Some(t) = doc.family_type(type_id) {
            self.type_props = Some(TypeProps {
                type_id,
                params: t.params.clone(),
                layers: t.layers.clone(),
            });
        }
    }
}

/// Shown wherever Hausit names Autodesk products (see ROADMAP.md, 5.1).
pub const TRADEMARK_NOTICE: &str = "Revit and Autodesk are registered trademarks of Autodesk, Inc. \
     Hausit is not affiliated with or endorsed by Autodesk.";

fn window(title: &str) -> egui::Window<'_> {
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
}

pub fn show(app: &mut HausitApp, ctx: &Context) {
    type_properties(app, ctx);
    new_schedule(app, ctx);
    rename(app, ctx);
    project_info(app, ctx);
    units(app, ctx);
    revit_file(app, ctx);
    confirm(app, ctx);
    roof_level(app, ctx);
    attach_walls(app, ctx);
    object_styles(app, ctx);
    materials(app, ctx);

    if app.dialogs.about {
        let mut open = true;
        window("About Hausit").open(&mut open).show(ctx, |ui| {
            ui.heading("Hausit");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.label("Open-source building information modelling with a Revit-style workflow.");
            ui.label("Written in Rust; runs on Windows, macOS and Linux.");
            ui.hyperlink("https://github.com/raccoonfred/Hausit");
            ui.add_space(6.0);
            ui.label(RichText::new(TRADEMARK_NOTICE).small().color(Color32::DARK_GRAY));
        });
        app.dialogs.about = open;
    }
    if let Some(msg) = app.dialogs.error.clone() {
        let mut close = false;
        window("Hausit").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("⚠").size(24.0).color(Color32::from_rgb(220, 150, 0)));
                ui.label(&msg);
            });
            ui.add_space(6.0);
            ui.vertical_centered(|ui| close = ui.button("   OK   ").clicked());
        });
        if close {
            app.dialogs.error = None;
        }
    }
}

fn type_properties(app: &mut HausitApp, ctx: &Context) {
    let Some(tp) = app.dialogs.type_props.as_mut() else { return };
    let Some(ft) = app.doc.family_type(tp.type_id).cloned() else {
        app.dialogs.type_props = None;
        return;
    };
    let unit = app.doc.length_unit;
    let mut action: Option<&str> = None;
    let mut switch_to = None;
    window("Type Properties").show(ctx, |ui| {
        egui::Grid::new("tp_head").num_columns(2).show(ui, |ui| {
            ui.label("Family:");
            let kind = if ft.category.is_hosted() { "Family" } else { "System Family" };
            ui.label(RichText::new(format!("{kind}: {}", ft.family)).strong());
            ui.end_row();
            ui.label("Type:");
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("tp_type")
                    .selected_text(&ft.name)
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for t in app.doc.types_of(ft.category).filter(|t| t.family == ft.family) {
                            if ui.selectable_label(t.id == ft.id, &t.name).clicked() {
                                switch_to = Some(t.id);
                            }
                        }
                    });
                if ui.button("Duplicate…").clicked() {
                    action = Some("duplicate");
                }
                if ui.button("Rename…").clicked() {
                    action = Some("rename");
                }
            });
            ui.end_row();
        });
        ui.separator();
        ui.label(RichText::new("Type Parameters").strong());
        egui::Grid::new("tp_params")
            .striped(true)
            .num_columns(2)
            .min_col_width(160.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Parameter").strong());
                ui.label(RichText::new("Value").strong());
                ui.end_row();
                for (name, value) in tp.params.iter_mut() {
                    ui.label(name);
                    let from_layers = !tp.layers.is_empty() && (name == "Width" || name == "Thickness");
                    match value {
                        ParamValue::Length(v) if from_layers => {
                            ui.label(hausit_core::units::format_length(*v, unit))
                                .on_hover_text("Set by the layers below");
                        }
                        ParamValue::Length(v) => {
                            length_edit(ui, &format!("tp-{}-{name}", ft.id), v, unit);
                        }
                        ParamValue::Integer(i) => {
                            ui.add(egui::DragValue::new(i).range(1..=2));
                        }
                        ParamValue::Text(s) => {
                            ui.text_edit_singleline(s);
                        }
                        ParamValue::Bool(b) => {
                            ui.checkbox(b, "");
                        }
                        other => {
                            ui.label(format!("{other:?}"));
                        }
                    }
                    ui.end_row();
                }
            });
        if ft.has_structure() {
            let side = if ft.category == Category::Walls {
                "exterior side first"
            } else {
                "top first"
            };
            egui::CollapsingHeader::new(format!("Structure ({side})"))
                .default_open(!tp.layers.is_empty())
                .show(ui, |ui| {
                    let names: Vec<String> = app.doc.materials.iter().map(|m| m.name.clone()).collect();
                    edit_layers(ui, &mut tp.layers, &names, unit, ft.id);
                });
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    action = Some("cancel");
                }
                if ui.button("Apply").clicked() {
                    action = Some("apply");
                }
                if ui.button("   OK   ").clicked() {
                    action = Some("ok");
                }
            });
        });
    });
    if let Some(t) = switch_to {
        app.dialogs.open_type_properties(&app.doc, t);
        return;
    }
    let apply = |app: &mut HausitApp| {
        let Some(tp) = app.dialogs.type_props.as_ref() else { return };
        let (id, params, layers) = (tp.type_id, tp.params.clone(), tp.layers.clone());
        if app
            .doc
            .family_type(id)
            .map(|t| t.params == params && t.layers == layers)
            .unwrap_or(true)
        {
            return;
        }
        app.transact("Modify Type", |doc| {
            for (k, v) in params {
                set_type_parameter(doc, id, &k, v)?;
            }
            if let Some(t) = doc.family_type_mut(id) {
                if t.layers != layers {
                    t.set_layers(layers);
                }
            }
            doc.touch();
            Ok(())
        });
        // Show the width the layers add up to.
        app.dialogs.open_type_properties(&app.doc, id);
    };
    match action {
        Some("ok") => {
            apply(app);
            app.dialogs.type_props = None;
        }
        Some("apply") => apply(app),
        Some("cancel") => app.dialogs.type_props = None,
        Some("duplicate") => {
            let mut n = 2;
            let name = loop {
                let cand = format!("{} {n}", ft.name);
                if !app.doc.types_of(ft.category).any(|t| t.name == cand) {
                    break cand;
                }
                n += 1;
            };
            if let Some(new) = app.transact("Duplicate Type", |doc| doc.duplicate_type(ft.id, &name)) {
                app.dialogs.open_type_properties(&app.doc, new);
                app.dialogs.rename = Some(Rename {
                    target: new,
                    text: name,
                    is_type: true,
                });
            }
        }
        Some("rename") => {
            app.dialogs.rename = Some(Rename {
                target: ft.id,
                text: ft.name.clone(),
                is_type: true,
            })
        }
        _ => {}
    }
}

fn new_schedule(app: &mut HausitApp, ctx: &Context) {
    let Some(ns) = app.dialogs.new_schedule.as_mut() else { return };
    let mut result = None;
    window("New Schedule").show(ctx, |ui| {
        ui.label("Category:");
        for cat in Category::SCHEDULABLE {
            ui.radio_value(&mut ns.category, cat, cat.name());
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Name:");
            if ns.name.is_empty() {
                ns.name = format!("{} Schedule", ns.category.singular());
            }
            ui.text_edit_singleline(&mut ns.name);
        });
        ui.horizontal(|ui| {
            if ui.button("   OK   ").clicked() {
                result = Some(true);
            }
            if ui.button("Cancel").clicked() {
                result = Some(false);
            }
        });
    });
    match result {
        Some(true) => {
            let ns = app.dialogs.new_schedule.take().unwrap();
            let name = if app.doc.views().any(|v| v.name == ns.name) {
                app.doc.unique_view_name(&ns.name)
            } else {
                ns.name
            };
            if let Some(id) = app.transact("Create Schedule", |doc| {
                Ok(doc.add_view(&name, ViewKind::Schedule(ScheduleDef::new(ns.category))))
            }) {
                app.open_view(id);
            }
        }
        Some(false) => app.dialogs.new_schedule = None,
        None => {}
    }
}

fn rename(app: &mut HausitApp, ctx: &Context) {
    let Some(r) = app.dialogs.rename.as_mut() else { return };
    let mut result = None;
    window("Rename").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("New name:");
            let resp = ui.text_edit_singleline(&mut r.text);
            resp.request_focus();
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                result = Some(true);
            }
        });
        ui.horizontal(|ui| {
            if ui.button("   OK   ").clicked() {
                result = Some(true);
            }
            if ui.button("Cancel").clicked() {
                result = Some(false);
            }
        });
    });
    match result {
        Some(true) => {
            let r = app.dialogs.rename.take().unwrap();
            if r.is_type {
                app.transact("Rename Type", |doc| doc.rename_type(r.target, r.text.trim()));
            } else if !r.text.trim().is_empty() {
                app.transact("Rename View", |doc| {
                    if let Some(v) = doc.view_mut(r.target) {
                        v.name = r.text.trim().to_string();
                    }
                    Ok(())
                });
            }
        }
        Some(false) => app.dialogs.rename = None,
        None => {}
    }
}

fn project_info(app: &mut HausitApp, ctx: &Context) {
    if !app.dialogs.project_info {
        return;
    }
    let mut info = app.doc.project_info.clone();
    let mut close = false;
    window("Project Information").show(ctx, |ui| {
        egui::Grid::new("pi").num_columns(2).show(ui, |ui| {
            for (label, field) in [
                ("Project Name", &mut info.name),
                ("Project Number", &mut info.number),
                ("Client Name", &mut info.client),
                ("Project Address", &mut info.address),
                ("Author", &mut info.author),
                ("Project Status", &mut info.status),
            ] {
                ui.label(label);
                ui.text_edit_singleline(field);
                ui.end_row();
            }
        });
        close = ui.button("   OK   ").clicked();
    });
    if info != app.doc.project_info {
        // Typing edits live; one undo step per dialog would need buffering.
        app.doc.project_info = info;
        app.doc.touch();
    }
    if close {
        app.dialogs.project_info = false;
    }
}

fn object_styles(app: &mut HausitApp, ctx: &Context) {
    let Some((styles, weights)) = app.dialogs.object_styles.as_mut() else {
        return;
    };
    let mut result = None;
    window("Object Styles").show(ctx, |ui| {
        ui.label("Line weights are pen numbers (1 to 16). Pen widths per scale are below.");
        ui.add_space(4.0);
        egui::Grid::new("object_styles").striped(true).num_columns(4).show(ui, |ui| {
            for h in ["Category", "Projection", "Cut", "Line Color"] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            for (cat, st) in styles.0.iter_mut() {
                ui.label(cat.name());
                ui.add(egui::DragValue::new(&mut st.projection).range(1..=16));
                if cat.is_model() {
                    ui.add(egui::DragValue::new(&mut st.cut).range(1..=16));
                } else {
                    ui.label("");
                }
                ui.color_edit_button_srgb(&mut st.color);
                ui.end_row();
            }
        });
        ui.add_space(6.0);
        egui::CollapsingHeader::new("Line Weights (paper mm)").show(ui, |ui| {
            egui::Grid::new("line_weights").striped(true).show(ui, |ui| {
                ui.label(RichText::new("Pen").strong());
                for s in hausit_model::styles::SCALES {
                    ui.label(RichText::new(format!("1:{s}")).strong());
                }
                ui.end_row();
                for (i, row) in weights.widths.iter_mut().enumerate() {
                    ui.label((i + 1).to_string());
                    for w in row.iter_mut() {
                        ui.add(egui::DragValue::new(w).range(0.01..=10.0).speed(0.01).max_decimals(2));
                    }
                    ui.end_row();
                }
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("   OK   ").clicked() {
                result = Some(true);
            }
            if ui.button(" Cancel ").clicked() {
                result = Some(false);
            }
        });
    });
    match result {
        Some(true) => {
            let (styles, weights) = app.dialogs.object_styles.take().unwrap();
            app.transact("Object Styles", |doc| {
                doc.object_styles = styles;
                doc.line_weights = weights;
                doc.touch();
                Ok(())
            });
        }
        Some(false) => app.dialogs.object_styles = None,
        None => {}
    }
}

fn units(app: &mut HausitApp, ctx: &Context) {
    if !app.dialogs.units {
        return;
    }
    let mut unit = app.doc.length_unit;
    let mut close = false;
    window("Project Units").show(ctx, |ui| {
        ui.label("Length");
        for u in LengthUnit::ALL {
            ui.radio_value(&mut unit, u, u.label());
        }
        close = ui.button("   OK   ").clicked();
    });
    if unit != app.doc.length_unit {
        app.transact("Project Units", |doc| {
            doc.length_unit = unit;
            doc.touch();
            Ok(())
        });
    }
    if close {
        app.dialogs.units = false;
    }
}

fn revit_file(app: &mut HausitApp, ctx: &Context) {
    let Some(d) = app.dialogs.revit_file.as_ref() else { return };
    let mut close = false;
    window("Autodesk Revit File").show(ctx, |ui| {
        ui.set_max_width(520.0);
        ui.label(
            RichText::new(d.path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default())
                .strong()
                .size(15.0),
        );
        ui.add_space(4.0);
        ui.horizontal_top(|ui| {
            if let Some(png) = &d.info.preview_png {
                ui.add(
                    egui::Image::from_bytes(format!("bytes://rvt-preview-{}.png", d.path.display()), png.clone())
                        .max_size(vec2(160.0, 160.0)),
                );
            }
            ui.label(d.info.summary());
        });
        ui.add_space(6.0);
        egui::Frame::new()
            .fill(Color32::from_rgb(255, 248, 225))
            .inner_margin(8)
            .corner_radius(4)
            .show(ui, |ui| {
                ui.label(
                    "Hausit can read this file's header (Revit version, build and preview), but it cannot \
                 import the model's elements yet. Revit's format is proprietary and undocumented, so \
                 reading .rvt and .rfa files from every Revit version is a planned, research-heavy \
                 milestone. Nothing has been imported.",
                );
            });
        ui.collapsing(format!("Container streams ({})", d.info.streams.len()), |ui| {
            egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                for s in &d.info.streams {
                    ui.monospace(format!("{:>10}  {}", s.size, s.path));
                }
            });
        });
        ui.vertical_centered(|ui| close = ui.button("   Close   ").clicked());
    });
    if close {
        app.dialogs.revit_file = None;
    }
}

fn confirm(app: &mut HausitApp, ctx: &Context) {
    if !(app.dialogs.confirm_close || app.dialogs.confirm_home) {
        return;
    }
    let mut choice = None;
    window("Hausit").show(ctx, |ui| {
        ui.label(format!("Do you want to save changes to {}?", app.project_title()));
        ui.horizontal(|ui| {
            if ui.button("Yes").clicked() {
                choice = Some(0);
            }
            if ui.button("No").clicked() {
                choice = Some(1);
            }
            if ui.button("Cancel").clicked() {
                choice = Some(2);
            }
        });
    });
    let Some(c) = choice else { return };
    let closing = app.dialogs.confirm_close;
    app.dialogs.confirm_close = false;
    app.dialogs.confirm_home = false;
    let proceed = match c {
        0 => app.save(),
        1 => true,
        _ => false,
    };
    if proceed {
        if closing {
            app.dialogs.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            app.show_home = true;
        }
    }
}

/// Revit asks to move a roof sketched in a lower level up to the next one.
fn roof_level(app: &mut HausitApp, ctx: &Context) {
    let Some((here, above)) = app.dialogs.roof_level else { return };
    let mut choice = None;
    window("Lowest Level Notice").show(ctx, |ui| {
        ui.label(format!(
            "The roof will be created on {}. Would you like to move it to {}?",
            app.doc.level_name(here),
            app.doc.level_name(above)
        ));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("   Yes   ").clicked() {
                choice = Some(above);
            }
            if ui.button("   No   ").clicked() {
                choice = Some(here);
            }
        });
    });
    if let Some(level) = choice {
        app.options.roof_level = level;
        app.dialogs.roof_level = None;
    }
}

fn attach_walls(app: &mut HausitApp, ctx: &Context) {
    let Some((roof, walls)) = app.dialogs.attach_walls.clone() else {
        return;
    };
    let mut choice = None;
    window("Hausit").show(ctx, |ui| {
        ui.label("Would you like to attach the highlighted walls to the roof?");
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("  Attach  ").clicked() {
                choice = Some(true);
            }
            if ui.button("  Don't attach  ").clicked() {
                choice = Some(false);
            }
        });
    });
    let Some(attach) = choice else { return };
    app.dialogs.attach_walls = None;
    if attach {
        app.transact("Attach Top/Base", |doc| {
            for w in &walls {
                if let Some(hausit_model::ElementData::Wall(wall)) = doc.element_mut(*w).map(|e| &mut e.data) {
                    if !wall.attached_top.contains(&roof) {
                        wall.attached_top.push(roof);
                    }
                }
            }
            Ok(())
        });
        app.status = format!("Attached {} walls to the roof.", walls.len());
    }
    app.selection.clear();
}

/// Revit-style Home screen.
pub fn home(app: &mut HausitApp, ui: &mut Ui) {
    egui::Frame::new().fill(Color32::WHITE).inner_margin(32).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        ui.heading(RichText::new("Hausit").size(32.0).color(crate::theme::FILE_TAB));
        ui.label(
            RichText::new("Building information modelling, the Revit way.")
                .size(15.0)
                .color(Color32::DARK_GRAY),
        );
        ui.add_space(24.0);
        ui.label(RichText::new("Models").strong().size(16.0));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if card(ui, Icon::New, "New…", "Start from the Architectural template") {
                app.set_document(hausit_model::template::new_project(), None);
            }
            if card(ui, Icon::Open, "Open…", ".hausit projects, or inspect .rvt/.rfa files") {
                app.open_dialog();
            }
            if card(
                ui,
                Icon::ThreeD,
                "Sample House",
                "A small house with walls, doors, windows, a floor and a roof",
            ) {
                app.set_document(hausit_model::template::sample_project(), None);
            }
        });
        ui.add_space(24.0);
        egui::Frame::new()
            .fill(Color32::from_rgb(243, 247, 252))
            .inner_margin(12)
            .corner_radius(6)
            .show(ui, |ui| {
                ui.set_max_width(640.0);
                ui.label(RichText::new("About Revit files").strong());
                ui.label(
                    "Hausit opens .rvt/.rfa files far enough to show their Revit version and preview. \
                 Importing their contents is a planned milestone; until then, work is saved in the \
                 open .hausit format.",
                );
            });
    });
}

fn card(ui: &mut Ui, icon: Icon, title: &str, text: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(200.0, 150.0), Sense::click());
    let p = ui.painter();
    let fill = if resp.hovered() {
        Color32::from_rgb(232, 241, 253)
    } else {
        Color32::from_rgb(247, 248, 250)
    };
    p.rect(
        rect,
        6.0,
        fill,
        egui::Stroke::new(1.0, Color32::from_gray(210)),
        egui::StrokeKind::Inside,
    );
    icons::paint(
        p,
        egui::Rect::from_center_size(rect.center_top() + vec2(0.0, 40.0), vec2(44.0, 44.0)),
        icon,
        true,
    );
    p.text(
        rect.center_top() + vec2(0.0, 82.0),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(15.0),
        Color32::BLACK,
    );
    let galley = p.layout(
        text.to_string(),
        egui::FontId::proportional(11.5),
        Color32::DARK_GRAY,
        rect.width() - 20.0,
    );
    p.galley(rect.left_top() + vec2(10.0, 98.0), galley, Color32::DARK_GRAY);
    resp.clicked()
}

/// The layer table of Type Properties ▸ Structure (Revit's Edit Assembly).
fn edit_layers(ui: &mut Ui, layers: &mut Vec<Layer>, materials: &[String], unit: LengthUnit, salt: ElementId) {
    let mut op: Option<(&str, usize)> = None;
    egui::Grid::new(("layers", salt)).striped(true).num_columns(5).show(ui, |ui| {
        for h in ["", "Function", "Material", "Thickness", ""] {
            ui.label(RichText::new(h).strong());
        }
        ui.end_row();
        for (i, l) in layers.iter_mut().enumerate() {
            ui.label((i + 1).to_string());
            egui::ComboBox::from_id_salt(("lf", salt, i))
                .selected_text(l.function.label())
                .width(150.0)
                .show_ui(ui, |ui| {
                    for f in LayerFunction::ALL {
                        ui.selectable_value(&mut l.function, f, f.label());
                    }
                });
            egui::ComboBox::from_id_salt(("lm", salt, i))
                .selected_text(&l.material)
                .width(190.0)
                .show_ui(ui, |ui| {
                    for m in materials {
                        if ui.selectable_label(&l.material == m, m).clicked() {
                            l.material = m.clone();
                        }
                    }
                });
            length_edit(ui, &format!("layer-{salt}-{i}"), &mut l.thickness, unit);
            ui.horizontal(|ui| {
                if ui.small_button("⬆").on_hover_text("Up").clicked() {
                    op = Some(("up", i));
                }
                if ui.small_button("⬇").on_hover_text("Down").clicked() {
                    op = Some(("down", i));
                }
                if ui.small_button("✕").on_hover_text("Delete").clicked() {
                    op = Some(("delete", i));
                }
            });
            ui.end_row();
        }
    });
    let total: f64 = layers.iter().map(|l| l.thickness).sum();
    ui.horizontal(|ui| {
        if ui.button("Insert").clicked() {
            op = Some(("insert", layers.len()));
        }
        ui.label(format!("Total thickness: {}", hausit_core::units::format_length(total, unit)));
    });
    match op {
        Some(("up", i)) if i > 0 => layers.swap(i, i - 1),
        Some(("down", i)) if i + 1 < layers.len() => layers.swap(i, i + 1),
        Some(("delete", i)) => {
            layers.remove(i);
        }
        Some(("insert", i)) => {
            let material = materials.first().cloned().unwrap_or_default();
            layers.insert(
                i,
                Layer {
                    function: LayerFunction::Structure,
                    material,
                    thickness: 100.0,
                },
            );
        }
        _ => {}
    }
    for l in layers.iter_mut() {
        l.thickness = l.thickness.max(0.0);
    }
}

fn materials(app: &mut HausitApp, ctx: &Context) {
    let Some(md) = app.dialogs.materials.as_mut() else { return };
    let mut result = None;
    window("Material Browser").show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(220.0);
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for (i, (_, m)) in md.rows.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let [r, g, b] = m.color;
                            let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
                            if ui.selectable_label(md.selected == i, &m.name).clicked() {
                                md.selected = i;
                            }
                        });
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Duplicate").clicked() {
                        if let Some((_, m)) = md.rows.get(md.selected) {
                            let mut copy = m.clone();
                            copy.name = format!("{} (1)", m.name);
                            md.rows.push((String::new(), copy));
                            md.selected = md.rows.len() - 1;
                        }
                    }
                    if ui.button("Delete").clicked() && md.rows.len() > 1 {
                        md.rows.remove(md.selected);
                        md.selected = md.selected.min(md.rows.len() - 1);
                    }
                });
            });
            ui.add_space(12.0);
            if let Some((_, m)) = md.rows.get_mut(md.selected) {
                egui::Grid::new("material").num_columns(2).show(ui, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut m.name);
                    ui.end_row();
                    ui.label("Shading colour");
                    ui.color_edit_button_srgb(&mut m.color);
                    ui.end_row();
                    ui.label("Cut pattern");
                    egui::ComboBox::from_id_salt("cut_pattern")
                        .selected_text(m.cut_pattern.label())
                        .show_ui(ui, |ui| {
                            for f in FillPattern::ALL {
                                ui.selectable_value(&mut m.cut_pattern, f, f.label());
                            }
                        });
                    ui.end_row();
                    ui.label("Pattern colour");
                    ui.color_edit_button_srgb(&mut m.pattern_color);
                    ui.end_row();
                });
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("   OK   ").clicked() {
                result = Some(true);
            }
            if ui.button(" Cancel ").clicked() {
                result = Some(false);
            }
        });
    });
    match result {
        Some(true) => {
            let md = app.dialogs.materials.take().unwrap();
            app.transact("Materials", |doc| {
                // Layers follow renamed materials.
                for (old, m) in &md.rows {
                    if !old.is_empty() && *old != m.name {
                        let ids: Vec<ElementId> = doc.family_types().map(|t| t.id).collect();
                        for id in ids {
                            if let Some(t) = doc.family_type_mut(id) {
                                for l in t.layers.iter_mut().filter(|l| l.material == *old) {
                                    l.material = m.name.clone();
                                }
                            }
                        }
                    }
                }
                doc.materials = md.rows.into_iter().map(|(_, m)| m).collect();
                doc.touch();
                Ok(())
            });
        }
        Some(false) => app.dialogs.materials = None,
        None => {}
    }
}
