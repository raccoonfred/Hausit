//! Schedule views: the table, and the Fields/Sorting settings shown in the
//! Properties palette.

use crate::app::HausitApp;
use egui::{Color32, RichText, Ui};
use egui_extras::{Column, TableBuilder};
use hausit_core::ElementId;
use hausit_model::{ScheduleField, ViewKind};

pub fn show(app: &mut HausitApp, ui: &mut Ui, view_id: ElementId) {
    let Some(view) = app.doc.view(view_id) else { return };
    let ViewKind::Schedule(def) = &view.kind else { return };
    let table = hausit_schedules::build_table(&app.doc, def);
    let name = view.name.clone();
    egui::Frame::new().fill(Color32::WHITE).inner_margin(12).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("<{name}>")).strong().size(16.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Export CSV…").clicked() {
                    export_active(app);
                }
            });
        });
        ui.add_space(6.0);
        let mut clicked = None;
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .columns(Column::auto().at_least(90.0), table.headers.len())
            .header(22.0, |mut h| {
                for (i, t) in table.headers.iter().enumerate() {
                    h.col(|ui| {
                        ui.label(RichText::new(format!("{}  {t}", (b'A' + i as u8) as char)).strong());
                    });
                }
            })
            .body(|mut body| {
                for (ri, r) in table.rows.iter().enumerate() {
                    let selected = r.elements.iter().any(|e| app.selection.contains(e));
                    body.row(20.0, |mut row| {
                        row.set_selected(selected);
                        for c in &r.cells {
                            row.col(|ui| {
                                if ui.add(egui::Label::new(&c.text).sense(egui::Sense::click())).clicked() {
                                    clicked = Some(ri);
                                }
                            });
                        }
                    });
                }
                if let Some(t) = &table.totals {
                    body.row(22.0, |mut row| {
                        for c in t {
                            row.col(|ui| {
                                ui.label(RichText::new(c).strong());
                            });
                        }
                    });
                }
            });
        if let Some(ri) = clicked {
            app.selection = table.rows[ri].elements.clone();
        }
    });
}

/// Schedule settings, shown in the Properties palette.
pub fn properties(app: &mut HausitApp, ui: &mut Ui, view_id: ElementId) {
    let Some(ViewKind::Schedule(def)) = app.doc.view(view_id).map(|v| v.kind.clone()) else {
        return;
    };
    let mut d = def.clone();
    egui::CollapsingHeader::new(RichText::new("Fields").strong())
        .default_open(true)
        .show(ui, |ui| {
            let available = ScheduleField::available(d.category);
            let mut move_up = None;
            for (i, f) in d.fields.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(f.label());
                    if i > 0 && ui.small_button("▲").on_hover_text("Move up").clicked() {
                        move_up = Some(i);
                    }
                });
            }
            if let Some(i) = move_up {
                d.fields.swap(i, i - 1);
            }
            ui.separator();
            ui.label(RichText::new("Available fields").color(Color32::DARK_GRAY));
            for f in available {
                let mut on = d.fields.contains(&f);
                if ui.checkbox(&mut on, f.label()).changed() {
                    if on {
                        d.fields.push(f);
                    } else {
                        d.fields.retain(|x| *x != f);
                    }
                }
            }
        });
    egui::CollapsingHeader::new(RichText::new("Sorting/Grouping").strong())
        .default_open(true)
        .show(ui, |ui| {
            let label = d.sort_by.map(|f| f.label()).unwrap_or("(none)");
            egui::ComboBox::from_label("Sort by").selected_text(label).show_ui(ui, |ui| {
                ui.selectable_value(&mut d.sort_by, None, "(none)");
                for f in d.fields.clone() {
                    ui.selectable_value(&mut d.sort_by, Some(f), f.label());
                }
            });
            ui.checkbox(&mut d.itemize, "Itemize every instance");
            ui.checkbox(&mut d.grand_totals, "Grand totals");
        });
    if d != def {
        app.transact("Edit Schedule", |doc| {
            if let Some(v) = doc.view_mut(view_id) {
                v.kind = ViewKind::Schedule(d);
            }
            Ok(())
        });
    }
}

/// Exports the active schedule to CSV.
pub fn export_active(app: &mut HausitApp) {
    let Some(view) = app.active_view.and_then(|v| app.doc.view(v)) else {
        return;
    };
    let ViewKind::Schedule(def) = &view.kind else {
        app.dialogs.error = Some("Open a schedule view first, then export it.".into());
        return;
    };
    let csv = hausit_schedules::to_csv(&hausit_schedules::build_table(&app.doc, def));
    let name = format!("{}.csv", view.name);
    if let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(name).save_file() {
        match std::fs::write(&path, csv) {
            Ok(()) => app.status = format!("Exported {}", path.display()),
            Err(e) => app.dialogs.error = Some(format!("Could not export: {e}")),
        }
    }
}
