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
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::Levels,
        Category::Grids,
        Category::Walls,
        Category::Floors,
        Category::Roofs,
        Category::Doors,
        Category::Windows,
    ];

    /// Categories that have family types (and so appear under "Families").
    pub const TYPED: [Category; 5] = [
        Category::Doors,
        Category::Floors,
        Category::Roofs,
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
        }
    }

    /// Hosted categories are placed in a wall.
    pub fn is_hosted(self) -> bool {
        matches!(self, Category::Doors | Category::Windows)
    }
}
