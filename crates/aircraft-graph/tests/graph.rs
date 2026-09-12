#![allow(clippy::unwrap_used)]

use aircraft_graph::{build, check_predicates, evaluate_graph, evaluate_incremental, FieldKind};
use aircraft_model::aircraft::{Component, Wing};
use aircraft_model::{parse_document, Code, Severity, TypedValue};

const EXAMPLE: &str = include_str!("../../../examples/cranked-wing.v0.1.json");

fn example() -> aircraft_model::AircraftDefinition {
    parse_document(EXAMPLE).unwrap().0
}

fn wing_mut(doc: &mut aircraft_model::AircraftDefinition) -> &mut Wing {
    match &mut doc.components[0] {
        Component::Wing(wing) => wing,
    }
}

#[test]
fn example_builds_and_evaluates_the_documented_cascade() {
    let doc = example();
    let (graph, warnings) = build(&doc).unwrap();
    assert!(warnings.is_empty(), "every example parameter is referenced");

    let evaluation = evaluate_graph(&graph, &doc);
    assert!(
        evaluation.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        evaluation.diagnostics
    );

    // Kink = root + (0.8, -3.2, 0.1); tip = kink + (1.4, -4.1, 0.35).
    let kink_x = graph
        .station_field_node(0, 1, FieldKind::PositionX)
        .unwrap();
    let kink_y = graph
        .station_field_node(0, 1, FieldKind::PositionY)
        .unwrap();
    let tip_x = graph
        .station_field_node(0, 2, FieldKind::PositionX)
        .unwrap();
    let tip_y = graph
        .station_field_node(0, 2, FieldKind::PositionY)
        .unwrap();
    let tip_z = graph
        .station_field_node(0, 2, FieldKind::PositionZ)
        .unwrap();
    assert!((evaluation.values[kink_x] - 0.8).abs() < 1e-12);
    assert!((evaluation.values[kink_y] + 3.2).abs() < 1e-12);
    assert!((evaluation.values[tip_x] - 2.2).abs() < 1e-12);
    assert!((evaluation.values[tip_y] + 7.3).abs() < 1e-12);
    assert!((evaluation.values[tip_z] - 0.45).abs() < 1e-12);

    // Root twist of 1.5 degrees in a degree document is canonical radians.
    let root_twist = graph.station_field_node(0, 0, FieldKind::Twist).unwrap();
    let expected = 1.5_f64.to_radians();
    assert!((evaluation.values[root_twist] - expected).abs() < 1e-12);

    // Chords convert from document meters to canonical meters.
    let root_chord = graph.station_field_node(0, 0, FieldKind::Chord).unwrap();
    assert!((evaluation.values[root_chord] - 2.1).abs() < 1e-12);

    check_predicates(&graph, &doc, &evaluation);
}

#[test]
fn editing_a_parameter_recomputes_exactly_its_descendants() {
    let doc = example();
    let (graph, _) = build(&doc).unwrap();
    let first_pass = evaluate_graph(&graph, &doc);

    // Move the root-to-kink offset: only kink and tip move.
    let mut edited = example();
    edited
        .parameters
        .insert("wing.rootToKink.dx".to_string(), 1.3);
    let seed = graph.parameter_node("wing.rootToKink.dx").unwrap();

    let incremental = evaluate_incremental(&graph, &edited, &first_pass, &[seed]);
    let full = evaluate_graph(&graph, &edited);

    assert_eq!(incremental.values, full.values);
    let kink_x = graph
        .station_field_node(0, 1, FieldKind::PositionX)
        .unwrap();
    let kink_y = graph
        .station_field_node(0, 1, FieldKind::PositionY)
        .unwrap();
    assert!((incremental.values[kink_x] - 1.3).abs() < 1e-12);
    assert!((incremental.values[kink_y] + 3.2).abs() < 1e-12);

    // The affected set is exactly the parameter, kink x, and tip x: the y and
    // z cascades hang off other parameters.
    let affected = graph.descendants(&[seed]);
    let kink_x = graph
        .station_field_node(0, 1, FieldKind::PositionX)
        .unwrap();
    let tip_x = graph
        .station_field_node(0, 2, FieldKind::PositionX)
        .unwrap();
    let expected: std::collections::BTreeSet<_> = [seed, kink_x, tip_x].into_iter().collect();
    assert_eq!(affected, expected);
}

#[test]
fn expression_cycles_are_rejected_with_the_chain() {
    let mut doc = example();
    let wing = wing_mut(&mut doc);
    wing.stations[0].position.x = TypedValue::expression("@station.tip.position.x + 1");
    let errors = build(&doc).expect_err("cycle must be rejected");
    assert!(
        errors
            .iter()
            .any(|d| d.code == Code::CycleDetected && d.message.contains("root.position.x")),
        "expected a cycle chain naming the root, got: {errors:?}"
    );
}

#[test]
fn unknown_parameter_references_are_rejected() {
    let mut doc = example();
    let wing = wing_mut(&mut doc);
    wing.stations[2].chord = TypedValue::expression("@param.does.not.exist + 1");
    let errors = build(&doc).expect_err("unknown parameter must be rejected");
    assert!(errors.iter().any(|d| d.code == Code::UnknownReference));
}

