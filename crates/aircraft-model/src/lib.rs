//! Typed model of the `aircraft-definition/v0.1` source document, with JSON
//! Schema validation and semantic (meaning-level) checks.

pub mod aircraft;
pub mod diagnostic;
pub mod schema;
pub mod semantic;
pub mod tangency;
pub mod typed_value;
pub mod units;

pub use aircraft::{
    AircraftDefinition, Airfoil, AnalysisCase, Axes, Axis, CenterlineTreatment, Component,
    CoordinateOrigin, CoordinateSystem, Frame, FrameAxes, Interface, OriginKind, Position,
    SourceSide, Station, Symmetry, SymmetryPlane, TrailingEdge, Wing, WingRole, SCHEMA_ID,
};
pub use diagnostic::{Code, Diagnostic, Severity};
pub use typed_value::TypedValue;
pub use units::{AngleUnit, ForceUnit, LengthUnit, MassUnit, PressureUnit, UnitSystem};

/// Parse a document from JSON source: schema validation first, then typed
/// deserialization, then semantic validation. Returns `Err` whenever any
/// error-severity diagnostic was produced, so a partially valid document is
/// never mistaken for a usable one; `Ok` may still carry warnings.
pub fn parse_document(
    source: &str,
) -> Result<(AircraftDefinition, Vec<Diagnostic>), Vec<Diagnostic>> {
    let value: serde_json::Value = serde_json::from_str(source).map_err(|error| {
        vec![Diagnostic::error(
            Code::SchemaViolation,
            format!("invalid JSON: {error}"),
        )]
    })?;

    let mut diagnostics = schema::validate_schema(&value);
    if diagnostics.iter().any(Diagnostic::is_error) {
        return Err(diagnostics);
    }

    let document: AircraftDefinition = serde_json::from_value(value).map_err(|error| {
        vec![Diagnostic::error(
            Code::SchemaViolation,
            format!("document does not match the typed model: {error}"),
        )]
    })?;

    diagnostics.extend(semantic::validate(&document));
    if diagnostics.iter().any(Diagnostic::is_error) {
        return Err(diagnostics);
    }
    Ok((document, diagnostics))
}
