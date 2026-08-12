//! Integration tests for the Change Engine (steps 1.1-1.7): prepare-only
//! context resolution, `CreateElement` application, ontology + invariant
//! validation, `ChangeRevision` construction, persistence, and CAS
//! publication.
//!
//! Step 1.1 produces only a [`ChangeContext`]: it resolves the accepted head
//! and loads the base SemanticState + its OntologyVersion. It performs **no**
//! mutation, persistence, or publication — these tests prove that invariant:
//! the object store and `refs/accepted` are unchanged after a prepare.
//!
//! Because the engine operates on an already-`open_repository`-validated
//! [`Repository`], integrity failures (missing / wrong-kind referenced
//! objects) are rejected by the `open` layer first. That is the intended
//! layered boundary: repository integrity is guaranteed before the engine can
//! run, and the engine's own kind checks are defense-in-depth. The first test
//! pins this boundary.

use std::fs;
use std::path::{Path, PathBuf};

use kat::domain::identity::ObjectId;
use kat::domain::property::PropertyValue;
use kat::repository::change::{
    apply_create_element, persist_prepared_change, prepare_change, prepare_change_revision,
    publish_persisted_change, validate_create_element_invariants, validate_create_element_ontology,
};
use kat::repository::init::init_repository;
use kat::repository::open::open_repository;
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

fn object_id(byte: u8) -> ObjectId {
    ObjectId::from_bytes([byte; 32])
}

/// Runs the full engine pipeline up to and including persistence against an
/// open repository: prepare -> create -> ontology -> invariants -> revision ->
/// persist. Used by the step 1.7 publication tests.
fn prepare_and_persist(
    repo: &kat::repository::open::Repository,
    element_n: u128,
    change_n: u128,
) -> kat::repository::change::PersistedChange {
    let context = prepare_change(repo).unwrap();
    let prepared = apply_create_element(
        context,
        kat::repository::change::CreateElementInput {
            element_id: kat::domain::identity::ElementId::from_uuid(Uuid::from_u128(element_n)),
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
        kat::domain::identity::ChangeId::from_uuid(Uuid::from_u128(change_n)),
        None,
    )
    .unwrap();
    persist_prepared_change(repo, revision).unwrap()
}

#[test]
fn prepare_change_resolves_loads_and_does_not_publish() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let init = init_repository(root).unwrap();
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();
    let objects_before = object_ids(root);

    let repo = open_repository(root).unwrap();
    let context = prepare_change(&repo).unwrap();

    // Accepted head resolved exactly as initialized.
    assert_eq!(
        context.accepted,
        AcceptedRef {
            state: init.state,
            change: None,
        }
    );

    // Base SemanticState loaded: S0, empty, referencing O1.
    assert_eq!(context.base_state_id, init.state);
    assert_eq!(context.base_state.ontology_version, init.ontology);
    assert!(context.base_state.elements.is_empty());
    assert!(context.base_state.relationships.is_empty());

    // Base OntologyVersion loaded: the spec-derived core ontology.
    assert_eq!(context.ontology.element_types.len(), 7);
    assert_eq!(context.ontology.relationship_types.len(), 10);
    assert_eq!(
        context.ontology.element_types[5].type_id,
        "kat.core/requirement"
    );

    // No mutation: object store and accepted ref are byte-for-byte unchanged.
    assert_eq!(object_ids(root), objects_before);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
    // Still exactly the two immutable objects from init (O1, S0).
    assert_eq!(objects_before.len(), 2);
}

#[test]
fn prepared_base_matches_plan_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    // Reopen like a fresh process; the context matches the plan's S0 base
    // with no accepted change head.
    let repo = open_repository(root).unwrap();
    let context = prepare_change(&repo).unwrap();
    let head = &context.accepted;
    assert_eq!(head.change, None, "fresh repository has no change head");
    assert_eq!(context.base_state_id, head.state, "base == accepted state");
}

#[test]
fn integrity_failures_rejected_at_open_boundary_before_prepare() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = init_repository(root).unwrap();

    // Removing the accepted SemanticState object makes the repository
    // integrity-invalid; the open layer rejects it, so the engine is never
    // reached on a broken repository.
    fs::remove_file(kat_dir(root).join("objects").join(init.state.to_string())).unwrap();

    let err = open_repository(root).unwrap_err();
    assert!(matches!(
        err,
        kat::repository::error::RepositoryError::ObjectStore(
            kat::repository::object_store::ObjectStoreError::NotFound(_)
        )
    ));

    // (prepare_change could not be invoked: there is no valid Repository to
    // pass to it, which is exactly the layered guarantee.)
}

