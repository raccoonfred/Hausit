//! The default project template, modelled on Revit's metric
//! "Architectural Template": two levels, the standard views and a small
//! library of system and loadable family types.

use crate::category::Category;
use crate::document::Document;
use crate::element::*;
use crate::family::{default_type_params, TypeParams};
use crate::params::ParamValue;
use crate::view::{CardinalDirection, ViewKind};
use hausit_core::{DVec2, ElementId};

fn with(category: Category, values: &[(&str, ParamValue)]) -> TypeParams {
    let mut p = default_type_params(category);
    for (k, v) in values {
        p.insert((*k).to_string(), v.clone());
    }
    p
}

fn len(v: f64) -> ParamValue {
    ParamValue::Length(v)
}

fn text(s: &str) -> ParamValue {
    ParamValue::Text(s.into())
}

/// Adds the default type library to a document.
pub fn load_default_types(doc: &mut Document) {
    use Category::*;
    let wall = |w: f64, f: &str| with(Walls, &[("Width", len(w)), ("Function", text(f))]);
    doc.add_type(Walls, "Basic Wall", "Generic - 200mm", wall(200.0, "Exterior"));
    doc.add_type(Walls, "Basic Wall", "Generic - 300mm", wall(300.0, "Exterior"));
    doc.add_type(Walls, "Basic Wall", "Exterior - Brick on Block", wall(350.0, "Exterior"));
    doc.add_type(Walls, "Basic Wall", "Interior - 135mm Partition", wall(135.0, "Interior"));
    doc.add_type(Walls, "Basic Wall", "Interior - 100mm Partition", wall(100.0, "Interior"));

    doc.add_type(Floors, "Floor", "Generic 150mm", with(Floors, &[("Thickness", len(150.0))]));
    doc.add_type(Floors, "Floor", "Generic 300mm", with(Floors, &[("Thickness", len(300.0))]));
    doc.add_type(
        Floors,
        "Floor",
        "Concrete - Domestic 425mm",
        with(Floors, &[("Thickness", len(425.0))]),
    );

    doc.add_type(Roofs, "Basic Roof", "Generic - 400mm", with(Roofs, &[("Thickness", len(400.0))]));
    doc.add_type(Roofs, "Basic Roof", "Generic - 125mm", with(Roofs, &[("Thickness", len(125.0))]));

    let door = |w: f64, h: f64, leaves: i64| {
        with(
            Doors,
            &[("Width", len(w)), ("Height", len(h)), ("Leaves", ParamValue::Integer(leaves))],
        )
    };
    doc.add_type(Doors, "M_Single-Flush", "0864 x 2032mm", door(864.0, 2032.0, 1));
    doc.add_type(Doors, "M_Single-Flush", "0915 x 2134mm", door(915.0, 2134.0, 1));
    doc.add_type(Doors, "M_Double-Flush", "1730 x 2032mm", door(1730.0, 2032.0, 2));

    let window = |w: f64, h: f64, sill: f64| {
        with(
            Windows,
            &[("Width", len(w)), ("Height", len(h)), ("Default Sill Height", len(sill))],
        )
    };
    doc.add_type(Windows, "M_Fixed", "0915 x 1220mm", window(915.0, 1220.0, 915.0));
    doc.add_type(Windows, "M_Fixed", "0610 x 1830mm", window(610.0, 1830.0, 610.0));
    doc.add_type(Windows, "M_Casement", "1200 x 1500mm", window(1200.0, 1500.0, 800.0));

    let column = |b: f64, h: f64| with(StructuralColumns, &[("b", len(b)), ("h", len(h))]);
    for (b, h) in [(300.0, 300.0), (300.0, 450.0), (450.0, 600.0)] {
        doc.add_type(
            StructuralColumns,
            "M_Concrete-Rectangular-Column",
            &format!("{b:.0} x {h:.0}mm"),
            column(b, h),
        );
    }
}

