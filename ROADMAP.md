# Hausit roadmap

The goal is for Hausit to do the job of Autodesk Revit for architects and
engineers: the same modelling workflow, the same kinds of drawings coming out
the other end, and the ability to open the `.rvt` and `.rfa` files that
practices already have. This document says where Hausit stands today, what is
missing by Revit area, the order to build it in, and the legal lines the
project has to stay inside while doing it.

Autodesk's Revit documentation (help.autodesk.com/view/RVT) is the reference
for behaviour. When this roadmap says "like Revit", it means a user who knows
Revit should find the same tool in the same place, doing the same thing.

Last reviewed: 26 October 2026.

---

## 1. Where Hausit is today

Hausit 0.1 already covers the core of a small architectural model:

- Revit-style shell: ribbon, Quick Access Toolbar, Options Bar, Properties
  palette, Project Browser, view tabs, View Control Bar, status bar, Revit
  keyboard shortcuts.
- Model: levels, grids, walls with joins and Attach Top/Base, floors,
  footprint roofs (any shape, hips, valleys, gables), doors and windows,
  structural columns, rooms with automatic boundaries.
- Views: floor plans, elevations, sections, orthographic 3D with a ViewCube,
  four visual styles.
- Annotation: aligned dimensions, room tags, door and window tags.
- Editing: Move, Copy, Rotate, Mirror, Trim/Extend, Split, Delete, grips,
  undo/redo, snapping with temporary dimensions.
- Schedules with sorting, grouping, totals and CSV export.
- Native `.hausit` format (versioned JSON).
- `.rvt/.rfa` files: container, version and preview only, no model import.

That is roughly what a Revit user touches in the first week of a project. The
gaps below are what stands between that and replacing Revit on a real job.

---

## 2. Gap analysis by Revit area

Rating: **Have** (basic version works), **Partial**, **Missing**.
Priority: **P0** is needed before anyone can use Hausit on a real project,
**P1** is needed to replace Revit for most architectural work, **P2** is
needed for full parity, **P3** is long-tail.

### 2.1 Architecture

| Revit feature | Status | Priority | Notes |
|---|---|---|---|
| Walls (basic, joins, attach) | Have | — | |
| Compound wall layers, wrapping, layer joins | Partial | P0 | Layers with plan and section poché and an assembly editor; wrapping at ends and layer joins still to do |
| Stacked walls, curtain walls, curtain grids, mullions, panels | Missing | P1 | Curtain walls are on almost every commercial job |
| Wall sweeps and reveals, profile edits, wall openings | Missing | P1 | |
| Arc, ellipse and spline walls | Missing | P1 | Arc walls are P0 for many users |
| Floors (sketch, slope arrows, shape editing, sub-elements) | Partial | P0 | Shape editing for falls is P1 |
| Roofs by footprint | Have | — | |
| Roof by extrusion, soffits, fascia, gutters, dormers, join/unjoin roof | Missing | P1 | |
| Ceilings (automatic and sketch, ceiling grids, reflected ceiling plans) | Missing | P0 | |
| Stairs by component and by sketch, landings, supports | Partial | P0 | Runs with landings at turns, risers from the level heights, plan cut line and UP arrow; sketch-based stairs, winders and supports still to do |
| Railings (path, host to stairs, balusters, top rails) | Partial | P0 | Sketched or hosted on stairs (created with them), balusters, top and intermediate rails |
| Ramps | Missing | P1 | |
| Shafts, vertical and face openings, dormer openings | Missing | P1 | |
| Rooms, areas, area plans (gross, rentable), color fill schemes | Partial | P1 | Rooms done; areas and color schemes missing |
| Model text, model lines, model groups, detail groups | Missing | P1 | Groups are used heavily |
| Components (furniture, casework, plumbing fixtures as families) | Missing | P0 | Depends on the family system (2.4) |
| Massing, conceptual mass, wall/floor/roof by face | Missing | P2 | |
| Toposolid, site, building pads, property lines | Missing | P2 | |
| Curtain systems, adaptive components | Missing | P3 | |

### 2.2 Structure

