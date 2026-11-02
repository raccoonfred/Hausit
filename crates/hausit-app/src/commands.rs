//! Every command Hausit offers, in one list: what keyboard shortcuts run,
//! what the command search (Ctrl+K or F3) finds, what "Repeat" repeats
//! and what Help ▸ Keyboard Shortcuts lists.

use crate::app::HausitApp;
use crate::tools::{self, Sketch, Tool};
use hausit_core::DVec2;
use hausit_model::{Category, ViewKind};

pub struct Command {
    pub name: &'static str,
    /// Two-letter keyboard shortcuts, as in Revit (first one is shown).
    pub keys: &'static [&'static str],
    /// Where the command lives in the ribbon or menus.
    pub place: &'static str,
    pub run: fn(&mut HausitApp),
}

macro_rules! cmd {
    ($name:expr, [$($k:expr),*], $place:expr, $run:expr) => {
        Command { name: $name, keys: &[$($k),*], place: $place, run: $run }
    };
}

fn fit(app: &mut HausitApp) {
    if let Some(c) = app.active_view.and_then(|v| app.cameras.get_mut(&v)) {
        c.fitted = false;
    }
}

fn open_3d(app: &mut HausitApp) {
    let id = app.doc.views().find(|v| matches!(v.kind, ViewKind::ThreeD)).map(|v| v.id);
    let id = id.or_else(|| app.transact("Create 3D View", |doc| Ok(doc.add_view("{3D}", ViewKind::ThreeD))));
    if let Some(id) = id {
        app.open_view(id);
    }
}

fn hide(app: &mut HausitApp, c: crate::hide_isolate::HideCmd) {
    crate::hide_isolate::temporary(app, c);
}

