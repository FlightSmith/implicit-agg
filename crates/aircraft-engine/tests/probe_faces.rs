#![allow(clippy::unwrap_used)]
use aircraft_engine::Engine;
use aircraft_geom::step_model::StepModel;
use aircraft_model::parse_document;

/// Manual debugging harness, not a CI test: writes one STEP file per face
/// into the system temp dir for side-by-side inspection. Run with
/// `cargo test -p aircraft-engine --test probe_faces -- --ignored`.
#[test]
#[ignore = "manual STEP face dumper; run with --ignored"]
fn write_each_face() {
    let (doc, _) = parse_document(include_str!("fixtures/cranked-wing-tangency.json")).unwrap();
    let engine = Engine::open(doc).unwrap();
    let symmetry = true;
    let (_, stations) = engine.evaluated_stations(0).unwrap();

    let out_dir = std::env::temp_dir().join("aircraft-faces");
    std::fs::create_dir_all(&out_dir).unwrap();
    let panel_tangencies = engine.resolve_panel_tangencies(0).unwrap();
    let model = aircraft_geom::step_model::wing_model(
        "wing",
        &stations,
        symmetry,
        Default::default(),
        &panel_tangencies,
        true,
    )
    .unwrap();
    for (index, face) in model.faces.iter().enumerate() {
        let sub = StepModel {
            name: format!("face{index}"),
            vertices: model.vertices.clone(),
            edges: model.edges.clone(),
            faces: vec![face.clone()],
            shells: vec![vec![0]],
        };
        let text = meshio::step::write_step(&sub);
        std::fs::write(out_dir.join(format!("face{index}.step")), text).unwrap();
    }
    let full_text = meshio::step::write_step(&model);
    std::fs::write(out_dir.join("wing_full.step"), full_text).unwrap();
    println!("wrote {} faces to {}", model.faces.len(), out_dir.display());
}
