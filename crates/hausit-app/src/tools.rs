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
    Wall {
        start: Option<DVec2>,
        structural: bool,
    },
    Door,
    Window,
    Floor {
        sketch: Sketch,
        structural: bool,
    },
    Roof {
        sketch: Sketch,
    },
    Level,
    Grid {
        start: Option<DVec2>,
    },
    Section {
        start: Option<DVec2>,
    },
    /// Callout: a rectangle in a plan or section that becomes an enlarged
    /// detail view.
    Callout {
        start: Option<DVec2>,
    },
    Move {
        base: Option<DVec2>,
    },
    Copy {
        base: Option<DVec2>,
    },
    Column,
    /// A loadable component (furniture, casework, fixtures, equipment)
    /// of the given category.
    Component {
        category: Category,
    },
    /// Structural framing between two points: a beam, or a brace from
    /// this level up to the level above.
    Beam {
        start: Option<DVec2>,
        brace: bool,
    },
    /// Stair by runs: the points clicked so far along its path.
    Stair {
        pts: Vec<DVec2>,
    },
    /// Railing path being sketched.
    Railing {
        pts: Vec<DVec2>,
    },
    /// Isolated (pad) foundation.
    Footing,
    /// Wall foundation: pick walls.
    WallFoundation,
    /// Automatic Ceiling: click inside walls.
    Ceiling,
    Room,
    RoomSeparation {
        start: Option<DVec2>,
    },
    /// Aligned dimension: references picked so far.
    Dimension {
        refs: Vec<DimRef>,
    },
    /// Rotates the selection about `center`; `from` is the start ray.
    Rotate {
        center: DVec2,
        from: Option<DVec2>,
    },
    /// Mirror - Pick Axis (`draw` false) or Draw Axis.
    Mirror {
        draw: bool,
        start: Option<DVec2>,
    },
    /// Trim/Extend to Corner: the first wall and where it was clicked.
    Trim {
        first: Option<(ElementId, DVec2)>,
    },
    Split,
    /// Attach Top/Base (or Detach) of these walls to a roof or floor.
    Attach {
        walls: Vec<ElementId>,
        detach: bool,
    },
}

