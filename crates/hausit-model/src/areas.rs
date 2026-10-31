//! Area plans, areas and colour schemes.
//!
//! An area plan is a floor plan with an area scheme (Gross Building or
//! Rentable). Areas are measured inside the area boundary lines drawn in
//! that view, the way rooms are measured inside walls. A colour scheme on
//! a plan fills its rooms (or, in an area plan, its areas) by name,
//! department or area, with a colour fill legend.

use crate::category::Category;
use crate::document::{Document, ModelError};
use crate::element::ElementData;
use crate::rooms::regions;
use hausit_core::geom::{point_in_polygon, polygon_area, polygon_perimeter};
use hausit_core::{DVec2, ElementId};
use serde::{Deserialize, Serialize};

/// Area schemes, as Revit ships them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AreaScheme {
    Gross,
    Rentable,
}

impl AreaScheme {
    pub const ALL: [AreaScheme; 2] = [AreaScheme::Gross, AreaScheme::Rentable];

    pub fn label(self) -> &'static str {
        match self {
            AreaScheme::Gross => "Gross Building",
            AreaScheme::Rentable => "Rentable",
        }
    }
}

/// What a colour scheme colours by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorBy {
    Name,
    Department,
    /// Area in ranges of 10 m².
    Area,
}

impl ColorBy {
    pub const ALL: [ColorBy; 3] = [ColorBy::Name, ColorBy::Department, ColorBy::Area];

    pub fn label(self) -> &'static str {
        match self {
            ColorBy::Name => "Name",
            ColorBy::Department => "Department",
            ColorBy::Area => "Area",
        }
    }
}

/// An area boundary line, drawn in one area plan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AreaBoundary {
    pub view: ElementId,
    pub start: DVec2,
    pub end: DVec2,
}

/// An area, placed in an area plan inside its boundary lines.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Area {
    pub view: ElementId,
    pub point: DVec2,
    pub name: String,
    pub number: String,
}

/// One row of a colour fill legend.
#[derive(Clone, Debug, PartialEq)]
pub struct LegendEntry {
    pub value: String,
    pub color: [u8; 3],
}

/// A soft, distinct colour for a legend value.
fn palette(i: usize) -> [u8; 3] {
    const P: [[u8; 3]; 10] = [
        [244, 177, 131],
        [169, 209, 142],
        [155, 194, 230],
        [255, 217, 102],
        [201, 160, 220],
        [244, 143, 177],
        [128, 203, 196],
        [255, 183, 77],
        [174, 213, 129],
        [179, 157, 219],
    ];
    P[i % P.len()]
}

impl Document {
    /// The boundary of an area, or `None` if its lines do not enclose it.
    pub fn area_boundary(&self, a: &Area) -> Option<Vec<DVec2>> {
        let segs: Vec<(DVec2, DVec2)> = self
            .elements_of(Category::AreaBoundary)
            .filter_map(|e| match &e.data {
                ElementData::AreaBoundary(b) if b.view == a.view => Some((b.start, b.end)),
                _ => None,
            })
            .collect();
        regions(&segs)
            .into_iter()
            .filter(|f| point_in_polygon(a.point, f))
            .min_by(|x, y| polygon_area(x).abs().total_cmp(&polygon_area(y).abs()))
    }

    /// Area and perimeter of an area (zero when not enclosed).
    pub fn area_size(&self, a: &Area) -> (f64, f64) {
        match self.area_boundary(a) {
            Some(b) => (polygon_area(&b).abs(), polygon_perimeter(&b)),
            None => (0.0, 0.0),
        }
    }

