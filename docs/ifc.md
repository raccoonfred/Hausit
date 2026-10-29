# IFC in Hausit

Hausit reads and writes IFC, the open BIM exchange format.

## Opening an IFC file

Use **File ▸ Open** or **Insert ▸ Open IFC**. IFC2x3 and IFC4 files are
supported, in any length unit. The file opens as a new, unsaved project.
The status bar then says what came in.

This is also the way to bring a Revit project into Hausit today: export it
to IFC from Revit (File ▸ Export ▸ IFC) and open the result.

Hausit turns what it can into native, editable elements:

| IFC | Hausit |
|---|---|
| `IfcBuildingStorey` | A level, with its floor and ceiling plans |
| `IfcWall` with a straight axis and an upright body | A wall. Its type keeps the family and type names, the width and the material layers. Exterior walls face out. |
| `IfcSlab` extruded straight up | A floor, with its outline and thickness |
| `IfcColumn` with a rectangular or I-shaped section | A structural column of that size |
| `IfcDoor` and `IfcWindow` in an imported wall | A door or window hosted by that wall, at the same position and size, using the Single Flush, Double Flush or Fixed families |
| `IfcSpace` | A room, with its number and name |
| `IfcGrid` axes | Grids |

Everything else that has geometry, for example roofs, stairs, railings,
beams, curtain walls, furniture and coverings, comes in as **imported
geometry**. It keeps its shape, its colours and transparency, and a
category that matches its IFC class, so it shows in plans, sections,
elevations, 3D and on sheets. In properties you can change its level, and
you can move, rotate, mirror, copy and delete it. Transparent surfaces get
a glass material.

Walls whose top is cut by a roof in the IFC file are attached to the
imported roof, so they keep their shape.

The geometry Hausit reads: extrusions (straight and slanted), faceted
breps, polygonal and triangulated face sets, shell and face based surface
models, mapped items, bounding boxes, blocks and half-space clipping.
Profiles can be arbitrary, rectangle, circle, ellipse, I, L and derived
profiles. Curves can be polylines, composite and trimmed curves, circles,
ellipses and indexed poly curves.

To convert from the command line without opening the app:

```
cargo run -p hausit-io --example ifc_import -- model.ifc model.hausit
```

## Exporting IFC

**File ▸ Export ▸ IFC** (or `hausit project.hausit --export-ifc out.ifc`)
writes an IFC4 Reference View file with storeys, typed elements, material
layer sets, openings, spaces, grids and parameters as property sets.
Imported geometry is written back with the IFC class of its category.