impl Tool {
    /// For foundation tools: whether they place wall foundations.
    pub fn wall_foundation(&self) -> Option<bool> {
        match self {
            Tool::Footing => Some(false),
            Tool::WallFoundation => Some(true),
            _ => None,
        }
    }

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
            Tool::Callout { .. } => "Modify | Callout",
            Tool::Move { .. } => "Modify | Move",
            Tool::Copy { .. } => "Modify | Copy",
            Tool::Column => "Modify | Place Structural Column",
            Tool::Component { .. } => "Modify | Place Component",
            Tool::Beam { brace: false, .. } => "Modify | Place Beam",
            Tool::Beam { .. } => "Modify | Place Brace",
            Tool::Footing => "Modify | Place Isolated Foundation",
            Tool::Stair { .. } => "Modify | Create Stair",
            Tool::Ceiling => "Modify | Place Ceiling",
            Tool::Railing { .. } => "Modify | Create Railing Path",
            Tool::WallFoundation => "Modify | Place Wall Foundation",
            Tool::Room => "Modify | Place Room",
            Tool::RoomSeparation { .. } => "Modify | Place Room Separation",
            Tool::Dimension { .. } => "Modify | Place Dimensions",
            Tool::Rotate { .. } => "Modify | Rotate",
            Tool::Mirror { .. } => "Modify | Mirror",
            Tool::Trim { .. } => "Modify | Trim/Extend",
            Tool::Split => "Modify | Split Element",
            Tool::Attach { detach: false, .. } => "Modify | Walls > Attach Top/Base",
            Tool::Attach { .. } => "Modify | Walls > Detach Top/Base",
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
            Tool::Callout { start: None } => "Click the first corner of the callout. It becomes a detail view at 1:20, Fine detail.",
            Tool::Callout { .. } => "Click the opposite corner of the callout.",
            Tool::Move { base: None } | Tool::Copy { base: None } => "Click to enter move start point.",
            Tool::Move { .. } | Tool::Copy { .. } => "Click to enter move end point.",
            Tool::Column => "Click to place a structural column. Space rotates it 90°. At Grids places one at every grid intersection.",
            Tool::Component { .. } => "Click to place the component. Space rotates it 90°. Pick another type in Properties.",
            Tool::Beam { start: None, brace: false } => "Click to enter beam start point.",
            Tool::Beam { start: None, .. } => "Click to enter brace start point (on this level).",
            Tool::Beam { brace: false, .. } => "Click to enter beam end point. Esc ends the chain.",
            Tool::Beam { .. } => "Click to enter brace end point (on the level above).",
            Tool::Footing => "Click to place an isolated foundation. Space rotates it 90°. At Columns places one under every column.",
            Tool::WallFoundation => "Click walls to add a wall foundation under them.",
            Tool::Ceiling => "Click inside a room's walls to create a ceiling there (Automatic Ceiling).",
            Tool::Stair { pts } if pts.is_empty() => "Click to start the run at the first riser.",
            Tool::Stair { .. } => "Click the end of the run, or a point to turn at a landing. Finish (Enter) completes the last run.",
            Tool::Railing { pts } if pts.is_empty() => "Click to start the railing path.",
            Tool::Railing { .. } => "Click the next point. Finish (Enter) creates the railing.",
            Tool::Room => "Click inside an area enclosed by walls or room separation lines to place a room.",
            Tool::RoomSeparation { start: None } => "Click to enter room separation line start point.",
            Tool::RoomSeparation { .. } => "Click to enter line end point. Esc ends the chain.",
            Tool::Dimension { refs } if refs.len() < 2 => "Click wall faces, wall centerlines or grids to dimension.",
            Tool::Dimension { .. } => "Click more parallel references, or click an empty spot to place the dimension.",
            Tool::Rotate { from: None, .. } => "Click to enter the start ray, or type an angle and press Enter.",
            Tool::Rotate { .. } => "Click to enter the end ray, or type an angle and press Enter.",
            Tool::Mirror { draw: false, .. } => "Pick a wall or grid to mirror about.",
            Tool::Mirror { start: None, .. } => "Click to enter the start of the mirror axis.",
            Tool::Mirror { .. } => "Click to enter the end of the mirror axis.",
            Tool::Trim { first: None } => "Click the first wall, on the part you want to keep.",
            Tool::Trim { .. } => "Click the second wall, on the part you want to keep.",
            Tool::Split => "Click a wall where you want to split it.",
            Tool::Attach { .. } => "Pick a roof or floor to attach the selected walls to.",
        }
    }

    pub fn category(&self) -> Option<Category> {
        match self {
            Tool::Wall { .. } => Some(Category::Walls),
            Tool::Door => Some(Category::Doors),
            Tool::Window => Some(Category::Windows),
            Tool::Floor { .. } => Some(Category::Floors),
            Tool::Roof { .. } => Some(Category::Roofs),
            Tool::Column => Some(Category::StructuralColumns),
            Tool::Component { category } => Some(*category),
            Tool::Beam { .. } => Some(Category::StructuralFraming),
            Tool::Footing | Tool::WallFoundation => Some(Category::StructuralFoundations),
            Tool::Stair { .. } => Some(Category::Stairs),
            Tool::Ceiling => Some(Category::Ceilings),
            Tool::Railing { .. } => Some(Category::Railings),
            _ => None,
        }
    }

    /// Tools that work by clicking existing elements: the canvas
    /// pre-highlights the ones of these categories under the cursor.
    pub fn picks(&self, cat: Category) -> bool {
        match self {
            Tool::Modify => true,
            Tool::Mirror { draw: false, .. } => matches!(cat, Category::Walls | Category::Grids | Category::RoomSeparation),
            Tool::Trim { .. } | Tool::Split | Tool::WallFoundation => cat == Category::Walls,
            Tool::Attach { .. } => matches!(cat, Category::Roofs | Category::Floors),
            _ => false,
        }
    }

    /// The last fixed point, used for rubber-band lines and typed lengths.
    pub fn anchor(&self) -> Option<DVec2> {
        match self {
            Tool::Wall { start, .. }
            | Tool::Grid { start }
            | Tool::Section { start }
            | Tool::Callout { start }
            | Tool::RoomSeparation { start }
            | Tool::Beam { start, .. }
            | Tool::Mirror { start, .. } => *start,
            Tool::Move { base } | Tool::Copy { base } => *base,
            Tool::Rotate { center, .. } => Some(*center),
            Tool::Floor { sketch, .. } | Tool::Roof { sketch } => sketch.start,
            Tool::Stair { pts } | Tool::Railing { pts } => pts.last().copied(),
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
            Tool::Wall { start, .. }
            | Tool::Grid { start }
            | Tool::Section { start }
            | Tool::Callout { start }
            | Tool::RoomSeparation { start }
            | Tool::Beam { start, .. }
            | Tool::Mirror { start, .. } => *start = None,
            Tool::Stair { pts } | Tool::Railing { pts } => pts.clear(),
            Tool::Move { base } | Tool::Copy { base } => *base = None,
            Tool::Rotate { from, .. } => *from = None,
            Tool::Dimension { refs } => refs.clear(),
            Tool::Trim { first } => *first = None,
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
        let in_progress = match self {
            Tool::Rotate { from, .. } => from.is_some(),
            Tool::Dimension { refs } => !refs.is_empty(),
            Tool::Trim { first } => first.is_some(),
            _ => self.anchor().is_some(),
        };
        if in_progress {
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
    /// Spacebar flip while placing walls, doors and windows (and a 90°
    /// turn for columns).
    pub flip: bool,
    /// Mirror and Rotate: "Copy" on the options bar.
    pub copy: bool,
    /// Picking tolerance at the current zoom, model mm.
    pub pick_tol: f64,
    /// Ceiling height above the level.
    pub ceiling_height: f64,
    /// Rotation of the component being placed, degrees.
    pub component_rotation: f64,
    /// Category of the component type used last.
    pub last_component: Option<Category>,
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
            copy: false,
            pick_tol: 200.0,
            ceiling_height: 2600.0,
            component_rotation: 0.0,
            last_component: None,
        }
    }
}

// ----- starting tools ---------------------------------------------------------

