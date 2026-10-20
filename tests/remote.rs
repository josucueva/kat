use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::sync::Mutex;

use kat::domain::change::ChangeRevision;
use kat::domain::element::{KnowledgeElementVersion, Lifecycle};
use kat::domain::identity::{
    ChangeId, ChangeRevisionId, ElementId, ObjectId, RepositoryId, RepositoryRevisionId,
    SemanticStateId, WorkspaceSnapshotId,
};
use kat::domain::operation::Operation;
use kat::domain::reference::NamedReference;
use kat::domain::revision::RepositoryRevision;
use kat::domain::state::{ElementStateEntry, SemanticState};
use kat::domain::workspace::{Workspace, WorkspaceId};
use kat::encoding::canonical_bytes;
use kat::encoding::decode::decode_canonical;
use kat::encoding::hash::object_id;
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::repository::init::init_repository;
use kat::repository::object_store::{ObjectStore, ObjectStoreError};
use kat::repository::ref_store::{AcceptedRef, FileRefStore};
use kat::repository::remote::{
    RemoteCoordinator, RemoteError, RemoteReferences, RemoteTransportError, SemanticTransport,
    WorkspaceTransport,
};
use kat::repository::workspace::state::{read_workspace_state, write_workspace_state_atomic};
use tempfile::TempDir;

const PROJECT_FILE: &str = "project.txt";

struct Fixture {
    _dir: TempDir,
    repository_id: RepositoryId,
    store: ObjectStore,
    refs: FileRefStore,
    accepted: AcceptedRef,
    workspace: Workspace,
    ontology_id: ObjectId,
    initial_state: ObjectId,
    updated_state: ObjectId,
    element_version: ObjectId,
    change: ChangeRevisionId,
    r0: RepositoryRevisionId,
    r1: RepositoryRevisionId,
    r2: RepositoryRevisionId,
    w0: WorkspaceSnapshotId,
    w1: WorkspaceSnapshotId,
    w2: WorkspaceSnapshotId,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let initialized = init_repository(dir.path()).unwrap();
        let kat_dir = dir.path().join(".kat");
        let store = ObjectStore::new(&kat_dir);
        let refs = FileRefStore::new(&kat_dir);

        let w0 = WorkspaceSnapshotId::new(vec![0x10]);
        let w1 = WorkspaceSnapshotId::new(vec![0x11]);
        let w2 = WorkspaceSnapshotId::new(vec![0x12]);
        let initial_state = initialized.state;
        let ontology_id = initialized.ontology;
        let r0 = put_revision(
            &store,
            vec![],
            SemanticStateId::from_object_id(initial_state),
            w0.clone(),
            None,
        );

        let element_id = ElementId::new();
        let element_version = put_object(
            &store,
            CanonicalPayload::KnowledgeElementVersion(KnowledgeElementVersion {
                element_id,
                type_id: "kat.core/requirement".into(),
                lifecycle: Lifecycle::Active,
                properties: vec![],
            }),
        );
        let updated_state = put_object(
            &store,
            CanonicalPayload::SemanticState(SemanticState {
                ontology_version: ontology_id,
                elements: vec![ElementStateEntry {
                    element_id,
                    version: element_version,
                }],
                relationships: vec![],
            }),
        );
        let change = ChangeRevisionId::from_object_id(put_object(
            &store,
            CanonicalPayload::ChangeRevision(ChangeRevision {
                change_id: ChangeId::new(),
                base_states: vec![initial_state],
                result_state: updated_state,
                operations: vec![Operation::CreateElement {
                    new_version: element_version,
                }],
                dependencies: vec![],
                description: Some("remote test fixture".into()),
            }),
        ));
        let r1 = put_revision(
            &store,
            vec![r0],
            SemanticStateId::from_object_id(updated_state),
            w1.clone(),
            Some(change),
        );
        let r2 = put_revision(
            &store,
            vec![r0],
            SemanticStateId::from_object_id(initial_state),
            w2.clone(),
            None,
        );

