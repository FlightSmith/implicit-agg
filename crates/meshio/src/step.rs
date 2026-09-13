//! ISO 10303-21 (STEP) writer for the analytic wing model.
//!
//! Emits an AP214-style advanced B-rep: NURBS and planar surfaces, shared
//! edge topology, one closed shell per solid, and the minimal product
//! structure CAD readers expect. Geometry is written in metres.
use std::collections::HashMap;

use aircraft_geom::step_model::{StepCurve, StepFaceDef, StepModel, StepSurface};

struct Writer {
    out: String,
    next: usize,
    point_ids: std::collections::HashMap<[u64; 3], usize>,
    vertex_ids: HashMap<usize, usize>,
    edge_ids: HashMap<usize, usize>,
}

impl Writer {
    fn new(name: &str) -> Self {
        let mut writer = Writer {
            out: String::with_capacity(512 * 1024),
            next: 0,
            point_ids: std::collections::HashMap::new(),
            vertex_ids: std::collections::HashMap::new(),
            edge_ids: std::collections::HashMap::new(),
        };
        writer.out.push_str("ISO-10303-21;\nHEADER;\n");
        writer.out.push_str("FILE_DESCRIPTION((''),'2;1');\n");
        writer.out.push_str(&format!(
            "FILE_NAME('{name}.step','2026-01-01T00:00:00',(''),(''),('aircraft-workspace'),('aircraft-workspace'),(''));\n"
        ));
        writer.out.push_str(
            "FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\nENDSEC;\nDATA;\n",
        );
        writer
    }

    fn add(&mut self, entity: &str) -> usize {
        self.next += 1;
        let id = self.next;
        self.out.push_str(&format!("#{id} = {entity};\n"));
        id
    }

    fn point(&mut self, point: [f64; 3]) -> usize {
        let key = [point[0].to_bits(), point[1].to_bits(), point[2].to_bits()];
        if let Some(&id) = self.point_ids.get(&key) {
            return id;
        }
        let id = self.add(&format!(
            "CARTESIAN_POINT('',({},{},{}))",
            num(point[0]),
            num(point[1]),
            num(point[2])
        ));
        self.point_ids.insert(key, id);
        id
    }

