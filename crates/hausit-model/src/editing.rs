//! Modify-tab operations beyond move, copy, rotate and mirror: Align,
//! Offset, Array, Pin and Paste Aligned to Selected Levels.

use crate::category::Category;
use crate::document::{Document, ModelError};
use crate::element::{ElementData, TopConstraint};
use hausit_core::geom::perp;
use hausit_core::{DVec2, ElementId};

/// A straight reference Align and Offset work with: a wall's faces or
/// location line, a grid, a room separation line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RefLine {
    pub element: ElementId,
    pub a: DVec2,
    pub b: DVec2,
}

impl RefLine {
    pub fn dir(&self) -> DVec2 {
        (self.b - self.a).normalize_or_zero()
    }

    pub fn distance(&self, p: DVec2) -> f64 {
        hausit_core::geom::distance_to_segment(p, self.a, self.b)
    }
}

impl Document {
    /// The lines of an element Align and dimensions can pick: both faces
    /// and the centreline of a wall, a grid's line, a separation line.
    pub fn reference_lines(&self, id: ElementId) -> Vec<RefLine> {
        let Some(e) = self.element(id) else { return vec![] };
        let line = |a, b| RefLine { element: id, a, b };
        match &e.data {
            ElementData::Wall(w) if w.arc_mid.is_none() => {
                let t = self.type_of(id).map(|t| t.thickness()).unwrap_or(200.0) / 2.0;
                let n = perp(w.direction());
                vec![
                    line(w.start, w.end),
                    line(w.start + n * t, w.end + n * t),
                    line(w.start - n * t, w.end - n * t),
                ]
            }
            ElementData::Grid(g) => vec![line(g.start, g.end)],
            ElementData::RoomSeparation(l) => vec![line(l.start, l.end)],
            ElementData::Framing(f) => vec![line(f.start, f.end)],
            _ => vec![],
        }
    }

    /// The reference of an element nearest a point.
    pub fn nearest_reference(&self, id: ElementId, p: DVec2) -> Option<RefLine> {
        self.reference_lines(id)
            .into_iter()
            .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
    }

    /// Pinned elements among `ids`.
    pub fn pinned_among(&self, ids: &[ElementId]) -> Vec<ElementId> {
        ids.iter()
            .copied()
            .filter(|id| self.element(*id).is_some_and(|e| e.pinned))
            .collect()
    }

    /// Refuses an edit that would move or delete pinned elements, as
    /// Revit does.
    pub fn check_unpinned(&self, ids: &[ElementId]) -> Result<(), ModelError> {
        let pinned = self.pinned_among(ids);
        if pinned.is_empty() {
            Ok(())
        } else {
            Err(ModelError::Rejected(format!(
                "{} pinned element(s) cannot be changed this way. Unpin them first (UP).",
                pinned.len()
            )))
        }
    }

    pub fn set_pinned(&mut self, ids: &[ElementId], pinned: bool) {
        for id in ids {
            if let Some(e) = self.element_mut(*id) {
                e.pinned = pinned;
            }
        }
    }

    /// Align: moves `target` across so that its picked reference lies on
    /// `reference` (both lines must be parallel). Point elements (columns,
    /// components, foundations) move their insertion point onto the line.
    pub fn align(&mut self, reference: RefLine, target: ElementId, picked: DVec2) -> Result<(), ModelError> {
        self.check_unpinned(&[target])?;
        let n = perp(reference.dir());
        let along_line = |p: DVec2| (p - reference.a).dot(n);
        let delta = match self.nearest_reference(target, picked) {
            Some(t) => {
                if t.dir().perp_dot(reference.dir()).abs() > 1e-3 {
                    return Err(ModelError::Rejected("The references must be parallel to align.".into()));
                }
                -n * along_line(t.a)
            }
            None => {
                let point = match self.element(target).map(|e| &e.data) {
                    Some(ElementData::Column(c)) => c.point,
                    Some(ElementData::Component(c)) => c.point,
                    Some(ElementData::Footing(f)) => f.point,
                    _ => return Err(ModelError::Rejected("This element cannot be aligned.".into())),
                };
                -n * along_line(point)
            }
        };
        if delta.length() > 1e-9 {
            self.translate(&[target], delta);
        }
        Ok(())
    }

    /// Offset (Copy ticked): a copy of a wall, grid or line `distance` to
    /// the side of `side`. Returns the copy.
    pub fn offset_copy(&mut self, id: ElementId, distance: f64, side: DVec2) -> Result<ElementId, ModelError> {
        let line = self
            .reference_lines(id)
            .into_iter()
            .next()
            .ok_or_else(|| ModelError::Rejected("Only walls, grids and lines can be offset.".into()))?;
        let n = perp(line.dir());
        let sign = if (side - line.a).dot(n) >= 0.0 { 1.0 } else { -1.0 };
        let new = self.copy(&[id], n * distance * sign);
        new.first()
            .copied()
            .ok_or_else(|| ModelError::Rejected("The element could not be copied.".into()))
    }