| Revit feature | Status | Priority | Notes |
|---|---|---|---|
| Structural columns | Have | — | Vertical only |
| Slanted columns | Missing | P2 | |
| Beams, beam systems, braces, trusses | Partial | P1 | Beams and braces with steel I, angle and rectangular sections; beam systems and trusses still to do |
| Isolated, wall and slab foundations | Partial | P1 | Isolated (with At Columns) and wall foundations; slab foundations still to do |
| Structural walls and floors (flag, analytical) | Missing | P1 | Mostly a parameter on existing elements |
| Steel connections | Missing | P3 | |
| Rebar, area/path reinforcement, fabric | Missing | P2 | Big subsystem |
| Analytical model, loads, boundary conditions, export to analysis | Missing | P2 | Export path through IFC structural or a common format |
| Span direction symbols, beam annotations, structural framing tags | Missing | P1 | |

### 2.3 MEP (mechanical, electrical, plumbing)

MEP is a separate product line inside Revit and is the largest single gap.

| Revit feature | Status | Priority | Notes |
|---|---|---|---|
| Spaces and zones | Missing | P2 | Builds on rooms |
| Ducts, duct fittings, flex duct, air terminals, routing | Missing | P2 | |
| Pipes, pipe fittings, plumbing fixtures, sloped piping | Missing | P2 | |
| Systems (supply, return, sanitary), system browser | Missing | P2 | |
| Electrical devices, lighting, circuits, panels, panel schedules | Missing | P2 | |
| Cable tray, conduit | Missing | P2 | |
| Sizing, pressure loss, heating/cooling loads | Missing | P3 | Engineering calculations need careful validation |
| Fabrication parts | Missing | P3 | |

### 2.4 Families and the family editor

Families are what make Revit Revit. Almost every other gap depends on this.

| Revit feature | Status | Priority | Notes |
|---|---|---|---|
| System families (walls, floors, roofs) with types | Have | — | |
| Loadable families with types | Partial | P0 | Doors/windows are hard-coded today |
| Family editor: reference planes, dimensions, labels, parameters, formulas | Missing | P0 | |
| Extrusion, blend, revolve, sweep, swept blend, voids | Missing | P0 | Needs a solid modeller (see 3.2) |
| Constraints solver (locks, EQ, alignment) | Missing | P0 | |
| Family types table, type catalogs | Missing | P1 | |
| Nested and shared families | Missing | P1 | |
| Symbolic lines, visibility per view/detail level, masking regions | Missing | P1 | |
| Family templates (`.rft`), hosted families (wall, face, ceiling, floor) | Missing | P1 | |
| Shared parameters, project parameters, global parameters | Missing | P1 | |
| Detail components, profiles, annotation families (tags, symbols, title blocks) | Missing | P1 | |
| Adaptive and conceptual mass families | Missing | P3 | |
| A starter family library (doors, windows, furniture, fixtures, title blocks) | Missing | P0 | Must be authored from scratch, never copied from Autodesk content |

### 2.5 Annotation and documentation

| Revit feature | Status | Priority | Notes |
|---|---|---|---|
| Aligned dimensions | Have | — | |
| Linear, angular, radial, diameter, arc length, spot elevation/coordinate/slope | Missing | P0/P1 | Linear and spot elevation P0 |
| Text, leaders, keynotes, symbols | Missing | P0 | |
| Tag all, multi-category, material tags | Partial | P1 | |
| Detail lines, filled regions, masking regions, detail components, insulation, repeating details | Missing | P0 | |
| Callouts, drafting views, legends | Missing | P0 | |
| Revision clouds and revision tracking | Missing | P1 | |
| Object styles, line styles, line weights by scale, line patterns, fill patterns | Missing | P0 | Drawing quality depends on this |
| View templates, visibility/graphics overrides, filters | Missing | P0 | |
| View range, underlay, crop regions, scope boxes, detail levels | Partial | P0 | |
| Materials with graphics, appearance, cut/surface patterns | Partial | P0 | Material Browser with colour and cut patterns; surface patterns and appearance still to do |
| Schedules (quantities, material takeoff, sheet list, view list, note blocks, key schedules, calculated values, conditional formatting) | Partial | P1 | |

