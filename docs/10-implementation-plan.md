# Implementation Plan

Status: planning baseline for Version 1 (Milestones 0–2 of the [roadmap](07-roadmap-and-verification.md)). Written 2026-09-12 against the current design pack.

## 1. Where the project stands

The repository is a complete design pack and nothing else:

- Nine specification documents, including a consolidated design spec and the Version 4 Boolean plan.
- Normative machine assets: `aircraft-definition/v0.1` JSON Schema, wing component catalog, cranked-wing example, presentation sidecar example.
- No implementation code, no build tooling, no git commits yet (all files are untracked).

Implementation therefore starts from Milestone 0. Everything in this plan defers to the fixed decisions already made in docs 01–09; it only adds engineering choices those docs leave open.

## 2. Scope

Deliver Version 1: foundation core, parametric multi-station wing geometry, and the live workspace UI. Milestones 3–5 (multi-surface, buried propulsion, imported-solid Booleans) are out of scope; the crate boundaries, named interfaces, and typed node model below are the seams they will plug into.

## 3. Technology decisions

The architecture doc fixes the boundaries and recommends Tauri 2 + React/TypeScript + Rust + WebGPU. Decisions here fill in specifics; each is replaceable without touching the boundaries.

| Layer | Choice | Notes |
| --- | --- | --- |
| Compute core | Rust, stable, Cargo workspace | One crate per architecture component; UI-independent and headless-testable. |
| JSON Schema validation | `jsonschema` crate (draft 2020-12) | Schemas loaded from `schemas/` and also embedded for tests. |
| Semantic validation, expressions, graph, geometry | Hand-written | The domain logic is the product; no generic frameworks. |
| Math | `glam` | Vec3/Mat4 only; no CAD kernel, no heavy geometry deps. |
| Serialization | `serde` / `serde_json` | Canonical pretty-printed output for source round-tripping. |
| Desktop shell | Tauri 2 | IPC commands map 1:1 onto the service API in docs/02. |
| UI | React + TypeScript + Vite, `zustand` | Thin layer over the engine; never owns geometry truth. |
| Viewport | three.js | Start on the WebGL2 renderer for compatibility; the mesh-buffer contract keeps a WebGPU swap-in cheap. |
| Testing | `cargo test`, `proptest`, snapshot tests, Playwright | Fixture-driven; see section 8. |
| CI | GitHub Actions | fmt, clippy `-D warnings`, full test suite, CLI fixture run, UI build. |

### Load-bearing design decisions

These are the choices the rest of the code will assume. Each is consistent with docs 01–05.

