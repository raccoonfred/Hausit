//! IFC import (IFC2x3 and IFC4): turns an IFC model into a Hausit project.
//!
//! Storeys become levels. Walls, floors, columns, doors and windows that
//! are simple enough become native elements, so they can be edited:
//! straight walls from a vertical extrusion and their axis, floors from a
//! vertical slab extrusion, rectangular columns, and doors and windows in
//! those walls. Spaces become rooms and grid axes become grids. Everything
//! else with geometry (roofs, stairs, furniture, beams, curtain walls...)
//! comes in as direct shapes that keep the element's geometry, colour and
//! category. This is also the route for Revit projects exported to IFC.

use crate::step::{parse, Entity, StepFile, Value};
use glam::{DAffine3, DMat3};
use hausit_core::geom::{perp, point_in_polygon, polygon_area, polygon_normal, Plane};
use hausit_core::{DVec2, DVec3, ElementId};
use hausit_model::element::*;
use hausit_model::family::TypeParams;
use hausit_model::materials::{FillPattern, Layer, LayerFunction, MaterialDef, SurfacePattern};
use hausit_model::view::{CardinalDirection, ViewKind};
use hausit_model::{Category, Document, ParamValue};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};

/// What an import did, for the user.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportReport {
    pub schema: String,
    pub application: String,
    pub levels: usize,
    pub walls: usize,
    pub floors: usize,
    pub columns: usize,
    pub doors: usize,
    pub windows: usize,
    pub rooms: usize,
    pub grids: usize,
    /// Elements kept as direct shapes, by IFC class.
    pub shapes: BTreeMap<String, usize>,
    /// Elements with no geometry Hausit could read, by IFC class.
    pub skipped: BTreeMap<String, usize>,
    /// Openings in walls with no door or window in them (not imported).
    pub empty_openings: usize,
}

impl ImportReport {
    pub fn summary(&self) -> String {
        let shapes: usize = self.shapes.values().sum();
        let skipped: usize = self.skipped.values().sum();
        let mut s = format!(
            "Imported {} levels, {} walls, {} floors, {} columns, {} doors, {} windows, {} rooms and {} grids as Hausit elements, \
             and {} other elements as imported geometry.",
            self.levels, self.walls, self.floors, self.columns, self.doors, self.windows, self.rooms, self.grids, shapes
        );
        if skipped > 0 {
            let names: Vec<String> = self.skipped.iter().map(|(k, v)| format!("{k} ({v})")).collect();
            s.push_str(&format!(" {skipped} elements had no readable geometry: {}.", names.join(", ")));
        }
        if self.empty_openings > 0 {
            s.push_str(&format!(
                " {} wall openings without a door or window were not cut.",
                self.empty_openings
            ));
        }
        s
    }
}

/// Imports IFC text into a new project.
pub fn import(text: &str) -> Result<(Document, ImportReport), String> {
    let file = parse(text)?;
    if !file.schema.starts_with("IFC") {
        return Err(format!("this is a STEP file but not IFC (schema {})", file.schema));
    }
    let mut imp = Importer::new(&file);
    imp.run();
    Ok((imp.doc, imp.report))
}

/// A surface colour and how transparent it is (0 opaque, 1 clear).
#[derive(Clone, Debug, PartialEq)]
struct Style {
    name: String,
    color: [u8; 3],
    transparency: f64,
}

/// A solid read from IFC, in model millimetres.
#[derive(Clone, Debug)]
enum Solid {
    Prism { footprint: Vec<DVec2>, z0: f64, z1: f64 },
    Faces(Vec<Vec<DVec3>>),
}

impl Solid {
    fn faces(&self) -> Vec<Vec<DVec3>> {
        match self {
            Solid::Faces(f) => f.clone(),
            Solid::Prism { footprint, z0, z1 } => {
                let mut fp = footprint.clone();
                if polygon_area(&fp) < 0.0 {
                    fp.reverse();
                }
                let at = |p: DVec2, z: f64| DVec3::new(p.x, p.y, z);
                let mut faces = vec![
                    fp.iter().rev().map(|p| at(*p, *z0)).collect(),
                    fp.iter().map(|p| at(*p, *z1)).collect::<Vec<_>>(),
                ];
                for i in 0..fp.len() {
                    let (a, b) = (fp[i], fp[(i + 1) % fp.len()]);
                    faces.push(vec![at(a, *z0), at(b, *z0), at(b, *z1), at(a, *z1)]);
                }
                faces
            }
        }
    }

    fn points(&self) -> Vec<DVec3> {
        match self {
            Solid::Faces(f) => f.iter().flatten().copied().collect(),
            Solid::Prism { footprint, z0, z1 } => footprint
                .iter()
                .flat_map(|p| [DVec3::new(p.x, p.y, *z0), DVec3::new(p.x, p.y, *z1)])
                .collect(),
        }
    }
}

/// A body solid with its style.
#[derive(Clone, Debug)]
struct Piece {
    solid: Solid,
    style: Option<Style>,
}

/// A vertical extrusion found in a body: the outline in plan (model mm),
/// its base and top, and the profile's own frame for parametric profiles.
#[derive(Clone, Debug)]
struct Extrusion {
    footprint: Vec<DVec2>,
    z0: f64,
    z1: f64,
}

/// A column section recognised from its outline.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Section {
    /// Width across x, depth along y.
    Rect { b: f64, h: f64 },
    /// Flange width across x, depth along y, web and flange thickness.
    I { bf: f64, d: f64, tw: f64, tf: f64 },
}

/// The section of a column outline: its centre, x axis and shape, if it
/// is a rectangle or a symmetric I.
fn section_of(outline: &[DVec2]) -> Option<(DVec2, DVec2, Section)> {
    let pts = dedupe(outline.to_vec());
    let n = pts.len();
    if n < 4 {
        return None;
    }
    let near = |a: f64, b: f64| (a - b).abs() < 0.5;
    for i in 0..n {
        let x = (pts[(i + 1) % n] - pts[i]).normalize_or_zero();
        if x == DVec2::ZERO {
            continue;
        }
        let y = perp(x);
        let (xs, ys): (Vec<f64>, Vec<f64>) = pts.iter().map(|p| (p.dot(x), p.dot(y))).unzip();
        let (x0, x1) = xs
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| (l.min(*v), h.max(*v)));
        let (y0, y1) = ys
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| (l.min(*v), h.max(*v)));
        let centre = x * (x0 + x1) / 2.0 + y * (y0 + y1) / 2.0;
        let (hw, hd) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
        let local: Vec<DVec2> = pts.iter().map(|p| DVec2::new((*p - centre).dot(x), (*p - centre).dot(y))).collect();
        if n == 4 && local.iter().all(|p| near(p.x.abs(), hw) && near(p.y.abs(), hd)) {
            return Some((centre, x, Section::Rect { b: 2.0 * hw, h: 2.0 * hd }));
        }
        if n == 12 {
            // Flanges across x at ±hd: the inner corners give the web and
            // flange thickness.
            let web = local.iter().map(|p| p.x.abs()).filter(|v| !near(*v, hw)).fold(f64::NAN, f64::min);
            let inner = local.iter().map(|p| p.y.abs()).filter(|v| !near(*v, hd)).fold(f64::NAN, f64::max);
            if web.is_finite() && inner.is_finite() && web < hw && inner < hd {
                let ok = local.iter().all(|p| {
                    (near(p.x.abs(), hw) && (near(p.y.abs(), hd) || near(p.y.abs(), inner)))
                        || (near(p.x.abs(), web) && near(p.y.abs(), inner))
                });
                if ok {
                    let s = Section::I {
                        bf: 2.0 * hw,
                        d: 2.0 * hd,
                        tw: 2.0 * web,
                        tf: hd - inner,
                    };
                    return Some((centre, x, s));
                }
            }
        }
    }
    None
}

struct Importer<'a> {
    f: &'a StepFile,
    /// File length unit in mm, and plane angle unit in radians.
    scale: f64,
    angle: f64,
    doc: Document,
    report: ImportReport,
    placements: RefCell<HashMap<u32, DAffine3>>,
    styles: HashMap<u32, Style>,
    /// Material name → its style, from material definition representations.
    material_styles: HashMap<String, Style>,
    /// Element → material association (material, layer set or usage).
    materials: HashMap<u32, u32>,
    /// Element → its type object.
    types: HashMap<u32, u32>,
    /// Element → the spatial structure that contains it.
    container: HashMap<u32, u32>,
    /// Part → the element it is a part of.
    parent: HashMap<u32, u32>,
    /// Storey (IFC id) → level.
    levels: Vec<(u32, ElementId, f64)>,
    /// Wall (IFC id) → Hausit wall.
    walls: HashMap<u32, ElementId>,
    /// Door or window → the wall opening it fills, and that wall.
    fills: HashMap<u32, (u32, u32)>,
    /// Elements with a boolean property `IsExternal` set.
    external: HashSet<u32>,
    type_keys: HashMap<String, ElementId>,
    /// Walls whose IFC body slopes on top: attached to the roofs above.
    sloped_walls: Vec<ElementId>,
}

/// IFC classes that are spatial, annotation or abstract: not imported as
/// elements.
const NOT_ELEMENTS: &[&str] = &[
    "IFCOPENINGELEMENT",
    "IFCOPENINGSTANDARDCASE",
    "IFCSPACE",
    "IFCSITE",
    "IFCBUILDING",
    "IFCBUILDINGSTOREY",
    "IFCPROJECT",
    "IFCANNOTATION",
    "IFCGRID",
    "IFCVIRTUALELEMENT",
    "IFCSPATIALZONE",
    "IFCEXTERNALSPATIALELEMENT",
];

/// The model category for an IFC class (and predefined type).
fn category_of(class: &str, predefined: Option<&str>, parent: Option<&str>) -> Category {
    match (class, parent) {
        ("IFCSLAB" | "IFCMEMBER" | "IFCPLATE" | "IFCBEAM", Some("IFCROOF")) => return Category::Roofs,
        (_, Some("IFCSTAIR")) => return Category::Stairs,
        (_, Some("IFCRAMP")) => return Category::Floors,
        (_, Some("IFCRAILING")) => return Category::Railings,
        (_, Some("IFCCURTAINWALL")) => return Category::Walls,
        _ => {}
    }
    match class {
        "IFCWALL" | "IFCWALLSTANDARDCASE" | "IFCWALLELEMENTEDCASE" | "IFCCURTAINWALL" => Category::Walls,
        "IFCSLAB" | "IFCSLABSTANDARDCASE" | "IFCSLABELEMENTEDCASE" if predefined == Some("ROOF") => Category::Roofs,
        "IFCSLAB" | "IFCSLABSTANDARDCASE" | "IFCSLABELEMENTEDCASE" | "IFCRAMP" | "IFCRAMPFLIGHT" => Category::Floors,
        "IFCROOF" => Category::Roofs,
        "IFCCOVERING" if predefined == Some("CEILING") => Category::Ceilings,
        "IFCCOVERING" if predefined == Some("ROOFING") => Category::Roofs,
        "IFCCOVERING" => Category::Floors,
        "IFCDOOR" | "IFCDOORSTANDARDCASE" => Category::Doors,
        "IFCWINDOW" | "IFCWINDOWSTANDARDCASE" => Category::Windows,
        "IFCCOLUMN" | "IFCCOLUMNSTANDARDCASE" | "IFCPILE" => Category::StructuralColumns,
        "IFCBEAM" | "IFCBEAMSTANDARDCASE" | "IFCMEMBER" | "IFCMEMBERSTANDARDCASE" => Category::StructuralFraming,
        "IFCFOOTING" => Category::StructuralFoundations,
        "IFCSTAIR" | "IFCSTAIRFLIGHT" => Category::Stairs,
        "IFCRAILING" => Category::Railings,
        "IFCFURNISHINGELEMENT" | "IFCFURNITURE" | "IFCSYSTEMFURNITUREELEMENT" => Category::Furniture,
        "IFCSANITARYTERMINAL" | "IFCFLOWTERMINAL" | "IFCWASTETERMINAL" => Category::PlumbingFixtures,
        "IFCPLATE" => Category::Walls,
        _ => Category::GenericModels,
    }
}

