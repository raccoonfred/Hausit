//! A small vector PDF writer for display lists: lines, fills, arcs and
//! text in the standard Helvetica font. Coordinates are paper millimetres
//! with y up, which is PDF's own orientation.

use crate::{Anchor, Dash, DisplayList, Prim, Rgba, Stroke, PX_PER_MM};
use hausit_core::DVec2;
use std::fmt::Write;

/// One page: its paper size in mm and what is drawn on it.
pub struct Page<'a> {
    pub size: DVec2,
    pub list: &'a DisplayList,
}

/// Advance widths of Helvetica for ASCII 32..=126, per 1000 units of em.
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // space../
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // 0..9
    278, 278, 584, 584, 584, 556, 1015, // :..@
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667,
    611, // A..Z
    278, 278, 278, 469, 556, 333, // [..`
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500,
    500, // a..z
    334, 260, 334, 584, // {..~
];

/// Width of `text` set in Helvetica at font size `size`.
pub fn text_width(text: &str, size: f64) -> f64 {
    let units: u32 = text
        .chars()
        .map(|c| match c as u32 {
            32..=126 => HELVETICA[c as usize - 32] as u32,
            _ => 556,
        })
        .sum();
    units as f64 / 1000.0 * size
}

/// Writes the pages as a PDF file.
pub fn write(pages: &[Page], title: &str) -> Vec<u8> {
    // Objects: 1 catalog, 2 page tree, 3 font, 4 info, then a page and
    // its content stream for each page.
    let mut objects: Vec<Vec<u8>> = Vec::new();
    let kids: Vec<String> = (0..pages.len()).map(|i| format!("{} 0 R", 5 + 2 * i)).collect();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objects.push(format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()).into_bytes());
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
    let mut info = b"<< /Producer (Hausit) /Title ".to_vec();
    info.extend(pdf_string(title));
    info.extend(b" >>");
    objects.push(info);
    for (i, page) in pages.iter().enumerate() {
        let pt = page.size * MM_TO_PT;
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                num(pt.x),
                num(pt.y),
                6 + 2 * i
            )
            .into_bytes(),
        );
        let content = content_stream(page.list);
        let mut obj = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        obj.extend(content);
        obj.extend(b"\nendstream");
        objects.push(obj);
    }

    let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).into_bytes());
        out.extend(body);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for o in offsets {
        let _ = writeln!(tail, "{o:010} 00000 n ");
    }
    let _ = write!(
        tail,
        "trailer\n<< /Size {} /Root 1 0 R /Info 4 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out.extend(tail.into_bytes());
    out
}

const MM_TO_PT: f64 = 72.0 / 25.4;

fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

fn pt(p: DVec2) -> String {
    format!("{} {}", num(p.x), num(p.y))
}

/// Fills with transparency are blended onto white paper.
fn rgb(c: Rgba) -> [f64; 3] {
    let a = c[3] as f64 / 255.0;
    [0, 1, 2].map(|i| (c[i] as f64 * a + 255.0 * (1.0 - a)) / 255.0)
}

fn set_stroke(s: &mut String, stroke: Stroke) {
    let [r, g, b] = rgb(stroke.color);
    let w = (stroke.width as f64 / PX_PER_MM).max(0.08);
    let dash = match stroke.dash {
        Dash::Solid => "[] 0 d",
        Dash::Dashed => "[2.5 1.25] 0 d",
        Dash::DashDot => "[5 1.25 0.6 1.25] 0 d",
    };
    let _ = writeln!(s, "{} {} {} RG {} w {dash}", num(r), num(g), num(b), num(w));
}

fn set_fill(s: &mut String, c: Rgba) {
    let [r, g, b] = rgb(c);
    let _ = writeln!(s, "{} {} {} rg", num(r), num(g), num(b));
}

fn path(s: &mut String, pts: &[DVec2], closed: bool) {
    for (i, p) in pts.iter().enumerate() {
        let _ = writeln!(s, "{} {}", pt(*p), if i == 0 { "m" } else { "l" });
    }
    if closed {
        s.push_str("h\n");
    }
}

fn arc_points(center: DVec2, r: f64, a0: f64, a1: f64) -> Vec<DVec2> {
    let n = ((a1 - a0).abs() / 0.1).ceil().max(4.0) as usize;
    (0..=n)
        .map(|i| center + DVec2::from_angle(a0 + (a1 - a0) * i as f64 / n as f64) * r)
        .collect()
}

