//! The Family Editor: creates and changes loadable families (`.hfam`),
//! as Revit's Family Editor does for `.rfa` files.
//!
//! The window has Revit's Family Types panel (types and their parameter
//! values, adding, renaming and deleting parameters), the family's solids
//! and plan symbols with every dimension as a formula, and a live preview
//! in plan, front, right and 3D. "Load into Project" puts the family in the
//! open project, updating every instance; "Save As" writes the `.hfam`.

use crate::app::HausitApp;
use egui::{Color32, Context, Pos2, RichText, Sense, Stroke, Ui};
use hausit_core::{DVec2, DVec3};
use hausit_model::component::{
    format_number, plan_lines, solid_prisms, Expr, FamilyParam, ParamType, PlanSymbol, Repeat, Solid, SolidCommon, Vars, EXTENSION,
    WALL_THICKNESS,
};
use hausit_model::{Category, FamilyDef};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selected {
    Solid(usize),
    Plan(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewView {
    Plan,
    Front,
    Right,
    ThreeD,
}

pub struct FamilyEditor {
    pub def: FamilyDef,
    pub path: Option<PathBuf>,
    pub cur_type: usize,
    pub selected: Option<Selected>,
    pub view: PreviewView,
    /// 3D preview camera, in degrees.
    pub yaw: f64,
    pub pitch: f64,
    /// Parameter names as being typed, applied when the field is left.
    names: Vec<String>,
    type_name: String,
    new_param: (String, ParamType),
    dirty: bool,
    confirm_close: bool,
    pub message: Option<String>,
}

impl FamilyEditor {
    pub fn new(def: FamilyDef, path: Option<PathBuf>) -> Self {
        let type_name = def.types.first().map(|t| t.name.clone()).unwrap_or_default();
        FamilyEditor {
            names: def.parameters.iter().map(|p| p.name.clone()).collect(),
            def,
            path,
            cur_type: 0,
            selected: None,
            view: PreviewView::ThreeD,
            yaw: -35.0,
            pitch: 25.0,
            type_name,
            new_param: (String::new(), ParamType::Length),
            dirty: false,
            confirm_close: false,
            message: None,
        }
    }

    fn changed(&mut self) {
        self.dirty = true;
    }
}

// ----- opening the editor -----------------------------------------------------

/// File ▸ New ▸ Family: a new family of a category.
pub fn new_family(app: &mut HausitApp, category: Category) {
    app.dialogs.family_editor = Some(Box::new(FamilyEditor::new(FamilyDef::template(category), None)));
}

/// File ▸ Open ▸ Family: a `.hfam` from disk.
pub fn open_family_dialog(app: &mut HausitApp) {
    if let Some(p) = rfd::FileDialog::new().add_filter("Hausit family", &[EXTENSION]).pick_file() {
        open_family(app, &p);
    }
}

pub fn open_family(app: &mut HausitApp, path: &std::path::Path) {
    match std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str::<FamilyDef>(&t).map_err(|e| e.to_string()))
    {
        Ok(def) => app.dialogs.family_editor = Some(Box::new(FamilyEditor::new(def, Some(path.to_path_buf())))),
        Err(e) => app.dialogs.error = Some(format!("{} could not be opened: {e}", path.display())),
    }
}

/// The loaded family behind a type, if it has one.
pub fn family_of_type(app: &HausitApp, type_id: hausit_core::ElementId) -> Option<FamilyDef> {
    let t = app.doc.family_type(type_id)?;
    app.doc.family_def(t.category, &t.family).cloned()
}

/// Edit Family: opens the family of the selected element (or of the
/// current type of the active tool).
pub fn edit_family(app: &mut HausitApp, type_id: hausit_core::ElementId) {
    let Some(def) = family_of_type(app, type_id) else { return };
    let name = app.doc.family_type(type_id).map(|t| t.name.clone()).unwrap_or_default();
    let mut ed = FamilyEditor::new(def, None);
    if let Some(i) = ed.def.types.iter().position(|t| t.name == name) {
        ed.cur_type = i;
        ed.type_name = name;
    }
    app.dialogs.family_editor = Some(Box::new(ed));
}

// ----- the window ------------------------------------------------------------

pub fn show(app: &mut HausitApp, ctx: &Context) {
    let Some(mut ed) = app.dialogs.family_editor.take() else { return };
    let mut open = true;
    let mut action = None;
    egui::Window::new(format!("Family Editor - {}", ed.def.name))
        .id(egui::Id::unique("family_editor"))
        .open(&mut open)
        .default_size([1240.0, 720.0])
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| {
            action = toolbar(app, &mut ed, ui);
            ui.separator();
            let check = ed.def.check();
            match &check {
                Ok(()) => ui.label(RichText::new("✔ The family is valid").color(Color32::from_rgb(30, 120, 50))),
                Err(e) => ui.label(RichText::new(format!("⚠ {e}")).color(Color32::from_rgb(190, 40, 30))),
            };
            if let Some(m) = &ed.message {
                ui.label(RichText::new(m).color(Color32::from_rgb(40, 80, 160)));
            }
            ui.separator();
            let height = ui.available_height().max(420.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(egui::vec2(380.0, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_min_width(380.0);
                    family_types(app, &mut ed, ui);
                });
                ui.separator();
                ui.allocate_ui_with_layout(egui::vec2(330.0, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_min_width(330.0);
                    elements(app, &mut ed, ui);
                });
                ui.separator();
                let w = ui.available_width().max(300.0);
                ui.allocate_ui_with_layout(egui::vec2(w, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                    preview(app, &mut ed, ui)
                });
            });
        });
    if !open {
        if ed.dirty {
            ed.confirm_close = true;
        } else {
            return;
        }
    }
    match action {
        Some(Action::Close) => return,
        Some(Action::Load) => load_into_project(app, &mut ed),
        Some(Action::SaveAs) => save_as(&mut ed),
        Some(Action::Save) => {
            if let Some(p) = ed.path.clone() {
                save_to(&mut ed, &p);
            } else {
                save_as(&mut ed);
            }
        }
        None => {}
    }
    app.dialogs.family_editor = Some(ed);
}

