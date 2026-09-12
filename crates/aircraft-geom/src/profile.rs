//! Airfoil profile curves: NACA 4-series generation and normalized
//! coordinate profiles with lossless repair.
//!
//! A [`ProfileCurve`] splits a profile into upper and lower surfaces, each a
//! polyline sorted by x from the leading edge (x ≈ 0) to the trailing edge
//! (x ≈ 1). Coordinates are fractions of chord; the y ordinate is the usual
//! profile ordinate (positive up).

use aircraft_model::{Code, Diagnostic};

/// Dense sampling used when a smooth reference curve is needed.
const DENSE_SAMPLES: usize = 400;

#[derive(Debug, Clone)]
pub struct ProfileCurve {
    upper: Vec<[f64; 2]>,
    lower: Vec<[f64; 2]>,
}

impl ProfileCurve {
    /// Generate a NACA 4-digit section, e.g. `2412`.
    pub fn naca4(code: &str) -> Result<Self, Diagnostic> {
        let digits: Option<Vec<u32>> = code.chars().map(|c| c.to_digit(10)).collect();
        let Some(digits) = digits else {
            return Err(Diagnostic::error(
                Code::InvalidProfile,
                format!("NACA code {code:?} must be four decimal digits"),
            ));
        };
        if digits.len() != 4 {
            return Err(Diagnostic::error(
                Code::InvalidProfile,
                format!("NACA code {code:?} must have exactly four digits"),
            ));
        }
        let max_camber = f64::from(digits[0]) / 100.0;
        let camber_position = f64::from(digits[1]) / 10.0;
        let thickness = f64::from(digits[2] * 10 + digits[3]) / 100.0;
        if max_camber > 0.0 && camber_position == 0.0 {
            return Err(Diagnostic::error(
                Code::InvalidProfile,
                format!("NACA code {code:?} declares camber without a camber position"),
            ));
        }

        let mut upper = Vec::with_capacity(DENSE_SAMPLES + 1);
        let mut lower = Vec::with_capacity(DENSE_SAMPLES + 1);
        for i in 0..=DENSE_SAMPLES {
            // Cosine spacing concentrates points at the leading edge.
            let x = (1.0 - (std::f64::consts::PI * i as f64 / DENSE_SAMPLES as f64).cos()) / 2.0;
            let yt = 5.0
                * thickness
                * (0.2969 * x.sqrt() - 0.1260 * x - 0.3516 * x * x + 0.2843 * x * x * x
                    - 0.1015 * x * x * x * x);
            let (yc, dyc) = camber_line(x, max_camber, camber_position);
            let theta = dyc.atan();
            upper.push([x - yt * theta.sin(), yc + yt * theta.cos()]);
            lower.push([x + yt * theta.sin(), yc - yt * theta.cos()]);
        }

        Ok(ProfileCurve {
            upper: enforce_monotonic(upper),
            lower: enforce_monotonic(lower),
        })
    }

