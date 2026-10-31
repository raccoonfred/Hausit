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

pub mod annotation;
pub mod category;
pub mod component;
pub mod curtain;
pub mod detail;
pub mod dimensions;
pub mod document;
pub mod editing;
pub mod element;
pub mod family;
pub mod geometry;
pub mod history;
pub mod index;
pub mod library;
pub mod materials;
pub mod params;
pub mod phasing;
pub mod rooms;
pub mod schedule;
pub mod sheet;
pub mod stairs;
pub mod styles;
pub mod template;
pub mod view;

pub use category::Category;
pub use component::FamilyDef;
pub use curtain::{CurtainGrids, CurtainLayout, CURTAIN_WALL};
pub use detail::{DetailLine, FilledRegion, LineStyle, SpotElevation};
pub use document::{Document, ModelError, ProjectInfo};
pub use editing::RefLine;
pub use element::*;
pub use family::{FamilyType, TypeParams};
pub use history::History;
pub use params::{ParamGroup, ParamKind, ParamValue, Parameter};
pub use phasing::{Phase, PhaseDisplay, PhaseFilter, PhaseStatus};
pub use schedule::{ScheduleDef, ScheduleField};
pub use sheet::{SheetDef, SheetSize, Viewport};
pub use view::{
    CardinalDirection, CategoryGraphics, DetailLevel, Graphics, SectionBox, TempHide, View, ViewKind, ViewRange, ViewTemplate, VisualStyle,
};

pub use hausit_core::{DVec2, DVec3, ElementId};
