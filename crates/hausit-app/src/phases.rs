//! Manage ▸ Phases: the project's phases in time order, and what each
//! phase filter shows.

use crate::app::HausitApp;
use egui::{Context, RichText};
use hausit_core::ElementId;
use hausit_model::{Document, PhaseFilter, PhaseStatus};

pub struct PhasesDialog {
    pub selected: Option<ElementId>,
    pub tab: usize,
}

impl PhasesDialog {
    pub fn new(doc: &Document) -> Self {
        PhasesDialog {
            selected: doc.phases.last().map(|p| p.id),
            tab: 0,
        }
    }
}

pub fn show(app: &mut HausitApp, ctx: &Context) {
    let Some(mut d) = app.dialogs.phases.take() else { return };
    let mut open = true;
    egui::Window::new("Phasing")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut d.tab, 0, "Project Phases");
                ui.selectable_value(&mut d.tab, 1, "Phase Filters");
            });
            ui.separator();
            if d.tab == 0 {
                project_phases(app, ui, &mut d);
            } else {
                phase_filters(ui);
            }
        });
    if open {
        app.dialogs.phases = Some(d);
    }
}

fn project_phases(app: &mut HausitApp, ui: &mut egui::Ui, d: &mut PhasesDialog) {
    ui.label("Past (earliest first)");
    let mut rename: Option<(ElementId, String)> = None;
    egui::Grid::new("phases_grid").num_columns(3).striped(true).show(ui, |ui| {
        ui.label(RichText::new("").strong());
        ui.label(RichText::new("Name").strong());
        ui.label(RichText::new("Description").strong());
        ui.end_row();
        for (i, p) in app.doc.phases.iter().enumerate() {
            if ui.selectable_label(d.selected == Some(p.id), (i + 1).to_string()).clicked() {
                d.selected = Some(p.id);
            }
            let (mut name, mut desc) = (p.name.clone(), p.description.clone());
            let a = ui.add_sized([180.0, 20.0], egui::TextEdit::singleline(&mut name));
            let b = ui.add_sized([220.0, 20.0], egui::TextEdit::singleline(&mut desc));
            if a.gained_focus() || b.gained_focus() {
                d.selected = Some(p.id);
            }
            if (a.changed() && !name.trim().is_empty()) || b.changed() {
                rename = Some((
                    p.id,
                    format!("{}\u{1f}{}", if name.trim().is_empty() { p.name.clone() } else { name }, desc),
                ));
            }
            ui.end_row();
        }
    });
    ui.label("Future");
    if let Some((id, v)) = rename {
        let (name, desc) = v
            .split_once('\u{1f}')
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .unwrap_or_default();
        // Typing edits the phase directly; one undo step per field change
        // would flood the history, so this bypasses it like Revit's grid.
        if let Some(p) = app.doc.phases.iter_mut().find(|p| p.id == id) {
            p.name = name;
            p.description = desc;
        }
        app.doc.touch();
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let sel = d.selected;
        if ui.button("Insert After").clicked() {
            let new = app.transact("Insert Phase", |doc| Ok(doc.add_phase(sel, "Phase")));
            d.selected = new.or(d.selected);
        }
        let i = sel.and_then(|s| app.doc.phases.iter().position(|p| p.id == s));
        if ui.add_enabled(i.is_some(), egui::Button::new("Insert Before")).clicked() {
            let after = i.and_then(|i| i.checked_sub(1)).map(|k| app.doc.phases[k].id);
            let new = app.transact("Insert Phase", |doc| {
                let id = doc.add_phase(after, "Phase");
                if after.is_none() {
                    // Move it to the front.
                    let p = doc.phases.pop().expect("just added");
                    doc.phases.insert(0, p);
                }
                Ok(id)
            });
            d.selected = new.or(d.selected);
        }
        let can_combine = i.is_some() && app.doc.phases.len() > 1;
        if ui
            .add_enabled(can_combine, egui::Button::new("Combine with Previous"))
            .on_hover_text("Removes the phase; its elements and views move to the previous phase (the next one for the first phase).")
            .clicked()
        {
            if let Some(s) = sel {
                app.transact("Combine Phases", |doc| {
                    doc.combine_phase(s);
                    Ok(())
                });
                d.selected = app.doc.phases.last().map(|p| p.id);
            }
        }
    });
}

fn phase_filters(ui: &mut egui::Ui) {
    ui.label("How each filter shows elements by their status in the view's phase.");
    ui.add_space(4.0);
    egui::Grid::new("phase_filters").num_columns(5).striped(true).show(ui, |ui| {
        ui.label(RichText::new("Filter Name").strong());
        for s in PhaseStatus::ALL {
            ui.label(RichText::new(s.label()).strong());
        }
        ui.end_row();
        for f in PhaseFilter::ALL {
            ui.label(f.label());
            for s in PhaseStatus::ALL {
                ui.label(match f.display(s) {
                    hausit_model::PhaseDisplay::ByCategory => "By Category",
                    hausit_model::PhaseDisplay::Overridden => "Overridden",
                    hausit_model::PhaseDisplay::NotDisplayed => "Not Displayed",
                });
            }
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new("Graphic Overrides").strong());
    egui::Grid::new("phase_overrides").num_columns(2).show(ui, |ui| {
        for (s, how) in [
            (PhaseStatus::Existing, "Grey lines, halftone fill"),
            (PhaseStatus::Demolished, "Dashed lines, no fill"),
            (PhaseStatus::New, "By category"),
            (PhaseStatus::Temporary, "Dash-dot halftone lines"),
        ] {
            ui.label(s.label());
            ui.label(how);
            ui.end_row();
        }
    });
}
