//! Ribbon icons, drawn with vector shapes so they stay crisp at any DPI.

use egui::{pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Modify,
    Wall,
    Door,
    Window,
    Floor,
    Roof,
    Level,
    Grid,
    Section,
    Callout,
    ThreeD,
    Elevation,
    Schedule,
    Move,
    Copy,
    Delete,
    Open,
    Save,
    New,
    Undo,
    Redo,
    Info,
    Units,
    TypeProps,
    Finish,
    Cancel,
    Sheet,
    PlaceView,
    Pdf,
    Column,
    Room,
    Dimension,
    Import,
    Close,
    RoomSeparator,
    Rotate,
    Mirror,
    Trim,
    Split,
    Attach,
    Beam,
    Brace,
    Footing,
    WallFoundation,
    Stair,
    Railing,
    Ceiling,
    Component,
    LoadFamily,
    Text,
    Tag,
    Demolish,
    Align,
    Offset,
    Array,
    Pin,
    Paste,
    Phases,
}

const INK: Color32 = Color32::from_rgb(50, 60, 75);
const BLUE: Color32 = Color32::from_rgb(40, 110, 200);
const BRICK: Color32 = Color32::from_rgb(196, 120, 80);
const GREEN: Color32 = Color32::from_rgb(40, 150, 60);
const RED: Color32 = Color32::from_rgb(200, 50, 50);

