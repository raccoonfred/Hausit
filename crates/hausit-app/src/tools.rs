//! Interactive tools: placement of walls, doors, windows, floors, roofs,
//! levels, grids and sections, and the Move/Copy modify tools. Also object
//! snapping and Revit's two-letter keyboard shortcuts.

use crate::app::{HausitApp, RibbonTab};
use hausit_core::geom::{closest_on_segment, distance_to_segment, line_intersection, perp};
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
    Floor { sketch: Sketch, structural: bool },
    Roof { sketch: Sketch },
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
                "Sketch a closed loop: pick walls or draw lines. Finish Edit Mode (Enter) creates it; Ctrl+Z removes the last line."
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
            Tool::Floor { sketch, .. } | Tool::Roof { sketch } => sketch.start,
            _ => None,
        }
    }

    pub fn sketch(&self) -> Option<&Sketch> {
        match self {
            Tool::Floor { sketch, .. } | Tool::Roof { sketch } => Some(sketch),
            _ => None,
        }
    }

    pub fn sketch_mut(&mut self) -> Option<&mut Sketch> {
        match self {
            Tool::Floor { sketch, .. } | Tool::Roof { sketch } => Some(sketch),
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
            Tool::Floor { sketch, .. } | Tool::Roof { sketch } => sketch.start = None,
            _ => {}
        }
    }

    pub fn cancel(&mut self) {
        *self = Tool::Modify;
    }

    /// Esc: first clears in-progress input, then leaves the tool. A
    /// sketch with lines stays open until Finish or Cancel, as in Revit.
    pub fn escape(&mut self) {
        if self.anchor().is_some() {
            self.cancel_keep_kind();
        } else if !self.sketch().map(|s| s.lines.is_empty()).unwrap_or(true) {
        } else {
            self.cancel();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawShape {
    Line,
    Rectangle,
    PickWalls,
}

/// One boundary line of a floor or roof sketch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SketchLine {
    pub a: DVec2,
    pub b: DVec2,
    /// Roofs: "Defines Slope".
    pub slope: bool,
    /// Roofs: overhang applied outwards when the sketch is finished.
    pub overhang: f64,
    /// The wall this line was picked from, if any.
    pub wall: Option<ElementId>,
}

/// A floor or roof boundary sketch ("Edit Boundary" mode in Revit).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sketch {
    pub lines: Vec<SketchLine>,
    /// Start of the line being drawn.
    pub start: Option<DVec2>,
    /// Where the current chain began, to stop chaining when it closes.
    pub chain_from: Option<DVec2>,
}

impl Sketch {
    /// Orders the lines into one closed loop. Returns the loop's corner
    /// points with the line each edge came from.
    pub fn closed_loop(&self) -> Result<Vec<(DVec2, SketchLine)>, String> {
        const TOL: f64 = 1.0;
        let open = || "Lines must be in closed loops. Some sketch lines are open at one end.".to_string();
        if self.lines.len() < 3 {
            return Err(if self.lines.is_empty() {
                "The sketch is empty. Pick walls or draw lines first.".into()
            } else {
                open()
            });
        }
        let mut used = vec![false; self.lines.len()];
        used[0] = true;
        let first = self.lines[0];
        let mut out = vec![(first.a, first)];
        let mut cur = first.b;
        while cur.distance(first.a) > TOL {
            let next = (0..self.lines.len()).filter(|i| !used[*i]).find_map(|i| {
                let l = self.lines[i];
                if l.a.distance(cur) <= TOL {
                    Some((i, l.a, l.b))
                } else if l.b.distance(cur) <= TOL {
                    Some((i, l.b, l.a))
                } else {
                    None
                }
            });
            let Some((i, from, to)) = next else { return Err(open()) };
            used[i] = true;
            out.push((from, self.lines[i]));
            cur = to;
        }
        if used.iter().any(|u| !u) {
            return Err("A sketch can only have one loop for now. Delete the extra lines (Ctrl+Z).".into());
        }
        Ok(out)
    }
}

