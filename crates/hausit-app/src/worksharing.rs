//! Collaborate ▸ Worksets and Manage ▸ Design Options: their dialogs, the
//! Visibility/Graphics pages for them, and the status bar pickers.

use crate::app::HausitApp;
use egui::{Context, RichText, Ui};
use hausit_core::ElementId;
use hausit_model::WorksetVisibility;

#[derive(Default)]
pub struct WorksetsDialog {
    pub selected: Option<ElementId>,
    pub new_name: String,
}

#[derive(Default)]
pub struct OptionsDialog {
    pub selected_set: Option<ElementId>,
    pub selected_option: Option<ElementId>,
}

pub fn show(app: &mut HausitApp, ctx: &Context) {
    if let Some(mut d) = app.dialogs.worksets.take() {
        let mut open = true;
        egui::Window::new("Worksets")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .show(ctx, |ui| worksets(app, ui, &mut d));
        if open {
            app.dialogs.worksets = Some(d);
        }
    }
    if let Some(mut d) = app.dialogs.design_options.take() {
        let mut open = true;
        egui::Window::new("Design Options")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| design_options(app, ui, &mut d));
        if open {
            app.dialogs.design_options = Some(d);
        }
    }
}

fn worksets(app: &mut HausitApp, ui: &mut Ui, d: &mut WorksetsDialog) {
    if !app.doc.worksharing() {
        ui.label("Worksharing lets a project's elements be divided into worksets that can be closed, or hidden per view.");
        ui.add_space(4.0);
        ui.label("Enabling it puts levels and grids on \"Shared Levels and Grids\" and everything else on \"Workset1\".");
        ui.add_space(6.0);
        if ui.button("Enable Worksharing").clicked() {
            app.transact("Enable Worksharing", |doc| doc.enable_worksharing());
        }
        return;
    }
    ui.horizontal(|ui| {
        ui.label("Active workset:");
        active_workset_combo(app, ui, "ws_dialog_active");
    });
    ui.add_space(4.0);
    let mut edited: Option<hausit_model::Workset> = None;
    egui::Grid::new("worksets_grid").num_columns(4).striped(true).show(ui, |ui| {
        for h in ["Name", "Opened", "Visible in all views", "Elements"] {
            ui.label(RichText::new(h).strong());
        }
        ui.end_row();
        for w in &app.doc.worksets {
            let mut e = w.clone();
            let shared = w.name == hausit_model::worksharing::SHARED_LEVELS_AND_GRIDS;
            let r = ui.add_enabled(!shared, egui::TextEdit::singleline(&mut e.name).desired_width(200.0));
            if r.gained_focus() || r.clicked() {
                d.selected = Some(w.id);
            }
            if ui.selectable_label(d.selected == Some(w.id), "").clicked() {
                d.selected = Some(w.id);
            }
            ui.checkbox(&mut e.open, "");
            ui.checkbox(&mut e.visible_by_default, "");
            ui.label(app.doc.elements().filter(|x| x.workset == Some(w.id)).count().to_string());
            if e != *w && !e.name.trim().is_empty() {
                edited = Some(e);
            }
            ui.end_row();
        }
    });
    if let Some(e) = edited {
        let name_clash = app.doc.worksets.iter().any(|w| w.id != e.id && w.name == e.name);
        if !name_clash {
            app.transact("Edit Workset", |doc| {
                if let Some(w) = doc.worksets.iter_mut().find(|w| w.id == e.id) {
                    *w = e;
                }
                Ok(())
            });
        }
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut d.new_name)
                .hint_text("New workset name")
                .desired_width(180.0),
        );
        if ui.button("New").clicked() {
            let name = std::mem::take(&mut d.new_name);
            let name = if name.trim().is_empty() {
                let n = (1..)
                    .find(|n| !app.doc.worksets.iter().any(|w| w.name == format!("Workset{n}")))
                    .unwrap_or(1);
                format!("Workset{n}")
            } else {
                name
            };
            d.selected = app.transact("New Workset", |doc| doc.add_workset(&name)).or(d.selected);
        }
        let sel = d.selected.filter(|s| app.doc.workset(*s).is_some());
        let target = app.doc.active_workset.filter(|a| Some(*a) != sel).or_else(|| {
            app.doc
                .worksets
                .iter()
                .find(|w| Some(w.id) != sel && w.name != hausit_model::worksharing::SHARED_LEVELS_AND_GRIDS)
                .map(|w| w.id)
        });
        let label = format!("Delete (move elements to {})", app.doc.workset_name(target));
        if ui
            .add_enabled(sel.is_some() && target.is_some(), egui::Button::new(label))
            .clicked()
        {
            if let (Some(s), Some(t)) = (sel, target) {
                if app.transact("Delete Workset", |doc| doc.delete_workset(s, t)).is_some() {
                    d.selected = None;
                }
            }
        }
    });
    ui.label(
        RichText::new("Closed worksets show in no view. Views can override \"Visible in all views\" in Visibility/Graphics.")
            .small()
            .color(egui::Color32::DARK_GRAY),
    );
}

