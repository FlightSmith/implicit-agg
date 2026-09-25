//! Mesh quality tiers: interactive preview sampling and export sampling
//! resolved from explicit tolerances (chordal deviation, edge length, normal
//! angle). Quality controls are export options, never aircraft parameters.

use crate::profile::ProfileCurve;

#[derive(Debug, Clone, Copy)]
pub enum MeshQuality {
    /// Debounced live preview; chord samples resolve against a loose
    /// absolute sag budget with a floor and a cost cap.
    Interactive,
    /// High-quality adaptive tessellation driven by explicit tolerances.
    Export(ExportTolerances),
}

#[derive(Debug, Clone, Copy)]
pub struct ExportTolerances {
    /// Maximum chordal deviation of the sampled profile, in meters.
    pub max_chordal_deviation: Option<f64>,
    /// Maximum spanwise edge length, in meters.
    pub max_edge_length: Option<f64>,
    /// Maximum angle between adjacent panel facets, in degrees.
    pub max_normal_angle_deg: Option<f64>,
}

impl Default for ExportTolerances {
    fn default() -> Self {
        ExportTolerances {
            max_chordal_deviation: Some(1.0e-3),
            max_edge_length: Some(0.25),
            max_normal_angle_deg: Some(12.0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedQuality {
    /// Chord samples per surface per ring.
    pub chord_samples: usize,
    /// Span subdivisions per panel (index-aligned with station panels).
    pub span_subdivisions: Vec<usize>,
}

/// Spanwise stations per model semispan for the interactive tier; each
/// panel's subdivision count is its share of that budget.
pub const INTERACTIVE_STATIONS_PER_SPAN: f64 = 20.0;
const INTERACTIVE_MIN_SUBDIVISIONS: usize = 4;
const INTERACTIVE_MAX_SUBDIVISIONS: usize = 24;
// The square-root leading-edge singularity makes chordal sag decay as ~1/k,
// so tight tolerances need genuinely large sample counts.
const CHORD_SAMPLE_LADDER: [usize; 13] =
    [16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024];
const MAX_SPAN_SUBDIVISIONS: usize = 64;

/// Interactive-tier chordal-sag budget in canonical meters. Chord-aware:
/// a large root chord gets proportionally more samples than a small tip.
const INTERACTIVE_CHORDAL_TOLERANCE: f64 = 1.5e-3;
/// Interactive sampling floor: the live preview never drops below this.
const INTERACTIVE_MIN_CHORD_SAMPLES: usize = 48;
/// Interactive sampling cap: keeps debounced rebuilds bounded for very
/// large chords even when the tolerance is not reachable.
const INTERACTIVE_MAX_CHORD_SAMPLES: usize = 384;

/// Whether every curve sampled at `k` cosine stations stays within an
/// absolute (meter) chordal-sag budget. `chordal_deviation` returns a chord
/// fraction, so each curve's own chord scales it into meters.
fn meets_chordal_tolerance(curves: &[(&ProfileCurve, f64)], k: usize, tolerance: f64) -> bool {
    curves
        .iter()
        .all(|(curve, chord)| curve.chordal_deviation(k) * chord <= tolerance)
}

/// Resolve quality into concrete sample counts. `panel_angles_deg` holds the
/// estimated per-panel facet angle (twist change plus LE-line kink).
pub fn resolve_quality(
    curves: &[(&ProfileCurve, f64)],
    panel_span_lengths: &[f64],
    panel_angles_deg: &[f64],
    quality: &MeshQuality,
) -> ResolvedQuality {
    let panels = panel_span_lengths.len();
    match *quality {
        MeshQuality::Interactive => {
            // Dense enough for close-ups: curvature regions (LE nose, tip
            // cap, TE wedge) need spanwise stations, not just chord samples.
            // Chord samples scale with the largest chord so the preview nose
            // stays as smooth, in meters, across chord sizes.
            let ladder_start = CHORD_SAMPLE_LADDER
                .iter()
                .position(|&k| k >= INTERACTIVE_MIN_CHORD_SAMPLES)
                .expect("ladder contains the interactive floor");
            let ladder_end = CHORD_SAMPLE_LADDER
                .iter()
                .position(|&k| k > INTERACTIVE_MAX_CHORD_SAMPLES)
                .unwrap_or(CHORD_SAMPLE_LADDER.len());
            let chord_samples = CHORD_SAMPLE_LADDER[ladder_start..ladder_end]
                .iter()
                .copied()
                .find(|&k| meets_chordal_tolerance(curves, k, INTERACTIVE_CHORDAL_TOLERANCE))
                .unwrap_or(INTERACTIVE_MAX_CHORD_SAMPLES);
            let total_span: f64 = panel_span_lengths.iter().sum();
            ResolvedQuality {
                chord_samples,
                span_subdivisions: panel_span_lengths
                    .iter()
                    .map(|&span| {
                        if span <= 0.0 || total_span <= 0.0 {
                            INTERACTIVE_MIN_SUBDIVISIONS
                        } else {
                            ((INTERACTIVE_STATIONS_PER_SPAN * span / total_span).round() as usize)
                                .clamp(INTERACTIVE_MIN_SUBDIVISIONS, INTERACTIVE_MAX_SUBDIVISIONS)
                        }
                    })
                    .collect(),
            }
        }
        MeshQuality::Export(tolerances) => {
            let chord_samples = tolerances
                .max_chordal_deviation
                .map(|deviation| {
                    CHORD_SAMPLE_LADDER
                        .iter()
                        .copied()
                        .find(|&k| meets_chordal_tolerance(curves, k, deviation))
                        .unwrap_or(*CHORD_SAMPLE_LADDER.last().expect("non-empty ladder"))
                })
                .unwrap_or(96);

            let span_subdivisions = (0..panels)
                .map(|panel| {
                    let mut subdivisions = 1usize;
                    if let Some(max_edge) = tolerances.max_edge_length {
                        if max_edge > 0.0 {
                            let needed = (panel_span_lengths[panel] / max_edge).ceil() as usize;
                            subdivisions = subdivisions.max(needed);
                        }
                    }
                    if let Some(max_angle) = tolerances.max_normal_angle_deg {
                        if max_angle > 0.0 {
                            let needed = (panel_angles_deg[panel] / max_angle).ceil() as usize;
                            subdivisions = subdivisions.max(needed);
                        }
                    }
                    subdivisions.clamp(1, MAX_SPAN_SUBDIVISIONS)
                })
                .collect();

            ResolvedQuality {
                chord_samples,
                span_subdivisions,
            }
        }
    }
}
