//! Quick Access Toolbar and the ribbon (tabs → panels → buttons).

use crate::app::{HausitApp, RibbonTab};
use crate::icons::{self, Icon};
use crate::theme::{CONTEXT_TAB, FILE_TAB, RIBBON_BG, TAB_BAR_BG};
use crate::tools::{self, DrawShape, Tool};
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
                } else if cats.is_empty() && crate::sheets::active_sheet(app).is_some() {
                    "Modify | Viewports".into()
                } else if cats.is_empty() {
                    "Modify | Views".into()
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
    ui.menu_button("New Family", |ui| {
        for c in [Category::Doors, Category::Windows].into_iter().chain(Category::COMPONENTS) {
            if ui.button(c.name()).clicked() {
                crate::family_editor::new_family(app, c);
                ui.close();
            }
        }
    });
    if ui.button("Open Family…").clicked() {
        crate::family_editor::open_family_dialog(app);
    }
    ui.separator();
    if ui.button("Open Sample House").clicked() {
        app.set_document(crate::sheets::sample_house(), None);
    }
    if ui.button("Open Sample Warehouse").clicked() {
        app.set_document(crate::sheets::sample_warehouse(), None);
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
            if ui.button("IFC…").clicked() {
                export_ifc(app);
                ui.close();
            }
            let view = app.active_view.and_then(|v| app.doc.view(v)).filter(|v| v.kind.is_graphical());
            if ui
                .add_enabled(view.is_some(), egui::Button::new("DXF… (current view)"))
                .on_hover_text("Export the active view as DXF, with layers by category, for AutoCAD and other CAD programs")
                .clicked()
            {
                export_dxf(app);
                ui.close();
            }
            ui.add_enabled(false, egui::Button::new("DWG… (planned)"));
            if ui.button("PDF…").clicked() {
                crate::sheets::start_export(app);
                ui.close();
            }
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
    // Wide enough for the label.
    let font = egui::FontId::proportional(11.5);
    let text_w = ui.fonts_mut(|f| f.layout_no_wrap(label.to_string(), font.clone(), Color32::BLACK).size().x);
    let size = vec2((text_w + 8.0).max(54.0), 62.0);
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
        font,
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
        tool_button(
            app,
            ui,
            Icon::Component,
            "Component",
            "Place a Component (CM)",
            Tool::Component {
                category: Category::Furniture,
            },
        );
        tool_button(app, ui, Icon::Column, "Column", "Structural Column (CL)", Tool::Column);
        tool_button(
            app,
            ui,
            Icon::Roof,
            "Roof",
            "Roof by Footprint (RF)",
            Tool::Roof {
                sketch: Default::default(),
            },
        );
        tool_button(app, ui, Icon::Ceiling, "Ceiling", "Ceiling: Automatic", Tool::Ceiling);
        tool_button(
            app,
            ui,
            Icon::Floor,
            "Floor",
            "Floor: Architectural (FL)",
            Tool::Floor {
                sketch: Default::default(),
                structural: false,
            },
        );
    });
    panel(ui, "Circulation", |ui| {
        tool_button(app, ui, Icon::Stair, "Stair", "Stair: Run", Tool::Stair { pts: vec![] });
        tool_button(
            app,
            ui,
            Icon::Railing,
            "Railing",
            "Railing: Sketch Path",
            Tool::Railing { pts: vec![] },
        );
    });
    panel(ui, "Room & Area", |ui| {
        tool_button(app, ui, Icon::Room, "Room", "Room (RM)", Tool::Room);
        tool_button(
            app,
            ui,
            Icon::RoomSeparator,
            "Separator",
            "Room Separator: divide a space into rooms without walls",
            Tool::RoomSeparation { start: None },
        );
    });
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
        tool_button(app, ui, Icon::Beam, "Beam", "Beam (BM)", Tool::Beam { start: None, brace: false });
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
        tool_button(app, ui, Icon::Column, "Column", "Structural Column (CL)", Tool::Column);
        tool_button(
            app,
            ui,
            Icon::Floor,
            "Floor",
            "Floor: Structural",
            Tool::Floor {
                sketch: Default::default(),
                structural: true,
            },
        );
        tool_button(app, ui, Icon::Brace, "Brace", "Brace (BR)", Tool::Beam { start: None, brace: true });
    });
    panel(ui, "Foundation", |ui| {
        tool_button(app, ui, Icon::Footing, "Isolated", "Isolated Foundation", Tool::Footing);
        tool_button(app, ui, Icon::WallFoundation, "Wall", "Wall Foundation (FT)", Tool::WallFoundation);
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
        if big(
            ui,
            Icon::Import,
            "Open IFC",
            "Open an IFC file (IFC2x3 or IFC4) as a new project.\nWalls, floors, columns, doors, windows, rooms, grids and levels become Hausit elements; everything else comes in as imported geometry.\nThis is also how to bring in a Revit project: export it to IFC from Revit.",
            false,
            true,
        ) {
            let picked = rfd::FileDialog::new().add_filter("IFC", &["ifc"]).pick_file();
            if let Some(p) = picked {
                app.open_path(&p);
            }
        }
        planned(ui, Icon::Import, "Link CAD", "Link CAD");
    });
    panel(ui, "Load from Library", |ui| {
        if big(
            ui,
            Icon::LoadFamily,
            "Load Family",
            "Load families from Hausit family files (.hfam)",
            false,
            true,
        ) {
            crate::families::load_from_file(app);
        }
        if big(
            ui,
            Icon::LoadFamily,
            "Library",
            "Load families from Hausit's starter library: furniture, casework, plumbing fixtures and equipment",
            false,
            true,
        ) {
            crate::families::open_library(app);
        }
    });
}