fn orthonormal(z: DVec3, x: DVec3) -> DMat3 {
    let z = z.try_normalize().unwrap_or(DVec3::Z);
    let mut x = (x - z * x.dot(z)).try_normalize().unwrap_or(DVec3::ZERO);
    if x == DVec3::ZERO {
        x = if z.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
        x = (x - z * x.dot(z)).normalize();
    }
    let y = z.cross(x);
    DMat3::from_cols(x, y, z)
}

/// The point on a circle through three points, and its radius.
fn circle_through(a: DVec2, b: DVec2, c: DVec2) -> Option<(DVec2, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let (a2, b2, c2) = (a.length_squared(), b.length_squared(), c.length_squared());
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    let o = DVec2::new(ux, uy);
    Some((o, o.distance(a)))
}

/// Points along an arc from angle `a0` to `a1` (radians).
fn arc(center: DVec2, rx: f64, ry: f64, a0: f64, a1: f64) -> Vec<DVec2> {
    let n = (((a1 - a0).abs() / (std::f64::consts::PI / 18.0)).ceil() as usize).clamp(2, 72);
    (0..=n)
        .map(|i| {
            let t = a0 + (a1 - a0) * i as f64 / n as f64;
            center + DVec2::new(rx * t.cos(), ry * t.sin())
        })
        .collect()
}

fn dedupe(mut pts: Vec<DVec2>) -> Vec<DVec2> {
    pts.dedup_by(|a, b| a.distance(*b) < 1e-6);
    while pts.len() > 2 && pts.first().unwrap().distance(*pts.last().unwrap()) < 1e-6 {
        pts.pop();
    }
    pts
}

