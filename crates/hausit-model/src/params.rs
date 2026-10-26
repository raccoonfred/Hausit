//! Generic parameter access, so the UI can show and edit any element in a
//! Revit-style Properties palette without knowing its concrete type.

use crate::document::{Document, ModelError};
use crate::element::{ElementData, FramingKind, RoofShape, TopConstraint};
use hausit_core::ElementId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParamValue {
    /// mm
    Length(f64),
    /// mm²
    Area(f64),
    /// mm³
    Volume(f64),
    /// degrees
    Angle(f64),
    Number(f64),
    Integer(i64),
    Text(String),
    Bool(bool),
    /// A reference to another element (e.g. a level). `INVALID` means none.
    Element(ElementId),
    /// One of a fixed set of choices, by index.
    Choice(usize),
}

impl ParamValue {
    pub fn as_length(&self) -> Option<f64> {
        match self {
            ParamValue::Length(v) => Some(*v),
            _ => None,
        }
    }
}

/// Property groups, in the order Revit's Properties palette shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ParamGroup {
    Constraints,
    Construction,
    Structural,
    Dimensions,
    IdentityData,
    Other,
}

impl ParamGroup {
    pub fn label(self) -> &'static str {
        match self {
            ParamGroup::Constraints => "Constraints",
            ParamGroup::Construction => "Construction",
            ParamGroup::Structural => "Structural",
            ParamGroup::Dimensions => "Dimensions",
            ParamGroup::IdentityData => "Identity Data",
            ParamGroup::Other => "Other",
        }
    }
}

/// What kind of editor the UI should present.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamKind {
    Value,
    /// Pick a level.
    Level,
    /// Pick a level, or "Unconnected" (`ElementId::INVALID`).
    LevelOrNone,
    /// Choice among labels.
    Choice(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub name: &'static str,
    pub group: ParamGroup,
    pub value: ParamValue,
    pub kind: ParamKind,
    pub read_only: bool,
}

impl Parameter {
    fn new(name: &'static str, group: ParamGroup, value: ParamValue) -> Self {
        Parameter {
            name,
            group,
            value,
            kind: ParamKind::Value,
            read_only: false,
        }
    }
    fn ro(mut self) -> Self {
        self.read_only = true;
        self
    }
    fn kind(mut self, k: ParamKind) -> Self {
        self.kind = k;
        self
    }
}

use ParamGroup as G;
use ParamValue as V;

