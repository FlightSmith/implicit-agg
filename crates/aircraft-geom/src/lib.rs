//! Wing geometry generation: profiles, station sections, lofting, symmetry
//! welding, meshing, and planform metrics. The generator consumes evaluated
//! (canonical-unit) station values and produces mesh and metric artifacts.

pub mod mesh;
pub mod metrics;
pub mod profile;
pub mod quality;
pub mod section;
pub mod wing;

pub use mesh::{Mesh, MeshValidation};
pub use metrics::{BasisPair, PanelMetrics, PlanformMetrics, VolumeMetrics};
pub use profile::ProfileCurve;
pub use quality::ResolvedQuality;
pub use section::{Ring, StationSpec, TrailingEdgeSpec};
pub use wing::{build_wing_mesh, FaceSource, WingMesh};
