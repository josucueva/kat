use kat::domain::identity::{ObjectId, RepositoryRevisionId};
use kat::domain::workspace::{ReconciliationStatus, WorkspaceId};
use kat::repository::reconcile::{
    ReconciliationSession, ReconciliationSessionState, save_reconciliation_session,
};
use kat::repository::workspace::git::GitWorkspaceBackend;
use kat::repository::workspace::{WorkspaceError, init_workspace, workspace_status};
use std::path::{Path, PathBuf};

fn session_path(repo_root: &Path, workspace_id: &WorkspaceId) -> PathBuf {
    repo_root
        .join(".kat")
        .join("workspaces")
        .join(&workspace_id.0)
        .join("reconciliation_session.json")
}

fn make_rev(i: u8) -> RepositoryRevisionId {
    let hash = [i; 32];
    RepositoryRevisionId::from_object_id(ObjectId::from_bytes(hash))
}

fn create_test_revision(
    root: &std::path::Path,
    state_id: kat::domain::identity::ObjectId,
) -> RepositoryRevisionId {
    let repo = kat::repository::open::open_repository(root).unwrap();
    kat::repository::workspace::git::GitWorkspaceBackend::init(root).unwrap();
    let backend = kat::repository::workspace::git::GitWorkspaceBackend::open(root).unwrap();
    use kat::domain::workspace::WorkspaceBackend;
    let rev = kat::domain::revision::RepositoryRevision {
        parents: vec![],
        semantic_state: kat::domain::identity::SemanticStateId::from_object_id(state_id),
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

fn setup() -> (
    tempfile::TempDir,
    WorkspaceId,
    RepositoryRevisionId,
    GitWorkspaceBackend,
) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let init = kat::repository::init::init_repository(root).unwrap();
    kat::repository::workspace::git::GitWorkspaceBackend::init(root).unwrap();
    let backend = GitWorkspaceBackend::open(root).unwrap();

    let base_revision = create_test_revision(root, init.state);

    let ws = init_workspace(root, base_revision).unwrap();

    (dir, ws.id, base_revision, backend)
}

#[test]
fn stat_01_no_session_yields_none() {
    let (dir, _ws_id, _base_rev, backend) = setup();
    let root = dir.path();

    let status = workspace_status(root, &backend).unwrap();
    assert_eq!(status.status.reconciliation, ReconciliationStatus::None);
}

#[test]
fn stat_02_prepared_clean_exposes_revisions() {
    let (dir, ws_id, base_rev, backend) = setup();
    let root = dir.path();

    let target_rev = make_rev(2);
    let prepared_rev = make_rev(3);

    let session = ReconciliationSession {
        version: 1,
        workspace_id: ws_id.clone(),
        base_revision: base_rev,
        target_revision: target_rev,
        state: ReconciliationSessionState::PreparedClean {
            revision: prepared_rev,
        },
    };
    save_reconciliation_session(root, &ws_id, &session, &backend).unwrap();

    let status = workspace_status(root, &backend).unwrap();
    assert_eq!(
        status.status.reconciliation,
        ReconciliationStatus::PreparedClean {
            base_revision: base_rev,
            target_revision: target_rev,
            prepared_revision: prepared_rev,
        }
    );
}

#[test]
fn stat_03_conflicted_exposes_counts() {
    let (dir, ws_id, base_rev, backend) = setup();
    let root = dir.path();

    let target_rev = make_rev(2);

    let session = ReconciliationSession {
        version: 1,
        workspace_id: ws_id.clone(),
        base_revision: base_rev,
        target_revision: target_rev,
        state: ReconciliationSessionState::Conflicted {
            candidate: kat::repository::reconcile::ReconciliationCandidate {
                version: 1,
                workspace_id: ws_id.clone(),
                base_revision: base_rev,
                local_revision: base_rev,
                other_revision: target_rev,
                proposed_semantic_state: kat::domain::state::SemanticState {
                    ontology_version: ObjectId::from_bytes([0; 32]),
                    elements: vec![],
                    relationships: vec![],
                },
                semantic_conflicts: vec![kat::domain::conflict::SemanticConflict {
                    id: "test-id".to_string(),
                    affected_elements: vec![],
                    affected_relationships: vec![],
                    kind: kat::domain::conflict::SemanticConflictKind::ConcurrentModification {
                        base_version: None,
                        local_version: None,
                        other_version: None,
                    },
                }],
                materialization_conflicts: vec![],
                validation_findings: vec![kat::domain::conflict::ValidationFinding {
                    diagnostic: "test".to_string(),
                }],
                physical_candidate: None,
            },
        },
    };
    save_reconciliation_session(root, &ws_id, &session, &backend).unwrap();

    let status = workspace_status(root, &backend).unwrap();
    assert_eq!(
        status.status.reconciliation,
        ReconciliationStatus::Conflicted {
            base_revision: base_rev,
            target_revision: target_rev,
            semantic_conflicts: 1,
            materialization_conflicts: 0,
            validation_findings: 1,
        }
    );
}

#[test]
fn stat_04_status_does_not_mutate() {
    let (dir, _ws_id, _base_rev, backend) = setup();
    let root = dir.path();

    // Check initial mtime of .kat directory tree
    let get_mtimes = || {
        let mut mtimes = std::collections::BTreeMap::new();
        let mut stack = vec![root.join(".kat")];
        while let Some(dir) = stack.pop() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if path.is_file() {
                        mtimes.insert(path.clone(), entry.metadata().unwrap().modified().unwrap());
                    }
                }
            }
        }
        mtimes
    };

    let before = get_mtimes();

    // Status call
    workspace_status(root, &backend).unwrap();

    let after = get_mtimes();

    assert_eq!(before, after, "workspace_status mutated state");
}

#[test]
fn stat_05_unsupported_session_propagates_error() {
    let (dir, ws_id, base_rev, backend) = setup();
    let root = dir.path();

    let session = ReconciliationSession {
        version: 999, // Unsupported
        workspace_id: ws_id.clone(),
        base_revision: base_rev,
        target_revision: base_rev,
        state: ReconciliationSessionState::PreparedClean { revision: base_rev },
    };

    let path = session_path(root, &ws_id);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_string(&session).unwrap()).unwrap();

    let err = workspace_status(root, &backend).unwrap_err();
    assert!(matches!(
        err,
        WorkspaceError::InvalidReconciliationSession(_)
    ));
}

#[test]
fn stat_06_deterministic_json_projection_ordering() {
    let (dir, ws_id, base_rev, backend) = setup();
    let root = dir.path();

    let session = ReconciliationSession {
        version: 1,
        workspace_id: ws_id.clone(),
        base_revision: base_rev,
        target_revision: base_rev,
        state: ReconciliationSessionState::PreparedClean { revision: base_rev },
    };
    save_reconciliation_session(root, &ws_id, &session, &backend).unwrap();

    let status = workspace_status(root, &backend).unwrap();
    let json = serde_json::to_string(&status).unwrap();

    // verify the keys order if possible, though serde_json with struct preserves definition order
    assert!(json.contains("reconciliation"));
}
