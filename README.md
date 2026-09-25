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

## Implementation status

Milestones 0, 1, and 2 are implemented as a Rust workspace plus a web workspace:

- `crates/aircraft-model` — typed v0.1 document model, JSON Schema validation, semantic checks.
- `crates/aircraft-expr` — the typed dimensional expression language (parser, checker, evaluator).
- `crates/aircraft-graph` — the dependency graph: reference resolution, parameter dimension inference, cycle detection, deterministic evaluation, physical predicates.
- `crates/aircraft-geom` — profiles (NACA 4-series, normalized coordinates with lossless repair), station sections with twist and trailing-edge closure, lofting, symmetry mirror and centerline weld, manifold validation, adaptive meshing, planform and volume metrics.
- `crates/meshio` — binary/ASCII STL, OBJ, and GLB export.
- `crates/aircraft-engine` — the service API: transactional patches with affected-node reporting, cancellation tokens and stale-revision guards, content-addressed mesh cache, selection tracing, derived reports.
- `crates/aircraft-wasm` — the engine bound for WebAssembly; the browser compute core, with a command surface mirroring the planned Tauri IPC API.
- `apps/desktop` — the live workspace (React + TypeScript + three.js via Vite): design tree, 3D viewport with symmetry-plane overlay and mesh picking traced to source stations, an inspector with literal/parameter/expression value modes, adaptive sliders, undo/redo, live diagnostics, and STL/OBJ/GLB export.
- `apps/cli` — headless `validate`, `evaluate`, `report`, and `export` (STEP/STL/OBJ/GLB batch writer) commands; the fixture harness under `examples/fixtures/`.

### Running the live workspace

```sh
cd apps/desktop
npm install
npx playwright install chromium   # one-time, for the E2E suite
npm run dev                       # wasm core + vite dev server on :5173
npm run e2e                       # headless E2E suite against the production build
```

The compute core is the same Rust engine compiled to `wasm32-unknown-unknown`
and regenerated automatically by `npm run core:wasm`. The UI talks to it only
through the `CoreApi` interface in `src/core/api.ts`, so a Tauri 2 desktop
shell can replace the backend without UI changes; the shell is deferred until
a build environment with `libwebkit2gtk-4.1-dev` is available.

Milestone 3+ (multi-surface assembly, propulsion, imported-solid Booleans)
comes next; see the [implementation plan](docs/10-implementation-plan.md).

### Building and testing

```sh
cargo test --workspace                 # unit, integration, and fixture tests
cargo clippy --workspace --all-targets # lint gate (CI runs this with -D warnings)
cargo run -p aircraft-cli -- validate examples/cranked-wing.v0.1.json
cargo run -p aircraft-cli -- evaluate examples/cranked-wing.v0.1.json
cargo run -p aircraft-cli -- report examples/cranked-wing.v0.1.json
```
