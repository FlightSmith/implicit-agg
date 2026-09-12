# Aircraft Parametric Design System Design

## Decision

Create a local-first, nTop-inspired conceptual aircraft design tool whose first deliverable is a live parametric fixed-wing generator. The canonical interchange format is a compact JSON document for one aircraft study. Version 1 is a negative-local-Y wing half-model with an optional local-XZ mirrored full model.

## Fixed conventions

- Right-handed axes: X aft, Y starboard, Z up.
- The root station is on local XZ at local `y = 0`; source stations extend into negative local Y.
- The origin is a named author-selected aircraft datum; the wing frame locates local station geometry within it.
- Units are global and immutable after document creation. No inline units appear in the source.
- Parameters are raw values. Their dimensions are inferred from typed consumers.
- A numeric field may be a literal, a parameter reference, or a typed expression.
- Wing stations are named and may cascade: root to kink to tip.
- The aircraft source excludes UI ranges, cache data, and derived outputs.

## Architecture

An immutable document store feeds a schema/semantic validator and a typed dependency graph. The graph evaluates expressions, then a wing generator creates surface data, preview/export meshes, and metrics. The renderer displays only produced mesh data. A separate versioned component catalog governs validation details and adaptive UI control policy.

Every update is transactional. A valid edit commits one snapshot revision; invalid edits retain the previous valid source. Cancellation tokens and source hashes prevent stale geometry work from replacing a newer preview.

## Data model

The source document has global units, raw named parameters, airfoils, components, and optional analysis intent. A wing contains symmetry settings, named stations, local profiles, trailing-edge treatment, and published interfaces. The v0.1 JSON Schema and worked example are the normative machine-readable assets in this repository.

The geometry canonical form is stations, not duplicate span/area/sweep/taper inputs. These values are derived reports. UI sizing helpers may edit station values, but may not add conflicting source drivers.

## Version 1 scope

Implement NACA 4-series and normalized-coordinate profiles; linear/spline station lofting; chord, root/kink/tip twist; trailing-edge control; centerline welding; preview/export meshing; and planform metrics. Support import/export of the canonical JSON plus standard mesh formats.

## Future compatibility

Fuselage, buried engine, top/side inlet, diffuser, duct, exhaust, clearance, and implicit-body components will be added as new node kinds. They attach through named interfaces and expressions, preserving existing wing documents. Version 4 adds imported-solid asset nodes and reproducible union, subtraction, intersection, offset, and blend operations; see [the dedicated plan](../../09-version-4-imported-solid-booleans.md).

## Verification

Automated tests cover JSON, semantic constraints, unit dimensions, dependency cycles, symmetry welding, mesh manifoldness, metric fixtures, live update cancellation, and source round-tripping. The cranked-wing example is a baseline integration fixture.

## References within this design pack

- [Product scope](../../01-product-scope.md)
- [Architecture](../../02-system-architecture.md)
- [JSON contract](../../03-aircraft-json-contract.md)
- [Live graph](../../04-live-parameter-graph.md)
- [Geometry pipeline](../../05-geometry-pipeline.md)
- [UI model](../../06-user-interface.md)
- [Roadmap](../../07-roadmap-and-verification.md)
