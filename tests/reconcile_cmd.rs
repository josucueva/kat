use kat::domain::identity::{RepositoryRevisionId, SemanticStateId};
use kat::domain::revision::RepositoryRevision;
use kat::domain::workspace::WorkspaceBackend;
use kat::encoding::cbor::canonical_bytes;
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::repository::object_store::ObjectStore;
use kat::repository::reconcile::{ReconcileWorkspaceOutcome, reconcile_workspace};
use kat::repository::workspace::git::GitWorkspaceBackend;
use kat::repository::workspace::{init_workspace, open_workspace};

fn setup_temp_project(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let project_root = dir.path().join(name);
    std::fs::create_dir_all(&project_root).unwrap();
    (dir, project_root)
}

fn put_mock_revision(
    obj_store: &ObjectStore,
    root: &std::path::Path,
    byte: u8,
    mut parents: Vec<RepositoryRevisionId>,
) -> RepositoryRevisionId {
    parents.sort_by_key(|id| id.to_string());

    // Put ontology
    let ont = kat::domain::ontology::OntologyVersion {
        ontology_id: kat::domain::identity::OntologyId::from_uuid(uuid::Uuid::from_bytes([0; 16])),
        element_types: vec![],
        relationship_types: vec![],
    };
    let ont_obj = CanonicalObject {
        payload: CanonicalPayload::OntologyVersion(ont),
    };
    let ont_bytes = canonical_bytes(&ont_obj).unwrap();
    let ont_id = obj_store.put(&ont_bytes).unwrap();

    // Put element
    let element_id =
        kat::domain::identity::ElementId::from_uuid(uuid::Uuid::from_bytes([byte; 16]));
    let ev = kat::domain::element::KnowledgeElementVersion {
        element_id,
        type_id: "kat.core/requirement".to_string(),
        lifecycle: kat::domain::element::Lifecycle::Active,
        properties: vec![],
    };
    let ev_obj = CanonicalObject {
        payload: CanonicalPayload::KnowledgeElementVersion(ev),
    };
    let ev_bytes = canonical_bytes(&ev_obj).unwrap();
    let ev_id = obj_store.put(&ev_bytes).unwrap();

    // Put semantic state
    let state = kat::domain::state::SemanticState {
        ontology_version: ont_id,
        elements: vec![kat::domain::state::ElementStateEntry {
            element_id,
            version: ev_id,
        }],
        relationships: vec![],
    };
    let state_obj = CanonicalObject {
        payload: CanonicalPayload::SemanticState(state),
    };
    let state_bytes = canonical_bytes(&state_obj).unwrap();
    let state_id = obj_store.put(&state_bytes).unwrap();

    let backend = GitWorkspaceBackend::open(root).unwrap();
    let snapshot = backend.create_snapshot(&[]).unwrap();

    let rev = RepositoryRevision {
        parents,
        semantic_state: SemanticStateId::from_object_id(state_id),
        workspace_snapshot: snapshot,
        semantic_change: None,
    };
    let obj = CanonicalObject {
        payload: CanonicalPayload::RepositoryRevision(rev),
    };
    let bytes = canonical_bytes(&obj).unwrap();
    RepositoryRevisionId::from_object_id(obj_store.put(&bytes).unwrap())
}

#[test]
fn rec_cmd_01_same_no_session_no_mutation() {
    let (_dir, root) = setup_temp_project("rec_cmd_01");
    kat::repository::init::init_repository(&root).unwrap();
    GitWorkspaceBackend::init(&root).unwrap();
    let repo = kat::repository::open::open_repository(&root).unwrap();
    let store = repo.object_store();
    let rev_a = put_mock_revision(store, &root, 1, vec![]);
    init_workspace(&root, rev_a).unwrap();

    let outcome = reconcile_workspace(
        &root,
        &open_workspace(
            &root,
            &kat::repository::workspace::git::GitWorkspaceBackend::open(&root).unwrap(),
        )
        .unwrap()
        .id,
        rev_a,
    )
    .unwrap();
    assert_eq!(outcome, ReconcileWorkspaceOutcome::Same);
}

#[test]
fn rec_cmd_02_local_ahead() {
    let (_dir, root) = setup_temp_project("rec_cmd_02");
    kat::repository::init::init_repository(&root).unwrap();
    GitWorkspaceBackend::init(&root).unwrap();
    let repo = kat::repository::open::open_repository(&root).unwrap();
    let store = repo.object_store();
    let rev_base = put_mock_revision(store, &root, 1, vec![]);
    let rev_local = put_mock_revision(store, &root, 2, vec![rev_base]);
    init_workspace(&root, rev_local).unwrap();

    let outcome = reconcile_workspace(
        &root,
        &open_workspace(
            &root,
            &kat::repository::workspace::git::GitWorkspaceBackend::open(&root).unwrap(),
        )
        .unwrap()
        .id,
        rev_base,
    )
    .unwrap();
    assert_eq!(outcome, ReconcileWorkspaceOutcome::LocalAhead);
}

