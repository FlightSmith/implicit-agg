//! The aircraft engine: the service API over one document. Immutable
//! snapshots feed a transactional patch/commit cycle, geometry jobs carry
//! cancellation tokens and revision checks, and mesh artifacts are cached by
//! content hash. The engine never depends on UI state.

pub mod report;

use aircraft_geom::quality::MeshQuality;
use aircraft_geom::section::TrailingEdgeSpec;
use aircraft_geom::wing::EvaluatedStation;
use aircraft_geom::{build_wing_mesh, FaceSource, ProfileCurve, WingMesh};
use aircraft_graph::graph::evaluate_incremental;
use aircraft_graph::{
    build as build_graph, check_predicates, evaluate_graph, Evaluation, FieldKind, Graph,
};
use aircraft_model::aircraft::{Component, TrailingEdge};
use aircraft_model::TypedValue;
use aircraft_model::{semantic, Code, Diagnostic};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Bumped when geometry output could change for identical inputs.
pub const GEOMETRY_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionId(pub u64);

/// A source edit offered to the engine. Invalid patches leave the committed
/// snapshot untouched.
#[derive(Debug, Clone)]
pub enum Patch {
    SetParameter {
        id: String,
        value: f64,
    },
    /// Create a named raw scalar. The id must be unique and well-formed; its
    /// dimension is inferred from the first field that consumes it.
    AddParameter {
        id: String,
        value: f64,
    },
    /// Set or clear a station's leading-edge tangency.
    SetStationTangency {
        component_index: usize,
        station_index: usize,
        tangency: Option<aircraft_model::StationTangency>,
    },
    SetStationField {
        component_index: usize,
        station_index: usize,
        field: FieldKind,
        value: TypedValue,
    },
}

#[derive(Debug, Clone)]
pub struct UpdateResult {
    pub transaction: TransactionId,
    pub committed: bool,
    pub revision: u64,
    pub diagnostics: Vec<Diagnostic>,
    /// Subjects of the graph nodes affected by the committed patch.
    pub affected: Vec<String>,
}

/// Cooperative cancellation for geometry jobs.
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone)]
pub enum MeshJobError {
    Cancelled,
    Stale { current: u64, expected: u64 },
    Failed(Vec<Diagnostic>),
}

/// A generated mesh for one wing, in aircraft coordinates, with its
/// face-to-source map for selection tracing.
#[derive(Clone, Debug)]
pub struct MeshArtifact {
    pub revision: u64,
    pub wing_id: String,
    /// Station ids index-aligned with [`FaceSource`] station indices.
    pub station_ids: Vec<String>,
    pub mesh: aircraft_geom::Mesh,
    pub faces: Vec<FaceSource>,
}