enum Action {
    Close,
    Load,
    Save,
    SaveAs,
}

fn toolbar(app: &mut HausitApp, ed: &mut FamilyEditor, ui: &mut Ui) -> Option<Action> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.label("Family:");
        if ui.add(egui::TextEdit::singleline(&mut ed.def.name).desired_width(200.0)).changed() {
            ed.changed();
        }
        ui.label(RichText::new(format!("Category: {}", ed.def.category.name())).color(Color32::from_gray(90)));
        ui.separator();
        ui.label("Add solid:");
        let n = ed.def.solids.len();
        if ui.button("Box").on_hover_text("A box between two corners").clicked() {
            ed.def.solids.push(new_box());
        }
        if ui
            .button("Extrusion")
            .on_hover_text("A plan profile extruded between two heights")
            .clicked()
        {
            ed.def.solids.push(new_extrusion());
        }
        if ui.button("Cylinder").on_hover_text("A vertical cylinder").clicked() {
            ed.def.solids.push(new_cylinder());
        }
        if ed.def.solids.len() > n {
            ed.selected = Some(Selected::Solid(ed.def.solids.len() - 1));
            ed.changed();
        }
        ui.separator();
        ui.label("Add plan symbol:");
        let n = ed.def.plan.len();
        for kind in ["Line", "Rectangle", "Circle", "Arc", "Polyline"] {
            if ui.button(kind).clicked() {
                ed.def.plan.push(new_symbol(kind));
            }
        }
        if ed.def.plan.len() > n {
            ed.selected = Some(Selected::Plan(ed.def.plan.len() - 1));
            ed.view = PreviewView::Plan;
            ed.changed();
        }
    });
    ui.horizontal(|ui| {
        let valid = ed.def.check().is_ok();
        if ui
            .add_enabled(valid, egui::Button::new(RichText::new("Load into Project").strong()))
            .on_hover_text("Load the family into the open project. Types already in the project take the family's values, and every placed instance updates.")
            .clicked()
        {
            action = Some(Action::Load);
        }
        let save_tip = match &ed.path {
            Some(p) => format!("Save to {}", p.display()),
            None => "Save the family as a .hfam file".into(),
        };
        if ui.button("Save").on_hover_text(save_tip).clicked() {
            action = Some(Action::Save);
        }
        if ui.button("Save As…").clicked() {
            action = Some(Action::SaveAs);
        }
        if ed.confirm_close {
            ui.label(RichText::new("Close without saving your changes?").color(Color32::from_rgb(190, 40, 30)));
            if ui.button("Discard and close").clicked() {
                action = Some(Action::Close);
            }
            if ui.button("Keep editing").clicked() {
                ed.confirm_close = false;
            }
        } else if ui.button("Close").clicked() {
            if ed.dirty {
                ed.confirm_close = true;
            } else {
                action = Some(Action::Close);
            }
        }
        if app.doc.family_def(ed.def.category, &ed.def.name).is_some() {
            ui.label(RichText::new("(loaded in the project)").color(Color32::from_gray(110)));
        }
    });
    action
}

fn load_into_project(app: &mut HausitApp, ed: &mut FamilyEditor) {
    let def = ed.def.clone();
    let name = def.name.clone();
    let existed = app.doc.family_def(def.category, &def.name).is_some();
    if app.transact("Load Family", |doc| doc.load_family_with(def, true)).is_some() {
        ed.message = Some(if existed {
            format!("Reloaded {name} into the project; its types and instances are updated.")
        } else {
            format!("Loaded {name} into the project.")
        });
        app.status = ed.message.clone().unwrap_or_default();
        if let Some(t) = app
            .doc
            .types_of(ed.def.category)
            .find(|t| t.family == ed.def.name && Some(&t.name) == ed.def.types.get(ed.cur_type).map(|t| &t.name))
        {
            let id = t.id;
            app.current_type.insert(ed.def.category, id);
        }
    }
}

fn save_as(ed: &mut FamilyEditor) {
    let picked = rfd::FileDialog::new()
        .add_filter("Hausit family", &[EXTENSION])
        .set_file_name(format!("{}.{EXTENSION}", ed.def.name))
        .save_file();
    if let Some(mut p) = picked {
        if p.extension().is_none() {
            p.set_extension(EXTENSION);
        }
        save_to(ed, &p);
    }
}

fn save_to(ed: &mut FamilyEditor, p: &std::path::Path) {
    match std::fs::write(p, ed.def.to_json()) {
        Ok(()) => {
            ed.path = Some(p.to_path_buf());
            ed.dirty = false;
            ed.message = Some(format!("Saved {}", p.display()));
        }
        Err(e) => ed.message = Some(format!("Could not save {}: {e}", p.display())),
    }
}

// ----- Family Types -------------------------------------------------------------

/// Materials to offer: the project's and the library's.
fn material_names(app: &HausitApp) -> Vec<String> {
    let mut names: Vec<String> = app.doc.materials.iter().map(|m| m.name.clone()).collect();
    for m in hausit_model::materials::default_materials() {
        if !names.contains(&m.name) {
            names.push(m.name);
        }
    }
    names.sort();
    names
}