pub static COMMANDS: &[Command] = &[
    // Architecture
    cmd!("Wall", ["WA"], "Architecture ▸ Build", |a| tools::start(
        a,
        Tool::Wall {
            start: None,
            structural: false
        }
    )),
    cmd!("Door", ["DR"], "Architecture ▸ Build", |a| tools::start(a, Tool::Door)),
    cmd!("Window", ["WN"], "Architecture ▸ Build", |a| tools::start(a, Tool::Window)),
    cmd!("Component", ["CM"], "Architecture ▸ Build", |a| tools::start(
        a,
        Tool::Component {
            category: Category::Furniture
        }
    )),
    cmd!("Structural Column", ["CL"], "Architecture ▸ Build", |a| tools::start(
        a,
        Tool::Column
    )),
    cmd!("Roof by Footprint", ["RF"], "Architecture ▸ Build", |a| tools::start(
        a,
        Tool::Roof { sketch: Sketch::default() }
    )),
    cmd!("Ceiling", [], "Architecture ▸ Build", |a| tools::start(a, Tool::Ceiling)),
    cmd!("Sketch Ceiling", [], "Architecture ▸ Build ▸ Ceiling", |a| {
        tools::start(
            a,
            Tool::Boundary {
                sketch: Sketch::default(),
                kind: tools::BoundaryKind::Ceiling,
            },
        );
        a.options.sketch_mode = tools::DrawShape::Line;
    }),
    cmd!("Shaft Opening", [], "Architecture ▸ Opening", start_shaft),
    cmd!("Edit Boundary", ["EB"], "Modify | Floors, Roofs, Ceilings", |a| {
        match a.selection.first().copied().filter(|id| crate::canvas::sketched(a, *id)) {
            Some(id) => tools::edit_boundary(a, id),
            None => a.status = "Select a floor, roof, ceiling, shaft opening or toposolid first.".into(),
        }
    }),
    cmd!("Floor", ["FL"], "Architecture ▸ Build", |a| tools::start(
        a,
        Tool::Floor {
            sketch: Sketch::default(),
            structural: false
        }
    )),
    cmd!("Curtain Grid", [], "Architecture ▸ Build", |a| tools::start(a, Tool::CurtainGrid)),
    cmd!("Stair", [], "Architecture ▸ Circulation", |a| tools::start(
        a,
        Tool::Stair { pts: vec![] }
    )),
    cmd!("Railing", [], "Architecture ▸ Circulation", |a| tools::start(
        a,
        Tool::Railing { pts: vec![] }
    )),
    cmd!("Room", ["RM"], "Architecture ▸ Room & Area", |a| tools::start(a, Tool::Room)),
    cmd!("Room Separator", [], "Architecture ▸ Room & Area", |a| tools::start(
        a,
        Tool::RoomSeparation { start: None }
    )),
    cmd!("Area Plan", [], "Architecture ▸ Room & Area", |a| {
        a.dialogs.new_area_plan = Some(crate::dialogs::NewAreaPlan {
            scheme: hausit_model::AreaScheme::Gross,
            level: a.active_level().or_else(|| a.doc.levels().first().map(|l| l.0)),
            boundaries: true,
        });
    }),
    cmd!("Area Boundary", [], "Architecture ▸ Room & Area", |a| tools::start(
        a,
        Tool::AreaBoundary { start: None }
    )),
    cmd!("Area", [], "Architecture ▸ Room & Area", |a| tools::start(a, Tool::Area)),
    cmd!("Level", ["LL"], "Architecture ▸ Datum", |a| tools::start(a, Tool::Level)),
    cmd!("Grid", ["GR"], "Architecture ▸ Datum", |a| tools::start(
        a,
        Tool::Grid { start: None }
    )),
    // Structure
    cmd!("Beam", ["BM"], "Structure ▸ Structure", |a| tools::start(
        a,
        Tool::Beam { start: None, brace: false }
    )),
    cmd!("Brace", ["BR"], "Structure ▸ Structure", |a| tools::start(
        a,
        Tool::Beam { start: None, brace: true }
    )),
    cmd!("Structural Wall", [], "Structure ▸ Structure", |a| tools::start(
        a,
        Tool::Wall {
            start: None,
            structural: true
        }
    )),
    cmd!("Structural Floor", [], "Structure ▸ Structure", |a| tools::start(
        a,
        Tool::Floor {
            sketch: Sketch::default(),
            structural: true
        }
    )),
    cmd!("Isolated Foundation", [], "Structure ▸ Foundation", |a| tools::start(
        a,
        Tool::Footing
    )),
    cmd!("Wall Foundation", ["FT"], "Structure ▸ Foundation", |a| tools::start(
        a,
        Tool::WallFoundation
    )),
    cmd!("Columns at Grids", [], "Modify | Structural Columns", tools::columns_at_grids),
    // Insert
    cmd!("Link Model", [], "Insert ▸ Link", crate::links::link_dialog),
    cmd!("Manage Links", [], "Insert ▸ Link", |a| a.dialogs.manage_links =
        Some(Default::default())),
    cmd!("Load Family", [], "Insert ▸ Load from Library", crate::families::load_from_file),
    cmd!("Family Library", [], "Insert ▸ Load from Library", crate::families::open_library),
    // Annotate
    cmd!("Aligned Dimension", ["DI"], "Annotate ▸ Dimension", |a| tools::start(
        a,
        Tool::Dimension { refs: vec![] }
    )),
    cmd!("Linear Dimension", [], "Annotate ▸ Dimension", |a| tools::start(
        a,
        Tool::LinearDimension { pts: vec![] }
    )),
    cmd!("Spot Elevation", [], "Annotate ▸ Spot", |a| tools::start(
        a,
        Tool::SpotElevation { point: None }
    )),
    cmd!("Detail Line", ["DL"], "Annotate ▸ Detail", |a| tools::start(
        a,
        Tool::DetailLine { start: None }
    )),
    cmd!("Filled Region", [], "Annotate ▸ Detail", |a| tools::start(
        a,
        Tool::FilledRegion { pts: vec![] }
    )),
    cmd!("Revision Cloud", [], "Annotate ▸ Revision", |a| tools::start(
        a,
        Tool::RevisionCloud { pts: vec![] }
    )),
    cmd!("Color Fill Legend", [], "Annotate ▸ Color Fill", |a| tools::start(
        a,
        Tool::ColorLegend
    )),
    cmd!("Text", ["TX"], "Annotate ▸ Text", |a| tools::start(a, Tool::Text)),
    cmd!("Tag by Category", ["TG"], "Annotate ▸ Tag", |a| tools::start(a, Tool::Tag)),
    cmd!("Tag All Not Tagged", [], "Annotate ▸ Tag", |a| {
        a.dialogs.tag_all = Some(
            hausit_model::annotation::TAGGABLE
                .iter()
                .map(|c| (*c, matches!(c, Category::Doors | Category::Windows)))
                .collect(),
        );
    }),
    // Massing & Site
    cmd!("Toposolid", [], "Massing & Site ▸ Model Site", |a| tools::start(
        a,
        Tool::Toposolid { sketch: Sketch::default() }
    )),
    cmd!("Property Line", [], "Massing & Site ▸ Modify Site", |a| tools::start(
        a,
        Tool::PropertyLine { pts: vec![] }
    )),
    // View
    cmd!(
        "Visibility/Graphics",
        ["VV", "VG"],
        "View ▸ Graphics",
        crate::view_graphics::open_for_active_view
    ),
    cmd!("View Templates", [], "View ▸ Graphics", crate::view_graphics::open_templates),
    cmd!("Default 3D View", [], "View ▸ Create", open_3d),
    cmd!("Section", [], "View ▸ Create", |a| tools::start(a, Tool::Section { start: None })),
    cmd!("Elevation", [], "View ▸ Create", |a| tools::start(a, Tool::Elevation)),
    cmd!("Callout", [], "View ▸ Create", |a| tools::start(a, Tool::Callout { start: None })),
    cmd!("Plan Views", [], "View ▸ Create", |a| a.dialogs.plan_views =
        Some(crate::dialogs::PlanViews::new(&a.doc))),
    cmd!("Schedule/Quantities", [], "View ▸ Create", |a| {
        a.dialogs.new_schedule = Some(crate::dialogs::NewSchedule {
            category: Category::Doors,
            name: String::new(),
        });
    }),
    cmd!("Sheet", [], "View ▸ Sheet Composition", crate::sheets::start_new_sheet),
    cmd!("Place View on Sheet", [], "View ▸ Sheet Composition", |a| {
        a.dialogs.place_view = crate::sheets::active_sheet(a).map(|s| (s, None));
        if a.dialogs.place_view.is_none() {
            a.status = "Open a sheet first, then Place View.".into();
        }
    }),
    cmd!("Zoom to Fit", ["ZF", "ZE", "ZA"], "View Control Bar", fit),
    cmd!("Close Inactive Views", [], "View ▸ Windows", |a| a
        .open_views
        .retain(|v| Some(*v) == a.active_view)),
    cmd!("Hide Element (temporary)", ["HH"], "View Control Bar ▸ Hide/Isolate", |a| hide(
        a,
        crate::hide_isolate::HideCmd::HideElement
    )),
    cmd!("Isolate Element (temporary)", ["HI"], "View Control Bar ▸ Hide/Isolate", |a| hide(
        a,
        crate::hide_isolate::HideCmd::IsolateElement
    )),
    cmd!("Hide Category (temporary)", ["HC"], "View Control Bar ▸ Hide/Isolate", |a| hide(
        a,
        crate::hide_isolate::HideCmd::HideCategory
    )),
    cmd!("Isolate Category (temporary)", ["IC"], "View Control Bar ▸ Hide/Isolate", |a| {
        hide(a, crate::hide_isolate::HideCmd::IsolateCategory)
    }),
    cmd!("Reset Temporary Hide/Isolate", ["HR"], "View Control Bar ▸ Hide/Isolate", |a| {
        hide(a, crate::hide_isolate::HideCmd::Reset)
    }),
    cmd!(
        "Hide in View: Elements",
        ["EH"],
        "Right-click ▸ Hide in View",
        crate::hide_isolate::hide_in_view
    ),
    cmd!(
        "Hide in View: Category",
        ["VH"],
        "Right-click ▸ Hide in View",
        crate::canvas::hide_category_in_view
    ),
    // Manage
    cmd!("Project Information", [], "Manage ▸ Settings", |a| a.dialogs.project_info = true),
    cmd!("Project Units", ["UN"], "Manage ▸ Settings", |a| a.dialogs.units = true),
    cmd!("Materials", [], "Manage ▸ Settings", |a| {
        a.dialogs.materials = Some(crate::dialogs::MaterialsDialog {
            rows: a.doc.materials.iter().map(|m| (m.name.clone(), m.clone())).collect(),
            selected: 0,
        });
    }),
    cmd!("Object Styles", [], "Manage ▸ Settings", |a| a.dialogs.object_styles =
        Some((a.doc.object_styles.clone(), a.doc.line_weights.clone()))),
    cmd!("Phases", [], "Manage ▸ Phasing", |a| a.dialogs.phases =
        Some(crate::phases::PhasesDialog::new(&a.doc))),
    cmd!("Design Options", [], "Manage ▸ Design Options", |a| a.dialogs.design_options =
        Some(Default::default())),
    cmd!("Sheet Issues/Revisions", [], "Manage ▸ Revisions", |a| a.dialogs.revisions =
        Some(crate::revisions::RevisionsDialog { selected: None })),
    cmd!("Worksets", [], "Collaborate ▸ Manage Collaboration", |a| a.dialogs.worksets =
        Some(Default::default())),
    // Modify
    cmd!("Modify (Select)", [], "Modify ▸ Select", |a| {
        a.tool.cancel();
        a.status = a.tool.prompt().into();
    }),
    cmd!("Move", ["MV"], "Modify ▸ Modify", |a| tools::start(a, Tool::Move { base: None })),
    cmd!("Copy", ["CO", "CC"], "Modify ▸ Modify", |a| tools::start(
        a,
        Tool::Copy { base: None }
    )),
    cmd!("Rotate", ["RO"], "Modify ▸ Modify", |a| tools::start(
        a,
        Tool::Rotate {
            center: DVec2::ZERO,
            from: None
        }
    )),
    cmd!("Mirror - Pick Axis", ["MM"], "Modify ▸ Modify", |a| tools::start(
        a,
        Tool::Mirror { draw: false, start: None }
    )),
    cmd!("Mirror - Draw Axis", ["DM"], "Modify ▸ Modify", |a| tools::start(
        a,
        Tool::Mirror { draw: true, start: None }
    )),
    cmd!("Trim/Extend to Corner", ["TR"], "Modify ▸ Modify", |a| tools::start(
        a,
        Tool::Trim { first: None }
    )),
    cmd!("Split Element", ["SL"], "Modify ▸ Modify", |a| tools::start(a, Tool::Split)),
    cmd!("Align", ["AL"], "Modify ▸ Modify", |a| tools::start(
        a,
        Tool::Align { reference: None }
    )),
    cmd!("Offset", ["OF"], "Modify ▸ Modify", |a| tools::start(a, Tool::Offset)),
    cmd!("Array", ["AR"], "Modify ▸ Modify", |a| tools::start(
        a,
        Tool::Array { start: None }
    )),
    cmd!("Demolish", [], "Modify ▸ Modify", |a| tools::start(a, Tool::Demolish)),
    cmd!("Pin", ["PN"], "Modify ▸ Modify", |a| crate::hide_isolate::pin(a, true)),
    cmd!("Unpin", ["UP"], "Modify ▸ Modify", |a| crate::hide_isolate::pin(a, false)),
    cmd!("Delete", ["DE"], "Modify ▸ Modify", HausitApp::delete_selection),
    cmd!("Filter Selection", ["FI"], "Modify ▸ Selection", |a| a.dialogs.filter =
        Some(crate::palette::FilterDialog::new(a))),
    cmd!(
        "Select All Instances",
        ["SA"],
        "Right-click ▸ Select All Instances",
        select_all_instances
    ),
    cmd!("Create Group", ["GP"], "Modify ▸ Group", crate::groups::create_group),
    cmd!("Selection Box", ["BX"], "Modify ▸ View", HausitApp::selection_box),
    cmd!(
        "Copy to Clipboard",
        [],
        "Modify ▸ Clipboard",
        crate::hide_isolate::copy_to_clipboard
    ),
    cmd!("Paste", [], "Modify ▸ Clipboard", crate::hide_isolate::paste),
    cmd!(
        "Paste Aligned to Selected Levels",
        [],
        "Modify ▸ Clipboard",
        crate::hide_isolate::open_paste_levels
    ),
    // File
    cmd!("New Project", [], "File (Ctrl+N)", |a| a
        .set_document(hausit_model::template::new_project(), None)),
    cmd!("Open", [], "File (Ctrl+O)", HausitApp::open_dialog),
    cmd!("Save", [], "File (Ctrl+S)", |a| {
        a.save();
    }),
    cmd!("Save As", [], "File (Ctrl+Shift+S)", |a| {
        a.save_as();
    }),
    cmd!("Export PDF", [], "File ▸ Export", crate::sheets::start_export),
    cmd!("Export IFC", [], "File ▸ Export", crate::ribbon::export_ifc),
    cmd!("Export DXF (current view)", [], "File ▸ Export", crate::ribbon::export_dxf),
    // Interface
    cmd!("Properties palette", ["PP"], "View ▸ User Interface (Ctrl+1)", |a| a
        .ui
        .show_properties =
        !a.ui.show_properties),
    cmd!("Project Browser", ["PB"], "View ▸ User Interface", |a| a.ui.show_browser =
        !a.ui.show_browser),
    cmd!("Keyboard Shortcuts", ["KS"], "Help", |a| a.dialogs.shortcuts = true),
    cmd!("Search Commands", [], "Ctrl+K or F3", |a| a.palette.open = true),
];