pub fn start(app: &mut HausitApp, mut tool: Tool) {
    let needs_plan = matches!(
        tool,
        Tool::Wall { .. }
            | Tool::Door
            | Tool::Window
            | Tool::Floor { .. }
            | Tool::Roof { .. }
            | Tool::Grid { .. }
            | Tool::Section { .. }
            | Tool::Column
            | Tool::Component { .. }
            | Tool::Beam { .. }
            | Tool::Footing
            | Tool::WallFoundation
            | Tool::Stair { .. }
            | Tool::Railing { .. }
            | Tool::Ceiling
            | Tool::Room
            | Tool::RoomSeparation { .. }
            | Tool::Dimension { .. }
            | Tool::Rotate { .. }
            | Tool::Mirror { .. }
            | Tool::Trim { .. }
            | Tool::Split
    );
    let needs_selection = matches!(
        tool,
        Tool::Move { .. } | Tool::Copy { .. } | Tool::Rotate { .. } | Tool::Mirror { .. } | Tool::Attach { .. }
    );
    if matches!(tool, Tool::Callout { .. }) {
        let ok = app
            .active_view
            .and_then(|v| app.doc.view(v))
            .is_some_and(|v| v.kind.is_graphical() && !matches!(v.kind, ViewKind::ThreeD));
        if !ok {
            app.status = "Callouts are drawn in a plan, section or elevation.".into();
            return;
        }
    }
    if needs_plan && app.active_level().is_none() {
        app.status = "This tool works in a floor plan. Open a floor plan from the Project Browser.".into();
        return;
    }
    if needs_selection && app.selection.iter().all(|id| app.doc.element(*id).is_none()) {
        app.status = "Select elements first, then choose the modify tool.".into();
        return;
    }
    match &mut tool {
        // Components: the category of the type last used, or the first
        // category with loaded types; with none loaded, offer the library.
        Tool::Component { category } => {
            let has = |app: &HausitApp, c: Category| app.doc.types_of(c).next().is_some();
            if let Some(c) = app.options.last_component.filter(|c| has(app, *c)) {
                *category = c;
            }
            if !has(app, *category) {
                match Category::COMPONENTS.into_iter().find(|c| has(app, *c)) {
                    Some(c) => *category = c,
                    None => {
                        crate::families::open_library(app);
                        app.status = "Load a family first: pick one from the library.".into();
                        return;
                    }
                }
            }
        }
        Tool::Rotate { center, .. } => match selection_center(app) {
            Some(c) => *center = c,
            None => return,
        },
        Tool::Attach { walls, .. } => {
            *walls = app
                .selection
                .iter()
                .copied()
                .filter(|id| app.doc.element(*id).and_then(|e| e.as_wall()).is_some())
                .collect();
            if walls.is_empty() {
                app.status = "Select walls first.".into();
                return;
            }
        }
        _ => {}
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
        attached_top: vec![],
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
        Tool::Callout { start } => match *start {
            None => *start = Some(p),
            Some(a) if (p - a).abs().min_element() > 10.0 => {
                let Some(parent) = app.active_view else { return };
                let id = app.transact("Create Callout", |doc| Ok(doc.add_callout(parent, a, p)));
                *start = None;
                if let Some(Some(id)) = id {
                    app.status = format!(
                        "Created {}. Double-click its bubble to open it.",
                        app.doc.view(id).map(|v| v.name.clone()).unwrap_or_default()
                    );
                }
            }
            _ => {}
        },
        Tool::Component { category } => {
            let Some(level) = app.active_level() else { return };
            let category = *category;
            let Some(type_id) = app.type_for(category) else { return };
            app.options.last_component = Some(category);
            let c = Component {
                type_id,
                category,
                level,
                point: p,
                rotation: app.options.component_rotation,
                offset: 0.0,
            };
            app.transact("Place Component", |doc| Ok(doc.add_element(ElementData::Component(c))));
        }
        Tool::Column => {
            let Some(level) = app.active_level() else { return };
            let Some(type_id) = app.type_for(Category::StructuralColumns) else {
                return;
            };
            let c = new_column(app, type_id, level, p);
            app.transact("Create Structural Column", |doc| Ok(doc.add_element(ElementData::Column(c))));
        }
        Tool::Beam { start, brace } => match *start {
            None => *start = Some(p),
            Some(a) if a.distance(p) > 10.0 => {
                let Some(level) = app.active_level() else { return };
                let Some(type_id) = app.type_for(Category::StructuralFraming) else {
                    return;
                };
                let rise = app
                    .doc
                    .level_above(level)
                    .map(|l| app.doc.level_elevation(l) - app.doc.level_elevation(level))
                    .unwrap_or(3000.0);
                let f = Framing {
                    type_id,
                    kind: if *brace { FramingKind::Brace } else { FramingKind::Beam },
                    level,
                    start: a,
                    end: p,
                    start_offset: 0.0,
                    end_offset: if *brace { rise } else { 0.0 },
                };
                let what = if *brace { "Create Brace" } else { "Create Beam" };
                app.transact(what, |doc| Ok(doc.add_element(ElementData::Framing(f))));
                *start = if app.options.chain && !*brace { Some(p) } else { None };
            }
            _ => {}
        },
        Tool::Stair { pts } => {
            if pts.last().is_some_and(|q| q.distance(p) < 10.0) {
                app.tool = tool;
                return;
            }
            pts.push(p);
            // Done once the runs hold every riser.
            if pts.len() >= 2 {
                if let Some(l) = stair_preview(app, pts) {
                    if l.placed >= l.risers {
                        let path = std::mem::take(pts);
                        app.tool = tool;
                        create_stair(app, path);
                        return;
                    }
                }
            }
        }
        Tool::Railing { pts } => {
            if !pts.last().is_some_and(|q| q.distance(p) < 10.0) {
                pts.push(p);
            }
        }
        Tool::Ceiling => match ceiling_region(app, p) {
            Some(boundary) => {
                let Some(level) = app.active_level() else { return };
                let Some(type_id) = app.type_for(Category::Ceilings) else { return };
                let c = Ceiling {
                    type_id,
                    level,
                    boundary,
                    height_offset: app.options.ceiling_height,
                };
                app.transact("Create Ceiling", |doc| Ok(doc.add_element(ElementData::Ceiling(c))));
            }
            None => app.status = "Click inside an area enclosed by walls.".into(),
        },
        Tool::Footing => {
            let Some(level) = app.active_level() else { return };
            let Some(type_id) = app.type_for(Category::StructuralFoundations) else {
                return;
            };
            let f = new_footing(app, type_id, level, p);
            app.transact("Create Isolated Foundation", |doc| Ok(doc.add_element(ElementData::Footing(f))));
        }
        Tool::WallFoundation => {
            let Some(wall) = pick_wall(app, p) else {
                app.status = "Click a wall.".into();
                app.tool = tool;
                return;
            };
            let has = app
                .doc
                .elements_of(Category::StructuralFoundations)
                .any(|e| matches!(&e.data, ElementData::WallFooting(f) if f.wall == wall));
            if has {
                app.status = "That wall already has a foundation.".into();
            } else if let Some(type_id) = app.type_for(Category::StructuralFoundations) {
                app.transact("Create Wall Foundation", |doc| {
                    Ok(doc.add_element(ElementData::WallFooting(WallFooting { type_id, wall })))
                });
            }
        }
        Tool::Room => {
            if let Some(id) = place_room(app, p) {
                app.status = format!("Placed {}.", app.doc.describe(id));
                app.tool = tool;
                return;
            }
        }
        Tool::RoomSeparation { start } => match *start {
            None => *start = Some(p),
            Some(a) if a.distance(p) > 10.0 => {
                let Some(level) = app.active_level() else { return };
                let l = SketchedLine { level, start: a, end: p };
                app.transact("Create Room Separation", |doc| Ok(doc.add_element(ElementData::RoomSeparation(l))));
                *start = if app.options.chain { Some(p) } else { None };
            }
            _ => {}
        },
        Tool::Dimension { refs } => match pick_dim_ref(app, p, app.options.pick_tol) {
            Some((r, _)) if refs.contains(&r) => {}
            Some((r, _))
                if refs
                    .first()
                    .map(|f| hausit_model::dimensions::parallel(&app.doc, f, &r))
                    .unwrap_or(true) =>
            {
                refs.push(r)
            }
            Some(_) => {
                app.status = "References must be parallel to the first one.".into();
                app.tool = tool;
                return;
            }
            None if refs.len() >= 2 => {
                if let Some(view) = app.active_view {
                    let d = Dimension {
                        view,
                        refs: std::mem::take(refs),
                        line_point: p,
                    };
                    app.transact("Create Dimension", |doc| Ok(doc.add_element(ElementData::Dimension(d))));
                }
            }
            None => {}
        },
        Tool::Rotate { center, from } => match *from {
            None if p.distance(*center) > 1.0 => *from = Some(p),
            None => {}
            Some(f) => {
                let (u, v) = (f - *center, p - *center);
                let angle = (u.x * v.y - u.y * v.x).atan2(u.dot(v)).to_degrees();
                rotate_selection(app, *center, angle);
                return;
            }
        },
        Tool::Mirror { draw: false, .. } => {
            let axis = app.hover.and_then(|h| app.doc.element(h)).and_then(|e| match &e.data {
                ElementData::Wall(w) => Some((w.start, w.end)),
                ElementData::Grid(g) => Some((g.start, g.end)),
                ElementData::RoomSeparation(l) => Some((l.start, l.end)),
                _ => None,
            });
            match axis {
                Some((a, b)) => {
                    mirror_selection(app, a, b);
                    return;
                }
                None => app.status = "Pick a wall, grid or line to use as the mirror axis.".into(),
            }
        }
        Tool::Mirror { start, .. } => match *start {
            None => *start = Some(p),
            Some(a) if a.distance(p) > 1.0 => {
                mirror_selection(app, a, p);
                return;
            }
            _ => {}
        },
        Tool::Trim { first } => {
            let wall = app.hover.filter(|h| app.doc.element(*h).and_then(|e| e.as_wall()).is_some());
            match (wall, *first) {
                (None, _) => app.status = "Click a wall.".into(),
                (Some(w), None) => *first = Some((w, p)),
                (Some(w), Some((w1, _))) if w == w1 => {}
                (Some(w), Some((w1, p1))) => {
                    trim_to_corner(app, (w1, p1), (w, p));
                    *first = None;
                    app.tool = tool;
                    return;
                }
            }
        }
        Tool::Split => {
            if let Some(w) = app.hover.filter(|h| app.doc.element(*h).and_then(|e| e.as_wall()).is_some()) {
                split_wall(app, w, p);
                app.tool = tool;
                return;
            }
        }
        Tool::Attach { walls, detach } => {
            let target = app
                .hover
                .filter(|h| matches!(app.doc.element(*h).map(|e| e.category()), Some(Category::Roofs | Category::Floors)));
            let Some(target) = target else {
                app.status = "Pick a roof or floor.".into();
                app.tool = tool;
                return;
            };
            let (walls, detach) = (walls.clone(), *detach);
            app.transact(if detach { "Detach Top/Base" } else { "Attach Top/Base" }, |doc| {
                for w in &walls {
                    if let Some(ElementData::Wall(wall)) = doc.element_mut(*w).map(|e| &mut e.data) {
                        wall.attached_top.retain(|t| *t != target);
                        if !detach {
                            wall.attached_top.push(target);
                        }
                    }
                }
                Ok(())
            });
            app.selection = walls;
            app.tool = Tool::Modify;
            app.status = "Ready".into();
            return;
        }
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

/// The stair the Stair tool would create along `path`.
pub fn stair_preview(app: &mut HausitApp, path: &[DVec2]) -> Option<hausit_model::stairs::StairLayout> {
    use hausit_model::stairs::{layout, StairRules};
    let level = app.active_level()?;
    let top = app.doc.level_above(level)?;
    let type_id = app.type_for(Category::Stairs)?;
    let t = app.doc.family_type(type_id);
    let width = t.map(|t| t.length_or("Minimum Run Width", 1000.0)).unwrap_or(1000.0);
    if path.len() < 2 {
        return None;
    }
    let (z0, z1) = (app.doc.level_elevation(level), app.doc.level_elevation(top));
    Some(layout(StairRules::of(t), z0, z1, width, path))
}

/// Creates a stair along `path` with a railing on each side, as Revit's
/// Stair tool does by default.
fn create_stair(app: &mut HausitApp, path: Vec<DVec2>) {
    let Some(level) = app.active_level() else { return };
    let Some(top) = app.doc.level_above(level) else {
        app.status = "A stair needs a level above this one.".into();
        return;
    };
    let Some(type_id) = app.type_for(Category::Stairs) else { return };
    let rail_type = app.type_for(Category::Railings);
    let width = app
        .doc
        .family_type(type_id)
        .map(|t| t.length_or("Minimum Run Width", 1000.0))
        .unwrap_or(1000.0);
    let Some(l) = stair_preview(app, &path) else { return };
    let stair = Stair {
        type_id,
        base_level: level,
        base_offset: 0.0,
        top_level: top,
        top_offset: 0.0,
        path: l.path.clone(),
        width,
    };
    let risers = l.risers;
    app.transact("Create Stair", |doc| {
        let id = doc.add_element(ElementData::Stair(stair));
        if let Some(rt) = rail_type {
            for path in hausit_model::stairs::stair_railing_paths(&l, width, 50.0) {
                doc.add_element(ElementData::Railing(Railing {
                    type_id: rt,
                    level,
                    base_offset: 0.0,
                    path,
                    host: id,
                }));
            }
        }
        Ok(id)
    });
    app.status = format!("Created a stair with {risers} risers and its railings.");
}

/// Finish for the Stair and Railing tools.
pub fn finish_path_tool(app: &mut HausitApp) {
    match &mut app.tool {
        Tool::Stair { pts } if pts.len() >= 2 => {
            let path = std::mem::take(pts);
            create_stair(app, path);
        }
        Tool::Railing { pts } if pts.len() >= 2 => {
            let path = std::mem::take(pts);
            let Some(level) = app.active_level() else { return };
            let Some(type_id) = app.type_for(Category::Railings) else { return };
            app.transact("Create Railing", |doc| {
                Ok(doc.add_element(ElementData::Railing(Railing {
                    type_id,
                    level,
                    base_offset: 0.0,
                    path,
                    host: ElementId::INVALID,
                })))
            });
        }
        _ => app.status = "Click at least two points first.".into(),
    }
}

fn new_footing(app: &HausitApp, type_id: ElementId, level: ElementId, p: DVec2) -> Footing {
    Footing {
        type_id,
        level,
        point: p,
        rotation: if app.options.flip { 90.0 } else { 0.0 },
        offset: 0.0,
    }
}

/// "At Columns": an isolated foundation under every column standing on
/// the active level that does not have one yet.
pub fn footings_at_columns(app: &mut HausitApp) {
    let Some(level) = app.active_level() else { return };
    let Some(type_id) = app.type_for(Category::StructuralFoundations) else {
        return;
    };
    let existing: Vec<DVec2> = app
        .doc
        .elements_of(Category::StructuralFoundations)
        .filter_map(|e| match &e.data {
            ElementData::Footing(f) if f.level == level => Some(f.point),
            _ => None,
        })
        .collect();
    let feet: Vec<Footing> = app
        .doc
        .elements_of(Category::StructuralColumns)
        .filter_map(|e| match &e.data {
            ElementData::Column(c) if c.base_level == level && !existing.iter().any(|q| q.distance(c.point) < 1.0) => Some(Footing {
                type_id,
                level,
                point: c.point,
                rotation: c.rotation,
                offset: c.base_offset,
            }),
            _ => None,
        })
        .collect();
    if feet.is_empty() {
        app.status = "Every column on this level already has a foundation.".into();
        return;
    }
    let n = feet.len();
    app.transact("Create Isolated Foundations at Columns", |doc| {
        for f in feet {
            doc.add_element(ElementData::Footing(f));
        }
        Ok(())
    });
    app.status = format!("Placed {n} isolated foundations under columns.");
}

/// The wall under the cursor on the active level.
fn pick_wall(app: &HausitApp, p: DVec2) -> Option<ElementId> {
    let level = app.active_level()?;
    app.doc
        .elements_of(Category::Walls)
        .filter_map(|e| {
            let w = e.as_wall()?;
            let t = app.doc.type_of(e.id).map(|t| t.thickness()).unwrap_or(200.0);
            let d = distance_to_segment(p, w.start, w.end);
            (w.base_level == level && d <= t / 2.0 + app.options.pick_tol).then_some((e.id, d))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

fn new_column(app: &HausitApp, type_id: ElementId, level: ElementId, p: DVec2) -> Column {
    Column {
        type_id,
        point: p,
        rotation: if app.options.flip { 90.0 } else { 0.0 },
        base_level: level,
        base_offset: 0.0,
        top_level: app.doc.level_above(level).unwrap_or(ElementId::INVALID),
        top_offset: 0.0,
    }
}

/// "At Grids": a column at every grid intersection on the active level
/// that does not have one yet.
pub fn columns_at_grids(app: &mut HausitApp) {
    let Some(level) = app.active_level() else { return };
    let Some(type_id) = app.type_for(Category::StructuralColumns) else {
        return;
    };
    let grids: Vec<(DVec2, DVec2)> = app
        .doc
        .elements_of(Category::Grids)
        .filter_map(|e| match &e.data {
            ElementData::Grid(g) => Some((g.start, g.end)),
            _ => None,
        })
        .collect();
    let existing: Vec<DVec2> = app
        .doc
        .elements_of(Category::StructuralColumns)
        .filter_map(|e| match &e.data {
            ElementData::Column(c) if c.base_level == level => Some(c.point),
            _ => None,
        })
        .collect();
    let mut pts: Vec<DVec2> = Vec::new();
    for i in 0..grids.len() {
        for j in i + 1..grids.len() {
            let ((a, b), (c, d)) = (grids[i], grids[j]);
            let Some(x) = line_intersection(a, b - a, c, d - c) else { continue };
            if distance_to_segment(x, a, b) < 1.0
                && distance_to_segment(x, c, d) < 1.0
                && !pts.iter().chain(&existing).any(|q| q.distance(x) < 1.0)
            {
                pts.push(x);
            }
        }
    }
    if pts.is_empty() {
        app.status = "Every grid intersection already has a column.".into();
        return;
    }
    let cols: Vec<Column> = pts.iter().map(|p| new_column(app, type_id, level, *p)).collect();
    let n = cols.len();
    app.transact("Create Structural Columns at Grids", |doc| {
        for c in cols {
            doc.add_element(ElementData::Column(c));
        }
        Ok(())
    });
    app.status = format!("Placed {n} structural columns at grid intersections.");
}

/// Places a room in the enclosed region at `p`, as Revit's Room tool.
/// The smallest region bounded by walls (and room separation lines)
/// around `p`: where Automatic Ceiling places a ceiling.
pub fn ceiling_region(app: &HausitApp, p: DVec2) -> Option<Vec<DVec2>> {
    let level = app.active_level()?;
    let segs = hausit_model::rooms::bounding_segments(&app.doc, level);
    hausit_model::rooms::regions(&segs)
        .into_iter()
        .filter(|f| hausit_core::geom::point_in_polygon(p, f))
        .min_by(|a, b| hausit_core::geom::polygon_area(a).total_cmp(&hausit_core::geom::polygon_area(b)))
}

fn place_room(app: &mut HausitApp, p: DVec2) -> Option<ElementId> {
    let level = app.active_level()?;
    if !hausit_model::rooms::rooms_at(&app.doc, level, p).is_empty() {
        app.status = "Room is redundant: there is already a room in this enclosed region.".into();
        return None;
    }
    let (upper_limit, limit_offset) = match app.doc.level_above(level) {
        Some(l) => (l, 0.0),
        None => (level, 2500.0),
    };
    let room = Room {
        level,
        point: p,
        name: "Room".into(),
        number: app.doc.next_room_number(),
        upper_limit,
        limit_offset,
    };
    let enclosed = hausit_model::rooms::room_boundary(&app.doc, &room).is_some();
    let id = app.transact("Create Room", |doc| Ok(doc.add_element(ElementData::Room(room))))?;
    if !enclosed {
        app.dialogs.error = Some("Room is not in a properly enclosed region. Add walls or room separation lines around it.".into());
    }
    Some(id)
}

/// The dimension reference under `p`: the nearest wall face or
/// centreline, or a grid. Returns the reference and its line.
pub fn pick_dim_ref(app: &HausitApp, p: DVec2, tol: f64) -> Option<(DimRef, (DVec2, DVec2))> {
    let level = app.active_level()?;
    let mut best: Option<(f64, DimRef, (DVec2, DVec2))> = None;
    let mut consider = |r: DimRef, a: DVec2, b: DVec2| {
        let d = distance_to_segment(p, a, b);
        if d <= tol && best.as_ref().map(|x| d < x.0).unwrap_or(true) {
            best = Some((d, r, (a, b)));
        }
    };
    for e in app.doc.elements() {
        match &e.data {
            ElementData::Wall(w) if w.base_level == level => {
                let Some(o) = wall_outline(&app.doc, e.id) else { continue };
                consider(DimRef::Wall { wall: e.id, side: 1 }, o[0], o[1]);
                consider(DimRef::Wall { wall: e.id, side: 0 }, w.start, w.end);
                consider(DimRef::Wall { wall: e.id, side: -1 }, o[3], o[2]);
            }
            ElementData::Grid(g) => consider(DimRef::Grid { grid: e.id }, g.start, g.end),
            _ => {}
        }
    }
    best.map(|b| (b.1, b.2))
}

/// Bounding-box centre of the selection in plan (Rotate's default centre).
pub fn selection_center(app: &HausitApp) -> Option<DVec2> {
    let mut pts: Vec<DVec2> = Vec::new();
    for id in &app.selection {
        let Some(e) = app.doc.element(*id) else { continue };
        match &e.data {
            ElementData::Wall(w) => pts.extend([w.start, w.end]),
            ElementData::Grid(g) => pts.extend([g.start, g.end]),
            ElementData::Floor(f) => pts.extend(&f.boundary),
            ElementData::Roof(r) => pts.extend(&r.footprint),
            ElementData::Column(c) => pts.push(c.point),
            ElementData::Framing(f) => pts.extend([f.start, f.end]),
            ElementData::Footing(f) => pts.push(f.point),
            ElementData::Component(c) => pts.push(c.point),
            ElementData::Stair(s) => pts.extend(&s.path),
            ElementData::Ceiling(c) => pts.extend(&c.boundary),
            ElementData::Railing(r) => pts.extend(&r.path),
            ElementData::WallFooting(f) => {
                if let Some(w) = app.doc.element(f.wall).and_then(|h| h.as_wall()) {
                    pts.extend([w.start, w.end]);
                }
            }
            ElementData::Room(r) => pts.push(r.point),
            ElementData::RoomSeparation(l) => pts.extend([l.start, l.end]),
            ElementData::Door(o) | ElementData::Window(o) => {
                if let Some(w) = app.doc.element(o.host).and_then(|h| h.as_wall()) {
                    pts.push(w.point_at(o.offset));
                }
            }
            ElementData::Level(_) | ElementData::Dimension(_) => {}
        }
    }
    let first = *pts.first()?;
    let (min, max) = pts.iter().fold((first, first), |(a, b), p| (a.min(*p), b.max(*p)));
    Some((min + max) * 0.5)
}

/// Elements Rotate and Mirror act on: openings follow their host walls.
fn transformable(app: &HausitApp) -> Vec<ElementId> {
    app.selection
        .iter()
        .copied()
        .filter(|id| {
            app.doc
                .element(*id)
                .map(|e| !e.category().is_hosted() && e.category() != Category::Levels)
                .unwrap_or(false)
        })
        .collect()
}

fn rotate_selection(app: &mut HausitApp, center: DVec2, angle: f64) {
    let ids = transformable(app);
    let copy = app.options.copy;
    let new = app.transact("Rotate", |doc| {
        let ids = if copy { doc.copy(&ids, DVec2::ZERO) } else { ids };
        doc.rotate(&ids, center, angle);
        Ok(ids)
    });
    if let Some(new) = new {
        app.selection = new;
    }
    app.tool = Tool::Modify;
    app.status = format!("Rotated {:.2}°.", angle);
}

fn mirror_selection(app: &mut HausitApp, a: DVec2, b: DVec2) {
    let ids = transformable(app);
    let copy = app.options.copy;
    let new = app.transact("Mirror", |doc| {
        let ids = if copy { doc.copy(&ids, DVec2::ZERO) } else { ids };
        doc.mirror(&ids, a, b);
        Ok(ids)
    });
    if let Some(new) = new {
        app.selection = new;
    }
    app.tool = Tool::Modify;
    app.status = "Ready".into();
}

/// Trim/Extend to Corner: both walls end at the intersection of their
/// lines, keeping the side of each wall that was clicked.
fn trim_to_corner(app: &mut HausitApp, (w1, p1): (ElementId, DVec2), (w2, p2): (ElementId, DVec2)) {
    let get = |id: ElementId| app.doc.element(id).and_then(|e| e.as_wall()).cloned();
    let (Some(a), Some(b)) = (get(w1), get(w2)) else { return };
    let Some(x) = line_intersection(a.start, a.end - a.start, b.start, b.end - b.start) else {
        app.status = "The walls are parallel and cannot form a corner.".into();
        return;
    };
    let cut = |w: &Wall, click: DVec2| {
        let d = w.end - w.start;
        let t = |q: DVec2| (q - w.start).dot(d) / d.length_squared();
        // Keep the part of the wall on the clicked side of the corner.
        if t(click) < t(x) {
            (w.start, x)
        } else {
            (x, w.end)
        }
    };
    let (na, nb) = (cut(&a, p1), cut(&b, p2));
    if na.0.distance(na.1) < 10.0 || nb.0.distance(nb.1) < 10.0 {
        app.status = "That would make a wall too short.".into();
        return;
    }
    app.transact("Trim/Extend to Corner", |doc| {
        for (id, (s, e)) in [(w1, na), (w2, nb)] {
            if let Some(ElementData::Wall(w)) = doc.element_mut(id).map(|e| &mut e.data) {
                w.start = s;
                w.end = e;
            }
        }
        Ok(())
    });
}

/// Split Element: two walls meeting at the clicked point. Openings past
/// the split move to the new wall.
fn split_wall(app: &mut HausitApp, id: ElementId, p: DVec2) {
    let Some(w) = app.doc.element(id).and_then(|e| e.as_wall()).cloned() else {
        return;
    };
    let (q, t) = closest_on_segment(p, w.start, w.end);
    let at = t * w.length();
    if at < 10.0 || w.length() - at < 10.0 {
        app.status = "Click away from the wall's ends to split it.".into();
        return;
    }
    let moved: Vec<(ElementId, f64)> = app
        .doc
        .openings_in(id)
        .filter(|(_, o)| o.offset > at)
        .map(|(e, o)| (e.id, o.offset - at))
        .collect();
    app.transact("Split Wall", |doc| {
        let mut second = w.clone();
        second.start = q;
        let new = doc.add_element(ElementData::Wall(second));
        if let Some(ElementData::Wall(first)) = doc.element_mut(id).map(|e| &mut e.data) {
            first.end = q;
        }
        for (o, off) in moved {
            if let Some(ElementData::Door(h) | ElementData::Window(h)) = doc.element_mut(o).map(|e| &mut e.data) {
                h.host = new;
                h.offset = off;
            }
        }
        Ok(())
    });
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
            let roof_z = app.doc.level_elevation(roof_level);
            let footprint = r.footprint.clone();
            let Some(roof) = app.transact("Create Roof", |doc| Ok(doc.add_element(ElementData::Roof(r)))) else {
                return;
            };
            app.status = format!("Roof created on {}.", app.doc.level_name(roof_level));
            // As in Revit: offer to attach the walls under the roof, so
            // gable ends and slopes close up against it.
            let picked: Vec<ElementId> = corners.iter().filter_map(|c| c.1.wall).collect();
            let walls: Vec<ElementId> = app
                .doc
                .elements_of(Category::Walls)
                .filter(|e| {
                    let w = e.as_wall().unwrap();
                    // Picked walls; with drawn lines, the walls topping
                    // out under the roof.
                    picked.contains(&e.id)
                        || (picked.is_empty()
                            && (app.doc.wall_top_z(w) - roof_z).abs() < 1.0
                            && hausit_core::geom::point_in_polygon((w.start + w.end) * 0.5, &footprint))
                })
                .filter(|e| !e.as_wall().unwrap().attached_top.contains(&roof))
                .map(|e| e.id)
                .collect();
            if !walls.is_empty() {
                app.selection = walls.clone();
                app.dialogs.attach_walls = Some((roof, walls));
            }
        }
        other => app.tool = other,
    }
}

/// Applies a typed length along the current rubber-band direction.
pub fn apply_typed_length(app: &mut HausitApp, cursor: DVec2) {
    if let Tool::Rotate { center, .. } = app.tool {
        // Rotate takes a typed angle in degrees, counter-clockwise.
        let angle = app.options.typed.trim().parse::<f64>();
        app.options.typed.clear();
        if let Ok(a) = angle {
            rotate_selection(app, center, a);
        }
        return;
    }
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
    if space
        && matches!(
            app.tool,
            Tool::Wall { .. } | Tool::Door | Tool::Window | Tool::Column | Tool::Footing
        )
    {
        // Spacebar flips the wall's exterior side or the door's hand.
        app.options.flip = !app.options.flip;
    }
    if space && matches!(app.tool, Tool::Component { .. }) {
        app.options.component_rotation = (app.options.component_rotation + 90.0) % 360.0;
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
    if enter && matches!(app.tool, Tool::Stair { .. } | Tool::Railing { .. }) && app.options.typed.is_empty() {
        finish_path_tool(app);
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
        "BX" => {
            app.selection_box();
            true
        }
        "RM" => {
            start(app, Tool::Room);
            true
        }
        "DI" => {
            start(app, Tool::Dimension { refs: vec![] });
            true
        }
        "CL" => {
            start(app, Tool::Column);
            true
        }
        "CM" => {
            start(
                app,
                Tool::Component {
                    category: Category::Furniture,
                },
            );
            true
        }
        "BM" => {
            start(app, Tool::Beam { start: None, brace: false });
            true
        }
        "BR" => {
            start(app, Tool::Beam { start: None, brace: true });
            true
        }
        "FT" => {
            start(app, Tool::WallFoundation);
            true
        }
        "RO" => {
            start(
                app,
                Tool::Rotate {
                    center: DVec2::ZERO,
                    from: None,
                },
            );
            true
        }
        "MM" | "DM" => {
            start(
                app,
                Tool::Mirror {
                    draw: cmd == "DM",
                    start: None,
                },
            );
            true
        }
        "TR" => {
            start(app, Tool::Trim { first: None });
            true
        }
        "SL" => {
            start(app, Tool::Split);
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
