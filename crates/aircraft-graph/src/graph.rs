//! The typed dependency graph over an aircraft document.
//!
//! Every raw parameter and typed numeric field becomes a node; references in
//! expressions become edges. Building resolves and dimension-checks every
//! expression, infers parameter dimensions from their consumers, and rejects
//! cycles before any evaluation. Evaluation is a deterministic topological
//! walk in canonical units (meters, radians), and can be re-run for only the
//! descendants of edited nodes.

use crate::node::{FieldKind, Node, NodeId, NodeKind, ValueSource};
use aircraft_expr::{
    check, evaluate, parse_field_expression, Coordinate, Dim, Dimension, EvalContext, EvalError,
    Ref, RefResolver, RefValues,
};
use aircraft_model::aircraft::{station_path, Component, Interface, Station, Wing};
use aircraft_model::{Code, Diagnostic, TrailingEdge, TypedValue, UnitSystem};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub use aircraft_expr::Value;

/// Per parameter: the dimensions demanded by each consuming field, with the
/// consumer's subject for conflict diagnostics.
type ParameterConstraints = HashMap<String, Vec<(Dimension, String)>>;

/// A built graph. Construct with [`build`].
#[derive(Debug, Clone)]
pub struct Graph {
    pub nodes: Vec<Node>,
    /// For each node, the indices of nodes that consume it.
    pub consumers: Vec<Vec<NodeId>>,
    /// Deterministic evaluation order (dependencies first).
    pub order: Vec<NodeId>,
    param_nodes: BTreeMap<String, NodeId>,
    station_nodes: HashMap<(usize, String), StationNodes>,
    component_stations: HashMap<usize, Vec<String>>,
}

#[derive(Debug, Clone, Copy)]
struct StationNodes {
    position: [NodeId; 3],
    chord: NodeId,
    twist: NodeId,
    trailing_edge: Option<NodeId>,
}

/// The result of evaluating a graph against a document.
#[derive(Debug, Clone)]
pub struct Evaluation {
    /// Canonical-unit value per node index.
    pub values: Vec<f64>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Build the graph for a document. Returns errors for unresolvable
/// references, cycles, expression syntax problems, and conflicting parameter
/// dimensions; unreferenced parameters come back as warnings.
pub fn build(
    doc: &aircraft_model::AircraftDefinition,
) -> Result<(Graph, Vec<Diagnostic>), Vec<Diagnostic>> {
    let mut builder = Builder::new(doc);
    builder.create_nodes();
    if !builder.errors.is_empty() {
        return Err(std::mem::take(&mut builder.errors));
    }
    let constraints = builder.resolve_dependencies()?;
    let warnings = builder.infer_parameter_dimensions(constraints)?;
    if let Some(cycle) = builder.find_cycle() {
        return Err(vec![Diagnostic::error(
            Code::CycleDetected,
            format!("dependency cycle detected: {}", builder.cycle_chain(&cycle)),
        )]);
    }
    let order = builder.topological_order();
    let consumers = builder.build_consumers();
    let graph = Graph {
        nodes: builder.nodes,
        consumers,
        order,
        param_nodes: builder.param_nodes,
        station_nodes: builder.station_nodes,
        component_stations: builder.component_stations,
    };
    Ok((graph, warnings))
}

impl Graph {
    pub fn parameter_node(&self, id: &str) -> Option<NodeId> {
        self.param_nodes.get(id).copied()
    }

    pub fn station_field_node(
        &self,
        component_index: usize,
        station_index: usize,
        field: FieldKind,
    ) -> Option<NodeId> {
        let stations = self.component_stations.get(&component_index)?;
        let station_id = stations.get(station_index)?;
        let nodes = self
            .station_nodes
            .get(&(component_index, station_id.clone()))?;
        match field {
            FieldKind::PositionX => Some(nodes.position[0]),
            FieldKind::PositionY => Some(nodes.position[1]),
            FieldKind::PositionZ => Some(nodes.position[2]),
            FieldKind::Chord => Some(nodes.chord),
            FieldKind::Twist => Some(nodes.twist),
            FieldKind::TrailingEdgeThickness | FieldKind::TrailingEdgeFraction => {
                nodes.trailing_edge
            }
        }
    }

