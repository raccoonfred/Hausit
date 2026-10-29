use crate::category::Category;
use crate::element::*;
use crate::family::FamilyType;
use crate::view::{View, ViewKind};
use hausit_core::{DVec2, ElementId, LengthUnit};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ModelError {
    #[error("element {0} not found")]
    NotFound(ElementId),
    #[error("unknown parameter \"{0}\"")]
    UnknownParameter(String),
    #[error("parameter \"{0}\" is read-only")]
    ReadOnly(String),
    #[error("invalid value for \"{0}\"")]
    InvalidValue(String),
    #[error("{0}")]
    Rejected(String),
}

/// Project Information (Manage ▸ Project Information).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub number: String,
    pub client: String,
    pub address: String,
    pub author: String,
    pub status: String,
}

/// A Hausit project: every element, type and view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub project_info: ProjectInfo,
    #[serde(default)]
    pub length_unit: LengthUnit,
    #[serde(default)]
    pub object_styles: crate::styles::ObjectStyles,
    #[serde(default)]
    pub line_weights: crate::styles::LineWeights,
    #[serde(default = "crate::materials::default_materials")]
    pub materials: Vec<crate::materials::MaterialDef>,
    /// Loadable families in the project.
    #[serde(default)]
    pub families: Vec<crate::component::FamilyDef>,
    elements: IndexMap<ElementId, Element>,
    types: IndexMap<ElementId, FamilyType>,
    views: IndexMap<ElementId, View>,
    next_id: u64,
    /// Bumped on every change; lets views cache derived graphics.
    #[serde(skip)]
    revision: u64,
    #[serde(skip)]
    index: crate::index::IndexCache,
}

impl Default for Document {
    fn default() -> Self {
        Self::empty()
    }
}