/// Offsets each polygon edge outwards by its own distance and rejoins
/// the corners (overhangs on picked walls).
pub fn offset_outwards(pts: &[DVec2], dist: &[f64]) -> Vec<DVec2> {
    let n = pts.len();
    let ccw = hausit_core::geom::polygon_area(pts) > 0.0;
    let line = |i: usize| {
        let d = (pts[(i + 1) % n] - pts[i]).normalize_or_zero();
        let out = if ccw { DVec2::new(d.y, -d.x) } else { DVec2::new(-d.y, d.x) };
        (pts[i] + out * dist[i], d)
    };
    (0..n)
        .map(|i| {
            let (p0, d0) = line((i + n - 1) % n);
            let (p1, d1) = line(i);
            line_intersection(p0, d0, p1, d1).unwrap_or(p1)
        })
        .collect()
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
    pub roof_overhang: f64,
    /// Roofs: new sketch lines define slope.
    pub defines_slope: bool,
    /// Draw mode for floor and roof sketches.
    pub sketch_mode: DrawShape,
    /// Level the roof being sketched will be created on.
    pub roof_level: ElementId,
    /// Length typed while drawing.
    pub typed: String,
    /// Spacebar flip while placing walls, doors and windows.
    pub flip: bool,
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
            roof_overhang: 600.0,
            defines_slope: true,
            sketch_mode: DrawShape::PickWalls,
            roof_level: ElementId::INVALID,
            typed: String::new(),
            flip: false,
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
    if matches!(tool, Tool::Roof { .. }) {
        let level = app.active_level().unwrap_or(ElementId::INVALID);
        app.options.roof_level = level;
        if let Some(above) = app.doc.level_above(level) {
            // Revit's "Lowest Level Notice".
            app.dialogs.roof_level = Some((level, above));
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
    Perpendicular,
    Nearest,
    /// Lined up with another point (Revit's dashed alignment line).
    Alignment,
    /// Angle and length increments along the rubber band.
    Increment,
}

impl SnapKind {
    pub fn label(self) -> &'static str {
        match self {
            SnapKind::Endpoint => "Endpoint",
            SnapKind::Midpoint => "Midpoint",
            SnapKind::Intersection => "Intersection",
            SnapKind::Perpendicular => "Perpendicular",
            SnapKind::Nearest => "Nearest",
            SnapKind::Alignment => "Alignment",
            SnapKind::Increment => "",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snap {
    pub point: DVec2,
    pub kind: SnapKind,
    /// Dashed guide lines to draw (alignment and angle snaps).
    pub guides: Vec<(DVec2, DVec2)>,
}

/// Lines and points on the active level that tools snap to.
fn snap_geometry(app: &HausitApp) -> (Vec<(DVec2, DVec2)>, Vec<DVec2>) {
    let level = app.active_level();
    let mut segs: Vec<(DVec2, DVec2)> = Vec::new();
    let mut points: Vec<DVec2> = Vec::new();
    let poly = |pts: &[DVec2], segs: &mut Vec<(DVec2, DVec2)>| {
        for i in 0..pts.len() {
            segs.push((pts[i], pts[(i + 1) % pts.len()]));
        }
    };
    for e in app.doc.elements() {
        match &e.data {
            ElementData::Wall(w) if Some(w.base_level) == level => {
                segs.push((w.start, w.end));
                if let Some(o) = wall_outline(&app.doc, e.id) {
                    segs.push((o[0], o[1]));
                    segs.push((o[3], o[2]));
                }
            }
            ElementData::Grid(g) => segs.push((g.start, g.end)),
            ElementData::Floor(f) if Some(f.level) == level => poly(&f.boundary, &mut segs),
            ElementData::Roof(r) => poly(&r.footprint, &mut segs),
            ElementData::Door(o) | ElementData::Window(o) => {
                if let Some(w) = app.doc.element(o.host).and_then(|h| h.as_wall()) {
                    if Some(w.base_level) == level {
                        points.push(w.point_at(o.offset));
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(sk) = app.tool.sketch() {
        segs.extend(sk.lines.iter().map(|l| (l.a, l.b)));
    }
    (segs, points)
}

/// Revit-style object snaps for a plan point: endpoints, midpoints,
/// intersections, perpendicular from the rubber-band start, nearest, and
/// alignment with existing endpoints. `tol` is in model mm.
pub fn snap(app: &HausitApp, p: DVec2, tol: f64) -> Option<Snap> {
    let (segs, extra) = snap_geometry(app);
    let anchor = app.tool.anchor();
    let mut best: Option<(DVec2, SnapKind, f64)> = None;
    let mut consider = |q: DVec2, kind: SnapKind, bias: f64| {
        let d = q.distance(p) * bias;
        if d <= tol && best.map(|b| d < b.2).unwrap_or(true) {
            best = Some((q, kind, d));
        }
    };
    let near: Vec<&(DVec2, DVec2)> = segs.iter().filter(|(a, b)| distance_to_segment(p, *a, *b) < tol * 2.0).collect();
    for (a, b) in &near {
        consider(*a, SnapKind::Endpoint, 0.55);
        consider(*b, SnapKind::Endpoint, 0.55);
        consider((*a + *b) * 0.5, SnapKind::Midpoint, 0.7);
        if let Some(s) = anchor {
            let (q, t) = closest_on_segment(s, *a, *b);
            if t > 0.0 && t < 1.0 && q.distance(s) > 1.0 {
                consider(q, SnapKind::Perpendicular, 0.75);
            }
        }
    }
    for q in &extra {
        consider(*q, SnapKind::Midpoint, 0.7);
    }
    for i in 0..near.len() {
        for j in i + 1..near.len() {
            let ((a, b), (c, d)) = (near[i], near[j]);
            if let Some(x) = line_intersection(*a, *b - *a, *c, *d - *c) {
                if distance_to_segment(x, *a, *b) < 1.0 && distance_to_segment(x, *c, *d) < 1.0 {
                    consider(x, SnapKind::Intersection, 0.6);
                }
            }
        }
    }
    if let Some((q, kind, _)) = best {
        return Some(Snap {
            point: q,
            kind,
            guides: vec![],
        });
    }

    // Alignment: line up with endpoints horizontally or vertically.
    let mut ends: Vec<DVec2> = segs.iter().flat_map(|(a, b)| [*a, *b]).collect();
    ends.extend(anchor);
    let (mut ax, mut ay): (Option<DVec2>, Option<DVec2>) = (None, None);
    for e in &ends {
        if (e.x - p.x).abs() < tol * 0.6 && (e.y - p.y).abs() > tol && ax.map(|a| (a.x - p.x).abs() > (e.x - p.x).abs()).unwrap_or(true) {
            ax = Some(*e);
        }
        if (e.y - p.y).abs() < tol * 0.6 && (e.x - p.x).abs() > tol && ay.map(|a| (a.y - p.y).abs() > (e.y - p.y).abs()).unwrap_or(true) {
            ay = Some(*e);
        }
    }
    let q = DVec2::new(ax.map(|a| a.x).unwrap_or(p.x), ay.map(|a| a.y).unwrap_or(p.y));
    let guides: Vec<(DVec2, DVec2)> = ax.iter().chain(ay.iter()).map(|a| (*a, q)).collect();
    if !guides.is_empty() {
        return Some(Snap {
            point: q,
            kind: if guides.len() == 2 {
                SnapKind::Intersection
            } else {
                SnapKind::Alignment
            },
            guides,
        });
    }

    let nearest = near
        .iter()
        .map(|(a, b)| closest_on_segment(p, *a, *b).0)
        .min_by(|x, y| x.distance(p).total_cmp(&y.distance(p)))
        .filter(|q| q.distance(p) < tol * 0.7)?;
    Some(Snap {
        point: nearest,
        kind: SnapKind::Nearest,
        guides: vec![],
    })
}

/// Revit's angle and length snap increments along the rubber band: angles
/// lock to 15° steps (or always to 90° with Shift), and lengths round to
/// an increment that suits the zoom. `px` is model mm per screen pixel.
pub fn constrain(anchor: DVec2, p: DVec2, px: f64, ortho: bool) -> Option<Snap> {
    let d = p - anchor;
    let len = d.length();
    if len < px * 4.0 {
        return None;
    }
    let ang = d.y.atan2(d.x);
    let step = if ortho { 90f64.to_radians() } else { 15f64.to_radians() };
    let snapped = (ang / step).round() * step;
    let dir = DVec2::new(snapped.cos(), snapped.sin());
    // How far the cursor is from the snapped ray, in pixels.
    if !ortho && (d - dir * d.dot(dir)).length() / px > 7.0 {
        return None;
    }
    let inc = [1.0, 5.0, 10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0]
        .into_iter()
        .find(|i| i / px >= 6.0)
        .unwrap_or(10000.0);
    let l = (d.dot(dir) / inc).round() * inc;
    let q = anchor + dir * l.max(inc);
    Some(Snap {
        point: q,
        kind: SnapKind::Increment,
        guides: vec![(anchor, anchor + dir * (l.max(inc) + px * 60.0))],
    })
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
    // The exterior face is on the left of start→end; Space flips it.
    let (s, e) = if app.options.flip {
        centreline(app, b, a, type_id)
    } else {
        centreline(app, a, b, type_id)
    };
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
    let roof = matches!(tool, Tool::Roof { .. });
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
                        flip_hand: app.options.flip,
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
        Tool::Floor { sketch, .. } | Tool::Roof { sketch } => {
            let (slope, overhang) = if roof { (app.options.defines_slope, 0.0) } else { (false, 0.0) };
            let line = |a: DVec2, b: DVec2| SketchLine {
                a,
                b,
                slope,
                overhang,
                wall: None,
            };
            match app.options.sketch_mode {
                DrawShape::PickWalls => match pick_wall_face(app, p, roof) {
                    Some(l) if sketch.lines.iter().any(|x| x.wall == l.wall) => {
                        app.status = "That wall is already picked.".into();
                        app.tool = tool;
                        return;
                    }
                    Some(l) => sketch.lines.push(l),
                    None => {
                        app.status = "Click a wall to add its face to the sketch.".into();
                        app.tool = tool;
                        return;
                    }
                },
                DrawShape::Rectangle => match sketch.start.take() {
                    None => sketch.start = Some(p),
                    Some(a) if a.distance(p) > 10.0 => {
                        let r = rectangle(a, p);
                        sketch.lines.extend((0..4).map(|i| line(r[i], r[(i + 1) % 4])));
                    }
                    Some(a) => sketch.start = Some(a),
                },
                DrawShape::Line => match sketch.start {
                    None => {
                        sketch.start = Some(p);
                        sketch.chain_from = Some(p);
                    }
                    Some(a) if a.distance(p) > 1.0 => {
                        sketch.lines.push(line(a, p));
                        let closed = sketch.chain_from.map(|c| c.distance(p) < 1.0).unwrap_or(false);
                        sketch.start = if closed || !app.options.chain { None } else { Some(p) };
                    }
                    _ => {}
                },
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

/// The face of the wall under `p` as a sketch line: the exterior face
/// (with overhang) for roofs, the interior face for floors.
pub fn pick_wall_face(app: &HausitApp, p: DVec2, roof: bool) -> Option<SketchLine> {
    let level = app.active_level()?;
    let mut best: Option<(f64, SketchLine)> = None;
    for e in app.doc.elements_of(Category::Walls) {
        let w = e.as_wall()?;
        // Walls on this level, or walls below that end at it (picking
        // the walls under a roof from the roof's level).
        let here = app.doc.level_elevation(level);
        if w.base_level != level && (app.doc.wall_top_z(w) - here).abs() > 1.0 {
            continue;
        }
        let half = app.doc.type_of(e.id).map(|t| t.thickness() / 2.0).unwrap_or(100.0);
        let d = hausit_core::geom::distance_to_segment(p, w.start, w.end);
        if d > half + 150.0 || best.as_ref().map(|b| d >= b.0).unwrap_or(false) {
            continue;
        }
        let o = wall_outline(&app.doc, e.id)?;
        let (a, b) = if roof { (o[0], o[1]) } else { (o[3], o[2]) };
        best = Some((
            d,
            SketchLine {
                a,
                b,
                slope: roof && app.options.defines_slope,
                overhang: if roof { app.options.roof_overhang } else { 0.0 },
                wall: Some(e.id),
            },
        ));
    }
    best.map(|b| b.1)
}

/// Completes a floor or roof sketch ("Finish Edit Mode").
pub fn finish_sketch(app: &mut HausitApp) {
    let Some(level) = app.active_level() else { return };
    let Some(sketch) = app.tool.sketch().cloned() else { return };
    let corners = match sketch.closed_loop() {
        Ok(c) => c,
        Err(e) => {
            app.dialogs.error = Some(e.clone());
            app.status = e;
            return;
        }
    };
    let pts: Vec<DVec2> = corners.iter().map(|c| c.0).collect();
    let tool = std::mem::replace(&mut app.tool, Tool::Modify);
    match tool {
        Tool::Floor { structural, .. } => {
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
        Tool::Roof { .. } => {
            let Some(type_id) = app.type_for(Category::Roofs) else { return };
            let overhangs: Vec<f64> = corners.iter().map(|c| c.1.overhang).collect();
            let footprint = offset_outwards(&pts, &overhangs);
            let roof_level = if app.doc.level(app.options.roof_level).is_some() {
                app.options.roof_level
            } else {
                level
            };
            let r = Roof {
                type_id,
                level: roof_level,
                footprint,
                base_offset: 0.0,
                slope: app.options.roof_slope,
                shape: if app.options.roof_slope > 0.0 {
                    RoofShape::Hip
                } else {
                    RoofShape::Flat
                },
                defines_slope: corners.iter().map(|c| c.1.slope).collect(),
            };
            app.transact("Create Roof", |doc| Ok(doc.add_element(ElementData::Roof(r))));
            app.status = format!("Roof created on {}.", app.doc.level_name(roof_level));
        }
        other => app.tool = other,
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
    let (esc, del, enter, z, y, s, o, n, shift_s, text, now, space) = ctx.input(|i| {
        let cmd = i.modifiers.command;
        let text: String = i
            .events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Text(t) if !cmd && t != " " => Some(t.clone()),
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
            i.key_pressed(egui::Key::Space),
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
        // Inside a sketch, undo removes the last sketch line first.
        let popped = match app.tool.sketch_mut() {
            Some(sk) if sk.start.is_none() && !sk.lines.is_empty() => {
                sk.lines.pop();
                true
            }
            _ => false,
        };
        if !popped {
            app.undo();
        }
    }
    if space && matches!(app.tool, Tool::Wall { .. } | Tool::Door | Tool::Window) {
        // Spacebar flips the wall's exterior side or the door's hand.
        app.options.flip = !app.options.flip;
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
                    sketch: Sketch::default(),
                    structural: false,
                },
            );
            true
        }
        "RF" => {
            start(app, Tool::Roof { sketch: Sketch::default() });
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
