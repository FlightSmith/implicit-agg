use aircraft_geom::section::TrailingEdgeSpec;
use aircraft_geom::wing::EvaluatedStation;
use aircraft_geom::{build_wing_mesh, ProfileCurve};
use std::sync::Arc;

#[test]
fn probe() {
    let curve = Arc::new(
        ProfileCurve::coordinates(&[[0.0, 0.0], [0.5, 0.05], [1.0, 0.0], [0.5, -0.05]]).unwrap(),
    );
    let mk = |id: &str, y: f64| EvaluatedStation {
        id: id.into(),
        position: [0.0, y, 0.0],
        chord: 1.5,
        twist: 0.0,
        trailing_edge: TrailingEdgeSpec::Sharp,
        curve: Arc::clone(&curve),
    };
    let stations = vec![mk("root", 0.0), mk("tip", -2.0)];
    let quality = aircraft_geom::ResolvedQuality {
        chord_samples: 8,
        span_subdivisions: vec![1],
    };
    let half = build_wing_mesh("w", &stations, true, &quality, true, false).unwrap();
    let v = half.mesh.validate();
    println!(
        "half: closed={} manifold={} oriented={} degen={}",
        v.closed, v.manifold, v.oriented, v.degenerate_triangles
    );
    let mut counts = std::collections::HashMap::new();
    for t in &half.mesh.triangles {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    for (k, c) in counts.iter().filter(|(_, c)| **c != 2) {
        println!(
            "edge {:?} count={c} verts={:?} {:?}",
            k, half.mesh.vertices[k.0 as usize], half.mesh.vertices[k.1 as usize]
        );
    }
}
