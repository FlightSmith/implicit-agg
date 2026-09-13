//! Leading-edge tangency DSL: `left:auto;right:0.8,0,0.1` and friends.
//!
//! Sides: `left` is the root-kink panel, `right` the kink-tip panel, `full`
//! both at once. A side's spec is `auto` (adapt to the other side; when both
//! sides are `auto`, they meet on the mean of their original directions) or
//! an explicit tangent vector `x,y,z` in aircraft axes.

use crate::diagnostic::{Code, Diagnostic};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LeSide {
    Auto,
    Vector([f64; 3]),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LeTangencyDsl {
    pub left: Option<LeSide>,
    pub right: Option<LeSide>,
}

impl LeTangencyDsl {
    pub fn is_empty(&self) -> bool {
        self.left.is_none() && self.right.is_none()
    }
}

fn parse_vector(text: &str, offset: usize) -> Result<[f64; 3], Diagnostic> {
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() != 3 {
        return Err(Diagnostic::error(
            Code::InvalidTangency,
            format!("tangent vector {text:?} must be x,y,z"),
        ));
    }
    let mut out = [0.0; 3];
    for (index, part) in parts.iter().enumerate() {
        out[index] = part.trim().parse::<f64>().map_err(|_| {
            Diagnostic::error(
                Code::InvalidTangency,
                format!("tangent component {part:?} is not a number"),
            )
        })?;
    }
    let _ = offset;
    Ok(out)
}

fn parse_side_spec(text: &str) -> Result<LeSide, Diagnostic> {
    if text.trim() == "auto" {
        return Ok(LeSide::Auto);
    }
    Ok(LeSide::Vector(parse_vector(text, 0)?))
}

/// Parse the leading-edge tangency DSL.
pub fn parse_le_tangency(dsl: &str) -> Result<LeTangencyDsl, Diagnostic> {
    let mut parsed = LeTangencyDsl::default();
    for clause in dsl.split(';') {
        let clause = clause.trim();
        if clause.is_empty() {
            return Err(Diagnostic::error(
                Code::InvalidTangency,
                "tangency contains an empty clause",
            ));
        }
        let Some((side, spec)) = clause.split_once(':') else {
            return Err(Diagnostic::error(
                Code::InvalidTangency,
                format!("tangency clause {clause:?} must read <side>:<spec>"),
            ));
        };
        let value = parse_side_spec(spec)?;
        match side.trim() {
            "left" => {
                if parsed.left.is_some() {
                    return Err(Diagnostic::error(
                        Code::InvalidTangency,
                        "duplicate 'left' clause",
                    ));
                }
                parsed.left = Some(value);
            }
            "right" => {
                if parsed.right.is_some() {
                    return Err(Diagnostic::error(
                        Code::InvalidTangency,
                        "duplicate 'right' clause",
                    ));
                }
                parsed.right = Some(value);
            }
            "full" => {
                if parsed.left.is_some() || parsed.right.is_some() {
                    return Err(Diagnostic::error(
                        Code::InvalidTangency,
                        "'full' cannot be combined with 'left' or 'right'",
                    ));
                }
                parsed.left = Some(value);
                parsed.right = Some(value);
            }
            other => {
                return Err(Diagnostic::error(
                    Code::InvalidTangency,
                    format!("unknown tangency side {other:?}; use left, right, or full"),
                ));
            }
        }
    }
    if parsed.is_empty() {
        return Err(Diagnostic::error(
            Code::InvalidTangency,
            "tangency must name at least one side",
        ));
    }
    Ok(parsed)
}