    fn axis_placement(&mut self, origin: [f64; 3], z_dir: [f64; 3]) -> usize {
        let origin_point = self.point(origin);
        let z = self.direction(z_dir);
        // Reference direction: anything not parallel to z.
        let reference = if z_dir[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let x = self.direction(reference);
        self.add(&format!(
            "AXIS2_PLACEMENT_3D('',#{},#{},#{})",
            origin_point, z, x
        ))
    }

    fn direction(&mut self, direction: [f64; 3]) -> usize {
        self.add(&format!(
            "DIRECTION('',({},{},{}))",
            num(direction[0]),
            num(direction[1]),
            num(direction[2])
        ))
    }

    fn vertex_point(&mut self, model: &StepModel, vertex_index: usize) -> usize {
        if let Some(&id) = self.vertex_ids.get(&vertex_index) {
            return id;
        }
        let point = self.point(model.vertices[vertex_index]);
        let id = self.add(&format!("VERTEX_POINT('',#{point})"));
        self.vertex_ids.insert(vertex_index, id);
        id
    }

    fn curve(&mut self, curve: &StepCurve) -> usize {
        match curve {
            StepCurve::Line(start, end) => {
                let start_point = self.point(*start);
                let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
                let length =
                    (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
                let direction =
                    self.direction([delta[0] / length, delta[1] / length, delta[2] / length]);
                let vector = self.add(&format!("VECTOR('',#{direction},{})", num(length)));
                self.add(&format!("LINE('',#{start_point},#{vector})"))
            }
            StepCurve::Nurbs(nurbs) => {
                let point_ids: Vec<String> = nurbs
                    .controls
                    .iter()
                    .map(|&p| format!("#{}", self.point(homogeneous(p))))
                    .collect();
                let (mults, knots) = knot_groups(&nurbs.knots);
                self.add(&format!(
                    "B_SPLINE_CURVE_WITH_KNOTS('',{},({}),.UNSPECIFIED.,.F.,.F.,({}),({}),.UNSPECIFIED.)",
                    nurbs.degree,
                    point_ids.join(","),
                    ints(&mults),
                    reals(&knots),
                ))
            }
        }
    }

    fn nurbs_surface(&mut self, surface: &aircraft_geom::nurbs::NurbsSurface) -> usize {
        // Control list: outer index = u, inner = v.
        let rows: Vec<String> = surface
            .controls
            .iter()
            .map(|column| {
                column
                    .iter()
                    .map(|&p| format!("#{}", self.point(homogeneous(p))))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        let (u_mults, u_knots) = knot_groups(&surface.u_knots);
        let (v_mults, v_knots) = knot_groups(&surface.v_knots);
        self.add(&format!(
            "B_SPLINE_SURFACE_WITH_KNOTS('',({},{}),({}),.UNSPECIFIED.,.F.,.F.,({}),({}),({}),({}),.UNSPECIFIED.)",
            surface.u_degree,
            surface.v_degree,
            rows.join(","),
            ints(&u_mults),
            ints(&v_mults),
            reals(&u_knots),
            reals(&v_knots),
        ))
    }

    fn plane(&mut self, origin: [f64; 3], normal: [f64; 3]) -> usize {
        let placement = self.axis_placement(origin, normal);
        self.add(&format!("PLANE('',#{placement})"))
    }

    fn surface(&mut self, _model: &StepModel, face: &StepFaceDef) -> usize {
        match &face.surface {
            StepSurface::Nurbs(nurbs) => self.nurbs_surface(nurbs),
            StepSurface::Plane { origin, normal } => self.plane(*origin, *normal),
        }
    }

    fn edge(&mut self, model: &StepModel, index: usize) -> usize {
        if let Some(&id) = self.edge_ids.get(&index) {
            return id;
        }
        let edge = &model.edges[index];
        let start = self.vertex_point(model, edge.start);
        let end = self.vertex_point(model, edge.end);
        let curve = self.curve(&edge.curve);
        let id = self.add(&format!("EDGE_CURVE('',#{start},#{end},#{curve},.T.)"));
        self.edge_ids.insert(index, id);
        id
    }

    fn face(&mut self, model: &StepModel, face_index: usize) -> usize {
        let face: &StepFaceDef = &model.faces[face_index];
        let surface = self.surface(model, face);
        let mut oriented = Vec::with_capacity(face.loop_edges.len());
        for &(edge_index, reversed) in &face.loop_edges {
            let edge_id = self.edge(model, edge_index);
            oriented.push(format!(
                "ORIENTED_EDGE('',*,*,#{edge_id},{})",
                if reversed { ".F." } else { ".T." }
            ));
        }
        let loop_id = self.add(&format!("EDGE_LOOP('',({}))", oriented.join(",")));
        let bound = self.add(&format!("FACE_OUTER_BOUND('',#{loop_id},.T.)"));
        self.add(&format!("ADVANCED_FACE('',(#{bound}),#{surface},.T.)"))
    }

    fn shell(&mut self, model: &StepModel, faces: &[usize]) -> usize {
        let advanced: Vec<String> = faces
            .iter()
            .map(|&index| format!("#{}", self.face(model, index)))
            .collect();
        self.add(&format!("CLOSED_SHELL('',({}))", advanced.join(",")))
    }
}

/// Write the model as an ISO 10303-21 document (geometry in metres).
pub fn write_step(model: &StepModel) -> String {
    let mut writer = Writer::new(&model.name);

    // Unit/context preamble (metres, radians, standard uncertainty).
    writer.add("APPLICATION_CONTEXT('core data for automotive mechanical design processes')");
    let context = writer.add(
        "APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,#1)",
    );
    let length_unit = writer.add("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT($,.METRE.))");
    let _angle_unit = writer.add("(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))");
    let solid_angle_unit = writer.add("(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_UNIT())");
    let uncertainty = writer.add(
        "UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-07),#3,'distance_accuracy_value','validation')",
    );
    let context_block = writer.add(
        "(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#3,#4,#5))GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#6))REPRESENTATION_CONTEXT('',''))",
    );
    let _ = (
        context,
        length_unit,
        solid_angle_unit,
        uncertainty,
        context_block,
    );

    // Geometry.
    let shells: Vec<usize> = model
        .shells
        .iter()
        .map(|faces| writer.shell(model, faces))
        .collect();

    // Product structure.
    let product_context = writer.add("PRODUCT_DEFINITION_CONTEXT('part definition',#1,'design')");
    let product = writer.add(&format!(
        "PRODUCT('{}','{}','',(#{}))",
        model.name, model.name, product_context
    ));
    let formation = writer.add(&format!("PRODUCT_DEFINITION_FORMATION('','',#{product})"));
    let definition = writer.add(&format!(
        "PRODUCT_DEFINITION('design','',#{formation},#{product_context})"
    ));
    let definition_shape = writer.add(&format!("PRODUCT_DEFINITION_SHAPE('','',#{definition})"));

    let axis = writer.axis_placement([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let brep_ids: Vec<usize> = shells
        .iter()
        .map(|&shell| writer.add(&format!("MANIFOLD_SOLID_BREP('{}',#{shell})", model.name)))
        .collect();
    let items = format!(
        "#{axis},{}",
        brep_ids
            .iter()
            .map(|id| format!("#{id}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    let shape = writer.add(&format!(
        "ADVANCED_BREP_SHAPE_REPRESENTATION('{}',({items}),#{})",
        model.name, context_block
    ));
    writer.add(&format!(
        "SHAPE_DEFINITION_REPRESENTATION(#{definition_shape},#{shape})"
    ));
    let _ = axis;

    let mut out = writer.out;
    out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    out
}

fn homogeneous(point: [f64; 4]) -> [f64; 3] {
    [point[0], point[1], point[2]]
}

/// Collapse a clamped knot vector into (multiplicities, unique knots).
fn knot_groups(knots: &[f64]) -> (Vec<usize>, Vec<f64>) {
    let mut mults = Vec::new();
    let mut uniq = Vec::new();
    for (index, &knot) in knots.iter().enumerate() {
        if index > 0 && (knot - knots[index - 1]).abs() < 1e-12 {
            *mults.last_mut().expect("non-empty") += 1;
        } else {
            mults.push(1);
            uniq.push(knot);
        }
    }
    (mults, uniq)
}

fn ints(values: &[usize]) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn reals(values: &[f64]) -> String {
    values.iter().map(|&v| num(v)).collect::<Vec<_>>().join(",")
}

/// STEP real: decimal point, signed exponent with two digits.
fn num(value: f64) -> String {
    if value == 0.0 {
        return "0.E0".to_string();
    }
    let formatted = format!("{:.12E}", value);
    let (mantissa, exponent) = formatted.split_once('E').expect("E format");
    let mantissa = mantissa.trim_end_matches('0');
    let mantissa = if mantissa.ends_with('.') {
        format!("{mantissa}0")
    } else {
        mantissa.to_string()
    };
    let exponent_value: i32 = exponent.parse().expect("exponent");
    format!("{mantissa}E{exponent_value:+03}")
}
