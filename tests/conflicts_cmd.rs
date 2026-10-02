use kat::domain::conflict::{
    MaterializationConflict, MaterializationConflictKind, SemanticConflict, SemanticConflictKind,
    ValidationFinding,
};
use kat::domain::identity::{
    ElementId, ObjectId, PhysicalCandidateId, RepositoryRevisionId, SemanticStateId,
    WorkspaceSnapshotId,
};
use kat::domain::state::SemanticState;
use kat::domain::workspace::PhysicalReconciliationCandidate;
use kat::domain::workspace::{WorkspaceBackend, WorkspaceId};
use kat::repository::reconcile::{
    ReconciliationCandidate, ReconciliationSession, ReconciliationSessionState,
};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

use kat::domain::revision::RepositoryRevision;
use kat::encoding::cbor::canonical_bytes;
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::repository::object_store::ObjectStore;

fn new_rev_id(byte: u8) -> RepositoryRevisionId {
    RepositoryRevisionId::from_object_id(ObjectId::from_bytes([byte; 32]))
}

fn empty_semantic_state() -> SemanticState {
    SemanticState {
        ontology_version: ObjectId::from_bytes([0; 32]),
        elements: vec![],
        relationships: vec![],
    }
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

    let backend = kat::repository::workspace::git::GitWorkspaceBackend::open(root).unwrap();
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

fn setup_test_workspace() -> (TempDir, PathBuf, WorkspaceId) {
    let temp = TempDir::new().unwrap();
    let repo_root = temp.path().join("repo");
    std::fs::create_dir_all(&repo_root).unwrap();

    kat::repository::init::init_repository(&repo_root).unwrap();
    kat::repository::workspace::git::GitWorkspaceBackend::init(&repo_root).unwrap();

    let repo = kat::repository::open::open_repository(&repo_root).unwrap();
    let base_rev = put_mock_revision(repo.object_store(), &repo_root, 0, vec![]);

    kat::repository::workspace::init_workspace(&repo_root, base_rev).unwrap();
    let ws = kat::repository::workspace::open_workspace(
        &repo_root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&repo_root).unwrap(),
    )
    .unwrap();
    (temp, repo_root, ws.id)
}

/// CONFLICT-01: No ReconciliationSession -> explicit NoActiveReconciliation error
#[test]
fn conflicts_cmd_01_no_session() {
    let (_temp, repo_root, _ws_id) = setup_test_workspace();

    let output = Command::new(env!("CARGO_BIN_EXE_kat"))
        .current_dir(&repo_root)
        .arg("conflicts")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("No active reconciliation session"));
}

/// CONFLICT-02: PreparedClean -> valid empty conflict projection
#[test]
fn conflicts_cmd_02_prepared_clean() {
    let (_temp, repo_root, _ws_id) = setup_test_workspace();
    let ws = kat::repository::workspace::open_workspace(
        &repo_root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&repo_root).unwrap(),
    )
    .unwrap();

    let target = new_rev_id(0);
    let session = ReconciliationSession {
        version: 1,
        workspace_id: ws.id.clone(),
        base_revision: ws.base_revision,
        target_revision: target,
        state: ReconciliationSessionState::PreparedClean { revision: target },
    };

    let path = kat::repository::reconcile::session_path(&repo_root, &ws.id);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_string(&session).unwrap()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kat"))
        .current_dir(&repo_root)
        .arg("conflicts")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Status: prepared (clean)"));
}

/// CONFLICT-03: semantic-only conflict -> semantic conflict visible, physical empty
#[test]
fn conflicts_cmd_03_semantic_only() {
    let (_temp, repo_root, _ws_id) = setup_test_workspace();
    let ws = kat::repository::workspace::open_workspace(
        &repo_root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&repo_root).unwrap(),
    )
    .unwrap();

    let target = new_rev_id(0);
    let session = ReconciliationSession {
        version: 1,
        workspace_id: ws.id.clone(),
        base_revision: ws.base_revision,
        target_revision: target,
        state: ReconciliationSessionState::Conflicted {
            candidate: ReconciliationCandidate {
                version: 1,
                workspace_id: ws.id.clone(),
                base_revision: ws.base_revision,
                local_revision: ws.base_revision,
                other_revision: target,
                proposed_semantic_state: empty_semantic_state(),
                physical_candidate: None,
                semantic_conflicts: vec![SemanticConflict {
                    id: "test-id".to_string(),
                    affected_elements: vec![ElementId::new()],
                    affected_relationships: vec![],
                    kind: SemanticConflictKind::AmbiguousAccountability,
                }],
                materialization_conflicts: vec![],
                validation_findings: vec![],
            },
        },
    };

    let path = kat::repository::reconcile::session_path(&repo_root, &ws.id);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_string(&session).unwrap()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kat"))
        .current_dir(&repo_root)
        .arg("conflicts")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Semantic Conflicts (1):"));
    assert!(stdout.contains("AmbiguousAccountability"));
    assert!(stdout.contains("Physical Conflicts: None"));
}

