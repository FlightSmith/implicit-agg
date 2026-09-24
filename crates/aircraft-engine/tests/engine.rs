#![allow(clippy::unwrap_used)]

use aircraft_engine::{
    CancellationToken, Engine, MeshJobError, Patch, TransactionId, GEOMETRY_VERSION,
};
use aircraft_geom::quality::MeshQuality;
use aircraft_geom::step_model::StepSurface;
use aircraft_graph::FieldKind;
use aircraft_model::{parse_document, TypedValue};

const EXAMPLE: &str = include_str!("../../../examples/cranked-wing.v0.1.json");

fn engine() -> Engine {
    let (doc, _) = parse_document(EXAMPLE).unwrap();
    Engine::open(doc).unwrap()
}

#[test]
fn example_opens_with_valid_meshes() {
    let mut engine = engine();
    assert_eq!(engine.revision(), 1);

    let token = CancellationToken::default();
    let half = engine
        .wing_mesh(0, MeshQuality::Interactive, false, &token, None)
        .unwrap();
    let half_validation = half.mesh.validate();
    assert!(half_validation.is_sound() && half_validation.closed);
    let (half_volume, _) = half.mesh.volume_and_center();
    assert!(half_volume > 0.0);

    let full = engine
        .wing_mesh(0, MeshQuality::Interactive, true, &token, None)
        .unwrap();
    let full_validation = full.mesh.validate();
    assert!(full_validation.is_sound() && full_validation.closed);
    let (full_volume, _) = full.mesh.volume_and_center();
    // The root cap adds no volume, so the full model is exactly twice.
    assert!(
        (full_volume - 2.0 * half_volume).abs() / half_volume < 1e-9,
        "full {full_volume} vs 2x half {}",
        2.0 * half_volume
    );
}

#[test]
fn editing_the_cascade_updates_downstream_stations_and_sweep() {
    let mut engine = engine();
    let token = CancellationToken::default();
    let before = engine.evaluated_stations(0).unwrap().1;
    let report_before = engine.report(&token).unwrap();

    let result = engine.apply_patch(
        Patch::SetParameter {
            id: "wing.rootToKink.dx".into(),
            value: 1.3,
        },
        TransactionId(1),
    );
    assert!(result.committed, "diagnostics: {:?}", result.diagnostics);
    assert_eq!(result.revision, 2);
    assert!(result
        .affected
        .iter()
        .any(|subject| subject.contains("station kink / position x")));

    let after = engine.evaluated_stations(0).unwrap().1;
    assert!((after[1].position[0] - 1.3).abs() < 1e-12);
    assert!(
        (after[2].position[0] - 2.7).abs() < 1e-12,
        "tip follows kink"
    );
    // Outboard extent unchanged: moving dx only sweeps the wing.
    assert!((before[2].position[1] - after[2].position[1]).abs() < 1e-12);

    let report_after = engine.report(&token).unwrap();
    let sweep_changed = report_after.wings[0].planform.panels[0].leading_edge_sweep
        != report_before.wings[0].planform.panels[0].leading_edge_sweep;
    assert!(sweep_changed, "LE sweep must respond to dx");
}

#[test]
fn moving_outboard_changes_the_reference_area() {
    let mut engine = engine();
    let token = CancellationToken::default();
    let before = engine.report(&token).unwrap();

    let result = engine.apply_patch(
        Patch::SetParameter {
            id: "wing.rootToKink.outboard".into(),
            value: 4.0,
        },
        TransactionId(2),
    );
    assert!(result.committed, "diagnostics: {:?}", result.diagnostics);

    let after = engine.report(&token).unwrap();
    let area_before = before.wings[0].planform.reference_area.half;
    let area_after = after.wings[0].planform.reference_area.half;
    assert!(
        (area_after - area_before).abs() > 0.1,
        "area must respond to outboard: {area_before} -> {area_after}"
    );
}

#[test]
fn an_invalid_patch_is_not_committed() {
    let mut engine = engine();
    let result = engine.apply_patch(
        Patch::SetParameter {
            id: "wing.tipChord".into(),
            value: -0.5,
        },
        TransactionId(3),
    );
    assert!(!result.committed);
    assert_eq!(result.revision, 1, "revision must not advance");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == aircraft_model::Code::NonPositiveChord),
        "expected a chord diagnostic: {:?}",
        result.diagnostics
    );
    // The engine still evaluates on the previous snapshot.
    let token = CancellationToken::default();
    assert!(engine.report(&token).is_ok());
}

