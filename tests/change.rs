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

use kat::domain::element::{KnowledgeElementVersion, Lifecycle};
use kat::domain::identity::{ChangeId, ElementId, ObjectId, OntologyId, RelationshipId};
use kat::domain::ontology::{ElementTypeDefinition, OntologyVersion};
use kat::domain::property::PropertyValue;
use kat::domain::state::{ElementStateEntry, RelationshipStateEntry, SemanticState};
use kat::encoding::canonical_bytes;
use kat::encoding::canonical_object_id;
use kat::encoding::object::{CanonicalObject, CanonicalPayload, ObjectKind};
use kat::repository::change::{
    ChangeContext, ChangeError, CreateElementInput, DeprecateElementInput, PreconditionError,
    PreparedElementUpdate, UpdateElementInput, apply_create_element, apply_deprecate_element,
    apply_update_element, persist_prepared_change, persist_prepared_update_change, prepare_change,
    prepare_change_revision, prepare_update_change_revision, publish_persisted_change,
    publish_persisted_update_change, validate_create_element_invariants,
    validate_create_element_ontology, validate_deprecate_element_ontology,
    validate_update_element_invariants, validate_update_element_ontology,
};
use kat::repository::init::init_repository;
use kat::repository::open::{Repository, open_repository};
use kat::repository::ref_store::{AcceptedRef, RefStore};
use kat::repository::validation::invariant::InvariantError;
use kat::repository::validation::invariant::validate_update_element_invariants as validate_update_candidate_invariants;
use kat::repository::validation::ontology::OntologyError;
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

// ---------------------------------------------------------------------------
// Step 2.1 — apply_update_element
// ---------------------------------------------------------------------------

/// A fresh repository with one published Active element (properties `title`
/// "Original" and `priority` "medium") at the accepted head, plus a fresh
/// open handle (so `prepare_change` sees the published head).
struct RepoWithElement {
    _dir: tempfile::TempDir,
    root: PathBuf,
    repo: Repository,
    element_id: ElementId,
    version_id: ObjectId,
    state_id: ObjectId,
}

fn repo_with_element(n: u128, change_n: u128) -> RepoWithElement {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    init_repository(&root).unwrap();

    let element_id = ElementId::from_uuid(Uuid::from_u128(n));
    let (version_id, state_id) = {
        let repo = open_repository(&root).unwrap();
        let context = prepare_change(&repo).unwrap();
        let prepared = apply_create_element(
            context,
            CreateElementInput {
                element_id,
                type_id: "kat.core/requirement".into(),
                properties: vec![
                    ("title".into(), PropertyValue::Text("Original".into())),
                    ("priority".into(), PropertyValue::Text("medium".into())),
                ],
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
        let state_id = revision.state_id;
        let persisted = persist_prepared_change(&repo, revision).unwrap();
        publish_persisted_change(&repo, persisted).unwrap();
        (version_id, state_id)
    };

    // Reopen so the handle's accepted snapshot reflects the published head.
    let repo = open_repository(&root).unwrap();
    RepoWithElement {
        _dir: dir,
        root,
        repo,
        element_id,
        version_id,
        state_id,
    }
}

/// Prepares a fresh change against `repo`'s accepted head and applies an
/// `UpdateElement` for the given element/patch.
fn prepare_update(
    repo: &Repository,
    element_id: ElementId,
    expected_version: ObjectId,
    properties: Vec<(&str, PropertyValue)>,
) -> Result<kat::repository::change::PreparedElementUpdate, ChangeError> {
    let context = prepare_change(repo).unwrap();
    apply_update_element(
        repo,
        context,
        UpdateElementInput {
            element_id,
            expected_version,
            properties: properties
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        },
    )
}

/// Persists an arbitrary KnowledgeElementVersion into the repo's store.
fn store_element(repo: &Repository, element: KnowledgeElementVersion) -> ObjectId {
    let bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::KnowledgeElementVersion(element),
    })
    .unwrap();
    repo.object_store().put(&bytes).unwrap()
}

/// Replaces the accepted head with `{ state, change: None }` (CAS from the
/// current head) and returns a freshly-opened handle.
fn adopt_state(root: &Path, state_id: ObjectId) -> Repository {
    let repo = open_repository(root).unwrap();
    let current = repo.ref_store().read_accepted().unwrap();
    repo.ref_store()
        .compare_and_swap_accepted(
            &current,
            &AcceptedRef {
                state: state_id,
                change: None,
            },
        )
        .unwrap();
    open_repository(root).unwrap()
}

