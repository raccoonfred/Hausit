//! Performance check: a model of about 10,000 elements (a grid of rooms
//! with doors and windows) must regenerate its views quickly.

use hausit_core::DVec2;
use hausit_model::element::*;
use hausit_model::template::new_project;
use hausit_model::{Category, Document, ElementData, ViewKind};
use std::time::Instant;

fn grid_building(n: usize) -> Document {
    let mut doc = new_project();
    let levels = doc.levels();
    let (l1, l2) = (levels[0].0, levels[1].0);
    let wall_t = doc.types_of(Category::Walls).next().unwrap().id;
    let door_t = doc.types_of(Category::Doors).next().unwrap().id;
    let win_t = doc.types_of(Category::Windows).next().unwrap().id;
    let s = 4000.0;
    let add_wall = |doc: &mut Document, a: DVec2, b: DVec2| {
        doc.add_element(ElementData::Wall(Wall {
            type_id: wall_t,
            start: a,
            end: b,
            base_level: l1,
            base_offset: 0.0,
            top: TopConstraint::UpToLevel { level: l2, offset: 0.0 },
            structural: false,
            room_bounding: true,
            attached_top: vec![],
            disallow_join: [false; 2],
            arc_mid: None,
        }))
    };
    let mut walls = Vec::new();
    for i in 0..=n {
        for j in 0..n {
            let x = i as f64 * s;
            let (y0, y1) = (j as f64 * s, (j + 1) as f64 * s);
            walls.push(add_wall(&mut doc, DVec2::new(x, y0), DVec2::new(x, y1)));
            walls.push(add_wall(&mut doc, DVec2::new(y0, x), DVec2::new(y1, x)));
        }
    }
    for (k, w) in walls.iter().enumerate() {
        let o = HostedOpening {
            type_id: if k % 2 == 0 { door_t } else { win_t },
            host: *w,
            offset: 2000.0,
            sill_height: if k % 2 == 0 { 0.0 } else { 900.0 },
            flip_hand: false,
            flip_facing: false,
        };
        doc.add_element(if k % 2 == 0 { ElementData::Door(o) } else { ElementData::Window(o) });
    }
    doc
}

#[test]
fn ten_thousand_elements() {
    let t = Instant::now();
    let doc = grid_building(50);
    let count = doc.elements().count();
    assert!(count >= 10_000, "{count} elements");
    let built = t.elapsed();

    let plan = doc.views().find(|v| matches!(v.kind, ViewKind::FloorPlan { .. })).unwrap().clone();
    let t = Instant::now();
    let dl = hausit_views::build(&doc, &plan);
    let plan_time = t.elapsed();
    assert!(!dl.items.is_empty());

    let v3 = doc.views().find(|v| matches!(v.kind, ViewKind::ThreeD)).unwrap().clone();
    let t = Instant::now();
    let dl = hausit_views::build(&doc, &v3);
    let three_d = t.elapsed();
    assert!(!dl.items.is_empty());
    eprintln!("{count} elements: model {built:?}, plan {plan_time:?}, 3D {three_d:?}");
    // Generous limits so slow CI machines pass; they catch quadratic blow-ups.
    assert!(plan_time.as_secs_f64() < 3.0, "plan took {plan_time:?}");
    assert!(three_d.as_secs_f64() < 15.0, "3D took {three_d:?}");
}
