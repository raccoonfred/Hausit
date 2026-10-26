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
| Model | Levels, grids, walls (with automatic corner and T joins), floors, footprint roofs (flat, hip, gable), doors and windows hosted in walls |
| Families and types | Wall, floor and roof types; door and window families with types. Type Properties dialog with Duplicate, Rename and editable type parameters |
| Properties | Revit-style instance parameters (Base Constraint, Top Constraint, Unconnected Height, Sill Height, Mark, Comments…) with live Length/Area/Volume |
| Views | Floor plans (cut plane, poché, door swings, window symbols, grid bubbles), elevations and sections (with level heads and cut elements), orthographic 3D with orbit and view presets. Wireframe, Hidden Line, Shaded and Consistent Colors styles |
| Tools | Wall (line or rectangle, chain, location line, typed lengths), Door, Window, Floor and Roof sketches with Finish/Cancel, Level, Grid, Section, Move, Copy, Delete, Undo/Redo |
| Selection | Click, Ctrl/Shift to add/remove, window (left→right) and crossing (right→left) selection, pre-highlight on hover |
| Snapping | Endpoints, midpoints, intersections, nearest, and horizontal/vertical alignment |
| Schedules | Door, window, wall, floor and roof schedules with field selection, sorting, grouping, grand totals and CSV export |
| Files | Native `.hausit` project format (versioned JSON) |
| Revit files | `.rvt/.rfa/.rte/.rft` files can be opened to show the Revit version, build and preview image. See below |

Revit keyboard shortcuts work: `WA` wall, `DR` door, `WN` window, `FL` floor,
`RF` roof, `LL` level, `GR` grid, `MV` move, `CO` copy, `DE` delete, `ZF`
zoom to fit, plus `Esc`, `Delete`, `Tab` (cycle overlapping elements) and
`Ctrl+Z/Y/S/O/N`.

Ribbon buttons for features that are not built yet (stairs, rooms,
dimensions, tags, sheets, rotate, mirror and so on) are shown greyed out with
a "planned" tooltip, so the layout matches Revit while the features arrive.

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