        refs.compare_and_swap_ref("local/main", None, r0).unwrap();
        let workspace = Workspace {
            id: WorkspaceId("remote-test-workspace".into()),
            base_revision: r0,
        };
        write_workspace_state_atomic(dir.path(), &workspace).unwrap();
        fs::write(dir.path().join(PROJECT_FILE), b"unchanged working tree\n").unwrap();
        let accepted = refs.read_accepted().unwrap();

        Self {
            _dir: dir,
            repository_id: initialized.repository_id,
            store,
            refs,
            accepted,
            workspace,
            ontology_id,
            initial_state,
            updated_state,
            element_version,
            change,
            r0,
            r1,
            r2,
            w0,
            w1,
            w2,
        }
    }

    fn root(&self) -> &std::path::Path {
        self._dir.path()
    }

    fn coordinator<'a>(
        &'a self,
        semantic: &'a dyn SemanticTransport,
        workspace: &'a dyn WorkspaceTransport,
    ) -> RemoteCoordinator<'a> {
        RemoteCoordinator::new(
            self.repository_id,
            "origin",
            &self.store,
            &self.refs,
            semantic,
            workspace,
        )
    }

    fn fetch_client(&self) -> FetchClient {
        let dir = tempfile::tempdir().unwrap();
        let kat_dir = dir.path().join(".kat");
        fs::create_dir_all(&kat_dir).unwrap();
        let objects = ObjectStore::new(&kat_dir);
        let refs = FileRefStore::new(&kat_dir);
        for id in [self.ontology_id, self.initial_state, self.r0.as_object_id()] {
            objects.put(&self.store.get(id).unwrap()).unwrap();
        }
        refs.init_accepted(&self.accepted).unwrap();
        refs.compare_and_swap_ref("local/main", None, self.r0)
            .unwrap();
        let workspace = Workspace {
            id: WorkspaceId("remote-fetch-client".into()),
            base_revision: self.r0,
        };
        write_workspace_state_atomic(dir.path(), &workspace).unwrap();
        fs::write(dir.path().join(PROJECT_FILE), b"unchanged working tree\n").unwrap();
        let accepted = refs.read_accepted().unwrap();

        FetchClient {
            _dir: dir,
            objects,
            refs,
            accepted,
            workspace,
            r0: self.r0,
        }
    }

    fn assert_local_authority_unchanged(&self) {
        assert_eq!(self.refs.read_accepted().unwrap(), self.accepted);
        assert_eq!(self.refs.read_ref("local/main").unwrap(), self.r0);
        assert_eq!(read_workspace_state(self.root()).unwrap(), self.workspace);
        assert_eq!(
            fs::read(self.root().join(PROJECT_FILE)).unwrap(),
            b"unchanged working tree\n"
        );
    }
}

struct FetchClient {
    _dir: TempDir,
    objects: ObjectStore,
    refs: FileRefStore,
    accepted: AcceptedRef,
    workspace: Workspace,
    r0: RepositoryRevisionId,
}

impl FetchClient {
    fn coordinator<'a>(
        &'a self,
        repository_id: RepositoryId,
        semantic: &'a dyn SemanticTransport,
        workspace: &'a dyn WorkspaceTransport,
    ) -> RemoteCoordinator<'a> {
        RemoteCoordinator::new(
            repository_id,
            "origin",
            &self.objects,
            &self.refs,
            semantic,
            workspace,
        )
    }

    fn assert_local_authority_unchanged(&self) {
        assert_eq!(self.refs.read_accepted().unwrap(), self.accepted);
        assert_eq!(self.refs.read_ref("local/main").unwrap(), self.r0);
        assert_eq!(
            read_workspace_state(self._dir.path()).unwrap(),
            self.workspace
        );
        assert_eq!(
            fs::read(self._dir.path().join(PROJECT_FILE)).unwrap(),
            b"unchanged working tree\n"
        );
    }
}

fn put_object(store: &ObjectStore, payload: CanonicalPayload) -> ObjectId {
    let bytes = canonical_bytes(&CanonicalObject { payload }).unwrap();
    store.put(&bytes).unwrap()
}

