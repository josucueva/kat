use std::path::PathBuf;
use tempfile::TempDir;

use kat::domain::identity::{
    ObjectId, PhysicalCandidateId, RepositoryRevisionId, SemanticStateId, WorkspaceSnapshotId,
};
use kat::domain::revision::RepositoryRevision;
use kat::domain::state::SemanticState;
use kat::domain::workspace::PhysicalReconciliationCandidate;
use kat::domain::workspace::{WorkspaceBackend, WorkspaceId};
use kat::encoding::cbor::canonical_bytes;
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::repository::materialize::{MaterializeError, materialize_conflicts};
use kat::repository::object_store::ObjectStore;
use kat::repository::reconcile::{
    ReconciliationCandidate, ReconciliationSession, ReconciliationSessionState,
    load_reconciliation_session, save_reconciliation_session,
};
use kat::repository::workspace::fake::FakeWorkspaceBackend;

fn new_rev_id(byte: u8) -> RepositoryRevisionId {
    RepositoryRevisionId::from_object_id(ObjectId::from_bytes([byte; 32]))
}

fn put_mock_revision(
    obj_store: &ObjectStore,
    _root: &std::path::Path,
    byte: u8,
    mut parents: Vec<RepositoryRevisionId>,
    snapshot: WorkspaceSnapshotId,
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

struct TestEnv {
    #[allow(dead_code)]
    temp: TempDir,
    repo_root: PathBuf,
    ws_id: WorkspaceId,
    base_rev: RepositoryRevisionId,
    backend: FakeWorkspaceBackend,
}

fn setup_test_env() -> TestEnv {
    let temp = TempDir::new().unwrap();
    let repo_root = temp.path().join("repo");
    std::fs::create_dir_all(&repo_root).unwrap();

    kat::repository::init::init_repository(&repo_root).unwrap();
    kat::repository::workspace::git::GitWorkspaceBackend::init(&repo_root).unwrap();
    let backend = FakeWorkspaceBackend::with_root(&repo_root);

    let repo = kat::repository::open::open_repository(&repo_root).unwrap();

    let git_backend =
        kat::repository::workspace::git::GitWorkspaceBackend::open(&repo_root).unwrap();
    let base_snap = git_backend.create_snapshot(&[]).unwrap();
    let base_rev = put_mock_revision(repo.object_store(), &repo_root, 0, vec![], base_snap);

    kat::repository::workspace::init_workspace(&repo_root, base_rev).unwrap();
    let ws = kat::repository::workspace::open_workspace(
        &repo_root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&repo_root).unwrap(),
    )
    .unwrap();
    let base_rev = ws.base_revision;

    TestEnv {
        temp,
        repo_root,
        ws_id: ws.id,
        base_rev,
        backend,
    }
}

struct FailBackend {
    inner: FakeWorkspaceBackend,
    fail_candidate: bool,
    fail_snapshot: bool,
}

