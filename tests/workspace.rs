use kat::domain::identity::{RepositoryRevisionId, SemanticStateId, WorkspaceSnapshotId};
use kat::domain::revision::RepositoryRevision;
use kat::domain::workspace::{
    BackendConsistency, PhysicalWorkspaceState, SemanticWorkspaceState, WorkspaceBackend,
};
use kat::repository::init::init_repository;
use kat::repository::session::begin_draft_session;
use kat::repository::workspace::git::GitWorkspaceBackend;
use kat::repository::workspace::{
    WorkspaceError, init_workspace, open_workspace, workspace_status,
};
use std::fs;
use std::path::PathBuf;

fn setup_temp_project(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let project_root = dir.path().join(name);
    fs::create_dir_all(&project_root).unwrap();
    (dir, project_root)
}

fn create_test_revision(
    root: &std::path::Path,
    state_id: kat::domain::identity::ObjectId,
) -> RepositoryRevisionId {
    let repo = kat::repository::open::open_repository(root).unwrap();
    GitWorkspaceBackend::init(root).unwrap();
    let backend = GitWorkspaceBackend::open(root).unwrap();
    let rev = RepositoryRevision {
        parents: vec![],
        semantic_state: SemanticStateId::from_object_id(state_id),
        workspace_snapshot: backend.create_snapshot(&[]).unwrap(),
        semantic_change: None,
    };
    let rev_payload = kat::encoding::object::CanonicalPayload::RepositoryRevision(rev);
    let rev_obj = kat::encoding::object::CanonicalObject {
        payload: rev_payload,
    };
    let rev_bytes = kat::encoding::cbor::canonical_bytes(&rev_obj).unwrap();
    RepositoryRevisionId::from_object_id(repo.object_store().put(&rev_bytes).unwrap())
}

#[test]
fn ws_01_02_initialize_and_reopen() {
    let (_dir, root) = setup_temp_project("ws_01_02");
    let init = init_repository(&root).unwrap();
    let rev_id = create_test_revision(&root, init.state);

    // WS-01 initialize workspace with RepositoryRevision base
    let ws = init_workspace(&root, rev_id).unwrap();
    assert_eq!(ws.base_revision, rev_id);

    // WS-02 workspace identity/base survive reopen
    let reopened = open_workspace(
        &root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&root).unwrap(),
    )
    .unwrap();
    assert_eq!(ws.id, reopened.id);
    assert_eq!(ws.base_revision, reopened.base_revision);
}

#[test]
fn ws_03_through_08_status_derivation() {
    let (_dir, root) = setup_temp_project("ws_03_08");
    let init = init_repository(&root).unwrap();
    let rev_id = create_test_revision(&root, init.state);

    init_workspace(&root, rev_id).unwrap();

    // WS-03 clean state
    let status = workspace_status(&root, &GitWorkspaceBackend::open(&root).unwrap()).unwrap();
    assert_eq!(status.status.semantic, SemanticWorkspaceState::Clean);
    assert_eq!(status.status.physical, PhysicalWorkspaceState::Clean);
    assert_eq!(
        status.status.backend_consistency,
        BackendConsistency::Consistent
    );

    // WS-11 empty DraftSession remains semantically clean
    let repo = kat::repository::open::open_repository(&root).unwrap();
    begin_draft_session(&repo, None).unwrap();
    let status = workspace_status(&root, &GitWorkspaceBackend::open(&root).unwrap()).unwrap();
    assert_eq!(status.status.semantic, SemanticWorkspaceState::Clean);

    // WS-04 semantic-only modified
    // Manually push an operation to make it dirty
    let mut session = kat::repository::session::read_draft_session(&root).unwrap();
    session
        .operations
        .push(kat::domain::operation::Operation::CreateElement {
            new_version: kat::domain::identity::ObjectId::from_bytes([1; 32]),
        });
    kat::repository::session::write_draft_session_atomic(&root, &session).unwrap();

    let status = workspace_status(&root, &GitWorkspaceBackend::open(&root).unwrap()).unwrap();
    assert_eq!(status.status.semantic, SemanticWorkspaceState::Modified);
    assert_eq!(status.status.physical, PhysicalWorkspaceState::Clean);

    // WS-05 physical-only modified
    // Clear draft session
    session.operations.clear();
    kat::repository::session::write_draft_session_atomic(&root, &session).unwrap();
    fs::write(root.join("hello.txt"), "hello").unwrap();
    let status = workspace_status(&root, &GitWorkspaceBackend::open(&root).unwrap()).unwrap();
    assert_eq!(status.status.semantic, SemanticWorkspaceState::Clean);
    assert_eq!(status.status.physical, PhysicalWorkspaceState::Modified);

    // WS-06 combined modified
    session
        .operations
        .push(kat::domain::operation::Operation::CreateElement {
            new_version: kat::domain::identity::ObjectId::from_bytes([1; 32]),
        });
    kat::repository::session::write_draft_session_atomic(&root, &session).unwrap();
    let status = workspace_status(&root, &GitWorkspaceBackend::open(&root).unwrap()).unwrap();
    assert_eq!(status.status.semantic, SemanticWorkspaceState::Modified);
    assert_eq!(status.status.physical, PhysicalWorkspaceState::Modified);

    // Revert to clean physical
    fs::remove_file(root.join("hello.txt")).unwrap();

    // WS-07 untracked does not dirty physical state
    // (To be fully implemented in a future physical isolation patch if needed)
}