#[test]
fn apply_create_element_only_creates_a_logical_candidate_nothing_persisted() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();
    let objects_before = object_ids(root);

    let repo = open_repository(root).unwrap();
    let context = prepare_change(&repo).unwrap();
    let prepared = apply_create_element(
        context,
        kat::repository::change::CreateElementInput {
            element_id: kat::domain::identity::ElementId::from_uuid(Uuid::from_u128(42)),
            type_id: "kat.core/requirement".into(),
            properties: vec![("title".into(), PropertyValue::Text("A requirement".into()))],
        },
    )
    .unwrap();

    // S1 exists logically and maps E42 -> V1 (V1 ObjectId known), but...
    assert_eq!(prepared.candidate_state.elements.len(), 1);
    assert_eq!(
        prepared.element.lifecycle,
        kat::domain::element::Lifecycle::Active
    );

    // ...nothing was persisted and the accepted ref is unchanged.
    assert_eq!(object_ids(root), objects_before);
    assert_eq!(objects_before.len(), 2);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
}

#[test]
fn ontology_validation_end_to_end_does_not_persist_or_publish() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();
    let objects_before = object_ids(root);

    let repo = open_repository(root).unwrap();
    let context = prepare_change(&repo).unwrap();
    let prepared = apply_create_element(
        context,
        kat::repository::change::CreateElementInput {
            element_id: kat::domain::identity::ElementId::from_uuid(Uuid::from_u128(7)),
            type_id: "kat.core/requirement".into(),
            properties: vec![("title".into(), PropertyValue::Text("A requirement".into()))],
        },
    )
    .unwrap();

    let validated = validate_create_element_ontology(prepared).unwrap();

    // Candidate and element preserved by the validation stage.
    assert_eq!(validated.element.type_id, "kat.core/requirement");
    assert_eq!(validated.candidate_state.elements.len(), 1);

    // Repository untouched.
    assert_eq!(object_ids(root), objects_before);
    assert_eq!(objects_before.len(), 2);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
}

#[test]
fn invariant_validation_end_to_end_has_no_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();
    let objects_before = object_ids(root);

    let repo = open_repository(root).unwrap();
    let context = prepare_change(&repo).unwrap();
    let prepared = apply_create_element(
        context,
        kat::repository::change::CreateElementInput {
            element_id: kat::domain::identity::ElementId::from_uuid(Uuid::from_u128(13)),
            type_id: "kat.core/requirement".into(),
            properties: vec![("title".into(), PropertyValue::Text("A requirement".into()))],
        },
    )
    .unwrap();
    let validated =
        validate_create_element_invariants(validate_create_element_ontology(prepared).unwrap())
            .unwrap();

    assert_eq!(
        validated.prepared().element.lifecycle,
        kat::domain::element::Lifecycle::Active
    );
    assert_eq!(validated.prepared().candidate_state.elements.len(), 1);

    // V1/S1 are logical only: nothing persisted, accepted ref unchanged.
    assert_eq!(object_ids(root), objects_before);
    assert_eq!(objects_before.len(), 2);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
}