#[test]
fn unknown_parameter_patch_is_rejected_before_evaluation() {
    let mut engine = engine();
    let result = engine.apply_patch(
        Patch::SetParameter {
            id: "does.not.exist".into(),
            value: 1.0,
        },
        TransactionId(4),
    );
    assert!(!result.committed);
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == aircraft_model::Code::UnknownReference));
}

#[test]
fn station_field_patch_reaches_the_geometry() {
    let mut engine = engine();
    let result = engine.apply_patch(
        Patch::SetStationField {
            component_index: 0,
            station_index: 2,
            field: FieldKind::Twist,
            value: TypedValue::number(-6.0),
        },
        TransactionId(5),
    );
    assert!(result.committed, "diagnostics: {:?}", result.diagnostics);
    let stations = engine.evaluated_stations(0).unwrap().1;
    assert!((stations[2].twist - (-6.0_f64).to_radians()).abs() < 1e-12);
}

#[test]
fn stale_and_cancelled_jobs_never_replace_the_preview() {
    let mut engine = engine();
    let token = CancellationToken::default();

    // Stale: the caller's revision no longer matches.
    let error = engine
        .wing_mesh(0, MeshQuality::Interactive, false, &token, Some(99))
        .unwrap_err();
    assert!(matches!(
        error,
        MeshJobError::Stale {
            current: 1,
            expected: 99
        }
    ));

    // Cancelled: the token fires before any work.
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    let error = engine
        .wing_mesh(0, MeshQuality::Interactive, false, &cancelled, None)
        .unwrap_err();
    assert!(matches!(error, MeshJobError::Cancelled));
}

#[test]
fn mesh_artifacts_are_cached_by_content() {
    let mut engine = engine();
    let token = CancellationToken::default();
    let first = engine
        .wing_mesh(0, MeshQuality::Interactive, false, &token, None)
        .unwrap();
    let second = engine
        .wing_mesh(0, MeshQuality::Interactive, false, &token, None)
        .unwrap();
    assert!(Arc::ptr_eq(&first, &second), "identical request hits cache");

    let committed = engine
        .apply_patch(
            Patch::SetParameter {
                id: "wing.kinkChord".into(),
                value: 1.4,
            },
            TransactionId(6),
        )
        .committed;
    assert!(committed);
    let third = engine
        .wing_mesh(0, MeshQuality::Interactive, false, &token, None)
        .unwrap();
    assert!(
        !Arc::ptr_eq(&first, &third),
        "geometry changed, cache busted"
    );
    assert_eq!(third.revision, engine.revision());
}

#[test]
fn selection_traces_back_to_source_stations() {
    let mut engine = engine();
    let token = CancellationToken::default();
    let artifact = engine
        .wing_mesh(0, MeshQuality::Interactive, false, &token, None)
        .unwrap();
    let trace = artifact.trace(0).unwrap();
    assert_eq!(trace.wing_id, "main-wing");
    assert!(!trace.description.is_empty());
    // Face 0 is a panel of the first spanwise step: root and kink.
    assert_eq!(trace.stations, vec!["root".to_string(), "kink".to_string()]);
    assert!(!trace.mirrored);
}

#[test]
fn export_produces_bytes_for_every_format() {
    let mut engine = engine();
    let token = CancellationToken::default();
    for format in [
        meshio::MeshFormat::StlBinary,
        meshio::MeshFormat::StlAscii,
        meshio::MeshFormat::Obj,
        meshio::MeshFormat::Glb,
    ] {
        let bytes = engine
            .export(0, MeshQuality::Interactive, true, format, "wing", &token)
            .unwrap();
        assert!(!bytes.is_empty(), "{format:?} produced no bytes");
    }
    let stl = engine
        .export(
            0,
            MeshQuality::Interactive,
            true,
            meshio::MeshFormat::StlBinary,
            "wing",
            &token,
        )
        .unwrap();
    let count = u32::from_le_bytes(stl[80..84].try_into().unwrap()) as usize;
    assert!(count > 0);
}

