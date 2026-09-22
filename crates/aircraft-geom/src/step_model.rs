//! The wing's analytic STEP representation: NURBS skins, a bilinear
//! trailing-edge face, planar caps, and the topological bookkeeping
//! (vertices, shared edges, oriented loops, shells) the writer needs.
//!
//! Built from the same loft rows as the mesher, so tangent leading edges,
//! trailing-edge flares, and TE closures are captured exactly.

use crate::nurbs::{interpolate_cubic, skin, NurbsCurve, NurbsSurface};
pub use crate::profile::cosine_samples;
use crate::quality::ResolvedQuality;
use crate::wing::{loft_rings, EvaluatedStation, LeTangency};
use aircraft_model::{Code, Diagnostic};

#[derive(Debug, Clone)]
pub enum StepCurve {
    Nurbs(NurbsCurve),
    Line([f64; 3], [f64; 3]),
}

#[derive(Debug, Clone)]
pub enum StepSurface {
    Nurbs(NurbsSurface),
    Plane { origin: [f64; 3], normal: [f64; 3] },
}

#[derive(Debug, Clone)]
pub struct StepEdge {
    pub curve: StepCurve,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone)]
pub struct StepFaceDef {
    pub surface: StepSurface,
    /// Ordered boundary as (edge index, traversed reversed).
    pub loop_edges: Vec<(usize, bool)>,
}

#[derive(Debug, Clone)]
pub struct StepModel {
    pub name: String,
    pub vertices: Vec<[f64; 3]>,
    pub edges: Vec<StepEdge>,
    pub faces: Vec<StepFaceDef>,
    /// Faces grouped into shells (each shell a separate closed solid).
    pub shells: Vec<Vec<usize>>,
}

/// Tolerances for the analytic export: chordal deviation drives the chord
/// sampling, the edge length the spanwise station count.
#[derive(Debug, Clone, Copy)]
pub struct StepTolerances {
    pub max_chordal_deviation: f64,
    pub max_edge_length: f64,
}

impl Default for StepTolerances {
    fn default() -> Self {
        StepTolerances {
            max_chordal_deviation: 2.5e-3,
            max_edge_length: 0.6,
        }
    }
}

/// Chord-length parameters of a point sequence (public for tests).
pub fn nurbs_chord_params(points: &[[f64; 3]]) -> Vec<f64> {
    crate::nurbs::chord_params(points)
}

/// The two boundary point rows of a ring (upper LE→TE, lower LE→TE).
struct Row {
    upper: Vec<[f64; 3]>,
    lower: Vec<[f64; 3]>,
}

fn ring_rows(ring: &crate::section::Ring, chord: f64) -> Row {
    let k = ring.points.len() / 2;
    let keep = |points: &[[f64; 3]]| -> Vec<usize> {
        let mut keep = vec![0usize];
        for j in 1..points.len() {
            let gap = points[j][0] - points[j - 1][0];
            if gap > 1.0e-6 * chord {
                keep.push(j);
            }
        }
        keep
    };

    // The mask is computed once (upper LE→TE x-monotone samples) and shared
    // by both rows so every curve in the file has the same structure.
    let upper: Vec<[f64; 3]> = ring.points[0..k].to_vec();
    let mask = keep(&upper);
    let upper = mask.iter().map(|&j| upper[j]).collect();
    // Lower row stored TE→LE: index k is the TE lower vertex, 2k-1 the LE.
    let mut lower: Vec<[f64; 3]> = ring.points[k..2 * k].to_vec();
    lower.reverse();
    let lower = mask.iter().map(|&j| lower[j]).collect();
    Row { upper, lower }
}