    /// All nodes transitively consumed by `seeds`, including the seeds
    /// themselves. Deterministic order.
    pub fn descendants(&self, seeds: &[NodeId]) -> BTreeSet<NodeId> {
        let mut affected: BTreeSet<NodeId> = seeds.iter().copied().collect();
        let mut queue: Vec<NodeId> = seeds.to_vec();
        while let Some(node) = queue.pop() {
            for &consumer in &self.consumers[node] {
                if affected.insert(consumer) {
                    queue.push(consumer);
                }
            }
        }
        affected
    }
}

/// Evaluate a graph fully.
pub fn evaluate_graph(graph: &Graph, doc: &aircraft_model::AircraftDefinition) -> Evaluation {
    let mut state = EvalState {
        values: vec![f64::NAN; graph.nodes.len()],
        poisoned: vec![false; graph.nodes.len()],
        diagnostics: Vec::new(),
    };
    for &node in &graph.order {
        compute_node(graph, node, doc, &mut state, &doc.units);
    }
    Evaluation {
        values: state.values,
        diagnostics: state.diagnostics,
    }
}

/// Re-evaluate only the descendants of `seeds` (the seeds included), reusing
/// the previous evaluation's values for everything else. Produces the same
/// values as a full recompute for acyclic graphs.
pub fn evaluate_incremental(
    graph: &Graph,
    doc: &aircraft_model::AircraftDefinition,
    previous: &Evaluation,
    seeds: &[NodeId],
) -> Evaluation {
    let affected = graph.descendants(seeds);
    let mut state = EvalState {
        values: previous.values.clone(),
        poisoned: vec![false; graph.nodes.len()],
        diagnostics: Vec::new(),
    };
    for &node in &graph.order {
        if affected.contains(&node) {
            compute_node(graph, node, doc, &mut state, &doc.units);
        }
    }
    Evaluation {
        values: state.values,
        diagnostics: state.diagnostics,
    }
}

struct EvalState {
    values: Vec<f64>,
    poisoned: Vec<bool>,
    diagnostics: Vec<Diagnostic>,
}

fn compute_node(
    graph: &Graph,
    node: NodeId,
    doc: &aircraft_model::AircraftDefinition,
    state: &mut EvalState,
    units: &UnitSystem,
) {
    let node_data = &graph.nodes[node];

    // Parameter nodes read their raw value from the document being evaluated,
    // so an edited document re-evaluates without rebuilding the graph.
    if let NodeKind::Parameter { id } = &node_data.kind {
        let raw = doc.parameters.get(id).copied().unwrap_or(f64::NAN);
        let value = canonical_value(raw, node_data.dimension, units);
        if value.is_finite() {
            state.values[node] = value;
        } else {
            push_error(
                state,
                node,
                node_data,
                Code::NonFiniteResult,
                "value is not finite",
            );
        }
        return;
    }

    // If any dependency failed, this node fails silently: the root cause
    // already produced a diagnostic.
    if node_data
        .deps
        .iter()
        .any(|&dep| state.poisoned[dep] || state.values[dep].is_nan())
    {
        poison(state, node);
        return;
    }

    let value = match &node_data.source {
        ValueSource::Raw(raw) => canonical_value(*raw, node_data.dimension, units),
        ValueSource::Literal(literal) => canonical_value(*literal, node_data.dimension, units),
        ValueSource::Parameter(id) => {
            let param = graph
                .param_nodes
                .get(id)
                .copied()
                .expect("parameter node exists after build");
            state.values[param]
        }
        ValueSource::Expression(checked) => {
            let refs = ExpressionRefValues {
                graph,
                values: &state.values,
                node,
            };
            let ctx = EvalContext { units, refs: &refs };
            match evaluate(&checked.expr, &ctx) {
                Ok(Value::Number(number)) => number,
                Ok(Value::Bool(_)) => {
                    push_error(
                        state,
                        node,
                        node_data,
                        Code::DimensionMismatch,
                        "expression produced a boolean but this field needs a number",
                    );
                    return;
                }
                Err(error) => {
                    push_error(
                        state,
                        node,
                        node_data,
                        Code::NonFiniteResult,
                        format!("expression failed to evaluate: {error}"),
                    );
                    return;
                }
            }
        }
        ValueSource::Unresolved(_) => {
            push_error(
                state,
                node,
                node_data,
                Code::Internal,
                "expression was never resolved",
            );
            return;
        }
    };

    if !value.is_finite() {
        push_error(
            state,
            node,
            node_data,
            Code::NonFiniteResult,
            "value is not finite",
        );
        return;
    }
    state.values[node] = value;
}

fn poison(state: &mut EvalState, node: NodeId) {
    state.poisoned[node] = true;
    state.values[node] = f64::NAN;
}

fn push_error(
    state: &mut EvalState,
    node: NodeId,
    node_data: &Node,
    code: Code,
    message: impl Into<String>,
) {
    poison(state, node);
    state.diagnostics.push(
        Diagnostic::error(code, message)
            .with_path(node_data.path.clone())
            .with_subject(node_data.subject.clone()),
    );
}

/// Convert a document-unit number into canonical units for its dimension.
fn canonical_value(raw: f64, dimension: Option<Dimension>, units: &UnitSystem) -> f64 {
    match dimension {
        Some(d) if d == Dimension::LENGTH => units.length_to_canonical(raw),
        Some(d) if d == Dimension::ANGLE => units.angle_to_canonical(raw),
        _ => raw,
    }
}

struct ExpressionRefValues<'a> {
    graph: &'a Graph,
    values: &'a [f64],
    node: NodeId,
}

