//! Insert ▸ Link and Manage Links: linking other Hausit projects or IFC
//! files, and reloading, unloading or removing them.

use crate::app::HausitApp;
use egui::{Context, RichText};
use std::path::{Path, PathBuf};

/// Reads a project to link: a Hausit project or an IFC file.
pub fn read_linked(path: &Path) -> Result<hausit_model::Document, String> {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ifc")) {
        hausit_io::import_ifc(path).map(|(doc, _)| doc).map_err(|e| e.to_string())
    } else {
        hausit_io::load(path).map_err(|e| e.to_string())
    }
}

/// Where a link's path points, relative paths read from the host's folder.
fn resolve(app: &HausitApp, path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        return p;
    }
    match app.path.as_ref().and_then(|h| h.parent()) {
        Some(dir) => dir.join(p),
        None => p,
    }
}

/// Insert ▸ Link: pick a file and place it origin to origin.
pub fn link_dialog(app: &mut HausitApp) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Hausit project or IFC", &[hausit_io::EXTENSION, "ifc"])
        .pick_file()
    else {
        return;
    };
    link_file(app, &path);
}

pub fn link_file(app: &mut HausitApp, path: &Path) {
    if app.path.as_deref() == Some(path) {
        app.dialogs.error = Some("A project can't link itself.".into());
        return;
    }
    let linked = match read_linked(path) {
        Ok(d) => d,
        Err(e) => {
            app.dialogs.error = Some(format!("Could not link {}:\n{e}", path.display()));
            return;
        }
    };
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let path_text = path.to_string_lossy().to_string();
    let placed = app.transact("Link", |doc| {
        let link = doc.add_link(&name, &path_text);
        doc.set_link_model(link, linked);
        doc.place_link(link)
    });
    if let Some(id) = placed {
        app.selection = vec![id];
        app.status = format!("Linked {name}, origin to origin. Move or rotate it like any element.");
    }
}

/// Loads every link of the open project (after opening it).
pub fn load_all(app: &mut HausitApp) {
    let mut missing = Vec::new();
    for l in app.doc.links.clone() {
        if l.unloaded {
            continue;
        }
        match read_linked(&resolve(app, &l.path)) {
            Ok(d) => app.doc.set_link_model(l.id, d),
            Err(_) => missing.push(l.name),
        }
    }
    // Loading is not an edit.
    app.saved_revision = app.doc.revision();
    if !missing.is_empty() {
        app.status = format!("Links not found: {}. See Manage Links.", missing.join(", "));
    }
}

#[derive(Default)]
pub struct ManageLinks {
    pub selected: Option<hausit_core::ElementId>,
}

pub fn show(app: &mut HausitApp, ctx: &Context) {
    let Some(mut d) = app.dialogs.manage_links.take() else { return };
    let mut open = true;
    egui::Window::new("Manage Links")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(620.0)
        .show(ctx, |ui| {
            if app.doc.links.is_empty() {
                ui.label("No links. Insert ▸ Link adds a Hausit project or an IFC file.");
            }
            egui::Grid::new("links_grid").num_columns(4).striped(true).show(ui, |ui| {
                for h in ["Link Name", "Status", "Instances", "Saved Path"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for l in &app.doc.links {
                    if ui.selectable_label(d.selected == Some(l.id), &l.name).clicked() {
                        d.selected = Some(l.id);
                    }
                    let status = if l.unloaded {
                        "Unloaded"
                    } else if app.doc.link_model(l.id).is_some() {
                        "Loaded"
                    } else {
                        "Not Found"
                    };
                    ui.label(status);
                    let n = app
                        .doc
                        .elements_of(hausit_model::Category::RvtLinks)
                        .filter(|e| matches!(&e.data, hausit_model::ElementData::LinkInstance(i) if i.link == l.id))
                        .count();
                    ui.label(n.to_string());
                    ui.label(RichText::new(&l.path).small());
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            let sel = d.selected.filter(|s| app.doc.link_type(*s).is_some());
            ui.horizontal(|ui| {
                if ui.add_enabled(sel.is_some(), egui::Button::new("Reload")).clicked() {
                    if let Some(id) = sel {
                        let path = resolve(app, &app.doc.link_type(id).map(|l| l.path.clone()).unwrap_or_default());
                        match read_linked(&path) {
                            Ok(m) => {
                                app.doc.set_link_model(id, m);
                                app.status = "Link reloaded.".into();
                            }
                            Err(e) => app.dialogs.error = Some(format!("Could not reload {}:\n{e}", path.display())),
                        }
                    }
                }
                if ui.add_enabled(sel.is_some(), egui::Button::new("Reload From…")).clicked() {
                    if let (Some(id), Some(p)) = (
                        sel,
                        rfd::FileDialog::new()
                            .add_filter("Hausit project or IFC", &[hausit_io::EXTENSION, "ifc"])
                            .pick_file(),
                    ) {
                        match read_linked(&p) {
                            Ok(m) => {
                                let text = p.to_string_lossy().to_string();
                                app.transact("Reload Link From", |doc| {
                                    if let Some(l) = doc.links.iter_mut().find(|l| l.id == id) {
                                        l.path = text;
                                    }
                                    doc.set_link_model(id, m);
                                    Ok(())
                                });
                            }
                            Err(e) => app.dialogs.error = Some(format!("Could not load {}:\n{e}", p.display())),
                        }
                    }
                }
                if ui.add_enabled(sel.is_some(), egui::Button::new("Unload")).clicked() {
                    if let Some(id) = sel {
                        app.transact("Unload Link", |doc| {
                            doc.unload_link(id);
                            Ok(())
                        });
                    }
                }
                if ui.add_enabled(sel.is_some(), egui::Button::new("Place Another")).clicked() {
                    if let Some(id) = sel {
                        if let Some(inst) = app.transact("Place Link", |doc| doc.place_link(id)) {
                            app.selection = vec![inst];
                        }
                    }
                }
                if ui.add_enabled(sel.is_some(), egui::Button::new("Remove")).clicked() {
                    if let Some(id) = sel {
                        app.transact("Remove Link", |doc| doc.remove_link(id));
                        d.selected = None;
                    }
                }
            });
            ui.label(
                RichText::new("Linked elements can't be edited here: open the linked project to change it, then Reload.")
                    .small()
                    .color(egui::Color32::DARK_GRAY),
            );
        });
    if open {
        app.dialogs.manage_links = Some(d);
    }
}
