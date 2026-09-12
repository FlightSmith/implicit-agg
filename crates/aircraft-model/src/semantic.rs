//! Semantic validation: the meaning-level constraints that JSON Schema cannot
//! express. Structural checks run before evaluation; physical predicates that
//! need evaluated values (root on the symmetry plane, station ordering,
//! positive chord) run after graph evaluation and live in `aircraft-graph`.

use crate::aircraft::{station_path, AircraftDefinition, Component};
use crate::diagnostic::{Code, Diagnostic};

/// Validate identifier uniqueness and reference existence.
pub fn validate(doc: &AircraftDefinition) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if doc.schema != crate::aircraft::SCHEMA_ID {
        diagnostics.push(
            Diagnostic::error(
                Code::SchemaViolation,
                format!(
                    "unknown document schema {:?}; expected {:?}",
                    doc.schema,
                    crate::aircraft::SCHEMA_ID
                ),
            )
            .with_path("schema"),
        );
    }
    if !doc.unit_system_locked {
        diagnostics.push(
            Diagnostic::error(
                Code::InvalidUnits,
                "unitSystemLocked must be true; the application does not convert a document's \
                 unit system in place",
            )
            .with_path("unitSystemLocked"),
        );
    }

    let mut component_ids: Vec<&str> = Vec::new();
    for (component_index, component) in doc.components.iter().enumerate() {
        let id = component.id();
        if component_ids.contains(&id) {
            diagnostics.push(
                Diagnostic::error(Code::DuplicateIdentifier, format!("duplicate component id {id:?}"))
                    .with_path(format!("components/{component_index}"))
                    .with_subject(format!("component {id}")),
            );
        }
        component_ids.push(id);

        match component {
            Component::Wing(wing) => validate_wing(doc, component_index, wing, &mut diagnostics),
        }
    }

    diagnostics
}

fn validate_wing(
    doc: &AircraftDefinition,
    component_index: usize,
    wing: &crate::aircraft::Wing,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut station_ids: Vec<&str> = Vec::new();
    for (station_index, station) in wing.stations.iter().enumerate() {
        let path = station_path(component_index, station_index);
        let subject = format!("component {} / station {}", wing.id, station.id);

        if station_ids.contains(&station.id.as_str()) {
            diagnostics.push(
                Diagnostic::error(
                    Code::DuplicateIdentifier,
                    format!("duplicate station id {:?} in wing {:?}", station.id, wing.id),
                )
                .with_path(format!("{path}/id"))
                .with_subject(subject.clone()),
            );
        }
        station_ids.push(&station.id);

        if !doc.airfoils.contains_key(&station.airfoil) {
            diagnostics.push(
                Diagnostic::error(
                    Code::UnknownReference,
                    format!(
                        "station {:?} references airfoil {:?} which is not defined",
                        station.id, station.airfoil
                    ),
                )
                .with_path(format!("{path}/airfoil"))
                .with_subject(subject),
            );
        }
    }

    for (name, interface) in &wing.interfaces {
        let crate::aircraft::Interface::StationPlane { station } = interface;
        if !station_ids.contains(&station.as_str()) {
            diagnostics.push(
                Diagnostic::error(
                    Code::UnknownReference,
                    format!(
                        "interface {name:?} references station {station:?} which is not defined in \
                         wing {:?}",
                        wing.id
                    ),
                )
                .with_path(format!("components/{component_index}/interfaces/{name}"))
                .with_subject(format!("component {} / interface {}", wing.id, name)),
            );
        }
    }
}
