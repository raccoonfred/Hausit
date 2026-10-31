//! DXF export of a view's display list (AutoCAD R12 DXF, which every CAD
//! program reads). Elements go on AIA-style layers by category, as Revit's
//! DWG/DXF export does by default; lines, arcs, circles, text and solid
//! fills keep their colour and line pattern. Units are millimetres: model
//! millimetres for plans, sections and elevations, paper millimetres for
//! sheets.

use crate::{Anchor, Dash, DisplayList, Prim, Rgba};
use hausit_core::geom::triangulate;
use hausit_core::{DVec2, ElementId};
use hausit_model::{Category, Document};
use std::collections::BTreeSet;
use std::fmt::Write;

/// The layer of a category, after the AIA CAD Layer Guidelines.
pub fn layer_of(cat: Option<Category>) -> &'static str {
    match cat {
        Some(Category::Walls) => "A-WALL",
        Some(Category::Doors) => "A-DOOR",
        Some(Category::Windows) => "A-GLAZ",
        Some(Category::Floors) => "A-FLOR",
        Some(Category::Roofs) => "A-ROOF",
        Some(Category::Ceilings) => "A-CLNG",
        Some(Category::StructuralColumns) => "S-COLS",
        Some(Category::StructuralFraming) => "S-BEAM",
        Some(Category::StructuralFoundations) => "S-FNDN",
        Some(Category::Stairs) => "A-FLOR-STRS",
        Some(Category::Railings) => "A-FLOR-HRAL",
        Some(Category::Furniture) => "A-FURN",
        Some(Category::Casework) => "A-FLOR-CASE",
        Some(Category::PlumbingFixtures) => "P-FIXT",
        Some(Category::SpecialtyEquipment) => "A-EQPM",
        Some(Category::GenericModels) => "A-GENM",
        Some(Category::Rooms) => "A-AREA-IDEN",
        Some(Category::RoomSeparation) => "A-AREA-BNDY",
        Some(Category::Dimensions) => "A-ANNO-DIMS",
        Some(Category::TextNotes) => "A-ANNO-TEXT",
        Some(Category::Tags) => "A-ANNO-TAGS",
        Some(Category::Lines) => "A-DETL-LINE",
        Some(Category::DetailItems) => "A-DETL-PATT",
        Some(Category::SpotElevations) => "A-ANNO-SPOT",
        Some(Category::ModelGroups) => "A-GENM",
        Some(Category::Areas) => "A-AREA-IDEN",
        Some(Category::AreaBoundary) => "A-AREA-BNDY",
        Some(Category::RevisionClouds) => "G-ANNO-REVS",
        Some(Category::Grids) => "S-GRID",
        Some(Category::Levels) => "A-ANNO-LEVL",
        None => "A-ANNO-SYMB",
    }
}

/// The AutoCAD Color Index nearest to a colour (black prints as 7, which
/// CAD shows white on black and black on white).
fn aci(c: Rgba) -> u8 {
    const PALETTE: [(u8, [u8; 3]); 13] = [
        (1, [255, 0, 0]),
        (2, [255, 255, 0]),
        (3, [0, 255, 0]),
        (4, [0, 255, 255]),
        (5, [0, 0, 255]),
        (6, [255, 0, 255]),
        (7, [0, 0, 0]),
        (8, [128, 128, 128]),
        (9, [192, 192, 192]),
        (250, [51, 51, 51]),
        (251, [80, 80, 80]),
        (252, [105, 105, 105]),
        (253, [130, 130, 130]),
    ];
    let d = |p: [u8; 3]| {
        (0..3)
            .map(|i| {
                let x = c[i] as i32 - p[i] as i32;
                x * x
            })
            .sum::<i32>()
    };
    PALETTE.iter().min_by_key(|(_, p)| d(*p)).map(|(k, _)| *k).unwrap_or(7)
}

fn linetype(d: Dash) -> &'static str {
    match d {
        Dash::Solid => "CONTINUOUS",
        Dash::Dashed => "DASHED",
        Dash::DashDot => "DASHDOT",
    }
}

fn num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

/// DXF text is plain 8-bit; other characters become `?`.
fn ascii(t: &str) -> String {
    t.chars().map(|c| if c.is_ascii() && !c.is_control() { c } else { '?' }).collect()
}

struct Out {
    s: String,
}

