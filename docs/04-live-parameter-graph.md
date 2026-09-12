# Live Parameter Graph

## Goal

Live editing must be predictable. The system treats an aircraft source as a typed directed acyclic graph rather than a collection of mutable text fields. A parameter is a graph input; a station coordinate is a dependent value; a wing surface, mesh, and report are later graph products.

## Expression language

Use a deliberately small language. An expression starts with `=` and supports literals, parentheses, arithmetic, comparison, and the following pure functions:

```text
min, max, clamp, abs, sqrt, pow, sin, cos, tan, lerp
```

References have stable prefixes:

```text
@param.wing.rootToKink.dx
@station.root.position.x
@component.main-wing.interface.root-attachment.origin.z
```

The parser resolves the longest valid identifier after the prefix. Parameter IDs begin with a lowercase letter and may use letters, numbers, `.`, `_`, and `-`; whitespace or an operator ends a reference. Component and station IDs remain lower-kebab identifiers. IDs never rely on a list index.

## Dimensional rules

The evaluator carries one dimension per node, not a unit string per value.

| Operation | Rule |
| --- | --- |
| `length + length` | Valid; result is length. |
| `length - length` | Valid; result is length. |
| `angle + angle` | Valid; result is angle. |
| `length + angle` | Invalid. |
| `length * ratio` | Valid; result is length. |
| `length / length` | Valid; result is ratio. |
| `sin(angle)` | Valid; result is ratio. |
| `lerp(a, b, t)` | `a` and `b` must share a dimension; `t` is a ratio. |

The target field supplies an expected dimension. The evaluator rejects a valid-looking expression that produces the wrong dimension. It also rejects non-finite values and geometry-critical zero/negative values such as non-positive chord.

## Dependency evaluation

For this chain:

```text
root position
  -> kink position
       -> tip position
            -> wing surface
                 -> preview mesh and planform metrics
```

an edit to `wing.rootToKink.dx` invalidates only the kink, tip, wing surface, mesh, and metrics. It does not rebuild unrelated airfoils or UI-only state.

Evaluation steps:

1. Parse source and catalog field definitions into typed graph nodes.
2. Resolve references and infer parameter dimensions.
3. Run a cycle check with a diagnostic path such as `root -> kink -> tip -> root`.
4. Topologically evaluate affected nodes in deterministic ID order.
5. Validate physical predicates after each component's inputs resolve.
6. Publish one snapshot revision or retain the previous snapshot with diagnostics.

## Editing behavior

While a user drags a control, the UI may send a stream of transient patches. The core coalesces them by field and uses a cancellation token for preview jobs. A pointer release creates one undoable committed patch. Keyboard edits commit when the value parses and focus leaves the field or the user confirms it.

Diagnostics must identify the editable source:

```text
main-wing / station tip / position x
Expression result is 1.12 deg but this field requires a length.
```

The UI should offer a trace view for every generated value: its literal, parameter references, expression, direct dependencies, and downstream consequences.

## Catalog-driven controls

The aircraft JSON contains no slider limits. The wing catalog declares the dimensional type, editor kind, and an adaptive range policy. A `positiveLength` control might initialize around the current value, protect zero, and expand automatically when the handle reaches an edge. An `angle` control may initialize around the current angle and expand within a policy-defined hard safety interval.

The user can customize a range in an optional presentation sidecar. This change is visual only: it does not affect validation, geometry, JSON export, or another user's ability to import the source.

## Required diagnostics

- JSON schema violation.
- Unknown parameter, station, component, or interface reference.
- Cycle detected with full dependency chain.
- Unit/dimension mismatch.
- Non-finite numerical result.
- Root station not on local `y = 0` for a symmetric wing.
- Non-monotonic negative-local-Y station ordering.
- Non-positive chord or invalid trailing-edge thickness.
- Profile invalid, self-intersecting, or impossible to loft.
- Mesh failure with the geometry node and failing tolerance.
