//! Openings in floors and ceilings: holes sketched with the floor, and
//! shaft openings (Revit's "Shaft"), which cut every floor and ceiling
//! between their base and top.

use crate::document::Document;
use crate::element::ElementData;
use crate::Category;
use hausit_core::geom::{point_in_polygon, polygon_area};
use hausit_core::{DVec2, ElementId};
use serde::{Deserialize, Serialize};

/// A vertical opening through floors and ceilings between two levels,
/// for stairs, lifts and service risers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShaftOpening {
    pub base_level: ElementId,
    #[serde(default)]
    pub base_offset: f64,
    pub top_level: ElementId,
    #[serde(default)]
    pub top_offset: f64,
    pub boundary: Vec<DVec2>,
}

impl Document {
    /// Bottom and top height of a shaft.
    pub fn shaft_extent(&self, s: &ShaftOpening) -> (f64, f64) {
        let z0 = self.level_elevation(s.base_level) + s.base_offset;
        let z1 = self.level_elevation(s.top_level) + s.top_offset;
        (z0.min(z1), z0.max(z1))
    }

    /// Holes through a slab spanning `z0..z1` with outline `outline`: its
    /// own sketched openings plus every shaft that reaches into it.
    pub fn slab_holes(&self, outline: &[DVec2], own: &[Vec<DVec2>], z0: f64, z1: f64) -> Vec<Vec<DVec2>> {
        let mut holes: Vec<Vec<DVec2>> = own.to_vec();
        for e in self.elements_of(Category::ShaftOpenings) {
            let ElementData::Shaft(s) = &e.data else { continue };
            let (b, t) = self.shaft_extent(s);
            if t <= z0 + 1.0 || b >= z1 - 1.0 || s.boundary.len() < 3 {
                continue;
            }
            if s.boundary.iter().all(|p| point_in_polygon(*p, outline)) {
                holes.push(s.boundary.clone());
            }
        }
        holes
    }
}

/// Splits sketched loops into an outline (the largest) and the openings
/// inside it. Loops outside the largest one are an error, as in Revit.
pub fn outline_and_openings(mut loops: Vec<Vec<DVec2>>) -> Result<(Vec<DVec2>, Vec<Vec<DVec2>>), String> {
    if loops.is_empty() {
        return Err("The sketch is empty.".into());
    }
    loops.sort_by(|a, b| polygon_area(b).abs().total_cmp(&polygon_area(a).abs()));
    let outer = loops.remove(0);
    for l in &loops {
        if !l.iter().all(|p| point_in_polygon(*p, &outer)) {
            return Err("Openings must be inside the boundary. Each extra loop of the sketch cuts a hole in it.".into());
        }
    }
    Ok((outer, loops))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<DVec2> {
        vec![DVec2::new(x0, y0), DVec2::new(x1, y0), DVec2::new(x1, y1), DVec2::new(x0, y1)]
    }

    #[test]
    fn floor_opening_and_shaft_cut_the_slab() {
        let mut doc = template::new_project();
        let levels = doc.levels();
        let (l1, l2) = (levels[0].0, levels[1].0);
        let ft = doc.types_of(Category::Floors).next().unwrap().id;
        let floor = doc.add_element(ElementData::Floor(crate::Floor {
            type_id: ft,
            level: l2,
            boundary: rect(0.0, 0.0, 10000.0, 8000.0),
            openings: vec![rect(1000.0, 1000.0, 2000.0, 4000.0)],
            height_offset: 0.0,
            room_bounding: true,
            structural: false,
        }));
        let vol = |doc: &Document| {
            crate::geometry::element_geometry(doc, floor)
                .unwrap()
                .parts
                .iter()
                .map(|p| match &p.shape {
                    crate::geometry::Shape::Prism(p) => p.volume(),
                    _ => 0.0,
                })
                .sum::<f64>()
        };
        let t = doc.type_of(floor).unwrap().thickness();
        assert!((vol(&doc) - (80e6 - 3e6) * t).abs() < 1.0);
        doc.add_element(ElementData::Shaft(ShaftOpening {
            base_level: l1,
            base_offset: 0.0,
            top_level: l2,
            top_offset: 100.0,
            boundary: rect(5000.0, 2000.0, 7000.0, 3000.0),
        }));
        assert!((vol(&doc) - (80e6 - 3e6 - 2e6) * t).abs() < 1.0);
    }

    #[test]
    fn loops_split_into_outline_and_openings() {
        let (outer, holes) = outline_and_openings(vec![rect(1.0, 1.0, 2.0, 2.0), rect(0.0, 0.0, 10.0, 10.0)]).unwrap();
        assert_eq!(outer, rect(0.0, 0.0, 10.0, 10.0));
        assert_eq!(holes.len(), 1);
        assert!(outline_and_openings(vec![rect(0.0, 0.0, 10.0, 10.0), rect(20.0, 0.0, 30.0, 10.0)]).is_err());
    }
}