    /// Build a profile from normalized coordinate points, applying lossless
    /// repair only: duplicate removal, rotation to lead from the leading
    /// edge, and orientation fix so the upper surface comes first. Anything
    /// else becomes a diagnostic.
    pub fn coordinates(points: &[[f64; 2]]) -> Result<Self, Diagnostic> {
        if points.len() < 3 {
            return Err(Diagnostic::error(
                Code::InvalidProfile,
                "a coordinate profile needs at least three points",
            ));
        }
        for point in points {
            let [x, y] = *point;
            if !x.is_finite() || !y.is_finite() {
                return Err(Diagnostic::error(
                    Code::InvalidProfile,
                    "profile coordinates must be finite",
                ));
            }
            if !(-0.1..=1.1).contains(&x) || !(-0.5..=0.5).contains(&y) {
                return Err(Diagnostic::error(
                    Code::InvalidProfile,
                    format!("profile point ({x}, {y}) is outside the normalized chord box"),
                ));
            }
        }

        let mut loop_points = dedup_consecutive(points.to_vec());
        if loop_points.len() < 3 {
            return Err(Diagnostic::error(
                Code::InvalidProfile,
                "profile has fewer than three unique points",
            ));
        }

        // Rotate the loop to start at the leading edge (minimum x).
        let leading_edge = loop_points
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])))
            .map(|(index, _)| index)
            .unwrap_or(0);
        loop_points.rotate_left(leading_edge);

        // Find the trailing edge (maximum x) and split the loop into the two
        // paths from leading to trailing edge.
        let trailing_edge = loop_points
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])))
            .map(|(index, _)| index)
            .unwrap_or(0);
        if trailing_edge == 0 {
            return Err(Diagnostic::error(
                Code::InvalidProfile,
                "profile leading and trailing edges coincide",
            ));
        }
        // Two paths run from the LE to the TE: forward through the stored
        // order, and backward wrapping around the loop's end.
        let forward: Vec<[f64; 2]> = loop_points[..=trailing_edge].to_vec();
        let mut backward_le_to_te: Vec<[f64; 2]> = loop_points[trailing_edge..].to_vec();
        backward_le_to_te.reverse();
        backward_le_to_te.insert(0, loop_points[0]);

        // The upper path has the larger mean y.
        let upper_is_forward = mean_y(&forward) >= mean_y(&backward_le_to_te);
        let (upper, lower) = if upper_is_forward {
            (forward, backward_le_to_te)
        } else {
            (backward_le_to_te, forward)
        };

        let upper = enforce_monotonic(upper);
        let lower = enforce_monotonic(lower);
        if upper.len() < 2 || lower.len() < 2 {
            return Err(Diagnostic::error(
                Code::InvalidProfile,
                "profile surfaces are too sparse to loft; x must progress monotonically from \
                 leading to trailing edge along each surface",
            ));
        }

        let curve = ProfileCurve { upper, lower };
        if polygon_self_intersects(&curve.closed_loop()) {
            return Err(Diagnostic::error(
                Code::InvalidProfile,
                "profile is self-intersecting",
            ));
        }
        Ok(curve)
    }

    /// Ordinate of the upper surface at chordwise fraction `x`.
    pub fn sample_upper(&self, x: f64) -> f64 {
        sample(&self.upper, x)
    }

    /// Ordinate of the lower surface at chordwise fraction `x`.
    pub fn sample_lower(&self, x: f64) -> f64 {
        sample(&self.lower, x)
    }

    /// Leading-edge x (typically 0).
    pub fn x_le(&self) -> f64 {
        self.upper.first().map(|p| p[0]).unwrap_or(0.0)
    }

    /// Trailing-edge x (typically 1).
    pub fn x_te(&self) -> f64 {
        self.upper.last().map(|p| p[0]).unwrap_or(1.0)
    }

    /// Gap between upper and lower ordinates at the trailing edge.
    pub fn te_gap(&self) -> f64 {
        self.sample_upper(self.x_te()) - self.sample_lower(self.x_te())
    }

    /// The closed profile loop: upper surface TE->LE then lower LE->TE.
    /// The closing duplicate of the first point is dropped.
    pub fn closed_loop(&self) -> Vec<[f64; 2]> {
        let mut points: Vec<[f64; 2]> = self.upper.iter().rev().copied().collect();
        points.extend(self.lower.iter().skip(1));
        if points.len() > 1 {
            let first = points[0];
            let last = *points.last().unwrap_or(&first);
            if first[0] == last[0] && first[1] == last[1] {
                points.pop();
            }
        }
        points
    }

    /// Maximum deviation between a candidate polyline sampled at `k`
    /// cosine-spaced stations and the dense true curve, over both surfaces.
    pub fn chordal_deviation(&self, k: usize) -> f64 {
        deviation_of_surface(&self.upper, k).max(deviation_of_surface(&self.lower, k))
    }
}

fn camber_line(x: f64, max_camber: f64, camber_position: f64) -> (f64, f64) {
    if max_camber == 0.0 {
        return (0.0, 0.0);
    }
    if x < camber_position {
        let yc =
            max_camber / (camber_position * camber_position) * (2.0 * camber_position * x - x * x);
        let dyc = 2.0 * max_camber / (camber_position * camber_position) * (camber_position - x);
        (yc, dyc)
    } else {
        let one_minus_p2 = (1.0 - camber_position) * (1.0 - camber_position);
        let yc = max_camber / one_minus_p2
            * ((1.0 - 2.0 * camber_position) + 2.0 * camber_position * x - x * x);
        let dyc = 2.0 * max_camber / one_minus_p2 * (camber_position - x);
        (yc, dyc)
    }
}

/// Cosine-spaced chordwise sample positions with `k` points from 0 to 1.
pub fn cosine_samples(k: usize) -> Vec<f64> {
    if k < 2 {
        return vec![0.0, 1.0];
    }
    (0..k)
        .map(|j| (1.0 - (std::f64::consts::PI * j as f64 / (k - 1) as f64).cos()) / 2.0)
        .collect()
}

fn mean_y(points: &[[f64; 2]]) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    points.iter().map(|p| p[1]).sum::<f64>() / points.len() as f64
}

