use std::path::PathBuf;
use tempfile::TempDir;

use kat::domain::conflict::{MaterializationConflict, MaterializationConflictKind};
use kat::domain::identity::{ObjectId, RepositoryRevisionId, SemanticStateId, WorkspaceSnapshotId};
use kat::domain::revision::RepositoryRevision;
use kat::domain::state::SemanticState;
use kat::domain::workspace::{WorkspaceBackend, WorkspaceId};
use kat::encoding::canonical_bytes;
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::repository::object_store::ObjectStore;
use kat::repository::reconcile::{
    ReconciliationCandidate, ReconciliationSession, ReconciliationSessionState,
    abort_reconciliation_session, load_reconciliation_session, save_reconciliation_session,
};

fn put_mock_revision(
    obj_store: &ObjectStore,
    _root: &std::path::Path,
    _byte: u8,
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

    // Put semantic state
    let state = kat::domain::state::SemanticState {
        ontology_version: ont_id,
        elements: vec![],
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
        semantic_change: None,
        workspace_snapshot: snapshot,
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
    backend: kat::repository::workspace::git::GitWorkspaceBackend,
}

fn setup_test_env() -> TestEnv {
    let temp = TempDir::new().unwrap();
    let repo_root = temp.path().join("repo");
    std::fs::create_dir_all(&repo_root).unwrap();

    kat::repository::init::init_repository(&repo_root).unwrap();
    kat::repository::workspace::git::GitWorkspaceBackend::init(&repo_root).unwrap();
    let backend = kat::repository::workspace::git::GitWorkspaceBackend::open(&repo_root).unwrap();

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

fn create_base_candidate(
    env: &TestEnv,
    repo: &kat::repository::open::Repository,
) -> ReconciliationCandidate {
    let local = RepositoryRevisionId::from_object_id(ObjectId::from_bytes([1; 32]));
    let other = RepositoryRevisionId::from_object_id(ObjectId::from_bytes([2; 32]));

    let base_rev_obj = repo
        .object_store()
        .get(env.base_rev.as_object_id())
        .unwrap();
    let base_rev = match kat::encoding::decode_canonical(&base_rev_obj)
        .unwrap()
        .payload
    {
        CanonicalPayload::RepositoryRevision(r) => r,
        _ => panic!("Not a revision"),
    };
    let base_state_obj = repo
        .object_store()
        .get(base_rev.semantic_state.as_object_id())
        .unwrap();
    let base_state = match kat::encoding::decode_canonical(&base_state_obj)
        .unwrap()
        .payload
    {
        CanonicalPayload::SemanticState(s) => s,
        _ => panic!("Not a semantic state"),
    };

    let semantic_state = SemanticState {
        ontology_version: base_state.ontology_version,
        elements: vec![],
        relationships: vec![],
    };
    ReconciliationCandidate {
        version: 1,
        workspace_id: env.ws_id.clone(),
        base_revision: env.base_rev,
        local_revision: local,
        other_revision: other,
        proposed_semantic_state: semantic_state,
        physical_candidate: None,
        semantic_conflicts: vec![],
        materialization_conflicts: vec![],
        validation_findings: vec![],
    }
}

#[test]
fn test_abort_no_session() {
    let env = setup_test_env();
    let res = abort_reconciliation_session(&env.repo_root, &env.backend).unwrap();
    assert!(!res);
}

#[test]
fn test_abort_conflicted_materialized() {
    let env = setup_test_env();

    let repo = kat::repository::open::open_repository(&env.repo_root).unwrap();
    let mut candidate = create_base_candidate(&env, &repo);
    let path = std::path::PathBuf::from("conflict.txt");
    candidate
        .materialization_conflicts
        .push(MaterializationConflict {
            id: "test-id".to_string(),
            kind: MaterializationConflictKind::Content,
            paths: vec![path.clone()],
        });

    let session = ReconciliationSession {
        version: 1,
        workspace_id: env.ws_id.clone(),
        base_revision: env.base_rev,
        target_revision: candidate.other_revision,
        state: ReconciliationSessionState::ConflictedMaterialized { candidate },
    };
    save_reconciliation_session(&env.repo_root, &env.ws_id, &session, &env.backend).unwrap();

    let res = abort_reconciliation_session(&env.repo_root, &env.backend).unwrap();
    assert!(res);

    let loaded = load_reconciliation_session(&env.repo_root, &env.ws_id, env.base_rev).unwrap();
    assert!(loaded.is_none());
}
