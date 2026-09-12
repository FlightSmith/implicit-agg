//! The typed expression language for aircraft documents.
//!
//! A deliberately small, pure language: literals, `+ - * /`, comparisons,
//! parentheses, and the functions `min max clamp abs sqrt pow sin cos tan
//! lerp`. References start with `@param`, `@station`, or `@component`.
//! Every expression is dimension-checked against the field that consumes it;
//! there is no I/O, no scripting, and no user-defined functions.

pub mod ast;
pub mod check;
pub mod dim;
pub mod eval;
pub mod lexer;
pub mod parser;

pub use ast::{print, BinOp, CmpOp, Coordinate, Expr, Function, Ref, StationLeaf};
pub use check::{check, Checked, RefResolver, TExpr};
pub use dim::{Dim, Dimension};
pub use eval::{evaluate, EvalContext, EvalError, RefValues, Value};
pub use parser::{parse_field_expression, parse_formula};

#[cfg(test)]
mod tests {
    use super::*;

    fn formula(src: &str) -> Expr {
        parse_formula(src).unwrap()
    }

    #[test]
    fn numbers_and_operators_parse() {
        assert_eq!(formula("1 + 2 * 3"), Expr::Binary(
            BinOp::Add,
            Box::new(Expr::Literal(1.0)),
            Box::new(Expr::Binary(BinOp::Mul, Box::new(Expr::Literal(2.0)), Box::new(Expr::Literal(3.0)))),
        ));
    }

    #[test]
    fn references_parse_with_longest_identifier() {
        assert_eq!(
            formula("@param.wing.rootToKink.dx"),
            Expr::Reference(Ref::Parameter("wing.rootToKink.dx".to_string()))
        );
        assert_eq!(
            formula("@station.root.position.x"),
            Expr::Reference(Ref::Station {
                station: "root".to_string(),
                leaf: StationLeaf::PositionX,
            })
        );
        assert_eq!(
            formula("@component.main-wing.interface.root-attachment.origin.z"),
            Expr::Reference(Ref::InterfaceOrigin {
                component: "main-wing".to_string(),
                interface: "root-attachment".to_string(),
                coordinate: Coordinate::Z,
            })
        );
    }

    #[test]
    fn unknown_prefixes_are_rejected() {
        assert!(parse_formula("@parametric.value").is_err());
        assert!(parse_formula("@bogus.thing").is_err());
    }

    #[test]
    fn field_expression_requires_leading_equals() {
        assert!(parse_field_expression("1 + 2").is_err());
        assert!(parse_field_expression("= 1 + 2").is_ok());
    }

    #[test]
    fn trailing_input_is_rejected() {
        assert!(parse_formula("1 2").is_err());
        assert!(parse_formula("(1))").is_err());
        assert!(parse_formula("").is_err());
    }
}
