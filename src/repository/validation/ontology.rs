//! Ontology conformance: whether knowledge objects conform to the repository
//! ontology (e.g. an element's `type_id` exists in the current `OntologyVersion`).
//!
//! Phase 1 (step 1.3) adds the minimal rule: the element type must exist in
//! `state.ontology_version`. Further ontology rules are future work and must
//! not be added before the slice requires them.
//!
//! Errors surface as `OntologyError`.
