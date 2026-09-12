//! A typed numeric input: a literal, a parameter reference, or an expression.
//!
//! Exactly one of the three forms is accepted per field; the JSON Schema
//! enforces the shape and the containing field supplies the dimension.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TypedValue {
    Number(f64),
    ParamRef {
        #[serde(rename = "$param")]
        param: String,
    },
    Expression(String),
}

impl TypedValue {
    pub fn number(value: f64) -> Self {
        TypedValue::Number(value)
    }

    pub fn param(id: impl Into<String>) -> Self {
        TypedValue::ParamRef { param: id.into() }
    }

    /// Build an expression value from formula source without the leading `=`.
    pub fn expression(formula: impl Into<String>) -> Self {
        TypedValue::Expression(format!("= {}", formula.into()))
    }
}