fn put_revision(
    store: &ObjectStore,
    parents: Vec<RepositoryRevisionId>,
    semantic_state: SemanticStateId,
    workspace_snapshot: WorkspaceSnapshotId,
    semantic_change: Option<ChangeRevisionId>,
) -> RepositoryRevisionId {
    RepositoryRevisionId::from_object_id(put_object(
        store,
        CanonicalPayload::RepositoryRevision(RepositoryRevision {
            parents,
            semantic_state,
            workspace_snapshot,
            semantic_change,
        }),
    ))
}

struct MemorySemanticTransport {
    repository_id: RepositoryId,
    objects: Mutex<HashMap<ObjectId, Vec<u8>>>,
    references: Mutex<BTreeMap<String, RepositoryRevisionId>>,
    writes: Mutex<usize>,
    fail_after_writes: Mutex<Option<usize>>,
}

impl MemorySemanticTransport {
    fn new(repository_id: RepositoryId) -> Self {
        Self {
            repository_id,
            objects: Mutex::new(HashMap::new()),
            references: Mutex::new(BTreeMap::new()),
            writes: Mutex::new(0),
            fail_after_writes: Mutex::new(None),
        }
    }

    fn seed_store(&self, store: &ObjectStore) {
        for id in store.find_by_prefix("").unwrap() {
            self.seed(id, store.get(id).unwrap());
        }
    }

    fn seed(&self, id: ObjectId, bytes: Vec<u8>) {
        self.objects.lock().unwrap().insert(id, bytes);
    }

    fn set_reference(&self, name: &str, target: RepositoryRevisionId) {
        self.references.lock().unwrap().insert(name.into(), target);
    }

    fn reference(&self, name: &str) -> Option<RepositoryRevisionId> {
        self.references.lock().unwrap().get(name).copied()
    }

    fn contains(&self, id: ObjectId) -> bool {
        self.objects.lock().unwrap().contains_key(&id)
    }

    fn remove(&self, id: ObjectId) {
        self.objects.lock().unwrap().remove(&id);
    }

    fn write_count(&self) -> usize {
        *self.writes.lock().unwrap()
    }

    fn fail_after_writes(&self, count: Option<usize>) {
        *self.fail_after_writes.lock().unwrap() = count;
    }

    fn check_repository(&self, expected: RepositoryId) -> Result<(), RemoteTransportError> {
        if expected == self.repository_id {
            Ok(())
        } else {
            Err(RemoteTransportError::RepositoryMismatch {
                expected,
                actual: self.repository_id,
            })
        }
    }
}

impl SemanticTransport for MemorySemanticTransport {
    fn list_references(
        &self,
        repository_id: RepositoryId,
    ) -> Result<RemoteReferences, RemoteTransportError> {
        self.check_repository(repository_id)?;
        let named = self
            .references
            .lock()
            .unwrap()
            .iter()
            .map(|(name, target)| NamedReference {
                name: name.clone(),
                target: *target,
            })
            .collect::<Vec<_>>();
        let named_targets = named
            .iter()
            .map(|reference| reference.target)
            .collect::<HashSet<_>>();
        let objects = self.objects.lock().unwrap();
        let mut revisions = HashMap::new();
        for (id, bytes) in objects.iter() {
            let Ok(object) = decode_canonical(bytes) else {
                continue;
            };
            if let CanonicalPayload::RepositoryRevision(revision) = object.payload {
                revisions.insert(RepositoryRevisionId::from_object_id(*id), revision);
            }
        }
        let mut ancestors = BTreeSet::new();
        for revision in revisions.values() {
            ancestors.extend(revision.parents.iter().copied());
        }
        let unnamed_heads = revisions
            .keys()
            .filter(|revision| !ancestors.contains(revision) && !named_targets.contains(revision))
            .copied()
            .collect();
        Ok(RemoteReferences {
            named,
            unnamed_heads,
        })
    }

