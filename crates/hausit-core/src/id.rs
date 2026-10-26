use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable identifier of an element inside a document, like Revit's ElementId.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ElementId(pub u64);

impl ElementId {
    pub const INVALID: ElementId = ElementId(0);

    pub fn is_valid(self) -> bool {
        self.0 != 0
    }
}

impl fmt::Display for ElementId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
