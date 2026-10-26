//! Quick Access Toolbar and the ribbon (tabs → panels → buttons).

use crate::app::{HausitApp, RibbonTab};
use crate::icons::{self, Icon};
use crate::theme::{CONTEXT_TAB, FILE_TAB, RIBBON_BG, TAB_BAR_BG};
use crate::tools::{self, Tool};
use egui::{vec2, Color32, RichText, Sense, Stroke, Ui};
use hausit_model::{Category, ViewKind};

pub fn show(app: &mut HausitApp, ui: &mut Ui) {
    egui::Panel::top("ribbon_tabs")
        .exact_size(30.0)
        .frame(egui::Frame::new().fill(TAB_BAR_BG).inner_margin(egui::Margin::symmetric(4, 2)))
        .show(ui, |ui| tab_row(app, ui));
    if app.show_home {
        return;
    }
    egui::Panel::top("ribbon")
        .exact_size(92.0)
        .frame(
            egui::Frame::new()
                .fill(RIBBON_BG)
                .inner_margin(egui::Margin::symmetric(4, 3))
                .stroke(Stroke::new(1.0, Color32::from_gray(205))),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| match app.ribbon_tab {
                RibbonTab::Architecture => architecture(app, ui),
                RibbonTab::Structure => structure(app, ui),
                RibbonTab::Insert => insert(app, ui),
                RibbonTab::Annotate => annotate(app, ui),
                RibbonTab::View => view_tab(app, ui),
                RibbonTab::Manage => manage(app, ui),
                RibbonTab::Modify => modify(app, ui),
            });
        });
}

fn tab_row(app: &mut HausitApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        // Application menu ("File" tab).
        let file = ui.add(egui::Button::new(RichText::new("  File  ").color(Color32::WHITE).strong()).fill(FILE_TAB));
        egui::Popup::menu(&file).show(|ui| file_menu(app, ui));
        // Quick Access Toolbar.
        ui.add_space(4.0);
        if qat(ui, Icon::Open, "Open (Ctrl+O)") {
            app.open_dialog();
        }
        if qat(ui, Icon::Save, "Save (Ctrl+S)") && !app.show_home {
            app.save();
        }
        let undo_tip = app
            .history
            .undo_name()
            .map(|n| format!("Undo {n} (Ctrl+Z)"))
            .unwrap_or_else(|| "Undo (Ctrl+Z)".into());
        if qat(ui, Icon::Undo, &undo_tip) {
            app.undo();
        }
        let redo_tip = app
            .history
            .redo_name()
            .map(|n| format!("Redo {n} (Ctrl+Y)"))
            .unwrap_or_else(|| "Redo (Ctrl+Y)".into());
        if qat(ui, Icon::Redo, &redo_tip) {
            app.redo();
        }
        ui.separator();
        if app.show_home {
            ui.label(RichText::new("Home").strong());
            return;
        }
        for tab in RibbonTab::ALL {
            let selected = app.ribbon_tab == tab;
            let label = if tab == RibbonTab::Modify && app.tool != Tool::Modify {
                RichText::new(app.tool.context_title())
                    .color(if selected { Color32::WHITE } else { CONTEXT_TAB })
                    .strong()
            } else if tab == RibbonTab::Modify && !app.selection.is_empty() {
                let cats: Vec<Category> = app
                    .selection
                    .iter()
                    .filter_map(|id| app.doc.element(*id))
                    .map(|e| e.category())
                    .collect();
                let name = if !cats.is_empty() && cats.iter().all(|c| *c == cats[0]) {
                    format!("Modify | {}", cats[0].name())
                } else {
                    "Modify | Multi-Select".into()
                };
                RichText::new(name)
                    .color(if selected { Color32::WHITE } else { CONTEXT_TAB })
                    .strong()
            } else {
                RichText::new(tab.label())
            };
            let contextual = tab == RibbonTab::Modify && (app.tool != Tool::Modify || !app.selection.is_empty());
            let fill = if selected {
                if contextual {
                    CONTEXT_TAB
                } else {
                    RIBBON_BG
                }
            } else {
                Color32::TRANSPARENT
            };
            if ui
                .add(egui::Button::new(label).fill(fill).stroke(if selected {
                    Stroke::new(1.0, Color32::from_gray(200))
                } else {
                    Stroke::NONE
                }))
                .clicked()
            {
                app.ribbon_tab = tab;
            }
        }
    });
}

fn qat(ui: &mut Ui, icon: Icon, tip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(210, 225, 245));
    }
    icons::paint(ui.painter(), rect.shrink(2.0), icon, true);
    resp.on_hover_text(tip).clicked()
}