#[test]
fn rec_cmd_03_other_ahead() {
    let (_dir, root) = setup_temp_project("rec_cmd_03");
    kat::repository::init::init_repository(&root).unwrap();
    GitWorkspaceBackend::init(&root).unwrap();
    let repo = kat::repository::open::open_repository(&root).unwrap();
    let store = repo.object_store();
    let rev_base = put_mock_revision(store, &root, 1, vec![]);
    let rev_target = put_mock_revision(store, &root, 2, vec![rev_base]);
    init_workspace(&root, rev_base).unwrap();

    let outcome = reconcile_workspace(
        &root,
        &open_workspace(
            &root,
            &kat::repository::workspace::git::GitWorkspaceBackend::open(&root).unwrap(),
        )
        .unwrap()
        .id,
        rev_target,
    )
    .unwrap();
    assert_eq!(outcome, ReconcileWorkspaceOutcome::OtherAhead);
}

#[test]
fn rec_cmd_08_ambiguous_merge_base() {
    let (_dir, root) = setup_temp_project("rec_cmd_08");
    kat::repository::init::init_repository(&root).unwrap();
    GitWorkspaceBackend::init(&root).unwrap();
    let repo = kat::repository::open::open_repository(&root).unwrap();
    let store = repo.object_store();
    let base1 = put_mock_revision(store, &root, 1, vec![]);
    let base2 = put_mock_revision(store, &root, 2, vec![]);
    let local = put_mock_revision(store, &root, 3, vec![base1, base2]);
    let target = put_mock_revision(store, &root, 4, vec![base1, base2]);
    init_workspace(&root, local).unwrap();

    let result = reconcile_workspace(
        &root,
        &open_workspace(
            &root,
            &kat::repository::workspace::git::GitWorkspaceBackend::open(&root).unwrap(),
        )
        .unwrap()
        .id,
        target,
    );
    match result {
        Err(kat::repository::query::QueryError::AmbiguousMergeBase(bases)) => {
            assert_eq!(bases.len(), 2);
        }
        _ => panic!("Expected AmbiguousMergeBase"),
    }
}

#[test]
fn rec_cmd_09_unrelated() {
    let (_dir, root) = setup_temp_project("rec_cmd_09");
    kat::repository::init::init_repository(&root).unwrap();
    GitWorkspaceBackend::init(&root).unwrap();
    let repo = kat::repository::open::open_repository(&root).unwrap();
    let store = repo.object_store();
    let local = put_mock_revision(store, &root, 1, vec![]);
    let target = put_mock_revision(store, &root, 2, vec![]);
    init_workspace(&root, local).unwrap();

    let result = reconcile_workspace(
        &root,
        &open_workspace(
            &root,
            &kat::repository::workspace::git::GitWorkspaceBackend::open(&root).unwrap(),
        )
        .unwrap()
        .id,
        target,
    );
    match result {
        Err(kat::repository::query::QueryError::UnrelatedHistory) => {}
        _ => panic!("Expected UnrelatedHistory"),
    }
}

#[test]
fn rec_cmd_10_draft_session_rejected() {
    let (_dir, root) = setup_temp_project("rec_cmd_10");
    kat::repository::init::init_repository(&root).unwrap();
    GitWorkspaceBackend::init(&root).unwrap();
    let repo = kat::repository::open::open_repository(&root).unwrap();
    let store = repo.object_store();
    let base = put_mock_revision(store, &root, 1, vec![]);
    let local = put_mock_revision(store, &root, 2, vec![base]);
    let target = put_mock_revision(store, &root, 3, vec![base]);
    let ws = init_workspace(&root, local).unwrap();

    // Create fake draft session
    let path = root.join(".kat").join("work").join("change");
    std::fs::create_dir_all(&path).unwrap();
    let file_path = path.join("session.json");
    std::fs::write(&file_path, "{}").unwrap();

    let result = reconcile_workspace(&root, &ws.id, target);
    match result {
        Err(kat::repository::query::QueryError::WorkspaceConflict(_)) => {}
        other => panic!(
            "Expected WorkspaceConflict due to draft session, got {:?}",
            other
        ),
    }
}