impl Out {
    fn pair(&mut self, code: i32, value: impl std::fmt::Display) {
        let _ = writeln!(self.s, "{code:>3}\n{value}");
    }

    fn point(&mut self, base: i32, p: DVec2) {
        self.pair(base, num(p.x));
        self.pair(base + 10, num(p.y));
        self.pair(base + 20, 0);
    }

    fn entity(&mut self, kind: &str, layer: &str, color: u8, ltype: &str) {
        self.pair(0, kind);
        self.pair(8, layer);
        if ltype != "CONTINUOUS" {
            self.pair(6, ltype);
        }
        self.pair(62, color);
    }
}

/// Writes the display list as DXF text.
pub fn write(doc: &Document, list: &DisplayList) -> String {
    let cat = |id: Option<ElementId>| id.and_then(|id| doc.element(id)).map(|e| e.category());
    let layers: BTreeSet<&str> = std::iter::once("0")
        .chain(list.items.iter().map(|it| layer_of(cat(it.element))))
        .collect();
    let (lo, hi) = list.bounds().unwrap_or((DVec2::ZERO, DVec2::ZERO));
    let mut o = Out { s: String::new() };

    // Header.
    o.pair(0, "SECTION");
    o.pair(2, "HEADER");
    o.pair(9, "$ACADVER");
    o.pair(1, "AC1009");
    o.pair(9, "$EXTMIN");
    o.point(10, lo);
    o.pair(9, "$EXTMAX");
    o.point(10, hi);
    o.pair(9, "$LTSCALE");
    o.pair(40, 100);
    o.pair(0, "ENDSEC");

    // Tables: line types and layers.
    o.pair(0, "SECTION");
    o.pair(2, "TABLES");
    o.pair(0, "TABLE");
    o.pair(2, "LTYPE");
    o.pair(70, 3);
    for (name, desc, pattern) in [
        ("CONTINUOUS", "Solid line", vec![]),
        ("DASHED", "Dashed __ __ __", vec![6.0, -3.0]),
        ("DASHDOT", "Dash dot __ . __ .", vec![8.0, -2.0, 0.0, -2.0]),
    ] {
        o.pair(0, "LTYPE");
        o.pair(2, name);
        o.pair(70, 0);
        o.pair(3, desc);
        o.pair(72, 65);
        o.pair(73, pattern.len());
        o.pair(40, num(pattern.iter().map(|v: &f64| v.abs()).sum()));
        for v in pattern {
            o.pair(49, num(v));
        }
    }
    o.pair(0, "ENDTAB");
    o.pair(0, "TABLE");
    o.pair(2, "LAYER");
    o.pair(70, layers.len());
    for l in &layers {
        o.pair(0, "LAYER");
        o.pair(2, l);
        o.pair(70, 0);
        o.pair(62, 7);
        o.pair(6, "CONTINUOUS");
    }
    o.pair(0, "ENDTAB");
    o.pair(0, "TABLE");
    o.pair(2, "STYLE");
    o.pair(70, 1);
    o.pair(0, "STYLE");
    o.pair(2, "STANDARD");
    o.pair(70, 0);
    o.pair(40, 0);
    o.pair(41, 1);
    o.pair(50, 0);
    o.pair(71, 0);
    o.pair(42, 2.5);
    o.pair(3, "txt");
    o.pair(4, "");
    o.pair(0, "ENDTAB");
    o.pair(0, "ENDSEC");

    // Entities.
    o.pair(0, "SECTION");
    o.pair(2, "ENTITIES");
    for it in &list.items {
        let layer = layer_of(cat(it.element));
        let (color, lt) = match it.stroke {
            Some(s) => (aci(s.color), linetype(s.dash)),
            None => (7, "CONTINUOUS"),
        };
        match &it.prim {
            Prim::Line(a, b) => {
                if it.stroke.is_some() {
                    o.entity("LINE", layer, color, lt);
                    o.point(10, *a);
                    o.point(11, *b);
                }
            }
            Prim::Polyline { pts, closed } => {
                if it.stroke.is_some() {
                    polyline(&mut o, layer, color, lt, pts, *closed);
                }
            }
            Prim::Polygon(pts) => {
                // Solid fills that show (not white masking or invisible
                // pick regions), as triangles.
                if let Some(f) = it.fill.filter(|f| f[3] > 0 && !(f[0] > 245 && f[1] > 245 && f[2] > 245)) {
                    let fc = aci(f);
                    for t in triangulate(pts) {
                        o.entity("SOLID", layer, fc, "CONTINUOUS");
                        o.point(10, pts[t[0]]);
                        o.point(11, pts[t[1]]);
                        o.point(12, pts[t[2]]);
                        o.point(13, pts[t[2]]);
                    }
                }
                if it.stroke.is_some() {
                    polyline(&mut o, layer, color, lt, pts, true);
                }
            }
            Prim::Arc {
                center,
                radius,
                start,
                end,
            } => {
                if it.stroke.is_some() {
                    // DXF arcs run counter-clockwise.
                    let (a0, a1) = if end >= start { (*start, *end) } else { (*end, *start) };
                    o.entity("ARC", layer, color, lt);
                    o.point(10, *center);
                    o.pair(40, num(*radius));
                    o.pair(50, num(a0.to_degrees()));
                    o.pair(51, num(a1.to_degrees()));
                }
            }
            Prim::Circle { center, radius } => {
                if let Some(f) = it.fill.filter(|f| f[3] > 0 && f[0] < 128) {
                    // Filled dots (leader ends, section marks): a disc.
                    let n = 16;
                    let pts: Vec<DVec2> = (0..n)
                        .map(|k| *center + DVec2::from_angle(std::f64::consts::TAU * k as f64 / n as f64) * *radius)
                        .collect();
                    for k in 1..n - 1 {
                        o.entity("SOLID", layer, aci(f), "CONTINUOUS");
                        o.point(10, pts[0]);
                        o.point(11, pts[k]);
                        o.point(12, pts[k + 1]);
                        o.point(13, pts[k + 1]);
                    }
                }
                if it.stroke.is_some() {
                    o.entity("CIRCLE", layer, color, lt);
                    o.point(10, *center);
                    o.pair(40, num(*radius));
                }
            }
            Prim::Text {
                pos,
                text,
                height,
                anchor,
                angle,
            } => {
                if text.trim().is_empty() {
                    continue;
                }
                let (h, v) = match anchor {
                    Anchor::Center => (1, 2),
                    Anchor::Left => (0, 2),
                    Anchor::Right => (2, 2),
                    Anchor::BottomLeft => (0, 1),
                    Anchor::BottomRight => (2, 1),
                };
                o.entity("TEXT", layer, 7, "CONTINUOUS");
                o.point(10, *pos);
                // Cap height is about 70% of the em height used on screen.
                o.pair(40, num(height * 0.7));
                o.pair(1, ascii(text));
                if angle.abs() > 1e-9 {
                    o.pair(50, num(angle.to_degrees()));
                }
                o.pair(72, h);
                o.point(11, *pos);
                o.pair(73, v);
            }
        }
    }
    o.pair(0, "ENDSEC");
    o.pair(0, "EOF");
    o.s
}

