# Version 4 Imported Solid Booleans

## Outcome

Version 4 allows an engineer to import an arbitrary external solid, place it using the same named aircraft interfaces and expression graph as native components, and combine it with native geometry through robust Boolean operations. Typical uses include fairings, landing-gear envelopes, sensor housings, engine hardware envelopes, pre-existing fuselage shells, and duct or intake packaging volumes.

This is not a commitment to build a general-purpose CAD editor. The product imports a bounded solid asset, records how it was interpreted, converts it into the native implicit-body workflow, and produces a traceable Boolean result.

## Supported source assets

| Source class | Initial formats | Acceptance requirement |
| --- | --- | --- |
| CAD solid | STEP AP203/AP214/AP242 and IGES solid entities | Closed, orientable solid after import and approved repair. |
| Triangulated solid | STL, OBJ, GLB | Closed two-manifold mesh with consistent normals and no self-intersection. |
| Native geometry | Wing, fuselage, duct, inlet, engine-envelope nodes | Evaluates successfully as a bounded implicit body. |

Version 4 does not promise useful Boolean results for open surfaces, self-intersecting meshes, corrupt B-reps, or zero-thickness sheet data. The import UI must expose these as diagnostics rather than silently inventing solid volume.

## Asset manifest

Aircraft JSON remains the source of design intent. External geometry belongs in a portable asset manifest adjacent to the aircraft document or embedded in a managed project package. It never depends on an absolute machine path.

```json
{
  "id": "engine-envelope",
  "kind": "imported-solid",
  "source": {
    "uri": "assets/engine-envelope.step",
    "sha256": "content-hash",
    "format": "step"
  },
  "units": "inherit-project-length",
  "import": {
    "repairPolicy": "validate-then-explicit-repair",
    "healTolerance": 0.0001,
    "conversionTolerance": 0.0005
  }
}
```

This is a planned V4 extension, not a v0.1 aircraft-definition object. It intentionally records tolerances so two users obtain the same result from the same asset and project.

## Graph nodes

```text
ImportedSolidAsset
  -> AssetValidationReport
  -> RepairedSolid (optional explicit repair node)
  -> ImplicitBodyFromSolid
  -> Transform / Offset / Blend
  -> BooleanUnion | BooleanSubtract | BooleanIntersect
  -> Mesh and derived metrics
```

Each node has an input hash, engine version, numerical settings, diagnostics, and source trace. A mesh selection on a Boolean result should identify the operation and its left/right operands.

## Boolean contract

| Operation | Inputs | Result |
| --- | --- | --- |
| Union | Two or more bounded implicit bodies | Occupied region of any operand. |
| Subtract | Base body and one or more tool bodies | Base region not occupied by tools. |
| Intersect | Two or more bounded implicit bodies | Shared occupied region. |
| Offset | Bounded implicit body and signed length | Expanded or contracted body, subject to local-feature diagnostics. |
| Blend | Two bodies plus radius/field control | Smooth field union with explicit blend limits. |

Operations are explicit graph nodes. Operand order matters for subtraction and must be visible in the notebook/tree. Each operation stores its tolerance and a user-facing label; it must not inherit an invisible global Boolean tolerance.

## Import and conversion pipeline

1. Copy or register the asset in the portable project manifest and calculate its content hash.
2. Parse the external file with a format-specific importer.
3. Validate closure, orientation, degeneracy, self-intersection, and unit interpretation.
4. Present a repair proposal when safe; require an explicit repair node and record its report before continuing.
5. Normalize the imported solid into a bounded representation and transform it using aircraft interfaces or expressions.
6. Convert it to an implicit body at an explicit tolerance with a recorded error bound.
7. Evaluate requested Boolean nodes and generate an adaptive mesh.
8. Verify manifoldness, disconnected components, minimum feature size, and any export tolerance.

## Diagnostics and recovery

Diagnostics must be localized and reversible:

- Source asset changed: content hash mismatch; offer reload as a new revision.
- Invalid or open geometry: identify the source entity/mesh region where possible.
- Ambiguous units: block evaluation until the user selects an interpretation.
- Repair would change topology: show the consequence and require an explicit user choice.
- Conversion error exceeds requested tolerance: identify resolution requirements and memory cost.
- Boolean result is empty, disconnected, non-manifold, or below minimum feature size: retain the prior valid preview and show the failing operation.

No repair, unit scaling, tolerance relaxation, or operand replacement may happen silently.

## Integration with buried propulsion

The native phase-3 duct and intake components remain parametric source geometry. Version 4 permits imported engine envelopes, compressor-face models, equipment volumes, and external constraints to interact with them. For example, a subtract node can remove an imported engine envelope from a native fuselage implicit body, while a blend node creates a controlled transition into a top intake. The result retains both the wing/fuselage parameter trace and imported asset provenance.

## Verification

- Golden imported STEP and STL fixtures with expected validation reports.
- Re-import test proving unchanged content hash has deterministic conversion output.
- Unit-interpretation tests for millimetre and metre assets under each permitted global project length unit.
- Boolean fixture matrix: union, subtraction, intersection, offset, and blend against native and imported operands.
- Stress fixtures with near-tangent bodies, thin features, small gaps, and high-curvature regions.
- Mesh manifoldness, normal orientation, error-bound, and source-trace checks.
- Portable-project test proving an asset package opens without machine-specific absolute paths.

## Exit criteria

Version 4 is ready when a user can import a validated solid, locate it through a named aircraft interface, perform a persisted Boolean operation, reopen the project on another machine, and regenerate either the same manifold result or the same actionable diagnostics.
