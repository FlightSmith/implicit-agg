//! Mesh export: binary and ASCII STL, OBJ, and GLB.
//!
//! Exporters are pure functions from a mesh to bytes; file writing stays with
//! the caller. GLB output carries a documented Z-up to Y-up root rotation.

use aircraft_geom::mesh::{cross, vsub};
use aircraft_geom::Mesh;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshFormat {
    StlBinary,
    StlAscii,
    Obj,
    Glb,
}

impl MeshFormat {
    /// Canonical file extension without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            MeshFormat::StlBinary | MeshFormat::StlAscii => "stl",
            MeshFormat::Obj => "obj",
            MeshFormat::Glb => "glb",
        }
    }
}

/// Export a mesh in the requested format.
pub fn export(mesh: &Mesh, name: &str, format: MeshFormat) -> Vec<u8> {
    match format {
        MeshFormat::StlBinary => export_stl_binary(mesh, name),
        MeshFormat::StlAscii => export_stl_ascii(mesh, name).into_bytes(),
        MeshFormat::Obj => export_obj(mesh, name).into_bytes(),
        MeshFormat::Glb => export_glb(mesh, name),
    }
}

fn triangle_vertices(mesh: &Mesh, triangle: usize) -> ([[f64; 3]; 3], [f64; 3]) {
    let [a, b, c] = mesh.triangles[triangle];
    let (va, vb, vc) = (
        mesh.vertices[a as usize],
        mesh.vertices[b as usize],
        mesh.vertices[c as usize],
    );
    let raw = cross(vsub(vb, va), vsub(vc, va));
    let magnitude = (raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2]).sqrt();
    let normal = if magnitude > 0.0 {
        [raw[0] / magnitude, raw[1] / magnitude, raw[2] / magnitude]
    } else {
        [0.0; 3]
    };
    ([va, vb, vc], normal)
}

/// Binary STL: 80-byte header, triangle count, then 50 bytes per triangle.
pub fn export_stl_binary(mesh: &Mesh, name: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(84 + mesh.triangles.len() * 50);
    let mut header = [b' '; 80];
    let label = name.as_bytes();
    header[..label.len().min(80)].copy_from_slice(&label[..label.len().min(80)]);
    out.extend_from_slice(&header);
    out.extend_from_slice(&(mesh.triangles.len() as u32).to_le_bytes());

    let mut scratch = Vec::new();
    for triangle in 0..mesh.triangle_count() {
        let (vertices, normal) = triangle_vertices(mesh, triangle);
        scratch.clear();
        for value in normal.into_iter().chain(vertices.iter().flatten().copied()) {
            scratch.extend_from_slice(&(value as f32).to_le_bytes());
        }
        out.extend_from_slice(&scratch);
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    out
}

/// ASCII STL for tools that want readable output.
pub fn export_stl_ascii(mesh: &Mesh, name: &str) -> String {
    let mut out = String::with_capacity(mesh.triangles.len() * 256 + 64);
    out.push_str(&format!("solid {name}\n"));
    for triangle in 0..mesh.triangle_count() {
        let (vertices, normal) = triangle_vertices(mesh, triangle);
        out.push_str(&format!(
            "  facet normal {:.6e} {:.6e} {:.6e}\n    outer loop\n",
            normal[0], normal[1], normal[2]
        ));
        for vertex in vertices {
            out.push_str(&format!(
                "      vertex {:.6e} {:.6e} {:.6e}\n",
                vertex[0], vertex[1], vertex[2]
            ));
        }
        out.push_str("    endloop\n  endfacet\n");
    }
    out.push_str(&format!("endsolid {name}\n"));
    out
}

/// Wavefront OBJ with 1-based indices.
pub fn export_obj(mesh: &Mesh, name: &str) -> String {
    let mut out = String::with_capacity(mesh.vertices.len() * 48 + mesh.triangles.len() * 40);
    out.push_str(&format!("# {name}\n"));
    for vertex in &mesh.vertices {
        out.push_str(&format!("v {} {} {}\n", vertex[0], vertex[1], vertex[2]));
    }
    for triangle in &mesh.triangles {
        out.push_str(&format!(
            "f {} {} {}\n",
            triangle[0] + 1,
            triangle[1] + 1,
            triangle[2] + 1
        ));
    }
    out
}

/// Minimal GLB (glTF 2.0) container: one mesh, indexed triangles, POSITION
/// plus indices, and a root node rotating Z-up geometry into glTF's Y-up
/// convention (rotation about X by -90 degrees).
pub fn export_glb(mesh: &Mesh, name: &str) -> Vec<u8> {
    let mut bin: Vec<u8> = Vec::new();
    let position_offset = 0u32;
    let position_length = (mesh.vertices.len() * 12) as u32;
    for vertex in &mesh.vertices {
        for value in vertex {
            bin.extend_from_slice(&(*value as f32).to_le_bytes());
        }
    }
    let indices_offset = position_length;
    let indices_length = (mesh.triangles.len() * 12) as u32;
    for triangle in &mesh.triangles {
        for index in triangle {
            bin.extend_from_slice(&index.to_le_bytes());
        }
    }

    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for vertex in &mesh.vertices {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex[axis]);
            max[axis] = max[axis].max(vertex[axis]);
        }
    }
    let fmt = |mut values: [f64; 3]| {
        for value in &mut values {
            *value = *value as f32 as f64;
        }
        let [a, b, c] = values;
        format!("[{a},{b},{c}]")
    };

    // Rotating the model by -90 degrees about X maps aircraft Z-up into
    // glTF Y-up: (x, y, z) -> (x, z, -y).
    let json = format!(
        r#"{{"asset":{{"version":"2.0","generator":"{name}"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0,"rotation":[-0.7071067811865476,0,0,0.7071067811865476]}}],"meshes":[{{"name":"{name}","primitives":[{{"attributes":{{"POSITION":0}},"indices":1,"mode":4}}]}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":{count_vertices},"type":"VEC3","min":{min},"max":{max}}},{{"bufferView":1,"componentType":5125,"count":{count_indices},"type":"SCALAR"}}],"bufferViews":[{{"buffer":0,"byteOffset":{position_offset},"byteLength":{position_length}}},{{"buffer":0,"byteOffset":{indices_offset},"byteLength":{indices_length}}}],"buffers":[{{"byteLength":{buffer_length}}}]}}"#,
        count_vertices = mesh.vertices.len(),
        count_indices = mesh.triangles.len() * 3,
        buffer_length = bin.len(),
        min = fmt(min),
        max = fmt(max),
    );

    let json_bytes = json.as_bytes();
    let json_pad = (4 - json_bytes.len() % 4) % 4;
    let bin_pad = (4 - bin.len() % 4) % 4;
    let json_chunk_length = json_bytes.len() + json_pad;
    let bin_chunk_length = bin.len() + bin_pad;
    let total = 12 + 8 + json_chunk_length + 8 + bin_chunk_length;

    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&0x46546C67u32.to_le_bytes()); // "glTF"
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_chunk_length as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F534Au32.to_le_bytes()); // "JSON"
    out.extend_from_slice(json_bytes);
    out.extend(std::iter::repeat_n(b' ', json_pad));
    out.extend_from_slice(&(bin_chunk_length as u32).to_le_bytes());
    out.extend_from_slice(&0x004E4942u32.to_le_bytes()); // "BIN\0"
    out.extend_from_slice(&bin);
    out.extend(std::iter::repeat_n(0u8, bin_pad));
    out
}
