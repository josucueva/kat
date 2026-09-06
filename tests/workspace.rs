use kat::domain::identity::{RepositoryRevisionId, SemanticStateId};
use kat::domain::revision::RepositoryRevision;
use kat::domain::workspace::WorkspaceBackend;
use kat::domain::workspace::WorkspaceStatus;
use kat::repository::init::init_repository;
use kat::repository::session::begin_draft_session;
use kat::repository::workspace::git::GitWorkspaceBackend;
use kat::repository::workspace::{init_workspace, open_workspace, workspace_status};
use std::fs;
use std::path::PathBuf;

fn setup_temp_project(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let project_root = dir.path().join(name);
    fs::create_dir_all(&project_root).unwrap();
    (dir, project_root)
}

#[test]
fn workspace_persistence_and_restart() {
    let (_dir, root) = setup_temp_project("workspace_persistence");
    let init = init_repository(&root).unwrap();

    let repo = kat::repository::open::open_repository(&root).unwrap();

    GitWorkspaceBackend::init(&root).unwrap();
    let backend = GitWorkspaceBackend::open(&root).unwrap();
    let rev = RepositoryRevision {
        parents: vec![],
        semantic_state: SemanticStateId::from_object_id(init.state),
        workspace_snapshot: backend.create_snapshot(&[]).unwrap(),
        semantic_change: None,
    };
    let rev_payload = kat::encoding::object::CanonicalPayload::RepositoryRevision(rev);
    let rev_obj = kat::encoding::object::CanonicalObject {
        payload: rev_payload,
    };
    let rev_bytes = kat::encoding::cbor::canonical_bytes(&rev_obj).unwrap();
    let rev_id = RepositoryRevisionId::from_object_id(repo.object_store().put(&rev_bytes).unwrap());

    let ws = init_workspace(&root, rev_id).unwrap();
    assert_eq!(ws.base_revision, rev_id);

    let opened = open_workspace(&root).unwrap();
    assert_eq!(ws.id, opened.id);
    assert_eq!(ws.base_revision, opened.base_revision);
}

#[test]
fn workspace_status_derivation() {
    let (_dir, root) = setup_temp_project("workspace_status");
    let init = init_repository(&root).unwrap();
    GitWorkspaceBackend::init(&root).unwrap();

    let repo = kat::repository::open::open_repository(&root).unwrap();

    let backend = GitWorkspaceBackend::open(&root).unwrap();
    let rev = RepositoryRevision {
        parents: vec![],
        semantic_state: SemanticStateId::from_object_id(init.state),
        workspace_snapshot: backend.create_snapshot(&[]).unwrap(),
        semantic_change: None,
    };
    let rev_payload = kat::encoding::object::CanonicalPayload::RepositoryRevision(rev);
    let rev_obj = kat::encoding::object::CanonicalObject {
        payload: rev_payload,
    };
    let rev_bytes = kat::encoding::cbor::canonical_bytes(&rev_obj).unwrap();
    let rev_id = RepositoryRevisionId::from_object_id(repo.object_store().put(&rev_bytes).unwrap());

    init_workspace(&root, rev_id).unwrap();

    assert_eq!(workspace_status(&root).unwrap(), WorkspaceStatus::Clean);

    // Physical modified
    fs::write(root.join("hello.txt"), "world").unwrap();
    assert_eq!(
        workspace_status(&root).unwrap(),
        WorkspaceStatus::PhysicalModified
    );

    // Combined modified
    begin_draft_session(&repo, None).unwrap();
    assert_eq!(
        workspace_status(&root).unwrap(),
        WorkspaceStatus::CombinedModified
    );

    // Semantic modified (revert physical)
    fs::remove_file(root.join("hello.txt")).unwrap();
    assert_eq!(
        workspace_status(&root).unwrap(),
        WorkspaceStatus::SemanticModified
    );

    // Untracked behavior
    fs::write(root.join("untracked.txt"), "foo").unwrap();
    assert_eq!(
        workspace_status(&root).unwrap(),
        WorkspaceStatus::CombinedModified
    );
}
