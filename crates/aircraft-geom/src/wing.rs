//! The wing generator: evaluated stations -> watertight surface -> preview or
//! export mesh, with a face-to-source map for picking and tracing.
//!
//! The source half is generated in wing-local coordinates with its root on
//! local XZ (`y = 0`). A full model reflects it across local XZ and welds the
//! centerline before any other consumer sees the mesh.

use crate::mesh::Mesh;
use crate::profile::ProfileCurve;
use crate::quality::ResolvedQuality;
use crate::section::{lerp_rings, Ring, StationSpec};
use aircraft_model::{Code, Diagnostic};
use std::sync::Arc;

/// An evaluated station, ready for geometry.
#[derive(Debug, Clone)]
pub struct EvaluatedStation {
    pub id: String,
    /// Wing-local leading-edge position, canonical meters.
    pub position: [f64; 3],
    pub chord: f64,
    /// Canonical radians.
    pub twist: f64,
    pub trailing_edge: crate::section::TrailingEdgeSpec,
    pub curve: Arc<ProfileCurve>,
}

/// What a mesh triangle was generated from, for selection tracing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FaceSource {
    /// Part of the lofted surface between two adjacent stations.
    Panel {
        lower_station: usize,
        upper_station: usize,
        mirrored: bool,
    },
    /// The flat tip closure of the outboard-most station.
    TipCap { station: usize, mirrored: bool },
    /// The symmetry-plane cap of a standalone half model.
    RootCap,
}

#[derive(Debug)]
pub struct WingMesh {
    pub mesh: Mesh,
    pub faces: Vec<FaceSource>,
}