impl RefValues for ExpressionRefValues<'_> {
    fn value_of(&self, reference: &Ref) -> Result<f64, EvalError> {
        let (_, producer) = self.graph.nodes[self.node]
            .ref_nodes
            .iter()
            .find(|(known, _)| known == reference)
            .ok_or_else(|| EvalError::Reference(reference.to_string()))?;
        Ok(self.values[*producer])
    }
}

struct Builder<'a> {
    doc: &'a aircraft_model::AircraftDefinition,
    nodes: Vec<Node>,
    param_nodes: BTreeMap<String, NodeId>,
    /// Per wing component: station id -> its field nodes.
    station_nodes: HashMap<(usize, String), StationNodes>,
    /// Per component index: ordered station ids.
    component_stations: HashMap<usize, Vec<String>>,
    errors: Vec<Diagnostic>,
}

impl<'a> Builder<'a> {
    fn new(doc: &'a aircraft_model::AircraftDefinition) -> Self {
        Builder {
            doc,
            nodes: Vec::new(),
            param_nodes: BTreeMap::new(),
            station_nodes: HashMap::new(),
            component_stations: HashMap::new(),
            errors: Vec::new(),
        }
    }

    fn create_nodes(&mut self) {
        for (id, value) in &self.doc.parameters {
            let index = self.nodes.len();
            self.nodes.push(Node {
                kind: NodeKind::Parameter { id: id.clone() },
                subject: format!("parameter {id}"),
                path: format!("parameters/{id}"),
                short_name: format!("param:{id}"),
                dimension: None,
                source: ValueSource::Raw(*value),
                ref_nodes: Vec::new(),
                deps: Vec::new(),
            });
            self.param_nodes.insert(id.clone(), index);
        }

        for (component_index, component) in self.doc.components.iter().enumerate() {
            let Component::Wing(wing) = component;
            let mut station_ids = Vec::new();
            self.create_frame_origin_nodes(component_index, wing);
            for (station_index, station) in wing.stations.iter().enumerate() {
                self.create_station_nodes(component_index, station_index, station);
                station_ids.push(station.id.clone());
            }
            self.component_stations.insert(component_index, station_ids);
        }
    }