fn file_menu(app: &mut HausitApp, ui: &mut Ui) {
    ui.set_min_width(220.0);
    if ui.button("New Project            Ctrl+N").clicked() {
        app.set_document(hausit_model::template::new_project(), None);
    }
    if ui.button("Open…                     Ctrl+O").clicked() {
        app.open_dialog();
    }
    if ui.button("Open Sample House").clicked() {
        app.set_document(hausit_model::template::sample_project(), None);
    }
    ui.separator();
    ui.add_enabled_ui(!app.show_home, |ui| {
        if ui.button("Save                        Ctrl+S").clicked() {
            app.save();
        }
        if ui.button("Save As…             Ctrl+Shift+S").clicked() {
            app.save_as();
        }
        ui.menu_button("Export", |ui| {
            if ui.button("Schedule as CSV…").clicked() {
                crate::schedule_view::export_active(app);
            }
            ui.add_enabled(false, egui::Button::new("IFC… (planned)"));
            ui.add_enabled(false, egui::Button::new("DWG… (planned)"));
            ui.add_enabled(false, egui::Button::new("PDF… (planned)"));
        });
        ui.separator();
        if ui.button("Close Project (Home)").clicked() {
            if app.is_dirty() {
                app.dialogs.confirm_home = true;
            } else {
                app.show_home = true;
            }
        }
    });
    ui.separator();
    if ui.button("About Hausit").clicked() {
        app.dialogs.about = true;
    }
    if ui.button("Exit").clicked() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

// ----- ribbon building blocks -------------------------------------------------

/// A titled ribbon panel, like Revit's "Build" or "Datum".
fn panel(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        let row = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                add(ui);
            })
            .response;
        ui.add_space((ui.available_height() - 16.0).max(0.0));
        ui.set_max_width(row.rect.width().max(40.0));
        ui.vertical_centered(|ui| ui.add(egui::Label::new(RichText::new(title).size(11.0).color(Color32::from_gray(90))).extend()));
    });
    ui.separator();
}

