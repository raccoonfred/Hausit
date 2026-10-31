//! Linked models: another project shown inside this one, as Revit links
//! do. A link type points at the file; link instances place it, moved and
//! rotated as one piece. Linked elements are read-only here.
//!
//! The model crate does not read files: the app loads the linked project
//! and hands it over with [`Document::set_link_model`]. Loaded models are
//! not saved with the project; only the path is.

use crate::category::Category;
use crate::document::{Document, ModelError};
use crate::element::ElementData;
use crate::geometry::{element_geometry, Face, Material, Part, Shape};
use hausit_core::{DVec2, DVec3, ElementId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinkType {
    pub id: ElementId,
    pub name: String,
    /// The linked file, as picked.
    pub path: String,
    /// Unloaded links keep their instances but show nothing.
    #[serde(default)]
    pub unloaded: bool,
}

/// A placed link: the linked model's origin goes to `origin` (with `z`),
/// turned by `rotation` degrees.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinkInstance {
    pub link: ElementId,
    pub origin: DVec2,
    #[serde(default)]
    pub z: f64,
    #[serde(default)]
    pub rotation: f64,
}

impl LinkInstance {
    /// Linked coordinates to host coordinates, in plan.
    pub fn map(&self, p: DVec2) -> DVec2 {
        let (s, c) = self.rotation.to_radians().sin_cos();
        self.origin + DVec2::new(p.x * c - p.y * s, p.x * s + p.y * c)
    }

    pub fn map3(&self, p: DVec3) -> DVec3 {
        self.map(p.truncate()).extend(p.z + self.z)
    }
}

/// A loaded linked model with its geometry, in its own coordinates.
#[derive(Debug)]
pub struct LinkedModel {
    pub doc: Document,
    pub parts: Vec<Part>,
}

/// Loaded linked models by link type. Not saved; equal whatever they hold
/// so that documents compare by their own content.
#[derive(Clone, Debug, Default)]
pub struct LinkModels(pub HashMap<ElementId, Arc<LinkedModel>>);

impl PartialEq for LinkModels {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// The material name a part of a linked element shows: its type's
/// structure layer, or its type's material.
fn part_material(doc: &Document, id: ElementId) -> Option<String> {
    let t = doc.type_of(id)?;
    // Roofs and floors show their top finish.
    let top = matches!(doc.element(id)?.category(), Category::Roofs | Category::Floors);
    let layer = if top {
        t.layers.first()
    } else {
        t.layers
            .iter()
            .find(|l| l.function == crate::materials::LayerFunction::Structure)
            .or(t.layers.first())
    };
    if let Some(l) = layer {
        return Some(l.material.clone());
    }
    ["Material", "Structural Material"].iter().find_map(|k| match t.params.get(*k) {
        Some(crate::ParamValue::Text(name)) if !name.is_empty() => Some(name.clone()),
        _ => None,
    })
}

impl Document {
    pub fn link_type(&self, id: ElementId) -> Option<&LinkType> {
        self.links.iter().find(|l| l.id == id)
    }

    /// Adds a link type for a file. The model is loaded separately.
    pub fn add_link(&mut self, name: &str, path: &str) -> ElementId {
        let id = self.new_id();
        self.links.push(LinkType {
            id,
            name: name.into(),
            path: path.into(),
            unloaded: false,
        });
        id
    }

    /// Hands over a loaded linked model. Its materials the host lacks are
    /// added so linked elements keep their look.
    pub fn set_link_model(&mut self, id: ElementId, linked: Document) {
        for m in &linked.materials {
            if self.material(&m.name).is_none() {
                self.materials.push(m.clone());
            }
        }
        let mut parts = Vec::new();
        for e in linked.elements() {
            if !e.category().is_model() || e.category() == Category::RvtLinks {
                continue;
            }
            let Some(g) = element_geometry(&linked, e.id) else { continue };
            let fallback = part_material(&linked, e.id);
            for mut p in g.parts {
                if p.material_name.is_none() && p.material == Material::Default {
                    p.material_name = fallback.clone();
                }
                parts.push(p);
            }
        }
        self.link_models.0.insert(id, Arc::new(LinkedModel { doc: linked, parts }));
        if let Some(l) = self.links.iter_mut().find(|l| l.id == id) {
            l.unloaded = false;
        }
        self.touch();
    }

    /// The loaded model of a link type, unless it is unloaded.
    pub fn link_model(&self, id: ElementId) -> Option<&Arc<LinkedModel>> {
        if self.link_type(id).is_some_and(|l| l.unloaded) {
            return None;
        }
        self.link_models.0.get(&id)
    }

