# Hausit

Hausit is an open-source building information modelling (BIM) application
that works like Autodesk Revit. It is written in Rust and runs on Windows,
macOS and Linux.

This is an early version. It covers the core Revit workflow in a basic form,
and features are added and deepened over time.

## Running it

1. Install Rust from <https://rustup.rs> (version 1.95 or newer).
2. Clone and run:

   ```sh
   git clone https://github.com/raccoonfred/Hausit.git
   cd Hausit
   cargo run --release
   ```

   `cargo run --release -- --sample` opens a sample house straight away.

On Linux you may need the usual windowing libraries
(`sudo apt install libxkbcommon-x11-0 libgtk-3-dev` on Debian/Ubuntu).
Hausit uses your GPU through wgpu (Vulkan, Metal or DirectX 12) and falls
back to OpenGL when that is not available.

## What works

The layout follows Revit: File menu and Quick Access Toolbar, ribbon tabs,
Options Bar, Properties palette, Project Browser, view tabs, View Control Bar
and status bar.

| Area | In this version |
|------|-----------------|
| Model | Levels, grids, walls (with automatic corner and T joins, Attach Top/Base to roofs and floors), floors, footprint roofs (flat, hip, gable), doors and windows hosted in walls, structural columns, rooms (bounded automatically by walls, columns and room separation lines, with area, perimeter and volume) |
| Annotation | Aligned dimensions to wall faces, wall centerlines and grids; room tags with area; door and window tags |
| Families and types | Wall, floor and roof types; door and window families with types. Type Properties dialog with Duplicate, Rename and editable type parameters |
| Properties | Revit-style instance parameters (Base Constraint, Top Constraint, Unconnected Height, Sill Height, Mark, Comments…) with live Length/Area/Volume |
| Views | Floor plans (cut plane, poché, door swings, window symbols, grid bubbles), elevations and sections (with level heads and cut elements), orthographic 3D with a Revit ViewCube (faces, edges, corners, compass, Home, roll arrows, drag to orbit). Wireframe, Hidden Line, Shaded and Consistent Colors styles |
| Tools | Wall (line or rectangle, chain, location line, typed lengths, Space to flip), Door, Window (Space flips the hand), Floor and Roof by Footprint in sketch mode (Pick Walls, Line, Rectangle, Defines Slope, overhang, Lowest Level Notice; hips, valleys and gables on any footprint shape), Structural Column (with At Grids), Room, Room Separator, Aligned Dimension, Level, Grid, Section, Move, Copy, Rotate, Mirror (pick or draw axis, with Copy), Trim/Extend to Corner, Split, Attach/Detach Top/Base, Delete, Undo/Redo. In plan, selected walls show end grips, and selected elements can be dragged |
| Selection | Click, Ctrl/Shift to add/remove, window (left→right) and crossing (right→left) selection, pre-highlight on hover |
| Snapping | Endpoints, midpoints, intersections, perpendicular, nearest, dashed alignment lines, 15° angle and zoom-based length increments, with Revit's snap glyphs and tooltips. Temporary dimensions show length and angle |
| Schedules | Door, window, wall, floor, roof, room and structural column schedules with field selection, sorting, grouping, grand totals and CSV export |
| Files | Native `.hausit` project format (versioned JSON) |
| Revit files | `.rvt/.rfa/.rte/.rft` files can be opened to show the Revit version, build and preview image. See below |

Revit keyboard shortcuts work: `WA` wall, `DR` door, `WN` window, `FL` floor,
`RF` roof, `CL` column, `RM` room, `DI` dimension, `LL` level, `GR` grid,
`MV` move, `CO` copy, `RO` rotate, `MM`/`DM` mirror, `TR` trim, `SL` split,
`DE` delete, `ZF` zoom to fit, plus `Esc`, `Delete`, `Tab` (cycle overlapping elements) and
`Ctrl+Z/Y/S/O/N`.

Ribbon buttons for features that are not built yet (stairs, railings,
ceilings, beams, foundations, text, sheets and so on) are shown greyed out with
a "planned" tooltip, so the layout matches Revit while the features arrive.

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the gap analysis against Revit, the phased
plan, the path to opening RVT files and the legal ground rules.

## Opening Revit files

Opening `.rvt` and `.rfa` files from every Revit version is a core goal.
Revit's format is proprietary and undocumented, so it is a research-heavy
milestone and is staged in the `hausit-rvt` crate:

1. Read the OLE compound-file container ✔
2. Read `BasicFileInfo` (Revit version, build, worksharing) ✔
3. Extract the preview image ✔
4. Decompress the `Global/*` and `Partitions/*` streams
5. Decode the per-version object schema
6. Decode element records
7. Map them onto the Hausit model

Today Hausit stops after step 3, shows what it found, and says plainly that
the model content was not imported. It never shows a made-up model.

## Code layout

```
crates/
  hausit-core       ids, units, geometry helpers
  hausit-model      the BIM model: elements, families/types, parameters, geometry, undo
  hausit-views      plan / elevation / section / 3D graphics as display lists
  hausit-schedules  schedule tables and CSV export
  hausit-io         .hausit files and the open/import dispatcher
  hausit-rvt        Revit file reader (container, version, preview)
  hausit-app        the desktop application (egui + wgpu)
```

Internal lengths are millimetres. Run the tests with `cargo test --workspace`.

## License

Hausit is dual-licensed under either of

- the MIT license ([LICENSE-MIT](LICENSE-MIT)), or
- the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)),

at your option. Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion in Hausit, as defined in the Apache-2.0 license, is dual-licensed as above, without any additional terms or conditions.
