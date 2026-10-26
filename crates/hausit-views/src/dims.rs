//! Aligned dimension strings, drawn like Revit's default "Diagonal - 2.5mm
//! Arial" type: witness lines with a gap to the element, heavy diagonal
//! ticks, and the value above the line, reading from the bottom or right.

use crate::{Anchor, DisplayList, Prim, Stroke, BLACK, THIN};
use hausit_core::{format_length, DVec2, ElementId, LengthUnit};
use hausit_model::dimensions::DimLayout;

/// Paper sizes, mm.
const TEXT_H: f64 = 2.5;
const WITNESS_GAP: f64 = 1.5;
const WITNESS_EXT: f64 = 2.0;
const LINE_EXT: f64 = 2.0;
const TICK: f64 = 1.5;

pub fn draw_dimension(dl: &mut DisplayList, l: &DimLayout, scale: f64, unit: LengthUnit, id: Option<ElementId>) {
    let (Some(first), Some(last)) = (l.ticks.first(), l.ticks.last()) else {
        return;
    };
    let n = l.n;
    let dir = (*last - *first).normalize_or(n);
    dl.line(*first - dir * LINE_EXT * scale, *last + dir * LINE_EXT * scale, THIN, id);
    for (tick, origin) in l.ticks.iter().zip(&l.origins) {
        let to_line = *tick - *origin;
        let len = to_line.length();
        if len > WITNESS_GAP * scale {
            let u = to_line / len;
            dl.line(*origin + u * WITNESS_GAP * scale, *tick + u * WITNESS_EXT * scale, THIN, id);
        }
        // Diagonal tick across the dimension line.
        let along = dir * TICK * scale;
        let across = DVec2::new(-dir.y, dir.x) * TICK * scale;
        dl.line(*tick - along - across, *tick + along + across, Stroke::new(2.2, BLACK), id);
    }
    // Text reads left to right, or bottom to top.
    let read = if dir.x < -1e-6 || (dir.x.abs() <= 1e-6 && dir.y < 0.0) {
        -dir
    } else {
        dir
    };
    let up = DVec2::new(-read.y, read.x);
    for (w, v) in l.ticks.windows(2).zip(&l.values) {
        let mid = (w[0] + w[1]) * 0.5;
        dl.push(
            Prim::Text {
                pos: mid + up * TEXT_H * scale * 0.9,
                text: format_length(v.abs(), unit),
                height: TEXT_H * scale,
                anchor: Anchor::Center,
                angle: read.y.atan2(read.x),
            },
            None,
            None,
            id,
        );
    }
}