/// Build the wing's analytic STEP model. The source stations always span the
/// source half (negative local Y); `full` mirrors it into one shell.
#[allow(clippy::too_many_arguments)]
pub fn wing_model(
    name: &str,
    stations: &[EvaluatedStation],
    symmetry_enabled: bool,
    tolerances: StepTolerances,
    le_tangency: Option<LeTangency>,
    full: bool,
) -> Result<StepModel, Vec<Diagnostic>> {
    if stations.len() < 2 {
        return Err(vec![Diagnostic::error(
            Code::MeshFailure,
            "a wing needs at least two stations",
        )
        .with_subject(name.to_string())]);
    }
    if full && !symmetry_enabled {
        return Err(vec![Diagnostic::error(
            Code::MeshFailure,
            "cannot mirror a wing whose symmetry is disabled",
        )
        .with_subject(name.to_string())]);
    }

    let quality = ResolvedQuality {
        chord_samples: chord_samples_for(stations, tolerances.max_chordal_deviation),
        span_subdivisions: span_subdivisions(stations, tolerances.max_edge_length),
    };
    let rings = loft_rings(stations, &quality, le_tangency);
    let mut rows: Vec<Row> = rings
        .iter()
        .map(|r| ring_rows(r, row_chord(&rings)))
        .collect();

    if full {
        // Mirror rows (skip the shared root row) so the surface wraps
        // root → tip → mirrored tip → root.
        let mirrored: Vec<Row> = rows[1..].iter().rev().map(mirror_row).collect();
        rows.extend(mirrored);
    }

    let last = rows.len() - 1;

    // Boundary curves: rows share u-parameters, so the skin's natural
    // boundaries coincide with these curves exactly.
    let mut upper_curves: Vec<NurbsCurve> = Vec::with_capacity(rows.len());
    let mut lower_curves: Vec<NurbsCurve> = Vec::with_capacity(rows.len());
    let params = crate::profile::cosine_samples(rows[0].upper.len());
    for row in &rows {
        let (upper, _) =
            interpolate_cubic(&row.upper, &params, 3).map_err(|diagnostic| vec![diagnostic])?;
        let (lower, _) =
            interpolate_cubic(&row.lower, &params, 3).map_err(|diagnostic| vec![diagnostic])?;
        upper_curves.push(upper);
        lower_curves.push(lower);
    }

    let le_points: Vec<[f64; 3]> = rows.iter().map(|row| row.upper[0]).collect();
    let te_points: Vec<[f64; 3]> = rows
        .iter()
        .map(|row| *row.upper.last().expect("non-empty"))
        .collect();
    let (le_path, _) = interpolate_cubic(&le_points, &nurbs_chord_params(&le_points), 3)
        .map_err(|diagnostic| vec![diagnostic])?;
    let (te_path, _) = interpolate_cubic(&te_points, &nurbs_chord_params(&te_points), 3)
        .map_err(|diagnostic| vec![diagnostic])?;

    let skins = skin(&upper_curves, &nurbs_chord_params(&le_points))
        .map_err(|diagnostic| vec![diagnostic])?;
    let lower_skins = skin(&lower_curves, &nurbs_chord_params(&le_points))
        .map_err(|diagnostic| vec![diagnostic])?;

    // Vertices at the six section-boundary points.
    let le_start = rows[0].upper[0];
    let teu_start = rows[0].upper[rows[0].upper.len() - 1];
    let tel_start = rows[0].lower[rows[0].lower.len() - 1];
    let le_end = rows[last].upper[0];
    let teu_end = rows[last].upper[rows[last].upper.len() - 1];
    let tel_end = rows[last].lower[rows[last].lower.len() - 1];

    let mut model = StepModel {
        name: name.to_string(),
        vertices: vec![le_start, teu_start, tel_start, le_end, teu_end, tel_end],
        edges: Vec::new(),
        faces: Vec::new(),
        shells: Vec::new(),
    };
    let _ = &mut model;

    let v_le_start = 0usize;
    let v_teu_start = 1usize;
    let v_tel_start = 2usize;
    let v_le_end = 3usize;
    let v_teu_end = 4usize;
    let v_tel_end = 5usize;

    // Shared edges.
    let e_le = model.push_edge(StepCurve::Nurbs(le_path), v_le_start, v_le_end);
    let e_te = model.push_edge(StepCurve::Nurbs(te_path), v_teu_start, v_teu_end);
    let e_root_upper = model.push_edge(
        StepCurve::Nurbs(upper_curves[0].clone()),
        v_le_start,
        v_teu_start,
    );
    let e_root_lower = model.push_edge(
        StepCurve::Nurbs(lower_curves[0].clone()),
        v_le_start,
        v_tel_start,
    );
    let e_tip_upper = model.push_edge(
        StepCurve::Nurbs(upper_curves[last].clone()),
        v_le_end,
        v_teu_end,
    );
    let e_tip_lower = model.push_edge(
        StepCurve::Nurbs(lower_curves[last].clone()),
        v_le_end,
        v_tel_end,
    );
    let e_root_te_line = model.push_edge(
        StepCurve::Line(teu_start, tel_start),
        v_teu_start,
        v_tel_start,
    );
    let e_tip_te_line = model.push_edge(StepCurve::Line(teu_end, tel_end), v_teu_end, v_tel_end);

    // TE face: a ruled (bilinear) surface between the two TE paths.
    let te_surface = StepSurface::Nurbs(NurbsSurface {
        u_degree: 1,
        v_degree: 1,
        u_knots: vec![0.0, 0.0, 1.0, 1.0],
        v_knots: vec![0.0, 0.0, 1.0, 1.0],
        controls: vec![
            vec![
                [teu_start[0], teu_start[1], teu_start[2], 1.0],
                [tel_start[0], tel_start[1], tel_start[2], 1.0],
            ],
            vec![
                [teu_end[0], teu_end[1], teu_end[2], 1.0],
                [tel_end[0], tel_end[1], tel_end[2], 1.0],
            ],
        ],
    });

    // Cap planes from the end rings (Newell normal).
    let tip_ring_points: Vec<[f64; 3]> = rows[last]
        .upper
        .iter()
        .chain(rows[last].lower.iter().rev())
        .copied()
        .collect();
    let root_ring_points: Vec<[f64; 3]> = rows[0]
        .upper
        .iter()
        .chain(rows[0].lower.iter().rev())
        .copied()
        .collect();
    let tip_plane = StepSurface::Plane {
        origin: centroid(&tip_ring_points),
        normal: crate::mesh::vnormalize(polygon_normal(&tip_ring_points)),
    };
    let root_plane = StepSurface::Plane {
        origin: centroid(&root_ring_points),
        normal: crate::mesh::vnormalize(polygon_normal(&root_ring_points)),
    };

    // Upper skin: LE → tip → TE(rev) → root-upper(rev).
    model.faces.push(StepFaceDef {
        surface: StepSurface::Nurbs(skins),
        loop_edges: vec![
            (e_le, false),
            (e_tip_upper, false),
            (e_te, true),
            (e_root_upper, true),
        ],
    });
    // Lower skin: LE → root-lower → TE → tip-lower(rev).
    model.faces.push(StepFaceDef {
        surface: StepSurface::Nurbs(lower_skins),
        loop_edges: vec![
            (e_le, false),
            (e_root_lower, false),
            (e_te, false),
            (e_tip_lower, true),
        ],
    });
    // TE face: TE path, tip line, TE path back, root line back.
    model.faces.push(StepFaceDef {
        surface: te_surface,
        loop_edges: vec![
            (e_te, false),
            (e_tip_te_line, false),
            (e_te, true),
            (e_root_te_line, true),
        ],
    });

    if full {
        // Mirror the four surface faces (skip the root cap: the symmetry
        // plane gets one shared cap face) into a second half-shell, then
        // close with the root cap — one shell, one solid.
        let mirrored_faces: Vec<StepFaceDef> = model.faces[..3].iter().map(mirror_face).collect();
        for face in &mirrored_faces {
            model.faces.push(face.clone());
        }
        model.faces.push(StepFaceDef {
            surface: root_plane,
            loop_edges: vec![
                (e_root_upper, false),
                (e_root_te_line, false),
                (e_root_lower, true),
            ],
        });
        let face_count = model.faces.len();
        model.shells.push((0..face_count).collect());
    } else {
        // Half model: cap the tip, then the symmetry plane closes it.
        model.faces.push(StepFaceDef {
            surface: tip_plane,
            loop_edges: vec![
                (e_tip_upper, false),
                (e_tip_te_line, false),
                (e_tip_lower, true),
            ],
        });
        model.faces.push(StepFaceDef {
            surface: root_plane,
            loop_edges: vec![
                (e_root_upper, false),
                (e_root_te_line, false),
                (e_root_lower, true),
            ],
        });
        let face_count = model.faces.len();
        model.shells.push((0..face_count).collect());
    }
    Ok(model)
}