    /// Offset with Copy cleared: moves the element instead.
    pub fn offset_move(&mut self, id: ElementId, distance: f64, side: DVec2) -> Result<(), ModelError> {
        self.check_unpinned(&[id])?;
        let line = self
            .reference_lines(id)
            .into_iter()
            .next()
            .ok_or_else(|| ModelError::Rejected("Only walls, grids and lines can be offset.".into()))?;
        let n = perp(line.dir());
        let sign = if (side - line.a).dot(n) >= 0.0 { 1.0 } else { -1.0 };
        self.translate(&[id], n * distance * sign);
        Ok(())
    }

    /// Linear Array: `count` items in all (the originals are the first),
    /// each `step` from the last. Returns the new elements.
    pub fn array_linear(&mut self, ids: &[ElementId], step: DVec2, count: usize) -> Vec<ElementId> {
        let mut out = Vec::new();
        for k in 1..count.max(1) {
            out.extend(self.copy(ids, step * k as f64));
        }
        out
    }

    /// Radial Array: `count` items in all, turned `angle` (degrees) apart
    /// about `center`.
    pub fn array_radial(&mut self, ids: &[ElementId], center: DVec2, angle: f64, count: usize) -> Vec<ElementId> {
        let mut out = Vec::new();
        for k in 1..count.max(1) {
            let new = self.copy(ids, DVec2::ZERO);
            self.rotate(&new, center, angle * k as f64);
            out.extend(new);
        }
        out
    }

    /// Paste Aligned to Selected Levels: copies of elements standing on
    /// `from`, on each of `to`. Other levels they refer to (a wall's top,
    /// a column's top) shift by the same height when a level is there;
    /// otherwise walls become unconnected at their height.
    pub fn paste_aligned(&mut self, ids: &[ElementId], from: ElementId, to: &[ElementId]) -> Vec<ElementId> {
        let base = self.level_elevation(from);
        let levels: Vec<(ElementId, f64)> = self.levels().into_iter().map(|(id, l)| (id, l.elevation)).collect();
        let at = |z: f64| levels.iter().find(|(_, e)| (e - z).abs() < 1.0).map(|(id, _)| *id);
        let mut out = Vec::new();
        for &target in to {
            if target == from {
                continue;
            }
            let dz = self.level_elevation(target) - base;
            let new = self.copy(ids, DVec2::ZERO);
            for id in &new {
                let heights: Vec<(ElementId, f64)> = levels.clone();
                let Some(e) = self.element_mut(*id) else { continue };
                // A wall whose top level has no counterpart keeps its height.
                if let ElementData::Wall(w) = &mut e.data {
                    if let TopConstraint::UpToLevel { level, offset } = w.top {
                        let top = heights.iter().find(|(l, _)| *l == level).map(|(_, z)| *z).unwrap_or(0.0);
                        if at(top + dz).is_none() {
                            let bottom = heights.iter().find(|(l, _)| *l == w.base_level).map(|(_, z)| *z).unwrap_or(0.0);
                            w.top = TopConstraint::Unconnected {
                                height: (top + offset - bottom - w.base_offset).max(1.0),
                            };
                        }
                    }
                    // Attachments to roofs and floors of the source level do not carry over.
                    w.attached_top.clear();
                }
                for r in e.data.level_refs_mut() {
                    let z = heights.iter().find(|(l, _)| *l == *r).map(|(_, z)| *z);
                    if let Some(new_level) = z.and_then(|z| at(z + dz)) {
                        *r = new_level;
                    }
                }
            }
            out.extend(new);
        }
        self.touch();
        out
    }

    /// Flip a wall's orientation (its flip control or Space): the
    /// exterior side swaps about the location line. Hosted doors and
    /// windows keep their place and orientation.
    pub fn flip_wall(&mut self, id: ElementId) -> Result<(), ModelError> {
        self.check_unpinned(&[id])?;
        let Some(ElementData::Wall(w)) = self.element_mut(id).map(|e| &mut e.data) else {
            return Err(ModelError::Rejected("Only walls can be flipped this way.".into()));
        };
        std::mem::swap(&mut w.start, &mut w.end);
        w.disallow_join.swap(0, 1);
        let len = w.length();
        let hosted: Vec<ElementId> = self.openings_in(id).map(|(e, _)| e.id).collect();
        for h in hosted {
            if let Some(ElementData::Door(o) | ElementData::Window(o)) = self.element_mut(h).map(|e| &mut e.data) {
                o.offset = len - o.offset;
                o.flip_hand = !o.flip_hand;
                o.flip_facing = !o.flip_facing;
            }
        }
        self.touch();
        Ok(())
    }

    /// The flip controls of a door or window: swap the hand (hinge side)
    /// or the facing (swing side).
    pub fn flip_opening(&mut self, id: ElementId, hand: bool) -> Result<(), ModelError> {
        let Some(ElementData::Door(o) | ElementData::Window(o)) = self.element_mut(id).map(|e| &mut e.data) else {
            return Err(ModelError::Rejected("Only doors and windows have flip controls.".into()));
        };
        if hand {
            o.flip_hand = !o.flip_hand;
        } else {
            o.flip_facing = !o.flip_facing;
        }
        self.touch();
        Ok(())
    }

