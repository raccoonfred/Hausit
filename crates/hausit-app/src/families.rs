//! Loadable families: Load Family (a `.hfam` file) and Load from Library.

use crate::app::HausitApp;
use crate::tools::{self, Tool};
use egui::{Context, RichText};
use hausit_model::{library, Category, FamilyDef};

/// Opens the Load from Library dialog.
pub fn open_library(app: &mut HausitApp) {
    app.dialogs.library = Some(vec![false; library::FILES.len()]);
}

/// Load Family: picks `.hfam` files and loads them.
pub fn load_from_file(app: &mut HausitApp) {
    let Some(paths) = rfd::FileDialog::new()
        .add_filter("Hausit family", &[hausit_model::component::EXTENSION])
        .pick_files()
    else {
        return;
    };
    let mut defs = Vec::new();
    for p in paths {
        match std::fs::read_to_string(&p)
            .map_err(|e| e.to_string())
            .and_then(|t| FamilyDef::from_json(&t))
        {
            Ok(d) => defs.push(d),
            Err(e) => {
                app.dialogs.error = Some(format!("{} could not be loaded: {e}", p.display()));
                return;
            }
        }
    }
    load(app, defs);
}

/// Loads families in one undoable step and starts the Component tool
/// with the first new type.
fn load(app: &mut HausitApp, defs: Vec<FamilyDef>) {
    if defs.is_empty() {
        return;
    }
    let names: Vec<String> = defs.iter().map(|d| d.name.clone()).collect();
    let added = app.transact("Load Family", |doc| {
        let mut added = Vec::new();
        for d in defs {
            added.extend(doc.load_family(d)?);
        }
        Ok(added)
    });
    let Some(added) = added else { return };
    app.status = format!("Loaded {}.", names.join(", "));
    if let Some(t) = added.first().and_then(|t| app.doc.family_type(*t)) {
        let category = t.category;
        app.current_type.insert(category, t.id);
        if Category::COMPONENTS.contains(&category) {
            app.options.last_component = Some(category);
        }
        if app.active_level().is_some() {
            let tool = match category {
                Category::Doors => Tool::Door,
                Category::Windows => Tool::Window,
                _ => Tool::Component { category },
            };
            tools::start(app, tool);
        }
    }
}

pub fn show(app: &mut HausitApp, ctx: &Context) {
    let Some(mut checked) = app.dialogs.library.take() else { return };
    let defs = library::families();
    let mut result = None;
    egui::Window::new("Load from Library")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label("Hausit's starter families. Ticked families are loaded into the project with all their types.");
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                for cat in [Category::Doors, Category::Windows].into_iter().chain(Category::COMPONENTS) {
                    ui.label(RichText::new(cat.name()).strong());
                    for (i, d) in defs.iter().enumerate().filter(|(_, d)| d.category == cat) {
                        let loaded = app.doc.family_def(d.category, &d.name).is_some();
                        let label = if loaded { format!("{} (loaded)", d.name) } else { d.name.clone() };
                        let types: Vec<&str> = d.types.iter().map(|t| t.name.as_str()).collect();
                        ui.checkbox(&mut checked[i], label)
                            .on_hover_text(format!("{}\nTypes: {}", d.description, types.join(", ")));
                    }
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let any = checked.iter().any(|c| *c);
                if ui.add_enabled(any, egui::Button::new("   Load   ")).clicked() {
                    result = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    result = Some(false);
                }
            });
        });
    match result {
        Some(true) => {
            let chosen: Vec<FamilyDef> = defs.into_iter().zip(&checked).filter(|(_, c)| **c).map(|(d, _)| d).collect();
            load(app, chosen);
        }
        Some(false) => {}
        None => app.dialogs.library = Some(checked),
    }
}