impl WorkspaceBackend for FailBackend {
    fn inspect_working_state(
        &self,
        base: &WorkspaceSnapshotId,
    ) -> Result<kat::domain::workspace::WorkingState, kat::domain::workspace::WorkspaceBackendError>
    {
        self.inner.inspect_working_state(base)
    }
    fn create_snapshot(
        &self,
        tracked_paths: &[std::path::PathBuf],
    ) -> Result<WorkspaceSnapshotId, kat::domain::workspace::WorkspaceBackendError> {
        self.inner.create_snapshot(tracked_paths)
    }
    fn materialize_snapshot(
        &self,
        _id: &WorkspaceSnapshotId,
    ) -> Result<(), kat::domain::workspace::WorkspaceBackendError> {
        if self.fail_snapshot {
            return Err(kat::domain::workspace::WorkspaceBackendError::Io(
                std::io::Error::other("forced failure in snapshot"),
            ));
        }
        Ok(())
    }
    fn compare_snapshots(
        &self,
        base: &WorkspaceSnapshotId,
        target: &WorkspaceSnapshotId,
    ) -> Result<
        kat::domain::workspace::PhysicalChanges,
        kat::domain::workspace::WorkspaceBackendError,
    > {
        self.inner.compare_snapshots(base, target)
    }
    fn resolve_materialization(
        &self,
        path: &std::path::Path,
        snapshot_id: &WorkspaceSnapshotId,
    ) -> Result<
        kat::domain::workspace::MaterializationResolution,
        kat::domain::workspace::WorkspaceBackendError,
    > {
        self.inner.resolve_materialization(path, snapshot_id)
    }
    fn resolve_working_materialization(
        &self,
        path: &std::path::Path,
    ) -> Result<
        kat::domain::workspace::MaterializationResolution,
        kat::domain::workspace::WorkspaceBackendError,
    > {
        self.inner.resolve_working_materialization(path)
    }
    fn verify_snapshot_integrity(
        &self,
        id: &WorkspaceSnapshotId,
    ) -> Result<bool, kat::domain::workspace::WorkspaceBackendError> {
        self.inner.verify_snapshot_integrity(id)
    }
    fn reconcile_physical(
        &self,
        base: &WorkspaceSnapshotId,
        local: &WorkspaceSnapshotId,
        other: &WorkspaceSnapshotId,
    ) -> Result<
        kat::domain::workspace::PhysicalReconciliationResult,
        kat::domain::workspace::WorkspaceBackendError,
    > {
        self.inner.reconcile_physical(base, local, other)
    }
    fn persist_physical_candidate(
        &self,
        workspace_id: &WorkspaceId,
        candidate: &PhysicalReconciliationCandidate,
    ) -> Result<(), kat::domain::workspace::WorkspaceBackendError> {
        self.inner
            .persist_physical_candidate(workspace_id, candidate)
    }
    fn materialize_candidate(
        &self,
        workspace_id: &WorkspaceId,
        candidate: &PhysicalReconciliationCandidate,
    ) -> Result<(), kat::domain::workspace::WorkspaceBackendError> {
        if self.fail_candidate {
            return Err(kat::domain::workspace::WorkspaceBackendError::Io(
                std::io::Error::other("forced failure in candidate"),
            ));
        }
        self.inner.materialize_candidate(workspace_id, candidate)
    }
    fn clear_physical_candidate(
        &self,
        _workspace_id: &WorkspaceId,
    ) -> Result<(), kat::domain::workspace::WorkspaceBackendError> {
        Ok(())
    }
}

// MAT-T01: successful materialization
#[test]
fn mat_t01_successful_materialization() {
    let env = setup_test_env();

    // Create session
    let phys_candidate = PhysicalReconciliationCandidate {
        provisional: PhysicalCandidateId::new(),
        base: WorkspaceSnapshotId::new(vec![2; 32]),
        local: WorkspaceSnapshotId::new(vec![3; 32]),
        other: WorkspaceSnapshotId::new(vec![4; 32]),
        conflicts: vec![],
    };
    let candidate = ReconciliationCandidate {
        version: 1,
        workspace_id: env.ws_id.clone(),
        base_revision: env.base_rev,
        local_revision: new_rev_id(1),
        other_revision: new_rev_id(2),
        proposed_semantic_state: SemanticState {
            ontology_version: ObjectId::from_bytes([0; 32]),
            elements: vec![],
            relationships: vec![],
        },
        physical_candidate: Some(phys_candidate.clone()),
        semantic_conflicts: vec![],
        materialization_conflicts: vec![],
        validation_findings: vec![],
    };

    let session = ReconciliationSession {
        version: 1,
        target_revision: new_rev_id(2),
        workspace_id: env.ws_id.clone(),
        base_revision: env.base_rev,
        state: ReconciliationSessionState::Conflicted { candidate },
    };

    save_reconciliation_session(&env.repo_root, &env.ws_id, &session, &env.backend).unwrap();

    let res = materialize_conflicts(&env.repo_root, &env.backend).unwrap();
    assert!(matches!(
        res.state,
        ReconciliationSessionState::ConflictedMaterialized { .. }
    ));

    let saved = load_reconciliation_session(&env.repo_root, &env.ws_id, env.base_rev)
        .unwrap()
        .unwrap();
    assert!(matches!(
        saved.state,
        ReconciliationSessionState::ConflictedMaterialized { .. }
    ));

    // Check workspace base is unchanged
    let ws = kat::repository::workspace::open_workspace(
        &env.repo_root,
        &kat::repository::workspace::git::GitWorkspaceBackend::open(&env.repo_root).unwrap(),
    )
    .unwrap();
    assert_eq!(ws.base_revision, env.base_rev);
}

