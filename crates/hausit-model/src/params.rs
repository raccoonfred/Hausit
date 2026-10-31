//! Generic parameter access, so the UI can show and edit any element in a
//! Revit-style Properties palette without knowing its concrete type.

use crate::detail::{LineStyle, REGION_PATTERNS};
use crate::document::{Document, ModelError};
use crate::element::{ElementData, FramingKind, RoofShape, TopConstraint};
use hausit_core::{DVec2, ElementId};
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
    Graphics,
    Construction,
    Structural,
    Dimensions,
    IdentityData,
    Phasing,
    Other,
}

impl ParamGroup {
    pub fn label(self) -> &'static str {
        match self {
            ParamGroup::Constraints => "Constraints",
            ParamGroup::Graphics => "Graphics",
            ParamGroup::Construction => "Construction",
            ParamGroup::Structural => "Structural",
            ParamGroup::Dimensions => "Dimensions",
            ParamGroup::IdentityData => "Identity Data",
            ParamGroup::Phasing => "Phasing",
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
            if let Some(arc) = w.arc() {
                p.push(Parameter::new("Radius", G::Dimensions, V::Length(arc.radius)).ro());
            }
            p.push(Parameter::new("Area", G::Dimensions, V::Area(area)).ro());
            p.push(Parameter::new("Volume", G::Dimensions, V::Volume(area * t)).ro());
            let _ = h;
        }
        ElementData::Toposolid(t) => {
            p.push(Parameter::new("Level", G::Constraints, V::Element(t.level)).kind(ParamKind::Level));
            p.push(Parameter::new(
                "Height Offset From Level",
                G::Constraints,
                V::Length(t.height_offset),
            ));
            p.push(Parameter::new("Area", G::Dimensions, V::Area(hausit_core::geom::polygon_area(&t.boundary).abs())).ro());
            p.push(
                Parameter::new(
                    "Perimeter",
                    G::Dimensions,
                    V::Length(hausit_core::geom::polygon_perimeter(&t.boundary)),
                )
                .ro(),
            );
            p.push(Parameter::new("Points", G::Other, V::Integer(t.points.len() as i64)).ro());
        }
        ElementData::LinkInstance(i) => {
            let t = doc.link_type(i.link);
            p.push(Parameter::new("Name", G::IdentityData, V::Text(t.map(|t| t.name.clone()).unwrap_or_default())).ro());
            p.push(Parameter::new("Path", G::Other, V::Text(t.map(|t| t.path.clone()).unwrap_or_default())).ro());
            p.push(Parameter::new("Elevation Offset", G::Constraints, V::Length(i.z)));
            p.push(Parameter::new("Rotation", G::Constraints, V::Angle(i.rotation)));
        }
        ElementData::PropertyLine(l) => {
            p.push(Parameter::new("Area", G::Dimensions, V::Area(doc.property_area(l))).ro());
            let n = l.points.len();
            let segs = if l.closed { n } else { n.saturating_sub(1) };
            let length: f64 = (0..segs).map(|i| l.points[i].distance(l.points[(i + 1) % n])).sum();
            p.push(Parameter::new("Length", G::Dimensions, V::Length(length)).ro());
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
            p.push(Parameter::new("Slope Arrow", G::Constraints, V::Bool(r.slope_arrow.is_some())));
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
        ElementData::DirectShape(d) => {
            p.push(Parameter::new("Level", G::Constraints, V::Element(d.level)).kind(ParamKind::Level));
            p.push(Parameter::new("Source", G::IdentityData, V::Text(d.name.clone())).ro());
        }
        ElementData::Component(c) => {
            p.push(Parameter::new("Level", G::Constraints, V::Element(c.level)).kind(ParamKind::Level));
            p.push(Parameter::new("Offset", G::Constraints, V::Length(c.offset)));
            p.push(Parameter::new("Rotation", G::Constraints, V::Angle(c.rotation)));
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
        ElementData::Ceiling(c) => {
            p.push(Parameter::new("Level", G::Constraints, V::Element(c.level)).kind(ParamKind::Level));
            p.push(Parameter::new(
                "Height Offset From Level",
                G::Constraints,
                V::Length(c.height_offset),
            ));
            let area = hausit_core::geom::polygon_area(&c.boundary).abs();
            let perimeter = hausit_core::geom::polygon_perimeter(&c.boundary);
            p.push(Parameter::new("Perimeter", G::Dimensions, V::Length(perimeter)).ro());
            p.push(Parameter::new("Area", G::Dimensions, V::Area(area)).ro());
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
            p.push(Parameter::new("Department", G::IdentityData, V::Text(r.department.clone())));
        }
        ElementData::Area(a) => {
            let (area, perimeter) = doc.area_size(a);
            p.push(Parameter::new("Area", G::Dimensions, V::Area(area)).ro());
            p.push(Parameter::new("Perimeter", G::Dimensions, V::Length(perimeter)).ro());
            p.push(Parameter::new("Number", G::IdentityData, V::Text(a.number.clone())));
            p.push(Parameter::new("Name", G::IdentityData, V::Text(a.name.clone())));
        }
        ElementData::AreaBoundary(b) => {
            p.push(Parameter::new("Length", G::Dimensions, V::Length(b.start.distance(b.end))).ro());
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
        ElementData::TextNote(t) => {
            p.push(Parameter::new("Text", G::Other, V::Text(t.text.clone())));
            p.push(Parameter::new("Text Size", G::Dimensions, V::Length(t.size)));
            p.push(Parameter::new("Leader", G::Other, V::Bool(t.leader.is_some())));
        }
        ElementData::RevisionCloud(c) => {
            let names: Vec<String> = doc.revisions.iter().map(|r| doc.revision_label(r.id)).collect();
            let idx = doc.revision_sequence(c.revision).saturating_sub(1);
            let locked = doc.cloud_locked(el.id);
            let mut rev = Parameter::new("Revision", G::IdentityData, V::Choice(idx)).kind(ParamKind::Choice(names));
            if locked {
                rev = rev.ro();
            }
            p.push(rev);
            let r = doc.revision_def(c.revision);
            p.push(Parameter::new("Revision Number", G::IdentityData, V::Text(doc.revision_number(c.revision))).ro());
            p.push(
                Parameter::new(
                    "Revision Date",
                    G::IdentityData,
                    V::Text(r.map(|r| r.date.clone()).unwrap_or_default()),
                )
                .ro(),
            );
            p.push(
                Parameter::new(
                    "Issued to",
                    G::IdentityData,
                    V::Text(r.map(|r| r.issued_to.clone()).unwrap_or_default()),
                )
                .ro(),
            );
            p.push(
                Parameter::new(
                    "Issued by",
                    G::IdentityData,
                    V::Text(r.map(|r| r.issued_by.clone()).unwrap_or_default()),
                )
                .ro(),
            );
            p.push(Parameter::new("Tag", G::Graphics, V::Bool(c.tag.is_some())));
        }
        ElementData::DetailLine(l) => {
            let styles: Vec<String> = LineStyle::ALL[..4].iter().map(|s| s.label().to_string()).collect();
            let idx = LineStyle::ALL.iter().position(|s| *s == l.style).unwrap_or(0).min(3);
            p.push(Parameter::new("Line Style", G::Graphics, V::Choice(idx)).kind(ParamKind::Choice(styles)));
            p.push(Parameter::new("Length", G::Dimensions, V::Length(l.start.distance(l.end))).ro());
        }
        ElementData::FilledRegion(r) => {
            let names: Vec<String> = REGION_PATTERNS.iter().map(|(n, _)| n.to_string()).collect();
            let idx = REGION_PATTERNS.iter().position(|(_, f)| *f == r.pattern).unwrap_or(0);
            p.push(Parameter::new("Fill Pattern", G::Graphics, V::Choice(idx)).kind(ParamKind::Choice(names)));
            p.push(Parameter::new("Masking", G::Graphics, V::Bool(r.masking)));
            let styles: Vec<String> = LineStyle::ALL.iter().map(|s| s.label().to_string()).collect();
            let idx = LineStyle::ALL.iter().position(|s| *s == r.border).unwrap_or(0);
            p.push(Parameter::new("Line Style", G::Graphics, V::Choice(idx)).kind(ParamKind::Choice(styles)));
            p.push(Parameter::new("Area", G::Dimensions, V::Area(hausit_core::geom::polygon_area(&r.boundary).abs())).ro());
        }
        ElementData::Group(g) => {
            p.push(
                Parameter::new("Reference Level", G::Constraints, V::Element(g.level))
                    .kind(ParamKind::Level)
                    .ro(),
            );
            p.push(Parameter::new("Members", G::Other, V::Integer(g.members.len() as i64)).ro());
        }
        ElementData::SpotElevation(sp) => {
            p.push(Parameter::new("Elevation", G::Dimensions, V::Length(doc.spot_height(sp))).ro());
        }
        ElementData::Tag(t) => {
            let host = doc.element(t.host).map(|h| h.category().singular()).unwrap_or("Nothing");
            p.push(Parameter::new("Tagged Element", G::Other, V::Text(host.to_string())).ro());
            p.push(Parameter::new("Label", G::Other, V::Text(crate::annotation::tag_label(doc, t))).ro());
            p.push(Parameter::new("Leader", G::Other, V::Bool(t.leader)));
        }
    }
    if el.category().is_model() {
        p.push(Parameter::new("Comments", G::IdentityData, V::Text(el.comments.clone())));
        p.push(Parameter::new("Mark", G::IdentityData, V::Text(el.mark.clone())));
    } else if el.category() == crate::Category::Rooms {
        p.push(Parameter::new("Comments", G::IdentityData, V::Text(el.comments.clone())));
    }
    if crate::phasing::is_phased(el.category()) && !doc.phases.is_empty() {
        let names: Vec<String> = doc.phases.iter().map(|p| p.name.clone()).collect();
        p.push(
            Parameter::new("Phase Created", G::Phasing, V::Choice(doc.phase_index(el.phase_created)))
                .kind(ParamKind::Choice(names.clone())),
        );
        let mut demo = vec!["None".to_string()];
        demo.extend(names);
        let d = el.phase_demolished.map(|d| doc.phase_index(Some(d)) + 1).unwrap_or(0);
        p.push(Parameter::new("Phase Demolished", G::Phasing, V::Choice(d)).kind(ParamKind::Choice(demo)));
    }
    if doc.worksharing() {
        let names: Vec<String> = doc.worksets.iter().map(|w| w.name.clone()).collect();
        let i = el.workset.and_then(|w| doc.worksets.iter().position(|x| x.id == w)).unwrap_or(0);
        p.push(Parameter::new("Workset", G::IdentityData, V::Choice(i)).kind(ParamKind::Choice(names)));
    }
    if !doc.option_sets.is_empty() {
        p.push(Parameter::new("Design Option", G::IdentityData, V::Text(doc.option_label(el.design_option))).ro());
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
    // Renaming a level renames its plans that still carry its name, as
    // Revit offers to.
    if let (Some(ElementData::Level(l)), "Name", V::Text(new)) = (doc.element(id).map(|e| &e.data), name, &value) {
        if !new.trim().is_empty() {
            doc.rename_level_views(id, &l.name.clone(), new);
        }
    }
    if let ("Workset", V::Choice(i)) = (name, &value) {
        let w = doc.worksets.get(*i).map(|w| w.id).ok_or_else(bad)?;
        doc.element_mut(id).ok_or(ModelError::NotFound(id))?.workset = Some(w);
        return Ok(());
    }
    // Phasing: an element cannot be demolished before it is created.
    if let ("Phase Created" | "Phase Demolished", V::Choice(i)) = (name, &value) {
        let el = doc.element(id).ok_or(ModelError::NotFound(id))?;
        let (mut created, mut demolished) = (
            doc.phase_index(el.phase_created),
            el.phase_demolished.map(|d| doc.phase_index(Some(d))),
        );
        if name == "Phase Created" {
            created = *i;
        } else {
            demolished = i.checked_sub(1);
        }
        if created >= doc.phases.len() || demolished.is_some_and(|d| d >= doc.phases.len()) {
            return Err(bad());
        }
        if demolished.is_some_and(|d| d < created) {
            return Err(ModelError::Rejected(
                "An element cannot be demolished before the phase it is created in.".into(),
            ));
        }
        let (c, d) = (doc.phases[created].id, demolished.map(|d| doc.phases[d].id));
        let el = doc.element_mut(id).ok_or(ModelError::NotFound(id))?;
        el.phase_created = Some(c);
        el.phase_demolished = d;
        return Ok(());
    }
    // Revision clouds of issued revisions are locked.
    if doc.cloud_locked(id) {
        return Err(ModelError::Rejected("This revision is issued: its clouds can't be changed.".into()));
    }
    let revision_ids: Vec<ElementId> = doc.revisions.iter().map(|r| r.id).collect();
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
            ElementData::Toposolid(t) => match (name, value) {
                ("Level", V::Element(l)) if l.is_valid() => t.level = l,
                ("Height Offset From Level", V::Length(v)) => t.height_offset = v,
                _ => return Err(bad()),
            },
            ElementData::PropertyLine(_) => return Err(bad()),
            ElementData::LinkInstance(i) => match (name, value) {
                ("Elevation Offset", V::Length(v)) => i.z = v,
                ("Rotation", V::Angle(v)) => i.rotation = v,
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
                    r.slope_arrow = None;
                }
                ("Slope Arrow", V::Bool(false)) => r.slope_arrow = None,
                ("Slope Arrow", V::Bool(true)) => {
                    if r.slope_arrow.is_none() {
                        r.slope_arrow = crate::element::default_slope_arrow(&r.footprint);
                        if r.slope <= 0.0 {
                            r.slope = 5.0;
                        }
                    }
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
            ElementData::DirectShape(d) => match (name, value) {
                ("Level", V::Element(l)) if l.is_valid() => d.level = l,
                _ => return Err(bad()),
            },
            ElementData::Component(c) => match (name, value) {
                ("Level", V::Element(l)) if l.is_valid() => c.level = l,
                ("Offset", V::Length(v)) => c.offset = v,
                ("Rotation", V::Angle(a)) => c.rotation = a,
                _ => return Err(bad()),
            },
            ElementData::Stair(s) => match (name, value) {
                ("Base Level", V::Element(l)) if l.is_valid() => s.base_level = l,
                ("Base Offset", V::Length(v)) => s.base_offset = v,
                ("Top Level", V::Element(l)) => s.top_level = l,
                ("Top Offset", V::Length(v)) => s.top_offset = v,
                ("Actual Run Width", V::Length(v)) if v > 0.0 => s.width = v,
                _ => return Err(bad()),
            },
            ElementData::Ceiling(c) => match (name, value) {
                ("Level", V::Element(l)) if l.is_valid() => c.level = l,
                ("Height Offset From Level", V::Length(v)) => c.height_offset = v,
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
                ("Department", V::Text(s)) => r.department = s,
                _ => return Err(bad()),
            },
            ElementData::Area(a) => match (name, value) {
                ("Number", V::Text(s)) => a.number = s,
                ("Name", V::Text(s)) => a.name = s,
                _ => return Err(bad()),
            },
            ElementData::TextNote(t) => match (name, value) {
                ("Text", V::Text(s)) => t.text = s,
                ("Text Size", V::Length(v)) if v > 0.0 => t.size = v,
                ("Leader", V::Bool(b)) => t.leader = b.then(|| t.leader.unwrap_or(t.position + DVec2::new(-1000.0, -600.0))),
                _ => return Err(bad()),
            },
            ElementData::Tag(t) => match (name, value) {
                ("Leader", V::Bool(b)) => t.leader = b,
                _ => return Err(bad()),
            },
            ElementData::RevisionCloud(c) => match (name, value) {
                ("Revision", V::Choice(i)) if i < revision_ids.len() => c.revision = revision_ids[i],
                ("Tag", V::Bool(b)) => c.tag = b.then(|| c.tag.unwrap_or_else(|| crate::revisions::default_tag_point(&c.boundary))),
                _ => return Err(bad()),
            },
            ElementData::DetailLine(l) => match (name, value) {
                ("Line Style", V::Choice(i)) if i < 4 => l.style = LineStyle::ALL[i],
                _ => return Err(bad()),
            },
            ElementData::FilledRegion(r) => match (name, value) {
                ("Fill Pattern", V::Choice(i)) if i < REGION_PATTERNS.len() => r.pattern = REGION_PATTERNS[i].1,
                ("Masking", V::Bool(b)) => r.masking = b,
                ("Line Style", V::Choice(i)) if i < LineStyle::ALL.len() => r.border = LineStyle::ALL[i],
                _ => return Err(bad()),
            },
            ElementData::RoomSeparation(_)
            | ElementData::Dimension(_)
            | ElementData::SpotElevation(_)
            | ElementData::Group(_)
            | ElementData::AreaBoundary(_) => return Err(bad()),
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
        // Zero is allowed where it means "none".
        if v <= 0.0 && !matches!(name, "Default Sill Height" | "Grid Spacing") {
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
