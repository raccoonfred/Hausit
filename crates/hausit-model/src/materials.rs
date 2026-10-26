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
    /// Batt insulation: a looped curve filling the layer.
    Insulation,
    /// Lines along the layer.
    Parallel,
    /// Rigid board insulation: crossed diagonals between the faces.
    RigidInsulation,
}

impl FillPattern {
    pub const ALL: [FillPattern; 8] = [
        FillPattern::None,
        FillPattern::Solid,
        FillPattern::Diagonal,
        FillPattern::Crosshatch,
        FillPattern::Concrete,
        FillPattern::Insulation,
        FillPattern::Parallel,
        FillPattern::RigidInsulation,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FillPattern::None => "<none>",
            FillPattern::Solid => "Solid fill",
            FillPattern::Diagonal => "Diagonal",
            FillPattern::Crosshatch => "Crosshatch",
            FillPattern::Concrete => "Concrete",
            FillPattern::Insulation => "Insulation - Batt",
            FillPattern::Parallel => "Parallel lines",
            FillPattern::RigidInsulation => "Insulation - Rigid",
        }
    }
}

/// Model pattern drawn on a material's surfaces in 3D and elevations, at
/// real size (brick courses, panel ribs).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfacePattern {
    #[default]
    None,
    /// 215 x 65 bricks in stretcher bond (75 mm courses).
    Brick,
    /// 440 x 215 blocks in stretcher bond (225 mm courses).
    Block,
    /// Vertical ribs at 333 mm, as on profiled metal cladding.
    Ribs,
    /// Tile courses at 250 mm, with joints at 300 mm.
    Tiles,
}

impl SurfacePattern {
    pub const ALL: [SurfacePattern; 5] = [Self::None, Self::Brick, Self::Block, Self::Ribs, Self::Tiles];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "<none>",
            Self::Brick => "Brick - Stretcher Bond",
            Self::Block => "Block - Stretcher Bond",
            Self::Ribs => "Profiled Sheet Ribs",
            Self::Tiles => "Roof Tiles",
        }
    }

    /// (course height, joint spacing, staggered); 0 means none.
    pub fn layout(self) -> (f64, f64, bool) {
        match self {
            Self::None => (0.0, 0.0, false),
            Self::Brick => (75.0, 225.0, true),
            Self::Block => (225.0, 450.0, true),
            Self::Ribs => (0.0, 333.0, false),
            Self::Tiles => (250.0, 300.0, true),
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
    #[serde(default)]
    pub surface_pattern: SurfacePattern,
}

/// Hausit's starter materials.
pub fn default_materials() -> Vec<MaterialDef> {
    let m = |name: &str, color: [u8; 3], cut_pattern: FillPattern| MaterialDef {
        name: name.into(),
        color,
        cut_pattern,
        // Solid cuts (steel, membranes) read darker than hatching.
        pattern_color: if cut_pattern == FillPattern::Solid {
            [50, 50, 50]
        } else {
            [96, 96, 96]
        },
        surface_pattern: SurfacePattern::None,
    };
    let surface = |mut m: MaterialDef, p: SurfacePattern| {
        m.surface_pattern = p;
        m
    };
    use FillPattern::*;
    vec![
        m("Default", [236, 236, 234], None),
        m("Concrete, Cast-in-Place", [190, 190, 186], Concrete),
        m("Concrete, Precast", [200, 200, 196], Concrete),
        surface(m("Masonry - Brick", [176, 98, 70], Diagonal), SurfacePattern::Brick),
        surface(m("Masonry - Concrete Block", [198, 196, 190], Crosshatch), SurfacePattern::Block),
        m("Insulation - Mineral Wool", [238, 222, 140], Insulation),
        m("Insulation - Rigid Board", [235, 235, 200], RigidInsulation),
        m("Air Gap", [245, 245, 245], None),
        m("Plaster", [240, 238, 232], None),
        m("Render", [232, 228, 218], None),
        m("Gypsum Board", [244, 244, 242], None),
        m("Ceiling Tile - Mineral Fibre", [246, 246, 240], None),
        m("Screed", [210, 208, 202], Concrete),
        m("Steel", [150, 156, 162], Solid),
        surface(m("Metal Panel - Steel", [170, 178, 186], Solid), SurfacePattern::Ribs),
        m("Timber - Softwood", [214, 178, 128], Parallel),
        m("Membrane", [60, 60, 60], Solid),
        surface(m("Roof Tile - Clay", [168, 84, 58], Diagonal), SurfacePattern::Tiles),
        // Finishes used by the family library.
        m("Wood - Oak", [190, 146, 98], Parallel),
        m("Laminate - White", [240, 240, 238], None),
        m("Fabric - Grey", [132, 138, 146], None),
        m("Fabric - White", [246, 246, 244], None),
        m("Porcelain", [250, 250, 248], None),
        m("Stainless Steel", [196, 200, 204], Solid),
        m("Glass", [180, 210, 225], None),
        m("Rubber", [46, 46, 46], Solid),
        m("Cardboard", [200, 164, 112], None),
        m("Steel - Painted Blue", [38, 88, 160], Solid),
        m("Steel - Painted Orange", [232, 118, 32], Solid),
        m("Steel - Painted Yellow", [240, 196, 30], Solid),
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
