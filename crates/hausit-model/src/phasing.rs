//! Phasing, as Revit does it: a project's phases in time order, the phase
//! each element was created and demolished in, and the phase filters that
//! decide how a view shows existing, new, demolished and temporary work.

use crate::category::Category;
use crate::document::Document;
use crate::element::{Element, ElementData};
use crate::view::View;
use hausit_core::ElementId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A phase of the project (Manage ▸ Phases), earliest first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Phase {
    pub id: ElementId,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// Where an element stands in a view's phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PhaseStatus {
    /// Created in an earlier phase and still standing.
    Existing,
    /// Created in an earlier phase, demolished in this one.
    Demolished,
    /// Created in this phase.
    New,
    /// Created and demolished in this phase.
    Temporary,
}

impl PhaseStatus {
    pub const ALL: [PhaseStatus; 4] = [
        PhaseStatus::Existing,
        PhaseStatus::Demolished,
        PhaseStatus::New,
        PhaseStatus::Temporary,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PhaseStatus::Existing => "Existing",
            PhaseStatus::Demolished => "Demolished",
            PhaseStatus::New => "New",
            PhaseStatus::Temporary => "Temporary",
        }
    }
}

/// How a phase filter shows elements of one status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhaseDisplay {
    ByCategory,
    /// Drawn with the phase's graphic overrides.
    Overridden,
    NotDisplayed,
}

/// The phase filters every Revit project starts with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhaseFilter {
    /// No filter: everything by category.
    None,
    #[default]
    ShowAll,
    ShowComplete,
    ShowDemoNew,
    ShowNew,
    ShowPrevious,
    ShowPreviousDemo,
    ShowPreviousNew,
}

impl PhaseFilter {
    pub const ALL: [PhaseFilter; 8] = [
        PhaseFilter::None,
        PhaseFilter::ShowAll,
        PhaseFilter::ShowComplete,
        PhaseFilter::ShowDemoNew,
        PhaseFilter::ShowNew,
        PhaseFilter::ShowPrevious,
        PhaseFilter::ShowPreviousDemo,
        PhaseFilter::ShowPreviousNew,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PhaseFilter::None => "None",
            PhaseFilter::ShowAll => "Show All",
            PhaseFilter::ShowComplete => "Show Complete",
            PhaseFilter::ShowDemoNew => "Show Demo + New",
            PhaseFilter::ShowNew => "Show New",
            PhaseFilter::ShowPrevious => "Show Previous Phase",
            PhaseFilter::ShowPreviousDemo => "Show Previous + Demo",
            PhaseFilter::ShowPreviousNew => "Show Previous + New",
        }
    }

    pub fn display(self, status: PhaseStatus) -> PhaseDisplay {
        use PhaseDisplay::*;
        use PhaseStatus as S;
        match (self, status) {
            (PhaseFilter::None, _) => ByCategory,
            (PhaseFilter::ShowAll, S::New) => ByCategory,
            (PhaseFilter::ShowAll, _) => Overridden,
            (PhaseFilter::ShowComplete, S::New | S::Existing) => ByCategory,
            (PhaseFilter::ShowDemoNew, S::New) => ByCategory,
            (PhaseFilter::ShowDemoNew, S::Demolished) => Overridden,
            (PhaseFilter::ShowNew, S::New) => ByCategory,
            (PhaseFilter::ShowPrevious, S::Existing) => Overridden,
            (PhaseFilter::ShowPreviousDemo, S::Existing | S::Demolished) => Overridden,
            (PhaseFilter::ShowPreviousNew, S::New) => ByCategory,
            (PhaseFilter::ShowPreviousNew, S::Existing) => Overridden,
            _ => NotDisplayed,
        }
    }
}

/// Whether an element takes part in phasing. Datums, rooms and
/// view-specific annotation do not (tags follow what they tag).
pub fn is_phased(cat: Category) -> bool {
    cat.is_model()
}

/// The phases a new project has.
pub fn default_phases(mut alloc: impl FnMut() -> ElementId) -> Vec<Phase> {
    ["Existing", "New Construction"]
        .into_iter()
        .map(|name| Phase {
            id: alloc(),
            name: name.into(),
            description: String::new(),
        })
        .collect()
}

impl Document {
    pub fn phase(&self, id: ElementId) -> Option<&Phase> {
        self.phases.iter().find(|p| p.id == id)
    }

    /// Position of a phase in time; `None` and unknown phases count as the
    /// latest one, as elements made before phasing existed were.
    pub fn phase_index(&self, id: Option<ElementId>) -> usize {
        id.and_then(|id| self.phases.iter().position(|p| p.id == id))
            .unwrap_or(self.phases.len().saturating_sub(1))
    }

