//! Triangle mesh representation, welding, cleanup, manifold validation, and
//! integral measures.

use std::collections::HashMap;

use aircraft_model::{Code, Diagnostic};

#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn triangle_normal(&self, triangle: [u32; 3]) -> [f64; 3] {
        let (va, vb, vc) = (
            self.vertices[triangle[0] as usize],
            self.vertices[triangle[1] as usize],
            self.vertices[triangle[2] as usize],
        );
        cross(vsub(vb, va), vsub(vc, va))
    }

    /// Merge vertices with identical positions, drop degenerate triangles,
    /// and report which original triangles survived. Coincidence is exact
    /// (bit-level, with negative zero normalized) because structured rings
    /// and the symmetry mirror produce bit-identical shared vertices.
    pub fn weld_and_clean(self) -> (Mesh, Vec<usize>) {
        let Mesh {
            vertices,
            triangles,
        } = self;
        let mut remap: Vec<u32> = Vec::with_capacity(vertices.len());
        let mut unique: HashMap<u64, u32> = HashMap::with_capacity(vertices.len());
        let mut welded_vertices: Vec<[f64; 3]> = Vec::with_capacity(vertices.len());
        for vertex in &vertices {
            let key = position_key(*vertex);
            let index = match unique.get(&key) {
                Some(&index) => index,
                None => {
                    let index = welded_vertices.len() as u32;
                    welded_vertices.push(*vertex);
                    unique.insert(key, index);
                    index
                }
            };
            remap.push(index);
        }

        let mut welded = Mesh {
            vertices: welded_vertices,
            triangles: Vec::with_capacity(triangles.len()),
        };
        let mut kept: Vec<usize> = Vec::with_capacity(triangles.len());
        for (original, triangle) in triangles.iter().enumerate() {
            let mapped = [
                remap[triangle[0] as usize],
                remap[triangle[1] as usize],
                remap[triangle[2] as usize],
            ];
            if mapped[0] == mapped[1] || mapped[1] == mapped[2] || mapped[0] == mapped[2] {
                continue;
            }
            let normal = welded.triangle_normal(mapped);
            // Structured cleanup only ever produces exactly-degenerate
            // triangles, so this threshold sits far below any real feature.
            if dot(normal, normal) < 1.0e-24 {
                continue;
            }
            welded.triangles.push(mapped);
            kept.push(original);
        }
        (welded, kept)
    }

    /// Topological validation: closedness, edge-manifoldness, and orientation
    /// consistency of interior edges.
    pub fn validate(&self) -> MeshValidation {
        let mut edges: HashMap<(u32, u32), (usize, i32)> = HashMap::new();
        let mut degenerate = 0usize;
        for &triangle in &self.triangles {
            let [a, b, c] = triangle;
            if a == b || b == c || a == c {
                degenerate += 1;
                continue;
            }
            for (p, q) in [(a, b), (b, c), (c, a)] {
                let key = if p < q { (p, q) } else { (q, p) };
                let entry = edges.entry(key).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += if p < q { 1 } else { -1 };
            }
        }

        let mut validation = MeshValidation {
            closed: true,
            manifold: true,
            oriented: true,
            degenerate_triangles: degenerate,
        };
        for &(count, direction_sum) in edges.values() {
            if count > 2 {
                validation.manifold = false;
            }
            if count == 1 {
                validation.closed = false;
            }
            if count == 2 && direction_sum != 0 {
                validation.oriented = false;
            }
        }
        validation
    }

    /// Projected area on the XY plane — the planform view (span x chord,
    /// looking down Z). Exact for the faceted mesh, so tangent-LE curvature
    /// is fully accounted for.
    pub fn projected_area_xy(&self) -> f64 {
        self.triangles
            .iter()
            .map(|&triangle| self.triangle_normal(triangle)[2].abs() / 2.0)
            .sum()
    }

    /// Total triangle area.
    pub fn surface_area(&self) -> f64 {
        self.triangles
            .iter()
            .map(|&triangle| 0.5 * vnorm(self.triangle_normal(triangle)))
            .sum()
    }

    /// Signed enclosed volume and center of volume via the divergence
    /// theorem. Positive volume means consistently outward-oriented faces.
    pub fn volume_and_center(&self) -> (f64, [f64; 3]) {
        let mut volume = 0.0;
        let mut weighted = [0.0; 3];
        for &triangle in &self.triangles {
            let (va, vb, vc) = (
                self.vertices[triangle[0] as usize],
                self.vertices[triangle[1] as usize],
                self.vertices[triangle[2] as usize],
            );
            let tetra = dot(va, cross(vb, vc)) / 6.0;
            volume += tetra;
            for axis in 0..3 {
                weighted[axis] += tetra * (va[axis] + vb[axis] + vc[axis]) / 4.0;
            }
        }
        if volume.abs() > 0.0 {
            for value in &mut weighted {
                *value /= volume;
            }
        }
        (volume, weighted)
    }

    /// Translate every vertex by `offset`.
    pub fn translated(&self, offset: [f64; 3]) -> Mesh {
        Mesh {
            vertices: self
                .vertices
                .iter()
                .map(|v| [v[0] + offset[0], v[1] + offset[1], v[2] + offset[2]])
                .collect(),
            triangles: self.triangles.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MeshValidation {
    pub closed: bool,
    pub manifold: bool,
    pub oriented: bool,
    pub degenerate_triangles: usize,
}

impl MeshValidation {
    pub fn is_sound(&self) -> bool {
        self.manifold && self.oriented && self.degenerate_triangles == 0
    }

    /// Turn an unsound validation into mesh-failure diagnostics.
    pub fn diagnostics(&self, subject: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        if !self.manifold {
            diagnostics.push(
                Diagnostic::error(Code::MeshFailure, "mesh has non-manifold edges")
                    .with_subject(subject.to_string()),
            );
        }
        if !self.oriented {
            diagnostics.push(
                Diagnostic::error(Code::MeshFailure, "mesh face orientation is inconsistent")
                    .with_subject(subject.to_string()),
            );
        }
        if self.degenerate_triangles > 0 {
            diagnostics.push(
                Diagnostic::error(
                    Code::MeshFailure,
                    format!(
                        "mesh has {} degenerate triangles",
                        self.degenerate_triangles
                    ),
                )
                .with_subject(subject.to_string()),
            );
        }
        diagnostics
    }
}

/// Hash key for exact vertex welding. Negative zero is normalized so the
/// symmetry mirror (y = 0 -> -0.0) welds against the source ring.
fn position_key(vertex: [f64; 3]) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for value in vertex {
        let normalized = if value == 0.0 { 0.0 } else { value };
        hasher.write_u64(normalized.to_bits());
    }
    hasher.finish()
}

pub fn vsub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn vadd(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn vscale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn vnorm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

pub fn vnormalize(a: [f64; 3]) -> [f64; 3] {
    vscale(a, 1.0 / vnorm(a))
}

pub fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    // Endpoint exactness matters: panel-boundary rings are welded bit-exactly,
    // so `t = 1` must return `b` itself, not `a + (b - a)` (off by an ULP).
    if t == 0.0 {
        return a;
    }
    if t == 1.0 {
        return b;
    }
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}
