//! The Project Browser: views by type, schedules, and families → types.

use crate::app::HausitApp;
use egui::{CollapsingHeader, RichText, Ui};
use hausit_core::ElementId;
use hausit_model::{Category, ViewKind};

pub fn show(app: &mut HausitApp, ui: &mut Ui) {
    egui::Frame::new()
        .fill(crate::theme::HEADER_BG)
        .inner_margin(egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(format!("Project Browser - {}", app.project_title())).strong());
        });
    egui::ScrollArea::vertical()
        .id_salt("browser_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(2.0);
            CollapsingHeader::new("Views (all)").default_open(true).show(ui, |ui| {
                for group in [
                    "Floor Plans",
                    "Ceiling Plans",
                    "3D Views",
                    "Elevations (Building Elevation)",
                    "Sections (Building Section)",
                    "Detail Views (Callouts)",
                ] {
                    let mut views: Vec<(ElementId, String, f64)> = app
                        .doc
                        .views()
                        .filter(|v| {
                            let g = if v.callout_of.is_some() {
                                "Detail Views (Callouts)"
                            } else {
                                v.kind.browser_group()
                            };
                            g == group
                        })
                        .map(|v| {
                            (
                                v.id,
                                v.name.clone(),
                                v.kind.plan_level().map(|l| app.doc.level_elevation(l)).unwrap_or(0.0),
                            )
                        })
                        .collect();
                    // Plans go up the building, lowest level first.
                    views.sort_by(|a, b| a.2.total_cmp(&b.2));
                    if views.is_empty() {
                        continue;
                    }
                    CollapsingHeader::new(group).default_open(true).show(ui, |ui| {
                        for (id, name, _) in views {
                            view_item(app, ui, id, &name);
                        }
                    });
                }
            });
            let schedules: Vec<(ElementId, String)> = app
                .doc
                .views()
                .filter(|v| matches!(v.kind, ViewKind::Schedule(_)))
                .map(|v| (v.id, v.name.clone()))
                .collect();
            CollapsingHeader::new("Schedules/Quantities (all)")
                .default_open(true)
                .show(ui, |ui| {
                    for (id, name) in schedules {
                        view_item(app, ui, id, &name);
                    }
                    if ui.small_button("New Schedule…").clicked() {
                        app.dialogs.new_schedule = Some(crate::dialogs::NewSchedule {
                            category: Category::Doors,
                            name: String::new(),
                        });
                    }
                });
            CollapsingHeader::new("Families").default_open(false).show(ui, |ui| {
                for cat in Category::TYPED {
                    CollapsingHeader::new(cat.name()).id_salt(("fam", cat.name())).show(ui, |ui| {
                        let mut families: Vec<String> = app.doc.types_of(cat).map(|t| t.family.clone()).collect();
                        families.dedup();
                        for fam in families {
                            CollapsingHeader::new(&fam).id_salt(("fam", cat.name(), &fam)).show(ui, |ui| {
                                let types: Vec<(ElementId, String)> = app
                                    .doc
                                    .types_of(cat)
                                    .filter(|t| t.family == fam)
                                    .map(|t| (t.id, t.name.clone()))
                                    .collect();
                                for (id, name) in types {
                                    type_item(app, ui, cat, id, &name);
                                }
                            });
                        }
                    });
                }
            });
        });
}

fn view_item(app: &mut HausitApp, ui: &mut Ui, id: ElementId, name: &str) {
    let active = app.active_view == Some(id);
    let r = ui.selectable_label(
        active,
        if active {
            RichText::new(name).strong()
        } else {
            RichText::new(name)
        },
    );
    if r.double_clicked() || r.clicked() {
        app.open_view(id);
    }
    r.context_menu(|ui| {
        if ui.button("Open").clicked() {
            app.open_view(id);
            ui.close();
        }
        if ui.button("Rename…").clicked() {
            app.dialogs.rename = Some(crate::dialogs::Rename {
                target: id,
                text: name.to_string(),
                is_type: false,
            });
            ui.close();
        }
        let is_plan = matches!(app.doc.view(id).map(|v| &v.kind), Some(ViewKind::FloorPlan { .. }));
        if ui
            .add_enabled(!is_plan, egui::Button::new("Delete"))
            .on_disabled_hover_text("Delete the level to remove its plan.")
            .clicked()
        {
            app.transact("Delete View", |doc| {
                doc.delete_view(id);
                Ok(())
            });
            ui.close();
        }
    });
}

fn type_item(app: &mut HausitApp, ui: &mut Ui, cat: Category, id: ElementId, name: &str) {
    let r = ui.selectable_label(app.current_type.get(&cat) == Some(&id), name);
    if r.double_clicked() {
        app.dialogs.open_type_properties(&app.doc, id);
    }
    r.context_menu(|ui| {
        if ui.button("Type Properties…").clicked() {
            app.dialogs.open_type_properties(&app.doc, id);
            ui.close();
        }
        if ui.button("Duplicate…").clicked() {
            let new_name = format!("{name} 2");
            if let Some(n) = app.transact("Duplicate Type", |doc| doc.duplicate_type(id, &new_name)) {
                app.dialogs.rename = Some(crate::dialogs::Rename {
                    target: n,
                    text: new_name,
                    is_type: true,
                });
            }
            ui.close();
        }
        if ui.button("Rename…").clicked() {
            app.dialogs.rename = Some(crate::dialogs::Rename {
                target: id,
                text: name.to_string(),
                is_type: true,
            });
            ui.close();
        }
        if ui.button("Delete").clicked() {
            app.transact("Delete Type", |doc| doc.delete_type(id));
            ui.close();
        }
        if ui.button("Select All Instances").clicked() {
            app.selection = app.doc.elements().filter(|e| e.type_id() == Some(id)).map(|e| e.id).collect();
            ui.close();
        }
    });
}
