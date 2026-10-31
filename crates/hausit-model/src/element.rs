use crate::category::Category;
use hausit_core::{DVec2, DVec3, ElementId};
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
    /// Phasing: the phase the element was created in (`None`: the latest
    /// phase) and the phase it is demolished in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_created: Option<ElementId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_demolished: Option<ElementId>,
    /// The workset the element is on, once worksharing is enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workset: Option<ElementId>,
    /// The design option the element belongs to (`None`: main model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design_option: Option<ElementId>,
    /// Pinned elements cannot be moved, rotated, mirrored or deleted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
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
            ElementData::Toposolid(t) => Some(t.type_id),
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
            ElementData::DirectShape(_) => None,
            ElementData::Group(g) => Some(g.type_id),
            ElementData::Level(_)
            | ElementData::Grid(_)
            | ElementData::Room(_)
            | ElementData::RoomSeparation(_)
            | ElementData::Dimension(_)
            | ElementData::TextNote(_)
            | ElementData::DetailLine(_)
            | ElementData::FilledRegion(_)
            | ElementData::SpotElevation(_)
            | ElementData::RevisionCloud(_)
            | ElementData::PropertyLine(_)
            | ElementData::AreaBoundary(_)
            | ElementData::Area(_)
            | ElementData::Tag(_) => None,
        }
    }

    pub fn set_type_id(&mut self, t: ElementId) {
        match &mut self.data {
            ElementData::Wall(w) => w.type_id = t,
            ElementData::Floor(f) => f.type_id = t,
            ElementData::Toposolid(s) => s.type_id = t,
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
            // A group's type is changed by placing the other group.
            ElementData::DirectShape(_) | ElementData::Group(_) => {}
            ElementData::Level(_)
            | ElementData::Grid(_)
            | ElementData::Room(_)
            | ElementData::RoomSeparation(_)
            | ElementData::Dimension(_)
            | ElementData::TextNote(_)
            | ElementData::DetailLine(_)
            | ElementData::FilledRegion(_)
            | ElementData::SpotElevation(_)
            | ElementData::RevisionCloud(_)
            | ElementData::PropertyLine(_)
            | ElementData::AreaBoundary(_)
            | ElementData::Area(_)
            | ElementData::Tag(_) => {}
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
    DirectShape(DirectShape),
    Room(Room),
    RoomSeparation(SketchedLine),
    Dimension(Dimension),
    TextNote(TextNote),
    Tag(Tag),
    DetailLine(crate::detail::DetailLine),
    FilledRegion(crate::detail::FilledRegion),
    SpotElevation(crate::detail::SpotElevation),
    Group(crate::groups::ModelGroup),
    AreaBoundary(crate::areas::AreaBoundary),
    Area(crate::areas::Area),
    RevisionCloud(crate::revisions::RevisionCloud),
    Toposolid(crate::site::Toposolid),
    PropertyLine(crate::site::PropertyLine),
}

impl ElementData {
    /// Every level the element refers to, for copying it to other levels.
    pub fn level_refs_mut(&mut self) -> Vec<&mut ElementId> {
        match self {
            ElementData::Wall(w) => {
                let mut v = vec![&mut w.base_level];
                if let TopConstraint::UpToLevel { level, .. } = &mut w.top {
                    v.push(level);
                }
                v
            }
            ElementData::Floor(f) => vec![&mut f.level],
            ElementData::Toposolid(t) => vec![&mut t.level],
            ElementData::PropertyLine(l) => vec![&mut l.level],
            ElementData::Roof(r) => vec![&mut r.level],
            ElementData::Column(c) => vec![&mut c.base_level, &mut c.top_level],
            ElementData::Framing(f) => vec![&mut f.level],
            ElementData::Footing(f) => vec![&mut f.level],
            ElementData::Stair(s) => vec![&mut s.base_level, &mut s.top_level],
            ElementData::Railing(r) => vec![&mut r.level],
            ElementData::Ceiling(c) => vec![&mut c.level],
            ElementData::Component(c) => vec![&mut c.level],
            ElementData::DirectShape(d) => vec![&mut d.level],
            ElementData::Room(r) => vec![&mut r.level, &mut r.upper_limit],
            ElementData::RoomSeparation(l) => vec![&mut l.level],
            ElementData::Group(g) => vec![&mut g.level],
            _ => vec![],
        }
    }

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
            ElementData::DirectShape(d) => d.category,
            ElementData::Room(_) => Category::Rooms,
            ElementData::RoomSeparation(_) => Category::RoomSeparation,
            ElementData::Dimension(_) => Category::Dimensions,
            ElementData::TextNote(_) => Category::TextNotes,
            ElementData::Tag(_) => Category::Tags,
            ElementData::DetailLine(_) => Category::Lines,
            ElementData::FilledRegion(_) => Category::DetailItems,
            ElementData::SpotElevation(_) => Category::SpotElevations,
            ElementData::RevisionCloud(_) => Category::RevisionClouds,
            ElementData::Toposolid(_) => Category::Toposolid,
            ElementData::PropertyLine(_) => Category::PropertyLines,
            ElementData::Group(_) => Category::ModelGroups,
            ElementData::AreaBoundary(_) => Category::AreaBoundary,
            ElementData::Area(_) => Category::Areas,
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
    /// Disallow Join at the start and end: the wall end stays square and
    /// does not clean up with walls that meet it there.
    #[serde(default)]
    pub disallow_join: [bool; 2],
    /// Arc walls: the point halfway along the arc from start to end.
    /// `None` for a straight wall.
    #[serde(default)]
    pub arc_mid: Option<DVec2>,
    /// Curtain walls: grid lines added to this instance.
    #[serde(default)]
    pub curtain: crate::curtain::CurtainGrids,
}

/// A circular arc: centre, radius, start angle and signed sweep (radians,
/// positive counter-clockwise).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcCurve {
    pub center: DVec2,
    pub radius: f64,
    pub start_angle: f64,
    pub sweep: f64,
}

