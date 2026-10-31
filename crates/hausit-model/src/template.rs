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
    use crate::materials::{Layer, LayerFunction as F};
    use Category::*;
    let wall = |w: f64, f: &str| with(Walls, &[("Width", len(w)), ("Function", text(f))]);
    // A compound type: the layers set its width or thickness.
    let layered = |doc: &mut Document, cat: Category, family: &str, name: &str, params: TypeParams, layers: Vec<Layer>| {
        let id = doc.add_type(cat, family, name, params);
        if let Some(t) = doc.family_type_mut(id) {
            t.set_layers(layers);
        }
        id
    };
    let l = Layer::new;
    doc.add_type(Walls, "Basic Wall", "Generic - 200mm", wall(200.0, "Exterior"));
    doc.add_type(Walls, "Basic Wall", "Generic - 300mm", wall(300.0, "Exterior"));
    layered(
        doc,
        Walls,
        "Basic Wall",
        "Exterior - Brick on Block",
        wall(350.0, "Exterior"),
        vec![
            l(F::Finish1, "Masonry - Brick", 102.5),
            l(F::Thermal, "Air Gap", 50.0),
            l(F::Thermal, "Insulation - Rigid Board", 50.0),
            l(F::Structure, "Masonry - Concrete Block", 140.0),
            l(F::Finish2, "Plaster", 7.5),
        ],
    );
    layered(
        doc,
        Walls,
        "Basic Wall",
        "Exterior - Rendered Concrete 310mm",
        wall(310.0, "Exterior"),
        vec![
            l(F::Finish1, "Render", 10.0),
            l(F::Thermal, "Insulation - Rigid Board", 100.0),
            l(F::Structure, "Concrete, Cast-in-Place", 200.0),
        ],
    );
    layered(
        doc,
        Walls,
        "Basic Wall",
        "Exterior - Insulated Metal Panel 150mm",
        wall(150.0, "Exterior"),
        vec![
            l(F::Finish1, "Metal Panel - Steel", 2.0),
            l(F::Thermal, "Insulation - Mineral Wool", 146.0),
            l(F::Finish2, "Metal Panel - Steel", 2.0),
        ],
    );
    layered(
        doc,
        Walls,
        "Basic Wall",
        "Interior - 135mm Partition",
        wall(135.0, "Interior"),
        vec![
            l(F::Finish1, "Gypsum Board", 12.5),
            l(F::Structure, "Insulation - Mineral Wool", 110.0),
            l(F::Finish2, "Gypsum Board", 12.5),
        ],
    );
    layered(
        doc,
        Walls,
        "Basic Wall",
        "Interior - 100mm Partition",
        wall(100.0, "Interior"),
        vec![
            l(F::Finish1, "Gypsum Board", 12.5),
            l(F::Structure, "Insulation - Mineral Wool", 75.0),
            l(F::Finish2, "Gypsum Board", 12.5),
        ],
    );
    layered(
        doc,
        Walls,
        "Basic Wall",
        "Interior - 200mm Blockwork",
        wall(200.0, "Interior"),
        vec![l(F::Structure, "Masonry - Concrete Block", 200.0)],
    );

    use crate::curtain::{curtain_type_params, CURTAIN_WALL};
    doc.add_type(Walls, CURTAIN_WALL, "Glazing - No Grid", curtain_type_params(0.0, 0.0, 0.0));
    doc.add_type(
        Walls,
        CURTAIN_WALL,
        "Storefront - 1500 Grid",
        curtain_type_params(1500.0, 2200.0, 50.0),
    );

    doc.add_type(Floors, "Floor", "Generic 150mm", with(Floors, &[("Thickness", len(150.0))]));
    doc.add_type(Floors, "Floor", "Generic 300mm", with(Floors, &[("Thickness", len(300.0))]));
    layered(
        doc,
        Floors,
        "Floor",
        "Concrete - Domestic 425mm",
        with(Floors, &[("Thickness", len(425.0))]),
        vec![
            l(F::Finish1, "Screed", 65.0),
            l(F::Thermal, "Insulation - Rigid Board", 100.0),
            l(F::Structure, "Concrete, Cast-in-Place", 260.0),
        ],
    );
    layered(
        doc,
        Floors,
        "Floor",
        "Concrete Slab - Industrial 250mm",
        with(Floors, &[("Thickness", len(250.0))]),
        vec![l(F::Structure, "Concrete, Cast-in-Place", 250.0)],
    );

    doc.add_type(Roofs, "Basic Roof", "Generic - 400mm", with(Roofs, &[("Thickness", len(400.0))]));
    doc.add_type(Roofs, "Basic Roof", "Generic - 125mm", with(Roofs, &[("Thickness", len(125.0))]));
    layered(
        doc,
        Roofs,
        "Basic Roof",
        "Tiled - Insulated Timber Rafters 300mm",
        with(Roofs, &[("Thickness", len(300.0))]),
        vec![
            l(F::Finish1, "Roof Tile - Clay", 50.0),
            l(F::Substrate, "Timber - Softwood", 25.0),
            l(F::Membrane, "Membrane", 5.0),
            l(F::Structure, "Insulation - Mineral Wool", 205.0),
            l(F::Finish2, "Gypsum Board", 15.0),
        ],
    );
    layered(
        doc,
        Roofs,
        "Basic Roof",
        "Insulated Metal Sheet 200mm",
        with(Roofs, &[("Thickness", len(200.0))]),
        vec![
            l(F::Finish1, "Metal Panel - Steel", 2.0),
            l(F::Thermal, "Insulation - Mineral Wool", 196.0),
            l(F::Finish2, "Metal Panel - Steel", 2.0),
        ],
    );

    // Doors and windows are loadable families from the library.
    for name in ["Door - Single Flush", "Door - Double Flush", "Window - Fixed", "Window - Casement"] {
        if let Some(def) = crate::library::family(name) {
            let _ = doc.load_family(def);
        }
    }

    let column = |b: f64, h: f64| with(StructuralColumns, &[("b", len(b)), ("h", len(h))]);
    for (b, h) in [(300.0, 300.0), (300.0, 450.0), (450.0, 600.0)] {
        doc.add_type(
            StructuralColumns,
            "Concrete Column - Rectangular",
            &format!("{b:.0} x {h:.0}mm"),
            column(b, h),
        );
    }

    // Steel I-sections: depth, flange width, web and flange thickness.
    let steel = |cat: Category, d: f64, bf: f64, tw: f64, tf: f64| {
        let mut p = with(cat, &[("d", len(d)), ("bf", len(bf)), ("tw", len(tw)), ("tf", len(tf))]);
        p.insert("Section Shape".into(), text("I"));
        p.insert("Structural Material".into(), text("Steel"));
        p
    };
    for (name, d, bf, tw, tf) in [
        ("IS 254x254", 260.3, 256.3, 10.3, 17.3),
        ("IS 305x305", 307.9, 305.3, 11.0, 18.0),
        ("IS 203x203", 203.2, 203.6, 7.2, 11.0),
    ] {
        doc.add_type(
            StructuralColumns,
            "Steel I-Section Column",
            name,
            steel(StructuralColumns, d, bf, tw, tf),
        );
    }
    for (name, d, bf, tw, tf) in [
        ("IS 203x133", 203.2, 133.2, 5.7, 7.8),
        ("IS 305x165", 303.4, 165.0, 6.0, 10.2),
        ("IS 406x178", 402.6, 177.7, 7.9, 10.9),
        ("IS 533x210", 528.3, 208.8, 9.6, 13.2),
    ] {
        doc.add_type(StructuralFraming, "Steel I-Section", name, steel(StructuralFraming, d, bf, tw, tf));
    }
    let rect = |b: f64, h: f64, material: &str| {
        let mut p = with(StructuralFraming, &[("b", len(b)), ("h", len(h))]);
        for k in ["d", "bf", "tw", "tf"] {
            p.shift_remove(k);
        }
        p.insert("Section Shape".into(), text("Rectangular"));
        p.insert("Structural Material".into(), text(material));
        p
    };
    doc.add_type(
        StructuralFraming,
        "Concrete Rectangular Beam",
        "300 x 600mm",
        rect(300.0, 600.0, "Concrete, Cast-in-Place"),
    );
    doc.add_type(
        StructuralFraming,
        "Concrete Rectangular Beam",
        "400 x 800mm",
        rect(400.0, 800.0, "Concrete, Cast-in-Place"),
    );
    doc.add_type(
        StructuralFraming,
        "Timber Rectangular Beam",
        "47 x 200mm",
        rect(47.0, 200.0, "Timber - Softwood"),
    );
    doc.add_type(
        StructuralFraming,
        "Timber Rectangular Beam",
        "75 x 225mm",
        rect(75.0, 225.0, "Timber - Softwood"),
    );
    let mut angle = with(StructuralFraming, &[("d", len(100.0)), ("bf", len(100.0)), ("tw", len(10.0))]);
    angle.shift_remove("tf");
    angle.insert("Section Shape".into(), text("Angle"));
    doc.add_type(StructuralFraming, "Steel Angle", "L 100x100x10", angle);

    let pad = |w: f64, l: f64, t: f64| {
        with(
            StructuralFoundations,
            &[("Width", len(w)), ("Length", len(l)), ("Thickness", len(t))],
        )
    };
    doc.add_type(
        StructuralFoundations,
        "Pad Footing",
        "1200 x 1200 x 500mm",
        pad(1200.0, 1200.0, 500.0),
    );
    doc.add_type(
        StructuralFoundations,
        "Pad Footing",
        "1800 x 1800 x 600mm",
        pad(1800.0, 1800.0, 600.0),
    );
    let strip = |w: f64, t: f64| {
        let mut p = with(StructuralFoundations, &[("Width", len(w)), ("Thickness", len(t))]);
        p.shift_remove("Length");
        p
    };
    let stair = |riser: f64, tread: f64, width: f64, depth: f64, landing: f64, monolithic: bool, material: &str| {
        let mut p = with(
            Stairs,
            &[
                ("Maximum Riser Height", len(riser)),
                ("Minimum Tread Depth", len(tread)),
                ("Minimum Run Width", len(width)),
                ("Structural Depth", len(depth)),
                ("Landing Thickness", len(landing)),
            ],
        );
        p.insert("Monolithic".into(), ParamValue::Bool(monolithic));
        p.insert("Material".into(), text(material));
        p
    };
    // Assembled stairs: nosing, closed risers, stringer width and depth.
    let assembled = |mut p: TypeParams, nosing: f64, risers: bool, sw: f64, sd: f64| {
        p.insert("Nosing Length".into(), len(nosing));
        p.insert("Closed Risers".into(), ParamValue::Bool(risers));
        p.insert("Stringer Width".into(), len(sw));
        p.insert("Stringer Depth".into(), len(sd));
        p
    };
    doc.add_type(
        Stairs,
        "Cast-In-Place Stair",
        "Concrete - 180 max riser 280 tread",
        stair(180.0, 280.0, 1000.0, 150.0, 200.0, true, "Concrete, Cast-in-Place"),
    );
    doc.add_type(
        Stairs,
        "Assembled Stair",
        "Timber - 220 max riser 220 tread",
        assembled(
            stair(220.0, 220.0, 900.0, 32.0, 50.0, false, "Timber - Softwood"),
            25.0,
            true,
            40.0,
            275.0,
        ),
    );
    doc.add_type(
        Stairs,
        "Assembled Stair",
        "Steel - 190 max riser 250 tread",
        assembled(stair(190.0, 250.0, 1000.0, 40.0, 60.0, false, "Steel"), 0.0, false, 15.0, 250.0),
    );
    let railing = |height: f64, spacing: f64, baluster: f64, rail: f64, mid: i64| {
        let mut p = with(
            Railings,
            &[
                ("Railing Height", len(height)),
                ("Baluster Spacing", len(spacing)),
                ("Baluster Size", len(baluster)),
                ("Rail Size", len(rail)),
            ],
        );
        p.insert("Intermediate Rails".into(), ParamValue::Integer(mid));
        p
    };
    doc.add_type(
        Railings,
        "Railing",
        "Handrail - 900mm Balusters",
        railing(900.0, 120.0, 25.0, 50.0, 0),
    );
    doc.add_type(
        Railings,
        "Railing",
        "Guardrail - 1100mm Industrial",
        railing(1100.0, 1500.0, 50.0, 48.0, 2),
    );

    let ceiling = |t: f64, grid: f64| with(Ceilings, &[("Thickness", len(t)), ("Grid Spacing", len(grid))]);
    layered(
        doc,
        Ceilings,
        "Compound Ceiling",
        "600 x 600mm Grid",
        ceiling(20.0, 600.0),
        vec![l(F::Finish1, "Ceiling Tile - Mineral Fibre", 20.0)],
    );
    layered(
        doc,
        Ceilings,
        "Compound Ceiling",
        "Plasterboard 12.5mm",
        ceiling(12.5, 0.0),
        vec![l(F::Finish1, "Gypsum Board", 12.5)],
    );

    let wf = crate::family::WALL_FOUNDATION;
    doc.add_type(StructuralFoundations, wf, "Bearing Footing - 600 x 300", strip(600.0, 300.0));
    doc.add_type(StructuralFoundations, wf, "Bearing Footing - 900 x 400", strip(900.0, 400.0));
}

