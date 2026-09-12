//! Derived planform and volume metrics. These are computed from evaluated
//! geometry only and always declare whether a number describes the source
//! half or the mirrored full wing.

use crate::mesh::Mesh;
use crate::wing::EvaluatedStation;
use aircraft_model::{Code, Diagnostic};

/// A quantity reported for both the source half and the mirrored full wing.
#[derive(Debug, Clone, Copy)]
pub struct BasisPair {
    pub half: f64,
    pub full: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct PanelMetrics {
    /// Leading-edge sweep, canonical radians.
    pub leading_edge_sweep: f64,
    /// Quarter-chord sweep, canonical radians.
    pub quarter_chord_sweep: f64,
    /// Trailing-edge sweep, canonical radians.
    pub trailing_edge_sweep: f64,
    /// Dihedral from station placements, canonical radians.
    pub dihedral: f64,
}

#[derive(Debug, Clone)]
pub struct PlanformMetrics {
    /// Projected reference area on the XZ plane.
    pub reference_area: BasisPair,
    pub span: BasisPair,
    /// Mean aerodynamic chord.
    pub mac: f64,
    /// Leading-edge point of the MAC, wing-local coordinates.
    pub mac_le: [f64; 3],
    /// Aspect ratio from full span and full reference area.
    pub aspect_ratio: f64,
    /// Root-to-tip taper ratio where the root chord is positive.
    pub taper_ratio: Option<f64>,
    pub panels: Vec<PanelMetrics>,
}

#[derive(Debug, Clone, Copy)]
pub struct VolumeMetrics {
    pub volume: BasisPair,
    pub wetted_area: BasisPair,
    /// Center of volume of the source half, mesh coordinates.
    pub center_of_volume: [f64; 3],
}

fn sweep(x_lo: f64, x_hi: f64, y_lo: f64, y_hi: f64) -> Result<f64, Diagnostic> {
    let span = (y_hi - y_lo).abs();
    if span <= f64::EPSILON {
        return Err(Diagnostic::error(
            Code::MeshFailure,
            "panel has degenerate span; sweep is undefined",
        ));
    }
    Ok(((x_hi - x_lo) / span).atan())
}

/// Planform metrics from evaluated station placements (wing-local
/// coordinates, canonical units).
pub fn planform_metrics(stations: &[EvaluatedStation]) -> Result<PlanformMetrics, Diagnostic> {
    if stations.len() < 2 {
        return Err(Diagnostic::error(
            Code::MeshFailure,
            "a wing needs at least two stations for metrics",
        ));
    }

    // Projected area and per-panel metrics via trapezoids.
    let mut half_area = 0.0;
    let mut panels = Vec::with_capacity(stations.len() - 1);
    for pair in stations.windows(2) {
        let (lo, hi) = (&pair[0], &pair[1]);
        let dy = (hi.position[1] - lo.position[1]).abs();
        half_area += (lo.chord + hi.chord) / 2.0 * dy;

        let quarter_lo = lo.position[0] + 0.25 * lo.chord;
        let quarter_hi = hi.position[0] + 0.25 * hi.chord;
        let te_lo = lo.position[0] + lo.chord;
        let te_hi = hi.position[0] + hi.chord;
        panels.push(PanelMetrics {
            leading_edge_sweep: sweep(
                lo.position[0],
                hi.position[0],
                lo.position[1],
                hi.position[1],
            )?,
            quarter_chord_sweep: sweep(quarter_lo, quarter_hi, lo.position[1], hi.position[1])?,
            trailing_edge_sweep: sweep(te_lo, te_hi, lo.position[1], hi.position[1])?,
            dihedral: sweep(
                lo.position[2],
                hi.position[2],
                lo.position[1],
                hi.position[1],
            )?,
        });
    }

    // MAC and its leading-edge location, weighted by panel area.
    let mut mac = 0.0;
    let mut mac_le = [0.0; 3];
    let mut weight = 0.0;
    for (_panel, pair) in panels.iter().zip(stations.windows(2)) {
        let (lo, hi) = (&pair[0], &pair[1]);
        let dy = (hi.position[1] - lo.position[1]).abs();
        let panel_area = (lo.chord + hi.chord) / 2.0 * dy;
        if panel_area <= 0.0 {
            continue;
        }
        let taper = if lo.chord > 0.0 {
            hi.chord / lo.chord
        } else {
            0.0
        };
        let panel_mac = if taper > 0.0 {
            2.0 / 3.0 * lo.chord * (1.0 + taper + taper * taper) / (1.0 + taper)
        } else {
            2.0 / 3.0 * hi.chord
        };
        // Centroid of the chord-length weighting along the span.
        let fraction = if (1.0 + taper) > 0.0 {
            (1.0 + 2.0 * taper) / (3.0 * (1.0 + taper))
        } else {
            0.5
        };
        let y = lo.position[1] + (hi.position[1] - lo.position[1]) * fraction;
        let x = lo.position[0] + (hi.position[0] - lo.position[0]) * fraction;
        let z = lo.position[2] + (hi.position[2] - lo.position[2]) * fraction;

        mac += panel_area * panel_mac;
        mac_le[0] += panel_area * x;
        mac_le[1] += panel_area * y;
        mac_le[2] += panel_area * z;
        weight += panel_area;
    }
    if weight <= 0.0 {
        return Err(Diagnostic::error(
            Code::MeshFailure,
            "wing has zero projected area; MAC is undefined",
        ));
    }
    mac /= weight;
    for value in &mut mac_le {
        *value /= weight;
    }

    let half_span = (stations[stations.len() - 1].position[1] - stations[0].position[1]).abs();
    let full_area = 2.0 * half_area;
    let full_span = 2.0 * half_span;
    let taper_ratio = if stations[0].chord > 0.0 {
        Some(stations[stations.len() - 1].chord / stations[0].chord)
    } else {
        None
    };

    Ok(PlanformMetrics {
        reference_area: BasisPair {
            half: half_area,
            full: full_area,
        },
        span: BasisPair {
            half: half_span,
            full: full_span,
        },
        mac,
        mac_le,
        aspect_ratio: if full_area > 0.0 {
            full_span * full_span / full_area
        } else {
            0.0
        },
        taper_ratio,
        panels,
    })
}

/// Volume, wetted area, and center of volume from a closed half-model mesh.
/// The full-wing values are twice the half values for a symmetric wing.
pub fn volume_metrics(half_mesh: &Mesh) -> VolumeMetrics {
    let (volume, center) = half_mesh.volume_and_center();
    let wetted = half_mesh.surface_area();
    VolumeMetrics {
        volume: BasisPair {
            half: volume,
            full: 2.0 * volume,
        },
        wetted_area: BasisPair {
            half: wetted,
            full: 2.0 * wetted,
        },
        center_of_volume: center,
    }
}