#[test]
fn prepare_change_revision_end_to_end_is_preparatory_only() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = init_repository(root).unwrap();
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();
    let objects_before = object_ids(root);

    let repo = open_repository(root).unwrap();
    let context = prepare_change(&repo).unwrap();
    let prepared = apply_create_element(
        context,
        kat::repository::change::CreateElementInput {
            element_id: kat::domain::identity::ElementId::from_uuid(Uuid::from_u128(21)),
            type_id: "kat.core/requirement".into(),
            properties: vec![("title".into(), PropertyValue::Text("A requirement".into()))],
        },
    )
    .unwrap();
    let validated =
        validate_create_element_invariants(validate_create_element_ontology(prepared).unwrap())
            .unwrap();

    let change_id = kat::domain::identity::ChangeId::from_uuid(Uuid::from_u128(100));
    let revision =
        prepare_change_revision(validated, change_id, Some("create requirement".into())).unwrap();

    // base_states == [S0]; dependencies == [] (accepted.change == none).
    assert_eq!(revision.change.base_states, vec![init.state]);
    assert_eq!(revision.change.dependencies, vec![]);
    assert_eq!(revision.change.change_id, change_id);
    assert_eq!(
        revision.change.description.as_deref(),
        Some("create requirement")
    );

    // Operations: exactly one CreateElement -> V1.
    assert_eq!(revision.change.operations.len(), 1);
    assert!(matches!(
        &revision.change.operations[0],
        kat::domain::operation::Operation::CreateElement { new_version } if *new_version == revision.creation.element_version_id
    ));

    // result_state == S1 ObjectId == canonical id of candidate state.
    let expected_state_id =
        kat::encoding::canonical_object_id(&kat::encoding::object::CanonicalObject {
            payload: kat::encoding::object::CanonicalPayload::SemanticState(
                revision.creation.candidate_state.clone(),
            ),
        })
        .unwrap();
    assert_eq!(revision.state_id, expected_state_id);
    assert_eq!(revision.change.result_state, revision.state_id);

    // change_revision_id == canonical id of the ChangeRevision.
    let expected_change_id =
        kat::encoding::canonical_object_id(&kat::encoding::object::CanonicalObject {
            payload: kat::encoding::object::CanonicalPayload::ChangeRevision(
                revision.change.clone(),
            ),
        })
        .unwrap();
    assert_eq!(revision.change_revision_id, expected_change_id);

    // Still purely preparatory: V1/S1/C1 not persisted; accepted ref unchanged.
    assert_eq!(object_ids(root), objects_before);
    assert_eq!(objects_before.len(), 2);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
}

#[test]
fn persist_prepared_change_materializes_v1_s1_c1_and_leaves_accepted_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = init_repository(root).unwrap();
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();

    let repo = open_repository(root).unwrap();
    let context = prepare_change(&repo).unwrap();
    let prepared = apply_create_element(
        context,
        kat::repository::change::CreateElementInput {
            element_id: kat::domain::identity::ElementId::from_uuid(Uuid::from_u128(31)),
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
        kat::domain::identity::ChangeId::from_uuid(Uuid::from_u128(131)),
        Some("persist test".into()),
    )
    .unwrap();

    let expected_v1_id = revision.creation.element_version_id;
    let expected_s1_id = revision.state_id;
    let expected_c1_id = revision.change_revision_id;
    let expected_v1 = revision.creation.element.clone();
    let expected_s1 = revision.creation.candidate_state.clone();
    let expected_c1 = revision.change.clone();

    let persisted = persist_prepared_change(&repo, revision).unwrap();

    assert_eq!(
        persisted.prepared.creation.element_version_id,
        expected_v1_id
    );
    assert_eq!(persisted.prepared.state_id, expected_s1_id);
    assert_eq!(persisted.prepared.change_revision_id, expected_c1_id);

    let v1_bytes = repo.object_store().get(expected_v1_id).unwrap();
    let s1_bytes = repo.object_store().get(expected_s1_id).unwrap();
    let c1_bytes = repo.object_store().get(expected_c1_id).unwrap();

    let decoded_v1 = match kat::encoding::decode_canonical(&v1_bytes).unwrap().payload {
        kat::encoding::object::CanonicalPayload::KnowledgeElementVersion(v) => v,
        other => panic!("expected KnowledgeElementVersion, got {other:?}"),
    };
    let decoded_s1 = match kat::encoding::decode_canonical(&s1_bytes).unwrap().payload {
        kat::encoding::object::CanonicalPayload::SemanticState(s) => s,
        other => panic!("expected SemanticState, got {other:?}"),
    };
    let decoded_c1 = match kat::encoding::decode_canonical(&c1_bytes).unwrap().payload {
        kat::encoding::object::CanonicalPayload::ChangeRevision(c) => c,
        other => panic!("expected ChangeRevision, got {other:?}"),
    };

    assert_eq!(decoded_v1, expected_v1);
    assert_eq!(decoded_s1, expected_s1);
    assert_eq!(decoded_c1, expected_c1);

    let objects = object_ids(root);
    assert!(objects.contains(&init.ontology.to_string()));
    assert!(objects.contains(&init.state.to_string()));
    assert!(objects.contains(&expected_v1_id.to_string()));
    assert!(objects.contains(&expected_s1_id.to_string()));
    assert!(objects.contains(&expected_c1_id.to_string()));

    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
}