/// CONFLICT-04: physical-only conflict -> physical conflict visible, semantic empty
#[test]
fn conflicts_cmd_04_physical_only() {
    let (_temp, repo_root, _ws_id) = setup_test_workspace();
    let ws = kat::repository::workspace::open_workspace(
        &repo_root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&repo_root).unwrap(),
    )
    .unwrap();

    let target = new_rev_id(0);
    let session = ReconciliationSession {
        version: 1,
        workspace_id: ws.id.clone(),
        base_revision: ws.base_revision,
        target_revision: target,
        state: ReconciliationSessionState::Conflicted {
            candidate: ReconciliationCandidate {
                version: 1,
                workspace_id: ws.id.clone(),
                base_revision: ws.base_revision,
                local_revision: ws.base_revision,
                other_revision: target,
                proposed_semantic_state: empty_semantic_state(),
                physical_candidate: Some(PhysicalReconciliationCandidate {
                    base: WorkspaceSnapshotId::new(vec![0; 20]),
                    local: WorkspaceSnapshotId::new(vec![0; 20]),
                    other: WorkspaceSnapshotId::new(vec![0; 20]),
                    provisional: PhysicalCandidateId::new(),
                    conflicts: vec![MaterializationConflict {
                        id: "test-id".to_string(),
                        kind: MaterializationConflictKind::Content,
                        paths: vec![PathBuf::from("src/main.rs")],
                    }],
                }),
                semantic_conflicts: vec![],
                materialization_conflicts: vec![MaterializationConflict {
                    id: "test-id".to_string(),
                    kind: MaterializationConflictKind::Content,
                    paths: vec![PathBuf::from("src/main.rs")],
                }],
                validation_findings: vec![],
            },
        },
    };

    let path = kat::repository::reconcile::session_path(&repo_root, &ws.id);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_string(&session).unwrap()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kat"))
        .current_dir(&repo_root)
        .arg("conflicts")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Semantic Conflicts: None"));
    assert!(stdout.contains("Physical Conflicts (1):"));
    assert!(stdout.contains("Content"));
    assert!(stdout.contains("src/main.rs"));
}

/// CONFLICT-05 & 06: mixed conflict and validation findings
#[test]
fn conflicts_cmd_05_06_mixed() {
    let (_temp, repo_root, _ws_id) = setup_test_workspace();
    let ws = kat::repository::workspace::open_workspace(
        &repo_root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&repo_root).unwrap(),
    )
    .unwrap();

    let target = new_rev_id(0);
    let session = ReconciliationSession {
        version: 1,
        workspace_id: ws.id.clone(),
        base_revision: ws.base_revision,
        target_revision: target,
        state: ReconciliationSessionState::Conflicted {
            candidate: ReconciliationCandidate {
                version: 1,
                workspace_id: ws.id.clone(),
                base_revision: ws.base_revision,
                local_revision: ws.base_revision,
                other_revision: target,
                proposed_semantic_state: empty_semantic_state(),
                physical_candidate: None,
                semantic_conflicts: vec![SemanticConflict {
                    id: "test-id".to_string(),
                    affected_elements: vec![ElementId::new()],
                    affected_relationships: vec![],
                    kind: SemanticConflictKind::AmbiguousAccountability,
                }],
                materialization_conflicts: vec![MaterializationConflict {
                    id: "test-id".to_string(),
                    kind: MaterializationConflictKind::Content,
                    paths: vec![PathBuf::from("src/main.rs")],
                }],
                validation_findings: vec![ValidationFinding {
                    diagnostic: "Orphan element detected".to_string(),
                }],
            },
        },
    };

    let path = kat::repository::reconcile::session_path(&repo_root, &ws.id);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_string(&session).unwrap()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kat"))
        .current_dir(&repo_root)
        .arg("conflicts")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Semantic Conflicts (1):"));
    assert!(stdout.contains("Physical Conflicts (1):"));
    assert!(stdout.contains("Validation Findings (1):"));
    assert!(stdout.contains("Orphan element detected"));
}

/// CONFLICT-08: command is read-only
#[test]
fn conflicts_cmd_08_readonly() {
    let (_temp, repo_root, _ws_id) = setup_test_workspace();
    let ws = kat::repository::workspace::open_workspace(
        &repo_root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&repo_root).unwrap(),
    )
    .unwrap();

    let target = new_rev_id(0);
    let session = ReconciliationSession {
        version: 1,
        workspace_id: ws.id.clone(),
        base_revision: ws.base_revision,
        target_revision: target,
        state: ReconciliationSessionState::PreparedClean { revision: target },
    };

    let path = kat::repository::reconcile::session_path(&repo_root, &ws.id);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let json_before = serde_json::to_string(&session).unwrap();
    fs::write(&path, &json_before).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kat"))
        .current_dir(&repo_root)
        .arg("conflicts")
        .output()
        .unwrap();

    assert!(output.status.success());

    let json_after = fs::read_to_string(&path).unwrap();
    assert_eq!(json_before, json_after);

    let ws_after = kat::repository::workspace::open_workspace(
        &repo_root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&repo_root).unwrap(),
    )
    .unwrap();
    assert_eq!(ws.base_revision, ws_after.base_revision);
}
