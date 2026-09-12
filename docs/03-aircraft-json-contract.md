# Aircraft JSON Contract

## Design intent

The aircraft definition is human-readable JSON that describes one aircraft study, not a parametric product family. It is intentionally smaller than CPACS: a component tree, named profiles, numeric inputs, references, and expressions. It preserves the useful CPACS ideas of components, segments, profiles, transforms, and identifiers without adopting XML or a large all-aircraft vocabulary.

The canonical source format is `aircraft-definition/v0.1`. Its JSON Schema validates structure; semantic validation applies the rules in this document.

## Top-level shape

```text
schema, id, name
units, unitSystemLocked, coordinateSystem
parameters, airfoils, components
analysisCases (optional)
```

`id` and every component, airfoil, and station ID are stable identifiers. Display names may change without breaking expressions. Do not use array indexes as references.

## Global units

The required `units` object appears once. Its values are fixed for the document lifecycle.

```json
"units": {
  "length": "m",
  "angle": "deg",
  "mass": "kg",
  "force": "N",
  "pressure": "Pa"
},
"unitSystemLocked": true
```

All JSON values below it are plain numbers. Field semantics determine dimensions. No component or parameter may add a `unit`, `min`, `max`, or display-step field.

## Value forms

Every typed numeric input accepts exactly one of these forms:

```json
0.8
{ "$param": "wing.rootToKink.dx" }
"= @station.root.position.x + @param.wing.rootToKink.dx"
```

- A **literal** uses the dimension required by its field.
- A **parameter reference** reads a raw numeric value from `parameters` and inherits the target field's dimension.
- An **expression** starts with `=` and must evaluate to the target field's dimension.

Parameter values are raw numeric scalars:

```json
"parameters": {
  "wing.rootChord": 2.1,
  "wing.rootTwist": 1.5,
  "wing.tipTrailingEdgeRatio": 0.006
}
```

The core infers and records parameter dimensions from their consuming fields. A parameter may feed multiple fields only when their dimensions agree. An unreferenced parameter is a warning; an angle parameter used as a chord is an error.

## Wing definition

A v0.1 wing has a local frame and an ordered list of named stations. For a symmetric wing, source stations run from the centerline root toward negative local Y. The root belongs on local XZ; later stations must progress outboard in negative local Y unless an explicitly future-supported topology says otherwise.

| Station property | Meaning | Dimension |
| --- | --- | --- |
| `position.x`, `.y`, `.z` | Leading-edge reference point in wing-local coordinates | Length |
| `chord` | Local airfoil chord | Length |
| `twist` | Local chord rotation about the configured twist axis | Angle |
| `airfoil` | ID of a normalized profile | Identifier |
| `trailingEdge` | Local sharp, absolute, or chord-relative closure | Length or ratio |

The normal station order is `root`, optional `kink` stations, then `tip`. It is an ordering convention, not an identifier requirement. Expressions may reference a named upstream station; a topological sort, not file order, determines evaluation order. The required `frame.origin` expresses the local root frame in the user-selected aircraft coordinates.

### Symmetry

```json
"symmetry": {
  "enabled": true,
  "plane": "local-xz",
  "sourceSide": "negative-local-y",
  "centerlineTreatment": "weld"
}
```

When enabled, the source half is reflected across the wing's local XZ plane for the full geometry. `centerlineTreatment: "weld"` identifies root vertices on that symmetry plane as shared topology. It prevents duplicate faces and non-manifold centerline seams. This local frame preserves the same symmetry behavior even if the aircraft document origin is a tip or another off-center datum.

### Trailing edge

Use a discriminated object so a raw number never has an ambiguous meaning:

```json
{ "mode": "sharp" }
{ "mode": "absolute", "thickness": 0.008 }
{ "mode": "chordFraction", "value": 0.006 }
```

Absolute thickness uses the document's global length unit. `chordFraction` is dimensionless and usually much smaller than one. A profile generator must report an error if requested thickness cannot be achieved without profile self-intersection.

## Profiles

Version 1 supports two profile forms:

```json
{ "kind": "naca4", "code": "2412" }
{ "kind": "coordinates", "points": [[1, 0], [0.5, 0.06], [0, 0]] }
```

Coordinate profiles are normalized: leading edge near `(0, 0)`, trailing edge near `(1, 0)`, and coordinates represent fractions of chord. The import routine repairs common ordering and closure issues only when the repair is lossless; otherwise it reports a profile diagnostic.

## Interfaces and future components

Components can publish named interfaces without exposing their implementation details. Version 1 uses `station-plane` for the root attachment. Later fuselage, inlet, duct, engine, and exhaust components may consume interfaces such as planes, points, axes, surface regions, and clearance volumes.

```json
"interfaces": {
  "root-attachment": { "kind": "station-plane", "station": "root" }
}
```

This keeps phase-3 intake placement declarative: an inlet can depend on the root plane or a kink location rather than on hardcoded mesh coordinates.

## What the source does not serialize

- Interactive or export meshes.
- Derived planform metrics or analysis results.
- Viewport camera, panel state, selection, or UI slider range.
- Cached profile samples and expression evaluation results.
- Machine-specific paths.

Each belongs in a regenerated artifact cache, a presentation sidecar, or an external analysis result package.