/// Instance parameters of an element, grouped as Revit shows them.
pub fn element_parameters(doc: &Document, id: ElementId) -> Vec<Parameter> {
    let Some(el) = doc.element(id) else { return Vec::new() };
    let mut p = Vec::new();
    match &el.data {
        ElementData::Level(l) => {
            p.push(Parameter::new("Elevation", G::Constraints, V::Length(l.elevation)));
            p.push(Parameter::new("Name", G::IdentityData, V::Text(l.name.clone())));
            p.push(Parameter::new("Building Story", G::Other, V::Bool(l.building_story)));
        }
        ElementData::Grid(g) => {
            p.push(Parameter::new("Name", G::IdentityData, V::Text(g.name.clone())));
            p.push(Parameter::new("Length", G::Dimensions, V::Length(g.start.distance(g.end))).ro());
        }
        ElementData::Wall(w) => {
            p.push(Parameter::new("Base Constraint", G::Constraints, V::Element(w.base_level)).kind(ParamKind::Level));
            p.push(Parameter::new("Base Offset", G::Constraints, V::Length(w.base_offset)));
            let (top_level, top_offset, unconnected) = match w.top {
                TopConstraint::Unconnected { height } => (ElementId::INVALID, 0.0, height),
                TopConstraint::UpToLevel { level, offset } => (level, offset, doc.wall_height(w)),
            };
            p.push(Parameter::new("Top Constraint", G::Constraints, V::Element(top_level)).kind(ParamKind::LevelOrNone));
            let unc = Parameter::new("Unconnected Height", G::Constraints, V::Length(unconnected));
            p.push(if top_level.is_valid() { unc.ro() } else { unc });
            let off = Parameter::new("Top Offset", G::Constraints, V::Length(top_offset));
            p.push(if top_level.is_valid() { off } else { off.ro() });
            p.push(Parameter::new("Room Bounding", G::Constraints, V::Bool(w.room_bounding)));
            p.push(Parameter::new("Structural", G::Structural, V::Bool(w.structural)));
            let len = w.length();
            let h = doc.wall_height(w);
            let area = doc.wall_net_area(id);
            let t = doc.type_of(id).map(|t| t.thickness()).unwrap_or(0.0);
            p.push(Parameter::new("Length", G::Dimensions, V::Length(len)).ro());
            p.push(Parameter::new("Area", G::Dimensions, V::Area(area)).ro());
            p.push(Parameter::new("Volume", G::Dimensions, V::Volume(area * t)).ro());
            let _ = h;
        }
        ElementData::Floor(f) => {
            p.push(Parameter::new("Level", G::Constraints, V::Element(f.level)).kind(ParamKind::Level));
            p.push(Parameter::new(
                "Height Offset From Level",
                G::Constraints,
                V::Length(f.height_offset),
            ));
            p.push(Parameter::new("Room Bounding", G::Constraints, V::Bool(f.room_bounding)));
            p.push(Parameter::new("Structural", G::Structural, V::Bool(f.structural)));
            let area = hausit_core::geom::polygon_area(&f.boundary).abs();
            let t = doc.type_of(id).map(|t| t.thickness()).unwrap_or(0.0);
            p.push(
                Parameter::new(
                    "Perimeter",
                    G::Dimensions,
                    V::Length(hausit_core::geom::polygon_perimeter(&f.boundary)),
                )
                .ro(),
            );
            p.push(Parameter::new("Area", G::Dimensions, V::Area(area)).ro());
            p.push(Parameter::new("Volume", G::Dimensions, V::Volume(area * t)).ro());
            p.push(Parameter::new("Thickness", G::Dimensions, V::Length(t)).ro());
        }
        ElementData::Roof(r) => {
            p.push(Parameter::new("Base Level", G::Constraints, V::Element(r.level)).kind(ParamKind::Level));
            p.push(Parameter::new("Base Offset From Level", G::Constraints, V::Length(r.base_offset)));
            let shapes = RoofShape::ALL.iter().map(|s| s.label().to_string()).collect();
            let idx = RoofShape::ALL.iter().position(|s| *s == r.shape).unwrap_or(0);
            p.push(Parameter::new("Roof Shape", G::Constraints, V::Choice(idx)).kind(ParamKind::Choice(shapes)));
            p.push(Parameter::new("Slope", G::Dimensions, V::Angle(r.slope)));
            let t = doc.type_of(id).map(|t| t.thickness()).unwrap_or(0.0);
            p.push(Parameter::new("Thickness", G::Dimensions, V::Length(t)).ro());
            let area = hausit_core::geom::polygon_area(&r.footprint).abs();
            p.push(Parameter::new("Area", G::Dimensions, V::Area(area)).ro());
        }
        ElementData::Door(o) | ElementData::Window(o) => {
            let level = doc
                .element(o.host)
                .and_then(|h| h.as_wall())
                .map(|w| w.base_level)
                .unwrap_or_default();
            p.push(
                Parameter::new("Level", G::Constraints, V::Element(level))
                    .kind(ParamKind::Level)
                    .ro(),
            );
            p.push(Parameter::new("Sill Height", G::Constraints, V::Length(o.sill_height)));
            let h = doc.type_of(id).map(|t| t.height()).unwrap_or(0.0);
            p.push(Parameter::new("Head Height", G::Constraints, V::Length(o.sill_height + h)).ro());
            p.push(Parameter::new("Host Offset", G::Constraints, V::Length(o.offset)));
            p.push(Parameter::new("Host", G::Other, V::Text(doc.describe(o.host))).ro());
            p.push(Parameter::new("Flip Hand", G::Other, V::Bool(o.flip_hand)));
            p.push(Parameter::new("Flip Facing", G::Other, V::Bool(o.flip_facing)));
        }
        ElementData::Column(c) => {
            p.push(Parameter::new("Base Level", G::Constraints, V::Element(c.base_level)).kind(ParamKind::Level));
            p.push(Parameter::new("Base Offset", G::Constraints, V::Length(c.base_offset)));
            p.push(Parameter::new("Top Level", G::Constraints, V::Element(c.top_level)).kind(ParamKind::LevelOrNone));
            p.push(Parameter::new("Top Offset", G::Constraints, V::Length(c.top_offset)));
            p.push(Parameter::new("Rotation", G::Constraints, V::Angle(c.rotation)));
            let (z0, z1) = crate::geometry::column_extent(doc, c);
            p.push(Parameter::new("Length", G::Dimensions, V::Length(z1 - z0)).ro());
            let area = section_area(doc, id);
            p.push(Parameter::new("Volume", G::Dimensions, V::Volume(area * (z1 - z0))).ro());
        }
        ElementData::Framing(f) => {
            p.push(Parameter::new("Reference Level", G::Constraints, V::Element(f.level)).kind(ParamKind::Level));
            p.push(Parameter::new("Start Level Offset", G::Constraints, V::Length(f.start_offset)));
            p.push(Parameter::new("End Level Offset", G::Constraints, V::Length(f.end_offset)));
            let usages = vec!["Beam".to_string(), "Brace".to_string()];
            let idx = usize::from(f.kind == FramingKind::Brace);
            p.push(Parameter::new("Structural Usage", G::Structural, V::Choice(idx)).kind(ParamKind::Choice(usages)));
            let (a, b) = crate::geometry::framing_axis(doc, id, f);
            let len = a.distance(b);
            p.push(Parameter::new("Length", G::Dimensions, V::Length(len)).ro());
            p.push(Parameter::new("Volume", G::Dimensions, V::Volume(section_area(doc, id) * len)).ro());
        }
        ElementData::Footing(f) => {
            p.push(Parameter::new("Level", G::Constraints, V::Element(f.level)).kind(ParamKind::Level));
            p.push(Parameter::new("Height Offset From Level", G::Constraints, V::Length(f.offset)));
            p.push(Parameter::new("Rotation", G::Constraints, V::Angle(f.rotation)));
            p.push(Parameter::new("Volume", G::Dimensions, V::Volume(foundation_volume(doc, id))).ro());
        }
        ElementData::Stair(s) => {
            p.push(Parameter::new("Base Level", G::Constraints, V::Element(s.base_level)).kind(ParamKind::Level));
            p.push(Parameter::new("Base Offset", G::Constraints, V::Length(s.base_offset)));
            p.push(Parameter::new("Top Level", G::Constraints, V::Element(s.top_level)).kind(ParamKind::LevelOrNone));
            p.push(Parameter::new("Top Offset", G::Constraints, V::Length(s.top_offset)));
            p.push(Parameter::new("Actual Run Width", G::Dimensions, V::Length(s.width)));
            if let Some(l) = crate::stairs::stair_layout(doc, id) {
                let (z0, z1) = (l.z0, l.z1);
                let treads = l.pieces.iter().filter(|p| !p.landing).count() as i64;
                p.push(Parameter::new("Desired Stair Height", G::Dimensions, V::Length(z1 - z0)).ro());
                p.push(Parameter::new("Actual Number of Risers", G::Dimensions, V::Integer(l.risers as i64)).ro());
                p.push(Parameter::new("Actual Riser Height", G::Dimensions, V::Length(l.riser)).ro());
                p.push(Parameter::new("Actual Number of Treads", G::Dimensions, V::Integer(treads)).ro());
                let g = crate::stairs::StairRules::of(doc.type_of(id)).tread;
                p.push(Parameter::new("Actual Tread Depth", G::Dimensions, V::Length(g)).ro());
            }
        }
        ElementData::Railing(r) => {
            p.push(Parameter::new("Base Level", G::Constraints, V::Element(r.level)).kind(ParamKind::Level));
            p.push(Parameter::new("Base Offset", G::Constraints, V::Length(r.base_offset)));
            if r.host.is_valid() {
                p.push(Parameter::new("Host", G::Other, V::Text(doc.describe(r.host))).ro());
            }
            let len: f64 = r.path.windows(2).map(|w| w[0].distance(w[1])).sum();
            p.push(Parameter::new("Length", G::Dimensions, V::Length(len)).ro());
        }
        ElementData::WallFooting(f) => {
            p.push(Parameter::new("Host", G::Other, V::Text(doc.describe(f.wall))).ro());
            let len = doc.element(f.wall).and_then(|w| w.as_wall()).map(|w| w.length()).unwrap_or(0.0);
            p.push(Parameter::new("Length", G::Dimensions, V::Length(len)).ro());
            p.push(Parameter::new("Volume", G::Dimensions, V::Volume(foundation_volume(doc, id))).ro());
        }
        ElementData::Room(r) => {
            let info = crate::rooms::room_info(doc, r);
            p.push(
                Parameter::new("Level", G::Constraints, V::Element(r.level))
                    .kind(ParamKind::Level)
                    .ro(),
            );
            p.push(Parameter::new("Upper Limit", G::Constraints, V::Element(r.upper_limit)).kind(ParamKind::LevelOrNone));
            p.push(Parameter::new("Limit Offset", G::Constraints, V::Length(r.limit_offset)));
            p.push(Parameter::new("Area", G::Dimensions, V::Area(info.area)).ro());
            p.push(Parameter::new("Perimeter", G::Dimensions, V::Length(info.perimeter)).ro());
            p.push(Parameter::new("Unbounded Height", G::Dimensions, V::Length(info.height)).ro());
            p.push(Parameter::new("Volume", G::Dimensions, V::Volume(info.volume)).ro());
            p.push(Parameter::new("Number", G::IdentityData, V::Text(r.number.clone())));
            p.push(Parameter::new("Name", G::IdentityData, V::Text(r.name.clone())));
        }
        ElementData::RoomSeparation(l) => {
            p.push(
                Parameter::new("Level", G::Constraints, V::Element(l.level))
                    .kind(ParamKind::Level)
                    .ro(),
            );
            p.push(Parameter::new("Length", G::Dimensions, V::Length(l.start.distance(l.end))).ro());
        }
        ElementData::Dimension(d) => {
            p.push(Parameter::new("Segments", G::Other, V::Integer(d.refs.len().saturating_sub(1) as i64)).ro());
        }
    }
    if el.category().is_model() {
        p.push(Parameter::new("Comments", G::IdentityData, V::Text(el.comments.clone())));
        p.push(Parameter::new("Mark", G::IdentityData, V::Text(el.mark.clone())));
    } else if el.category() == crate::Category::Rooms {
        p.push(Parameter::new("Comments", G::IdentityData, V::Text(el.comments.clone())));
    }
    p.sort_by_key(|p| p.group);
    p
}