fn annotate(app: &mut HausitApp, ui: &mut Ui) {
    select_panel(app, ui);
    panel(ui, "Dimension", |ui| {
        tool_button(
            app,
            ui,
            Icon::Dimension,
            "Aligned",
            "Aligned Dimension (DI)",
            Tool::Dimension { refs: vec![] },
        );
        tool_button(
            app,
            ui,
            Icon::Dimension,
            "Linear",
            "Linear Dimension: pick two points; it measures horizontally or vertically",
            Tool::LinearDimension { pts: vec![] },
        );
    });
    panel(ui, "Text", |ui| {
        tool_button(
            app,
            ui,
            Icon::Text,
            "Text",
            "Text (TX): click where the text starts, then type it",
            Tool::Text,
        );
    });
    panel(ui, "Tag", |ui| {
        tool_button(
            app,
            ui,
            Icon::Tag,
            "By Category",
            "Tag by Category (TG): click elements to tag them",
            Tool::Tag,
        );
        let in_plan = app
            .active_view
            .and_then(|v| app.doc.view(v))
            .is_some_and(|v| matches!(v.kind, ViewKind::FloorPlan { .. } | ViewKind::CeilingPlan { .. }));
        if big(
            ui,
            Icon::Tag,
            "Tag All",
            "Tag All Not Tagged: tag every element of chosen categories in this plan",
            false,
            in_plan,
        ) {
            app.dialogs.tag_all = Some(
                hausit_model::annotation::TAGGABLE
                    .iter()
                    .map(|c| (*c, matches!(c, Category::Doors | Category::Windows)))
                    .collect(),
            );
        }
    });
}