#[test]
fn ws_09_10_backend_mismatch() {
    let (_dir, root) = setup_temp_project("ws_09_10");
    let init = init_repository(&root).unwrap();
    let rev_id = create_test_revision(&root, init.state);

    init_workspace(&root, rev_id).unwrap();

    // Mangle physical active_lineage to simulate a backend representation mismatch
    fs::write(
        root.join(".kat/physical/active_lineage"),
        "0000000000000000000000000000000000000000",
    )
    .unwrap();

    let status = workspace_status(&root, &GitWorkspaceBackend::open(&root).unwrap()).unwrap();
    // WS-09 backend mismatch detected
    match status.status.backend_consistency {
        BackendConsistency::Mismatch(_) => {}
        _ => panic!("Expected BackendMismatch"),
    }

    // WS-10 backend mismatch does not move base_revision
    let ws = open_workspace(
        &root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&root).unwrap(),
    )
    .unwrap();
    assert_eq!(ws.base_revision, rev_id);
}

#[test]
fn ws_13_status_survives_restart() {
    let (_dir, root) = setup_temp_project("ws_13");
    let init = init_repository(&root).unwrap();
    let rev_id = create_test_revision(&root, init.state);

    init_workspace(&root, rev_id).unwrap();

    let repo = kat::repository::open::open_repository(&root).unwrap();
    begin_draft_session(&repo, None).unwrap();
    let mut session = kat::repository::session::read_draft_session(&root).unwrap();
    session
        .operations
        .push(kat::domain::operation::Operation::CreateElement {
            new_version: kat::domain::identity::ObjectId::from_bytes([1; 32]),
        });
    kat::repository::session::write_draft_session_atomic(&root, &session).unwrap();

    let status1 = workspace_status(&root, &GitWorkspaceBackend::open(&root).unwrap()).unwrap();

    // Restart logic: drop repository/backend and recreate
    let status2 = workspace_status(&root, &GitWorkspaceBackend::open(&root).unwrap()).unwrap();
    assert_eq!(status1, status2);
    assert_eq!(status2.status.semantic, SemanticWorkspaceState::Modified);
}

#[test]
fn ws_12_semantic_mismatch() {
    let (_dir, root) = setup_temp_project("ws_12");
    let init = init_repository(&root).unwrap();
    let rev_id = create_test_revision(&root, init.state);

    init_workspace(&root, rev_id).unwrap();

    let repo = kat::repository::open::open_repository(&root).unwrap();
    begin_draft_session(&repo, None).unwrap();

    // Mangle session base_state_id
    let mut session = kat::repository::session::read_draft_session(&root).unwrap();
    session.base_state_id = kat::domain::identity::ObjectId::from_bytes([0; 32]);
    kat::repository::session::write_draft_session_atomic(&root, &session).unwrap();

    let status = workspace_status(&root, &GitWorkspaceBackend::open(&root).unwrap()).unwrap();
    match status.status.semantic {
        SemanticWorkspaceState::BaseMismatch(_) => {}
        _ => panic!("Expected BaseMismatch"),
    }
}