    fn fetch_object(
        &self,
        repository_id: RepositoryId,
        id: ObjectId,
    ) -> Result<Option<Vec<u8>>, RemoteTransportError> {
        self.check_repository(repository_id)?;
        Ok(self.objects.lock().unwrap().get(&id).cloned())
    }

    fn store_object(
        &self,
        repository_id: RepositoryId,
        id: ObjectId,
        bytes: &[u8],
    ) -> Result<(), RemoteTransportError> {
        self.check_repository(repository_id)?;
        if object_id(bytes) != id {
            return Err(RemoteTransportError::ObjectIntegrity(id));
        }
        let mut objects = self.objects.lock().unwrap();
        if let Some(existing) = objects.get(&id) {
            if existing != bytes {
                return Err(RemoteTransportError::ObjectIntegrity(id));
            }
        } else {
            if self
                .fail_after_writes
                .lock()
                .unwrap()
                .is_some_and(|limit| *self.writes.lock().unwrap() >= limit)
            {
                return Err(RemoteTransportError::Unavailable);
            }
            objects.insert(id, bytes.to_vec());
            *self.writes.lock().unwrap() += 1;
        }
        Ok(())
    }

    fn compare_and_swap_reference(
        &self,
        repository_id: RepositoryId,
        name: &str,
        expected: Option<RepositoryRevisionId>,
        new: RepositoryRevisionId,
    ) -> Result<(), RemoteTransportError> {
        self.check_repository(repository_id)?;
        let mut references = self.references.lock().unwrap();
        if references.get(name).copied() != expected {
            return Err(RemoteTransportError::ReferenceConflict);
        }
        references.insert(name.into(), new);
        Ok(())
    }
}

#[derive(Default)]
struct MemoryWorkspaceTransport {
    local: Mutex<BTreeSet<WorkspaceSnapshotId>>,
    remote: Mutex<BTreeSet<WorkspaceSnapshotId>>,
}

impl MemoryWorkspaceTransport {
    fn add_local(&self, id: WorkspaceSnapshotId) {
        self.local.lock().unwrap().insert(id);
    }

    fn add_remote(&self, id: WorkspaceSnapshotId) {
        self.remote.lock().unwrap().insert(id);
    }

    fn remote_contains(&self, id: &WorkspaceSnapshotId) -> bool {
        self.remote.lock().unwrap().contains(id)
    }
}

impl WorkspaceTransport for MemoryWorkspaceTransport {
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
            return Err(RemoteTransportError::Other(
                "test remote snapshot is missing".into(),
            ));
        }
        self.local.lock().unwrap().insert(id.clone());
        Ok(())
    }

    fn publish_snapshot(&self, id: &WorkspaceSnapshotId) -> Result<(), RemoteTransportError> {
        if !self.local.lock().unwrap().contains(id) {
            return Err(RemoteTransportError::Other(
                "test local snapshot is missing".into(),
            ));
        }
        self.remote.lock().unwrap().insert(id.clone());
        Ok(())
    }
}

#[test]
fn fetch_transfers_recursive_closures_and_only_tracks_remote_references() {
    let fixture = Fixture::new();
    let client = fixture.fetch_client();
    let semantic = MemorySemanticTransport::new(fixture.repository_id);
    semantic.seed_store(&fixture.store);
    semantic.set_reference("main", fixture.r1);
    let workspace = MemoryWorkspaceTransport::default();
    workspace.add_local(fixture.w0.clone());
    workspace.add_remote(fixture.w0.clone());
    workspace.add_remote(fixture.w1.clone());
    workspace.add_remote(fixture.w2.clone());

    let outcome = client
        .coordinator(fixture.repository_id, &semantic, &workspace)
        .fetch()
        .unwrap();

    assert_eq!(outcome.references.len(), 1);
    assert_eq!(outcome.references[0].name, "main");
    assert_eq!(outcome.references[0].target, fixture.r1);
    assert_eq!(outcome.unnamed_heads, vec![fixture.r2]);
    assert_eq!(
        client.refs.read_ref("remotes/origin/main").unwrap(),
        fixture.r1
    );
    assert!(workspace.local_snapshot_available(&fixture.w1).unwrap());
    assert!(workspace.local_snapshot_available(&fixture.w2).unwrap());
    for id in [
        fixture.ontology_id,
        fixture.initial_state,
        fixture.updated_state,
        fixture.element_version,
        fixture.change.as_object_id(),
        fixture.r0.as_object_id(),
        fixture.r1.as_object_id(),
        fixture.r2.as_object_id(),
    ] {
        client.objects.get(id).unwrap();
    }
    fixture.assert_local_authority_unchanged();
    client.assert_local_authority_unchanged();
}

