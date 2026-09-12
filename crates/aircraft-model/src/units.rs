//! The single global, immutable unit system.
//!
//! Numbers in the document are plain values interpreted through the semantic
//! type of their field: positions and chords use the length unit, twist uses
//! the angle unit, ratios are dimensionless. Internally the compute core works
//! in canonical units (meters, radians) and converts at the document boundary.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LengthUnit {
    #[serde(rename = "m")]
    Meters,
    #[serde(rename = "mm")]
    Millimeters,
    #[serde(rename = "ft")]
    Feet,
    #[serde(rename = "in")]
    Inches,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AngleUnit {
    #[serde(rename = "deg")]
    Degrees,
    #[serde(rename = "rad")]
    Radians,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MassUnit {
    #[serde(rename = "kg")]
    Kilograms,
    #[serde(rename = "lbm")]
    PoundsMass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForceUnit {
    #[serde(rename = "N")]
    Newtons,
    #[serde(rename = "lbf")]
    PoundsForce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PressureUnit {
    #[serde(rename = "Pa")]
    Pascals,
    #[serde(rename = "kPa")]
    Kilopascals,
    #[serde(rename = "psi")]
    Psi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitSystem {
    pub length: LengthUnit,
    pub angle: AngleUnit,
    pub mass: MassUnit,
    pub force: ForceUnit,
    pub pressure: PressureUnit,
}

impl LengthUnit {
    /// Factor converting a value in this unit to meters.
    pub fn to_meters(self) -> f64 {
        match self {
            LengthUnit::Meters => 1.0,
            LengthUnit::Millimeters => 1.0e-3,
            LengthUnit::Feet => 0.3048,
            LengthUnit::Inches => 0.0254,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            LengthUnit::Meters => "m",
            LengthUnit::Millimeters => "mm",
            LengthUnit::Feet => "ft",
            LengthUnit::Inches => "in",
        }
    }
}

impl AngleUnit {
    /// Factor converting a value in this unit to radians.
    pub fn to_radians(self) -> f64 {
        match self {
            AngleUnit::Degrees => std::f64::consts::PI / 180.0,
            AngleUnit::Radians => 1.0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            AngleUnit::Degrees => "deg",
            AngleUnit::Radians => "rad",
        }
    }
}

impl UnitSystem {
    pub fn length_to_canonical(&self, value: f64) -> f64 {
        value * self.length.to_meters()
    }

    pub fn angle_to_canonical(&self, value: f64) -> f64 {
        value * self.angle.to_radians()
    }

    pub fn length_from_canonical(&self, value: f64) -> f64 {
        value / self.length.to_meters()
    }

    pub fn angle_from_canonical(&self, value: f64) -> f64 {
        value / self.angle.to_radians()
    }

    pub fn length_unit_name(&self) -> &'static str {
        self.length.name()
    }

    pub fn angle_unit_name(&self) -> &'static str {
        self.angle.name()
    }
}
