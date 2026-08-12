//! Integration tests for `open_repository` (step 0.10): reopening a
//! repository written by `kat init` and proving its integrity.
//!
//! The repository-open layer checks **encoding validity** and **repository
//! integrity** (hashes, references, canonical kinds, accepted-head
//! consistency), not full semantic validity.

use std::fs;
use std::path::{Path, PathBuf};

use kat::domain::change::ChangeRevision;
use kat::domain::element::{KnowledgeElementVersion, Lifecycle};
use kat::domain::identity::{ChangeId, ElementId, ObjectId, RelationshipId};
use kat::domain::operation::Operation;
use kat::domain::relationship::RelationshipVersion;
use kat::domain::state::{ElementStateEntry, RelationshipStateEntry, SemanticState};
use kat::encoding::canonical_bytes;
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::repository::error::RepositoryError;
use kat::repository::init::init_repository;
use kat::repository::object_store::ObjectStore;
use kat::repository::open::{Repository, open_repository};
use kat::repository::ref_store::AcceptedRef;

fn kat_dir(root: &Path) -> PathBuf {
    root.join(".kat")
}

fn write_accepted(root: &Path, accepted: &AcceptedRef) {
    let path = kat_dir(root).join("refs").join("accepted");
    fs::write(path, accepted.to_string()).unwrap();
}

/// Encodes a payload and persists it in the repository's object store,
/// returning its ObjectId.
fn put_payload(store: &ObjectStore, payload: CanonicalPayload) -> ObjectId {
    let bytes = canonical_bytes(&CanonicalObject { payload }).unwrap();
    store.put(&bytes).unwrap()
}

#[test]
fn open_succeeds_after_init() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let result = init_repository(root).unwrap();
    let repo = open_repository(root).unwrap();

    assert_eq!(repo.accepted.state, result.state);
    assert_eq!(repo.accepted.change, None);
    assert_eq!(repo.metadata.repository_id, result.repository_id);
    assert_eq!(repo.metadata.software_id, result.software_id);
}

#[test]
fn open_fails_when_kat_missing() {
    let dir = tempfile::tempdir().unwrap();
    let err = open_repository(dir.path()).unwrap_err();
    assert!(matches!(err, RepositoryError::NotFound(_)));
}

#[test]
fn open_fails_on_corrupt_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    fs::write(kat_dir(root).join("repository.toml"), b"not = valid = toml").unwrap();
    let err = open_repository(root).unwrap_err();
    assert!(
        matches!(err, RepositoryError::Metadata(_)),
        "expected metadata error, got {err:?}"
    );
}

#[test]
fn open_fails_on_malformed_accepted_ref() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    fs::write(kat_dir(root).join("refs").join("accepted"), b"garbage").unwrap();
    let err = open_repository(root).unwrap_err();
    assert!(
        matches!(err, RepositoryError::RefStore(_)),
        "expected ref store error, got {err:?}"
    );
}

#[test]
fn open_fails_when_accepted_state_missing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let result = init_repository(root).unwrap();

    let state_path = kat_dir(root).join("objects").join(result.state.to_string());
    fs::remove_file(&state_path).unwrap();
    let err = open_repository(root).unwrap_err();
    assert!(
        matches!(err, RepositoryError::ObjectStore(_)),
        "expected object store error, got {err:?}"
    );
}

#[test]
fn open_fails_when_accepted_state_tampered() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let result = init_repository(root).unwrap();

    let state_path = kat_dir(root).join("objects").join(result.state.to_string());
    let mut bytes = fs::read(&state_path).unwrap();
    bytes[0] ^= 0xff;
    fs::write(&state_path, bytes).unwrap();

    let err = open_repository(root).unwrap_err();
    assert!(
        matches!(err, RepositoryError::ObjectStore(_)),
        "tampered state bytes must fail hash integrity, got {err:?}"
    );
}

#[test]
fn open_fails_when_accepted_state_wrong_kind() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    // accepted.state must decode as a SemanticState; point it at a
    // KnowledgeElementVersion instead.
    let store = ObjectStore::new(kat_dir(root));
    let element = put_payload(
        &store,
        CanonicalPayload::KnowledgeElementVersion(KnowledgeElementVersion {
            element_id: ElementId::new(),
            type_id: "kat.core/requirement".into(),
            lifecycle: Lifecycle::Active,
            properties: vec![],
        }),
    );
    write_accepted(
        root,
        &AcceptedRef {
            state: element,
            change: None,
        },
    );

    let err = open_repository(root).unwrap_err();
    assert!(
        matches!(err, RepositoryError::UnexpectedObjectKind { .. }),
        "expected UnexpectedObjectKind, got {err:?}"
    );
}

#[test]
fn open_fails_when_ontology_missing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let result = init_repository(root).unwrap();

    let ontology_path = kat_dir(root)
        .join("objects")
        .join(result.ontology.to_string());
    fs::remove_file(&ontology_path).unwrap();
    let err = open_repository(root).unwrap_err();
    assert!(
        matches!(err, RepositoryError::ObjectStore(_)),
        "missing ontology must fail at the object store, got {err:?}"
    );
}