#[test]
fn repository_reopen_after_persist_before_publish_still_opens_accepted_s0() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = init_repository(root).unwrap();
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();

    let repo = open_repository(root).unwrap();
    let context = prepare_change(&repo).unwrap();
    let prepared = apply_create_element(
        context,
        kat::repository::change::CreateElementInput {
            element_id: kat::domain::identity::ElementId::from_uuid(Uuid::from_u128(41)),
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
        kat::domain::identity::ChangeId::from_uuid(Uuid::from_u128(141)),
        None,
    )
    .unwrap();

    persist_prepared_change(&repo, revision).unwrap();

    let reopened = open_repository(root).unwrap();
    assert_eq!(reopened.accepted.state, init.state);
    assert_eq!(reopened.accepted.change, None);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
}

#[test]
fn persist_prepared_change_is_idempotent_for_same_prepared_revision() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let repo = open_repository(root).unwrap();
    let context = prepare_change(&repo).unwrap();
    let prepared = apply_create_element(
        context,
        kat::repository::change::CreateElementInput {
            element_id: kat::domain::identity::ElementId::from_uuid(Uuid::from_u128(51)),
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
        kat::domain::identity::ChangeId::from_uuid(Uuid::from_u128(151)),
        Some("idempotent persist".into()),
    )
    .unwrap();

    let expected_v1_id = revision.creation.element_version_id;
    let expected_s1_id = revision.state_id;
    let expected_c1_id = revision.change_revision_id;

    let first = persist_prepared_change(&repo, revision).unwrap();
    assert_eq!(first.prepared.creation.element_version_id, expected_v1_id);
    assert_eq!(first.prepared.state_id, expected_s1_id);
    assert_eq!(first.prepared.change_revision_id, expected_c1_id);
    let second = persist_prepared_change(&repo, first.prepared).unwrap();

    assert_eq!(second.prepared.creation.element_version_id, expected_v1_id);
    assert_eq!(second.prepared.state_id, expected_s1_id);
    assert_eq!(second.prepared.change_revision_id, expected_c1_id);

    let objects = object_ids(root);
    assert_eq!(objects.len(), 5);
    assert!(objects.contains(&expected_v1_id.to_string()));
    assert!(objects.contains(&expected_s1_id.to_string()));
    assert!(objects.contains(&expected_c1_id.to_string()));
}

#[test]
fn publish_first_change_advances_accepted_head_and_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();

    let repo = open_repository(root).unwrap();
    let persisted = prepare_and_persist(&repo, 61, 161);

    let v1 = persisted.prepared.creation.element_version_id;
    let s1 = persisted.prepared.state_id;
    let c1 = persisted.prepared.change_revision_id;
    let objects_before_publish = object_ids(root);

    let published = publish_persisted_change(&repo, persisted).unwrap();

    // accepted head advanced to { state: S1, change: Some(C1) }.
    assert_eq!(published.accepted.state, s1);
    assert_eq!(published.accepted.change, Some(c1));

    // The critical relationship: accepted S1 == C1.result_state.
    assert_eq!(published.persisted.prepared.change.result_state, s1);
    assert_eq!(
        published.accepted.state,
        published.persisted.prepared.change.result_state
    );

    // refs/accepted now carries the new head (changed from {S0, none}).
    let refs_after = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();
    assert_ne!(refs_after, refs_before);
    assert_eq!(refs_after, published.accepted.to_string());

    // Publication changes only refs/accepted: no new immutable object is
    // created. Still exactly O1, S0, V1, S1, C1.
    assert_eq!(object_ids(root), objects_before_publish);
    assert_eq!(objects_before_publish.len(), 5);

    // A fresh process reopens at the new head and E61 resolves to V1.
    let reopened = open_repository(root).unwrap();
    assert_eq!(reopened.accepted.state, s1);
    assert_eq!(reopened.accepted.change, Some(c1));

    let context = prepare_change(&reopened).unwrap();
    assert_eq!(context.base_state_id, s1);
    assert_eq!(context.base_state.elements.len(), 1);
    assert_eq!(
        context.base_state.elements[0].element_id,
        kat::domain::identity::ElementId::from_uuid(Uuid::from_u128(61))
    );
    assert_eq!(context.base_state.elements[0].version, v1);
}

