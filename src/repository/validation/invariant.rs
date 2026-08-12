//! Semantic repository invariants: conditions every accepted state must
//! satisfy (see `docs/invariants.md`).
//!
//! Phase 1 (step 1.4) enforces only the minimal group that `CreateElement`
//! and candidate-state correctness require:
//!
//! - stable identity: no duplicate `ElementId` in the candidate state;
//! - valid lifecycle: the new version is Active;
//! - referenced objects exist and object kinds match;
//! - candidate state internally coherent (canonical structural validity).
//!
//! The full invariant groups (relationship, traceability, authority,
//! validation, history) are deliberately not enforced in this slice.
//!
//! Errors surface as `InvariantError`.