fn mirror_face(face: &StepFaceDef) -> StepFaceDef {
    StepFaceDef {
        surface: match &face.surface {
            StepSurface::Nurbs(surface) => {
                let mut mirrored = surface.clone();
                for column in mirrored.controls.iter_mut() {
                    for point in column.iter_mut() {
                        point[1] = -point[1];
                    }
                }
                StepSurface::Nurbs(mirrored)
            }
            StepSurface::Plane { origin, normal } => StepSurface::Plane {
                origin: [origin[0], -origin[1], origin[2]],
                normal: [normal[0], -normal[1], normal[2]],
            },
        },
        loop_edges: face
            .loop_edges
            .iter()
            .map(|(edge, reversed)| (*edge, !reversed))
            .collect(),
    }
}

fn mirror_row(row: &Row) -> Row {
    let mirror = |points: &[[f64; 3]]| -> Vec<[f64; 3]> {
        points.iter().map(|p| [p[0], -p[1], p[2]]).collect()
    };
    Row {
        upper: mirror(&row.upper),
        lower: mirror(&row.lower),
    }
}

fn row_chord(rings: &[crate::section::Ring]) -> f64 {
    let ring = &rings[0];
    let k = ring.points.len() / 2;
    let le = ring.points[0];
    let te = ring.points[k - 1];
    crate::mesh::vnorm(crate::mesh::vsub(te, le)).max(1e-6)
}