    fn create_frame_origin_nodes(&mut self, component_index: usize, wing: &Wing) {
        for (axis, coordinate) in [
            ("x", Coordinate::X),
            ("y", Coordinate::Y),
            ("z", Coordinate::Z),
        ] {
            let typed = match coordinate {
                Coordinate::X => &wing.frame.origin.x,
                Coordinate::Y => &wing.frame.origin.y,
                Coordinate::Z => &wing.frame.origin.z,
            };
            let path = format!("components/{component_index}/frame/origin/{axis}");
            self.push_field_node(
                NodeKind::FrameOrigin {
                    component_index,
                    coordinate,
                },
                format!("{} / frame origin {axis}", wing.id),
                path,
                format!("frame.origin.{axis}"),
                Dimension::LENGTH,
                typed,
            );
        }
    }

    fn create_station_nodes(
        &mut self,
        component_index: usize,
        station_index: usize,
        station: &Station,
    ) {
        let base = station_path(component_index, station_index);
        let wing_id = self.wing_id(component_index);
        let subject = format!("{wing_id} / station {}", station.id);

        let mut position = [0usize; 3];
        for (axis_index, (axis, coordinate)) in [
            ("x", Coordinate::X),
            ("y", Coordinate::Y),
            ("z", Coordinate::Z),
        ]
        .into_iter()
        .enumerate()
        {
            let typed = match coordinate {
                Coordinate::X => &station.position.x,
                Coordinate::Y => &station.position.y,
                Coordinate::Z => &station.position.z,
            };
            position[axis_index] = self.push_field_node(
                NodeKind::StationField {
                    component_index,
                    station_index,
                    field: match coordinate {
                        Coordinate::X => FieldKind::PositionX,
                        Coordinate::Y => FieldKind::PositionY,
                        _ => FieldKind::PositionZ,
                    },
                },
                format!("{subject} / position {axis}"),
                format!("{base}/position/{axis}"),
                format!("{}.position.{axis}", station.id),
                Dimension::LENGTH,
                typed,
            );
        }

        let chord = self.push_field_node(
            NodeKind::StationField {
                component_index,
                station_index,
                field: FieldKind::Chord,
            },
            format!("{subject} / chord"),
            format!("{base}/chord"),
            format!("{}.chord", station.id),
            Dimension::LENGTH,
            &station.chord,
        );

        let twist = self.push_field_node(
            NodeKind::StationField {
                component_index,
                station_index,
                field: FieldKind::Twist,
            },
            format!("{subject} / twist"),
            format!("{base}/twist"),
            format!("{}.twist", station.id),
            Dimension::ANGLE,
            &station.twist,
        );

        let trailing_edge = match station.trailing_edge() {
            TrailingEdge::Sharp => None,
            TrailingEdge::Absolute { thickness } => Some(self.push_field_node(
                NodeKind::StationField {
                    component_index,
                    station_index,
                    field: FieldKind::TrailingEdgeThickness,
                },
                format!("{subject} / trailing edge thickness"),
                format!("{base}/trailingEdge/thickness"),
                format!("{}.trailing-edge-thickness", station.id),
                Dimension::LENGTH,
                thickness,
            )),
            TrailingEdge::ChordFraction { value } => Some(self.push_field_node(
                NodeKind::StationField {
                    component_index,
                    station_index,
                    field: FieldKind::TrailingEdgeFraction,
                },
                format!("{subject} / trailing edge fraction"),
                format!("{base}/trailingEdge/value"),
                format!("{}.trailing-edge-fraction", station.id),
                Dimension::RATIO,
                value,
            )),
        };

        self.station_nodes.insert(
            (component_index, station.id.clone()),
            StationNodes {
                position,
                chord,
                twist,
                trailing_edge,
            },
        );
    }