#[test]
fn partial_fetch_keeps_remote_tracking_and_local_authority_unchanged() {
    let fixture = Fixture::new();
    let client = fixture.fetch_client();
    let semantic = MemorySemanticTransport::new(fixture.repository_id);
    semantic.seed_store(&fixture.store);
    semantic.set_reference("main", fixture.r1);
    let workspace = MemoryWorkspaceTransport::default();
    workspace.add_local(fixture.w0.clone());
    workspace.add_remote(fixture.w0.clone());
    // W1 is absent remotely, so the revision closure cannot be verified.

    let result = client
        .coordinator(fixture.repository_id, &semantic, &workspace)
        .fetch();

    assert!(matches!(
        result,
        Err(RemoteError::MissingRemoteSnapshot(ref id)) if id == &fixture.w1
    ));
    assert!(matches!(
        client.refs.read_ref("remotes/origin/main"),
        Err(kat::repository::ref_store::RefStoreError::NotFound)
    ));
    // Valid immutable semantic bytes already fetched are safe to retain.
    client.objects.get(fixture.r1.as_object_id()).unwrap();
    fixture.assert_local_authority_unchanged();
    client.assert_local_authority_unchanged();
}

#[test]
fn fetch_rejects_a_missing_transitive_semantic_dependency() {
    let fixture = Fixture::new();
    let client = fixture.fetch_client();
    let semantic = MemorySemanticTransport::new(fixture.repository_id);
    semantic.seed_store(&fixture.store);
    semantic.remove(fixture.updated_state);
    semantic.set_reference("main", fixture.r1);
    let workspace = MemoryWorkspaceTransport::default();
    for id in [&fixture.w0, &fixture.w1, &fixture.w2] {
        workspace.add_local((*id).clone());
        workspace.add_remote((*id).clone());
    }

    let result = client
        .coordinator(fixture.repository_id, &semantic, &workspace)
        .fetch();

    assert!(matches!(
        result,
        Err(RemoteError::MissingRemoteObject(id)) if id == fixture.updated_state
    ));
    assert!(matches!(
        client.refs.read_ref("remotes/origin/main"),
        Err(kat::repository::ref_store::RefStoreError::NotFound)
    ));
    client.assert_local_authority_unchanged();
}

#[test]
fn fetch_rejects_a_reference_to_the_wrong_canonical_object_kind() {
    let fixture = Fixture::new();
    let client = fixture.fetch_client();
    let semantic = MemorySemanticTransport::new(fixture.repository_id);
    semantic.seed(
        fixture.initial_state,
        fixture.store.get(fixture.initial_state).unwrap(),
    );
    semantic.set_reference(
        "main",
        RepositoryRevisionId::from_object_id(fixture.initial_state),
    );
    let workspace = MemoryWorkspaceTransport::default();

    let result = client
        .coordinator(fixture.repository_id, &semantic, &workspace)
        .fetch();

    assert!(matches!(
        result,
        Err(RemoteError::UnexpectedObjectKind {
            expected: kat::encoding::object::ObjectKind::RepositoryRevision,
            actual: kat::encoding::object::ObjectKind::SemanticState,
        })
    ));
    assert!(matches!(
        client.refs.read_ref("remotes/origin/main"),
        Err(kat::repository::ref_store::RefStoreError::NotFound)
    ));
    client.assert_local_authority_unchanged();
}