fn content_stream(dl: &DisplayList) -> Vec<u8> {
    let mut s = String::new();
    // Work in millimetres, with round caps and joins.
    let _ = writeln!(s, "{0} 0 0 {0} 0 0 cm 1 J 1 j", num(MM_TO_PT));
    for item in &dl.items {
        if let Some(st) = item.stroke {
            set_stroke(&mut s, st);
        }
        if let Some(f) = item.fill {
            set_fill(&mut s, f);
        }
        let paint = match (item.fill.is_some(), item.stroke.is_some()) {
            (true, true) => "B",
            (true, false) => "f",
            (false, true) => "S",
            (false, false) => continue,
        };
        match &item.prim {
            Prim::Line(a, b) => {
                if item.stroke.is_some() {
                    path(&mut s, &[*a, *b], false);
                    s.push_str("S\n");
                }
            }
            Prim::Polyline { pts, closed } => {
                if item.stroke.is_some() && pts.len() > 1 {
                    path(&mut s, pts, *closed);
                    s.push_str("S\n");
                }
            }
            Prim::Polygon(pts) => {
                if pts.len() > 2 {
                    path(&mut s, pts, true);
                    let _ = writeln!(s, "{paint}");
                }
            }
            Prim::Arc {
                center,
                radius,
                start,
                end,
            } => {
                if item.stroke.is_some() {
                    path(&mut s, &arc_points(*center, *radius, *start, *end), false);
                    s.push_str("S\n");
                }
            }
            Prim::Circle { center, radius } => {
                let pts = arc_points(*center, *radius, 0.0, std::f64::consts::TAU);
                path(&mut s, &pts[..pts.len() - 1], true);
                let _ = writeln!(s, "{paint}");
            }
            Prim::Text { .. } => {}
        }
    }
    // Text last, on top of fills, as on screen.
    s.push_str("0 0 0 rg\n");
    for item in &dl.items {
        if let Prim::Text {
            pos,
            text,
            height,
            anchor,
            angle,
        } = &item.prim
        {
            if text.is_empty() || *height <= 0.0 {
                continue;
            }
            let w = text_width(text, *height);
            // Offset of the baseline start from `pos`, before rotation.
            // Rotated text is centred, as on screen.
            let (dx, dy) = match (anchor, angle.abs() > 1e-4) {
                (_, true) | (Anchor::Center, _) => (-w / 2.0, -0.35),
                (Anchor::Left, _) => (0.0, -0.35),
                (Anchor::Right, _) => (-w, -0.35),
                (Anchor::BottomLeft, _) => (0.0, 0.22),
                (Anchor::BottomRight, _) => (-w, 0.22),
            };
            let rot = DVec2::from_angle(*angle);
            let o = *pos + rot.rotate(DVec2::new(dx, dy * height));
            let _ = write!(
                s,
                "BT /F1 {} Tf {} {} {} {} {} Tm ",
                num(*height),
                num(rot.x),
                num(rot.y),
                num(-rot.y),
                num(rot.x),
                pt(o)
            );
            s.push_str(&String::from_utf8_lossy(&pdf_string(text)));
            s.push_str(" Tj ET\n");
        }
    }
    s.into_bytes()
}

/// A PDF literal string in WinAnsi (Latin-1 for the characters Hausit
/// uses: degrees, squared, cubed), with escapes.
fn pdf_string(text: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for c in text.chars() {
        let b = match c as u32 {
            0x20..=0x7e => c as u8,
            0xa0..=0xff => c as u8,
            0x2013 => 0x96,
            0x2014 => 0x97,
            0x2022 => 0x95,
            _ => b'?',
        };
        match b {
            b'(' | b')' | b'\\' => out.extend([b'\\', b]),
            0x20..=0x7e => out.push(b),
            _ => out.extend(format!("\\{b:03o}").into_bytes()),
        }
    }
    out.push(b')');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DisplayList, THIN};

    #[test]
    fn writes_a_valid_looking_pdf() {
        let mut dl = DisplayList::default();
        dl.line(DVec2::ZERO, DVec2::new(100.0, 50.0), THIN, None);
        dl.push(
            Prim::Text {
                pos: DVec2::new(10.0, 10.0),
                text: "Area (m²) 12".into(),
                height: 3.0,
                anchor: Anchor::Left,
                angle: 0.0,
            },
            None,
            None,
            None,
        );
        let pdf = write(
            &[Page {
                size: DVec2::new(420.0, 297.0),
                list: &dl,
            }],
            "Test (1)",
        );
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/MediaBox [0 0 1190.551 841.89]"));
        assert!(text.contains("(Area \\(m\\262\\) 12) Tj"));
        assert!(text.contains("/Title (Test \\(1\\))"));
        assert!(text.trim_end().ends_with("%%EOF"));
        // The xref offsets (in bytes) point at the objects.
        let xref = pdf.windows(6).position(|w| w == b"\nxref\n").unwrap() + 1;
        let first = String::from_utf8_lossy(&pdf[xref..]).lines().nth(3).unwrap().to_string();
        let off: usize = first[..10].parse().unwrap();
        assert!(pdf[off..].starts_with(b"1 0 obj"));
        assert!((text_width("Hausit", 10.0) - 28.34).abs() < 0.01);
    }
}