fn family_types(app: &mut HausitApp, ed: &mut FamilyEditor, ui: &mut Ui) {
    ui.label(RichText::new("Family Types").strong());
    ui.horizontal(|ui| {
        ui.label("Type:");
        let current = ed.def.types.get(ed.cur_type).map(|t| t.name.clone()).unwrap_or_default();
        egui::ComboBox::from_id_salt("fe_type")
            .selected_text(current)
            .width(200.0)
            .show_ui(ui, |ui| {
                for i in 0..ed.def.types.len() {
                    let name = ed.def.types[i].name.clone();
                    if ui.selectable_label(i == ed.cur_type, name.clone()).clicked() {
                        ed.cur_type = i;
                        ed.type_name = name;
                    }
                }
            });
    });
    ui.horizontal(|ui| {
        if ui
            .button("New…")
            .on_hover_text("A new type with the current type's values")
            .clicked()
        {
            let mut t = ed.def.types[ed.cur_type.min(ed.def.types.len() - 1)].clone();
            t.name = ed.def.unique_type_name("Type");
            ed.type_name = t.name.clone();
            ed.def.types.push(t);
            ed.cur_type = ed.def.types.len() - 1;
            ed.changed();
        }
        let r = ui.add(egui::TextEdit::singleline(&mut ed.type_name).desired_width(140.0));
        if r.lost_focus() {
            let name = ed.type_name.trim().to_string();
            let taken = ed.def.types.iter().enumerate().any(|(i, t)| i != ed.cur_type && t.name == name);
            if name.is_empty() || taken {
                ed.message = Some(format!("A type needs a name that no other type has: \"{name}\""));
                ed.type_name = ed.def.types[ed.cur_type].name.clone();
            } else if ed.def.types[ed.cur_type].name != name {
                ed.def.types[ed.cur_type].name = name;
                ed.changed();
            }
        }
        r.on_hover_text("Rename the type");
        if ui
            .add_enabled(ed.def.types.len() > 1, egui::Button::new("Delete"))
            .on_hover_text("Delete the type")
            .clicked()
        {
            ed.def.types.remove(ed.cur_type);
            ed.cur_type = ed.cur_type.min(ed.def.types.len() - 1);
            ed.type_name = ed.def.types[ed.cur_type].name.clone();
            ed.changed();
        }
    });
    ui.add_space(4.0);
    if ed.names.len() != ed.def.parameters.len() {
        ed.names = ed.def.parameters.iter().map(|p| p.name.clone()).collect();
    }
    let materials = material_names(app);
    let mut remove = None;
    egui::ScrollArea::vertical()
        .id_salt("fe_params")
        .max_height(ui.available_height() - 110.0)
        .show(ui, |ui| {
            egui::Grid::new("fe_param_grid")
                .num_columns(4)
                .striped(true)
                .spacing([6.0, 3.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Parameter").strong());
                    ui.label(RichText::new("Value").strong());
                    ui.label(RichText::new("Type").strong());
                    ui.label("");
                    ui.end_row();
                    for i in 0..ed.def.parameters.len() {
                        let p = ed.def.parameters[i].clone();
                        let r = ui.add_sized([140.0, 20.0], egui::TextEdit::singleline(&mut ed.names[i]));
                        if r.lost_focus() && ed.names[i] != p.name {
                            let new = ed.names[i].clone();
                            match ed.def.rename_parameter(&p.name, &new) {
                                Ok(()) => {
                                    ed.message = Some(format!("Renamed \"{}\" to \"{}\" in every formula.", p.name, new.trim()));
                                    ed.changed();
                                }
                                Err(e) => ed.message = Some(e),
                            }
                            ed.names[i] = ed.def.parameters[i].name.clone();
                        }
                        if value_editor(ed, i, &materials, ui) {
                            ed.changed();
                        }
                        let mut kind = p.kind;
                        egui::ComboBox::from_id_salt(("fe_kind", i))
                            .selected_text(kind.label())
                            .width(80.0)
                            .show_ui(ui, |ui| {
                                for k in ParamType::ALL {
                                    ui.selectable_value(&mut kind, k, k.label());
                                }
                            });
                        if kind != p.kind {
                            set_kind(&mut ed.def, i, kind);
                            ed.changed();
                        }
                        let required = ed.def.is_hosted() && (p.name == "Width" || p.name == "Height");
                        let tip = if required {
                            "Doors and windows need Width and Height"
                        } else {
                            "Delete the parameter"
                        };
                        if ui
                            .add_enabled(!required, egui::Button::new("✖").small())
                            .on_hover_text(tip)
                            .clicked()
                        {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                    if ed.def.is_hosted() {
                        ui.label(RichText::new(WALL_THICKNESS).italics());
                        ui.label(RichText::new("from the host wall").italics().color(Color32::from_gray(110)));
                        ui.label(RichText::new("Length").color(Color32::from_gray(110)));
                        ui.label("");
                        ui.end_row();
                    }
                });
        });
    if let Some(i) = remove {
        let name = ed.def.parameters[i].name.clone();
        if ed.def.uses_parameter(&name) {
            ed.message = Some(format!(
                "\"{name}\" is still used by a formula or material; those now show an error."
            ));
        }
        ed.def.remove_parameter(&name);
        ed.changed();
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut ed.new_param.0)
                .hint_text("New parameter")
                .desired_width(130.0),
        );
        egui::ComboBox::from_id_salt("fe_new_kind")
            .selected_text(ed.new_param.1.label())
            .width(80.0)
            .show_ui(ui, |ui| {
                for k in ParamType::ALL {
                    ui.selectable_value(&mut ed.new_param.1, k, k.label());
                }
            });
        if ui.button("Add").clicked() {
            let name = ed.new_param.0.trim().to_string();
            let kind = ed.new_param.1;
            // Add under a placeholder, then rename to check the name.
            let tmp = "__new_parameter__";
            ed.def.parameters.push(FamilyParam {
                name: tmp.into(),
                kind,
                default: kind.default_value(),
            });
            match ed.def.rename_parameter(tmp, &name) {
                Ok(()) => {
                    ed.new_param.0.clear();
                    ed.changed();
                }
                Err(e) => {
                    ed.def.remove_parameter(tmp);
                    ed.message = Some(e);
                }
            }
        }
    });
    ui.add_space(4.0);
    ui.label("Description:");
    if ui
        .add(
            egui::TextEdit::multiline(&mut ed.def.description)
                .desired_rows(2)
                .desired_width(360.0),
        )
        .changed()
    {
        ed.changed();
    }
}