/// Paints `icon` inside `r` (a square).
pub fn paint(p: &Painter, r: Rect, icon: Icon, enabled: bool) {
    let s = r.width() / 32.0;
    let at = |x: f32, y: f32| -> Pos2 { pos2(r.left() + x * s, r.top() + y * s) };
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(at(x0, y0), at(x1, y1));
    let k = |c: Color32| if enabled { c } else { Color32::from_gray(180) };
    let line = |w: f32, c: Color32| Stroke::new(w * s.max(0.6), k(c));
    let poly = |pts: &[(f32, f32)], fill: Color32, stroke: Stroke| {
        p.add(Shape::convex_polygon(
            pts.iter().map(|(x, y)| at(*x, *y)).collect(),
            k(fill),
            stroke,
        ));
    };
    let seg = |a: (f32, f32), b: (f32, f32), st: Stroke| {
        p.line_segment([at(a.0, a.1), at(b.0, b.1)], st);
    };
    let ink = line(1.6, INK);
    match icon {
        Icon::Modify => poly(
            &[
                (9.0, 4.0),
                (9.0, 26.0),
                (14.0, 21.0),
                (18.0, 29.0),
                (21.0, 27.5),
                (17.0, 20.0),
                (24.0, 20.0),
            ],
            Color32::WHITE,
            ink,
        ),
        Icon::Wall => {
            p.rect(rect(3.0, 9.0, 29.0, 25.0), 0.0, k(BRICK), ink, egui::StrokeKind::Middle);
            for y in [14.3, 19.6] {
                seg((3.0, y), (29.0, y), line(1.0, INK));
            }
            for (y0, y1, xs) in [(9.0, 14.3, [11.0, 20.0]), (14.3, 19.6, [7.0, 16.0]), (19.6, 25.0, [11.0, 20.0])] {
                for x in xs {
                    seg((x, y0), (x, y1), line(1.0, INK));
                }
            }
        }
        Icon::Door => {
            p.rect(
                rect(8.0, 3.0, 24.0, 29.0),
                0.0,
                k(Color32::from_rgb(200, 160, 110)),
                ink,
                egui::StrokeKind::Middle,
            );
            p.circle_filled(at(20.5, 16.0), 1.4 * s, k(INK));
        }
        Icon::Window => {
            p.rect(
                rect(5.0, 5.0, 27.0, 27.0),
                0.0,
                k(Color32::from_rgb(190, 225, 250)),
                ink,
                egui::StrokeKind::Middle,
            );
            seg((16.0, 5.0), (16.0, 27.0), ink);
            seg((5.0, 16.0), (27.0, 16.0), ink);
        }
        Icon::Floor => {
            poly(
                &[(3.0, 18.0), (12.0, 11.0), (29.0, 11.0), (20.0, 18.0)],
                Color32::from_rgb(200, 200, 196),
                ink,
            );
            poly(
                &[(3.0, 18.0), (20.0, 18.0), (20.0, 22.0), (3.0, 22.0)],
                Color32::from_rgb(160, 160, 156),
                ink,
            );
            poly(
                &[(20.0, 18.0), (29.0, 11.0), (29.0, 15.0), (20.0, 22.0)],
                Color32::from_rgb(140, 140, 136),
                ink,
            );
        }
        Icon::Roof => {
            poly(&[(2.0, 18.0), (16.0, 6.0), (30.0, 18.0)], Color32::from_rgb(190, 90, 70), ink);
            p.rect(
                rect(7.0, 18.0, 25.0, 28.0),
                0.0,
                k(Color32::from_rgb(235, 225, 210)),
                ink,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Level => {
            seg((2.0, 20.0), (22.0, 20.0), line(1.4, BLUE));
            p.circle(at(26.0, 20.0), 4.0 * s, k(Color32::WHITE), line(1.4, BLUE));
            poly(&[(26.0, 20.0), (30.0, 20.0), (26.0, 16.0)], BLUE, Stroke::NONE);
            p.text(
                at(4.0, 13.0),
                egui::Align2::LEFT_CENTER,
                "L2",
                egui::FontId::proportional(9.0 * s),
                k(INK),
            );
        }
        Icon::Grid => {
            p.add(Shape::dashed_line(
                &[at(16.0, 31.0), at(16.0, 14.0)],
                line(1.4, INK),
                4.0 * s,
                2.0 * s,
            ));
            p.circle(at(16.0, 8.0), 6.0 * s, k(Color32::WHITE), ink);
            p.text(
                at(16.0, 8.0),
                egui::Align2::CENTER_CENTER,
                "A",
                egui::FontId::proportional(8.0 * s),
                k(INK),
            );
        }
        Icon::Section => {
            p.add(Shape::dashed_line(
                &[at(4.0, 16.0), at(28.0, 16.0)],
                line(2.0, INK),
                4.0 * s,
                2.0 * s,
            ));
            poly(&[(4.0, 16.0), (10.0, 16.0), (7.0, 25.0)], INK, Stroke::NONE);
            poly(&[(22.0, 16.0), (28.0, 16.0), (25.0, 25.0)], INK, Stroke::NONE);
            p.circle(at(7.0, 10.0), 4.0 * s, k(Color32::WHITE), ink);
        }
        Icon::Callout => {
            p.rect_stroke(
                egui::Rect::from_min_max(at(4.0, 12.0), at(20.0, 28.0)),
                3.0 * s,
                line(1.6, INK),
                egui::StrokeKind::Middle,
            );
            p.line_segment([at(20.0, 14.0), at(24.0, 10.0)], line(1.0, INK));
            p.circle(at(26.0, 8.0), 4.0 * s, k(Color32::WHITE), ink);
        }
        Icon::ThreeD | Icon::Elevation => {
            poly(
                &[(16.0, 4.0), (28.0, 10.0), (16.0, 16.0), (4.0, 10.0)],
                Color32::from_rgb(200, 220, 245),
                ink,
            );
            poly(
                &[(4.0, 10.0), (16.0, 16.0), (16.0, 29.0), (4.0, 23.0)],
                Color32::from_rgb(140, 175, 220),
                ink,
            );
            poly(
                &[(16.0, 16.0), (28.0, 10.0), (28.0, 23.0), (16.0, 29.0)],
                Color32::from_rgb(100, 140, 200),
                ink,
            );
            if icon == Icon::Elevation {
                p.circle(at(26.0, 26.0), 5.0 * s, k(Color32::WHITE), ink);
            }
        }
        Icon::Schedule => {
            p.rect(rect(4.0, 5.0, 28.0, 27.0), 0.0, k(Color32::WHITE), ink, egui::StrokeKind::Middle);
            p.rect_filled(rect(4.0, 5.0, 28.0, 10.0), 0.0, k(Color32::from_rgb(200, 215, 235)));
            for y in [10.0, 15.7, 21.3] {
                seg((4.0, y), (28.0, y), line(1.0, INK));
            }
            seg((13.0, 5.0), (13.0, 27.0), line(1.0, INK));
        }
        Icon::Move => {
            let st = line(1.8, INK);
            seg((16.0, 4.0), (16.0, 28.0), st);
            seg((4.0, 16.0), (28.0, 16.0), st);
            for (a, b, c) in [
                ((16.0, 3.0), (12.0, 8.0), (20.0, 8.0)),
                ((16.0, 29.0), (12.0, 24.0), (20.0, 24.0)),
                ((3.0, 16.0), (8.0, 12.0), (8.0, 20.0)),
                ((29.0, 16.0), (24.0, 12.0), (24.0, 20.0)),
            ] {
                poly(&[a, b, c], INK, Stroke::NONE);
            }
        }
        Icon::Copy => {
            p.rect(rect(4.0, 4.0, 20.0, 20.0), 0.0, k(Color32::WHITE), ink, egui::StrokeKind::Middle);
            p.rect(
                rect(12.0, 12.0, 28.0, 28.0),
                0.0,
                k(Color32::from_rgb(200, 220, 245)),
                ink,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Delete | Icon::Cancel | Icon::Close => {
            let st = line(3.0, RED);
            seg((7.0, 7.0), (25.0, 25.0), st);
            seg((25.0, 7.0), (7.0, 25.0), st);
        }
        Icon::Finish => {
            let st = line(3.2, GREEN);
            seg((5.0, 17.0), (13.0, 25.0), st);
            seg((13.0, 25.0), (27.0, 7.0), st);
        }
        Icon::Open => {
            poly(
                &[(3.0, 9.0), (12.0, 9.0), (14.0, 12.0), (29.0, 12.0), (29.0, 26.0), (3.0, 26.0)],
                Color32::from_rgb(240, 200, 90),
                ink,
            );
        }
        Icon::Save => {
            p.rect(rect(4.0, 4.0, 28.0, 28.0), 2.0, k(BLUE), ink, egui::StrokeKind::Middle);
            p.rect_filled(rect(9.0, 4.0, 23.0, 12.0), 0.0, k(Color32::WHITE));
            p.rect_filled(rect(8.0, 18.0, 24.0, 28.0), 0.0, k(Color32::from_gray(230)));
        }
        Icon::New => {
            poly(
                &[(7.0, 3.0), (20.0, 3.0), (26.0, 9.0), (26.0, 29.0), (7.0, 29.0)],
                Color32::WHITE,
                ink,
            );
            seg((20.0, 3.0), (20.0, 9.0), ink);
            seg((20.0, 9.0), (26.0, 9.0), ink);
        }
        Icon::Undo | Icon::Redo => {
            let flip = if icon == Icon::Redo { -1.0 } else { 1.0 };
            let x = |v: f32| if flip < 0.0 { 32.0 - v } else { v };
            let pts: Vec<Pos2> = (0..=12)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.0 + i as f32 / 12.0);
                    at(x(16.0 - 9.0 * a.cos()), 18.0 + 8.0 * a.sin())
                })
                .collect();
            p.add(Shape::line(pts, line(2.2, BLUE)));
            poly(&[(x(3.0), 18.0), (x(11.0), 14.0), (x(11.0), 22.0)], BLUE, Stroke::NONE);
        }
        Icon::Info => {
            p.circle(at(16.0, 16.0), 12.0 * s, k(BLUE), Stroke::NONE);
            p.text(
                at(16.0, 16.5),
                egui::Align2::CENTER_CENTER,
                "i",
                egui::FontId::proportional(16.0 * s),
                Color32::WHITE,
            );
        }
        Icon::Units => {
            p.rect(
                rect(3.0, 11.0, 29.0, 21.0),
                0.0,
                k(Color32::from_rgb(245, 215, 120)),
                ink,
                egui::StrokeKind::Middle,
            );
            for i in 0..6 {
                let xx = 6.0 + i as f32 * 4.0;
                seg((xx, 11.0), (xx, if i % 2 == 0 { 17.0 } else { 15.0 }), line(1.0, INK));
            }
        }
        Icon::TypeProps => {
            p.rect(rect(5.0, 4.0, 27.0, 28.0), 0.0, k(Color32::WHITE), ink, egui::StrokeKind::Middle);
            for y in [10.0, 15.0, 20.0] {
                seg((9.0, y), (23.0, y), line(1.2, BLUE));
            }
        }
        Icon::Column => {
            p.rect(
                rect(11.0, 4.0, 21.0, 28.0),
                0.0,
                k(Color32::from_gray(200)),
                ink,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Component => {
            // A chair, side on.
            let wood = k(Color32::from_rgb(196, 150, 100));
            p.rect(rect(8.0, 16.0, 24.0, 19.0), 0.0, wood, ink, egui::StrokeKind::Middle);
            p.rect(rect(20.0, 4.0, 23.0, 16.0), 0.0, wood, ink, egui::StrokeKind::Middle);
            seg((9.5, 19.0), (9.5, 28.0), ink);
            seg((22.5, 19.0), (22.5, 28.0), ink);
        }
        Icon::LoadFamily => {
            // A box with an arrow into it.
            p.rect(
                rect(6.0, 14.0, 26.0, 28.0),
                0.0,
                k(Color32::from_rgb(230, 240, 255)),
                ink,
                egui::StrokeKind::Middle,
            );
            let blue = line(2.0, BLUE);
            seg((16.0, 3.0), (16.0, 20.0), blue);
            seg((11.0, 15.0), (16.0, 20.0), blue);
            seg((21.0, 15.0), (16.0, 20.0), blue);
        }
        Icon::Room => {
            p.rect(
                rect(4.0, 6.0, 28.0, 26.0),
                0.0,
                k(Color32::from_rgb(230, 240, 255)),
                ink,
                egui::StrokeKind::Middle,
            );
            seg((4.0, 6.0), (28.0, 26.0), line(1.0, BLUE));
            seg((28.0, 6.0), (4.0, 26.0), line(1.0, BLUE));
        }
        Icon::Dimension => {
            seg((4.0, 16.0), (28.0, 16.0), ink);
            seg((4.0, 10.0), (4.0, 22.0), ink);
            seg((28.0, 10.0), (28.0, 22.0), ink);
            p.text(
                at(16.0, 11.0),
                egui::Align2::CENTER_CENTER,
                "3000",
                egui::FontId::proportional(7.0 * s),
                k(INK),
            );
        }
        Icon::Text => {
            p.text(
                at(13.0, 15.0),
                egui::Align2::CENTER_CENTER,
                "A",
                egui::FontId::proportional(20.0 * s),
                k(INK),
            );
            seg((22.0, 8.0), (29.0, 8.0), line(1.4, BLUE));
            seg((22.0, 14.0), (29.0, 14.0), line(1.4, BLUE));
            seg((22.0, 20.0), (29.0, 20.0), line(1.4, BLUE));
        }
        Icon::Tag => {
            p.rect(rect(3.0, 16.0, 13.0, 26.0), 0.0, k(Color32::WHITE), ink, egui::StrokeKind::Middle);
            seg((13.0, 21.0), (20.0, 14.0), ink);
            p.rect(
                rect(17.0, 5.0, 30.0, 14.0),
                0.0,
                k(Color32::WHITE),
                line(1.4, BLUE),
                egui::StrokeKind::Middle,
            );
            p.text(
                at(23.5, 9.5),
                egui::Align2::CENTER_CENTER,
                "D1",
                egui::FontId::proportional(6.5 * s),
                k(INK),
            );
        }
        Icon::Align => {
            seg((6.0, 3.0), (6.0, 29.0), line(1.4, BLUE));
            p.rect(
                rect(10.0, 8.0, 26.0, 24.0),
                0.0,
                k(Color32::from_gray(225)),
                ink,
                egui::StrokeKind::Middle,
            );
            seg((20.0, 16.0), (8.0, 16.0), line(1.6, RED));
            poly(&[(8.0, 16.0), (12.0, 13.0), (12.0, 19.0)], RED, Stroke::NONE);
        }
        Icon::Offset => {
            seg((5.0, 26.0), (27.0, 26.0), line(3.0, INK));
            p.add(Shape::dashed_line(
                &[at(5.0, 12.0), at(27.0, 12.0)],
                line(2.0, BLUE),
                3.0 * s,
                2.0 * s,
            ));
            seg((16.0, 24.0), (16.0, 14.0), line(1.2, RED));
            poly(&[(16.0, 13.0), (13.0, 17.0), (19.0, 17.0)], RED, Stroke::NONE);
        }
        Icon::Array => {
            for x in [4.0, 13.0, 22.0] {
                p.rect(
                    rect(x, 11.0, x + 6.0, 21.0),
                    0.0,
                    k(if x == 4.0 { BRICK } else { Color32::from_gray(220) }),
                    ink,
                    egui::StrokeKind::Middle,
                );
            }
            seg((7.0, 26.0), (25.0, 26.0), line(1.2, BLUE));
        }
        Icon::Pin => {
            p.circle(at(16.0, 10.0), 6.0 * s, Color32::from_rgb(200, 60, 60), ink);
            seg((16.0, 16.0), (16.0, 29.0), line(2.0, INK));
        }
        Icon::Paste => {
            p.rect(
                rect(7.0, 5.0, 25.0, 29.0),
                2.0 * s,
                k(Color32::from_rgb(196, 160, 110)),
                ink,
                egui::StrokeKind::Middle,
            );
            p.rect(rect(11.0, 10.0, 27.0, 28.0), 0.0, k(Color32::WHITE), ink, egui::StrokeKind::Middle);
            p.rect(
                rect(12.0, 3.0, 20.0, 7.0),
                1.0 * s,
                k(Color32::from_gray(120)),
                ink,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Demolish => {
            // A hammer over a broken wall.
            p.rect(rect(3.0, 18.0, 29.0, 29.0), 0.0, k(BRICK), ink, egui::StrokeKind::Middle);
            seg((14.0, 18.0), (17.0, 23.0), line(1.4, Color32::WHITE));
            seg((17.0, 23.0), (15.0, 29.0), line(1.4, Color32::WHITE));
            seg((8.0, 15.0), (22.0, 4.0), line(2.6, Color32::from_rgb(140, 100, 60)));
            poly(&[(18.0, 3.0), (24.0, 0.5), (28.0, 6.0), (22.0, 9.0)], Color32::from_gray(110), ink);
        }
        Icon::Phases => {
            // Three stages in time: existing grey, demolished dashed, new.
            p.rect(
                rect(3.0, 10.0, 11.0, 26.0),
                0.0,
                k(Color32::from_gray(190)),
                ink,
                egui::StrokeKind::Middle,
            );
            p.add(Shape::dashed_line(
                &[at(13.0, 26.0), at(13.0, 10.0), at(19.0, 10.0), at(19.0, 26.0)],
                line(1.2, INK),
                2.0 * s,
                2.0 * s,
            ));
            p.rect(rect(21.0, 6.0, 29.0, 26.0), 0.0, k(BRICK), ink, egui::StrokeKind::Middle);
        }
        Icon::Import => {
            p.rect(rect(6.0, 12.0, 26.0, 28.0), 0.0, k(Color32::WHITE), ink, egui::StrokeKind::Middle);
            seg((16.0, 2.0), (16.0, 20.0), line(2.2, BLUE));
            poly(&[(11.0, 15.0), (21.0, 15.0), (16.0, 21.0)], BLUE, Stroke::NONE);
        }
        Icon::RoomSeparator => {
            p.rect(rect(4.0, 6.0, 28.0, 26.0), 0.0, k(Color32::WHITE), ink, egui::StrokeKind::Middle);
            p.add(Shape::dashed_line(
                &[at(16.0, 6.0), at(16.0, 26.0)],
                line(1.6, BLUE),
                3.0 * s,
                2.0 * s,
            ));
        }
        Icon::Rotate => {
            let c = at(16.0, 17.0);
            let pts: Vec<Pos2> = (0..=20)
                .map(|i| {
                    let a = std::f32::consts::PI * (0.1 + 1.4 * i as f32 / 20.0);
                    c + vec2(a.cos(), -a.sin()) * 10.0 * s
                })
                .collect();
            let end = *pts.last().unwrap();
            p.add(Shape::line(pts, line(1.8, INK)));
            p.add(Shape::convex_polygon(
                vec![end + vec2(-4.0, -3.0) * s, end + vec2(4.0, -3.0) * s, end + vec2(0.0, 3.0) * s],
                k(INK),
                Stroke::NONE,
            ));
            p.circle_filled(c, 2.0 * s, k(BLUE));
        }
        Icon::Mirror => {
            p.add(Shape::dashed_line(
                &[at(16.0, 3.0), at(16.0, 29.0)],
                line(1.2, BLUE),
                3.0 * s,
                2.0 * s,
            ));
            poly(&[(4.0, 24.0), (13.0, 24.0), (13.0, 8.0)], Color32::WHITE, ink);
            poly(&[(19.0, 8.0), (19.0, 24.0), (28.0, 24.0)], Color32::from_rgb(200, 220, 245), ink);
        }
        Icon::Trim => {
            seg((6.0, 26.0), (6.0, 8.0), line(3.0, INK));
            seg((6.0, 8.0), (26.0, 8.0), line(3.0, INK));
            seg((6.0, 8.0), (6.0, 3.0), line(1.2, RED));
            seg((6.0, 8.0), (2.0, 8.0), line(1.2, RED));
        }
        Icon::Split => {
            seg((3.0, 16.0), (13.0, 16.0), line(3.0, INK));
            seg((19.0, 16.0), (29.0, 16.0), line(3.0, INK));
            seg((18.0, 6.0), (14.0, 26.0), line(1.4, RED));
        }
        Icon::Attach => {
            poly(&[(3.0, 12.0), (16.0, 3.0), (29.0, 12.0)], BRICK, ink);
            p.rect(
                rect(8.0, 12.0, 24.0, 29.0),
                0.0,
                k(Color32::from_gray(220)),
                ink,
                egui::StrokeKind::Middle,
            );
            seg((16.0, 26.0), (16.0, 15.0), line(1.8, BLUE));
            seg((16.0, 15.0), (13.0, 19.0), line(1.8, BLUE));
            seg((16.0, 15.0), (19.0, 19.0), line(1.8, BLUE));
        }
        Icon::Beam => {
            // An I-beam in perspective.
            let steel = Color32::from_rgb(150, 160, 172);
            poly(&[(3.0, 12.0), (23.0, 6.0), (29.0, 8.0), (9.0, 14.0)], steel, ink);
            poly(&[(5.0, 21.0), (25.0, 15.0), (25.0, 17.0), (5.0, 23.0)], steel, ink);
            poly(
                &[(6.0, 13.5), (26.0, 7.5), (26.0, 15.5), (6.0, 21.5)],
                Color32::from_rgb(120, 130, 142),
                ink,
            );
        }
        Icon::Brace => {
            seg((5.0, 4.0), (5.0, 28.0), line(2.5, INK));
            seg((27.0, 4.0), (27.0, 28.0), line(2.5, INK));
            seg((5.0, 27.0), (27.0, 5.0), line(2.2, Color32::from_rgb(120, 130, 142)));
            seg((5.0, 5.0), (27.0, 27.0), line(2.2, Color32::from_rgb(120, 130, 142)));
        }
        Icon::Footing => {
            p.rect(
                rect(13.0, 3.0, 19.0, 17.0),
                0.0,
                k(Color32::from_gray(200)),
                ink,
                egui::StrokeKind::Middle,
            );
            poly(
                &[(4.0, 20.0), (10.0, 16.0), (28.0, 16.0), (22.0, 20.0)],
                Color32::from_rgb(205, 205, 200),
                ink,
            );
            poly(
                &[(4.0, 20.0), (22.0, 20.0), (22.0, 27.0), (4.0, 27.0)],
                Color32::from_rgb(175, 175, 170),
                ink,
            );
            poly(
                &[(22.0, 20.0), (28.0, 16.0), (28.0, 23.0), (22.0, 27.0)],
                Color32::from_rgb(150, 150, 146),
                ink,
            );
        }
        Icon::WallFoundation => {
            p.rect(rect(11.0, 3.0, 21.0, 19.0), 0.0, k(BRICK), ink, egui::StrokeKind::Middle);
            p.rect(
                rect(5.0, 19.0, 27.0, 27.0),
                0.0,
                k(Color32::from_rgb(185, 185, 180)),
                ink,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Stair => {
            let pts = [
                (4.0, 28.0),
                (4.0, 23.0),
                (10.0, 23.0),
                (10.0, 18.0),
                (16.0, 18.0),
                (16.0, 13.0),
                (22.0, 13.0),
                (22.0, 8.0),
                (28.0, 8.0),
                (28.0, 28.0),
            ];
            p.add(Shape::closed_line(pts.iter().map(|(x, y)| at(*x, *y)).collect(), ink));
            seg((4.0, 28.0), (28.0, 28.0), ink);
            seg((7.0, 19.0), (25.0, 4.0), line(1.6, BLUE));
        }
        Icon::Railing => {
            seg((3.0, 9.0), (29.0, 9.0), line(2.4, INK));
            seg((3.0, 27.0), (29.0, 27.0), line(1.2, INK));
            for x in [5.0, 11.0, 17.0, 23.0, 28.0] {
                seg((x, 9.0), (x, 27.0), line(1.2, BLUE));
            }
        }
        Icon::Ceiling => {
            poly(
                &[(3.0, 8.0), (12.0, 3.0), (29.0, 3.0), (20.0, 8.0)],
                Color32::from_rgb(235, 235, 230),
                ink,
            );
            for (a, b) in [((7.5, 5.5), (24.5, 5.5)), ((7.7, 3.0), (16.0, 8.0)), ((16.0, 3.0), (24.0, 8.0))] {
                seg(a, b, line(0.8, INK));
            }
            seg((6.0, 14.0), (6.0, 29.0), line(1.4, INK));
            seg((26.0, 10.0), (26.0, 29.0), line(1.4, INK));
            seg((16.0, 10.0), (16.0, 16.0), line(1.4, BLUE));
            seg((16.0, 10.0), (13.0, 13.0), line(1.4, BLUE));
            seg((16.0, 10.0), (19.0, 13.0), line(1.4, BLUE));
        }
        Icon::Sheet => {
            p.rect(rect(3.0, 7.0, 29.0, 25.0), 0.0, k(Color32::WHITE), ink, egui::StrokeKind::Middle);
            p.rect(
                rect(5.0, 9.0, 27.0, 23.0),
                0.0,
                Color32::TRANSPARENT,
                line(0.8, INK),
                egui::StrokeKind::Middle,
            );
            seg((21.0, 9.0), (21.0, 23.0), line(1.0, INK));
            for y in [15.0, 19.0] {
                seg((21.0, y), (27.0, y), line(0.8, INK));
            }
        }
        Icon::PlaceView => {
            p.rect(rect(3.0, 7.0, 29.0, 25.0), 0.0, k(Color32::WHITE), ink, egui::StrokeKind::Middle);
            p.rect(
                rect(7.0, 11.0, 19.0, 20.0),
                0.0,
                k(Color32::from_rgb(200, 220, 245)),
                line(1.0, BLUE),
                egui::StrokeKind::Middle,
            );
            seg((7.0, 22.5), (19.0, 22.5), line(1.4, INK));
        }
        Icon::Pdf => {
            poly(
                &[(7.0, 3.0), (21.0, 3.0), (26.0, 8.0), (26.0, 29.0), (7.0, 29.0)],
                Color32::WHITE,
                ink,
            );
            p.rect(
                rect(4.0, 14.0, 22.0, 22.0),
                1.0,
                k(Color32::from_rgb(200, 40, 40)),
                Stroke::NONE,
                egui::StrokeKind::Middle,
            );
            p.text(
                at(13.0, 18.0),
                egui::Align2::CENTER_CENTER,
                "PDF",
                egui::FontId::proportional(6.5 * s),
                Color32::WHITE,
            );
        }
    }
    let _ = vec2;
}
