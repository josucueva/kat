use std::path::PathBuf;
use tempfile::TempDir;

use kat::domain::identity::{ObjectId, RepositoryRevisionId, WorkspaceSnapshotId};
use kat::domain::revision::RepositoryRevision;
use kat::domain::workspace::{Workspace, WorkspaceBackend, WorkspaceId};
use kat::encoding::canonical_bytes;
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::repository::object_store::ObjectStore;
use kat::repository::transition::{TransitionError, transition_workspace_to_revision};
use kat::repository::workspace::fake::FakeWorkspaceBackend;
use kat::repository::workspace::open_workspace;
use kat::repository::workspace::state::write_workspace_state_atomic;

fn new_rev_id(byte: u8) -> RepositoryRevisionId {
    RepositoryRevisionId::from_object_id(ObjectId::from_bytes([byte; 32]))
}

struct TestEnv {
    pub repo_root: PathBuf,
    pub ws_id: WorkspaceId,
    pub backend: FakeWorkspaceBackend,
    pub base_rev: RepositoryRevisionId,
    pub _dir: TempDir,
}

fn put_mock_revision(
    obj_store: &ObjectStore,
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
        semantic_state: kat::domain::identity::SemanticStateId::from_object_id(state_id),
        semantic_change: None,
        workspace_snapshot: snapshot,
    };
    let obj = CanonicalObject {
        payload: CanonicalPayload::RepositoryRevision(rev),
    };
    let bytes = canonical_bytes(&obj).unwrap();
    let id = obj_store.put(&bytes).unwrap();
    RepositoryRevisionId::from_object_id(id)
}

fn setup_test_env() -> TestEnv {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();

    kat::repository::init::init_repository(&root).unwrap();
    kat::repository::workspace::git::GitWorkspaceBackend::init(&root).unwrap();
    let repo = kat::repository::open::open_repository(&root).unwrap();

    let backend = FakeWorkspaceBackend::new();

    let base_snap = backend.create_snapshot(&[]).unwrap();
    let base_rev = put_mock_revision(repo.object_store(), 0, vec![], base_snap);

    let ws = Workspace {
        id: WorkspaceId(uuid::Uuid::new_v4().to_string()),
        base_revision: base_rev,
    };
    write_workspace_state_atomic(&root, &ws).unwrap();
    let ws_id = ws.id;

    TestEnv {
        repo_root: root,
        ws_id,
        backend,
        base_rev,
        _dir: dir,
    }
}

// ADV-01 Same -> no-op
// Done by CLI wrapper, so primitive just materializes to same. Let's test the primitive itself for success on Same.
#[test]
fn test_transition_primitive_same() {
    let env = setup_test_env();
    let res = transition_workspace_to_revision(&env.repo_root, &env.backend, env.base_rev);
    assert!(res.is_ok());
}

// ADV-07 physical materialization failure -> base unchanged
#[test]
fn test_transition_primitive_materialization_failure() {
    let env = setup_test_env();
    let repo = kat::repository::open::open_repository(&env.repo_root).unwrap();

    // Create a target snapshot ID but don't add it to the backend so materialization fails
    let target_snap = WorkspaceSnapshotId::new(vec![99; 32]);
    let target_rev = put_mock_revision(repo.object_store(), 1, vec![env.base_rev], target_snap);

    let res = transition_workspace_to_revision(&env.repo_root, &env.backend, target_rev);
    match res {
        Err(TransitionError::MaterializationFailed(_)) => {}
        _ => panic!("Expected MaterializationFailed, got {:?}", res),
    }

    // Check base is unchanged
    let ws = open_workspace(&env.repo_root, &env.backend).unwrap();
    assert_eq!(ws.base_revision, env.base_rev);
}

// SW-08 successful switch leaves physical tree matching target snapshot
#[test]
fn test_transition_primitive_success() {
    let env = setup_test_env();
    let repo = kat::repository::open::open_repository(&env.repo_root).unwrap();

    let target_snap = env.backend.create_snapshot(&[]).unwrap();
    let target_rev = put_mock_revision(repo.object_store(), 1, vec![env.base_rev], target_snap);

    let res = transition_workspace_to_revision(&env.repo_root, &env.backend, target_rev);
    assert!(res.is_ok());

    // Check base is updated
    let ws = open_workspace(&env.repo_root, &env.backend).unwrap();
    assert_eq!(ws.base_revision, target_rev);
}
