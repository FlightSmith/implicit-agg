#![allow(clippy::unwrap_used)]

use aircraft_geom::mesh::vnormalize;
use aircraft_geom::metrics::{planform_metrics, volume_metrics};
use aircraft_geom::quality::{resolve_quality, ExportTolerances, MeshQuality};
use aircraft_geom::section::{StationSpec, TrailingEdgeSpec};
use aircraft_geom::wing::EvaluatedStation;
use aircraft_geom::{build_wing_mesh, ProfileCurve};
use aircraft_model::Diagnostic;
use std::sync::Arc;

fn naca(code: &str) -> Arc<ProfileCurve> {
    Arc::new(ProfileCurve::naca4(code).unwrap())
}

fn diamond() -> Arc<ProfileCurve> {
    Arc::new(
        ProfileCurve::coordinates(&[[0.0, 0.0], [0.5, 0.05], [1.0, 0.0], [0.5, -0.05]]).unwrap(),
    )
}

fn station(
    id: &str,
    curve: &Arc<ProfileCurve>,
    position: [f64; 3],
    chord: f64,
) -> EvaluatedStation {
    EvaluatedStation {
        id: id.to_string(),
        position,
        chord,
        twist: 0.0,
        trailing_edge: TrailingEdgeSpec::Sharp,
        curve: Arc::clone(curve),
    }
}

fn uniform_quality(stations: usize, k: usize, span: usize) -> aircraft_geom::ResolvedQuality {
    aircraft_geom::ResolvedQuality {
        chord_samples: k,
        span_subdivisions: vec![span; stations - 1],
    }
}

fn shoelace(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    let mut area = 0.0;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        area += a[0] * b[1] - b[0] * a[1];
    }
    area / 2.0
}

#[test]
fn naca0012_is_symmetric_with_finite_trailing_edge() {
    let curve = ProfileCurve::naca4("0012").unwrap();
    for &x in &[0.1, 0.3, 0.5, 0.8] {
        let upper = curve.sample_upper(x);
        let lower = curve.sample_lower(x);
        assert!(
            (upper + lower).abs() < 1e-12,
            "symmetric profile violated at x={x}: {upper} vs {lower}"
        );
    }
    // Maximum thickness is 12% near x = 0.3.
    let max_thickness: f64 = (0..=1000)
        .map(|i| {
            let x = i as f64 / 1000.0;
            curve.sample_upper(x) - curve.sample_lower(x)
        })
        .fold(0.0, f64::max);
    assert!((max_thickness - 0.12).abs() < 1e-3);
    // The 4-series trailing edge is finite; closure is the TE field's job.
    assert!(curve.te_gap() > 0.0 && curve.te_gap() < 0.01);
}

#[test]
fn naca2412_has_the_documented_camber() {
    let curve = ProfileCurve::naca4("2412").unwrap();
    // Max camber 2% at x = 0.4.
    let camber_at = |x: f64| (curve.sample_upper(x) + curve.sample_lower(x)) / 2.0;
    assert!(
        (camber_at(0.4) - 0.02).abs() < 1e-3,
        "camber {}",
        camber_at(0.4)
    );
    assert!(curve.sample_upper(0.4) > curve.sample_lower(0.4));
}

#[test]
fn invalid_naca_codes_are_rejected() {
    assert!(ProfileCurve::naca4("241").is_err());
    assert!(ProfileCurve::naca4("24a1").is_err());
    assert!(
        ProfileCurve::naca4("2004").is_err(),
        "camber without position"
    );
}

#[test]
fn coordinate_profiles_support_loop_repairs() {
    // Same diamond, supplied starting from the TE with a duplicated closing
    // point and a duplicated consecutive point: repair is lossless.
    let curve = ProfileCurve::coordinates(&[
        [1.0, 0.0],
        [0.5, -0.05],
        [0.5, -0.05],
        [0.0, 0.0],
        [0.5, 0.05],
        [1.0, 0.0],
    ])
    .unwrap();
    assert!((curve.sample_upper(0.5) - 0.05).abs() < 1e-12);
    assert!((curve.sample_lower(0.5) + 0.05).abs() < 1e-12);
    assert!((curve.x_le()).abs() < 1e-12);
    assert!((curve.x_te() - 1.0).abs() < 1e-12);
}