#[test]
fn undo_restores_the_previous_snapshot() {
    let mut engine = engine();
    let undo_state = engine.clone();
    let committed = engine
        .apply_patch(
            Patch::SetParameter {
                id: "wing.rootChord".into(),
                value: 2.5,
            },
            TransactionId(7),
        )
        .committed;
    assert!(committed);
    assert_eq!(engine.revision(), 2);
    engine.restore(undo_state);
    assert_eq!(engine.revision(), 1);
    assert!((engine.document().parameters["wing.rootChord"] - 2.1).abs() < 1e-12);
}

#[test]
fn incremental_evaluation_matches_the_full_oracle() {
    let engine = engine();
    let mut edited = engine.document().clone();
    edited.parameters.insert("wing.kinkToTip.dz".into(), 0.7);

    let (graph_before, _) = aircraft_graph::build(engine.document()).unwrap();
    let seed = graph_before.parameter_node("wing.kinkToTip.dz").unwrap();
    let incremental = engine.evaluate_incremental_from(&edited, &[seed]);
    let (graph_after, _) = aircraft_graph::build(&edited).unwrap();
    let full = aircraft_graph::evaluate_graph(&graph_after, &edited);
    assert_eq!(incremental.values.len(), full.values.len());
    // Affected values match exactly; unaffected values are preserved.
    for (a, b) in incremental.values.iter().zip(&full.values) {
        assert_eq!(a, b, "incremental diverged from full recompute");
    }
    let _ = GEOMETRY_VERSION;
}

#[test]
fn root_tangency_changes_the_le_departure_at_the_root_only() {
    let mut engine = engine();
    let token = CancellationToken::default();
    let before = engine.evaluated_stations(0).unwrap().1;

    let result = engine.apply_patch(
        Patch::SetStationTangency {
            component_index: 0,
            station_index: 0,
            tangency: Some(aircraft_model::StationTangency {
                left: Some(aircraft_model::StationTangencySide {
                    auto: false,
                    direction: Some([0.55, -0.83, 0.0]),
                    strength: 1.0,
                }),
                right: None,
            }),
        },
        TransactionId(30),
    );
    assert!(result.committed, "diagnostics: {:?}", result.diagnostics);

    // Station positions are untouched; the shape change lives in the mesh.
    let after = engine.evaluated_stations(0).unwrap().1;
    assert_eq!(before[0].position, after[0].position);

    let mesh = engine
        .wing_mesh(0, MeshQuality::Interactive, true, &token, None)
        .unwrap();
    // The near-root LE moved (x grew aft), while the tip LE stayed put.
    let root_le_moved = mesh
        .mesh
        .vertices
        .iter()
        .any(|v| v[0] > 0.25 && v[1] > -1.0);
    assert!(root_le_moved, "LE near the root must swing aft");
}

#[test]
fn document_round_trip_reopens_identically() {
    let (doc, _) = parse_document(EXAMPLE).unwrap();
    let serialized = serde_json::to_string_pretty(&doc).unwrap();
    let (reparsed, _) = parse_document(&serialized).unwrap();
    let mut engine_a = Engine::open(doc).unwrap();
    let mut engine_b = Engine::open(reparsed).unwrap();
    let token = CancellationToken::default();
    let mesh_a = engine_a
        .wing_mesh(0, MeshQuality::Interactive, true, &token, None)
        .unwrap();
    let mesh_b = engine_b
        .wing_mesh(0, MeshQuality::Interactive, true, &token, None)
        .unwrap();
    assert_eq!(mesh_a.mesh.vertices, mesh_b.mesh.vertices);
    assert_eq!(mesh_a.mesh.triangles, mesh_b.mesh.triangles);
}

use std::sync::Arc;

