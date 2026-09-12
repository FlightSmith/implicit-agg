//! Evaluator for checked expressions.
//!
//! Values flow in canonical units (meters, radians): reference values come
//! from [`RefValues`] in canonical form, and literals convert once here using
//! the dimension the checker assigned them and the document unit system.

use crate::ast::{BinOp, CmpOp, Function, Ref};
use crate::check::TExpr;
use crate::dim::{Dim, Dimension};
use aircraft_model::UnitSystem;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Number(f64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EvalError {
    #[error("division by zero")]
    DivideByZero,
    #[error("non-finite result")]
    NonFinite,
    #[error("{0}")]
    Domain(String),
    #[error("reference failed to evaluate: {0}")]
    Reference(String),
}

/// Supplies canonical-unit values for references.
pub trait RefValues {
    fn value_of(&self, reference: &Ref) -> Result<f64, EvalError>;
}

pub struct EvalContext<'a> {
    pub units: &'a UnitSystem,
    pub refs: &'a dyn RefValues,
}

/// Evaluate a checked expression to a canonical-unit value.
pub fn evaluate(expr: &TExpr, ctx: &EvalContext<'_>) -> Result<Value, EvalError> {
    let value = walk(expr, ctx)?;
    if let Value::Number(number) = value {
        if !number.is_finite() {
            return Err(EvalError::NonFinite);
        }
    }
    Ok(value)
}

fn walk(expr: &TExpr, ctx: &EvalContext<'_>) -> Result<Value, EvalError> {
    match expr {
        TExpr::Literal(value, dim) => Ok(Value::Number(literal_in_canonical(*value, *dim, ctx.units))),
        TExpr::Reference(reference, _) => {
            Ok(Value::Number(ctx.refs.value_of(reference)?))
        }
        TExpr::Negate(_, operand) => match walk(operand, ctx)? {
            Value::Number(number) => Ok(Value::Number(-number)),
            Value::Bool(_) => Err(EvalError::Domain("negate expects a number".to_string())),
        },
        TExpr::Binary(op, _, lhs, rhs) => {
            let lhs = expect_number(walk(lhs, ctx)?)?;
            let rhs = expect_number(walk(rhs, ctx)?)?;
            let result = match op {
                BinOp::Add => lhs + rhs,
                BinOp::Sub => lhs - rhs,
                BinOp::Mul => lhs * rhs,
                BinOp::Div => {
                    if rhs == 0.0 {
                        return Err(EvalError::DivideByZero);
                    }
                    lhs / rhs
                }
            };
            Ok(Value::Number(result))
        }
        TExpr::Compare(op, lhs, rhs) => {
            let lhs = expect_number(walk(lhs, ctx)?)?;
            let rhs = expect_number(walk(rhs, ctx)?)?;
            let result = match op {
                CmpOp::Less => lhs < rhs,
                CmpOp::LessEqual => lhs <= rhs,
                CmpOp::Greater => lhs > rhs,
                CmpOp::GreaterEqual => lhs >= rhs,
                CmpOp::Equal => lhs == rhs,
                CmpOp::NotEqual => lhs != rhs,
            };
            Ok(Value::Bool(result))
        }
        TExpr::Call(function, _, args) => call(*function, args, ctx),
    }
}

fn call(function: Function, args: &[TExpr], ctx: &EvalContext<'_>) -> Result<Value, EvalError> {
    let numbers = |args: &[TExpr]| -> Result<Vec<f64>, EvalError> {
        args.iter()
            .map(|arg| expect_number(walk(arg, ctx)?))
            .collect()
    };
    let value = match function {
        Function::Min => numbers(args)?.iter().copied().fold(f64::INFINITY, f64::min),
        Function::Max => numbers(args)?.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        Function::Abs => numbers(args)?[0].abs(),
        Function::Clamp => {
            let values = numbers(args)?;
            values[0].clamp(values[1], values[2])
        }
        Function::Sqrt => {
            let value = numbers(args)?[0];
            if value < 0.0 {
                return Err(EvalError::Domain("sqrt of a negative number".to_string()));
            }
            value.sqrt()
        }
        Function::Pow => {
            let base = expect_number(walk(&args[0], ctx)?)?;
            let TExpr::Literal(exponent, _) = &args[1] else {
                return Err(EvalError::Domain(
                    "pow exponent must be a literal integer".to_string(),
                ));
            };
            base.powi(*exponent as i32)
        }
        Function::Sin => numbers(args)?[0].sin(),
        Function::Cos => numbers(args)?[0].cos(),
        Function::Tan => numbers(args)?[0].tan(),
        Function::Lerp => {
            let values = numbers(args)?;
            values[0] + (values[1] - values[0]) * values[2]
        }
    };
    Ok(Value::Number(value))
}

fn expect_number(value: Value) -> Result<f64, EvalError> {
    match value {
        Value::Number(number) => Ok(number),
        Value::Bool(_) => Err(EvalError::Domain("expected a number, got a boolean".to_string())),
    }
}

/// Convert a literal annotated with a dimension into canonical units.
/// A literal checked as a plain scalar (ratio or unconstrained) is used as-is.
fn literal_in_canonical(value: f64, dim: Dim, units: &UnitSystem) -> f64 {
    match dim {
        Dim::Of(dimension) if dimension == Dimension::LENGTH => units.length_to_canonical(value),
        Dim::Of(dimension) if dimension == Dimension::ANGLE => units.angle_to_canonical(value),
        _ => value,
    }
}
