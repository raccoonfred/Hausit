use crate::category::Category;
use serde::{Deserialize, Serialize};

/// A schedulable field. Which fields apply depends on the category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScheduleField {
    Family,
    Type,
    FamilyAndType,
    Mark,
    TypeMark,
    Comments,
    Level,
    Count,
    Length,
    Area,
    Volume,
    Perimeter,
    Width,
    Height,
    Thickness,
    SillHeight,
    HeadHeight,
    BaseConstraint,
    TopConstraint,
    UnconnectedHeight,
    Host,
    Slope,
    Structural,
    Name,
    Number,
    UnboundedHeight,
}

impl ScheduleField {
    pub fn label(self) -> &'static str {
        use ScheduleField::*;
        match self {
            Family => "Family",
            Type => "Type",
            FamilyAndType => "Family and Type",
            Mark => "Mark",
            TypeMark => "Type Mark",
            Comments => "Comments",
            Level => "Level",
            Count => "Count",
            Length => "Length",
            Area => "Area",
            Volume => "Volume",
            Perimeter => "Perimeter",
            Width => "Width",
            Height => "Height",
            Thickness => "Thickness",
            SillHeight => "Sill Height",
            HeadHeight => "Head Height",
            BaseConstraint => "Base Constraint",
            TopConstraint => "Top Constraint",
            UnconnectedHeight => "Unconnected Height",
            Host => "Host",
            Slope => "Slope",
            Structural => "Structural",
            Name => "Name",
            Number => "Number",
            UnboundedHeight => "Unbounded Height",
        }
    }

    /// Fields that can be summed for totals.
    pub fn is_summable(self) -> bool {
        matches!(
            self,
            ScheduleField::Count | ScheduleField::Length | ScheduleField::Area | ScheduleField::Volume | ScheduleField::Perimeter
        )
    }

    /// Fields available for a category, in the order of Revit's
    /// "Schedule Properties ▸ Fields" list.
    pub fn available(cat: Category) -> Vec<ScheduleField> {
        use ScheduleField::*;
        if cat == Category::Rooms {
            return vec![Number, Name, Level, Area, Perimeter, UnboundedHeight, Volume, Comments, Count];
        }
        let mut v = vec![Family, Type, FamilyAndType, TypeMark, Mark, Comments, Count];
        match cat {
            Category::Walls => v.extend([
                BaseConstraint,
                TopConstraint,
                UnconnectedHeight,
                Length,
                Area,
                Volume,
                Width,
                Structural,
            ]),
            Category::Floors => v.extend([Level, Area, Perimeter, Volume, Thickness, Structural]),
            Category::Roofs => v.extend([Level, Area, Slope, Thickness]),
            Category::Doors | Category::Windows => v.extend([Level, Width, Height, SillHeight, HeadHeight, Host]),
            Category::StructuralColumns => v.extend([Level, Length, Volume]),
            Category::StructuralFraming => v.extend([Level, Length, Volume]),
            Category::StructuralFoundations => v.extend([Level, Length, Volume]),
            Category::Stairs | Category::Railings => v.extend([Level, Length]),
            Category::Ceilings => v.extend([Level, Area, Perimeter]),
            Category::Furniture
            | Category::Casework
            | Category::PlumbingFixtures
            | Category::SpecialtyEquipment
            | Category::GenericModels => v.extend([Level]),
            Category::Levels | Category::Grids | Category::Rooms | Category::RoomSeparation | Category::Dimensions => return vec![],
        }
        v
    }

    /// Fields a new schedule starts with.
    pub fn defaults(cat: Category) -> Vec<ScheduleField> {
        use ScheduleField::*;
        match cat {
            Category::Walls => vec![Type, BaseConstraint, Length, Area, Volume],
            Category::Floors => vec![Type, Level, Area, Volume],
            Category::Roofs => vec![Type, Level, Area, Slope],
            Category::Doors | Category::Windows => vec![Mark, Type, Width, Height, Level, Count],
            Category::StructuralColumns => vec![Type, Level, Length, Count],
            Category::StructuralFraming => vec![Type, Level, Length, Count],
            Category::StructuralFoundations => vec![Type, Level, Volume, Count],
            Category::Stairs | Category::Railings => vec![Type, Level, Count],
            Category::Ceilings => vec![Type, Level, Area],
            Category::Furniture
            | Category::Casework
            | Category::PlumbingFixtures
            | Category::SpecialtyEquipment
            | Category::GenericModels => vec![Family, Type, Level, Count],
            Category::Rooms => vec![Number, Name, Level, Area, Perimeter, Volume],
            Category::Levels | Category::Grids | Category::RoomSeparation | Category::Dimensions => vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScheduleDef {
    pub category: Category,
    pub fields: Vec<ScheduleField>,
    pub sort_by: Option<ScheduleField>,
    /// When false, identical rows are grouped and counted (Revit's
    /// "Itemize every instance" unchecked).
    pub itemize: bool,
    pub grand_totals: bool,
}

impl ScheduleDef {
    pub fn new(category: Category) -> Self {
        ScheduleDef {
            category,
            fields: ScheduleField::defaults(category),
            sort_by: Some(match category {
                Category::Doors | Category::Windows => ScheduleField::Mark,
                Category::Rooms => ScheduleField::Number,
                _ => ScheduleField::Type,
            }),
            itemize: true,
            grand_totals: true,
        }
    }
}
