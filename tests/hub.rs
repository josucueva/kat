use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use git2::{Repository, Signature};
use kat::domain::identity::{
    ObjectId, RepositoryId, RepositoryRevisionId, SemanticStateId, WorkspaceSnapshotId,
};
use kat::domain::reference::NamedReference;
use kat::domain::revision::RepositoryRevision;
use kat::encoding::canonical_bytes;
use kat::encoding::hash::{hash_workspace_snapshot, object_id};
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::repository::hub::TrustedHubSemanticTransport;
use kat::repository::init::init_repository;
use kat::repository::object_store::ObjectStore;
use kat::repository::ref_store::FileRefStore;
use kat::repository::remote::{
    RemoteCoordinator, RemoteReferences, RemoteTransportError, SemanticTransport,
    WorkspaceTransport,
};
use kat::repository::workspace::git_transport::{
    ControlledGitBackend, GitWorkspaceBinding, WorkspaceBackendId,
};
use tempfile::TempDir;

struct Fixture {
    _source_dir: TempDir,
    _backend_dir: TempDir,
    _hub_dir: TempDir,
    repository_id: RepositoryId,
    snapshot: WorkspaceSnapshotId,
    ontology: ObjectId,
    state: ObjectId,
    revision: RepositoryRevisionId,
    source_objects: ObjectStore,
    hub: TrustedHubSemanticTransport,
}

impl Fixture {
    fn new() -> Self {
        let source_dir = tempfile::tempdir().unwrap();
        let initialized = init_repository(source_dir.path()).unwrap();
        let source_objects = ObjectStore::new(source_dir.path().join(".kat"));

        let backend_dir = tempfile::tempdir().unwrap();
        let backend_path = backend_dir.path().join("workspace.git");
        let binding = GitWorkspaceBinding {
            repository_id: initialized.repository_id,
            workspace_backend_id: WorkspaceBackendId::new(),
        };
        ControlledGitBackend::provision(&backend_path, binding).unwrap();
        let snapshot = hash_workspace_snapshot(&[]);
        add_empty_snapshot(&backend_path, &snapshot);

        let hub_dir = tempfile::tempdir().unwrap();
        let hub = TrustedHubSemanticTransport::provision(
            &hub_dir.path().join("hub"),
            binding,
            &backend_path,
        )
        .unwrap();
        let revision = put_revision(
            &source_objects,
            vec![],
            SemanticStateId::from_object_id(initialized.state),
            snapshot.clone(),
        );

        Self {
            _source_dir: source_dir,
            _backend_dir: backend_dir,
            _hub_dir: hub_dir,
            repository_id: initialized.repository_id,
            snapshot,
            ontology: initialized.ontology,
            state: initialized.state,
            revision,
            source_objects,
            hub,
        }
    }

    fn backend_path(&self) -> PathBuf {
        self._backend_dir.path().join("workspace.git")
    }

    fn hub_root(&self) -> PathBuf {
        self._hub_dir.path().join("hub")
    }

    fn seed_semantics(&self) {
        for id in [self.ontology, self.state, self.revision.as_object_id()] {
            self.hub
                .store_object(
                    self.repository_id,
                    id,
                    &self.source_objects.get(id).unwrap(),
                )
                .unwrap();
        }
    }
}

#[test]
fn interrupted_semantic_upload_stays_unpublished_then_publish_and_fetch_succeed() {
    let fixture = Fixture::new();
    let ontology = fixture.source_objects.get(fixture.ontology).unwrap();
    fixture
        .hub
        .store_object(fixture.repository_id, fixture.ontology, &ontology)
        .unwrap();
    assert_eq!(
        fixture.hub.list_references(fixture.repository_id).unwrap(),
        RemoteReferences::default()
    );

    let source_refs = FileRefStore::new(fixture._source_dir.path().join(".kat"));
    let source_workspace = TestWorkspace::new([fixture.snapshot.clone()], []);
    let publisher = RemoteCoordinator::new(
        fixture.repository_id,
        "origin",
        &fixture.source_objects,
        &source_refs,
        &fixture.hub,
        &source_workspace,
    );
    publisher.publish("main", None, fixture.revision).unwrap();
    assert_eq!(
        fixture
            .hub
            .list_references(fixture.repository_id)
            .unwrap()
            .named,
        vec![NamedReference {
            name: "main".into(),
            target: fixture.revision,
        }]
    );
    let reopened_hub =
        TrustedHubSemanticTransport::open(&fixture.hub_root(), fixture.repository_id).unwrap();

    let fetch_dir = tempfile::tempdir().unwrap();
    let fetch_kat = fetch_dir.path().join(".kat");
    fs::create_dir_all(&fetch_kat).unwrap();
    let fetch_objects = ObjectStore::new(&fetch_kat);
    let fetch_refs = FileRefStore::new(&fetch_kat);
    let fetch_workspace = source_workspace.for_fetch();
    let fetcher = RemoteCoordinator::new(
        fixture.repository_id,
        "origin",
        &fetch_objects,
        &fetch_refs,
        &reopened_hub,
        &fetch_workspace,
    );
    let result = fetcher.fetch().unwrap();
    assert_eq!(result.updated.len(), 1);
    assert_eq!(
        fetch_refs.read_ref("remotes/origin/main").unwrap(),
        fixture.revision
    );
    assert!(fetch_refs.read_ref("local/main").is_err());
    for id in [
        fixture.ontology,
        fixture.state,
        fixture.revision.as_object_id(),
    ] {
        assert_eq!(
            fetch_objects.get(id).unwrap(),
            fixture.source_objects.get(id).unwrap()
        );
    }
}