/// Sets an instance parameter. Read-only parameters are rejected.
pub fn set_element_parameter(doc: &mut Document, id: ElementId, name: &str, value: ParamValue) -> Result<(), ModelError> {
    let current = element_parameters(doc, id);
    let Some(param) = current.iter().find(|p| p.name == name) else {
        return Err(ModelError::UnknownParameter(name.to_string()));
    };
    if param.read_only {
        return Err(ModelError::ReadOnly(name.to_string()));
    }
    let bad = || ModelError::InvalidValue(name.to_string());
    let el = doc.element_mut(id).ok_or(ModelError::NotFound(id))?;
    match (name, &value) {
        ("Comments", V::Text(s)) => el.comments = s.clone(),
        ("Mark", V::Text(s)) => el.mark = s.clone(),
        _ => match &mut el.data {
            ElementData::Level(l) => match (name, value) {
                ("Elevation", V::Length(v)) => l.elevation = v,
                ("Name", V::Text(s)) if !s.trim().is_empty() => l.name = s,
                ("Building Story", V::Bool(b)) => l.building_story = b,
                _ => return Err(bad()),
            },
            ElementData::Grid(g) => match (name, value) {
                ("Name", V::Text(s)) if !s.trim().is_empty() => g.name = s,
                _ => return Err(bad()),
            },
            ElementData::Wall(w) => match (name, value) {
                ("Base Constraint", V::Element(l)) if l.is_valid() => w.base_level = l,
                ("Base Offset", V::Length(v)) => w.base_offset = v,
                ("Top Constraint", V::Element(l)) => {
                    w.top = if l.is_valid() {
                        TopConstraint::UpToLevel { level: l, offset: 0.0 }
                    } else {
                        TopConstraint::Unconnected { height: 3000.0 }
                    }
                }
                ("Unconnected Height", V::Length(v)) if v > 0.0 => w.top = TopConstraint::Unconnected { height: v },
                ("Top Offset", V::Length(v)) => {
                    if let TopConstraint::UpToLevel { offset, .. } = &mut w.top {
                        *offset = v
                    }
                }
                ("Room Bounding", V::Bool(b)) => w.room_bounding = b,
                ("Structural", V::Bool(b)) => w.structural = b,
                _ => return Err(bad()),
            },
            ElementData::Floor(f) => match (name, value) {
                ("Level", V::Element(l)) if l.is_valid() => f.level = l,
                ("Height Offset From Level", V::Length(v)) => f.height_offset = v,
                ("Room Bounding", V::Bool(b)) => f.room_bounding = b,
                ("Structural", V::Bool(b)) => f.structural = b,
                _ => return Err(bad()),
            },
            ElementData::Roof(r) => match (name, value) {
                ("Base Level", V::Element(l)) if l.is_valid() => r.level = l,
                ("Base Offset From Level", V::Length(v)) => r.base_offset = v,
                ("Roof Shape", V::Choice(i)) if i < RoofShape::ALL.len() => {
                    r.shape = RoofShape::ALL[i];
                    r.defines_slope.clear();
                }
                ("Slope", V::Angle(a)) if (0.0..80.0).contains(&a) => r.slope = a,
                _ => return Err(bad()),
            },
            ElementData::Door(o) | ElementData::Window(o) => match (name, value) {
                ("Sill Height", V::Length(v)) => o.sill_height = v,
                ("Host Offset", V::Length(v)) => o.offset = v,
                ("Flip Hand", V::Bool(b)) => o.flip_hand = b,
                ("Flip Facing", V::Bool(b)) => o.flip_facing = b,
                _ => return Err(bad()),
            },
            ElementData::Column(c) => match (name, value) {
                ("Base Level", V::Element(l)) if l.is_valid() => c.base_level = l,
                ("Base Offset", V::Length(v)) => c.base_offset = v,
                ("Top Level", V::Element(l)) => c.top_level = l,
                ("Top Offset", V::Length(v)) => c.top_offset = v,
                ("Rotation", V::Angle(a)) => c.rotation = a,
                _ => return Err(bad()),
            },
            ElementData::Framing(f) => match (name, value) {
                ("Reference Level", V::Element(l)) if l.is_valid() => f.level = l,
                ("Start Level Offset", V::Length(v)) => f.start_offset = v,
                ("End Level Offset", V::Length(v)) => f.end_offset = v,
                ("Structural Usage", V::Choice(i)) if i < 2 => f.kind = if i == 0 { FramingKind::Beam } else { FramingKind::Brace },
                _ => return Err(bad()),
            },
            ElementData::Footing(f) => match (name, value) {
                ("Level", V::Element(l)) if l.is_valid() => f.level = l,
                ("Height Offset From Level", V::Length(v)) => f.offset = v,
                ("Rotation", V::Angle(a)) => f.rotation = a,
                _ => return Err(bad()),
            },
            ElementData::WallFooting(_) => return Err(bad()),
            ElementData::Stair(s) => match (name, value) {
                ("Base Level", V::Element(l)) if l.is_valid() => s.base_level = l,
                ("Base Offset", V::Length(v)) => s.base_offset = v,
                ("Top Level", V::Element(l)) => s.top_level = l,
                ("Top Offset", V::Length(v)) => s.top_offset = v,
                ("Actual Run Width", V::Length(v)) if v > 0.0 => s.width = v,
                _ => return Err(bad()),
            },
            ElementData::Railing(r) => match (name, value) {
                ("Base Level", V::Element(l)) if l.is_valid() => r.level = l,
                ("Base Offset", V::Length(v)) => r.base_offset = v,
                _ => return Err(bad()),
            },
            ElementData::Room(r) => match (name, value) {
                ("Upper Limit", V::Element(l)) => r.upper_limit = l,
                ("Limit Offset", V::Length(v)) => r.limit_offset = v,
                ("Number", V::Text(s)) => r.number = s,
                ("Name", V::Text(s)) => r.name = s,
                _ => return Err(bad()),
            },
            ElementData::RoomSeparation(_) | ElementData::Dimension(_) => return Err(bad()),
        },
    }
    doc.touch();
    Ok(())
}

