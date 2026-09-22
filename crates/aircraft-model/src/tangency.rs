//! Leading-edge tangency DSL.
//!
//! Panel clauses shape the two panels: `left` is the root-kink panel, `right`
//! the kink-tip panel, `full` both at once. Their spec is `auto` (adapt to
//! the other side; when both sides are `auto`, they meet on the mean of their
//! original directions) or an explicit tangent vector `x,y,z` in aircraft
//! axes — both constraints act at the shared kink.
//!
//! Station clauses anchor the LE path at the ends: `root:x,y,z` sets the
//! direction the LE leaves the root section with, `tip:x,y,z` the direction
//! it arrives at the tip with.
//!
//! Every spec takes an optional trailing `:number` tangency strength
//! (0 = straight, 1 = default fullness); omitted means 1.0.

use crate::diagnostic::{Code, Diagnostic};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Side {
    Left,
    Right,
    Full,
    Root,
    Tip,
}

/// A parsed spec: `auto` or a tangent vector, plus its strength.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spec {
    /// `true` for `auto`; `false` for an explicit vector in `vector`.
    pub auto: bool,
    pub vector: Option<[f64; 3]>,
    pub strength: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LeTangencyDsl {
    pub left: Option<Spec>,
    pub right: Option<Spec>,
    pub root: Option<Spec>,
    pub tip: Option<Spec>,
}

impl LeTangencyDsl {
    pub fn is_empty(&self) -> bool {
        self.left.is_none() && self.right.is_none() && self.root.is_none() && self.tip.is_none()
    }
}

fn parse_number(text: &str, what: &str) -> Result<f64, Diagnostic> {
    text.trim().parse::<f64>().map_err(|_| {
        Diagnostic::error(
            Code::InvalidTangency,
            format!("{what} {text:?} is not a number"),
        )
    })
}

fn parse_vector(text: &str) -> Result<[f64; 3], Diagnostic> {
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() != 3 {
        return Err(Diagnostic::error(
            Code::InvalidTangency,
            format!("tangent vector {text:?} must be x,y,z"),
        ));
    }
    let mut out = [0.0; 3];
    for (index, part) in parts.iter().enumerate() {
        out[index] = parse_number(part, "tangent component")?;
    }
    Ok(out)
}

fn parse_spec(text: &str, allow_auto: bool) -> Result<Spec, Diagnostic> {
    // Split the optional trailing :strength — only when a single colon
    // follows a complete spec (auto or three vector components).
    let mut segments: Vec<&str> = text.split(':').collect();
    let mut strength = 1.0;
    if segments.len() > 1 {
        let last = segments.last().expect("non-empty");
        if let Ok(value) = parse_number(last, "strength") {
            strength = value;
            segments.pop();
        }
    }
    let spec_text = segments.join(":");
    if spec_text.trim() == "auto" {
        if !allow_auto {
            return Err(Diagnostic::error(
                Code::InvalidTangency,
                "this clause requires an explicit tangent vector; 'auto' is only valid for left, right, or full",
            ));
        }
        return Ok(Spec {
            auto: true,
            vector: None,
            strength,
        });
    }
    Ok(Spec {
        auto: false,
        vector: Some(parse_vector(&spec_text)?),
        strength,
    })
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
        let side = side.trim();
        let take = |slot: &mut Option<Spec>, name: &str| -> Result<(), Diagnostic> {
            if slot.is_some() {
                return Err(Diagnostic::error(
                    Code::InvalidTangency,
                    format!("duplicate {name:?} clause"),
                ));
            }
            *slot = Some(parse_spec(spec, !matches!(side, "root" | "tip"))?);
            Ok(())
        };
        match side {
            "left" => take(&mut parsed.left, "left")?,
            "right" => take(&mut parsed.right, "right")?,
            "root" => take(&mut parsed.root, "root")?,
            "tip" => take(&mut parsed.tip, "tip")?,
            "full" => {
                if parsed.left.is_some() || parsed.right.is_some() {
                    return Err(Diagnostic::error(
                        Code::InvalidTangency,
                        "'full' cannot be combined with 'left' or 'right'",
                    ));
                }
                let value = parse_spec(spec, true)?;
                parsed.left = Some(value);
                parsed.right = Some(value);
            }
            other => {
                return Err(Diagnostic::error(
                    Code::InvalidTangency,
                    format!("unknown tangency side {other:?}; use left, right, full, root, or tip"),
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
