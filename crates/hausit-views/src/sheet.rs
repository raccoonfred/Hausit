//! Sheet graphics: the paper, the title block, and each placed view drawn
//! at its scale, with a viewport title. Schedules are drawn as tables.
//! Everything is in paper millimetres, origin at the paper's bottom-left.

use crate::pdf::text_width;
use crate::{Anchor, DisplayList, Item, Prim, Stroke, HEAVY, MEDIUM, THIN};
use hausit_core::{DVec2, ElementId};
use hausit_model::sheet::{border, strip_width};
use hausit_model::{Document, ScheduleDef, SheetDef, View, ViewKind};

const PAPER_EDGE: Stroke = Stroke::new(1.0, [150, 150, 150, 255]);

pub fn build(doc: &Document, sheet: &View, def: &SheetDef) -> DisplayList {
    let mut dl = DisplayList::default();
    let paper = def.size.paper();
    dl.push(
        Prim::Polygon(rect(DVec2::ZERO, paper)),
        Some(PAPER_EDGE),
        Some([255, 255, 255, 255]),
        None,
    );
    for vp in &def.viewports {
        let Some(view) = doc.view(vp.view) else { continue };
        match &view.kind {
            ViewKind::Schedule(s) => schedule_table(&mut dl, doc, view, s, vp.position),
            ViewKind::Sheet(_) => {}
            _ => {
                let Some((list, min, max)) = view_graphics(doc, view) else {
                    continue;
                };
                let scale = view.scale as f64;
                let map = |p: DVec2| vp.position + (p - (min + max) * 0.5) / scale;
                for item in list.items {
                    dl.items.push(Item {
                        prim: transform(item.prim, &map, scale),
                        element: Some(view.id),
                        ..item
                    });
                }
                let (lo, _) = (map(min), map(max));
                viewport_title(&mut dl, view, &vp.detail_number, DVec2::new(lo.x, lo.y - 8.0));
            }
        }
    }
    title_block(&mut dl, doc, sheet, def);
    dl
}

/// The display list of a placed view and its extents in view coordinates.
fn view_graphics(doc: &Document, view: &View) -> Option<(DisplayList, DVec2, DVec2)> {
    let list = crate::build(doc, view);
    let (min, max) = list.bounds()?;
    Some((list, min, max))
}

/// Size on paper of a view placed on a sheet, in mm, including its title.
pub fn viewport_size(doc: &Document, view: &View) -> DVec2 {
    match &view.kind {
        ViewKind::Schedule(s) => {
            let t = Table::new(doc, view, s);
            DVec2::new(t.widths.iter().sum(), t.height())
        }
        ViewKind::Sheet(_) => DVec2::ZERO,
        _ => match view_graphics(doc, view) {
            Some((_, min, max)) => (max - min) / view.scale as f64 + DVec2::new(0.0, 16.0),
            None => DVec2::ZERO,
        },
    }
}

/// The paper a viewport covers, title included.
pub fn viewport_rect(doc: &Document, vp: &hausit_model::Viewport) -> Option<(DVec2, DVec2)> {
    let view = doc.view(vp.view)?;
    let size = viewport_size(doc, view);
    Some(match view.kind {
        ViewKind::Schedule(_) => (vp.position - DVec2::new(0.0, size.y), vp.position + DVec2::new(size.x, 0.0)),
        _ => {
            let half = DVec2::new(size.x, size.y - 16.0) / 2.0;
            (vp.position - half - DVec2::new(0.0, 16.0), vp.position + half)
        }
    })
}

