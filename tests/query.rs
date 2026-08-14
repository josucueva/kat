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
use kat::domain::element::Lifecycle;
use kat::domain::identity::{ChangeId, ElementId, ObjectId, RelationshipId};
use kat::domain::operation::Operation;
use kat::domain::property::PropertyValue;
use kat::domain::state::{ElementStateEntry, SemanticState};
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
    QueryError, TraversalDirection, analyze_impact, history, show_element, trace_origin,
};
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
    let err = trace_origin(&repo, missing_id).unwrap_err();
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
    let res_req = trace_origin(&reopened, e_req).unwrap();
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
    let res_intent = trace_origin(&reopened, e_intent).unwrap();
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
    let res = trace_origin(&reopened, e_art).unwrap();
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
    trace_origin(&repo, ids.element_id).unwrap();

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
    let err = analyze_impact(&repo, missing_id).unwrap_err();
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
    let res = analyze_impact(&reopened, e_req).unwrap();
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
    analyze_impact(&repo, ids.element_id).unwrap();

    assert_eq!(object_ids(root), objects_before);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
}
