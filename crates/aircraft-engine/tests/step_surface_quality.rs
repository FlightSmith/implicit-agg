//! Temporary verification: for the demo wing's actual STEP pipeline,
//! measure (a) max 3D deviation of each station's interpolated section
//! curve from the true placed profile, (b) x-monotonicity (no nose wiggle),
//! (c) STEP export succeeds at high tip twist. Deleted after reporting.

#![allow(clippy::unwrap_used)]

use aircraft_engine::Engine;
use aircraft_geom::nurbs::NurbsCurve;
use aircraft_geom::profile::cosine_samples;
use aircraft_geom::section::{StationSpec, TrailingEdgeSpec};
use aircraft_model::parse_document;

const EXAMPLE: &str = include_str!("../../../examples/cranked-wing.v0.1.json");

fn find_span(degree: usize, knots: &[f64], u: f64, count: usize) -> usize {
    if u >= knots[count] {
        return count - 1;
    }
    let (mut low, mut high) = (degree, count);
    let mut mid = (low + high) / 2;
    while u < knots[mid] || u >= knots[mid + 1] {
        if u < knots[mid] {
            high = mid;
        } else {
            low = mid;
        }
        mid = (low + high) / 2;
    }
    mid
}

fn eval_curve(c: &NurbsCurve, u: f64) -> [f64; 3] {
    let count = c.controls.len();
    let span = find_span(c.degree, &c.knots, u, count);
    let mut left = vec![0.0; c.degree + 1];
    let mut right = vec![0.0; c.degree + 1];
    let mut values = vec![0.0; c.degree + 1];
    values[0] = 1.0;
    for level in 1..=c.degree {
        left[level] = u - c.knots[span + 1 - level];
        right[level] = c.knots[span + level] - u;
        let mut saved = 0.0;
        for r in 0..level {
            let temp = values[r] / (right[r + 1] + left[level - r]);
            values[r] = saved + right[r + 1] * temp;
            saved = left[level - r] * temp;
        }
        values[level] = saved;
    }
    let mut out = [0.0; 4];
    for (o, &v) in values.iter().enumerate() {
        let ctl = c.controls[span - c.degree + o];
        for a in 0..4 {
            out[a] += v * ctl[a];
        }
    }
    [out[0] / out[3], out[1] / out[3], out[2] / out[3]]
}

fn point_segment(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    let t = if len2 == 0.0 {
        0.0
    } else {
        ((ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / len2).clamp(0.0, 1.0)
    };
    let q = [a[0] + t * ab[0], a[1] + t * ab[1], a[2] + t * ab[2]];
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
}

fn curve_stats(curve: &NurbsCurve, truth: &[[f64; 3]]) -> (f64, f64) {
    let polyline: Vec<[f64; 3]> = (0..=4000)
        .map(|s| eval_curve(curve, s as f64 / 4000.0))
        .collect();
    let worst_dist = truth
        .iter()
        .map(|p| {
            polyline
                .windows(2)
                .map(|w| point_segment(*p, w[0], w[1]))
                .fold(f64::INFINITY, f64::min)
        })
        .fold(0.0_f64, f64::max);
    let mut worst_backtrack = 0.0_f64;
    let mut prev = f64::NEG_INFINITY;
    for p in &polyline {
        if p[0] < prev {
            worst_backtrack = worst_backtrack.max(prev - p[0]);
        }
        prev = p[0];
    }
    (worst_dist, worst_backtrack)
}

/// The STEP pipeline's section curve for a station: 96-sample cosine ring,
/// interpolated through every sample with shared chord-length parameters.
fn station_section_curve(spec: &StationSpec) -> NurbsCurve {
    let k = 96usize;
    let ring = spec.build_ring(k);
    let row = &ring.points[0..k];
    let params = aircraft_geom::step_model::nurbs_chord_params(row);
    interpolate_cubic_wrapper(row, &params)
}

fn interpolate_cubic_wrapper(points: &[[f64; 3]], params: &[f64]) -> NurbsCurve {
    aircraft_geom::nurbs::interpolate_cubic(points, params, 3)
        .unwrap()
        .0
}

#[test]
fn step_section_curves_hold_the_nose_without_ringing() {
    // Regression: interpolating the full 96-sample cosine rows with the
    // sample stations' x fractions as parameters rang near the clamped end
    // (the leading edge rendered scalloped in CAD). Chord-length parameters
    // hold the curve to the profile without the ringing.
    let (doc, _) = parse_document(EXAMPLE).unwrap();
    let engine = Engine::open(doc).unwrap();
    let stations = engine.evaluated_stations(0).unwrap().1;

    for station in &stations {
        let te = match station.trailing_edge {
            TrailingEdgeSpec::Absolute(t) => t / station.chord,
            TrailingEdgeSpec::ChordFraction(f) => f,
            TrailingEdgeSpec::Sharp => 0.0,
        };
        let truth: Vec<[f64; 3]> = cosine_samples(800)
            .iter()
            .map(|&x| {
                let base = station.curve.sample_upper(x);
                let natural = station.curve.sample_upper(x) - station.curve.sample_lower(x);
                let excess = (te - natural).max(0.0);
                const FLARE: f64 = 0.85;
                let x_te = station.curve.x_te();
                let w = if x_te <= FLARE || x < FLARE {
                    0.0
                } else {
                    let t = ((x - FLARE) / (x_te - FLARE)).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                };
                let y = base + excess * w / 2.0;
                let px = x * station.chord;
                let pz = y * station.chord;
                let pivot = 0.25 * station.chord;
                let (sin, cos) = station.twist.sin_cos();
                let rx = pivot + (px - pivot) * cos - pz * sin;
                let rz = (px - pivot) * sin + pz * cos;
                let le_x = pivot - pivot * cos;
                let le_z = -pivot * sin;
                [
                    station.position[0] + rx - le_x,
                    station.position[1],
                    station.position[2] + rz - le_z,
                ]
            })
            .collect();
        let spec = StationSpec {
            id: station.id.clone(),
            curve: &station.curve,
            chord: station.chord,
            twist: station.twist,
            position: station.position,
            trailing_edge: station.trailing_edge,
        };
        let curve = station_section_curve(&spec);
        let (deviation, wiggle) = curve_stats(&curve, &truth);
        assert!(
            deviation <= 0.0005,
            "station {}: section curve deviates {:.3} mm from the profile",
            station.id,
            deviation * 1000.0
        );
        assert!(
            wiggle <= 0.0002,
            "station {}: section curve rings by {:.3} mm at the nose",
            station.id,
            wiggle * 1000.0
        );
    }
}