#[test]
fn update_applies_patch_preserving_unspecified_properties() {
    let setup = repo_with_element(101, 201);
    let repo = &setup.repo;

    let prepared = prepare_update(
        repo,
        setup.element_id,
        setup.version_id,
        vec![("title", PropertyValue::Text("Changed".into()))],
    )
    .unwrap();

    // Previous version loaded and carried (canonical property order is by the
    // full encoded text form, so "title" (len 5) sorts before "priority" (len 8)).
    assert_eq!(prepared.previous_version_id, setup.version_id);
    assert_eq!(prepared.previous_element.element_id, setup.element_id);
    assert_eq!(prepared.previous_element.type_id, "kat.core/requirement");
    assert_eq!(prepared.previous_element.lifecycle, Lifecycle::Active);
    assert_eq!(
        prepared.previous_element.properties,
        vec![
            ("title".to_string(), PropertyValue::Text("Original".into())),
            ("priority".to_string(), PropertyValue::Text("medium".into())),
        ]
    );

    // Vn+1: identity/type/lifecycle preserved; title replaced; priority kept.
    assert_eq!(prepared.element.element_id, setup.element_id);
    assert_eq!(prepared.element.type_id, "kat.core/requirement");
    assert_eq!(prepared.element.lifecycle, Lifecycle::Active);
    assert_eq!(
        prepared.element.properties,
        vec![
            ("title".to_string(), PropertyValue::Text("Changed".into())),
            ("priority".to_string(), PropertyValue::Text("medium".into())),
        ]
    );

    // New content identity, distinct from Vn and equal to encode-then-hash.
    assert_ne!(prepared.element_version_id, setup.version_id);
    let expected_id = canonical_object_id(&CanonicalObject {
        payload: CanonicalPayload::KnowledgeElementVersion(prepared.element.clone()),
    })
    .unwrap();
    assert_eq!(prepared.element_version_id, expected_id);

    // Candidate: exactly E -> Vn+1; ontology/relationships unchanged.
    assert_eq!(
        prepared.candidate_state.ontology_version,
        prepared.context.base_state.ontology_version
    );
    assert_eq!(
        prepared.candidate_state.relationships,
        prepared.context.base_state.relationships
    );
    assert_eq!(prepared.candidate_state.elements.len(), 1);
    assert_eq!(
        prepared.candidate_state.elements[0].element_id,
        setup.element_id
    );
    assert_eq!(
        prepared.candidate_state.elements[0].version,
        prepared.element_version_id
    );

    // Base state untouched; nothing persisted (O1, S0 + V1, S1, C1 = 5 objects);
    // the accepted head is the published one and is unchanged.
    assert_eq!(
        prepared.context.base_state.elements[0].version,
        setup.version_id
    );
    assert_eq!(object_ids(&setup.root).len(), 5);
    assert_eq!(prepared.context.accepted.state, setup.state_id);
    assert!(prepared.context.accepted.change.is_some());
}

#[test]
fn update_adds_new_property_preserving_existing() {
    let setup = repo_with_element(102, 202);
    let repo = &setup.repo;

    let prepared = prepare_update(
        repo,
        setup.element_id,
        setup.version_id,
        vec![("status", PropertyValue::Text("new".into()))],
    )
    .unwrap();

    // Canonical encoded-text order: "title" (0x65), "status" (0x66), "priority" (0x68).
    assert_eq!(
        prepared.element.properties,
        vec![
            ("title".to_string(), PropertyValue::Text("Original".into())),
            ("status".to_string(), PropertyValue::Text("new".into())),
            ("priority".to_string(), PropertyValue::Text("medium".into())),
        ]
    );
}

#[test]
fn update_rejects_missing_element() {
    let setup = repo_with_element(103, 203);
    let repo = &setup.repo;
    let missing = ElementId::from_uuid(Uuid::from_u128(999));

    let err = prepare_update(
        repo,
        missing,
        setup.version_id,
        vec![("title", PropertyValue::Text("x".into()))],
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ChangeError::Precondition(PreconditionError::ElementNotFound(id)) if id == missing
    ));
}

#[test]
fn update_rejects_version_mismatch() {
    let setup = repo_with_element(104, 204);
    let repo = &setup.repo;
    let wrong_expected = object_id(0xAB);

    let err = prepare_update(
        repo,
        setup.element_id,
        wrong_expected,
        vec![("title", PropertyValue::Text("x".into()))],
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ChangeError::Precondition(PreconditionError::VersionMismatch {
            element_id,
            expected,
            actual,
        }) if element_id == setup.element_id
            && expected == wrong_expected
            && actual == setup.version_id
    ));
}

#[test]
fn update_rejects_non_active_element() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = init_repository(root).unwrap();

    let element_id = ElementId::from_uuid(Uuid::from_u128(105));
    let deprecated = {
        let repo = open_repository(root).unwrap();
        store_element(
            &repo,
            KnowledgeElementVersion {
                element_id,
                type_id: "kat.core/requirement".into(),
                lifecycle: Lifecycle::Deprecated,
                properties: vec![],
            },
        )
    };
    let state = {
        let repo = open_repository(root).unwrap();
        let bytes = canonical_bytes(&CanonicalObject {
            payload: CanonicalPayload::SemanticState(SemanticState {
                ontology_version: init.ontology,
                elements: vec![ElementStateEntry {
                    element_id,
                    version: deprecated,
                }],
                relationships: vec![],
            }),
        })
        .unwrap();
        repo.object_store().put(&bytes).unwrap()
    };
    let repo = adopt_state(root, state);

    let context = prepare_change(&repo).unwrap();
    let err = apply_update_element(
        &repo,
        context,
        UpdateElementInput {
            element_id,
            expected_version: deprecated,
            properties: vec![("title".into(), PropertyValue::Text("x".into()))],
        },
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ChangeError::Precondition(PreconditionError::ElementNotActive(id)) if id == element_id
    ));
}

#[test]
fn update_rejects_empty_patch() {
    let setup = repo_with_element(106, 206);
    let repo = &setup.repo;

    let err = prepare_update(repo, setup.element_id, setup.version_id, vec![]).unwrap_err();
    assert!(matches!(
        err,
        ChangeError::Precondition(PreconditionError::EmptyUpdate)
    ));
}

#[test]
fn update_rejects_no_effective_change() {
    let setup = repo_with_element(107, 207);
    let repo = &setup.repo;

    // Setting a property to its current value yields an identical Vn+1.
    let err = prepare_update(
        repo,
        setup.element_id,
        setup.version_id,
        vec![("title", PropertyValue::Text("Original".into()))],
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ChangeError::Precondition(PreconditionError::NoEffectiveChange)
    ));
}

