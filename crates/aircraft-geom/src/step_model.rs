//! The wing's analytic STEP representation: NURBS skins, a bilinear
//! trailing-edge face, planar caps, and the topological bookkeeping
//! (vertices, shared edges, oriented loops, shells) the writer needs.
//!
//! Built from the same loft rows as the mesher, so tangent leading edges,
//! trailing-edge flares, and TE closures are captured exactly.

use crate::nurbs::{interpolate_cubic, skin, NurbsCurve, NurbsSurface};
pub use crate::profile::cosine_samples;
use crate::quality::ResolvedQuality;
use crate::wing::{loft_rings, EvaluatedStation, PanelTangency};
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

/// Tolerances for the analytic export: the edge length drives the spanwise
/// station count; chordwise sampling is fixed at [`STEP_CHORD_SAMPLES`].
#[derive(Debug, Clone, Copy)]
pub struct StepTolerances {
    pub max_edge_length: f64,
}

impl Default for StepTolerances {
    fn default() -> Self {
        StepTolerances {
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

fn ring_rows(ring: &crate::section::Ring) -> Row {
    let k = ring.points.len() / 2;
    // Rows keep every ring sample: the ring's vertex layout is
    // landmark-aligned (upper LE→TE, lower TE→LE), so both rows sit exactly
    // on the shared cosine parameter grid and every row in the wing has the
    // same point count — the shared-parameters invariant of the skins.
    let upper: Vec<[f64; 3]> = ring.points[0..k].to_vec();
    // Lower row stored TE→LE: index k is the TE lower vertex, 2k-1 the LE.
    let mut lower: Vec<[f64; 3]> = ring.points[k..2 * k].to_vec();
    lower.reverse();
    Row { upper, lower }
}

/// Build the wing's analytic STEP model: one closed solid per half-wing
/// (NURBS upper/lower skins, NURBS TE band, planar caps). The source
/// stations span the source half (negative local Y); `full` appends an
/// independent mirrored solid on the other side of the symmetry plane.
#[allow(clippy::too_many_arguments)]
pub fn wing_model(
    name: &str,
    stations: &[EvaluatedStation],
    symmetry_enabled: bool,
    tolerances: StepTolerances,
    panel_tangencies: &[PanelTangency],
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
        chord_samples: STEP_CHORD_SAMPLES,
        span_subdivisions: span_subdivisions(stations, tolerances.max_edge_length),
    };
    let rings = loft_rings(stations, &quality, panel_tangencies);
    let rows: Vec<Row> = rings.iter().map(ring_rows).collect();
    let last = rows.len() - 1;

    // Section curves: rows share chordwise parameters, so the skins' natural
    // boundaries coincide with these curves exactly. The parameters are the
    // root row's chord lengths, not the sample stations' x fractions: with
    // cosine sampling the x gaps at the nose are ~100x smaller than at
    // mid-chord, and x-parameterized interpolation rings there (a scalloped
    // leading edge). Chord-length parameters equalize the spacing.
    let params = nurbs_chord_params(&rows[0].upper);
    let mut upper_curves: Vec<NurbsCurve> = Vec::with_capacity(rows.len());
    let mut lower_curves: Vec<NurbsCurve> = Vec::with_capacity(rows.len());
    for row in &rows {
        let (upper, _) =
            interpolate_cubic(&row.upper, &params, 3).map_err(|diagnostic| vec![diagnostic])?;
        let (lower, _) =
            interpolate_cubic(&row.lower, &params, 3).map_err(|diagnostic| vec![diagnostic])?;
        upper_curves.push(upper);
        lower_curves.push(lower);
    }

    // Spanwise paths all share the LE-based parameters, so each path is
    // exactly the corresponding v-boundary of both skins and the paths
    // themselves skin into the TE band.
    let le_points: Vec<[f64; 3]> = rows.iter().map(|row| row.upper[0]).collect();
    let teu_points: Vec<[f64; 3]> = rows
        .iter()
        .map(|row| *row.upper.last().expect("non-empty"))
        .collect();
    let tel_points: Vec<[f64; 3]> = rows
        .iter()
        .map(|row| *row.lower.last().expect("non-empty"))
        .collect();
    let span_params = nurbs_chord_params(&le_points);
    let (le_path, _) =
        interpolate_cubic(&le_points, &span_params, 3).map_err(|diagnostic| vec![diagnostic])?;
    let (teu_path, _) =
        interpolate_cubic(&teu_points, &span_params, 3).map_err(|diagnostic| vec![diagnostic])?;
    let (tel_path, _) =
        interpolate_cubic(&tel_points, &span_params, 3).map_err(|diagnostic| vec![diagnostic])?;

    let skins = skin(&upper_curves, &span_params).map_err(|diagnostic| vec![diagnostic])?;
    let lower_skins = skin(&lower_curves, &span_params).map_err(|diagnostic| vec![diagnostic])?;
    let te_surface = skin(&[teu_path.clone(), tel_path.clone()], &[0.0, 1.0])
        .map_err(|diagnostic| vec![diagnostic])?;

    // Vertices at the six section-boundary points (root, then tip).
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

    let v_le_start = 0usize;
    let v_teu_start = 1usize;
    let v_tel_start = 2usize;
    let v_le_end = 3usize;
    let v_teu_end = 4usize;
    let v_tel_end = 5usize;

    // Shared edges.
    let e_le = model.push_edge(StepCurve::Nurbs(le_path), v_le_start, v_le_end);
    let e_teu = model.push_edge(StepCurve::Nurbs(teu_path), v_teu_start, v_teu_end);
    let e_tel = model.push_edge(StepCurve::Nurbs(tel_path), v_tel_start, v_tel_end);
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

    // Cap planes from the end rings. Both rings wind upper LE→TE then lower
    // TE→LE, so their Newell normal points toward positive span: outward at
    // the root cap (symmetry plane), inward at the tip cap, which flips it.
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
    let outward = crate::mesh::vnormalize(polygon_normal(&root_ring_points));
    let tip_plane = StepSurface::Plane {
        origin: centroid(&tip_ring_points),
        normal: [-outward[0], -outward[1], -outward[2]],
    };
    let root_plane = StepSurface::Plane {
        origin: centroid(&root_ring_points),
        normal: outward,
    };

    // Upper skin: LE, tip upper, TE back, root upper back — wound clockwise
    // in (u, v) so the face normal points up, out of the wing.
    model.faces.push(StepFaceDef {
        surface: StepSurface::Nurbs(skins),
        loop_edges: vec![
            (e_le, false),
            (e_tip_upper, false),
            (e_teu, true),
            (e_root_upper, true),
        ],
    });
    // Lower skin: root lower, TE, tip lower back, LE back — counter-clockwise
    // in (u, v), face normal points down, out of the wing.
    model.faces.push(StepFaceDef {
        surface: StepSurface::Nurbs(lower_skins),
        loop_edges: vec![
            (e_root_lower, false),
            (e_tel, false),
            (e_tip_lower, true),
            (e_le, true),
        ],
    });
    // TE band: upper TE path, tip line, lower TE path back, root line back.
    model.faces.push(StepFaceDef {
        surface: StepSurface::Nurbs(te_surface),
        loop_edges: vec![
            (e_teu, false),
            (e_tip_te_line, false),
            (e_tel, true),
            (e_root_te_line, true),
        ],
    });
    // Tip cap (traversed against the skin so shared edges oppose).
    model.faces.push(StepFaceDef {
        surface: tip_plane,
        loop_edges: vec![
            (e_tip_upper, true),
            (e_tip_lower, false),
            (e_tip_te_line, true),
        ],
    });
    // Root cap on the symmetry plane.
    model.faces.push(StepFaceDef {
        surface: root_plane,
        loop_edges: vec![
            (e_root_upper, false),
            (e_root_te_line, false),
            (e_root_lower, true),
        ],
    });
    model.shells.push((0..model.faces.len()).collect());

    if full {
        mirror_solid(&mut model);
    }

    model.validate().map_err(|message| {
        vec![Diagnostic::error(Code::MeshFailure, message).with_subject(name.to_string())]
    })?;
    Ok(model)
}

/// Append an independent mirrored copy of the first shell: a second solid on
/// the other side of the symmetry plane, with its own vertices, edges, and
/// faces. Loop order reverses so the mirrored faces stay outward-oriented.
fn mirror_solid(model: &mut StepModel) {
    let vertex_base = model.vertices.len();
    let cloned = model.vertices.clone();
    for point in cloned {
        model.vertices.push([point[0], -point[1], point[2]]);
    }
    let mut edge_map = Vec::with_capacity(model.edges.len());
    let edges = model.edges.clone();
    for edge in edges {
        let index = model.push_edge(
            mirror_curve(&edge.curve),
            vertex_base + edge.start,
            vertex_base + edge.end,
        );
        edge_map.push(index);
    }
    let face_base = model.faces.len();
    let faces = model.faces.clone();
    for face in &faces {
        model.faces.push(StepFaceDef {
            surface: mirror_surface(&face.surface),
            loop_edges: face
                .loop_edges
                .iter()
                .rev()
                .map(|&(edge, reversed)| (edge_map[edge], !reversed))
                .collect(),
        });
    }
    model.shells.push((face_base..model.faces.len()).collect());
}

fn mirror_curve(curve: &StepCurve) -> StepCurve {
    let mirror_point = |point: [f64; 3]| [point[0], -point[1], point[2]];
    match curve {
        StepCurve::Line(start, end) => StepCurve::Line(mirror_point(*start), mirror_point(*end)),
        StepCurve::Nurbs(nurbs) => {
            let mut mirrored = nurbs.clone();
            for point in mirrored.controls.iter_mut() {
                point[1] = -point[1];
            }
            StepCurve::Nurbs(mirrored)
        }
    }
}

fn mirror_surface(surface: &StepSurface) -> StepSurface {
    match surface {
        StepSurface::Nurbs(nurbs) => {
            let mut mirrored = nurbs.clone();
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
    }
}

impl StepModel {
    /// Topological soundness checks: every face loop is a closed walk over
    /// its vertices, and every shell uses each edge exactly twice, once per
    /// sense. CAD readers reject the shell otherwise (or worse, segfault).
    pub fn validate(&self) -> Result<(), String> {
        for (face_index, face) in self.faces.iter().enumerate() {
            if face.loop_edges.len() < 3 {
                return Err(format!("face {face_index}: loop needs at least 3 edges"));
            }
            let walk = |edge_index: usize, reversed: bool| {
                let edge = &self.edges[edge_index];
                if reversed {
                    (edge.end, edge.start)
                } else {
                    (edge.start, edge.end)
                }
            };
            let (first_entry, first_exit) = walk(face.loop_edges[0].0, face.loop_edges[0].1);
            let mut cursor = first_exit;
            for &(edge_index, reversed) in face.loop_edges.iter().skip(1) {
                let (entry, exit) = walk(edge_index, reversed);
                if cursor != entry {
                    return Err(format!(
                        "face {face_index}: loop jumps at edge {edge_index} ({cursor} → {entry})"
                    ));
                }
                cursor = exit;
            }
            if cursor != first_entry {
                return Err(format!(
                    "face {face_index}: loop does not close ({cursor} ≠ {first_entry})"
                ));
            }
        }
        for (shell_index, shell) in self.shells.iter().enumerate() {
            let mut usage: std::collections::HashMap<usize, [usize; 2]> =
                std::collections::HashMap::new();
            for &face_index in shell {
                for &(edge_index, reversed) in &self.faces[face_index].loop_edges {
                    let counts = usage.entry(edge_index).or_default();
                    counts[if reversed { 1 } else { 0 }] += 1;
                }
            }
            for (edge_index, [forward, backward]) in usage {
                if forward != 1 || backward != 1 {
                    return Err(format!(
                        "shell {shell_index}: edge {edge_index} used {forward}× forward, {backward}× backward"
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Chord samples per surface per STEP ring. The rows interpolate through
/// every ring sample, so this count is the analytic surfaces' chordwise
/// resolution: dense enough to hold the leading-edge nose, sparse enough
/// that the global cubic interpolation stays well-conditioned.
const STEP_CHORD_SAMPLES: usize = 96;

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
