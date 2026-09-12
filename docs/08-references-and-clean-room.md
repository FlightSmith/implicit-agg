# References and Clean Room Boundary

## Purpose

This design pack uses public engineering concepts as inputs to an original architecture. The resulting application should be independently implemented and branded. Do not copy source code, notebook files, icons, UI layouts, documentation text, file-format internals, or proprietary algorithms from commercial products.

## Public references

- [nTop implicit modeling support](https://support.ntop.com/hc/en-us/sections/360010614074-Implicit-Modeling) describes the public product category that motivates field-oriented, robust geometry workflows.
- [nTop field-driven design learning material](https://learn.ntop.com/courses/220-intro-to-field-driven-design/lessons/different-types-of-fields-in-ntop/) describes implicit bodies as signed-distance fields and fields as drivers of geometry. This pack uses that as inspiration only for the phase-3 implicit-body extension.
- [NASA OpenVSP Wings](https://www.nasa.gov/reference/openvsp-wings/) documents public wing controls including span, area, chord, sweep, twist, dihedral, and multi-section wings. It informs the v1 parameter vocabulary, not this product's implementation.
- [NASA wing geometry primer](https://www1.grc.nasa.gov/beginners-guide-to-aeronautics/wing-geometry/) defines public wing terms such as span, chord, projected area, aspect ratio, planform, and dihedral.
- [CPACS documentation](https://www.cpacs.de/documentation/CPACS_2_3_0_Docs/html/89b6a288-0944-bd56-a1ef-8d3c8e48ad95.htm) describes an XML-based aircraft data structure. The aircraft JSON here is an intentionally simpler original contract, not a CPACS serialization.

## Independent design choices

The following choices are specific to this project:

- One global locked unit system instead of unit-bearing leaf values.
- Negative-local-Y source half-model, local-XZ reflection, wing-local frame, and welded root centerline.
- Station-first canonical wing data rather than multiple conflicting planform drivers.
- Raw parameter scalars whose dimensions are inferred from schema-defined consumer fields.
- A restricted dimensional expression language using `@param` and `@station` references.
- Separation of aircraft source, UI catalog, presentation settings, and derived artifacts.

These decisions can be implemented, tested, and evolved without depending on another product's file formats or behavior.
