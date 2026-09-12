//! Rust types mirroring `schemas/aircraft-definition-v0.1.schema.json`.
//!
//! Maps and component collections are ordered deterministically (BTreeMap) so
//! serialization is canonical; the source document remains the only truth.

use crate::typed_value::TypedValue;
use crate::units::UnitSystem;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_ID: &str = "aircraft-definition/v0.1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AircraftDefinition {
    pub schema: String,
    pub id: String,
    pub name: String,
    pub units: UnitSystem,
    #[serde(rename = "unitSystemLocked")]
    pub unit_system_locked: bool,
    #[serde(rename = "coordinateSystem")]
    pub coordinate_system: CoordinateSystem,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub airfoils: BTreeMap<String, Airfoil>,
    pub components: Vec<Component>,
    #[serde(
        rename = "analysisCases",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub analysis_cases: Vec<AnalysisCase>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    #[serde(rename = "aft")]
    Aft,
    #[serde(rename = "starboard")]
    Starboard,
    #[serde(rename = "up")]
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Axes {
    pub x: Axis,
    pub y: Axis,
    pub z: Axis,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoordinateSystem {
    pub axes: Axes,
    pub origin: CoordinateOrigin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoordinateOrigin {
    pub kind: OriginKind,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OriginKind {
    NamedDatum,
    UserSelected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Airfoil {
    #[serde(rename = "naca4")]
    Naca4 { code: String },
    #[serde(rename = "coordinates")]
    Coordinates { points: Vec<[f64; 2]> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: TypedValue,
    pub y: TypedValue,
    pub z: TypedValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum TrailingEdge {
    Sharp,
    Absolute { thickness: TypedValue },
    ChordFraction { value: TypedValue },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Station {
    pub id: String,
    pub position: Position,
    pub chord: TypedValue,
    pub twist: TypedValue,
    pub airfoil: String,
    #[serde(
        rename = "trailingEdge",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub trailing_edge: Option<TrailingEdge>,
}

impl Station {
    /// The effective trailing-edge treatment; an omitted field defaults to sharp.
    pub fn trailing_edge(&self) -> &TrailingEdge {
        static SHARP: TrailingEdge = TrailingEdge::Sharp;
        self.trailing_edge.as_ref().unwrap_or(&SHARP)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymmetryPlane {
    #[serde(rename = "local-xz")]
    LocalXz,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceSide {
    #[serde(rename = "negative-local-y")]
    NegativeLocalY,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CenterlineTreatment {
    Weld,
    Open,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Symmetry {
    pub enabled: bool,
    pub plane: SymmetryPlane,
    #[serde(rename = "sourceSide")]
    pub source_side: SourceSide,
    #[serde(rename = "centerlineTreatment")]
    pub centerline_treatment: CenterlineTreatment,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub origin: Position,
    pub axes: FrameAxes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameAxes {
    #[serde(rename = "inherit-aircraft")]
    InheritAircraft,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Interface {
    #[serde(rename = "station-plane")]
    StationPlane { station: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WingRole {
    Main,
    Canard,
    Tail,
    Fin,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wing {
    pub id: String,
    pub role: WingRole,
    pub frame: Frame,
    pub symmetry: Symmetry,
    pub stations: Vec<Station>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub interfaces: BTreeMap<String, Interface>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Component {
    Wing(Wing),
}

impl Component {
    pub fn id(&self) -> &str {
        match self {
            Component::Wing(wing) => &wing.id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisCase {
    pub id: String,
    pub mach: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub altitude: Option<TypedValue>,
}

/// JSON path of a station inside the document, for diagnostics.
pub fn station_path(component_index: usize, station_index: usize) -> String {
    format!("components/{component_index}/stations/{station_index}")
}