#[test]
fn out_of_range_coordinates_are_rejected() {
    assert!(ProfileCurve::coordinates(&[[0.0, 0.0], [1.5, 0.0], [0.9, 0.1]]).is_err());
    assert!(ProfileCurve::coordinates(&[[0.0, 0.0], [1.0, f64::NAN], [0.9, 0.1]]).is_err());
}

#[test]
fn bowtie_polygons_are_detected_as_self_intersecting() {
    use aircraft_geom::profile::polygon_self_intersects;
    assert!(polygon_self_intersects(&[
        [0.0, 0.0],
        [1.0, 1.0],
        [1.0, 0.0],
        [0.0, 1.0]
    ]));
    assert!(!polygon_self_intersects(&[
        [0.0, 0.0],
        [0.5, 0.05],
        [1.0, 0.0],
        [0.5, -0.05]
    ]));
}

#[test]
fn rings_place_the_leading_edge_and_follow_twist() {
    let curve = naca("0012");
    let spec = StationSpec {
        id: "tip".to_string(),
        curve: &curve,
        chord: 1.0,
        twist: 0.0,
        position: [5.0, -2.0, 0.5],
        trailing_edge: TrailingEdgeSpec::Sharp,
    };
    let ring = spec.build_ring(8);
    assert_eq!(ring.points.len(), 16, "2k vertices for k chord samples");
    let le = ring.points[0];
    assert!((le[0] - 5.0).abs() < 1e-12);
    assert!((le[1] + 2.0).abs() < 1e-12);
    assert!((le[2] - 0.5).abs() < 1e-12);
    // Chord extends aft by exactly the chord length.
    let max_x = ring
        .points
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    assert!((max_x - 6.0).abs() < 1e-9);

    // A 90-degree twist stands the section on its tail: the trailing edge
    // lands directly above the leading edge (right-hand rule about the
    // local spanwise direction, source half in negative Y).
    let twisted = StationSpec {
        id: "tip".to_string(),
        curve: &curve,
        chord: 1.0,
        twist: std::f64::consts::FRAC_PI_2,
        position: [0.0, 0.0, 0.0],
        trailing_edge: TrailingEdgeSpec::Sharp,
    };
    let ring = twisted.build_ring(8);
    let te_upper = ring.points[7];
    assert!((te_upper[0] - 0.0).abs() < 1e-9, "te x {}", te_upper[0]);
    assert!((te_upper[2] - 1.0).abs() < 1e-9, "te z {}", te_upper[2]);
}

#[test]
fn blunt_trailing_edge_feasibility_is_enforced() {
    let curve = naca("0012");
    let absurd = StationSpec {
        id: "root".to_string(),
        curve: &curve,
        chord: 1.0,
        twist: 0.0,
        position: [0.0; 3],
        trailing_edge: TrailingEdgeSpec::Absolute(0.6),
    };
    let error = absurd.check_feasible("wing / station root").unwrap_err();
    assert_eq!(error.code, aircraft_model::Code::InvalidTrailingEdge);
}

