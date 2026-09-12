//! Dimensional type checker.
//!
//! Walks the AST once, unifying dimensions bottom-up with the field's
//! expected dimension flowing down. The result is a checked tree whose
//! literal and reference nodes carry their resolved dimension, plus the list
//! of references used (with their dimension) so the caller can build
//! dependency edges.

use crate::ast::{BinOp, CmpOp, Expr, Function, Ref};
use crate::dim::{Dim, Dimension};
use aircraft_model::{Code, Diagnostic};

/// Supplies the dimension of a reference; an unknown reference is an error.
pub trait RefResolver {
    fn dimension_of(&mut self, reference: &Ref) -> Result<Dim, Diagnostic>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum TExpr {
    Literal(f64, Dim),
    Reference(Ref, Dim),
    Negate(Dim, Box<TExpr>),
    Binary(BinOp, Dim, Box<TExpr>, Box<TExpr>),
    Compare(CmpOp, Box<TExpr>, Box<TExpr>),
    Call(Function, Dim, Vec<TExpr>),
}

impl TExpr {
    pub fn dim(&self) -> Dim {
        match self {
            TExpr::Literal(_, dim) | TExpr::Reference(_, dim) => *dim,
            TExpr::Negate(dim, _) | TExpr::Binary(_, dim, _, _) | TExpr::Call(_, dim, _) => *dim,
            TExpr::Compare(..) => Dim::Bool,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Checked {
    pub expr: TExpr,
    /// The dimension the whole expression evaluates to; `Dim::Any` at the root
    /// binds to the field's expected dimension before this is reported.
    pub dim: Dim,
    /// Every reference used, with the dimension it was checked at. Duplicates
    /// are preserved; order follows the source.
    pub references: Vec<(Ref, Dim)>,
}

/// Check an expression against the expected dimension of its field.
pub fn check(
    expr: &Expr,
    expected: Dim,
    resolver: &mut dyn RefResolver,
) -> Result<Checked, Diagnostic> {
    let mut checker = Checker {
        resolver,
        references: Vec::new(),
    };
    let checked = checker.walk(expr, expected)?;
    let dim = match checked.dim() {
        Dim::Any => expected,
        other => other,
    };
    // The field's expected dimension is the final authority: a well-formed
    // expression with the wrong dimension is still a rejected edit.
    let compatible = match (expected, dim) {
        (Dim::Any, _) | (_, Dim::Any) => true,
        (Dim::Bool, Dim::Bool) => true,
        (Dim::Of(expected_dimension), Dim::Of(actual_dimension)) => {
            expected_dimension == actual_dimension
        }
        _ => false,
    };
    if !compatible {
        return Err(Diagnostic::error(
            Code::DimensionMismatch,
            format!(
                "expression result is {}, but this field requires {}",
                dim.describe(),
                expected.describe()
            ),
        ));
    }
    Ok(Checked {
        dim,
        expr: checked,
        references: checker.references,
    })
}

struct Checker<'r> {
    resolver: &'r mut dyn RefResolver,
    references: Vec<(Ref, Dim)>,
}

impl Checker<'_> {
    fn walk(&mut self, expr: &Expr, expected: Dim) -> Result<TExpr, Diagnostic> {
        match expr {
            Expr::Literal(value) => Ok(TExpr::Literal(*value, expected)),
            Expr::Reference(reference) => {
                let dim = self.resolver.dimension_of(reference)?;
                self.references.push((reference.clone(), dim));
                Ok(TExpr::Reference(reference.clone(), dim))
            }
            Expr::Negate(inner) => {
                let operand = self.walk(inner, numeric(expected)?)?;
                let dim = operand.dim();
                Ok(TExpr::Negate(dim, Box::new(operand)))
            }
            Expr::Binary(op, lhs, rhs) => self.binary(*op, lhs, rhs, expected),
            Expr::Compare(op, lhs, rhs) => {
                let lhs = self.walk(lhs, Dim::Any)?;
                let rhs = self.walk(rhs, Dim::Any)?;
                let unified = Dim::unify(lhs.dim(), rhs.dim())
                    .ok_or_else(|| mismatch("compare", lhs.dim(), rhs.dim()))?;
                if unified == Dim::Bool {
                    return Err(Diagnostic::error(
                        Code::DimensionMismatch,
                        "comparisons cannot be chained or compared",
                    ));
                }
                Ok(TExpr::Compare(*op, Box::new(lhs), Box::new(rhs)))
            }
            Expr::Call(function, args) => self.call(*function, args, expected),
        }
    }

    fn binary(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        expected: Dim,
    ) -> Result<TExpr, Diagnostic> {
        match op {
            BinOp::Add | BinOp::Sub => {
                let operation = if op == BinOp::Add { "add" } else { "subtract" };
                let lhs = self.walk(lhs, numeric(expected)?)?;
                let rhs = self.walk(rhs, numeric(expected)?)?;
                let unified = Dim::unify(lhs.dim(), rhs.dim())
                    .ok_or_else(|| mismatch(operation, lhs.dim(), rhs.dim()))?;
                if unified == Dim::Bool {
                    return Err(Diagnostic::error(
                        Code::DimensionMismatch,
                        format!("cannot {operation} boolean values"),
                    ));
                }
                Ok(TExpr::Binary(op, unified, Box::new(lhs), Box::new(rhs)))
            }
            BinOp::Mul => {
                let lhs = self.walk(lhs, Dim::Any)?;
                let rhs = self.walk(rhs, Dim::Any)?;
                let dim = product_dim(lhs.dim(), rhs.dim())
                    .ok_or_else(|| mismatch("multiply", lhs.dim(), rhs.dim()))?;
                Ok(TExpr::Binary(BinOp::Mul, dim, Box::new(lhs), Box::new(rhs)))
            }
            BinOp::Div => {
                let lhs = self.walk(lhs, Dim::Any)?;
                let rhs = self.walk(rhs, Dim::Any)?;
                let dim = quotient_dim(lhs.dim(), rhs.dim())
                    .ok_or_else(|| mismatch("divide", lhs.dim(), rhs.dim()))?;
                Ok(TExpr::Binary(BinOp::Div, dim, Box::new(lhs), Box::new(rhs)))
            }
        }
    }

    fn call(
        &mut self,
        function: Function,
        args: &[Expr],
        expected: Dim,
    ) -> Result<TExpr, Diagnostic> {
        match function {
            Function::Min | Function::Max | Function::Abs | Function::Clamp => {
                let operand_expected = numeric(expected)?;
                let mut checked = Vec::new();
                for arg in args {
                    checked.push(self.walk(arg, operand_expected)?);
                }
                let dim = self.unify_all(
                    &format!("{} arguments", function.name()),
                    &checked.iter().collect::<Vec<_>>(),
                )?;
                Ok(TExpr::Call(function, dim, checked))
            }
            Function::Lerp => {
                let operand_expected = numeric(expected)?;
                let a = self.walk(&args[0], operand_expected)?;
                let b = self.walk(&args[1], operand_expected)?;
                let blend = self.walk(&args[2], Dim::Of(Dimension::RATIO))?;
                let unified = self.unify_all("lerp endpoints", &[&a, &b])?;
                if Dim::unify(blend.dim(), Dim::Of(Dimension::RATIO)).is_none() {
                    return Err(Diagnostic::error(
                        Code::DimensionMismatch,
                        format!(
                            "lerp blend parameter must be a ratio, got {}",
                            blend.dim().describe()
                        ),
                    ));
                }
                Ok(TExpr::Call(function, unified, vec![a, b, blend]))
            }
            Function::Sqrt => {
                let operand = self.walk(&args[0], Dim::Any)?;
                let dim = match operand.dim() {
                    Dim::Any => numeric(expected)?,
                    Dim::Bool => {
                        return Err(Diagnostic::error(
                            Code::DimensionMismatch,
                            "sqrt does not accept a boolean",
                        ))
                    }
                    Dim::Of(dimension) => Dim::Of(dimension.sqrt().ok_or_else(|| {
                        Diagnostic::error(
                            Code::DimensionMismatch,
                            format!(
                                "sqrt needs even exponents (for example the squared length \
                                     inside a distance), got {}",
                                dimension.describe()
                            ),
                        )
                    })?),
                };
                Ok(TExpr::Call(function, dim, vec![operand]))
            }
            Function::Pow => {
                let base = self.walk(&args[0], Dim::Any)?;
                let Expr::Literal(exponent) = &args[1] else {
                    return Err(Diagnostic::error(
                        Code::DimensionMismatch,
                        "pow exponent must be a literal integer",
                    ));
                };
                if exponent.fract() != 0.0 {
                    return Err(Diagnostic::error(
                        Code::DimensionMismatch,
                        format!("pow exponent must be an integer, got {exponent}"),
                    ));
                }
                let dim = match base.dim() {
                    Dim::Any | Dim::Bool => base.dim(),
                    Dim::Of(dimension) => Dim::Of(dimension.pow(*exponent as i32)),
                };
                let exponent = TExpr::Literal(*exponent, Dim::Any);
                Ok(TExpr::Call(function, dim, vec![base, exponent]))
            }
            Function::Sin | Function::Cos | Function::Tan => {
                let operand = self.walk(&args[0], Dim::Of(Dimension::ANGLE))?;
                if Dim::unify(operand.dim(), Dim::Of(Dimension::ANGLE)).is_none() {
                    return Err(Diagnostic::error(
                        Code::DimensionMismatch,
                        format!(
                            "{} expects an angle, got {}",
                            function.name(),
                            operand.dim().describe()
                        ),
                    ));
                }
                Ok(TExpr::Call(
                    function,
                    Dim::Of(Dimension::RATIO),
                    vec![operand],
                ))
            }
        }
    }

    fn unify_all(&mut self, what: &str, operands: &[&TExpr]) -> Result<Dim, Diagnostic> {
        let mut unified = Dim::Any;
        for operand in operands {
            unified = Dim::unify(unified, operand.dim())
                .ok_or_else(|| mismatch(what, unified, operand.dim()))?;
        }
        if unified == Dim::Bool {
            return Err(Diagnostic::error(
                Code::DimensionMismatch,
                format!("{what} cannot be boolean"),
            ));
        }
        Ok(unified)
    }
}

fn numeric(expected: Dim) -> Result<Dim, Diagnostic> {
    match expected {
        Dim::Bool => Err(Diagnostic::error(
            Code::DimensionMismatch,
            "a boolean cannot be used where a numeric value is required",
        )),
        other => Ok(other),
    }
}

fn product_dim(lhs: Dim, rhs: Dim) -> Option<Dim> {
    match (lhs, rhs) {
        (Dim::Bool, _) | (_, Dim::Bool) => None,
        (Dim::Any, other) | (other, Dim::Any) => Some(other),
        (Dim::Of(a), Dim::Of(b)) => Some(Dim::Of(a * b)),
    }
}

fn quotient_dim(lhs: Dim, rhs: Dim) -> Option<Dim> {
    match (lhs, rhs) {
        (Dim::Bool, _) | (_, Dim::Bool) => None,
        (Dim::Any, Dim::Any) => Some(Dim::Any),
        (Dim::Any, Dim::Of(rhs)) => Some(Dim::Of(rhs.invert())),
        (Dim::Of(lhs), Dim::Any) => Some(Dim::Of(lhs)),
        (Dim::Of(lhs), Dim::Of(rhs)) => Some(Dim::Of(lhs / rhs)),
    }
}

fn mismatch(operation: &str, got: Dim, expected: Dim) -> Diagnostic {
    Diagnostic::error(
        Code::DimensionMismatch,
        format!(
            "cannot {operation} {} and {}",
            got.describe(),
            expected.describe()
        ),
    )
}
