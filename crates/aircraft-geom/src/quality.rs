//! Mesh quality tiers: interactive preview sampling and export sampling
//! resolved from explicit tolerances (chordal deviation, edge length, normal
//! angle). Quality controls are export options, never aircraft parameters.

use crate::profile::ProfileCurve;

#[derive(Debug, Clone, Copy)]
pub enum MeshQuality {
    /// Low tessellation for debounced live preview.
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

const INTERACTIVE_CHORD_SAMPLES: usize = 24;
const INTERACTIVE_SPAN_SUBDIVISIONS: usize = 2;
// The square-root leading-edge singularity makes chordal sag decay as ~1/k,
// so tight tolerances need genuinely large sample counts.
const CHORD_SAMPLE_LADDER: [usize; 13] =
    [16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024];
const MAX_SPAN_SUBDIVISIONS: usize = 64;

/// Resolve quality into concrete sample counts. `panel_angles_deg` holds the
/// estimated per-panel facet angle (twist change plus LE-line kink).
pub fn resolve_quality(
    curves: &[&ProfileCurve],
    panel_span_lengths: &[f64],
    panel_angles_deg: &[f64],
    quality: &MeshQuality,
) -> ResolvedQuality {
    let panels = panel_span_lengths.len();
    match *quality {
        MeshQuality::Interactive => ResolvedQuality {
            chord_samples: INTERACTIVE_CHORD_SAMPLES,
            span_subdivisions: vec![INTERACTIVE_SPAN_SUBDIVISIONS; panels],
        },
        MeshQuality::Export(tolerances) => {
            let chord_samples = tolerances
                .max_chordal_deviation
                .map(|deviation| {
                    CHORD_SAMPLE_LADDER
                        .iter()
                        .copied()
                        .find(|&k| {
                            curves
                                .iter()
                                .all(|curve| curve.chordal_deviation(k) <= deviation)
                        })
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