pub fn start_shaft(app: &mut HausitApp) {
    tools::start(
        app,
        Tool::Boundary {
            sketch: Sketch::default(),
            kind: tools::BoundaryKind::Shaft,
        },
    );
    app.options.sketch_mode = tools::DrawShape::Rectangle;
}

fn select_all_instances(app: &mut HausitApp) {
    if let Some(t) = app.selection.first().and_then(|s| app.doc.element(*s)).and_then(|e| e.type_id()) {
        app.selection = app.doc.elements().filter(|e| e.type_id() == Some(t)).map(|e| e.id).collect();
        app.status = format!("Selected {} instance(s)", app.selection.len());
    } else {
        app.status = "Select an element first.".into();
    }
}

/// Runs a command and remembers it for Repeat and Recent Commands.
pub fn run(app: &mut HausitApp, index: usize) {
    let Some(c) = COMMANDS.get(index) else { return };
    // Opening the palette or repeating is not itself a command to repeat.
    if c.name != "Search Commands" {
        app.recent_commands.retain(|i| *i != index);
        app.recent_commands.insert(0, index);
        app.recent_commands.truncate(5);
    }
    (c.run)(app);
}

/// The command a two-letter shortcut runs.
pub fn by_key(key: &str) -> Option<usize> {
    COMMANDS.iter().position(|c| c.keys.contains(&key))
}

/// Commands matching `query`, best first: name starts with it, a word
/// starts with it, it is a shortcut, then anywhere in the name or place.
pub fn search(query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(u8, usize)> = COMMANDS
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let name = c.name.to_lowercase();
            let score = if name.starts_with(&q) {
                0
            } else if c.keys.iter().any(|k| k.eq_ignore_ascii_case(&q)) {
                1
            } else if name.split([' ', '/', '-', '(']).any(|w| w.starts_with(&q)) {
                2
            } else if name.contains(&q) {
                3
            } else if c.place.to_lowercase().contains(&q) {
                4
            } else {
                return None;
            };
            Some((score, i))
        })
        .collect();
    scored.sort();
    scored.into_iter().map(|(_, i)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for c in COMMANDS {
            for k in c.keys {
                assert!(seen.insert(*k), "shortcut {k} used twice");
                assert_eq!(k.len(), 2);
            }
        }
    }

    #[test]
    fn search_ranks_prefix_first() {
        let r = search("wa");
        assert_eq!(COMMANDS[r[0]].name, "Wall");
        let r = search("mv");
        assert_eq!(COMMANDS[r[0]].name, "Move");
        assert!(search("zzzz").is_empty());
    }
}