impl ArcCurve {
    /// The arc from `a` through `m` to `b`, or `None` when the points are
    /// (nearly) in line.
    pub fn through(a: DVec2, m: DVec2, b: DVec2) -> Option<ArcCurve> {
        let d = 2.0 * (a.x * (m.y - b.y) + m.x * (b.y - a.y) + b.x * (a.y - m.y));
        let chord = a.distance(b).max(1.0);
        if d.abs() < 1e-6 * chord * chord {
            return None;
        }
        let sq = |p: DVec2| p.length_squared();
        let center = DVec2::new(
            (sq(a) * (m.y - b.y) + sq(m) * (b.y - a.y) + sq(b) * (a.y - m.y)) / d,
            (sq(a) * (b.x - m.x) + sq(m) * (a.x - b.x) + sq(b) * (m.x - a.x)) / d,
        );
        let radius = center.distance(a);
        if radius > 1e7 {
            return None;
        }
        let ang = |p: DVec2| (p.y - center.y).atan2(p.x - center.x);
        let tau = std::f64::consts::TAU;
        let (a0, am, ab) = (ang(a), ang(m), ang(b));
        let ccw = |x: f64| (x - a0).rem_euclid(tau);
        // Counter-clockwise when the middle comes before the end going CCW.
        let sweep = if ccw(am) <= ccw(ab) { ccw(ab) } else { ccw(ab) - tau };
        Some(ArcCurve {
            center,
            radius,
            start_angle: a0,
            sweep,
        })
    }

    pub fn length(&self) -> f64 {
        self.radius * self.sweep.abs()
    }

    pub fn point_at_angle(&self, t: f64) -> DVec2 {
        self.center + DVec2::new(t.cos(), t.sin()) * self.radius
    }

    /// Point at arc length `s` from the start.
    pub fn point_at(&self, s: f64) -> DVec2 {
        self.point_at_angle(self.start_angle + self.sweep.signum() * s / self.radius)
    }

    /// Unit tangent (in the direction of travel) at arc length `s`.
    pub fn tangent_at(&self, s: f64) -> DVec2 {
        let t = self.start_angle + self.sweep.signum() * s / self.radius;
        DVec2::new(-t.sin(), t.cos()) * self.sweep.signum()
    }

    /// Arc length from the start of the point on the arc nearest `p`,
    /// clamped to the arc.
    pub fn param_of(&self, p: DVec2) -> f64 {
        let tau = std::f64::consts::TAU;
        let t = (p.y - self.center.y).atan2(p.x - self.center.x);
        let mut d = (t - self.start_angle) * self.sweep.signum();
        d = d.rem_euclid(tau);
        let span = self.sweep.abs();
        if d > span {
            // Past the end: snap to whichever end is nearer in angle.
            d = if d - span < tau - d { span } else { 0.0 };
        }
        d * self.radius
    }

