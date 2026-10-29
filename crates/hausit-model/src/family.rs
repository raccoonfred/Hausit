use crate::category::Category;
use crate::params::ParamValue;
use hausit_core::ElementId;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Parameters stored on a type, by name, in display order.
pub type TypeParams = IndexMap<String, ParamValue>;

/// A family type, e.g. `Basic Wall : Generic - 200mm` or
/// `Door - Single Flush : 0926 x 2040mm`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FamilyType {
    pub id: ElementId,
    pub category: Category,
    pub family: String,
    pub name: String,
    pub params: TypeParams,
    /// Compound walls, floors and roofs: the layers, exterior (or top)
    /// first. Empty for a plain type with one unnamed material.
    #[serde(default)]
    pub layers: Vec<crate::materials::Layer>,
}

impl FamilyType {
    pub fn length(&self, name: &str) -> Option<f64> {
        match self.params.get(name)? {
            ParamValue::Length(v) => Some(*v),
            _ => None,
        }
    }

    pub fn length_or(&self, name: &str, default: f64) -> f64 {
        self.length(name).unwrap_or(default)
    }

    pub fn display_name(&self) -> String {
        format!("{} : {}", self.family, self.name)
    }

    /// Thickness of walls/floors/roofs ("Width" for walls, as in Revit).
    pub fn thickness(&self) -> f64 {
        match self.category {
            Category::Walls => self.length_or("Width", 200.0),
            Category::Doors | Category::Windows => self.length_or("Thickness", 50.0),
            _ => self.length_or("Thickness", 200.0),
        }
    }

    /// Sets the layers and keeps the type's Width/Thickness equal to
    /// their total.
    pub fn set_layers(&mut self, layers: Vec<crate::materials::Layer>) {
        let total: f64 = layers.iter().map(|l| l.thickness).sum();
        if total > 0.0 {
            let key = if self.category == Category::Walls { "Width" } else { "Thickness" };
            self.params.insert(key.into(), ParamValue::Length(total));
        }
        self.layers = layers;
    }

    /// Wall foundation types (a system family) as opposed to isolated
    /// foundations, both in Structural Foundations.
    pub fn is_wall_foundation(&self) -> bool {
        self.category == Category::StructuralFoundations && self.family == WALL_FOUNDATION
    }

    /// Whether the type has a layer structure to edit.
    pub fn has_structure(&self) -> bool {
        matches!(
            self.category,
            Category::Walls | Category::Floors | Category::Roofs | Category::Ceilings
        )
    }

    pub fn width(&self) -> f64 {
        self.length_or("Width", 900.0)
    }

    pub fn height(&self) -> f64 {
        self.length_or("Height", 2100.0)
    }
}

/// Family name of wall (strip) foundation types.
pub const WALL_FOUNDATION: &str = "Wall Foundation";

/// Parameters a newly created type of each category starts with.
pub fn default_type_params(category: Category) -> TypeParams {
    let mut p = TypeParams::new();
    let len = |v| ParamValue::Length(v);
    match category {
        Category::Walls => {
            p.insert("Width".into(), len(200.0));
            p.insert("Function".into(), ParamValue::Text("Exterior".into()));
        }
        Category::Floors | Category::Roofs => {
            p.insert("Thickness".into(), len(200.0));
        }
        Category::Doors => {
            p.insert("Width".into(), len(900.0));
            p.insert("Height".into(), len(2100.0));
            p.insert("Thickness".into(), len(50.0));
            p.insert("Leaves".into(), ParamValue::Integer(1));
        }
        Category::Windows => {
            p.insert("Width".into(), len(900.0));
            p.insert("Height".into(), len(1200.0));
            p.insert("Default Sill Height".into(), len(900.0));
            p.insert("Thickness".into(), len(50.0));
        }
        Category::StructuralColumns => {
            p.insert("b".into(), len(300.0));
            p.insert("h".into(), len(300.0));
            p.insert("Structural Material".into(), ParamValue::Text("Concrete, Cast-in-Place".into()));
        }
        Category::StructuralFraming => {
            p.insert("Section Shape".into(), ParamValue::Text("I".into()));
            p.insert("d".into(), len(300.0));
            p.insert("bf".into(), len(150.0));
            p.insert("tw".into(), len(7.0));
            p.insert("tf".into(), len(11.0));
            p.insert("Structural Material".into(), ParamValue::Text("Steel".into()));
        }
        Category::StructuralFoundations => {
            p.insert("Width".into(), len(1200.0));
            p.insert("Length".into(), len(1200.0));
            p.insert("Thickness".into(), len(500.0));
            p.insert("Structural Material".into(), ParamValue::Text("Concrete, Cast-in-Place".into()));
        }
        Category::Ceilings => {
            p.insert("Thickness".into(), len(25.0));
            p.insert("Grid Spacing".into(), len(600.0));
        }
        Category::Stairs => {
            p.insert("Maximum Riser Height".into(), len(180.0));
            p.insert("Minimum Tread Depth".into(), len(280.0));
            p.insert("Minimum Run Width".into(), len(1000.0));
            p.insert("Structural Depth".into(), len(150.0));
            p.insert("Landing Thickness".into(), len(200.0));
            p.insert("Monolithic".into(), ParamValue::Bool(true));
            p.insert("Nosing Length".into(), len(25.0));
            p.insert("Closed Risers".into(), ParamValue::Bool(false));
            p.insert("Riser Thickness".into(), len(20.0));
            p.insert("Stringer Width".into(), len(50.0));
            p.insert("Stringer Depth".into(), len(250.0));
            p.insert("Material".into(), ParamValue::Text("Concrete, Cast-in-Place".into()));
        }
        Category::Railings => {
            p.insert("Railing Height".into(), len(1000.0));
            p.insert("Baluster Spacing".into(), len(120.0));
            p.insert("Baluster Size".into(), len(25.0));
            p.insert("Rail Size".into(), len(50.0));
            p.insert("Intermediate Rails".into(), ParamValue::Integer(0));
            p.insert("Material".into(), ParamValue::Text("Steel".into()));
        }
        // Loadable families bring their own parameters.
        Category::Furniture | Category::Casework | Category::PlumbingFixtures | Category::SpecialtyEquipment | Category::GenericModels => {}
        Category::Levels
        | Category::Grids
        | Category::Rooms
        | Category::RoomSeparation
        | Category::Dimensions
        | Category::TextNotes
        | Category::Tags => {}
    }
    p.insert("Type Mark".into(), ParamValue::Text(String::new()));
    p.insert("Description".into(), ParamValue::Text(String::new()));
    p
}
