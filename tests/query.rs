//! Integration tests for the read-side query layer (steps 1.8-1.9):
//! resolving the currently accepted version of an element, and reconstructing
//! the accepted Change history from the dependency graph.
//!
//! Queries are strictly read-only — dedicated tests pin that invariant
//! (object store and `refs/accepted` byte-for-byte unchanged after a query).
//!
//! History tests that need more than the single Phase 1 change construct
//! `ChangeRevision` objects directly and store them (bypassing the engine):
//! `history` must follow whatever stored dependency graph it finds, and must
//! not be accidentally hardcoded to the linear single-change case.
//!
//! Note on the cycle case: a genuine dependency cycle is **unconstructible**
//! through the content-addressed store — every dependency ObjectId is the
//! SHA-256 of its target's content, so a cycle would require a hash
//! fixed-point. `history`'s visiting-state cycle rejection is therefore
//! defense-in-depth (tested implicitly: the shared-dependency test proves the
//! visited set prevents duplicate traversal).

use std::fs;
use std::path::{Path, PathBuf};

use kat::domain::change::ChangeRevision;
use kat::domain::element::{KnowledgeElementVersion, Lifecycle};
use kat::domain::identity::{ChangeId, ElementId, ObjectId, RelationshipId};
use kat::domain::operation::Operation;
use kat::domain::property::PropertyValue;
use kat::domain::relationship::RelationshipVersion;
use kat::domain::state::{ElementStateEntry, RelationshipStateEntry, SemanticState};
use kat::encoding::canonical_bytes;
use kat::encoding::object::{CanonicalObject, CanonicalPayload, ObjectKind};
use kat::repository::change::{
    CreateElementInput, LinkElementInput, apply_create_element, apply_link_element,
    persist_prepared_change, persist_prepared_link_change, prepare_change, prepare_change_revision,
    prepare_link_change_revision, publish_persisted_change, publish_persisted_link_change,
    validate_create_element_invariants, validate_create_element_ontology,
    validate_link_element_invariants, validate_link_element_ontology,
};
use kat::repository::init::init_repository;
use kat::repository::object_store::ObjectStoreError;
use kat::repository::open::open_repository;
use kat::repository::query::{
    ArtifactAccountabilityStatus, ListFilter, QueryError, TraversalDirection,
    analyze_artifact_accountability, analyze_impact, history, list_elements, repository_status,
    show_element, trace_origin,
};
use kat::repository::ref_store::{AcceptedRef, RefStore};
use kat::repository::validation::repository::{ValidationViolationKind, validate_repository};
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

/// The identities produced by publishing one element change.
struct FirstChange {
    element_id: ElementId,
    version_id: ObjectId,
    state_id: ObjectId,
    change_revision_id: ObjectId,
}

/// Runs the full engine pipeline (prepare -> create -> validate -> revision ->
/// persist -> publish) for one element against a fresh repository.
fn publish_first_change(root: &Path, element_n: u128, change_n: u128) -> FirstChange {
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
    let state_id = revision.state_id;
    let change_revision_id = revision.change_revision_id;
    let persisted = persist_prepared_change(&repo, revision).unwrap();
    publish_persisted_change(&repo, persisted).unwrap();
    FirstChange {
        element_id,
        version_id,
        state_id,
        change_revision_id,
    }
}

/// Constructs and stores a ChangeRevision directly (bypassing the engine —
/// the point is that `history` follows whatever stored dependency graph it
/// finds), returning its ObjectId. `dependencies` are canonicalized (sorted,
/// unique) exactly as the canonical validator requires.
fn store_change_revision(
    repo: &kat::repository::open::Repository,
    change_n: u128,
    base_states: Vec<ObjectId>,
    result_state: ObjectId,
    operations: Vec<Operation>,
    mut dependencies: Vec<ObjectId>,
) -> ObjectId {
    dependencies.sort();
    dependencies.dedup();
    let change = ChangeRevision {
        change_id: ChangeId::from_uuid(Uuid::from_u128(change_n)),
        base_states,
        result_state,
        operations,
        dependencies,
        description: None,
    };
    let bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::ChangeRevision(change),
    })
    .unwrap();
    repo.object_store().put(&bytes).unwrap()
}

// ---------------------------------------------------------------------------
// Step 1.8 — show_element
// ---------------------------------------------------------------------------

#[test]
fn show_returns_view_for_published_element() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let ids = publish_first_change(root, 71, 171);
    let repo = open_repository(root).unwrap();
    let view = show_element(&repo, ids.element_id).unwrap();

    assert_eq!(view.element_id, ids.element_id);
    assert_eq!(view.version_id, ids.version_id);
    assert_eq!(view.element.element_id, ids.element_id);
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
    let stored_bytes = repo.object_store().get(ids.version_id).unwrap();
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

    let ids = publish_first_change(root, 72, 172);

    // A fresh process reopens and queries the published head.
    let reopened = open_repository(root).unwrap();
    let view = show_element(&reopened, ids.element_id).unwrap();
    assert_eq!(view.version_id, ids.version_id);

    // And it is exactly what the accepted state maps E72 -> V72 to.
    let context = prepare_change(&reopened).unwrap();
    let entry = context
        .base_state
        .elements
        .iter()
        .find(|e| e.element_id == ids.element_id)
        .expect("accepted state must contain the element");
    assert_eq!(entry.version, view.version_id);
}

#[test]
fn show_unknown_element_returns_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let present = publish_first_change(root, 73, 173).element_id;
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

    let ids = publish_first_change(root, 75, 175);

    // A completely new process (fresh open) resolves the same view.
    let reopened = open_repository(root).unwrap();
    let view = show_element(&reopened, ids.element_id).unwrap();
    assert_eq!(view.element_id, ids.element_id);
    assert_eq!(view.version_id, ids.version_id);
    assert_eq!(view.element.lifecycle, Lifecycle::Active);
}

#[test]
fn show_query_does_not_mutate_repository() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let element_id = publish_first_change(root, 76, 176).element_id;
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

// ---------------------------------------------------------------------------
// Step 1.9 — history
// ---------------------------------------------------------------------------

#[test]
fn history_is_empty_on_fresh_repository() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let repo = open_repository(root).unwrap();
    assert_eq!(repo.ref_store().read_accepted().unwrap().change, None);
    assert!(history(&repo).unwrap().is_empty());
}

#[test]
fn history_returns_published_first_change() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = init_repository(root).unwrap();

    let ids = publish_first_change(root, 77, 177);
    let repo = open_repository(root).unwrap();
    let entries = history(&repo).unwrap();

    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.revision_id, ids.change_revision_id);
    assert_eq!(
        entry.change.change_id,
        ChangeId::from_uuid(Uuid::from_u128(177))
    );
    assert_eq!(entry.change.result_state, ids.state_id);
    assert_eq!(entry.change.base_states, vec![init.state]);
    assert!(entry.change.dependencies.is_empty());
    assert_eq!(
        entry.change.operations,
        vec![Operation::CreateElement {
            new_version: ids.version_id,
        }]
    );
}

#[test]
fn history_reopen_fresh_process_returns_same_history() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let ids = publish_first_change(root, 78, 178);

    // A fresh process reconstructs the same history from the live ref.
    let reopened = open_repository(root).unwrap();
    let entries = history(&reopened).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].revision_id, ids.change_revision_id);
    assert_eq!(entries[0].change.result_state, ids.state_id);
}

#[test]
fn history_missing_accepted_change_object_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let ids = publish_first_change(root, 79, 179);
    let repo = open_repository(root).unwrap();
    fs::remove_file(
        kat_dir(root)
            .join("objects")
            .join(ids.change_revision_id.to_string()),
    )
    .unwrap();

    let err = history(&repo).unwrap_err();
    assert!(matches!(
        err,
        QueryError::ObjectStore(ObjectStoreError::NotFound(_))
    ));
}

#[test]
fn history_accepted_change_wrong_object_kind_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = init_repository(root).unwrap();

    // Point the accepted change head at the ontology object (wrong kind).
    let repo = open_repository(root).unwrap();
    repo.ref_store()
        .compare_and_swap_accepted(
            &AcceptedRef {
                state: init.state,
                change: None,
            },
            &AcceptedRef {
                state: init.state,
                change: Some(init.ontology),
            },
        )
        .unwrap();

    let err = history(&repo).unwrap_err();
    assert!(matches!(
        err,
        QueryError::UnexpectedObjectKind {
            expected: ObjectKind::ChangeRevision,
            actual: ObjectKind::OntologyVersion,
        }
    ));
}

#[test]
fn history_accepted_change_result_state_mismatch_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = init_repository(root).unwrap();

    let ids = publish_first_change(root, 80, 180);
    let repo = open_repository(root).unwrap();

    // Move the accepted state back to S0 while keeping C1 (result_state S1)
    // as the head: the live-ref result-state relationship is broken.
    repo.ref_store()
        .compare_and_swap_accepted(
            &AcceptedRef {
                state: ids.state_id,
                change: Some(ids.change_revision_id),
            },
            &AcceptedRef {
                state: init.state,
                change: Some(ids.change_revision_id),
            },
        )
        .unwrap();

    let err = history(&repo).unwrap_err();
    assert!(matches!(
        err,
        QueryError::AcceptedChangeStateMismatch {
            change,
            expected,
            actual,
        } if change == ids.change_revision_id && expected == init.state && actual == ids.state_id
    ));
}

#[test]
fn history_dependency_missing_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let ids = publish_first_change(root, 81, 181);
    let repo = open_repository(root).unwrap();

    // C2 depends on C1; delete C1's object so the traversal hits a missing
    // dependency.
    let c1 = ids.change_revision_id;
    let c2 = store_change_revision(
        &repo,
        281,
        vec![ids.state_id],
        ids.state_id,
        vec![Operation::CreateElement {
            new_version: ids.version_id,
        }],
        vec![c1],
    );
    fs::remove_file(kat_dir(root).join("objects").join(c1.to_string())).unwrap();
    repo.ref_store()
        .compare_and_swap_accepted(
            &AcceptedRef {
                state: ids.state_id,
                change: Some(c1),
            },
            &AcceptedRef {
                state: ids.state_id,
                change: Some(c2),
            },
        )
        .unwrap();

    let err = history(&repo).unwrap_err();
    assert!(matches!(
        err,
        QueryError::ObjectStore(ObjectStoreError::NotFound(_))
    ));
}