#[test]
fn missing_semantic_or_physical_dependencies_never_advance_a_named_reference() {
    let fixture = Fixture::new();
    let missing_state = ObjectId::from_bytes([0xa5; 32]);
    let missing_semantic = put_revision(
        &fixture.source_objects,
        vec![],
        SemanticStateId::from_object_id(missing_state),
        fixture.snapshot.clone(),
    );
    let bytes = fixture
        .source_objects
        .get(missing_semantic.as_object_id())
        .unwrap();
    fixture
        .hub
        .store_object(
            fixture.repository_id,
            missing_semantic.as_object_id(),
            &bytes,
        )
        .unwrap();
    assert!(
        fixture
            .hub
            .compare_and_swap_reference(fixture.repository_id, "main", None, missing_semantic)
            .is_err()
    );
    assert!(
        fixture
            .hub
            .list_references(fixture.repository_id)
            .unwrap()
            .unnamed_heads
            .is_empty()
    );
    let missing_snapshot = WorkspaceSnapshotId::new(vec![0x33; 32]);
    let missing_physical = put_revision(
        &fixture.source_objects,
        vec![],
        SemanticStateId::from_object_id(fixture.state),
        missing_snapshot,
    );
    for id in [
        fixture.ontology,
        fixture.state,
        missing_physical.as_object_id(),
    ] {
        let bytes = fixture.source_objects.get(id).unwrap();
        fixture
            .hub
            .store_object(fixture.repository_id, id, &bytes)
            .unwrap();
    }
    assert!(
        fixture
            .hub
            .compare_and_swap_reference(fixture.repository_id, "main", None, missing_physical)
            .is_err()
    );
    assert!(
        fixture
            .hub
            .list_references(fixture.repository_id)
            .unwrap()
            .named
            .is_empty()
    );

    let hub_kat = fixture
        .hub_root()
        .join("repositories")
        .join(fixture.repository_id.to_string())
        .join(".kat");
    FileRefStore::new(hub_kat)
        .compare_and_swap_ref("local/incomplete", None, missing_semantic)
        .unwrap();
    assert!(fixture.hub.list_references(fixture.repository_id).is_err());
}

#[test]
fn repository_binding_is_fixed_and_policy_drift_blocks_publication() {
    let fixture = Fixture::new();
    let other_repository = RepositoryId::new();
    assert!(matches!(
        fixture.hub.list_references(other_repository),
        Err(RemoteTransportError::RepositoryMismatch { .. })
    ));

    let other_backend_dir = tempfile::tempdir().unwrap();
    let other_backend_path = other_backend_dir.path().join("other.git");
    let other_binding = GitWorkspaceBinding {
        repository_id: fixture.repository_id,
        workspace_backend_id: WorkspaceBackendId::new(),
    };
    ControlledGitBackend::provision(&other_backend_path, other_binding).unwrap();
    assert!(
        TrustedHubSemanticTransport::provision(
            &fixture.hub_root(),
            other_binding,
            &other_backend_path,
        )
        .is_err()
    );

    fixture.seed_semantics();
    fs::remove_file(fixture.backend_path().join("hooks/pre-receive")).unwrap();
    assert!(
        fixture
            .hub
            .compare_and_swap_reference(fixture.repository_id, "main", None, fixture.revision)
            .is_err()
    );
    assert!(fixture.hub.list_references(fixture.repository_id).is_err());
}

#[test]
fn concurrent_reference_cas_has_one_winner_and_discovery_verifies_closure() {
    let fixture = Fixture::new();
    fixture.seed_semantics();
    let hub = Arc::new(fixture.hub);
    let mut workers = Vec::new();
    for _ in 0..2 {
        let hub = Arc::clone(&hub);
        let repository_id = fixture.repository_id;
        let revision = fixture.revision;
        workers.push(thread::spawn(move || {
            hub.compare_and_swap_reference(repository_id, "main", None, revision)
        }));
    }
    let outcomes = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| matches!(result, Err(RemoteTransportError::ReferenceConflict)))
            .count(),
        1
    );
    assert_eq!(
        hub.list_references(fixture.repository_id).unwrap().named[0].target,
        fixture.revision
    );
}

