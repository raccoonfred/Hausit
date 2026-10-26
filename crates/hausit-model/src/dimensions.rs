//! Aligned dimensions: resolving references to lines and laying out the
//! dimension string.

use crate::document::Document;
use crate::element::{DimRef, Dimension, ElementData};
use crate::geometry::wall_outline;
use hausit_core::geom::closest_on_segment;
use hausit_core::DVec2;

/// The segment a reference measures to: a wall face or centreline, a
/// grid, or a fixed line.
pub fn ref_segment(doc: &Document, r: &DimRef) -> Option<(DVec2, DVec2)> {
    match *r {
        DimRef::Wall { wall, side } => {
            let w = doc.element(wall)?.as_wall()?;
            let o = wall_outline(doc, wall)?;
            Some(match side {
                1 => (o[0], o[1]),
                -1 => (o[3], o[2]),
                _ => (w.start, w.end),
            })
        }
        DimRef::Grid { grid } => match &doc.element(grid)?.data {
            ElementData::Grid(g) => Some((g.start, g.end)),
            _ => None,
        },
        DimRef::Line { point, dir } => Some((point, point + dir)),
    }
}

/// A dimension string ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct DimLayout {
    /// Direction the distances are measured in.
    pub n: DVec2,
    /// Points on the dimension line, one per reference, in measuring order.
    pub ticks: Vec<DVec2>,
    /// Where each witness line starts (on the referenced element).
    pub origins: Vec<DVec2>,
    /// Distances between consecutive ticks, mm.
    pub values: Vec<f64>,
}

/// Lays out a dimension. References must be parallel; the string is
/// sorted along the measuring direction.
pub fn layout(doc: &Document, d: &Dimension) -> Option<DimLayout> {
    let segs: Vec<(DVec2, DVec2)> = d.refs.iter().filter_map(|r| ref_segment(doc, r)).collect();
    if segs.len() < 2 || segs.len() != d.refs.len() {
        return None;
    }
    let along = (segs[0].1 - segs[0].0).normalize_or_zero();
    if along == DVec2::ZERO {
        return None;
    }
    let n = DVec2::new(-along.y, along.x);
    let mut items: Vec<(f64, DVec2)> = segs
        .iter()
        .map(|(a, b)| {
            let pos = a.dot(n);
            let tick = d.line_point + n * (pos - d.line_point.dot(n));
            // Witness lines start at the element, nearest the dimension line.
            let origin = if a.distance(*b) > 1e-6 {
                closest_on_segment(tick, *a, *b).0
            } else {
                *a
            };
            (pos, origin)
        })
        .collect();
    items.sort_by(|x, y| x.0.total_cmp(&y.0));
    let ticks: Vec<DVec2> = items
        .iter()
        .map(|(pos, _)| d.line_point + n * (pos - d.line_point.dot(n)))
        .collect();
    let values = items.windows(2).map(|w| w[1].0 - w[0].0).collect();
    Some(DimLayout {
        n,
        ticks,
        origins: items.iter().map(|i| i.1).collect(),
        values,
    })
}

/// Whether a new reference is parallel to the first one of a string.
pub fn parallel(doc: &Document, a: &DimRef, b: &DimRef) -> bool {
    match (ref_segment(doc, a), ref_segment(doc, b)) {
        (Some((a0, a1)), Some((b0, b1))) => {
            let (u, v) = ((a1 - a0).normalize_or_zero(), (b1 - b0).normalize_or_zero());
            (u.x * v.y - u.y * v.x).abs() < 1e-3
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::sample_project;
    use crate::Category;

    #[test]
    fn wall_faces_dimension() {
        let doc = sample_project();
        let walls: Vec<_> = doc.elements_of(Category::Walls).map(|e| e.id).collect();
        // West exterior wall (x = 0) to the partition (x = 6000), centrelines.
        let (west, part) = (walls[0], walls[4]);
        let d = Dimension {
            view: hausit_core::ElementId::INVALID,
            refs: vec![DimRef::Wall { wall: west, side: 0 }, DimRef::Wall { wall: part, side: 0 }],
            line_point: DVec2::new(0.0, -2000.0),
        };
        let l = layout(&doc, &d).unwrap();
        assert!((l.values[0].abs() - 6000.0).abs() < 1e-6, "{:?}", l.values);
        // Exterior face of the west wall is 150 mm further out.
        let d2 = Dimension {
            refs: vec![DimRef::Wall { wall: west, side: 1 }, DimRef::Wall { wall: part, side: 0 }],
            ..d
        };
        assert!((layout(&doc, &d2).unwrap().values[0].abs() - 6150.0).abs() < 1e-6);
    }
}
