# Delivery Roadmap and Verification

## Milestone 0 Foundation

Deliver a small core that can parse the source example and prove the data model.

- Rust types mirroring the JSON Schema.
- JSON Schema validator and semantic validator.
- Typed expression parser, evaluator, dependency sorter, and cycle diagnostics.
- Component catalog loader with adaptive range policy definitions.
- Unit tests for global-unit interpretation and no-inline-unit enforcement.

**Exit:** the supplied example validates, an edited cascade recomputes correctly, and invalid roots/expressions fail with JSON-path diagnostics.

## Milestone 1 Parametric half wing

- NACA 4-series and normalized-coordinate profile generators.
- Root/kink/tip and arbitrary station model.
- Chord, twist, trailing-edge treatment, and XZ symmetry weld.
- Watertight interactive mesh and high-quality export mesh.
- Derived planform metrics.

**Exit:** each test wing exports a manifold half and full mesh; changing any root or kink relationship updates the expected downstream station coordinates and metrics.

## Milestone 2 Usable live workspace

- Design tree, viewport, inspector, expression editor, diagnostics, and source tracing.
- Transient drag edits, debounced preview, cancellation, undo/redo.
- Import/export dialog and source-only round-trip.
- Presentation sidecar and catalog range overrides.

**Exit:** an engineer can create, edit, save, reopen, mirror, and export a wing without using raw JSON, while JSON remains the transparent interchange format.

## Milestone 3 Analysis-ready extensions

- Multiple lifting surfaces and control-surface definitions.
- Low-order aerodynamic interfaces and analysis-case contract.
- Fuselage and station-plane attachment components.
- Content-addressed mesh and report artifacts.

**Exit:** a conventional or tailless fixed-wing aircraft can assemble from independent components without leaking internal mesh data across boundaries.

## Milestone 4 Integrated propulsion geometry

- Fuselage/centerbody, buried engine envelope, top/side intake, diffuser, duct, and exhaust components.
- Named flow-path and clearance interfaces.
- Implicit-body evaluation and blending strategy where lofts are insufficient.
- Duct continuity, clearance, and mesh-quality diagnostics.

**Exit:** an intake location can depend on wing kinks or fuselage datums and update live without manual rework of adjacent geometry.

## Milestone 5 Version 4 imported-solid Booleans

- Import validated STEP/IGES B-rep solids and watertight mesh solids through a versioned asset manifest.
- Preserve source checksum, import options, repair report, unit interpretation, and conversion tolerance.
- Convert approved external solids into bounded implicit-body nodes with quantified approximation error.
- Implement explicit union, subtraction, intersection, offset, and blend graph operations.
- Provide tolerance, topology, self-intersection, and stale-asset diagnostics before committing a Boolean result.
- Export final meshes and retain a trace from every Boolean result to operands, source assets, and settings.

**Exit:** a user can import an arbitrary validated solid, bind it to aircraft interfaces, perform a reproducible Boolean with wing/fuselage/duct geometry, and receive either a manifold result or an actionable diagnostic. See [the detailed Version 4 plan](09-version-4-imported-solid-booleans.md).

## Verification matrix

| Area | Automated verification | Acceptance condition |
| --- | --- | --- |
| JSON contract | Schema tests | Valid examples pass; unknown required fields fail. |
| Units | Semantic tests | One global unit system is required; inline units and unit changes are rejected. |
| Expressions | Property and unit tests | Valid DAGs resolve deterministically; cycles and dimension errors identify the chain. |
| Symmetry | Geometry tests | Root is on local XZ; mirrored mesh has no duplicate centerline faces. |
| Loft | Mesh tests | No holes, inverted panels, non-manifold edges, or degenerate triangles within tolerance. |
| Metrics | Golden-reference tests | Area, span, MAC, taper, sweep, and dihedral match trusted fixtures. |
| Live updates | Integration tests | Stale jobs cannot overwrite a newer committed revision. |
| Round trip | Snapshot tests | Import then export preserves canonical source semantics. |
| UI ranges | UI tests | Catalog/sidecar changes alter controls without changing aircraft JSON. |

## Initial fixture set

- Rectangular wing with symmetric NACA profile.
- Tapered swept wing with non-zero root and tip twist.
- Cranked wing with root-to-kink-to-tip expressions.
- Forward-swept wing.
- Delta-like wing with strong taper.
- Invalid root off the symmetry plane.
- Expression cycle.
- Invalid angle-as-length dimension mismatch.
- Trailing-edge thickness larger than the profile permits.

## Release quality gate

Before calling a release usable:

1. All schema, semantic, graph, geometry, and UI tests pass.
2. Each fixture imports and exports without semantic drift.
3. Full mirrored meshes are inspected for the centerline seam and manifold status.
4. A representative drag edit is profiled; preview latency and cancellation behavior meet the product target on a documented reference machine.
5. Diagnostics have user-facing wording and direct paths to the offending source.
6. The canonical JSON, catalog, example, and user documentation versions agree.

## Risks and mitigations

| Risk | Mitigation |
| --- | --- |
| Conflicting geometric drivers | Use station coordinates as the canonical planform source; expose alternate sizing modes only as generated/editing helpers. |
| Live recompute flicker or stale results | Immutable revisions, debounce, cancellation tokens, and source-hash result checks. |
| Expression complexity becomes a scripting surface | Keep the grammar pure, short, typed, and without I/O, loops, or user-defined functions. |
| Loft failure at aggressive transitions | Validate early, make tolerances explicit, and produce a localized station/panel diagnostic. |
| Future duct blending forces redesign | Keep geometry outputs and named interfaces typed; add implicit bodies as a new node class rather than mutating station JSON. |
| Imported-solid Boolean failure from bad external geometry | Require an asset validation/repair report, explicit approximation tolerance, and operand-level diagnostics before evaluation. |