fn polyline(o: &mut Out, layer: &str, color: u8, lt: &str, pts: &[DVec2], closed: bool) {
    if pts.len() < 2 {
        return;
    }
    if pts.len() == 2 && !closed {
        o.entity("LINE", layer, color, lt);
        o.point(10, pts[0]);
        o.point(11, pts[1]);
        return;
    }
    o.entity("POLYLINE", layer, color, lt);
    o.pair(66, 1);
    o.point(10, DVec2::ZERO);
    o.pair(70, if closed { 1 } else { 0 });
    for p in pts {
        o.pair(0, "VERTEX");
        o.pair(8, layer);
        o.point(10, *p);
    }
    o.pair(0, "SEQEND");
    o.pair(8, layer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use hausit_model::template::sample_project;

    #[test]
    fn writes_a_plan() {
        let doc = sample_project();
        let plan = doc
            .views()
            .find(|v| matches!(v.kind, hausit_model::ViewKind::FloorPlan { .. }))
            .unwrap();
        let text = write(&doc, &crate::build(&doc, plan));
        assert!(text.starts_with("  0\nSECTION"));
        assert!(text.trim_end().ends_with("EOF"));
        for layer in ["A-WALL", "A-DOOR", "A-GLAZ", "S-GRID", "A-ANNO-TAGS"] {
            assert!(text.contains(&format!("  2\n{layer}\n")), "{layer}");
        }
        assert!(text.contains("TEXT"));
        // Pairs: code lines and value lines alternate.
        assert_eq!(text.lines().count() % 2, 0);
    }
}