### 2.6 Sheets and printing

| Revit feature | Status | Priority | Notes |
|---|---|---|---|
| Sheets, title blocks, placing views as viewports | Missing | P0 | |
| Viewport titles, view references, automatic sheet/detail numbering | Missing | P0 | |
| Schedules and legends on sheets | Missing | P0 | |
| Print and export to PDF (vector, line weights, halftone) | Missing | P0 | |
| Print sets, batch printing | Missing | P1 | |
| Export sheets to DWG | Missing | P1 | See 2.11 |

### 2.7 Worksharing and collaboration

| Revit feature | Status | Priority | Notes |
|---|---|---|---|
| Central model, local copies, Synchronize with Central | Missing | P2 | |
| Worksets, element borrowing, ownership | Missing | P2 | |
| Linked Revit/Hausit models, copy/monitor, coordination review | Missing | P1 | Linking is more common than worksharing on small jobs |
| Cloud collaboration (Autodesk Docs/BIM 360 equivalent) | Missing | P3 | Hausit should offer its own open server instead, see Phase 4 |

### 2.8 Phasing

| Revit feature | Status | Priority | Notes |
|---|---|---|---|
| Phases, Phase Created/Demolished, phase filters, graphic overrides | Missing | P1 | Essential for renovation work |
| Demolish tool, temporary elements | Missing | P1 | |

### 2.9 Design options

| Revit feature | Status | Priority | Notes |
|---|---|---|---|
| Option sets, primary/secondary options, accept primary | Missing | P2 | |

### 2.10 API and add-ins

Revit's ecosystem of add-ins (pyRevit, Dynamo, Enscape, and thousands of
in-house tools) is a large part of why practices stay on Revit.

| Revit feature | Status | Priority | Notes |
|---|---|---|---|
| Public API for elements, parameters, transactions, views | Missing | P1 | Stable Rust API first, then bindings |
| Scripting (Python) with a console | Missing | P1 | pyRevit-style scripting is what most users want first |
| Add-in loading (manifest, ribbon buttons, commands) | Missing | P1 | WASM plug-ins keep add-ins sandboxed and cross-platform |
| Visual programming (Dynamo equivalent) | Missing | P2 | Could integrate an existing open node editor |
| Revit API compatibility shim (.NET) | Missing | P3 | Very large. Only if demand justifies it |

### 2.11 Interoperability (IFC, DWG, RVT and others)

| Format | Status | Priority | Notes |
|---|---|---|---|
| `.hausit` native | Have | — | Move to a binary or SQLite container in Phase 2 for large models |
| IFC 2x3 and IFC4 export | Missing | P0 | The open BIM standard. First route to coordination with other teams |
| IFC import (as native elements where possible, else as reference geometry) | Missing | P0 | Also the fallback for Revit files (see section 4) |
| PDF export | Missing | P0 | |
| DWG/DXF export | Missing | P1 | DXF is documented and can be written directly. DWG needs a library (see 4.3) |
| DWG/DXF import and link (CAD underlays) | Missing | P1 | Very common for site plans and consultant drawings |
| Images, point clouds (E57, LAS) | Missing | P2 | |
| gbXML, COBie, BCF (issue exchange) | Missing | P2 | BCF is cheap and useful for coordination |
| SketchUp, FBX, OBJ, glTF, USD export | Missing | P2 | glTF and OBJ are easy and help visualisation |
| RVT/RFA read | Partial | P1 → P3 | Header and preview only today. See section 4 |
| RVT/RFA write | Missing | P3 | Probably never fully. See section 4 |

### 2.12 Rendering and visualisation

| Revit feature | Status | Priority | Notes |
|---|---|---|---|
| Wireframe, Hidden Line, Shaded, Consistent Colors | Have | — | |
| Realistic style, materials with textures | Missing | P1 | |
| Perspective cameras, walkthroughs | Missing | P1 | |
| Shadows, sun path, ambient occlusion, sketchy lines | Missing | P2 | |
| Section box in 3D | Missing | P0 | Users rely on it constantly |
| Photorealistic rendering | Missing | P3 | Better as an export to an existing open renderer (Blender Cycles via glTF/USD) |

