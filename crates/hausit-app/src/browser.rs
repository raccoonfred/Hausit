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
    let key = egui::Id::unique("browser_filter");
    let mut filter: String = ui.ctx().data(|d| d.get_temp(key)).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::singleline(&mut filter)
                .hint_text("🔍 Search views, sheets, families")
                .desired_width(ui.available_width() - 30.0),
        );
        if !filter.is_empty() && ui.small_button("✖").on_hover_text("Clear the search").clicked() {
            filter.clear();
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(key, filter.clone()));
    let f = filter.trim().to_lowercase();
    let hit = |name: &str| f.is_empty() || name.to_lowercase().contains(&f);
    // While searching, every branch with a match is open.
    let open = if f.is_empty() { None } else { Some(true) };
    egui::ScrollArea::vertical()
        .id_salt("browser_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(2.0);
            CollapsingHeader::new("Views (all)").default_open(true).open(open).show(ui, |ui| {
                for group in [
                    "Floor Plans",
                    "Ceiling Plans",
                    "Area Plans (Gross Building)",
                    "Area Plans (Rentable)",
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
                            } else if let Some(s) = v.area_scheme {
                                match s {
                                    hausit_model::AreaScheme::Gross => "Area Plans (Gross Building)",
                                    hausit_model::AreaScheme::Rentable => "Area Plans (Rentable)",
                                }
                            } else {
                                v.kind.browser_group()
                            };
                            g == group && hit(&v.name)
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
                    CollapsingHeader::new(group).default_open(true).open(open).show(ui, |ui| {
                        for (id, name, _) in views {
                            view_item(app, ui, id, &name);
                        }
                    });
                }
            });
            let schedules: Vec<(ElementId, String)> = app
                .doc
                .views()
                .filter(|v| matches!(v.kind, ViewKind::Schedule(_)) && hit(&v.name))
                .map(|v| (v.id, v.name.clone()))
                .collect();
            CollapsingHeader::new("Schedules/Quantities (all)")
                .default_open(true)
                .open(open)
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
            CollapsingHeader::new("Sheets (all)").default_open(true).open(open).show(ui, |ui| {
                for (id, label) in crate::sheets::sheets_in_order(&app.doc).into_iter().filter(|(_, l)| hit(l)) {
                    view_item(app, ui, id, &label);
                }
                if ui.small_button("New Sheet…").clicked() {
                    crate::sheets::start_new_sheet(app);
                }
            });
            CollapsingHeader::new("Families").default_open(false).open(open).show(ui, |ui| {
                for cat in Category::TYPED {
                    let type_hit = |t: &hausit_model::FamilyType| hit(&t.name) || hit(&t.family) || hit(cat.name());
                    if !app.doc.types_of(cat).any(type_hit) {
                        continue;
                    }
                    CollapsingHeader::new(cat.name())
                        .id_salt(("fam", cat.name()))
                        .open(open)
                        .show(ui, |ui| {
                            let mut families: Vec<String> =
                                app.doc.types_of(cat).filter(|t| type_hit(t)).map(|t| t.family.clone()).collect();
                            families.dedup();
                            for fam in families {
                                CollapsingHeader::new(&fam)
                                    .id_salt(("fam", cat.name(), &fam))
                                    .open(open)
                                    .show(ui, |ui| {
                                        let types: Vec<(ElementId, String)> = app
                                            .doc
                                            .types_of(cat)
                                            .filter(|t| t.family == fam && type_hit(t))
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
            crate::groups::browser_node(app, ui);
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
            // Sheets are listed as "A101 - Name"; rename the name.
            let text = app.doc.view(id).map(|v| v.name.clone()).unwrap_or_else(|| name.to_string());
            app.dialogs.rename = Some(crate::dialogs::Rename {
                target: id,
                text,
                is_type: false,
            });
            ui.close();
        }
        let graphical = app
            .doc
            .view(id)
            .is_some_and(|v| v.kind.is_graphical() && !matches!(v.kind, ViewKind::Sheet(_)));
        if graphical {
            ui.menu_button("Duplicate View", |ui| {
                for (label, detailing) in [("Duplicate", false), ("Duplicate with Detailing", true)] {
                    if ui.button(label).clicked() {
                        if let Some(Some(new)) = app.transact("Duplicate View", |doc| Ok(doc.duplicate_view(id, detailing))) {
                            app.open_view(new);
                        }
                        ui.close();
                    }
                }
            });
        }
        // A level keeps at least one floor plan; extra ones can go.
        let is_plan = match app.doc.view(id).map(|v| &v.kind) {
            Some(ViewKind::FloorPlan { level }) => {
                app.doc
                    .views()
                    .filter(|v| matches!(v.kind, ViewKind::FloorPlan { level: l } if l == *level))
                    .count()
                    < 2
            }
            _ => false,
        };
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
    r.on_hover_text("Double-click for Type Properties; right-click to place an instance")
        .context_menu(|ui| {
            if ui.button("Create Instance").clicked() {
                create_instance(app, cat, id);
                ui.close();
            }
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
                // Sheets are listed as "A101 - Name"; rename the name.
                let text = app.doc.view(id).map(|v| v.name.clone()).unwrap_or_else(|| name.to_string());
                app.dialogs.rename = Some(crate::dialogs::Rename {
                    target: id,
                    text,
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

/// Browser ▸ type ▸ Create Instance: starts placing that type.
fn create_instance(app: &mut HausitApp, cat: Category, id: ElementId) {
    use crate::tools::{Sketch, Tool};
    let tool = match cat {
        Category::Walls => Tool::Wall {
            start: None,
            structural: false,
        },
        Category::Doors => Tool::Door,
        Category::Windows => Tool::Window,
        Category::Floors => Tool::Floor {
            sketch: Sketch::default(),
            structural: false,
        },
        Category::Roofs => Tool::Roof { sketch: Sketch::default() },
        Category::Ceilings => Tool::Ceiling,
        Category::StructuralColumns => Tool::Column,
        Category::StructuralFraming => Tool::Beam { start: None, brace: false },
        Category::Stairs => Tool::Stair { pts: vec![] },
        Category::Railings => Tool::Railing { pts: vec![] },
        c if Category::COMPONENTS.contains(&c) || c == Category::Planting => Tool::Component { category: c },
        _ => {
            app.status = format!("{} are not placed from the browser.", cat.name());
            return;
        }
    };
    app.current_type.insert(cat, id);
    if let Tool::Component { category } = tool {
        app.options.last_component = Some(category);
    }
    crate::tools::start(app, tool);
}