fn view_tab(app: &mut HausitApp, ui: &mut Ui) {
    select_panel(app, ui);
    panel(ui, "Graphics", |ui| {
        let graphical = app
            .active_view
            .and_then(|v| app.doc.view(v))
            .is_some_and(|v| v.kind.is_graphical() && !matches!(v.kind, ViewKind::Sheet(_)));
        if big(
            ui,
            Icon::TypeProps,
            "Visibility/Graphics",
            "Visibility/Graphics (VV): show, hide and override categories in the active view",
            false,
            graphical,
        ) {
            crate::view_graphics::open_for_active_view(app);
        }
        if big(
            ui,
            Icon::TypeProps,
            "View Templates",
            "View Templates: create templates from views, edit them, and apply them",
            false,
            true,
        ) {
            crate::view_graphics::open_templates(app);
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
        tool_button(
            app,
            ui,
            Icon::Elevation,
            "Elevation",
            "Elevation: click in a plan; it looks at the nearest wall",
            Tool::Elevation,
        );
        if big(
            ui,
            Icon::Plan,
            "Plan Views",
            "Plan Views: floor or ceiling plans for existing levels",
            false,
            true,
        ) {
            app.dialogs.plan_views = Some(crate::dialogs::PlanViews::new(&app.doc));
        }
        tool_button(
            app,
            ui,
            Icon::Callout,
            "Callout",
            "Callout: draw a rectangle in a plan or section to make an enlarged detail view (1:20, Fine)",
            Tool::Callout { start: None },
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
    });
    panel(ui, "Sheet Composition", |ui| {
        if big(
            ui,
            Icon::Sheet,
            "Sheet",
            "Sheet: create a new sheet with a title block",
            false,
            true,
        ) {
            crate::sheets::start_new_sheet(app);
        }
        let sheet = crate::sheets::active_sheet(app);
        if big(
            ui,
            Icon::PlaceView,
            "Place View",
            "Place View: put a view or schedule on the active sheet",
            false,
            sheet.is_some(),
        ) {
            app.dialogs.place_view = sheet.map(|s| (s, None));
        }
        if big(
            ui,
            Icon::Pdf,
            "Export PDF",
            "Export sheets to PDF",
            false,
            app.doc.sheets().next().is_some(),
        ) {
            crate::sheets::start_export(app);
        }
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
        if big(ui, Icon::TypeProps, "Materials", "Materials: colours and cut patterns", false, true) {
            app.dialogs.materials = Some(crate::dialogs::MaterialsDialog {
                rows: app.doc.materials.iter().map(|m| (m.name.clone(), m.clone())).collect(),
                selected: 0,
            });
        }
        if big(
            ui,
            Icon::TypeProps,
            "Object Styles",
            "Object Styles: line weights and colours by category",
            false,
            true,
        ) {
            app.dialogs.object_styles = Some((app.doc.object_styles.clone(), app.doc.line_weights.clone()));
        }
    });
    panel(ui, "Phasing", |ui| {
        if big(
            ui,
            Icon::Phases,
            "Phases",
            "Phases: the project's phases and how phase filters show them",
            false,
            true,
        ) {
            app.dialogs.phases = Some(crate::phases::PhasesDialog::new(&app.doc));
        }
    });
}

fn modify(app: &mut HausitApp, ui: &mut Ui) {
    select_panel(app, ui);
    let has_sel = app.selection.iter().any(|id| app.doc.element(*id).is_some());
    panel(ui, "Clipboard", |ui| {
        if big(
            ui,
            Icon::Paste,
            "Paste",
            "Paste (Ctrl+V): on the active plan's level, or in place",
            false,
            !app.clipboard.is_empty(),
        ) {
            crate::hide_isolate::paste(app);
        }
        if big(
            ui,
            Icon::Paste,
            "To Levels",
            "Paste Aligned to Selected Levels",
            false,
            !app.clipboard.is_empty(),
        ) {
            crate::hide_isolate::open_paste_levels(app);
        }
        if big(ui, Icon::Copy, "Copy", "Copy to Clipboard (Ctrl+C)", false, has_sel) {
            crate::hide_isolate::copy_to_clipboard(app);
        }
    });
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
        let note = app
            .selection
            .first()
            .copied()
            .filter(|id| app.doc.element(*id).is_some_and(|e| e.category() == Category::TextNotes));
        if let Some(note) = note {
            if big(ui, Icon::Text, "Edit Text", "Edit the selected text note", false, true) {
                app.dialogs.text_note = crate::dialogs::TextNoteDialog::edit(&app.doc, note);
            }
        }
        let family = type_id.filter(|t| crate::family_editor::family_of_type(app, *t).is_some());
        if big(
            ui,
            Icon::LoadFamily,
            "Edit Family",
            "Edit Family: open this element's family in the Family Editor",
            false,
            family.is_some(),
        ) {
            if let Some(t) = family {
                crate::family_editor::edit_family(app, t);
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
        if big(
            ui,
            Icon::Rotate,
            "Rotate",
            "Rotate (RO)",
            matches!(app.tool, Tool::Rotate { .. }),
            has_sel,
        ) {
            tools::start(
                app,
                Tool::Rotate {
                    center: Default::default(),
                    from: None,
                },
            );
        }
        if big(
            ui,
            Icon::Mirror,
            "Mirror",
            "Mirror - Pick Axis (MM)",
            matches!(app.tool, Tool::Mirror { draw: false, .. }),
            has_sel,
        ) {
            tools::start(app, Tool::Mirror { draw: false, start: None });
        }
        if big(
            ui,
            Icon::Mirror,
            "Draw Axis",
            "Mirror - Draw Axis (DM)",
            matches!(app.tool, Tool::Mirror { draw: true, .. }),
            has_sel,
        ) {
            tools::start(app, Tool::Mirror { draw: true, start: None });
        }
        tool_button(
            app,
            ui,
            Icon::Trim,
            "Trim",
            "Trim/Extend to Corner (TR)",
            Tool::Trim { first: None },
        );
        tool_button(app, ui, Icon::Split, "Split", "Split Element (SL)", Tool::Split);
        tool_button(
            app,
            ui,
            Icon::Demolish,
            "Demolish",
            "Demolish: click elements to demolish them in this view's phase",
            Tool::Demolish,
        );
        tool_button(app, ui, Icon::Align, "Align", "Align (AL)", Tool::Align { reference: None });
        tool_button(app, ui, Icon::Offset, "Offset", "Offset (OF)", Tool::Offset);
        if big(
            ui,
            Icon::Array,
            "Array",
            "Array (AR): copies of the selection in a row",
            matches!(app.tool, Tool::Array { .. }),
            has_sel,
        ) {
            tools::start(app, Tool::Array { start: None });
        }
        let pinned = app.selection.iter().any(|s| app.doc.element(*s).is_some_and(|e| e.pinned));
        if big(
            ui,
            Icon::Pin,
            if pinned { "Unpin" } else { "Pin" },
            "Pin (PN) keeps elements from moving; Unpin (UP) releases them",
            false,
            has_sel,
        ) {
            crate::hide_isolate::pin(app, !pinned);
        }
        if big(ui, Icon::Delete, "Delete", "Delete (DE)", false, !app.selection.is_empty()) {
            app.delete_selection();
        }
    });
    panel(ui, "View", |ui| {
        if big(
            ui,
            Icon::ThreeD,
            "Selection Box",
            "Selection Box (BX): open a 3D view cut to the selected elements",
            false,
            app.selection.iter().any(|id| app.doc.element(*id).is_some()),
        ) {
            app.selection_box();
        }
    });
    wall_panel(app, ui);
    if matches!(app.tool, Tool::Column) {
        panel(ui, "Multiple", |ui| {
            if big(ui, Icon::Grid, "At Grids", "Place a column at every grid intersection", false, true) {
                tools::columns_at_grids(app);
            }
        });
    }
    draw_panel(app, ui);
    mode_panel(app, ui);
}

/// "Modify Wall" panel, shown when walls are selected.
fn wall_panel(app: &mut HausitApp, ui: &mut Ui) {
    let walls = app
        .selection
        .iter()
        .any(|id| app.doc.element(*id).and_then(|e| e.as_wall()).is_some());
    if !walls || !matches!(app.tool, Tool::Modify | Tool::Attach { .. }) {
        return;
    }
    panel(ui, "Modify Wall", |ui| {
        for (detach, label, tip) in [
            (false, "Attach", "Attach Top/Base: extend the walls up to a roof or floor"),
            (true, "Detach", "Detach Top/Base"),
        ] {
            let active = matches!(&app.tool, Tool::Attach { detach: d, .. } if *d == detach);
            if big(ui, Icon::Attach, label, tip, active, true) {
                tools::start(app, Tool::Attach { walls: vec![], detach });
            }
        }
    });
}

/// "Draw" panel of the contextual tab: how walls and sketch lines are
/// entered.
fn draw_panel(app: &mut HausitApp, ui: &mut Ui) {
    let sketch = app.tool.is_sketch();
    if !sketch && !matches!(app.tool, Tool::Wall { .. }) {
        return;
    }
    panel(ui, "Draw", |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let mode = if sketch {
                &mut app.options.sketch_mode
            } else {
                &mut app.options.shape
            };
            let mut item = |ui: &mut Ui, m: DrawShape, label: &str| {
                if ui.selectable_label(*mode == m, label).clicked() {
                    *mode = m;
                }
            };
            if sketch {
                item(ui, DrawShape::PickWalls, "▣ Pick Walls");
            }
            item(ui, DrawShape::Line, "╱ Line");
            item(ui, DrawShape::Rectangle, "▭ Rectangle");
        });
    });
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