1. **Canonical internal units.** Parsed document values are converted once at the boundary into canonical units (meters, radians, kg, N, Pa) and converted back on serialization. Evaluation always runs in canonical units, so `sin`/`cos` never need per-node unit context and round-trip stays exact.
2. **Dimensions are exponent vectors over `{length, angle}`.** A dimension is `(nLength, nAngle)`; ratio is `(0,0)`. This generalizes the dimension table in docs/04 and correctly handles the cases a three-symbol enum cannot: `length * length -> length²`, `sqrt(length²) -> length`, `pow`. Function rules: `pow(base, e)` requires `e` to be a literal (or constant-foldable) integer; `sqrt` requires all exponents even; `sin/cos/tan` take angle, return ratio; `lerp(a, b, t)` requires `a` and `b` to share a dimension with `t` a ratio. A result binds to a field only if it matches the field's expected dimension exactly.
3. **Expression engine.** Hand-written Pratt parser for the fixed grammar: literals, `+ - * /`, parentheses, comparison operators, and `min max clamp abs sqrt pow sin cos tan lerp`. Pure, no I/O, no loops. Comparison operators parse and type-check to a boolean dimension; no v0.1 field consumes a boolean, so using one in a numeric field is a clear binding error rather than silent coercion.
4. **Reference resolution.** `@param.<id>`; `@station.<id>.position.[x|y|z]`, plus `@station.<id>.chord` and `@station.<id>.twist`; `@component.<id>.interface.<name>....` resolved through the interface registry (v0.1: `station-plane`, exposing the station plane origin). The longest-valid-identifier rule from docs/04 applies. Any unresolvable leaf is an unknown-reference diagnostic carrying the JSON path of the consumer.
5. **Incremental typed graph.** One node per typed value (station coordinates, chord, twist, trailing edge), per generated product (profile sample set, station section, loft panel, surface, mesh, metric), with edges from dependency to consumer. An edit dirties only descendants; evaluation is a topological walk with deterministic ID-order tie-breaking; cycle diagnostics print the full chain (`root -> kink -> tip -> root`). Property tests compare incremental results against a full-recompute oracle.
6. **Structured adaptive loft meshing.** Every station ring is resampled to a shared chord-sample count aligned by leading- and trailing-edge landmarks; panels share ring boundaries exactly, so the half mesh is watertight by construction. Adaptivity lives in choosing chord/span sample counts from curvature, twist change, and quality tier — not in unstructured connectivity. Every vertex carries a `(station ring, chord parameter)` source map for picking and tracing.
7. **v1 loft interpolant is linear.** The v0.1 schema has no interpolation field (see gaps below). The loft is written behind a small trait so spline interpolation lands without touching document semantics.
8. **Twist axis is the quarter chord.** v0.1 has no `twistAxis` field. Positive twist follows the right-hand rule about the local spanwise direction (the source half extends into negative local Y); a viewport axis indicator makes the sign visible, per docs/05.
9. **Tip closure is a flat cap in v1.** The schema has no tip-treatment field; the cap plus the trailing-edge modes produce the closed tip ring. Treatment variants are a schema extension, not a silent behavior.
10. **Symmetry and the half model.** The half model is built with an open boundary at local `y = 0`; mirroring reflects it across local XZ and shares the identical root ring, so the weld is exact (zero tolerance) and duplicate centerline faces are structurally impossible. A standalone half-model export caps the symmetry plane by default (option to export open).
11. **Metrics are derived only.** Reference area, span, MAC and its location, aspect ratio, taper, per-panel LE/quarter-chord/TE sweep, dihedral, enclosed volume (divergence theorem on the closed mesh), wetted area, and center of volume are computed from evaluated geometry, and every report states whether it describes the source half or the mirrored full wing.
12. **GLB orientation.** glTF is Y-up; the exporter writes a documented Z-up→Y-up root node rotation rather than reordering mesh data.

## 4. Repository layout

```text
crates/
  aircraft-model    # serde types mirroring the v0.1 schema; ID newtypes; TypedValue;
                    # diagnostic type {code, severity, jsonPath, subject, message}; unit system
  aircraft-expr     # lexer, parser, AST, dimension checker, evaluator
  aircraft-graph    # typed dependency graph, cycle check, deterministic topo evaluation,
                    # dirty propagation, parameter dimension inference
  aircraft-geom     # profiles (NACA 4, coordinates), station sections, loft, mirror/weld,
                    # mesher, manifold checks, planform metrics
  aircraft-engine   # document store, validator facade, service API, transactions,
                    # cancellation tokens, content-addressed cache
  meshio            # STL (binary/ascii), OBJ, GLB writers
apps/
  cli               # validate / evaluate / mesh / export headless driver (dev + CI + fixtures)
  desktop           # Tauri 2 + React/TS workspace (Milestone 2)
schemas/  catalog/  examples/   # existing, unchanged; embedded in tests via include_str!
examples/fixtures/              # milestone fixture set (section 8)
```

## 5. Milestone 0 — Foundation

Goal: parse the supplied example, prove the data model, and fail invalid input with precise diagnostics.

1. **Housekeeping.** Initial commit of the design pack; workspace scaffold; CI; formatting and lint gates.
2. **`aircraft-model`.** Serde types for the full v0.1 schema (typed values as `Literal | ParamRef | Expr`), ID newtypes, the diagnostic record carrying JSON paths and parameter/station subjects, global unit-system type.
3. **Schema validation.** Validate documents against `schemas/aircraft-definition-v0.1.schema.json`; map validator output onto path-annotated diagnostics.
4. **`aircraft-expr`.** Lexer, Pratt parser, AST printer (round-trip), dimension checker, f64 evaluator. Non-finite results are errors at the node, not NaNs propagating downstream.
5. **`aircraft-graph`.** Build the typed graph from a validated document; infer parameter dimensions from consuming fields (multi-consumer agreement required; unreferenced parameter = warning); cycle detection with the dependency chain in the message; deterministic topological evaluation; dirty propagation from edited nodes.
6. **Semantic validator.** Root on local `y = 0` within tolerance for symmetric wings; strictly decreasing negative local Y between consecutive stations; strictly positive chords; unique station/airfoil/component IDs; profile references exist; trailing-edge feasibility pre-checks; frame/symmetry consistency.
7. **Catalog and sidecar types.** Loader for `component-catalog/v0.1` (field dimensions, constraints, editor kinds, range policies) and typed structs for the presentation sidecar.
8. **`apps/cli`.** `validate` and `evaluate` (dump evaluated stations, inferred dimensions, dependency paths). This gives CI and fixtures a UI-free end-to-end driver from day one.