#[test]
fn history_dependency_wrong_object_kind_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = init_repository(root).unwrap();

    let ids = publish_first_change(root, 82, 182);
    let repo = open_repository(root).unwrap();

    // C2 depends on the ontology object (wrong kind for a dependency).
    let c2 = store_change_revision(
        &repo,
        282,
        vec![ids.state_id],
        ids.state_id,
        vec![Operation::CreateElement {
            new_version: ids.version_id,
        }],
        vec![init.ontology],
    );
    repo.ref_store()
        .compare_and_swap_accepted(
            &AcceptedRef {
                state: ids.state_id,
                change: Some(ids.change_revision_id),
            },
            &AcceptedRef {
                state: ids.state_id,
                change: Some(c2),
            },
        )
        .unwrap();

    let err = history(&repo).unwrap_err();
    assert!(matches!(
        err,
        QueryError::UnexpectedObjectKind {
            expected: ObjectKind::ChangeRevision,
            actual: ObjectKind::OntologyVersion,
        }
    ));
}

#[test]
fn history_two_revision_chain_is_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let ids = publish_first_change(root, 83, 183);
    let repo = open_repository(root).unwrap();

    // C2 -> [C1]; C1 -> []. The traversal must not be hardcoded to the
    // single-change case.
    let c1 = ids.change_revision_id;
    let c2 = store_change_revision(
        &repo,
        283,
        vec![ids.state_id],
        ids.state_id,
        vec![Operation::CreateElement {
            new_version: ids.version_id,
        }],
        vec![c1],
    );
    repo.ref_store()
        .compare_and_swap_accepted(
            &AcceptedRef {
                state: ids.state_id,
                change: Some(c1),
            },
            &AcceptedRef {
                state: ids.state_id,
                change: Some(c2),
            },
        )
        .unwrap();

    let entries = history(&repo).unwrap();
    assert_eq!(entries.len(), 2);
    // Newest first: the accepted head, then its dependency.
    assert_eq!(entries[0].revision_id, c2);
    assert_eq!(entries[1].revision_id, c1);
    assert_eq!(entries[0].change.dependencies, vec![c1]);
    assert!(entries[1].change.dependencies.is_empty());
}

#[test]
fn history_shared_dependency_appears_once() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let ids = publish_first_change(root, 84, 184);
    let repo = open_repository(root).unwrap();

    // Diamond: C3 -> [C1, C2], C2 -> [C1]. C1 is reachable through two paths
    // but must appear exactly once (visited set).
    let c1 = ids.change_revision_id;
    let c2 = store_change_revision(
        &repo,
        284,
        vec![ids.state_id],
        ids.state_id,
        vec![Operation::CreateElement {
            new_version: ids.version_id,
        }],
        vec![c1],
    );
    let c3 = store_change_revision(
        &repo,
        384,
        vec![ids.state_id],
        ids.state_id,
        vec![Operation::CreateElement {
            new_version: ids.version_id,
        }],
        vec![c1, c2],
    );
    repo.ref_store()
        .compare_and_swap_accepted(
            &AcceptedRef {
                state: ids.state_id,
                change: Some(c1),
            },
            &AcceptedRef {
                state: ids.state_id,
                change: Some(c3),
            },
        )
        .unwrap();

    let entries = history(&repo).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].revision_id, c3);
    for id in [c1, c2, c3] {
        assert_eq!(
            entries.iter().filter(|e| e.revision_id == id).count(),
            1,
            "revision {id} must appear exactly once"
        );
    }
}

#[test]
fn history_does_not_mutate_repository() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    // Publishing is setup only; the test observes the repository before/after
    // the query.
    publish_first_change(root, 85, 185);
    let objects_before = object_ids(root);
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();

    let repo = open_repository(root).unwrap();
    history(&repo).unwrap();

    assert_eq!(object_ids(root), objects_before);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
}

// ---------------------------------------------------------------------------
// trace_origin tests (Step 7.3)
// ---------------------------------------------------------------------------

#[test]
fn trace_origin_unknown_element_returns_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();
    let repo = open_repository(root).unwrap();

    let missing_id = ElementId::from_uuid(Uuid::from_u128(9999));
    let err = trace_origin(&repo, missing_id, None).unwrap_err();
    assert!(matches!(err, QueryError::ElementNotFound(id) if id == missing_id));
}

