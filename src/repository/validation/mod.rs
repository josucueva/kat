//! Semantic validation, distinct from `encoding` (canonical structural
//! validity) and from `repository` open (repository integrity).
//!
//! The three validation layers are kept separate (per `rules.md`):
//!
//! ```text
//! encoding/validate.rs               canonical structural validity
//! repository/validation/ontology.rs   ontology conformance
//! repository/validation/invariant.rs  semantic repository invariants
//! repository/change.rs                change application + orchestration
//! ```
//!
//! Phase 1 fills the first slice: `ontology` (element type exists in the base
//! OntologyVersion) and `invariant` (the minimal group CreateElement and
//! candidate-state correctness require). Precondition checks (operation
//! application conditions) live with the Change Engine in `change.rs`.

pub mod invariant;
pub mod ontology;
