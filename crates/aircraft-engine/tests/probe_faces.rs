#![allow(clippy::unwrap_used)]
use aircraft_engine::Engine;
use aircraft_geom::step_model::StepModel;
use aircraft_model::parse_document;

#[test]
fn write_each_face() {
    let (doc, _) =
        parse_document(&std::fs::read_to_string("/dev/shm/cranked-wing-study-001.json").unwrap())
            .unwrap();
    let engine = Engine::open(doc).unwrap();
    let symmetry = true;
    let (_, stations) = engine.evaluated_stations(0).unwrap();

    std::fs::create_dir_all("/tmp/faces").unwrap();
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
        std::fs::write(format!("/tmp/faces/face{index}.step"), text).unwrap();
    }
    let full_text = meshio::step::write_step(&model);
    std::fs::write("/tmp/faces/wing_full.step", full_text).unwrap();
    println!("wrote {} faces", model.faces.len());
}
