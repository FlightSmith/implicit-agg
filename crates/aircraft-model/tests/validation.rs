use aircraft_model::parse_document;

const EXAMPLE: &str = include_str!("../../../examples/cranked-wing.v0.1.json");

#[test]
fn supplied_example_passes_schema_and_semantic_validation() {
    let (document, diagnostics) = parse_document(EXAMPLE).expect("example must validate");
    assert!(
        diagnostics.iter().all(|d| !d.is_error()),
        "unexpected error diagnostics: {diagnostics:?}"
    );
    assert_eq!(document.id, "cranked-wing-study-001");
    assert_eq!(document.units.length, aircraft_model::LengthUnit::Meters);
    assert_eq!(document.parameters["wing.rootChord"], 2.1);
}

#[test]
fn example_round_trips_without_semantic_change() {
    let (document, _) = parse_document(EXAMPLE).unwrap();
    let serialized = serde_json::to_string_pretty(&document).unwrap();
    let (reparsed, _) = parse_document(&serialized).unwrap();
    assert_eq!(document, reparsed);
}

#[test]
fn inline_units_are_rejected() {
    let mut value: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    value["components"][0]["stations"][0]["chord"] = serde_json::json!({
        "value": 2.1,
        "unit": "m"
    });
    let source = serde_json::to_string(&value).unwrap();
    let errors = parse_document(&source).expect_err("inline unit must be rejected");
    assert!(
        errors.iter().any(|d| d.code == aircraft_model::Code::SchemaViolation),
        "expected a schema violation, got: {errors:?}"
    );
}

#[test]
fn missing_required_field_is_rejected_with_a_path() {
    let mut value: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    value.as_object_mut().unwrap().remove("units");
    let source = serde_json::to_string(&value).unwrap();
    let errors = parse_document(&source).expect_err("missing units must be rejected");
    assert!(errors
        .iter()
        .any(|d| d.code == aircraft_model::Code::SchemaViolation
            && d.path.as_deref() == Some("")));
}

#[test]
fn unknown_station_airfoil_reference_is_rejected() {
    let mut value: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    value["components"][0]["stations"][0]["airfoil"] = serde_json::json!("missing-airfoil");
    let source = serde_json::to_string(&value).unwrap();
    let errors = parse_document(&source).expect_err("unknown airfoil must be rejected");
    assert!(errors
        .iter()
        .any(|d| d.code == aircraft_model::Code::UnknownReference));
}

#[test]
fn unlocked_unit_system_is_rejected() {
    let mut value: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    value["unitSystemLocked"] = serde_json::json!(false);
    let source = serde_json::to_string(&value).unwrap();
    let errors = parse_document(&source).expect_err("schema const must reject false");
    assert!(errors
        .iter()
        .any(|d| d.code == aircraft_model::Code::SchemaViolation));
}
