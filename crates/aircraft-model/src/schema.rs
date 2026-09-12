//! JSON Schema validation against the embedded normative v0.1 schema.
//!
//! Schema validation checks shape only; meaning-level constraints live in
//! [`crate::semantic`] and in the dependency graph's physical predicates.

use crate::diagnostic::{Code, Diagnostic};
use std::sync::OnceLock;

pub const AIRCRAFT_DEFINITION_SCHEMA: &str =
    include_str!("../../../schemas/aircraft-definition-v0.1.schema.json");

type Compiled = jsonschema::Validator;

static VALIDATOR: OnceLock<Option<Compiled>> = OnceLock::new();

fn validator() -> Option<&'static Compiled> {
    VALIDATOR
        .get_or_init(|| {
            let schema: serde_json::Value = serde_json::from_str(AIRCRAFT_DEFINITION_SCHEMA).ok()?;
            jsonschema::options().build(&schema).ok()
        })
        .as_ref()
}

/// Validate a raw document against the embedded schema, returning one
/// diagnostic per violation with the instance path attached.
pub fn validate_schema(document: &serde_json::Value) -> Vec<Diagnostic> {
    let Some(validator) = validator() else {
        return vec![Diagnostic::error(
            Code::Internal,
            "embedded JSON Schema failed to compile",
        )];
    };
    validator
        .iter_errors(document)
        .map(|error| {
            Diagnostic::error(Code::SchemaViolation, error.to_string())
                .with_path(error.instance_path().to_string())
        })
        .collect()
}