/// The convex hull of points in a plane (2D), counter-clockwise.
fn convex_hull(mut pts: Vec<DVec2>) -> Vec<DVec2> {
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup_by(|a, b| a.distance(*b) < 1e-6);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o);
    let mut lower: Vec<DVec2> = Vec::new();
    for p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], *p) <= 0.0 {
            lower.pop();
        }
        lower.push(*p);
    }
    let mut upper: Vec<DVec2> = Vec::new();
    for p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], *p) <= 0.0 {
            upper.pop();
        }
        upper.push(*p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Clips faces by a plane, keeping `signed_distance >= 0`, and closes the
/// cut with the convex hull of the cut points.
fn clip_faces(faces: &[Vec<DVec3>], plane: &Plane) -> Vec<Vec<DVec3>> {
    let mut out = Vec::new();
    let mut cut_pts: Vec<DVec3> = Vec::new();
    let mut any_clipped = false;
    for f in faces {
        let d: Vec<f64> = f.iter().map(|p| plane.signed_distance(*p)).collect();
        if d.iter().all(|v| *v >= -1e-6) {
            out.push(f.clone());
            for (p, v) in f.iter().zip(&d) {
                if v.abs() < 1e-6 {
                    cut_pts.push(*p);
                }
            }
            continue;
        }
        any_clipped = true;
        if d.iter().all(|v| *v < 1e-6) {
            continue;
        }
        let kept = hausit_core::geom::clip_polygon_3d(f, plane);
        for p in &kept {
            if plane.signed_distance(*p).abs() < 1e-4 {
                cut_pts.push(*p);
            }
        }
        if kept.len() >= 3 {
            out.push(kept);
        }
    }
    if any_clipped && cut_pts.len() >= 3 {
        // Cap the cut: the hull of the cut points in the plane, facing
        // out of the kept part (against the plane normal).
        let m = orthonormal(-plane.normal, DVec3::X);
        let (u, v) = (m.x_axis, m.y_axis);
        let o = cut_pts[0];
        let hull = convex_hull(cut_pts.iter().map(|p| DVec2::new((*p - o).dot(u), (*p - o).dot(v))).collect());
        if hull.len() >= 3 {
            out.push(hull.iter().map(|q| o + u * q.x + v * q.y).collect());
        }
    }
    out
}

impl<'a> Importer<'a> {
    fn new(f: &'a StepFile) -> Self {
        let mut doc = Document::empty();
        hausit_model::template::load_default_types(&mut doc);
        Importer {
            f,
            scale: 1.0,
            angle: 1.0,
            doc,
            report: ImportReport {
                schema: f.schema.clone(),
                ..Default::default()
            },
            placements: RefCell::new(HashMap::new()),
            styles: HashMap::new(),
            material_styles: HashMap::new(),
            materials: HashMap::new(),
            types: HashMap::new(),
            container: HashMap::new(),
            parent: HashMap::new(),
            levels: Vec::new(),
            walls: HashMap::new(),
            fills: HashMap::new(),
            external: HashSet::new(),
            type_keys: HashMap::new(),
            sloped_walls: Vec::new(),
        }
    }

    fn e(&self, id: u32) -> Option<&'a Entity> {
        self.f.get(id)
    }

    fn deref(&self, v: &Value) -> Option<&'a Entity> {
        v.as_ref().and_then(|r| self.f.get(r))
    }

    // ----- setup -------------------------------------------------------------

    fn run(&mut self) {
        self.units();
        self.relations();
        self.read_styles();
        self.project_info();
        self.storeys();
        self.grids();
        // Walls first, so doors and windows can find their host.
        let mut elements: Vec<u32> = self
            .f
            .entities
            .iter()
            .filter(|(_, e)| e.args.len() >= 7 && e.name.starts_with("IFC") && self.is_product(e))
            .map(|(id, _)| *id)
            .collect();
        elements.sort_unstable();
        let is_wall = |f: &StepFile, id: &u32| f.get(*id).is_some_and(|e| e.name.starts_with("IFCWALL"));
        let (walls, others): (Vec<u32>, Vec<u32>) = elements.into_iter().partition(|id| is_wall(self.f, id));
        for id in walls.iter().chain(&others) {
            self.element(*id);
        }
        self.attach_walls();
        for id in self.f.all("IFCSPACE") {
            self.space(id);
        }
        self.report.empty_openings = self
            .f
            .all("IFCRELVOIDSELEMENT")
            .iter()
            .filter_map(|r| self.e(*r))
            .filter(|r| self.walls.contains_key(&r.arg(4).as_ref().unwrap_or(0)))
            .filter(|r| {
                let o = r.arg(5).as_ref();
                !self.fills.values().any(|(op, _)| Some(*op) == o)
            })
            .count();
        self.views();
    }

    /// Products with a placement: elements, not relationships or
    /// resources.
    fn is_product(&self, e: &Entity) -> bool {
        if NOT_ELEMENTS.contains(&e.name.as_str()) || e.name.starts_with("IFCREL") || e.name.ends_with("TYPE") {
            return false;
        }
        // GlobalId, then an ObjectPlacement (a local placement) at index 5.
        e.arg(0).as_str().is_some_and(|g| g.len() == 22)
            && self
                .deref(e.arg(5))
                .is_some_and(|p| p.name == "IFCLOCALPLACEMENT" || p.name == "IFCGRIDPLACEMENT")
    }

    fn units(&mut self) {
        for ua in self.f.all("IFCUNITASSIGNMENT") {
            let Some(ua) = self.e(ua) else { continue };
            for u in ua.arg(0).refs() {
                let Some(u) = self.e(u) else { continue };
                let kind = u.arg(1).as_enum().unwrap_or("");
                match u.name.as_str() {
                    "IFCSIUNIT" => {
                        let prefix = match u.arg(2).as_enum() {
                            Some("MILLI") => 1e-3,
                            Some("CENTI") => 1e-2,
                            Some("DECI") => 1e-1,
                            Some("KILO") => 1e3,
                            _ => 1.0,
                        };
                        match kind {
                            "LENGTHUNIT" => self.scale = prefix * 1000.0,
                            "PLANEANGLEUNIT" => self.angle = prefix,
                            _ => {}
                        }
                    }
                    "IFCCONVERSIONBASEDUNIT" => {
                        // ConversionFactor: a measure with unit.
                        let factor = self
                            .deref(u.arg(3))
                            .and_then(|m| m.arg(0).as_f64().map(|v| (v, m.arg(1).as_ref())))
                            .map(|(v, unit)| {
                                let base = unit
                                    .and_then(|r| self.e(r))
                                    .map(|b| match b.arg(2).as_enum() {
                                        Some("MILLI") => 1e-3,
                                        Some("CENTI") => 1e-2,
                                        _ => 1.0,
                                    })
                                    .unwrap_or(1.0);
                                v * base
                            });
                        match (kind, factor) {
                            ("LENGTHUNIT", Some(v)) => self.scale = v * 1000.0,
                            ("PLANEANGLEUNIT", Some(v)) => self.angle = v,
                            ("PLANEANGLEUNIT", None) => self.angle = std::f64::consts::PI / 180.0,
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn relations(&mut self) {
        for r in self.f.entities.values() {
            match r.name.as_str() {
                "IFCRELCONTAINEDINSPATIALSTRUCTURE" => {
                    if let Some(s) = r.arg(5).as_ref() {
                        for el in r.arg(4).refs() {
                            self.container.insert(el, s);
                        }
                    }
                }
                "IFCRELAGGREGATES" | "IFCRELNESTS" => {
                    if let Some(p) = r.arg(4).as_ref() {
                        for el in r.arg(5).refs() {
                            self.parent.insert(el, p);
                        }
                    }
                }
                "IFCRELDEFINESBYTYPE" => {
                    if let Some(t) = r.arg(5).as_ref() {
                        for el in r.arg(4).refs() {
                            self.types.insert(el, t);
                        }
                    }
                }
                "IFCRELASSOCIATESMATERIAL" => {
                    if let Some(m) = r.arg(5).as_ref() {
                        for el in r.arg(4).refs() {
                            self.materials.insert(el, m);
                        }
                    }
                }
                "IFCRELDEFINESBYPROPERTIES" => {
                    let Some(ps) = self.deref(r.arg(5)) else { continue };
                    let ext = ps.arg(4).refs().into_iter().filter_map(|p| self.e(p)).any(|p| {
                        p.arg(0).as_str() == Some("IsExternal") && matches!(p.arg(2), Value::Typed(_, v) if v.as_enum() == Some("T"))
                    });
                    if ext {
                        for el in r.arg(4).refs() {
                            self.external.insert(el);
                        }
                    }
                }
                _ => {}
            }
        }
        for r in self.f.all("IFCRELFILLSELEMENT") {
            let Some(r) = self.e(r) else { continue };
            let (Some(opening), Some(filler)) = (r.arg(4).as_ref(), r.arg(5).as_ref()) else {
                continue;
            };
            let host = self
                .f
                .all("IFCRELVOIDSELEMENT")
                .into_iter()
                .filter_map(|v| self.e(v))
                .find(|v| v.arg(5).as_ref() == Some(opening))
                .and_then(|v| v.arg(4).as_ref());
            if let Some(host) = host {
                self.fills.insert(filler, (opening, host));
            }
        }
    }

    fn colour(&self, c: &Value) -> Option<[u8; 3]> {
        let c = self.deref(c)?;
        if c.name != "IFCCOLOURRGB" {
            return None;
        }
        let ch = |i: usize| (c.arg(i).as_f64().unwrap_or(0.8).clamp(0.0, 1.0) * 255.0).round() as u8;
        Some([ch(1), ch(2), ch(3)])
    }

    /// A surface style's colour and transparency.
    fn surface_style(&self, id: u32) -> Option<Style> {
        let s = self.e(id)?;
        match s.name.as_str() {
            "IFCPRESENTATIONSTYLEASSIGNMENT" => s.arg(0).refs().into_iter().find_map(|r| self.surface_style(r)),
            "IFCSURFACESTYLE" => {
                let name = s.arg(0).as_str().unwrap_or("").to_string();
                s.arg(2).refs().into_iter().find_map(|r| {
                    let st = self.e(r)?;
                    if !st.name.starts_with("IFCSURFACESTYLE") {
                        return None;
                    }
                    let color = self.colour(st.arg(0))?;
                    let transparency = st.arg(1).as_f64().unwrap_or(0.0);
                    Some(Style {
                        name: name.clone(),
                        color,
                        transparency,
                    })
                })
            }
            _ => None,
        }
    }

    fn read_styles(&mut self) {
        for id in self.f.all("IFCSTYLEDITEM") {
            let Some(si) = self.e(id) else { continue };
            let style = si.arg(1).refs().into_iter().find_map(|r| self.surface_style(r));
            if let (Some(item), Some(style)) = (si.arg(0).as_ref(), style) {
                self.styles.insert(item, style);
            }
        }
        // Material colours, from their styled representations.
        for id in self.f.all("IFCMATERIALDEFINITIONREPRESENTATION") {
            let Some(md) = self.e(id) else { continue };
            let Some(name) = self.deref(md.arg(3)).and_then(|m| m.arg(0).as_str()) else {
                continue;
            };
            let style = md.arg(2).refs().into_iter().find_map(|rep| {
                let rep = self.e(rep)?;
                rep.arg(3).refs().into_iter().find_map(|it| {
                    let it = self.e(it)?;
                    it.arg(1).refs().into_iter().find_map(|r| self.surface_style(r))
                })
            });
            if let Some(mut style) = style {
                style.name = name.to_string();
                self.material_styles.insert(name.to_string(), style);
            }
        }
    }

    fn project_info(&mut self) {
        if let Some(p) = self.f.all("IFCPROJECT").first().and_then(|p| self.e(*p)) {
            let long = p.arg(5).as_str().unwrap_or("");
            let name = p.arg(2).as_str().unwrap_or("");
            self.doc.project_info.name = if long.is_empty() { name } else { long }.to_string();
            if !long.is_empty() {
                self.doc.project_info.number = name.to_string();
            }
        }
        // The authoring application, from the owner history or header.
        if let Some(app) = self.f.all("IFCAPPLICATION").first().and_then(|a| self.e(*a)) {
            self.report.application = format!("{} {}", app.arg(2).as_str().unwrap_or(""), app.arg(1).as_str().unwrap_or(""))
                .trim()
                .to_string();
        }
    }

    // ----- placements and geometry ---------------------------------------------

    fn point3(&self, v: &Value) -> DVec3 {
        let c: Vec<f64> = self
            .deref(v)
            .map(|p| p.arg(0).as_list().iter().filter_map(|x| x.as_f64()).collect())
            .unwrap_or_default();
        DVec3::new(
            c.first().copied().unwrap_or(0.0),
            c.get(1).copied().unwrap_or(0.0),
            c.get(2).copied().unwrap_or(0.0),
        ) * self.scale
    }

    fn direction(&self, v: &Value) -> Option<DVec3> {
        let d = self.deref(v)?;
        let c: Vec<f64> = d.arg(0).as_list().iter().filter_map(|x| x.as_f64()).collect();
        let v = DVec3::new(
            c.first().copied().unwrap_or(0.0),
            c.get(1).copied().unwrap_or(0.0),
            c.get(2).copied().unwrap_or(0.0),
        );
        v.try_normalize()
    }

    /// An IfcAxis2Placement2D/3D as a transform.
    fn axis_placement(&self, v: &Value) -> DAffine3 {
        let Some(a) = self.deref(v) else { return DAffine3::IDENTITY };
        let origin = self.point3(a.arg(0));
        match a.name.as_str() {
            "IFCAXIS2PLACEMENT3D" => {
                let z = self.direction(a.arg(1)).unwrap_or(DVec3::Z);
                let x = self.direction(a.arg(2)).unwrap_or(DVec3::X);
                DAffine3::from_mat3_translation(orthonormal(z, x), origin)
            }
            "IFCAXIS2PLACEMENT2D" => {
                let x = self.direction(a.arg(1)).unwrap_or(DVec3::X);
                DAffine3::from_mat3_translation(orthonormal(DVec3::Z, x), origin)
            }
            _ => DAffine3::IDENTITY,
        }
    }

    /// The world transform of an object placement.
    fn placement(&self, id: u32) -> DAffine3 {
        if let Some(t) = self.placements.borrow().get(&id) {
            return *t;
        }
        let t = match self.e(id) {
            Some(p) if p.name == "IFCLOCALPLACEMENT" => {
                let parent = p.arg(0).as_ref().map(|r| self.placement(r)).unwrap_or(DAffine3::IDENTITY);
                parent * self.axis_placement(p.arg(1))
            }
            _ => DAffine3::IDENTITY,
        };
        self.placements.borrow_mut().insert(id, t);
        t
    }

    fn object_placement(&self, e: &Entity) -> DAffine3 {
        e.arg(5).as_ref().map(|p| self.placement(p)).unwrap_or(DAffine3::IDENTITY)
    }

    /// A cartesian transformation operator (mapped items).
    fn operator(&self, v: &Value) -> DAffine3 {
        let Some(o) = self.deref(v) else { return DAffine3::IDENTITY };
        let x = self.direction(o.arg(0)).unwrap_or(DVec3::X);
        let origin = self.point3(o.arg(2));
        let s = o.arg(3).as_f64().unwrap_or(1.0);
        let z = if o.name.contains("3D") {
            self.direction(o.arg(4)).unwrap_or(DVec3::Z)
        } else {
            DVec3::Z
        };
        let (sy, sz) = if o.name.contains("NONUNIFORM") {
            (o.arg(5).as_f64().unwrap_or(s), o.arg(6).as_f64().unwrap_or(s))
        } else {
            (s, s)
        };
        let m = orthonormal(z, x);
        let m = DMat3::from_cols(m.x_axis * s, m.y_axis * sy, m.z_axis * sz);
        DAffine3::from_mat3_translation(m, origin)
    }

    /// A curve as points in its own plane (2D, mm).
    fn curve(&self, id: u32) -> Vec<DVec2> {
        let Some(c) = self.e(id) else { return vec![] };
        let p2 = |v: &Value| self.point3(v).truncate();
        match c.name.as_str() {
            "IFCPOLYLINE" => c.arg(0).as_list().iter().map(p2).collect(),
            "IFCCOMPOSITECURVE" => {
                let mut pts: Vec<DVec2> = Vec::new();
                for seg in c.arg(0).refs() {
                    let Some(seg) = self.e(seg) else { continue };
                    let Some(parent) = seg.arg(2).as_ref() else { continue };
                    let mut sp = self.curve(parent);
                    if seg.arg(1).as_enum() == Some("F") {
                        sp.reverse();
                    }
                    if let (Some(last), Some(first)) = (pts.last(), sp.first()) {
                        if last.distance(*first) < 1e-6 {
                            sp.remove(0);
                        }
                    }
                    pts.extend(sp);
                }
                pts
            }
            "IFCTRIMMEDCURVE" => self.trimmed(c),
            "IFCCIRCLE" | "IFCELLIPSE" => {
                let t = self.axis_placement(c.arg(0));
                let rx = c.arg(1).as_f64().unwrap_or(0.0) * self.scale;
                let ry = if c.name == "IFCELLIPSE" {
                    c.arg(2).as_f64().unwrap_or(0.0) * self.scale
                } else {
                    rx
                };
                let mut pts = arc(DVec2::ZERO, rx, ry, 0.0, std::f64::consts::TAU);
                pts.pop();
                pts.iter().map(|p| t.transform_point3(p.extend(0.0)).truncate()).collect()
            }
            "IFCINDEXEDPOLYCURVE" => {
                let list: Vec<DVec2> = self
                    .deref(c.arg(0))
                    .map(|l| {
                        l.arg(0)
                            .as_list()
                            .iter()
                            .map(|p| {
                                let v: Vec<f64> = p.as_list().iter().filter_map(|x| x.as_f64()).collect();
                                DVec2::new(v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0)) * self.scale
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let segs = c.arg(1).as_list();
                if segs.is_empty() {
                    return list;
                }
                let at = |i: &Value| i.as_f64().and_then(|k| list.get(k as usize - 1).copied());
                let mut pts: Vec<DVec2> = Vec::new();
                for s in segs {
                    let Value::Typed(kind, idx) = s else { continue };
                    let idx: Vec<DVec2> = match idx.as_ref() {
                        Value::List(l) => l.iter().filter_map(at).collect(),
                        v => at(v).into_iter().collect(),
                    };
                    let mut sp = if kind == "IFCARCINDEX" && idx.len() == 3 {
                        match circle_through(idx[0], idx[1], idx[2]) {
                            Some((o, r)) => {
                                let ang = |p: DVec2| (p - o).y.atan2((p - o).x);
                                let (a0, am, mut a1) = (ang(idx[0]), ang(idx[1]), ang(idx[2]));
                                // Go through the middle point.
                                let ccw = (idx[1] - idx[0]).perp_dot(idx[2] - idx[1]) > 0.0;
                                let _ = am;
                                if ccw {
                                    while a1 < a0 {
                                        a1 += std::f64::consts::TAU;
                                    }
                                } else {
                                    while a1 > a0 {
                                        a1 -= std::f64::consts::TAU;
                                    }
                                }
                                arc(o, r, r, a0, a1)
                            }
                            None => idx,
                        }
                    } else {
                        idx
                    };
                    if let (Some(last), Some(first)) = (pts.last(), sp.first()) {
                        if last.distance(*first) < 1e-6 {
                            sp.remove(0);
                        }
                    }
                    pts.extend(sp);
                }
                pts
            }
            "IFCLINE" => {
                let p = p2(c.arg(0));
                let dir = self
                    .deref(c.arg(1))
                    .map(|v| self.direction(v.arg(0)).unwrap_or(DVec3::X).truncate() * v.arg(1).as_f64().unwrap_or(1.0) * self.scale)
                    .unwrap_or(DVec2::X);
                vec![p, p + dir]
            }
            _ => vec![],
        }
    }

    fn trimmed(&self, c: &Entity) -> Vec<DVec2> {
        let Some(basis) = self.deref(c.arg(0)) else { return vec![] };
        let sense = c.arg(3).as_enum() != Some("F");
        // A trim is a cartesian point or a parameter value.
        let trim = |v: &Value| -> (Option<DVec2>, Option<f64>) {
            let mut point = None;
            let mut param = None;
            for t in v.as_list() {
                match t {
                    Value::Ref(r) => point = Some(self.point3(&Value::Ref(*r)).truncate()),
                    Value::Typed(n, x) if n == "IFCPARAMETERVALUE" => param = x.as_f64(),
                    _ => {}
                }
            }
            (point, param)
        };
        let (t1, t2) = (trim(c.arg(1)), trim(c.arg(2)));
        match basis.name.as_str() {
            "IFCCIRCLE" | "IFCELLIPSE" => {
                let tr = self.axis_placement(basis.arg(0));
                let inv = tr.inverse();
                let rx = basis.arg(1).as_f64().unwrap_or(0.0) * self.scale;
                let ry = if basis.name == "IFCELLIPSE" {
                    basis.arg(2).as_f64().unwrap_or(0.0) * self.scale
                } else {
                    rx
                };
                let prefer_param = c.arg(4).as_enum() == Some("PARAMETER");
                let angle_of = |t: (Option<DVec2>, Option<f64>)| -> f64 {
                    match (t.0, t.1) {
                        (Some(p), Some(a)) => {
                            if prefer_param {
                                a * self.angle
                            } else {
                                let q = inv.transform_point3(p.extend(0.0));
                                (q.y / ry.max(1e-9)).atan2(q.x / rx.max(1e-9))
                            }
                        }
                        (Some(p), None) => {
                            let q = inv.transform_point3(p.extend(0.0));
                            (q.y / ry.max(1e-9)).atan2(q.x / rx.max(1e-9))
                        }
                        (None, Some(a)) => a * self.angle,
                        (None, None) => 0.0,
                    }
                };
                let (a0, mut a1) = (angle_of(t1), angle_of(t2));
                if sense {
                    while a1 <= a0 + 1e-9 {
                        a1 += std::f64::consts::TAU;
                    }
                } else {
                    while a1 >= a0 - 1e-9 {
                        a1 -= std::f64::consts::TAU;
                    }
                }
                arc(DVec2::ZERO, rx, ry, a0, a1)
                    .iter()
                    .map(|p| tr.transform_point3(p.extend(0.0)).truncate())
                    .collect()
            }
            "IFCLINE" => {
                let base = self.curve(c.arg(0).as_ref().unwrap_or(0));
                let (Some(&p0), Some(&p1)) = (base.first(), base.get(1)) else {
                    return vec![];
                };
                let dir = p1 - p0;
                let at = |t: (Option<DVec2>, Option<f64>)| match t {
                    (Some(p), _) => p,
                    (None, Some(u)) => p0 + dir * u,
                    _ => p0,
                };
                let (a, b) = (at(t1), at(t2));
                if sense {
                    vec![a, b]
                } else {
                    vec![b, a]
                }
            }
            _ => self.curve(c.arg(0).as_ref().unwrap_or(0)),
        }
    }

    /// The outer outline of a profile, in its own plane.
    fn profile(&self, id: u32) -> Vec<Vec<DVec2>> {
        let Some(p) = self.e(id) else { return vec![] };
        let s = self.scale;
        let place = |pts: Vec<DVec2>, at: &Value| -> Vec<DVec2> {
            let t = self.axis_placement(at);
            pts.iter().map(|q| t.transform_point3(q.extend(0.0)).truncate()).collect()
        };
        let rect = |w: f64, h: f64| {
            vec![
                DVec2::new(-w / 2.0, -h / 2.0),
                DVec2::new(w / 2.0, -h / 2.0),
                DVec2::new(w / 2.0, h / 2.0),
                DVec2::new(-w / 2.0, h / 2.0),
            ]
        };
        let num = |i: usize| p.arg(i).as_f64().unwrap_or(0.0) * s;
        let outer = match p.name.as_str() {
            "IFCARBITRARYCLOSEDPROFILEDEF" | "IFCARBITRARYPROFILEDEFWITHVOIDS" => {
                p.arg(2).as_ref().map(|c| self.curve(c)).unwrap_or_default()
            }
            "IFCRECTANGLEPROFILEDEF" | "IFCRECTANGLEHOLLOWPROFILEDEF" | "IFCROUNDEDRECTANGLEPROFILEDEF" => {
                place(rect(num(3), num(4)), p.arg(2))
            }
            "IFCCIRCLEPROFILEDEF" | "IFCCIRCLEHOLLOWPROFILEDEF" => {
                let mut c = arc(DVec2::ZERO, num(3), num(3), 0.0, std::f64::consts::TAU);
                c.pop();
                place(c, p.arg(2))
            }
            "IFCELLIPSEPROFILEDEF" => {
                let mut c = arc(DVec2::ZERO, num(3), num(4), 0.0, std::f64::consts::TAU);
                c.pop();
                place(c, p.arg(2))
            }
            "IFCISHAPEPROFILEDEF" => {
                let (b, d, tw, tf) = (num(3), num(4), num(5), num(6));
                let (hb, hd, ht) = (b / 2.0, d / 2.0, tw / 2.0);
                let pts = vec![
                    DVec2::new(-hb, -hd),
                    DVec2::new(hb, -hd),
                    DVec2::new(hb, -hd + tf),
                    DVec2::new(ht, -hd + tf),
                    DVec2::new(ht, hd - tf),
                    DVec2::new(hb, hd - tf),
                    DVec2::new(hb, hd),
                    DVec2::new(-hb, hd),
                    DVec2::new(-hb, hd - tf),
                    DVec2::new(-ht, hd - tf),
                    DVec2::new(-ht, -hd + tf),
                    DVec2::new(-hb, -hd + tf),
                ];
                place(pts, p.arg(2))
            }
            "IFCLSHAPEPROFILEDEF" => {
                let (d, b, t) = (num(3), if p.arg(4).as_f64().is_some() { num(4) } else { num(3) }, num(5));
                let pts = vec![
                    DVec2::new(-b / 2.0, -d / 2.0),
                    DVec2::new(b / 2.0, -d / 2.0),
                    DVec2::new(b / 2.0, -d / 2.0 + t),
                    DVec2::new(-b / 2.0 + t, -d / 2.0 + t),
                    DVec2::new(-b / 2.0 + t, d / 2.0),
                    DVec2::new(-b / 2.0, d / 2.0),
                ];
                place(pts, p.arg(2))
            }
            "IFCUSHAPEPROFILEDEF" | "IFCCSHAPEPROFILEDEF" | "IFCTSHAPEPROFILEDEF" | "IFCZSHAPEPROFILEDEF" => {
                // Approximated by their bounding rectangle.
                let (d, b) = (num(3), num(4));
                place(rect(b, d), p.arg(2))
            }
            "IFCDERIVEDPROFILEDEF" => {
                let parent = p.arg(2).as_ref().map(|r| self.profile(r)).unwrap_or_default();
                let t = self.operator(p.arg(3));
                return parent
                    .into_iter()
                    .map(|o| o.iter().map(|q| t.transform_point3(q.extend(0.0)).truncate()).collect())
                    .collect();
            }
            "IFCCOMPOSITEPROFILEDEF" => {
                return p.arg(2).refs().into_iter().flat_map(|r| self.profile(r)).collect();
            }
            _ => vec![],
        };
        let outer = dedupe(outer);
        if outer.len() >= 3 {
            vec![outer]
        } else {
            vec![]
        }
    }

    /// The style of an item, or the one inherited from its parent.
    fn style_of(&self, item: u32, inherited: &Option<Style>) -> Option<Style> {
        self.styles.get(&item).cloned().or_else(|| inherited.clone())
    }

    /// Solids of a representation item, in model coordinates.
    fn item(&self, id: u32, t: DAffine3, style: &Option<Style>, out: &mut Vec<Piece>, depth: usize) {
        if depth > 12 {
            return;
        }
        let Some(it) = self.e(id) else { return };
        let style = self.style_of(id, style);
        match it.name.as_str() {
            "IFCEXTRUDEDAREASOLID" | "IFCEXTRUDEDAREASOLIDTAPERED" => {
                let local = t * self.axis_placement(it.arg(1));
                let dir = self.direction(it.arg(2)).unwrap_or(DVec3::Z);
                let depth = it.arg(3).as_f64().unwrap_or(0.0) * self.scale;
                let ext = local.matrix3 * (dir * depth);
                for outer in it.arg(0).as_ref().map(|p| self.profile(p)).unwrap_or_default() {
                    let pts: Vec<DVec3> = outer.iter().map(|q| local.transform_point3(q.extend(0.0))).collect();
                    if let Some(s) = extrusion_solid(&pts, ext) {
                        out.push(Piece {
                            solid: s,
                            style: style.clone(),
                        });
                    }
                }
            }
            "IFCFACETEDBREP" | "IFCFACETEDBREPWITHVOIDS" | "IFCADVANCEDBREP" => {
                if let Some(shell) = it.arg(0).as_ref() {
                    let faces = self.shell(shell, t);
                    if !faces.is_empty() {
                        out.push(Piece {
                            solid: Solid::Faces(faces),
                            style,
                        });
                    }
                }
            }
            "IFCFACEBASEDSURFACEMODEL" | "IFCSHELLBASEDSURFACEMODEL" => {
                let faces: Vec<Vec<DVec3>> = it.arg(0).refs().into_iter().flat_map(|s| self.shell(s, t)).collect();
                if !faces.is_empty() {
                    out.push(Piece {
                        solid: Solid::Faces(faces),
                        style,
                    });
                }
            }
            "IFCCLOSEDSHELL" | "IFCOPENSHELL" | "IFCCONNECTEDFACESET" => {
                let faces = self.shell(id, t);
                if !faces.is_empty() {
                    out.push(Piece {
                        solid: Solid::Faces(faces),
                        style,
                    });
                }
            }
            "IFCPOLYGONALFACESET" | "IFCTRIANGULATEDFACESET" | "IFCTRIANGULATEDIRREGULARNETWORK" => {
                let faces = self.face_set(it, t);
                if !faces.is_empty() {
                    out.push(Piece {
                        solid: Solid::Faces(faces),
                        style,
                    });
                }
            }
            "IFCMAPPEDITEM" => {
                let Some(map) = self.deref(it.arg(0)) else { return };
                let mt = t * self.operator(it.arg(1)) * self.axis_placement(map.arg(0));
                if let Some(rep) = self.deref(map.arg(1)) {
                    for i in rep.arg(3).refs() {
                        self.item(i, mt, &style, out, depth + 1);
                    }
                }
            }
            "IFCBOOLEANCLIPPINGRESULT" | "IFCBOOLEANRESULT" => {
                let op = it.arg(0).as_enum().unwrap_or("DIFFERENCE");
                let mut first = Vec::new();
                if let Some(a) = it.arg(1).as_ref() {
                    self.item(a, t, &style, &mut first, depth + 1);
                }
                match op {
                    "UNION" => {
                        out.extend(first);
                        if let Some(b) = it.arg(2).as_ref() {
                            self.item(b, t, &style, out, depth + 1);
                        }
                    }
                    "DIFFERENCE" => {
                        // Half-space cuts (walls under roofs) are applied;
                        // other subtracted solids are ignored.
                        let plane = it.arg(2).as_ref().and_then(|b| self.half_space(b, t));
                        for p in first {
                            match &plane {
                                Some(plane) => {
                                    let faces = clip_faces(&p.solid.faces(), plane);
                                    if faces.len() >= 3 {
                                        out.push(Piece {
                                            solid: Solid::Faces(faces),
                                            style: p.style,
                                        });
                                    }
                                }
                                None => out.push(p),
                            }
                        }
                    }
                    _ => out.extend(first),
                }
            }
            "IFCBOUNDINGBOX" => {
                let c = self.point3(it.arg(0));
                let (x, y, z) = (
                    it.arg(1).as_f64().unwrap_or(0.0) * self.scale,
                    it.arg(2).as_f64().unwrap_or(0.0) * self.scale,
                    it.arg(3).as_f64().unwrap_or(0.0) * self.scale,
                );
                let base: Vec<DVec3> = [(0.0, 0.0), (x, 0.0), (x, y), (0.0, y)]
                    .iter()
                    .map(|(a, b)| t.transform_point3(c + DVec3::new(*a, *b, 0.0)))
                    .collect();
                if let Some(s) = extrusion_solid(&base, t.matrix3 * DVec3::new(0.0, 0.0, z)) {
                    out.push(Piece { solid: s, style });
                }
            }
            "IFCCSGSOLID" => {
                if let Some(r) = it.arg(0).as_ref() {
                    self.item(r, t, &style, out, depth + 1);
                }
            }
            "IFCBLOCK" => {
                let local = t * self.axis_placement(it.arg(0));
                let (x, y, z) = (
                    it.arg(1).as_f64().unwrap_or(0.0) * self.scale,
                    it.arg(2).as_f64().unwrap_or(0.0) * self.scale,
                    it.arg(3).as_f64().unwrap_or(0.0) * self.scale,
                );
                let base: Vec<DVec3> = [(0.0, 0.0), (x, 0.0), (x, y), (0.0, y)]
                    .iter()
                    .map(|(a, b)| local.transform_point3(DVec3::new(*a, *b, 0.0)))
                    .collect();
                if let Some(s) = extrusion_solid(&base, local.matrix3 * DVec3::new(0.0, 0.0, z)) {
                    out.push(Piece { solid: s, style });
                }
            }
            _ => {}
        }
    }

    /// The plane of a half-space solid, oriented towards what a
    /// difference keeps.
    fn half_space(&self, id: u32, t: DAffine3) -> Option<Plane> {
        let h = self.e(id)?;
        if !h.name.contains("HALFSPACESOLID") {
            return None;
        }
        let surface = self.deref(h.arg(0))?;
        if surface.name != "IFCPLANE" {
            return None;
        }
        let pt = t * self.axis_placement(surface.arg(0));
        let n = pt.matrix3.z_axis.normalize();
        let o = pt.translation;
        // AgreementFlag true: the half space (the removed material) lies
        // against the normal, so a difference keeps the normal side.
        let agree = h.arg(1).as_enum() != Some("F");
        Some(Plane::from_point_normal(o, if agree { n } else { -n }))
    }

    fn shell(&self, id: u32, t: DAffine3) -> Vec<Vec<DVec3>> {
        let Some(sh) = self.e(id) else { return vec![] };
        let mut faces = Vec::new();
        for f in sh.arg(0).refs() {
            let Some(face) = self.e(f) else { continue };
            // The outer bound (or the first bound if none is marked).
            let bounds = face.arg(0).refs();
            let bound = bounds
                .iter()
                .filter_map(|b| self.e(*b))
                .find(|b| b.name == "IFCFACEOUTERBOUND")
                .or_else(|| bounds.first().and_then(|b| self.e(*b)));
            let Some(bound) = bound else { continue };
            let Some(lp) = self.deref(bound.arg(0)) else { continue };
            if lp.name != "IFCPOLYLOOP" {
                continue;
            }
            let mut pts: Vec<DVec3> = lp.arg(0).as_list().iter().map(|p| t.transform_point3(self.point3(p))).collect();
            if bound.arg(1).as_enum() == Some("F") {
                pts.reverse();
            }
            if pts.len() >= 3 {
                faces.push(pts);
            }
        }
        faces
    }

    fn face_set(&self, it: &Entity, t: DAffine3) -> Vec<Vec<DVec3>> {
        let coords: Vec<DVec3> = self
            .deref(it.arg(0))
            .map(|l| {
                l.arg(0)
                    .as_list()
                    .iter()
                    .map(|p| {
                        let v: Vec<f64> = p.as_list().iter().filter_map(|x| x.as_f64()).collect();
                        t.transform_point3(
                            DVec3::new(
                                v.first().copied().unwrap_or(0.0),
                                v.get(1).copied().unwrap_or(0.0),
                                v.get(2).copied().unwrap_or(0.0),
                            ) * self.scale,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let (faces_arg, pn_arg) = if it.name == "IFCPOLYGONALFACESET" { (2, 3) } else { (3, 4) };
        let pn: Vec<usize> = it
            .arg(pn_arg)
            .as_list()
            .iter()
            .filter_map(|v| v.as_f64())
            .map(|v| v as usize)
            .collect();
        let at = |i: f64| -> Option<DVec3> {
            let k = i as usize;
            let k = if pn.is_empty() { k } else { *pn.get(k.checked_sub(1)?)? };
            coords.get(k.checked_sub(1)?).copied()
        };
        let mut out = Vec::new();
        if it.name == "IFCPOLYGONALFACESET" {
            for f in it.arg(faces_arg).refs() {
                let Some(face) = self.e(f) else { continue };
                let pts: Vec<DVec3> = face.arg(0).as_list().iter().filter_map(|v| v.as_f64()).filter_map(at).collect();
                if pts.len() >= 3 {
                    out.push(pts);
                }
            }
        } else {
            for tri in it.arg(faces_arg).as_list() {
                let pts: Vec<DVec3> = tri.as_list().iter().filter_map(|v| v.as_f64()).filter_map(at).collect();
                if pts.len() == 3 {
                    out.push(pts);
                }
            }
        }
        out
    }

    /// The representation of a product with this identifier.
    fn representation(&self, e: &Entity, ids: &[&str]) -> Option<&'a Entity> {
        let shape = self.deref(e.arg(6))?;
        let reps: Vec<&Entity> = shape.arg(2).refs().into_iter().filter_map(|r| self.e(r)).collect();
        ids.iter()
            .find_map(|id| reps.iter().find(|r| r.arg(1).as_str().is_some_and(|s| s.eq_ignore_ascii_case(id))))
            .copied()
    }

    /// The body solids of a product, in model coordinates.
    fn body(&self, e: &Entity) -> Vec<Piece> {
        let t = self.object_placement(e);
        let rep = self
            .representation(e, &["Body", "Facetation", "Mesh"])
            .or_else(|| {
                let shape = self.deref(e.arg(6))?;
                shape.arg(2).refs().into_iter().filter_map(|r| self.e(r)).find(|r| {
                    !matches!(
                        r.arg(1).as_str().unwrap_or(""),
                        "Axis" | "FootPrint" | "Box" | "Annotation" | "Profile" | "Reference"
                    )
                })
            })
            .or_else(|| self.representation(e, &["Box"]));
        let mut out = Vec::new();
        if let Some(rep) = rep {
            for i in rep.arg(3).refs() {
                self.item(i, t, &None, &mut out, 0);
            }
        }
        out
    }

    /// The body as one vertical extrusion, if that is all it is (walls,
    /// slabs, columns). Clipping by half spaces is ignored.
    fn vertical_extrusion(&self, e: &Entity) -> Option<Extrusion> {
        let mut t = self.object_placement(e);
        let rep = self.representation(e, &["Body"])?;
        let mut items = rep.arg(3).refs();
        let mut it;
        let mut guard = 0;
        loop {
            if items.len() != 1 || guard > 10 {
                return None;
            }
            guard += 1;
            it = self.e(items[0])?;
            match it.name.as_str() {
                "IFCBOOLEANCLIPPINGRESULT" | "IFCBOOLEANRESULT" => items = vec![it.arg(1).as_ref()?],
                "IFCMAPPEDITEM" => {
                    let map = self.deref(it.arg(0))?;
                    t = t * self.operator(it.arg(1)) * self.axis_placement(map.arg(0));
                    items = self.deref(map.arg(1))?.arg(3).refs();
                }
                "IFCEXTRUDEDAREASOLID" => break,
                _ => return None,
            }
        }
        let local = t * self.axis_placement(it.arg(1));
        let dir = self.direction(it.arg(2)).unwrap_or(DVec3::Z);
        let depth = it.arg(3).as_f64().unwrap_or(0.0) * self.scale;
        let ext = local.matrix3 * (dir * depth);
        // The profile must lie flat and the extrusion go straight up or down.
        if ext.truncate().length() > 1e-3 * ext.length().max(1.0) || local.matrix3.z_axis.truncate().length() > 1e-6 {
            return None;
        }
        let pid = it.arg(0).as_ref()?;
        let profiles = self.profile(pid);
        if profiles.len() != 1 {
            return None;
        }
        let pts: Vec<DVec3> = profiles[0].iter().map(|q| local.transform_point3(q.extend(0.0))).collect();
        let z = pts[0].z;
        let (z0, z1) = if ext.z >= 0.0 { (z, z + ext.z) } else { (z + ext.z, z) };
        Some(Extrusion {
            footprint: pts.iter().map(|p| p.truncate()).collect(),
            z0,
            z1,
        })
    }

    // ----- levels and grids ------------------------------------------------------

    fn storeys(&mut self) {
        let mut storeys: Vec<(u32, String, f64)> = self
            .f
            .all("IFCBUILDINGSTOREY")
            .into_iter()
            .filter_map(|id| {
                let s = self.e(id)?;
                let from_placement = self.object_placement(s).translation.z;
                let elev = if s.arg(5).as_ref().is_some() {
                    from_placement
                } else {
                    s.arg(9).as_f64().unwrap_or(0.0) * self.scale
                };
                let name = s
                    .arg(2)
                    .as_str()
                    .filter(|n| !n.is_empty())
                    .map(String::from)
                    .unwrap_or_else(|| format!("Level {id}"));
                Some((id, name, elev))
            })
            .collect();
        storeys.sort_by(|a, b| a.2.total_cmp(&b.2));
        if storeys.is_empty() {
            storeys.push((0, "Level 1".into(), 0.0));
        }
        let mut used: HashSet<String> = HashSet::new();
        for (id, name, elev) in storeys {
            let mut n = name.clone();
            let mut k = 2;
            while !used.insert(n.clone()) {
                n = format!("{name} ({k})");
                k += 1;
            }
            let l = hausit_model::template::add_level_with_plan(&mut self.doc, &n, elev.round());
            self.levels.push((id, l, elev));
        }
        self.report.levels = self.levels.len();
    }

    /// The storey an element belongs to: its container (or its whole's),
    /// else the storey just below its base.
    fn level_of(&self, id: u32, z: f64) -> ElementId {
        let mut cur = id;
        for _ in 0..10 {
            if let Some(s) = self.container.get(&cur) {
                let mut s = *s;
                // Spaces and other spatial parts: up to their storey.
                for _ in 0..5 {
                    if let Some(l) = self.levels.iter().find(|l| l.0 == s) {
                        return l.1;
                    }
                    match self.parent.get(&s) {
                        Some(p) => s = *p,
                        None => break,
                    }
                }
                break;
            }
            match self.parent.get(&cur) {
                Some(p) => cur = *p,
                None => break,
            }
        }
        self.level_below(z)
    }

    fn level_below(&self, z: f64) -> ElementId {
        self.levels
            .iter()
            .rev()
            .find(|l| l.2 <= z + 10.0)
            .or(self.levels.first())
            .map(|l| l.1)
            .unwrap_or_default()
    }

    fn level_elev(&self, l: ElementId) -> f64 {
        self.doc.level_elevation(l)
    }

    fn grids(&mut self) {
        for gid in self.f.all("IFCGRID") {
            let Some(g) = self.e(gid) else { continue };
            let t = self.object_placement(g);
            for axes in [7, 8, 9] {
                for a in g.arg(axes).refs() {
                    let Some(ax) = self.e(a) else { continue };
                    let pts = ax.arg(1).as_ref().map(|c| self.curve(c)).unwrap_or_default();
                    let (Some(&p0), Some(&p1)) = (pts.first(), pts.last()) else {
                        continue;
                    };
                    if p0.distance(p1) < 1.0 {
                        continue;
                    }
                    let w = |p: DVec2| t.transform_point3(p.extend(0.0)).truncate();
                    let name = ax.arg(0).as_str().unwrap_or("").to_string();
                    self.doc.add_element(ElementData::Grid(Grid {
                        name,
                        start: w(p0),
                        end: w(p1),
                    }));
                    self.report.grids += 1;
                }
            }
        }
    }

    // ----- materials and types -------------------------------------------------

    /// A Hausit material for an IFC material or style name, created with
    /// a fitting colour and cut pattern if the project lacks it.
    fn ensure_material(&mut self, name: &str, style: Option<&Style>) -> String {
        let name = name.trim();
        let name = if name.is_empty() { "Imported" } else { name };
        let transparent = style.is_some_and(|s| s.transparency > 0.3);
        let name = if transparent && !name.to_ascii_lowercase().contains("glass") {
            format!("Glass - {name}")
        } else {
            name.to_string()
        };
        if self.doc.material(&name).is_some() {
            return name;
        }
        let lower = name.to_ascii_lowercase();
        let has = |k: &[&str]| k.iter().any(|w| lower.contains(w));
        let (guess, pattern) = if has(&["concrete", "cmu", "block"]) {
            ([190, 190, 186], FillPattern::Concrete)
        } else if has(&["brick", "masonry"]) {
            ([176, 98, 70], FillPattern::Diagonal)
        } else if has(&["insulation", "wool", "batt"]) {
            ([238, 222, 140], FillPattern::Insulation)
        } else if has(&["gypsum", "plaster", "drywall"]) {
            ([244, 244, 242], FillPattern::None)
        } else if has(&["wood", "timber", "stud", "plywood", "osb"]) {
            ([206, 168, 118], FillPattern::Parallel)
        } else if has(&["steel", "metal", "alumin"]) {
            ([160, 166, 172], FillPattern::Solid)
        } else if has(&["glass", "glazing"]) {
            ([180, 210, 225], FillPattern::None)
        } else {
            ([205, 205, 200], FillPattern::None)
        };
        let color = style
            .map(|s| s.color)
            .or_else(|| self.material_styles.get(&name).map(|s| s.color))
            .unwrap_or(guess);
        self.doc.materials.push(MaterialDef {
            name: name.clone(),
            color,
            cut_pattern: pattern,
            pattern_color: if pattern == FillPattern::Solid {
                [50, 50, 50]
            } else {
                [96, 96, 96]
            },
            surface_pattern: SurfacePattern::None,
        });
        name
    }

    /// The layers of an element (or its type): (material, thickness).
    fn layers(&self, id: u32) -> Vec<(String, f64)> {
        let m = self
            .materials
            .get(&id)
            .or_else(|| self.types.get(&id).and_then(|t| self.materials.get(t)));
        let Some(mut m) = m.and_then(|m| self.e(*m)) else { return vec![] };
        if m.name == "IFCMATERIALLAYERSETUSAGE" {
            match self.deref(m.arg(0)) {
                Some(s) => m = s,
                None => return vec![],
            }
        }
        if m.name != "IFCMATERIALLAYERSET" {
            return vec![];
        }
        m.arg(0)
            .refs()
            .into_iter()
            .filter_map(|l| self.e(l))
            .map(|l| {
                let name = self
                    .deref(l.arg(0))
                    .and_then(|m| m.arg(0).as_str())
                    .unwrap_or("Imported")
                    .to_string();
                (name, l.arg(1).as_f64().unwrap_or(0.0) * self.scale)
            })
            .filter(|(_, t)| *t > 0.0)
            .collect()
    }

    /// The single material of an element, if it has one.
    fn material_name(&self, id: u32) -> Option<String> {
        let m = self
            .materials
            .get(&id)
            .or_else(|| self.types.get(&id).and_then(|t| self.materials.get(t)))?;
        let m = self.e(*m)?;
        match m.name.as_str() {
            "IFCMATERIAL" => m.arg(0).as_str().map(String::from),
            "IFCMATERIALLIST" => m
                .arg(0)
                .refs()
                .first()
                .and_then(|r| self.e(*r))
                .and_then(|x| x.arg(0).as_str())
                .map(String::from),
            _ => self.layers(id).into_iter().max_by(|a, b| a.1.total_cmp(&b.1)).map(|l| l.0),
        }
    }

    /// Family and type names: Revit writes `Family:Type:Id` as the name;
    /// otherwise the type object's name or the object type.
    fn names(&self, e: &Entity, id: u32) -> (String, String) {
        let name = e.arg(2).as_str().unwrap_or("");
        let parts: Vec<&str> = name.split(':').collect();
        if parts.len() >= 3 {
            return (parts[0].to_string(), parts[1].to_string());
        }
        let type_name = self
            .types
            .get(&id)
            .and_then(|t| self.e(*t))
            .and_then(|t| t.arg(2).as_str())
            .or(e.arg(4).as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(if name.is_empty() { "Imported" } else { name });
        let tparts: Vec<&str> = type_name.split(':').collect();
        if tparts.len() >= 2 {
            return (tparts[0].to_string(), tparts[1].to_string());
        }
        ("Imported".into(), type_name.to_string())
    }

    /// A type for an imported element, shared by elements with the same
    /// names and size.
    fn ensure_type(
        &mut self,
        cat: Category,
        family: &str,
        name: &str,
        key: &str,
        make: impl FnOnce(&mut Self) -> (TypeParams, Vec<Layer>),
    ) -> ElementId {
        let k = format!("{cat:?}|{family}|{name}|{key}");
        if let Some(t) = self.type_keys.get(&k) {
            return *t;
        }
        let (params, layers) = make(self);
        // Keep type names unique within the category.
        let mut type_name = name.to_string();
        let mut i = 2;
        while self.doc.types_of(cat).any(|t| t.family == family && t.name == type_name) {
            type_name = format!("{name} ({i})");
            i += 1;
        }
        let id = self.doc.add_type(cat, family, &type_name, params);
        if !layers.is_empty() {
            if let Some(t) = self.doc.family_type_mut(id) {
                t.set_layers(layers);
            }
        }
        self.type_keys.insert(k, id);
        id
    }

    fn layer_list(&mut self, id: u32, thickness: f64) -> Vec<Layer> {
        let layers = self.layers(id);
        let total: f64 = layers.iter().map(|l| l.1).sum();
        if layers.is_empty() || (total - thickness).abs() > 2.0 {
            return vec![];
        }
        let thickest = layers.iter().enumerate().max_by(|a, b| a.1 .1.total_cmp(&b.1 .1)).map(|(i, _)| i);
        layers
            .iter()
            .enumerate()
            .map(|(i, (m, t))| {
                let style = self.material_styles.get(m).cloned();
                let m = self.ensure_material(m, style.as_ref());
                let f = if Some(i) == thickest {
                    LayerFunction::Structure
                } else if i == 0 {
                    LayerFunction::Finish1
                } else {
                    LayerFunction::Finish2
                };
                Layer::new(f, &m, *t)
            })
            .collect()
    }

    // ----- elements --------------------------------------------------------------

    fn element(&mut self, id: u32) {
        let Some(e) = self.e(id) else { return };
        let done = match e.name.as_str() {
            "IFCWALL" | "IFCWALLSTANDARDCASE" | "IFCWALLELEMENTEDCASE" => self.wall(id, e),
            "IFCSLAB" | "IFCSLABSTANDARDCASE" if e.arg(8).as_enum() != Some("ROOF") && !self.part_of(id, "IFCROOF") => self.floor(id, e),
            "IFCCOLUMN" | "IFCCOLUMNSTANDARDCASE" => self.column(id, e),
            "IFCDOOR" | "IFCDOORSTANDARDCASE" | "IFCWINDOW" | "IFCWINDOWSTANDARDCASE" => self.opening(id, e),
            _ => false,
        };
        if !done {
            self.direct_shape(id, e);
        }
    }

    fn part_of(&self, id: u32, class: &str) -> bool {
        self.parent.get(&id).and_then(|p| self.e(*p)).is_some_and(|p| p.name == class)
    }

    /// A body made of several upright parts (layers, or pieces around
    /// openings) as one extrusion of their outline.
    fn upright_hull(&self, e: &Entity) -> Option<Extrusion> {
        let pieces = self.body(e);
        if pieces.is_empty() {
            return None;
        }
        let pts: Vec<DVec3> = pieces.iter().flat_map(|p| p.solid.points()).collect();
        // Upright sides and a flat bottom; the top may slope (walls
        // under a roof).
        let upright = pieces.iter().all(|p| match &p.solid {
            Solid::Prism { .. } => true,
            Solid::Faces(f) => f.iter().all(|f| {
                let n = polygon_normal(f).normalize_or_zero();
                n.z.abs() < 1e-3 || n.z > 0.05 || n.truncate().length() < 1e-3
            }),
        });
        if !upright {
            return None;
        }
        let (z0, z1) = pts
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.z), hi.max(p.z)));
        let footprint = convex_hull(pts.iter().map(|p| p.truncate()).collect());
        (footprint.len() >= 3 && z1 - z0 > 1.0).then_some(Extrusion { footprint, z0, z1 })
    }

    fn wall(&mut self, id: u32, e: &Entity) -> bool {
        let Some(ex) = self.vertical_extrusion(e).or_else(|| self.upright_hull(e)) else {
            return false;
        };
        let t = self.object_placement(e);
        // The wall's axis, or the long side of its outline.
        let axis = self.representation(e, &["Axis"]).and_then(|r| {
            let c = r.arg(3).refs().first().copied()?;
            let pts = self.curve(c);
            (pts.len() == 2).then(|| {
                let w = |p: DVec2| t.transform_point3(p.extend(0.0)).truncate();
                (w(pts[0]), w(pts[1]))
            })
        });
        let (a, b) = match axis {
            Some(ab) => ab,
            None => {
                let fp = &ex.footprint;
                let longest = (0..fp.len()).max_by(|i, j| {
                    let l = |k: usize| fp[k].distance(fp[(k + 1) % fp.len()]);
                    l(*i).total_cmp(&l(*j))
                });
                let Some(i) = longest else { return false };
                (fp[i], fp[(i + 1) % fp.len()])
            }
        };
        let dir = (b - a).normalize_or_zero();
        if dir == DVec2::ZERO {
            return false;
        }
        // The outline across the axis gives the thickness and centreline;
        // a curved or odd-shaped outline stays a direct shape.
        let n = perp(dir);
        let across: Vec<f64> = ex.footprint.iter().map(|p| (*p - a).dot(n)).collect();
        let along: Vec<f64> = ex.footprint.iter().map(|p| (*p - a).dot(dir)).collect();
        let (n0, n1) = across
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
        let (s0, s1) = along
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
        let thickness = n1 - n0;
        let rect_area = thickness * (s1 - s0);
        let area = polygon_area(&ex.footprint).abs();
        if thickness < 1.0 || rect_area <= 0.0 || area < rect_area * 0.8 || (s1 - s0) < thickness {
            return false;
        }
        let shift = n * (n0 + n1) / 2.0;
        let (mut start, mut end) = if axis.is_some() {
            (a + shift, b + shift)
        } else {
            (a + dir * s0 + shift, a + dir * s1 + shift)
        };
        // Exterior walls: Hausit keeps the exterior on the left, so turn
        // walls whose left side faces into the building.
        if self.external.contains(&id) {
            if let Some(c) = self.building_centre() {
                let mid = (start + end) / 2.0;
                if (c - mid).dot(perp((end - start).normalize())) > 0.0 {
                    std::mem::swap(&mut start, &mut end);
                }
            }
        }
        let thickness = (thickness * 10.0).round() / 10.0;
        let (family, type_name) = self.names(e, id);
        let layers = self.layer_list(id, thickness);
        let external = self.external.contains(&id);
        let type_id = self.ensure_type(Category::Walls, &family, &type_name, &format!("{thickness}"), |_| {
            let mut p = hausit_model::family::default_type_params(Category::Walls);
            p.insert("Width".into(), ParamValue::Length(thickness));
            p.insert(
                "Function".into(),
                ParamValue::Text(if external { "Exterior" } else { "Interior" }.into()),
            );
            (p, layers)
        });
        let level = self.level_of(id, ex.z0);
        let wall = Wall {
            type_id,
            start,
            end,
            base_level: level,
            base_offset: ex.z0 - self.level_elev(level),
            top: TopConstraint::Unconnected { height: ex.z1 - ex.z0 },
            structural: false,
            room_bounding: true,
            attached_top: vec![],
        };
        let w = self.doc.add_element(ElementData::Wall(wall));
        let sloped = self.body(e).iter().any(|p| match &p.solid {
            Solid::Faces(f) => f.iter().any(|f| {
                let n = polygon_normal(f).normalize_or_zero();
                n.z > 0.05 && n.z < 0.999
            }),
            Solid::Prism { .. } => false,
        });
        if sloped {
            self.sloped_walls.push(w);
        }
        self.walls.insert(id, w);
        self.report.walls += 1;
        true
    }

    /// Attaches walls with a sloping top to the imported roofs over them,
    /// so the wall follows the roof as it did in the source model.
    fn attach_walls(&mut self) {
        let roofs: Vec<(ElementId, Vec<DVec2>, f64)> = self
            .doc
            .elements()
            .filter_map(|e| match &e.data {
                ElementData::DirectShape(d) if d.category == Category::Roofs => {
                    let pts: Vec<DVec3> = d
                        .solids
                        .iter()
                        .flat_map(|s| match &s.geometry {
                            ShapeGeometry::Faces(f) => f.iter().flatten().copied().collect::<Vec<_>>(),
                            ShapeGeometry::Prism { footprint, z0, .. } => footprint.iter().map(|p| DVec3::new(p.x, p.y, *z0)).collect(),
                        })
                        .collect();
                    let zmin = pts.iter().map(|p| p.z).fold(f64::INFINITY, f64::min);
                    Some((e.id, convex_hull(pts.iter().map(|p| p.truncate()).collect()), zmin))
                }
                _ => None,
            })
            .collect();
        for w in std::mem::take(&mut self.sloped_walls) {
            let Some(wall) = self.doc.element(w).and_then(|e| e.as_wall()).cloned() else {
                continue;
            };
            let top = self.level_elev(wall.base_level)
                + wall.base_offset
                + match wall.top {
                    TopConstraint::Unconnected { height } => height,
                    _ => 0.0,
                };
            let samples = [0.05, 0.5, 0.95].map(|t| wall.start.lerp(wall.end, t));
            let over: Vec<ElementId> = roofs
                .iter()
                .filter(|(_, hull, zmin)| *zmin < top + 1.0 && samples.iter().any(|p| point_in_polygon(*p, hull)))
                .map(|r| r.0)
                .collect();
            if let Some(ElementData::Wall(wall)) = self.doc.element_mut(w).map(|e| &mut e.data) {
                wall.attached_top = over;
            }
        }
    }

    /// The centre of all walls marked external, for orienting them.
    fn building_centre(&self) -> Option<DVec2> {
        thread_local! {
            static CACHE: RefCell<Option<(usize, Option<DVec2>)>> = const { RefCell::new(None) };
        }
        let key = self as *const _ as usize;
        if let Some((k, c)) = CACHE.with(|c| *c.borrow()) {
            if k == key {
                return c;
            }
        }
        let pts: Vec<DVec2> = self
            .external
            .iter()
            .filter_map(|id| self.e(*id))
            .filter(|e| e.name.starts_with("IFCWALL"))
            .map(|e| self.object_placement(e).translation.truncate())
            .collect();
        let c = (!pts.is_empty()).then(|| pts.iter().copied().sum::<DVec2>() / pts.len() as f64);
        CACHE.with(|cell| *cell.borrow_mut() = Some((key, c)));
        c
    }

    fn floor(&mut self, id: u32, e: &Entity) -> bool {
        let Some(ex) = self.vertical_extrusion(e) else { return false };
        let thickness = ((ex.z1 - ex.z0) * 10.0).round() / 10.0;
        if thickness < 1.0 || ex.footprint.len() < 3 {
            return false;
        }
        // The level whose elevation is nearest the slab's top.
        let contained = self.level_of(id, ex.z1);
        let level = self
            .levels
            .iter()
            .min_by(|a, b| (a.2 - ex.z1).abs().total_cmp(&(b.2 - ex.z1).abs()))
            .filter(|l| (l.2 - ex.z1).abs() < 1500.0)
            .map(|l| l.1)
            .unwrap_or(contained);
        let (family, type_name) = self.names(e, id);
        let layers = self.layer_list(id, thickness);
        let type_id = self.ensure_type(Category::Floors, &family, &type_name, &format!("{thickness}"), |_| {
            let mut p = hausit_model::family::default_type_params(Category::Floors);
            p.insert("Thickness".into(), ParamValue::Length(thickness));
            (p, layers)
        });
        let mut boundary = ex.footprint.clone();
        if polygon_area(&boundary) < 0.0 {
            boundary.reverse();
        }
        self.doc.add_element(ElementData::Floor(Floor {
            type_id,
            level,
            boundary,
            height_offset: ex.z1 - self.level_elev(level),
            room_bounding: true,
            structural: false,
        }));
        self.report.floors += 1;
        true
    }

    fn column(&mut self, id: u32, e: &Entity) -> bool {
        let Some(ex) = self.vertical_extrusion(e) else { return false };
        let Some((centre, xdir, section)) = section_of(&ex.footprint) else {
            return false;
        };
        let (family, type_name) = self.names(e, id);
        let material = self.material_name(id);
        let r = |v: f64| (v * 10.0).round() / 10.0;
        let key = format!("{section:?}");
        let type_id = self.ensure_type(Category::StructuralColumns, &family, &type_name, &key, |imp| {
            let mut p = hausit_model::family::default_type_params(Category::StructuralColumns);
            match section {
                Section::Rect { b, h } => {
                    p.insert("b".into(), ParamValue::Length(r(b)));
                    p.insert("h".into(), ParamValue::Length(r(h)));
                }
                Section::I { bf, d, tw, tf } => {
                    p.shift_remove("b");
                    p.shift_remove("h");
                    p.insert("Section Shape".into(), ParamValue::Text("I".into()));
                    for (k, v) in [("d", d), ("bf", bf), ("tw", tw), ("tf", tf)] {
                        p.insert(k.into(), ParamValue::Length(r(v)));
                    }
                    p.insert("Structural Material".into(), ParamValue::Text("Steel".into()));
                }
            }
            if let Some(m) = material {
                let style = imp.material_styles.get(&m).cloned();
                let m = imp.ensure_material(&m, style.as_ref());
                p.insert("Structural Material".into(), ParamValue::Text(m));
            }
            (p, vec![])
        });
        let base = self.level_of(id, ex.z0);
        // The top level: the one nearest the column's top.
        let top = self
            .levels
            .iter()
            .min_by(|a, b| (a.2 - ex.z1).abs().total_cmp(&(b.2 - ex.z1).abs()))
            .map(|l| l.1)
            .unwrap_or(base);
        let top_offset = ex.z1 - self.level_elev(top);
        self.doc.add_element(ElementData::Column(Column {
            type_id,
            point: centre,
            rotation: xdir.y.atan2(xdir.x).to_degrees(),
            base_level: base,
            base_offset: ex.z0 - self.level_elev(base),
            top_level: top,
            top_offset,
        }));
        self.report.columns += 1;
        true
    }

    /// A door or window in an imported wall becomes a hosted door or
    /// window sized from the IFC.
    fn opening(&mut self, id: u32, e: &Entity) -> bool {
        let Some((opening, host)) = self.fills.get(&id).copied() else {
            return false;
        };
        let Some(wall_id) = self.walls.get(&host).copied() else {
            return false;
        };
        let Some(wall) = self.doc.element(wall_id).and_then(|w| w.as_wall()).cloned() else {
            return false;
        };
        // The opening's extent along the wall and in height, from the
        // opening's body (or the door's own).
        let mut pts: Vec<DVec3> = self
            .e(opening)
            .map(|o| self.body(o).iter().flat_map(|p| p.solid.points()).collect())
            .unwrap_or_default();
        if pts.is_empty() {
            pts = self.body(e).iter().flat_map(|p| p.solid.points()).collect();
        }
        if pts.is_empty() {
            return false;
        }
        let dir = wall.direction();
        let along: Vec<f64> = pts.iter().map(|p| (p.truncate() - wall.start).dot(dir)).collect();
        let (a0, a1) = along
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
        let (z0, z1) = pts
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.z), hi.max(p.z)));
        let door = e.name.starts_with("IFCDOOR");
        let width = e.arg(9).as_f64().map(|v| v * self.scale).filter(|v| *v > 1.0).unwrap_or(a1 - a0);
        let height = e.arg(8).as_f64().map(|v| v * self.scale).filter(|v| *v > 1.0).unwrap_or(z1 - z0);
        let (width, height) = ((width * 10.0).round() / 10.0, (height * 10.0).round() / 10.0);
        let family = match (door, width > 1400.0) {
            (true, false) => "Door - Single Flush",
            (true, true) => "Door - Double Flush",
            (false, _) => "Window - Fixed",
        };
        let cat = if door { Category::Doors } else { Category::Windows };
        if self.doc.family_def(cat, family).is_none() {
            if let Some(def) = hausit_model::library::family(family) {
                let _ = self.doc.load_family(def);
            }
        }
        let (_, type_name) = self.names(e, id);
        let type_name = if type_name == "Imported" {
            format!("{width:.0} x {height:.0}mm")
        } else {
            type_name
        };
        let type_id = self.ensure_type(cat, family, &type_name, &format!("{width}x{height}"), |imp| {
            let def = imp.doc.family_def(cat, family).cloned();
            let mut p = match &def {
                Some(d) => d.type_params(&d.types[0]),
                None => hausit_model::family::default_type_params(cat),
            };
            p.insert("Width".into(), ParamValue::Length(width));
            p.insert("Height".into(), ParamValue::Length(height));
            (p, vec![])
        });
        let base = self.level_elev(wall.base_level) + wall.base_offset;
        let o = HostedOpening {
            type_id,
            host: wall_id,
            offset: (a0 + a1) / 2.0,
            sill_height: z0 - base + wall.base_offset,
            flip_hand: false,
            flip_facing: false,
        };
        let mark = e.arg(7).as_str().unwrap_or("").to_string();
        let el = self
            .doc
            .add_element(if door { ElementData::Door(o) } else { ElementData::Window(o) });
        if let Some(el) = self.doc.element_mut(el) {
            el.mark = mark;
        }
        if door {
            self.report.doors += 1;
        } else {
            self.report.windows += 1;
        }
        true
    }

    fn direct_shape(&mut self, id: u32, e: &Entity) {
        let pieces = self.body(e);
        if pieces.is_empty() {
            // Wholes such as roofs and stairs often carry no geometry of
            // their own: their parts do.
            let has_parts = self.parent.values().any(|p| *p == id);
            if !has_parts && e.arg(6).as_ref().is_some() {
                *self.report.skipped.entry(class_label(&e.name)).or_default() += 1;
            }
            return;
        }
        let parent_class = self.parent.get(&id).and_then(|p| self.e(*p)).map(|p| p.name.clone());
        let cat = category_of(&e.name, e.arg(8).as_enum(), parent_class.as_deref());
        let element_material = self.material_name(id);
        let mut solids = Vec::new();
        let mut zmin = f64::INFINITY;
        for p in pieces {
            let material = match (&p.style, &element_material) {
                (Some(s), _) => Some(self.ensure_material(&s.name.clone(), Some(&s.clone()))),
                (None, Some(m)) => {
                    let style = self.material_styles.get(m).cloned();
                    Some(self.ensure_material(&m.clone(), style.as_ref()))
                }
                (None, None) => None,
            };
            for q in p.solid.points() {
                zmin = zmin.min(q.z);
            }
            solids.push(ShapeSolid {
                material,
                geometry: match p.solid {
                    Solid::Prism { footprint, z0, z1 } => ShapeGeometry::Prism { footprint, z0, z1 },
                    Solid::Faces(f) => ShapeGeometry::Faces(f),
                },
            });
        }
        let level = self.level_of(id, zmin);
        let name = format!("{}: {}", class_label(&e.name), e.arg(2).as_str().unwrap_or(""));
        self.doc.add_element(ElementData::DirectShape(DirectShape {
            category: cat,
            level,
            name,
            solids,
        }));
        *self.report.shapes.entry(class_label(&e.name)).or_default() += 1;
    }

    fn space(&mut self, id: u32) {
        let Some(e) = self.e(id) else { return };
        let pieces = self.body(e);
        let footprint: Vec<DVec2> = match pieces.first().map(|p| &p.solid) {
            Some(Solid::Prism { footprint, .. }) => footprint.clone(),
            Some(s @ Solid::Faces(_)) => {
                let pts: Vec<DVec2> = s.points().iter().map(|p| p.truncate()).collect();
                convex_hull(pts)
            }
            None => return,
        };
        if footprint.len() < 3 {
            return;
        }
        let zmin = pieces
            .iter()
            .flat_map(|p| p.solid.points())
            .map(|p| p.z)
            .fold(f64::INFINITY, f64::min);
        // A point inside the space: its centroid, else a vertex nudged in.
        let a = polygon_area(&footprint);
        let mut c = DVec2::ZERO;
        for i in 0..footprint.len() {
            let (p, q) = (footprint[i], footprint[(i + 1) % footprint.len()]);
            c += (p + q) * p.perp_dot(q);
        }
        let mut point = if a.abs() > 1e-6 { c / (6.0 * a) } else { footprint[0] };
        if !point_in_polygon(point, &footprint) {
            let tri = hausit_core::geom::triangulate(&footprint);
            if let Some(t) = tri.first() {
                point = (footprint[t[0]] + footprint[t[1]] + footprint[t[2]]) / 3.0;
            }
        }
        let level = self.level_of(id, zmin);
        let number = e.arg(2).as_str().unwrap_or("").to_string();
        let long = e.arg(7).as_str().unwrap_or("").to_string();
        let name = if long.is_empty() { number.clone() } else { long };
        let upper = self.doc.level_above(level).unwrap_or_default();
        self.doc.add_element(ElementData::Room(Room {
            level,
            point,
            name,
            number,
            upper_limit: upper,
            limit_offset: 0.0,
        }));
        self.report.rooms += 1;
    }

    fn views(&mut self) {
        self.doc.add_view("{3D}", ViewKind::ThreeD);
        for d in [
            CardinalDirection::East,
            CardinalDirection::North,
            CardinalDirection::South,
            CardinalDirection::West,
        ] {
            self.doc.add_view(d.label(), ViewKind::Elevation { direction: d, at: None });
        }
    }
}

/// `IFCWALLSTANDARDCASE` → `IfcWallStandardCase`, for messages.
fn class_label(name: &str) -> String {
    const WORDS: &[&str] = &[
        "Standard",
        "Case",
        "Element",
        "Elemented",
        "Wall",
        "Slab",
        "Column",
        "Beam",
        "Member",
        "Plate",
        "Door",
        "Window",
        "Covering",
        "Roof",
        "Stair",
        "Flight",
        "Railing",
        "Ramp",
        "Footing",
        "Pile",
        "Furnishing",
        "Furniture",
        "Building",
        "Proxy",
        "Curtain",
        "Flow",
        "Terminal",
        "Sanitary",
        "Fitting",
        "Segment",
        "Distribution",
        "Energy",
        "Conversion",
        "Device",
        "Controller",
        "Storage",
        "Moving",
        "Transport",
        "Space",
        "Opening",
        "Discrete",
        "Accessory",
        "Fastener",
        "Mechanical",
        "System",
        "Chimney",
        "Shading",
        "Civil",
        "Geographic",
        "Waste",
        "Air",
        "Light",
        "Fixture",
        "Lamp",
        "Outlet",
        "Switching",
        "Cable",
        "Carrier",
        "Duct",
        "Pipe",
        "Valve",
        "Pump",
        "Fan",
        "Tank",
        "Unit",
        "Assembly",
    ];
    let rest = name.strip_prefix("IFC").unwrap_or(name);
    let mut out = String::from("Ifc");
    let lower = rest.to_ascii_lowercase();
    let mut i = 0;
    while i < lower.len() {
        let w = WORDS
            .iter()
            .filter(|w| lower[i..].starts_with(&w.to_ascii_lowercase()))
            .max_by_key(|w| w.len());
        match w {
            Some(w) => {
                out.push_str(w);
                i += w.len();
            }
            None => {
                out.push_str(&rest[i..i + 1].to_ascii_lowercase());
                i += 1;
            }
        }
    }
    out
}

/// A solid from a planar outline extruded along `ext`: a vertical prism
/// when it stands straight, faces otherwise.
fn extrusion_solid(pts: &[DVec3], ext: DVec3) -> Option<Solid> {
    if pts.len() < 3 || ext.length() < 1e-6 {
        return None;
    }
    let flat = pts.iter().all(|p| (p.z - pts[0].z).abs() < 1e-6);
    if flat && ext.truncate().length() < 1e-6 {
        let z = pts[0].z;
        let (z0, z1) = if ext.z > 0.0 { (z, z + ext.z) } else { (z + ext.z, z) };
        return Some(Solid::Prism {
            footprint: pts.iter().map(|p| p.truncate()).collect(),
            z0,
            z1,
        });
    }
    let mut base = pts.to_vec();
    if polygon_normal(&base).dot(ext) < 0.0 {
        base.reverse();
    }
    let n = base.len();
    let mut faces = vec![
        base.iter().rev().copied().collect::<Vec<_>>(),
        base.iter().map(|p| *p + ext).collect::<Vec<_>>(),
    ];
    for i in 0..n {
        let (a, b) = (base[i], base[(i + 1) % n]);
        faces.push(vec![a, b, b + ext, a + ext]);
    }
    Some(Solid::Faces(faces))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hausit_model::template::{industrial_sample, sample_project};

    #[test]
    fn class_labels() {
        assert_eq!(class_label("IFCWALLSTANDARDCASE"), "IfcWallStandardCase");
        assert_eq!(class_label("IFCFURNISHINGELEMENT"), "IfcFurnishingElement");
    }

    /// Hausit's own export comes back as native elements.
    #[test]
    fn round_trips_the_sample_house() {
        let doc = sample_project();
        let text = crate::ifc::export(&doc, 0);
        let (back, report) = import(&text).unwrap();
        assert_eq!(report.levels, 2);
        assert_eq!(report.walls, doc.elements_of(Category::Walls).count(), "{report:?}");
        assert_eq!(report.doors, 2, "{report:?}");
        assert_eq!(report.windows, 5, "{report:?}");
        assert_eq!(report.floors, 1, "{report:?}");
        assert_eq!(report.rooms, 2);
        assert!(report.grids >= 4);
        // Wall types keep their width and layers.
        let ext = back
            .types_of(Category::Walls)
            .find(|t| t.name == "Exterior - Brick on Block")
            .expect("exterior wall type");
        assert!((ext.thickness() - 350.0).abs() < 0.5, "{}", ext.thickness());
        assert!(!ext.layers.is_empty());
        // The roof and the furniture come in as shapes.
        assert!(report.shapes.get("IfcRoof").copied().unwrap_or(0) >= 1, "{report:?}");
        assert!(report.shapes.get("IfcFurniture").copied().unwrap_or(0) >= 10, "{report:?}");
        // Rooms find their walls.
        for r in back.elements_of(Category::Rooms) {
            let ElementData::Room(room) = &r.data else { continue };
            assert!(hausit_model::rooms::room_info(&back, room).area > 40e6, "{}", room.name);
        }
    }

    #[test]
    fn round_trips_the_warehouse() {
        let text = crate::ifc::export(&industrial_sample(), 0);
        let (_, report) = import(&text).unwrap();
        assert_eq!(report.levels, 3);
        assert!(report.columns >= 12, "{report:?}");
        assert!(report.shapes.get("IfcBeam").copied().unwrap_or(0) >= 20, "{report:?}");
        assert_eq!(report.doors, 2, "{report:?}");
        assert_eq!(report.windows, 3, "{report:?}");
    }
}
