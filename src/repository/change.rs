//! The Change Engine: authoritative, controlled mutation of semantic state
//! (see `docs/prototype-design.md`, "Change Application Flow", and
//! `docs/architecture.md`, "Change Engine").
//!
//! The engine is the only path through which new authoritative semantic
//! states are produced. It composes, in order:
//!
//! ```text
//! prepare    (resolve accepted, load base state + ontology)
//!     ↓
//! apply      (build operations, check preconditions, apply to a candidate)
//!     ↓
//! validate   (ontology conformance + invariants — semantic validity)
//!     ↓
//! materialize (encode + persist V1, S1, C1 — immutable objects)
//!     ↓
//! publish    (compare-and-swap on refs/accepted)
//! ```
//!
//! Phase 0 established the separation of the three validation layers:
//! **encoding validity** (`encoding`), **repository integrity** (`open`),
//! and **semantic validity** (`repository::validation` + preconditions here).
//! This module is the orchestration boundary; it must not own ontology or
//! invariant semantics (those live in `repository::validation`).
//!
//! Step 1.1 is the smallest first piece: **prepare only**. It resolves the
//! accepted repository state and loads the base SemanticState and its
//! OntologyVersion into a reusable [`ChangeContext`]. It performs **no**
//! mutation, no persistence, and no publication.

use crate::domain::identity::ObjectId;
use crate::domain::ontology::OntologyVersion;
use crate::domain::state::SemanticState;
use crate::encoding::decode::DecodingError;
use crate::encoding::decode_canonical;
use crate::encoding::object::{CanonicalObject, ObjectKind};
use crate::repository::object_store::{ObjectStore, ObjectStoreError};
use crate::repository::open::Repository;
use crate::repository::ref_store::AcceptedRef;

/// Error produced by the Change Engine.
///
/// Only variants reachable by the engine are defined; further variants
/// (e.g. preconditions, ontology, invariants) are added when the respective
/// steps require them.
#[derive(Debug, thiserror::Error)]
pub enum ChangeError {
    /// An object store failure while loading a referenced object.
    #[error("object store error: {0}")]
    ObjectStore(#[from] ObjectStoreError),
    /// A referenced object failed strict canonical decoding.
    #[error("decoding error: {0}")]
    Decoding(#[from] DecodingError),
    /// A referenced object has a different canonical kind than expected.
    #[error("expected object kind {expected}, found {actual}")]
    UnexpectedObjectKind {
        /// The canonical kind the repository structure required.
        expected: ObjectKind,
        /// The canonical kind the stored object actually has.
        actual: ObjectKind,
    },
}

/// The resolved context a change is prepared against: the accepted head, the
/// selected base SemanticState, and the OntologyVersion interpreting it.
///
/// Carrying the loaded base state and ontology avoids re-reading and
/// re-decoding them for each operation in the change.
#[derive(Debug)]
pub struct ChangeContext {
    /// The accepted repository head this change is based on. This is also the
    /// `expected` value later passed to the CAS publication step.
    pub accepted: AcceptedRef,
    /// ObjectId of the base SemanticState the change applies to.
    pub base_state_id: ObjectId,
    /// The decoded base SemanticState.
    pub base_state: SemanticState,
    /// The OntologyVersion that interprets the base state.
    pub ontology: OntologyVersion,
}

/// Resolves the accepted repository head and loads the base SemanticState and
/// its OntologyVersion into a [`ChangeContext`].
///
/// `prepare_only: no mutation, no persistence, no publication.` The accepted
/// ref and object store are left exactly as they were.
pub fn prepare_change(repository: &Repository) -> Result<ChangeContext, ChangeError> {
    let accepted = repository.accepted.clone();
    let base_state_id = accepted.state;

    let base_state = match load_typed(
        repository.object_store(),
        base_state_id,
        ObjectKind::SemanticState,
    )?
    .payload
    {
        crate::encoding::object::CanonicalPayload::SemanticState(state) => state,
        _ => unreachable!("kind verified by load_typed"),
    };

    let ontology = match load_typed(
        repository.object_store(),
        base_state.ontology_version,
        ObjectKind::OntologyVersion,
    )?
    .payload
    {
        crate::encoding::object::CanonicalPayload::OntologyVersion(ontology) => ontology,
        _ => unreachable!("kind verified by load_typed"),
    };

    Ok(ChangeContext {
        accepted,
        base_state_id,
        base_state,
        ontology,
    })
}

/// Loads `id` from the store (hash verified by `ObjectStore::get`), decodes
/// it canonically, and requires exactly `expected` kind.
fn load_typed(
    store: &ObjectStore,
    id: ObjectId,
    expected: ObjectKind,
) -> Result<CanonicalObject, ChangeError> {
    let bytes = store.get(id)?;
    let object = decode_canonical(&bytes)?;
    let actual = object.object_kind();
    if actual != expected {
        return Err(ChangeError::UnexpectedObjectKind { expected, actual });
    }
    Ok(object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::init::init_repository;
    use crate::repository::open::open_repository;

    #[test]
    fn prepare_change_unit_loads_context() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let init = init_repository(root).unwrap();

        let repo = open_repository(root).unwrap();
        let context = prepare_change(&repo).unwrap();

        assert_eq!(context.accepted.state, init.state);
        assert_eq!(context.accepted.change, None);
        assert_eq!(context.base_state_id, init.state);
        assert_eq!(context.base_state.ontology_version, init.ontology);
        assert!(context.base_state.elements.is_empty());
        assert!(context.base_state.relationships.is_empty());
        assert_eq!(context.ontology.element_types.len(), 7);
        assert_eq!(context.ontology.relationship_types.len(), 10);
    }
}