#[test]
fn open_fails_when_ontology_wrong_kind() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    // Build a state whose ontology_version references a
    // KnowledgeElementVersion (wrong kind for the ontology slot).
    let store = ObjectStore::new(kat_dir(root));
    let element = put_payload(
        &store,
        CanonicalPayload::KnowledgeElementVersion(KnowledgeElementVersion {
            element_id: ElementId::new(),
            type_id: "kat.core/requirement".into(),
            lifecycle: Lifecycle::Active,
            properties: vec![],
        }),
    );
    let state = put_payload(
        &store,
        CanonicalPayload::SemanticState(SemanticState {
            ontology_version: element,
            elements: vec![],
            relationships: vec![],
        }),
    );
    write_accepted(
        root,
        &AcceptedRef {
            state,
            change: None,
        },
    );

    let err = open_repository(root).unwrap_err();
    assert!(
        matches!(err, RepositoryError::UnexpectedObjectKind { .. }),
        "expected UnexpectedObjectKind, got {err:?}"
    );
}

#[test]
fn open_fails_when_change_object_missing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let result = init_repository(root).unwrap();

    let missing = ObjectId::from_bytes([0xee; 32]);
    write_accepted(
        root,
        &AcceptedRef {
            state: result.state,
            change: Some(missing),
        },
    );

    let err = open_repository(root).unwrap_err();
    assert!(
        matches!(err, RepositoryError::ObjectStore(_)),
        "missing change object must fail at the object store, got {err:?}"
    );
}

#[test]
fn open_fails_when_change_wrong_kind() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let result = init_repository(root).unwrap();

    let store = ObjectStore::new(kat_dir(root));
    let element = put_payload(
        &store,
        CanonicalPayload::KnowledgeElementVersion(KnowledgeElementVersion {
            element_id: ElementId::new(),
            type_id: "kat.core/requirement".into(),
            lifecycle: Lifecycle::Active,
            properties: vec![],
        }),
    );
    write_accepted(
        root,
        &AcceptedRef {
            state: result.state,
            change: Some(element),
        },
    );

    let err = open_repository(root).unwrap_err();
    assert!(
        matches!(err, RepositoryError::UnexpectedObjectKind { .. }),
        "expected UnexpectedObjectKind, got {err:?}"
    );
}

#[test]
fn open_fails_when_change_result_state_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let result = init_repository(root).unwrap();

    let store = ObjectStore::new(kat_dir(root));
    let change = put_payload(
        &store,
        CanonicalPayload::ChangeRevision(ChangeRevision {
            change_id: ChangeId::new(),
            base_states: vec![result.state],
            result_state: ObjectId::from_bytes([0xdd; 32]),
            operations: vec![Operation::CreateElement {
                new_version: ObjectId::from_bytes([0xdd; 32]),
            }],
            dependencies: vec![],
            description: None,
        }),
    );
    write_accepted(
        root,
        &AcceptedRef {
            state: result.state,
            change: Some(change),
        },
    );

    let err = open_repository(root).unwrap_err();
    assert!(
        matches!(err, RepositoryError::AcceptedChangeStateMismatch { .. }),
        "expected AcceptedChangeStateMismatch, got {err:?}"
    );
}

#[test]
fn open_verifies_element_and_relationship_version_kinds() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let result = init_repository(root).unwrap();

    // A non-empty state: element and relationship versions must exist and be
    // the right canonical kinds even though the initial S0 is empty.
    let store = ObjectStore::new(kat_dir(root));
    let element = put_payload(
        &store,
        CanonicalPayload::KnowledgeElementVersion(KnowledgeElementVersion {
            element_id: ElementId::new(),
            type_id: "kat.core/requirement".into(),
            lifecycle: Lifecycle::Active,
            properties: vec![],
        }),
    );
    let relationship = put_payload(
        &store,
        CanonicalPayload::RelationshipVersion(RelationshipVersion {
            relationship_id: RelationshipId::new(),
            source_element_id: ElementId::new(),
            relationship_type: "kat.core/addresses".into(),
            target_element_id: ElementId::new(),
            properties: vec![],
        }),
    );
    let state = put_payload(
        &store,
        CanonicalPayload::SemanticState(SemanticState {
            ontology_version: result.ontology,
            elements: vec![ElementStateEntry {
                element_id: ElementId::new(),
                version: element,
            }],
            relationships: vec![RelationshipStateEntry {
                relationship_id: RelationshipId::new(),
                version: relationship,
            }],
        }),
    );
    write_accepted(
        root,
        &AcceptedRef {
            state,
            change: None,
        },
    );

    let repo: Repository = open_repository(root).unwrap();
    assert_eq!(repo.accepted.state, state);
}

#[test]
fn open_fails_when_element_version_wrong_kind() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let result = init_repository(root).unwrap();

    // The element slot references the ontology object (wrong kind).
    let store = ObjectStore::new(kat_dir(root));
    let state = put_payload(
        &store,
        CanonicalPayload::SemanticState(SemanticState {
            ontology_version: result.ontology,
            elements: vec![ElementStateEntry {
                element_id: ElementId::new(),
                version: result.ontology,
            }],
            relationships: vec![],
        }),
    );
    write_accepted(
        root,
        &AcceptedRef {
            state,
            change: None,
        },
    );

    let err = open_repository(root).unwrap_err();
    assert!(
        matches!(err, RepositoryError::UnexpectedObjectKind { .. }),
        "expected UnexpectedObjectKind, got {err:?}"
    );
}
