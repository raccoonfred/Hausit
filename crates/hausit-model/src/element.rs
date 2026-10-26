use crate::category::Category;
use hausit_core::{DVec2, ElementId};
use serde::{Deserialize, Serialize};

/// An element instance in the model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub id: ElementId,
    /// "Mark" identity parameter.
    #[serde(default)]
    pub mark: String,
    /// "Comments" identity parameter.
    #[serde(default)]
    pub comments: String,
    pub data: ElementData,
}

impl Element {
    pub fn category(&self) -> Category {
        self.data.category()
    }

    /// Family type of the element, if it has one (datums do not).
    pub fn type_id(&self) -> Option<ElementId> {
        match &self.data {
            ElementData::Wall(w) => Some(w.type_id),
            ElementData::Floor(f) => Some(f.type_id),
            ElementData::Roof(r) => Some(r.type_id),
            ElementData::Door(o) | ElementData::Window(o) => Some(o.type_id),
            ElementData::Column(c) => Some(c.type_id),
            ElementData::Framing(f) => Some(f.type_id),
            ElementData::Footing(f) => Some(f.type_id),
            ElementData::WallFooting(f) => Some(f.type_id),
            ElementData::Stair(s) => Some(s.type_id),
            ElementData::Railing(r) => Some(r.type_id),
            ElementData::Ceiling(c) => Some(c.type_id),
            ElementData::Component(c) => Some(c.type_id),
            ElementData::Level(_)
            | ElementData::Grid(_)
            | ElementData::Room(_)
            | ElementData::RoomSeparation(_)
            | ElementData::Dimension(_) => None,
        }
    }

    pub fn set_type_id(&mut self, t: ElementId) {
        match &mut self.data {
            ElementData::Wall(w) => w.type_id = t,
            ElementData::Floor(f) => f.type_id = t,
            ElementData::Roof(r) => r.type_id = t,
            ElementData::Door(o) | ElementData::Window(o) => o.type_id = t,
            ElementData::Column(c) => c.type_id = t,
            ElementData::Framing(f) => f.type_id = t,
            ElementData::Footing(f) => f.type_id = t,
            ElementData::WallFooting(f) => f.type_id = t,
            ElementData::Stair(s) => s.type_id = t,
            ElementData::Railing(r) => r.type_id = t,
            ElementData::Ceiling(c) => c.type_id = t,
            ElementData::Component(c) => c.type_id = t,
            ElementData::Level(_)
            | ElementData::Grid(_)
            | ElementData::Room(_)
            | ElementData::RoomSeparation(_)
            | ElementData::Dimension(_) => {}
        }
    }

    pub fn as_wall(&self) -> Option<&Wall> {
        match &self.data {
            ElementData::Wall(w) => Some(w),
            _ => None,
        }
    }

    pub fn as_level(&self) -> Option<&Level> {
        match &self.data {
            ElementData::Level(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_opening(&self) -> Option<&HostedOpening> {
        match &self.data {
            ElementData::Door(o) | ElementData::Window(o) => Some(o),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ElementData {
    Level(Level),
    Grid(Grid),
    Wall(Wall),
    Floor(Floor),
    Roof(Roof),
    Door(HostedOpening),
    Window(HostedOpening),
    Column(Column),
    Framing(Framing),
    Footing(Footing),
    WallFooting(WallFooting),
    Stair(Stair),
    Railing(Railing),
    Ceiling(Ceiling),
    Component(Component),
    Room(Room),
    RoomSeparation(SketchedLine),
    Dimension(Dimension),
}

impl ElementData {
    pub fn category(&self) -> Category {
        match self {
            ElementData::Level(_) => Category::Levels,
            ElementData::Grid(_) => Category::Grids,
            ElementData::Wall(_) => Category::Walls,
            ElementData::Floor(_) => Category::Floors,
            ElementData::Roof(_) => Category::Roofs,
            ElementData::Door(_) => Category::Doors,
            ElementData::Window(_) => Category::Windows,
            ElementData::Column(_) => Category::StructuralColumns,
            ElementData::Framing(_) => Category::StructuralFraming,
            ElementData::Footing(_) | ElementData::WallFooting(_) => Category::StructuralFoundations,
            ElementData::Stair(_) => Category::Stairs,
            ElementData::Railing(_) => Category::Railings,
            ElementData::Ceiling(_) => Category::Ceilings,
            ElementData::Component(c) => c.category,
            ElementData::Room(_) => Category::Rooms,
            ElementData::RoomSeparation(_) => Category::RoomSeparation,
            ElementData::Dimension(_) => Category::Dimensions,
        }
    }
}

/// A horizontal datum (Revit "Level").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub name: String,
    /// Elevation above project base point, mm.
    pub elevation: f64,
    #[serde(default = "yes")]
    pub building_story: bool,
}

/// A vertical datum line in plan (Revit "Grid").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Grid {
    pub name: String,
    pub start: DVec2,
    pub end: DVec2,
}

/// How the top of a wall is defined.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum TopConstraint {
    Unconnected { height: f64 },
    UpToLevel { level: ElementId, offset: f64 },
}

/// Which line of the wall the user draws along. The wall always stores its
/// centreline; this only affects placement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocationLine {
    #[default]
    WallCenterline,
    FinishFaceExterior,
    FinishFaceInterior,
}

impl LocationLine {
    pub const ALL: [LocationLine; 3] = [
        LocationLine::WallCenterline,
        LocationLine::FinishFaceExterior,
        LocationLine::FinishFaceInterior,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LocationLine::WallCenterline => "Wall Centerline",
            LocationLine::FinishFaceExterior => "Finish Face: Exterior",
            LocationLine::FinishFaceInterior => "Finish Face: Interior",
        }
    }
}

/// A straight basic wall. The exterior face is on the left of start→end,
/// which is the outside when walls are drawn clockwise, as in Revit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    pub type_id: ElementId,
    pub start: DVec2,
    pub end: DVec2,
    pub base_level: ElementId,
    #[serde(default)]
    pub base_offset: f64,
    pub top: TopConstraint,
    #[serde(default)]
    pub structural: bool,
    #[serde(default = "yes")]
    pub room_bounding: bool,
    /// Roofs and floors the wall top is attached to (Attach Top/Base):
    /// the wall extends or shortens to meet their underside.
    #[serde(default)]
    pub attached_top: Vec<ElementId>,
}