// MAT-T02/T03: materialization fails, rollback succeeds
#[test]
fn mat_t02_t03_materialization_fails_rollback_succeeds() {
    let env = setup_test_env();

    let phys_candidate = PhysicalReconciliationCandidate {
        provisional: PhysicalCandidateId::new(),
        base: WorkspaceSnapshotId::new(vec![2; 32]),
        local: WorkspaceSnapshotId::new(vec![3; 32]),
        other: WorkspaceSnapshotId::new(vec![4; 32]),
        conflicts: vec![],
    };
    let candidate = ReconciliationCandidate {
        version: 1,
        workspace_id: env.ws_id.clone(),
        base_revision: env.base_rev,
        local_revision: new_rev_id(1),
        other_revision: new_rev_id(2),
        proposed_semantic_state: SemanticState {
            ontology_version: ObjectId::from_bytes([0; 32]),
            elements: vec![],
            relationships: vec![],
        },
        physical_candidate: Some(phys_candidate.clone()),
        semantic_conflicts: vec![],
        materialization_conflicts: vec![],
        validation_findings: vec![],
    };

    let session = ReconciliationSession {
        version: 1,
        target_revision: new_rev_id(2),
        workspace_id: env.ws_id.clone(),
        base_revision: env.base_rev,
        state: ReconciliationSessionState::Conflicted { candidate },
    };

    save_reconciliation_session(&env.repo_root, &env.ws_id, &session, &env.backend).unwrap();

    let fail_backend = FailBackend {
        inner: env.backend,
        fail_candidate: true, // fails on materialize_candidate
        fail_snapshot: false, // succeeds on materialize_snapshot (rollback)
    };

    let err = materialize_conflicts(&env.repo_root, &fail_backend).unwrap_err();
    match err {
        MaterializeError::MaterializationFailed { restored, .. } => {
            assert!(restored);
        }
        _ => panic!("Expected MaterializationFailed, got {:?}", err),
    }

    let saved = load_reconciliation_session(&env.repo_root, &env.ws_id, env.base_rev)
        .unwrap()
        .unwrap();
    assert!(matches!(
        saved.state,
        ReconciliationSessionState::Conflicted { .. }
    ));
}

// MAT-T04: rollback itself fails
#[test]
fn mat_t04_rollback_fails() {
    let env = setup_test_env();

    let phys_candidate = PhysicalReconciliationCandidate {
        provisional: PhysicalCandidateId::new(),
        base: WorkspaceSnapshotId::new(vec![2; 32]),
        local: WorkspaceSnapshotId::new(vec![3; 32]),
        other: WorkspaceSnapshotId::new(vec![4; 32]),
        conflicts: vec![],
    };
    let candidate = ReconciliationCandidate {
        version: 1,
        workspace_id: env.ws_id.clone(),
        base_revision: env.base_rev,
        local_revision: new_rev_id(1),
        other_revision: new_rev_id(2),
        proposed_semantic_state: SemanticState {
            ontology_version: ObjectId::from_bytes([0; 32]),
            elements: vec![],
            relationships: vec![],
        },
        physical_candidate: Some(phys_candidate.clone()),
        semantic_conflicts: vec![],
        materialization_conflicts: vec![],
        validation_findings: vec![],
    };

    let session = ReconciliationSession {
        version: 1,
        target_revision: new_rev_id(2),
        workspace_id: env.ws_id.clone(),
        base_revision: env.base_rev,
        state: ReconciliationSessionState::Conflicted { candidate },
    };

    save_reconciliation_session(&env.repo_root, &env.ws_id, &session, &env.backend).unwrap();

    let fail_backend = FailBackend {
        inner: env.backend,
        fail_candidate: true,
        fail_snapshot: true,
    };

    let err = materialize_conflicts(&env.repo_root, &fail_backend).unwrap_err();
    match err {
        MaterializeError::MaterializationRecoveryFailed { .. } => {}
        _ => panic!("Expected MaterializationRecoveryFailed, got {:?}", err),
    }
}

// MAT-T05: stale session/base mismatch
#[test]
fn mat_t05_stale_session() {
    let env = setup_test_env();

    let session = ReconciliationSession {
        version: 1,
        target_revision: new_rev_id(2),
        workspace_id: env.ws_id.clone(),
        base_revision: new_rev_id(99), // Mismatch! (expected: env.base_rev)
        state: ReconciliationSessionState::PreparedClean {
            revision: new_rev_id(0),
        },
    };
    save_reconciliation_session(&env.repo_root, &env.ws_id, &session, &env.backend).unwrap();

    let err = materialize_conflicts(&env.repo_root, &env.backend).unwrap_err();
    assert!(matches!(
        err,
        MaterializeError::SessionLoad(kat::repository::reconcile::SessionLoadError::StaleBase)
    ));
}
