//! Core types shared by every Hausit crate: element ids, 2D/3D math helpers
//! and length units.
//!
//! Internal length unit is the **millimetre**. (Revit stores decimal feet
//! internally; the RVT importer will convert at its boundary.)

pub mod geom;
pub mod id;
pub mod skeleton;
pub mod units;

pub use geom::{Aabb, Plane};
pub use glam::{DVec2, DVec3};
pub use id::ElementId;
pub use units::{format_length, parse_length, LengthUnit};

/// Distance below which two points are considered coincident (mm).
pub const EPS: f64 = 1e-6;
/// Tolerance used to decide that two wall ends meet (mm).
pub const JOIN_TOLERANCE: f64 = 1.0;