/// The active workset picker (status bar and dialog).
pub fn active_workset_combo(app: &mut HausitApp, ui: &mut Ui, id: &str) {
    let current = app.doc.active_workset;
    let mut pick = current;
    egui::ComboBox::from_id_salt(id)
        .selected_text(app.doc.workset_name(current))
        .width(160.0)
        .show_ui(ui, |ui| {
            for w in &app.doc.worksets {
                ui.selectable_value(&mut pick, Some(w.id), w.name.clone());
            }
        })
        .response
        .on_hover_text("Active workset: new elements go on it");
    if pick != current {
        // Not an undoable model change in Revit either.
        app.doc.active_workset = pick;
        app.doc.touch();
    }
}

/// The design option being edited (status bar).
pub fn editing_option_combo(app: &mut HausitApp, ui: &mut Ui) {
    let current = app.doc.editing_option;
    let mut pick = current;
    egui::ComboBox::from_id_salt("editing_option")
        .selected_text(app.doc.option_label(current))
        .width(200.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut pick, None, "Main Model");
            for s in &app.doc.option_sets {
                for o in &s.options {
                    ui.selectable_value(&mut pick, Some(o.id), app.doc.option_label(Some(o.id)));
                }
            }
        })
        .response
        .on_hover_text("Design option being edited: new elements go into it, and only its elements can be picked");
    if pick != current {
        app.doc.editing_option = pick;
        app.selection.clear();
        app.doc.touch();
    }
}

fn design_options(app: &mut HausitApp, ui: &mut Ui, d: &mut OptionsDialog) {
    ui.horizontal(|ui| {
        ui.label("Now editing:");
        editing_option_combo(app, ui);
    });
    ui.separator();
    if app.doc.option_sets.is_empty() {
        ui.label("No option sets yet. An option set holds alternatives for one part of the design.");
    }
    for s in app.doc.option_sets.clone() {
        let set_sel = d.selected_set == Some(s.id);
        if ui
            .selectable_label(set_sel && d.selected_option.is_none(), RichText::new(&s.name).strong())
            .clicked()
        {
            d.selected_set = Some(s.id);
            d.selected_option = None;
        }
        for o in &s.options {
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                let n = app.doc.elements().filter(|e| e.design_option == Some(o.id)).count();
                let label = format!("{}{}  ({n} elements)", o.name, if s.primary == o.id { " (primary)" } else { "" });
                if ui.selectable_label(d.selected_option == Some(o.id), label).clicked() {
                    d.selected_set = Some(s.id);
                    d.selected_option = Some(o.id);
                }
            });
        }
    }
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("New Option Set").clicked() {
            let n = app.doc.option_sets.len() + 1;
            if let Some((set, _)) = app.transact("New Option Set", |doc| Ok(doc.add_option_set(&format!("Option Set {n}")))) {
                d.selected_set = Some(set);
                d.selected_option = None;
            }
        }
        if ui.add_enabled(d.selected_set.is_some(), egui::Button::new("New Option")).clicked() {
            if let Some(set) = d.selected_set {
                d.selected_option = app.transact("New Option", |doc| doc.add_option(set)).or(d.selected_option);
            }
        }
        let opt = d.selected_option;
        if ui.add_enabled(opt.is_some(), egui::Button::new("Make Primary")).clicked() {
            if let Some(o) = opt {
                app.transact("Make Primary", |doc| {
                    doc.make_primary(o);
                    Ok(())
                });
            }
        }
        if ui.add_enabled(opt.is_some(), egui::Button::new("Delete Option")).clicked() {
            if let Some(o) = opt {
                if app.transact("Delete Option", |doc| doc.delete_option(o)).is_some() {
                    d.selected_option = None;
                }
            }
        }
        let set = d.selected_set;
        if ui
            .add_enabled(set.is_some(), egui::Button::new("Accept Primary"))
            .on_hover_text("Keeps the primary option as part of the main model and deletes the other options")
            .clicked()
        {
            if let Some(s) = set {
                if app.transact("Accept Primary", |doc| doc.accept_primary(s)).is_some() {
                    d.selected_set = None;
                    d.selected_option = None;
                }
            }
        }
    });
    // Renaming the selection.
    if let Some(set) = d.selected_set {
        let mut name = match d.selected_option {
            Some(o) => app
                .doc
                .option_set(set)
                .and_then(|s| s.options.iter().find(|x| x.id == o))
                .map(|x| x.name.clone())
                .unwrap_or_default(),
            None => app.doc.option_set(set).map(|s| s.name.clone()).unwrap_or_default(),
        };
        ui.horizontal(|ui| {
            ui.label("Name:");
            if ui.text_edit_singleline(&mut name).changed() && !name.trim().is_empty() {
                if let Some(s) = app.doc.option_sets.iter_mut().find(|s| s.id == set) {
                    match d.selected_option {
                        Some(o) => {
                            if let Some(x) = s.options.iter_mut().find(|x| x.id == o) {
                                x.name = name;
                            }
                        }
                        None => s.name = name,
                    }
                }
                app.doc.touch();
            }
        });
    }
    ui.add_space(4.0);
    ui.label(
        RichText::new("Add to Set (Manage tab) moves the selected main model elements into options of a set.")
            .small()
            .color(egui::Color32::DARK_GRAY),
    );
}