/// Changes a parameter's kind, converting its default and type values.
fn set_kind(def: &mut FamilyDef, i: usize, kind: ParamType) {
    let p = &mut def.parameters[i];
    p.kind = kind;
    p.default = kind.convert(&p.default);
    let name = p.name.clone();
    for t in &mut def.types {
        if let Some(v) = t.values.get_mut(&name) {
            *v = kind.convert(v);
        }
    }
}

/// The current type's value of parameter `i`.
fn value_editor(ed: &mut FamilyEditor, i: usize, materials: &[String], ui: &mut Ui) -> bool {
    let p = ed.def.parameters[i].clone();
    let t = ed.cur_type;
    let v = ed.def.value(t, &p.name).cloned().unwrap_or_else(|| p.kind.default_value());
    let new: Option<serde_json::Value> = match p.kind {
        ParamType::Length | ParamType::Number | ParamType::Angle => {
            let mut x = v.as_f64().unwrap_or(0.0);
            let suffix = match p.kind {
                ParamType::Length => " mm",
                ParamType::Angle => "°",
                _ => "",
            };
            ui.add(egui::DragValue::new(&mut x).speed(1.0).suffix(suffix).max_decimals(3))
                .changed()
                .then(|| x.into())
        }
        ParamType::Integer => {
            let mut x = v.as_i64().unwrap_or_else(|| v.as_f64().unwrap_or(0.0).round() as i64);
            ui.add(egui::DragValue::new(&mut x).speed(0.1)).changed().then(|| x.into())
        }
        ParamType::YesNo => {
            let mut b = v.as_bool().unwrap_or_else(|| v.as_f64().unwrap_or(0.0) != 0.0);
            ui.checkbox(&mut b, "").changed().then(|| b.into())
        }
        ParamType::Text => {
            let mut s = v.as_str().unwrap_or_default().to_string();
            ui.add(egui::TextEdit::singleline(&mut s).desired_width(130.0))
                .changed()
                .then(|| s.into())
        }
        ParamType::Material => {
            let mut s = v.as_str().unwrap_or_default().to_string();
            let before = s.clone();
            egui::ComboBox::from_id_salt(("fe_mat", i))
                .selected_text(s.clone())
                .width(130.0)
                .show_ui(ui, |ui| {
                    for m in materials {
                        ui.selectable_value(&mut s, m.clone(), m);
                    }
                });
            (s != before).then(|| s.into())
        }
    };
    match new {
        Some(v) => {
            ed.def.set_value(t, &p.name, v);
            true
        }
        None => false,
    }
}

// ----- solids and plan symbols ---------------------------------------------------

fn n(v: f64) -> Expr {
    Expr::Number(v)
}

fn new_box() -> Solid {
    Solid::Box {
        min: [n(-300.0), n(-300.0), n(0.0)],
        max: [n(300.0), n(0.0), n(500.0)],
        common: SolidCommon::default(),
    }
}

fn new_extrusion() -> Solid {
    Solid::Extrusion {
        profile: vec![[n(-300.0), n(-300.0)], [n(300.0), n(-300.0)], [n(0.0), n(0.0)]],
        z: [n(0.0), n(500.0)],
        common: SolidCommon::default(),
    }
}

fn new_cylinder() -> Solid {
    Solid::Cylinder {
        center: [n(0.0), n(-150.0)],
        radius: n(150.0),
        z: [n(0.0), n(500.0)],
        common: SolidCommon::default(),
    }
}

fn new_symbol(kind: &str) -> PlanSymbol {
    match kind {
        "Line" => PlanSymbol::Line {
            from: [n(-300.0), n(-300.0)],
            to: [n(300.0), n(0.0)],
        },
        "Rectangle" => PlanSymbol::Rectangle {
            min: [n(-300.0), n(-300.0)],
            max: [n(300.0), n(0.0)],
        },
        "Circle" => PlanSymbol::Circle {
            center: [n(0.0), n(-150.0)],
            radius: n(150.0),
        },
        "Arc" => PlanSymbol::Arc {
            center: [n(0.0), n(0.0)],
            radius: n(300.0),
            start: n(180.0),
            end: n(270.0),
        },
        _ => PlanSymbol::Polyline {
            points: vec![[n(-300.0), n(0.0)], [n(0.0), n(-300.0)], [n(300.0), n(0.0)]],
            closed: false,
        },
    }
}

/// The material a solid uses, with a Material parameter resolved for the
/// current type.
fn solid_material(def: &FamilyDef, t: usize, s: &Solid) -> Option<String> {
    let m = s.common().material.clone()?;
    match def.parameters.iter().find(|p| p.name == m && p.kind == ParamType::Material) {
        Some(p) => def.value(t, &p.name).and_then(|v| v.as_str()).map(String::from),
        None => Some(m),
    }
}