    /// Points along the arc, no more than `max_angle` radians apart.
    pub fn points(&self, max_angle: f64) -> Vec<DVec2> {
        let n = ((self.sweep.abs() / max_angle).ceil() as usize).max(1);
        (0..=n)
            .map(|i| self.point_at_angle(self.start_angle + self.sweep * i as f64 / n as f64))
            .collect()
    }
}

impl Wall {
    /// The wall's arc, for arc walls.
    pub fn arc(&self) -> Option<ArcCurve> {
        ArcCurve::through(self.start, self.arc_mid?, self.end)
    }

    /// Length along the location line.
    pub fn length(&self) -> f64 {
        match self.arc() {
            Some(a) => a.length(),
            None => self.start.distance(self.end),
        }
    }

    /// Direction from start to end (the chord, for arc walls).
    pub fn direction(&self) -> DVec2 {
        (self.end - self.start).normalize_or_zero()
    }

    /// Point at `offset` mm along the wall from its start.
    pub fn point_at(&self, offset: f64) -> DVec2 {
        match self.arc() {
            Some(a) => a.point_at(offset),
            None => self.start + self.direction() * offset,
        }
    }

    /// Unit direction of the location line at `offset` mm from the start.
    pub fn tangent_at(&self, offset: f64) -> DVec2 {
        match self.arc() {
            Some(a) => a.tangent_at(offset),
            None => self.direction(),
        }
    }

    /// Unit direction pointing into the wall from an end (0 start, 1 end).
    pub fn into_from(&self, end: usize) -> DVec2 {
        if end == 0 {
            self.tangent_at(0.0)
        } else {
            -self.tangent_at(self.length())
        }
    }

    /// Distance along the location line of the point on it nearest `p`.
    pub fn param_of(&self, p: DVec2) -> f64 {
        match self.arc() {
            Some(a) => a.param_of(p),
            None => {
                let (_, t) = hausit_core::geom::closest_on_segment(p, self.start, self.end);
                t * self.length()
            }
        }
    }

    /// Distance from `p` to the location line.
    pub fn distance_to(&self, p: DVec2) -> f64 {
        p.distance(self.point_at(self.param_of(p)))
    }

    /// The location line as points (two for a straight wall).
    pub fn centreline(&self) -> Vec<DVec2> {
        match self.arc() {
            Some(a) => a.points(ARC_STEP),
            None => vec![self.start, self.end],
        }
    }
}

/// Largest angle between the straight pieces arc walls are built from.
pub const ARC_STEP: f64 = std::f64::consts::PI / 36.0;

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
    /// A slope arrow (Revit's sketch tool): the roof is one plane rising
    /// at `slope` from the tail towards the head. It replaces sloping
    /// edges.
    #[serde(default)]
    pub slope_arrow: Option<SlopeArrow>,
}

/// A slope arrow across the shorter side of a footprint's bounding box,
/// rising from one side to the other (a mono-pitch roof).
pub fn default_slope_arrow(footprint: &[DVec2]) -> Option<SlopeArrow> {
    let (min, max) = footprint
        .iter()
        .fold((DVec2::splat(f64::MAX), DVec2::splat(f64::MIN)), |(a, b), p| (a.min(*p), b.max(*p)));
    if footprint.len() < 3 {
        return None;
    }
    let c = (min + max) / 2.0;
    let (tail, head) = if max.x - min.x >= max.y - min.y {
        (DVec2::new(c.x, min.y), DVec2::new(c.x, max.y))
    } else {
        (DVec2::new(min.x, c.y), DVec2::new(max.x, c.y))
    };
    Some(SlopeArrow { tail, head })
}

/// The slope arrow of a roof: from `tail` (at the roof's base height)
/// rising towards `head`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlopeArrow {
    pub tail: DVec2,
    pub head: DVec2,
}

