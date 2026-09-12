//! Dimensional types for the expression language.
//!
//! A [`Dimension`] is an exponent vector over `{length, angle}`. A plain
//! number (ratio) is the zero vector. This generalizes the dimension rules in
//! docs/04 and lets distance-style expressions such as
//! `sqrt(dx*dx + dz*dz)` type-check as a length.
//!
//! [`Dim`] adds two non-numeric dimensions: [`Dim::Any`] marks a value whose
//! dimension is not yet constrained (literals, unconstrained parameters), and
//! [`Dim::Bool`] is the result of a comparison.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Dimension {
    pub length: i32,
    pub angle: i32,
}

impl Dimension {
    pub const RATIO: Self = Self { length: 0, angle: 0 };
    pub const LENGTH: Self = Self { length: 1, angle: 0 };
    pub const ANGLE: Self = Self { length: 0, angle: 1 };

    pub fn mul(self, other: Self) -> Self {
        Dimension {
            length: self.length + other.length,
            angle: self.angle + other.angle,
        }
    }

    pub fn div(self, other: Self) -> Self {
        Dimension {
            length: self.length - other.length,
            angle: self.angle - other.angle,
        }
    }

    pub fn invert(self) -> Self {
        Dimension {
            length: -self.length,
            angle: -self.angle,
        }
    }

    pub fn pow(self, exponent: i32) -> Self {
        Dimension {
            length: self.length * exponent,
            angle: self.angle * exponent,
        }
    }

    /// Square root; only exact when every exponent is even.
    pub fn sqrt(self) -> Option<Self> {
        if self.length % 2 != 0 || self.angle % 2 != 0 {
            return None;
        }
        Some(Dimension {
            length: self.length / 2,
            angle: self.angle / 2,
        })
    }

    /// Human-readable description for diagnostics.
    pub fn describe(self) -> String {
        match (self.length, self.angle) {
            (0, 0) => "a ratio (dimensionless)".to_string(),
            (1, 0) => "a length".to_string(),
            (0, 1) => "an angle".to_string(),
            (length, angle) => {
                let mut parts = Vec::new();
                if length != 0 {
                    parts.push(power_name("length", length));
                }
                if angle != 0 {
                    parts.push(power_name("angle", angle));
                }
                parts.join(" * ")
            }
        }
    }
}

fn power_name(base: &str, power: i32) -> String {
    match power {
        1 => base.to_string(),
        -1 => format!("1/{base}"),
        _ => format!("{base}^{power}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dim {
    /// Dimension not yet constrained; binds to whatever it unifies with.
    Any,
    /// Result of a comparison; cannot feed a numeric field.
    Bool,
    Of(Dimension),
}

impl Dim {
    pub fn unify(a: Self, b: Self) -> Option<Self> {
        match (a, b) {
            (Dim::Any, other) | (other, Dim::Any) => Some(other),
            (Dim::Bool, Dim::Bool) => Some(Dim::Bool),
            (Dim::Of(a), Dim::Of(b)) if a == b => Some(Dim::Of(a)),
            _ => None,
        }
    }

    pub fn describe(self) -> String {
        match self {
            Dim::Any => "an unconstrained number".to_string(),
            Dim::Bool => "a boolean".to_string(),
            Dim::Of(dimension) => dimension.describe(),
        }
    }
}
