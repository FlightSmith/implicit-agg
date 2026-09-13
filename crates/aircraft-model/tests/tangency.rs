#![allow(clippy::unwrap_used)]

use aircraft_model::tangency::{parse_le_tangency, LeSide};

#[test]
fn parses_auto_and_vector_sides() {
    let parsed = parse_le_tangency("left:auto").unwrap();
    assert_eq!(parsed.left, Some(LeSide::Auto));
    assert_eq!(parsed.right, None);

    let parsed = parse_le_tangency("right:0.8,0,0.1").unwrap();
    assert_eq!(parsed.right, Some(LeSide::Vector([0.8, 0.0, 0.1])));

    let parsed = parse_le_tangency("left:auto;right:-0.5,0,0.25").unwrap();
    assert_eq!(parsed.left, Some(LeSide::Auto));
    assert_eq!(parsed.right, Some(LeSide::Vector([-0.5, 0.0, 0.25])));

    let parsed = parse_le_tangency("full:1,0,0").unwrap();
    assert_eq!(parsed.left, Some(LeSide::Vector([1.0, 0.0, 0.0])));
    assert_eq!(parsed.right, Some(LeSide::Vector([1.0, 0.0, 0.0])));
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
