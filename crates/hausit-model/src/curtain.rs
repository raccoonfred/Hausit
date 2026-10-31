//! Curtain walls: a wall type whose body is glass panels between curtain
//! grid lines, with mullions on the grid lines and around the edges.
//! Grid lines come from the type ("Vertical Grid Spacing", "Horizontal
//! Grid Spacing", laid out from the wall's start and base) plus lines
//! added to the instance with the Curtain Grid tool. With "Automatically
//! Embed", a straight curtain wall drawn inside a basic wall cuts its own
//! opening in it, as storefronts do in Revit.

use crate::category::Category;
use crate::document::{Document, ModelError};
use crate::element::{ElementData, Wall};
use crate::family::FamilyType;
use crate::params::ParamValue;
use hausit_core::geom::cross2;
use hausit_core::ElementId;
use serde::{Deserialize, Serialize};

/// Family name of curtain wall types.
pub const CURTAIN_WALL: &str = "Curtain Wall";

/// Grid lines added to one curtain wall: `u` along the wall from its
/// start, `v` heights above its base, mm.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CurtainGrids {
    #[serde(default)]
    pub u: Vec<f64>,
    #[serde(default)]
    pub v: Vec<f64>,
}

impl FamilyType {
    pub fn is_curtain(&self) -> bool {
        self.category == Category::Walls && self.family == CURTAIN_WALL
    }

    fn bool_or(&self, name: &str, default: bool) -> bool {
        match self.params.get(name) {
            Some(ParamValue::Bool(b)) => *b,
            _ => default,
        }
    }
}

/// Parameters of a new curtain wall type.
pub fn curtain_type_params(v_spacing: f64, h_spacing: f64, mullion: f64) -> crate::family::TypeParams {
    let mut p = crate::family::TypeParams::new();
    let len = ParamValue::Length;
    p.insert("Width".into(), len(150.0));
    p.insert("Function".into(), ParamValue::Text("Exterior".into()));
    p.insert("Vertical Grid Spacing".into(), len(v_spacing));
    p.insert("Horizontal Grid Spacing".into(), len(h_spacing));
    p.insert("Mullion Width".into(), len(mullion));
    p.insert("Mullion Depth".into(), len(if mullion > 0.0 { 150.0 } else { 0.0 }));
    p.insert("Panel Thickness".into(), len(25.0));
    p.insert("Automatically Embed".into(), ParamValue::Bool(true));
    p
}

/// A curtain wall's grid: stations along the wall (including both ends)
/// and absolute heights (including base and top).
#[derive(Clone, Debug, PartialEq)]
pub struct CurtainLayout {
    pub u: Vec<f64>,
    pub z: Vec<f64>,
    pub mullion_width: f64,
    pub mullion_depth: f64,
    pub panel_thickness: f64,
}

impl Document {
    /// Whether a wall is a curtain wall.
    pub fn is_curtain(&self, id: ElementId) -> bool {
        self.type_of(id).is_some_and(|t| t.is_curtain())
    }

    /// The grid of a curtain wall.
    pub fn curtain_layout(&self, id: ElementId) -> Option<CurtainLayout> {
        let w = self.element(id)?.as_wall()?;
        let t = self.type_of(id).filter(|t| t.is_curtain())?;
        let (len, zb, zt) = (w.length(), self.wall_base_z(w), self.wall_top_z(w));
        let fixed = |spacing: f64, span: f64| -> Vec<f64> {
            if spacing < 100.0 {
                return vec![];
            }
            let n = ((span - 1.0) / spacing).floor() as usize;
            (1..=n).map(|i| i as f64 * spacing).collect()
        };
        let mut u: Vec<f64> = vec![0.0, len];
        u.extend(fixed(t.length_or("Vertical Grid Spacing", 0.0), len));
        u.extend(w.curtain.u.iter().copied());
        let mut z: Vec<f64> = vec![zb, zt];
        z.extend(
            fixed(t.length_or("Horizontal Grid Spacing", 0.0), zt - zb)
                .into_iter()
                .map(|v| zb + v),
        );
        z.extend(w.curtain.v.iter().map(|v| zb + v));
        let clean = |v: &mut Vec<f64>, lo: f64, hi: f64| {
            v.retain(|x| *x >= lo - 1e-6 && *x <= hi + 1e-6);
            v.sort_by(f64::total_cmp);
            v.dedup_by(|a, b| (*a - *b).abs() < 10.0);
        };
        clean(&mut u, 0.0, len);
        clean(&mut z, zb, zt);
        Some(CurtainLayout {
            u,
            z,
            mullion_width: t.length_or("Mullion Width", 50.0),
            mullion_depth: t.length_or("Mullion Depth", 150.0),
            panel_thickness: t.length_or("Panel Thickness", 25.0),
        })
    }