#[test]
fn update_rejects_duplicate_patch_key() {
    let setup = repo_with_element(108, 208);
    let repo = &setup.repo;

    let err = prepare_update(
        repo,
        setup.element_id,
        setup.version_id,
        vec![
            ("title", PropertyValue::Text("a".into())),
            ("title", PropertyValue::Text("b".into())),
        ],
    )
    .unwrap_err();
    assert!(matches!(err, ChangeError::DuplicatePropertyKey(k) if k == "title"));
}

#[test]
fn update_rejects_wrong_kind_current_version() {
    // A base state mapping E to an OntologyVersion can never be *opened* (the
    // repository-open layer rejects wrong-kind element versions first, as
    // tested in tests/open.rs). The engine's own kind check is therefore
    // defense-in-depth; this test reaches it directly with a manual context
    // whose element slot references the OntologyVersion (O1) in the store.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = init_repository(root).unwrap();
    let repo = open_repository(root).unwrap();

    let element_id = ElementId::from_uuid(Uuid::from_u128(109));
    let wrong_kind = init.ontology;
    let base = prepare_change(&repo).unwrap();
    let context = ChangeContext {
        base_state: SemanticState {
            ontology_version: init.ontology,
            elements: vec![ElementStateEntry {
                element_id,
                version: wrong_kind,
            }],
            relationships: vec![],
        },
        ..base
    };

    let err = apply_update_element(
        &repo,
        context,
        UpdateElementInput {
            element_id,
            expected_version: wrong_kind,
            properties: vec![("title".into(), PropertyValue::Text("x".into()))],
        },
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ChangeError::UnexpectedObjectKind {
            expected: ObjectKind::KnowledgeElementVersion,
            actual: ObjectKind::OntologyVersion,
        }
    ));
}

// ---------------------------------------------------------------------------
// Step 2.2 — validate_update_element_ontology
// ---------------------------------------------------------------------------

#[test]
fn update_ontology_accepts_known_type() {
    let setup = repo_with_element(110, 210);
    let repo = &setup.repo;

    let prepared = prepare_update(
        repo,
        setup.element_id,
        setup.version_id,
        vec![("title", PropertyValue::Text("B".into()))],
    )
    .unwrap();
    let validated = validate_update_element_ontology(prepared).unwrap();
    assert_eq!(validated.element.type_id, "kat.core/requirement");
}

#[test]
fn update_ontology_rejects_unknown_type() {
    // A naturally-reachable repository cannot hold an element whose type is
    // missing from its ontology (every prior change was validated), so the
    // failure path uses a manually tampered Vn+1 to pin the boundary.
    let setup = repo_with_element(111, 211);
    let repo = &setup.repo;

    let mut prepared = prepare_update(
        repo,
        setup.element_id,
        setup.version_id,
        vec![("title", PropertyValue::Text("B".into()))],
    )
    .unwrap();
    prepared.element.type_id = "kat.core/not-a-real-type".into();

    let err = validate_update_element_ontology(prepared).unwrap_err();
    assert!(matches!(
        err,
        ChangeError::Ontology(OntologyError::UnknownElementType(t))
            if t == "kat.core/not-a-real-type"
    ));
}

#[test]
fn update_ontology_uses_base_ontology_not_global_core() {
    let setup = repo_with_element(112, 212);
    let repo = &setup.repo;

    let mut prepared = prepare_update(
        repo,
        setup.element_id,
        setup.version_id,
        vec![("title", PropertyValue::Text("B".into()))],
    )
    .unwrap();
    // A custom authoritative base ontology that does NOT define "requirement".
    prepared.context.ontology = OntologyVersion {
        ontology_id: OntologyId::from_uuid(Uuid::from_u128(999)),
        element_types: vec![ElementTypeDefinition {
            type_id: "kat.core/constraint".into(),
            name: "Constraint".into(),
        }],
        relationship_types: vec![],
    };

    // "requirement" is not in the base ontology -> rejected, even though the
    // global core ontology would accept it.
    let err = validate_update_element_ontology(prepared).unwrap_err();
    assert!(matches!(
        err,
        ChangeError::Ontology(OntologyError::UnknownElementType(t))
            if t == "kat.core/requirement"
    ));
}

#[test]
fn update_ontology_preserves_prepared_and_changes_nothing() {
    let setup = repo_with_element(113, 213);
    let repo = &setup.repo;
    let objects_before = object_ids(&setup.root);
    let accepted_before = repo.ref_store().read_accepted().unwrap();

    let prepared = prepare_update(
        repo,
        setup.element_id,
        setup.version_id,
        vec![("title", PropertyValue::Text("B".into()))],
    )
    .unwrap();
    let element_before = prepared.element.clone();
    let candidate_before = prepared.candidate_state.clone();

    let validated = validate_update_element_ontology(prepared).unwrap();

    // The prepared update is returned unchanged.
    assert_eq!(validated.element, element_before);
    assert_eq!(validated.candidate_state, candidate_before);

    // Object store and accepted ref are unchanged.
    assert_eq!(object_ids(&setup.root), objects_before);
    assert_eq!(repo.ref_store().read_accepted().unwrap(), accepted_before);
}

// ---------------------------------------------------------------------------
// Step 2.3 — validate_update_element_invariants
// ---------------------------------------------------------------------------