#[test]
fn publish_verifies_the_full_closure_before_cas_and_duplicate_upload_is_idempotent() {
    let fixture = Fixture::new();
    let semantic = MemorySemanticTransport::new(fixture.repository_id);
    for id in [
        fixture.ontology_id,
        fixture.initial_state,
        fixture.r0.as_object_id(),
    ] {
        semantic.seed(id, fixture.store.get(id).unwrap());
    }
    semantic.set_reference("main", fixture.r0);
    semantic.set_reference("release", fixture.r0);
    let workspace = MemoryWorkspaceTransport::default();
    workspace.add_local(fixture.w0.clone());
    workspace.add_local(fixture.w1.clone());
    workspace.add_remote(fixture.w0.clone());

    let coordinator = fixture.coordinator(&semantic, &workspace);
    coordinator
        .publish("main", Some(fixture.r0), fixture.r1)
        .unwrap();
    assert_eq!(semantic.reference("main"), Some(fixture.r1));
    assert_eq!(semantic.reference("release"), Some(fixture.r0));
    assert!(workspace.remote_contains(&fixture.w0));
    assert!(workspace.remote_contains(&fixture.w1));
    for id in [
        fixture.ontology_id,
        fixture.initial_state,
        fixture.updated_state,
        fixture.element_version,
        fixture.change.as_object_id(),
        fixture.r0.as_object_id(),
        fixture.r1.as_object_id(),
    ] {
        assert!(semantic.contains(id), "missing remote object {id}");
    }
    let writes = semantic.write_count();
    coordinator
        .publish("main", Some(fixture.r1), fixture.r1)
        .unwrap();
    assert_eq!(semantic.write_count(), writes);
    fixture.assert_local_authority_unchanged();
}

#[test]
fn missing_local_snapshot_prevents_publication() {
    let fixture = Fixture::new();
    let semantic = MemorySemanticTransport::new(fixture.repository_id);
    semantic.seed_store(&fixture.store);
    semantic.set_reference("main", fixture.r0);
    let workspace = MemoryWorkspaceTransport::default();
    workspace.add_local(fixture.w0.clone());
    workspace.add_remote(fixture.w0.clone());

    let result =
        fixture
            .coordinator(&semantic, &workspace)
            .publish("main", Some(fixture.r0), fixture.r1);

    assert!(matches!(
        result,
        Err(RemoteError::MissingLocalSnapshot(ref id)) if id == &fixture.w1
    ));
    assert_eq!(semantic.reference("main"), Some(fixture.r0));
    assert_eq!(semantic.write_count(), 0);
    fixture.assert_local_authority_unchanged();
}

#[test]
fn partial_publish_keeps_the_named_reference_stable_and_retries_idempotently() {
    let fixture = Fixture::new();
    let semantic = MemorySemanticTransport::new(fixture.repository_id);
    for id in [
        fixture.ontology_id,
        fixture.initial_state,
        fixture.r0.as_object_id(),
    ] {
        semantic.seed(id, fixture.store.get(id).unwrap());
    }
    semantic.set_reference("main", fixture.r0);
    semantic.fail_after_writes(Some(1));
    let workspace = MemoryWorkspaceTransport::default();
    workspace.add_local(fixture.w0.clone());
    workspace.add_local(fixture.w1.clone());
    workspace.add_remote(fixture.w0.clone());

    let coordinator = fixture.coordinator(&semantic, &workspace);
    let result = coordinator.publish("main", Some(fixture.r0), fixture.r1);
    assert!(matches!(
        result,
        Err(RemoteError::Transport(RemoteTransportError::Unavailable))
    ));
    assert_eq!(semantic.reference("main"), Some(fixture.r0));
    assert_eq!(semantic.write_count(), 1);
    assert!(workspace.remote_contains(&fixture.w1));

    semantic.fail_after_writes(None);
    coordinator
        .publish("main", Some(fixture.r0), fixture.r1)
        .unwrap();
    assert_eq!(semantic.reference("main"), Some(fixture.r1));
    fixture.assert_local_authority_unchanged();
}