    fn push_field_node(
        &mut self,
        kind: NodeKind,
        subject: String,
        path: String,
        short_name: String,
        dimension: Dimension,
        typed: &TypedValue,
    ) -> NodeId {
        let source = match typed {
            TypedValue::Number(value) => ValueSource::Literal(*value),
            TypedValue::ParamRef { param } => ValueSource::Parameter(param.clone()),
            TypedValue::Expression(text) => match parse_field_expression(text) {
                Ok(expr) => ValueSource::Unresolved(expr),
                Err(diagnostic) => {
                    self.errors.push(
                        diagnostic
                            .with_path(path.clone())
                            .with_subject(subject.clone()),
                    );
                    ValueSource::Literal(f64::NAN)
                }
            },
        };
        let index = self.nodes.len();
        self.nodes.push(Node {
            kind,
            subject,
            path,
            short_name,
            dimension: Some(dimension),
            source,
            ref_nodes: Vec::new(),
            deps: Vec::new(),
        });
        index
    }

    fn wing_id(&self, component_index: usize) -> String {
        match &self.doc.components[component_index] {
            Component::Wing(wing) => wing.id.clone(),
        }
    }

    /// Resolve a reference to the node that produces its value.
    fn resolve_node(&self, reference: &Ref) -> Result<NodeId, Diagnostic> {
        match reference {
            Ref::Parameter(id) => self.param_nodes.get(id).copied().ok_or_else(|| {
                Diagnostic::error(Code::UnknownReference, format!("unknown parameter {id:?}"))
            }),
            Ref::Station { station, leaf } => {
                // Station references resolve within the owning component;
                // v0.1 documents have a single wing, index 0.
                let component_index = 0;
                let nodes = self
                    .station_nodes
                    .get(&(component_index, station.clone()))
                    .ok_or_else(|| {
                        Diagnostic::error(
                            Code::UnknownReference,
                            format!("unknown station {station:?}"),
                        )
                    })?;
                Ok(match leaf {
                    aircraft_expr::StationLeaf::PositionX => nodes.position[0],
                    aircraft_expr::StationLeaf::PositionY => nodes.position[1],
                    aircraft_expr::StationLeaf::PositionZ => nodes.position[2],
                    aircraft_expr::StationLeaf::Chord => nodes.chord,
                    aircraft_expr::StationLeaf::Twist => nodes.twist,
                })
            }
            Ref::InterfaceOrigin {
                component,
                interface,
                coordinate,
            } => {
                let component_index = self
                    .doc
                    .components
                    .iter()
                    .position(|c| c.id() == component)
                    .ok_or_else(|| {
                        Diagnostic::error(
                            Code::UnknownReference,
                            format!("unknown component {component:?}"),
                        )
                    })?;
                let Component::Wing(wing) = &self.doc.components[component_index];
                let Some(Interface::StationPlane { station }) = wing.interfaces.get(interface)
                else {
                    return Err(Diagnostic::error(
                        Code::UnknownReference,
                        format!(
                            "component {component:?} has no station-plane interface named \
                             {interface:?}"
                        ),
                    ));
                };
                let nodes = self
                    .station_nodes
                    .get(&(component_index, station.clone()))
                    .ok_or_else(|| {
                        Diagnostic::error(
                            Code::UnknownReference,
                            format!("interface references unknown station {station:?}"),
                        )
                    })?;
                Ok(match coordinate {
                    Coordinate::X => nodes.position[0],
                    Coordinate::Y => nodes.position[1],
                    Coordinate::Z => nodes.position[2],
                })
            }
        }
    }