#[test]
fn rec_cmd_11_reconciliation_session_rejected() {
    let (_dir, root) = setup_temp_project("rec_cmd_11");
    kat::repository::init::init_repository(&root).unwrap();
    GitWorkspaceBackend::init(&root).unwrap();
    let repo = kat::repository::open::open_repository(&root).unwrap();
    let store = repo.object_store();
    let base = put_mock_revision(store, &root, 1, vec![]);
    let local = put_mock_revision(store, &root, 2, vec![base]);
    let target = put_mock_revision(store, &root, 3, vec![base]);
    let ws = init_workspace(&root, local).unwrap();

    let session = kat::repository::reconcile::ReconciliationSession {
        version: 1,
        workspace_id: ws.id.clone(),
        base_revision: local,
        target_revision: target,
        state: kat::repository::reconcile::ReconciliationSessionState::PreparedClean {
            revision: target,
        },
    };
    let backend = kat::repository::workspace::fake::FakeWorkspaceBackend::with_root(&root);
    kat::repository::reconcile::save_reconciliation_session(&root, &ws.id, &session, &backend)
        .unwrap();

    let result = reconcile_workspace(&root, &ws.id, target);
    match result {
        Err(kat::repository::query::QueryError::WorkspaceConflict(_)) => {}
        _ => panic!("Expected WorkspaceConflict due to reconciliation session"),
    }
}

#[test]
fn rec_cmd_04_simple_divergence_clean_reconciliation() {
    let (_dir, root) = setup_temp_project("rec_cmd_04");
    kat::repository::init::init_repository(&root).unwrap();
    GitWorkspaceBackend::init(&root).unwrap();
    let repo = kat::repository::open::open_repository(&root).unwrap();
    let store = repo.object_store();
    let base = put_mock_revision(store, &root, 1, vec![]);
    let local = put_mock_revision(store, &root, 2, vec![base]);
    let target = put_mock_revision(store, &root, 3, vec![base]);
    let ws = init_workspace(&root, local).unwrap();

    let outcome = reconcile_workspace(&root, &ws.id, target).unwrap();
    match outcome {
        ReconcileWorkspaceOutcome::Prepared { session } => {
            assert_eq!(session.base_revision, local);
            assert_eq!(session.target_revision, target);
            match session.state {
                kat::repository::reconcile::ReconciliationSessionState::PreparedClean {
                    revision,
                } => {
                    // It should create a new revision merging local and target
                    let merge_rev = repo.read_revision(revision).unwrap();
                    assert_eq!(merge_rev.parents.len(), 2);
                    assert!(merge_rev.parents.contains(&local));
                    assert!(merge_rev.parents.contains(&target));
                }
                other => panic!("Expected PreparedClean, got {:?}", other),
            }
        }
        other => panic!("Expected Prepared outcome, got {:?}", other),
    }
}

#[test]
fn rec_cmd_12_clean_reconciliation_moves_no_authority() {
    let (_dir, root) = setup_temp_project("rec_cmd_12");
    kat::repository::init::init_repository(&root).unwrap();
    GitWorkspaceBackend::init(&root).unwrap();
    let repo = kat::repository::open::open_repository(&root).unwrap();
    let store = repo.object_store();
    let base = put_mock_revision(store, &root, 1, vec![]);
    let local = put_mock_revision(store, &root, 2, vec![base]);
    let target = put_mock_revision(store, &root, 3, vec![base]);
    let ws = init_workspace(&root, local).unwrap();

    let _outcome = reconcile_workspace(&root, &ws.id, target).unwrap();

    let updated_ws = open_workspace(
        &root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&root).unwrap(),
    )
    .unwrap();
    assert_eq!(updated_ws.base_revision, local);
}

#[test]
fn rec_cmd_14_session_persistence_survives_restart() {
    let (_dir, root) = setup_temp_project("rec_cmd_14");
    kat::repository::init::init_repository(&root).unwrap();
    GitWorkspaceBackend::init(&root).unwrap();
    let repo = kat::repository::open::open_repository(&root).unwrap();
    let store = repo.object_store();
    let base = put_mock_revision(store, &root, 1, vec![]);
    let local = put_mock_revision(store, &root, 2, vec![base]);
    let target = put_mock_revision(store, &root, 3, vec![base]);
    let ws = init_workspace(&root, local).unwrap();

    let _outcome = reconcile_workspace(&root, &ws.id, target).unwrap();

    // Reload the session from disk using the expected path
    let session_path = root
        .join(".kat")
        .join("workspaces")
        .join(&ws.id.0)
        .join("reconciliation_session.json");
    assert!(session_path.exists());
    let data = std::fs::read_to_string(session_path).unwrap();
    let session: kat::repository::reconcile::ReconciliationSession =
        serde_json::from_str(&data).unwrap();

    assert_eq!(session.base_revision, local);
    assert_eq!(session.target_revision, target);
}