/// File ▸ Export ▸ IFC: the whole model as IFC4.
fn export_dxf(app: &mut HausitApp) {
    let Some(view) = app.active_view.and_then(|v| app.doc.view(v)).cloned() else {
        return;
    };
    let Some(path) = rfd::FileDialog::new()
        .add_filter("DXF", &["dxf"])
        .set_file_name(format!("{}.dxf", view.name))
        .save_file()
    else {
        return;
    };
    let text = hausit_views::dxf::write(&app.doc, &hausit_views::build(&app.doc, &view));
    match std::fs::write(&path, text) {
        Ok(()) => app.status = format!("Exported {} to {}", view.name, path.display()),
        Err(e) => app.dialogs.error = Some(format!("Could not write the DXF file: {e}")),
    }
}

fn export_ifc(app: &mut HausitApp) {
    let name = if app.doc.project_info.name.is_empty() {
        "Project.ifc".to_string()
    } else {
        format!("{}.ifc", app.doc.project_info.name)
    };
    let Some(path) = rfd::FileDialog::new().add_filter("IFC", &["ifc"]).set_file_name(name).save_file() else {
        return;
    };
    match hausit_io::export_ifc(&app.doc, &path) {
        Ok(()) => app.status = format!("Exported IFC4 to {}", path.display()),
        Err(e) => app.dialogs.error = Some(format!("Could not write the IFC file: {e}")),
    }
}