/// Sets a type parameter (affects every instance of the type).
pub fn set_type_parameter(doc: &mut Document, type_id: ElementId, name: &str, value: ParamValue) -> Result<(), ModelError> {
    let t = doc.family_type_mut(type_id).ok_or(ModelError::NotFound(type_id))?;
    let slot = t
        .params
        .get_mut(name)
        .ok_or_else(|| ModelError::UnknownParameter(name.to_string()))?;
    if std::mem::discriminant(slot) != std::mem::discriminant(&value) {
        return Err(ModelError::InvalidValue(name.to_string()));
    }
    if let V::Length(v) = value {
        if v <= 0.0 && name != "Default Sill Height" {
            return Err(ModelError::InvalidValue(name.to_string()));
        }
    }
    *slot = value;
    doc.touch();
    Ok(())
}

/// Cross-section area of a column or framing type, mm².
fn section_area(doc: &Document, id: ElementId) -> f64 {
    hausit_core::geom::polygon_area(&crate::geometry::section_profile(doc.type_of(id))).abs()
}

fn foundation_volume(doc: &Document, id: ElementId) -> f64 {
    crate::geometry::foundation_extent(doc, id)
        .map(|(o, z0, z1)| hausit_core::geom::polygon_area(&o).abs() * (z1 - z0))
        .unwrap_or(0.0)
}
