//! The Hausit building information model.
//!
//! Mirrors Revit's object model in a simplified form:
//!
//! * a [`Document`] owns every element, family type, view and schedule,
//! * [`Element`]s are levels, grids, walls, floors, roofs, doors and windows,
//! * every element has a [`FamilyType`] (except datums) whose parameters are
//!   shared by all its instances,
//! * instance and type parameters are exposed generically through
//!   [`params`] so the UI can show a Revit-style Properties palette,
//! * [`geometry`] turns elements into solids that views project and cut.

pub mod category;
pub mod dimensions;
pub mod document;
pub mod element;
pub mod family;
pub mod geometry;
pub mod history;
pub mod index;
pub mod materials;
pub mod params;
pub mod rooms;
pub mod schedule;
pub mod stairs;
pub mod styles;
pub mod template;
pub mod view;

pub use category::Category;
pub use document::{Document, ModelError, ProjectInfo};
pub use element::*;
pub use family::{FamilyType, TypeParams};
pub use history::History;
pub use params::{ParamGroup, ParamKind, ParamValue, Parameter};
pub use schedule::{ScheduleDef, ScheduleField};
pub use view::{CardinalDirection, DetailLevel, SectionBox, View, ViewKind, VisualStyle};

pub use hausit_core::{DVec2, DVec3, ElementId};
