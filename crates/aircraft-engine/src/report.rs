//! Derived reports: planform metrics, volume metrics, and mesh statistics
//! per wing. Reports always state the half/full basis of each number.

use crate::{CancellationToken, Engine, MeshJobError};
use aircraft_geom::metrics::{volume_metrics, VolumeMetrics};
use aircraft_geom::quality::MeshQuality;
use aircraft_geom::PlanformMetrics;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WingReport {
    pub wing_id: String,
    /// Planform metrics in aircraft coordinates (position-like quantities
    /// include the wing frame origin).
    pub planform: PlanformMetrics,
    /// Volume metrics from the closed half model; `None` when the wing has
    /// no symmetry plane to cap and close.
    pub volume: Option<VolumeMetrics>,
    pub mesh_statistics: MeshStatistics,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshStatistics {
    pub vertices: usize,
    pub triangles: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AircraftReport {
    pub revision: u64,
    pub wings: Vec<WingReport>,
}

impl Engine {
    /// Build a full report at interactive mesh quality.
    pub fn report(&mut self, token: &CancellationToken) -> Result<AircraftReport, MeshJobError> {
        let mut wings = Vec::new();
        for component_index in 0..self.document().components.len() {
            let (wing_id, stations) = self
                .evaluated_stations(component_index)
                .map_err(MeshJobError::Failed)?;

            let mut planform = aircraft_geom::metrics::planform_metrics(&stations)
                .map_err(|diagnostic| MeshJobError::Failed(vec![diagnostic]))?;
            // Position-like planform quantities live in aircraft coordinates.
            for axis in [0usize, 2] {
                planform.mac_le[axis] += self.frame_origin(component_index)[axis];
            }

            let symmetry_enabled = self.wing_symmetry_enabled(component_index)?;
            // The half model of a symmetric wing caps its symmetry plane and
            // closes, which is what volume integration requires.
            let artifact = self.wing_mesh(
                component_index,
                MeshQuality::Interactive,
                false,
                token,
                None,
            )?;
            let volume = symmetry_enabled.then(|| volume_metrics(&artifact.mesh));
            // Reference area from the mesh projection: exact for the faceted
            // planform, so LE tangency curvature is fully accounted for (a
            // station-trapezoid would ignore the curved leading edge). The
            // upper and lower skins each project onto the same outline, so
            // the closed-mesh sum counts the outline twice.
            planform.reference_area.half = artifact.mesh.projected_area_xy() / 2.0;
            planform.reference_area.full = 2.0 * planform.reference_area.half;

            let mesh_statistics = MeshStatistics {
                vertices: artifact.mesh.vertex_count(),
                triangles: artifact.mesh.triangle_count(),
            };
            wings.push(WingReport {
                wing_id,
                planform,
                volume,
                mesh_statistics,
            });
        }
        Ok(AircraftReport {
            revision: self.revision(),
            wings,
        })
    }
}
