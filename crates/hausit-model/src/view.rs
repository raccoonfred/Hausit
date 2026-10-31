//! View definitions. Views are elements of the document, as in Revit; the
//! graphics they show are produced by the `hausit-views` crate.

use crate::category::Category;
use crate::schedule::ScheduleDef;
use crate::sheet::SheetDef;
use hausit_core::{DVec2, DVec3, ElementId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CardinalDirection {
    North,
    South,
    East,
    West,
}

impl CardinalDirection {
    pub const ALL: [CardinalDirection; 4] = [Self::North, Self::South, Self::East, Self::West];

    pub fn label(self) -> &'static str {
        match self {
            Self::North => "North",
            Self::South => "South",
            Self::East => "East",
            Self::West => "West",
        }
    }

    /// Unit vector pointing from the model towards this side (+Y is north).
    pub fn outward(self) -> DVec2 {
        match self {
            Self::North => DVec2::Y,
            Self::South => -DVec2::Y,
            Self::East => DVec2::X,
            Self::West => -DVec2::X,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetailLevel {
    Coarse,
    #[default]
    Medium,
    Fine,
}

impl DetailLevel {
    pub const ALL: [DetailLevel; 3] = [Self::Coarse, Self::Medium, Self::Fine];
    pub fn label(self) -> &'static str {
        match self {
            Self::Coarse => "Coarse",
            Self::Medium => "Medium",
            Self::Fine => "Fine",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisualStyle {
    Wireframe,
    #[default]
    HiddenLine,
    Shaded,
    ConsistentColors,
}

impl VisualStyle {
    pub const ALL: [VisualStyle; 4] = [Self::Wireframe, Self::HiddenLine, Self::Shaded, Self::ConsistentColors];
    pub fn label(self) -> &'static str {
        match self {
            Self::Wireframe => "Wireframe",
            Self::HiddenLine => "Hidden Line",
            Self::Shaded => "Shaded",
            Self::ConsistentColors => "Consistent Colors",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ViewKind {
    FloorPlan {
        level: ElementId,
    },
    /// Reflected ceiling plan: cut higher, showing ceilings above.
    CeilingPlan {
        level: ElementId,
    },
    ThreeD,
    Elevation {
        direction: CardinalDirection,
    },
    /// Section line drawn in plan; the view looks to the right of start→end.
    Section {
        start: DVec2,
        end: DVec2,
        far_clip: f64,
    },
    Schedule(ScheduleDef),
    /// A drawing sheet; the view's name is the sheet name.
    Sheet(SheetDef),
}

impl ViewKind {
    /// Heading used in the Project Browser.
    pub fn browser_group(&self) -> &'static str {
        match self {
            ViewKind::FloorPlan { .. } => "Floor Plans",
            ViewKind::CeilingPlan { .. } => "Ceiling Plans",
            ViewKind::ThreeD => "3D Views",
            ViewKind::Elevation { .. } => "Elevations (Building Elevation)",
            ViewKind::Section { .. } => "Sections (Building Section)",
            ViewKind::Schedule(_) => "Schedules/Quantities",
            ViewKind::Sheet(_) => "Sheets (all)",
        }
    }

    /// The level of a floor or ceiling plan.
    pub fn plan_level(&self) -> Option<ElementId> {
        match self {
            ViewKind::FloorPlan { level } | ViewKind::CeilingPlan { level } => Some(*level),
            _ => None,
        }
    }

    pub fn is_graphical(&self) -> bool {
        !matches!(self, ViewKind::Schedule(_))
    }
}

/// Orbit camera of a 3D view (orthographic, like Revit's default {3D}).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrbitCamera {
    /// Rotation about Z, degrees. 0 looks towards +Y (north).
    pub yaw: f64,
    /// Elevation angle above the horizon, degrees.
    pub pitch: f64,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        // Revit's default 3D view looks from the south-east, from above.
        OrbitCamera { yaw: -45.0, pitch: 30.0 }
    }
}

impl OrbitCamera {
    /// (right, up, forward) unit vectors. `forward` points into the screen.
    pub fn basis(&self) -> (DVec3, DVec3, DVec3) {
        let (y, p) = (self.yaw.to_radians(), self.pitch.to_radians());
        let forward = DVec3::new(y.sin() * p.cos(), y.cos() * p.cos(), -p.sin());
        let right = DVec3::new(y.cos(), -y.sin(), 0.0);
        let up = right.cross(forward);
        (right, up, forward)
    }
}

/// A 3D view's section box: the model is cut away outside it. The box is
/// axis-aligned in a frame turned `angle` radians about Z, so it can line
/// up with a section; `min`/`max` are in that frame.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SectionBox {
    pub min: DVec3,
    pub max: DVec3,
    pub angle: f64,
}

impl SectionBox {
    /// The box's x and y axes in model coordinates.
    pub fn axes(&self) -> (DVec2, DVec2) {
        let x = DVec2::from_angle(self.angle);
        (x, DVec2::new(-x.y, x.x))
    }

    pub fn to_local(&self, p: DVec3) -> DVec3 {
        let (x, y) = self.axes();
        let q = DVec2::new(p.x, p.y);
        DVec3::new(q.dot(x), q.dot(y), p.z)
    }

    pub fn to_model(&self, p: DVec3) -> DVec3 {
        let (x, y) = self.axes();
        let q = x * p.x + y * p.y;
        DVec3::new(q.x, q.y, p.z)
    }

    /// The six faces as (point, outward normal) in model coordinates.
    pub fn planes(&self) -> [(DVec3, DVec3); 6] {
        let (x, y) = self.axes();
        let (x, y) = (DVec3::new(x.x, x.y, 0.0), DVec3::new(y.x, y.y, 0.0));
        let (lo, hi) = (self.to_model(self.min), self.to_model(self.max));
        [(lo, -x), (hi, x), (lo, -y), (hi, y), (lo, -DVec3::Z), (hi, DVec3::Z)]
    }

    /// A box around model-coordinate bounds, in the model's own axes.
    pub fn around(min: DVec3, max: DVec3) -> SectionBox {
        SectionBox { min, max, angle: 0.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub id: ElementId,
    pub name: String,
    pub kind: ViewKind,
    /// Scale denominator: 100 means 1 : 100.
    pub scale: u32,
    pub detail_level: DetailLevel,
    pub visual_style: VisualStyle,
    /// Plan cut plane height above the level (View Range ▸ Cut Plane).
    pub cut_height: f64,
    pub camera: OrbitCamera,
    /// 3D views only: cut the model to this box.
    #[serde(default)]
    pub section_box: Option<SectionBox>,
    /// Crop region in view coordinates (min, max): only what is inside
    /// is shown. Callouts are cropped views.
    #[serde(default)]
    pub crop: Option<(DVec2, DVec2)>,
    /// For a callout: the view it was drawn in, which shows its boundary.
    #[serde(default)]
    pub callout_of: Option<ElementId>,
    /// Visibility/Graphics overrides by category; categories not listed
    /// show as their Object Styles say.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub graphics: Graphics,
    /// The view template that controls this view's settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<ElementId>,
    /// Phasing: the phase the view shows (`None`: the latest) and how.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<ElementId>,
    #[serde(default)]
    pub phase_filter: crate::phasing::PhaseFilter,
    /// Hide in View ▸ Elements (EH).
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub hidden_elements: std::collections::BTreeSet<ElementId>,
    /// Temporary Hide/Isolate: not saved, as in Revit.
    #[serde(skip)]
    pub temp_hide: Option<TempHide>,
}

/// Temporary Hide/Isolate (HH, HI, HC, IC): what is hidden, or isolated,
/// until Reset (HR).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TempHide {
    /// Isolate: everything else is hidden.
    pub isolate: bool,
    pub elements: Vec<ElementId>,
    pub categories: Vec<Category>,
}

/// Visibility/Graphics overrides of a view, by category.
pub type Graphics = BTreeMap<Category, CategoryGraphics>;

/// How a view shows one category (Revit's Visibility/Graphics dialog):
/// whether it is visible, halftone, and its line colour and weight in
/// projection and cut, overriding Object Styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryGraphics {
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub halftone: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection_color: Option<[u8; 3]>,
    /// Line weight number, 1 to 16.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection_weight: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_color: Option<[u8; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_weight: Option<u8>,
}

fn yes() -> bool {
    true
}

impl Default for CategoryGraphics {
    fn default() -> Self {
        CategoryGraphics {
            visible: true,
            halftone: false,
            projection_color: None,
            projection_weight: None,
            cut_color: None,
            cut_weight: None,
        }
    }
}

impl CategoryGraphics {
    pub fn is_default(&self) -> bool {
        *self == CategoryGraphics::default()
    }
}

/// A view template: settings applied to the views that use it. Each
/// setting is `None` when the template does not include it, and the view
/// then keeps its own, as Revit's "Include" checkboxes do.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewTemplate {
    pub id: ElementId,
    pub name: String,
    #[serde(default)]
    pub scale: Option<u32>,
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
    #[serde(default)]
    pub visual_style: Option<VisualStyle>,
    /// Plans only: View Range cut plane.
    #[serde(default)]
    pub cut_height: Option<f64>,
    #[serde(default)]
    pub graphics: Option<Graphics>,
    #[serde(default)]
    pub phase_filter: Option<crate::phasing::PhaseFilter>,
}

impl ViewTemplate {
    /// A template with every setting taken from `view`.
    pub fn from_view(id: ElementId, name: &str, view: &View) -> ViewTemplate {
        let plan = view.kind.plan_level().is_some();
        ViewTemplate {
            id,
            name: name.to_string(),
            scale: Some(view.scale),
            detail_level: Some(view.detail_level),
            visual_style: Some(view.visual_style),
            cut_height: plan.then_some(view.cut_height),
            graphics: Some(view.graphics.clone()),
            phase_filter: Some(view.phase_filter),
        }
    }

    /// Writes the included settings into a view.
    pub fn apply_to(&self, view: &mut View) {
        if let Some(s) = self.scale {
            view.scale = s;
        }
        if let Some(d) = self.detail_level {
            view.detail_level = d;
        }
        if let Some(v) = self.visual_style {
            view.visual_style = v;
        }
        if let (Some(c), Some(_)) = (self.cut_height, view.kind.plan_level()) {
            view.cut_height = c;
        }
        if let Some(g) = &self.graphics {
            view.graphics = g.clone();
        }
        if let Some(f) = self.phase_filter {
            view.phase_filter = f;
        }
    }
}

impl View {
    pub fn new(id: ElementId, name: &str, kind: ViewKind) -> Self {
        let visual_style = match kind {
            ViewKind::ThreeD => VisualStyle::Shaded,
            _ => VisualStyle::HiddenLine,
        };
        // Revit's ceiling plans cut at 2300, above door heads.
        let cut_height = if matches!(kind, ViewKind::CeilingPlan { .. }) {
            2300.0
        } else {
            1200.0
        };
        // Sheets are drawn full size, in paper millimetres.
        let scale = if matches!(kind, ViewKind::Sheet(_)) { 1 } else { 100 };
        View {
            id,
            name: name.to_string(),
            kind,
            scale,
            detail_level: DetailLevel::Medium,
            visual_style,
            cut_height,
            camera: OrbitCamera::default(),
            section_box: None,
            crop: None,
            callout_of: None,
            graphics: Graphics::new(),
            template: None,
            phase: None,
            phase_filter: Default::default(),
            hidden_elements: Default::default(),
            temp_hide: None,
        }
    }

    /// How the view shows a category.
    pub fn category_graphics(&self, cat: Category) -> CategoryGraphics {
        self.graphics.get(&cat).copied().unwrap_or_default()
    }
}
