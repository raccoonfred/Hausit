//! Undo/redo. Every modifying command runs as a named transaction; the
//! document is snapshotted before it, which keeps undo exact and simple.

use crate::document::Document;

const MAX_UNDO: usize = 200;

#[derive(Default)]
pub struct History {
    undo: Vec<(String, Document)>,
    redo: Vec<(String, Document)>,
}

impl History {
    /// Records the state before a change called `name`.
    pub fn commit(&mut self, name: &str, before: Document) {
        self.undo.push((name.to_string(), before));
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, doc: &mut Document) -> Option<String> {
        let (name, prev) = self.undo.pop()?;
        let rev = doc.revision();
        let current = std::mem::replace(doc, prev);
        doc.continue_revision_from(rev);
        self.redo.push((name.clone(), current));
        Some(name)
    }

    pub fn redo(&mut self, doc: &mut Document) -> Option<String> {
        let (name, next) = self.redo.pop()?;
        let rev = doc.revision();
        let current = std::mem::replace(doc, next);
        doc.continue_revision_from(rev);
        self.undo.push((name.clone(), current));
        Some(name)
    }

    pub fn undo_name(&self) -> Option<&str> {
        self.undo.last().map(|(n, _)| n.as_str())
    }

    pub fn redo_name(&self) -> Option<&str> {
        self.redo.last().map(|(n, _)| n.as_str())
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }
}