#[test]
fn rectangular_diamond_wing_volume_is_exact() {
    // A prismatic wing of a straight-edged profile: the mesh volume is
    // exactly ring area times span.
    let curve = diamond();
    let stations = vec![
        station("root", &curve, [0.0, 0.0, 0.0], 1.5),
        station("tip", &curve, [0.0, -2.0, 0.0], 1.5),
    ];
    let quality = uniform_quality(2, 8, 1);

    let half = build_wing_mesh("test-wing", &stations, true, &quality, true, false, None).unwrap();
    let full = build_wing_mesh("test-wing", &stations, true, &quality, false, true, None).unwrap();

    assert!(half.mesh.validate().closed);
    assert!(full.mesh.validate().closed);

    let (half_volume, _) = half.mesh.volume_and_center();
    let (full_volume, _) = full.mesh.volume_and_center();
    assert!(
        half_volume > 0.0 && full_volume > 0.0,
        "outward orientation"
    );

    // The mesh is a prism over the sampled ring polygon: its volume is
    // exactly the sampled ring's shoelace area times the span. (With k = 8
    // the cosine samples miss the profile kink, so the sampled area sits a
    // few percent under the true diamond area.)
    let ring = StationSpec {
        id: "root".into(),
        curve: &curve,
        chord: 1.5,
        twist: 0.0,
        position: [0.0; 3],
        trailing_edge: TrailingEdgeSpec::Sharp,
    }
    .build_ring(8);
    let sampled_area =
        shoelace(&ring.points.iter().map(|p| [p[0], p[2]]).collect::<Vec<_>>()).abs();
    let sampled_volume = sampled_area * 2.0;
    assert!(
        (sampled_area - 0.1125).abs() / 0.1125 < 0.1,
        "sampled ring area {sampled_area} should approximate the true 0.1125"
    );
    let expected_half = sampled_volume;
    assert!(
        (half_volume - expected_half).abs() < 1e-9,
        "half volume {half_volume} vs {expected_half}"
    );
    assert!(
        (full_volume - 2.0 * expected_half).abs() < 1e-9,
        "full volume {full_volume} vs {}",
        2.0 * expected_half
    );
}

#[test]
fn naca0012_rectangular_wing_volume_matches_the_analytic_integral() {
    let curve = naca("0012");
    let stations = vec![
        station("root", &curve, [0.0, 0.0, 0.0], 1.5),
        station("tip", &curve, [0.0, -2.0, 0.0], 1.5),
    ];
    let quality = uniform_quality(2, 64, 1);
    let half = build_wing_mesh("test-wing", &stations, true, &quality, true, false, None).unwrap();
    let validation = half.mesh.validate();
    assert!(validation.is_sound() && validation.closed);

    // Analytic profile area: 2 * integral of yt dx over the 4-series
    // thickness distribution with t = 0.12, times chord^2, times half span.
    let t = 0.12_f64;
    let integral =
        5.0 * t * (0.2969 * 2.0 / 3.0 - 0.126 / 2.0 - 0.3516 / 3.0 + 0.2843 / 4.0 - 0.1015 / 5.0);
    let profile_area = 2.0 * integral * 1.5_f64 * 1.5_f64;
    let expected = profile_area * 2.0;
    let (volume, center) = half.mesh.volume_and_center();
    assert!(
        (volume - expected).abs() / expected < 1e-3,
        "mesh volume {volume} vs analytic {expected}"
    );
    // The center of volume sits at the profile's area centroid (about 42%
    // chord for a 4-series section), mid-span, on the chord plane.
    let expected_x = 1.5
        * (0.2969 * 2.0 / 5.0 - 0.126 / 3.0 - 0.3516 / 4.0 + 0.2843 / 5.0 - 0.1015 / 6.0)
        / (0.2969 * 2.0 / 3.0 - 0.126 / 2.0 - 0.3516 / 3.0 + 0.2843 / 4.0 - 0.1015 / 5.0);
    assert!(
        (center[0] - expected_x).abs() < 5.0e-3,
        "center x {} vs expected {expected_x}",
        center[0]
    );
    // Prismatic wing: center of volume is exactly mid-span.
    assert!((center[1] + 1.0).abs() < 1e-9);
    assert!(center[2].abs() < 1e-6);
}

#[test]
fn full_model_has_no_duplicate_centerline_faces() {
    let curve = naca("0012");
    let stations = vec![
        station("root", &curve, [0.0, 0.0, 0.0], 1.5),
        station("tip", &curve, [0.0, -2.0, 0.0], 1.5),
    ];
    let quality = uniform_quality(2, 32, 1);
    let full = build_wing_mesh("test-wing", &stations, true, &quality, false, true, None).unwrap();
    let validation = full.mesh.validate();
    assert!(
        validation.closed,
        "full model must be closed: {validation:?}"
    );
    assert!(validation.manifold && validation.oriented);
    // The centerline must be welded: exactly one ring of vertices at y = 0.
    // The leading-edge and sharp trailing-edge vertex pairs merge, so the
    // ring contributes 2k - 2 unique vertices.
    let centerline = full
        .mesh
        .vertices
        .iter()
        .filter(|v| v[1].abs() < 1e-12)
        .count();
    let ring_vertices = 2 * quality.chord_samples - 2;
    assert_eq!(centerline, ring_vertices, "welded root ring only");
}

