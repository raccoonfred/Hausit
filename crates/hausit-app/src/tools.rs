//! Interactive tools: placement of walls, doors, windows, floors, roofs,
//! levels, grids and sections, and the Move/Copy modify tools. Also object
//! snapping and Revit's two-letter keyboard shortcuts.

use crate::app::{HausitApp, RibbonTab};
use hausit_core::geom::{closest_on_segment, line_intersection, perp};
use hausit_core::{DVec2, ElementId};
use hausit_model::element::*;
use hausit_model::geometry::wall_outline;
use hausit_model::{Category, ElementData, ViewKind};

#[derive(Clone, Debug, PartialEq)]
pub enum Tool {
    Modify,
    Wall { start: Option<DVec2>, structural: bool },
    Door,
    Window,
    Floor { pts: Vec<DVec2>, structural: bool },
    Roof { pts: Vec<DVec2> },
    Level,
    Grid { start: Option<DVec2> },
    Section { start: Option<DVec2> },
    Move { base: Option<DVec2> },
    Copy { base: Option<DVec2> },
}

impl Tool {
    /// Contextual tab title, e.g. "Modify | Place Wall".
    pub fn context_title(&self) -> &'static str {
        match self {
            Tool::Modify => "Modify",
            Tool::Wall { .. } => "Modify | Place Wall",
            Tool::Door => "Modify | Place Door",
            Tool::Window => "Modify | Place Window",
            Tool::Floor { .. } => "Modify | Create Floor Boundary",
            Tool::Roof { .. } => "Modify | Create Roof Footprint",
            Tool::Level => "Modify | Place Level",
            Tool::Grid { .. } => "Modify | Place Grid",
            Tool::Section { .. } => "Modify | Section",
            Tool::Move { .. } => "Modify | Move",
            Tool::Copy { .. } => "Modify | Copy",
        }
    }

    /// Status bar prompt.
    pub fn prompt(&self) -> &'static str {
        match self {
            Tool::Modify => "Click to select, drag to window-select. Ctrl adds, Shift removes, Tab cycles overlapping elements.",
            Tool::Wall { start: None, .. } => "Click to enter wall start point.",
            Tool::Wall { .. } => "Click to enter wall end point, or type a length and press Enter. Esc ends the chain.",
            Tool::Door => "Click on a wall to place a door. The side you click is the side it swings to.",
            Tool::Window => "Click on a wall to place a window.",
            Tool::Floor { .. } | Tool::Roof { .. } => {
                "Click to add boundary points. Click the first point or press Finish (Enter) to close the sketch."
            }
            Tool::Level => "Click in an elevation or section to place a level at that height.",
            Tool::Grid { start: None } => "Click to enter grid start point.",
            Tool::Grid { .. } => "Click to enter grid end point.",
            Tool::Section { start: None } => "Click to enter section start point.",
            Tool::Section { .. } => "Click to enter section end point.",
            Tool::Move { base: None } | Tool::Copy { base: None } => "Click to enter move start point.",
            Tool::Move { .. } | Tool::Copy { .. } => "Click to enter move end point.",
        }
    }

    pub fn category(&self) -> Option<Category> {
        match self {
            Tool::Wall { .. } => Some(Category::Walls),
            Tool::Door => Some(Category::Doors),
            Tool::Window => Some(Category::Windows),
            Tool::Floor { .. } => Some(Category::Floors),
            Tool::Roof { .. } => Some(Category::Roofs),
            _ => None,
        }
    }

    /// The last fixed point, used for rubber-band lines and typed lengths.
    pub fn anchor(&self) -> Option<DVec2> {
        match self {
            Tool::Wall { start, .. } | Tool::Grid { start } | Tool::Section { start } => *start,
            Tool::Move { base } | Tool::Copy { base } => *base,
            Tool::Floor { pts, .. } | Tool::Roof { pts } => pts.last().copied(),
            _ => None,
        }
    }

    pub fn is_sketch(&self) -> bool {
        matches!(self, Tool::Floor { .. } | Tool::Roof { .. })
    }

    /// Forgets in-progress input but stays in the tool.
    pub fn cancel_keep_kind(&mut self) {
        match self {
            Tool::Wall { start, .. } | Tool::Grid { start } | Tool::Section { start } => *start = None,
            Tool::Move { base } | Tool::Copy { base } => *base = None,
            Tool::Floor { pts, .. } | Tool::Roof { pts } => pts.clear(),
            _ => {}
        }
    }

    pub fn cancel(&mut self) {
        *self = Tool::Modify;
    }

    /// Esc: first clears in-progress input, then leaves the tool.
    pub fn escape(&mut self) {
        let busy = match self {
            Tool::Floor { pts, .. } | Tool::Roof { pts } => !pts.is_empty(),
            _ => self.anchor().is_some(),
        };
        if busy {
            self.cancel_keep_kind();
        } else {
            self.cancel();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawShape {
    Line,
    Rectangle,
}

/// Options bar settings, kept between uses like Revit does.
#[derive(Clone, Debug)]
pub struct ToolOptions {
    /// Wall top: a level, or `INVALID` for unconnected.
    pub wall_top: ElementId,
    pub wall_height: f64,
    pub location_line: LocationLine,
    pub chain: bool,
    pub shape: DrawShape,
    pub roof_slope: f64,
    pub roof_shape: RoofShape,
    pub roof_overhang: f64,
    /// Length typed while drawing.
    pub typed: String,
}

impl Default for ToolOptions {
    fn default() -> Self {
        ToolOptions {
            wall_top: ElementId::INVALID,
            wall_height: 3000.0,
            location_line: LocationLine::WallCenterline,
            chain: true,
            shape: DrawShape::Line,
            roof_slope: 30.0,
            roof_shape: RoofShape::Hip,
            roof_overhang: 600.0,
            typed: String::new(),
        }
    }
}

// ----- starting tools ---------------------------------------------------------

pub fn start(app: &mut HausitApp, tool: Tool) {
    let needs_plan = matches!(
        tool,
        Tool::Wall { .. } | Tool::Door | Tool::Window | Tool::Floor { .. } | Tool::Roof { .. } | Tool::Grid { .. } | Tool::Section { .. }
    );
    let needs_selection = matches!(tool, Tool::Move { .. } | Tool::Copy { .. });
    if needs_plan && app.active_level().is_none() {
        app.status = "This tool works in a floor plan. Open a floor plan from the Project Browser.".into();
        return;
    }
    if needs_selection && app.selection.iter().all(|id| app.doc.element(*id).is_none()) {
        app.status = "Select elements first, then choose Move or Copy.".into();
        return;
    }
    if matches!(tool, Tool::Level) {
        let ok = app
            .active_view
            .and_then(|v| app.doc.view(v))
            .map(|v| matches!(v.kind, ViewKind::Elevation { .. } | ViewKind::Section { .. }))
            .unwrap_or(false);
        if !ok {
            app.status = "Levels are placed in an elevation or section view. Open one from the Project Browser.".into();
            return;
        }
    }
    if let Some(cat) = tool.category() {
        app.type_for(cat);
    }
    if matches!(tool, Tool::Wall { .. }) && !app.options.wall_top.is_valid() {
        // Default height: up to the level above, as in Revit's template.
        if let Some(above) = app.active_level().and_then(|l| app.doc.level_above(l)) {
            app.options.wall_top = above;
        }
    }
    if !needs_selection {
        app.selection.clear();
    }
    app.options.typed.clear();
    app.tool = tool;
    app.ribbon_tab = RibbonTab::Modify;
    app.status = app.tool.prompt().into();
}

// ----- snapping ---------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapKind {
    Endpoint,
    Midpoint,
    Intersection,
    Nearest,
}

/// Snaps a plan point to nearby wall/grid points. `tol` is in model mm.
pub fn snap(app: &HausitApp, p: DVec2, tol: f64) -> Option<(DVec2, SnapKind)> {
    let level = app.active_level();
    let best = std::cell::Cell::new(None::<(DVec2, SnapKind, f64)>);
    let consider = |q: DVec2, kind: SnapKind, bias: f64| {
        let d = q.distance(p) * bias;
        if d <= tol && best.get().map(|b| d < b.2).unwrap_or(true) {
            best.set(Some((q, kind, d)));
        }
    };
    let mut segments: Vec<(DVec2, DVec2)> = Vec::new();
    for e in app.doc.elements() {
        match &e.data {
            ElementData::Wall(w) if Some(w.base_level) == level => {
                consider(w.start, SnapKind::Endpoint, 0.6);
                consider(w.end, SnapKind::Endpoint, 0.6);
                consider((w.start + w.end) * 0.5, SnapKind::Midpoint, 0.8);
                if let Some(o) = wall_outline(&app.doc, e.id) {
                    for q in o {
                        consider(q, SnapKind::Endpoint, 0.7);
                    }
                    segments.push((o[0], o[1]));
                    segments.push((o[3], o[2]));
                }
                segments.push((w.start, w.end));
            }
            ElementData::Grid(g) => {
                consider(g.start, SnapKind::Endpoint, 0.7);
                consider(g.end, SnapKind::Endpoint, 0.7);
                segments.push((g.start, g.end));
            }
            ElementData::Floor(f) if Some(f.level) == level => f.boundary.iter().for_each(|q| consider(*q, SnapKind::Endpoint, 0.8)),
            _ => {}
        }
    }
    if let Tool::Floor { pts, .. } | Tool::Roof { pts } = &app.tool {
        pts.iter().for_each(|q| consider(*q, SnapKind::Endpoint, 0.5));
    }
    // Intersections between grids and wall lines near the cursor.
    let near: Vec<_> = segments
        .iter()
        .filter(|(a, b)| hausit_core::geom::distance_to_segment(p, *a, *b) < tol * 2.0)
        .collect();
    for i in 0..near.len() {
        for j in i + 1..near.len() {
            let (a, b) = near[i];
            let (c, d) = near[j];
            if let Some(x) = line_intersection(*a, *b - *a, *c, *d - *c) {
                if hausit_core::geom::distance_to_segment(x, *a, *b) < 1.0 && hausit_core::geom::distance_to_segment(x, *c, *d) < 1.0 {
                    consider(x, SnapKind::Intersection, 0.65);
                }
            }
        }
    }
    if best.get().is_none() {
        for (a, b) in &segments {
            let (q, _) = closest_on_segment(p, *a, *b);
            consider(q, SnapKind::Nearest, 1.6);
        }
    }
    best.get().map(|b| (b.0, b.1))
}

/// Constrains `p` to horizontal/vertical from `anchor` when close (Revit's
/// orthogonal alignment), or always when `ortho` is held (Shift).
pub fn align(anchor: DVec2, p: DVec2, tol: f64, ortho: bool) -> DVec2 {
    let d = p - anchor;
    if ortho {
        return if d.x.abs() >= d.y.abs() {
            DVec2::new(p.x, anchor.y)
        } else {
            DVec2::new(anchor.x, p.y)
        };
    }
    if d.y.abs() < tol && d.x.abs() > tol {
        DVec2::new(p.x, anchor.y)
    } else if d.x.abs() < tol && d.y.abs() > tol {
        DVec2::new(anchor.x, p.y)
    } else {
        p
    }
}

// ----- placement --------------------------------------------------------------

/// Where a door/window would go for a cursor position in plan.
pub struct HostPick {
    pub wall: ElementId,
    pub offset: f64,
    pub flip_facing: bool,
}

pub fn pick_host(app: &HausitApp, p: DVec2, width: f64, tol: f64) -> Option<HostPick> {
    let level = app.active_level()?;
    let mut best: Option<(HostPick, f64)> = None;
    for e in app.doc.elements_of(Category::Walls) {
        let w = e.as_wall().unwrap();
        if w.base_level != level || w.length() < width {
            continue;
        }
        let half_t = app.doc.type_of(e.id).map(|t| t.thickness() / 2.0).unwrap_or(100.0);
        let (q, t) = closest_on_segment(p, w.start, w.end);
        let d = q.distance(p);
        if d > half_t + tol {
            continue;
        }
        if best.as_ref().map(|b| d < b.1).unwrap_or(true) {
            let offset = (t * w.length()).clamp(width / 2.0, w.length() - width / 2.0);
            // Cursor on the exterior (left) side flips the swing outwards.
            let flip_facing = (p - q).dot(perp(w.direction())) > 0.0;
            best = Some((
                HostPick {
                    wall: e.id,
                    offset,
                    flip_facing,
                },
                d,
            ));
        }
    }
    best.map(|b| b.0)
}

/// Wall centreline for a drawn line, honouring the Location Line option.
fn centreline(app: &HausitApp, a: DVec2, b: DVec2, type_id: ElementId) -> (DVec2, DVec2) {
    let t = app.doc.family_type(type_id).map(|t| t.thickness()).unwrap_or(200.0);
    let n = perp((b - a).normalize_or_zero());
    let shift = match app.options.location_line {
        LocationLine::WallCenterline => 0.0,
        LocationLine::FinishFaceExterior => -t / 2.0,
        LocationLine::FinishFaceInterior => t / 2.0,
    };
    (a + n * shift, b + n * shift)
}

fn make_wall(app: &mut HausitApp, a: DVec2, b: DVec2, structural: bool) -> Option<ElementId> {
    let wall = build_wall(app, a, b, structural)?;
    app.transact("Create Wall", |doc| Ok(doc.add_element(ElementData::Wall(wall))))
}

fn build_wall(app: &mut HausitApp, a: DVec2, b: DVec2, structural: bool) -> Option<Wall> {
    if a.distance(b) < 10.0 {
        app.status = "Wall is too short.".into();
        return None;
    }
    let level = app.active_level()?;
    let type_id = app.type_for(Category::Walls)?;
    let (s, e) = centreline(app, a, b, type_id);
    let top = if app.options.wall_top.is_valid() && app.doc.level(app.options.wall_top).is_some() {
        TopConstraint::UpToLevel {
            level: app.options.wall_top,
            offset: 0.0,
        }
    } else {
        TopConstraint::Unconnected {
            height: app.options.wall_height,
        }
    };
    Some(Wall {
        type_id,
        start: s,
        end: e,
        base_level: level,
        base_offset: 0.0,
        top,
        structural,
        room_bounding: true,
    })
}

/// Corners of a rectangle from two opposite corners, clockwise (so walls
/// get their exterior face outside, as in Revit).
fn rectangle(a: DVec2, b: DVec2) -> [DVec2; 4] {
    let (min, max) = (a.min(b), a.max(b));
    [
        DVec2::new(min.x, min.y),
        DVec2::new(min.x, max.y),
        DVec2::new(max.x, max.y),
        DVec2::new(max.x, min.y),
    ]
}

/// Handles a click at view coordinates `p` (already snapped).
pub fn click(app: &mut HausitApp, p: DVec2) {
    let mut tool = std::mem::replace(&mut app.tool, Tool::Modify);
    match &mut tool {
        Tool::Modify => {}
        Tool::Wall { start, structural } => match *start {
            None => *start = Some(p),
            Some(a) => {
                if app.options.shape == DrawShape::Rectangle {
                    let r = rectangle(a, p);
                    let walls: Option<Vec<Wall>> = (0..4).map(|i| build_wall(app, r[i], r[(i + 1) % 4], *structural)).collect();
                    if let Some(walls) = walls {
                        app.transact("Create Walls", |doc| {
                            for w in walls {
                                doc.add_element(ElementData::Wall(w));
                            }
                            Ok(())
                        });
                    }
                    *start = None;
                } else if make_wall(app, a, p, *structural).is_some() {
                    *start = if app.options.chain { Some(p) } else { None };
                }
            }
        },
        Tool::Door | Tool::Window => {
            let cat = if matches!(tool, Tool::Door) {
                Category::Doors
            } else {
                Category::Windows
            };
            let Some(type_id) = app.type_for(cat) else { return };
            let ty = app.doc.family_type(type_id).unwrap();
            let (width, sill) = (
                ty.width(),
                if cat == Category::Windows {
                    ty.length_or("Default Sill Height", 900.0)
                } else {
                    0.0
                },
            );
            match pick_host(app, p, width, 300.0) {
                Some(h) => {
                    let o = HostedOpening {
                        type_id,
                        host: h.wall,
                        offset: h.offset,
                        sill_height: sill,
                        flip_hand: false,
                        flip_facing: h.flip_facing,
                    };
                    let data = if cat == Category::Doors {
                        ElementData::Door(o)
                    } else {
                        ElementData::Window(o)
                    };
                    app.transact(&format!("Create {}", cat.singular()), |doc| Ok(doc.add_element(data)));
                }
                None => {
                    app.status = format!(
                        "A {} must be placed in a wall long enough to hold it.",
                        cat.singular().to_lowercase()
                    )
                }
            }
        }
        Tool::Floor { pts, .. } | Tool::Roof { pts } => {
            if app.options.shape == DrawShape::Rectangle {
                if let Some(a) = pts.first().copied() {
                    *pts = rectangle(a, p).to_vec();
                    app.tool = tool;
                    finish_sketch(app);
                    return;
                }
                pts.push(p);
            } else if pts.len() >= 3 && pts[0].distance(p) < 1.0 {
                app.tool = tool;
                finish_sketch(app);
                return;
            } else if pts.last().map(|l| l.distance(p) > 1.0).unwrap_or(true) {
                pts.push(p);
            }
        }
        Tool::Level => {
            let name = app.doc.next_level_name();
            let elevation = p.y.round();
            if app
                .transact("Create Level", |doc| {
                    Ok(hausit_model::template::add_level_with_plan(doc, &name, elevation))
                })
                .is_some()
            {
                app.status = format!(
                    "Created {name} at {} and its floor plan.",
                    hausit_core::format_length(elevation, app.doc.length_unit)
                );
            }
        }
        Tool::Grid { start } => match *start {
            None => *start = Some(p),
            Some(a) if a.distance(p) > 10.0 => {
                let name = app.doc.next_grid_name();
                app.transact("Create Grid", |doc| {
                    Ok(doc.add_element(ElementData::Grid(Grid { name, start: a, end: p })))
                });
                *start = None;
            }
            _ => {}
        },
        Tool::Section { start } => match *start {
            None => *start = Some(p),
            Some(a) if a.distance(p) > 10.0 => {
                let name = app.doc.unique_view_name("Section");
                let kind = ViewKind::Section {
                    start: a,
                    end: p,
                    far_clip: 15000.0,
                };
                app.transact("Create Section", |doc| Ok(doc.add_view(&name, kind)));
                app.status = format!("Created {name}. Double-click its head, or open it from the Project Browser.");
                *start = None;
            }
            _ => {}
        },
        Tool::Move { base } | Tool::Copy { base } => match *base {
            None => *base = Some(p),
            Some(a) => {
                let ids: Vec<ElementId> = app.selection.iter().copied().filter(|i| app.doc.element(*i).is_some()).collect();
                if matches!(tool, Tool::Move { .. }) {
                    app.transact("Move", |doc| {
                        doc.translate(&ids, p - a);
                        Ok(())
                    });
                } else if let Some(new) = app.transact("Copy", |doc| Ok(doc.copy(&ids, p - a))) {
                    app.selection = new;
                }
                app.tool = Tool::Modify;
                app.status = "Ready".into();
                return;
            }
        },
    }
    app.tool = tool;
    app.options.typed.clear();
    app.status = app.tool.prompt().into();
}

/// Completes a floor or roof sketch ("Finish Edit Mode").
pub fn finish_sketch(app: &mut HausitApp) {
    let Some(level) = app.active_level() else { return };
    let tool = std::mem::replace(&mut app.tool, Tool::Modify);
    match tool {
        Tool::Floor { pts, structural } if pts.len() >= 3 => {
            let Some(type_id) = app.type_for(Category::Floors) else { return };
            let f = Floor {
                type_id,
                level,
                boundary: pts,
                height_offset: 0.0,
                room_bounding: true,
                structural,
            };
            app.transact("Create Floor", |doc| Ok(doc.add_element(ElementData::Floor(f))));
            app.status = "Floor created.".into();
        }
        Tool::Roof { pts } if pts.len() >= 3 => {
            let Some(type_id) = app.type_for(Category::Roofs) else { return };
            // Overhang: offset a rectangular footprint outward.
            let mut fp = pts;
            let ov = app.options.roof_overhang;
            if fp.len() == 4 && ov != 0.0 {
                let c = fp.iter().copied().sum::<DVec2>() / 4.0;
                let (min, max) = (
                    fp.iter().fold(DVec2::INFINITY, |a, b| a.min(*b)),
                    fp.iter().fold(DVec2::NEG_INFINITY, |a, b| a.max(*b)),
                );
                if fp.iter().all(|p| (p.x - min.x).abs() < 1.0 || (p.x - max.x).abs() < 1.0) {
                    fp = fp
                        .iter()
                        .map(|p| *p + DVec2::new((p.x - c.x).signum() * ov, (p.y - c.y).signum() * ov))
                        .collect();
                }
            }
            let shape = if app.options.roof_slope <= 0.0 {
                RoofShape::Flat
            } else {
                app.options.roof_shape
            };
            let r = Roof {
                type_id,
                level,
                footprint: fp,
                base_offset: 0.0,
                slope: app.options.roof_slope,
                shape,
            };
            app.transact("Create Roof", |doc| Ok(doc.add_element(ElementData::Roof(r))));
            app.status = "Roof created.".into();
        }
        other => {
            app.tool = other;
            app.status = "A boundary needs at least three points.".into();
        }
    }
}

/// Applies a typed length along the current rubber-band direction.
pub fn apply_typed_length(app: &mut HausitApp, cursor: DVec2) {
    let Some(anchor) = app.tool.anchor() else { return };
    let Some(len) = hausit_core::parse_length(&app.options.typed, app.doc.length_unit) else {
        app.options.typed.clear();
        return;
    };
    let dir = (cursor - anchor).normalize_or_zero();
    if dir == DVec2::ZERO || len <= 0.0 {
        return;
    }
    click(app, anchor + dir * len);
    app.options.typed.clear();
}

// ----- keyboard ---------------------------------------------------------------

/// Global keys: Esc, Delete, Ctrl+Z/Y/S/O/N and Revit two-letter shortcuts
/// (WA, DR, WN, FL, RF, LL, GR, MV, CO, ZF/ZE, ZA…).
pub fn handle_shortcuts(app: &mut HausitApp, ctx: &egui::Context) {
    if ctx.egui_wants_keyboard_input() || app.show_home {
        return;
    }
    let (esc, del, enter, z, y, s, o, n, shift_s, text, now) = ctx.input(|i| {
        let cmd = i.modifiers.command;
        let text: String = i
            .events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Text(t) if !cmd => Some(t.clone()),
                _ => None,
            })
            .collect();
        (
            i.key_pressed(egui::Key::Escape),
            i.key_pressed(egui::Key::Delete) || (i.key_pressed(egui::Key::Backspace) && app.options.typed.is_empty()),
            i.key_pressed(egui::Key::Enter),
            cmd && i.key_pressed(egui::Key::Z) && !i.modifiers.shift,
            cmd && (i.key_pressed(egui::Key::Y) || (i.key_pressed(egui::Key::Z) && i.modifiers.shift)),
            cmd && i.key_pressed(egui::Key::S) && !i.modifiers.shift,
            cmd && i.key_pressed(egui::Key::O),
            cmd && i.key_pressed(egui::Key::N),
            cmd && i.key_pressed(egui::Key::S) && i.modifiers.shift,
            text,
            i.time,
        )
    });
    if esc {
        if app.tool == Tool::Modify {
            app.selection.clear();
        }
        app.tool.escape();
        app.options.typed.clear();
        app.status = app.tool.prompt().into();
    }
    if del && app.tool == Tool::Modify {
        app.delete_selection();
    }
    if z {
        app.undo();
    }
    if y {
        app.redo();
    }
    if s {
        app.save();
    }
    if shift_s {
        app.save_as();
    }
    if o {
        app.open_dialog();
    }
    if n {
        app.set_document(hausit_model::template::new_project(), None);
    }
    if enter && app.tool.is_sketch() && app.options.typed.is_empty() {
        finish_sketch(app);
    }

    // While drawing, digits go to the typed-length box.
    if app.tool.anchor().is_some() && !app.tool.is_sketch() || (app.tool.is_sketch() && app.tool.anchor().is_some()) {
        for c in text.chars() {
            if c.is_ascii_digit() || ".'\"m-".contains(c) || (c == ' ' && !app.options.typed.is_empty()) {
                app.options.typed.push(c);
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Backspace)) {
            app.options.typed.pop();
        }
        if !app.options.typed.is_empty() {
            return;
        }
    }

    // Two-letter shortcuts.
    let letters: String = text
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if letters.is_empty() {
        return;
    }
    if now - app.shortcut.1 > 1.5 {
        app.shortcut.0.clear();
    }
    app.shortcut.0.push_str(&letters);
    app.shortcut.1 = now;
    let code = app.shortcut.0.clone();
    let tail = |n: usize| if code.len() >= n { &code[code.len() - n..] } else { "" };
    let cmd = tail(2);
    let handled = match cmd {
        "WA" => {
            start(
                app,
                Tool::Wall {
                    start: None,
                    structural: false,
                },
            );
            true
        }
        "DR" => {
            start(app, Tool::Door);
            true
        }
        "WN" => {
            start(app, Tool::Window);
            true
        }
        "FL" => {
            start(
                app,
                Tool::Floor {
                    pts: vec![],
                    structural: false,
                },
            );
            true
        }
        "RF" => {
            start(app, Tool::Roof { pts: vec![] });
            true
        }
        "LL" => {
            start(app, Tool::Level);
            true
        }
        "GR" => {
            start(app, Tool::Grid { start: None });
            true
        }
        "MV" => {
            start(app, Tool::Move { base: None });
            true
        }
        "CO" | "CC" => {
            start(app, Tool::Copy { base: None });
            true
        }
        "ZF" | "ZE" | "ZA" => {
            if let Some(v) = app.active_view {
                if let Some(c) = app.cameras.get_mut(&v) {
                    c.fitted = false;
                }
            }
            true
        }
        "DE" => {
            app.delete_selection();
            true
        }
        _ => false,
    };
    if handled {
        app.shortcut.0.clear();
    } else if app.shortcut.0.len() > 2 {
        app.shortcut.0 = tail(1).to_string();
    }
}