### 2.13 Performance and scale

| Need | Status | Priority | Notes |
|---|---|---|---|
| Handle a 50 MB Revit-sized model (tens of thousands of elements, hundreds of views) | Unknown | P0 | Set up a benchmark model now |
| Incremental regeneration (only recompute what changed) | Missing | P0 | Dependency graph between elements |
| Spatial index for picking and view generation | Missing | P0 | |
| GPU instancing for repeated families | Missing | P1 | |
| Background view generation, multi-threading | Missing | P1 | Rust and rayon make this safer than in Revit |
| Fast open/save for large models | Missing | P1 | Lazy loading by view or level |

---

## 3. Foundations to build first

Some pieces are not features a user sees but block most of the gaps above.
Building them early avoids rewriting the model later.

### 3.1 Parametric element core

Revit elements depend on each other: a door depends on its wall, a wall top on
a level or roof, a dimension on references. Hausit needs:

- An element dependency graph with incremental regeneration.
- Stable references to faces, edges and reference planes, so that dimensions,
  hosted elements and constraints survive edits.
- A constraint solver for the family editor (2D sketch solving, locks,
  equality, labels driven by formulas).
- A formula engine for parameters (Revit syntax: `if()`, `and()`, units,
  `roundup()` and so on).

### 3.2 Solid modelling kernel

Family geometry needs real solids: extrusions, blends, revolves, sweeps, voids
cutting other geometry, and joins between elements. Options:

- **Truck** (Rust, B-rep, open source). Pure Rust, fits the project, still
  maturing.
- **OpenCascade** through FFI. Mature and battle-tested, LGPL with an
  exception, heavy C++ dependency.
- **Fornjot** (Rust). Early.
- A mesh-based CSG approach (for example `manifold`, Apache-2.0, C++ with
  bindings) for voids and joins, combined with simple analytic shapes.

Recommendation: start with a mesh CSG library for voids and joins because it
is robust and fast, and keep the element geometry API kernel-agnostic so a
B-rep kernel can be swapped in when Truck matures.

### 3.3 Graphics and drawing pipeline

Plan, section and sheet output must be drawing-quality: correct line weights
by scale, cut versus projection, overrides, halftone, and the same result on
screen, in PDF and in DWG. Build one 2D display-list pipeline (Hausit already
has the start of this in `hausit-views`) that feeds the screen, PDF writer and
DXF/DWG writer from the same data.

### 3.4 File format v2

JSON works for small models. For real projects, move to a chunked binary or
SQLite-based container with element-level versioning. This also prepares for
worksharing (Phase 4), where element-level change sets are needed.

---

## 4. Opening RVT files from every version

### 4.1 The problem

`.rvt` and `.rfa` are undocumented and change with every annual release.
The outer container is an OLE compound file (Hausit already reads it, along
with `BasicFileInfo` and the preview). Inside, the model data is compressed
and serialised with a per-version schema of several thousand classes.
Autodesk has never published it and does not license it.

### 4.2 The options

| Route | What it gives | Cost and risk |
|---|---|---|
| **A. IFC fallback** | Users export IFC from Revit and open it in Hausit. Works for every Revit version that exports IFC (2010 onwards). Legally clean. | Needs a Revit licence on the sending side. IFC loses some Revit intelligence: families come across as geometry plus properties, constraints are lost, views and sheets mostly lost. |
| **B. Open Design Alliance BimRv SDK** | ODA sells a commercial SDK that reads (and partly writes) RVT/RFA across many versions, built by their own reverse engineering. Fastest path to real RVT import. | Paid membership (thousands of US dollars per year, more for commercial distribution). Closed source C++, so it cannot be part of the MIT/Apache code. It would have to be an optional plug-in that users or a commercial edition license separately. Check ODA's current terms before committing. |
| **C. Clean-room reverse engineering** | Fully open, owned by the project, no fees. | Years of work, and must be repeated for every new Revit release. Legally possible in many jurisdictions for interoperability (see 5.3) but must be done carefully. |
| **D. Revit add-in exporter** | A small free add-in that runs inside Revit and writes a rich Hausit or IFC file using Revit's own API, keeping families, types, parameters, views and sheets. | Needs Revit installed once to convert. Uses the public Revit API, which is permitted. Captures far more than IFC. |