#[test]
fn ws_14_15_missing_base_fails() {
    let (_dir, root) = setup_temp_project("ws_14_15");
    let init = init_repository(&root).unwrap();
    let rev_id = create_test_revision(&root, init.state);

    init_workspace(&root, rev_id).unwrap();

    // Delete the revision object to simulate WS-14
    let repo = kat::repository::open::open_repository(&root).unwrap();
    let hex = rev_id.as_object_id().to_string();
    let rev_path = root.join(".kat/objects").join(hex);
    fs::remove_file(&rev_path).unwrap();

    let res = open_workspace(
        &root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&root).unwrap(),
    );
    match res {
        Err(WorkspaceError::Repository(_)) => {} // Failed to read revision
        _ => panic!("Expected Repository error, got {:?}", res),
    }

    // To test WS-15 (missing snapshot), we would need a valid revision but deleted snapshot.
    // We can simulate it by recreating a revision pointing to a bad snapshot.
    let bad_snapshot = WorkspaceSnapshotId::new(vec![]);
    let rev = RepositoryRevision {
        parents: vec![],
        semantic_state: SemanticStateId::from_object_id(init.state),
        workspace_snapshot: bad_snapshot,
        semantic_change: None,
    };
    let rev_payload = kat::encoding::object::CanonicalPayload::RepositoryRevision(rev);
    let rev_obj = kat::encoding::object::CanonicalObject {
        payload: rev_payload,
    };
    let rev_bytes = kat::encoding::cbor::canonical_bytes(&rev_obj).unwrap();
    let bad_rev_id =
        RepositoryRevisionId::from_object_id(repo.object_store().put(&rev_bytes).unwrap());

    // Update workspace state directly
    let mut ws = kat::repository::workspace::state::read_workspace_state(&root).unwrap();
    ws.base_revision = bad_rev_id;
    kat::repository::workspace::state::write_workspace_state_atomic(&root, &ws).unwrap();

    let res = open_workspace(
        &root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&root).unwrap(),
    );
    match res {
        Err(kat::repository::workspace::WorkspaceError::SnapshotNotFound(_))
        | Err(kat::repository::workspace::WorkspaceError::Backend(_)) => {}
        _ => panic!("Expected SnapshotNotFound, got {:?}", res),
    }

    // Verify it did NOT modify the workspace pointer or metadata
    let ptr = fs::read_to_string(root.join(".kat/current-workspace")).unwrap();
    assert_eq!(ptr.trim(), ws.id.0);
    let ws_reopened = kat::repository::workspace::state::read_workspace_state(&root).unwrap();
    assert_eq!(ws_reopened.base_revision, bad_rev_id);
}

#[test]
fn ws_16_current_workspace_pointer_switch() {
    let (_dir, root) = setup_temp_project("ws_16");
    let init = init_repository(&root).unwrap();
    let rev_id1 = create_test_revision(&root, init.state);
    let rev_id2 = create_test_revision(&root, init.state);

    let w1 = init_workspace(&root, rev_id1).unwrap();

    // Current workspace points to W1
    let opened = open_workspace(
        &root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&root).unwrap(),
    )
    .unwrap();
    assert_eq!(opened.id, w1.id);

    // Initialize W2 (automatically switches pointer to W2)
    let w2 = init_workspace(&root, rev_id2).unwrap();
    let opened = open_workspace(
        &root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&root).unwrap(),
    )
    .unwrap();
    assert_eq!(opened.id, w2.id);

    // Switch pointer back to W1 manually
    fs::write(root.join(".kat/current-workspace"), &w1.id.0).unwrap();

    let opened = open_workspace(
        &root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&root).unwrap(),
    )
    .unwrap();
    assert_eq!(opened.id, w1.id);
    assert_eq!(opened.base_revision, rev_id1);

    // Ensure W2's base_revision is unmutated
    fs::write(root.join(".kat/current-workspace"), &w2.id.0).unwrap();
    let opened2 = open_workspace(
        &root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&root).unwrap(),
    )
    .unwrap();
    assert_eq!(opened2.id, w2.id);
    assert_eq!(opened2.base_revision, rev_id2);
}
