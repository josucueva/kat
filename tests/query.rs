//! Integration tests for the read-side query layer (step 1.8): resolving the
//! currently accepted version of an element and decoding it.
//!
//! Queries are strictly read-only — the last test pins that invariant (object
//! store and `refs/accepted` byte-for-byte unchanged after a query).

use std::fs;
use std::path::{Path, PathBuf};

use kat::domain::element::Lifecycle;
use kat::domain::identity::{ChangeId, ElementId, ObjectId};
use kat::domain::property::PropertyValue;
use kat::domain::state::{ElementStateEntry, SemanticState};
use kat::encoding::canonical_bytes;
use kat::encoding::object::{CanonicalObject, CanonicalPayload, ObjectKind};
use kat::repository::change::{
    CreateElementInput, apply_create_element, persist_prepared_change, prepare_change,
    prepare_change_revision, publish_persisted_change, validate_create_element_invariants,
    validate_create_element_ontology,
};
use kat::repository::init::init_repository;
use kat::repository::open::open_repository;
use kat::repository::query::{QueryError, show_element};
use kat::repository::ref_store::{AcceptedRef, RefStore};
use uuid::Uuid;

fn kat_dir(root: &Path) -> PathBuf {
    root.join(".kat")
}

fn object_ids(root: &Path) -> Vec<String> {
    let mut ids: Vec<String> = fs::read_dir(kat_dir(root).join("objects"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    ids.sort();
    ids
}

/// Runs the full engine pipeline (prepare -> create -> validate -> revision ->
/// persist -> publish) for one element against a fresh repository, returning
/// its ElementId and the published version ObjectId.
fn publish_element(root: &Path, element_n: u128, change_n: u128) -> (ElementId, ObjectId) {
    let repo = open_repository(root).unwrap();
    let element_id = ElementId::from_uuid(Uuid::from_u128(element_n));
    let context = prepare_change(&repo).unwrap();
    let prepared = apply_create_element(
        context,
        CreateElementInput {
            element_id,
            type_id: "kat.core/requirement".into(),
            properties: vec![("title".into(), PropertyValue::Text("A requirement".into()))],
        },
    )
    .unwrap();
    let validated =
        validate_create_element_invariants(validate_create_element_ontology(prepared).unwrap())
            .unwrap();
    let revision = prepare_change_revision(
        validated,
        ChangeId::from_uuid(Uuid::from_u128(change_n)),
        None,
    )
    .unwrap();
    let version_id = revision.creation.element_version_id;
    let persisted = persist_prepared_change(&repo, revision).unwrap();
    publish_persisted_change(&repo, persisted).unwrap();
    (element_id, version_id)
}

#[test]
fn show_returns_view_for_published_element() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let (element_id, version_id) = publish_element(root, 71, 171);
    let repo = open_repository(root).unwrap();
    let view = show_element(&repo, element_id).unwrap();

    assert_eq!(view.element_id, element_id);
    assert_eq!(view.version_id, version_id);
    assert_eq!(view.element.element_id, element_id);
    assert_eq!(view.element.type_id, "kat.core/requirement");
    assert_eq!(view.element.lifecycle, Lifecycle::Active);
    assert_eq!(
        view.element.properties,
        vec![(
            "title".to_string(),
            PropertyValue::Text("A requirement".to_string())
        )]
    );

    // The returned payload is exactly the persisted V1.
    let stored_bytes = repo.object_store().get(version_id).unwrap();
    let stored = match kat::encoding::decode_canonical(&stored_bytes)
        .unwrap()
        .payload
    {
        CanonicalPayload::KnowledgeElementVersion(element) => element,
        other => panic!("expected KnowledgeElementVersion, got {other:?}"),
    };
    assert_eq!(view.element, stored);
}

#[test]
fn show_version_id_matches_accepted_state_entry() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let (element_id, version_id) = publish_element(root, 72, 172);

    // A fresh process reopens and queries the published head.
    let reopened = open_repository(root).unwrap();
    let view = show_element(&reopened, element_id).unwrap();
    assert_eq!(view.version_id, version_id);

    // And it is exactly what the accepted state maps E72 -> V72 to.
    let context = prepare_change(&reopened).unwrap();
    let entry = context
        .base_state
        .elements
        .iter()
        .find(|e| e.element_id == element_id)
        .expect("accepted state must contain the element");
    assert_eq!(entry.version, view.version_id);
}

#[test]
fn show_unknown_element_returns_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let (present, _) = publish_element(root, 73, 173);
    let missing = ElementId::from_uuid(Uuid::from_u128(999));
    assert_ne!(present, missing);

    let repo = open_repository(root).unwrap();
    let err = show_element(&repo, missing).unwrap_err();
    assert!(matches!(err, QueryError::ElementNotFound(id) if id == missing));
}

#[test]
fn show_rejects_wrong_object_kind_at_element_version() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = init_repository(root).unwrap();

    // Corrupt the repository after open: publish a state whose element entry
    // points at the ontology object (a valid object, but not a
    // KnowledgeElementVersion). The query layer must reject it with the kind
    // error rather than decoding garbage.
    let repo = open_repository(root).unwrap();
    let element_id = ElementId::from_uuid(Uuid::from_u128(74));
    let bad_state = SemanticState {
        ontology_version: init.ontology,
        elements: vec![ElementStateEntry {
            element_id,
            version: init.ontology,
        }],
        relationships: vec![],
    };
    let bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::SemanticState(bad_state),
    })
    .unwrap();
    let bad_state_id = repo.object_store().put(&bytes).unwrap();
    repo.ref_store()
        .compare_and_swap_accepted(
            &AcceptedRef {
                state: init.state,
                change: None,
            },
            &AcceptedRef {
                state: bad_state_id,
                change: None,
            },
        )
        .unwrap();

    let err = show_element(&repo, element_id).unwrap_err();
    assert!(matches!(
        err,
        QueryError::UnexpectedObjectKind {
            expected: ObjectKind::KnowledgeElementVersion,
            actual: ObjectKind::OntologyVersion,
        }
    ));
}

#[test]
fn show_succeeds_after_fresh_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let (element_id, version_id) = publish_element(root, 75, 175);

    // A completely new process (fresh open) resolves the same view.
    let reopened = open_repository(root).unwrap();
    let view = show_element(&reopened, element_id).unwrap();
    assert_eq!(view.element_id, element_id);
    assert_eq!(view.version_id, version_id);
    assert_eq!(view.element.lifecycle, Lifecycle::Active);
}

#[test]
fn show_query_does_not_mutate_repository() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let (element_id, _) = publish_element(root, 76, 176);
    let objects_before = object_ids(root);
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();

    let repo = open_repository(root).unwrap();
    show_element(&repo, element_id).unwrap();

    assert_eq!(object_ids(root), objects_before);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
}