    /// Curtain Grid: adds a vertical grid line at `along` mm from the
    /// wall's start, or a horizontal one `height` mm above its base.
    pub fn add_curtain_grid(&mut self, id: ElementId, along: Option<f64>, height: Option<f64>) -> Result<(), ModelError> {
        if !self.is_curtain(id) {
            return Err(ModelError::Rejected("Curtain grids go on curtain walls.".into()));
        }
        let Some(ElementData::Wall(w)) = self.element_mut(id).map(|e| &mut e.data) else {
            return Err(ModelError::Rejected("Curtain grids go on curtain walls.".into()));
        };
        if let Some(u) = along {
            w.curtain.u.push(u);
        }
        if let Some(v) = height {
            w.curtain.v.push(v);
        }
        self.touch();
        Ok(())
    }

    /// Openings that embedded curtain walls cut in a basic wall, as
    /// (start, end along the host, bottom z, top z).
    pub fn embedded_openings(&self, host: ElementId) -> Vec<(f64, f64, f64, f64)> {
        let Some(h) = self.element(host).and_then(|e| e.as_wall()) else {
            return vec![];
        };
        let index = self.index();
        if h.arc_mid.is_some() || index.curtain_walls().is_empty() || self.is_curtain(host) {
            return vec![];
        }
        let half = self.type_of(host).map(|t| t.thickness()).unwrap_or(200.0) / 2.0;
        index
            .curtain_walls()
            .iter()
            .filter(|id| **id != host)
            .filter_map(|id| {
                self.type_of(*id)
                    .filter(|t| t.is_curtain() && t.bool_or("Automatically Embed", true))?;
                let c: &Wall = self.element(*id)?.as_wall()?;
                if c.arc_mid.is_some() || cross2(c.direction(), h.direction()).abs() > 1e-3 {
                    return None;
                }
                if h.distance_to(c.start) > half || h.distance_to(c.end) > half {
                    return None;
                }
                let (a, b) = (h.param_of(c.start), h.param_of(c.end));
                let (a, b) = (a.min(b), a.max(b));
                (b - a > 1.0).then(|| (a, b, self.wall_base_z(c), self.wall_top_z(c)))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::TopConstraint;
    use crate::geometry::{element_geometry, Material};
    use crate::template::sample_project;
    use hausit_core::DVec2;

    /// A storefront drawn in the south wall: panels and mullions on its
    /// grid, and an opening cut in the wall it is embedded in.
    #[test]
    fn storefront_embeds_and_lays_out() {
        let mut doc = sample_project();
        let south = doc
            .elements_of(Category::Walls)
            .find(|e| e.as_wall().is_some_and(|w| w.start.y.abs() < 1.0 && w.end.y.abs() < 1.0))
            .unwrap()
            .id;
        let host = doc.element(south).unwrap().as_wall().unwrap().clone();
        let t = doc.types_of(Category::Walls).find(|t| t.name.starts_with("Storefront")).unwrap().id;
        let cw = Wall {
            type_id: t,
            start: DVec2::new(7000.0, 0.0),
            end: DVec2::new(10000.0, 0.0),
            top: TopConstraint::Unconnected { height: 2500.0 },
            attached_top: vec![],
            curtain: Default::default(),
            arc_mid: None,
            disallow_join: [false; 2],
            ..host.clone()
        };
        let before = element_geometry(&doc, south).unwrap().parts.len();
        let id = doc.add_element(ElementData::Wall(cw));
        // 3000 long at 1500: two bays; 2500 high at 2200: two rows.
        let l = doc.curtain_layout(id).unwrap();
        assert_eq!(l.u, vec![0.0, 1500.0, 3000.0]);
        assert_eq!(l.z.len(), 3);
        let g = element_geometry(&doc, id).unwrap();
        assert_eq!(g.parts.iter().filter(|p| p.material == Material::Glass).count(), 4);
        // Embedded: the host wall is now cut around the storefront.
        let holes = doc.embedded_openings(south);
        assert_eq!(holes.len(), 1);
        assert!(element_geometry(&doc, south).unwrap().parts.len() > before);
        // A grid line added on the instance splits a bay.
        doc.add_curtain_grid(id, Some(700.0), None).unwrap();
        assert_eq!(doc.curtain_layout(id).unwrap().u.len(), 4);
    }
}
