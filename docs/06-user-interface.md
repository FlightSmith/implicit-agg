# User Interface and Interaction Model

## User experience goal

The UI should make aircraft design intent legible. An engineer must be able to see what drives a shape, change it, understand what recomputed, and recover safely from a bad edit. Use a notebook-like workspace with a geometry viewport rather than a generic property-grid application.

## Workspace layout

```text
+-------------------+------------------------------+----------------------+
| Design tree       | 3D viewport                  | Inspector            |
|                   |                              |                      |
| Aircraft          | half / full / section views  | selected component   |
|  Main wing        | station labels               | grouped parameters   |
|   Root            | symmetry-plane overlay       | expression editor    |
|   Kink            | source trace on selection    | derived values       |
|   Tip             |                              | diagnostics          |
+-------------------+------------------------------+----------------------+
| Formula and dependency trace                                                     |
+-------------------------------------------------------------------------------+
```

## Design tree

The tree shows source components and their named stations, not generated mesh fragments. Selecting a station highlights its local airfoil plane, twist axis, and dependencies. Selecting a generated panel traces back to the two source stations that lofted it.

## Inspector

Group editable controls by engineering intent:

- Geometry: station position, chord, twist axis, twist.
- Profile: airfoil, camber/thickness controls, trailing edge.
- Relationships: parameter binding, literal value, or expression.
- Symmetry and tip treatment.
- Derived: read-only planform and mesh metrics.

Show an input's raw number using global project units. Its label supplies the dimension; the number itself carries no repeated unit string in the source JSON. The visible UI may append the global unit for clarity.

## Adaptive control ranges

Controls get their initial behavior from the component catalog. Example policies:

| Field type | Initial behavior | Edge behavior |
| --- | --- | --- |
| Positive length | Range centered around meaningful magnitude and never crosses zero | Expand outward smoothly |
| Signed offset | Range centered on current value | Expand in dragged direction |
| Angle | Local range around current value with safe clamp | Expand until catalog safety policy |
| Ratio | Normalized range and fine step | Expand only if field permits out-of-range ratio |
| Integer samples | Discrete, bounded catalog values | No continuous drag |

An advanced control menu lets the user override range, step, and preferred widget. Store that in `presentation.controlOverrides` or local settings, keyed by stable source path. The UI must label it as a presentation adjustment and offer “reset to catalog behavior.”

For a portable project-specific preference, use a separate sidecar keyed by component and station IDs, never list indexes:

```json
{
  "schema": "aircraft-presentation/v0.1",
  "aircraftId": "cranked-wing-study-001",
  "controlOverrides": {
    "main-wing/root/twist": {
      "range": { "min": -8, "max": 8, "step": 0.25 }
    }
  }
}
```

This is the only location in the design pack where a user-selected control range is serialized. Deleting it restores catalog behavior without altering geometry.

## Formula mode

Each numeric input offers three modes:

1. **Literal**: edit a number directly.
2. **Parameter**: select/create a named raw scalar.
3. **Expression**: edit a formula with autocomplete for parameters, stations, components, and interfaces.

Formula mode displays the evaluated result, inferred dimension, direct dependency list, and any diagnostic. It never evaluates arbitrary script code or accesses the filesystem/network.

## Live-update behavior

- Update lightweight labels and formula previews immediately.
- Debounce interactive mesh generation while dragging.
- Show the last valid preview with a visible pending state during recomputation.
- Never replace a valid mesh with a half-built or failed one.
- Keep editing focus and selection stable after a recompute.
- Provide undo/redo for all source edits and a change history of committed parameter transactions.

## Import and export

Import starts with schema and semantic validation. Present a concise report: aircraft name, unit system, components, warnings, and blocking errors. Do not partially import malformed geometry into the active document.

Export offers:

- Aircraft JSON: canonical, pretty-printed, source-only definition.
- Half or full mesh: STL, OBJ, and GLB initially.
- Report JSON/CSV: selected read-only metrics and validation results.

Future adapters may import/export CPACS, OpenVSP, or analysis-specific formats, but adapters are one-way translations around the canonical aircraft JSON—not new internal sources of truth.