fn elements(app: &mut HausitApp, ed: &mut FamilyEditor, ui: &mut Ui) {
    ui.label(RichText::new("Solids and plan symbols").strong());
    let vars = ed.def.type_vars(ed.cur_type);
    let list_h = 200.0;
    egui::ScrollArea::vertical().id_salt("fe_list").max_height(list_h).show(ui, |ui| {
        ui.set_min_width(310.0);
        for i in 0..ed.def.solids.len() {
            let s = &ed.def.solids[i];
            let err = solid_prisms(s, &vars).err();
            let mat = solid_material(&ed.def, ed.cur_type, s)
                .map(|m| format!(" - {m}"))
                .unwrap_or_default();
            let mut text = RichText::new(format!("{} {}{mat}", s.kind(), i + 1));
            if err.is_some() {
                text = text.color(Color32::from_rgb(190, 40, 30));
            }
            let r = ui.selectable_label(ed.selected == Some(Selected::Solid(i)), text);
            if r.clicked() {
                ed.selected = Some(Selected::Solid(i));
            }
            if let Some(e) = err {
                r.on_hover_text(e);
            }
        }
        if !ed.def.plan.is_empty() {
            ui.separator();
            ui.label(RichText::new("Plan symbol (drawn in plans instead of the solids)").small());
        }
        for i in 0..ed.def.plan.len() {
            let p = &ed.def.plan[i];
            let err = plan_lines(p, &vars).err();
            let mut text = RichText::new(format!("{} {}", p.kind(), i + 1));
            if err.is_some() {
                text = text.color(Color32::from_rgb(190, 40, 30));
            }
            if ui.selectable_label(ed.selected == Some(Selected::Plan(i)), text).clicked() {
                ed.selected = Some(Selected::Plan(i));
                ed.view = PreviewView::Plan;
            }
        }
        if ed.def.solids.is_empty() && ed.def.plan.is_empty() {
            ui.label("Add a solid from the toolbar.");
        }
    });
    // Duplicate, delete, reorder.
    ui.horizontal(|ui| {
        let sel = ed.selected;
        let enabled = sel.is_some();
        if ui.add_enabled(enabled, egui::Button::new("Duplicate")).clicked() {
            match sel {
                Some(Selected::Solid(i)) if i < ed.def.solids.len() => {
                    ed.def.solids.insert(i + 1, ed.def.solids[i].clone());
                    ed.selected = Some(Selected::Solid(i + 1));
                }
                Some(Selected::Plan(i)) if i < ed.def.plan.len() => {
                    ed.def.plan.insert(i + 1, ed.def.plan[i].clone());
                    ed.selected = Some(Selected::Plan(i + 1));
                }
                _ => {}
            }
            ed.changed();
        }
        if ui.add_enabled(enabled, egui::Button::new("Delete")).clicked() {
            match sel {
                Some(Selected::Solid(i)) if i < ed.def.solids.len() => {
                    ed.def.solids.remove(i);
                }
                Some(Selected::Plan(i)) if i < ed.def.plan.len() => {
                    ed.def.plan.remove(i);
                }
                _ => {}
            }
            ed.selected = None;
            ed.changed();
        }
    });
    ui.separator();
    let materials = material_names(app);
    let mut changed = false;
    egui::ScrollArea::vertical().id_salt("fe_props").show(ui, |ui| match ed.selected {
        Some(Selected::Solid(i)) if i < ed.def.solids.len() => {
            let material_params: Vec<String> = ed
                .def
                .parameters
                .iter()
                .filter(|p| p.kind == ParamType::Material)
                .map(|p| p.name.clone())
                .collect();
            let solid = &mut ed.def.solids[i];
            changed |= solid_editor(solid, &vars, &material_params, &materials, ui);
            if let Err(e) = solid_prisms(&ed.def.solids[i], &vars) {
                ui.label(RichText::new(e).color(Color32::from_rgb(190, 40, 30)));
            }
        }
        Some(Selected::Plan(i)) if i < ed.def.plan.len() => {
            changed |= symbol_editor(&mut ed.def.plan[i], &vars, ui);
            if let Err(e) = plan_lines(&ed.def.plan[i], &vars) {
                ui.label(RichText::new(e).color(Color32::from_rgb(190, 40, 30)));
            }
        }
        _ => {
            ui.label("Select a solid or plan symbol to edit it.");
            ui.add_space(6.0);
            ui.label(RichText::new("Coordinates").strong());
            let hosted = ed.def.is_hosted();
            ui.label(if hosted {
                "x runs along the wall, centred on the opening. y crosses the wall: 0 is the wall's centre line and positive y is the exterior. z runs up from the bottom of the opening."
            } else {
                "x points right and y to the back; the family faces -y, so its front is at y < 0. z runs up from the insertion point."
            });
            ui.add_space(4.0);
            ui.label("Every dimension can be a number or a formula over the parameters, for example \"Width / 2 - 25\". Formulas can use + - * / ( ), comparisons and min, max, abs, sqrt, round, floor and if(condition, then, else).");
        }
    });
    if changed {
        ed.changed();
    }
}

/// A dimension: the formula as text, red when it does not evaluate, with
/// its value on hover.
fn expr_field(ui: &mut Ui, e: &mut Expr, vars: &Vars) -> bool {
    let mut text = e.to_text();
    let result = e.eval(vars);
    let color = if result.is_ok() {
        None
    } else {
        Some(Color32::from_rgb(190, 40, 30))
    };
    let mut edit = egui::TextEdit::singleline(&mut text).desired_width(120.0);
    if let Some(c) = color {
        edit = edit.text_color(c);
    }
    let r = ui.add(edit);
    let changed = r.changed();
    match result {
        Ok(v) => r.on_hover_text(format!("= {}", format_number(v))),
        Err(err) => r.on_hover_text(err),
    };
    if changed {
        *e = Expr::parse(&text);
    }
    changed
}

fn xyz_row(ui: &mut Ui, label: &str, es: &mut [Expr], vars: &Vars) -> bool {
    let mut changed = false;
    ui.label(label);
    ui.vertical(|ui| {
        for (axis, e) in ["x", "y", "z"].iter().zip(es.iter_mut()) {
            ui.horizontal(|ui| {
                ui.label(*axis);
                changed |= expr_field(ui, e, vars);
            });
        }
    });
    ui.end_row();
    changed
}

