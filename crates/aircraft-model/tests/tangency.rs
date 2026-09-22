#![allow(clippy::unwrap_used)]

use aircraft_model::tangency::parse_le_tangency;

#[test]
fn parses_auto_and_vector_sides() {
    let parsed = parse_le_tangency("left:auto").unwrap();
    assert!(parsed.left.unwrap().auto);
    assert_eq!(parsed.right, None);

    let parsed = parse_le_tangency("right:0.8,0,0.1").unwrap();
    assert_eq!(parsed.right.unwrap().vector, Some([0.8, 0.0, 0.1]));

    let parsed = parse_le_tangency("left:auto;right:-0.5,0,0.25").unwrap();
    assert!(parsed.left.unwrap().auto);
    assert_eq!(parsed.right.unwrap().vector, Some([-0.5, 0.0, 0.25]));

    let parsed = parse_le_tangency("full:1,0,0").unwrap();
    assert_eq!(parsed.left.unwrap().vector, Some([1.0, 0.0, 0.0]));
    assert_eq!(parsed.right.unwrap().vector, Some([1.0, 0.0, 0.0]));
}

#[test]
fn rejects_malformed_specs() {
    for bad in [
        "left",
        "left:",
        "middle:auto",
        "left:auto;left:auto",
        "full:auto;left:auto",
        "left:1,2",
        "left:a,b,c",
        ";",
        "left:auto;;right:auto",
    ] {
        assert!(parse_le_tangency(bad).is_err(), "{bad:?} must be rejected");
    }
}

#[test]
fn parses_station_clauses_with_strength() {
    let parsed = parse_le_tangency("root:0.9,-0.3,0.05").unwrap();
    assert_eq!(parsed.root.unwrap().vector, Some([0.9, -0.3, 0.05]));
    assert!(!parsed.root.unwrap().auto);

    let parsed = parse_le_tangency("tip:1,0,0:0.5;left:auto").unwrap();
    assert!((parsed.tip.unwrap().strength - 0.5).abs() < 1e-12);
    assert!(parsed.left.unwrap().auto);
    assert!((parsed.left.unwrap().strength - 1.0).abs() < 1e-12);

    let parsed = parse_le_tangency("left:auto:0.6;right:auto:0.8").unwrap();
    assert!((parsed.left.unwrap().strength - 0.6).abs() < 1e-12);
    assert!((parsed.right.unwrap().strength - 0.8).abs() < 1e-12);
}

#[test]
fn rejects_station_clause_misuse() {
    // auto is only valid for the panel clauses.
    assert!(parse_le_tangency("root:auto").is_err());
    // duplicate root
    assert!(parse_le_tangency("root:1,0,0;root:0,1,0").is_err());
    // vector required
    assert!(parse_le_tangency("tip:1,2").is_err());
}