#[test]
fn trace_origin_single_hop_backward() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    // Create Intent I1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_intent = ElementId::from_uuid(Uuid::from_u128(7001));
    let prep_i1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_intent,
                    type_id: "kat.core/intent".into(),
                    properties: vec![("title".into(), PropertyValue::Text("Intent".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_i1 =
        prepare_change_revision(prep_i1, ChangeId::from_uuid(Uuid::from_u128(7101)), None).unwrap();
    publish_persisted_change(&repo, persist_prepared_change(&repo, rev_i1).unwrap()).unwrap();

    // Create Requirement R1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_req = ElementId::from_uuid(Uuid::from_u128(7002));
    let prep_r1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_req,
                    type_id: "kat.core/requirement".into(),
                    properties: vec![("title".into(), PropertyValue::Text("Requirement".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_r1 =
        prepare_change_revision(prep_r1, ChangeId::from_uuid(Uuid::from_u128(7102)), None).unwrap();
    publish_persisted_change(&repo, persist_prepared_change(&repo, rev_r1).unwrap()).unwrap();

    // Link I1 (motivates) -> R1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let r1_id = RelationshipId::from_uuid(Uuid::from_u128(7201));
    let prep_link = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: r1_id,
                    relationship_type_id: "kat.core/motivates".into(),
                    source_element_id: e_intent,
                    target_element_id: e_req,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_link =
        prepare_link_change_revision(prep_link, ChangeId::from_uuid(Uuid::from_u128(7103)), None)
            .unwrap();
    publish_persisted_link_change(
        &repo,
        persist_prepared_link_change(&repo, rev_link).unwrap(),
    )
    .unwrap();

    let reopened = open_repository(root).unwrap();

    // Tracing R1 finds 1 origin path (R1 <-motivates- I1)
    let res_req = trace_origin(&reopened, e_req, None).unwrap();
    assert_eq!(res_req.root_element_id, e_req);
    assert_eq!(res_req.paths.len(), 1);
    assert_eq!(res_req.paths[0].steps.len(), 1);
    assert_eq!(res_req.paths[0].steps[0].from_element_id, e_req);
    assert_eq!(res_req.paths[0].steps[0].to_element_id, e_intent);
    assert_eq!(
        res_req.paths[0].steps[0].direction,
        TraversalDirection::Backward
    );
    assert_eq!(
        res_req.paths[0].steps[0].relationship_type_id,
        "kat.core/motivates"
    );

    // Tracing I1 finds 0 origin paths (I1 is origin root)
    let res_intent = trace_origin(&reopened, e_intent, None).unwrap();
    assert_eq!(res_intent.root_element_id, e_intent);
    assert_eq!(res_intent.paths.len(), 0);
}

#[test]
fn trace_origin_multi_hop_forward_and_backward() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    // Intent I1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_intent = ElementId::from_uuid(Uuid::from_u128(8001));
    let prep_i1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_intent,
                    type_id: "kat.core/intent".into(),
                    properties: vec![("title".into(), PropertyValue::Text("Intent".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_i1 =
        prepare_change_revision(prep_i1, ChangeId::from_uuid(Uuid::from_u128(8101)), None).unwrap();
    publish_persisted_change(&repo, persist_prepared_change(&repo, rev_i1).unwrap()).unwrap();

    // Requirement R1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_req = ElementId::from_uuid(Uuid::from_u128(8002));
    let prep_r1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_req,
                    type_id: "kat.core/requirement".into(),
                    properties: vec![("title".into(), PropertyValue::Text("Requirement".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_r1 =
        prepare_change_revision(prep_r1, ChangeId::from_uuid(Uuid::from_u128(8102)), None).unwrap();
    publish_persisted_change(&repo, persist_prepared_change(&repo, rev_r1).unwrap()).unwrap();

    // Implementation M1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_impl = ElementId::from_uuid(Uuid::from_u128(8003));
    let prep_m1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_impl,
                    type_id: "kat.core/implementation".into(),
                    properties: vec![(
                        "title".into(),
                        PropertyValue::Text("Implementation".into()),
                    )],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_m1 =
        prepare_change_revision(prep_m1, ChangeId::from_uuid(Uuid::from_u128(8103)), None).unwrap();
    publish_persisted_change(&repo, persist_prepared_change(&repo, rev_m1).unwrap()).unwrap();

    // Artifact A1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_art = ElementId::from_uuid(Uuid::from_u128(8004));
    let prep_a1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_art,
                    type_id: "kat.core/artifact".into(),
                    properties: vec![("title".into(), PropertyValue::Text("Artifact".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_a1 =
        prepare_change_revision(prep_a1, ChangeId::from_uuid(Uuid::from_u128(8104)), None).unwrap();
    publish_persisted_change(&repo, persist_prepared_change(&repo, rev_a1).unwrap()).unwrap();

    // Link I1 (motivates) -> R1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let r1_id = RelationshipId::from_uuid(Uuid::from_u128(8201));
    let prep_link1 = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: r1_id,
                    relationship_type_id: "kat.core/motivates".into(),
                    source_element_id: e_intent,
                    target_element_id: e_req,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_link_change(
        &repo,
        persist_prepared_link_change(
            &repo,
            prepare_link_change_revision(
                prep_link1,
                ChangeId::from_uuid(Uuid::from_u128(8105)),
                None,
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Link M1 (realizes) -> R1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let r2_id = RelationshipId::from_uuid(Uuid::from_u128(8202));
    let prep_link2 = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: r2_id,
                    relationship_type_id: "kat.core/realizes".into(),
                    source_element_id: e_impl,
                    target_element_id: e_req,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_link_change(
        &repo,
        persist_prepared_link_change(
            &repo,
            prepare_link_change_revision(
                prep_link2,
                ChangeId::from_uuid(Uuid::from_u128(8106)),
                None,
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Link A1 (represents) -> M1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let r3_id = RelationshipId::from_uuid(Uuid::from_u128(8203));
    let prep_link3 = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: r3_id,
                    relationship_type_id: "kat.core/represents".into(),
                    source_element_id: e_art,
                    target_element_id: e_impl,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_link_change(
        &repo,
        persist_prepared_link_change(
            &repo,
            prepare_link_change_revision(
                prep_link3,
                ChangeId::from_uuid(Uuid::from_u128(8107)),
                None,
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    let reopened = open_repository(root).unwrap();

    // Tracing Artifact A1 finds multi-hop path: A1 (represents) -> M1 (realizes) -> R1 (motivates backward) -> I1
    let res = trace_origin(&reopened, e_art, None).unwrap();
    assert_eq!(res.root_element_id, e_art);
    assert_eq!(res.paths.len(), 1);

    let path = &res.paths[0];
    assert_eq!(path.steps.len(), 3);

    // Step 1: A1 -> M1 (represents, Forward)
    assert_eq!(path.steps[0].from_element_id, e_art);
    assert_eq!(path.steps[0].to_element_id, e_impl);
    assert_eq!(path.steps[0].direction, TraversalDirection::Forward);

    // Step 2: M1 -> R1 (realizes, Forward)
    assert_eq!(path.steps[1].from_element_id, e_impl);
    assert_eq!(path.steps[1].to_element_id, e_req);
    assert_eq!(path.steps[1].direction, TraversalDirection::Forward);

    // Step 3: R1 -> I1 (motivates, Backward)
    assert_eq!(path.steps[2].from_element_id, e_req);
    assert_eq!(path.steps[2].to_element_id, e_intent);
    assert_eq!(path.steps[2].direction, TraversalDirection::Backward);
}

#[test]
fn trace_origin_does_not_mutate_repository() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let ids = publish_first_change(root, 89, 189);
    let objects_before = object_ids(root);
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();

    let repo = open_repository(root).unwrap();
    trace_origin(&repo, ids.element_id, None).unwrap();

    assert_eq!(object_ids(root), objects_before);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
}

// ---------------------------------------------------------------------------
// analyze_impact tests (Step 8.3)
// ---------------------------------------------------------------------------

#[test]
fn impact_unknown_element_returns_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();
    let repo = open_repository(root).unwrap();

    let missing_id = ElementId::from_uuid(Uuid::from_u128(9999));
    let err = analyze_impact(&repo, missing_id, None).unwrap_err();
    assert!(matches!(err, QueryError::ElementNotFound(id) if id == missing_id));
}

#[test]
fn impact_single_hop_and_category_partitioning() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    // Create Requirement R1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_req = ElementId::from_uuid(Uuid::from_u128(9001));
    let prep_r1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_req,
                    type_id: "kat.core/requirement".into(),
                    properties: vec![(
                        "title".into(),
                        PropertyValue::Text("Requirement R1".into()),
                    )],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_change(
        &repo,
        persist_prepared_change(
            &repo,
            prepare_change_revision(prep_r1, ChangeId::from_uuid(Uuid::from_u128(9101)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Create Design Decision D1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_dec = ElementId::from_uuid(Uuid::from_u128(9002));
    let prep_d1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_dec,
                    type_id: "kat.core/design-decision".into(),
                    properties: vec![("title".into(), PropertyValue::Text("Decision D1".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_change(
        &repo,
        persist_prepared_change(
            &repo,
            prepare_change_revision(prep_d1, ChangeId::from_uuid(Uuid::from_u128(9102)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Create Implementation M1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_impl = ElementId::from_uuid(Uuid::from_u128(9003));
    let prep_m1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_impl,
                    type_id: "kat.core/implementation".into(),
                    properties: vec![(
                        "title".into(),
                        PropertyValue::Text("Implementation M1".into()),
                    )],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_change(
        &repo,
        persist_prepared_change(
            &repo,
            prepare_change_revision(prep_m1, ChangeId::from_uuid(Uuid::from_u128(9103)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Create Artifact A1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_art = ElementId::from_uuid(Uuid::from_u128(9004));
    let prep_a1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_art,
                    type_id: "kat.core/artifact".into(),
                    properties: vec![("title".into(), PropertyValue::Text("Artifact A1".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_change(
        &repo,
        persist_prepared_change(
            &repo,
            prepare_change_revision(prep_a1, ChangeId::from_uuid(Uuid::from_u128(9104)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Link D1 (addresses) -> R1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let r1_id = RelationshipId::from_uuid(Uuid::from_u128(9201));
    let prep_l1 = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: r1_id,
                    relationship_type_id: "kat.core/addresses".into(),
                    source_element_id: e_dec,
                    target_element_id: e_req,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_link_change(
        &repo,
        persist_prepared_link_change(
            &repo,
            prepare_link_change_revision(prep_l1, ChangeId::from_uuid(Uuid::from_u128(9105)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Link M1 (realizes) -> R1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let r2_id = RelationshipId::from_uuid(Uuid::from_u128(9202));
    let prep_l2 = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: r2_id,
                    relationship_type_id: "kat.core/realizes".into(),
                    source_element_id: e_impl,
                    target_element_id: e_req,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_link_change(
        &repo,
        persist_prepared_link_change(
            &repo,
            prepare_link_change_revision(prep_l2, ChangeId::from_uuid(Uuid::from_u128(9106)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Link A1 (represents) -> M1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let r3_id = RelationshipId::from_uuid(Uuid::from_u128(9203));
    let prep_l3 = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: r3_id,
                    relationship_type_id: "kat.core/represents".into(),
                    source_element_id: e_art,
                    target_element_id: e_impl,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_link_change(
        &repo,
        persist_prepared_link_change(
            &repo,
            prepare_link_change_revision(prep_l3, ChangeId::from_uuid(Uuid::from_u128(9107)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    let reopened = open_repository(root).unwrap();

    // Analyze impact when Requirement R1 changes
    let res = analyze_impact(&reopened, e_req, None).unwrap();
    assert_eq!(res.directly_changed, vec![e_req]);

    // Semantically affected elements: Decision D1 (addresses) and Implementation M1 (realizes)
    assert_eq!(res.semantically_affected.len(), 2);
    let sem_ids: Vec<ElementId> = res
        .semantically_affected
        .iter()
        .map(|e| e.element_id)
        .collect();
    assert!(sem_ids.contains(&e_dec));
    assert!(sem_ids.contains(&e_impl));

    // Affected artifacts: Artifact A1 (via represents -> M1 -> R1)
    assert_eq!(res.affected_artifacts.len(), 1);
    assert_eq!(res.affected_artifacts[0].element_id, e_art);
    assert_eq!(res.affected_artifacts[0].paths[0].steps.len(), 2);
    assert_eq!(
        res.affected_artifacts[0].paths[0].steps[0].from_element_id,
        e_req
    );
    assert_eq!(
        res.affected_artifacts[0].paths[0].steps[0].to_element_id,
        e_impl
    );
    assert_eq!(
        res.affected_artifacts[0].paths[0].steps[1].from_element_id,
        e_impl
    );
    assert_eq!(
        res.affected_artifacts[0].paths[0].steps[1].to_element_id,
        e_art
    );
}

#[test]
fn impact_does_not_mutate_repository() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let ids = publish_first_change(root, 99, 199);
    let objects_before = object_ids(root);
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();

    let repo = open_repository(root).unwrap();
    analyze_impact(&repo, ids.element_id, None).unwrap();

    assert_eq!(object_ids(root), objects_before);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
}

// ---------------------------------------------------------------------------
// validate_repository tests (Step 9.3)
// ---------------------------------------------------------------------------

#[test]
fn validate_clean_repository_returns_no_violations() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let repo = open_repository(root).unwrap();
    let report = validate_repository(&repo).unwrap();
    assert!(report.violations.is_empty());
    assert!(report.unverified_constraints.is_empty());
}

#[test]
fn validate_reports_unverified_constraints_with_restricts_targets() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    // Create Constraint C1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_con = ElementId::from_uuid(Uuid::from_u128(8001));
    let prep_c1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_con,
                    type_id: "kat.core/constraint".into(),
                    properties: vec![(
                        "title".into(),
                        PropertyValue::Text("TLS 1.3 Encryption Required".into()),
                    )],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_change(
        &repo,
        persist_prepared_change(
            &repo,
            prepare_change_revision(prep_c1, ChangeId::from_uuid(Uuid::from_u128(8101)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Create Design Decision D1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_dec = ElementId::from_uuid(Uuid::from_u128(8002));
    let prep_d1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_dec,
                    type_id: "kat.core/design-decision".into(),
                    properties: vec![(
                        "title".into(),
                        PropertyValue::Text("Use PASETO Tokens".into()),
                    )],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_change(
        &repo,
        persist_prepared_change(
            &repo,
            prepare_change_revision(prep_d1, ChangeId::from_uuid(Uuid::from_u128(8102)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Link C1 (restricts) -> D1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let r1_id = RelationshipId::from_uuid(Uuid::from_u128(8201));
    let prep_l1 = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: r1_id,
                    relationship_type_id: "kat.core/restricts".into(),
                    source_element_id: e_con,
                    target_element_id: e_dec,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_link_change(
        &repo,
        persist_prepared_link_change(
            &repo,
            prepare_link_change_revision(prep_l1, ChangeId::from_uuid(Uuid::from_u128(8103)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    let reopened = open_repository(root).unwrap();
    let report = validate_repository(&reopened).unwrap();
    assert!(report.violations.is_empty());
    assert_eq!(report.unverified_constraints.len(), 1);
    assert_eq!(
        report.unverified_constraints[0].constraint_element_id,
        e_con
    );
    assert_eq!(
        report.unverified_constraints[0].title.as_deref(),
        Some("TLS 1.3 Encryption Required")
    );
    assert_eq!(
        report.unverified_constraints[0].constrained_element_ids,
        vec![e_dec]
    );
}

#[test]
fn validate_does_not_mutate_repository() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let _ids = publish_first_change(root, 77, 177);
    let objects_before = object_ids(root);
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();

    let repo = open_repository(root).unwrap();
    validate_repository(&repo).unwrap();

    assert_eq!(object_ids(root), objects_before);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
}

fn publish_custom_state_fixture(
    root: &Path,
    elements: Vec<(ElementId, &str, Lifecycle)>,
    relationships: Vec<(RelationshipId, &str, ElementId, ElementId)>,
) -> kat::repository::open::Repository {
    let repo = open_repository(root).unwrap();
    let store = repo.object_store();

    let mut state_elements = Vec::new();
    for (el_id, type_id, lifecycle) in elements {
        let el_version = KnowledgeElementVersion {
            element_id: el_id,
            type_id: type_id.into(),
            lifecycle,
            properties: vec![(
                "title".into(),
                PropertyValue::Text(format!("Element {el_id}")),
            )],
        };
        let bytes = canonical_bytes(&CanonicalObject {
            payload: CanonicalPayload::KnowledgeElementVersion(el_version),
        })
        .unwrap();
        let obj_id = store.put(&bytes).unwrap();
        state_elements.push(ElementStateEntry {
            element_id: el_id,
            version: obj_id,
        });
    }
    state_elements.sort_by_key(|e| e.element_id);

    let mut state_rels = Vec::new();
    for (rel_id, type_id, src_id, tgt_id) in relationships {
        let rel_version = RelationshipVersion {
            relationship_id: rel_id,
            relationship_type: type_id.into(),
            source_element_id: src_id,
            target_element_id: tgt_id,
            properties: vec![],
        };
        let bytes = canonical_bytes(&CanonicalObject {
            payload: CanonicalPayload::RelationshipVersion(rel_version),
        })
        .unwrap();
        let obj_id = store.put(&bytes).unwrap();
        state_rels.push(RelationshipStateEntry {
            relationship_id: rel_id,
            version: obj_id,
        });
    }
    state_rels.sort_by_key(|r| r.relationship_id);

    let accepted = repo.ref_store().read_accepted().unwrap();
    let state_0_bytes = store.get(accepted.state).unwrap();
    let state_0_obj = kat::encoding::decode_canonical(&state_0_bytes).unwrap();
    let state_0 = match state_0_obj.payload {
        CanonicalPayload::SemanticState(s) => s,
        _ => unreachable!(),
    };

    let new_state = SemanticState {
        ontology_version: state_0.ontology_version,
        elements: state_elements,
        relationships: state_rels,
    };
    let state_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::SemanticState(new_state),
    })
    .unwrap();
    let new_state_id = store.put(&state_bytes).unwrap();

    let refs = repo.ref_store();
    refs.compare_and_swap_accepted(
        &accepted,
        &AcceptedRef {
            state: new_state_id,
            change: accepted.change,
        },
    )
    .unwrap();

    open_repository(root).unwrap()
}

#[test]
fn validate_reports_invalid_relationship_type() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let e1 = ElementId::from_uuid(Uuid::from_u128(7001));
    let e2 = ElementId::from_uuid(Uuid::from_u128(7002));
    let r1 = RelationshipId::from_uuid(Uuid::from_u128(7101));

    let repo = publish_custom_state_fixture(
        root,
        vec![
            (e1, "kat.core/requirement", Lifecycle::Active),
            (e2, "kat.core/design-decision", Lifecycle::Active),
        ],
        vec![(r1, "kat.core/unknown-rel-type", e1, e2)],
    );

    let report = validate_repository(&repo).unwrap();
    assert_eq!(report.violations.len(), 1);
    assert_eq!(
        report.violations[0].kind,
        ValidationViolationKind::UnknownRelationshipType
    );
    assert_eq!(report.violations[0].relationship_id, Some(r1));
}

#[test]
fn validate_reports_disallowed_source_and_target_types() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let e_req = ElementId::from_uuid(Uuid::from_u128(7011));
    let e_int = ElementId::from_uuid(Uuid::from_u128(7012));
    let r1 = RelationshipId::from_uuid(Uuid::from_u128(7111));

    // kat.core/addresses requires source to be design-decision and target requirement.
    // Here source is requirement and target is intent.
    let repo = publish_custom_state_fixture(
        root,
        vec![
            (e_req, "kat.core/requirement", Lifecycle::Active),
            (e_int, "kat.core/intent", Lifecycle::Active),
        ],
        vec![(r1, "kat.core/addresses", e_req, e_int)],
    );

    let report = validate_repository(&repo).unwrap();
    assert_eq!(report.violations.len(), 2);
    let kinds: Vec<ValidationViolationKind> = report.violations.iter().map(|v| v.kind).collect();
    assert!(kinds.contains(&ValidationViolationKind::RelationshipSourceTypeNotAllowed));
    assert!(kinds.contains(&ValidationViolationKind::RelationshipTargetTypeNotAllowed));
}

#[test]
fn validate_reports_duplicate_relationship_triples() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let e_dec = ElementId::from_uuid(Uuid::from_u128(7021));
    let e_req = ElementId::from_uuid(Uuid::from_u128(7022));
    let r1 = RelationshipId::from_uuid(Uuid::from_u128(7121));
    let r2 = RelationshipId::from_uuid(Uuid::from_u128(7122));

    let repo = publish_custom_state_fixture(
        root,
        vec![
            (e_dec, "kat.core/design-decision", Lifecycle::Active),
            (e_req, "kat.core/requirement", Lifecycle::Active),
        ],
        vec![
            (r1, "kat.core/addresses", e_dec, e_req),
            (r2, "kat.core/addresses", e_dec, e_req),
        ],
    );

    let report = validate_repository(&repo).unwrap();
    assert_eq!(report.violations.len(), 1);
    assert_eq!(
        report.violations[0].kind,
        ValidationViolationKind::DuplicateRelationshipTriple
    );
    assert_eq!(report.violations[0].relationship_id, Some(r2));
}

#[test]
fn validate_permits_deprecated_source_on_existing_relationship() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let e_dec = ElementId::from_uuid(Uuid::from_u128(7031));
    let e_req = ElementId::from_uuid(Uuid::from_u128(7032));
    let r1 = RelationshipId::from_uuid(Uuid::from_u128(7131));

    // Decision e_dec is Deprecated, but existing relationship is valid.
    let repo = publish_custom_state_fixture(
        root,
        vec![
            (e_dec, "kat.core/design-decision", Lifecycle::Deprecated),
            (e_req, "kat.core/requirement", Lifecycle::Active),
        ],
        vec![(r1, "kat.core/addresses", e_dec, e_req)],
    );

    let report = validate_repository(&repo).unwrap();
    assert!(report.violations.is_empty());
}

#[test]
fn validate_repository_classification_and_evidence_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let e_con = ElementId::from_uuid(Uuid::from_u128(8001));
    let e_req = ElementId::from_uuid(Uuid::from_u128(8002));
    let e_val = ElementId::from_uuid(Uuid::from_u128(8003));
    let r_restricts = RelationshipId::from_uuid(Uuid::from_u128(8101));
    let r_validates = RelationshipId::from_uuid(Uuid::from_u128(8102));

    let repo = publish_custom_state_fixture(
        root,
        vec![
            (e_con, "kat.core/constraint", Lifecycle::Active),
            (e_req, "kat.core/requirement", Lifecycle::Active),
            (e_val, "kat.core/validation", Lifecycle::Active),
        ],
        vec![
            (r_restricts, "kat.core/restricts", e_con, e_req),
            (r_validates, "kat.core/validates", e_val, e_con),
        ],
    );

    let report = validate_repository(&repo).unwrap();
    assert!(report.violations.is_empty());

    // Constraint details assertion
    assert_eq!(report.constraint_details.len(), 1);
    let detail = &report.constraint_details[0];
    assert_eq!(detail.constraint_id, e_con);
    assert_eq!(detail.constrained_element_ids, vec![e_req]);
    assert!(
        !detail.is_mechanically_verified,
        "critical invariant: evidence-backed != mechanically verified"
    );
    assert_eq!(detail.validation_evidence.len(), 1);
    assert_eq!(detail.validation_evidence[0].validation_element_id, e_val);

    // Category coverage assertions
    let con_summary = report
        .category_summaries
        .iter()
        .find(|s| s.category_type == "kat.core/constraint")
        .unwrap();
    assert_eq!(con_summary.total_count, 1);
    assert_eq!(con_summary.evidence_backed_count, 1);
    assert_eq!(con_summary.uncovered_count, 0);

    let req_summary = report
        .category_summaries
        .iter()
        .find(|s| s.category_type == "kat.core/requirement")
        .unwrap();
    assert_eq!(req_summary.total_count, 1);
    assert_eq!(req_summary.evidence_backed_count, 0);
    assert_eq!(req_summary.uncovered_count, 1);

    // Uncovered elements assertion
    assert!(
        report
            .uncovered_elements
            .iter()
            .any(|u| u.element_id == e_req)
    );
    assert!(
        !report
            .uncovered_elements
            .iter()
            .any(|u| u.element_id == e_con)
    );
}

// ---------------------------------------------------------------------------
// analyze_artifact_accountability tests (Step 10.3)
// ---------------------------------------------------------------------------

#[test]
fn accountability_no_artifacts_returns_empty_report() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let repo = open_repository(root).unwrap();
    let report = analyze_artifact_accountability(&repo).unwrap();
    assert!(report.artifacts.is_empty());
}

#[test]
fn accountability_unaccounted_artifact_status() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_art = ElementId::from_uuid(Uuid::from_u128(9001));
    let prep_a1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_art,
                    type_id: "kat.core/artifact".into(),
                    properties: vec![(
                        "title".into(),
                        PropertyValue::Text("auth_service.rs".into()),
                    )],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_change(
        &repo,
        persist_prepared_change(
            &repo,
            prepare_change_revision(prep_a1, ChangeId::from_uuid(Uuid::from_u128(9101)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    let reopened = open_repository(root).unwrap();
    let report = analyze_artifact_accountability(&reopened).unwrap();
    assert_eq!(report.artifacts.len(), 1);
    assert_eq!(report.artifacts[0].artifact_element_id, e_art);
    assert_eq!(
        report.artifacts[0].status,
        ArtifactAccountabilityStatus::Unaccounted
    );
    assert!(report.artifacts[0].baselines.is_empty());
}

#[test]
fn accountability_current_and_stale_and_relink_lifecycle() {
    use kat::repository::change::{
        UnlinkElementInput, UpdateElementInput, apply_unlink_element, apply_update_element,
        persist_prepared_unlink_change, persist_prepared_update_change,
        prepare_unlink_change_revision, prepare_update_change_revision,
        publish_persisted_unlink_change, publish_persisted_update_change,
        validate_unlink_element_invariants, validate_update_element_invariants,
        validate_update_element_ontology,
    };

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    // 1. Create Implementation M1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_imp = ElementId::from_uuid(Uuid::from_u128(9011));
    let prep_m1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_imp,
                    type_id: "kat.core/implementation".into(),
                    properties: vec![("title".into(), PropertyValue::Text("AuthX Core".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_change(
        &repo,
        persist_prepared_change(
            &repo,
            prepare_change_revision(prep_m1, ChangeId::from_uuid(Uuid::from_u128(9111)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // 2. Create Artifact A1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_art = ElementId::from_uuid(Uuid::from_u128(9012));
    let prep_a1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_art,
                    type_id: "kat.core/artifact".into(),
                    properties: vec![("title".into(), PropertyValue::Text("authx.jar".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_change(
        &repo,
        persist_prepared_change(
            &repo,
            prepare_change_revision(prep_a1, ChangeId::from_uuid(Uuid::from_u128(9112)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // 3. Link A1 (represents) -> M1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let r1_id = RelationshipId::from_uuid(Uuid::from_u128(9211));
    let prep_l1 = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: r1_id,
                    relationship_type_id: "kat.core/represents".into(),
                    source_element_id: e_art,
                    target_element_id: e_imp,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_link_change(
        &repo,
        persist_prepared_link_change(
            &repo,
            prepare_link_change_revision(prep_l1, ChangeId::from_uuid(Uuid::from_u128(9113)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Verify CURRENT status
    let repo = open_repository(root).unwrap();
    let report1 = analyze_artifact_accountability(&repo).unwrap();
    assert_eq!(report1.artifacts.len(), 1);
    assert_eq!(
        report1.artifacts[0].status,
        ArtifactAccountabilityStatus::Current
    );
    assert_eq!(report1.artifacts[0].baselines.len(), 1);
    assert!(!report1.artifacts[0].baselines[0].is_stale);

    // 4. Update M1 (advances Implementation version)
    let repo = open_repository(root).unwrap();
    let v_m1 = show_element(&repo, e_imp).unwrap().version_id;
    let ctx = prepare_change(&repo).unwrap();
    let prep_u1 = validate_update_element_invariants(
        validate_update_element_ontology(
            apply_update_element(
                &repo,
                ctx,
                UpdateElementInput {
                    element_id: e_imp,
                    expected_version: v_m1,
                    properties: vec![("title".into(), PropertyValue::Text("AuthX Core v2".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_update_change(
        &repo,
        persist_prepared_update_change(
            &repo,
            prepare_update_change_revision(
                prep_u1,
                ChangeId::from_uuid(Uuid::from_u128(9114)),
                None,
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Verify STALE status
    let repo = open_repository(root).unwrap();
    let report2 = analyze_artifact_accountability(&repo).unwrap();
    assert_eq!(report2.artifacts.len(), 1);
    assert_eq!(
        report2.artifacts[0].status,
        ArtifactAccountabilityStatus::Stale
    );
    assert_eq!(report2.artifacts[0].baselines.len(), 1);
    assert!(report2.artifacts[0].baselines[0].is_stale);

    // 5. Unlink r1 and Link r2 (A1 represents M1)
    let repo = open_repository(root).unwrap();
    let state_bytes = repo
        .object_store()
        .get(repo.ref_store().read_accepted().unwrap().state)
        .unwrap();
    let state_obj = kat::encoding::decode_canonical(&state_bytes).unwrap();
    let state_tmp = match state_obj.payload {
        CanonicalPayload::SemanticState(s) => s,
        _ => unreachable!(),
    };
    let r1_ver_id = state_tmp
        .relationships
        .iter()
        .find(|r| r.relationship_id == r1_id)
        .unwrap()
        .version;

    let ctx = prepare_change(&repo).unwrap();
    let prep_ul = validate_unlink_element_invariants(
        apply_unlink_element(
            &repo,
            ctx,
            UnlinkElementInput {
                relationship_id: r1_id,
                expected_version: r1_ver_id,
            },
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_unlink_change(
        &repo,
        persist_prepared_unlink_change(
            &repo,
            prepare_unlink_change_revision(
                prep_ul,
                ChangeId::from_uuid(Uuid::from_u128(9115)),
                None,
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let r2_id = RelationshipId::from_uuid(Uuid::from_u128(9212));
    let prep_l2 = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: r2_id,
                    relationship_type_id: "kat.core/represents".into(),
                    source_element_id: e_art,
                    target_element_id: e_imp,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_link_change(
        &repo,
        persist_prepared_link_change(
            &repo,
            prepare_link_change_revision(prep_l2, ChangeId::from_uuid(Uuid::from_u128(9116)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Verify CURRENT status restored after re-link
    let repo = open_repository(root).unwrap();
    let report3 = analyze_artifact_accountability(&repo).unwrap();
    assert_eq!(report3.artifacts.len(), 1);
    assert_eq!(
        report3.artifacts[0].status,
        ArtifactAccountabilityStatus::Current
    );
    assert_eq!(report3.artifacts[0].baselines.len(), 1);
    assert!(!report3.artifacts[0].baselines[0].is_stale);
}

#[test]
fn accountability_does_not_mutate_repository() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let _ids = publish_first_change(root, 88, 188);
    let objects_before = object_ids(root);
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();

    let repo = open_repository(root).unwrap();
    analyze_artifact_accountability(&repo).unwrap();

    assert_eq!(object_ids(root), objects_before);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
}

#[test]
fn accountability_stale_when_upstream_element_deprecated_or_superseded() {
    use kat::repository::change::{
        DeprecateElementInput, apply_deprecate_element, persist_prepared_deprecate_change,
        prepare_deprecate_change_revision, publish_persisted_deprecate_change,
        validate_deprecate_element_invariants, validate_deprecate_element_ontology,
    };

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    // 1. Create Decision D1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_dec = ElementId::from_uuid(Uuid::from_u128(9501));
    let prep_d1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_dec,
                    type_id: "kat.core/design-decision".into(),
                    properties: vec![("title".into(), PropertyValue::Text("Decision D1".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_change(
        &repo,
        persist_prepared_change(
            &repo,
            prepare_change_revision(prep_d1, ChangeId::from_uuid(Uuid::from_u128(9601)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // 2. Create Artifact A1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_art = ElementId::from_uuid(Uuid::from_u128(9502));
    let prep_a1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_art,
                    type_id: "kat.core/artifact".into(),
                    properties: vec![(
                        "title".into(),
                        PropertyValue::Text("architecture.md".into()),
                    )],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_change(
        &repo,
        persist_prepared_change(
            &repo,
            prepare_change_revision(prep_a1, ChangeId::from_uuid(Uuid::from_u128(9602)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // 3. Link A1 (derived-from) -> D1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let r1_id = RelationshipId::from_uuid(Uuid::from_u128(9701));
    let prep_l1 = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: r1_id,
                    relationship_type_id: "kat.core/derived-from".into(),
                    source_element_id: e_art,
                    target_element_id: e_dec,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_link_change(
        &repo,
        persist_prepared_link_change(
            &repo,
            prepare_link_change_revision(prep_l1, ChangeId::from_uuid(Uuid::from_u128(9603)), None)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Verify CURRENT
    let repo = open_repository(root).unwrap();
    let report1 = analyze_artifact_accountability(&repo).unwrap();
    assert_eq!(
        report1.artifacts[0].status,
        ArtifactAccountabilityStatus::Current
    );

    // 4. Deprecate D1
    let v_d1 = show_element(&repo, e_dec).unwrap().version_id;
    let ctx = prepare_change(&repo).unwrap();
    let prep_dep = validate_deprecate_element_invariants(
        validate_deprecate_element_ontology(
            apply_deprecate_element(
                &repo,
                ctx,
                DeprecateElementInput {
                    element_id: e_dec,
                    expected_version: v_d1,
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    publish_persisted_deprecate_change(
        &repo,
        persist_prepared_deprecate_change(
            &repo,
            prepare_deprecate_change_revision(
                prep_dep,
                ChangeId::from_uuid(Uuid::from_u128(9604)),
                None,
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Verify STALE due to upstream element deprecation
    let repo = open_repository(root).unwrap();
    let report2 = analyze_artifact_accountability(&repo).unwrap();
    assert_eq!(report2.artifacts.len(), 1);
    assert_eq!(
        report2.artifacts[0].status,
        ArtifactAccountabilityStatus::Stale
    );
    assert!(report2.artifacts[0].baselines[0].is_stale);
}

// ---------------------------------------------------------------------------
// repository_status tests
// ---------------------------------------------------------------------------

#[test]
fn status_on_fresh_repository_returns_zero_counts_and_no_latest_change() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let repo = open_repository(root).unwrap();
    let status = repository_status(&repo).unwrap();

    assert_eq!(status.knowledge.total_elements, 0);
    assert_eq!(status.knowledge.active_elements, 0);
    assert_eq!(status.knowledge.deprecated_elements, 0);
    assert_eq!(status.knowledge.superseded_elements, 0);
    assert_eq!(status.knowledge.total_relationships, 0);

    assert_eq!(status.consistency.violations, 0);
    assert_eq!(status.consistency.unverified_constraints, 0);

    assert_eq!(status.accountability.current, 0);
    assert_eq!(status.accountability.stale, 0);
    assert_eq!(status.accountability.unaccounted, 0);

    assert!(status.change_id.is_none());
    assert!(status.latest_change.is_none());
}

#[test]
fn status_does_not_mutate_repository() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let repo = open_repository(root).unwrap();
    let s1 = repository_status(&repo).unwrap();
    let s2 = repository_status(&repo).unwrap();
    assert_eq!(s1, s2);
}

// ---------------------------------------------------------------------------
// list_elements tests (Phase 11 Step 11.1)
// ---------------------------------------------------------------------------

#[test]
fn list_elements_empty_repository_returns_empty_vec() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let repo = open_repository(root).unwrap();
    let elements = list_elements(&repo, ListFilter::default()).unwrap();
    assert!(elements.is_empty());
}

#[test]
fn list_elements_all_elements_default_filter() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let p1 = publish_first_change(root, 101, 201);
    let p2 = publish_first_change(root, 102, 202);

    let repo = open_repository(root).unwrap();
    let elements = list_elements(&repo, ListFilter::default()).unwrap();
    assert_eq!(elements.len(), 2);

    // Elements are returned deterministically sorted by ElementId.
    let mut expected_ids = vec![p1.element_id, p2.element_id];
    expected_ids.sort();
    let actual_ids: Vec<_> = elements.iter().map(|e| e.element_id).collect();
    assert_eq!(actual_ids, expected_ids);
}

#[test]
fn list_elements_type_id_filter() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let p1 = publish_first_change(root, 103, 203); // kat.core/requirement

    let repo = open_repository(root).unwrap();
    let reqs = list_elements(
        &repo,
        ListFilter {
            type_id: Some("kat.core/requirement".to_string()),
            lifecycle: None,
        },
    )
    .unwrap();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].element_id, p1.element_id);

    let constraints = list_elements(
        &repo,
        ListFilter {
            type_id: Some("kat.core/constraint".to_string()),
            lifecycle: None,
        },
    )
    .unwrap();
    assert!(constraints.is_empty());
}

#[test]
fn list_elements_lifecycle_filter() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let p1 = publish_first_change(root, 104, 204);

    let repo = open_repository(root).unwrap();
    let active = list_elements(
        &repo,
        ListFilter {
            type_id: None,
            lifecycle: Some(Lifecycle::Active),
        },
    )
    .unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].element_id, p1.element_id);

    let deprecated = list_elements(
        &repo,
        ListFilter {
            type_id: None,
            lifecycle: Some(Lifecycle::Deprecated),
        },
    )
    .unwrap();
    assert!(deprecated.is_empty());
}

#[test]
fn list_elements_combined_filters() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let _p1 = publish_first_change(root, 105, 205);

    let repo = open_repository(root).unwrap();

    let match_both = list_elements(
        &repo,
        ListFilter {
            type_id: Some("kat.core/requirement".to_string()),
            lifecycle: Some(Lifecycle::Active),
        },
    )
    .unwrap();
    assert_eq!(match_both.len(), 1);

    let match_type_wrong_lifecycle = list_elements(
        &repo,
        ListFilter {
            type_id: Some("kat.core/requirement".to_string()),
            lifecycle: Some(Lifecycle::Deprecated),
        },
    )
    .unwrap();
    assert!(match_type_wrong_lifecycle.is_empty());

    let match_lifecycle_wrong_type = list_elements(
        &repo,
        ListFilter {
            type_id: Some("kat.core/constraint".to_string()),
            lifecycle: Some(Lifecycle::Active),
        },
    )
    .unwrap();
    assert!(match_lifecycle_wrong_type.is_empty());
}

#[test]
fn list_elements_does_not_mutate_repository() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();
    publish_first_change(root, 106, 206);

    let repo = open_repository(root).unwrap();
    let l1 = list_elements(&repo, ListFilter::default()).unwrap();
    let l2 = list_elements(&repo, ListFilter::default()).unwrap();

    assert_eq!(l1.len(), l2.len());
    assert_eq!(l1[0].element_id, l2[0].element_id);
    assert_eq!(l1[0].version_id, l2[0].version_id);
}

#[test]
fn show_element_includes_incoming_and_outgoing_relationships() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let e1 = ElementId::from_uuid(Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap());
    let e2 = ElementId::from_uuid(Uuid::parse_str("20000000-0000-0000-0000-000000000002").unwrap());
    let r1 =
        RelationshipId::from_uuid(Uuid::parse_str("30000000-0000-0000-0000-000000000003").unwrap());

    // e1: decision "Use WebAuthn"
    // e2: requirement "User auth"
    // r1: e1 addresses e2
    let repo1 = open_repository(root).unwrap();
    let ctx1 = prepare_change(&repo1).unwrap();
    let p1 = apply_create_element(
        ctx1,
        CreateElementInput {
            element_id: e1,
            type_id: "kat.core/design-decision".into(),
            properties: vec![("title".into(), PropertyValue::Text("Use WebAuthn".into()))],
        },
    )
    .unwrap();
    let v1 =
        validate_create_element_invariants(validate_create_element_ontology(p1).unwrap()).unwrap();
    let rev1 = prepare_change_revision(v1, ChangeId::from_uuid(Uuid::new_v4()), None).unwrap();
    publish_persisted_change(&repo1, persist_prepared_change(&repo1, rev1).unwrap()).unwrap();

    let repo2 = open_repository(root).unwrap();
    let ctx2 = prepare_change(&repo2).unwrap();
    let p2 = apply_create_element(
        ctx2,
        CreateElementInput {
            element_id: e2,
            type_id: "kat.core/requirement".into(),
            properties: vec![("title".into(), PropertyValue::Text("User auth".into()))],
        },
    )
    .unwrap();
    let v2 =
        validate_create_element_invariants(validate_create_element_ontology(p2).unwrap()).unwrap();
    let rev2 = prepare_change_revision(v2, ChangeId::from_uuid(Uuid::new_v4()), None).unwrap();
    publish_persisted_change(&repo2, persist_prepared_change(&repo2, rev2).unwrap()).unwrap();

    let repo3 = open_repository(root).unwrap();
    let ctx3 = prepare_change(&repo3).unwrap();
    let p3 = apply_link_element(
        &repo3,
        ctx3,
        LinkElementInput {
            relationship_id: r1,
            relationship_type_id: "kat.core/addresses".into(),
            source_element_id: e1,
            target_element_id: e2,
            properties: vec![],
        },
    )
    .unwrap();
    let v3 = validate_link_element_invariants(validate_link_element_ontology(p3).unwrap()).unwrap();
    let rev3 = prepare_link_change_revision(v3, ChangeId::from_uuid(Uuid::new_v4()), None).unwrap();
    publish_persisted_link_change(&repo3, persist_prepared_link_change(&repo3, rev3).unwrap())
        .unwrap();

    let repo = open_repository(root).unwrap();

    // Query e1 (outgoing: addresses -> e2)
    let view1 = show_element(&repo, e1).unwrap();
    assert!(view1.relationships.incoming.is_empty());
    assert_eq!(view1.relationships.outgoing.len(), 1);
    assert_eq!(view1.relationships.outgoing[0].relationship_id, r1);
    assert_eq!(view1.relationships.outgoing[0].other_element_id, e2);
    assert_eq!(
        view1.relationships.outgoing[0].other_title.as_deref(),
        Some("User auth")
    );

    // Query e2 (incoming: addresses <- e1)
    let view2 = show_element(&repo, e2).unwrap();
    assert_eq!(view2.relationships.incoming.len(), 1);
    assert!(view2.relationships.outgoing.is_empty());
    assert_eq!(view2.relationships.incoming[0].relationship_id, r1);
    assert_eq!(view2.relationships.incoming[0].other_element_id, e1);
    assert_eq!(
        view2.relationships.incoming[0].other_title.as_deref(),
        Some("Use WebAuthn")
    );
}

// ---------------------------------------------------------------------------
// Ontology Discovery Tests (Phase 17, Step 17.1)
// ---------------------------------------------------------------------------

use kat::domain::ontology::{ElementTypeDefinition, RelationshipTypeDefinition};
use kat::encoding::decode_canonical;
use kat::repository::query::{
    OntologyTypeView, active_ontology, inspect_ontology, show_ontology_type,
};

#[test]
fn inspect_ontology_returns_built_in_core_ontology() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();
    let repo = open_repository(root).unwrap();

    let summary = inspect_ontology(&repo).unwrap();
    assert_eq!(summary.element_types.len(), 7);
    assert_eq!(summary.relationship_types.len(), 10);

    // Deterministic ordering check (alphabetical by type_id)
    let elem_type_ids: Vec<&str> = summary
        .element_types
        .iter()
        .map(|e| e.type_id.as_str())
        .collect();
    assert_eq!(
        elem_type_ids,
        vec![
            "kat.core/artifact",
            "kat.core/constraint",
            "kat.core/design-decision",
            "kat.core/implementation",
            "kat.core/intent",
            "kat.core/requirement",
            "kat.core/validation",
        ]
    );

    let rel_type_ids: Vec<&str> = summary
        .relationship_types
        .iter()
        .map(|r| r.type_id.as_str())
        .collect();
    assert_eq!(
        rel_type_ids,
        vec![
            "kat.core/addresses",
            "kat.core/depends-on",
            "kat.core/derived-from",
            "kat.core/guides",
            "kat.core/motivates",
            "kat.core/realizes",
            "kat.core/represents",
            "kat.core/restricts",
            "kat.core/supersedes",
            "kat.core/validates",
        ]
    );
}

#[test]
fn show_ontology_type_exact_canonical_and_short_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();
    let repo = open_repository(root).unwrap();

    // 1. Exact canonical lookup
    let view_canon = show_ontology_type(&repo, "kat.core/requirement").unwrap();
    if let OntologyTypeView::Element(e) = view_canon {
        assert_eq!(e.type_id, "kat.core/requirement");
        assert_eq!(e.name, "Requirement");
        assert_eq!(e.outgoing.len(), 0);
        assert!(
            e.incoming
                .iter()
                .any(|cap| cap.relationship_type_id == "kat.core/realizes"
                    && cap.counterpart_type_id == "kat.core/implementation")
        );
    } else {
        panic!("expected ElementTypeView");
    }

    // 2. Short identifier lookup
    let view_short = show_ontology_type(&repo, "requirement").unwrap();
    if let OntologyTypeView::Element(e) = view_short {
        assert_eq!(e.type_id, "kat.core/requirement");
    } else {
        panic!("expected ElementTypeView");
    }

    // 3. Relationship type lookup
    let view_rel = show_ontology_type(&repo, "realizes").unwrap();
    if let OntologyTypeView::Relationship(r) = view_rel {
        assert_eq!(r.type_id, "kat.core/realizes");
        assert_eq!(r.name, "Realizes");
        assert_eq!(r.allowed_source_types, vec!["kat.core/implementation"]);
        assert_eq!(r.allowed_target_types, vec!["kat.core/requirement"]);
    } else {
        panic!("expected RelationshipTypeView");
    }
}

#[test]
fn show_ontology_type_unknown_type_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();
    let repo = open_repository(root).unwrap();

    let err = show_ontology_type(&repo, "nonexistent-type").unwrap_err();
    match err {
        QueryError::UnknownOntologyType(query) => assert_eq!(query, "nonexistent-type"),
        other => panic!("expected UnknownOntologyType, got {:?}", other),
    }
}

#[test]
fn inspect_and_show_ontology_type_do_not_mutate_repository() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let objects_before = object_ids(root);
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();

    let repo = open_repository(root).unwrap();
    let _ = inspect_ontology(&repo).unwrap();
    let _ = show_ontology_type(&repo, "requirement").unwrap();

    assert_eq!(object_ids(root), objects_before);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
}

#[test]
fn show_ontology_type_ambiguous_short_name_and_extension_ontology() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let repo = open_repository(root).unwrap();
    let (_orig_id, mut ontology) = active_ontology(&repo).unwrap();

    ontology.element_types.push(ElementTypeDefinition {
        type_id: "example/requirement".into(),
        name: "Example Requirement".into(),
    });
    ontology.element_types.push(ElementTypeDefinition {
        type_id: "example/service".into(),
        name: "Example Service".into(),
    });
    ontology
        .relationship_types
        .push(RelationshipTypeDefinition {
            type_id: "example/satisfies".into(),
            name: "Satisfies".into(),
            allowed_source_types: vec!["example/service".into()],
            allowed_target_types: vec!["example/requirement".into(), "kat.core/requirement".into()],
        });

    ontology.element_types.sort();
    ontology.relationship_types.sort();

    let custom_ont_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::OntologyVersion(ontology),
    })
    .unwrap();
    let custom_ont_id = repo.object_store().put(&custom_ont_bytes).unwrap();

    let state_bytes = repo.object_store().get(repo.accepted.state).unwrap();
    let mut state_obj = decode_canonical(&state_bytes).unwrap();
    if let CanonicalPayload::SemanticState(ref mut state) = state_obj.payload {
        state.ontology_version = custom_ont_id;
    }
    let new_state_bytes = canonical_bytes(&state_obj).unwrap();
    let new_state_id = repo.object_store().put(&new_state_bytes).unwrap();

    let accepted_path = root.join(".kat").join("refs").join("accepted");
    let new_accepted = kat::repository::ref_store::AcceptedRef {
        state: new_state_id,
        change: repo.accepted.change,
    };
    fs::write(&accepted_path, new_accepted.to_string()).unwrap();

    let repo_custom = open_repository(root).unwrap();

    let view_core = show_ontology_type(&repo_custom, "kat.core/requirement").unwrap();
    if let OntologyTypeView::Element(e) = view_core {
        assert_eq!(e.type_id, "kat.core/requirement");
    }

    let view_example = show_ontology_type(&repo_custom, "example/requirement").unwrap();
    if let OntologyTypeView::Element(e) = view_example {
        assert_eq!(e.type_id, "example/requirement");
    }

    let err = show_ontology_type(&repo_custom, "requirement").unwrap_err();
    match err {
        QueryError::AmbiguousOntologyType { query, matches } => {
            assert_eq!(query, "requirement");
            assert_eq!(matches, vec!["example/requirement", "kat.core/requirement"]);
        }
        other => panic!("expected AmbiguousOntologyType, got {:?}", other),
    }

    let view_service = show_ontology_type(&repo_custom, "service").unwrap();
    if let OntologyTypeView::Element(e) = view_service {
        assert_eq!(e.type_id, "example/service");
    }

    let view_satisfies = show_ontology_type(&repo_custom, "satisfies").unwrap();
    if let OntologyTypeView::Relationship(rel) = view_satisfies {
        assert_eq!(rel.type_id, "example/satisfies");
        assert_eq!(rel.allowed_source_types, vec!["example/service"]);
        assert_eq!(
            rel.allowed_target_types,
            vec!["example/requirement", "kat.core/requirement"]
        );
    } else {
        panic!("expected RelationshipTypeView");
    }
}

// ---------------------------------------------------------------------------
// Scalable Query Inspection Tests (Phase 18, Step 18.1)
// ---------------------------------------------------------------------------

#[test]
fn trace_origin_max_depth_bounding_and_tree_projection() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    // Intent I1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_intent = ElementId::from_uuid(Uuid::from_u128(9001));
    let prep_i1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_intent,
                    type_id: "kat.core/intent".into(),
                    properties: vec![("title".into(), PropertyValue::Text("Intent".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_i1 =
        prepare_change_revision(prep_i1, ChangeId::from_uuid(Uuid::from_u128(9101)), None).unwrap();
    publish_persisted_change(&repo, persist_prepared_change(&repo, rev_i1).unwrap()).unwrap();

    // Requirement R1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_req = ElementId::from_uuid(Uuid::from_u128(9002));
    let prep_r1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_req,
                    type_id: "kat.core/requirement".into(),
                    properties: vec![("title".into(), PropertyValue::Text("Requirement".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_r1 =
        prepare_change_revision(prep_r1, ChangeId::from_uuid(Uuid::from_u128(9102)), None).unwrap();
    publish_persisted_change(&repo, persist_prepared_change(&repo, rev_r1).unwrap()).unwrap();

    // Implementation M1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_impl = ElementId::from_uuid(Uuid::from_u128(9003));
    let prep_m1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_impl,
                    type_id: "kat.core/implementation".into(),
                    properties: vec![(
                        "title".into(),
                        PropertyValue::Text("Implementation".into()),
                    )],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_m1 =
        prepare_change_revision(prep_m1, ChangeId::from_uuid(Uuid::from_u128(9103)), None).unwrap();
    publish_persisted_change(&repo, persist_prepared_change(&repo, rev_m1).unwrap()).unwrap();

    // Artifact A1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_art = ElementId::from_uuid(Uuid::from_u128(9004));
    let prep_a1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_art,
                    type_id: "kat.core/artifact".into(),
                    properties: vec![("title".into(), PropertyValue::Text("Artifact".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_a1 =
        prepare_change_revision(prep_a1, ChangeId::from_uuid(Uuid::from_u128(9104)), None).unwrap();
    publish_persisted_change(&repo, persist_prepared_change(&repo, rev_a1).unwrap()).unwrap();

    // Link I1 -motivates-> R1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let prep_link1 = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: RelationshipId::from_uuid(Uuid::from_u128(9201)),
                    relationship_type_id: "kat.core/motivates".into(),
                    source_element_id: e_intent,
                    target_element_id: e_req,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_l1 =
        prepare_link_change_revision(prep_link1, ChangeId::from_uuid(Uuid::from_u128(9301)), None)
            .unwrap();
    publish_persisted_link_change(&repo, persist_prepared_link_change(&repo, rev_l1).unwrap())
        .unwrap();

    // Link M1 -realizes-> R1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let prep_link2 = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: RelationshipId::from_uuid(Uuid::from_u128(9202)),
                    relationship_type_id: "kat.core/realizes".into(),
                    source_element_id: e_impl,
                    target_element_id: e_req,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_l2 =
        prepare_link_change_revision(prep_link2, ChangeId::from_uuid(Uuid::from_u128(9302)), None)
            .unwrap();
    publish_persisted_link_change(&repo, persist_prepared_link_change(&repo, rev_l2).unwrap())
        .unwrap();

    // Link A1 -represents-> M1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let prep_link3 = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: RelationshipId::from_uuid(Uuid::from_u128(9203)),
                    relationship_type_id: "kat.core/represents".into(),
                    source_element_id: e_art,
                    target_element_id: e_impl,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_l3 =
        prepare_link_change_revision(prep_link3, ChangeId::from_uuid(Uuid::from_u128(9303)), None)
            .unwrap();
    publish_persisted_link_change(&repo, persist_prepared_link_change(&repo, rev_l3).unwrap())
        .unwrap();

    let reopened = open_repository(root).unwrap();

    // 1. max_depth = Some(0) fails
    let err_zero = trace_origin(&reopened, e_art, Some(0)).unwrap_err();
    assert!(matches!(err_zero, QueryError::InvalidMaxDepth(0)));

    // 2. max_depth = Some(1) -> 1 hop (A1 -> M1)
    let res_d1 = trace_origin(&reopened, e_art, Some(1)).unwrap();
    assert_eq!(res_d1.paths.len(), 1);
    assert_eq!(res_d1.paths[0].steps.len(), 1);
    assert_eq!(res_d1.paths[0].steps[0].to_element_id, e_impl);

    // 3. max_depth = Some(2) -> 2 hops (A1 -> M1 -> R1)
    let res_d2 = trace_origin(&reopened, e_art, Some(2)).unwrap();
    assert_eq!(res_d2.paths.len(), 1);
    assert_eq!(res_d2.paths[0].steps.len(), 2);
    assert_eq!(res_d2.paths[0].steps[1].to_element_id, e_req);

    // 4. max_depth = None -> full 3 hops (A1 -> M1 -> R1 -> I1)
    let res_full = trace_origin(&reopened, e_art, None).unwrap();
    assert_eq!(res_full.paths.len(), 1);
    assert_eq!(res_full.paths[0].steps.len(), 3);
    assert_eq!(res_full.paths[0].steps[2].to_element_id, e_intent);

    // 5. Test to_tree() conversion
    let tree = res_full.to_tree();
    assert_eq!(tree.element_id, e_art);
    assert_eq!(tree.children.len(), 1);
    assert_eq!(tree.children[0].target.element_id, e_impl);
    assert_eq!(tree.children[0].target.children.len(), 1);
    assert_eq!(tree.children[0].target.children[0].target.element_id, e_req);
}

#[test]
fn analyze_impact_max_depth_bounding() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();

    let e_req = ElementId::from_uuid(Uuid::from_u128(9501));
    let prep_r1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_req,
                    type_id: "kat.core/requirement".into(),
                    properties: vec![("title".into(), PropertyValue::Text("Req".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_r1 =
        prepare_change_revision(prep_r1, ChangeId::from_uuid(Uuid::from_u128(9601)), None).unwrap();
    publish_persisted_change(&repo, persist_prepared_change(&repo, rev_r1).unwrap()).unwrap();

    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_impl = ElementId::from_uuid(Uuid::from_u128(9502));
    let prep_m1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_impl,
                    type_id: "kat.core/implementation".into(),
                    properties: vec![("title".into(), PropertyValue::Text("Impl".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_m1 =
        prepare_change_revision(prep_m1, ChangeId::from_uuid(Uuid::from_u128(9602)), None).unwrap();
    publish_persisted_change(&repo, persist_prepared_change(&repo, rev_m1).unwrap()).unwrap();

    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_art = ElementId::from_uuid(Uuid::from_u128(9503));
    let prep_a1 = validate_create_element_invariants(
        validate_create_element_ontology(
            apply_create_element(
                ctx,
                CreateElementInput {
                    element_id: e_art,
                    type_id: "kat.core/artifact".into(),
                    properties: vec![("title".into(), PropertyValue::Text("Artifact".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_a1 =
        prepare_change_revision(prep_a1, ChangeId::from_uuid(Uuid::from_u128(9603)), None).unwrap();
    publish_persisted_change(&repo, persist_prepared_change(&repo, rev_a1).unwrap()).unwrap();

    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let prep_link1 = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: RelationshipId::from_uuid(Uuid::from_u128(9701)),
                    relationship_type_id: "kat.core/realizes".into(),
                    source_element_id: e_impl,
                    target_element_id: e_req,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_l1 =
        prepare_link_change_revision(prep_link1, ChangeId::from_uuid(Uuid::from_u128(9801)), None)
            .unwrap();
    publish_persisted_link_change(&repo, persist_prepared_link_change(&repo, rev_l1).unwrap())
        .unwrap();

    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let prep_link2 = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: RelationshipId::from_uuid(Uuid::from_u128(9702)),
                    relationship_type_id: "kat.core/represents".into(),
                    source_element_id: e_art,
                    target_element_id: e_impl,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_l2 =
        prepare_link_change_revision(prep_link2, ChangeId::from_uuid(Uuid::from_u128(9802)), None)
            .unwrap();
    publish_persisted_link_change(&repo, persist_prepared_link_change(&repo, rev_l2).unwrap())
        .unwrap();

    let reopened = open_repository(root).unwrap();

    // max_depth = Some(0) returns error
    let err_zero = analyze_impact(&reopened, e_req, Some(0)).unwrap_err();
    assert!(matches!(err_zero, QueryError::InvalidMaxDepth(0)));

    // max_depth = Some(1) -> only 1-hop (Implementation M1), artifact A1 (2-hop) excluded
    let res_d1 = analyze_impact(&reopened, e_req, Some(1)).unwrap();
    assert_eq!(res_d1.semantically_affected.len(), 1);
    assert_eq!(res_d1.semantically_affected[0].element_id, e_impl);
    assert_eq!(res_d1.affected_artifacts.len(), 0);

    // max_depth = Some(2) -> includes 2-hop artifact A1
    let res_d2 = analyze_impact(&reopened, e_req, Some(2)).unwrap();
    assert_eq!(res_d2.semantically_affected.len(), 1);
    assert_eq!(res_d2.affected_artifacts.len(), 1);
    assert_eq!(res_d2.affected_artifacts[0].element_id, e_art);
}

#[test]
fn accountability_filtered_stale_only_and_target_id() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let repo = open_repository(root).unwrap();

    // 1. Clean repository: empty report
    let rep_all = kat::repository::analyze_artifact_accountability_filtered(
        &repo,
        kat::repository::ArtifactFilter::default(),
    )
    .unwrap();
    assert!(rep_all.artifacts.is_empty());

    let rep_stale = kat::repository::analyze_artifact_accountability_filtered(
        &repo,
        kat::repository::ArtifactFilter {
            stale_only: true,
            target_artifact_id: None,
        },
    )
    .unwrap();
    assert!(rep_stale.artifacts.is_empty());
}

#[test]
fn accountability_filtered_preserves_repository_summary() {
    use kat::domain::identity::{ChangeId, ElementId, RelationshipId};
    use kat::domain::property::PropertyValue;
    use kat::repository::change::{
        CreateElementInput, LinkElementInput, UpdateElementInput, apply_create_element,
        apply_link_element, apply_update_element, persist_prepared_change,
        persist_prepared_link_change, persist_prepared_update_change, prepare_change,
        prepare_change_revision, prepare_link_change_revision, prepare_update_change_revision,
        publish_persisted_change, publish_persisted_link_change, publish_persisted_update_change,
        validate_create_element_invariants, validate_create_element_ontology,
        validate_link_element_invariants, validate_link_element_ontology,
        validate_update_element_invariants, validate_update_element_ontology,
    };
    use uuid::Uuid;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    // Helper to create element
    let create_elem = |type_id: &str, title: &str, num: u128| -> ElementId {
        let repo = open_repository(root).unwrap();
        let ctx = prepare_change(&repo).unwrap();
        let el_id = ElementId::from_uuid(Uuid::from_u128(num));
        let prep = validate_create_element_invariants(
            validate_create_element_ontology(
                apply_create_element(
                    ctx,
                    CreateElementInput {
                        element_id: el_id,
                        type_id: type_id.into(),
                        properties: vec![("title".into(), PropertyValue::Text(title.into()))],
                    },
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let rev =
            prepare_change_revision(prep, ChangeId::from_uuid(Uuid::from_u128(num + 1000)), None)
                .unwrap();
        let pers = persist_prepared_change(&repo, rev).unwrap();
        publish_persisted_change(&repo, pers).unwrap();
        el_id
    };

    let req = create_elem("kat.core/requirement", "Req 1", 5001);
    let imp = create_elem("kat.core/implementation", "Impl 1", 5002);
    let art1 = create_elem("kat.core/artifact", "src/a.js", 5003);
    let art2 = create_elem("kat.core/artifact", "src/b.js", 5004);

    let link_elems = |type_id: &str, src: ElementId, tgt: ElementId, num: u128| {
        let repo = open_repository(root).unwrap();
        let ctx = prepare_change(&repo).unwrap();
        let rel_id = RelationshipId::from_uuid(Uuid::from_u128(num));
        let prep = validate_link_element_invariants(
            validate_link_element_ontology(
                apply_link_element(
                    &repo,
                    ctx,
                    LinkElementInput {
                        relationship_id: rel_id,
                        relationship_type_id: type_id.into(),
                        source_element_id: src,
                        target_element_id: tgt,
                        properties: vec![],
                    },
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let rev = prepare_link_change_revision(
            prep,
            ChangeId::from_uuid(Uuid::from_u128(num + 1000)),
            None,
        )
        .unwrap();
        let pers = persist_prepared_link_change(&repo, rev).unwrap();
        publish_persisted_link_change(&repo, pers).unwrap();
    };

    link_elems("kat.core/realizes", imp, req, 6001);
    link_elems("kat.core/represents", art1, imp, 6002);
    link_elems("kat.core/represents", art2, imp, 6003);

    // Update imp -> art1 and art2 become stale
    let repo_up = open_repository(root).unwrap();
    let ctx_up = prepare_change(&repo_up).unwrap();
    let imp_ver = kat::repository::query::show_element(&repo_up, imp)
        .unwrap()
        .version_id;
    let prep_up = validate_update_element_invariants(
        validate_update_element_ontology(
            apply_update_element(
                &repo_up,
                ctx_up,
                UpdateElementInput {
                    element_id: imp,
                    expected_version: imp_ver,
                    properties: vec![("title".into(), PropertyValue::Text("Impl 1 v2".into()))],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let rev_up =
        prepare_update_change_revision(prep_up, ChangeId::from_uuid(Uuid::from_u128(7001)), None)
            .unwrap();
    let pers_up = persist_prepared_update_change(&repo_up, rev_up).unwrap();
    publish_persisted_update_change(&repo_up, pers_up).unwrap();

    let repo_read = open_repository(root).unwrap();
    let rep_stale = kat::repository::analyze_artifact_accountability_filtered(
        &repo_read,
        kat::repository::ArtifactFilter {
            stale_only: true,
            target_artifact_id: None,
        },
    )
    .unwrap();

    // Filtered artifacts vector contains only 2 stale items
    assert_eq!(rep_stale.artifacts.len(), 2);
    // Repository summary totals remain repository-wide (total: 2, stale: 2)
    assert_eq!(rep_stale.repository_summary.total, 2);
    assert_eq!(rep_stale.repository_summary.stale, 2);
    assert_eq!(rep_stale.repository_summary.current, 0);
}

#[test]
fn validate_coverage_category_filtering_ontology_driven() {
    use kat::domain::identity::{ChangeId, ElementId, RelationshipId};
    use kat::domain::property::PropertyValue;
    use kat::repository::change::{
        CreateElementInput, LinkElementInput, apply_create_element, apply_link_element,
        persist_prepared_change, persist_prepared_link_change, prepare_change,
        prepare_change_revision, prepare_link_change_revision, publish_persisted_change,
        publish_persisted_link_change, validate_create_element_invariants,
        validate_create_element_ontology, validate_link_element_invariants,
        validate_link_element_ontology,
    };
    use uuid::Uuid;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let create_elem = |type_id: &str, title: &str, num: u128| -> ElementId {
        let repo = open_repository(root).unwrap();
        let ctx = prepare_change(&repo).unwrap();
        let el_id = ElementId::from_uuid(Uuid::from_u128(num));
        let prep = validate_create_element_invariants(
            validate_create_element_ontology(
                apply_create_element(
                    ctx,
                    CreateElementInput {
                        element_id: el_id,
                        type_id: type_id.into(),
                        properties: vec![("title".into(), PropertyValue::Text(title.into()))],
                    },
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let rev =
            prepare_change_revision(prep, ChangeId::from_uuid(Uuid::from_u128(num + 1000)), None)
                .unwrap();
        let pers = persist_prepared_change(&repo, rev).unwrap();
        publish_persisted_change(&repo, pers).unwrap();
        el_id
    };

    let req = create_elem("kat.core/requirement", "Req", 8001);
    let imp = create_elem("kat.core/implementation", "Impl", 8002);
    let art = create_elem("kat.core/artifact", "src/app.js", 8003);
    let _dec = create_elem("kat.core/design-decision", "Decision", 8004);

    let link_elems = |type_id: &str, src: ElementId, tgt: ElementId, num: u128| {
        let repo = open_repository(root).unwrap();
        let ctx = prepare_change(&repo).unwrap();
        let rel_id = RelationshipId::from_uuid(Uuid::from_u128(num));
        let prep = validate_link_element_invariants(
            validate_link_element_ontology(
                apply_link_element(
                    &repo,
                    ctx,
                    LinkElementInput {
                        relationship_id: rel_id,
                        relationship_type_id: type_id.into(),
                        source_element_id: src,
                        target_element_id: tgt,
                        properties: vec![],
                    },
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let rev = prepare_link_change_revision(
            prep,
            ChangeId::from_uuid(Uuid::from_u128(num + 1000)),
            None,
        )
        .unwrap();
        let pers = persist_prepared_link_change(&repo, rev).unwrap();
        publish_persisted_link_change(&repo, pers).unwrap();
    };

    link_elems("kat.core/realizes", imp, req, 9001);
    link_elems("kat.core/represents", art, imp, 9002);

    let repo_read = open_repository(root).unwrap();
    let val_report = kat::repository::validation::validate_repository(&repo_read).unwrap();

    // Only validatable categories (requirement, implementation) appear in coverage summaries
    let cat_types: Vec<&str> = val_report
        .category_summaries
        .iter()
        .map(|c| c.category_type.as_str())
        .collect();
    assert!(cat_types.contains(&"kat.core/requirement"));
    assert!(cat_types.contains(&"kat.core/implementation"));
    // Non-validatable categories (artifact, design-decision) are excluded
    assert!(!cat_types.contains(&"kat.core/artifact"));
    assert!(!cat_types.contains(&"kat.core/design-decision"));

    // Uncovered elements list also excludes non-validatable types
    for unc in &val_report.uncovered_elements {
        assert_ne!(unc.type_id, "kat.core/artifact");
        assert_ne!(unc.type_id, "kat.core/design-decision");
    }
}

#[test]
fn retrieve_context_traversal_and_categorization() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let FirstChange { element_id: req_id, .. } = publish_first_change(root, 101, 201);
    let FirstChange { element_id: imp_id, .. } = publish_second_change(root, req_id, 102, 202);

    let repo = open_repository(root).unwrap();
    let ctx = kat::repository::query::retrieve_context(
        &repo,
        &[req_id],
        kat::repository::query::ContextDirection::Both,
        Some(3),
        true,
    )
    .unwrap();

    assert_eq!(ctx.roots, vec![req_id]);
    assert!(ctx.elements.iter().any(|e| e.element_id == req_id));
    assert!(ctx.elements.iter().any(|e| e.element_id == imp_id));

    let cat = ctx.categorized.unwrap();
    assert!(cat.requirements.contains(&req_id));
    assert!(cat.realizations.contains(&imp_id));
}
