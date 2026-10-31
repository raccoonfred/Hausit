//! View ▸ Sheet Issues/Revisions: the project's revision list, and the
//! "Revisions on Sheet" picker.

use crate::app::HausitApp;
use egui::{Context, RichText};
use hausit_core::ElementId;
use hausit_model::{Numbering, ViewKind};

pub struct RevisionsDialog {
    pub selected: Option<ElementId>,
}

pub fn show(app: &mut HausitApp, ctx: &Context) {
    let Some(mut d) = app.dialogs.revisions.take() else { return };
    let mut open = true;
    egui::Window::new("Sheet Issues/Revisions")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(760.0)
        .show(ctx, |ui| table(app, ui, &mut d));
    if open {
        app.dialogs.revisions = Some(d);
    }
}

fn table(app: &mut HausitApp, ui: &mut egui::Ui, d: &mut RevisionsDialog) {
    let mut edited: Option<hausit_model::Revision> = None;
    egui::Grid::new("revisions_grid").num_columns(8).striped(true).show(ui, |ui| {
        for h in [
            "Seq.",
            "Numbering",
            "Number",
            "Date",
            "Description",
            "Issued",
            "Issued to",
            "Issued by",
        ] {
            ui.label(RichText::new(h).strong());
        }
        ui.end_row();
        for (i, r) in app.doc.revisions.iter().enumerate() {
            let mut e = r.clone();
            if ui.selectable_label(d.selected == Some(r.id), (i + 1).to_string()).clicked() {
                d.selected = Some(r.id);
            }
            // An issued revision can only be un-issued.
            ui.add_enabled_ui(!r.issued, |ui| {
                egui::ComboBox::from_id_salt(("rev_numbering", r.id))
                    .selected_text(e.numbering.label())
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        for n in Numbering::ALL {
                            ui.selectable_value(&mut e.numbering, n, n.label());
                        }
                    });
            });
            ui.label(app.doc.revision_number(r.id));
            let mut fields = Vec::new();
            for (value, w) in [(&mut e.date, 90.0), (&mut e.description, 200.0)] {
                fields.push(ui.add_enabled(!r.issued, egui::TextEdit::singleline(value).desired_width(w)));
            }
            ui.checkbox(&mut e.issued, "");
            for value in [&mut e.issued_to, &mut e.issued_by] {
                fields.push(ui.add_enabled(!r.issued, egui::TextEdit::singleline(value).desired_width(90.0)));
            }
            if fields.iter().any(|f| f.gained_focus()) {
                d.selected = Some(r.id);
            }
            if e != *r {
                edited = Some(e);
            }
            ui.end_row();
        }
    });
    if let Some(e) = edited {
        // Text edits change the list directly, as typing in Revit's grid
        // does; numbering and issuing are undoable steps.
        let structural = app
            .doc
            .revision_def(e.id)
            .is_some_and(|r| r.numbering != e.numbering || r.issued != e.issued);
        if structural {
            app.transact("Edit Revisions", |doc| {
                if let Some(r) = doc.revisions.iter_mut().find(|r| r.id == e.id) {
                    *r = e;
                }
                Ok(())
            });
        } else {
            if let Some(r) = app.doc.revisions.iter_mut().find(|r| r.id == e.id) {
                *r = e;
            }
            app.doc.touch();
        }
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button("Add").clicked() {
            d.selected = app.transact("Add Revision", |doc| Ok(doc.add_revision())).or(d.selected);
        }
        let sel = d.selected.filter(|s| app.doc.revision_def(*s).is_some());
        if ui.add_enabled(sel.is_some(), egui::Button::new("Delete")).clicked() {
            if let Some(s) = sel {
                if app.transact("Delete Revision", |doc| doc.delete_revision(s)).is_some() {
                    d.selected = app.doc.revisions.last().map(|r| r.id);
                }
            }
        }
        for (label, up) in [("Move Up", true), ("Move Down", false)] {
            if ui.add_enabled(sel.is_some(), egui::Button::new(label)).clicked() {
                if let Some(s) = sel {
                    app.transact("Move Revision", |doc| {
                        doc.move_revision(s, up);
                        Ok(())
                    });
                }
            }
        }
    });
    ui.label(
        RichText::new("Issuing a revision locks its clouds. Numbers count up in sequence, numeric and alphabetic separately.")
            .small()
            .color(egui::Color32::DARK_GRAY),
    );
}

/// Sheet properties: which revisions the sheet carries. Revisions with
/// clouds in the sheet's views are always on it.
pub fn sheet_revisions(app: &mut HausitApp, ui: &mut egui::Ui, sheet: ElementId) {
    let Some(ViewKind::Sheet(def)) = app.doc.view(sheet).map(|v| &v.kind) else {
        return;
    };
    let ticked = def.revisions.clone();
    let on_sheet = app.doc.sheet_revisions(sheet);
    let mut change: Option<(ElementId, bool)> = None;
    egui::CollapsingHeader::new(RichText::new("Revisions on Sheet").strong())
        .default_open(true)
        .show(ui, |ui| {
            for r in &app.doc.revisions {
                let by_cloud = on_sheet.contains(&r.id) && !ticked.contains(&r.id);
                let mut on = on_sheet.contains(&r.id);
                let label = format!("{} - {} ({})", app.doc.revision_number(r.id), r.description, r.date);
                let resp = ui.add_enabled(!by_cloud, egui::Checkbox::new(&mut on, label));
                if resp.changed() {
                    change = Some((r.id, on));
                }
                if by_cloud {
                    resp.on_disabled_hover_text("A revision cloud of this revision is on the sheet.");
                }
            }
        });
    if let Some((id, on)) = change {
        app.transact("Revisions on Sheet", |doc| {
            if let Some(ViewKind::Sheet(s)) = doc.view_mut(sheet).map(|v| &mut v.kind) {
                s.revisions.retain(|r| *r != id);
                if on {
                    s.revisions.push(id);
                }
            }
            Ok(())
        });
    }
}