    /// Second pass: check every expression now that all nodes exist, wire
    /// dependency edges, and collect per-parameter dimension constraints.
    fn resolve_dependencies(&mut self) -> Result<ParameterConstraints, Vec<Diagnostic>> {
        let mut constraints: ParameterConstraints = HashMap::new();

        for index in 0..self.nodes.len() {
            enum Pending {
                Expr(aircraft_expr::Expr),
                Param(String),
                None,
            }
            let pending = match &self.nodes[index].source {
                ValueSource::Unresolved(expr) => Pending::Expr(expr.clone()),
                ValueSource::Parameter(id) => Pending::Param(id.clone()),
                _ => Pending::None,
            };
            let unresolved = match pending {
                Pending::Expr(expr) => Some(expr),
                Pending::Param(id) => {
                    let mut handled = false;
                    if let Some(&param) = self.param_nodes.get(&id) {
                        let expected = self.nodes[index].expected_dimension();
                        let subject = self.nodes[index].subject.clone();
                        self.nodes[index].deps.push(param);
                        constraints
                            .entry(id.clone())
                            .or_default()
                            .push((expected, subject));
                        handled = true;
                    }
                    if !handled {
                        let path = self.nodes[index].path.clone();
                        let subject = self.nodes[index].subject.clone();
                        self.errors.push(
                            Diagnostic::error(
                                Code::UnknownReference,
                                format!("unknown parameter {id:?}"),
                            )
                            .with_path(path)
                            .with_subject(subject),
                        );
                    }
                    None
                }
                Pending::None => None,
            };

            let Some(expr) = unresolved else {
                continue;
            };
            let expected = self.nodes[index].expected_dimension();
            let subject = self.nodes[index].subject.clone();
            let mut resolver = GraphResolver { builder: self };
            match check(&expr, Dim::Of(expected), &mut resolver) {
                Ok(checked) => {
                    for (reference, dim) in &checked.references {
                        if let Ref::Parameter(id) = reference {
                            // A concrete occurrence demands its own dimension;
                            // an unconstrained one inherits the field's.
                            let demanded = match dim {
                                Dim::Of(dimension) => Some(*dimension),
                                Dim::Any => Some(expected),
                                Dim::Bool => None,
                            };
                            if let Some(dimension) = demanded {
                                constraints
                                    .entry(id.clone())
                                    .or_default()
                                    .push((dimension, subject.clone()));
                            }
                        }
                    }
                    let mut producers = Vec::with_capacity(checked.references.len());
                    for (reference, _) in &checked.references {
                        match self.resolve_node(reference) {
                            Ok(node) => producers.push((reference.clone(), node)),
                            Err(diagnostic) => self.errors.push(diagnostic),
                        }
                    }
                    if !self.errors.is_empty() {
                        return Err(std::mem::take(&mut self.errors));
                    }
                    self.nodes[index].deps = producers.iter().map(|(_, node)| *node).collect();
                    self.nodes[index].ref_nodes = producers;
                    self.nodes[index].source = ValueSource::Expression(checked);
                }
                Err(mut diagnostic) => {
                    diagnostic
                        .path
                        .get_or_insert_with(|| self.nodes[index].path.clone());
                    diagnostic
                        .subject
                        .get_or_insert_with(|| self.nodes[index].subject.clone());
                    self.errors.push(diagnostic);
                }
            }
        }

        if !self.errors.is_empty() {
            return Err(std::mem::take(&mut self.errors));
        }
        Ok(constraints)
    }

    /// Unify each parameter's demanded dimensions; conflicts and unreferenced
    /// parameters become diagnostics.
    fn infer_parameter_dimensions(
        &mut self,
        constraints: ParameterConstraints,
    ) -> Result<Vec<Diagnostic>, Vec<Diagnostic>> {
        let mut warnings = Vec::new();

        for (id, index) in &self.param_nodes {
            let Some(demands) = constraints.get(id) else {
                warnings.push(
                    Diagnostic::warning(
                        Code::UnknownReference,
                        format!("parameter {id:?} is not referenced by any field"),
                    )
                    .with_path(format!("parameters/{id}"))
                    .with_subject(format!("parameter {id}")),
                );
                continue;
            };
            let mut dimension: Option<Dimension> = None;
            for (demanded, consumer) in demands {
                match dimension {
                    None => dimension = Some(*demanded),
                    Some(existing) if existing == *demanded => {}
                    Some(existing) => {
                        self.errors.push(
                            Diagnostic::error(
                                Code::DimensionMismatch,
                                format!(
                                    "parameter {id:?} is used as {} by {consumer:?} but as {} \
                                     elsewhere",
                                    demanded.describe(),
                                    existing.describe()
                                ),
                            )
                            .with_path(format!("parameters/{id}"))
                            .with_subject(format!("parameter {id}")),
                        );
                    }
                }
            }
            self.nodes[*index].dimension = dimension;
        }

        if !self.errors.is_empty() {
            return Err(std::mem::take(&mut self.errors));
        }
        Ok(warnings)
    }

