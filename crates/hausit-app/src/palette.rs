//! Command search (Ctrl+K or F3), the Keyboard Shortcuts list and the
//! selection Filter dialog.

use crate::app::HausitApp;
use crate::commands::{self, COMMANDS};
use egui::{Color32, Context, Key, RichText};
use hausit_model::Category;

#[derive(Default)]
pub struct Palette {
    pub open: bool,
    pub query: String,
    pub selected: usize,
    focus: bool,
}

/// Draws the command search box when it is open and runs what is picked.
pub fn show(app: &mut HausitApp, ctx: &Context) {
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::K) || i.consume_key(egui::Modifiers::NONE, Key::F3)) {
        app.palette.open = !app.palette.open;
    }
    if !app.palette.open {
        app.palette.focus = false;
        return;
    }
    let results: Vec<usize> = if app.palette.query.trim().is_empty() {
        app.recent_commands.clone()
    } else {
        commands::search(&app.palette.query)
    };
    let mut run = None;
    let mut close = ctx.input(|i| i.key_pressed(Key::Escape));
    let (down, up, enter) = ctx.input(|i| {
        (
            i.key_pressed(Key::ArrowDown),
            i.key_pressed(Key::ArrowUp),
            i.key_pressed(Key::Enter),
        )
    });
    if down {
        app.palette.selected = (app.palette.selected + 1).min(results.len().saturating_sub(1));
    }
    if up {
        app.palette.selected = app.palette.selected.saturating_sub(1);
    }
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::unique("command_palette"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(screen.center().x - 260.0, screen.top() + 70.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).inner_margin(8).show(ui, |ui| {
                ui.set_width(520.0);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut app.palette.query)
                        .hint_text("Search commands: wall, section, export…")
                        .desired_width(f32::INFINITY),
                );
                if !app.palette.focus {
                    r.request_focus();
                    app.palette.focus = true;
                }
                if r.changed() {
                    app.palette.selected = 0;
                }
                if results.is_empty() {
                    ui.add_space(4.0);
                    let msg = if app.palette.query.trim().is_empty() {
                        "Type to find any command. Recent commands show here."
                    } else {
                        "No command matches."
                    };
                    ui.label(RichText::new(msg).color(Color32::GRAY));
                } else if app.palette.query.trim().is_empty() {
                    ui.label(RichText::new("Recent").small().color(Color32::GRAY));
                }
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for (row, &i) in results.iter().enumerate().take(40) {
                        let c = &COMMANDS[i];
                        let sel = row == app.palette.selected;
                        let resp = ui
                            .horizontal(|ui| {
                                ui.set_width(ui.available_width());
                                let r = ui.selectable_label(sel, RichText::new(c.name).strong());
                                ui.label(RichText::new(c.place).color(Color32::GRAY).small());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if let Some(k) = c.keys.first() {
                                        key_cap(ui, k);
                                    }
                                });
                                r
                            })
                            .inner;
                        if sel && (down || up) {
                            resp.scroll_to_me(None);
                        }
                        if resp.clicked() {
                            run = Some(i);
                        }
                    }
                });
                ui.add_space(2.0);
                ui.label(
                    RichText::new("↑ ↓ to choose · Enter to run · Esc to close")
                        .small()
                        .color(Color32::GRAY),
                );
            });
        });
    if enter {
        run = run.or_else(|| results.get(app.palette.selected).copied());
    }
    if let Some(i) = run {
        close = true;
        app.palette.open = false;
        commands::run(app, i);
    }
    if close {
        app.palette.open = false;
        app.palette.query.clear();
        app.palette.selected = 0;
    }
}

pub fn key_cap(ui: &mut egui::Ui, k: &str) {
    egui::Frame::new()
        .fill(Color32::from_gray(238))
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(200)))
        .corner_radius(3)
        .inner_margin(egui::Margin::symmetric(4, 0))
        .show(ui, |ui| {
            ui.label(RichText::new(k).monospace().size(11.0));
        });
}