impl MeshArtifact {
    /// Human-readable trace of a mesh triangle back to its source.
    pub fn trace(&self, triangle_index: usize) -> Result<SourceTrace, Diagnostic> {
        let face = self.faces.get(triangle_index).ok_or_else(|| {
            Diagnostic::error(
                Code::MeshFailure,
                format!("triangle index {triangle_index} is outside the mesh"),
            )
        })?;
        let station_name = |index: usize| {
            self.station_ids
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("station {index}"))
        };
        Ok(match *face {
            FaceSource::Panel {
                lower_station,
                upper_station,
                mirrored,
            } => SourceTrace {
                wing_id: self.wing_id.clone(),
                stations: vec![station_name(lower_station), station_name(upper_station)],
                mirrored,
                description: format!(
                    "loft panel between stations {} and {}",
                    station_name(lower_station),
                    station_name(upper_station)
                ),
            },
            FaceSource::TipCap { station, mirrored } => SourceTrace {
                wing_id: self.wing_id.clone(),
                stations: vec![station_name(station)],
                mirrored,
                description: format!("tip cap of station {}", station_name(station)),
            },
            FaceSource::RootCap => SourceTrace {
                wing_id: self.wing_id.clone(),
                stations: Vec::new(),
                mirrored: false,
                description: "symmetry-plane cap of the half model".to_string(),
            },
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceTrace {
    pub wing_id: String,
    pub stations: Vec<String>,
    pub mirrored: bool,
    pub description: String,
}

#[derive(Clone)]
pub struct Engine {
    doc: aircraft_model::AircraftDefinition,
    graph: Graph,
    evaluation: Evaluation,
    revision: u64,
    mesh_cache: HashMap<u64, Arc<MeshArtifact>>,
}

impl Engine {
    /// Open a document: validate semantics, build the graph, evaluate, and
    /// check physical predicates. Any error aborts the open.
    pub fn open(doc: aircraft_model::AircraftDefinition) -> Result<Engine, Vec<Diagnostic>> {
        let mut diagnostics = semantic::validate(&doc);
        if diagnostics.iter().any(Diagnostic::is_error) {
            return Err(diagnostics);
        }
        let (graph, warnings) = build_graph(&doc)?;
        diagnostics.extend(warnings);
        let evaluation = evaluate_graph(&graph, &doc);
        diagnostics.extend(evaluation.diagnostics.clone());
        diagnostics.extend(check_predicates(&graph, &doc, &evaluation));
        if diagnostics.iter().any(Diagnostic::is_error) {
            return Err(diagnostics);
        }
        Ok(Engine {
            doc,
            graph,
            evaluation,
            revision: 1,
            mesh_cache: HashMap::new(),
        })
    }

    pub fn document(&self) -> &aircraft_model::AircraftDefinition {
        &self.doc
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Restore a prior snapshot (undo): the engine is a plain value, so the
    /// caller swaps in a previously cloned engine.
    pub fn restore(&mut self, engine: Engine) {
        self.doc = engine.doc;
        self.graph = engine.graph;
        self.evaluation = engine.evaluation;
        self.revision = engine.revision;
        self.mesh_cache = engine.mesh_cache;
    }

    /// Apply a patch transactionally: the candidate document is validated and
    /// evaluated in full; on any error the previous committed snapshot stays.
    pub fn apply_patch(&mut self, patch: Patch, transaction: TransactionId) -> UpdateResult {
        let mut candidate = self.doc.clone();
        if let Err(diagnostic) = apply_to_document(&mut candidate, &patch) {
            return self.reject(transaction, diagnostic);
        }

        // Validate and evaluate the candidate as a whole; seeds resolve
        // against the newly built graph because indices may have moved.
        let mut diagnostics = semantic::validate(&candidate);
        let (graph, warnings) = match build_graph(&candidate) {
            Ok(built) => built,
            Err(errors) => {
                return self.reject(transaction, errors.into_iter().next().expect("non-empty"))
            }
        };
        diagnostics.extend(warnings);
        let seeds = resolve_seeds(&graph, &patch);
        let affected = graph.descendants(&seeds);
        let evaluation = evaluate_graph(&graph, &candidate);
        diagnostics.extend(evaluation.diagnostics.clone());
        diagnostics.extend(check_predicates(&graph, &candidate, &evaluation));
        if let Some(error) = diagnostics.iter().find(|d| d.is_error()) {
            return self.reject(transaction, error.clone());
        }

        self.doc = candidate;
        self.graph = graph;
        self.evaluation = evaluation;
        self.revision += 1;
        self.mesh_cache.clear();
        UpdateResult {
            transaction,
            committed: true,
            revision: self.revision,
            diagnostics: Vec::new(),
            affected: affected
                .iter()
                .filter_map(|node| self.graph.nodes.get(*node).map(|n| n.subject.clone()))
                .collect(),
        }
    }

    /// Re-evaluate only the descendants of `seeds` against a candidate
    /// document, reusing this snapshot's values elsewhere. The test suite
    /// checks this against the full-recompute oracle.
    pub fn evaluate_incremental_from(
        &self,
        candidate: &aircraft_model::AircraftDefinition,
        seeds: &[usize],
    ) -> Evaluation {
        evaluate_incremental(&self.graph, candidate, &self.evaluation, seeds)
    }

    fn reject(&self, transaction: TransactionId, diagnostic: Diagnostic) -> UpdateResult {
        UpdateResult {
            transaction,
            committed: false,
            revision: self.revision,
            diagnostics: vec![diagnostic],
            affected: Vec::new(),
        }
    }

    /// Evaluated stations of one wing in wing-local canonical units.
    pub fn evaluated_stations(
        &self,
        component_index: usize,
    ) -> Result<(String, Vec<EvaluatedStation>), Vec<Diagnostic>> {
        let Component::Wing(wing) = self
            .doc
            .components
            .get(component_index)
            .ok_or_else(|| vec![unknown_component(component_index)])?;

        let mut errors = Vec::new();
        let mut stations = Vec::with_capacity(wing.stations.len());
        for (station_index, station) in wing.stations.iter().enumerate() {
            let value = |field: FieldKind| {
                self.graph
                    .station_field_node(component_index, station_index, field)
                    .map(|node| self.evaluation.values[node])
            };
            let position = [
                value(FieldKind::PositionX).unwrap_or(f64::NAN),
                value(FieldKind::PositionY).unwrap_or(f64::NAN),
                value(FieldKind::PositionZ).unwrap_or(f64::NAN),
            ];
            let trailing_edge = match station.trailing_edge() {
                TrailingEdge::Sharp => TrailingEdgeSpec::Sharp,
                TrailingEdge::Absolute { .. } => TrailingEdgeSpec::Absolute(
                    value(FieldKind::TrailingEdgeThickness).unwrap_or(f64::NAN),
                ),
                TrailingEdge::ChordFraction { .. } => TrailingEdgeSpec::ChordFraction(
                    value(FieldKind::TrailingEdgeFraction).unwrap_or(f64::NAN),
                ),
            };
            let curve = match self.curve(&station.airfoil) {
                Ok(curve) => curve,
                Err(diagnostic) => {
                    errors.push(diagnostic);
                    continue;
                }
            };
            stations.push(EvaluatedStation {
                id: station.id.clone(),
                position,
                chord: value(FieldKind::Chord).unwrap_or(f64::NAN),
                twist: value(FieldKind::Twist).unwrap_or(f64::NAN),
                trailing_edge,
                curve,
            });
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok((wing.id.clone(), stations))
    }

    fn curve(&self, id: &str) -> Result<Arc<ProfileCurve>, Diagnostic> {
        let airfoil = self.doc.airfoils.get(id).ok_or_else(|| {
            Diagnostic::error(Code::UnknownReference, format!("unknown airfoil {id:?}"))
        })?;
        match airfoil {
            aircraft_model::Airfoil::Naca4 { code } => ProfileCurve::naca4(code),
            aircraft_model::Airfoil::Coordinates { points } => ProfileCurve::coordinates(points),
        }
        .map(Arc::new)
    }

    /// Resolve per-panel tangencies from the stations' tangency specs.
    /// Panel `i` (station i → i+1) departs with station i's `left` and
    /// arrives with station i+1's `right`. `auto` on a kink side adopts the
    /// other side's direction if explicit, else the mean of the two panel
    /// sweeps when both are auto, else its own straight sweep (no-op).
    pub fn resolve_panel_tangencies(
        &self,
        component_index: usize,
    ) -> Result<Vec<aircraft_geom::wing::PanelTangency>, Vec<Diagnostic>> {
        let Some(Component::Wing(wing)) = self.doc.components.get(component_index) else {
            return Err(vec![unknown_component(component_index)]);
        };
        let (_, stations) = self.evaluated_stations(component_index)?;
        let count = stations.len();
        let unit = |vector: [f64; 3]| -> [f64; 3] {
            let length = aircraft_geom::mesh::vnorm(vector);
            if length > 1e-9 {
                aircraft_geom::mesh::vscale(vector, 1.0 / length)
            } else {
                [0.0; 3]
            }
        };
        let panel_dir: Vec<[f64; 3]> = (0..count.saturating_sub(1))
            .map(|i| {
                unit(aircraft_geom::mesh::vsub(
                    stations[i + 1].position,
                    stations[i].position,
                ))
            })
            .collect();

        let mean = |a: [f64; 3], b: [f64; 3]| unit(aircraft_geom::mesh::vadd(a, b));
        let mut panels =
            vec![aircraft_geom::wing::PanelTangency::default(); count.saturating_sub(1)];

        for (i, station) in wing.stations.iter().enumerate() {
            let Some(tangency) = &station.tangency else {
                continue;
            };
            fn other_side(
                side: &Option<aircraft_model::StationTangencySide>,
            ) -> Option<&aircraft_model::StationTangencySide> {
                side.as_ref()
            }
            let both_auto = |other: Option<&aircraft_model::StationTangencySide>| {
                other.map(|s| s.auto).unwrap_or(false)
            };

            // left: departure on panel i (toward station i+1).
            if let Some(left) = &tangency.left {
                if i < panels.len() {
                    let other = other_side(&tangency.right);
                    let (direction, strength) = if left.auto {
                        match other {
                            Some(o) if !o.auto => {
                                (unit(o.direction.expect("validated direction")), o.strength)
                            }
                            _ if both_auto(other) => {
                                // i == 0 (root): no inboard sweep — own straight.
                                let d_in = panel_dir.get(i.wrapping_sub(1)).copied();
                                match (d_in, panel_dir.get(i).copied()) {
                                    (Some(a), Some(b)) => (mean(a, b), left.strength),
                                    (None, Some(b)) => (b, left.strength),
                                    pair => (
                                        pair.0.unwrap_or(pair.1.unwrap_or([0.0; 3])),
                                        left.strength,
                                    ),
                                }
                            }
                            _ => (panel_dir[i], left.strength),
                        }
                    } else {
                        (
                            unit(left.direction.expect("validated direction")),
                            left.strength,
                        )
                    };
                    panels[i].start = Some(aircraft_geom::wing::Tangent {
                        direction,
                        strength,
                    });
                }
            }
            // right: arrival on panel i-1 (from station i-1).
            if let Some(right) = &tangency.right {
                if i >= 1 {
                    let other = other_side(&tangency.left);
                    let (direction, strength) = if right.auto {
                        match other {
                            Some(o) if !o.auto => {
                                (unit(o.direction.expect("validated direction")), o.strength)
                            }
                            _ if both_auto(other) => {
                                (mean(panel_dir[i - 1], panel_dir[i]), right.strength)
                            }
                            _ => (panel_dir[i - 1], right.strength),
                        }
                    } else {
                        (
                            unit(right.direction.expect("validated direction")),
                            right.strength,
                        )
                    };
                    panels[i - 1].end = Some(aircraft_geom::wing::Tangent {
                        direction,
                        strength,
                    });
                }
            }
        }
        Ok(panels)
    }

    /// Build the wing's analytic STEP document (NURBS skins, planar caps).
    pub fn export_step(
        &self,
        component_index: usize,
        full: bool,
        token: &CancellationToken,
    ) -> Result<String, MeshJobError> {
        if token.is_cancelled() {
            return Err(MeshJobError::Cancelled);
        }
        let symmetry = self.wing_symmetry_enabled(component_index)?;
        let panel_tangencies = self
            .resolve_panel_tangencies(component_index)
            .map_err(MeshJobError::Failed)?;
        let (_, stations) = self
            .evaluated_stations(component_index)
            .map_err(MeshJobError::Failed)?;
        let model = aircraft_geom::step_model::wing_model(
            "wing",
            &stations,
            symmetry,
            aircraft_geom::step_model::StepTolerances::default(),
            &panel_tangencies,
            full,
        )
        .map_err(MeshJobError::Failed)?;
        Ok(meshio::step::write_step(&model))
    }

    fn wing_symmetry_enabled(&self, component_index: usize) -> Result<bool, MeshJobError> {
        match &self.doc.components.get(component_index) {
            Some(Component::Wing(wing)) => Ok(wing.symmetry.enabled),
            _ => Err(MeshJobError::Failed(vec![unknown_component(
                component_index,
            )])),
        }
    }

    fn frame_origin(&self, component_index: usize) -> [f64; 3] {
        let mut origin = [0.0; 3];
        for (axis, value) in origin.iter_mut().enumerate() {
            if let Some(node) = self.graph.frame_origin_node(component_index, axis) {
                *value = self.evaluation.values[node];
            }
        }
        origin
    }

    /// Generate (or fetch from cache) the mesh of one wing. `full_model`
    /// mirrors across local XZ and welds the centerline; a half model caps
    /// the symmetry plane when the wing's symmetry is enabled.
    pub fn wing_mesh(
        &mut self,
        component_index: usize,
        quality: MeshQuality,
        full_model: bool,
        token: &CancellationToken,
        expected_revision: Option<u64>,
    ) -> Result<Arc<MeshArtifact>, MeshJobError> {
        if token.is_cancelled() {
            return Err(MeshJobError::Cancelled);
        }
        if let Some(expected) = expected_revision {
            if expected != self.revision {
                return Err(MeshJobError::Stale {
                    current: self.revision,
                    expected,
                });
            }
        }

        let key = self.mesh_cache_key(component_index, quality, full_model);
        if let Some(artifact) = self.mesh_cache.get(&key) {
            return Ok(Arc::clone(artifact));
        }

        let symmetry_enabled = self.wing_symmetry_enabled(component_index)?;
        let (wing_id, stations) = self
            .evaluated_stations(component_index)
            .map_err(MeshJobError::Failed)?;
        let panel_tangencies = self
            .resolve_panel_tangencies(component_index)
            .map_err(MeshJobError::Failed)?;
        let WingMesh { mesh, faces } = build_wing_mesh(
            &wing_id,
            &stations,
            symmetry_enabled,
            &resolved_geometry_quality(&stations, quality),
            symmetry_enabled && !full_model,
            full_model,
            &panel_tangencies,
        )
        .map_err(MeshJobError::Failed)?;

        // Translate into aircraft coordinates via the wing frame origin.
        let mesh = mesh.translated(self.frame_origin(component_index));
        let station_ids = stations.into_iter().map(|s| s.id).collect();

        let artifact = Arc::new(MeshArtifact {
            revision: self.revision,
            wing_id,
            station_ids,
            mesh,
            faces,
        });
        self.mesh_cache.insert(key, Arc::clone(&artifact));
        Ok(artifact)
    }

    fn mesh_cache_key(
        &self,
        component_index: usize,
        quality: MeshQuality,
        full_model: bool,
    ) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        GEOMETRY_VERSION.hash(&mut hasher);
        component_index.hash(&mut hasher);
        full_model.hash(&mut hasher);
        quality_key(quality).hash(&mut hasher);
        if let Some(Component::Wing(wing)) = self.doc.components.get(component_index) {
            for station in &wing.stations {
                station.id.hash(&mut hasher);
                if let Some(tangency) = &station.tangency {
                    1u8.hash(&mut hasher);
                    for side in [&tangency.left, &tangency.right] {
                        match side {
                            Some(side) => {
                                side.auto.hash(&mut hasher);
                                if let Some(direction) = side.direction {
                                    for value in direction {
                                        value.to_bits().hash(&mut hasher);
                                    }
                                }
                                side.strength.to_bits().hash(&mut hasher);
                            }
                            None => 0u8.hash(&mut hasher),
                        }
                    }
                } else {
                    0u8.hash(&mut hasher);
                }
            }
        }
        if let Ok((_, stations)) = self.evaluated_stations(component_index) {
            for station in &stations {
                station.id.hash(&mut hasher);
                for value in station.position {
                    value.to_bits().hash(&mut hasher);
                }
                station.chord.to_bits().hash(&mut hasher);
                station.twist.to_bits().hash(&mut hasher);
                format!("{:?}", station.trailing_edge).hash(&mut hasher);
                station.curve.x_le().to_bits().hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    /// Export a wing mesh in the requested format.
    pub fn export(
        &mut self,
        component_index: usize,
        quality: MeshQuality,
        full_model: bool,
        format: meshio::MeshFormat,
        name: &str,
        token: &CancellationToken,
    ) -> Result<Vec<u8>, MeshJobError> {
        let artifact = self.wing_mesh(component_index, quality, full_model, token, None)?;
        Ok(meshio::export(&artifact.mesh, name, format))
    }
}

/// Apply a patch's mutation to a candidate document (no validation).
fn apply_to_document(
    doc: &mut aircraft_model::AircraftDefinition,
    patch: &Patch,
) -> Result<(), Diagnostic> {
    match patch {
        Patch::SetParameter { id, value } => {
            if !doc.parameters.contains_key(id) {
                return Err(Diagnostic::error(
                    Code::UnknownReference,
                    format!("unknown parameter {id:?}"),
                )
                .with_path(format!("parameters/{id}")));
            }
            doc.parameters.insert(id.clone(), *value);
            Ok(())
        }
        Patch::SetStationTangency {
            component_index,
            station_index,
            tangency,
        } => {
            let Component::Wing(wing) = doc
                .components
                .get_mut(*component_index)
                .ok_or_else(|| unknown_component(*component_index))?;
            let station = wing.stations.get_mut(*station_index).ok_or_else(|| {
                Diagnostic::error(
                    Code::UnknownReference,
                    format!("station index {station_index} is outside the wing"),
                )
            })?;
            station.tangency = *tangency;
            Ok(())
        }
        Patch::AddParameter { id, value } => {
            if !is_valid_parameter_id(id) {
                return Err(Diagnostic::error(
                    Code::SchemaViolation,
                    format!(
                        "parameter id {id:?} must start with a lowercase letter and use only \
                         letters, digits, '.', '_' and '-'"
                    ),
                )
                .with_path(format!("parameters/{id}")));
            }
            if doc.parameters.contains_key(id) {
                return Err(Diagnostic::error(
                    Code::DuplicateIdentifier,
                    format!("parameter {id:?} already exists"),
                )
                .with_path(format!("parameters/{id}")));
            }
            doc.parameters.insert(id.clone(), *value);
            Ok(())
        }
        Patch::SetStationField {
            component_index,
            station_index,
            field,
            value,
        } => {
            let Component::Wing(wing) = doc
                .components
                .get_mut(*component_index)
                .ok_or_else(|| unknown_component(*component_index))?;
            let station = wing.stations.get_mut(*station_index).ok_or_else(|| {
                Diagnostic::error(
                    Code::UnknownReference,
                    format!("station index {station_index} is outside the wing"),
                )
            })?;
            match field {
                FieldKind::PositionX => station.position.x = value.clone(),
                FieldKind::PositionY => station.position.y = value.clone(),
                FieldKind::PositionZ => station.position.z = value.clone(),
                FieldKind::Chord => station.chord = value.clone(),
                FieldKind::Twist => station.twist = value.clone(),
                FieldKind::TrailingEdgeThickness => match &mut station.trailing_edge {
                    Some(TrailingEdge::Absolute { thickness }) => *thickness = value.clone(),
                    _ => {
                        return Err(Diagnostic::error(
                            Code::InvalidTrailingEdge,
                            "station's trailing edge is not in absolute mode",
                        ))
                    }
                },
                FieldKind::TrailingEdgeFraction => match &mut station.trailing_edge {
                    Some(TrailingEdge::ChordFraction { value: fraction }) => {
                        *fraction = value.clone()
                    }
                    _ => {
                        return Err(Diagnostic::error(
                            Code::InvalidTrailingEdge,
                            "station's trailing edge is not in chord-fraction mode",
                        ))
                    }
                },
            }
            Ok(())
        }
    }
}

/// Resolve a patch's seed nodes against a freshly built graph.
fn resolve_seeds(graph: &Graph, patch: &Patch) -> Vec<usize> {
    match patch {
        Patch::SetStationTangency { .. } => Vec::new(),
        Patch::AddParameter { id, .. } => graph.parameter_node(id).into_iter().collect(),
        Patch::SetParameter { id, .. } => graph.parameter_node(id).into_iter().collect(),
        Patch::SetStationField {
            component_index,
            station_index,
            field,
            ..
        } => graph
            .station_field_node(*component_index, *station_index, *field)
            .into_iter()
            .collect(),
    }
}

/// The schema's parameter id pattern: `^[a-z][A-Za-z0-9._-]*$`.
fn is_valid_parameter_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

fn unknown_component(component_index: usize) -> Diagnostic {
    Diagnostic::error(
        Code::UnknownReference,
        format!("component index {component_index} is outside the document"),
    )
}

/// Stable cache key for a quality setting (the type lives in aircraft-geom,
/// so the hash is implemented locally rather than through a foreign trait).
fn quality_key(quality: MeshQuality) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    match quality {
        MeshQuality::Interactive => 0u8.hash(&mut hasher),
        MeshQuality::Export(t) => {
            1u8.hash(&mut hasher);
            t.max_chordal_deviation
                .map(|v| v.to_bits())
                .hash(&mut hasher);
            t.max_edge_length.map(|v| v.to_bits()).hash(&mut hasher);
            t.max_normal_angle_deg
                .map(|v| v.to_bits())
                .hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Resolve quality against the actual station geometry.
fn resolved_geometry_quality(
    stations: &[EvaluatedStation],
    quality: MeshQuality,
) -> aircraft_geom::ResolvedQuality {
    let curves: Vec<(&ProfileCurve, f64)> = stations
        .iter()
        .map(|s| (s.curve.as_ref(), s.chord))
        .collect();
    let panel_spans: Vec<f64> = stations
        .windows(2)
        .map(|pair| {
            ((pair[1].position[1] - pair[0].position[1]).abs().powi(2)
                + (pair[1].position[2] - pair[0].position[2]).abs().powi(2))
            .sqrt()
        })
        .collect();
    let panel_angles: Vec<f64> = stations
        .windows(2)
        .map(|pair| (pair[1].twist - pair[0].twist).abs() * 180.0 / std::f64::consts::PI)
        .collect();
    aircraft_geom::quality::resolve_quality(&curves, &panel_spans, &panel_angles, &quality)
}
