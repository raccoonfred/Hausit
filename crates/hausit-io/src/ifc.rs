//! IFC4 export (ISO 16739-1, written as an ISO 10303-21 STEP file).
//!
//! Follows the Reference View: site, building and a storey per level, then
//! every model element as its IFC class with a type, materials (layer sets
//! for compound walls, floors and roofs), colours, its parameters as
//! property sets, and openings for doors and windows. Rooms become spaces
//! and grids an `IfcGrid`. Lengths are millimetres.

use hausit_model::geometry::{self, Face, Material, Shape};
use hausit_model::materials::Layer;
use hausit_model::params::element_parameters;
use hausit_model::{Category, Document, ElementData, ElementId, FamilyType, FramingKind, ParamKind, ParamValue, RoofShape};
use hausit_model::{DVec2, DVec3};
use std::collections::{BTreeMap, HashMap};
use std::fmt::{self, Write};

pub const EXTENSION: &str = "ifc";

/// Writes the document as an IFC4 file. `timestamp` is seconds since the
/// Unix epoch, recorded as the file's creation time.
pub fn export(doc: &Document, timestamp: i64) -> String {
    let mut x = Exporter::new(doc, timestamp);
    x.write();
    x.finish()
}

/// A STEP instance reference, `#n`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Ref(u32);

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// A STEP string literal: quotes doubled, backslashes escaped, anything
/// outside printable ASCII as `\X2\` hex.
fn s(text: &str) -> String {
    let mut out = String::from("'");
    for c in text.chars() {
        match c {
            '\'' => out.push_str("''"),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            _ => {
                let mut buf = [0u16; 2];
                out.push_str("\\X2\\");
                for u in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "{u:04X}");
                }
                out.push_str("\\X0\\");
            }
        }
    }
    out.push('\'');
    out
}

/// A STEP real: always with a decimal point.
fn r(v: f64) -> String {
    if !v.is_finite() || v.abs() < 1e-9 {
        return "0.".into();
    }
    let t = format!("{v:.6}");
    let t = t.trim_end_matches('0');
    if t == "-0." {
        "0.".into()
    } else {
        t.into()
    }
}

fn list<T: fmt::Display>(items: &[T]) -> String {
    let parts: Vec<String> = items.iter().map(|i| i.to_string()).collect();
    format!("({})", parts.join(","))
}

fn b(v: bool) -> &'static str {
    if v {
        ".T."
    } else {
        ".F."
    }
}