/// Creates a level together with its floor plan view.
pub fn add_level_with_plan(doc: &mut Document, name: &str, elevation: f64) -> crate::ElementId {
    let id = doc.add_element(ElementData::Level(Level {
        name: name.into(),
        elevation,
        building_story: true,
    }));
    doc.add_view(name, ViewKind::FloorPlan { level: id });
    id
}

/// A new, empty project like Revit's "Architectural Template".
pub fn new_project() -> Document {
    let mut doc = Document::empty();
    load_default_types(&mut doc);
    add_level_with_plan(&mut doc, "Level 1", 0.0);
    add_level_with_plan(&mut doc, "Level 2", 4000.0);
    doc.add_view("{3D}", ViewKind::ThreeD);
    for d in [
        CardinalDirection::East,
        CardinalDirection::North,
        CardinalDirection::South,
        CardinalDirection::West,
    ] {
        doc.add_view(d.label(), ViewKind::Elevation { direction: d });
    }
    doc
}

/// A small house, used as the "Sample project" and in tests.
pub fn sample_project() -> Document {
    let mut doc = new_project();
    doc.project_info.name = "Sample House".into();
    let levels = doc.levels();
    let (l1, l2) = (levels[0].0, levels[1].0);
    let t = |doc: &Document, c: Category, name: &str| doc.types_of(c).find(|t| t.name == name).unwrap().id;
    let ext = t(&doc, Category::Walls, "Generic - 300mm");
    let int = t(&doc, Category::Walls, "Interior - 135mm Partition");

    // Grids
    for (i, x) in [0.0, 6000.0, 12000.0].iter().enumerate() {
        doc.add_element(ElementData::Grid(Grid {
            name: (i + 1).to_string(),
            start: DVec2::new(*x, -2500.0),
            end: DVec2::new(*x, 10500.0),
        }));
    }
    for (i, y) in [0.0, 8000.0].iter().enumerate() {
        doc.add_element(ElementData::Grid(Grid {
            name: ((b'A' + i as u8) as char).to_string(),
            start: DVec2::new(-2500.0, *y),
            end: DVec2::new(14500.0, *y),
        }));
    }

    // Exterior walls, drawn clockwise so the exterior face is outside.
    let pts = [
        DVec2::new(0.0, 0.0),
        DVec2::new(0.0, 8000.0),
        DVec2::new(12000.0, 8000.0),
        DVec2::new(12000.0, 0.0),
    ];
    let mut walls = Vec::new();
    for i in 0..4 {
        walls.push(doc.add_element(ElementData::Wall(Wall {
            type_id: ext,
            start: pts[i],
            end: pts[(i + 1) % 4],
            base_level: l1,
            base_offset: 0.0,
            top: TopConstraint::UpToLevel { level: l2, offset: 0.0 },
            structural: false,
            room_bounding: true,
            attached_top: vec![],
        })));
    }
    let partition = doc.add_element(ElementData::Wall(Wall {
        type_id: int,
        start: DVec2::new(6000.0, 0.0),
        end: DVec2::new(6000.0, 8000.0),
        base_level: l1,
        base_offset: 0.0,
        top: TopConstraint::UpToLevel { level: l2, offset: 0.0 },
        structural: false,
        room_bounding: true,
        attached_top: vec![],
    }));

    for (name, x) in [("Living", 3000.0), ("Bedroom", 9000.0)] {
        let number = doc.next_room_number();
        doc.add_element(ElementData::Room(Room {
            level: l1,
            point: DVec2::new(x, 4000.0),
            name: name.into(),
            number,
            upper_limit: l2,
            limit_offset: 0.0,
        }));
    }

    let door = t(&doc, Category::Doors, "0915 x 2134mm");
    let door2 = t(&doc, Category::Doors, "0864 x 2032mm");
    let win = t(&doc, Category::Windows, "1200 x 1500mm");
    let opening = |type_id, host, offset, sill| HostedOpening {
        type_id,
        host,
        offset,
        sill_height: sill,
        flip_hand: false,
        flip_facing: false,
    };
    // South wall runs east→west (walls[3]: 12000,0 → 0,0).
    doc.add_element(ElementData::Door(opening(door, walls[3], 9000.0, 0.0)));
    doc.add_element(ElementData::Door(opening(door2, partition, 5000.0, 0.0)));
    doc.add_element(ElementData::Window(opening(win, walls[3], 3000.0, 800.0)));
    doc.add_element(ElementData::Window(opening(win, walls[1], 3000.0, 800.0)));
    doc.add_element(ElementData::Window(opening(win, walls[1], 9000.0, 800.0)));
    doc.add_element(ElementData::Window(opening(win, walls[0], 4000.0, 800.0)));
    doc.add_element(ElementData::Window(opening(win, walls[2], 4000.0, 800.0)));

    let floor_t = t(&doc, Category::Floors, "Generic 300mm");
    let outer = vec![
        DVec2::new(-150.0, -150.0),
        DVec2::new(12150.0, -150.0),
        DVec2::new(12150.0, 8150.0),
        DVec2::new(-150.0, 8150.0),
    ];
    doc.add_element(ElementData::Floor(Floor {
        type_id: floor_t,
        level: l1,
        boundary: outer.clone(),
        height_offset: 0.0,
        room_bounding: true,
        structural: false,
    }));

    let roof_t = t(&doc, Category::Roofs, "Generic - 400mm");
    let eaves = vec![
        DVec2::new(-750.0, -750.0),
        DVec2::new(12750.0, -750.0),
        DVec2::new(12750.0, 8750.0),
        DVec2::new(-750.0, 8750.0),
    ];
    let roof = doc.add_element(ElementData::Roof(Roof {
        type_id: roof_t,
        level: l2,
        footprint: eaves,
        base_offset: 0.0,
        slope: 30.0,
        shape: RoofShape::Hip,
        defines_slope: vec![],
    }));
    // Exterior walls are attached to the roof, as Revit offers when the
    // roof is created.
    for w in &walls {
        if let Some(ElementData::Wall(w)) = doc.element_mut(*w).map(|e| &mut e.data) {
            w.attached_top = vec![roof];
        }
    }

    if let Some(t) = doc.family_type_mut(win) {
        t.params.insert("Type Mark".into(), ParamValue::Text("W1".into()));
    }

    // Dimensions in the Level 1 plan: grid spacing and overall size.
    let plan = doc
        .views()
        .find(|v| matches!(v.kind, ViewKind::FloorPlan { level } if level == l1))
        .map(|v| v.id)
        .unwrap();
    let grids: Vec<ElementId> = doc.elements_of(Category::Grids).map(|e| e.id).collect();
    let dims = [
        (
            vec![
                DimRef::Grid { grid: grids[0] },
                DimRef::Grid { grid: grids[1] },
                DimRef::Grid { grid: grids[2] },
            ],
            DVec2::new(0.0, -1500.0),
        ),
        (
            vec![DimRef::Wall { wall: walls[0], side: 1 }, DimRef::Wall { wall: walls[2], side: 1 }],
            DVec2::new(0.0, -2100.0),
        ),
        (
            vec![DimRef::Grid { grid: grids[3] }, DimRef::Grid { grid: grids[4] }],
            DVec2::new(13600.0, 0.0),
        ),
    ];
    for (refs, line_point) in dims {
        doc.add_element(ElementData::Dimension(Dimension {
            view: plan,
            refs,
            line_point,
        }));
    }

    doc.add_view(
        &doc.unique_view_name("Section"),
        ViewKind::Section {
            start: DVec2::new(3000.0, -3000.0),
            end: DVec2::new(3000.0, 11000.0),
            far_clip: 15000.0,
        },
    );
    doc.add_view("Door Schedule", ViewKind::Schedule(crate::ScheduleDef::new(Category::Doors)));
    doc.add_view("Wall Schedule", ViewKind::Schedule(crate::ScheduleDef::new(Category::Walls)));
    doc.add_view("Room Schedule", ViewKind::Schedule(crate::ScheduleDef::new(Category::Rooms)));
    doc
}