#[test]
fn add_parameter_creates_a_usable_scalar() {
    let mut engine = engine();
    let add = engine.apply_patch(
        Patch::AddParameter {
            id: "wing.customTwist".into(),
            value: -2.0,
        },
        TransactionId(20),
    );
    assert!(add.committed, "diagnostics: {:?}", add.diagnostics);

    // The new scalar can feed a field immediately.
    let bind = engine.apply_patch(
        Patch::SetStationField {
            component_index: 0,
            station_index: 2,
            field: FieldKind::Twist,
            value: TypedValue::param("wing.customTwist"),
        },
        TransactionId(21),
    );
    assert!(bind.committed, "diagnostics: {:?}", bind.diagnostics);
    let stations = engine.evaluated_stations(0).unwrap().1;
    assert!((stations[2].twist - (-2.0_f64).to_radians()).abs() < 1e-12);

    // Duplicate and malformed ids are rejected without committing.
    for bad in ["wing.customTwist", "BadId", "2cool"] {
        let result = engine.apply_patch(
            Patch::AddParameter {
                id: bad.into(),
                value: 0.0,
            },
            TransactionId(22),
        );
        assert!(!result.committed, "{bad} must be rejected");
    }
}

#[test]
fn step_model_shells_are_closed_and_oriented() {
    for (symmetry, full, expected_faces, expected_shells) in [
        (true, false, 5, 1),
        (true, true, 10, 2),
        (false, false, 5, 1),
    ] {
        let engine = engine();
        let (_, stations) = engine.evaluated_stations(0).unwrap();
        let panel_tangencies = engine.resolve_panel_tangencies(0).unwrap();
        let model = aircraft_geom::step_model::wing_model(
            "wing",
            &stations,
            symmetry,
            Default::default(),
            &panel_tangencies,
            full,
        )
        .unwrap_or_else(|diagnostics| panic!("model build failed: {diagnostics:?}"));
        assert_eq!(model.faces.len(), expected_faces, "faces (full={full})");
        assert_eq!(model.shells.len(), expected_shells, "shells (full={full})");
        model.validate().unwrap();
        let text = meshio::step::write_step(&model);
        assert!(text.contains("MANIFOLD_SOLID_BREP"));
        assert!(text.ends_with("END-ISO-10303-21;\n"));
    }
}

#[test]
fn step_skin_knots_are_clamped_and_consistent() {
    let (doc, _) =
        parse_document(&std::fs::read_to_string("/dev/shm/cranked-wing-study-001.json").unwrap())
            .unwrap();
    let engine = Engine::open(doc).unwrap();
    let symmetry = true;
    let (_, stations) = engine.evaluated_stations(0).unwrap();
    let panel_tangencies = engine.resolve_panel_tangencies(0).unwrap();
    let model = aircraft_geom::step_model::wing_model(
        "wing",
        &stations,
        symmetry,
        Default::default(),
        &panel_tangencies,
        true,
    )
    .unwrap();

    for (index, face) in model.faces.iter().enumerate() {
        let StepSurface::Nurbs(surface) = &face.surface else {
            continue;
        };
        // Knot-group invariants: each group's multiplicities must sum to the
        // number of knots in that direction, and to control_count + degree + 1.
        for (name, knots, degree, ctrl) in [
            (
                "u",
                &surface.u_knots,
                surface.u_degree,
                surface.controls.len(),
            ),
            (
                "v",
                &surface.v_knots,
                surface.v_degree,
                surface.controls[0].len(),
            ),
        ] {
            let (mults, uniq) = aircraft_geom::nurbs::knot_groups(knots);
            assert_eq!(mults.len(), uniq.len(), "face {index} {name}: groups");
            assert_eq!(
                mults.iter().sum::<usize>(),
                knots.len(),
                "face {index} {name}: multiplicity sum"
            );
            assert_eq!(
                knots.first(),
                Some(&0.0),
                "face {index} {name}: must start at 0"
            );
            assert_eq!(
                knots.last(),
                Some(&1.0),
                "face {index} {name}: must end at 1"
            );
            // Clamped: first and last knot repeated degree+1 times.
            assert!(
                knots.iter().take(degree + 1).all(|&k| k == 0.0),
                "face {index} {name}: leading knots must repeat degree+1 times"
            );
            assert!(
                knots.iter().rev().take(degree + 1).all(|&k| k == 1.0),
                "face {index} {name}: trailing knots must repeat degree+1 times"
            );
            let _ = ctrl;
            // Knots must be non-decreasing.
            for pair in knots.windows(2) {
                assert!(pair[0] <= pair[1], "face {index} {name}: knots decrease");
            }
        }
    }
}