/// A deterministic IFC GlobalId (22 characters of IFC's base 64) from a
/// seed and a key, so the same element keeps its id across exports.
fn guid(seed: u64, kind: &str, key: u64) -> String {
    const CHARS: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";
    let mut h = seed;
    for byte in kind.bytes() {
        h = (h ^ byte as u64).wrapping_mul(0x100_0000_01b3);
    }
    let mix = |mut z: u64| {
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    let hi = mix(h ^ key.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    let lo = mix(hi ^ key.rotate_left(17) ^ 0x5851_f42d_4c95_7f2d);
    let n = ((hi as u128) << 64) | lo as u128;
    let mut out = String::with_capacity(22);
    out.push(CHARS[(n >> 126) as usize] as char);
    for i in (0..21).rev() {
        out.push(CHARS[((n >> (i * 6)) & 63) as usize] as char);
    }
    out
}

/// A local frame for an element: origin, and the plan direction of its x
/// axis (z stays vertical).
#[derive(Clone, Copy)]
struct Frame {
    origin: DVec3,
    x: DVec2,
}

impl Frame {
    const WORLD: Frame = Frame {
        origin: DVec3::ZERO,
        x: DVec2::X,
    };

    fn local(&self, p: DVec3) -> DVec3 {
        let d = p - self.origin;
        let y = DVec2::new(-self.x.y, self.x.x);
        DVec3::new(d.x * self.x.x + d.y * self.x.y, d.x * y.x + d.y * y.y, d.z)
    }

    fn local2(&self, p: DVec2) -> DVec2 {
        self.local(p.extend(self.origin.z)).truncate()
    }
}

/// IFC classes Hausit's categories map to: the occurrence, its type and
/// their predefined types.
struct Class {
    entity: &'static str,
    type_entity: &'static str,
    predefined: &'static str,
    type_predefined: &'static str,
    pset: &'static str,
}

fn class_of(doc: &Document, id: ElementId) -> Option<Class> {
    let e = doc.element(id)?;
    let c = |entity, type_entity, predefined, type_predefined, pset| Class {
        entity,
        type_entity,
        predefined,
        type_predefined,
        pset,
    };
    Some(match &e.data {
        ElementData::Wall(_) => c("IFCWALL", "IFCWALLTYPE", "SOLIDWALL", "SOLIDWALL", "Pset_WallCommon"),
        ElementData::Floor(_) => c("IFCSLAB", "IFCSLABTYPE", "FLOOR", "FLOOR", "Pset_SlabCommon"),
        ElementData::Roof(roof) => {
            let shape = match roof.shape {
                _ if !roof.is_sloped() => "FLAT_ROOF",
                RoofShape::Flat => "FLAT_ROOF",
                RoofShape::Hip => "HIP_ROOF",
                RoofShape::Gable => "GABLE_ROOF",
            };
            c("IFCROOF", "IFCROOFTYPE", shape, "NOTDEFINED", "Pset_RoofCommon")
        }
        ElementData::Door(_) => c("IFCDOOR", "IFCDOORTYPE", "DOOR", "DOOR", "Pset_DoorCommon"),
        ElementData::Window(_) => c("IFCWINDOW", "IFCWINDOWTYPE", "WINDOW", "WINDOW", "Pset_WindowCommon"),
        ElementData::Column(_) => c("IFCCOLUMN", "IFCCOLUMNTYPE", "COLUMN", "COLUMN", "Pset_ColumnCommon"),
        ElementData::Framing(f) if f.kind == FramingKind::Brace => c("IFCMEMBER", "IFCMEMBERTYPE", "BRACE", "BRACE", "Pset_MemberCommon"),
        ElementData::Framing(_) => c("IFCBEAM", "IFCBEAMTYPE", "BEAM", "BEAM", "Pset_BeamCommon"),
        ElementData::Footing(_) => c("IFCFOOTING", "IFCFOOTINGTYPE", "PAD_FOOTING", "PAD_FOOTING", "Pset_FootingCommon"),
        ElementData::WallFooting(_) => c(
            "IFCFOOTING",
            "IFCFOOTINGTYPE",
            "STRIP_FOOTING",
            "STRIP_FOOTING",
            "Pset_FootingCommon",
        ),
        ElementData::Stair(st) => {
            let shape = if st.path.len() == 2 { "STRAIGHT_RUN_STAIR" } else { "NOTDEFINED" };
            c("IFCSTAIR", "IFCSTAIRTYPE", shape, "NOTDEFINED", "Pset_StairCommon")
        }
        ElementData::Railing(rl) => {
            let kind = if rl.host.is_valid() { "HANDRAIL" } else { "GUARDRAIL" };
            c("IFCRAILING", "IFCRAILINGTYPE", kind, kind, "Pset_RailingCommon")
        }
        ElementData::Ceiling(_) => c("IFCCOVERING", "IFCCOVERINGTYPE", "CEILING", "CEILING", "Pset_CoveringCommon"),
        ElementData::Component(comp) => match comp.category {
            Category::Furniture | Category::Casework => c("IFCFURNITURE", "IFCFURNITURETYPE", "NOTDEFINED", "NOTDEFINED", ""),
            Category::PlumbingFixtures => {
                let family = doc.type_of(id).map(|t| t.family.to_ascii_lowercase()).unwrap_or_default();
                let kind = [
                    ("wc", "TOILETPAN"),
                    ("toilet", "TOILETPAN"),
                    ("basin", "WASHHANDBASIN"),
                    ("bath", "BATH"),
                    ("shower", "SHOWER"),
                    ("sink", "SINK"),
                ]
                .iter()
                .find(|(k, _)| family.contains(k))
                .map(|(_, v)| *v)
                .unwrap_or("NOTDEFINED");
                c("IFCSANITARYTERMINAL", "IFCSANITARYTERMINALTYPE", kind, kind, "")
            }
            _ => c(
                "IFCBUILDINGELEMENTPROXY",
                "IFCBUILDINGELEMENTPROXYTYPE",
                "NOTDEFINED",
                "NOTDEFINED",
                "Pset_BuildingElementProxyCommon",
            ),
        },
        _ => return None,
    })
}

/// The level an element belongs to.
fn element_level(doc: &Document, id: ElementId) -> ElementId {
    let Some(e) = doc.element(id) else {
        return ElementId::INVALID;
    };
    match &e.data {
        ElementData::Wall(w) => w.base_level,
        ElementData::Floor(f) => f.level,
        ElementData::Roof(r) => r.level,
        ElementData::Door(o) | ElementData::Window(o) => element_level(doc, o.host),
        ElementData::Column(c) => c.base_level,
        ElementData::Framing(f) => f.level,
        ElementData::Footing(f) => f.level,
        ElementData::WallFooting(f) => element_level(doc, f.wall),
        ElementData::Stair(s) => s.base_level,
        ElementData::Railing(r) => r.level,
        ElementData::Ceiling(c) => c.level,
        ElementData::Component(c) => c.level,
        ElementData::Room(r) => r.level,
        _ => ElementId::INVALID,
    }
}

/// The material a type is made of, when it has no layers.
fn type_material(t: &FamilyType) -> Option<&str> {
    ["Material", "Structural Material"].iter().find_map(|k| match t.params.get(*k) {
        Some(ParamValue::Text(name)) if !name.is_empty() => Some(name.as_str()),
        _ => None,
    })
}

struct Exporter<'a> {
    doc: &'a Document,
    timestamp: i64,
    seed: u64,
    out: String,
    next: u32,
    owner: Ref,
    body: Ref,
    axis: Ref,
    footprint: Ref,
    z: Ref,
    x: Ref,
    storeys: Vec<(ElementId, f64, Ref, Ref)>,
    contained: BTreeMap<Ref, Vec<Ref>>,
    spaces: BTreeMap<Ref, Vec<Ref>>,
    materials: HashMap<String, Ref>,
    styles: HashMap<String, Ref>,
    layer_sets: HashMap<ElementId, Ref>,
    /// Types: (type id, entity) → the type object and its occurrences.
    types: BTreeMap<(ElementId, &'static str), (Ref, Vec<Ref>)>,
    /// Material associations: material (or layer set usage) → elements.
    associations: BTreeMap<Ref, Vec<Ref>>,
    /// Layer set usages of wall types.
    wall_usages: HashMap<ElementId, Ref>,
    /// Exported elements: id → the IFC object.
    written: HashMap<ElementId, Ref>,
    /// Doors and windows, for their openings: id, object, storey
    /// placement, frame relative to the storey.
    fillers: Vec<(ElementId, Ref, Ref, Frame)>,
}

impl<'a> Exporter<'a> {
    fn new(doc: &'a Document, timestamp: i64) -> Self {
        let info = &doc.project_info;
        let mut seed = 0xcbf2_9ce4_8422_2325u64;
        for byte in format!("{}|{}", info.name, info.number).bytes() {
            seed = (seed ^ byte as u64).wrapping_mul(0x100_0000_01b3);
        }
        Exporter {
            doc,
            timestamp,
            seed,
            out: String::new(),
            next: 1,
            owner: Ref(0),
            body: Ref(0),
            axis: Ref(0),
            footprint: Ref(0),
            z: Ref(0),
            x: Ref(0),
            storeys: Vec::new(),
            contained: BTreeMap::new(),
            spaces: BTreeMap::new(),
            materials: HashMap::new(),
            styles: HashMap::new(),
            layer_sets: HashMap::new(),
            types: BTreeMap::new(),
            associations: BTreeMap::new(),
            wall_usages: HashMap::new(),
            written: HashMap::new(),
            fillers: Vec::new(),
        }
    }

    fn add(&mut self, entity: &str, args: impl AsRef<str>) -> Ref {
        let id = Ref(self.next);
        self.next += 1;
        let _ = writeln!(self.out, "{id}={entity}({});", args.as_ref());
        id
    }

    fn guid(&self, kind: &str, key: u64) -> String {
        s(&guid(self.seed, kind, key))
    }

    /// GlobalId and OwnerHistory, the first two attributes of every rooted
    /// entity.
    fn root(&self, kind: &str, key: u64) -> String {
        format!("{},{}", self.guid(kind, key), self.owner)
    }

    fn point(&mut self, p: DVec3) -> Ref {
        self.add("IFCCARTESIANPOINT", format!("({},{},{})", r(p.x), r(p.y), r(p.z)))
    }

    fn point2(&mut self, p: DVec2) -> Ref {
        self.add("IFCCARTESIANPOINT", format!("({},{})", r(p.x), r(p.y)))
    }

    fn direction(&mut self, d: DVec3) -> Ref {
        self.add("IFCDIRECTION", format!("({},{},{})", r(d.x), r(d.y), r(d.z)))
    }

    fn placement3(&mut self, origin: DVec3, x: Option<DVec2>) -> Ref {
        let p = self.point(origin);
        match x {
            Some(x) if (x - DVec2::X).length() > 1e-9 => {
                let xd = self.direction(x.extend(0.0));
                let z = self.z;
                self.add("IFCAXIS2PLACEMENT3D", format!("{p},{z},{xd}"))
            }
            _ => self.add("IFCAXIS2PLACEMENT3D", format!("{p},$,$")),
        }
    }

    fn local_placement(&mut self, parent: Option<Ref>, origin: DVec3, x: Option<DVec2>) -> Ref {
        let a = self.placement3(origin, x);
        let parent = parent.map(|p| p.to_string()).unwrap_or_else(|| "$".into());
        self.add("IFCLOCALPLACEMENT", format!("{parent},{a}"))
    }

    fn write(&mut self) {
        self.header_entities();
        let site_placement = self.local_placement(None, DVec3::ZERO, None);
        let building_placement = self.local_placement(Some(site_placement), DVec3::ZERO, None);
        let project = self.project();
        let info = &self.doc.project_info;
        let site = self.add(
            "IFCSITE",
            format!(
                "{},{},$,$,{site_placement},$,$,.ELEMENT.,$,$,$,$,$",
                self.root("site", 0),
                s("Default Site")
            ),
        );
        let address = if info.address.trim().is_empty() {
            "$".to_string()
        } else {
            let lines: Vec<String> = info.address.lines().filter(|l| !l.trim().is_empty()).map(s).collect();
            self.add("IFCPOSTALADDRESS", format!("$,$,$,$,{},$,$,$,$,$", list(&lines)))
                .to_string()
        };
        let building_name = if info.name.is_empty() {
            "Building".to_string()
        } else {
            info.name.clone()
        };
        let building = self.add(
            "IFCBUILDING",
            format!(
                "{},{},$,$,{building_placement},$,$,.ELEMENT.,$,$,{address}",
                self.root("building", 0),
                s(&building_name)
            ),
        );
        self.add(
            "IFCRELAGGREGATES",
            format!("{},$,$,{project},({site})", self.root("agg-project", 0)),
        );
        self.add("IFCRELAGGREGATES", format!("{},$,$,{site},({building})", self.root("agg-site", 0)));

        // A storey per level that is a building story (all levels if none
        // is marked).
        let levels = self.doc.levels();
        let any_story = levels.iter().any(|(_, l)| l.building_story);
        let mut storey_refs = Vec::new();
        for (id, l) in levels.iter().filter(|(_, l)| l.building_story || !any_story) {
            let placement = self.local_placement(Some(building_placement), DVec3::new(0.0, 0.0, l.elevation), None);
            let storey = self.add(
                "IFCBUILDINGSTOREY",
                format!(
                    "{},{},$,$,{placement},$,$,.ELEMENT.,{}",
                    self.root("storey", id.0),
                    s(&l.name),
                    r(l.elevation)
                ),
            );
            self.storeys.push((*id, l.elevation, storey, placement));
            storey_refs.push(storey);
        }
        if storey_refs.is_empty() {
            let placement = self.local_placement(Some(building_placement), DVec3::ZERO, None);
            let storey = self.add(
                "IFCBUILDINGSTOREY",
                format!("{},{},$,$,{placement},$,$,.ELEMENT.,0.", self.root("storey", 0), s("Level 1")),
            );
            self.storeys.push((ElementId::INVALID, 0.0, storey, placement));
            storey_refs.push(storey);
        }
        self.add(
            "IFCRELAGGREGATES",
            format!("{},$,$,{building},{}", self.root("agg-building", 0), list(&storey_refs)),
        );

        let mut ids: Vec<ElementId> = self.doc.elements().map(|e| e.id).collect();
        ids.sort();
        for id in &ids {
            self.element(*id);
        }
        for id in &ids {
            if let Some(ElementData::Room(_)) = self.doc.element(*id).map(|e| &e.data) {
                self.space(*id);
            }
        }
        self.openings();
        self.grids();
        self.relationships();
    }

    fn header_entities(&mut self) {
        let info = &self.doc.project_info;
        let author = if info.author.is_empty() { "Unknown" } else { &info.author };
        let person = self.add("IFCPERSON", format!("$,{},$,$,$,$,$,$", s(author)));
        let org_name = if info.client.is_empty() { "Unknown" } else { &info.client };
        let org = self.add("IFCORGANIZATION", format!("$,{},$,$,$", s(org_name)));
        let user = self.add("IFCPERSONANDORGANIZATION", format!("{person},{org},$"));
        let dev = self.add("IFCORGANIZATION", format!("$,{},$,$,$", s("Hausit contributors")));
        let app = self.add(
            "IFCAPPLICATION",
            format!("{dev},{},{},{}", s(env!("CARGO_PKG_VERSION")), s("Hausit"), s("Hausit")),
        );
        self.owner = self.add("IFCOWNERHISTORY", format!("{user},{app},$,.ADDED.,{0},$,$,{0}", self.timestamp));
        self.z = self.direction(DVec3::Z);
        self.x = self.direction(DVec3::X);
    }

    fn project(&mut self) -> Ref {
        let length = self.add("IFCSIUNIT", "*,.LENGTHUNIT.,.MILLI.,.METRE.");
        let area = self.add("IFCSIUNIT", "*,.AREAUNIT.,$,.SQUARE_METRE.");
        let volume = self.add("IFCSIUNIT", "*,.VOLUMEUNIT.,$,.CUBIC_METRE.");
        let angle = self.add("IFCSIUNIT", "*,.PLANEANGLEUNIT.,$,.RADIAN.");
        let units = self.add("IFCUNITASSIGNMENT", format!("({length},{area},{volume},{angle})"));
        let origin = self.point(DVec3::ZERO);
        let (z, x) = (self.z, self.x);
        let wcs = self.add("IFCAXIS2PLACEMENT3D", format!("{origin},{z},{x}"));
        let north = self.add("IFCDIRECTION", "(0.,1.)");
        let context = self.add("IFCGEOMETRICREPRESENTATIONCONTEXT", format!("$,'Model',3,1.E-05,{wcs},{north}"));
        self.body = self.add(
            "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
            format!("'Body','Model',*,*,*,*,{context},$,.MODEL_VIEW.,$"),
        );
        self.axis = self.add(
            "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
            format!("'Axis','Model',*,*,*,*,{context},$,.GRAPH_VIEW.,$"),
        );
        self.footprint = self.add(
            "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
            format!("'FootPrint','Model',*,*,*,*,{context},$,.SKETCH_VIEW.,$"),
        );
        let info = &self.doc.project_info;
        let name = if info.number.is_empty() {
            info.name.clone()
        } else {
            info.number.clone()
        };
        let phase = if info.status.is_empty() { "$".into() } else { s(&info.status) };
        self.add(
            "IFCPROJECT",
            format!(
                "{},{},$,$,{},{phase},({context}),{units}",
                self.root("project", 0),
                s(if name.is_empty() { "Hausit Project" } else { &name }),
                s(&info.name)
            ),
        )
    }

    /// The storey an element goes in: its level's storey, or the nearest
    /// storey below it.
    fn storey_for(&self, level: ElementId) -> (Ref, Ref, f64) {
        let elev = self.doc.level_elevation(level);
        let pick = self
            .storeys
            .iter()
            .find(|s| s.0 == level)
            .or_else(|| self.storeys.iter().rev().find(|s| s.1 <= elev + 1e-6))
            .or(self.storeys.first())
            .expect("there is always a storey");
        (pick.2, pick.3, pick.1)
    }

    /// The element's local frame, in model coordinates.
    fn frame_of(&self, id: ElementId, storey_elev: f64) -> Frame {
        let doc = self.doc;
        match doc.element(id).map(|e| &e.data) {
            Some(ElementData::Wall(w)) if w.length() > 1e-6 => Frame {
                origin: w.start.extend(doc.wall_base_z(w)),
                x: w.direction(),
            },
            Some(ElementData::Door(_) | ElementData::Window(_)) => match geometry::opening_frame(doc, id) {
                Some(f) => Frame {
                    origin: f.center.extend(f.z0),
                    x: f.dir,
                },
                None => Frame::WORLD,
            },
            Some(ElementData::Component(c)) => Frame {
                origin: c.point.extend(doc.level_elevation(c.level) + c.offset),
                x: DVec2::from_angle(c.rotation.to_radians()),
            },
            _ => Frame {
                origin: DVec3::new(0.0, 0.0, storey_elev),
                x: DVec2::X,
            },
        }
    }

    fn surface_style(&mut self, key: &str, color: [u8; 3], transparency: f64) -> Ref {
        if let Some(r) = self.styles.get(key) {
            return *r;
        }
        let [cr, cg, cb] = color.map(|c| c as f64 / 255.0);
        let colour = self.add("IFCCOLOURRGB", format!("$,{},{},{}", r(cr), r(cg), r(cb)));
        let shading = self.add("IFCSURFACESTYLESHADING", format!("{colour},{}", r(transparency)));
        let style = self.add("IFCSURFACESTYLE", format!("{},.BOTH.,({shading})", s(key)));
        self.styles.insert(key.to_string(), style);
        style
    }

    /// The surface style of an element's part: glass, or the colour of
    /// its outer layer or material.
    fn part_style(&mut self, t: Option<&FamilyType>, m: Material) -> Ref {
        if m == Material::Glass {
            return self.surface_style("Glass", [170, 205, 225], 0.6);
        }
        let name = t.and_then(|t| t.layers.first().map(|l| l.material.as_str()).or(type_material(t)));
        match name.and_then(|n| self.doc.material(n)) {
            Some(mat) if m != Material::Panel => {
                let (key, color) = (mat.name.clone(), mat.color);
                self.surface_style(&key, color, 0.0)
            }
            _ if m == Material::Panel => self.surface_style("Door Panel", [196, 164, 120], 0.0),
            _ => self.surface_style("Default", [200, 200, 200], 0.0),
        }
    }

    fn material(&mut self, name: &str) -> Ref {
        if let Some(r) = self.materials.get(name) {
            return *r;
        }
        let m = self.add("IFCMATERIAL", format!("{},$,$", s(name)));
        // The material's colour, for viewers that colour by material.
        if let Some(color) = self.doc.material(name).map(|m| m.color) {
            let style = self.surface_style(name, color, 0.0);
            let item = self.add("IFCSTYLEDITEM", format!("$,({style}),$"));
            let rep = self.add("IFCSTYLEDREPRESENTATION", format!("{},'Style','Material',({item})", self.body));
            self.add("IFCMATERIALDEFINITIONREPRESENTATION", format!("$,$,({rep}),{m}"));
        }
        self.materials.insert(name.to_string(), m);
        m
    }

    fn layer_set(&mut self, t: &FamilyType) -> Ref {
        if let Some(r) = self.layer_sets.get(&t.id) {
            return *r;
        }
        let layers: Vec<Layer> = t.layers.clone();
        let mut refs = Vec::new();
        for l in &layers {
            let m = self.material(&l.material);
            refs.push(self.add(
                "IFCMATERIALLAYER",
                format!("{m},{},$,{},$,{},$", r(l.thickness), s(l.function.label()), s(l.function.label())),
            ));
        }
        let set = self.add("IFCMATERIALLAYERSET", format!("{},{},$", list(&refs), s(&t.name)));
        self.layer_sets.insert(t.id, set);
        set
    }

    /// Representation items for the parts, in the element's frame.
    fn items(&mut self, parts: &[geometry::Part], frame: Frame, t: Option<&FamilyType>) -> (Vec<Ref>, &'static str) {
        let mut items = Vec::new();
        let (mut swept, mut brep) = (false, false);
        for part in parts {
            let item = match &part.shape {
                Shape::Prism(p) if p.footprint.len() >= 3 && p.z1 - p.z0 > 1e-6 => {
                    swept = true;
                    let mut pts: Vec<Ref> = p.footprint.iter().map(|q| frame.local2(*q)).map(|q| self.point2(q)).collect();
                    pts.push(pts[0]);
                    let line = self.add("IFCPOLYLINE", list(&pts));
                    let profile = self.add("IFCARBITRARYCLOSEDPROFILEDEF", format!(".AREA.,$,{line}"));
                    let at = self.placement3(DVec3::new(0.0, 0.0, p.z0 - frame.origin.z), None);
                    let z = self.z;
                    self.add("IFCEXTRUDEDAREASOLID", format!("{profile},{at},{z},{}", r(p.z1 - p.z0)))
                }
                Shape::Prism(_) => continue,
                Shape::Faces(faces) => {
                    brep = true;
                    match self.brep(faces, frame) {
                        Some(b) => b,
                        None => continue,
                    }
                }
            };
            let style = self.part_style(t, part.material);
            self.add("IFCSTYLEDITEM", format!("{item},({style}),$"));
            items.push(item);
        }
        let kind = match (swept, brep) {
            (true, false) => "SweptSolid",
            (false, true) => "Brep",
            _ => "SolidModel",
        };
        (items, kind)
    }

    fn brep(&mut self, faces: &[Face], frame: Frame) -> Option<Ref> {
        let mut cache: HashMap<[i64; 3], Ref> = HashMap::new();
        let mut face_refs = Vec::new();
        for f in faces {
            let mut loop_pts: Vec<Ref> = Vec::new();
            for p in &f.pts {
                let q = frame.local(*p);
                let key = [q.x, q.y, q.z].map(|v| (v * 1000.0).round() as i64);
                let pt = match cache.get(&key) {
                    Some(r) => *r,
                    None => {
                        let r = self.point(q);
                        cache.insert(key, r);
                        r
                    }
                };
                if loop_pts.last() != Some(&pt) {
                    loop_pts.push(pt);
                }
            }
            while loop_pts.len() > 1 && loop_pts.first() == loop_pts.last() {
                loop_pts.pop();
            }
            if loop_pts.len() < 3 {
                continue;
            }
            let poly = self.add("IFCPOLYLOOP", list(&loop_pts));
            let bound = self.add("IFCFACEOUTERBOUND", format!("{poly},.T."));
            face_refs.push(self.add("IFCFACE", format!("({bound})")));
        }
        if face_refs.len() < 4 {
            return None;
        }
        let shell = self.add("IFCCLOSEDSHELL", list(&face_refs));
        Some(self.add("IFCFACETEDBREP", shell.to_string()))
    }

    fn element(&mut self, id: ElementId) {
        let doc = self.doc;
        let Some(class) = class_of(doc, id) else { return };
        let Some(g) = geometry::element_geometry(doc, id) else { return };
        let Some(e) = doc.element(id) else { return };
        let (storey, storey_placement, storey_elev) = self.storey_for(element_level(doc, id));
        let frame = self.frame_of(id, storey_elev);
        let t = doc.type_of(id);
        let (items, kind) = self.items(&g.parts, frame, t);
        if items.is_empty() {
            return;
        }
        let mut reps = Vec::new();
        if let ElementData::Wall(w) = &e.data {
            let a = self.point2(DVec2::ZERO);
            let b = self.point2(DVec2::new(w.length(), 0.0));
            let line = self.add("IFCPOLYLINE", format!("({a},{b})"));
            reps.push(self.add("IFCSHAPEREPRESENTATION", format!("{},'Axis','Curve2D',({line})", self.axis)));
        }
        reps.push(self.add(
            "IFCSHAPEREPRESENTATION",
            format!("{},'Body',{},{}", self.body, s(kind), list(&items)),
        ));
        let shape = self.add("IFCPRODUCTDEFINITIONSHAPE", format!("$,$,{}", list(&reps)));
        let relative = Frame {
            origin: frame.origin - DVec3::new(0.0, 0.0, storey_elev),
            x: frame.x,
        };
        let placement = self.local_placement(Some(storey_placement), relative.origin, Some(frame.x));

        let type_name = t.map(|t| t.name.clone()).unwrap_or_default();
        let name = match t {
            Some(t) => format!("{}:{}:{}", t.family, t.name, id.0),
            None => format!("{}:{}", g.category.name(), id.0),
        };
        let head = format!(
            "{},{},$,{},{placement},{shape},{}",
            self.root("element", id.0),
            s(&name),
            s(&type_name),
            s(&id.0.to_string())
        );
        let obj = match &e.data {
            ElementData::Door(o) => {
                let operation = match t.and_then(|t| t.params.get("Leaves")) {
                    Some(ParamValue::Integer(2)) => "DOUBLE_DOOR_SINGLE_SWING",
                    // Doors without swinging leaves, such as sectional overhead doors.
                    Some(ParamValue::Integer(0)) => "ROLLINGUP",
                    _ if o.flip_hand => "SINGLE_SWING_RIGHT",
                    _ => "SINGLE_SWING_LEFT",
                };
                let (h, w) = t.map(|t| (t.height(), t.width())).unwrap_or((2100.0, 900.0));
                self.add(class.entity, format!("{head},{},{},.DOOR.,.{operation}.,$", r(h), r(w)))
            }
            ElementData::Window(_) => {
                let (h, w) = t.map(|t| (t.height(), t.width())).unwrap_or((1200.0, 900.0));
                self.add(class.entity, format!("{head},{},{},.WINDOW.,.SINGLE_PANEL.,$", r(h), r(w)))
            }
            _ => self.add(class.entity, format!("{head},.{}.", class.predefined)),
        };
        self.contained.entry(storey).or_default().push(obj);
        self.written.insert(id, obj);
        if matches!(e.data, ElementData::Door(_) | ElementData::Window(_)) {
            self.fillers.push((id, obj, storey_placement, relative));
        }
        if let Some(t) = t {
            self.type_object(t, &class, obj);
            self.element_material(t, obj, &e.data, frame.origin.z - storey_elev, storey_elev);
        }
        self.properties(id, obj, &class, t);
    }

    /// Openings in the host walls for doors and windows, each filled by
    /// its door or window.
    fn openings(&mut self) {
        for (id, filler, storey_placement, frame) in std::mem::take(&mut self.fillers) {
            let doc = self.doc;
            let (Some(f), Some(host)) = (
                geometry::opening_frame(doc, id),
                doc.element(id).and_then(|e| e.as_opening()).map(|o| o.host),
            ) else {
                continue;
            };
            let Some(wall) = self.written.get(&host).copied() else { continue };
            let (hw, ht) = (f.width * 0.5, f.wall_thickness * 0.5 + 20.0);
            let corners = [(-hw, -ht), (hw, -ht), (hw, ht), (-hw, ht)];
            let mut pts: Vec<Ref> = corners.iter().map(|(x, y)| self.point2(DVec2::new(*x, *y))).collect();
            pts.push(pts[0]);
            let line = self.add("IFCPOLYLINE", list(&pts));
            let profile = self.add("IFCARBITRARYCLOSEDPROFILEDEF", format!(".AREA.,$,{line}"));
            let at = self.placement3(DVec3::ZERO, None);
            let z = self.z;
            let solid = self.add("IFCEXTRUDEDAREASOLID", format!("{profile},{at},{z},{}", r(f.height)));
            let rep = self.add("IFCSHAPEREPRESENTATION", format!("{},'Body','SweptSolid',({solid})", self.body));
            let shape = self.add("IFCPRODUCTDEFINITIONSHAPE", format!("$,$,({rep})"));
            let placement = self.local_placement(Some(storey_placement), frame.origin, Some(frame.x));
            let opening = self.add(
                "IFCOPENINGELEMENT",
                format!(
                    "{},{},$,$,{placement},{shape},$,.OPENING.",
                    self.root("opening", id.0),
                    s(&format!("Opening:{}", id.0))
                ),
            );
            self.add("IFCRELVOIDSELEMENT", format!("{},$,$,{wall},{opening}", self.root("voids", id.0)));
            self.add("IFCRELFILLSELEMENT", format!("{},$,$,{opening},{filler}", self.root("fills", id.0)));
        }
    }

    fn type_object(&mut self, t: &FamilyType, class: &Class, obj: Ref) {
        let key = (t.id, class.type_entity);
        if let Some((_, occ)) = self.types.get_mut(&key) {
            occ.push(obj);
            return;
        }
        let props: Vec<(String, String)> = t
            .params
            .iter()
            .filter_map(|(k, v)| value(self.doc, v, None).map(|v| (k.clone(), v)))
            .collect();
        let psets = match self.property_set("type", t.id.0, "Hausit_Type", &props) {
            Some(p) => format!("({p})"),
            None => "$".into(),
        };
        let head = format!(
            "{},{},$,$,{psets},$,$,{}",
            self.root(class.type_entity, t.id.0),
            s(&t.name),
            s(&t.family)
        );
        let args = match class.type_entity {
            "IFCDOORTYPE" => format!("{head},.DOOR.,.NOTDEFINED.,$,$"),
            "IFCWINDOWTYPE" => format!("{head},.WINDOW.,.NOTDEFINED.,$,$"),
            // AssemblyPlace, then the predefined type.
            "IFCFURNITURETYPE" => format!("{head},.NOTDEFINED.,.{}.", class.type_predefined),
            _ => format!("{head},.{}.", class.type_predefined),
        };
        let type_ref = self.add(class.type_entity, args);
        self.types.insert(key, (type_ref, vec![obj]));
        // The type carries the material too.
        if !t.layers.is_empty() {
            let set = self.layer_set(t);
            self.associations.entry(set).or_default().push(type_ref);
        } else if let Some(name) = type_material(t) {
            let m = self.material(name);
            self.associations.entry(m).or_default().push(type_ref);
        }
    }

    /// Associates the element with its materials: a layer set usage for
    /// compound walls and floors, the layer set for roofs and ceilings, or
    /// the type's single material.
    fn element_material(&mut self, t: &FamilyType, obj: Ref, data: &ElementData, frame_z: f64, storey_elev: f64) {
        if t.layers.is_empty() {
            if let Some(name) = type_material(t) {
                let m = self.material(name);
                self.associations.entry(m).or_default().push(obj);
            }
            return;
        }
        let set = self.layer_set(t);
        let target = match data {
            // Layers run from the exterior face (local +y) inwards.
            ElementData::Wall(_) => match self.wall_usages.get(&t.id) {
                Some(u) => *u,
                None => {
                    let u = self.add(
                        "IFCMATERIALLAYERSETUSAGE",
                        format!("{set},.AXIS2.,.NEGATIVE.,{},$", r(t.thickness() * 0.5)),
                    );
                    self.wall_usages.insert(t.id, u);
                    u
                }
            },
            // Layers run down from the top of the slab.
            ElementData::Floor(f) => {
                let top = self.doc.level_elevation(f.level) + f.height_offset - storey_elev - frame_z;
                self.add("IFCMATERIALLAYERSETUSAGE", format!("{set},.AXIS3.,.NEGATIVE.,{},$", r(top)))
            }
            _ => set,
        };
        self.associations.entry(target).or_default().push(obj);
    }

    fn property_set(&mut self, kind: &str, key: u64, name: &str, props: &[(String, String)]) -> Option<Ref> {
        if props.is_empty() {
            return None;
        }
        let refs: Vec<Ref> = props
            .iter()
            .map(|(n, v)| self.add("IFCPROPERTYSINGLEVALUE", format!("{},$,{v},$", s(n))))
            .collect();
        Some(self.add(
            "IFCPROPERTYSET",
            format!("{},{},$,{}", self.root(&format!("pset-{kind}-{name}"), key), s(name), list(&refs)),
        ))
    }

    fn relate_properties(&mut self, kind: &str, key: u64, obj: Ref, pset: Option<Ref>) {
        if let Some(p) = pset {
            self.add(
                "IFCRELDEFINESBYPROPERTIES",
                format!("{},$,$,({obj}),{p}", self.root(&format!("props-{kind}"), key)),
            );
        }
    }

    /// The common property set of the element's class, and all its
    /// parameters as "Hausit_Parameters".
    fn properties(&mut self, id: ElementId, obj: Ref, class: &Class, t: Option<&FamilyType>) {
        let doc = self.doc;
        let mut common = Vec::new();
        if let Some(t) = t {
            common.push(("Reference".to_string(), format!("IFCIDENTIFIER({})", s(&t.name))));
        }
        let data = doc.element(id).map(|e| &e.data);
        let load_bearing = match data {
            Some(ElementData::Wall(w)) => Some(w.structural),
            Some(ElementData::Floor(f)) => Some(f.structural),
            Some(ElementData::Column(_) | ElementData::Framing(_)) => Some(true),
            Some(ElementData::Footing(_) | ElementData::WallFooting(_)) => Some(true),
            _ => None,
        };
        if let Some(lb) = load_bearing {
            common.push(("LoadBearing".to_string(), format!("IFCBOOLEAN({})", b(lb))));
        }
        let external = match data {
            Some(ElementData::Wall(_)) => {
                Some(matches!(t.and_then(|t| t.params.get("Function")), Some(ParamValue::Text(f)) if f == "Exterior"))
            }
            Some(ElementData::Roof(_)) => Some(true),
            Some(ElementData::Door(o) | ElementData::Window(o)) => {
                let host_type = doc.type_of(o.host);
                Some(matches!(host_type.and_then(|t| t.params.get("Function")), Some(ParamValue::Text(f)) if f == "Exterior"))
            }
            _ => None,
        };
        if let Some(ext) = external {
            common.push(("IsExternal".to_string(), format!("IFCBOOLEAN({})", b(ext))));
        }
        if !class.pset.is_empty() {
            let pset = self.property_set("common", id.0, class.pset, &common);
            self.relate_properties("common", id.0, obj, pset);
        }

        let params: Vec<(String, String)> = element_parameters(doc, id)
            .into_iter()
            .filter_map(|p| value(doc, &p.value, Some(&p.kind)).map(|v| (p.name.to_string(), v)))
            .collect();
        let pset = self.property_set("params", id.0, "Hausit_Parameters", &params);
        self.relate_properties("params", id.0, obj, pset);
    }

    fn space(&mut self, id: ElementId) {
        let doc = self.doc;
        let Some(ElementData::Room(room)) = doc.element(id).map(|e| &e.data) else {
            return;
        };
        let info = hausit_model::rooms::room_info(doc, room);
        let Some(boundary) = info.boundary.clone() else { return };
        if boundary.len() < 3 {
            return;
        }
        let (storey, storey_placement, storey_elev) = self.storey_for(room.level);
        let base = doc.level_elevation(room.level) - storey_elev;
        let height = if info.height > 1.0 { info.height } else { 2500.0 };
        let mut pts: Vec<Ref> = boundary.iter().map(|p| self.point2(*p)).collect();
        pts.push(pts[0]);
        let line = self.add("IFCPOLYLINE", list(&pts));
        let profile = self.add("IFCARBITRARYCLOSEDPROFILEDEF", format!(".AREA.,$,{line}"));
        let at = self.placement3(DVec3::ZERO, None);
        let z = self.z;
        let solid = self.add("IFCEXTRUDEDAREASOLID", format!("{profile},{at},{z},{}", r(height)));
        let style = self.surface_style("Room", [120, 170, 230], 0.85);
        self.add("IFCSTYLEDITEM", format!("{solid},({style}),$"));
        let rep = self.add("IFCSHAPEREPRESENTATION", format!("{},'Body','SweptSolid',({solid})", self.body));
        let shape = self.add("IFCPRODUCTDEFINITIONSHAPE", format!("$,$,({rep})"));
        let placement = self.local_placement(Some(storey_placement), DVec3::new(0.0, 0.0, base), None);
        let space = self.add(
            "IFCSPACE",
            format!(
                "{},{},$,$,{placement},{shape},{},.ELEMENT.,.INTERNAL.,$",
                self.root("space", id.0),
                s(&room.number),
                s(&room.name)
            ),
        );
        self.spaces.entry(storey).or_default().push(space);
        let area = self.add("IFCQUANTITYAREA", format!("'NetFloorArea',$,$,{},$", r(info.area / 1e6)));
        let perimeter = self.add("IFCQUANTITYLENGTH", format!("'GrossPerimeter',$,$,{},$", r(info.perimeter)));
        let h = self.add("IFCQUANTITYLENGTH", format!("'Height',$,$,{},$", r(info.height)));
        let volume = self.add("IFCQUANTITYVOLUME", format!("'NetVolume',$,$,{},$", r(info.volume / 1e9)));
        let qto = self.add(
            "IFCELEMENTQUANTITY",
            format!(
                "{},'Qto_SpaceBaseQuantities',$,$,({area},{perimeter},{h},{volume})",
                self.root("qto-space", id.0)
            ),
        );
        self.relate_properties("qto-space", id.0, space, Some(qto));
        let params: Vec<(String, String)> = element_parameters(doc, id)
            .into_iter()
            .filter_map(|p| value(doc, &p.value, Some(&p.kind)).map(|v| (p.name.to_string(), v)))
            .collect();
        let pset = self.property_set("params", id.0, "Hausit_Parameters", &params);
        self.relate_properties("params", id.0, space, pset);
    }

    /// Grids, as one `IfcGrid` on the lowest storey: the first direction
    /// gives the U axes and the others the V axes.
    fn grids(&mut self) {
        let doc = self.doc;
        let mut grids: Vec<(String, DVec2, DVec2)> = doc
            .elements_of(Category::Grids)
            .filter_map(|e| match &e.data {
                ElementData::Grid(g) if g.start.distance(g.end) > 1e-6 => Some((g.name.clone(), g.start, g.end)),
                _ => None,
            })
            .collect();
        grids.sort_by(|a, b| a.0.cmp(&b.0));
        let Some(first) = grids.first().map(|g| (g.2 - g.1).normalize()) else {
            return;
        };
        let (u, v): (Vec<_>, Vec<_>) = grids
            .into_iter()
            .partition(|g| (g.2 - g.1).normalize().perp_dot(first).abs() < 0.02);
        if v.is_empty() {
            return;
        }
        let (_, storey, storey_placement) = {
            let s = self.storeys[0];
            (s.1, s.2, s.3)
        };
        let mut curves = Vec::new();
        let mut axes = |this: &mut Self, group: &[(String, DVec2, DVec2)]| -> Vec<Ref> {
            group
                .iter()
                .map(|(name, a, b)| {
                    let pa = this.point2(*a);
                    let pb = this.point2(*b);
                    let line = this.add("IFCPOLYLINE", format!("({pa},{pb})"));
                    curves.push(line);
                    this.add("IFCGRIDAXIS", format!("{},{line},.T.", s(name)))
                })
                .collect()
        };
        let ua = axes(self, &u);
        let va = axes(self, &v);
        let set = self.add("IFCGEOMETRICCURVESET", list(&curves));
        let rep = self.add(
            "IFCSHAPEREPRESENTATION",
            format!("{},'FootPrint','GeometricCurveSet',({set})", self.footprint),
        );
        let shape = self.add("IFCPRODUCTDEFINITIONSHAPE", format!("$,$,({rep})"));
        let placement = self.local_placement(Some(storey_placement), DVec3::ZERO, None);
        let grid = self.add(
            "IFCGRID",
            format!(
                "{},'Grid',$,$,{placement},{shape},{},{},$,.RECTANGULAR.",
                self.root("grid", 0),
                list(&ua),
                list(&va)
            ),
        );
        self.contained.entry(storey).or_default().push(grid);
    }

    fn relationships(&mut self) {
        for (storey, elements) in std::mem::take(&mut self.contained) {
            self.add(
                "IFCRELCONTAINEDINSPATIALSTRUCTURE",
                format!("{},$,$,{},{storey}", self.root("contained", storey.0 as u64), list(&elements)),
            );
        }
        for (storey, spaces) in std::mem::take(&mut self.spaces) {
            self.add(
                "IFCRELAGGREGATES",
                format!("{},$,$,{storey},{}", self.root("agg-spaces", storey.0 as u64), list(&spaces)),
            );
        }
        for ((_, _), (type_ref, occ)) in std::mem::take(&mut self.types) {
            self.add(
                "IFCRELDEFINESBYTYPE",
                format!("{},$,$,{},{type_ref}", self.root("type-rel", type_ref.0 as u64), list(&occ)),
            );
        }
        for (material, objs) in std::mem::take(&mut self.associations) {
            self.add(
                "IFCRELASSOCIATESMATERIAL",
                format!("{},$,$,{},{material}", self.root("material-rel", material.0 as u64), list(&objs)),
            );
        }
    }

    fn finish(self) -> String {
        let info = &self.doc.project_info;
        let name = if info.name.is_empty() { "Hausit Project" } else { &info.name };
        let mut out = String::from("ISO-10303-21;\nHEADER;\n");
        out.push_str("FILE_DESCRIPTION(('ViewDefinition [ReferenceView_V1.2]'),'2;1');\n");
        let _ = writeln!(
            out,
            "FILE_NAME({},{},({}),({}),{},{},'');",
            s(&format!("{name}.ifc")),
            s(&iso_time(self.timestamp)),
            s(&info.author),
            s(&info.client),
            s(&format!("Hausit {}", env!("CARGO_PKG_VERSION"))),
            s("Hausit"),
        );
        out.push_str("FILE_SCHEMA(('IFC4'));\nENDSEC;\nDATA;\n");
        out.push_str(&self.out);
        out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
        out
    }
}

/// A parameter value as an IFC measure, or `None` when it has no value.
fn value(doc: &Document, v: &ParamValue, kind: Option<&ParamKind>) -> Option<String> {
    Some(match v {
        ParamValue::Length(x) => format!("IFCLENGTHMEASURE({})", r(*x)),
        ParamValue::Area(x) => format!("IFCAREAMEASURE({})", r(x / 1e6)),
        ParamValue::Volume(x) => format!("IFCVOLUMEMEASURE({})", r(x / 1e9)),
        ParamValue::Angle(x) => format!("IFCPLANEANGLEMEASURE({})", r(x.to_radians())),
        ParamValue::Number(x) => format!("IFCREAL({})", r(*x)),
        ParamValue::Integer(n) => format!("IFCINTEGER({n})"),
        ParamValue::Text(t) if t.is_empty() => return None,
        ParamValue::Text(t) => format!("IFCLABEL({})", s(t)),
        ParamValue::Bool(x) => format!("IFCBOOLEAN({})", b(*x)),
        ParamValue::Element(id) if !id.is_valid() => return None,
        ParamValue::Element(id) => match doc.level(*id) {
            Some(l) => format!("IFCLABEL({})", s(&l.name)),
            None => format!("IFCLABEL({})", s(&doc.describe(*id))),
        },
        ParamValue::Choice(i) => match kind {
            Some(ParamKind::Choice(labels)) => format!("IFCLABEL({})", s(labels.get(*i)?)),
            _ => format!("IFCINTEGER({i})"),
        },
    })
}

/// `YYYY-MM-DDThh:mm:ss` (UTC) for a Unix time.
fn iso_time(t: i64) -> String {
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hausit_model::template::{industrial_sample, sample_project};

    #[test]
    fn strings_and_reals_follow_step() {
        assert_eq!(s("It's a \\ test"), "'It''s a \\\\ test'");
        assert_eq!(s("m²"), "'m\\X2\\00B2\\X0\\'");
        assert_eq!(r(12.0), "12.");
        assert_eq!(r(-0.5), "-0.5");
        assert_eq!(r(1e-12), "0.");
        assert_eq!(iso_time(0), "1970-01-01T00:00:00");
        assert_eq!(iso_time(1_793_010_600), "2026-10-26T10:30:00");
    }

    #[test]
    fn guids_are_stable_and_distinct() {
        let a = guid(1, "element", 5);
        assert_eq!(a.len(), 22);
        assert_eq!(a, guid(1, "element", 5));
        assert_ne!(a, guid(1, "element", 6));
        assert_ne!(a, guid(1, "opening", 5));
        assert!(matches!(a.as_bytes()[0], b'0'..=b'3'));
    }

    #[test]
    fn exports_the_sample_house() {
        let doc = sample_project();
        let text = export(&doc, 1_793_010_600);
        assert!(text.starts_with("ISO-10303-21;"));
        assert!(text.contains("FILE_SCHEMA(('IFC4'));"));
        for entity in [
            "IFCPROJECT(",
            "IFCBUILDINGSTOREY(",
            "IFCWALL(",
            "IFCSLAB(",
            "IFCROOF(",
            "IFCDOOR(",
            "IFCWINDOW(",
            "IFCOPENINGELEMENT(",
            "IFCRELVOIDSELEMENT(",
            "IFCRELFILLSELEMENT(",
            "IFCMATERIALLAYERSETUSAGE(",
            "IFCSPACE(",
            "IFCWALLTYPE(",
            "IFCRELDEFINESBYTYPE(",
            "IFCFURNITURE(",
            "IFCFURNITURETYPE(",
        ] {
            assert!(text.contains(entity), "missing {entity}");
        }
        // Every reference points at an instance that exists.
        let defined: std::collections::HashSet<&str> = text
            .lines()
            .filter_map(|l| l.strip_prefix('#'))
            .filter_map(|l| l.split('=').next())
            .collect();
        for line in text.lines().filter(|l| l.starts_with('#')) {
            let body = line.split_once('=').unwrap().1;
            let mut rest = body;
            while let Some(i) = rest.find('#') {
                rest = &rest[i + 1..];
                let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                assert!(defined.contains(n.as_str()), "dangling #{n} in {line}");
            }
        }
        // The same model gives the same file.
        assert_eq!(text, export(&doc, 1_793_010_600));
    }

    #[test]
    fn exports_the_warehouse_structure() {
        let text = export(&industrial_sample(), 0);
        for entity in [
            "IFCCOLUMN(",
            "IFCBEAM(",
            "IFCMEMBER(",
            "IFCFOOTING(",
            "IFCSTAIR(",
            "IFCRAILING(",
            "IFCGRID(",
        ] {
            assert!(text.contains(entity), "missing {entity}");
        }
    }
}
