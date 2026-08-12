//! Repository opening + integrity validation (`kat` reopen, step 0.10).
//!
//! [`open_repository`] proves that a repository written by `kat init` (or any
//! conforming implementation) is a valid, self-consistent KAT repository. It
//! performs **encoding validity** and **repository integrity** checks only;
//! full semantic validation (ontology conformance, invariants) is out of
//! scope until later steps.
//!
//! The three validation layers stay separate:
//!
//! ```text
//! encoding validity        ObjectStore + decode_canonical
//! repository integrity    open_repository (references, kinds, hash chain)
//! semantic validity       later steps
//! ```
//!
//! `ObjectStore::get` verifies hashes on read; `decode_canonical` is as
//! strict as the encoder; this module checks that referenced objects exist
//! and have exactly the canonical kinds the repository structure requires.

use std::path::Path;

use crate::domain::identity::ObjectId;
use crate::encoding::decode_canonical;
use crate::encoding::object::{CanonicalObject, CanonicalPayload, ObjectKind};
use crate::repository::error::RepositoryError;
use crate::repository::metadata::RepositoryMetadata;
use crate::repository::object_store::ObjectStore;
use crate::repository::ref_store::{AcceptedRef, FileRefStore, RefStore};

/// An opened KAT repository.
#[derive(Debug)]
pub struct Repository {
    /// The validated repository metadata.
    pub metadata: RepositoryMetadata,
    /// The accepted repository head (SemanticState + optional ChangeRevision).
    pub accepted: AcceptedRef,
    /// Content-addressed object store over `.kat/objects`.
    store: ObjectStore,
}

impl Repository {
    /// The content-addressed object store of this repository.
    pub fn object_store(&self) -> &ObjectStore {
        &self.store
    }
}

/// Opens the KAT repository rooted at `path` (`.kat/` inside it), verifying
/// integrity.
///
/// The open sequence:
///
/// ```text
/// locate .kat
///     ↓
/// read + validate repository.toml
///     ↓
/// read refs/accepted
///     ↓
/// load accepted.state → verify hash → decode → require SemanticState
///     ↓
/// load state.ontology_version → verify hash → decode → require OntologyVersion
///     ↓
/// load each element version → require KnowledgeElementVersion
///     ↓
/// load each relationship version → require RelationshipVersion
///     ↓
/// if accepted.change present:
///     load it → verify hash → decode → require ChangeRevision
///     require change.result_state == accepted.state
/// ```
pub fn open_repository(path: &Path) -> Result<Repository, RepositoryError> {
    let kat_dir = path.join(".kat");
    if !kat_dir.is_dir() {
        return Err(RepositoryError::NotFound(kat_dir));
    }

    let metadata = RepositoryMetadata::read(&kat_dir.join("repository.toml"))?;
    let refs = FileRefStore::new(&kat_dir);
    let accepted = refs.read_accepted()?;
    let store = ObjectStore::new(&kat_dir);

    // The accepted SemanticState.
    let state = match load_typed(&store, accepted.state, ObjectKind::SemanticState)?.payload {
        CanonicalPayload::SemanticState(state) => state,
        _ => unreachable!("kind verified by load_typed"),
    };

    // The ontology the state is interpreted under.
    let _ontology =
        match load_typed(&store, state.ontology_version, ObjectKind::OntologyVersion)?.payload {
            CanonicalPayload::OntologyVersion(ontology) => ontology,
            _ => unreachable!("kind verified by load_typed"),
        };

    // Every active element and relationship version must exist and be the
    // right kind (correct even though the initial S0 is empty).
    for entry in &state.elements {
        let _ = load_typed(&store, entry.version, ObjectKind::KnowledgeElementVersion)?;
    }
    for entry in &state.relationships {
        let _ = load_typed(&store, entry.version, ObjectKind::RelationshipVersion)?;
    }

    // The accepted ChangeRevision head, when present.
    if let Some(change_id) = accepted.change {
        let change = match load_typed(&store, change_id, ObjectKind::ChangeRevision)?.payload {
            CanonicalPayload::ChangeRevision(change) => change,
            _ => unreachable!("kind verified by load_typed"),
        };
        if change.result_state != accepted.state {
            return Err(RepositoryError::AcceptedChangeStateMismatch {
                change: change_id,
                expected: accepted.state,
                actual: change.result_state,
            });
        }
    }

    Ok(Repository {
        metadata,
        accepted,
        store,
    })
}

/// Loads `id` from the store (hash verified by `ObjectStore::get`), decodes
/// it canonically, and requires exactly `expected` kind.
fn load_typed(
    store: &ObjectStore,
    id: ObjectId,
    expected: ObjectKind,
) -> Result<CanonicalObject, RepositoryError> {
    let bytes = store.get(id)?;
    let object = decode_canonical(&bytes)?;
    let actual = object.object_kind();
    if actual != expected {
        return Err(RepositoryError::UnexpectedObjectKind { expected, actual });
    }
    Ok(object)
}
