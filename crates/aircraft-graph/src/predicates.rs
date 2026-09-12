//! Physical predicates validated after a component's inputs resolve:
//! root on the symmetry plane, outboard station progression, positive chord,
//! and trailing-edge feasibility basics. Profile-level feasibility arrives
//! with the geometry milestone.

use crate::graph::{Evaluation, Graph};
use crate::node::FieldKind;
use aircraft_model::aircraft::Component;
use aircraft_model::{Code, Diagnostic};

/// Positional tolerance for the root station on local `y = 0`, in meters.
const ROOT_TOLERANCE: f64 = 1.0e-6;

/// Minimum separation between consecutive stations, in meters.
const STATION_SEPARATION: f64 = 1.0e-9;

pub fn check_predicates(
    graph: &Graph,
    doc: &aircraft_model::AircraftDefinition,
    evaluation: &Evaluation,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (component_index, component) in doc.components.iter().enumerate() {
        let Component::Wing(wing) = component;
        check_wing(graph, component_index, wing, evaluation, &mut diagnostics);
    }
    diagnostics
}

fn check_wing(
    graph: &Graph,
    component_index: usize,
    wing: &aircraft_model::aircraft::Wing,
    evaluation: &Evaluation,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let value = |station_index: usize, field: FieldKind| {
        graph
            .station_field_node(component_index, station_index, field)
            .map(|node| evaluation.values[node])
    };

    // A symmetric wing's root belongs exactly on local XZ.
    if wing.symmetry.enabled {
        if let Some(root_y) = value(0, FieldKind::PositionY) {
            if root_y.abs() > ROOT_TOLERANCE {
                diagnostics.push(
                    Diagnostic::error(
                        Code::RootOffSymmetryPlane,
                        format!(
                            "symmetric wing root must lie on local y = 0, but its y value is \
                             {:.6} m away",
                            root_y.abs()
                        ),
                    )
                    .with_path(
                        aircraft_model::aircraft::station_path(component_index, 0) + "/position/y",
                    )
                    .with_subject(format!(
                        "{} / station {} / position y",
                        wing.id, wing.stations[0].id
                    )),
                );
            }
        }
    }

    // Stations must progress strictly outboard into negative local Y.
    for station_index in 1..wing.stations.len() {
        if let (Some(previous), Some(current)) = (
            value(station_index - 1, FieldKind::PositionY),
            value(station_index, FieldKind::PositionY),
        ) {
            if current >= previous - STATION_SEPARATION {
                diagnostics.push(
                    Diagnostic::error(
                        Code::StationOrderViolation,
                        format!(
                            "station {:?} must be further outboard (more negative local y) than \
                             station {:?}",
                            wing.stations[station_index].id,
                            wing.stations[station_index - 1].id
                        ),
                    )
                    .with_path(
                        aircraft_model::aircraft::station_path(component_index, station_index)
                            + "/position/y",
                    )
                    .with_subject(format!(
                        "{} / station {} / position y",
                        wing.id, wing.stations[station_index].id
                    )),
                );
            }
        }
    }

    // Chords must be strictly positive.
    for (station_index, station) in wing.stations.iter().enumerate() {
        if let Some(chord) = value(station_index, FieldKind::Chord) {
            if chord <= 0.0 {
                diagnostics.push(
                    Diagnostic::error(
                        Code::NonPositiveChord,
                        format!(
                            "station {:?} has a non-positive chord ({:.6} m)",
                            station.id, chord
                        ),
                    )
                    .with_path(format!(
                        "{}/chord",
                        aircraft_model::aircraft::station_path(component_index, station_index)
                    ))
                    .with_subject(format!("{} / station {} / chord", wing.id, station.id)),
                );
            }
        }
    }

    // Trailing-edge basics: non-negative; a chord fraction stays below one.
    // Full profile feasibility is checked by the geometry generator.
    for (station_index, station) in wing.stations.iter().enumerate() {
        match station.trailing_edge() {
            aircraft_model::TrailingEdge::Absolute { .. } => {
                if let Some(thickness) = value(station_index, FieldKind::TrailingEdgeThickness) {
                    if thickness < 0.0 {
                        diagnostics.push(
                            Diagnostic::error(
                                Code::InvalidTrailingEdge,
                                format!(
                                    "station {:?} has a negative trailing-edge thickness",
                                    station.id
                                ),
                            )
                            .with_subject(format!(
                                "{} / station {} / trailing edge thickness",
                                wing.id, station.id
                            )),
                        );
                    }
                }
            }
            aircraft_model::TrailingEdge::ChordFraction { .. } => {
                if let Some(fraction) = value(station_index, FieldKind::TrailingEdgeFraction) {
                    if !(0.0..1.0).contains(&fraction) {
                        diagnostics.push(
                            Diagnostic::error(
                                Code::InvalidTrailingEdge,
                                format!(
                                    "station {:?} has a trailing-edge fraction of {:.4}, which \
                                     must lie in [0, 1)",
                                    station.id, fraction
                                ),
                            )
                            .with_subject(format!(
                                "{} / station {} / trailing edge fraction",
                                wing.id, station.id
                            )),
                        );
                    }
                }
            }
            aircraft_model::TrailingEdge::Sharp => {}
        }
    }
}