#[test]
fn half_model_boundary_is_the_root_ring_only() {
    let curve = naca("0012");
    let stations = vec![
        station("root", &curve, [0.0, 0.0, 0.0], 1.5),
        station("tip", &curve, [0.0, -2.0, 0.0], 1.5),
    ];
    let quality = uniform_quality(2, 16, 1);
    let open_half =
        build_wing_mesh("test-wing", &stations, true, &quality, false, false, None).unwrap();
    let validation = open_half.mesh.validate();
    assert!(!validation.closed, "open half has a root boundary");
    assert!(validation.manifold && validation.oriented);
    // Boundary edges form exactly one loop of the root ring size.
    let mut edge_count: std::collections::HashMap<(u32, u32), usize> =
        std::collections::HashMap::new();
    for &t in &open_half.mesh.triangles {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *edge_count.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let boundary = edge_count.values().filter(|&&c| c == 1).count();
    assert_eq!(boundary, 2 * quality.chord_samples - 2);
}

#[test]
fn face_sources_trace_to_stations() {
    let curve = naca("2412");
    let stations = vec![
        station("root", &curve, [0.0, 0.0, 0.0], 2.0),
        station("kink", &curve, [0.4, -2.0, 0.1], 1.2),
        station("tip", &curve, [1.0, -4.0, 0.3], 0.6),
    ];
    let quality = uniform_quality(3, 16, 2);
    let full = build_wing_mesh("test-wing", &stations, true, &quality, false, true, None).unwrap();
    let mut panels = 0;
    let mut mirrored = 0;
    let mut tip_caps = 0;
    for face in &full.faces {
        match *face {
            aircraft_geom::FaceSource::Panel {
                lower_station,
                upper_station,
                mirrored: is_mirrored,
            } => {
                panels += 1;
                assert!(upper_station == lower_station + 1);
                assert!(is_mirrored == (upper_station <= 2 && lower_station <= 2 && is_mirrored));
                if is_mirrored {
                    mirrored += 1;
                }
            }
            aircraft_geom::FaceSource::TipCap { station, mirrored } => {
                tip_caps += 1;
                assert_eq!(station, 2);
                assert!(mirrored || !mirrored);
            }
            aircraft_geom::FaceSource::RootCap => panic!("full model has no root cap"),
        }
    }
    assert!(panels > 0 && mirrored > 0 && tip_caps >= 2);
}

#[test]
fn planform_metrics_match_analytic_rectangles() {
    let curve = naca("0012");
    let stations = vec![
        station("root", &curve, [0.0, 0.0, 0.0], 1.5),
        station("tip", &curve, [0.0, -2.0, 0.0], 1.5),
    ];
    let metrics = planform_metrics(&stations).unwrap();
    assert!((metrics.reference_area.half - 3.0).abs() < 1e-12);
    assert!((metrics.reference_area.full - 6.0).abs() < 1e-12);
    assert!((metrics.span.full - 4.0).abs() < 1e-12);
    assert!((metrics.mac - 1.5).abs() < 1e-12);
    assert!((metrics.aspect_ratio - 16.0 / 6.0).abs() < 1e-12);
    assert!((metrics.taper_ratio.unwrap() - 1.0).abs() < 1e-12);
    for panel in &metrics.panels {
        assert!(panel.leading_edge_sweep.abs() < 1e-12);
        assert!(panel.quarter_chord_sweep.abs() < 1e-12);
        assert!(panel.trailing_edge_sweep.abs() < 1e-12);
        assert!(panel.dihedral.abs() < 1e-12);
    }
}

#[test]
fn planform_metrics_match_analytic_trapezoids() {
    let curve = naca("0012");
    // Root chord 2 at origin; tip chord 1 at (1, -4, 0.5).
    let stations = vec![
        station("root", &curve, [0.0, 0.0, 0.0], 2.0),
        station("tip", &curve, [1.0, -4.0, 0.5], 1.0),
    ];
    let metrics = planform_metrics(&stations).unwrap();
    assert!((metrics.reference_area.half - 6.0).abs() < 1e-12);
    assert!((metrics.taper_ratio.unwrap() - 0.5).abs() < 1e-12);

    // MAC = (2/3) cr (1 + l + l^2)/(1 + l) with l = 0.5, cr = 2.
    let taper = 0.5;
    let expected_mac = 2.0 / 3.0 * 2.0 * (1.0 + taper + taper * taper) / (1.0 + taper);
    assert!((metrics.mac - expected_mac).abs() < 1e-12);

    let panel = &metrics.panels[0];
    assert!((panel.leading_edge_sweep - 0.25_f64.atan()).abs() < 1e-12);
    // Quarter chord: 0.5 -> 1.25, delta 0.75 over span 4.
    assert!((panel.quarter_chord_sweep - (0.75_f64 / 4.0).atan()).abs() < 1e-12);
    // Trailing edge: 2.0 -> 2.0: parallel, zero sweep.
    assert!(panel.trailing_edge_sweep.abs() < 1e-12);
    assert!((panel.dihedral - (0.5_f64 / 4.0).atan()).abs() < 1e-12);
}

#[test]
fn volume_metrics_report_both_bases() {
    let curve = diamond();
    let stations = vec![
        station("root", &curve, [0.0, 0.0, 0.0], 1.5),
        station("tip", &curve, [0.0, -2.0, 0.0], 1.5),
    ];
    let quality = uniform_quality(2, 8, 1);
    let half = build_wing_mesh("test-wing", &stations, true, &quality, true, false, None).unwrap();
    let volume = volume_metrics(&half.mesh);
    assert!((volume.volume.full - 2.0 * volume.volume.half).abs() < 1e-12);
    assert!((volume.wetted_area.full - 2.0 * volume.wetted_area.half).abs() < 1e-12);
}

#[test]
fn export_quality_resolves_chord_samples_from_deviation() {
    let curve = naca("0012");
    let tight = resolve_quality(
        &[&curve],
        &[2.0],
        &[0.0],
        &MeshQuality::Export(ExportTolerances {
            max_chordal_deviation: Some(1.0e-4),
            max_edge_length: Some(0.5),
            max_normal_angle_deg: Some(10.0),
        }),
    );
    assert!(tight.chord_samples >= 64, "tight tolerance needs sampling");
    assert!(curve.chordal_deviation(tight.chord_samples) <= 1.0e-4);

    let loose = resolve_quality(
        &[&curve],
        &[2.0],
        &[0.0],
        &MeshQuality::Export(ExportTolerances {
            max_chordal_deviation: Some(1.0e-2),
            max_edge_length: Some(0.5),
            max_normal_angle_deg: Some(10.0),
        }),
    );
    assert!(loose.chord_samples < tight.chord_samples);

    // Edge length drives span subdivisions.
    assert!(tight.span_subdivisions[0] >= 4, "2 m panel / 0.5 m edges");
}

#[test]
fn interactive_quality_scales_span_stations_with_panel_share() {
    let curve = naca("0012");
    // A single panel spanning the whole model gets the full station budget.
    let single = resolve_quality(&[&curve], &[2.0], &[0.0], &MeshQuality::Interactive);
    assert_eq!(single.chord_samples, 48);
    assert_eq!(
        single.span_subdivisions[0],
        INTERACTIVE_STATIONS_PER_SPAN.round() as usize
    );
    // Two equal panels split the budget.
    let split = resolve_quality(
        &[&curve],
        &[2.0, 2.0],
        &[0.0, 0.0],
        &MeshQuality::Interactive,
    );
    assert_eq!(split.span_subdivisions[0], 10);
    assert_eq!(split.span_subdivisions[1], 10);
}

use aircraft_geom::quality::INTERACTIVE_STATIONS_PER_SPAN;

#[test]
fn degenerate_wings_are_rejected() {
    let curve = naca("0012");
    let single = vec![station("root", &curve, [0.0; 3], 1.0)];
    let error = build_wing_mesh(
        "w",
        &single,
        true,
        &uniform_quality(1, 8, 1),
        false,
        false,
        None,
    )
    .expect_err("single station must fail");
    assert!(error
        .iter()
        .any(|d: &Diagnostic| d.code == aircraft_model::Code::MeshFailure));

    // A wing whose stations coincide in span has a degenerate panel.
    let collapsed = vec![
        station("root", &curve, [0.0, 0.0, 0.0], 1.0),
        station("tip", &curve, [0.0, -0.0, 0.0], 1.0),
    ];
    let error = build_wing_mesh(
        "w",
        &collapsed,
        true,
        &uniform_quality(2, 8, 1),
        true,
        false,
        None,
    )
    .expect_err("collapsed wing must fail");
    assert!(!error.is_empty());
}

#[test]
fn trailing_edge_closure_matches_request_without_protruding() {
    // A NACA0008's natural TE gap is far smaller than the requested closure;
    // the flare must reach exactly the requested gap at the TE and grow
    // monotonically over the flare zone instead of notching the profile.
    let curve = naca("0008");
    let requested = 0.006; // chord fraction
    let spec = StationSpec {
        id: "tip".to_string(),
        curve: &curve,
        chord: 1.0,
        twist: 0.0,
        position: [0.0; 3],
        trailing_edge: TrailingEdgeSpec::ChordFraction(requested),
    };
    let ring = spec.build_ring(64);
    let te_upper = ring.points[63];
    let te_lower = ring.points[64];
    let gap = te_upper[2] - te_lower[2];
    assert!(
        (gap - requested).abs() < 1e-9,
        "closure gap {gap} must equal the requested {requested}"
    );

    // The closure gap tracks the natural envelope within the flare bound:
    // never notching below min(natural, requested) and never protruding
    // above max(natural, requested) by more than rounding.
    for j in 50..64 {
        let x = aircraft_geom::profile::cosine_samples(64)[j];
        let upper = ring.points[j];
        let lower = ring.points[127 - j]; // l_i sits at index 2k-1-i
        let gap = upper[2] - lower[2];
        let natural = curve.sample_upper(x) - curve.sample_lower(x);
        let bound = natural.max(requested);
        assert!(
            gap <= bound + 1e-9,
            "gap {gap} protrudes past the envelope {bound} at sample {j}"
        );
        assert!(
            gap >= natural.min(requested) - 1e-9,
            "gap {gap} notches below the envelope at sample {j}"
        );
    }

    // The closure never jumps off the surface: at the flare start the
    // flared ordinate equals the natural ordinate.
    let natural = curve.sample_upper(0.85);
    let flared = curve.sample_upper(0.85) + 0.0; // flare weight is zero at 0.85
    assert!((natural - flared).abs() < 1e-12);
}

#[test]
fn sharp_closure_collapses_to_the_mean_line() {
    let curve = naca("2412");
    let spec = StationSpec {
        id: "root".to_string(),
        curve: &curve,
        chord: 2.0,
        twist: 0.0,
        position: [0.0; 3],
        trailing_edge: TrailingEdgeSpec::Sharp,
    };
    let ring = spec.build_ring(16);
    let te_upper = ring.points[15];
    let te_lower = ring.points[16];
    assert!(
        (te_upper[2] - te_lower[2]).abs() < 1e-12,
        "sharp TE is a single point"
    );
    let mean = (curve.sample_upper(1.0) + curve.sample_lower(1.0)) / 2.0;
    assert!((te_upper[2] - mean * 2.0).abs() < 1e-9);
}

#[test]
fn le_tangency_bends_the_inboard_panel_and_keeps_stations_anchored() {
    let curve = naca("0012");
    let stations = vec![
        station("root", &curve, [0.0, 0.0, 0.0], 1.5),
        station("kink", &curve, [0.8, -3.2, 0.1], 1.15),
        station("tip", &curve, [2.2, -7.3, 0.45], 0.42),
    ];
    let quality = uniform_quality(3, 32, 4);
    // right:auto semantics: the outboard panel keeps its straight sweep, so
    // the kink-end tangent of the inboard panel is the outboard direction.
    let d2 = vnormalize(aircraft_geom::mesh::vsub(
        stations[2].position,
        stations[1].position,
    ));
    let tangency = aircraft_geom::wing::LeTangency {
        kink_end: Some(d2),
        kink_start: None, // right panel stays straight
        strength: 1.0,
    };

    let straight = build_wing_mesh("w", &stations, true, &quality, true, false, None).unwrap();
    let bent =
        build_wing_mesh("w", &stations, true, &quality, true, false, Some(tangency)).unwrap();
    assert!(bent.mesh.validate().is_sound() && bent.mesh.validate().closed);

    // Station LEs stay anchored in both meshes.
    for (label, mesh) in [("straight", &straight.mesh), ("bent", &bent.mesh)] {
        for expected in [[0.0, 0.0, 0.0], [0.8, -3.2, 0.1], [2.2, -7.3, 0.45]] {
            assert!(
                mesh.vertices
                    .iter()
                    .any(|v| (v[0] - expected[0]).abs() < 1e-9
                        && (v[1] - expected[1]).abs() < 1e-9
                        && (v[2] - expected[2]).abs() < 1e-9),
                "{label}: station LE {expected:?} must survive"
            );
        }
    }

    // Mid inboard panel: the bent LE is forward of the straight line (the
    // outboard panel sweeps harder, so tangency pulls the LE forward). The
    // tangent also shifts the ring's spanwise position, so search a band.
    let le_x = |mesh: &aircraft_geom::Mesh, y: f64, band: f64| -> f64 {
        mesh.vertices
            .iter()
            .filter(|v| (v[1] - y).abs() < band)
            .map(|v| v[0])
            .fold(f64::INFINITY, f64::min)
    };
    let straight_mid = le_x(&straight.mesh, -1.6, 1e-6);
    let bent_mid = le_x(&bent.mesh, -1.6, 0.25);
    assert!(
        bent_mid < straight_mid - 0.02,
        "tangent LE must bulge forward: straight {straight_mid} vs bent {bent_mid}"
    );
    // The outboard panel is untouched.
    assert!((le_x(&straight.mesh, -5.25, 1e-6) - le_x(&bent.mesh, -5.25, 0.2)).abs() < 1e-9);
}

#[test]
fn le_tangency_meets_in_the_middle_when_both_sides_are_set() {
    let curve = naca("0012");
    let stations = vec![
        station("root", &curve, [0.0, 0.0, 0.0], 1.5),
        station("kink", &curve, [0.8, -3.2, 0.1], 1.15),
        station("tip", &curve, [2.2, -7.3, 0.45], 0.42),
    ];
    let quality = uniform_quality(3, 32, 4);
    // Explicit vectors on both sides: each panel bends toward its vector.
    // Realistic LE tangents are dominated by the spanwise run.
    let tangency = aircraft_geom::wing::LeTangency {
        kink_end: Some(vnormalize([0.35, -0.94, 0.05])),
        kink_start: Some(vnormalize([0.2, -0.97, 0.1])),
        strength: 1.0,
    };
    let bent =
        build_wing_mesh("w", &stations, true, &quality, true, false, Some(tangency)).unwrap();
    let validation = bent.mesh.validate();
    assert!(validation.is_sound() && validation.closed);
    // Both panels now bulge: the straight reference has less projected area.
    let straight = build_wing_mesh("w", &stations, true, &quality, true, false, None).unwrap();
    assert!(bent.mesh.projected_area_xy() > straight.mesh.projected_area_xy());
}
