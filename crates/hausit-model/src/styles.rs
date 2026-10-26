//! Object styles and line weights (Manage ▸ Object Styles / Line Weights):
//! which pen each category draws with, cut and in projection, and how wide
//! each pen is at each view scale.

use crate::category::Category;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// View scales (1:n) the line weight table has columns for.
pub const SCALES: [u32; 7] = [10, 20, 50, 100, 200, 500, 1000];

/// Pen widths in paper millimetres, one row per pen (1 to 16), one
/// column per entry of [`SCALES`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LineWeights {
    pub widths: Vec<[f64; 7]>,
}

impl Default for LineWeights {
    fn default() -> Self {
        // Widths at 1:100, growing for detailed scales and shrinking for
        // small ones.
        const AT_100: [f64; 16] = [
            0.10, 0.18, 0.25, 0.35, 0.50, 0.65, 0.80, 1.00, 1.20, 1.40, 1.70, 2.00, 2.40, 2.80, 3.40, 4.00,
        ];
        const FACTOR: [f64; 7] = [2.0, 1.6, 1.25, 1.0, 0.75, 0.6, 0.5];
        LineWeights {
            widths: AT_100
                .iter()
                .map(|w| std::array::from_fn(|i| ((w * FACTOR[i]) * 100.0).round() / 100.0))
                .collect(),
        }
    }
}

impl LineWeights {
    /// Width of `pen` (1-based) at 1:`scale`, paper mm. Uses the column of
    /// the nearest scale.
    pub fn width(&self, pen: u8, scale: u32) -> f64 {
        let row = self.widths.get((pen.max(1) - 1) as usize).or(self.widths.last());
        let col = SCALES
            .iter()
            .enumerate()
            .min_by(|a, b| {
                let d = |s: u32| ((s as f64).ln() - (scale.max(1) as f64).ln()).abs();
                d(*a.1).total_cmp(&d(*b.1))
            })
            .map(|c| c.0)
            .unwrap_or(3);
        row.map(|r| r[col]).unwrap_or(0.18)
    }
}

/// Graphics of one category.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectStyle {
    /// Pen for edges seen beyond the cut plane.
    pub projection: u8,
    /// Pen for edges cut by the view (plan cut plane or section).
    pub cut: u8,
    pub color: [u8; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectStyles(pub BTreeMap<Category, ObjectStyle>);

impl Default for ObjectStyles {
    fn default() -> Self {
        let style = |projection, cut| ObjectStyle {
            projection,
            cut,
            color: [0, 0, 0],
        };
        ObjectStyles(
            Category::ALL
                .iter()
                .map(|c| {
                    let s = match c {
                        Category::Walls | Category::StructuralColumns => style(1, 5),
                        Category::Floors => style(1, 4),
                        Category::Roofs => style(3, 4),
                        Category::Doors => style(1, 3),
                        Category::Windows => style(1, 2),
                        _ => style(1, 1),
                    };
                    (*c, s)
                })
                .collect(),
        )
    }
}

impl ObjectStyles {
    pub fn get(&self, cat: Category) -> ObjectStyle {
        self.0.get(&cat).copied().unwrap_or(ObjectStyle {
            projection: 1,
            cut: 1,
            color: [0, 0, 0],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pens_scale_with_view() {
        let lw = LineWeights::default();
        assert_eq!(lw.width(5, 100), 0.5);
        assert!(lw.width(5, 50) > lw.width(5, 100));
        assert!(lw.width(5, 200) < lw.width(5, 100));
        // In between: the nearest column (75 is nearer 100 than 50 on a
        // log scale).
        assert_eq!(lw.width(5, 75), lw.width(5, 100));
    }
}