impl Wall {
    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }

    pub fn direction(&self) -> DVec2 {
        (self.end - self.start).normalize_or_zero()
    }

    /// Point at `offset` mm along the wall from its start.
    pub fn point_at(&self, offset: f64) -> DVec2 {
        self.start + self.direction() * offset
    }
}

/// A floor slab, its top at the level (plus offset) and thickness downward.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Floor {
    pub type_id: ElementId,
    pub level: ElementId,
    pub boundary: Vec<DVec2>,
    #[serde(default)]
    pub height_offset: f64,
    #[serde(default = "yes")]
    pub room_bounding: bool,
    #[serde(default)]
    pub structural: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoofShape {
    Flat,
    /// All edges define slope.
    #[default]
    Hip,
    /// Only the two long edges define slope (rectangular footprints).
    Gable,
}

impl RoofShape {
    pub const ALL: [RoofShape; 3] = [RoofShape::Flat, RoofShape::Hip, RoofShape::Gable];

    pub fn label(self) -> &'static str {
        match self {
            RoofShape::Flat => "Flat",
            RoofShape::Hip => "Hip (all edges define slope)",
            RoofShape::Gable => "Gable (long edges define slope)",
        }
    }
}

/// A footprint roof (Revit's Roof by Footprint). Each footprint edge can
/// define slope; the roof faces come from the straight skeleton, so any
/// simple footprint gets proper hips, valleys and gables.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Roof {
    pub type_id: ElementId,
    pub level: ElementId,
    pub footprint: Vec<DVec2>,
    #[serde(default)]
    pub base_offset: f64,
    /// Slope angle in degrees.
    pub slope: f64,
    #[serde(default)]
    pub shape: RoofShape,
    /// "Defines Slope" per footprint edge (edge i runs from point i to
    /// i + 1). Empty: derived from `shape`.
    #[serde(default)]
    pub defines_slope: Vec<bool>,
}

impl Roof {
    /// Which footprint edges define slope.
    pub fn edge_slopes(&self) -> Vec<bool> {
        let n = self.footprint.len();
        if self.defines_slope.len() == n {
            return self.defines_slope.clone();
        }
        match self.shape {
            RoofShape::Flat => vec![false; n],
            RoofShape::Hip => vec![true; n],
            RoofShape::Gable if n == 4 => {
                let len = |i: usize| self.footprint[i].distance(self.footprint[(i + 1) % 4]);
                let even_long = len(0) + len(2) >= len(1) + len(3);
                (0..4).map(|i| (i % 2 == 0) == even_long).collect()
            }
            RoofShape::Gable => vec![true; n],
        }
    }

    pub fn is_sloped(&self) -> bool {
        self.slope > 0.0 && self.edge_slopes().iter().any(|b| *b)
    }
}