/// Add to Set: the selection goes into every option of the set picked.
pub fn add_to_set_menu(app: &mut HausitApp, ui: &mut Ui) {
    let ids = app.selection.clone();
    if app.doc.option_sets.is_empty() {
        ui.label("Create an option set first (Design Options).");
        return;
    }
    for s in app.doc.option_sets.clone() {
        ui.menu_button(&s.name, |ui| {
            let all: Vec<ElementId> = s.options.iter().map(|o| o.id).collect();
            if ui.button("All options").clicked() {
                app.transact("Add to Set", |doc| doc.add_to_set(&ids, &all));
                ui.close();
            }
            for o in &s.options {
                if ui.button(&o.name).clicked() {
                    app.transact("Add to Set", |doc| doc.add_to_set(&ids, &[o.id]));
                    ui.close();
                }
            }
        });
    }
}

/// Visibility/Graphics ▸ Worksets.
pub fn vg_worksets(doc: &hausit_model::Document, ui: &mut Ui, rows: &mut Vec<(ElementId, WorksetVisibility)>) {
    if !doc.worksharing() {
        ui.label("Worksharing is not enabled (Collaborate ▸ Worksets).");
        return;
    }
    egui::Grid::new("vg_worksets").num_columns(2).striped(true).show(ui, |ui| {
        ui.label(RichText::new("Workset").strong());
        ui.label(RichText::new("Visibility Setting").strong());
        ui.end_row();
        for w in &doc.worksets {
            ui.label(&w.name);
            let mut v = rows.iter().find(|(x, _)| *x == w.id).map(|r| r.1).unwrap_or_default();
            egui::ComboBox::from_id_salt(("vg_ws", w.id))
                .selected_text(v.label())
                .width(170.0)
                .show_ui(ui, |ui| {
                    for x in WorksetVisibility::ALL {
                        ui.selectable_value(&mut v, x, x.label());
                    }
                });
            rows.retain(|(x, _)| *x != w.id);
            if v != WorksetVisibility::UseGlobal {
                rows.push((w.id, v));
            }
            ui.end_row();
        }
    });
}

/// Visibility/Graphics ▸ Design Options.
pub fn vg_options(doc: &hausit_model::Document, ui: &mut Ui, rows: &mut Vec<(ElementId, ElementId)>) {
    if doc.option_sets.is_empty() {
        ui.label("The project has no design options (Manage ▸ Design Options).");
        return;
    }
    egui::Grid::new("vg_options").num_columns(2).striped(true).show(ui, |ui| {
        ui.label(RichText::new("Design Option Set").strong());
        ui.label(RichText::new("Design Option").strong());
        ui.end_row();
        for s in &doc.option_sets {
            ui.label(&s.name);
            let mut v = rows.iter().find(|(x, _)| *x == s.id).map(|r| r.1);
            let name = |o: Option<ElementId>| match o {
                None => "<Automatic>".to_string(),
                Some(o) => s.options.iter().find(|x| x.id == o).map(|x| x.name.clone()).unwrap_or_default(),
            };
            egui::ComboBox::from_id_salt(("vg_opt", s.id))
                .selected_text(name(v))
                .width(170.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut v, None, "<Automatic>");
                    for o in &s.options {
                        ui.selectable_value(&mut v, Some(o.id), o.name.clone());
                    }
                });
            rows.retain(|(x, _)| *x != s.id);
            if let Some(o) = v {
                rows.push((s.id, o));
            }
            ui.end_row();
        }
    });
}
