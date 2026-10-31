use serde::{Deserialize, Serialize};

/// Built-in categories, matching the names Revit uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Category {
    Levels,
    Grids,
    Walls,
    Floors,
    Roofs,
    Ceilings,
    Doors,
    Windows,
    StructuralColumns,
    StructuralFraming,
    StructuralFoundations,
    Stairs,
    Railings,
    Furniture,
    Casework,
    PlumbingFixtures,
    SpecialtyEquipment,
    GenericModels,
    Rooms,
    RoomSeparation,
    Dimensions,
    TextNotes,
    Tags,
    /// Detail lines.
    Lines,
    /// Filled regions.
    DetailItems,
    SpotElevations,
    ModelGroups,
    Areas,
    AreaBoundary,
    RevisionClouds,
    Toposolid,
    PropertyLines,
    Planting,
}

impl Category {
    pub const ALL: [Category; 33] = [
        Category::Levels,
        Category::Grids,
        Category::Walls,
        Category::Floors,
        Category::Roofs,
        Category::Ceilings,
        Category::Doors,
        Category::Windows,
        Category::StructuralColumns,
        Category::StructuralFraming,
        Category::StructuralFoundations,
        Category::Stairs,
        Category::Railings,
        Category::Furniture,
        Category::Casework,
        Category::PlumbingFixtures,
        Category::SpecialtyEquipment,
        Category::GenericModels,
        Category::Rooms,
        Category::RoomSeparation,
        Category::Dimensions,
        Category::TextNotes,
        Category::Tags,
        Category::Lines,
        Category::DetailItems,
        Category::SpotElevations,
        Category::ModelGroups,
        Category::Areas,
        Category::AreaBoundary,
        Category::RevisionClouds,
        Category::Toposolid,
        Category::PropertyLines,
        Category::Planting,
    ];

    /// Categories of loadable components, placed with the Component tool.
    pub const COMPONENTS: [Category; 6] = [
        Category::Furniture,
        Category::Casework,
        Category::PlumbingFixtures,
        Category::SpecialtyEquipment,
        Category::GenericModels,
        Category::Planting,
    ];

    /// Categories that have family types (and so appear under "Families").
    pub const TYPED: [Category; 18] = [
        Category::Casework,
        Category::Ceilings,
        Category::Doors,
        Category::Floors,
        Category::Furniture,
        Category::GenericModels,
        Category::Planting,
        Category::PlumbingFixtures,
        Category::Railings,
        Category::Roofs,
        Category::SpecialtyEquipment,
        Category::StructuralColumns,
        Category::StructuralFoundations,
        Category::StructuralFraming,
        Category::Stairs,
        Category::Toposolid,
        Category::Walls,
        Category::Windows,
    ];

    /// Categories that can be scheduled.
    pub const SCHEDULABLE: [Category; 17] = [
        Category::Casework,
        Category::Ceilings,
        Category::Doors,
        Category::Floors,
        Category::Furniture,
        Category::GenericModels,
        Category::PlumbingFixtures,
        Category::Railings,
        Category::Roofs,
        Category::Rooms,
        Category::SpecialtyEquipment,
        Category::StructuralColumns,
        Category::StructuralFoundations,
        Category::StructuralFraming,
        Category::Stairs,
        Category::Walls,
        Category::Windows,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Category::Levels => "Levels",
            Category::Grids => "Grids",
            Category::Walls => "Walls",
            Category::Floors => "Floors",
            Category::Roofs => "Roofs",
            Category::Ceilings => "Ceilings",
            Category::Doors => "Doors",
            Category::Windows => "Windows",
            Category::StructuralColumns => "Structural Columns",
            Category::StructuralFraming => "Structural Framing",
            Category::StructuralFoundations => "Structural Foundations",
            Category::Stairs => "Stairs",
            Category::Railings => "Railings",
            Category::Furniture => "Furniture",
            Category::Casework => "Casework",
            Category::PlumbingFixtures => "Plumbing Fixtures",
            Category::SpecialtyEquipment => "Specialty Equipment",
            Category::GenericModels => "Generic Models",
            Category::Rooms => "Rooms",
            Category::RoomSeparation => "Room Separation Lines",
            Category::Dimensions => "Dimensions",
            Category::TextNotes => "Text Notes",
            Category::Tags => "Tags",
            Category::Lines => "Lines",
            Category::DetailItems => "Detail Items",
            Category::SpotElevations => "Spot Elevations",
            Category::ModelGroups => "Model Groups",
            Category::Areas => "Areas",
            Category::AreaBoundary => "Area Boundary",
            Category::RevisionClouds => "Revision Clouds",
            Category::Toposolid => "Toposolid",
            Category::PropertyLines => "Property Lines",
            Category::Planting => "Planting",
        }
    }

    /// Singular form, used in prompts ("Click to place Door").
    pub fn singular(self) -> &'static str {
        match self {
            Category::Levels => "Level",
            Category::Grids => "Grid",
            Category::Walls => "Wall",
            Category::Floors => "Floor",
            Category::Roofs => "Roof",
            Category::Ceilings => "Ceiling",
            Category::Doors => "Door",
            Category::Windows => "Window",
            Category::StructuralColumns => "Structural Column",
            Category::StructuralFraming => "Structural Framing",
            Category::StructuralFoundations => "Structural Foundation",
            Category::Stairs => "Stair",
            Category::Railings => "Railing",
            Category::Furniture => "Furniture",
            Category::Casework => "Casework",
            Category::PlumbingFixtures => "Plumbing Fixture",
            Category::SpecialtyEquipment => "Specialty Equipment",
            Category::GenericModels => "Generic Model",
            Category::Rooms => "Room",
            Category::RoomSeparation => "Room Separation Line",
            Category::Dimensions => "Dimension",
            Category::TextNotes => "Text Note",
            Category::Tags => "Tag",
            Category::Lines => "Detail Line",
            Category::DetailItems => "Filled Region",
            Category::SpotElevations => "Spot Elevation",
            Category::ModelGroups => "Model Group",
            Category::Areas => "Area",
            Category::AreaBoundary => "Area Boundary Line",
            Category::RevisionClouds => "Revision Cloud",
            Category::Toposolid => "Toposolid",
            Category::PropertyLines => "Property Line",
            Category::Planting => "Planting",
        }
    }

    /// View-specific annotation: drawn only in the view it was placed in.
    pub fn is_annotation(self) -> bool {
        matches!(
            self,
            Category::Dimensions
                | Category::TextNotes
                | Category::Tags
                | Category::Lines
                | Category::DetailItems
                | Category::SpotElevations
                | Category::RevisionClouds
        )
    }

    /// Hosted categories are placed in a wall.
    pub fn is_hosted(self) -> bool {
        matches!(self, Category::Doors | Category::Windows)
    }
}

impl Category {
    /// Annotation and spatial categories have no 3D geometry.
    pub fn is_model(self) -> bool {
        !matches!(
            self,
            Category::Levels
                | Category::Grids
                | Category::Rooms
                | Category::RoomSeparation
                | Category::Dimensions
                | Category::TextNotes
                | Category::Tags
                | Category::Lines
                | Category::DetailItems
                | Category::SpotElevations
                | Category::RevisionClouds
                | Category::Areas
                | Category::AreaBoundary
                | Category::PropertyLines
        )
    }
}