fn dedup_consecutive(points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    let mut result: Vec<[f64; 2]> = Vec::with_capacity(points.len());
    for point in points {
        if let Some(last) = result.last() {
            if last[0] == point[0] && last[1] == point[1] {
                continue;
            }
        }
        result.push(point);
    }
    // Wrap-around duplicate (closed loop supplied as open).
    if result.len() > 1 {
        let first = result[0];
        let last = *result.last().unwrap_or(&first);
        if first[0] == last[0] && first[1] == last[1] {
            result.pop();
        }
    }
    result
}

/// Keep points whose x strictly increases, dropping duplicates and small
/// backtracks introduced by generation or repair.
fn enforce_monotonic(points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    let mut result: Vec<[f64; 2]> = Vec::with_capacity(points.len());
    for point in points {
        if let Some(last) = result.last() {
            if point[0] <= last[0] {
                // Prefer the point closer to the surface extreme (larger |y|)
                // so the TE keeps its ordinate when x values tie.
                if (point[0] - last[0]).abs() < 1e-12 && point[1].abs() > last[1].abs() {
                    *result.last_mut().expect("checked non-empty") = point;
                }
                continue;
            }
        }
        result.push(point);
    }
    result
}

fn sample(points: &[[f64; 2]], x: f64) -> f64 {
    let Some(first) = points.first() else {
        return 0.0;
    };
    let Some(last) = points.last() else {
        return 0.0;
    };
    if x <= first[0] {
        return first[1];
    }
    if x >= last[0] {
        return last[1];
    }
    let index = match points.binary_search_by(|p| p[0].total_cmp(&x)) {
        Ok(index) => return points[index][1],
        Err(index) => index,
    };
    let a = &points[index - 1];
    let b = &points[index];
    let t = (x - a[0]) / (b[0] - a[0]);
    a[1] + t * (b[1] - a[1])
}

/// Maximum sag of a chordwise polyline sampled at `k` cosine stations
/// against the dense reference polyline.
fn deviation_of_surface(surface: &[[f64; 2]], k: usize) -> f64 {
    let mut worst = 0.0_f64;
    for chunk in cosine_samples(k).windows(2) {
        let [x0, x1] = [chunk[0], chunk[1]];
        let y0 = sample(surface, x0);
        let y1 = sample(surface, x1);
        let steps = 24;
        for s in 1..steps {
            let x = x0 + (x1 - x0) * s as f64 / steps as f64;
            let true_y = sample(surface, x);
            let t = (x - x0) / (x1 - x0);
            let chord_y = y0 + t * (y1 - y0);
            worst = worst.max((true_y - chord_y).abs());
        }
    }
    worst
}

/// Whether a closed polygon has any pair of properly intersecting edges.
/// Public because section feasibility (trailing-edge closure) reuses it.
pub fn polygon_self_intersects(points: &[[f64; 2]]) -> bool {
    let n = points.len();
    if n < 4 {
        return false;
    }
    for i in 0..n {
        let a0 = points[i];
        let a1 = points[(i + 1) % n];
        for j in (i + 1)..n {
            if j == i || (j + 1) % n == i || j == (i + 1) % n {
                continue;
            }
            let b0 = points[j];
            let b1 = points[(j + 1) % n];
            if segments_intersect(a0, a1, b0, b1) {
                return true;
            }
        }
    }
    false
}

fn segments_intersect(a0: [f64; 2], a1: [f64; 2], b0: [f64; 2], b1: [f64; 2]) -> bool {
    fn orient(p: [f64; 2], q: [f64; 2], r: [f64; 2]) -> f64 {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    }
    fn on_segment(p: [f64; 2], q: [f64; 2], r: [f64; 2]) -> bool {
        // Does q lie within the bounding box of p and r? (Given collinearity
        // this decides whether q is on the segment pr.)
        p[0].min(r[0]) <= q[0]
            && q[0] <= p[0].max(r[0])
            && p[1].min(r[1]) <= q[1]
            && q[1] <= p[1].max(r[1])
    }
    let d1 = orient(b0, b1, a0);
    let d2 = orient(b0, b1, a1);
    let d3 = orient(a0, a1, b0);
    let d4 = orient(a0, a1, b1);
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    const EPS: f64 = 1e-12;
    if d1.abs() < EPS && on_segment(b0, a0, b1) {
        return true;
    }
    if d2.abs() < EPS && on_segment(b0, a1, b1) {
        return true;
    }
    if d3.abs() < EPS && on_segment(a0, b0, a1) {
        return true;
    }
    if d4.abs() < EPS && on_segment(a0, b1, a1) {
        return true;
    }
    false
}