/// A door or window hosted by a wall.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HostedOpening {
    pub type_id: ElementId,
    pub host: ElementId,
    /// Distance from the host wall's start to the opening's centre, mm.
    pub offset: f64,
    /// Sill height above the host's base level, mm.
    pub sill_height: f64,
    /// Swaps hinge side (doors) / handing.
    #[serde(default)]
    pub flip_hand: bool,
    /// Swaps which side of the wall the door swings to.
    #[serde(default)]
    pub flip_facing: bool,
}

fn yes() -> bool {
    true
}

/// A structural column: a vertical member at a point, from its base level
/// up to its top level.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub type_id: ElementId,
    pub point: DVec2,
    /// Rotation in plan, degrees.
    #[serde(default)]
    pub rotation: f64,
    pub base_level: ElementId,
    #[serde(default)]
    pub base_offset: f64,
    pub top_level: ElementId,
    #[serde(default)]
    pub top_offset: f64,
}

/// Whether a framing member is a beam or a brace. Beams hang from their
/// reference line (its height is the top of the beam); braces are centred
/// on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FramingKind {
    #[default]
    Beam,
    Brace,
}

/// Structural framing: a beam or brace swept along a straight line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Framing {
    pub type_id: ElementId,
    pub kind: FramingKind,
    /// Reference level; the end heights are offsets from it.
    pub level: ElementId,
    pub start: DVec2,
    pub end: DVec2,
    #[serde(default)]
    pub start_offset: f64,
    #[serde(default)]
    pub end_offset: f64,
}

impl Framing {
    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }
}

/// An isolated (pad) foundation, its top at its level plus an offset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Footing {
    pub type_id: ElementId,
    pub level: ElementId,
    pub point: DVec2,
    /// Rotation in plan, degrees.
    #[serde(default)]
    pub rotation: f64,
    #[serde(default)]
    pub offset: f64,
}

/// A strip foundation under a wall; it follows the wall.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WallFooting {
    pub type_id: ElementId,
    pub wall: ElementId,
}

/// A stair between two levels, along a run path: straight runs joined by
/// landings at each turn. Risers are spread evenly over the height.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stair {
    pub type_id: ElementId,
    pub base_level: ElementId,
    #[serde(default)]
    pub base_offset: f64,
    pub top_level: ElementId,
    #[serde(default)]
    pub top_offset: f64,
    /// Centreline of the runs, from the first riser.
    pub path: Vec<DVec2>,
    /// Actual Run Width, mm.
    pub width: f64,
}

/// A ceiling: a boundary at a height above its level.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ceiling {
    pub type_id: ElementId,
    pub level: ElementId,
    pub boundary: Vec<DVec2>,
    /// Height of its underside above the level.
    pub height_offset: f64,
}

/// A railing along a path. Railings hosted by a stair follow its slope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Railing {
    pub type_id: ElementId,
    pub level: ElementId,
    #[serde(default)]
    pub base_offset: f64,
    pub path: Vec<DVec2>,
    /// The stair it sits on (`INVALID` when free-standing).
    #[serde(default)]
    pub host: ElementId,
}

/// A loadable component (furniture, casework, fixtures, equipment): an
/// instance of a family type at a point on a level.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Component {
    pub type_id: ElementId,
    /// The type's category, one of [`Category::COMPONENTS`].
    pub category: Category,
    pub level: ElementId,
    pub point: DVec2,
    /// Rotation in plan, degrees.
    #[serde(default)]
    pub rotation: f64,
    /// Offset of its base above the level, mm.
    #[serde(default)]
    pub offset: f64,
}

/// A room, placed at a point; its boundary comes from the room-bounding
/// walls, columns and room separation lines around that point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Room {
    pub level: ElementId,
    pub point: DVec2,
    pub name: String,
    pub number: String,
    /// Upper Limit level (`INVALID`: the room's own level).
    #[serde(default)]
    pub upper_limit: ElementId,
    /// Limit Offset above the upper limit, mm.
    #[serde(default)]
    pub limit_offset: f64,
}

/// A line drawn on a level, such as a room separation line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchedLine {
    pub level: ElementId,
    pub start: DVec2,
    pub end: DVec2,
}

/// What a dimension measures to. Wall references follow the wall when it
/// moves.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum DimRef {
    /// A wall face or centreline: `side` 1 = exterior face, 0 = location
    /// line (centreline), -1 = interior face.
    Wall {
        wall: ElementId,
        side: i8,
    },
    Grid {
        grid: ElementId,
    },
    /// A fixed line through `point` along `dir`.
    Line {
        point: DVec2,
        dir: DVec2,
    },
}

/// A linear (aligned) dimension string in one view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dimension {
    pub view: ElementId,
    pub refs: Vec<DimRef>,
    /// A point the dimension line passes through.
    pub line_point: DVec2,
}