#[test]
fn unknown_station_references_are_rejected() {
    let mut doc = example();
    let wing = wing_mut(&mut doc);
    wing.stations[2].chord = TypedValue::expression("@station.missing.chord");
    let errors = build(&doc).expect_err("unknown station must be rejected");
    assert!(errors.iter().any(|d| d.code == Code::UnknownReference));
}

#[test]
fn an_angle_parameter_cannot_feed_a_length_field() {
    let mut doc = example();
    let wing = wing_mut(&mut doc);
    wing.stations[0].chord = TypedValue::param("wing.rootTwist");
    let errors = build(&doc).expect_err("angle parameter as chord must be rejected");
    assert!(
        errors.iter().any(|d| d.code == Code::DimensionMismatch),
        "expected a dimension mismatch, got: {errors:?}"
    );
}

#[test]
fn interface_origins_resolve_to_station_values() {
    let mut doc = example();
    let wing = wing_mut(&mut doc);
    wing.stations[1].position.x =
        TypedValue::expression("@component.main-wing.interface.root-attachment.origin.x + 0.5");
    let (graph, _) = build(&doc).unwrap();
    let evaluation = evaluate_graph(&graph, &doc);
    let kink_x = graph
        .station_field_node(0, 1, FieldKind::PositionX)
        .unwrap();
    assert!((evaluation.values[kink_x] - 0.5).abs() < 1e-12);
}

#[test]
fn an_off_plane_root_fails_the_symmetry_predicate() {
    let mut doc = example();
    let wing = wing_mut(&mut doc);
    wing.stations[0].position.y = TypedValue::number(0.1);
    let (graph, _) = build(&doc).unwrap();
    let evaluation = evaluate_graph(&graph, &doc);
    let diagnostics = check_predicates(&graph, &doc, &evaluation);
    assert!(diagnostics
        .iter()
        .any(|d| d.code == Code::RootOffSymmetryPlane && d.is_error()));
}

#[test]
fn non_monotonic_station_ordering_is_rejected() {
    let mut doc = example();
    let wing = wing_mut(&mut doc);
    // Kink at -1, tip at -0.5: the tip is inboard of the kink.
    wing.stations[1].position.y = TypedValue::number(-1.0);
    wing.stations[2].position.y = TypedValue::number(-0.5);
    let (graph, _) = build(&doc).unwrap();
    let evaluation = evaluate_graph(&graph, &doc);
    let diagnostics = check_predicates(&graph, &doc, &evaluation);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == Code::StationOrderViolation
                && d.subject.as_deref() == Some("main-wing / station tip / position y")),
        "expected a station-order violation at the tip, got: {diagnostics:?}"
    );
}

#[test]
fn non_positive_chords_are_rejected() {
    let mut doc = example();
    doc.parameters.insert("wing.tipChord".to_string(), 0.0);
    let (graph, _) = build(&doc).unwrap();
    let evaluation = evaluate_graph(&graph, &doc);
    let diagnostics = check_predicates(&graph, &doc, &evaluation);
    assert!(diagnostics.iter().any(|d| d.code == Code::NonPositiveChord));
}

#[test]
fn division_by_zero_produces_a_non_finite_diagnostic() {
    let mut doc = example();
    let wing = wing_mut(&mut doc);
    wing.stations[1].chord = TypedValue::expression("@param.wing.kinkChord / 0");
    let (graph, _) = build(&doc).unwrap();
    let evaluation = evaluate_graph(&graph, &doc);
    assert!(
        evaluation
            .diagnostics
            .iter()
            .any(|d| d.code == Code::NonFiniteResult),
        "expected a non-finite diagnostic, got: {:?}",
        evaluation.diagnostics
    );
}

#[test]
fn unreferenced_parameters_are_warnings() {
    let mut doc = example();
    doc.parameters.insert("unused.things".to_string(), 5.0);
    let (_, warnings) = build(&doc).unwrap();
    assert!(warnings
        .iter()
        .any(|d| d.severity == Severity::Warning && d.message.contains("unused.things")));
}

#[test]
fn evaluation_is_deterministic() {
    let doc = example();
    let (graph, _) = build(&doc).unwrap();
    let first = evaluate_graph(&graph, &doc);
    let second = evaluate_graph(&graph, &doc);
    assert_eq!(first.values, second.values);
}

#[test]
fn trailing_edge_values_become_nodes() {
    let doc = example();
    let (graph, _) = build(&doc).unwrap();
    let root_te = graph
        .station_field_node(0, 0, FieldKind::TrailingEdgeThickness)
        .expect("root has an absolute trailing edge");
    let tip_te = graph
        .station_field_node(0, 2, FieldKind::TrailingEdgeFraction)
        .expect("tip has a chord-fraction trailing edge");
    let evaluation = evaluate_graph(&graph, &doc);
    assert!((evaluation.values[root_te] - 0.008).abs() < 1e-12);
    assert!((evaluation.values[tip_te] - 0.006).abs() < 1e-12);

    // The sharp kink trailing edge produces no node.
    assert!(graph
        .station_field_node(0, 1, FieldKind::TrailingEdgeThickness)
        .is_none());
}

#[test]
fn station_subject_and_path_are_traceable() {
    let doc = example();
    let (graph, _) = build(&doc).unwrap();
    let kink_x = graph
        .station_field_node(0, 1, FieldKind::PositionX)
        .unwrap();
    let node = &graph.nodes[kink_x];
    assert_eq!(node.path, "components/0/stations/1/position/x");
    assert_eq!(node.subject, "main-wing / station kink / position x");
}