/// Build the wing mesh in wing-local coordinates.
///
/// `root_cap` closes the symmetry plane of a standalone half model; a full
/// model (`mirror: true`, requires `symmetry_enabled`) is welded open at the
/// root instead.
pub fn build_wing_mesh(
    wing_id: &str,
    stations: &[EvaluatedStation],
    symmetry_enabled: bool,
    quality: &ResolvedQuality,
    root_cap: bool,
    mirror: bool,
) -> Result<WingMesh, Vec<Diagnostic>> {
    if stations.len() < 2 {
        return Err(vec![Diagnostic::error(
            Code::MeshFailure,
            "a wing needs at least two stations",
        )
        .with_subject(wing_id.to_string())]);
    }
    if mirror && !symmetry_enabled {
        return Err(vec![Diagnostic::error(
            Code::MeshFailure,
            "cannot mirror a wing whose symmetry is disabled",
        )
        .with_subject(wing_id.to_string())]);
    }

    // Feasibility first: reject rather than attempt to repair.
    let mut specs: Vec<StationSpec<'_>> = Vec::with_capacity(stations.len());
    let mut errors: Vec<Diagnostic> = Vec::new();
    for station in stations {
        let spec = StationSpec {
            id: station.id.clone(),
            curve: &station.curve,
            chord: station.chord,
            twist: station.twist,
            position: station.position,
            trailing_edge: station.trailing_edge,
        };
        let subject = format!("{wing_id} / station {}", station.id);
        if let Err(diagnostic) = spec.check_feasible(&subject) {
            errors.push(diagnostic);
        }
        specs.push(spec);
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let k = quality.chord_samples;
    let base_rings: Vec<Ring> = specs.iter().map(|spec| spec.build_ring(k)).collect();

    let mut builder = RawBuilder::default();
    for panel in 0..stations.len() - 1 {
        let subdivisions = quality.span_subdivisions.get(panel).copied().unwrap_or(1);
        let lower = &base_rings[panel];
        let upper = &base_rings[panel + 1];
        for step in 0..subdivisions {
            let ring_lo = lerp_rings(lower, upper, step as f64 / subdivisions as f64);
            let ring_hi = lerp_rings(lower, upper, (step + 1) as f64 / subdivisions as f64);
            add_panel(&mut builder, &ring_lo, &ring_hi, panel, panel + 1, false);
        }
    }

    // Tip closure: flat cap over the outboard-most ring.
    let tip_ring = base_rings[base_rings.len() - 1].clone();
    add_tip_cap(&mut builder, &tip_ring, k, stations.len() - 1, false);

    if root_cap {
        let root_ring = base_rings[0].clone();
        add_root_cap(&mut builder, &root_ring, k);
    }

    if mirror {
        let mirrored = mirror_builder(&builder);
        builder.append(mirrored);
    }

    let RawBuilder {
        vertices,
        triangles,
        faces: raw_faces,
    } = builder;
    let (mesh, kept) = Mesh {
        vertices,
        triangles,
    }
    .weld_and_clean();
    let faces: Vec<FaceSource> = kept.iter().map(|&index| raw_faces[index]).collect();

    let validation = mesh.validate();
    if !validation.is_sound() {
        return Err(validation.diagnostics(wing_id));
    }
    if validation.closed {
        let (volume, _) = mesh.volume_and_center();
        if volume <= 0.0 {
            return Err(vec![Diagnostic::error(
                Code::MeshFailure,
                format!("closed mesh has non-positive volume ({volume})"),
            )
            .with_subject(wing_id.to_string())]);
        }
    }

    Ok(WingMesh { mesh, faces })
}

#[derive(Default)]
struct RawBuilder {
    vertices: Vec<[f64; 3]>,
    triangles: Vec<[u32; 3]>,
    faces: Vec<FaceSource>,
}

impl RawBuilder {
    fn push_vertex(&mut self, point: [f64; 3]) -> u32 {
        let index = self.vertices.len() as u32;
        self.vertices.push(point);
        index
    }

    fn push_ring(&mut self, ring: &Ring) -> Vec<u32> {
        ring.points.iter().map(|&p| self.push_vertex(p)).collect()
    }

    fn push_triangle(&mut self, a: u32, b: u32, c: u32, face: FaceSource) {
        self.triangles.push([a, b, c]);
        self.faces.push(face);
    }

    fn append(&mut self, other: RawBuilder) {
        let offset = self.vertices.len() as u32;
        self.vertices.extend(other.vertices);
        for (triangle, face) in other.triangles.into_iter().zip(other.faces) {
            self.triangles.push([
                triangle[0] + offset,
                triangle[1] + offset,
                triangle[2] + offset,
            ]);
            self.faces.push(face);
        }
    }
}

/// Loft one spanwise step between two aligned rings.
fn add_panel(
    builder: &mut RawBuilder,
    lo: &Ring,
    hi: &Ring,
    lower_station: usize,
    upper_station: usize,
    mirrored: bool,
) {
    let a: Vec<u32> = builder.push_ring(lo);
    let b: Vec<u32> = builder.push_ring(hi);
    let face = FaceSource::Panel {
        lower_station,
        upper_station,
        mirrored,
    };
    let m = a.len();
    for i in 0..m {
        let next = (i + 1) % m;
        // Winding chosen so the source half's faces point outward.
        builder.push_triangle(a[i], b[i], b[next], face);
        builder.push_triangle(a[i], b[next], a[next], face);
    }
}

/// Flat tip cap facing outboard (negative local Y).
/// Flat caps triangulate the ring as a band between the aligned upper and
/// lower chains (the ring is x-monotone by construction), which stays robust
/// where a leading-edge fan would create collinear, zero-area triangles.
/// Ring layout: `u_i = i`, `l_i = 2k-1-i` for `i < k-1`, `l_{k-1} = k`.
fn add_cap_band(
    builder: &mut RawBuilder,
    ring: &Ring,
    k: usize,
    face: FaceSource,
    tip_outward: bool,
) {
    let indices = builder.push_ring(ring);
    let u = |i: usize| indices[i];
    let l = |i: usize| {
        if i == k - 1 {
            indices[k]
        } else {
            indices[2 * k - 1 - i]
        }
    };
    for i in 0..k - 1 {
        if tip_outward {
            builder.push_triangle(u(i), l(i + 1), u(i + 1), face);
            builder.push_triangle(u(i), l(i), l(i + 1), face);
        } else {
            builder.push_triangle(u(i), u(i + 1), l(i + 1), face);
            builder.push_triangle(u(i), l(i + 1), l(i), face);
        }
    }
}

/// Flat tip cap facing outboard (negative local Y).
fn add_tip_cap(builder: &mut RawBuilder, ring: &Ring, k: usize, station: usize, mirrored: bool) {
    let face = FaceSource::TipCap { station, mirrored };
    add_cap_band(builder, ring, k, face, true);
}

/// Symmetry-plane cap of a half model, facing +Y.
fn add_root_cap(builder: &mut RawBuilder, ring: &Ring, k: usize) {
    add_cap_band(builder, ring, k, FaceSource::RootCap, false);
}

/// Reflect every vertex across local XZ, flip winding, and mark faces
/// mirrored. Centerline vertices (y = 0) coincide with the source's and are
/// merged by the weld.
fn mirror_builder(builder: &RawBuilder) -> RawBuilder {
    let mut mirrored = RawBuilder {
        vertices: builder
            .vertices
            .iter()
            .map(|v| [v[0], -v[1], v[2]])
            .collect(),
        triangles: Vec::with_capacity(builder.triangles.len()),
        faces: Vec::with_capacity(builder.faces.len()),
    };
    for (triangle, face) in builder.triangles.iter().zip(&builder.faces) {
        let flipped = [triangle[0], triangle[2], triangle[1]];
        mirrored.triangles.push(flipped);
        mirrored.faces.push(mirror_face(*face));
    }
    mirrored
}

fn mirror_face(face: FaceSource) -> FaceSource {
    match face {
        FaceSource::Panel {
            lower_station,
            upper_station,
            ..
        } => FaceSource::Panel {
            lower_station,
            upper_station,
            mirrored: true,
        },
        FaceSource::TipCap { station, .. } => FaceSource::TipCap {
            station,
            mirrored: true,
        },
        FaceSource::RootCap => FaceSource::RootCap,
    }
}