fn solid_editor(solid: &mut Solid, vars: &Vars, material_params: &[String], materials: &[String], ui: &mut Ui) -> bool {
    let mut changed = false;
    ui.label(RichText::new(solid.kind()).strong());
    egui::Grid::new("fe_solid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        match solid {
            Solid::Box { min, max, .. } => {
                changed |= xyz_row(ui, "From corner", min, vars);
                changed |= xyz_row(ui, "To corner", max, vars);
            }
            Solid::Extrusion { profile, z, .. } => {
                ui.label("Profile");
                ui.vertical(|ui| {
                    let mut remove = None;
                    for (k, p) in profile.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("{}", k + 1));
                            changed |= expr_field(ui, &mut p[0], vars);
                            changed |= expr_field(ui, &mut p[1], vars);
                            if ui.small_button("✖").on_hover_text("Remove the point").clicked() {
                                remove = Some(k);
                            }
                        });
                    }
                    if let Some(k) = remove {
                        if profile.len() > 3 {
                            profile.remove(k);
                            changed = true;
                        }
                    }
                    if ui.small_button("Add point").clicked() {
                        let last = profile.last().cloned().unwrap_or([n(0.0), n(0.0)]);
                        profile.push(last);
                        changed = true;
                    }
                });
                ui.end_row();
                changed |= z_rows(ui, z, vars);
            }
            Solid::Cylinder { center, radius, z, .. } => {
                ui.label("Centre (x, y)");
                ui.horizontal(|ui| {
                    changed |= expr_field(ui, &mut center[0], vars);
                    changed |= expr_field(ui, &mut center[1], vars);
                });
                ui.end_row();
                ui.label("Radius");
                changed |= expr_field(ui, radius, vars);
                ui.end_row();
                changed |= z_rows(ui, z, vars);
            }
        }
        let common = solid.common_mut();
        ui.label("Material");
        let mut m = common.material.clone();
        let label = |m: &Option<String>| match m {
            None => "<By Category>".to_string(),
            Some(s) if material_params.contains(s) => format!("Parameter: {s}"),
            Some(s) => s.clone(),
        };
        egui::ComboBox::from_id_salt("fe_solid_mat")
            .selected_text(label(&m))
            .width(180.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut m, None, "<By Category>");
                for p in material_params {
                    ui.selectable_value(&mut m, Some(p.clone()), format!("Parameter: {p}"));
                }
                ui.separator();
                for name in materials {
                    ui.selectable_value(&mut m, Some(name.clone()), name);
                }
            });
        if m != common.material {
            common.material = m;
            changed = true;
        }
        ui.end_row();
        ui.label("Visible");
        ui.vertical(|ui| {
            let mut conditional = common.visible.is_some();
            if ui.checkbox(&mut conditional, "only when").changed() {
                common.visible = conditional.then(|| Expr::Text("1".into()));
                changed = true;
            }
            if let Some(e) = &mut common.visible {
                changed |= expr_field(ui, e, vars);
            }
        });
        ui.end_row();
        ui.label("Repeat");
        ui.vertical(|ui| {
            let mut on = common.repeat.is_some();
            if ui.checkbox(&mut on, "copies").changed() {
                common.repeat = on.then(|| Repeat {
                    count: n(2.0),
                    step: [n(0.0), n(0.0), n(300.0)],
                });
                changed = true;
            }
            if let Some(r) = &mut common.repeat {
                ui.horizontal(|ui| {
                    ui.label("count");
                    changed |= expr_field(ui, &mut r.count, vars);
                });
                for (axis, e) in ["step x", "step y", "step z"].iter().zip(r.step.iter_mut()) {
                    ui.horizontal(|ui| {
                        ui.label(*axis);
                        changed |= expr_field(ui, e, vars);
                    });
                }
            }
        });
        ui.end_row();
    });
    changed
}

fn z_rows(ui: &mut Ui, z: &mut [Expr; 2], vars: &Vars) -> bool {
    let mut changed = false;
    ui.label("Bottom (z)");
    changed |= expr_field(ui, &mut z[0], vars);
    ui.end_row();
    ui.label("Top (z)");
    changed |= expr_field(ui, &mut z[1], vars);
    ui.end_row();
    changed
}

fn symbol_editor(sym: &mut PlanSymbol, vars: &Vars, ui: &mut Ui) -> bool {
    let mut changed = false;
    ui.label(RichText::new(sym.kind()).strong());
    let xy = |ui: &mut Ui, label: &str, p: &mut [Expr; 2], changed: &mut bool| {
        ui.label(label);
        ui.horizontal(|ui| {
            *changed |= expr_field(ui, &mut p[0], vars);
            *changed |= expr_field(ui, &mut p[1], vars);
        });
        ui.end_row();
    };
    egui::Grid::new("fe_symbol")
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| match sym {
            PlanSymbol::Line { from, to } => {
                xy(ui, "From (x, y)", from, &mut changed);
                xy(ui, "To (x, y)", to, &mut changed);
            }
            PlanSymbol::Rectangle { min, max } => {
                xy(ui, "Corner (x, y)", min, &mut changed);
                xy(ui, "Opposite (x, y)", max, &mut changed);
            }
            PlanSymbol::Circle { center, radius } => {
                xy(ui, "Centre (x, y)", center, &mut changed);
                ui.label("Radius");
                changed |= expr_field(ui, radius, vars);
                ui.end_row();
            }
            PlanSymbol::Arc {
                center,
                radius,
                start,
                end,
            } => {
                xy(ui, "Centre (x, y)", center, &mut changed);
                ui.label("Radius");
                changed |= expr_field(ui, radius, vars);
                ui.end_row();
                ui.label("Start angle (°)");
                changed |= expr_field(ui, start, vars);
                ui.end_row();
                ui.label("End angle (°)");
                changed |= expr_field(ui, end, vars);
                ui.end_row();
            }
            PlanSymbol::Polyline { points, closed } => {
                let mut remove = None;
                for (k, p) in points.iter_mut().enumerate() {
                    xy(ui, &format!("Point {}", k + 1), p, &mut changed);
                    if ui.small_button("✖").clicked() {
                        remove = Some(k);
                    }
                    ui.end_row();
                }
                if let Some(k) = remove {
                    if points.len() > 2 {
                        points.remove(k);
                        changed = true;
                    }
                }
                if ui.small_button("Add point").clicked() {
                    let last = points.last().cloned().unwrap_or([n(0.0), n(0.0)]);
                    points.push(last);
                    changed = true;
                }
                changed |= ui.checkbox(closed, "Closed").changed();
                ui.end_row();
            }
        });
    changed
}

// ----- preview -------------------------------------------------------------------------

/// A face of the preview, in family coordinates.
struct Face {
    pts: Vec<DVec3>,
    color: [u8; 3],
    glass: bool,
    selected: bool,
}