/// Where to put `view` on a sheet: the first free spot, reading from the
/// top left of the drawing area, clear of the viewports already there.
/// Falls back to the middle when nothing fits.
pub fn free_position(doc: &Document, def: &SheetDef, view: &View) -> DVec2 {
    let (amin, amax) = def.drawing_area();
    let (amin, amax) = (amin + DVec2::splat(10.0), amax - DVec2::splat(10.0));
    let size = viewport_size(doc, view);
    let taken: Vec<(DVec2, DVec2)> = def.viewports.iter().filter_map(|v| viewport_rect(doc, v)).collect();
    let gap = 10.0;
    let free = |lo: DVec2, hi: DVec2| {
        taken
            .iter()
            .all(|(a, b)| hi.x + gap <= a.x || lo.x >= b.x + gap || hi.y + gap <= a.y || lo.y >= b.y + gap)
    };
    // The rectangle's top-left corner → the viewport's position.
    let position = |tl: DVec2| match view.kind {
        ViewKind::Schedule(_) => tl,
        _ => DVec2::new(tl.x + size.x / 2.0, tl.y - (size.y - 16.0) / 2.0),
    };
    let mut y = amax.y;
    while y - size.y >= amin.y {
        let mut x = amin.x;
        while x + size.x <= amax.x {
            let (lo, hi) = (DVec2::new(x, y - size.y), DVec2::new(x + size.x, y));
            if free(lo, hi) {
                return position(DVec2::new(x, y));
            }
            x += 5.0;
        }
        y -= 5.0;
    }
    let mid = (amin + amax) * 0.5;
    position(mid + DVec2::new(-size.x, size.y) / 2.0)
}

/// Places a view on a sheet at the first free spot.
pub fn place_view(doc: &mut Document, sheet: ElementId, view: ElementId) -> Result<(), hausit_model::ModelError> {
    let pos = match (doc.view(sheet).map(|v| &v.kind), doc.view(view)) {
        (Some(ViewKind::Sheet(def)), Some(v)) => free_position(doc, def, v),
        _ => return Err(hausit_model::ModelError::NotFound(view)),
    };
    doc.place_view(sheet, view, pos)
}

/// A starter set of sheets for the sample projects: plans, elevations,
/// sections and schedules, each laid out on its own sheet.
pub fn add_sample_sheets(doc: &mut Document) {
    let find = |doc: &Document, pred: &dyn Fn(&View) -> bool| doc.views().filter(|v| pred(v)).map(|v| v.id).collect::<Vec<_>>();
    let plans = find(doc, &|v| matches!(v.kind, ViewKind::FloorPlan { .. }) && v.callout_of.is_none());
    let elevations = find(doc, &|v| matches!(v.kind, ViewKind::Elevation { .. }));
    let sections = find(doc, &|v| matches!(v.kind, ViewKind::Section { .. }) && v.callout_of.is_none());
    let schedules = find(doc, &|v| matches!(v.kind, ViewKind::Schedule(_)));
    let three_d = find(doc, &|v| matches!(v.kind, ViewKind::ThreeD));
    let sheet = |doc: &mut Document, number: &str, name: &str, size: hausit_model::SheetSize, views: &[ElementId]| {
        let id = doc.add_sheet(number, name, size);
        if let Some(ViewKind::Sheet(def)) = doc.view_mut(id).map(|v| &mut v.kind) {
            def.drawn_by = "Hausit".into();
            def.issue_date = "2026-10-26".into();
        }
        for v in views {
            let _ = place_view(doc, id, *v);
        }
    };
    use hausit_model::SheetSize::*;
    let mut first: Vec<ElementId> = plans.iter().take(2).copied().collect();
    first.extend(three_d.first());
    sheet(doc, "A101", "Plans", A1, &first);
    sheet(doc, "A201", "Elevations", A1, &elevations);
    let mut secs = sections.clone();
    secs.extend(schedules.first());
    sheet(doc, "A301", "Sections", A1, &secs);
    sheet(doc, "A601", "Schedules", A3, &schedules[1.min(schedules.len())..]);
}

fn transform(prim: Prim, map: &impl Fn(DVec2) -> DVec2, scale: f64) -> Prim {
    match prim {
        Prim::Line(a, b) => Prim::Line(map(a), map(b)),
        Prim::Polyline { pts, closed } => Prim::Polyline {
            pts: pts.into_iter().map(map).collect(),
            closed,
        },
        Prim::Polygon(pts) => Prim::Polygon(pts.into_iter().map(map).collect()),
        Prim::Arc {
            center,
            radius,
            start,
            end,
        } => Prim::Arc {
            center: map(center),
            radius: radius / scale,
            start,
            end,
        },
        Prim::Circle { center, radius } => Prim::Circle {
            center: map(center),
            radius: radius / scale,
        },
        Prim::Text {
            pos,
            text,
            height,
            anchor,
            angle,
        } => Prim::Text {
            pos: map(pos),
            text,
            height: height / scale,
            anchor,
            angle,
        },
    }
}

