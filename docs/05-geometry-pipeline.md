# Geometry Pipeline

## Representation strategy

Version 1 should not begin by implementing a general CAD kernel. It needs a reliable, parameterized wing generator whose model is defined by stations and profiles. The generator owns a clear boundary:

```text
Evaluated stations and profiles -> wing surface -> watertight preview/export mesh -> metrics
```

The dependency graph must represent this result as a geometry node from day one. Phase 2 can add analytic surface and attachment nodes; phase 3 can introduce signed-distance-field or implicit-body nodes for robust ducts, inlets, and blended booleans without changing document semantics.

## Wing generation algorithm

1. Resolve station values and check root/symmetry rules.
2. Create a local 2D airfoil curve at each station from NACA parameters or normalized coordinates.
3. Apply local profile scaling by chord, profile rotation by twist, and local trailing-edge closure.
4. Place the station curve at its wing-local leading-edge location.
5. Match equal upper/lower sample counts and align curves by leading and trailing-edge landmarks.
6. Loft between neighboring stations with a consistent spanwise interpolant.
7. Close the tip according to its selected treatment.
8. For a full model, reflect the validated half mesh over local XZ and weld centerline vertices before applying the wing-frame transform and calculating normals.
9. Generate metrics from the unambiguous half/full semantic selected by the report.

## Twist definition

Version 1 twists an airfoil around its local quarter-chord point unless `twistAxis` is explicitly set to another chord fraction. Positive sign follows the right-hand rule around the local spanwise direction; the UI must show a small axis indicator rather than relying on sign intuition.

Root twist is not special. The root station has a normal `twist` field and participates in all spanwise interpolation.

## Planform metrics

Do not save these as source inputs in station mode. Compute them from evaluated geometry:

- Half and full projected reference area.
- Half and full span.
- Mean aerodynamic chord and its location.
- Aspect ratio using full span and full reference area.
- Root-to-tip taper ratio where defined.
- Leading-edge, quarter-chord, and trailing-edge sweep per panel.
- Dihedral per panel, derived from station placements.
- Approximate enclosed volume, wetted area, and estimated center of volume.

The report must declare whether a number represents the source half or mirrored full wing.

## Meshing

Use a parameter-adaptive mesh rather than a fixed global tessellation. Increase sampling where curvature, profile change, twist change, or station spacing demands it. Guarantee shared edge vertices between adjacent panels so the mesh is watertight before symmetry weld and export.

Quality controls are export options, not aircraft parameters:

```text
preview: chord samples, span samples, curvature tolerance
export: maximum chordal deviation, maximum edge length, normal-angle tolerance
```

The mesh service should expose a stable vertex-to-source map so mesh picking traces to a station, profile, or loft panel.

## Validation before meshing

Reject, rather than attempt to repair silently:

- Station chord less than or equal to zero.
- Symmetric root farther than a positional tolerance from local `y = 0`.
- An outboard station whose local Y coordinate is not more negative than its upstream neighbor.
- Degenerate station separation.
- Profile self-intersection after a requested trailing-edge thickness.
- Loft surface inversion or non-manifold seam.

Repair is allowed only for explicitly selected presentation operations such as resampling an imported profile; the source change must be visible and undoable.

## Phase 3 implicit geometry

Buried engines and integrated top/side inlets create blends and Boolean-like interactions that station lofts alone cannot reliably model. Add a separate `ImplicitBody` geometry node then:

- Surface/solid contributions evaluate as signed-distance or occupancy fields.
- Union, subtraction, blend, offset, and shell are graph operations with explicit tolerances.
- Adaptive dual contouring or a comparable feature-preserving mesher extracts display/export geometry.
- The wing station model remains a source generator that can publish an implicit representation when needed.

This staged approach gives v1 a fast, explainable wing workflow while reserving a robust path toward nTop-like field operations.

## Version 4 imported-solid Booleans

Version 4 extends the implicit-body path to external solids. The importer, asset manifest, repair contract, Boolean semantics, tolerances, and acceptance checks are specified in [the Version 4 plan](09-version-4-imported-solid-booleans.md). Imported solids remain independent graph nodes; the wing generator and existing source format do not become a general B-rep editor.
