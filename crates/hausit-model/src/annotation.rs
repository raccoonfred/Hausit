//! Text notes and tags: what a tag says, the point on its element it
//! refers to, and Revit's "Tag All Not Tagged".

use crate::category::Category;
use crate::document::Document;
use crate::element::{ElementData, Tag};
use crate::geometry::{element_geometry, opening_frame};
use crate::params::ParamValue;
use crate::view::ViewKind;
use hausit_core::geom::perp;
use hausit_core::{DVec2, DVec3, ElementId};

/// Categories that can be tagged with Tag by Category. Rooms carry their
/// own room tag.
pub const TAGGABLE: [Category; 16] = [
    Category::Doors,
    Category::Windows,
    Category::Walls,
    Category::Floors,
    Category::Roofs,
    Category::Ceilings,
    Category::StructuralColumns,
    Category::StructuralFraming,
    Category::StructuralFoundations,
    Category::Stairs,
    Category::Railings,
    Category::Furniture,
    Category::Casework,
    Category::PlumbingFixtures,
    Category::SpecialtyEquipment,
    Category::GenericModels,
];

/// Paper height of tag text, mm.
pub const TAG_TEXT: f64 = 2.5;

fn type_text(doc: &Document, id: ElementId, name: &str) -> Option<String> {
    match doc.type_of(id)?.params.get(name)? {
        ParamValue::Text(s) if !s.trim().is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// What a tag of an element shows, as Revit's default tags do: a door's
/// Mark, a window's Type Mark, and for other elements their Type Mark or
/// else their type name. "?" when the value is empty, as in Revit.
pub fn label_for(doc: &Document, host: ElementId) -> String {
    let Some(e) = doc.element(host) else { return "?".into() };
    let text = match e.category() {
        Category::Doors => Some(e.mark.clone()).filter(|m| !m.trim().is_empty()),
        Category::Windows => type_text(doc, host, "Type Mark"),
        _ => type_text(doc, host, "Type Mark").or_else(|| doc.type_of(host).map(|t| t.name.clone())),
    };
    text.unwrap_or_else(|| "?".into())
}

pub fn tag_label(doc: &Document, t: &Tag) -> String {
    label_for(doc, t.host)
}

/// The level an element stands on, for deciding which plans show it.
pub fn element_level(doc: &Document, id: ElementId) -> Option<ElementId> {
    let e = doc.element(id)?;
    Some(match &e.data {
        ElementData::Wall(w) => w.base_level,
        ElementData::Floor(f) => f.level,
        ElementData::Toposolid(t) => t.level,
        ElementData::Roof(r) => r.level,
        ElementData::Door(o) | ElementData::Window(o) => return element_level(doc, o.host),
        ElementData::Column(c) => c.base_level,
        ElementData::Framing(f) => f.level,
        ElementData::Footing(f) => f.level,
        ElementData::WallFooting(f) => return element_level(doc, f.wall),
        ElementData::Stair(s) => s.base_level,
        ElementData::Railing(r) => r.level,
        ElementData::Ceiling(c) => c.level,
        ElementData::Component(c) => c.level,
        ElementData::DirectShape(d) => d.level,
        ElementData::Room(r) => r.level,
        _ => return None,
    })
}

/// The point of an element a tag refers to (and its leader points at).
pub fn anchor(doc: &Document, host: ElementId) -> Option<DVec3> {
    let e = doc.element(host)?;
    let at = |p: DVec2, z: f64| Some(DVec3::new(p.x, p.y, z));
    let level = |l: ElementId| doc.level_elevation(l);
    match &e.data {
        ElementData::Door(_) | ElementData::Window(_) => {
            let f = opening_frame(doc, host)?;
            at(f.center, f.z0 + f.height / 2.0)
        }
        ElementData::Wall(w) => {
            let b = element_geometry(doc, host).map(|g| g.bounds());
            let z = b.map(|b| (b.min.z + b.max.z) / 2.0).unwrap_or(level(w.base_level));
            at(w.point_at(w.length() / 2.0), z)
        }
        ElementData::Room(r) => at(r.point, level(r.level)),
        ElementData::Column(c) => at(c.point, level(c.base_level) + c.base_offset),
        ElementData::Component(c) => at(c.point, level(c.level) + c.offset),
        ElementData::Footing(f) => at(f.point, level(f.level)),
        _ => {
            let b = element_geometry(doc, host)?.bounds();
            (!b.is_empty()).then(|| b.center())
        }
    }
}

/// Where a new tag goes in a plan: doors on their swing side, windows
/// outside the wall, other elements on their anchor.
pub fn default_position(doc: &Document, host: ElementId, scale: f64) -> Option<DVec2> {
    let e = doc.element(host)?;
    let h = TAG_TEXT * scale;
    match &e.data {
        ElementData::Door(o) | ElementData::Window(o) => {
            let f = opening_frame(doc, host)?;
            let n = perp(f.dir);
            let side = match (&e.data, o.flip_facing) {
                (ElementData::Door(_), false) => -n,
                (ElementData::Door(_), true) => n,
                _ => n,
            };
            Some(f.center + side * (f.wall_thickness / 2.0 + h * 2.2))
        }
        _ => anchor(doc, host).map(|p| p.truncate()),
    }
}

/// Whether a view already has a tag of the element.
pub fn is_tagged(doc: &Document, view: ElementId, host: ElementId) -> bool {
    doc.elements_of(Category::Tags)
        .any(|e| matches!(&e.data, ElementData::Tag(t) if t.view == view && t.host == host))
}

/// The elements of the categories a plan view shows that are not tagged
/// in it yet.
pub fn untagged(doc: &Document, view: ElementId, cats: &[Category]) -> Vec<ElementId> {
    let Some(v) = doc.view(view) else { return vec![] };
    let level = match v.kind {
        ViewKind::FloorPlan { level } | ViewKind::CeilingPlan { level } => level,
        _ => return vec![],
    };
    let cut = doc.level_elevation(level) + v.cut_height;
    doc.elements()
        .filter(|e| cats.contains(&e.category()))
        .filter(|e| element_level(doc, e.id) == Some(level))
        .filter(|e| match opening_frame(doc, e.id) {
            // Doors and windows the plan's cut plane passes through.
            Some(f) if e.category().is_hosted() => f.z0 <= cut && cut <= f.z0 + f.height,
            _ => true,
        })
        .filter(|e| !is_tagged(doc, view, e.id))
        .map(|e| e.id)
        .collect()
}

/// Tag All Not Tagged: tags every untagged element of the categories in a
/// plan view. Returns the new tags.
pub fn tag_all(doc: &mut Document, view: ElementId, cats: &[Category]) -> Vec<ElementId> {
    let scale = doc.view(view).map(|v| v.scale as f64).unwrap_or(100.0);
    let mut added = Vec::new();
    for host in untagged(doc, view, cats) {
        if let Some(position) = default_position(doc, host, scale) {
            added.push(doc.add_element(ElementData::Tag(Tag {
                view,
                host,
                position,
                leader: false,
            })));
        }
    }
    added
}

/// Tags doors and windows in every floor plan: what plans showed before
/// tags were elements, and what the sample projects start with.
pub fn tag_openings_in_plans(doc: &mut Document) {
    let plans: Vec<ElementId> = doc
        .views()
        .filter(|v| matches!(v.kind, ViewKind::FloorPlan { .. }))
        .map(|v| v.id)
        .collect();
    for v in plans {
        tag_all(doc, v, &[Category::Doors, Category::Windows]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_doors_and_windows_once() {
        let mut doc = crate::template::sample_project();
        let plan = doc
            .views()
            .find(|v| matches!(v.kind, ViewKind::FloorPlan { .. }))
            .map(|v| v.id)
            .unwrap();
        let before = doc.elements_of(Category::Tags).count();
        let added = tag_all(&mut doc, plan, &[Category::Doors, Category::Windows]);
        assert!(added.is_empty(), "the sample is already tagged");
        assert!(before > 0);
        // Deleting a door deletes its tag.
        let door = doc.elements_of(Category::Doors).next().unwrap().id;
        assert!(doc
            .dependents(&[door])
            .iter()
            .any(|d| doc.element(*d).is_some_and(|e| e.category() == Category::Tags)));
        assert_eq!(label_for(&doc, door), doc.element(door).unwrap().mark);
    }
}
