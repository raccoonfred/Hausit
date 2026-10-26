//! Materials (Manage ▸ Materials) and the layers of compound wall, floor
//! and roof types (Type Properties ▸ Structure ▸ Edit).

use serde::{Deserialize, Serialize};

/// How a material's cut surface is drawn (its "cut pattern").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillPattern {
    #[default]
    None,
    Solid,
    /// 45° lines.
    Diagonal,
    /// 45° lines both ways.
    Crosshatch,
    /// Scattered dots and small triangles.
    Concrete,
    /// Zigzag batt symbol, along the layer.
    Insulation,
    /// Lines along the layer.
    Parallel,
}

impl FillPattern {
    pub const ALL: [FillPattern; 7] = [
        FillPattern::None,
        FillPattern::Solid,
        FillPattern::Diagonal,
        FillPattern::Crosshatch,
        FillPattern::Concrete,
        FillPattern::Insulation,
        FillPattern::Parallel,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FillPattern::None => "<none>",
            FillPattern::Solid => "Solid fill",
            FillPattern::Diagonal => "Diagonal",
            FillPattern::Crosshatch => "Crosshatch",
            FillPattern::Concrete => "Concrete",
            FillPattern::Insulation => "Insulation",
            FillPattern::Parallel => "Parallel lines",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaterialDef {
    pub name: String,
    /// Shading colour in 3D.
    pub color: [u8; 3],
    pub cut_pattern: FillPattern,
    /// Colour of the cut pattern.
    #[serde(default)]
    pub pattern_color: [u8; 3],
}

/// Hausit's starter materials.
pub fn default_materials() -> Vec<MaterialDef> {
    let m = |name: &str, color: [u8; 3], cut_pattern: FillPattern| MaterialDef {
        name: name.into(),
        color,
        cut_pattern,
        pattern_color: [96, 96, 96],
    };
    use FillPattern::*;
    vec![
        m("Default", [236, 236, 234], None),
        m("Concrete, Cast-in-Place", [190, 190, 186], Concrete),
        m("Concrete, Precast", [200, 200, 196], Concrete),
        m("Masonry - Brick", [176, 98, 70], Diagonal),
        m("Masonry - Concrete Block", [198, 196, 190], Crosshatch),
        m("Insulation - Mineral Wool", [238, 222, 140], Insulation),
        m("Insulation - Rigid Board", [235, 235, 200], Insulation),
        m("Air Gap", [245, 245, 245], None),
        m("Plaster", [240, 238, 232], None),
        m("Render", [232, 228, 218], None),
        m("Gypsum Board", [244, 244, 242], None),
        m("Screed", [210, 208, 202], Concrete),
        m("Steel", [150, 156, 162], Solid),
        m("Metal Panel - Steel", [170, 178, 186], Solid),
        m("Timber - Softwood", [214, 178, 128], Parallel),
        m("Membrane", [60, 60, 60], Solid),
    ]
}

/// What a layer does, as in Revit's priorities: structure first, then
/// substrate, thermal/air, membrane and the finishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerFunction {
    Structure,
    Substrate,
    Thermal,
    Membrane,
    Finish1,
    Finish2,
}

impl LayerFunction {
    pub const ALL: [LayerFunction; 6] = [
        LayerFunction::Structure,
        LayerFunction::Substrate,
        LayerFunction::Thermal,
        LayerFunction::Membrane,
        LayerFunction::Finish1,
        LayerFunction::Finish2,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LayerFunction::Structure => "Structure [1]",
            LayerFunction::Substrate => "Substrate [2]",
            LayerFunction::Thermal => "Thermal/Air Layer [3]",
            LayerFunction::Membrane => "Membrane Layer",
            LayerFunction::Finish1 => "Finish 1 [4]",
            LayerFunction::Finish2 => "Finish 2 [5]",
        }
    }
}

/// One layer of a compound type. Walls list layers from the exterior
/// side; floors and roofs from the top.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub function: LayerFunction,
    pub material: String,
    pub thickness: f64,
}

impl Layer {
    pub fn new(function: LayerFunction, material: &str, thickness: f64) -> Self {
        Layer {
            function,
            material: material.into(),
            thickness,
        }
    }
}