    /// Allow Join / Disallow Join at a wall end (0 start, 1 end).
    pub fn set_join_allowed(&mut self, id: ElementId, end: usize, allowed: bool) -> Result<(), ModelError> {
        let Some(ElementData::Wall(w)) = self.element_mut(id).map(|e| &mut e.data) else {
            return Err(ModelError::Rejected("Only walls have joins.".into()));
        };
        w.disallow_join[end.min(1)] = !allowed;
        self.touch();
        Ok(())
    }

    /// The level elements are drawn on, for Paste Aligned: the base level
    /// most of them share.
    pub fn common_level(&self, ids: &[ElementId]) -> Option<ElementId> {
        let mut counts: std::collections::HashMap<ElementId, usize> = Default::default();
        for id in ids {
            if let Some(l) = crate::annotation::element_level(self, *id) {
                *counts.entry(l).or_default() += 1;
            }
        }
        counts.into_iter().max_by_key(|(_, n)| *n).map(|(l, _)| l)
    }
}

/// Categories Temporary Hide/Isolate leaves visible when isolating.
pub fn always_visible(cat: Category) -> bool {
    matches!(cat, Category::Levels | Category::Grids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::sample_project;

    #[test]
    fn align_offset_array_pin_paste() {
        let mut doc = sample_project();
        let walls: Vec<ElementId> = doc.elements_of(Category::Walls).map(|e| e.id).collect();
        // Offset the west wall (x = 0) 1000 to the east as a copy.
        let copy = doc.offset_copy(walls[0], 1000.0, DVec2::new(500.0, 4000.0)).unwrap();
        let w = doc.element(copy).unwrap().as_wall().unwrap().clone();
        assert!((w.start.x - 1000.0).abs() < 1e-6, "{:?}", w.start);
        // Align its centreline back onto grid 1 (x = 0).
        let grid = doc.elements_of(Category::Grids).next().unwrap().id;
        let reference = doc.reference_lines(grid)[0];
        doc.align(reference, copy, DVec2::new(1000.0, 4000.0)).unwrap();
        let w = doc.element(copy).unwrap().as_wall().unwrap().clone();
        assert!(w.start.x.abs() < 1e-6, "{:?}", w.start);
        // Pinned: cannot align or move.
        doc.set_pinned(&[copy], true);
        assert!(doc.align(reference, copy, DVec2::ZERO).is_err());
        // Array of a column: 4 in all, 2000 apart.
        let col = doc.elements_of(Category::StructuralColumns).next().map(|e| e.id);
        if let Some(col) = col {
            let new = doc.array_linear(&[col], DVec2::new(2000.0, 0.0), 4);
            assert_eq!(new.len(), 3);
        }
        // Paste the exterior walls to Level 2: they stand on Level 2.
        let levels = doc.levels();
        let (l1, l2) = (levels[0].0, levels[1].0);
        let new = doc.paste_aligned(&walls[..4], l1, &[l2]);
        assert_eq!(new.len(), 4);
        for id in new {
            let w = doc.element(id).unwrap().as_wall().unwrap();
            assert_eq!(w.base_level, l2);
            assert!(matches!(w.top, TopConstraint::Unconnected { .. }), "no level above Level 2");
        }
    }

    #[test]
    fn flip_and_disallow_join() {
        use crate::geometry::wall_outline;
        let mut doc = sample_project();
        let walls: Vec<ElementId> = doc.elements_of(Category::Walls).map(|e| e.id).collect();
        // The south wall hosts a door: flipping keeps the door in place.
        let south = walls[3];
        let door = doc.openings_in(south).map(|(e, _)| e.id).next().unwrap();
        let before = crate::geometry::element_geometry(&doc, door).unwrap();
        let outline = wall_outline(&doc, south).unwrap();
        doc.flip_wall(south).unwrap();
        let after = crate::geometry::element_geometry(&doc, door).unwrap();
        let bounds = |g: &crate::geometry::ElementGeometry| {
            g.parts
                .iter()
                .flat_map(|p| p.faces())
                .flat_map(|f| f.pts)
                .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.x + p.y), b.max(p.x + p.y)))
        };
        let (b0, b1) = (bounds(&before), bounds(&after));
        assert!((b0.0 - b1.0).abs() < 1.0 && (b0.1 - b1.1).abs() < 1.0, "{b0:?} {b1:?}");
        // Same footprint, exterior on the other side.
        let flipped = wall_outline(&doc, south).unwrap();
        assert!(flipped[0].distance(outline[2]) < 1e-6, "{outline:?} {flipped:?}");
        // Disallow Join at the west wall's start: its end stays square.
        let west = walls[0];
        doc.set_join_allowed(west, 0, false).unwrap();
        let o = wall_outline(&doc, west).unwrap();
        let w = doc.element(west).unwrap().as_wall().unwrap();
        assert!((o[0].y - w.start.y).abs() < 1e-6 && (o[3].y - w.start.y).abs() < 1e-6, "{o:?}");
    }
}
