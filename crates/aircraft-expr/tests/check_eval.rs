#![allow(clippy::unwrap_used)]

use aircraft_expr::{
    check, evaluate, parse_formula, Dim, Dimension, EvalContext, EvalError, Expr, Function, Ref,
    RefResolver, RefValues,
};
use aircraft_model::{AngleUnit, Code, Diagnostic, LengthUnit, UnitSystem};

/// Resolver with a fixed catalog of reference dimensions.
struct FixedResolver(Vec<(Ref, Dim)>);

impl RefResolver for FixedResolver {
    fn dimension_of(&mut self, reference: &Ref) -> Result<Dim, Diagnostic> {
        self.0
            .iter()
            .find(|(known, _)| known == reference)
            .map(|(_, dim)| *dim)
            .ok_or_else(|| {
                Diagnostic::error(
                    Code::UnknownReference,
                    format!("unknown reference {reference}"),
                )
            })
    }
}

impl Default for FixedResolver {
    fn default() -> Self {
        Self(vec![
            (
                Ref::Station {
                    station: "root".into(),
                    leaf: aircraft_expr::StationLeaf::PositionX,
                },
                Dim::Of(Dimension::LENGTH),
            ),
            (
                Ref::Station {
                    station: "root".into(),
                    leaf: aircraft_expr::StationLeaf::PositionZ,
                },
                Dim::Of(Dimension::LENGTH),
            ),
            (
                Ref::Station {
                    station: "root".into(),
                    leaf: aircraft_expr::StationLeaf::Twist,
                },
                Dim::Of(Dimension::ANGLE),
            ),
            (Ref::Parameter("length".into()), Dim::Of(Dimension::LENGTH)),
            (Ref::Parameter("angle".into()), Dim::Of(Dimension::ANGLE)),
            (Ref::Parameter("free".into()), Dim::Any),
        ])
    }
}

struct FixedValues(Vec<(Ref, f64)>);

impl RefValues for FixedValues {
    fn value_of(&self, reference: &Ref) -> Result<f64, EvalError> {
        self.0
            .iter()
            .find(|(known, _)| known == reference)
            .map(|(_, value)| *value)
            .ok_or_else(|| EvalError::Reference(format!("{reference}")))
    }
}

impl Default for FixedValues {
    fn default() -> Self {
        Self(vec![(
            Ref::Station {
                station: "root".into(),
                leaf: aircraft_expr::StationLeaf::PositionX,
            },
            2.0,
        )])
    }
}

const DEG: UnitSystem = UnitSystem {
    length: LengthUnit::Meters,
    angle: AngleUnit::Degrees,
    mass: aircraft_model::MassUnit::Kilograms,
    force: aircraft_model::ForceUnit::Newtons,
    pressure: aircraft_model::PressureUnit::Pascals,
};

fn checked(src: &str, expected: Dim) -> aircraft_expr::Checked {
    let expr = parse_formula(src).unwrap();
    check(&expr, expected, &mut FixedResolver::default()).unwrap()
}

fn number(src: &str, expected: Dim) -> f64 {
    let checked = checked(src, expected);
    let ctx = EvalContext {
        units: &DEG,
        refs: &FixedValues::default(),
    };
    match evaluate(&checked.expr, &ctx).unwrap() {
        aircraft_expr::Value::Number(value) => value,
        aircraft_expr::Value::Bool(_) => panic!("expected a number from {src}"),
    }
}

fn error_of(src: &str, expected: Dim) -> Diagnostic {
    let expr = parse_formula(src).unwrap();
    check(&expr, expected, &mut FixedResolver::default())
        .expect_err("expression should not type-check")
}

#[test]
fn arithmetic_respects_precedence() {
    assert!((number("1 + 2 * 3", Dim::Any) - 7.0).abs() < 1e-12);
    assert!((number("(1 + 2) * 3", Dim::Any) - 9.0).abs() < 1e-12);
    assert!((number("-2 + 5", Dim::Any) - 3.0).abs() < 1e-12);
    assert!((number("10 / 4", Dim::Any) - 2.5).abs() < 1e-12);
}

#[test]
fn functions_evaluate() {
    assert!((number("min(3, 1, 2)", Dim::Any) - 1.0).abs() < 1e-12);
    assert!((number("max(3, 1, 2)", Dim::Any) - 3.0).abs() < 1e-12);
    assert!((number("clamp(5, 0, 3)", Dim::Any) - 3.0).abs() < 1e-12);
    assert!((number("abs(0 - 4)", Dim::Any) - 4.0).abs() < 1e-12);
    assert!((number("sqrt(9)", Dim::Any) - 3.0).abs() < 1e-12);
    assert!((number("pow(2, 10)", Dim::Any) - 1024.0).abs() < 1e-9);
    assert!((number("lerp(0, 10, 0.25)", Dim::Any) - 2.5).abs() < 1e-12);
}

#[test]
fn trig_functions_interpret_angles_in_document_units() {
    // 30 degrees in a degree document evaluates in canonical radians.
    assert!((number("sin(30)", Dim::Of(Dimension::RATIO)) - 0.5).abs() < 1e-12);
}

#[test]
fn distances_type_check_as_length() {
    // sqrt of a squared length is a length: the exponent-vector rule.
    let checked = checked(
        "sqrt(@station.root.position.x * @station.root.position.x)",
        Dim::Of(Dimension::LENGTH),
    );
    assert_eq!(checked.dim, Dim::Of(Dimension::LENGTH));
}

