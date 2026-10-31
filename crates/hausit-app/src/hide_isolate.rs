//! Temporary Hide/Isolate (the sunglasses on the View Control Bar), Hide in
//! View, Pin, and Copy/Paste through Hausit's clipboard.

use crate::app::HausitApp;
use egui::{Context, RichText, Ui};
use hausit_core::ElementId;
use hausit_model::{Category, TempHide};

/// What Hide/Isolate acts on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HideCmd {
    IsolateCategory,
    HideCategory,
    IsolateElement,
    HideElement,
    Reset,
}

fn selected_elements(app: &HausitApp) -> Vec<ElementId> {
    app.selection.iter().copied().filter(|i| app.doc.element(*i).is_some()).collect()
}

/// Applies a Temporary Hide/Isolate command to the active view.
pub fn temporary(app: &mut HausitApp, cmd: HideCmd) {
    let Some(view_id) = app.active_view else { return };
    let ids = selected_elements(app);
    if cmd != HideCmd::Reset && ids.is_empty() {
        app.status = "Select elements first.".into();
        return;
    }
    let mut cats: Vec<Category> = ids.iter().filter_map(|i| app.doc.element(*i)).map(|e| e.category()).collect();
    cats.sort();
    cats.dedup();
    let t = match cmd {
        HideCmd::IsolateCategory => Some(TempHide {
            isolate: true,
            elements: vec![],
            categories: cats,
        }),
        HideCmd::HideCategory => Some(TempHide {
            isolate: false,
            elements: vec![],
            categories: cats,
        }),
        HideCmd::IsolateElement => Some(TempHide {
            isolate: true,
            elements: ids,
            categories: vec![],
        }),
        HideCmd::HideElement => Some(TempHide {
            isolate: false,
            elements: ids,
            categories: vec![],
        }),
        HideCmd::Reset => None,
    };
    if let Some(v) = app.doc.view_mut(view_id) {
        v.temp_hide = t;
    }
    if cmd != HideCmd::Reset {
        app.selection.clear();
    }
    app.status = match cmd {
        HideCmd::Reset => "Temporary Hide/Isolate reset.".into(),
        _ => "Temporary Hide/Isolate is on: HR resets it.".into(),
    };
}

/// Hide in View ▸ Elements (EH): hides the selection in the active view
/// for good (undoable; View ▸ Reveal Hidden lists them).
pub fn hide_in_view(app: &mut HausitApp) {
    let (Some(view_id), ids) = (app.active_view, selected_elements(app)) else {
        return;
    };
    if ids.is_empty() {
        return;
    }
    app.transact("Hide in View", |doc| {
        if let Some(v) = doc.view_mut(view_id) {
            v.hidden_elements.extend(ids.iter().copied());
        }
        Ok(())
    });
    app.selection.clear();
}

/// Unhides every element hidden with Hide in View in the active view.
pub fn unhide_all(app: &mut HausitApp) {
    let Some(view_id) = app.active_view else { return };
    app.transact("Unhide in View", |doc| {
        if let Some(v) = doc.view_mut(view_id) {
            v.hidden_elements.clear();
        }
        Ok(())
    });
}

pub fn pin(app: &mut HausitApp, pinned: bool) {
    let ids = selected_elements(app);
    if ids.is_empty() {
        app.status = "Select elements first.".into();
        return;
    }
    app.transact(if pinned { "Pin" } else { "Unpin" }, |doc| {
        doc.set_pinned(&ids, pinned);
        Ok(())
    });
    app.status = format!("{} {} element(s).", if pinned { "Pinned" } else { "Unpinned" }, ids.len());
}

/// Copy to Clipboard (Ctrl+C).
pub fn copy_to_clipboard(app: &mut HausitApp) {
    let ids = selected_elements(app);
    if ids.is_empty() {
        return;
    }
    app.status = format!("Copied {} element(s) to the clipboard.", ids.len());
    app.clipboard = ids;
}