    pub fn phase_name(&self, id: Option<ElementId>) -> String {
        self.phases.get(self.phase_index(id)).map(|p| p.name.clone()).unwrap_or_default()
    }

    /// The phase new elements are created in when drawn in `view`.
    pub fn view_phase(&self, view: &View) -> Option<ElementId> {
        view.phase
            .filter(|p| self.phase(*p).is_some())
            .or_else(|| self.phases.last().map(|p| p.id))
    }

    /// Where an element stands in a phase, or `None` when it does not
    /// exist then (created later, or demolished earlier).
    pub fn phase_status(&self, e: &Element, phase: usize) -> Option<PhaseStatus> {
        let created = self.phase_index(e.phase_created);
        let demolished = e.phase_demolished.map(|d| self.phase_index(Some(d)));
        if created > phase || demolished.is_some_and(|d| d < phase) {
            return None;
        }
        Some(match (created == phase, demolished == Some(phase)) {
            (true, true) => PhaseStatus::Temporary,
            (true, false) => PhaseStatus::New,
            (false, true) => PhaseStatus::Demolished,
            (false, false) => PhaseStatus::Existing,
        })
    }

    /// How a view shows each phased element it can show: hidden elements
    /// are left out, and so are tags of hidden elements.
    pub fn phase_displays(&self, view: &View) -> HashMap<ElementId, (PhaseStatus, PhaseDisplay)> {
        let phase = self.phase_index(self.view_phase(view));
        let mut out = HashMap::new();
        for e in self.elements() {
            if !is_phased(e.category()) {
                continue;
            }
            let shown = self.phase_status(e, phase).map(|s| (s, view.phase_filter.display(s)));
            out.insert(e.id, shown.unwrap_or((PhaseStatus::Existing, PhaseDisplay::NotDisplayed)));
        }
        out
    }

    /// Elements a view's phase and phase filter hide, with the openings in
    /// hidden walls and the tags of hidden elements.
    pub fn phase_hidden(&self, view: &View) -> std::collections::HashSet<ElementId> {
        let shows = self.phase_displays(view);
        let hidden: std::collections::HashSet<ElementId> = shows
            .iter()
            .filter(|(_, (_, d))| *d == PhaseDisplay::NotDisplayed)
            .map(|(id, _)| *id)
            .collect();
        self.with_dependents_hidden(hidden)
    }

    /// Everything a view leaves out: by phase, Hide in View and Temporary
    /// Hide/Isolate.
    pub fn view_hidden(&self, view: &View) -> std::collections::HashSet<ElementId> {
        let mut hidden = self.phase_hidden(view);
        hidden.extend(view.hidden_elements.iter().copied());
        if let Some(t) = &view.temp_hide {
            for e in self.elements() {
                let listed = t.elements.contains(&e.id) || t.categories.contains(&e.category());
                let hide = if t.isolate {
                    !listed && !crate::editing::always_visible(e.category())
                } else {
                    listed
                };
                if hide {
                    hidden.insert(e.id);
                }
            }
            if t.isolate {
                // Isolated hosts keep their doors and windows, and their tags.
                let kept: Vec<ElementId> = self
                    .elements()
                    .filter(|e| match &e.data {
                        ElementData::Door(o) | ElementData::Window(o) => !hidden.contains(&o.host),
                        ElementData::Tag(t) => !hidden.contains(&t.host),
                        _ => false,
                    })
                    .map(|e| e.id)
                    .collect();
                for id in kept {
                    hidden.remove(&id);
                }
            }
        }
        self.with_dependents_hidden(hidden)
    }

    fn with_dependents_hidden(&self, mut hidden: std::collections::HashSet<ElementId>) -> std::collections::HashSet<ElementId> {
        for e in self.elements() {
            let host = match &e.data {
                ElementData::Door(o) | ElementData::Window(o) => Some(o.host),
                ElementData::Tag(t) => Some(t.host),
                ElementData::WallFooting(f) => Some(f.wall),
                _ => None,
            };
            if host.is_some_and(|h| hidden.contains(&h)) {
                hidden.insert(e.id);
            }
        }
        hidden
    }