    pub fn unload_link(&mut self, id: ElementId) {
        self.link_models.0.remove(&id);
        if let Some(l) = self.links.iter_mut().find(|l| l.id == id) {
            l.unloaded = true;
        }
        self.touch();
    }

    /// Removes a link type and its instances.
    pub fn remove_link(&mut self, id: ElementId) -> Result<(), ModelError> {
        let instances: Vec<ElementId> = self
            .elements_of(Category::RvtLinks)
            .filter(|e| matches!(&e.data, ElementData::LinkInstance(i) if i.link == id))
            .map(|e| e.id)
            .collect();
        if !instances.is_empty() {
            self.delete(&instances)?;
        }
        self.links.retain(|l| l.id != id);
        self.link_models.0.remove(&id);
        self.touch();
        Ok(())
    }

    /// Places a link, origin to origin.
    pub fn place_link(&mut self, link: ElementId) -> Result<ElementId, ModelError> {
        if self.link_type(link).is_none() {
            return Err(ModelError::Rejected("No such link.".into()));
        }
        Ok(self.add_element(ElementData::LinkInstance(LinkInstance {
            link,
            origin: DVec2::ZERO,
            z: 0.0,
            rotation: 0.0,
        })))
    }

    /// Geometry of a link instance: the linked model's parts, placed.
    pub fn link_parts(&self, inst: &LinkInstance) -> Vec<Part> {
        let Some(m) = self.link_model(inst.link) else { return vec![] };
        let identity = inst.origin == DVec2::ZERO && inst.rotation == 0.0 && inst.z == 0.0;
        m.parts
            .iter()
            .map(|p| {
                if identity {
                    return p.clone();
                }
                let shape = match &p.shape {
                    Shape::Prism(pr) => {
                        let mut pr = pr.clone();
                        pr.footprint.iter_mut().for_each(|q| *q = inst.map(*q));
                        pr.z0 += inst.z;
                        pr.z1 += inst.z;
                        Shape::Prism(pr)
                    }
                    Shape::Faces(fs) => Shape::Faces(
                        fs.iter()
                            .map(|f| Face {
                                pts: f.pts.iter().map(|q| inst.map3(*q)).collect(),
                            })
                            .collect(),
                    ),
                };
                Part {
                    material: p.material,
                    material_name: p.material_name.clone(),
                    shape,
                }
            })
            .collect()
    }

    /// The linked level a host plan at `elevation` shows: the one at that
    /// height, else the highest one below it.
    pub fn link_level_for(&self, inst: &LinkInstance, elevation: f64) -> Option<ElementId> {
        let m = self.link_model(inst.link)?;
        let levels = m.doc.levels();
        levels
            .iter()
            .filter(|(_, l)| l.elevation + inst.z <= elevation + 1.0)
            .max_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation))
            .or_else(|| levels.iter().min_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation)))
            .map(|(id, _)| *id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::sample_project;

    #[test]
    fn linked_model_places_and_unloads() {
        let mut host = crate::template::new_project();
        let linked = sample_project();
        let link = host.add_link("House", "/tmp/house.hausit");
        host.set_link_model(link, linked);
        let inst_id = host.place_link(link).unwrap();
        let g = element_geometry(&host, inst_id).unwrap();
        assert_eq!(g.category, Category::RvtLinks);
        let b0 = g.bounds();
        assert!(!b0.is_empty());
        // Moving the instance moves the linked geometry.
        host.translate(&[inst_id], DVec2::new(20000.0, 0.0));
        let b1 = element_geometry(&host, inst_id).unwrap().bounds();
        assert!((b1.min.x - b0.min.x - 20000.0).abs() < 1e-6);
        // Rotating it a quarter turn about its origin.
        if let Some(ElementData::LinkInstance(i)) = host.element_mut(inst_id).map(|e| &mut e.data) {
            i.origin = DVec2::ZERO;
            i.rotation = 90.0;
        }
        let b2 = element_geometry(&host, inst_id).unwrap().bounds();
        assert!((b2.max.x + b0.min.y).abs() < 1.0, "{b0:?} {b2:?}");
        // Unloading hides it; removing deletes the instance.
        host.unload_link(link);
        assert!(element_geometry(&host, inst_id).is_none());
        host.remove_link(link).unwrap();
        assert!(host.element(inst_id).is_none());
        assert!(host.links.is_empty());
    }
}