/// A large ribbon button: icon over label.
fn big(ui: &mut Ui, icon: Icon, label: &str, tip: &str, active: bool, enabled: bool) -> bool {
    let size = vec2(54.0, 62.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    if active {
        p.rect(
            rect,
            3.0,
            Color32::from_rgb(196, 220, 252),
            Stroke::new(1.0, Color32::from_rgb(80, 130, 210)),
            egui::StrokeKind::Inside,
        );
    } else if resp.hovered() && enabled {
        p.rect(
            rect,
            3.0,
            Color32::from_rgb(225, 236, 250),
            Stroke::new(1.0, Color32::from_rgb(160, 190, 230)),
            egui::StrokeKind::Inside,
        );
    }
    let icon_rect = egui::Rect::from_center_size(rect.center_top() + vec2(0.0, 20.0), vec2(30.0, 30.0));
    icons::paint(p, icon_rect, icon, enabled);
    let color = if enabled { Color32::from_gray(30) } else { Color32::from_gray(160) };
    p.text(
        rect.center_bottom() - vec2(0.0, 12.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.5),
        color,
    );
    let resp = if enabled {
        resp.on_hover_text(tip)
    } else {
        resp.on_hover_text(format!("{tip}\n\nNot available yet: planned for a later version."))
    };
    enabled && resp.clicked()
}

fn tool_button(app: &mut HausitApp, ui: &mut Ui, icon: Icon, label: &str, tip: &str, tool: Tool) {
    let active = std::mem::discriminant(&app.tool) == std::mem::discriminant(&tool)
        && match (&app.tool, &tool) {
            (Tool::Wall { structural: a, .. }, Tool::Wall { structural: b, .. })
            | (Tool::Floor { structural: a, .. }, Tool::Floor { structural: b, .. }) => a == b,
            _ => true,
        };
    if big(ui, icon, label, tip, active, true) {
        tools::start(app, tool);
    }
}

fn planned(ui: &mut Ui, icon: Icon, label: &str, tip: &str) {
    big(ui, icon, label, tip, false, false);
}

// ----- tabs -------------------------------------------------------------------

fn select_panel(app: &mut HausitApp, ui: &mut Ui) {
    panel(ui, "Select", |ui| {
        if big(
            ui,
            Icon::Modify,
            "Modify",
            "Modify: select elements (Esc)",
            app.tool == Tool::Modify,
            true,
        ) {
            app.tool.cancel();
            app.status = app.tool.prompt().into();
        }
    });
}

fn architecture(app: &mut HausitApp, ui: &mut Ui) {
    select_panel(app, ui);
    panel(ui, "Build", |ui| {
        tool_button(
            app,
            ui,
            Icon::Wall,
            "Wall",
            "Wall: Architectural (WA)",
            Tool::Wall {
                start: None,
                structural: false,
            },
        );
        tool_button(app, ui, Icon::Door, "Door", "Door (DR)", Tool::Door);
        tool_button(app, ui, Icon::Window, "Window", "Window (WN)", Tool::Window);
        planned(ui, Icon::Planned, "Component", "Place a Component");
        planned(ui, Icon::Column, "Column", "Column: Architectural");
        tool_button(app, ui, Icon::Roof, "Roof", "Roof by Footprint (RF)", Tool::Roof { pts: vec![] });
        planned(ui, Icon::Planned, "Ceiling", "Ceiling");
        tool_button(
            app,
            ui,
            Icon::Floor,
            "Floor",
            "Floor: Architectural (FL)",
            Tool::Floor {
                pts: vec![],
                structural: false,
            },
        );
    });
    panel(ui, "Circulation", |ui| {
        planned(ui, Icon::Planned, "Stair", "Stair");
        planned(ui, Icon::Planned, "Railing", "Railing");
    });
    panel(ui, "Room & Area", |ui| planned(ui, Icon::Room, "Room", "Room"));
    panel(ui, "Datum", |ui| {
        tool_button(
            app,
            ui,
            Icon::Level,
            "Level",
            "Level (LL): place in an elevation or section",
            Tool::Level,
        );
        tool_button(app, ui, Icon::Grid, "Grid", "Grid (GR)", Tool::Grid { start: None });
    });
    mode_panel(app, ui);
}

fn structure(app: &mut HausitApp, ui: &mut Ui) {
    select_panel(app, ui);
    panel(ui, "Structure", |ui| {
        planned(ui, Icon::Planned, "Beam", "Beam");
        tool_button(
            app,
            ui,
            Icon::Wall,
            "Wall",
            "Wall: Structural",
            Tool::Wall {
                start: None,
                structural: true,
            },
        );
        planned(ui, Icon::Column, "Column", "Structural Column");
        tool_button(
            app,
            ui,
            Icon::Floor,
            "Floor",
            "Floor: Structural",
            Tool::Floor {
                pts: vec![],
                structural: true,
            },
        );
    });
    panel(ui, "Foundation", |ui| {
        planned(ui, Icon::Planned, "Isolated", "Isolated Foundation");
        planned(ui, Icon::Planned, "Wall", "Wall Foundation");
    });
    panel(ui, "Datum", |ui| {
        tool_button(app, ui, Icon::Level, "Level", "Level (LL)", Tool::Level);
        tool_button(app, ui, Icon::Grid, "Grid", "Grid (GR)", Tool::Grid { start: None });
    });
    mode_panel(app, ui);
}

fn insert(app: &mut HausitApp, ui: &mut Ui) {
    select_panel(app, ui);
    panel(ui, "Link", |ui| {
        if big(
            ui,
            Icon::Import,
            "Revit File",
            "Open an Autodesk Revit file (.rvt/.rfa) to see its version and preview.\nImporting Revit elements is a planned milestone.",
            false,
            true,
        ) {
            let picked = rfd::FileDialog::new()
                .add_filter("Autodesk Revit files", &hausit_rvt::EXTENSIONS)
                .pick_file();
            if let Some(p) = picked {
                app.open_path(&p);
            }
        }
        planned(ui, Icon::Import, "Link IFC", "Link IFC");
        planned(ui, Icon::Import, "Link CAD", "Link CAD");
    });
    panel(ui, "Load from Library", |ui| {
        planned(ui, Icon::Import, "Load Family", "Load Family (.rfa)")
    });
}

fn annotate(app: &mut HausitApp, ui: &mut Ui) {
    select_panel(app, ui);
    panel(ui, "Dimension", |ui| {
        planned(ui, Icon::Dimension, "Aligned", "Aligned Dimension (DI)");
        planned(ui, Icon::Dimension, "Linear", "Linear Dimension");
    });
    panel(ui, "Text", |ui| planned(ui, Icon::Planned, "Text", "Text (TX)"));
    panel(ui, "Tag", |ui| {
        planned(ui, Icon::Planned, "Tag by Category", "Tag by Category (TG)");
        planned(ui, Icon::Planned, "Tag All", "Tag All Not Tagged");
    });
}

fn view_tab(app: &mut HausitApp, ui: &mut Ui) {
    select_panel(app, ui);
    panel(ui, "Graphics", |ui| {
        let style_enabled = app
            .active_view
            .and_then(|v| app.doc.view(v))
            .map(|v| v.kind.is_graphical())
            .unwrap_or(false);
        if big(
            ui,
            Icon::TypeProps,
            "Visibility",
            "Visual style of the active view: cycles Wireframe, Hidden Line, Shaded, Consistent Colors",
            false,
            style_enabled,
        ) {
            if let Some(v) = app.active_view.and_then(|v| app.doc.view_mut(v)) {
                let all = hausit_model::VisualStyle::ALL;
                let i = all.iter().position(|s| *s == v.visual_style).unwrap_or(0);
                v.visual_style = all[(i + 1) % all.len()];
            }
        }
    });
    panel(ui, "Create", |ui| {
        if big(ui, Icon::ThreeD, "3D View", "Open the default 3D view", false, true) {
            let id = app.doc.views().find(|v| matches!(v.kind, ViewKind::ThreeD)).map(|v| v.id);
            let id = id.or_else(|| app.transact("Create 3D View", |doc| Ok(doc.add_view("{3D}", ViewKind::ThreeD))));
            if let Some(id) = id {
                app.open_view(id);
            }
        }
        tool_button(
            app,
            ui,
            Icon::Section,
            "Section",
            "Section: draw a section line in a plan",
            Tool::Section { start: None },
        );
        if big(
            ui,
            Icon::Schedule,
            "Schedules",
            "Schedule/Quantities: create a new schedule",
            false,
            true,
        ) {
            app.dialogs.new_schedule = Some(crate::dialogs::NewSchedule {
                category: Category::Doors,
                name: String::new(),
            });
        }
        planned(ui, Icon::Planned, "Sheet", "Sheet");
    });
    panel(ui, "Windows", |ui| {
        if big(
            ui,
            Icon::Close,
            "Close Inactive",
            "Close all view tabs except the active one",
            false,
            app.open_views.len() > 1,
        ) {
            app.open_views.retain(|v| Some(*v) == app.active_view);
        }
    });
}

fn manage(app: &mut HausitApp, ui: &mut Ui) {
    select_panel(app, ui);
    panel(ui, "Settings", |ui| {
        if big(ui, Icon::Info, "Project Info", "Project Information", false, true) {
            app.dialogs.project_info = true;
        }
        if big(ui, Icon::Units, "Units", "Project Units", false, true) {
            app.dialogs.units = true;
        }
        planned(ui, Icon::Planned, "Materials", "Materials");
        planned(ui, Icon::Planned, "Object Styles", "Object Styles");
    });
    panel(ui, "Phasing", |ui| planned(ui, Icon::Planned, "Phases", "Phases"));
}

fn modify(app: &mut HausitApp, ui: &mut Ui) {
    select_panel(app, ui);
    let has_sel = app.selection.iter().any(|id| app.doc.element(*id).is_some());
    panel(ui, "Properties", |ui| {
        let type_id = app
            .selection
            .first()
            .and_then(|id| app.doc.element(*id))
            .and_then(|e| e.type_id())
            .or_else(|| app.tool.category().and_then(|c| app.type_for(c)));
        if big(ui, Icon::TypeProps, "Type Props", "Type Properties", false, type_id.is_some()) {
            if let Some(t) = type_id {
                app.dialogs.open_type_properties(&app.doc, t);
            }
        }
    });
    panel(ui, "Modify", |ui| {
        if big(ui, Icon::Move, "Move", "Move (MV)", matches!(app.tool, Tool::Move { .. }), has_sel) {
            tools::start(app, Tool::Move { base: None });
        }
        if big(ui, Icon::Copy, "Copy", "Copy (CO)", matches!(app.tool, Tool::Copy { .. }), has_sel) {
            tools::start(app, Tool::Copy { base: None });
        }
        planned(ui, Icon::Planned, "Rotate", "Rotate (RO)");
        planned(ui, Icon::Planned, "Mirror", "Mirror (MM)");
        planned(ui, Icon::Planned, "Trim", "Trim/Extend to Corner (TR)");
        if big(ui, Icon::Delete, "Delete", "Delete (DE)", false, !app.selection.is_empty()) {
            app.delete_selection();
        }
    });
    mode_panel(app, ui);
}

/// "Mode" panel with Finish/Cancel while sketching, as in Revit.
fn mode_panel(app: &mut HausitApp, ui: &mut Ui) {
    if !app.tool.is_sketch() {
        return;
    }
    panel(ui, "Mode", |ui| {
        if big(ui, Icon::Finish, "Finish", "Finish Edit Mode (Enter)", false, true) {
            tools::finish_sketch(app);
        }
        if big(ui, Icon::Cancel, "Cancel", "Cancel Edit Mode (Esc)", false, true) {
            app.tool.cancel();
            app.status = app.tool.prompt().into();
        }
    });
}