fn rect(min: DVec2, max: DVec2) -> Vec<DVec2> {
    vec![min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)]
}

fn text(dl: &mut DisplayList, pos: DVec2, s: &str, height: f64, anchor: Anchor, element: Option<ElementId>) {
    dl.push(
        Prim::Text {
            pos,
            text: s.to_string(),
            height,
            anchor,
            angle: 0.0,
        },
        None,
        None,
        element,
    );
}

/// Detail number in a circle, the view name over a heavy line, and the
/// scale beneath, with `at` the left end of the line.
fn viewport_title(dl: &mut DisplayList, view: &View, number: &str, at: DVec2) {
    let id = Some(view.id);
    let r = 4.5;
    let c = at + DVec2::new(r, 0.0);
    dl.push(Prim::Circle { center: c, radius: r }, Some(MEDIUM), Some([255, 255, 255, 255]), id);
    text(dl, c, number, 3.5, Anchor::Center, id);
    let x = at.x + 2.0 * r + 2.0;
    let name = view.name.to_uppercase();
    let len = text_width(&name, 4.0).max(40.0) + 4.0;
    dl.line(DVec2::new(c.x + r, at.y), DVec2::new(x + len, at.y), HEAVY, id);
    text(dl, DVec2::new(x, at.y + 1.0), &name, 4.0, Anchor::BottomLeft, id);
    text(
        dl,
        DVec2::new(x, at.y - 1.5 - 2.5),
        &format!("1 : {}", view.scale),
        2.5,
        Anchor::BottomLeft,
        id,
    );
}

/// A schedule laid out for a sheet: column widths and rows of text.
struct Table {
    title: String,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    widths: Vec<f64>,
}

const CELL_H: f64 = 2.5;
const ROW: f64 = 6.0;

impl Table {
    fn new(doc: &Document, view: &View, def: &ScheduleDef) -> Table {
        let t = hausit_schedules::build_table(doc, def);
        let mut rows: Vec<Vec<String>> = t.rows.iter().map(|r| r.cells.iter().map(|c| c.text.clone()).collect()).collect();
        rows.extend(t.totals);
        let mut widths: Vec<f64> = t.headers.iter().map(|h| text_width(h, CELL_H) + 4.0).collect();
        for r in &rows {
            for (w, c) in widths.iter_mut().zip(r) {
                *w = w.max(text_width(c, CELL_H) + 4.0);
            }
        }
        // The title spans the table.
        let title_w = text_width(&view.name, 3.5) + 6.0;
        let total: f64 = widths.iter().sum();
        if total < title_w && !widths.is_empty() {
            let extra = (title_w - total) / widths.len() as f64;
            widths.iter_mut().for_each(|w| *w += extra);
        }
        Table {
            title: view.name.to_uppercase(),
            headers: t.headers,
            rows,
            widths,
        }
    }

    fn height(&self) -> f64 {
        ROW * (2 + self.rows.len()) as f64
    }
}

fn schedule_table(dl: &mut DisplayList, doc: &Document, view: &View, def: &ScheduleDef, top_left: DVec2) {
    let t = Table::new(doc, view, def);
    let id = Some(view.id);
    let width: f64 = t.widths.iter().sum();
    let bottom = top_left.y - t.height();
    dl.push(
        Prim::Polygon(rect(DVec2::new(top_left.x, bottom), DVec2::new(top_left.x + width, top_left.y))),
        Some(MEDIUM),
        Some([255, 255, 255, 255]),
        id,
    );
    text(
        dl,
        top_left + DVec2::new(width / 2.0, -ROW / 2.0),
        &t.title,
        3.5,
        Anchor::Center,
        id,
    );
    let rows = std::iter::once(&t.headers).chain(&t.rows);
    for (i, row) in rows.enumerate() {
        let y = top_left.y - ROW * (i + 1) as f64;
        dl.line(
            DVec2::new(top_left.x, y),
            DVec2::new(top_left.x + width, y),
            if i == 0 { MEDIUM } else { THIN },
            id,
        );
        let mut x = top_left.x;
        for (w, cell) in t.widths.iter().zip(row) {
            text(dl, DVec2::new(x + 2.0, y - ROW / 2.0), cell, CELL_H, Anchor::Left, id);
            x += w;
        }
    }
    let mut x = top_left.x;
    for w in &t.widths[..t.widths.len().saturating_sub(1)] {
        x += w;
        dl.line(DVec2::new(x, top_left.y - ROW), DVec2::new(x, bottom), THIN, id);
    }
}

