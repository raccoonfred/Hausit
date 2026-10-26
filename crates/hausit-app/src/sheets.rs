//! Sheets: the New Sheet, Place View and Export PDF dialogs, and the
//! sample projects with their sheet sets.

use crate::app::HausitApp;
use egui::{Context, RichText};
use hausit_core::ElementId;
use hausit_model::{Document, SheetSize, ViewKind};
use std::path::Path;

/// New Sheet dialog: the title block (paper size) and the sheet's name.
pub struct NewSheet {
    pub size: SheetSize,
    pub number: String,
    pub name: String,
}

/// Export PDF dialog: every sheet, and whether it is ticked.
pub struct ExportPdf {
    pub sheets: Vec<(ElementId, bool)>,
}

pub fn sample_house() -> Document {
    let mut doc = hausit_model::template::sample_project();
    hausit_views::sheet::add_sample_sheets(&mut doc);
    doc
}

pub fn sample_warehouse() -> Document {
    let mut doc = hausit_model::template::industrial_sample();
    hausit_views::sheet::add_sample_sheets(&mut doc);
    doc
}

/// The active view, if it is a sheet.
pub fn active_sheet(app: &HausitApp) -> Option<ElementId> {
    let v = app.doc.view(app.active_view?)?;
    matches!(v.kind, ViewKind::Sheet(_)).then_some(v.id)
}

/// Sheets in number order, for the Project Browser and dialogs.
pub fn sheets_in_order(doc: &Document) -> Vec<(ElementId, String)> {
    let mut sheets: Vec<(String, ElementId, String)> = doc
        .sheets()
        .map(|(v, s)| (s.number.clone(), v.id, format!("{} - {}", s.number, v.name)))
        .collect();
    sheets.sort_by(|a, b| (a.0.len(), &a.0).cmp(&(b.0.len(), &b.0)));
    sheets.into_iter().map(|(_, id, label)| (id, label)).collect()
}

/// Writes the given sheets to one PDF, a page per sheet.
pub fn write_pdf(doc: &Document, sheets: &[ElementId], path: &Path) -> std::io::Result<()> {
    let lists: Vec<(hausit_core::DVec2, hausit_views::DisplayList)> = sheets
        .iter()
        .filter_map(|id| {
            let v = doc.view(*id)?;
            let ViewKind::Sheet(def) = &v.kind else { return None };
            Some((def.size.paper(), hausit_views::build(doc, v)))
        })
        .collect();
    let pages: Vec<hausit_views::pdf::Page> = lists
        .iter()
        .map(|(size, list)| hausit_views::pdf::Page { size: *size, list })
        .collect();
    let title = if doc.project_info.name.is_empty() {
        "Hausit"
    } else {
        &doc.project_info.name
    };
    std::fs::write(path, hausit_views::pdf::write(&pages, title))
}

pub fn start_new_sheet(app: &mut HausitApp) {
    app.dialogs.new_sheet = Some(NewSheet {
        size: SheetSize::A1,
        number: app.doc.next_sheet_number(),
        name: "Unnamed".into(),
    });
}

pub fn start_export(app: &mut HausitApp) {
    let active = active_sheet(app);
    let sheets: Vec<(ElementId, bool)> = sheets_in_order(&app.doc)
        .into_iter()
        .map(|(id, _)| (id, active.is_none_or(|a| a == id)))
        .collect();
    if sheets.is_empty() {
        app.dialogs.error = Some("There are no sheets to export. Create one with View ▸ Sheet.".into());
        return;
    }
    app.dialogs.export_pdf = Some(ExportPdf { sheets });
}

pub fn show(app: &mut HausitApp, ctx: &Context) {
    new_sheet(app, ctx);
    place_view(app, ctx);
    export_pdf(app, ctx);
}

fn window(title: &str) -> egui::Window<'_> {
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
}

fn new_sheet(app: &mut HausitApp, ctx: &Context) {
    let Some(ns) = app.dialogs.new_sheet.as_mut() else { return };
    let mut result = None;
    window("New Sheet").show(ctx, |ui| {
        ui.label("Select title block:");
        for s in SheetSize::ALL {
            let p = s.paper();
            ui.radio_value(&mut ns.size, s, format!("{}  ({} x {} mm)", s.title_block(), p.x, p.y));
        }
        ui.add_space(4.0);
        egui::Grid::new("new_sheet").num_columns(2).show(ui, |ui| {
            ui.label("Sheet Number");
            ui.text_edit_singleline(&mut ns.number);
            ui.end_row();
            ui.label("Sheet Name");
            ui.text_edit_singleline(&mut ns.name);
            ui.end_row();
        });
        ui.add_space(4.0);
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
            let ns = app.dialogs.new_sheet.take().unwrap();
            if app.doc.sheets().any(|(_, s)| s.number == ns.number) {
                app.dialogs.error = Some(format!("Sheet number {} is already in use.", ns.number));
                return;
            }
            let id = app.transact("Create Sheet", |doc| Ok(doc.add_sheet(&ns.number, &ns.name, ns.size)));
            if let Some(id) = id {
                app.open_view(id);
                app.status = "Place views on the sheet with View ▸ Place View.".into();
            }
        }
        Some(false) => app.dialogs.new_sheet = None,
        None => {}
    }
}