/// Creates a level together with its floor plan view.
pub fn add_level_with_plan(doc: &mut Document, name: &str, elevation: f64) -> crate::ElementId {
    let id = doc.add_element(ElementData::Level(Level {
        name: name.into(),
        elevation,
        building_story: true,
    }));
    doc.add_view(name, ViewKind::FloorPlan { level: id });
    doc.add_view(name, ViewKind::CeilingPlan { level: id });
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
        doc.add_view(d.label(), ViewKind::Elevation { direction: d, at: None });
    }
    add_default_view_templates(&mut doc);
    doc
}

/// Revit-style starting view templates: an architectural plan, a
/// structural plan without furniture, fixtures and rooms, and a shaded 3D
/// presentation view.
pub fn add_default_view_templates(doc: &mut Document) {
    use crate::view::{CategoryGraphics, Graphics, ViewTemplate};
    let id = |doc: &mut Document, t: ViewTemplate| {
        doc.insert_view_template(t);
    };
    let hidden = CategoryGraphics {
        visible: false,
        ..Default::default()
    };
    let structural: Graphics = [
        Category::Furniture,
        Category::Casework,
        Category::PlumbingFixtures,
        Category::SpecialtyEquipment,
        Category::GenericModels,
        Category::Rooms,
        Category::RoomSeparation,
        Category::Ceilings,
    ]
    .into_iter()
    .map(|c| (c, hidden))
    .chain([(
        Category::Doors,
        CategoryGraphics {
            halftone: true,
            ..Default::default()
        },
    )])
    .collect();
    let base = ViewTemplate {
        id: ElementId::INVALID,
        name: String::new(),
        scale: Some(100),
        detail_level: Some(crate::DetailLevel::Medium),
        visual_style: Some(crate::VisualStyle::HiddenLine),
        cut_height: Some(1200.0),
        graphics: Some(Graphics::new()),
        phase_filter: Some(crate::PhaseFilter::ShowAll),
    };
    id(
        doc,
        ViewTemplate {
            name: "Architectural Plan".into(),
            ..base.clone()
        },
    );
    id(
        doc,
        ViewTemplate {
            name: "Structural Plan".into(),
            detail_level: Some(crate::DetailLevel::Coarse),
            graphics: Some(structural),
            ..base.clone()
        },
    );
    id(
        doc,
        ViewTemplate {
            name: "3D Presentation".into(),
            scale: None,
            detail_level: Some(crate::DetailLevel::Fine),
            visual_style: Some(crate::VisualStyle::Shaded),
            cut_height: None,
            graphics: None,
            ..base
        },
    );
}

