# Product Scope and Fixed Decisions

## Purpose

Build a dependable conceptual-aircraft design environment in which an engineer edits named parameters and immediately sees the resulting geometry. The user experience should feel stable under frequent edits: every visible result has a traceable source, invalid edits fail with a useful diagnostic, and one change recomputes only its downstream dependents.

The product is inspired by the engineering qualities of field-driven, notebook-style modeling. It is not an attempt to duplicate nTop's proprietary product, implementation, or user interface.

## Version 1 outcome

Version 1 creates and exports a fixed-wing half-model and its optional mirrored full model. It supports a single multi-station main wing with:

- Root, kink, tip, and arbitrary additional named stations.
- Linear or spline-interpolated chord, twist, leading-edge location, and profile transitions.
- NACA 4-series airfoils plus normalized imported 2D coordinate profiles.
- Root twist, local station twist, absolute or chord-relative trailing-edge thickness, tip treatment, and XZ symmetry.
- Literal values, named parameters, and safe expressions for cascade relationships.
- Read-only calculated area, span, MAC, aspect ratio, taper, leading- and quarter-chord sweep, dihedral, volume estimate, and mesh statistics.
- Preview mesh and export mesh generation.

This station-based definition covers conventional wings as well as cranked, forward-swept, delta, tailless, and blended planform studies. It does not imply that every shape is structurally or aerodynamically viable.

## Aircraft coordinate convention

The document uses a right-handed aircraft frame:

| Axis | Direction | Meaning |
| --- | --- | --- |
| X | aft | Positive values move toward the tail. |
| Y | starboard | Positive values move to the aircraft's right. |
| Z | up | Positive values move upward. |

Each wing owns a local frame whose axes inherit the aircraft directions. The design source is the negative local-Y half. A root station lies on its local XZ plane, so its local `y` coordinate is exactly zero. The full model is produced by reflecting the source geometry about local XZ, then applying the wing-frame transform. The engine must weld centerline topology rather than leave two overlapping root skins.

The global origin is a named datum chosen by the author. It may be a nose point, root leading edge, tip, or another meaningful location. A wing's `frame.origin` locates its local root frame in those global coordinates. This separation lets a tip be global `(0, 0, 0)` while the wing root remains on local XZ.

## Unit policy

Units are document-wide and fixed at aircraft creation. The file records one length unit, one angle unit, and any later analysis-unit categories. Each number is interpreted through the semantic type of its containing field:

- `position.x`, `chord`, and absolute trailing-edge thickness use the global length unit.
- `twist` uses the global angle unit.
- Ratios, fractions, station blend locations, and interpolation weights are dimensionless.

There are no per-parameter or per-field unit strings. The application must not offer an in-place unit-system switch. A future conversion utility, if needed, creates a separate definition with a new identity and regenerated numeric values; it never silently alters an existing aircraft source.

## Source of truth boundaries

| Asset | Contains | Must not contain |
| --- | --- | --- |
| Aircraft JSON | Geometry intent, parameters, profiles, component connections, analysis-case intent | Slider ranges, tessellation cache, calculated metrics, UI panel state |
| Component catalog | Field dimensions, widgets, default/adaptive range policies, validation rules | Aircraft-specific values |
| Presentation sidecar | User panel layout and optional custom control-range overrides | Geometry truth or simulation results |
| Derived artifact cache | Meshes, metrics, render data, analysis results keyed by input hash | Editable source parameters |

## Phase boundaries

### Version 1

Wing geometry, parameter graph, visualizer, JSON import/export, mesh export, and basic planform reports.

### Phase 2

Fuselage and attachment interfaces, control surfaces, more advanced profiles, multiple lifting surfaces, and low-order aerodynamic analysis. The schema grows through new component kinds; existing wing JSON stays valid.

### Phase 3

Buried propulsion: centerbody/fuselage, top or side inlet, intake lip, diffuser, duct, engine envelope, exhaust, flow-path centerline, and clearance/exclusion volumes. These components use named interfaces and the same expression graph as the wing, allowing an inlet location to depend on a kink, root plane, or fuselage datum.

### Phase 4

Arbitrary imported-solid Boolean operations. Engineers can bring a validated external solid into the graph, place it from named aircraft interfaces, and use explicit union, subtract, intersect, offset, and blend nodes. The system preserves source-asset identity and conversion settings so a Boolean result remains reproducible after reopening the aircraft study. This is intentionally later than buried-propulsion geometry: it requires a robust asset-import, repair, implicit-conversion, and diagnostic pipeline.

## Success criteria

- Editing a valid source parameter updates the preview without manual rebuild.
- A dependent chain such as `root -> kink -> tip` updates correctly and reports its dependency path.
- Invalid dimensions, unknown references, cycles, and a root off the symmetry plane are rejected before mesh generation.
- The supplied example imports, validates, renders as a half wing, mirrors cleanly, and re-exports without semantic change.
- A user can change component-control behavior in the catalog or presentation sidecar without modifying aircraft geometry JSON.