#[test]
fn publish_conflicts_when_accepted_ref_moved_since_preparation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = init_repository(root).unwrap();

    let repo = open_repository(root).unwrap();
    let persisted = prepare_and_persist(&repo, 62, 162);
    let objects_before = object_ids(root);

    // A concurrent writer advances the head after this change was prepared
    // (simulated with a direct CAS against the ref store).
    let other_winner = AcceptedRef {
        state: object_id(0xAA),
        change: Some(object_id(0xBB)),
    };
    repo.ref_store()
        .compare_and_swap_accepted(
            &AcceptedRef {
                state: init.state,
                change: None,
            },
            &other_winner,
        )
        .unwrap();

    // Publishing the stale change fails with the domain-level conflict.
    let err = publish_persisted_change(&repo, persisted).unwrap_err();
    assert!(matches!(
        err,
        kat::repository::change::ChangeError::Conflict
    ));

    // The accepted head remains the concurrent winner; the failed publication
    // created no new objects and nothing was rolled back.
    assert_eq!(repo.ref_store().read_accepted().unwrap(), other_winner);
    assert_eq!(object_ids(root), objects_before);
}

#[test]
fn two_writers_from_s0_exactly_one_publication_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let repo = open_repository(root).unwrap();
    // Both writers prepare + persist from the same accepted {S0, none}.
    let writer_a = prepare_and_persist(&repo, 63, 163);
    let writer_b = prepare_and_persist(&repo, 64, 164);

    let a_v1 = writer_a.prepared.creation.element_version_id;
    let a_s1 = writer_a.prepared.state_id;
    let a_c1 = writer_a.prepared.change_revision_id;
    let b_v1 = writer_b.prepared.creation.element_version_id;
    let b_s1 = writer_b.prepared.state_id;
    let b_c1 = writer_b.prepared.change_revision_id;

    // Writer A publishes: {S0, none} -> {S1A, C1A} succeeds.
    let published_a = publish_persisted_change(&repo, writer_a).unwrap();
    assert_eq!(published_a.accepted.state, a_s1);
    assert_eq!(published_a.accepted.change, Some(a_c1));

    // Writer B publishes with the same expected {S0, none}: the CAS sees
    // {S1A, C1A} and fails. Exactly one publication wins.
    let err = publish_persisted_change(&repo, writer_b).unwrap_err();
    assert!(matches!(
        err,
        kat::repository::change::ChangeError::Conflict
    ));

    // The accepted head is A's.
    assert_eq!(
        repo.ref_store().read_accepted().unwrap(),
        published_a.accepted
    );

    // B's objects remain stored but unreachable from the accepted head.
    let objects = object_ids(root);
    assert_eq!(objects.len(), 8); // O1, S0 + A{V1,S1,C1} + B{V1,S1,C1}
    for id in [a_v1, a_s1, a_c1, b_v1, b_s1, b_c1] {
        assert!(objects.contains(&id.to_string()));
    }

    // A fresh process opens at A's head — the losing change is not
    // authoritative (open integrity verifies only the reachable head).
    let reopened = open_repository(root).unwrap();
    assert_eq!(reopened.accepted.state, a_s1);
    assert_eq!(reopened.accepted.change, Some(a_c1));
    let context = prepare_change(&reopened).unwrap();
    assert_eq!(context.base_state.elements.len(), 1);
    assert_eq!(context.base_state.elements[0].version, a_v1);
}

#[test]
fn publish_rejects_internally_inconsistent_prepared_change() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = init_repository(root).unwrap();

    let repo = open_repository(root).unwrap();
    let mut persisted = prepare_and_persist(&repo, 65, 165);

    // Tamper so the ChangeRevision claims a result state that is not the
    // prepared S1. Fields are public, so this simulates an inconsistent
    // revision reaching the publication boundary.
    let mut tampered = persisted.prepared.change.clone();
    tampered.result_state = object_id(0xEE);
    persisted.prepared.change = tampered;

    // Publication must refuse to make the inconsistent Change authoritative.
    let err = publish_persisted_change(&repo, persisted).unwrap_err();
    assert!(matches!(
        err,
        kat::repository::change::ChangeError::PublicationStateMismatch { .. }
    ));

    // Nothing was published: the accepted head is still {S0, none}.
    assert_eq!(
        repo.ref_store().read_accepted().unwrap(),
        AcceptedRef {
            state: init.state,
            change: None,
        }
    );
}
