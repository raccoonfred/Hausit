use serde::{Deserialize, Serialize};

/// Built-in categories, matching the names Revit uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Category {
    Levels,
    Grids,
    Walls,
    Floors,
    Roofs,
    Doors,
    Windows,
    StructuralColumns,
    StructuralFraming,
    StructuralFoundations,
    Stairs,
    Railings,
    Rooms,
    RoomSeparation,
    Dimensions,
}

impl Category {
    pub const ALL: [Category; 15] = [
        Category::Levels,
        Category::Grids,
        Category::Walls,
        Category::Floors,
        Category::Roofs,
        Category::Doors,
        Category::Windows,
        Category::StructuralColumns,
        Category::StructuralFraming,
        Category::StructuralFoundations,
        Category::Stairs,
        Category::Railings,
        Category::Rooms,
        Category::RoomSeparation,
        Category::Dimensions,
    ];

    /// Categories that have family types (and so appear under "Families").
    pub const TYPED: [Category; 10] = [
        Category::Doors,
        Category::Floors,
        Category::Railings,
        Category::Roofs,
        Category::StructuralColumns,
        Category::StructuralFoundations,
        Category::StructuralFraming,
        Category::Stairs,
        Category::Walls,
        Category::Windows,
    ];

    /// Categories that can be scheduled.
    pub const SCHEDULABLE: [Category; 11] = [
        Category::Doors,
        Category::Floors,
        Category::Railings,
        Category::Roofs,
        Category::Rooms,
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
            Category::Doors => "Doors",
            Category::Windows => "Windows",
            Category::StructuralColumns => "Structural Columns",
            Category::StructuralFraming => "Structural Framing",
            Category::StructuralFoundations => "Structural Foundations",
            Category::Stairs => "Stairs",
            Category::Railings => "Railings",
            Category::Rooms => "Rooms",
            Category::RoomSeparation => "Room Separation Lines",
            Category::Dimensions => "Dimensions",
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
            Category::Doors => "Door",
            Category::Windows => "Window",
            Category::StructuralColumns => "Structural Column",
            Category::StructuralFraming => "Structural Framing",
            Category::StructuralFoundations => "Structural Foundation",
            Category::Stairs => "Stair",
            Category::Railings => "Railing",
            Category::Rooms => "Room",
            Category::RoomSeparation => "Room Separation Line",
            Category::Dimensions => "Dimension",
        }
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
            Category::Levels | Category::Grids | Category::Rooms | Category::RoomSeparation | Category::Dimensions
        )
    }
}