/// Publishes one element from the repo's current accepted head (reopening so
/// `prepare_change` sees the latest head), returning (element_id, version_id).
fn publish_one(root: &Path, n: u128, change_n: u128) -> (ElementId, ObjectId) {
    let repo = open_repository(root).unwrap();
    let element_id = ElementId::from_uuid(Uuid::from_u128(n));
    let context = prepare_change(&repo).unwrap();
    let prepared = apply_create_element(
        context,
        CreateElementInput {
            element_id,
            type_id: "kat.core/requirement".into(),
            properties: vec![("title".into(), PropertyValue::Text("T".into()))],
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

/// A repo with two published Active elements at the accepted head.
struct RepoWithTwo {
    _dir: tempfile::TempDir,
    repo: Repository,
    e1: ElementId,
    v1: ObjectId,
    e2: ElementId,
}

fn repo_with_two_elements() -> RepoWithTwo {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    init_repository(&root).unwrap();
    let (e1, v1) = publish_one(&root, 301, 401);
    let (e2, _v2) = publish_one(&root, 302, 402);
    let repo = open_repository(&root).unwrap();
    RepoWithTwo {
        _dir: dir,
        repo,
        e1,
        v1,
        e2,
    }
}

/// Prepares a valid (unvalidated) `UpdateElement` for `setup`'s element.
fn valid_update(setup: &RepoWithElement) -> PreparedElementUpdate {
    let context = prepare_change(&setup.repo).unwrap();
    apply_update_element(
        &setup.repo,
        context,
        UpdateElementInput {
            element_id: setup.element_id,
            expected_version: setup.version_id,
            properties: vec![("title".into(), PropertyValue::Text("B".into()))],
        },
    )
    .unwrap()
}

/// Prepares a valid (unvalidated) `UpdateElement` for an arbitrary repo/element.
fn valid_update_for(
    repo: &Repository,
    element_id: ElementId,
    version_id: ObjectId,
) -> PreparedElementUpdate {
    let context = prepare_change(repo).unwrap();
    apply_update_element(
        repo,
        context,
        UpdateElementInput {
            element_id,
            expected_version: version_id,
            properties: vec![("title".into(), PropertyValue::Text("B".into()))],
        },
    )
    .unwrap()
}

#[test]
fn update_invariants_valid_passes() {
    let setup = repo_with_element(114, 214);
    let prepared = valid_update(&setup);
    let validated = validate_update_element_invariants(prepared).unwrap();
    assert_eq!(validated.prepared().element.element_id, setup.element_id);
    assert_eq!(validated.prepared().element.type_id, "kat.core/requirement");
}

#[test]
fn update_invariants_identity_changed_fails() {
    let setup = repo_with_element(115, 215);
    let mut prepared = valid_update(&setup);
    prepared.element.element_id = ElementId::from_uuid(Uuid::from_u128(999));
    let err = validate_update_candidate_invariants(&prepared).unwrap_err();
    assert_eq!(err, InvariantError::UpdateIdentityChanged);
}

#[test]
fn update_invariants_type_changed_fails() {
    let setup = repo_with_element(116, 216);
    let mut prepared = valid_update(&setup);
    prepared.element.type_id = "kat.core/implementation".into();
    let err = validate_update_candidate_invariants(&prepared).unwrap_err();
    assert_eq!(err, InvariantError::UpdateTypeChanged);
}

#[test]
fn update_invariants_previous_lifecycle_not_active_fails() {
    let setup = repo_with_element(117, 217);
    let mut prepared = valid_update(&setup);
    prepared.previous_element.lifecycle = Lifecycle::Deprecated;
    let err = validate_update_candidate_invariants(&prepared).unwrap_err();
    assert_eq!(err, InvariantError::UpdateLifecycleChanged);
}

#[test]
fn update_invariants_new_lifecycle_not_active_fails() {
    let setup = repo_with_element(118, 218);
    let mut prepared = valid_update(&setup);
    prepared.element.lifecycle = Lifecycle::Superseded;
    let err = validate_update_candidate_invariants(&prepared).unwrap_err();
    assert_eq!(err, InvariantError::UpdateLifecycleChanged);
}

#[test]
fn update_invariants_version_identity_tampered_fails() {
    let setup = repo_with_element(119, 219);
    let mut prepared = valid_update(&setup);
    prepared.element_version_id = object_id(0xEE);
    let err = validate_update_candidate_invariants(&prepared).unwrap_err();
    assert!(matches!(
        err,
        InvariantError::UpdateVersionIdentityMismatch { actual, .. } if actual == object_id(0xEE)
    ));
}

#[test]
fn update_invariants_expected_version_mismatch_fails() {
    let setup = repo_with_element(120, 220);
    let mut prepared = valid_update(&setup);
    prepared.expected_version = object_id(0xAB);
    let err = validate_update_candidate_invariants(&prepared).unwrap_err();
    assert_eq!(err, InvariantError::UpdateBaseVersionMismatch);
}

#[test]
fn update_invariants_candidate_wrong_version_fails() {
    let setup = repo_with_element(121, 221);
    let mut prepared = valid_update(&setup);
    prepared.candidate_state.elements[0].version = object_id(0xDD);
    let err = validate_update_candidate_invariants(&prepared).unwrap_err();
    assert_eq!(err, InvariantError::UpdateCandidateReferenceMismatch);
}

#[test]
fn update_invariants_candidate_keeps_vn_fails() {
    let setup = repo_with_element(122, 222);
    let mut prepared = valid_update(&setup);
    prepared.candidate_state.elements[0].version = prepared.previous_version_id;
    let err = validate_update_candidate_invariants(&prepared).unwrap_err();
    assert_eq!(err, InvariantError::UpdateCandidateReferenceMismatch);
}

#[test]
fn update_invariants_another_element_changed_fails() {
    let two = repo_with_two_elements();
    let mut prepared = valid_update_for(&two.repo, two.e1, two.v1);
    for entry in &mut prepared.candidate_state.elements {
        if entry.element_id == two.e2 {
            entry.version = object_id(0xCC);
        }
    }
    let err = validate_update_candidate_invariants(&prepared).unwrap_err();
    assert_eq!(err, InvariantError::UnexpectedElementMutation);
}

#[test]
fn update_invariants_another_element_inserted_fails() {
    let two = repo_with_two_elements();
    let mut prepared = valid_update_for(&two.repo, two.e1, two.v1);
    // Insert a third entry at its canonical (sorted) position so the candidate
    // stays structurally canonical and only the delta rule can reject it.
    prepared.candidate_state.elements.insert(
        0,
        ElementStateEntry {
            element_id: ElementId::from_uuid(Uuid::from_u128(300)),
            version: object_id(1),
        },
    );
    let err = validate_update_candidate_invariants(&prepared).unwrap_err();
    assert_eq!(err, InvariantError::UnexpectedElementMutation);
}

#[test]
fn update_invariants_another_element_removed_fails() {
    let two = repo_with_two_elements();
    let mut prepared = valid_update_for(&two.repo, two.e1, two.v1);
    prepared
        .candidate_state
        .elements
        .retain(|e| e.element_id != two.e2);
    let err = validate_update_candidate_invariants(&prepared).unwrap_err();
    assert_eq!(err, InvariantError::UnexpectedElementMutation);
}

#[test]
fn update_invariants_ontology_version_changed_fails() {
    let setup = repo_with_element(123, 223);
    let mut prepared = valid_update(&setup);
    prepared.candidate_state.ontology_version = object_id(0xEE);
    let err = validate_update_candidate_invariants(&prepared).unwrap_err();
    assert_eq!(err, InvariantError::OntologyVersionChanged);
}

#[test]
fn update_invariants_relationship_added_fails() {
    let setup = repo_with_element(124, 224);
    let mut prepared = valid_update(&setup);
    prepared
        .candidate_state
        .relationships
        .push(RelationshipStateEntry {
            relationship_id: RelationshipId::from_uuid(Uuid::from_u128(7)),
            version: object_id(4),
        });
    let err = validate_update_candidate_invariants(&prepared).unwrap_err();
    assert_eq!(err, InvariantError::UnexpectedRelationshipMutation);
}

#[test]
fn update_invariants_candidate_noncanonical_fails() {
    let setup = repo_with_element(125, 225);
    let mut prepared = valid_update(&setup);
    // Force unsorted element entries (an entry that sorts before E appended
    // after it) so the candidate is structurally non-canonical.
    prepared.candidate_state.elements.push(ElementStateEntry {
        element_id: ElementId::from_uuid(Uuid::from_u128(1)),
        version: object_id(1),
    });
    let err = validate_update_candidate_invariants(&prepared).unwrap_err();
    assert!(matches!(err, InvariantError::InvalidCanonicalStructure(_)));
}

#[test]
fn update_invariants_no_side_effects() {
    let setup = repo_with_element(126, 226);
    let repo = &setup.repo;
    let objects_before = object_ids(&setup.root);
    let accepted_before = repo.ref_store().read_accepted().unwrap();

    let prepared = valid_update(&setup);
    let _ = validate_update_element_invariants(prepared).unwrap();

    assert_eq!(object_ids(&setup.root), objects_before);
    assert_eq!(repo.ref_store().read_accepted().unwrap(), accepted_before);
}

#[test]
fn prepare_update_change_revision_end_to_end_is_preparatory_only() {
    let setup = repo_with_element(127, 227);
    let objects_before = object_ids(&setup.root);
    let refs_before =
        fs::read_to_string(kat_dir(&setup.root).join("refs").join("accepted")).unwrap();

    let prepared = valid_update(&setup);
    let validated =
        validate_update_element_invariants(validate_update_element_ontology(prepared).unwrap())
            .unwrap();

    let change_id_2 = ChangeId::from_uuid(Uuid::from_u128(300));
    let revision =
        prepare_update_change_revision(validated, change_id_2, Some("update title".into()))
            .unwrap();

    // base_states == [S1]; dependencies == [C1] (accepted.change head from C1).
    assert_eq!(revision.change.base_states, vec![setup.state_id]);
    assert_eq!(
        revision.change.dependencies,
        vec![setup.repo.accepted.change.unwrap()]
    );
    assert_eq!(revision.change.change_id, change_id_2);
    assert_eq!(revision.change.description.as_deref(), Some("update title"));

    // Operations: exactly one UpdateElement.
    assert_eq!(revision.change.operations.len(), 1);
    match &revision.change.operations[0] {
        kat::domain::operation::Operation::UpdateElement {
            element_id,
            expected_version,
            new_version,
        } => {
            assert_eq!(*element_id, setup.element_id);
            assert_eq!(*expected_version, setup.version_id);
            assert_eq!(*expected_version, revision.update.previous_version_id);
            assert_eq!(*new_version, revision.update.element_version_id);
            assert_ne!(*new_version, setup.version_id);
        }
        other => panic!("expected UpdateElement operation, got {:?}", other),
    }

    // result_state == S2 ObjectId == canonical id of candidate state.
    let expected_state_id = canonical_object_id(&CanonicalObject {
        payload: CanonicalPayload::SemanticState(revision.update.candidate_state.clone()),
    })
    .unwrap();
    assert_eq!(revision.state_id, expected_state_id);
    assert_eq!(revision.change.result_state, revision.state_id);

    // change_revision_id == canonical id of the ChangeRevision.
    let expected_change_id = canonical_object_id(&CanonicalObject {
        payload: CanonicalPayload::ChangeRevision(revision.change.clone()),
    })
    .unwrap();
    assert_eq!(revision.change_revision_id, expected_change_id);

    // Still purely preparatory: V2/S2/C2 not persisted; accepted ref unchanged.
    assert_eq!(object_ids(&setup.root), objects_before);
    assert_eq!(
        fs::read_to_string(kat_dir(&setup.root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
}

#[test]
fn prepare_update_change_revision_preserves_none_description() {
    let setup = repo_with_element(128, 228);
    let prepared = valid_update(&setup);
    let validated =
        validate_update_element_invariants(validate_update_element_ontology(prepared).unwrap())
            .unwrap();

    let change_id = ChangeId::from_uuid(Uuid::from_u128(301));
    let revision = prepare_update_change_revision(validated, change_id, None).unwrap();

    assert_eq!(revision.change.description, None);
}

#[test]
fn prepare_update_change_revision_with_no_accepted_change_head() {
    let setup = repo_with_element(129, 229);
    // Force repository head to have change = None via adopt_state
    let repo_no_change_head = adopt_state(&setup.root, setup.state_id);

    let prepared = prepare_update(
        &repo_no_change_head,
        setup.element_id,
        setup.version_id,
        vec![("title", PropertyValue::Text("Updated title".into()))],
    )
    .unwrap();

    let validated =
        validate_update_element_invariants(validate_update_element_ontology(prepared).unwrap())
            .unwrap();

    let change_id = ChangeId::from_uuid(Uuid::from_u128(302));
    let revision = prepare_update_change_revision(validated, change_id, None).unwrap();

    // When accepted.change is None, dependencies is empty [].
    assert_eq!(revision.change.dependencies, vec![]);
    assert_eq!(revision.change.base_states, vec![setup.state_id]);
}

#[test]
fn persist_prepared_update_change_materializes_v2_s2_c2_and_leaves_accepted_unchanged() {
    let setup = repo_with_element(130, 230);
    let repo = &setup.repo;
    let objects_before = object_ids(&setup.root);
    let count_before = objects_before.len();
    let v1_bytes_before = repo.object_store().get(setup.version_id).unwrap();
    let refs_before =
        fs::read_to_string(kat_dir(&setup.root).join("refs").join("accepted")).unwrap();

    let prepared = valid_update(&setup);
    let validated =
        validate_update_element_invariants(validate_update_element_ontology(prepared).unwrap())
            .unwrap();
    let change_id_2 = ChangeId::from_uuid(Uuid::from_u128(303));
    let revision =
        prepare_update_change_revision(validated, change_id_2, Some("update title".into()))
            .unwrap();

    let expected_v2_id = revision.update.element_version_id;
    let expected_s2_id = revision.state_id;
    let expected_c2_id = revision.change_revision_id;
    let expected_v2 = revision.update.element.clone();
    let expected_s2 = revision.update.candidate_state.clone();
    let expected_c2 = revision.change.clone();

    let persisted = persist_prepared_update_change(repo, revision).unwrap();

    assert_eq!(persisted.prepared.update.element_version_id, expected_v2_id);
    assert_eq!(persisted.prepared.state_id, expected_s2_id);
    assert_eq!(persisted.prepared.change_revision_id, expected_c2_id);

    let v2_bytes = repo.object_store().get(expected_v2_id).unwrap();
    let s2_bytes = repo.object_store().get(expected_s2_id).unwrap();
    let c2_bytes = repo.object_store().get(expected_c2_id).unwrap();

    let decoded_v2 = match kat::encoding::decode_canonical(&v2_bytes).unwrap().payload {
        CanonicalPayload::KnowledgeElementVersion(v) => v,
        other => panic!("expected KnowledgeElementVersion, got {other:?}"),
    };
    let decoded_s2 = match kat::encoding::decode_canonical(&s2_bytes).unwrap().payload {
        CanonicalPayload::SemanticState(s) => s,
        other => panic!("expected SemanticState, got {other:?}"),
    };
    let decoded_c2 = match kat::encoding::decode_canonical(&c2_bytes).unwrap().payload {
        CanonicalPayload::ChangeRevision(c) => c,
        other => panic!("expected ChangeRevision, got {other:?}"),
    };

    assert_eq!(decoded_v2, expected_v2);
    assert_eq!(decoded_s2, expected_s2);
    assert_eq!(decoded_c2, expected_c2);

    // V1 still exists and is byte-for-byte unchanged
    let v1_bytes_after = repo.object_store().get(setup.version_id).unwrap();
    assert_eq!(v1_bytes_after, v1_bytes_before);

    // Accepted ref is untouched
    assert_eq!(
        fs::read_to_string(kat_dir(&setup.root).join("refs").join("accepted")).unwrap(),
        refs_before
    );

    // Object count increases by exactly 3
    let objects_after = object_ids(&setup.root);
    assert_eq!(objects_after.len(), count_before + 3);
    assert!(objects_after.contains(&expected_v2_id.to_string()));
    assert!(objects_after.contains(&expected_s2_id.to_string()));
    assert!(objects_after.contains(&expected_c2_id.to_string()));
}

#[test]
fn repository_reopen_and_query_after_persist_update_before_publish_still_resolves_v1() {
    let setup = repo_with_element(131, 231);
    let repo = &setup.repo;

    let prepared = valid_update(&setup);
    let validated =
        validate_update_element_invariants(validate_update_element_ontology(prepared).unwrap())
            .unwrap();
    let change_id_2 = ChangeId::from_uuid(Uuid::from_u128(304));
    let revision =
        prepare_update_change_revision(validated, change_id_2, Some("update title".into()))
            .unwrap();

    let expected_v2_id = revision.update.element_version_id;
    let _persisted = persist_prepared_update_change(repo, revision).unwrap();

    // Reopen repository before publication
    let reopened = open_repository(&setup.root).unwrap();
    assert_eq!(reopened.accepted.state, setup.state_id);
    assert_eq!(reopened.accepted.change, repo.accepted.change);

    // Query show_element still resolves V1, not V2
    let view = kat::repository::show_element(&reopened, setup.element_id).unwrap();
    assert_eq!(view.version_id, setup.version_id);
    assert_ne!(view.version_id, expected_v2_id);
}

#[test]
fn persist_prepared_update_change_is_idempotent() {
    let setup = repo_with_element(132, 232);
    let repo = &setup.repo;
    let prepared = valid_update(&setup);
    let validated =
        validate_update_element_invariants(validate_update_element_ontology(prepared).unwrap())
            .unwrap();
    let change_id_2 = ChangeId::from_uuid(Uuid::from_u128(305));
    let revision =
        prepare_update_change_revision(validated, change_id_2, Some("update title".into()))
            .unwrap();

    let expected_v2_id = revision.update.element_version_id;
    let expected_s2_id = revision.state_id;
    let expected_c2_id = revision.change_revision_id;

    let count_before = object_ids(&setup.root).len();

    let first = persist_prepared_update_change(repo, revision).unwrap();
    let count_after_first = object_ids(&setup.root).len();
    assert_eq!(count_after_first, count_before + 3);

    let second = persist_prepared_update_change(repo, first.prepared).unwrap();
    let count_after_second = object_ids(&setup.root).len();
    assert_eq!(count_after_second, count_after_first);

    assert_eq!(second.prepared.update.element_version_id, expected_v2_id);
    assert_eq!(second.prepared.state_id, expected_s2_id);
    assert_eq!(second.prepared.change_revision_id, expected_c2_id);
}

#[test]
fn publish_persisted_update_change_advances_accepted_head_and_survives_reopen() {
    let setup = repo_with_element(133, 233);
    let repo = &setup.repo;
    let v1_bytes_before = repo.object_store().get(setup.version_id).unwrap();

    let prepared = valid_update(&setup);
    let validated =
        validate_update_element_invariants(validate_update_element_ontology(prepared).unwrap())
            .unwrap();
    let change_id_2 = ChangeId::from_uuid(Uuid::from_u128(306));
    let revision =
        prepare_update_change_revision(validated, change_id_2, Some("update title".into()))
            .unwrap();

    let expected_v2_id = revision.update.element_version_id;
    let expected_s2_id = revision.state_id;
    let expected_c2_id = revision.change_revision_id;

    let persisted = persist_prepared_update_change(repo, revision).unwrap();
    let objects_after_persist = object_ids(&setup.root);

    let published = publish_persisted_update_change(repo, persisted).unwrap();

    // Accepted ref updated to { S2, C2 }
    assert_eq!(published.accepted.state, expected_s2_id);
    assert_eq!(published.accepted.change, Some(expected_c2_id));

    // Publication creates NO additional objects
    assert_eq!(object_ids(&setup.root), objects_after_persist);

    // V1 still exists byte-for-byte unchanged in store
    assert_eq!(
        repo.object_store().get(setup.version_id).unwrap(),
        v1_bytes_before
    );

    // Fresh reopen resolves to V2
    let reopened = open_repository(&setup.root).unwrap();
    assert_eq!(reopened.accepted.state, expected_s2_id);
    assert_eq!(reopened.accepted.change, Some(expected_c2_id));

    let view = kat::repository::show_element(&reopened, setup.element_id).unwrap();
    assert_eq!(view.version_id, expected_v2_id);
    assert_eq!(
        view.element.properties,
        vec![
            ("title".into(), PropertyValue::Text("B".into())),
            ("priority".into(), PropertyValue::Text("medium".into())),
        ]
    );
}

#[test]
fn publish_update_conflicts_when_accepted_ref_moved_since_preparation() {
    let setup = repo_with_element(134, 234);
    let repo = &setup.repo;

    // Writer A and Writer B both prepare from head { S1, C1 }
    let prep_a = valid_update(&setup);
    let val_a =
        validate_update_element_invariants(validate_update_element_ontology(prep_a).unwrap())
            .unwrap();
    let rev_a = prepare_update_change_revision(
        val_a,
        ChangeId::from_uuid(Uuid::from_u128(307)),
        Some("update A".into()),
    )
    .unwrap();
    let pers_a = persist_prepared_update_change(repo, rev_a).unwrap();

    let prep_b = prepare_update(
        repo,
        setup.element_id,
        setup.version_id,
        vec![("priority", PropertyValue::Text("high".into()))],
    )
    .unwrap();
    let val_b =
        validate_update_element_invariants(validate_update_element_ontology(prep_b).unwrap())
            .unwrap();
    let rev_b = prepare_update_change_revision(
        val_b,
        ChangeId::from_uuid(Uuid::from_u128(308)),
        Some("update B".into()),
    )
    .unwrap();
    let b_v2_id = rev_b.update.element_version_id;
    let b_s2_id = rev_b.state_id;
    let b_c2_id = rev_b.change_revision_id;
    let pers_b = persist_prepared_update_change(repo, rev_b).unwrap();

    // Writer A publishes: { S1, C1 } -> { S2A, C2A } succeeds
    let pub_a = publish_persisted_update_change(repo, pers_a).unwrap();
    assert_eq!(pub_a.accepted.state, pub_a.persisted.prepared.state_id);

    // Writer B publishes with expected { S1, C1 }: CAS fails with Conflict
    let err = publish_persisted_update_change(repo, pers_b).unwrap_err();
    assert!(matches!(err, ChangeError::Conflict));

    // Accepted head remains Writer A's winner
    assert_eq!(repo.ref_store().read_accepted().unwrap(), pub_a.accepted);

    // Writer B's objects remain stored in ObjectStore but unreferenced by head
    let objects = object_ids(&setup.root);
    assert!(objects.contains(&b_v2_id.to_string()));
    assert!(objects.contains(&b_s2_id.to_string()));
    assert!(objects.contains(&b_c2_id.to_string()));
}

#[test]
fn publish_update_rejects_internally_inconsistent_prepared_change() {
    let setup = repo_with_element(135, 235);
    let repo = &setup.repo;

    let prepared = valid_update(&setup);
    let validated =
        validate_update_element_invariants(validate_update_element_ontology(prepared).unwrap())
            .unwrap();
    let revision =
        prepare_update_change_revision(validated, ChangeId::from_uuid(Uuid::from_u128(309)), None)
            .unwrap();

    let mut persisted = persist_prepared_update_change(repo, revision).unwrap();

    // Tamper with result_state before publication
    let mut tampered = persisted.prepared.change.clone();
    tampered.result_state = object_id(0xFF);
    persisted.prepared.change = tampered;

    let err = publish_persisted_update_change(repo, persisted).unwrap_err();
    assert!(matches!(err, ChangeError::PublicationStateMismatch { .. }));

    // Accepted ref unchanged
    assert_eq!(
        repo.ref_store().read_accepted().unwrap().state,
        setup.state_id
    );
}

// ---------------------------------------------------------------------------
// Step 3.1 — apply_deprecate_element
// ---------------------------------------------------------------------------

#[test]
fn deprecate_applies_candidate_with_lifecycle_deprecated() {
    let setup = repo_with_element(140, 240);
    let repo = &setup.repo;
    let context = prepare_change(repo).unwrap();

    let prepared = apply_deprecate_element(
        repo,
        context,
        DeprecateElementInput {
            element_id: setup.element_id,
            expected_version: setup.version_id,
        },
    )
    .unwrap();

    assert_eq!(prepared.element.element_id, setup.element_id);
    assert_eq!(prepared.element.lifecycle, Lifecycle::Deprecated);
    assert_eq!(prepared.previous_version_id, setup.version_id);
    assert_eq!(
        prepared.element.properties,
        prepared.previous_element.properties
    );
    assert_eq!(prepared.element.type_id, prepared.previous_element.type_id);
    assert_ne!(prepared.element_version_id, setup.version_id);

    // Candidate state maps E1 -> V2
    assert_eq!(prepared.candidate_state.elements.len(), 1);
    assert_eq!(
        prepared.candidate_state.elements[0].element_id,
        setup.element_id
    );
    assert_eq!(
        prepared.candidate_state.elements[0].version,
        prepared.element_version_id
    );

    // Purely preparatory: nothing persisted or published
    assert_eq!(
        repo.ref_store().read_accepted().unwrap().state,
        setup.state_id
    );
}

#[test]
fn deprecate_rejects_missing_element() {
    let setup = repo_with_element(141, 241);
    let repo = &setup.repo;
    let context = prepare_change(repo).unwrap();
    let missing_id = ElementId::from_uuid(Uuid::from_u128(999));

    let err = apply_deprecate_element(
        repo,
        context,
        DeprecateElementInput {
            element_id: missing_id,
            expected_version: object_id(0x11),
        },
    )
    .unwrap_err();

    assert!(matches!(
        err,
        ChangeError::Precondition(PreconditionError::ElementNotFound(id)) if id == missing_id
    ));
}

#[test]
fn deprecate_rejects_version_mismatch() {
    let setup = repo_with_element(142, 242);
    let repo = &setup.repo;
    let context = prepare_change(repo).unwrap();
    let wrong_version = object_id(0x88);

    let err = apply_deprecate_element(
        repo,
        context,
        DeprecateElementInput {
            element_id: setup.element_id,
            expected_version: wrong_version,
        },
    )
    .unwrap_err();

    assert!(matches!(
        err,
        ChangeError::Precondition(PreconditionError::VersionMismatch { expected, actual, .. })
            if expected == wrong_version && actual == setup.version_id
    ));
}

// ---------------------------------------------------------------------------
// Step 3.2 — validate_deprecate_element_ontology
// ---------------------------------------------------------------------------

#[test]
fn deprecate_ontology_accepts_known_type() {
    let setup = repo_with_element(143, 243);
    let repo = &setup.repo;
    let context = prepare_change(repo).unwrap();

    let prepared = apply_deprecate_element(
        repo,
        context,
        DeprecateElementInput {
            element_id: setup.element_id,
            expected_version: setup.version_id,
        },
    )
    .unwrap();

    let validated = validate_deprecate_element_ontology(prepared).unwrap();
    assert_eq!(validated.element.lifecycle, Lifecycle::Deprecated);
}
