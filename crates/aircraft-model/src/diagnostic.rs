//! Diagnostic model shared by every validation and evaluation stage.
//!
//! Diagnostics always point at the editable source: either a JSON path into the
//! aircraft document, or a human-readable subject such as
//! `main-wing / station tip / position x`.

use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Code {
    /// The document does not satisfy the JSON Schema.
    SchemaViolation,
    /// An expression failed to lex or parse.
    ExpressionSyntax,
    /// A parameter, station, component, or interface reference does not resolve.
    UnknownReference,
    /// A dependency cycle was found; the message carries the chain.
    CycleDetected,
    /// An expression or parameter produces the wrong dimension for its field.
    DimensionMismatch,
    /// A value is NaN or infinite.
    NonFiniteResult,
    /// A symmetric wing's root station is not on local `y = 0`.
    RootOffSymmetryPlane,
    /// Stations do not progress strictly outboard into negative local Y.
    StationOrderViolation,
    /// Chord is zero or negative.
    NonPositiveChord,
    /// A trailing-edge treatment is infeasible for its field.
    InvalidTrailingEdge,
    /// A profile is invalid, self-intersecting, or impossible to loft.
    InvalidProfile,
    /// Mesh generation failed; the message names the geometry and tolerance.
    MeshFailure,
    /// A required identifier is duplicated.
    DuplicateIdentifier,
    /// The global unit system is missing or inconsistent.
    InvalidUnits,
    /// An internal invariant failed; never expected in normal operation.
    Internal,
}

impl Code {
    pub fn as_str(self) -> &'static str {
        match self {
            Code::SchemaViolation => "schema-violation",
            Code::ExpressionSyntax => "expression-syntax",
            Code::UnknownReference => "unknown-reference",
            Code::CycleDetected => "cycle-detected",
            Code::DimensionMismatch => "dimension-mismatch",
            Code::NonFiniteResult => "non-finite-result",
            Code::RootOffSymmetryPlane => "root-off-symmetry-plane",
            Code::StationOrderViolation => "station-order-violation",
            Code::NonPositiveChord => "non-positive-chord",
            Code::InvalidTrailingEdge => "invalid-trailing-edge",
            Code::InvalidProfile => "invalid-profile",
            Code::MeshFailure => "mesh-failure",
            Code::DuplicateIdentifier => "duplicate-identifier",
            Code::InvalidUnits => "invalid-units",
            Code::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub code: Code,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

impl Diagnostic {
    pub fn error(code: Code, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: Severity::Error,
            message: message.into(),
            path: None,
            subject: None,
        }
    }

    pub fn warning(code: Code, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            code,
            message: message.into(),
            path: None,
            subject: None,
        }
    }

    /// Attach the JSON path of the offending source.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Attach a human-readable subject such as `main-wing / station tip / position x`.
    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code.as_str(), self.message)?;
        if let Some(subject) = &self.subject {
            write!(f, " (at {subject})")?;
        } else if let Some(path) = &self.path {
            write!(f, " (at {path})")?;
        }
        Ok(())
    }
}
