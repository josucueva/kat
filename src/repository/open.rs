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

use std::path::{Path, PathBuf};

use crate::domain::identity::ObjectId;
use crate::encoding::decode_canonical;
use crate::encoding::object::{CanonicalObject, CanonicalPayload, ObjectKind};
use crate::repository::error::RepositoryError;
use crate::repository::metadata::RepositoryMetadata;
use crate::repository::object_store::ObjectStore;
use crate::repository::ref_store::{AcceptedRef, FileRefStore};

/// An opened KAT repository.
#[derive(Debug)]
pub struct Repository {
    /// Workspace root directory.
    root: PathBuf,
    /// The validated repository metadata.
    pub metadata: RepositoryMetadata,
    /// The accepted repository head (SemanticState + optional ChangeRevision).
    pub accepted: AcceptedRef,
    /// Content-addressed object store over `.kat/objects`.
    store: ObjectStore,
    /// Reference store over `.kat/refs` (CAS publication of `refs/accepted`).
    refs: FileRefStore,
}

impl Repository {
    /// The workspace root directory containing `.kat/`.
    pub fn root_dir(&self) -> &Path {
        &self.root
    }

    /// The content-addressed object store of this repository.
    pub fn object_store(&self) -> &ObjectStore {
        &self.store
    }

    /// The reference store of this repository, used to publish `refs/accepted`
    /// via compare-and-swap.
    pub fn ref_store(&self) -> &FileRefStore {
        &self.refs
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
        root: path.to_path_buf(),
        metadata,
        accepted,
        store,
        refs,
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

/// Validates the repository integrity of a `RepositoryRevision`.
///
/// Ensures all its cryptographic references point to existing objects of the
/// correct canonical kinds.
pub fn check_repository_revision_integrity(
    store: &ObjectStore,
    revision: &crate::domain::revision::RepositoryRevision,
) -> Result<(), RepositoryError> {
    let _ = load_typed(
        store,
        revision.semantic_state.as_object_id(),
        ObjectKind::SemanticState,
    )?;

    if let Some(change_id) = &revision.semantic_change {
        let _ = load_typed(store, change_id.as_object_id(), ObjectKind::ChangeRevision)?;
    }

    for parent_id in &revision.parents {
        let _ = load_typed(
            store,
            parent_id.as_object_id(),
            ObjectKind::RepositoryRevision,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::change::ChangeRevision;
    use crate::domain::identity::{
        ChangeRevisionId, RepositoryRevisionId, SemanticStateId, WorkspaceSnapshotId,
    };
    use crate::domain::revision::RepositoryRevision;
    use crate::domain::state::SemanticState;

    use crate::encoding::object::{CanonicalObject, CanonicalPayload};
    use crate::repository::object_store::ObjectStoreError;

    fn store_object(store: &ObjectStore, payload: CanonicalPayload) -> ObjectId {
        let obj = CanonicalObject { payload };
        let bytes = crate::encoding::cbor::canonical_bytes(&obj).unwrap();
        store.put(&bytes).unwrap()
    }

    #[test]
    fn check_repository_revision_integrity_validates() {
        let temp = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(temp.path());

        let state_id = store_object(
            &store,
            CanonicalPayload::SemanticState(SemanticState {
                ontology_version: ObjectId::from_bytes([0; 32]),
                elements: vec![],
                relationships: vec![],
            }),
        );

        let change_id = store_object(
            &store,
            CanonicalPayload::ChangeRevision(ChangeRevision {
                change_id: crate::domain::identity::ChangeId::new(),
                base_states: vec![ObjectId::from_bytes([0; 32])],
                result_state: state_id,
                operations: vec![crate::domain::operation::Operation::CreateElement {
                    new_version: ObjectId::from_bytes([0; 32]),
                }],
                dependencies: vec![],
                description: None,
            }),
        );

        let rev = RepositoryRevision {
            parents: vec![],
            semantic_state: SemanticStateId::from_object_id(state_id),
            workspace_snapshot: WorkspaceSnapshotId::new(vec![]),
            semantic_change: Some(ChangeRevisionId::from_object_id(change_id)),
        };

        assert!(check_repository_revision_integrity(&store, &rev).is_ok());

        // 1. Missing SemanticState
        let mut rev_missing_state = rev.clone();
        rev_missing_state.semantic_state =
            SemanticStateId::from_object_id(ObjectId::from_bytes([1; 32]));
        assert!(matches!(
            check_repository_revision_integrity(&store, &rev_missing_state),
            Err(RepositoryError::ObjectStore(ObjectStoreError::NotFound(_)))
        ));

        // 2. semantic_state wrong object kind
        let mut rev_wrong_state = rev.clone();
        rev_wrong_state.semantic_state = SemanticStateId::from_object_id(change_id);
        assert!(matches!(
            check_repository_revision_integrity(&store, &rev_wrong_state),
            Err(RepositoryError::UnexpectedObjectKind {
                expected: ObjectKind::SemanticState,
                ..
            })
        ));

        // 3. semantic_change wrong object kind
        let mut rev_wrong_change = rev.clone();
        rev_wrong_change.semantic_change = Some(ChangeRevisionId::from_object_id(state_id));
        assert!(matches!(
            check_repository_revision_integrity(&store, &rev_wrong_change),
            Err(RepositoryError::UnexpectedObjectKind {
                expected: ObjectKind::ChangeRevision,
                ..
            })
        ));

        // 4. missing semantic change
        let mut rev_missing_change = rev.clone();
        rev_missing_change.semantic_change = Some(ChangeRevisionId::from_object_id(
            ObjectId::from_bytes([1; 32]),
        ));
        assert!(matches!(
            check_repository_revision_integrity(&store, &rev_missing_change),
            Err(RepositoryError::ObjectStore(ObjectStoreError::NotFound(_)))
        ));

        // 5. missing parent
        let mut rev_missing_parent = rev.clone();
        rev_missing_parent.parents = vec![RepositoryRevisionId::from_object_id(
            ObjectId::from_bytes([1; 32]),
        )];
        assert!(matches!(
            check_repository_revision_integrity(&store, &rev_missing_parent),
            Err(RepositoryError::ObjectStore(ObjectStoreError::NotFound(_)))
        ));

        // 6. parent wrong object kind
        let mut rev_wrong_parent = rev.clone();
        rev_wrong_parent.parents = vec![RepositoryRevisionId::from_object_id(state_id)];
        assert!(matches!(
            check_repository_revision_integrity(&store, &rev_wrong_parent),
            Err(RepositoryError::UnexpectedObjectKind {
                expected: ObjectKind::RepositoryRevision,
                ..
            })
        ));
    }
}