#[test]
fn failed_local_closure_and_stale_cas_never_advance_remote_authority() {
    let fixture = Fixture::new();
    let missing_parent = RepositoryRevisionId::from_object_id(ObjectId::from_bytes([0x99; 32]));
    let broken = put_revision(
        &fixture.store,
        vec![missing_parent],
        SemanticStateId::from_object_id(fixture.initial_state),
        WorkspaceSnapshotId::new(vec![0x13]),
        None,
    );
    let semantic = MemorySemanticTransport::new(fixture.repository_id);
    semantic.set_reference("main", fixture.r2);
    let workspace = MemoryWorkspaceTransport::default();
    let result =
        fixture
            .coordinator(&semantic, &workspace)
            .publish("main", Some(fixture.r0), broken);
    assert!(matches!(
        result,
        Err(RemoteError::ObjectStore(ObjectStoreError::NotFound(id))) if id == missing_parent.as_object_id()
    ));
    assert_eq!(semantic.reference("main"), Some(fixture.r2));
    assert_eq!(semantic.write_count(), 0);

    // Now use a complete candidate, but let another writer win the named-ref CAS.
    semantic.seed_store(&fixture.store);
    let workspace = MemoryWorkspaceTransport::default();
    for id in [&fixture.w0, &fixture.w1, &fixture.w2] {
        workspace.add_local((*id).clone());
        workspace.add_remote((*id).clone());
    }
    let result =
        fixture
            .coordinator(&semantic, &workspace)
            .publish("main", Some(fixture.r0), fixture.r1);
    assert!(matches!(
        result,
        Err(RemoteError::Transport(
            RemoteTransportError::ReferenceConflict
        ))
    ));
    assert_eq!(semantic.reference("main"), Some(fixture.r2));
    assert!(semantic.contains(fixture.r1.as_object_id()));
    assert!(
        semantic
            .list_references(fixture.repository_id)
            .unwrap()
            .unnamed_heads
            .contains(&fixture.r1)
    );
    fixture.assert_local_authority_unchanged();
}

#[test]
fn remote_hash_mismatch_is_rejected_before_tracking_reference_update() {
    let fixture = Fixture::new();
    let client = fixture.fetch_client();
    let semantic = MemorySemanticTransport::new(fixture.repository_id);
    semantic.seed_store(&fixture.store);
    semantic.set_reference("main", fixture.r1);
    semantic.seed(fixture.r1.as_object_id(), b"wrong bytes".to_vec());
    let workspace = MemoryWorkspaceTransport::default();
    workspace.add_local(fixture.w0.clone());
    workspace.add_remote(fixture.w1.clone());

    let result = client
        .coordinator(fixture.repository_id, &semantic, &workspace)
        .fetch();

    assert!(matches!(
        result,
        Err(RemoteError::ObjectHashMismatch { expected, .. })
            if expected == fixture.r1.as_object_id()
    ));
    assert!(matches!(
        client.refs.read_ref("remotes/origin/main"),
        Err(kat::repository::ref_store::RefStoreError::NotFound)
    ));
    fixture.assert_local_authority_unchanged();
    client.assert_local_authority_unchanged();
}

#[test]
fn repository_identity_mismatch_is_reported_as_a_transport_error() {
    let fixture = Fixture::new();
    let client = fixture.fetch_client();
    let wrong_repository = MemorySemanticTransport::new(RepositoryId::new());
    let workspace = MemoryWorkspaceTransport::default();

    let result = client
        .coordinator(fixture.repository_id, &wrong_repository, &workspace)
        .fetch();

    assert!(matches!(
        result,
        Err(RemoteError::Transport(
            RemoteTransportError::RepositoryMismatch { .. }
        ))
    ));
    assert!(matches!(
        client.refs.read_ref("remotes/origin/main"),
        Err(kat::repository::ref_store::RefStoreError::NotFound)
    ));
    client.assert_local_authority_unchanged();
}
