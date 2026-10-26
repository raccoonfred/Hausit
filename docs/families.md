# Hausit families (`.hfam`)

Furniture, casework, plumbing fixtures, equipment and other components
are **loadable families**. Each family is a small JSON file with the
`.hfam` extension. It describes:

- the family's parameters,
- its types, which are named sets of parameter values,
- its solids and plan symbol, both written as formulas over those
  parameters.

There are two ways to load families:

- **Insert ▸ Load Family** loads `.hfam` files from disk.
- **Insert ▸ Library** loads families from Hausit's starter library.

You place a loaded family with **Component** (`CM`) on the Architecture
tab. As in Revit, a loaded family is stored inside the project. Loading a
family again replaces its definition and adds any types that are missing,
while types already in the project keep their values.

The starter library lives in
[`crates/hausit-model/library/`](../crates/hausit-model/library). It was
written for Hausit and is a good place to start when you write your own
families.

## Coordinates

All lengths are in millimetres. The x axis points right and the y axis
points to the back. The family's front faces −y, so a sofa or cabinet
sits at y ≤ 0 with its back on y = 0. z points up from the insertion
point.

When you place the family, its insertion point goes where you click, at
the level plus the instance's Offset. Rotation turns it in plan.

## File layout

```json
{
  "name": "Table - Rectangular",
  "category": "Furniture",
  "description": "Rectangular table on four legs.",
  "parameters": [
    { "name": "Width",    "type": "Length",   "default": 1600 },
    { "name": "Depth",    "type": "Length",   "default": 900 },
    { "name": "Height",   "type": "Length",   "default": 740 },
    { "name": "Material", "type": "Material", "default": "Wood - Oak" }
  ],
  "types": [
    { "name": "1200 x 800mm", "values": { "Width": 1200, "Depth": 800 } },
    { "name": "1600 x 900mm" }
  ],
  "solids": [
    { "shape": "Box",
      "min": ["-Width / 2", "-Depth / 2", "Height - 30"],
      "max": ["Width / 2",  "Depth / 2",  "Height"],
      "material": "Material" }
  ],
  "plan": [
    { "shape": "Rectangle", "min": ["-Width / 2", "-Depth / 2"], "max": ["Width / 2", "Depth / 2"] }
  ]
}
```

### `category`

The category must be one of the following:

- `Furniture`
- `Casework`
- `PlumbingFixtures`
- `SpecialtyEquipment`
- `GenericModels`

### `parameters`

Each parameter has a `name`, a `type` and a `default`. The available
types are:

- `Length`
- `Integer`
- `Number`
- `Angle`
- `Yes/No`
- `Text`
- `Material`

Parameter names may contain spaces, for example `"Seat Height"`.

Parameters become the type's parameters in the project, so you can change
them in Edit Type. When you do, the geometry follows.

### `types`

Each type has a name and the values it changes. Any value a type does not
give comes from the parameter's default.

### Formulas

Wherever a number is expected, you can write either a number or a formula
as a string. Formulas support the following:

- **Arithmetic:** `+ - * /` and parentheses.
- **Comparisons:** `> < >= <= == !=`. A comparison gives 1 when true and
  0 when false.
- **Functions:** `min`, `max`, `abs`, `sqrt`, `round`, `floor`, and
  `if(condition, then, else)`.
- **Parameter names:** a parameter can be used by its name. A `Yes/No`
  parameter counts as 1 or 0.

### `solids`

Every solid has a `shape`, which is one of the following:

| `shape` | Fields |
|---|---|
| `Box` | `min` and `max` give opposite corners as `[x, y, z]`. |
| `Extrusion` | `profile` is a list of `[x, y]` points, extruded from `z[0]` to `z[1]`. |
| `Cylinder` | `center` is `[x, y]`, with a `radius`, extruded from `z[0]` to `z[1]`. |

Any solid can also have these optional fields:

- **`material`:** a material name, such as `"Stainless Steel"`, or the
  name of a `Material` parameter. Hausit adds library materials to the
  project when a family uses them.
- **`visible`:** a formula. The solid is shown only when the formula is
  not 0, for example `"Screen"` or `"Load Height > 0"`.
- **`repeat`:** written as `{ "count": formula, "step": [dx, dy, dz] }`.
  It makes `count` copies of the solid, each moved by `step` from the
  one before. Shelves, legs and racking beams are made this way.

### `plan`

These are the lines that floor plans draw for the family, in place of
the outlines of its solids. Each symbol has a `shape`, which is one of
the following:

| `shape` | Fields |
|---|---|
| `Line` | `from`, `to` |
| `Rectangle` | `min`, `max` |
| `Polyline` | `points`, `closed` |
| `Circle` | `center`, `radius` |
| `Arc` | `center`, `radius`, `start`, `end`. Angles are in degrees. |

If you leave `plan` out, plans draw the outline of every solid. A
component drawn entirely above the plan's cut plane, such as a wall
cabinet, is shown dashed.

## IFC

When you export to IFC4, components are written as follows:

- Furniture and casework become `IfcFurniture`.
- Plumbing fixtures become `IfcSanitaryTerminal`, with WC, basin, bath,
  shower and sink recognised from the family name.
- Equipment and generic models become `IfcBuildingElementProxy`.

In every case the family type is exported, and the element keeps its
materials and parameters.