    /// Demolish (Modify ▸ Demolish): marks elements as demolished in a
    /// phase. Doors and windows go with their wall, as in Revit. Elements
    /// created after the phase cannot be demolished in it.
    pub fn demolish(&mut self, ids: &[ElementId], phase: ElementId) -> Vec<ElementId> {
        let at = self.phase_index(Some(phase));
        let mut targets: Vec<ElementId> = ids.to_vec();
        for e in self.elements() {
            if let ElementData::Door(o) | ElementData::Window(o) = &e.data {
                if ids.contains(&o.host) && !targets.contains(&e.id) {
                    targets.push(e.id);
                }
            }
        }
        let mut done = Vec::new();
        for id in targets {
            let ok = self
                .element(id)
                .is_some_and(|e| is_phased(e.category()) && self.phase_index(e.phase_created) <= at);
            if ok {
                if let Some(e) = self.element_mut(id) {
                    e.phase_demolished = Some(phase);
                    done.push(id);
                }
            }
        }
        done
    }

    /// Adds a phase after `after` (or at the end), named uniquely.
    pub fn add_phase(&mut self, after: Option<ElementId>, name: &str) -> ElementId {
        let id = self.new_id();
        let mut name = name.to_string();
        let mut n = 1;
        while self.phases.iter().any(|p| p.name == name) {
            n += 1;
            name = format!("{} {n}", name.trim_end_matches(|c: char| c.is_ascii_digit() || c == ' '));
        }
        let at = after
            .and_then(|a| self.phases.iter().position(|p| p.id == a))
            .map(|i| i + 1)
            .unwrap_or(self.phases.len());
        self.phases.insert(
            at,
            Phase {
                id,
                name,
                description: String::new(),
            },
        );
        self.touch();
        id
    }

    /// Combines a phase with the previous one (or the next, for the first),
    /// as Revit's "Combine with" does: its elements and views move there.
    pub fn combine_phase(&mut self, id: ElementId) -> bool {
        let Some(i) = self.phases.iter().position(|p| p.id == id) else {
            return false;
        };
        if self.phases.len() < 2 {
            return false;
        }
        let into = self.phases[if i == 0 { 1 } else { i - 1 }].id;
        self.phases.remove(i);
        let ids: Vec<ElementId> = self.elements().map(|e| e.id).collect();
        for eid in ids {
            if let Some(e) = self.element_mut(eid) {
                if e.phase_created == Some(id) {
                    e.phase_created = Some(into);
                }
                if e.phase_demolished == Some(id) {
                    e.phase_demolished = Some(into);
                }
            }
        }
        // An element demolished in the phase it is now created in stays
        // temporary; one demolished before it was created would vanish, so
        // it moves with the creation.
        let views: Vec<ElementId> = self.views().map(|v| v.id).collect();
        for v in views {
            if let Some(v) = self.view_mut(v) {
                if v.phase == Some(id) {
                    v.phase = Some(into);
                }
            }
        }
        self.touch();
        true
    }

    /// Gives files saved before phasing their phases.
    pub fn ensure_phases(&mut self) {
        if self.phases.is_empty() {
            let mut ids = Vec::new();
            for _ in 0..2 {
                ids.push(self.new_id());
            }
            let mut it = ids.into_iter();
            self.phases = default_phases(|| it.next().unwrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::sample_project;

    #[test]
    fn phase_filters_and_demolish() {
        let mut doc = sample_project();
        let existing = doc.phases[0].id;
        let new = doc.phases[1].id;
        let walls: Vec<ElementId> = doc.elements_of(Category::Walls).map(|e| e.id).collect();
        // The first wall stood before; it is demolished in New Construction.
        doc.element_mut(walls[0]).unwrap().phase_created = Some(existing);
        let done = doc.demolish(&[walls[0]], new);
        assert!(done.len() > 1, "its doors and windows go too: {done:?}");
        let mut view = doc.views().find(|v| v.kind.plan_level().is_some()).unwrap().clone();
        let status = |doc: &Document, id| doc.phase_status(doc.element(id).unwrap(), 1);
        assert_eq!(status(&doc, walls[0]), Some(PhaseStatus::Demolished));
        assert_eq!(status(&doc, walls[1]), Some(PhaseStatus::New));
        view.phase_filter = PhaseFilter::ShowComplete;
        assert!(doc.phase_hidden(&view).contains(&walls[0]));
        view.phase_filter = PhaseFilter::ShowAll;
        assert!(doc.phase_hidden(&view).is_empty());
        // In the Existing phase, only what stood then shows.
        view.phase = Some(existing);
        let hidden = doc.phase_hidden(&view);
        assert!(!hidden.contains(&walls[0]) && hidden.contains(&walls[1]));
        // Combining New Construction into Existing makes the wall temporary.
        assert!(doc.combine_phase(new));
        assert_eq!(doc.phases.len(), 1);
        assert_eq!(status_at(&doc, walls[0]), Some(PhaseStatus::Temporary));
    }

    fn status_at(doc: &Document, id: ElementId) -> Option<PhaseStatus> {
        doc.phase_status(doc.element(id).unwrap(), 0)
    }
}