/// Scale shown in the title block: the views' common scale, or "As
/// indicated" when they differ.
fn sheet_scale(doc: &Document, def: &SheetDef) -> String {
    let mut scales: Vec<u32> = def
        .viewports
        .iter()
        .filter_map(|v| doc.view(v.view))
        .filter(|v| !matches!(v.kind, ViewKind::Schedule(_)))
        .map(|v| v.scale)
        .collect();
    scales.dedup();
    match scales.as_slice() {
        [] => String::new(),
        [s] => format!("1 : {s}"),
        _ => "As indicated".into(),
    }
}

/// The title block: a border, and a strip down the right with the project
/// and sheet information. An original design, sized from the A1 layout.
fn title_block(dl: &mut DisplayList, doc: &Document, sheet: &View, def: &SheetDef) {
    let name = sheet.name.as_str();
    let revisions: Vec<ElementId> = doc
        .sheet_revisions(sheet.id)
        .into_iter()
        .filter(|r| doc.revision_def(*r).is_some_and(|r| r.numbering != hausit_model::Numbering::None))
        .collect();
    let (min, max) = border(def.size);
    let w = strip_width(def.size);
    let k = w / 180.0;
    dl.push(
        Prim::Polyline {
            pts: rect(min, max),
            closed: true,
        },
        Some(HEAVY),
        None,
        None,
    );
    let x0 = max.x - w;
    dl.line(DVec2::new(x0, min.y), DVec2::new(x0, max.y), HEAVY, None);
    let info = &doc.project_info;
    let label_h = 2.0 * k;
    let pad = 2.5 * k;
    let hline = |dl: &mut DisplayList, y: f64, s: Stroke| dl.line(DVec2::new(x0, y), DVec2::new(max.x, y), s, None);
    let label = |dl: &mut DisplayList, x: f64, top: f64, s: &str| {
        text(dl, DVec2::new(x + pad, top - pad - label_h), s, label_h, Anchor::BottomLeft, None)
    };
    let value = |dl: &mut DisplayList, x: f64, top: f64, s: &str, h: f64| {
        text(
            dl,
            DVec2::new(x + pad, top - pad - label_h - 2.0 * k - h),
            s,
            h,
            Anchor::BottomLeft,
            None,
        )
    };

    // From the bottom up: sheet number, scale and date, drawn/checked,
    // sheet name, project, client, status, then the notes area.
    let mid = x0 + w * 0.45;
    let mut y = min.y;
    let rows: [(f64, &str); 6] = [
        (34.0, "number"),
        (14.0, "drawn"),
        (26.0, "sheet"),
        (34.0, "project"),
        (16.0, "client"),
        (14.0, "status"),
    ];
    for (h, row) in rows {
        let top = y + h * k;
        match row {
            "number" => {
                label(dl, mid, top, "SHEET NUMBER");
                if let Some(rev) = doc.sheet_current_revision(sheet.id) {
                    text(
                        dl,
                        DVec2::new(mid + pad, y + pad * 1.5),
                        &format!("REV {}", doc.revision_number(rev)),
                        3.0 * k,
                        Anchor::BottomLeft,
                        None,
                    );
                }
                text(
                    dl,
                    DVec2::new(max.x - pad, y + pad * 1.5),
                    &def.number,
                    13.0 * k,
                    Anchor::BottomRight,
                    None,
                );
                dl.line(DVec2::new(mid, y), DVec2::new(mid, top), THIN, None);
                let half = y + h * k / 2.0;
                dl.line(DVec2::new(x0, half), DVec2::new(mid, half), THIN, None);
                label(dl, x0, top, "SCALE");
                value(dl, x0, top, &sheet_scale(doc, def), 3.0 * k);
                label(dl, x0, half, "DATE");
                value(dl, x0, half, &def.issue_date, 3.0 * k);
            }
            "drawn" => {
                let half = x0 + w / 2.0;
                dl.line(DVec2::new(half, y), DVec2::new(half, top), THIN, None);
                label(dl, x0, top, "DRAWN");
                value(dl, x0, top, &def.drawn_by, 3.0 * k);
                label(dl, half, top, "CHECKED");
                value(dl, half, top, &def.checked_by, 3.0 * k);
            }
            "sheet" => {
                label(dl, x0, top, "SHEET");
                value(dl, x0, top, &name.to_uppercase(), 6.0 * k);
            }
            "project" => {
                label(dl, x0, top, "PROJECT");
                value(dl, x0, top, &info.name, 5.0 * k);
                let small = 2.8 * k;
                let mut ty = top - pad - label_h - 2.0 * k - 5.0 * k - 3.0 * k;
                for line in [info.address.as_str(), &format!("Project No. {}", info.number)] {
                    if !line.trim().is_empty() && !line.ends_with("No. ") {
                        text(dl, DVec2::new(x0 + pad, ty - small), line, small, Anchor::BottomLeft, None);
                        ty -= small * 1.6;
                    }
                }
            }
            "client" => {
                label(dl, x0, top, "CLIENT");
                value(dl, x0, top, &info.client, 3.5 * k);
            }
            _ => {
                label(dl, x0, top, "STATUS");
                value(dl, x0, top, &info.status, 3.5 * k);
            }
        }
        y = top;
        hline(dl, y, if row == "status" { MEDIUM } else { THIN });
    }
    // Revision schedule at the foot of the notes area: newest on top.
    if !revisions.is_empty() {
        let rh = 5.0 * k;
        let th = 2.2 * k;
        let (c1, c2) = (x0 + 14.0 * k, max.x - 30.0 * k);
        // Above the "Drawn with Hausit" credit.
        let y0 = y + pad + 3.0 * k;
        let mut ry = y0;
        let row = |dl: &mut DisplayList, ry: f64, cells: [&str; 3]| {
            for (x, s) in [x0, c1, c2].into_iter().zip(cells) {
                text(dl, DVec2::new(x + pad, ry + (rh - th) / 2.0), s, th, Anchor::BottomLeft, None);
            }
            dl.line(DVec2::new(x0, ry + rh), DVec2::new(max.x, ry + rh), THIN, None);
        };
        for r in revisions.iter().rev() {
            let Some(def) = doc.revision_def(*r) else { continue };
            row(dl, ry, [&doc.revision_number(*r), &def.description, &def.date]);
            ry += rh;
        }
        row(dl, ry, ["REV", "DESCRIPTION", "DATE"]);
        for x in [c1, c2] {
            dl.line(DVec2::new(x, y0), DVec2::new(x, ry + rh), THIN, None);
        }
        dl.line(DVec2::new(x0, y0), DVec2::new(max.x, y0), THIN, None);
    }
    label(dl, x0, max.y, "NOTES");
    text(
        dl,
        DVec2::new(max.x - pad, y + pad),
        "Drawn with Hausit",
        1.8 * k,
        Anchor::BottomRight,
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use hausit_model::template::{industrial_sample, sample_project};

    #[test]
    fn sample_sheets_draw_their_views() {
        for mut doc in [sample_project(), industrial_sample()] {
            add_sample_sheets(&mut doc);
            let sheets: Vec<(&View, &SheetDef)> = doc.sheets().collect();
            assert!(!sheets.is_empty());
            for (v, def) in sheets {
                let dl = crate::build(&doc, v);
                let paper = def.size.paper();
                for vp in &def.viewports {
                    assert!(dl.items.iter().any(|i| i.element == Some(vp.view)), "{} on {}", vp.view, v.name);
                }
                // Everything sits on the paper.
                let (min, max) = dl.bounds().unwrap();
                assert!(min.x >= -1e-6 && min.y >= -1e-6, "{} {min}", v.name);
                assert!(max.x <= paper.x + 1e-6 && max.y <= paper.y + 1e-6, "{} {max}", v.name);
                let texts: Vec<&str> = dl
                    .items
                    .iter()
                    .filter_map(|i| match &i.prim {
                        Prim::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                assert!(texts.contains(&def.number.as_str()));
            }
        }
    }
}
