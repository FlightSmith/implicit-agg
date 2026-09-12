# Aircraft Parametric Design System

This repository is a design pack for recreating an nTop-inspired, live parametric aircraft-concept tool. It deliberately reproduces useful engineering principles, not nTop code, branding, file formats, or visual identity.

Version 1 is a robust fixed-wing **half-model generator**. The source half lies on the negative local-Y side of its wing frame; a full model mirrors it across that frame's local XZ plane. The first supported component is a multi-station wing. It can represent straight, swept, forward-swept, cranked, delta, tailless, and winglet-like planforms.

The source definition has one global, immutable unit system. It stores design intent only: no mesh cache, calculated metrics, UI slider ranges, or per-value units.

## Design pack

- [Product scope and fixed decisions](docs/01-product-scope.md)
- [System architecture](docs/02-system-architecture.md)
- [Aircraft JSON contract](docs/03-aircraft-json-contract.md)
- [Live parameter graph](docs/04-live-parameter-graph.md)
- [Geometry pipeline](docs/05-geometry-pipeline.md)
- [User interface and interaction model](docs/06-user-interface.md)
- [Delivery roadmap and verification](docs/07-roadmap-and-verification.md)
- [References and clean-room boundary](docs/08-references-and-clean-room.md)
- [Version 4 imported-solid Boolean plan](docs/09-version-4-imported-solid-booleans.md)
- [Consolidated implementation specification](docs/superpowers/specs/2026-09-11-aircraft-parametric-design-system-design.md)
- [Implementation plan](docs/10-implementation-plan.md)

## Machine-readable assets

- [Aircraft definition JSON Schema](schemas/aircraft-definition-v0.1.schema.json)
- [Wing component catalog](catalog/wing-component.v0.1.json)
- [Cranked-wing example](examples/cranked-wing.v0.1.json)
- [Optional presentation sidecar example](examples/cranked-wing.presentation.v0.1.json)

## Key decisions

- Aircraft axes are **X aft, Y starboard, Z up**.
- The half-model is authored from the root at local `y = 0` to negative local Y. The positive local-Y half is an optional local-XZ reflection.
- Global units are selected when an aircraft document is created and are locked thereafter.
- A wing consists of named root, kink, tip, or additional stations. Station positions may be literal, parameter references, or dimensional expressions.
- Parameters are raw scalars. Their dimensions come from the typed fields that consume them; units are never repeated per parameter.
- UI range policy lives in a versioned component catalog or a non-geometric presentation sidecar, never in the aircraft definition.

## Non-goals for version 1

- Full aircraft assembly, fuselage, inlet, duct, engine, and exhaust geometry.
- CFD, FEM, optimization, certification, or structural sizing claims.
- Generic B-rep CAD editing or arbitrary imported-solid booleans before Version 4.
- Exact copies of any nTop UI, file format, proprietary algorithm, or brand asset.

Those capabilities have explicit extension points so the future buried-engine / top- or side-intake aircraft workflow does not require a data-model rewrite. Version 4 adds robust Boolean operations on arbitrary imported solids.
