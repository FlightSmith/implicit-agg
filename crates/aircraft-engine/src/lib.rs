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
    /// Set or clear the wing's leading-edge tangency DSL.
    SetLeTangency {
        component_index: usize,
        spec: Option<String>,
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

    /// Resolve the wing's leading-edge tangency DSL into unit tangent
    /// vectors at the kink, using the evaluated station positions.
    fn resolve_le_tangency(
        &self,
        component_index: usize,
    ) -> Result<Option<aircraft_geom::wing::LeTangency>, Vec<Diagnostic>> {
        let Some(Component::Wing(wing)) = self.doc.components.get(component_index) else {
            return Err(vec![unknown_component(component_index)]);
        };
        let Some(tangency) = &wing.tangency else {
            return Ok(None);
        };
        let dsl = aircraft_model::tangency::parse_le_tangency(&tangency.leading_edge)
            .map_err(|diagnostic| vec![diagnostic])?;
        let (_, stations) = self.evaluated_stations(component_index)?;
        if stations.len() < 3 {
            return Err(vec![Diagnostic::error(
                Code::InvalidTangency,
                "leading-edge tangency needs at least three stations (two panels)",
            )]);
        }
        let le = |index: usize| stations[index].position;
        let unit = |vector: [f64; 3]| -> [f64; 3] {
            let length = aircraft_geom::mesh::vnorm(vector);
            if length > 1e-9 {
                aircraft_geom::mesh::vscale(vector, 1.0 / length)
            } else {
                [0.0; 3]
            }
        };
        let d1 = unit(aircraft_geom::mesh::vsub(le(1), le(0)));
        let d2 = unit(aircraft_geom::mesh::vsub(le(2), le(1)));
        let mean = unit(aircraft_geom::mesh::vadd(d1, d2));
        // When both sides are auto they meet on the mean of the two
        // original directions; a single auto matches the other side's
        // direction (explicit vector if present, else its straight sweep).
        let both_auto = dsl.left == Some(aircraft_model::tangency::LeSide::Auto)
            && dsl.right == Some(aircraft_model::tangency::LeSide::Auto);
        let resolve = |side: Option<aircraft_model::tangency::LeSide>,
                       other: Option<aircraft_model::tangency::LeSide>,
                       own_straight: [f64; 3],
                       other_straight: [f64; 3],
                       mean: [f64; 3]| {
            let _ = other_straight;
            match side {
                None => None,
                Some(aircraft_model::tangency::LeSide::Vector(vector)) => Some(unit(vector)),
                Some(aircraft_model::tangency::LeSide::Auto) => Some(match other {
                    Some(aircraft_model::tangency::LeSide::Vector(vector)) => unit(vector),
                    _ => {
                        if both_auto {
                            mean
                        } else {
                            own_straight
                        }
                    }
                }),
            }
        };
        let kink_end = resolve(dsl.left, dsl.right, d2, d1, mean);
        let kink_start = resolve(dsl.right, dsl.left, d1, d2, mean);
        Ok(Some(aircraft_geom::wing::LeTangency {
            kink_end,
            kink_start,
            // Reserved for a future tangency-strength control.
            strength: 1.0,
        }))
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
        let le_tangency = self
            .resolve_le_tangency(component_index)
            .map_err(MeshJobError::Failed)?;
        let (_, stations) = self
            .evaluated_stations(component_index)
            .map_err(MeshJobError::Failed)?;
        let model = aircraft_geom::step_model::wing_model(
            "wing",
            &stations,
            symmetry,
            aircraft_geom::step_model::StepTolerances::default(),
            le_tangency,
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
        let le_tangency = self
            .resolve_le_tangency(component_index)
            .map_err(MeshJobError::Failed)?;
        let WingMesh { mesh, faces } = build_wing_mesh(
            &wing_id,
            &stations,
            symmetry_enabled,
            &resolved_geometry_quality(&stations, quality),
            symmetry_enabled && !full_model,
            full_model,
            le_tangency,
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
            if let Some(tangency) = &wing.tangency {
                tangency.leading_edge.hash(&mut hasher);
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
        Patch::SetLeTangency {
            component_index,
            spec,
        } => {
            let Component::Wing(wing) = doc
                .components
                .get_mut(*component_index)
                .ok_or_else(|| unknown_component(*component_index))?;
            wing.tangency = spec
                .clone()
                .map(|leading_edge| aircraft_model::aircraft::Tangency { leading_edge });
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
        Patch::SetLeTangency { .. } => Vec::new(),
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
    let curves: Vec<&ProfileCurve> = stations.iter().map(|s| s.curve.as_ref()).collect();
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