/// Views that can still go on a sheet: model views on no sheet yet, and
/// schedules not already on this one.
fn placeable_views(doc: &Document, sheet: ElementId) -> Vec<(ElementId, String)> {
    let on_this: Vec<ElementId> = match doc.view(sheet).map(|v| &v.kind) {
        Some(ViewKind::Sheet(s)) => s.viewports.iter().map(|v| v.view).collect(),
        _ => vec![],
    };
    doc.views()
        .filter(|v| !matches!(v.kind, ViewKind::Sheet(_)))
        .filter(|v| match v.kind {
            ViewKind::Schedule(_) => !on_this.contains(&v.id),
            _ => doc.sheet_of(v.id).is_none(),
        })
        .map(|v| {
            let kind = match v.kind {
                _ if v.callout_of.is_some() => "Detail View",
                ViewKind::FloorPlan { .. } => "Floor Plan",
                ViewKind::CeilingPlan { .. } => "Ceiling Plan",
                ViewKind::ThreeD => "3D View",
                ViewKind::Elevation { .. } => "Elevation",
                ViewKind::Section { .. } => "Section",
                ViewKind::Schedule(_) => "Schedule",
                ViewKind::Sheet(_) => "Sheet",
            };
            (v.id, format!("{kind}: {}", v.name))
        })
        .collect()
}

fn place_view(app: &mut HausitApp, ctx: &Context) {
    let Some((sheet, mut chosen)) = app.dialogs.place_view else {
        return;
    };
    let views = placeable_views(&app.doc, sheet);
    let mut result = None;
    window("Views").show(ctx, |ui| {
        if views.is_empty() {
            ui.label("Every view is already on a sheet.");
        }
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for (id, label) in &views {
                let r = ui.selectable_label(chosen == Some(*id), label);
                if r.clicked() {
                    chosen = Some(*id);
                }
                if r.double_clicked() {
                    result = Some(true);
                }
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(chosen.is_some(), egui::Button::new("Add View to Sheet")).clicked() {
                result = Some(true);
            }
            if ui.button("Cancel").clicked() {
                result = Some(false);
            }
        });
    });
    app.dialogs.place_view = Some((sheet, chosen));
    match result {
        Some(true) => {
            app.dialogs.place_view = None;
            if let Some(view) = chosen {
                let r = app.transact("Place View", |doc| hausit_views::sheet::place_view(doc, sheet, view));
                if r.is_some() {
                    app.selection = vec![view];
                    app.status = "Drag the view to move it. Double-click it to open the view.".into();
                }
            }
        }
        Some(false) => app.dialogs.place_view = None,
        None => {}
    }
}

fn export_pdf(app: &mut HausitApp, ctx: &Context) {
    let Some(mut ex) = app.dialogs.export_pdf.take() else { return };
    let labels: std::collections::HashMap<ElementId, String> = sheets_in_order(&app.doc).into_iter().collect();
    let mut result = None;
    window("Export PDF").show(ctx, |ui| {
        ui.label("Sheets to export, one page each at full size:");
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for (id, on) in ex.sheets.iter_mut() {
                ui.checkbox(on, labels.get(id).map(String::as_str).unwrap_or("?"));
            }
        });
        ui.horizontal(|ui| {
            if ui.small_button("Check All").clicked() {
                ex.sheets.iter_mut().for_each(|s| s.1 = true);
            }
            if ui.small_button("Check None").clicked() {
                ex.sheets.iter_mut().for_each(|s| s.1 = false);
            }
        });
        ui.label(RichText::new("Vector output with the view's line weights; text in Helvetica.").small());
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let any = ex.sheets.iter().any(|s| s.1);
            if ui.add_enabled(any, egui::Button::new("Export…")).clicked() {
                result = Some(true);
            }
            if ui.button("Cancel").clicked() {
                result = Some(false);
            }
        });
    });
    match result {
        Some(true) => {
            let ids: Vec<ElementId> = ex.sheets.iter().filter(|s| s.1).map(|s| s.0).collect();
            let name = if app.doc.project_info.name.is_empty() {
                "Sheets.pdf".to_string()
            } else {
                format!("{}.pdf", app.doc.project_info.name)
            };
            if let Some(path) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(name).save_file() {
                match write_pdf(&app.doc, &ids, &path) {
                    Ok(()) => app.status = format!("Exported {} sheet(s) to {}", ids.len(), path.display()),
                    Err(e) => app.dialogs.error = Some(format!("Could not write the PDF: {e}")),
                }
            }
        }
        Some(false) => {}
        None => app.dialogs.export_pdf = Some(ex),
    }
}