/// Paste (Ctrl+V): the clipboard's elements at the same place, on the
/// active plan's level (Revit's "Aligned to Current View"), or in place
/// elsewhere.
pub fn paste(app: &mut HausitApp) {
    let ids: Vec<ElementId> = app.clipboard.iter().copied().filter(|i| app.doc.element(*i).is_some()).collect();
    if ids.is_empty() {
        app.status = "The clipboard is empty.".into();
        return;
    }
    let from = app.doc.common_level(&ids);
    let to = app.active_level();
    let new = match (from, to) {
        (Some(f), Some(t)) if f != t => app.transact("Paste Aligned to Current View", |doc| Ok(doc.paste_aligned(&ids, f, &[t]))),
        _ => app.transact("Paste Aligned to Same Place", |doc| Ok(doc.copy(&ids, hausit_core::DVec2::ZERO))),
    };
    if let Some(new) = new {
        app.status = format!("Pasted {} element(s).", new.len());
        app.selection = new;
    }
}

/// Paste ▸ Aligned to Selected Levels: the levels ticked so far.
pub struct PasteLevels {
    pub ticked: Vec<(ElementId, bool)>,
}

pub fn open_paste_levels(app: &mut HausitApp) {
    if app.clipboard.iter().all(|i| app.doc.element(*i).is_none()) {
        app.status = "The clipboard is empty.".into();
        return;
    }
    app.dialogs.paste_levels = Some(PasteLevels {
        ticked: app.doc.levels().into_iter().map(|(id, _)| (id, false)).collect(),
    });
}

pub fn show(app: &mut HausitApp, ctx: &Context) {
    let Some(mut d) = app.dialogs.paste_levels.take() else { return };
    let mut open = true;
    let mut go = false;
    egui::Window::new("Select Levels")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Paste the clipboard's elements onto:");
            for (id, on) in &mut d.ticked {
                ui.checkbox(on, app.doc.level_name(*id));
            }
            ui.add_space(6.0);
            go = ui.button("  OK  ").clicked();
        });
    if go {
        let ids: Vec<ElementId> = app.clipboard.iter().copied().filter(|i| app.doc.element(*i).is_some()).collect();
        let levels: Vec<ElementId> = d.ticked.iter().filter(|t| t.1).map(|t| t.0).collect();
        if let Some(from) = app.doc.common_level(&ids) {
            if let Some(new) = app.transact("Paste Aligned to Selected Levels", |doc| Ok(doc.paste_aligned(&ids, from, &levels))) {
                app.status = format!("Pasted {} element(s) on {} level(s).", new.len(), levels.len());
                app.selection = new;
            }
        }
        return;
    }
    if open {
        app.dialogs.paste_levels = Some(d);
    }
}

/// The sunglasses menu on the View Control Bar.
pub fn control_bar_menu(app: &mut HausitApp, ui: &mut Ui, active: bool) {
    let label = if active {
        RichText::new("🕶 Hide/Isolate").color(egui::Color32::from_rgb(0, 140, 170))
    } else {
        RichText::new("🕶 Hide/Isolate")
    };
    ui.menu_button(label, |ui| {
        let has = !app.selection.is_empty();
        for (cmd, text) in [
            (HideCmd::IsolateCategory, "Isolate Category (IC)"),
            (HideCmd::HideCategory, "Hide Category (HC)"),
            (HideCmd::IsolateElement, "Isolate Element (HI)"),
            (HideCmd::HideElement, "Hide Element (HH)"),
        ] {
            if ui.add_enabled(has, egui::Button::new(text)).clicked() {
                temporary(app, cmd);
                ui.close();
            }
        }
        if ui
            .add_enabled(active, egui::Button::new("Reset Temporary Hide/Isolate (HR)"))
            .clicked()
        {
            temporary(app, HideCmd::Reset);
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(has, egui::Button::new("Hide in View: Elements (EH)")).clicked() {
            hide_in_view(app);
            ui.close();
        }
        let hidden = app
            .active_view
            .and_then(|v| app.doc.view(v))
            .map(|v| v.hidden_elements.len())
            .unwrap_or(0);
        if ui
            .add_enabled(hidden > 0, egui::Button::new(format!("Unhide All in View ({hidden})")))
            .clicked()
        {
            unhide_all(app);
            ui.close();
        }
    });
}