fn material_color(app: &HausitApp, name: Option<&str>) -> [u8; 3] {
    let Some(name) = name else { return [200, 200, 196] };
    app.doc
        .material(name)
        .map(|m| m.color)
        .or_else(|| {
            hausit_model::materials::default_materials()
                .into_iter()
                .find(|m| m.name == name)
                .map(|m| m.color)
        })
        .unwrap_or([200, 200, 196])
}

fn faces(app: &HausitApp, ed: &FamilyEditor, vars: &Vars) -> Vec<Face> {
    let mut out = Vec::new();
    for (i, s) in ed.def.solids.iter().enumerate() {
        let Ok((prisms, _)) = solid_prisms(s, vars) else { continue };
        let material = solid_material(&ed.def, ed.cur_type, s);
        let color = material_color(app, material.as_deref());
        let glass = material.as_deref().is_some_and(|m| m.to_ascii_lowercase().contains("glass"));
        let selected = ed.selected == Some(Selected::Solid(i));
        for p in prisms {
            let mut fp = p.footprint.clone();
            if hausit_core::geom::polygon_area(&fp) < 0.0 {
                fp.reverse();
            }
            let at = |q: DVec2, z: f64| DVec3::new(q.x, q.y, z);
            let mut add = |pts: Vec<DVec3>| {
                out.push(Face {
                    pts,
                    color,
                    glass,
                    selected,
                })
            };
            add(fp.iter().rev().map(|q| at(*q, p.z0)).collect());
            add(fp.iter().map(|q| at(*q, p.z1)).collect());
            for k in 0..fp.len() {
                let (a, b) = (fp[k], fp[(k + 1) % fp.len()]);
                add(vec![at(a, p.z0), at(b, p.z0), at(b, p.z1), at(a, p.z1)]);
            }
        }
    }
    out
}

/// The view's axes: screen right, screen up, and the direction looked in.
fn basis(ed: &FamilyEditor) -> (DVec3, DVec3, DVec3) {
    match ed.view {
        PreviewView::Plan => (DVec3::X, DVec3::Y, -DVec3::Z),
        PreviewView::Front => (DVec3::X, DVec3::Z, DVec3::Y),
        PreviewView::Right => (DVec3::Y, DVec3::Z, -DVec3::X),
        PreviewView::ThreeD => {
            let (yaw, pitch) = (ed.yaw.to_radians(), ed.pitch.to_radians());
            // Looking from the front-left, down onto the family.
            let fwd = DVec3::new(-yaw.sin() * pitch.cos(), yaw.cos() * pitch.cos(), -pitch.sin()).normalize();
            let right = fwd.cross(DVec3::Z).normalize();
            let up = right.cross(fwd);
            (right, up, fwd)
        }
    }
}