### 4.3 Recommended path

Do them in this order, so users get value early and the expensive route is
only taken if needed:

1. **IFC import first (Phase 1).** It is needed anyway for open BIM, and it is
   the universal fallback for every Revit version.
2. **Revit-to-Hausit exporter add-in (Phase 2).** A C# add-in using the
   public Revit API that exports the full model (elements, family geometry,
   types, parameters, views, sheets) to the Hausit format. This is the best
   fidelity available legally and cheaply for practices migrating their
   archive, as long as they still have one Revit seat.
3. **Clean-room native reader, staged (Phases 2 to 5).** Continue the
   `hausit-rvt` crate one step at a time, in this order of value:
   - Decompress streams and read the per-file class schema (the schema is
     stored in the file itself, which helps across versions).
   - Read the element table, categories, levels and parameters. This alone
     enables "open RVT and see its schedules and data".
   - Read family types and instance placements, using the embedded geometry
     caches where present.
   - Map walls, floors, roofs, doors and windows to native Hausit elements.
   - Views and sheets last.
   Each Revit version gets a set of test files created by the team in Revit
   (never customer files without permission) and a version-coverage table in
   the README. Support newest versions first (2027, 2026, 2025...) because
   that is what users have, then work backwards.
4. **ODA SDK as an optional commercial plug-in**, only if the project gains a
   commercial edition or sponsor that can pay for it, and only behind a plug-in
   boundary so the open-source core stays free of it.

Writing RVT files is not a goal. Exchanging with Revit users happens through
IFC, DWG and PDF, the same way other BIM tools (ArchiCAD, Vectorworks)
exchange with Revit today.

### 4.4 Rules for the reverse-engineering work

- Work only from files the team created, or files whose owners gave written
  permission. Never use files obtained in breach of a licence.
- Do not decompile, disassemble or debug Revit's binaries. Revit's licence
  forbids it, and in some jurisdictions it is only lawful under narrow
  interoperability conditions. Study the file format from the outside:
  create a file, change one thing, compare the bytes.
