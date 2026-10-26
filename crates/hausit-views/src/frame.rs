use hausit_core::{DVec2, DVec3};
use hausit_model::geometry::model_bounds;
use hausit_model::{Document, View, ViewKind};

/// Orthographic projection frame of a view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewFrame {
    pub origin: DVec3,
    pub right: DVec3,
    pub up: DVec3,
    /// Points into the screen (the view direction).
    pub forward: DVec3,
    /// Keep only geometry with depth in this range (sections).
    pub depth_clip: Option<(f64, f64)>,
    /// Keep only geometry with u in this range (section crop width).
    pub u_clip: Option<(f64, f64)>,
}

impl ViewFrame {
    pub fn for_view(doc: &Document, view: &View) -> Option<ViewFrame> {
        match &view.kind {
            ViewKind::ThreeD => {
                let (right, up, forward) = view.camera.basis();
                Some(ViewFrame {
                    origin: DVec3::ZERO,
                    right,
                    up,
                    forward,
                    depth_clip: None,
                    u_clip: None,
                })
            }
            ViewKind::Elevation { direction } => {
                let b = model_bounds(doc);
                let center = b.map(|b| b.center()).unwrap_or(DVec3::ZERO);
                let out = direction.outward();
                let forward = DVec3::new(-out.x, -out.y, 0.0);
                let right = forward.cross(DVec3::Z);
                Some(ViewFrame {
                    origin: DVec3::new(center.x, center.y, 0.0),
                    right,
                    up: DVec3::Z,
                    forward,
                    depth_clip: None,
                    u_clip: None,
                })
            }
            ViewKind::Section { start, end, far_clip } => {
                let d = (*end - *start).normalize_or_zero();
                if d == DVec2::ZERO {
                    return None;
                }
                let forward = DVec3::new(d.y, -d.x, 0.0);
                let right = forward.cross(DVec3::Z);
                let origin = DVec3::new(start.x, start.y, 0.0);
                let ue = (DVec3::new(end.x, end.y, 0.0) - origin).dot(right);
                Some(ViewFrame {
                    origin,
                    right,
                    up: DVec3::Z,
                    forward,
                    depth_clip: Some((0.0, *far_clip)),
                    u_clip: Some((ue.min(0.0), ue.max(0.0))),
                })
            }
            _ => None,
        }
    }

    /// (u, v, depth) of a model point.
    pub fn to_view(&self, p: DVec3) -> (DVec2, f64) {
        let d = p - self.origin;
        (DVec2::new(d.dot(self.right), d.dot(self.up)), d.dot(self.forward))
    }

    /// Model point on the view plane for view coordinates (depth 0).
    pub fn to_model(&self, uv: DVec2) -> DVec3 {
        self.origin + self.right * uv.x + self.up * uv.y
    }
}