/// A small house, used as the "Sample project" and in tests.
pub fn sample_project() -> Document {
    let mut doc = new_project();
    doc.project_info.name = "Sample House".into();
    let levels = doc.levels();
    let (l1, l2) = (levels[0].0, levels[1].0);
    let t = |doc: &Document, c: Category, name: &str| doc.types_of(c).find(|t| t.name == name).unwrap().id;
    let ext = t(&doc, Category::Walls, "Exterior - Brick on Block");
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
    // They run down past the 425 mm ground slab onto strip footings.
    let mut walls = Vec::new();
    for i in 0..4 {
        walls.push(doc.add_element(ElementData::Wall(Wall {
            type_id: ext,
            start: pts[i],
            end: pts[(i + 1) % 4],
            base_level: l1,
            base_offset: -425.0,
            top: TopConstraint::UpToLevel { level: l2, offset: 0.0 },
            structural: false,
            room_bounding: true,
            attached_top: vec![],
            disallow_join: [false; 2],
            arc_mid: None,
            curtain: Default::default(),
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
        disallow_join: [false; 2],
        arc_mid: None,
        curtain: Default::default(),
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

    let door = t(&doc, Category::Doors, "0926 x 2040mm");
    let door2 = t(&doc, Category::Doors, "0826 x 2040mm");
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

    let floor_t = t(&doc, Category::Floors, "Concrete - Domestic 425mm");
    // The slab spans between the walls' inner faces.
    let inner = vec![
        DVec2::new(175.0, 175.0),
        DVec2::new(11825.0, 175.0),
        DVec2::new(11825.0, 7825.0),
        DVec2::new(175.0, 7825.0),
    ];
    doc.add_element(ElementData::Floor(Floor {
        type_id: floor_t,
        level: l1,
        boundary: inner,
        height_offset: 0.0,
        room_bounding: true,
        structural: false,
    }));

    let roof_t = t(&doc, Category::Roofs, "Tiled - Insulated Timber Rafters 300mm");
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
        slope_arrow: None,
    }));
    // Exterior walls are attached to the roof, as Revit offers when the
    // roof is created.
    for w in &walls {
        if let Some(ElementData::Wall(w)) = doc.element_mut(*w).map(|e| &mut e.data) {
            w.attached_top = vec![roof];
        }
    }

    // Ceilings in both rooms, 2600 above the floor.
    let ceiling_t = t(&doc, Category::Ceilings, "600 x 600mm Grid");
    for (x0, x1) in [(175.0, 6000.0 - 67.5), (6000.0 + 67.5, 11825.0)] {
        doc.add_element(ElementData::Ceiling(Ceiling {
            type_id: ceiling_t,
            level: l1,
            boundary: vec![
                DVec2::new(x0, 175.0),
                DVec2::new(x1, 175.0),
                DVec2::new(x1, 7825.0),
                DVec2::new(x0, 7825.0),
            ],
            height_offset: 2600.0,
        }));
    }

    let strip = doc
        .types_of(Category::StructuralFoundations)
        .find(|t| t.is_wall_foundation())
        .unwrap()
        .id;
    for w in &walls {
        doc.add_element(ElementData::WallFooting(WallFooting { type_id: strip, wall: *w }));
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
            measure: None,
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

    // Kitchen, living and bedroom furniture from the family library.
    place_components(
        &mut doc,
        l1,
        &[
            ("Base Cabinet", "600mm", 475.0, 7825.0, 0.0),
            ("Sink Unit", "1000mm", 1275.0, 7825.0, 0.0),
            ("Base Cabinet", "600mm", 2075.0, 7825.0, 0.0),
            ("Wall Cabinet", "600mm", 475.0, 7825.0, 0.0),
            ("Wall Cabinet", "600mm", 2075.0, 7825.0, 0.0),
            ("Table - Rectangular", "1600 x 900mm", 3500.0, 5600.0, 0.0),
            ("Chair", "Standard", 3100.0, 6300.0, 0.0),
            ("Chair", "Standard", 3900.0, 6300.0, 0.0),
            ("Chair", "Standard", 3100.0, 4900.0, 180.0),
            ("Chair", "Standard", 3900.0, 4900.0, 180.0),
            ("Sofa", "3 Seat 2000mm", 175.0, 2400.0, 90.0),
            ("Shelving Unit", "900 x 1800mm", 5932.0, 2000.0, 270.0),
            ("Bed", "King 1600 x 2000mm", 6068.0, 2600.0, 90.0),
            ("Wardrobe", "1200mm 2 Door", 11825.0, 6800.0, 270.0),
            ("Desk", "1400 x 700mm", 10400.0, 7825.0, 0.0),
            ("Chair", "Standard", 10400.0, 6950.0, 180.0),
        ],
    );
    doc.add_view(
        "Furniture Schedule",
        ViewKind::Schedule(crate::ScheduleDef::new(Category::Furniture)),
    );
    crate::annotation::tag_openings_in_plans(&mut doc);
    doc
}

/// Places library components on a level, loading their families first:
/// (family, type, x, y, rotation in degrees).
pub fn place_components(doc: &mut Document, level: ElementId, items: &[(&str, &str, f64, f64, f64)]) {
    for (family, type_name, x, y, rotation) in items {
        let Some(def) = crate::library::family(family) else { continue };
        let category = def.category;
        if doc.family_def(category, family).is_none() && doc.load_family(def).is_err() {
            continue;
        }
        let Some(type_id) = doc
            .types_of(category)
            .find(|t| t.family == *family && t.name == *type_name)
            .map(|t| t.id)
        else {
            continue;
        };
        doc.add_element(ElementData::Component(Component {
            type_id,
            category,
            level,
            point: DVec2::new(*x, *y),
            rotation: *rotation,
            offset: 0.0,
        }));
    }
}

/// A small steel portal-frame shed: five 6 m bays spanning 15 m, with pad
/// footings, cross-bracing in the end bays, insulated panel walls and a
/// 6° insulated metal roof.
pub fn industrial_sample() -> Document {
    let mut doc = new_project();
    doc.project_info.name = "Sample Warehouse".into();
    let levels = doc.levels();
    // Level 1 is the slab; the template's Level 2 becomes the roof level
    // at the top of the steel (6 m eaves); the mezzanine is Level 2.
    let (l1, eaves) = (levels[0].0, levels[1].0);
    doc.rename_level_views(eaves, "Level 2", "Roof");
    if let Some(ElementData::Level(l)) = doc.element_mut(eaves).map(|e| &mut e.data) {
        l.name = "Roof".into();
        l.elevation = 6000.0;
    }
    // No ceilings up there: drop the roof level's ceiling plan.
    let rcp = doc
        .views()
        .find(|v| matches!(v.kind, ViewKind::CeilingPlan { level } if level == eaves))
        .map(|v| v.id);
    if let Some(v) = rcp {
        doc.delete_view(v);
    }
    let t = |doc: &Document, c: Category, name: &str| doc.types_of(c).find(|t| t.name == name).unwrap().id;
    let (bays, bay, span) = (5, 6000.0, 15000.0);
    let pitch = 6.0_f64.to_radians().tan();
    let rise = span / 2.0 * pitch;

    for i in 0..=bays {
        let x = i as f64 * bay;
        doc.add_element(ElementData::Grid(Grid {
            name: (i + 1).to_string(),
            start: DVec2::new(x, -2500.0),
            end: DVec2::new(x, span + 2500.0),
        }));
    }
    for (i, y) in [0.0, span].iter().enumerate() {
        doc.add_element(ElementData::Grid(Grid {
            name: ((b'A' + i as u8) as char).to_string(),
            start: DVec2::new(-2500.0, *y),
            end: DVec2::new(bays as f64 * bay + 2500.0, *y),
        }));
    }

    // Portal frames: columns on pads, a rafter pair up to the ridge.
    let column = t(&doc, Category::StructuralColumns, "IS 305x305");
    let rafter = t(&doc, Category::StructuralFraming, "IS 533x210");
    let eaves_beam = t(&doc, Category::StructuralFraming, "IS 305x165");
    let angle = t(&doc, Category::StructuralFraming, "L 100x100x10");
    let pad = t(&doc, Category::StructuralFoundations, "1800 x 1800 x 600mm");
    let beam = |type_id, kind, level, start, end, start_offset, end_offset| {
        ElementData::Framing(Framing {
            type_id,
            kind,
            level,
            start,
            end,
            start_offset,
            end_offset,
        })
    };
    for i in 0..=bays {
        let x = i as f64 * bay;
        for y in [0.0, span] {
            let p = DVec2::new(x, y);
            doc.add_element(ElementData::Column(Column {
                type_id: column,
                point: p,
                rotation: 0.0,
                base_level: l1,
                base_offset: 0.0,
                top_level: eaves,
                top_offset: 0.0,
            }));
            doc.add_element(ElementData::Footing(Footing {
                type_id: pad,
                level: l1,
                point: p,
                rotation: 0.0,
                offset: -250.0,
            }));
        }
        let ridge = DVec2::new(x, span / 2.0);
        doc.add_element(beam(rafter, FramingKind::Beam, eaves, DVec2::new(x, 0.0), ridge, 0.0, rise));
        doc.add_element(beam(rafter, FramingKind::Beam, eaves, ridge, DVec2::new(x, span), rise, 0.0));
    }
    for i in 0..bays {
        let (x0, x1) = (i as f64 * bay, (i + 1) as f64 * bay);
        for y in [0.0, span] {
            doc.add_element(beam(
                eaves_beam,
                FramingKind::Beam,
                eaves,
                DVec2::new(x0, y),
                DVec2::new(x1, y),
                0.0,
                0.0,
            ));
        }
    }
    // Cross-bracing in both end bays, in the side walls' plane.
    for (x0, x1) in [(0.0, bay), ((bays - 1) as f64 * bay, bays as f64 * bay)] {
        for y in [0.0, span] {
            let (a, b) = (DVec2::new(x0, y), DVec2::new(x1, y));
            doc.add_element(beam(angle, FramingKind::Brace, l1, a, b, 300.0, 5400.0));
            doc.add_element(beam(angle, FramingKind::Brace, l1, b, a, 300.0, 5400.0));
        }
    }

    // Cladding outside the steel, a slab inside it.
    let panel = t(&doc, Category::Walls, "Exterior - Insulated Metal Panel 150mm");
    let (lo, hi) = (-240.0, bays as f64 * bay + 240.0);
    let (ylo, yhi) = (-240.0, span + 240.0);
    let corners = [DVec2::new(lo, ylo), DVec2::new(lo, yhi), DVec2::new(hi, yhi), DVec2::new(hi, ylo)];
    let mut walls = Vec::new();
    for i in 0..4 {
        walls.push(doc.add_element(ElementData::Wall(Wall {
            type_id: panel,
            start: corners[i],
            end: corners[(i + 1) % 4],
            base_level: l1,
            base_offset: 0.0,
            top: TopConstraint::UpToLevel { level: eaves, offset: 0.0 },
            structural: false,
            room_bounding: true,
            attached_top: vec![],
            disallow_join: [false; 2],
            arc_mid: None,
            curtain: Default::default(),
        })));
    }
    let slab = t(&doc, Category::Floors, "Concrete Slab - Industrial 250mm");
    let inset = 75.0;
    doc.add_element(ElementData::Floor(Floor {
        type_id: slab,
        level: l1,
        boundary: vec![
            DVec2::new(lo + inset, ylo + inset),
            DVec2::new(hi - inset, ylo + inset),
            DVec2::new(hi - inset, yhi - inset),
            DVec2::new(lo + inset, yhi - inset),
        ],
        height_offset: 0.0,
        room_bounding: true,
        structural: true,
    }));
    let roof_t = t(&doc, Category::Roofs, "Insulated Metal Sheet 200mm");
    let o = 300.0;
    let roof = doc.add_element(ElementData::Roof(Roof {
        type_id: roof_t,
        level: eaves,
        footprint: vec![
            DVec2::new(lo - o, ylo - o),
            DVec2::new(hi + o, ylo - o),
            DVec2::new(hi + o, yhi + o),
            DVec2::new(lo - o, yhi + o),
        ],
        base_offset: 0.0,
        slope: 6.0,
        shape: RoofShape::Gable,
        defines_slope: vec![],
        slope_arrow: None,
    }));
    for w in &walls {
        if let Some(ElementData::Wall(w)) = doc.element_mut(*w).map(|e| &mut e.data) {
            w.attached_top = vec![roof];
        }
    }
    // A mezzanine in the last bay, on light columns and beams, reached by a
    // steel stair with guardrails along its open edges.
    let mezz = add_level_with_plan(&mut doc, "Level 2", 3000.0);
    let light = t(&doc, Category::StructuralColumns, "IS 203x203");
    let x0 = (bays - 1) as f64 * bay;
    let (mx1, my0, my1) = (hi - 75.0, 9000.0, yhi - 75.0);
    for x in [x0, bays as f64 * bay] {
        doc.add_element(ElementData::Column(Column {
            type_id: light,
            point: DVec2::new(x, my0),
            rotation: 0.0,
            base_level: l1,
            base_offset: 0.0,
            top_level: mezz,
            top_offset: 0.0,
        }));
    }
    for (a, b) in [
        (DVec2::new(x0, my0), DVec2::new(bays as f64 * bay, my0)),
        (DVec2::new(x0, my0), DVec2::new(x0, span)),
        (DVec2::new(bays as f64 * bay, my0), DVec2::new(bays as f64 * bay, span)),
    ] {
        doc.add_element(beam(eaves_beam, FramingKind::Beam, mezz, a, b, -150.0, -150.0));
    }
    let deck = t(&doc, Category::Floors, "Generic 150mm");
    doc.add_element(ElementData::Floor(Floor {
        type_id: deck,
        level: mezz,
        boundary: vec![
            DVec2::new(x0 - 100.0, my0 - 100.0),
            DVec2::new(mx1, my0 - 100.0),
            DVec2::new(mx1, my1),
            DVec2::new(x0 - 100.0, my1),
        ],
        height_offset: 0.0,
        room_bounding: true,
        structural: true,
    }));
    let steel_stair = t(&doc, Category::Stairs, "Steel - 190 max riser 250 tread");
    let guard = t(&doc, Category::Railings, "Guardrail - 1100mm Industrial");
    let stair_y = my0 + 600.0;
    let run = 15.0 * 250.0;
    let stair_id = doc.add_element(ElementData::Stair(Stair {
        type_id: steel_stair,
        base_level: l1,
        base_offset: 0.0,
        top_level: mezz,
        top_offset: 0.0,
        path: vec![DVec2::new(x0 - 100.0 - run, stair_y), DVec2::new(x0 - 100.0, stair_y)],
        width: 1000.0,
    }));
    if let Some(l) = crate::stairs::stair_layout(&doc, stair_id) {
        for path in crate::stairs::stair_railing_paths(&l, 1000.0, 50.0) {
            doc.add_element(ElementData::Railing(Railing {
                type_id: guard,
                level: l1,
                base_offset: 0.0,
                path,
                host: stair_id,
            }));
        }
    }
    let edge = x0 - 50.0;
    for path in [
        vec![
            DVec2::new(mx1 - 50.0, my0 - 50.0),
            DVec2::new(edge, my0 - 50.0),
            DVec2::new(edge, stair_y - 500.0),
        ],
        vec![DVec2::new(edge, stair_y + 500.0), DVec2::new(edge, my1 - 50.0)],
    ] {
        doc.add_element(ElementData::Railing(Railing {
            type_id: guard,
            level: mezz,
            base_offset: 0.0,
            path,
            host: ElementId::INVALID,
        }));
    }

    let door = t(&doc, Category::Doors, "0926 x 2040mm");
    let opening = |type_id, host, offset, sill_height| HostedOpening {
        type_id,
        host,
        offset,
        sill_height,
        flip_hand: false,
        flip_facing: false,
    };
    doc.add_element(ElementData::Door(opening(door, walls[3], 3000.0, 0.0)));
    // A loading bay door between the columns of the middle bay, and strip
    // windows high in the north wall.
    for name in ["Door - Sectional Overhead", "Window - Strip"] {
        if let Some(def) = crate::library::family(name) {
            let _ = doc.load_family(def);
        }
    }
    let overhead = t(&doc, Category::Doors, "4000 x 4500mm");
    doc.add_element(ElementData::Door(opening(overhead, walls[3], 15240.0, 0.0)));
    let strip = t(&doc, Category::Windows, "3000 x 900mm");
    for x in [3000.0, 9000.0, 15000.0] {
        doc.add_element(ElementData::Window(opening(strip, walls[1], x - lo, 2400.0)));
    }

    doc.add_view(
        &doc.unique_view_name("Section"),
        ViewKind::Section {
            start: DVec2::new(bay * 1.5, -3000.0),
            end: DVec2::new(bay * 1.5, span + 3000.0),
            far_clip: 20000.0,
        },
    );
    // Along the stair, looking at the mezzanine.
    doc.add_view(
        &doc.unique_view_name("Section"),
        ViewKind::Section {
            start: DVec2::new(x0 - 6000.0, stair_y),
            end: DVec2::new(hi + 2000.0, stair_y),
            far_clip: 8000.0,
        },
    );
    doc.add_view(
        "Framing Schedule",
        ViewKind::Schedule(crate::ScheduleDef::new(Category::StructuralFraming)),
    );
    doc.add_view(
        "Foundation Schedule",
        ViewKind::Schedule(crate::ScheduleDef::new(Category::StructuralFoundations)),
    );

    // Back-to-back pallet racking with a 3.1 m aisle, loaded pallets by
    // the door, a workbench under the mezzanine and bollards outside.
    place_components(
        &mut doc,
        l1,
        &[
            ("Pallet Racking", "5 Bay 2700mm", 10000.0, 3700.0, 0.0),
            ("Pallet Racking", "5 Bay 2700mm", 10000.0, 3800.0, 180.0),
            ("Pallet Racking", "5 Bay 2700mm", 10000.0, 9100.0, 0.0),
            ("Pallet Racking", "5 Bay 2700mm", 10000.0, 9200.0, 180.0),
            ("Pallet", "EUR 1200 x 800mm Loaded", 21500.0, 2500.0, 0.0),
            ("Pallet", "EUR 1200 x 800mm Loaded", 21500.0, 3600.0, 0.0),
            ("Pallet", "EUR 1200 x 800mm", 23000.0, 2500.0, 90.0),
            ("Workbench", "2000mm", 27000.0, 14800.0, 0.0),
            ("Equipment Plinth", "1500 x 2500mm", 27500.0, 5000.0, 0.0),
            ("Bollard", "168mm", 26500.0, -700.0, 0.0),
            ("Bollard", "168mm", 28000.0, -700.0, 0.0),
        ],
    );
    crate::annotation::tag_openings_in_plans(&mut doc);
    doc
}
