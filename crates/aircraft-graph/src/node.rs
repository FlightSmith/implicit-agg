//! Typed graph nodes: one per raw parameter and per typed numeric field.

use aircraft_expr::{Checked, Dimension, Ref};
use aircraft_model::Code;

pub type NodeId = usize;

/// Which typed field of a station a node represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    PositionX,
    PositionY,
    PositionZ,
    Chord,
    Twist,
    TrailingEdgeThickness,
    TrailingEdgeFraction,
}

impl FieldKind {
    pub fn dimension(self) -> Dimension {
        match self {
            FieldKind::PositionX
            | FieldKind::PositionY
            | FieldKind::PositionZ
            | FieldKind::Chord
            | FieldKind::TrailingEdgeThickness => Dimension::LENGTH,
            FieldKind::Twist => Dimension::ANGLE,
            FieldKind::TrailingEdgeFraction => Dimension::RATIO,
        }
    }

    /// Human-readable fragment for diagnostics, matching docs/04 wording.
    pub fn subject_name(self) -> &'static str {
        match self {
            FieldKind::PositionX => "position x",
            FieldKind::PositionY => "position y",
            FieldKind::PositionZ => "position z",
            FieldKind::Chord => "chord",
            FieldKind::Twist => "twist",
            FieldKind::TrailingEdgeThickness => "trailing edge thickness",
            FieldKind::TrailingEdgeFraction => "trailing edge fraction",
        }
    }
}

#[derive(Debug, Clone)]
pub enum NodeKind {
    Parameter { id: String },
    StationField {
        component_index: usize,
        station_index: usize,
        field: FieldKind,
    },
    FrameOrigin {
        component_index: usize,
        coordinate: aircraft_expr::Coordinate,
    },
}

/// Where a node's numeric value comes from.
#[derive(Debug, Clone)]
pub enum ValueSource {
    /// The raw value of a parameter node (interpreted in document units).
    Raw(f64),
    Literal(f64),
    Parameter(String),
    Expression(Checked),
    /// A parsed expression awaiting dependency resolution; never present in a
    /// successfully built graph.
    Unresolved(aircraft_expr::Expr),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    /// Human-readable identity, e.g. `main-wing / station kink / position x`.
    pub subject: String,
    /// JSON path into the source document.
    pub path: String,
    /// Short identity for dependency chains, e.g. `kink.position.x`.
    pub short_name: String,
    /// The dimension this node's value is evaluated in (canonical units).
    pub dimension: Option<Dimension>,
    pub source: ValueSource,
    /// References used by this node's expression, mapped to producer nodes.
    pub ref_nodes: Vec<(Ref, NodeId)>,
    /// Producer nodes this node depends on.
    pub deps: Vec<NodeId>,
}

impl Node {
    /// The dimension demanded of any parameter feeding this node.
    pub fn expected_dimension(&self) -> Dimension {
        self.dimension.unwrap_or(Dimension::RATIO)
    }

    /// Code used when this node's value is not finite.
    pub fn non_finite_code(&self) -> Code {
        Code::NonFiniteResult
    }
}
