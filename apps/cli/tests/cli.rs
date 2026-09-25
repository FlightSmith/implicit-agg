#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

fn repo_path(relative: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(relative)
        .to_string_lossy()
        .to_string()
}

#[test]
fn the_supplied_example_validates_and_evaluates() {
    let (code, output) =
        aircraft_cli::run(["validate", &repo_path("examples/cranked-wing.v0.1.json")]);
    assert_eq!(code, aircraft_cli::EXIT_VALID, "output:\n{output}");
    assert!(output.contains("result: valid"));

    let (code, output) =
        aircraft_cli::run(["evaluate", &repo_path("examples/cranked-wing.v0.1.json")]);
    assert_eq!(code, aircraft_cli::EXIT_VALID, "output:\n{output}");
    // Kink x = 0.8 m and tip x = 2.2 m in document units.
    assert!(output.contains("kink"), "output:\n{output}");
    assert!(output.contains("0.8000"), "output:\n{output}");
    assert!(output.contains("2.2000"), "output:\n{output}");
}

#[test]
fn the_rectangular_fixture_validates() {
    let (code, output) = aircraft_cli::run([
        "validate",
        &repo_path("examples/fixtures/valid/rectangular-wing.json"),
    ]);
    assert_eq!(code, aircraft_cli::EXIT_VALID, "output:\n{output}");
}

#[test]
fn invalid_fixtures_fail_with_the_right_diagnostics() {
    let cases: &[(&str, &str)] = &[
        ("root-off-symmetry-plane", "root-off-symmetry-plane"),
        ("expression-cycle", "cycle-detected"),
        ("angle-as-length", "dimension-mismatch"),
        ("unknown-reference", "unknown-reference"),
        ("station-order", "station-order-violation"),
        ("zero-chord", "non-positive-chord"),
    ];
    for (fixture, expected_code) in cases {
        let path = repo_path(&format!("examples/fixtures/invalid/{fixture}.json"));
        let (code, output) = aircraft_cli::run(["validate", &path]);
        assert_eq!(
            code,
            aircraft_cli::EXIT_DIAGNOSTICS,
            "{fixture} must be rejected:\n{output}"
        );
        assert!(
            output.contains(expected_code),
            "{fixture} must report {expected_code}, got:\n{output}"
        );
        assert!(output.contains("result: invalid"));
    }
}

#[test]
fn a_missing_file_is_a_usage_failure_not_a_validation() {
    let (code, _) = aircraft_cli::run(["validate", "/nonexistent/aircraft.json"]);
    assert_eq!(code, aircraft_cli::EXIT_FAILURE);
}

#[test]
fn cycle_diagnostic_names_the_chain() {
    let (code, output) = aircraft_cli::run([
        "validate",
        &repo_path("examples/fixtures/invalid/expression-cycle.json"),
    ]);
    assert_eq!(code, aircraft_cli::EXIT_DIAGNOSTICS);
    assert!(
        output.contains("root.position.x") && output.contains("kink.position.x"),
        "chain must name the involved stations:\n{output}"
    );
}

#[test]
fn export_writes_geometry_files() {
    let dir = std::env::temp_dir().join(format!(
        "aircraft-cli-export-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let doc = repo_path("examples/cranked-wing.v0.1.json");

    // STEP by extension inference.
    let step = dir.join("wing.step");
    let (code, output) = aircraft_cli::run(["export", &doc, step.to_str().unwrap()]);
    assert_eq!(code, aircraft_cli::EXIT_VALID, "output:\n{output}");
    assert!(output.contains("wrote "), "output:\n{output}");
    let text = std::fs::read_to_string(&step).unwrap();
    assert!(text.contains("MANIFOLD_SOLID_BREP"), "no solid in STEP");

    // Full model with an explicit mesh format.
    let obj = dir.join("wing-full.obj");
    let (code, output) = aircraft_cli::run([
        "export",
        &doc,
        obj.to_str().unwrap(),
        "--format",
        "obj",
        "--full",
    ]);
    assert_eq!(code, aircraft_cli::EXIT_VALID, "output:\n{output}");
    let text = std::fs::read_to_string(&obj).unwrap();
    assert!(text.contains("v "), "no vertices in OBJ");

    // Unknown extension without --format is a usage failure.
    let bogus = dir.join("wing.bogus");
    let (code, output) = aircraft_cli::run(["export", &doc, bogus.to_str().unwrap()]);
    assert_eq!(code, aircraft_cli::EXIT_FAILURE, "output:\n{output}");
    assert!(output.contains("--format"), "output:\n{output}");

    // An invalid document fails before anything is written.
    let bad = dir.join("bad.step");
    let (code, output) = aircraft_cli::run([
        "export",
        &repo_path("examples/fixtures/invalid/zero-chord.json"),
        bad.to_str().unwrap(),
    ]);
    assert_eq!(code, aircraft_cli::EXIT_DIAGNOSTICS, "output:\n{output}");
    assert!(!bad.exists(), "invalid document must not write a file");

    std::fs::remove_dir_all(&dir).unwrap();
}
