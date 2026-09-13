#![allow(clippy::unwrap_used)]

use aircraft_geom::mesh::Mesh;
use aircraft_geom::section::{Ring, StationSpec, TrailingEdgeSpec};
use aircraft_geom::wing::EvaluatedStation;
use aircraft_geom::{build_wing_mesh, ProfileCurve};
use meshio::{export, export_glb, export_obj, export_stl_ascii, export_stl_binary, MeshFormat};
use std::sync::Arc;

fn fixture_mesh() -> Mesh {
    let curve = Arc::new(ProfileCurve::naca4("2412").unwrap());
    let mk = |id: &str, y: f64, x: f64| EvaluatedStation {
        id: id.into(),
        position: [x, y, 0.0],
        chord: 1.5,
        twist: 0.0,
        trailing_edge: TrailingEdgeSpec::Absolute(0.004),
        curve: Arc::clone(&curve),
    };
    let stations = vec![mk("root", 0.0, 0.0), mk("tip", -2.0, 0.6)];
    let quality = aircraft_geom::ResolvedQuality {
        chord_samples: 16,
        span_subdivisions: vec![3],
    };
    build_wing_mesh("fixture", &stations, true, &quality, true, false, None)
        .expect("fixture mesh")
        .mesh
}

#[test]
fn binary_stl_round_trips_triangle_count_and_normals() {
    let mesh = fixture_mesh();
    let bytes = export_stl_binary(&mesh, "fixture");
    assert_eq!(bytes.len(), 84 + mesh.triangle_count() * 50);

    let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
    assert_eq!(count, mesh.triangle_count());

    // First triangle normal matches the computed normal, at f32 precision.
    let read_f32 =
        |offset: usize| f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as f64;
    let stored = [read_f32(84), read_f32(88), read_f32(92)];
    let expected = mesh.triangle_normal(mesh.triangles[0]);
    let magnitude =
        (expected[0] * expected[0] + expected[1] * expected[1] + expected[2] * expected[2]).sqrt();
    for axis in 0..3 {
        assert!((stored[axis] - expected[axis] / magnitude).abs() < 1e-5);
    }
}

#[test]
fn ascii_stl_contains_every_facet() {
    let mesh = fixture_mesh();
    let text = export_stl_ascii(&mesh, "fixture");
    assert!(text.starts_with("solid fixture"));
    assert!(text.ends_with("endsolid fixture\n"));
    assert_eq!(text.matches("facet normal").count(), mesh.triangle_count());
    assert_eq!(text.matches("vertex").count(), mesh.triangle_count() * 3);
}

#[test]
fn obj_writes_one_indexed_face_per_triangle() {
    let mesh = fixture_mesh();
    let text = export_obj(&mesh, "fixture");
    let vertices = text.lines().filter(|l| l.starts_with("v ")).count();
    let faces = text.lines().filter(|l| l.starts_with("f ")).count();
    assert_eq!(vertices, mesh.vertex_count());
    assert_eq!(faces, mesh.triangle_count());
    // OBJ indices are 1-based.
    let first_face = text.lines().find(|l| l.starts_with("f ")).unwrap();
    let first_index: usize = first_face
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(first_index, mesh.triangles[0][0] as usize + 1);
}

#[test]
fn glb_has_consistent_chunks_and_json() {
    let mesh = fixture_mesh();
    let bytes = export_glb(&mesh, "fixture");

    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    assert_eq!(magic, 0x46546C67, "glTF magic");
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    assert_eq!(version, 2);
    let total = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    assert_eq!(total, bytes.len(), "GLB length header matches output");

    let json_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let json_type = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    assert_eq!(json_type, 0x4E4F534A, "JSON chunk type");
    let json: serde_json::Value =
        serde_json::from_slice(&bytes[20..20 + json_length]).expect("valid JSON chunk");
    assert_eq!(json["asset"]["version"], "2.0");
    assert_eq!(
        json["accessors"][0]["count"],
        mesh.vertex_count() as u64,
        "POSITION accessor count"
    );
    assert_eq!(
        json["accessors"][1]["count"],
        (mesh.triangle_count() * 3) as u64,
        "index accessor count"
    );
    let buffer_length = json["buffers"][0]["byteLength"].as_u64().unwrap();
    let bin_offset = 20 + json_length + 8;
    assert_eq!(
        buffer_length as usize,
        bytes.len() - bin_offset,
        "buffer length covers the BIN chunk payload"
    );

    // The rotation maps Z-up into Y-up.
    let rotation = json["nodes"][0]["rotation"].as_array().unwrap();
    assert_eq!(rotation.len(), 4);
}

#[test]
fn dispatch_matches_the_specific_exporters() {
    let mesh = fixture_mesh();
    for (format, specific) in [
        (MeshFormat::StlBinary, export_stl_binary(&mesh, "x")),
        (
            MeshFormat::StlAscii,
            export_stl_ascii(&mesh, "x").into_bytes(),
        ),
        (MeshFormat::Obj, export_obj(&mesh, "x").into_bytes()),
        (MeshFormat::Glb, export_glb(&mesh, "x")),
    ] {
        assert_eq!(export(&mesh, "x", format), specific);
    }
    assert_eq!(MeshFormat::StlBinary.extension(), "stl");
    assert_eq!(MeshFormat::Glb.extension(), "glb");
}

#[test]
fn ring_helper_keeps_fixture_honest() {
    // Guards against the fixture silently degrading to an empty mesh.
    let curve = ProfileCurve::naca4("2412").unwrap();
    let spec = StationSpec {
        id: "root".into(),
        curve: &curve,
        chord: 1.5,
        twist: 0.0,
        position: [0.0; 3],
        trailing_edge: TrailingEdgeSpec::Absolute(0.004),
    };
    let ring: Ring = spec.build_ring(16);
    assert_eq!(ring.points.len(), 32);
}