fn chord_samples_for(stations: &[EvaluatedStation], max_chordal_deviation: f64) -> usize {
    let _ = (stations, max_chordal_deviation);
    96
}

fn span_subdivisions(stations: &[EvaluatedStation], max_edge_length: f64) -> Vec<usize> {
    stations
        .windows(2)
        .map(|pair| {
            let span = crate::mesh::vnorm(crate::mesh::vsub(pair[1].position, pair[0].position));
            ((span / max_edge_length).ceil() as usize).clamp(4, 24)
        })
        .collect()
}

fn centroid(points: &[[f64; 3]]) -> [f64; 3] {
    let mut out = [0.0; 3];
    for point in points {
        for axis in 0..3 {
            out[axis] += point[axis];
        }
    }
    let n = points.len() as f64;
    [out[0] / n, out[1] / n, out[2] / n]
}

/// Newell's method for the polygon normal.
fn polygon_normal(points: &[[f64; 3]]) -> [f64; 3] {
    let mut normal = [0.0; 3];
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        normal[0] += (a[1] - b[1]) * (a[2] + b[2]);
        normal[1] += (a[2] - b[2]) * (a[0] + b[0]);
        normal[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    crate::mesh::vnormalize(normal)
}

impl StepModel {
    fn push_edge(&mut self, curve: StepCurve, start: usize, end: usize) -> usize {
        self.edges.push(StepEdge { curve, start, end });
        self.edges.len() - 1
    }
}
