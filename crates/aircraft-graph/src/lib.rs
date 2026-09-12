//! The typed dependency graph: nodes for every parameter and typed numeric
//! field, edges from references, deterministic evaluation, and the physical
//! predicates that run after values resolve.

pub mod graph;
pub mod node;
pub mod predicates;

pub use graph::{build, evaluate_graph, evaluate_incremental, Evaluation, Graph, Value};
pub use node::{FieldKind, Node, NodeId, NodeKind, ValueSource};
pub use predicates::check_predicates;