impl Document {
    /// A document with nothing in it. See [`crate::template`] for a usable
    /// starting project.
    pub fn empty() -> Self {
        Document {
            project_info: ProjectInfo {
                name: "Project Name".into(),
                ..Default::default()
            },
            length_unit: LengthUnit::Millimeters,
            object_styles: Default::default(),
            line_weights: Default::default(),
            materials: crate::materials::default_materials(),
            families: Vec::new(),
            elements: IndexMap::new(),
            types: IndexMap::new(),
            views: IndexMap::new(),
            next_id: 1,
            revision: 0,
            index: Default::default(),
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Continues the revision counter of a document this one replaces, so
    /// caches keyed on the revision never see a stale number.
    pub(crate) fn continue_revision_from(&mut self, previous: u64) {
        self.revision = self.revision.max(previous) + 1;
    }

    /// Marks the document as changed.
    pub fn touch(&mut self) {
        self.revision += 1;
    }

    fn alloc_id(&mut self) -> ElementId {
        let id = ElementId(self.next_id);
        self.next_id += 1;
        id
    }

    // ----- elements ---------------------------------------------------------

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.elements.values()
    }

    pub fn elements_of(&self, cat: Category) -> impl Iterator<Item = &Element> {
        let ids = self.index().of_category(cat).to_vec();
        ids.into_iter().filter_map(move |id| self.elements.get(&id))
    }

    /// Lookup tables for this revision of the document.
    pub fn index(&self) -> std::sync::Arc<crate::index::DocIndex> {
        self.index
            .get(self.revision, || crate::index::DocIndex::build(self.elements.values()))
    }

    pub fn element(&self, id: ElementId) -> Option<&Element> {
        self.elements.get(&id)
    }

    pub fn element_mut(&mut self, id: ElementId) -> Option<&mut Element> {
        self.revision += 1;
        self.elements.get_mut(&id)
    }

    pub fn contains(&self, id: ElementId) -> bool {
        self.elements.contains_key(&id) || self.types.contains_key(&id) || self.views.contains_key(&id)
    }

    pub fn add_element(&mut self, data: ElementData) -> ElementId {
        let id = self.alloc_id();
        let mark = match data.category() {
            Category::Doors | Category::Windows => self.next_mark(data.category()),
            _ => String::new(),
        };
        self.elements.insert(
            id,
            Element {
                id,
                mark,
                comments: String::new(),
                data,
            },
        );
        self.touch();
        id
    }

    /// Next free numeric Mark for a category, as Revit auto-numbers doors.
    fn next_mark(&self, cat: Category) -> String {
        // Scans directly: called while adding elements, when the index is stale.
        let max = self
            .elements
            .values()
            .filter(|e| e.category() == cat)
            .filter_map(|e| e.mark.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        (max + 1).to_string()
    }

    /// Ids that would be deleted together with `ids` (hosted openings,
    /// elements on a deleted level, and the level's plan views).
    pub fn dependents(&self, ids: &[ElementId]) -> Vec<ElementId> {
        let mut out: Vec<ElementId> = ids.to_vec();
        let mut i = 0;
        while i < out.len() {
            let id = out[i];
            for e in self.elements.values() {
                let dep = match &e.data {
                    ElementData::Door(o) | ElementData::Window(o) => o.host == id,
                    ElementData::Wall(w) => w.base_level == id,
                    ElementData::Floor(f) => f.level == id,
                    ElementData::Roof(r) => r.level == id,
                    ElementData::Column(c) => c.base_level == id,
                    ElementData::Framing(f) => f.level == id,
                    ElementData::Footing(f) => f.level == id,
                    ElementData::WallFooting(f) => f.wall == id,
                    ElementData::Stair(s) => s.base_level == id,
                    ElementData::Ceiling(c) => c.level == id,
                    ElementData::Component(c) => c.level == id,
                    ElementData::DirectShape(d) => d.level == id,
                    ElementData::Railing(r) => r.level == id || r.host == id,
                    ElementData::Room(r) => r.level == id,
                    ElementData::RoomSeparation(l) => l.level == id,
                    // Annotation goes with its view; tags with what they tag.
                    ElementData::TextNote(t) => t.view == id,
                    ElementData::Tag(t) => t.view == id || t.host == id,
                    // Dimensions go with their view or any element they measure.
                    ElementData::Dimension(d) => {
                        d.view == id
                            || d.refs.iter().any(|r| match r {
                                DimRef::Wall { wall, .. } => *wall == id,
                                DimRef::Grid { grid } => *grid == id,
                                DimRef::Line { .. } => false,
                            })
                    }
                    _ => false,
                };
                if dep && !out.contains(&e.id) {
                    out.push(e.id);
                }
            }
            for v in self.views.values() {
                if let Some(level) = v.kind.plan_level() {
                    if level == id && !out.contains(&v.id) {
                        out.push(v.id);
                    }
                }
            }
            i += 1;
        }
        out
    }

    /// Deletes elements and everything that depends on them. Walls whose
    /// top was constrained to a deleted level become unconnected.
    pub fn delete(&mut self, ids: &[ElementId]) -> Result<Vec<ElementId>, ModelError> {
        let all = self.dependents(ids);
        let levels_left = self.elements_of(Category::Levels).filter(|l| !all.contains(&l.id)).count();
        if levels_left == 0 {
            return Err(ModelError::Rejected("A project must have at least one level.".into()));
        }
        // Detach top constraints that point at deleted levels.
        let heights: Vec<(ElementId, f64)> = self
            .elements
            .values()
            .filter_map(|e| match &e.data {
                ElementData::Wall(w) => match w.top {
                    TopConstraint::UpToLevel { level, .. } if all.contains(&level) => Some((e.id, self.wall_height(w))),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        for (id, h) in heights {
            if let Some(ElementData::Wall(w)) = self.elements.get_mut(&id).map(|e| &mut e.data) {
                w.top = TopConstraint::Unconnected { height: h.max(1.0) };
            }
        }
        for id in &all {
            self.elements.shift_remove(id);
            self.views.shift_remove(id);
        }
        self.touch();
        Ok(all)
    }

    /// Railings hosted by stairs among `ids` that are not in `ids` themselves.
    fn stair_railings(&self, ids: &[ElementId]) -> Vec<ElementId> {
        self.elements
            .values()
            .filter(|e| matches!(&e.data, ElementData::Railing(r) if ids.contains(&r.host)) && !ids.contains(&e.id))
            .map(|e| e.id)
            .collect()
    }

    /// Moves elements by `delta` in plan. Hosted openings move with walls.
    pub fn translate(&mut self, ids: &[ElementId], delta: DVec2) {
        for id in ids {
            let Some(e) = self.elements.get_mut(id) else { continue };
            match &mut e.data {
                ElementData::Wall(w) => {
                    w.start += delta;
                    w.end += delta;
                }
                ElementData::Grid(g) => {
                    g.start += delta;
                    g.end += delta;
                }
                ElementData::Floor(f) => f.boundary.iter_mut().for_each(|p| *p += delta),
                ElementData::Roof(r) => r.footprint.iter_mut().for_each(|p| *p += delta),
                ElementData::Column(c) => c.point += delta,
                ElementData::Framing(f) => {
                    f.start += delta;
                    f.end += delta;
                }
                ElementData::Footing(f) => f.point += delta,
                ElementData::Component(c) => c.point += delta,
                ElementData::DirectShape(d) => d.points_mut().for_each(|(x, y)| {
                    *x += delta.x;
                    *y += delta.y;
                }),
                ElementData::Stair(s) => s.path.iter_mut().for_each(|p| *p += delta),
                ElementData::Ceiling(c) => c.boundary.iter_mut().for_each(|p| *p += delta),
                ElementData::Railing(r) => r.path.iter_mut().for_each(|p| *p += delta),
                ElementData::Room(r) => r.point += delta,
                ElementData::RoomSeparation(l) => {
                    l.start += delta;
                    l.end += delta;
                }
                ElementData::TextNote(t) => {
                    t.position += delta;
                    if let Some(l) = &mut t.leader {
                        *l += delta;
                    }
                }
                ElementData::Tag(t) => t.position += delta,
                ElementData::Dimension(d) => {
                    d.line_point += delta;
                    for r in &mut d.refs {
                        if let DimRef::Line { point, .. } = r {
                            *point += delta;
                        }
                    }
                }
                ElementData::Door(_) | ElementData::Window(_) | ElementData::WallFooting(_) | ElementData::Level(_) => {}
            }
        }
        // Railings on a stair move with it.
        let riders = self.stair_railings(ids);
        for id in riders {
            if let Some(ElementData::Railing(r)) = self.elements.get_mut(&id).map(|e| &mut e.data) {
                r.path.iter_mut().for_each(|p| *p += delta);
            }
        }
        // Openings selected without their host slide along it.
        for id in ids {
            let host = match self.elements.get(id).and_then(|e| e.as_opening()) {
                Some(o) if !ids.contains(&o.host) => o.host,
                _ => continue,
            };
            let dir = match self.elements.get(&host).and_then(|e| e.as_wall()) {
                Some(w) => w.direction(),
                None => continue,
            };
            if let Some(ElementData::Door(o) | ElementData::Window(o)) = self.elements.get_mut(id).map(|e| &mut e.data) {
                o.offset += delta.dot(dir);
            }
        }
        self.touch();
    }

    /// Copies elements, offsetting them by `delta`. Openings whose hosts are
    /// copied are re-hosted on the copies. Returns the new ids.
    pub fn copy(&mut self, ids: &[ElementId], delta: DVec2) -> Vec<ElementId> {
        let mut map = std::collections::HashMap::new();
        let mut new_ids = Vec::new();
        // Hosts first so openings and wall foundations can be re-hosted.
        let hosted = |e: &Element| e.category().is_hosted() || matches!(e.data, ElementData::WallFooting(_) | ElementData::Railing(_));
        let mut order: Vec<ElementId> = ids
            .iter()
            .copied()
            .filter(|i| self.element(*i).map(|e| !hosted(e)).unwrap_or(false))
            .collect();
        order.extend(ids.iter().copied().filter(|i| self.element(*i).map(hosted).unwrap_or(false)));
        for id in order {
            let Some(src) = self.element(id).cloned() else { continue };
            let mut data = src.data.clone();
            match &mut data {
                ElementData::Level(l) => l.name = self.next_level_name(),
                ElementData::Grid(g) => g.name = self.next_grid_name(),
                ElementData::Door(o) | ElementData::Window(o) => match map.get(&o.host) {
                    Some(h) => o.host = *h,
                    None => continue, // Revit also requires the host when copying in plan.
                },
                ElementData::WallFooting(f) => match map.get(&f.wall) {
                    Some(w) => f.wall = *w,
                    None => continue,
                },
                // A railing copied without its stair becomes free-standing.
                ElementData::Railing(r) if r.host.is_valid() => r.host = map.get(&r.host).copied().unwrap_or_default(),
                ElementData::Room(r) => r.number = self.next_room_number(),
                // Dimensions stay with the elements they measure.
                ElementData::Dimension(_) => continue,
                // A tag copied with its element tags the copy.
                ElementData::Tag(t) => {
                    if let Some(h) = map.get(&t.host) {
                        t.host = *h;
                    }
                }
                _ => {}
            }
            let nid = self.add_element(data);
            if let Some(e) = self.elements.get_mut(&nid) {
                e.comments = src.comments.clone();
            }
            map.insert(id, nid);
            new_ids.push(nid);
        }
        self.translate(
            &new_ids
                .iter()
                .copied()
                .filter(|i| self.element(*i).map(|e| !e.category().is_hosted()).unwrap_or(false))
                .collect::<Vec<_>>(),
            delta,
        );
        new_ids
    }

    /// Next free room number ("1", "2", …), as Revit increments them.
    pub fn next_room_number(&self) -> String {
        let max = self
            .elements
            .values()
            .filter_map(|e| match &e.data {
                ElementData::Room(r) => r.number.parse::<u64>().ok(),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        (max + 1).to_string()
    }

    /// Rotates elements about `center` by `angle` degrees (counter-clockwise).
    pub fn rotate(&mut self, ids: &[ElementId], center: DVec2, angle: f64) {
        let (s, c) = angle.to_radians().sin_cos();
        let f = move |p: DVec2| {
            let d = p - center;
            center + DVec2::new(d.x * c - d.y * s, d.x * s + d.y * c)
        };
        self.transform(ids, &f, false, angle);
    }

    /// Mirrors elements about the line through `a` and `b`.
    pub fn mirror(&mut self, ids: &[ElementId], a: DVec2, b: DVec2) {
        let d = (b - a).normalize_or_zero();
        let f = move |p: DVec2| {
            let v = p - a;
            a + d * (2.0 * v.dot(d)) - v
        };
        // A column's rotation mirrors about the axis angle.
        let axis = d.y.atan2(d.x).to_degrees();
        self.transform(ids, &f, true, axis);
    }

    /// Applies a plan transform. Mirrored walls are reversed so their
    /// exterior stays outside, and their openings keep their positions.
    fn transform(&mut self, ids: &[ElementId], f: &dyn Fn(DVec2) -> DVec2, mirror: bool, angle: f64) {
        let mut reversed: Vec<(ElementId, f64)> = Vec::new();
        for id in ids {
            let Some(e) = self.elements.get_mut(id) else { continue };
            match &mut e.data {
                ElementData::Wall(w) => {
                    let (a, b) = (f(w.start), f(w.end));
                    if mirror {
                        reversed.push((*id, w.length()));
                        w.start = b;
                        w.end = a;
                    } else {
                        w.start = a;
                        w.end = b;
                    }
                }
                ElementData::Grid(g) => {
                    g.start = f(g.start);
                    g.end = f(g.end);
                }
                ElementData::Floor(fl) => fl.boundary.iter_mut().for_each(|p| *p = f(*p)),
                ElementData::Roof(r) => r.footprint.iter_mut().for_each(|p| *p = f(*p)),
                ElementData::Column(c) => {
                    c.point = f(c.point);
                    c.rotation = if mirror { 2.0 * angle - c.rotation } else { c.rotation + angle };
                }
                ElementData::Framing(fr) => {
                    fr.start = f(fr.start);
                    fr.end = f(fr.end);
                }
                ElementData::Footing(ft) => {
                    ft.point = f(ft.point);
                    ft.rotation = if mirror { 2.0 * angle - ft.rotation } else { ft.rotation + angle };
                }
                ElementData::DirectShape(d) => d.transform(f, mirror),
                ElementData::Component(c) => {
                    c.point = f(c.point);
                    c.rotation = if mirror { 2.0 * angle - c.rotation } else { c.rotation + angle };
                }
                ElementData::Stair(s) => {
                    s.path.iter_mut().for_each(|p| *p = f(*p));
                }
                ElementData::Railing(r) => r.path.iter_mut().for_each(|p| *p = f(*p)),
                ElementData::Ceiling(c) => c.boundary.iter_mut().for_each(|p| *p = f(*p)),
                ElementData::Room(r) => r.point = f(r.point),
                ElementData::RoomSeparation(l) => {
                    l.start = f(l.start);
                    l.end = f(l.end);
                }
                ElementData::TextNote(t) => {
                    t.position = f(t.position);
                    t.leader = t.leader.map(f);
                }
                ElementData::Tag(t) => t.position = f(t.position),
                ElementData::Dimension(d) => {
                    d.line_point = f(d.line_point);
                    for r in &mut d.refs {
                        if let DimRef::Line { point, dir } = r {
                            let p2 = f(*point + *dir);
                            *point = f(*point);
                            *dir = (p2 - *point).normalize_or_zero();
                        }
                    }
                }
                ElementData::Door(_) | ElementData::Window(_) | ElementData::WallFooting(_) | ElementData::Level(_) => {}
            }
        }
        for id in self.stair_railings(ids) {
            if let Some(ElementData::Railing(r)) = self.elements.get_mut(&id).map(|e| &mut e.data) {
                r.path.iter_mut().for_each(|p| *p = f(*p));
            }
        }
        for e in self.elements.values_mut() {
            if let ElementData::Door(o) | ElementData::Window(o) = &mut e.data {
                if let Some((_, len)) = reversed.iter().find(|(w, _)| *w == o.host) {
                    o.offset = len - o.offset;
                    o.flip_hand = !o.flip_hand;
                }
            }
        }
        self.touch();
    }

    // ----- levels -----------------------------------------------------------

    /// Levels sorted by elevation.
    pub fn levels(&self) -> Vec<(ElementId, &Level)> {
        let mut v: Vec<_> = self.elements.values().filter_map(|e| e.as_level().map(|l| (e.id, l))).collect();
        v.sort_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation));
        v
    }

    pub fn level(&self, id: ElementId) -> Option<&Level> {
        self.element(id).and_then(|e| e.as_level())
    }

    pub fn level_elevation(&self, id: ElementId) -> f64 {
        self.level(id).map(|l| l.elevation).unwrap_or(0.0)
    }

    pub fn level_name(&self, id: ElementId) -> String {
        self.level(id).map(|l| l.name.clone()).unwrap_or_else(|| "(none)".into())
    }

    /// The level directly above `id`, if any.
    pub fn level_above(&self, id: ElementId) -> Option<ElementId> {
        let levels = self.levels();
        let i = levels.iter().position(|(l, _)| *l == id)?;
        levels.get(i + 1).map(|(l, _)| *l)
    }

    pub fn next_level_name(&self) -> String {
        next_name(
            self.elements.values().filter_map(|e| e.as_level()).map(|l| l.name.as_str()),
            "Level ",
            1,
        )
    }

    pub fn next_grid_name(&self) -> String {
        let names: Vec<&str> = self
            .elements
            .values()
            .filter_map(|e| match &e.data {
                ElementData::Grid(g) => Some(g.name.as_str()),
                _ => None,
            })
            .collect();
        // Revit increments the last grid's name: 1,2,3… or A,B,C…
        match names.last() {
            None => "1".into(),
            Some(last) => {
                let mut cand = increment_name(last);
                while names.contains(&cand.as_str()) {
                    cand = increment_name(&cand);
                }
                cand
            }
        }
    }

    // ----- walls ------------------------------------------------------------

    pub fn wall_base_z(&self, w: &Wall) -> f64 {
        self.level_elevation(w.base_level) + w.base_offset
    }

    pub fn wall_top_z(&self, w: &Wall) -> f64 {
        match w.top {
            TopConstraint::Unconnected { height } => self.wall_base_z(w) + height,
            TopConstraint::UpToLevel { level, offset } => self.level_elevation(level) + offset,
        }
    }

    pub fn wall_height(&self, w: &Wall) -> f64 {
        (self.wall_top_z(w) - self.wall_base_z(w)).max(0.0)
    }

    /// Doors and windows hosted by a wall.
    pub fn openings_in(&self, wall: ElementId) -> impl Iterator<Item = (&Element, &HostedOpening)> {
        let ids = self.index().openings_of(wall).to_vec();
        ids.into_iter().filter_map(move |id| {
            let e = self.elements.get(&id)?;
            Some((e, e.as_opening()?)).filter(|(_, o)| o.host == wall)
        })
    }

    /// One face area of a wall minus its openings (Revit's "Area").
    pub fn wall_net_area(&self, id: ElementId) -> f64 {
        let Some(w) = self.element(id).and_then(|e| e.as_wall()) else {
            return 0.0;
        };
        let gross = w.length() * self.wall_height(w);
        let holes: f64 = self
            .openings_in(id)
            .map(|(e, _)| self.type_of(e.id).map(|t| t.width() * t.height()).unwrap_or(0.0))
            .sum();
        (gross - holes).max(0.0)
    }

    // ----- types ------------------------------------------------------------

    pub fn material(&self, name: &str) -> Option<&crate::materials::MaterialDef> {
        self.materials.iter().find(|m| m.name == name)
    }

    pub fn family_types(&self) -> impl Iterator<Item = &FamilyType> {
        self.types.values()
    }

    pub fn types_of(&self, cat: Category) -> impl Iterator<Item = &FamilyType> {
        self.types.values().filter(move |t| t.category == cat)
    }

    pub fn family_type(&self, id: ElementId) -> Option<&FamilyType> {
        self.types.get(&id)
    }

    pub fn family_type_mut(&mut self, id: ElementId) -> Option<&mut FamilyType> {
        self.revision += 1;
        self.types.get_mut(&id)
    }

    /// Type of an element instance.
    pub fn type_of(&self, element: ElementId) -> Option<&FamilyType> {
        self.element(element).and_then(|e| e.type_id()).and_then(|t| self.types.get(&t))
    }

    pub fn add_type(&mut self, category: Category, family: &str, name: &str, params: crate::family::TypeParams) -> ElementId {
        let id = self.alloc_id();
        self.types.insert(
            id,
            FamilyType {
                id,
                category,
                family: family.into(),
                name: name.into(),
                params,
                layers: vec![],
            },
        );
        self.touch();
        id
    }

    /// "Duplicate…" in the Type Properties dialog.
    pub fn duplicate_type(&mut self, id: ElementId, new_name: &str) -> Result<ElementId, ModelError> {
        let src = self.types.get(&id).cloned().ok_or(ModelError::NotFound(id))?;
        if self.types.values().any(|t| t.family == src.family && t.name == new_name) {
            return Err(ModelError::Rejected(format!("The name \"{new_name}\" is already in use.")));
        }
        Ok(self.add_type(src.category, &src.family, new_name, src.params))
    }

    pub fn rename_type(&mut self, id: ElementId, new_name: &str) -> Result<(), ModelError> {
        let family = self.types.get(&id).ok_or(ModelError::NotFound(id))?.family.clone();
        if new_name.trim().is_empty() || self.types.values().any(|t| t.id != id && t.family == family && t.name == new_name) {
            return Err(ModelError::Rejected(format!("The name \"{new_name}\" is already in use.")));
        }
        self.types.get_mut(&id).unwrap().name = new_name.to_string();
        self.touch();
        Ok(())
    }

    /// Deletes a type if no instance uses it and it isn't the last of its
    /// category.
    pub fn delete_type(&mut self, id: ElementId) -> Result<(), ModelError> {
        let t = self.types.get(&id).ok_or(ModelError::NotFound(id))?;
        if self.types_of(t.category).count() <= 1 {
            return Err(ModelError::Rejected("The last type in a category cannot be deleted.".into()));
        }
        if self.elements.values().any(|e| e.type_id() == Some(id)) {
            return Err(ModelError::Rejected("The type is in use and cannot be deleted.".into()));
        }
        self.types.shift_remove(&id);
        self.touch();
        Ok(())
    }

    pub fn change_type(&mut self, element: ElementId, new_type: ElementId) -> Result<(), ModelError> {
        let cat = self.element(element).ok_or(ModelError::NotFound(element))?.category();
        let t = self.types.get(&new_type).ok_or(ModelError::NotFound(new_type))?;
        if t.category != cat {
            return Err(ModelError::Rejected("Type belongs to another category.".into()));
        }
        let is_wall_footing = matches!(self.element(element).map(|e| &e.data), Some(ElementData::WallFooting(_)));
        if cat == Category::StructuralFoundations && t.is_wall_foundation() != is_wall_footing {
            return Err(ModelError::Rejected("Isolated and wall foundations use different types.".into()));
        }
        self.elements.get_mut(&element).unwrap().set_type_id(new_type);
        self.touch();
        Ok(())
    }

    // ----- views ------------------------------------------------------------

    pub fn views(&self) -> impl Iterator<Item = &View> {
        self.views.values()
    }

    pub fn view(&self, id: ElementId) -> Option<&View> {
        self.views.get(&id)
    }

    pub fn view_mut(&mut self, id: ElementId) -> Option<&mut View> {
        self.revision += 1;
        self.views.get_mut(&id)
    }

    pub fn add_view(&mut self, name: &str, kind: ViewKind) -> ElementId {
        let id = self.alloc_id();
        self.views.insert(id, View::new(id, name, kind));
        self.touch();
        id
    }

    pub fn delete_view(&mut self, id: ElementId) {
        self.views.shift_remove(&id);
        let callouts: Vec<ElementId> = self.views.values().filter(|v| v.callout_of == Some(id)).map(|v| v.id).collect();
        for c in callouts {
            self.delete_view(c);
        }
        for v in self.views.values_mut() {
            if let ViewKind::Sheet(s) = &mut v.kind {
                s.viewports.retain(|p| p.view != id);
            }
        }
        self.touch();
    }

    /// Adds a callout of `parent`: a copy of the view cropped to the
    /// rectangle `a`–`b` (view coordinates), at 1:20 and Fine detail.
    pub fn add_callout(&mut self, parent: ElementId, a: DVec2, b: DVec2) -> Option<ElementId> {
        let p = self.view(parent)?.clone();
        if !p.kind.is_graphical() || matches!(p.kind, ViewKind::ThreeD | ViewKind::Sheet(_)) {
            return None;
        }
        // "Callout of Section 1", then "Callout of Section 1 (2)"…
        let base = format!("Callout of {}", p.name);
        let taken = |n: &str| self.views.values().any(|v| v.name == n);
        let name = (1..)
            .map(|k| if k == 1 { base.clone() } else { format!("{base} ({k})") })
            .find(|n| !taken(n))
            .unwrap_or(base);
        let id = self.add_view(&name, p.kind.clone());
        let v = self.views.get_mut(&id)?;
        v.cut_height = p.cut_height;
        v.visual_style = p.visual_style;
        v.scale = 20;
        v.detail_level = crate::DetailLevel::Fine;
        v.crop = Some((a.min(b), a.max(b)));
        v.callout_of = Some(parent);
        Some(id)
    }

    /// Renames the floor and ceiling plans of `level` named `old`.
    pub fn rename_level_views(&mut self, level: ElementId, old: &str, new: &str) {
        for v in self.views.values_mut() {
            if v.kind.plan_level() == Some(level) && v.name == old {
                v.name = new.to_string();
            }
        }
        self.touch();
    }

    /// A view name not yet used, based on `base` ("Section 1", "Section 2"…).
    pub fn unique_view_name(&self, base: &str) -> String {
        next_name(self.views.values().map(|v| v.name.as_str()), &format!("{base} "), 1)
    }

    // ----- misc -------------------------------------------------------------

    /// "Category : Family : Type" description used in the status bar.
    pub fn describe(&self, id: ElementId) -> String {
        if let Some(e) = self.element(id) {
            let cat = e.category().name();
            match (&e.data, self.type_of(id)) {
                (ElementData::Level(l), _) => format!("{cat} : Level : {}", l.name),
                (ElementData::Grid(g), _) => format!("{cat} : Grid : {}", g.name),
                (_, Some(t)) => format!("{cat} : {} : {}", t.family, t.name),
                _ => cat.to_string(),
            }
        } else if let Some(t) = self.types.get(&id) {
            format!("{} : {}", t.category.name(), t.display_name())
        } else if let Some(v) = self.views.get(&id) {
            format!("View : {}", v.name)
        } else {
            "<none>".into()
        }
    }
}

fn next_name<'a>(existing: impl Iterator<Item = &'a str>, prefix: &str, start: u64) -> String {
    let max = existing
        .filter_map(|n| n.strip_prefix(prefix).and_then(|r| r.trim().parse::<u64>().ok()))
        .max()
        .map(|m| m + 1)
        .unwrap_or(start);
    format!("{prefix}{max}")
}

/// "1"→"2", "A"→"B", "Z"→"AA", "Level 9"→"Level 10".
pub(crate) fn increment_name(name: &str) -> String {
    let digits: String = name
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if !digits.is_empty() {
        let n: u64 = digits.parse().unwrap_or(0);
        return format!("{}{}", &name[..name.len() - digits.len()], n + 1);
    }
    let mut chars: Vec<char> = name.chars().collect();
    let mut i = chars.len();
    loop {
        if i == 0 {
            chars.insert(0, 'A');
            break;
        }
        i -= 1;
        match chars[i] {
            'Z' => chars[i] = 'A',
            'z' => chars[i] = 'a',
            c if c.is_ascii_alphabetic() => {
                chars[i] = (c as u8 + 1) as char;
                break;
            }
            _ => return format!("{name}1"),
        }
    }
    chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_increments() {
        assert_eq!(increment_name("1"), "2");
        assert_eq!(increment_name("A"), "B");
        assert_eq!(increment_name("Z"), "AA");
        assert_eq!(increment_name("Level 9"), "Level 10");
    }
}
