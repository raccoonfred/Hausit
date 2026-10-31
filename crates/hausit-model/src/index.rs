//! Lookup tables derived from a document: elements by category, openings
//! by host wall, and a spatial hash of walls for joins and picking. Built
//! on first use after each change, so large models stay interactive.

use crate::category::Category;
use crate::element::ElementData;
use hausit_core::{DVec2, ElementId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Side of a spatial-hash cell, mm.
const CELL: f64 = 2000.0;

#[derive(Debug, Default)]
pub struct DocIndex {
    by_category: HashMap<Category, Vec<ElementId>>,
    openings_by_host: HashMap<ElementId, Vec<ElementId>>,
    /// Walls by (base level, cell) of every cell their segment passes near.
    wall_cells: HashMap<(ElementId, i64, i64), Vec<ElementId>>,
    /// Curtain walls, for embedding them in basic walls.
    curtain_walls: Vec<ElementId>,
    /// The model group each grouped element belongs to.
    group_of: HashMap<ElementId, ElementId>,
}

fn cell(p: DVec2) -> (i64, i64) {
    ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64)
}

impl DocIndex {
    pub(crate) fn build<'a>(elements: impl Iterator<Item = &'a crate::Element>, is_curtain_type: impl Fn(ElementId) -> bool) -> Self {
        let mut ix = DocIndex::default();
        for e in elements {
            ix.by_category.entry(e.category()).or_default().push(e.id);
            match &e.data {
                ElementData::Door(o) | ElementData::Window(o) => ix.openings_by_host.entry(o.host).or_default().push(e.id),
                ElementData::Group(g) => {
                    for m in &g.members {
                        ix.group_of.insert(*m, e.id);
                    }
                }
                ElementData::Wall(w) => {
                    if is_curtain_type(w.type_id) {
                        ix.curtain_walls.push(e.id);
                    }
                    // Margin covers wall thickness and join tolerance.
                    let m = DVec2::splat(1000.0);
                    let pts = w.centreline();
                    let (min, max) = pts.iter().fold((pts[0], pts[0]), |(a, b), p| (a.min(*p), b.max(*p)));
                    let (lo, hi) = (cell(min - m), cell(max + m));
                    if (hi.0 - lo.0 + 1) * (hi.1 - lo.1 + 1) > 10_000 {
                        // Very long walls go in a catch-all bucket.
                        ix.wall_cells.entry((w.base_level, i64::MIN, i64::MIN)).or_default().push(e.id);
                        continue;
                    }
                    for x in lo.0..=hi.0 {
                        for y in lo.1..=hi.1 {
                            ix.wall_cells.entry((w.base_level, x, y)).or_default().push(e.id);
                        }
                    }
                }
                _ => {}
            }
        }
        ix
    }

    pub fn group_of(&self, id: ElementId) -> Option<ElementId> {
        self.group_of.get(&id).copied()
    }

    pub fn curtain_walls(&self) -> &[ElementId] {
        &self.curtain_walls
    }

    pub fn of_category(&self, cat: Category) -> &[ElementId] {
        self.by_category.get(&cat).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn openings_of(&self, wall: ElementId) -> &[ElementId] {
        self.openings_by_host.get(&wall).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Walls on `level` that may pass within about 1 m of `p`.
    pub fn walls_near(&self, level: ElementId, p: DVec2) -> Vec<ElementId> {
        let c = cell(p);
        let mut out: Vec<ElementId> = self.wall_cells.get(&(level, c.0, c.1)).cloned().unwrap_or_default();
        out.extend(self.wall_cells.get(&(level, i64::MIN, i64::MIN)).into_iter().flatten());
        out
    }
}

/// The cached index of a document, rebuilt when its revision changes.
#[derive(Default)]
pub(crate) struct IndexCache(Mutex<Option<(u64, Arc<DocIndex>)>>);

impl IndexCache {
    pub(crate) fn get(&self, revision: u64, build: impl FnOnce() -> DocIndex) -> Arc<DocIndex> {
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        match &*slot {
            Some((r, ix)) if *r == revision => ix.clone(),
            _ => {
                let ix = Arc::new(build());
                *slot = Some((revision, ix.clone()));
                ix
            }
        }
    }
}

impl Clone for IndexCache {
    fn clone(&self) -> Self {
        IndexCache::default()
    }
}

impl PartialEq for IndexCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for IndexCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("IndexCache")
    }
}
