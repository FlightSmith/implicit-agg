//! Station sections: profile sampling, trailing-edge closure, chord scaling,
//! twist about the quarter chord, and placement at the station position.
//!
//! Twist rotates the section about its quarter-chord point following the
//! right-hand rule around the local spanwise direction (the source half
//! extends into negative local Y, so positive twist moves the source half's
//! leading edge down). The station position is the leading-edge reference of
//! the twisted section: planform sweep is defined by station positions.

use crate::mesh::lerp3;
use crate::profile::{cosine_samples, polygon_self_intersects, ProfileCurve};
use aircraft_model::{Code, Diagnostic};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrailingEdgeSpec {
    Sharp,
    /// Closure gap in canonical length units.
    Absolute(f64),
    /// Closure gap as a fraction of local chord.
    ChordFraction(f64),
}

/// A resolved station: evaluated values plus its profile curve.
#[derive(Debug, Clone)]
pub struct StationSpec<'a> {
    pub id: String,
    pub curve: &'a ProfileCurve,
    pub chord: f64,
    /// Canonical radians.
    pub twist: f64,
    /// Wing-local leading-edge position, canonical meters.
    pub position: [f64; 3],
    pub trailing_edge: TrailingEdgeSpec,
}

/// A sampled station ring in wing-local space. Vertex layout is
/// landmark-aligned: index 0 is the leading edge, indices `k-1` and `k` are
/// the trailing-edge upper and lower vertices, and the loop closes back at
/// the leading edge. `2k` vertices total.
#[derive(Debug, Clone)]
pub struct Ring {
    pub points: Vec<[f64; 3]>,
}

impl StationSpec<'_> {
    /// Resolved trailing-edge closure gap in chord units.
    pub fn trailing_edge_thickness(&self) -> f64 {
        match self.trailing_edge {
            TrailingEdgeSpec::Sharp => 0.0,
            TrailingEdgeSpec::Absolute(thickness) => thickness / self.chord,
            TrailingEdgeSpec::ChordFraction(fraction) => fraction,
        }
    }

    /// Reject infeasible sections before any meshing: non-positive chords,
    /// absurd or negative trailing-edge gaps, and profiles that would
    /// self-intersect under the requested closure.
    pub fn check_feasible(&self, subject: &str) -> Result<(), Diagnostic> {
        if !(self.chord.is_finite() && self.chord > 0.0) {
            return Err(Diagnostic::error(
                Code::NonPositiveChord,
                format!("station {:?} has a non-positive chord", self.id),
            )
            .with_subject(subject.to_string()));
        }
        let thickness = self.trailing_edge_thickness();
        if !thickness.is_finite() || thickness < 0.0 {
            return Err(Diagnostic::error(
                Code::InvalidTrailingEdge,
                format!(
                    "station {:?} has a negative or non-finite trailing-edge closure",
                    self.id
                ),
            )
            .with_subject(subject.to_string()));
        }
        if thickness >= 0.5 {
            return Err(Diagnostic::error(
                Code::InvalidTrailingEdge,
                format!(
                    "station {:?} requests a trailing-edge thickness of {:.3} chord, which \
                     exceeds what any plausible profile can close",
                    self.id, thickness
                ),
            )
            .with_subject(subject.to_string()));
        }

        // Sample the closed section polygon with the requested closure and
        // reject self-intersection.
        const PROBE_SAMPLES: usize = 48;
        let polygon = self.closed_polygon(PROBE_SAMPLES);
        if polygon_self_intersects(&polygon) {
            let code = if thickness > 0.0 {
                Code::InvalidTrailingEdge
            } else {
                Code::InvalidProfile
            };
            return Err(Diagnostic::error(
                code,
                format!(
                    "station {:?} profile self-intersects with the requested trailing-edge \
                     closure",
                    self.id
                ),
            )
            .with_subject(subject.to_string()));
        }
        Ok(())
    }

    /// The section's closed 2D polygon at chord scale with trailing-edge
    /// closure applied. A zero-width closure contributes a single vertex.
    fn closed_polygon(&self, k: usize) -> Vec<[f64; 2]> {
        let xs = cosine_samples(k);
        let thickness = self.trailing_edge_thickness();
        let te_mean = (self.curve.sample_upper(1.0) + self.curve.sample_lower(1.0)) / 2.0;
        let mut polygon: Vec<[f64; 2]> = Vec::with_capacity(2 * k);
        for (j, &x) in xs.iter().enumerate() {
            let y = if j + 1 == k {
                te_mean + thickness / 2.0
            } else {
                self.curve.sample_upper(x)
            };
            polygon.push([x, y]);
        }
        // Trailing-edge lower vertex, then the lower surface back to the LE.
        if thickness > 0.0 {
            polygon.push([1.0, te_mean - thickness / 2.0]);
        }
        for &x in xs.iter().take(k - 1).skip(1).rev() {
            polygon.push([x, self.curve.sample_lower(x)]);
        }
        polygon
    }

    /// Sample the station into a placed 3D ring with `k` chord samples per
    /// surface (2k vertices).
    pub fn build_ring(&self, k: usize) -> Ring {
        let xs = cosine_samples(k);
        let thickness = self.trailing_edge_thickness();
        let te_mean = (self.curve.sample_upper(1.0) + self.curve.sample_lower(1.0)) / 2.0;

        let mut ring: Vec<[f64; 3]> = Vec::with_capacity(2 * k);
        for (j, &x) in xs.iter().enumerate() {
            let y = if j + 1 == k {
                te_mean + thickness / 2.0
            } else {
                self.curve.sample_upper(x)
            };
            ring.push(self.place([x, y]));
        }
        ring.push(self.place([1.0, te_mean - thickness / 2.0]));
        for &x in xs.iter().take(k - 1).rev() {
            ring.push(self.place([x, self.curve.sample_lower(x)]));
        }
        Ring { points: ring }
    }

    /// Scale a chord-fraction 2D point by the chord, rotate by twist about
    /// the quarter chord, and translate so the (rotated) leading edge sits at
    /// the station position.
    fn place(&self, point: [f64; 2]) -> [f64; 3] {
        let x = point[0] * self.chord;
        let z = point[1] * self.chord;
        let pivot = 0.25 * self.chord;
        let (dx, dz) = (x - pivot, z);
        let (sin, cos) = self.twist.sin_cos();
        let rotated_x = pivot + dx * cos - dz * sin;
        let rotated_z = dx * sin + dz * cos;
        // Where the LE lands after rotation; offset so it lands on position.
        let le_x = pivot + (-pivot) * cos;
        let le_z = (-pivot) * sin;
        [
            self.position[0] + rotated_x - le_x,
            self.position[1],
            self.position[2] + rotated_z - le_z,
        ]
    }
}

/// Vertex-wise linear interpolation between two rings of equal length.
pub fn lerp_rings(a: &Ring, b: &Ring, t: f64) -> Ring {
    Ring {
        points: a
            .points
            .iter()
            .zip(&b.points)
            .map(|(pa, pb)| lerp3(*pa, *pb, t))
            .collect(),
    }
}
