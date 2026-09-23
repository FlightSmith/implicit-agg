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
                Diagnostic::error(
                    Code::DuplicateIdentifier,
                    format!("duplicate component id {id:?}"),
                )
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

/// Station tangency rules: `left` (departure tipward) is invalid on the
/// tip; `right` (arrival from inboard) is invalid on the root; `auto` and
/// `direction` are mutually exclusive; strength must be non-negative.
#[allow(clippy::too_many_arguments)]
fn validate_station_tangency(
    station_index: usize,
    last_station_index: usize,
    station: &crate::aircraft::Station,
    tangency: &crate::aircraft::StationTangency,
    path: &str,
    wing_id: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let subject = format!("component {} / station {} / tangency", wing_id, station.id);
    let mut push = |message: String, side: &str| {
        diagnostics.push(
            Diagnostic::error(Code::InvalidTangency, message)
                .with_path(format!("{path}/{side}"))
                .with_subject(subject.clone()),
        );
    };

    if tangency.left.is_some() && station_index == last_station_index {
        push(
            "'left' (departure) is not valid on the tip station; use 'right' (arrival)".into(),
            "left",
        );
    }
    if tangency.right.is_some() && station_index == 0 {
        push(
            "'right' (arrival) is not valid on the root station; use 'left' (departure)".into(),
            "right",
        );
    }

    for (name, side) in [("left", &tangency.left), ("right", &tangency.right)] {
        let Some(side) = side else { continue };
        if side.auto && side.direction.is_some() {
            push(
                format!("{name} cannot combine 'auto' with an explicit direction"),
                name,
            );
        }
        if !side.auto && side.direction.is_none() {
            push(format!("{name} needs a direction or \"auto\": true"), name);
        }
        if !(0.0..=10.0).contains(&side.strength) {
            push(format!("{name} strength must lie in [0, 10]"), name);
        }
    }
}

fn validate_wing(
    doc: &AircraftDefinition,
    component_index: usize,
    wing: &crate::aircraft::Wing,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut station_ids: Vec<&str> = Vec::new();
    let last_station_index = wing.stations.len().saturating_sub(1);
    for (station_index, station) in wing.stations.iter().enumerate() {
        let path = station_path(component_index, station_index);
        let subject = format!("component {} / station {}", wing.id, station.id);

        if station_ids.contains(&station.id.as_str()) {
            diagnostics.push(
                Diagnostic::error(
                    Code::DuplicateIdentifier,
                    format!(
                        "duplicate station id {:?} in wing {:?}",
                        station.id, wing.id
                    ),
                )
                .with_path(format!("{path}/id"))
                .with_subject(subject.clone()),
            );
        }
        station_ids.push(&station.id);

        if let Some(tangency) = &station.tangency {
            let path = format!("{path}/tangency");
            validate_station_tangency(
                station_index,
                last_station_index,
                station,
                tangency,
                &path,
                &wing.id,
                diagnostics,
            );
        }

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