#[test]
fn invalid_ref_names_and_bad_object_bytes_are_rejected_without_mutation() {
    let fixture = Fixture::new();
    fixture.seed_semantics();
    assert!(
        fixture
            .hub
            .compare_and_swap_reference(fixture.repository_id, "../main", None, fixture.revision)
            .is_err()
    );

    let bytes = b"not canonical";
    assert!(
        fixture
            .hub
            .store_object(fixture.repository_id, object_id(bytes), bytes)
            .is_err()
    );
    let ontology = fixture.source_objects.get(fixture.ontology).unwrap();
    assert_eq!(
        fixture.hub.store_object(
            fixture.repository_id,
            object_id(b"different bytes"),
            &ontology,
        ),
        Err(RemoteTransportError::ObjectIntegrity(object_id(
            b"different bytes"
        )))
    );
    assert!(
        !fixture
            .hub_root()
            .join("repositories")
            .join(fixture.repository_id.to_string())
            .join(".kat/refs/local/main")
            .exists()
    );
    let malformed_ref = fixture
        .hub_root()
        .join("repositories")
        .join(fixture.repository_id.to_string())
        .join(".kat/refs/local/broken");
    fs::create_dir_all(malformed_ref.parent().unwrap()).unwrap();
    fs::write(&malformed_ref, b"not a revision id\n").unwrap();
    assert!(fixture.hub.list_references(fixture.repository_id).is_err());
}

#[derive(Clone)]
struct TestWorkspace {
    local: Arc<Mutex<BTreeSet<WorkspaceSnapshotId>>>,
    remote: Arc<Mutex<BTreeSet<WorkspaceSnapshotId>>>,
}

impl TestWorkspace {
    fn new(
        local: impl IntoIterator<Item = WorkspaceSnapshotId>,
        remote: impl IntoIterator<Item = WorkspaceSnapshotId>,
    ) -> Self {
        Self {
            local: Arc::new(Mutex::new(local.into_iter().collect())),
            remote: Arc::new(Mutex::new(remote.into_iter().collect())),
        }
    }

    fn for_fetch(&self) -> Self {
        Self {
            local: Arc::new(Mutex::new(BTreeSet::new())),
            remote: Arc::clone(&self.remote),
        }
    }
}

impl WorkspaceTransport for TestWorkspace {
    fn local_snapshot_available(
        &self,
        id: &WorkspaceSnapshotId,
    ) -> Result<bool, RemoteTransportError> {
        Ok(self.local.lock().unwrap().contains(id))
    }

    fn remote_snapshot_available(
        &self,
        id: &WorkspaceSnapshotId,
    ) -> Result<bool, RemoteTransportError> {
        Ok(self.remote.lock().unwrap().contains(id))
    }

    fn fetch_snapshot(&self, id: &WorkspaceSnapshotId) -> Result<(), RemoteTransportError> {
        if !self.remote.lock().unwrap().contains(id) {
            return Err(RemoteTransportError::SnapshotIntegrity(id.clone()));
        }
        self.local.lock().unwrap().insert(id.clone());
        Ok(())
    }

    fn publish_snapshot(&self, id: &WorkspaceSnapshotId) -> Result<(), RemoteTransportError> {
        if !self.local.lock().unwrap().contains(id) {
            return Err(RemoteTransportError::SnapshotIntegrity(id.clone()));
        }
        self.remote.lock().unwrap().insert(id.clone());
        Ok(())
    }
}

fn put_revision(
    objects: &ObjectStore,
    parents: Vec<RepositoryRevisionId>,
    semantic_state: SemanticStateId,
    snapshot: WorkspaceSnapshotId,
) -> RepositoryRevisionId {
    let bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::RepositoryRevision(RepositoryRevision {
            parents,
            semantic_state,
            workspace_snapshot: snapshot,
            semantic_change: None,
        }),
    })
    .unwrap();
    RepositoryRevisionId::from_object_id(objects.put(&bytes).unwrap())
}

fn add_empty_snapshot(backend_path: &Path, snapshot: &WorkspaceSnapshotId) {
    let repo = Repository::open_bare(backend_path).unwrap();
    let tree_id = repo.treebuilder(None).unwrap().write().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("KAT Hub test", "kat@example.invalid").unwrap();
    let commit = repo
        .commit(None, &signature, &signature, "empty snapshot", &tree, &[])
        .unwrap();
    let reference = format!("refs/kat/lineage/{}/{commit}", snapshot.to_hex());
    repo.reference(&reference, commit, false, "KAT snapshot retention test")
        .unwrap();
}