    fn find_cycle(&self) -> Option<Vec<NodeId>> {
        const WHITE: u8 = 0;
        const GRAY: u8 = 1;
        const BLACK: u8 = 2;
        let mut color = vec![WHITE; self.nodes.len()];

        for start in 0..self.nodes.len() {
            if color[start] != WHITE {
                continue;
            }
            let mut chain: Vec<NodeId> = vec![start];
            let mut stack: Vec<(NodeId, usize)> = vec![(start, 0)];
            color[start] = GRAY;
            while let Some(frame) = stack.last_mut() {
                let node = frame.0;
                if frame.1 < self.nodes[node].deps.len() {
                    let dep = self.nodes[node].deps[frame.1];
                    frame.1 += 1;
                    match color[dep] {
                        GRAY => {
                            let position = chain.iter().position(|&n| n == dep)?;
                            let mut cycle: Vec<NodeId> = chain[position..].to_vec();
                            cycle.push(dep);
                            return Some(cycle);
                        }
                        WHITE => {
                            color[dep] = GRAY;
                            chain.push(dep);
                            stack.push((dep, 0));
                        }
                        _ => {}
                    }
                } else {
                    color[node] = BLACK;
                    chain.pop();
                    stack.pop();
                }
            }
        }
        None
    }

    fn cycle_chain(&self, cycle: &[NodeId]) -> String {
        cycle
            .iter()
            .map(|&node| self.nodes[node].short_name.as_str())
            .collect::<Vec<_>>()
            .join(" -> ")
    }

    fn topological_order(&self) -> Vec<NodeId> {
        let consumers = self.build_consumers();
        let mut pending_deps: Vec<usize> = self.nodes.iter().map(|node| node.deps.len()).collect();
        let mut ready: BTreeSet<(String, NodeId)> = BTreeSet::new();
        for (index, pending) in pending_deps.iter().enumerate() {
            if *pending == 0 {
                ready.insert((self.nodes[index].subject.clone(), index));
            }
        }
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some((_, node)) = ready.pop_first() {
            order.push(node);
            for &consumer in &consumers[node] {
                pending_deps[consumer] -= 1;
                if pending_deps[consumer] == 0 {
                    ready.insert((self.nodes[consumer].subject.clone(), consumer));
                }
            }
        }
        order
    }

    fn build_consumers(&self) -> Vec<Vec<NodeId>> {
        let mut consumers = vec![Vec::new(); self.nodes.len()];
        for (index, node) in self.nodes.iter().enumerate() {
            for &dep in &node.deps {
                consumers[dep].push(index);
            }
        }
        consumers
    }
}

/// Resolves references against the builder's node maps while checking.
struct GraphResolver<'r, 'b> {
    builder: &'r mut Builder<'b>,
}

impl RefResolver for GraphResolver<'_, '_> {
    fn dimension_of(&mut self, reference: &Ref) -> Result<Dim, Diagnostic> {
        let node = self.builder.resolve_node(reference)?;
        match self.builder.nodes[node].dimension {
            Some(dimension) => Ok(Dim::Of(dimension)),
            // Parameters are dimensionless until constrained; inference
            // binds them to their consumers afterwards.
            None => Ok(Dim::Any),
        }
    }
}