Exit (from docs/07): the example validates; an edited cascade (`wing.rootToKink.dx`) recomputes kink and tip correctly; invalid roots, cycles, dimension mismatches, and unknown references fail with JSON-path diagnostics. Unit tests cover global-unit interpretation and rejection of inline units.

## 6. Milestone 1 — Parametric half wing

Goal: a headless engine that turns a validated document into watertight half/full meshes, metrics, and exports, with live-update semantics.

1. **Profiles.** NACA 4-series generator (cosine spacing, camber line plus thickness distribution) and the normalized-coordinates loader: lossless repair of ordering and closure issues only, otherwise a profile diagnostic. Resampling to a shared sample count aligned by LE/TE landmarks.
2. **Station sections.** Scale by chord; rotate by twist about the quarter chord (right-hand rule about the local spanwise direction); apply trailing-edge closure (`sharp`, `absolute` length, `chordFraction` ratio) with an explicit error when the requested thickness would self-intersect the profile; place at the evaluated station position.
3. **Loft and cap.** Linear interpolation between rings (behind the interpolant trait); per-panel quad/triangle generation with shared ring edges; flat tip cap.
4. **Mirror, weld, validate.** Reflect across local XZ; exact root-ring weld; consistent outward normals. Manifold validator: closed surface, oriented and edge-manifold, no degenerate triangles.
5. **Mesher.** Interactive and export quality tiers with the documented controls (chord/span samples and curvature tolerance for preview; maximum chordal deviation, maximum edge length, normal-angle tolerance for export); adaptive sample-count selection; vertex-to-source map.
6. **Metrics.** The full planform and volume report from decision 11, with golden-reference tests against analytic values for a rectangular wing, a straight-tapered trapezoid, and the cranked example.
7. **`meshio`.** Binary/ASCII STL, OBJ, GLB.
8. **`aircraft-engine`.** Immutable document snapshots; `applyPatch` with transaction IDs; `evaluate`, `mesh(quality)`, `export`, `traceSelection`; cancellation tokens and source-hash checks so a stale job can never replace a newer preview; content-addressed cache keyed by evaluated inputs + quality + generator version.

Exit (from docs/07): every fixture exports a manifold half and full mesh; changing any root or kink relationship updates the expected downstream stations and metrics; import/export round-trips preserve source semantics.

## 7. Milestone 2 — Usable live workspace

Goal: the docs/06 experience on top of the headless engine.

1. **Tauri 2 scaffold.** IPC commands mirroring the service API (`openDocument`, `validate`, `applyPatch`, `evaluate`, `mesh`, `export`, `traceSelection`); events for preview/metric updates; TypeScript bindings.
2. **Workspace shell.** Three-pane layout: design tree (components and named stations), viewport, inspector; formula/dependency trace panel along the bottom.
3. **Viewport.** three.js scene from engine mesh buffers; half/full toggle; symmetry-plane overlay; station labels; mesh picking resolving to stations and loft panels through the source map; visible pending state during recompute, never replacing a valid mesh with a failed one.
4. **Editing.** Three-mode inputs (literal / parameter / expression) with autocomplete over parameters, stations, components, and interfaces; catalog-driven adaptive ranges with sidecar overrides and "reset to catalog behavior"; transient drag patches with debounced preview and cancellation; undo/redo and a committed-transaction history.
5. **Files.** Open/save aircraft JSON with the import report (units, components, warnings, blocking errors); export source JSON, half/full mesh (STL/OBJ/GLB), and metrics report; sidecar create/edit.
6. **E2E and performance.** Playwright flows: open example, edit the cascade, mirror, export, reopen. Profile a representative drag edit; target interactive preview well under the debounce interval (~100 ms) on the documented reference machine, with stale jobs demonstrably cancelled.