- Do not look at leaked Autodesk source code or SDK internals, or at code from
  proprietary readers (including ODA's), in any form.
- Keep a written log of how each part of the format was learned. This is the
  evidence that the work is clean-room if it is ever questioned.
- Contributors who have signed an Autodesk NDA or work for Autodesk should not
  contribute to `hausit-rvt`.

---

## 5. Legal risks

This is not legal advice. Before a public release or any commercial use, have
a lawyer who knows software IP review the points below.

### 5.1 Trademarks

"Revit", "Autodesk", "AutoCAD", "DWG" (contested, but Autodesk claims it),
"BIM 360", "Dynamo" and the Revit "R" logo are Autodesk trademarks.

- Do not use "Revit" in the product name, logo, app icon, window title or
  domain name. "Hausit" is fine.
- Saying Hausit "opens Revit files" or "is an alternative to Revit" is
  generally allowed as truthful, descriptive (nominative) use. Keep it plain
  text, never in Autodesk's logo or styling, and add a line such as:
  "Revit and Autodesk are registered trademarks of Autodesk, Inc. Hausit is
  not affiliated with or endorsed by Autodesk."
- Do not imply compatibility that is not there. The current README already
  says plainly that RVT model import is not done yet. Keep that honesty.
- Add the trademark notice to the README and the About dialog. **(Action for
  Phase 0.)**

### 5.2 Copying the user interface and icons

- **Icons, graphics, splash screens, sounds:** these are copyrighted artwork.
  Never copy Revit's icon images or trace them. Hausit's ribbon icons are
  already drawn from scratch as vector shapes in `icons.rs`, which is the
  right approach. Keep it that way, and make sure they do not end up
  looking like near-copies of Autodesk's artwork.
- **Layout and workflow:** the general arrangement (ribbon, properties
  palette, project browser), tool names that describe what they do ("Wall",
  "Attach Top/Base") and keyboard shortcuts are functional and widely shared
  across CAD software. In the United States, functional UI elements and
  command structures are generally not protected by copyright (see *Lotus v.
  Borland*, and *Google v. Oracle* on interfaces). In the EU, the CJEU held in
  *SAS Institute v. World Programming* that functionality and programming
  languages are not protected. This supports matching Revit's workflow.
- **The risk is in the "look":** a pixel-for-pixel copy of Revit's colour
  scheme, layout spacing and visual design together could invite a "trade
  dress" or copyright claim even if each part is functional. Match where
  things are and how they work, but give Hausit its own visual style
  (colours, icon art, typography).
- **Text:** Revit's help text, tooltips and dialog wording are copyrighted.
  Write Hausit's own descriptions. Using the same short tool and parameter
  names ("Base Offset", "Unconnected Height") is fine because they are short
  functional labels, but do not copy paragraphs from Autodesk's help.
- **Content:** Revit's bundled families, templates, title blocks, materials
  and sample projects are Autodesk's copyright. Hausit's library must be
  authored from scratch. Standards-based data (for example steel section
  sizes from published standards) is fine.

### 5.3 File format reverse engineering

- **EU:** the Software Directive (2009/24/EC, Article 6) allows decompilation
  for interoperability under strict conditions, and *SAS v. WPL* allows
  studying a program's behaviour. Studying file output is safer still.
- **US:** reverse engineering for interoperability has been found to be fair
  use (*Sega v. Accolade*, *Sony v. Connectix*), but licence terms that forbid
  it can be enforced as contract (*Bowers v. Baystate*). Anyone who accepted
  Revit's licence is bound by its no-reverse-engineering clause for Revit's
  software. Observing the format of files you created is a weaker target than
  decompiling, but this is the area to get legal advice on.
- **DMCA section 1201:** RVT files are not encrypted today. If a future Revit
  version adds encryption, do not circumvent it without advice.
- **Patents:** Autodesk holds patents on BIM techniques. Hausit implementing
  general features is low risk, but a patent search before implementing
  very specific, novel Revit behaviours is prudent if the project becomes
  commercial.

### 5.4 Practical checklist

- [x] Trademark disclaimer in README and About dialog.
- [x] Contributor guidelines: no Autodesk artwork, help text, families or
      decompiled code. Add a `CONTRIBUTING.md` section.
- [x] Clean-room log for `hausit-rvt` (`crates/hausit-rvt/CLEAN_ROOM.md`).
- [ ] Legal review before 1.0 or any commercial offering.

---

## 6. Phased plan

Each phase ends with something an architect could actually use. The order
favours what blocks a real project, then what makes people switch, then
parity.

### Phase 0: Groundwork (now, about 1 month)

Goal: make the existing app solid enough to build on.

- Add the trademark notice and contributor rules (section 5.4).
- Element dependency graph and incremental regeneration (3.1).
- Spatial index for picking.
- Benchmark model of 10,000+ elements and a performance test in CI.
- Line styles, line weights by scale, object styles (3.3). Every later drawing
  feature depends on these.
- Test suite: model regression tests and image snapshot tests for views.

**Milestone:** a 10,000-element model opens, edits and regenerates
interactively; the trademark notice is in place.

### Phase 1: First real project (about 6 months)

Goal: a small residential project can be modelled, documented and issued
entirely in Hausit.

1. Compound walls and materials, with correct poché.
2. Loadable families with a basic family editor (reference planes,
   dimensions, parameters, extrusion, blend, void) and a starter library.
3. Stairs and railings.
4. Ceilings and reflected ceiling plans.
5. Annotation basics: text, linear dimensions, spot elevations, detail lines,
   filled regions, callouts, drafting views, legends.
6. View templates, visibility/graphics, filters, crop regions, section box.
7. Sheets, title blocks, viewports, and PDF export.
8. IFC4 export and IFC import (also the first RVT fallback).
9. DXF export of plans and sheets.

**Milestone:** a two-storey house from blank project to a printed PDF set
and an IFC file that opens in another BIM viewer, with no Revit involved.

### Phase 2: Architectural practice replacement (about 9 to 12 months)

Goal: a small architecture practice can switch for new projects.

1. Curtain walls, stacked walls, sweeps and reveals, arc walls.
2. Family editor completeness: nested and shared families, formulas, type
   catalogs, hosted families, annotation families and tags.
3. Phasing and the Demolish tool.
4. Groups, linked models, DWG/DXF import and link.
5. Areas, area plans, colour schemes.
6. Full schedules: material takeoff, sheet and view lists, key schedules,
   calculated values.
7. Revisions and revision clouds.
8. Perspective views and realistic visual style.
9. File format v2 for large models.
10. Revit-to-Hausit exporter add-in (4.3, step 2).
11. Scripting API (Python) and add-in loading.
12. `hausit-rvt`: decompress streams, read schema, read element and parameter
    tables for the latest three Revit versions.

**Milestone:** a mid-size project (an apartment building or small office)
documented to construction drawings, and existing Revit projects brought in
through the exporter add-in or IFC.

### Phase 3: Structure and coordination (about 9 months)

Goal: structural engineers can model and document in Hausit, and teams can
coordinate.

1. Beams, beam systems, braces, foundations, structural walls and floors.
2. Structural annotation and framing tags.
3. Analytical model and export to analysis tools.
4. Design options.
5. Massing and model by face, site and toposolid.
6. Clash detection and BCF issue exchange.
7. DWG export through a library (see 4.2, or LibreDWG, which is GPL and so
   would have to be an optional separate component).
8. `hausit-rvt`: map walls, floors, roofs, doors, windows and families to
   native elements, newest versions first.

**Milestone:** an architect and a structural engineer work on linked
architectural and structural models of the same building.

### Phase 4: Worksharing and MEP (about 12 to 18 months)

Goal: larger teams and MEP engineers.

1. Worksharing: worksets, central model, element borrowing, sync. Offer an
   open-source Hausit server for central models, instead of a cloud service
   only one company can run.
2. MEP foundations: spaces, zones, systems.
3. Ducts, pipes, fittings, routing, then electrical, lighting, circuits,
   panels.
4. Rebar and reinforcement.
5. Visual programming (Dynamo equivalent).
6. `hausit-rvt`: views and sheets; work back through older Revit versions.

**Milestone:** a 10-person team works on one model at the same time; an MEP
engineer models and schedules HVAC and plumbing.

### Phase 5: Full parity and long tail (ongoing)

- MEP calculations (sizing, loads), fabrication parts.
- Adaptive components, conceptual mass families.
- Point clouds, gbXML, COBie, USD.
- Rendering through export to Blender or another open renderer.
- `hausit-rvt`: coverage back to the oldest versions practices still hold
  (around Revit 2011), and RFA family import into the family editor.
- Optional ODA-based commercial RVT plug-in, if funding allows.

**Milestone:** an RVT file from any supported Revit version opens with walls,
floors, roofs, doors, windows, families, levels, views and schedules.

### Timeline summary

The estimates assume a small, steady team (two to four full-time
developers). Revit is the product of hundreds of engineers over more than
20 years, so full parity is a multi-year effort. The phases are ordered so
that Hausit becomes useful well before parity: Phase 1 makes it usable for
small projects, Phase 2 makes it a practical replacement for many
architects.

| Phase | Focus | Rough duration |
|---|---|---|
| 0 | Groundwork | 1 month |
| 1 | First real project | 6 months |
| 2 | Architectural practice replacement | 9 to 12 months |
| 3 | Structure and coordination | 9 months |
| 4 | Worksharing and MEP | 12 to 18 months |
| 5 | Parity and long tail | Ongoing |

---

## 7. How to keep this roadmap honest

- Each feature lands with a test and a README update in the "What works"
  table.
- The RVT version-coverage table only lists versions that pass tests on real
  files.
- Greyed-out ribbon buttons are the in-app record of what is planned; when a
  feature ships, the button turns on.
- Review this file at the end of each phase.