fn preview(app: &mut HausitApp, ed: &mut FamilyEditor, ui: &mut Ui) {
    ui.horizontal(|ui| {
        for (v, label) in [
            (PreviewView::Plan, "Plan"),
            (PreviewView::Front, if ed.def.is_hosted() { "Interior" } else { "Front" }),
            (PreviewView::Right, "Right"),
            (PreviewView::ThreeD, "3D"),
        ] {
            ui.selectable_value(&mut ed.view, v, label);
        }
        if ed.view == PreviewView::ThreeD {
            ui.label(RichText::new("drag to orbit").small().color(Color32::from_gray(110)));
        }
    });
    let size = ui.available_size().max(egui::vec2(300.0, 300.0));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::drag());
    if ed.view == PreviewView::ThreeD && resp.dragged() {
        let d = resp.drag_delta();
        ed.yaw = (ed.yaw - d.x as f64 * 0.5) % 360.0;
        ed.pitch = (ed.pitch + d.y as f64 * 0.5).clamp(-89.0, 89.0);
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    let vars = ed.def.type_vars(ed.cur_type);
    let faces = faces(app, ed, &vars);
    let (right, up, fwd) = basis(ed);
    let hosted = ed.def.is_hosted();
    let w = vars.get("Width").copied().unwrap_or(900.0);
    let h = vars.get("Height").copied().unwrap_or(2100.0);
    let wall_t = vars.get(WALL_THICKNESS).copied().unwrap_or(200.0);

    // Plan symbols, in plan.
    let symbols: Vec<(DVec2, DVec2, bool)> = if ed.view == PreviewView::Plan {
        ed.def
            .plan
            .iter()
            .enumerate()
            .filter_map(|(i, p)| plan_lines(p, &vars).ok().map(|l| (i, l)))
            .flat_map(|(i, l)| {
                let selected = ed.selected == Some(Selected::Plan(i));
                l.into_iter().map(move |(a, b)| (a, b, selected))
            })
            .collect()
    } else {
        vec![]
    };

    // Fit everything (and the reference planes' crossing) in the view.
    let mut pts: Vec<DVec3> = faces.iter().flat_map(|f| f.pts.iter().copied()).collect();
    pts.extend(symbols.iter().flat_map(|(a, b, _)| [a.extend(0.0), b.extend(0.0)]));
    pts.push(DVec3::ZERO);
    if hosted {
        let half = w / 2.0 + 400.0;
        pts.extend([DVec3::new(-half, -wall_t / 2.0, 0.0), DVec3::new(half, wall_t / 2.0, h + 300.0)]);
    }
    let proj = |p: DVec3| (p.dot(right), p.dot(up));
    let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for p in &pts {
        let (x, y) = proj(*p);
        x0 = x0.min(x);
        x1 = x1.max(x);
        y0 = y0.min(y);
        y1 = y1.max(y);
    }
    let margin = 40.0;
    let scale = ((rect.width() as f64 - 2.0 * margin) / (x1 - x0).max(1.0))
        .min((rect.height() as f64 - 2.0 * margin) / (y1 - y0).max(1.0))
        .max(1e-4);
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let c = rect.center();
    let to_screen = |p: DVec3| {
        let (x, y) = proj(p);
        Pos2::new(c.x + ((x - cx) * scale) as f32, c.y - ((y - cy) * scale) as f32)
    };

    // Reference planes through the origin, as Revit's family templates
    // have, and the host wall for doors and windows.
    let ref_color = Color32::from_rgb(60, 160, 90);
    let dashed = |a: Pos2, b: Pos2| {
        painter.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(1.0, ref_color), 8.0, 5.0));
    };
    let big = (x1 - x0).max(y1 - y0) * 0.6 + 200.0;
    let ref_lines: Vec<(DVec3, DVec3)> = match ed.view {
        PreviewView::Plan => vec![
            (DVec3::new(0.0, -big, 0.0), DVec3::new(0.0, big, 0.0)),
            (DVec3::new(-big, 0.0, 0.0), DVec3::new(big, 0.0, 0.0)),
        ],
        PreviewView::Front => vec![
            (DVec3::new(0.0, 0.0, -big * 0.2), DVec3::new(0.0, 0.0, big)),
            (DVec3::new(-big, 0.0, 0.0), DVec3::new(big, 0.0, 0.0)),
        ],
        PreviewView::Right => vec![
            (DVec3::new(0.0, 0.0, -big * 0.2), DVec3::new(0.0, 0.0, big)),
            (DVec3::new(0.0, -big, 0.0), DVec3::new(0.0, big, 0.0)),
        ],
        PreviewView::ThreeD => vec![],
    };
    for (a, b) in ref_lines {
        dashed(to_screen(a), to_screen(b));
    }
    if hosted && ed.view != PreviewView::ThreeD {
        let wall = Color32::from_gray(150);
        let half = w / 2.0 + 400.0;
        let ht = wall_t / 2.0;
        let lines: Vec<(DVec3, DVec3)> = match ed.view {
            PreviewView::Plan => vec![
                (DVec3::new(-half, -ht, 0.0), DVec3::new(-w / 2.0, -ht, 0.0)),
                (DVec3::new(-half, ht, 0.0), DVec3::new(-w / 2.0, ht, 0.0)),
                (DVec3::new(-w / 2.0, -ht, 0.0), DVec3::new(-w / 2.0, ht, 0.0)),
                (DVec3::new(w / 2.0, -ht, 0.0), DVec3::new(half, -ht, 0.0)),
                (DVec3::new(w / 2.0, ht, 0.0), DVec3::new(half, ht, 0.0)),
                (DVec3::new(w / 2.0, -ht, 0.0), DVec3::new(w / 2.0, ht, 0.0)),
            ],
            PreviewView::Front => vec![
                (DVec3::new(-w / 2.0, 0.0, 0.0), DVec3::new(w / 2.0, 0.0, 0.0)),
                (DVec3::new(w / 2.0, 0.0, 0.0), DVec3::new(w / 2.0, 0.0, h)),
                (DVec3::new(w / 2.0, 0.0, h), DVec3::new(-w / 2.0, 0.0, h)),
                (DVec3::new(-w / 2.0, 0.0, h), DVec3::new(-w / 2.0, 0.0, 0.0)),
            ],
            _ => vec![
                (DVec3::new(0.0, -ht, 0.0), DVec3::new(0.0, -ht, h)),
                (DVec3::new(0.0, ht, 0.0), DVec3::new(0.0, ht, h)),
            ],
        };
        for (a, b) in lines {
            painter.line_segment([to_screen(a), to_screen(b)], Stroke::new(1.5, wall));
        }
    }

    // Faces, far to near, shaded by their angle to the light.
    let has_symbol = !symbols.is_empty();
    let light = (-fwd + up * 0.6 + right * 0.3).normalize();
    let mut order: Vec<(f64, &Face)> = faces
        .iter()
        .map(|f| (f.pts.iter().map(|p| p.dot(fwd)).sum::<f64>() / f.pts.len() as f64, f))
        .collect();
    order.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, f) in order {
        let nrm = hausit_core::geom::polygon_normal(&f.pts).normalize_or_zero();
        let facing = nrm.dot(fwd) < -1e-6;
        if !facing && !f.glass {
            continue;
        }
        let shade = (0.55 + 0.45 * nrm.dot(light).abs()) as f32;
        let [r, g, b] = f.color;
        let k = |v: u8| (v as f32 * shade).min(255.0) as u8;
        let mut fill = if f.glass {
            Color32::from_rgba_unmultiplied(k(r), k(g), k(b), 90)
        } else {
            Color32::from_rgb(k(r), k(g), k(b))
        };
        if ed.view == PreviewView::Plan && has_symbol {
            fill = Color32::from_gray(245);
        }
        let screen: Vec<Pos2> = f.pts.iter().map(|p| to_screen(*p)).collect();
        let edge = if f.selected {
            Stroke::new(2.0, Color32::from_rgb(30, 110, 230))
        } else if ed.view == PreviewView::Plan && has_symbol {
            Stroke::new(0.8, Color32::from_gray(200))
        } else {
            Stroke::new(1.0, Color32::from_gray(40))
        };
        painter.add(egui::Shape::convex_polygon(screen.clone(), fill, Stroke::NONE));
        let mut closed = screen;
        closed.push(closed[0]);
        painter.add(egui::Shape::line(closed, edge));
    }
    for (a, b, selected) in &symbols {
        let stroke = if *selected {
            Stroke::new(2.0, Color32::from_rgb(30, 110, 230))
        } else {
            Stroke::new(1.4, Color32::BLACK)
        };
        painter.line_segment([to_screen(a.extend(0.0)), to_screen(b.extend(0.0))], stroke);
    }
    let caption = match ed.view {
        PreviewView::Plan if has_symbol => "Plan: the plan symbol, over the solids in grey",
        PreviewView::Plan => "Plan: the solids' outlines (add a plan symbol to draw something else)",
        PreviewView::Front if hosted => "Interior elevation, looking at the wall from inside",
        PreviewView::Front => "Front elevation",
        PreviewView::Right => "Right elevation",
        PreviewView::ThreeD => "3D",
    };
    painter.text(
        rect.left_bottom() + egui::vec2(8.0, -8.0),
        egui::Align2::LEFT_BOTTOM,
        caption,
        egui::FontId::proportional(12.0),
        Color32::from_gray(100),
    );
}