#[test]
fn length_times_length_is_not_a_length() {
    let diagnostic = error_of(
        "@station.root.position.x * @station.root.position.x",
        Dim::Of(Dimension::LENGTH),
    );
    assert_eq!(diagnostic.code, Code::DimensionMismatch);
}

#[test]
fn length_plus_angle_is_rejected() {
    let diagnostic = error_of(
        "@station.root.position.x + @station.root.twist",
        Dim::Of(Dimension::LENGTH),
    );
    assert_eq!(diagnostic.code, Code::DimensionMismatch);
    assert!(diagnostic.message.contains("length"));
    assert!(diagnostic.message.contains("angle"));
}

#[test]
fn angle_literal_converts_for_trig() {
    // sin of a 90 degree angle parameter: canonical radians in, ratio out.
    struct AngleValue;
    impl RefValues for AngleValue {
        fn value_of(&self, _reference: &Ref) -> Result<f64, EvalError> {
            Ok(std::f64::consts::FRAC_PI_2)
        }
    }
    let checked = checked("sin(@station.root.twist)", Dim::Of(Dimension::RATIO));
    let ctx = EvalContext {
        units: &DEG,
        refs: &AngleValue,
    };
    match evaluate(&checked.expr, &ctx).unwrap() {
        aircraft_expr::Value::Number(value) => assert!((value - 1.0).abs() < 1e-12),
        aircraft_expr::Value::Bool(_) => panic!("expected a number"),
    }
}

#[test]
fn lerp_rejects_a_non_ratio_blend() {
    let diagnostic = error_of(
        "lerp(0, 10, @station.root.twist)",
        Dim::Of(Dimension::LENGTH),
    );
    assert_eq!(diagnostic.code, Code::DimensionMismatch);
}

#[test]
fn pow_requires_an_integer_literal_exponent() {
    assert!(parse_formula("pow(2, 0.5)").is_ok());
    let diagnostic = error_of("pow(4, 0.5)", Dim::Any);
    assert_eq!(diagnostic.code, Code::DimensionMismatch);

    let expr: Expr = parse_formula("pow(2, @param.free)").unwrap();
    let diagnostic = check(&expr, Dim::Any, &mut FixedResolver::default())
        .expect_err("non-literal exponent must be rejected");
    assert_eq!(diagnostic.code, Code::DimensionMismatch);
}

#[test]
fn comparisons_produce_booleans_that_cannot_bind_to_numbers() {
    let checked = checked("1 < 2", Dim::Any);
    assert_eq!(checked.dim, Dim::Bool);

    let expr = parse_formula("1 < 2").unwrap();
    let diagnostic = check(&expr, Dim::Of(Dimension::LENGTH), &mut FixedResolver::default())
        .expect_err("boolean must not bind to a length field");
    assert_eq!(diagnostic.code, Code::DimensionMismatch);
}

#[test]
fn unknown_reference_is_reported() {
    let expr = parse_formula("@param.does.not.exist + 1").unwrap();
    let diagnostic = check(&expr, Dim::Of(Dimension::LENGTH), &mut FixedResolver::default())
        .expect_err("unknown reference must be rejected");
    assert_eq!(diagnostic.code, Code::UnknownReference);
}

#[test]
fn unconstrained_parameters_bind_to_the_field_dimension() {
    let checked = checked("@param.free * 2", Dim::Of(Dimension::LENGTH));
    assert_eq!(checked.dim, Dim::Of(Dimension::LENGTH));
    assert_eq!(checked.references.len(), 1);
}

#[test]
fn division_by_zero_and_overflow_are_runtime_errors() {
    let division = checked("1 / 0", Dim::Any);
    let ctx = EvalContext {
        units: &DEG,
        refs: &FixedValues::default(),
    };
    assert_eq!(
        evaluate(&division.expr, &ctx).unwrap_err(),
        EvalError::DivideByZero
    );

    let overflow = checked("pow(10, 1000)", Dim::Any);
    assert_eq!(
        evaluate(&overflow.expr, &ctx).unwrap_err(),
        EvalError::NonFinite
    );
}

#[test]
fn negative_sqrt_is_a_domain_error() {
    let checked = checked("sqrt(0 - 4)", Dim::Any);
    let ctx = EvalContext {
        units: &DEG,
        refs: &FixedValues::default(),
    };
    assert!(matches!(
        evaluate(&checked.expr, &ctx),
        Err(EvalError::Domain(_))
    ));
}

#[test]
fn printing_round_trips_through_the_parser() {
    for source in [
        "1 + 2 * 3",
        "-(1 + 2) * 3.5",
        "sqrt(@station.root.position.x * @station.root.position.x + 1)",
        "lerp(@param.length, 2, 0.5)",
        "@component.main-wing.interface.root-attachment.origin.z + 0.5",
        "min(1, 2, 3) == 3",
        "clamp(@param.free, 0, 10) / @station.root.position.x",
    ] {
        let parsed = parse_formula(source).unwrap();
        let printed = aircraft_expr::print(&parsed);
        let reparsed = parse_formula(&printed).unwrap();
        assert_eq!(parsed, reparsed, "round-trip failed for {source} -> {printed}");
    }
}

#[test]
fn functions_have_declared_arity() {
    assert_eq!(Function::Lerp.arity(), (3, 3));
    assert_eq!(Function::Min.arity().1, usize::MAX);
    assert!(parse_formula("lerp(1, 2)").is_err());
    assert!(parse_formula("min()").is_err());
    assert!(parse_formula("unknownfn(1)").is_err());
}
