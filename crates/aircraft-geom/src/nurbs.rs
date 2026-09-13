#![allow(clippy::needless_range_loop)]
//! Minimal NURBS: cubic global curve interpolation and surface skinning —
//! exactly enough to emit the wing's analytic surfaces into STEP.
//!
//! Conventions follow The NURBS Book: clamped knot vectors, control points
//! in homogeneous form (w = 1 everywhere here, so non-rational).

use aircraft_model::Diagnostic;

#[derive(Debug, Clone)]
pub struct NurbsCurve {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub controls: Vec<[f64; 4]>,
}

#[derive(Debug, Clone)]
pub struct NurbsSurface {
    pub u_degree: usize,
    pub v_degree: usize,
    pub u_knots: Vec<f64>,
    pub v_knots: Vec<f64>,
    /// Control net indexed [u][v].
    pub controls: Vec<Vec<[f64; 4]>>,
}

/// Chord-length parameters for a point sequence, normalized to [0, 1].
pub fn chord_params(points: &[[f64; 3]]) -> Vec<f64> {
    let n = points.len();
    let mut params = vec![0.0; n];
    let mut total = 0.0;
    for i in 1..n {
        total += dist(points[i - 1], points[i]);
        params[i] = total;
    }
    if total > 0.0 {
        for value in &mut params {
            *value /= total;
        }
    }
    params
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// Averaging knot vector (The NURBS Book eq. 9.8) for `params` and degree.
fn averaging_knots(params: &[f64], degree: usize) -> Vec<f64> {
    let n = params.len() - 1; // last point index
    let mut knots = vec![0.0; n + degree + 2];
    let last = knots.len();
    for i in degree + 1..last {
        let j = i - degree - 1; // 0-based interior index
        let mut sum = 0.0;
        for k in j..j + degree {
            sum += params[k.min(n)];
        }
        knots[i] = sum / degree as f64;
    }
    knots[last - degree - 1..last].fill(1.0);
    knots
}

/// Cox-de Boor basis values for the `degree + 1` nonzero functions whose
/// support covers `u` (span found by binary search).
fn basis_functions(span: usize, degree: usize, u: f64, knots: &[f64]) -> Vec<f64> {
    let mut left = vec![0.0; degree + 1];
    let mut right = vec![0.0; degree + 1];
    let mut values = vec![0.0; degree + 1];
    values[0] = 1.0;
    for level in 1..=degree {
        left[level] = u - knots[span + 1 - level];
        right[level] = knots[span + level] - u;
        let mut saved = 0.0;
        for r in 0..level {
            let temp = values[r] / (right[r + 1] + left[level - r]);
            values[r] = saved + right[r + 1] * temp;
            saved = left[level - r] * temp;
        }
        values[level] = saved;
    }
    values
}

fn find_span(degree: usize, knots: &[f64], u: f64, control_count: usize) -> usize {
    // Clamped: u == 1 belongs to the last span.
    if u >= knots[control_count] {
        return control_count - 1;
    }
    let mut low = degree;
    let mut high = control_count;
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

/// Solve the dense linear system A x = b (partial pivoting).
fn solve(matrix: &mut [Vec<f64>], rhs: &mut [[f64; 4]]) -> Vec<[f64; 4]> {
    let n = rhs.len();
    for column in 0..n {
        let mut pivot = column;
        for row in column + 1..n {
            if matrix[row][column].abs() > matrix[pivot][column].abs() {
                pivot = row;
            }
        }
        matrix.swap(column, pivot);
        rhs.swap(column, pivot);
        for row in column + 1..n {
            let factor = matrix[row][column] / matrix[column][column];
            if factor != 0.0 {
                for c in column..n {
                    matrix[row][c] -= factor * matrix[column][c];
                }
                for axis in 0..4 {
                    rhs[row][axis] -= factor * rhs[column][axis];
                }
            }
        }
    }
    let mut out = vec![[0.0; 4]; n];
    for row in (0..n).rev() {
        let mut acc = rhs[row];
        for c in row + 1..n {
            for axis in 0..4 {
                acc[axis] -= matrix[row][c] * out[c][axis];
            }
        }
        let diagonal = matrix[row][row];
        for axis in 0..4 {
            out[row][axis] = acc[axis] / diagonal;
        }
    }
    out
}

/// Global cubic interpolation through `points` with the given parameters.
/// Returns the curve and its knot vector (shared by identically
/// parameterized curves, which is what surface skinning requires).
pub fn interpolate_cubic(
    points: &[[f64; 3]],
    params: &[f64],
    degree: usize,
) -> Result<(NurbsCurve, Vec<f64>), Diagnostic> {
    let count = points.len();
    if count < 2 {
        return Err(Diagnostic::error(
            aircraft_model::Code::InvalidProfile,
            "a NURBS curve needs at least two points",
        ));
    }
    let degree = degree.min(count - 1);
    let knots = averaging_knots(params, degree);

    // Basis matrix: row = data point, columns = control points.
    let mut matrix = vec![vec![0.0; count]; count];
    let mut rhs: Vec<[f64; 4]> = points.iter().map(|p| [p[0], p[1], p[2], 1.0]).collect();
    for (row, &u) in params.iter().enumerate() {
        let span = find_span(degree, &knots, u, count);
        let basis = basis_functions(span, degree, u, &knots);
        for (offset, &value) in basis.iter().enumerate() {
            let column = span - degree + offset;
            matrix[row][column] = value;
        }
    }
    let controls = solve(&mut matrix, &mut rhs);
    Ok((
        NurbsCurve {
            degree,
            knots: knots.clone(),
            controls,
        },
        knots,
    ))
}

/// Skin section curves (all sharing degree, knots, and control count) into a
/// surface. `v_params` positions the sections spanwise (e.g. chord length of
/// the leading-edge points).
pub fn skin(curves: &[NurbsCurve], v_params: &[f64]) -> Result<NurbsSurface, Diagnostic> {
    if curves.len() < 2 {
        return Err(Diagnostic::error(
            aircraft_model::Code::MeshFailure,
            "skinning needs at least two section curves",
        ));
    }
    let u_degree = curves[0].degree;
    let u_knots = curves[0].knots.clone();
    let u_count = curves[0].controls.len();
    let v_degree = (curves.len() - 1).min(3);
    let v_knots = averaging_knots(v_params, v_degree);

    // Interpolate each u-column of control points across v.
    let mut controls: Vec<Vec<[f64; 4]>> = Vec::with_capacity(u_count);
    for j in 0..u_count {
        let column: Vec<[f64; 3]> = curves
            .iter()
            .map(|curve| {
                let c = curve.controls[j];
                [c[0], c[1], c[2]]
            })
            .collect();
        let mut matrix = column_matrix(&column, v_degree, &v_knots, v_params);
        let mut rhs: Vec<[f64; 4]> = column.iter().map(|p| [p[0], p[1], p[2], 1.0]).collect();
        let solved = solve(&mut matrix, &mut rhs);
        controls.push(solved);
    }
    Ok(NurbsSurface {
        u_degree,
        v_degree,
        u_knots,
        v_knots,
        controls,
    })
}

/// Basis matrix for interpolating `points` at `params` with preset knots.
fn column_matrix(
    points: &[[f64; 3]],
    degree: usize,
    knots: &[f64],
    params: &[f64],
) -> Vec<Vec<f64>> {
    let count = points.len();
    let mut matrix = vec![vec![0.0; count]; count];
    for (row, &u) in params.iter().enumerate() {
        let span = find_span(degree, knots, u, count);
        let basis = basis_functions(span, degree, u, knots);
        for (offset, &value) in basis.iter().enumerate() {
            matrix[row][span - degree + offset] = value;
        }
    }
    matrix
}

/// Evaluate a curve with de Boor's algorithm.
pub fn evaluate_curve(curve: &NurbsCurve, u: f64) -> [f64; 3] {
    let degree = curve.degree;
    let n = curve.controls.len();
    let span = find_span(degree, &curve.knots, u, n);
    let mut work: Vec<[f64; 4]> = (span - degree..=span).map(|i| curve.controls[i]).collect();
    for level in 1..=degree {
        for j in (level..=degree).rev() {
            let i = span - degree + j;
            let denominator = curve.knots[i + degree + 1 - level] - curve.knots[i];
            let alpha = if denominator > 0.0 {
                (u - curve.knots[i]) / denominator
            } else {
                0.0
            };
            let previous = work[j - 1];
            for (axis, value) in previous.iter().enumerate() {
                work[j][axis] = (1.0 - alpha) * value + alpha * work[j][axis];
            }
        }
    }
    let homogeneous = work[degree];
    [
        homogeneous[0] / homogeneous[3],
        homogeneous[1] / homogeneous[3],
        homogeneous[2] / homogeneous[3],
    ]
}

/// Evaluate the surface at (u, v): de Boor in v on each u column, then u.
pub fn evaluate_surface(surface: &NurbsSurface, u: f64, v: f64) -> [f64; 3] {
    let u_count = surface.controls.len();
    let mut row_points: Vec<[f64; 4]> = Vec::with_capacity(u_count);
    for column in &surface.controls {
        let curve = NurbsCurve {
            degree: surface.v_degree,
            knots: surface.v_knots.clone(),
            controls: column.clone(),
        };
        let point = evaluate_curve(&curve, v);
        row_points.push([point[0], point[1], point[2], 1.0]);
    }
    let u_curve = NurbsCurve {
        degree: surface.u_degree,
        knots: surface.u_knots.clone(),
        controls: row_points,
    };
    evaluate_curve(&u_curve, u)
}