    /// Next free area number in a view.
    pub fn next_area_number(&self, view: ElementId) -> String {
        let max = self
            .elements_of(Category::Areas)
            .filter_map(|e| match &e.data {
                ElementData::Area(a) if a.view == view => a.number.parse::<u64>().ok(),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        (max + 1).to_string()
    }

    /// Creates an area plan for `level`. For the Gross Building scheme,
    /// boundary lines are added along the outside faces of the exterior
    /// walls, as Revit offers to.
    pub fn create_area_plan(&mut self, level: ElementId, scheme: AreaScheme, boundaries: bool) -> Result<ElementId, ModelError> {
        if self.level(level).is_none() {
            return Err(ModelError::Rejected("Pick a level for the area plan.".into()));
        }
        // Named after the level, as in Revit; the browser groups area plans
        // by scheme, so only a second plan of the same scheme is numbered.
        let base = self.level_name(level);
        let name = if self.views().any(|v| v.area_scheme == Some(scheme) && v.name == base) {
            self.unique_view_name(&base)
        } else {
            base
        };
        let id = self.add_view(&name, crate::ViewKind::FloorPlan { level });
        if let Some(v) = self.view_mut(id) {
            v.area_scheme = Some(scheme);
        }
        if boundaries {
            let walls: Vec<ElementId> = self
                .elements_of(Category::Walls)
                .filter(|e| e.as_wall().is_some_and(|w| w.base_level == level && w.arc_mid.is_none()))
                .filter(|e| {
                    self.type_of(e.id)
                        .is_some_and(|t| matches!(t.params.get("Function"), Some(crate::ParamValue::Text(f)) if f == "Exterior"))
                })
                .map(|e| e.id)
                .collect();
            for w in walls {
                if let Some(o) = crate::geometry::wall_outline(self, w) {
                    let (start, end) = match scheme {
                        AreaScheme::Gross => (o[0], o[1]),
                        AreaScheme::Rentable => (o[3], o[2]),
                    };
                    self.add_element(ElementData::AreaBoundary(AreaBoundary { view: id, start, end }));
                }
            }
        }
        Ok(id)
    }

    /// The value a colour scheme reads from a room or an area.
    pub fn color_value(&self, id: ElementId, by: ColorBy) -> Option<String> {
        let e = self.element(id)?;
        let (name, dept, area) = match &e.data {
            ElementData::Room(r) => {
                let info = crate::rooms::room_info(self, r);
                info.boundary.as_ref()?;
                (r.name.clone(), r.department.clone(), info.area)
            }
            ElementData::Area(a) => {
                self.area_boundary(a)?;
                (a.name.clone(), String::new(), self.area_size(a).0)
            }
            _ => return None,
        };
        Some(match by {
            ColorBy::Name => name,
            ColorBy::Department if dept.is_empty() => "(none)".into(),
            ColorBy::Department => dept,
            ColorBy::Area => {
                let m2 = area / 1e6;
                let lo = (m2 / 10.0).floor() * 10.0;
                format!("{lo:.0} - {:.0} m²", lo + 10.0)
            }
        })
    }

    /// The rooms (or areas, in an area plan) a view's colour scheme fills,
    /// with each one's colour, and the legend.
    pub fn color_fill(&self, view: ElementId) -> (Vec<(ElementId, [u8; 3])>, Vec<LegendEntry>) {
        let Some(v) = self.view(view) else { return (vec![], vec![]) };
        let (Some(by), Some(level)) = (v.color_scheme, v.kind.plan_level()) else {
            return (vec![], vec![]);
        };
        let ids: Vec<ElementId> = match v.area_scheme {
            Some(_) => self
                .elements_of(Category::Areas)
                .filter(|e| matches!(&e.data, ElementData::Area(a) if a.view == view))
                .map(|e| e.id)
                .collect(),
            None => self
                .elements_of(Category::Rooms)
                .filter(|e| matches!(&e.data, ElementData::Room(r) if r.level == level))
                .map(|e| e.id)
                .collect(),
        };
        let valued: Vec<(ElementId, String)> = ids.into_iter().filter_map(|id| Some((id, self.color_value(id, by)?))).collect();
        let mut values: Vec<String> = valued.iter().map(|(_, v)| v.clone()).collect();
        if by == ColorBy::Area {
            values.sort_by(|a, b| {
                let n = |s: &str| s.split(' ').next().and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                n(a).total_cmp(&n(b))
            });
        } else {
            values.sort();
        }
        values.dedup();
        let color = |v: &str| palette(values.iter().position(|x| x == v).unwrap_or(0));
        let fills = valued.iter().map(|(id, v)| (*id, color(v))).collect();
        let legend = values
            .iter()
            .map(|v| LegendEntry {
                value: v.clone(),
                color: color(v),
            })
            .collect();
        (fills, legend)
    }

    /// Whether an element belongs to a view as area-plan content.
    pub fn is_area_of(&self, data: &ElementData, view: ElementId) -> bool {
        match data {
            ElementData::AreaBoundary(b) => b.view == view,
            ElementData::Area(a) => a.view == view,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::sample_project;

    #[test]
    fn gross_area_plan_and_colour_scheme() {
        let mut doc = sample_project();
        let l1 = doc.levels()[0].0;
        let plan = doc.create_area_plan(l1, AreaScheme::Gross, true).unwrap();
        assert_eq!(doc.view(plan).unwrap().area_scheme, Some(AreaScheme::Gross));
        // The exterior walls' outer faces enclose the gross area.
        let a = Area {
            view: plan,
            point: DVec2::new(3000.0, 3000.0),
            name: "Gross".into(),
            number: doc.next_area_number(plan),
        };
        let (area, _) = doc.area_size(&a);
        let t = doc
            .type_of(doc.elements_of(Category::Walls).next().unwrap().id)
            .unwrap()
            .thickness();
        let expected = (12000.0 + t) * (8000.0 + t);
        assert!((area - expected).abs() < 1.0, "{area} vs {expected}");
        let id = doc.add_element(ElementData::Area(a));
        // Colour by name in the area plan.
        doc.view_mut(plan).unwrap().color_scheme = Some(ColorBy::Name);
        let (fills, legend) = doc.color_fill(plan);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].0, id);
        assert_eq!(legend.len(), 1);
        // Rooms in the floor plan, by name: one legend row per room name.
        let fp = doc
            .views()
            .find(|v| v.kind.plan_level() == Some(l1) && v.area_scheme.is_none() && matches!(v.kind, crate::ViewKind::FloorPlan { .. }))
            .unwrap()
            .id;
        doc.view_mut(fp).unwrap().color_scheme = Some(ColorBy::Name);
        let (fills, legend) = doc.color_fill(fp);
        assert_eq!(fills.len(), doc.elements_of(Category::Rooms).count());
        assert_eq!(legend.len(), 2);
        // Deleting the area plan deletes its boundaries and areas.
        doc.delete(&[plan]).unwrap();
        assert_eq!(doc.elements_of(Category::Areas).count(), 0);
        assert_eq!(doc.elements_of(Category::AreaBoundary).count(), 0);
    }
}