impl Roof {
    /// Which footprint edges define slope.
    pub fn edge_slopes(&self) -> Vec<bool> {
        let n = self.footprint.len();
        if self.slope_arrow.is_some() {
            return vec![false; n];
        }
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

    /// The underside plane `(a, b, c)`, z = a·x + b·y + c, of a roof
    /// sloped by an arrow, with its base at `zb`.
    pub fn arrow_plane(&self, zb: f64) -> Option<DVec3> {
        let arrow = self.slope_arrow?;
        let d = (arrow.head - arrow.tail).normalize_or_zero();
        if d == DVec2::ZERO || self.slope <= 0.0 {
            return None;
        }
        let tan = self.slope.to_radians().tan();
        Some(DVec3::new(tan * d.x, tan * d.y, zb - tan * arrow.tail.dot(d)))
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

/// Geometry kept as it is, like Revit's DirectShape: what an import could
/// not turn into a native element, in any model category.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DirectShape {
    pub category: Category,
    pub level: ElementId,
    /// Where it came from, such as the IFC element's name.
    #[serde(default)]
    pub name: String,
    pub solids: Vec<ShapeSolid>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeSolid {
    #[serde(default)]
    pub material: Option<String>,
    pub geometry: ShapeGeometry,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ShapeGeometry {
    /// A plan polygon extruded between two heights.
    Prism { footprint: Vec<DVec2>, z0: f64, z1: f64 },
    /// Planar faces, each counter-clockwise seen from outside.
    Faces(Vec<Vec<DVec3>>),
}

impl DirectShape {
    /// Every point of the shape, for moving and bounds.
    pub fn points_mut(&mut self) -> impl Iterator<Item = (&mut f64, &mut f64)> {
        self.solids
            .iter_mut()
            .flat_map(|s| -> Box<dyn Iterator<Item = (&mut f64, &mut f64)>> {
                match &mut s.geometry {
                    ShapeGeometry::Prism { footprint, .. } => Box::new(footprint.iter_mut().map(|p| (&mut p.x, &mut p.y))),
                    ShapeGeometry::Faces(faces) => Box::new(faces.iter_mut().flatten().map(|p| (&mut p.x, &mut p.y))),
                }
            })
    }

    /// Plan points of the shape.
    pub fn plan_points(&self) -> Vec<DVec2> {
        self.solids
            .iter()
            .flat_map(|s| match &s.geometry {
                ShapeGeometry::Prism { footprint, .. } => footprint.clone(),
                ShapeGeometry::Faces(faces) => faces.iter().flatten().map(|p| p.truncate()).collect(),
            })
            .collect()
    }

    /// Applies a plan transform; `mirror` reverses faces so they still
    /// face outwards.
    pub fn transform(&mut self, f: impl Fn(DVec2) -> DVec2, mirror: bool) {
        for s in &mut self.solids {
            match &mut s.geometry {
                ShapeGeometry::Prism { footprint, .. } => {
                    footprint.iter_mut().for_each(|p| *p = f(*p));
                    if mirror {
                        footprint.reverse();
                    }
                }
                ShapeGeometry::Faces(faces) => {
                    for face in faces.iter_mut() {
                        for p in face.iter_mut() {
                            let q = f(p.truncate());
                            *p = DVec3::new(q.x, q.y, p.z);
                        }
                        if mirror {
                            face.reverse();
                        }
                    }
                }
            }
        }
    }
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
    #[serde(default)]
    pub department: String,
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
    /// A fixed point (Linear dimensions between points).
    Point {
        point: DVec2,
    },
}

/// Text placed in a view, as Revit's Text tool makes. Its position is
/// in the view's own coordinates (model x and y in plans).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextNote {
    pub view: ElementId,
    /// The top left corner of the text.
    pub position: DVec2,
    pub text: String,
    /// Height of the letters on paper, mm.
    #[serde(default = "default_text_size")]
    pub size: f64,
    /// An arrow from the text to this point.
    #[serde(default)]
    pub leader: Option<DVec2>,
}

fn default_text_size() -> f64 {
    2.5
}

/// A tag: a label in one view showing a value of the element it tags (a
/// door's Mark, a window's Type Mark...), as Revit's Tag by Category.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub view: ElementId,
    pub host: ElementId,
    /// The centre of the tag, in the view's coordinates.
    pub position: DVec2,
    /// Draw a line from the tag to the element.
    #[serde(default)]
    pub leader: bool,
}

/// A linear (aligned) dimension string in one view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dimension {
    pub view: ElementId,
    pub refs: Vec<DimRef>,
    /// A point the dimension line passes through.
    pub line_point: DVec2,
    /// Linear dimensions: the direction measured in (horizontal or
    /// vertical). `None`: aligned, across its references.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measure: Option<DVec2>,
}