/// Help ▸ Keyboard Shortcuts.
pub fn shortcuts_window(app: &mut HausitApp, ctx: &Context) {
    if !app.dialogs.shortcuts {
        return;
    }
    let mut open = true;
    egui::Window::new("Keyboard Shortcuts")
        .open(&mut open)
        .default_size([520.0, 520.0])
        .collapsible(false)
        .show(ctx, |ui| {
            ui.label("Type the two letters anywhere in a view; no need to press Enter. Ctrl+K or F3 finds any command by name.");
            ui.add_space(6.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("shortcut_grid").striped(true).num_columns(3).show(ui, |ui| {
                    for (k, what) in [
                        ("Esc", "Cancel the tool, then clear the selection"),
                        ("Enter", "Finish a sketch; with nothing running, repeat the last command"),
                        ("Space", "Flip the wall or door being placed or selected; rotate components"),
                        ("Tab", "Cycle through overlapping elements under the cursor"),
                        ("Ctrl+Z / Ctrl+Y", "Undo / Redo"),
                        ("Ctrl+C / Ctrl+V", "Copy to and paste from the clipboard"),
                        ("Ctrl+S", "Save"),
                        ("Ctrl+1", "Show or hide the Properties palette"),
                        ("Ctrl+K, F3", "Search commands"),
                    ] {
                        key_cap(ui, k);
                        ui.label(what);
                        ui.label("");
                        ui.end_row();
                    }
                    for c in COMMANDS.iter().filter(|c| !c.keys.is_empty()) {
                        key_cap(ui, &c.keys.join(", "));
                        ui.label(c.name);
                        ui.label(RichText::new(c.place).color(Color32::GRAY));
                        ui.end_row();
                    }
                });
            });
        });
    if !open {
        app.dialogs.shortcuts = false;
    }
}

/// Modify ▸ Filter: keep only some categories of the selection.
pub struct FilterDialog {
    pub rows: Vec<(Category, usize, bool)>,
}

impl FilterDialog {
    pub fn new(app: &HausitApp) -> Self {
        let mut rows: Vec<(Category, usize, bool)> = Vec::new();
        for c in app.selection.iter().filter_map(|id| app.doc.element(*id)).map(|e| e.category()) {
            match rows.iter_mut().find(|r| r.0 == c) {
                Some(r) => r.1 += 1,
                None => rows.push((c, 1, true)),
            }
        }
        rows.sort_by_key(|r| r.0.name());
        FilterDialog { rows }
    }
}

pub fn filter_window(app: &mut HausitApp, ctx: &Context) {
    let Some(d) = &mut app.dialogs.filter else { return };
    let mut open = true;
    let mut apply = false;
    egui::Window::new("Filter")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            if d.rows.is_empty() {
                ui.label("Select elements first; Filter keeps the categories you tick.");
                return;
            }
            egui::Grid::new("filter_grid").striped(true).num_columns(2).show(ui, |ui| {
                ui.label(RichText::new("Category").strong());
                ui.label(RichText::new("Count").strong());
                ui.end_row();
                for (c, n, on) in &mut d.rows {
                    ui.checkbox(on, c.name());
                    ui.label(n.to_string());
                    ui.end_row();
                }
            });
            let total: usize = d.rows.iter().filter(|r| r.2).map(|r| r.1).sum();
            ui.label(format!("Total selected items: {total}"));
            ui.horizontal(|ui| {
                if ui.button("Check All").clicked() {
                    d.rows.iter_mut().for_each(|r| r.2 = true);
                }
                if ui.button("Check None").clicked() {
                    d.rows.iter_mut().for_each(|r| r.2 = false);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("OK").clicked() {
                        apply = true;
                    }
                });
            });
        });
    if apply {
        let keep: Vec<Category> = d.rows.iter().filter(|r| r.2).map(|r| r.0).collect();
        app.selection
            .retain(|id| app.doc.element(*id).is_none_or(|e| keep.contains(&e.category())));
        app.status = format!("{} element(s) selected", app.selection.len());
    }
    if apply || !open {
        app.dialogs.filter = None;
    }
}
