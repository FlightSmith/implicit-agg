//! WebAssembly binding of the aircraft engine.
//!
//! The exported API mirrors the service API from docs/02 one-to-one, so the
//! browser frontend and a future Tauri shell share the same command surface.
//! All values cross the boundary as plain JSON-compatible objects.

use aircraft_engine::{
    CancellationToken, Engine, MeshJobError, Patch, TransactionId, UpdateResult,
};
use aircraft_graph::FieldKind;
use aircraft_model::{parse_document, Diagnostic, TypedValue};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// Runtime state of one aircraft document plus its derived caches.
#[wasm_bindgen]
pub struct WasmEngine {
    engine: Engine,
    last_diagnostics: Vec<DiagnosticDto>,
}

/// The example document embedded for the demo entry point.
#[wasm_bindgen]
pub fn demo_document_json() -> String {
    include_str!("../../../examples/cranked-wing.v0.1.json").to_string()
}

/// The wing component catalog: field dimensions, constraints, and adaptive
/// control-range policies that drive the inspector's editors.
#[wasm_bindgen]
pub fn wing_component_catalog() -> Result<JsValue, JsValue> {
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../../../catalog/wing-component.v0.1.json"))
            .map_err(|error| JsValue::from_str(&format!("embedded catalog is invalid: {error}")))?;
    serde_wasm_bindgen::to_value(&catalog).map_err(|error| JsValue::from_str(&error.to_string()))
}

fn diagnostics_to_dto(diagnostics: &[Diagnostic]) -> Vec<DiagnosticDto> {
    diagnostics
        .iter()
        .map(|d| DiagnosticDto {
            code: d.code.as_str().to_string(),
            severity: if d.is_error() { "error" } else { "warning" }.to_string(),
            message: d.message.clone(),
            path: d.path.clone(),
            subject: d.subject.clone(),
        })
        .collect()
}

fn update_result_dto(result: &UpdateResult, engine: &Engine) -> UpdateResultDto {
    UpdateResultDto {
        committed: result.committed,
        revision: u32::try_from(result.revision).expect("revision fits u32"),
        diagnostics: diagnostics_to_dto(&result.diagnostics),
        affected: result.affected.clone(),
        current_revision: u32::try_from(engine.revision()).expect("revision fits u32"),
    }
}

