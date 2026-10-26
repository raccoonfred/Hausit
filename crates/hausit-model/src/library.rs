//! Hausit's starter family library: furniture, casework, plumbing
//! fixtures, industrial equipment and site items, authored for Hausit.
//! The `.hfam` sources are in `crates/hausit-model/library/`.

use crate::component::FamilyDef;

/// Every library family, as (path, `.hfam` text).
pub const FILES: &[(&str, &str)] = &[
    ("Casework/Base Cabinet.hfam", include_str!("../library/Casework/Base Cabinet.hfam")),
    ("Casework/Sink Unit.hfam", include_str!("../library/Casework/Sink Unit.hfam")),
    ("Casework/Wall Cabinet.hfam", include_str!("../library/Casework/Wall Cabinet.hfam")),
    ("Furniture/Bed.hfam", include_str!("../library/Furniture/Bed.hfam")),
    ("Furniture/Chair.hfam", include_str!("../library/Furniture/Chair.hfam")),
    ("Furniture/Desk.hfam", include_str!("../library/Furniture/Desk.hfam")),
    (
        "Furniture/Shelving Unit.hfam",
        include_str!("../library/Furniture/Shelving Unit.hfam"),
    ),
    ("Furniture/Sofa.hfam", include_str!("../library/Furniture/Sofa.hfam")),
    (
        "Furniture/Table - Rectangular.hfam",
        include_str!("../library/Furniture/Table - Rectangular.hfam"),
    ),
    ("Furniture/Wardrobe.hfam", include_str!("../library/Furniture/Wardrobe.hfam")),
    (
        "Generic Models/Bollard.hfam",
        include_str!("../library/Generic Models/Bollard.hfam"),
    ),
    (
        "Generic Models/Equipment Plinth.hfam",
        include_str!("../library/Generic Models/Equipment Plinth.hfam"),
    ),
    (
        "Plumbing Fixtures/Bath.hfam",
        include_str!("../library/Plumbing Fixtures/Bath.hfam"),
    ),
    (
        "Plumbing Fixtures/Shower Tray.hfam",
        include_str!("../library/Plumbing Fixtures/Shower Tray.hfam"),
    ),
    (
        "Plumbing Fixtures/WC - Close Coupled.hfam",
        include_str!("../library/Plumbing Fixtures/WC - Close Coupled.hfam"),
    ),
    (
        "Plumbing Fixtures/Washbasin - Pedestal.hfam",
        include_str!("../library/Plumbing Fixtures/Washbasin - Pedestal.hfam"),
    ),
    (
        "Specialty Equipment/Pallet Racking.hfam",
        include_str!("../library/Specialty Equipment/Pallet Racking.hfam"),
    ),
    (
        "Specialty Equipment/Pallet.hfam",
        include_str!("../library/Specialty Equipment/Pallet.hfam"),
    ),
    (
        "Specialty Equipment/Workbench.hfam",
        include_str!("../library/Specialty Equipment/Workbench.hfam"),
    ),
];

/// The library's families, parsed.
pub fn families() -> Vec<FamilyDef> {
    FILES
        .iter()
        .map(|(path, text)| FamilyDef::from_json(text).unwrap_or_else(|e| panic!("library family {path}: {e}")))
        .collect()
}

/// A library family by name.
pub fn family(name: &str) -> Option<FamilyDef> {
    FILES
        .iter()
        .filter(|(path, _)| path.ends_with(&format!("/{name}.hfam")))
        .find_map(|(_, text)| FamilyDef::from_json(text).ok())
}