Exit (from docs/07): an engineer can create, edit, save, reopen, mirror, and export a wing without touching raw JSON, while JSON remains the transparent interchange format.

## 8. Fixture set and verification mapping

Fixtures live under `examples/fixtures/` and run through the CLI in CI:

| Fixture | Expected |
| --- | --- |
| Rectangular wing, symmetric NACA profile | Analytic area/span/MAC/AR metrics; manifold half+full |
| Tapered swept wing, non-zero root/tip twist | Golden metrics; manifold mesh |
| Cranked wing (the supplied example) | Baseline integration fixture; cascade recompute |
| Forward-swept wing | Loft and metrics valid |
| Delta-like strong taper | Loft and metrics valid |
| Root off the symmetry plane | Semantic rejection before meshing |
| Expression cycle | Cycle diagnostic with the chain |
| Angle used as length | Dimension-mismatch diagnostic with JSON path |
| TE thickness beyond profile feasibility | Profile feasibility diagnostic |

Every valid fixture additionally runs: round-trip import/export semantic equality, manifold checks on half and full meshes, and stale-cancellation integration tests on the engine.

This reproduces the verification matrix in docs/07 row by row; each row names its owning crate and test file in the corresponding milestone.

## 9. Spec gaps found while planning

Open items in the v0.1 assets, each with a recommendation. None blocks Milestone 0.

1. **No wing `interpolation` field** although linear/spline lofting is promised. → Ship linear in v0.1; add an optional `"interpolation": "linear" | "spline"` in a v0.2 schema revision (the example remains valid; the default keeps old documents' meaning).
2. **No `twistAxis` field** although docs/05 defines it. → Fix quarter-chord for v0.1; add the optional field later.
3. **No tip-treatment field** although product scope mentions tip treatment. → Flat cap in v0.1; variants as a schema extension.
4. **No JSON Schema for the component catalog or the presentation sidecar** even though both are machine-consumed contracts. → Add `component-catalog-v0.1.schema.json` and `aircraft-presentation-v0.1.schema.json`.
5. **Half-model symmetry-plane cap on export is unspecified.** → Default to a capped (closed-solid) half export with an open-boundary option.
6. **Comparison operators have no v0.1 consumer.** → Parse them, but binding one to a numeric field is a diagnostic, not a silent truthiness coercion.
7. **`sqrt`/`pow` dimensional semantics are unspecified.** → Exponent-vector dimensions with integer-literal exponents, as decided above.
8. **GLB is Y-up while the document is Z-up.** → Documented root-node rotation at export.

Each should get a one-line decision recorded in the relevant doc (or folded into the v0.2 schema revision) before the corresponding feature ships.

## 10. Risks

| Risk | Mitigation |
| --- | --- |
| Loft/mesher corner cases at aggressive transitions | Fixture-driven development from the start; localized station/panel diagnostics; explicit tolerances |
| Incremental graph diverges from full recompute | Property tests with a full-recompute oracle over random edit sequences |
| UI/engine integration churn | Engine stays headless-testable; the Tauri layer is a thin command map; Playwright guards the workflow |
| Scope creep toward a CAD kernel | Crate boundaries and the clean-room non-goals enforce it; implicit bodies and Booleans stay future node classes |
| Preview latency regressions | Benchmark the fixture wing in CI; cache keyed by content hash; cancellation paths tested |

## 11. Suggested commit sequence

1. Commit the design pack as the baseline.
2. Workspace scaffold + CI.
3. `aircraft-model` + schema validation.
4. `aircraft-expr`.
5. `aircraft-graph` + semantic validator.
6. CLI `validate`/`evaluate` + Milestone 0 exit tests.
7. Profiles.
8. Sections, loft, tip cap.
9. Mirror, weld, manifold validation.
10. Metrics + golden fixtures.
11. `meshio`.
12. Engine facade, cancellation, cache; Milestone 1 exit tests.
13. Tauri scaffold + IPC.
14. Design tree, inspector, diagnostics, trace panel.
15. Viewport.
16. Live editing, undo/redo, sidecar/catalog controls.
17. Import/export UX; Milestone 2 exit tests and performance pass.