fn mesh_error_dto(error: &MeshJobError) -> MeshErrorDto {
    match error {
        MeshJobError::Cancelled => MeshErrorDto {
            kind: "cancelled".to_string(),
            current_revision: 0,
            expected_revision: 0,
            diagnostics: Vec::new(),
        },
        MeshJobError::Stale { current, expected } => MeshErrorDto {
            kind: "stale".to_string(),
            current_revision: *current,
            expected_revision: *expected,
            diagnostics: Vec::new(),
        },
        MeshJobError::Failed(diagnostics) => MeshErrorDto {
            kind: "failed".to_string(),
            current_revision: 0,
            expected_revision: 0,
            diagnostics: diagnostics_to_dto(diagnostics),
        },
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DocumentMetaDto {
    id: String,
    name: String,
    length_unit: String,
    angle_unit: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticDto {
    code: String,
    severity: String,
    message: String,
    path: Option<String>,
    subject: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateResultDto {
    committed: bool,
    revision: u32,
    current_revision: u32,
    diagnostics: Vec<DiagnosticDto>,
    affected: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeshErrorDto {
    kind: String,
    current_revision: u64,
    expected_revision: u64,
    diagnostics: Vec<DiagnosticDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeshDto {
    revision: u64,
    wing_id: String,
    station_ids: Vec<String>,
    /// Flat xyz triples for the renderer.
    vertices: Vec<f32>,
    indices: Vec<u32>,
    faces: Vec<FaceDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FaceDto {
    kind: String,
    lower_station: Option<usize>,
    upper_station: Option<usize>,
    mirrored: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StationRowDto {
    id: String,
    airfoil: String,
    /// Document units, ready for display.
    x: f64,
    y: f64,
    z: f64,
    chord: f64,
    twist: f64,
    value_kinds: ValueKindsDto,
    /// The parameter bound to each field, when bound as `$param`.
    bindings: ValueBindingsDto,
    tangency: Option<StationTangencyDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ValueKindsDto {
    x: String,
    y: String,
    z: String,
    chord: String,
    twist: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct ValueBindingsDto {
    x: Option<String>,
    y: Option<String>,
    z: Option<String>,
    chord: Option<String>,
    twist: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StationTangencySideDto {
    auto: bool,
    direction: Option<Vec<f64>>,
    strength: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StationTangencyDto {
    left: Option<StationTangencySideDto>,
    right: Option<StationTangencySideDto>,
}

fn side_dto(side: &aircraft_model::StationTangencySide) -> StationTangencySideDto {
    StationTangencySideDto {
        auto: side.auto,
        direction: side.direction.map(|d| vec![d[0], d[1], d[2]]),
        strength: side.strength,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WingStationsDto {
    wing_id: String,
    /// Interface names published by this wing, for expression autocomplete.
    interfaces: Vec<String>,
    stations: Vec<StationRowDto>,
}

#[wasm_bindgen]
impl WasmEngine {
    /// Parse and open a document; errors carry the diagnostics array.
    #[wasm_bindgen(constructor)]
    pub fn new(document_json: &str) -> Result<WasmEngine, JsValue> {
        console_error_panic_hook::set_once();
        let (doc, diagnostics) = parse_document(document_json).map_err(|errors| {
            serde_wasm_bindgen::to_value(&diagnostics_to_dto(&errors)).expect("serializable")
        })?;
        let engine = Engine::open(doc).map_err(|errors| {
            serde_wasm_bindgen::to_value(&diagnostics_to_dto(&errors)).expect("serializable")
        })?;
        Ok(WasmEngine {
            engine,
            last_diagnostics: diagnostics_to_dto(&diagnostics),
        })
    }

    pub fn revision(&self) -> u32 {
        u32::try_from(self.engine.revision()).expect("revision fits u32")
    }

    /// The canonical, pretty-printed source document.
    pub fn document_json(&self) -> String {
        serde_json::to_string_pretty(self.engine.document()).expect("serializable document")
    }

    pub fn document_meta(&self) -> JsValue {
        let doc = self.engine.document();
        let meta = DocumentMetaDto {
            id: doc.id.clone(),
            name: doc.name.clone(),
            length_unit: doc.units.length_unit_name().to_string(),
            angle_unit: doc.units.angle_unit_name().to_string(),
        };
        serde_wasm_bindgen::to_value(&meta).expect("serializable meta")
    }

    pub fn diagnostics(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.last_diagnostics).expect("serializable diagnostics")
    }

    pub fn parameters(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.engine.document().parameters)
            .expect("serializable parameters")
    }

    pub fn set_parameter(&mut self, id: &str, value: f64, transaction: u32) -> JsValue {
        let result = self.engine.apply_patch(
            Patch::SetParameter {
                id: id.to_string(),
                value,
            },
            TransactionId(u64::from(transaction)),
        );
        self.record(&result);
        serde_wasm_bindgen::to_value(&update_result_dto(&result, &self.engine))
            .expect("serializable result")
    }

    pub fn set_station_tangency(
        &mut self,
        component_index: usize,
        station_index: usize,
        tangency: Option<JsValue>,
        transaction: u32,
    ) -> Result<JsValue, JsValue> {
        let parsed: Option<aircraft_model::StationTangency> = match tangency {
            Some(value) => Some(
                serde_wasm_bindgen::from_value(value)
                    .map_err(|error| JsValue::from_str(&format!("invalid tangency: {error}")))?,
            ),
            None => None,
        };
        let result = self.engine.apply_patch(
            aircraft_engine::Patch::SetStationTangency {
                component_index,
                station_index,
                tangency: parsed,
            },
            aircraft_engine::TransactionId(u64::from(transaction)),
        );
        self.record(&result);
        Ok(
            serde_wasm_bindgen::to_value(&update_result_dto(&result, &self.engine))
                .expect("serializable result"),
        )
    }

    /// Build the wing's analytic STEP document (text, metres).
    pub fn export_step(
        &mut self,
        component_index: usize,
        full_model: bool,
    ) -> Result<String, JsValue> {
        self.engine
            .export_step(component_index, full_model, &CancellationToken::default())
            .map_err(|error| {
                serde_wasm_bindgen::to_value(&mesh_error_dto(&error)).expect("serializable")
            })
    }

    pub fn add_parameter(&mut self, id: &str, value: f64, transaction: u32) -> JsValue {
        let result = self.engine.apply_patch(
            Patch::AddParameter {
                id: id.to_string(),
                value,
            },
            TransactionId(u64::from(transaction)),
        );
        self.record(&result);
        serde_wasm_bindgen::to_value(&update_result_dto(&result, &self.engine))
            .expect("serializable result")
    }

    /// Bind a station field to a typed value; `value_json` is
    /// `number | {"$param": id} | "= expression"`.
    pub fn set_station_field(
        &mut self,
        component_index: usize,
        station_index: usize,
        field: &str,
        value_json: JsValue,
        transaction: u32,
    ) -> Result<JsValue, JsValue> {
        let typed: TypedValue = serde_wasm_bindgen::from_value(value_json)
            .map_err(|error| JsValue::from_str(&format!("invalid typed value: {error}")))?;
        let field_kind = field_kind_from_str(field)
            .ok_or_else(|| JsValue::from_str(&format!("unknown field {field:?}")))?;
        let result = self.engine.apply_patch(
            Patch::SetStationField {
                component_index,
                station_index,
                field: field_kind,
                value: typed,
            },
            TransactionId(u64::from(transaction)),
        );
        self.record(&result);
        Ok(
            serde_wasm_bindgen::to_value(&update_result_dto(&result, &self.engine))
                .expect("serializable result"),
        )
    }

    /// Evaluated stations per wing, converted back into document units.
    pub fn stations(&self) -> JsValue {
        let doc = self.engine.document();
        let units = doc.units;
        let mut wings = Vec::new();
        for component_index in 0..doc.components.len() {
            let Some(aircraft_model::aircraft::Component::Wing(wing)) =
                doc.components.get(component_index)
            else {
                continue;
            };
            let Ok((wing_id, stations)) = self.engine.evaluated_stations(component_index) else {
                continue;
            };
            let rows = stations
                .iter()
                .enumerate()
                .filter_map(|(station_index, station)| {
                    let doc_station = wing.stations.get(station_index)?;
                    Some(StationRowDto {
                        id: station.id.clone(),
                        airfoil: doc_station.airfoil.clone(),
                        x: units.length_from_canonical(station.position[0]),
                        y: units.length_from_canonical(station.position[1]),
                        z: units.length_from_canonical(station.position[2]),
                        chord: units.length_from_canonical(station.chord),
                        twist: units.angle_from_canonical(station.twist),
                        value_kinds: ValueKindsDto {
                            x: kind_of(&doc_station.position.x),
                            y: kind_of(&doc_station.position.y),
                            z: kind_of(&doc_station.position.z),
                            chord: kind_of(&doc_station.chord),
                            twist: kind_of(&doc_station.twist),
                        },
                        bindings: ValueBindingsDto {
                            x: bound_of(&doc_station.position.x),
                            y: bound_of(&doc_station.position.y),
                            z: bound_of(&doc_station.position.z),
                            chord: bound_of(&doc_station.chord),
                            twist: bound_of(&doc_station.twist),
                        },
                        tangency: doc_station.tangency.as_ref().map(|t| StationTangencyDto {
                            left: t.left.as_ref().map(side_dto),
                            right: t.right.as_ref().map(side_dto),
                        }),
                    })
                })
                .collect();
            let interfaces = match doc.components.get(component_index) {
                Some(aircraft_model::aircraft::Component::Wing(wing)) => {
                    wing.interfaces.keys().cloned().collect()
                }
                _ => Vec::new(),
            };
            wings.push(WingStationsDto {
                wing_id,
                interfaces,
                stations: rows,
            });
        }
        serde_wasm_bindgen::to_value(&wings).expect("serializable stations")
    }

    /// Generate the wing mesh. `quality` is `"interactive"` or `"export"`.
    pub fn mesh(&mut self, quality: &str, full_model: bool) -> Result<JsValue, JsValue> {
        let quality = quality_from_str(quality)?;
        let artifact = self
            .engine
            .wing_mesh(0, quality, full_model, &CancellationToken::default(), None)
            .map_err(|error| {
                serde_wasm_bindgen::to_value(&mesh_error_dto(&error)).expect("serializable")
            })?;
        let dto = MeshDto {
            revision: artifact.revision,
            wing_id: artifact.wing_id.clone(),
            station_ids: artifact.station_ids.clone(),
            vertices: artifact
                .mesh
                .vertices
                .iter()
                .flat_map(|v| v.iter().map(|value| *value as f32))
                .collect(),
            indices: artifact
                .mesh
                .triangles
                .iter()
                .flat_map(|t| t.iter().copied())
                .collect(),
            faces: artifact
                .faces
                .iter()
                .map(|face| match *face {
                    aircraft_geom::FaceSource::Panel {
                        lower_station,
                        upper_station,
                        mirrored,
                    } => FaceDto {
                        kind: "panel".to_string(),
                        lower_station: Some(lower_station),
                        upper_station: Some(upper_station),
                        mirrored,
                    },
                    aircraft_geom::FaceSource::TipCap { station, mirrored } => FaceDto {
                        kind: "tipCap".to_string(),
                        lower_station: Some(station),
                        upper_station: None,
                        mirrored,
                    },
                    aircraft_geom::FaceSource::RootCap => FaceDto {
                        kind: "rootCap".to_string(),
                        lower_station: None,
                        upper_station: None,
                        mirrored: false,
                    },
                })
                .collect(),
        };
        serde_wasm_bindgen::to_value(&dto).map_err(|error| JsValue::from_str(&error.to_string()))
    }

    /// Trace a mesh triangle back to the stations that generated it. The
    /// mesh is regenerated with the same geometry as the displayed model
    /// (cached, so this is cheap).
    pub fn trace(&self, triangle_index: usize, full_model: bool) -> Result<JsValue, JsValue> {
        let mut probe = self.engine.clone();
        let artifact = probe
            .wing_mesh(
                0,
                aircraft_geom::quality::MeshQuality::Interactive,
                full_model,
                &CancellationToken::default(),
                None,
            )
            .map_err(|error| {
                serde_wasm_bindgen::to_value(&mesh_error_dto(&error)).expect("serializable")
            })?;
        let source = artifact.trace(triangle_index).map_err(|diagnostic| {
            serde_wasm_bindgen::to_value(&diagnostics_to_dto(&[diagnostic])).expect("serializable")
        })?;
        serde_wasm_bindgen::to_value(&source).map_err(|error| JsValue::from_str(&error.to_string()))
    }

    /// Derived planform and volume report at interactive quality.
    pub fn report(&mut self) -> Result<JsValue, JsValue> {
        let report = self
            .engine
            .report(&CancellationToken::default())
            .map_err(|error| {
                serde_wasm_bindgen::to_value(&mesh_error_dto(&error)).expect("serializable")
            })?;
        serde_wasm_bindgen::to_value(&report).map_err(|error| JsValue::from_str(&error.to_string()))
    }

    /// Export the wing mesh; `format` is `"stl" | "obj" | "glb"`.
    pub fn export_mesh(
        &mut self,
        format: &str,
        quality: &str,
        full_model: bool,
    ) -> Result<Vec<u8>, JsValue> {
        let mesh_format = match format {
            "stl" => meshio::MeshFormat::StlBinary,
            "obj" => meshio::MeshFormat::Obj,
            "glb" => meshio::MeshFormat::Glb,
            other => return Err(JsValue::from_str(&format!("unknown format {other:?}"))),
        };
        self.engine
            .export(
                0,
                quality_from_str(quality)?,
                full_model,
                mesh_format,
                "wing",
                &CancellationToken::default(),
            )
            .map_err(|error| {
                serde_wasm_bindgen::to_value(&mesh_error_dto(&error)).expect("serializable")
            })
    }

    /// Deep snapshot for undo (the engine is a plain value).
    #[wasm_bindgen]
    pub fn snapshot(&self) -> WasmEngine {
        self.clone()
    }

    /// Restore a previously taken snapshot.
    pub fn restore(&mut self, other: WasmEngine) {
        self.engine.restore(other.engine);
        self.last_diagnostics.clear();
    }
}

impl Clone for WasmEngine {
    fn clone(&self) -> Self {
        WasmEngine {
            engine: self.engine.clone(),
            last_diagnostics: self.last_diagnostics.clone(),
        }
    }
}

impl WasmEngine {
    fn record(&mut self, result: &UpdateResult) {
        self.last_diagnostics = diagnostics_to_dto(&result.diagnostics);
    }
}

fn kind_of(value: &TypedValue) -> String {
    match value {
        TypedValue::Number(_) => "literal".to_string(),
        TypedValue::ParamRef { .. } => "parameter".to_string(),
        TypedValue::Expression(_) => "expression".to_string(),
    }
}

fn bound_of(value: &TypedValue) -> Option<String> {
    match value {
        TypedValue::ParamRef { param } => Some(param.clone()),
        _ => None,
    }
}

fn field_kind_from_str(field: &str) -> Option<FieldKind> {
    Some(match field {
        "position.x" => FieldKind::PositionX,
        "position.y" => FieldKind::PositionY,
        "position.z" => FieldKind::PositionZ,
        "chord" => FieldKind::Chord,
        "twist" => FieldKind::Twist,
        "trailingEdge.thickness" => FieldKind::TrailingEdgeThickness,
        "trailingEdge.value" => FieldKind::TrailingEdgeFraction,
        _ => return None,
    })
}

fn quality_from_str(quality: &str) -> Result<aircraft_geom::quality::MeshQuality, JsValue> {
    match quality {
        "interactive" => Ok(aircraft_geom::quality::MeshQuality::Interactive),
        // The auto-settle tier: finer than the drag preview, cheaper than a
        // full export run.
        "settled" => Ok(aircraft_geom::quality::MeshQuality::Export(
            aircraft_geom::quality::ExportTolerances {
                max_chordal_deviation: Some(2.5e-3),
                max_edge_length: Some(0.6),
                max_normal_angle_deg: Some(10.0),
            },
        )),
        "export" => Ok(aircraft_geom::quality::MeshQuality::Export(
            aircraft_geom::quality::ExportTolerances::default(),
        )),
        other => Err(JsValue::from_str(&format!("unknown quality {other:?}"))),
    }
}
