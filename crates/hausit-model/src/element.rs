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
            ElementData::Level(_) | ElementData::Grid(_) => None,
        }
    }

    pub fn set_type_id(&mut self, t: ElementId) {
        match &mut self.data {
            ElementData::Wall(w) => w.type_id = t,
            ElementData::Floor(f) => f.type_id = t,
            ElementData::Roof(r) => r.type_id = t,
            ElementData::Door(o) | ElementData::Window(o) => o.type_id = t,
            ElementData::Level(_) | ElementData::Grid(_) => {}
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
