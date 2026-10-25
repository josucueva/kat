use std::cell::Cell;
use std::fs;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

use tiny_http::Server;

use super::{HttpSemanticTransport, HubHttpService};
use crate::domain::change::ChangeRevision;
use crate::domain::element::{KnowledgeElementVersion, Lifecycle};
use crate::domain::identity::{
    ChangeId, ChangeRevisionId, ElementId, ObjectId, RepositoryId, RepositoryRevisionId,
    SemanticStateId, WorkspaceSnapshotId,
};
use crate::domain::operation::Operation;
use crate::domain::property::PropertyValue;
use crate::domain::revision::RepositoryRevision;
use crate::domain::state::{ElementStateEntry, SemanticState};
use crate::domain::workspace::WorkspaceBackend;
use crate::encoding::canonical_bytes;
use crate::encoding::object::{CanonicalObject, CanonicalPayload};
use crate::repository::auth::{
    AuthorizationAudience, AuthorizationGrant, AuthorizationPolicy, AuthorizationScope,
    AuthorizationVerifier,
};
use crate::repository::git_http::GitHttpGateway;
use crate::repository::hub::TrustedHubSemanticTransport;
use crate::repository::init::init_repository;
use crate::repository::object_store::ObjectStore;
use crate::repository::ref_store::FileRefStore;
use crate::repository::remote::{
    RemoteCoordinator, RemoteError, RemoteTransportError, SemanticTransport, WorkspaceTransport,
};
use crate::repository::workspace::git::GitWorkspaceBackend;
use crate::repository::workspace::git_transport::{
    ControlledGitBackend, GitWorkspaceBinding, GitWorkspaceTransport, WorkspaceBackendId,
};
use crate::repository::workspace::state::read_workspace_state;

const READ_TOKEN: &str = "rrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrr";
const PUBLISH_TOKEN: &str = "ppppppppppppppppppppppppppppppppppppppppppp";
const WRONG_REPOSITORY_TOKEN: &str = "wrong-repository-auth-token-000000000000000000";

struct RunningServer {
    url: String,
    server: Arc<Server>,
    thread: Option<JoinHandle<()>>,
}

impl RunningServer {
    fn start(
        make_url: impl FnOnce(SocketAddr) -> String,
        serve: impl FnOnce(&Server) + Send + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = Arc::new(Server::from_listener(listener, None).unwrap());
        let serving_server = Arc::clone(&server);
        let thread = thread::spawn(move || serve(&serving_server));
        Self {
            url: make_url(address),
            server,
            thread: Some(thread),
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

struct Fixture {
    git_server: RunningServer,
    hub_server: RunningServer,
    _source: tempfile::TempDir,
    _workspace_remote: tempfile::TempDir,
    _hub: tempfile::TempDir,
    source_root: PathBuf,
    hub_root: PathBuf,
    policy_path: PathBuf,
    repository_id: RepositoryId,
    wrong_repository_id: RepositoryId,
    binding: GitWorkspaceBinding,
    base_revision: RepositoryRevisionId,
    candidate_revision: RepositoryRevisionId,
    base_snapshot: WorkspaceSnapshotId,
    candidate_snapshot: WorkspaceSnapshotId,
    semantic_objects: Vec<ObjectId>,
    authorization: Arc<AuthorizationVerifier>,
}

impl Fixture {
    fn new() -> Self {
        let source = tempfile::tempdir().unwrap();
        let source_root = source.path().to_path_buf();
        let initialized = init_repository(&source_root).unwrap();
        let source_backend = GitWorkspaceBackend::init(&source_root).unwrap();
        let base_snapshot = source_backend.create_snapshot(&[]).unwrap();
        let source_objects = ObjectStore::new(source_root.join(".kat"));
        let base_revision = RepositoryRevisionId::from_object_id(put_payload(
            &source_objects,
            CanonicalPayload::RepositoryRevision(RepositoryRevision {
                parents: vec![],
                semantic_state: SemanticStateId::from_object_id(initialized.state),
                workspace_snapshot: base_snapshot.clone(),
                semantic_change: None,
            }),
        ));
        let source_refs = FileRefStore::new(source_root.join(".kat"));
        source_refs
            .compare_and_swap_ref("local/main", None, base_revision)
            .unwrap();
        crate::repository::workspace::init_workspace(&source_root, base_revision).unwrap();

        fs::write(
            source_root.join("published.txt"),
            "semantic and physical update\n",
        )
        .unwrap();
        let candidate_snapshot = source_backend
            .create_snapshot(&[PathBuf::from("published.txt")])
            .unwrap();
        let element_id = ElementId::new();
        let version_id = put_payload(
            &source_objects,
            CanonicalPayload::KnowledgeElementVersion(KnowledgeElementVersion {
                element_id,
                type_id: "kat.core/requirement".into(),
                lifecycle: Lifecycle::Active,
                properties: vec![(
                    "title".into(),
                    PropertyValue::Text("Authenticated remote requirement".into()),
                )],
            }),
        );
        let candidate_state = put_payload(
            &source_objects,
            CanonicalPayload::SemanticState(SemanticState {
                ontology_version: initialized.ontology,
                elements: vec![ElementStateEntry {
                    element_id,
                    version: version_id,
                }],
                relationships: vec![],
            }),
        );
        let change = put_payload(
            &source_objects,
            CanonicalPayload::ChangeRevision(ChangeRevision {
                change_id: ChangeId::new(),
                base_states: vec![initialized.state],
                result_state: candidate_state,
                operations: vec![Operation::CreateElement {
                    new_version: version_id,
                }],
                dependencies: vec![],
                description: Some("Add an authenticated remote requirement".into()),
            }),
        );
        let candidate_revision = RepositoryRevisionId::from_object_id(put_payload(
            &source_objects,
            CanonicalPayload::RepositoryRevision(RepositoryRevision {
                parents: vec![base_revision],
                semantic_state: SemanticStateId::from_object_id(candidate_state),
                workspace_snapshot: candidate_snapshot.clone(),
                semantic_change: Some(ChangeRevisionId::from_object_id(change)),
            }),
        ));

        let binding = GitWorkspaceBinding {
            repository_id: initialized.repository_id,
            workspace_backend_id: WorkspaceBackendId::new(),
        };
        let workspace_remote = tempfile::tempdir().unwrap();
        let backend_path = workspace_remote.path().join("workspace.git");
        ControlledGitBackend::provision(&backend_path, binding).unwrap();
        let hub_dir = tempfile::tempdir().unwrap();
        let hub_root = hub_dir.path().join("hub");
        let hub =
            TrustedHubSemanticTransport::provision(&hub_root, binding, &backend_path).unwrap();
        let wrong_repository_id = RepositoryId::new();
        let policy_path = hub_dir.path().join("authorization.json");
        let grants = vec![
            grant(
                READ_TOKEN,
                initialized.repository_id,
                AuthorizationScope::Read,
            ),
            grant(
                PUBLISH_TOKEN,
                initialized.repository_id,
                AuthorizationScope::Publish,
            ),
            grant(
                WRONG_REPOSITORY_TOKEN,
                wrong_repository_id,
                AuthorizationScope::Publish,
            ),
        ];
        save_policy(&policy_path, &policy(grants));
        let authorization = Arc::new(AuthorizationVerifier::open(&policy_path).unwrap());

        let serving_hub = Arc::clone(&authorization);
        let hub_server = RunningServer::start(
            |address| format!("http://{address}"),
            move |server| HubHttpService::new(hub, serving_hub).serve(server),
        );
        let serving_git = Arc::clone(&authorization);
        let git_backend_path = backend_path.clone();
        let git_server = RunningServer::start(
            |address| {
                format!(
                    "http://{address}/v1/repositories/{}/git",
                    initialized.repository_id
                )
            },
            move |server| {
                GitHttpGateway::open(&git_backend_path, binding, serving_git)
                    .unwrap()
                    .serve(server)
            },
        );

        Self {
            git_server,
            hub_server,
            _source: source,
            _workspace_remote: workspace_remote,
            _hub: hub_dir,
            source_root,
            hub_root,
            policy_path,
            repository_id: initialized.repository_id,
            wrong_repository_id,
            binding,
            base_revision,
            candidate_revision,
            base_snapshot,
            candidate_snapshot,
            semantic_objects: vec![
                initialized.ontology,
                initialized.state,
                version_id,
                candidate_state,
                change,
                base_revision.as_object_id(),
                candidate_revision.as_object_id(),
            ],
            authorization,
        }
    }

    fn hub_url(&self) -> &str {
        &self.hub_server.url
    }

    fn git_url(&self) -> &str {
        &self.git_server.url
    }

    fn reload_policy(&self, grants: Vec<AuthorizationGrant>) {
        save_policy(&self.policy_path, &policy(grants));
        self.authorization.reload(&self.policy_path).unwrap();
    }

    fn full_grants(&self) -> Vec<AuthorizationGrant> {
        vec![
            grant(READ_TOKEN, self.repository_id, AuthorizationScope::Read),
            grant(
                PUBLISH_TOKEN,
                self.repository_id,
                AuthorizationScope::Publish,
            ),
            grant(
                WRONG_REPOSITORY_TOKEN,
                self.wrong_repository_id,
                AuthorizationScope::Publish,
            ),
        ]
    }

    fn hub_core(&self) -> TrustedHubSemanticTransport {
        TrustedHubSemanticTransport::open(&self.hub_root, self.repository_id).unwrap()
    }
}

fn grant(
    token: &str,
    repository_id: RepositoryId,
    scope: AuthorizationScope,
) -> AuthorizationGrant {
    AuthorizationGrant::from_token(
        token,
        repository_id,
        scope,
        [
            AuthorizationAudience::SemanticApi,
            AuthorizationAudience::WorkspaceGit,
        ],
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600,
    )
    .unwrap()
}

fn policy(grants: Vec<AuthorizationGrant>) -> AuthorizationPolicy {
    AuthorizationPolicy::new(grants).unwrap()
}

fn save_policy(path: &Path, policy: &AuthorizationPolicy) {
    fs::write(path, serde_json::to_vec(policy).unwrap()).unwrap();
}

fn put_payload(objects: &ObjectStore, payload: CanonicalPayload) -> ObjectId {
    let bytes = canonical_bytes(&CanonicalObject { payload }).unwrap();
    objects.put(&bytes).unwrap()
}

fn publisher_workspace(fixture: &Fixture) -> GitWorkspaceTransport {
    GitWorkspaceTransport::configure(&fixture.source_root, fixture.git_url(), fixture.binding)
        .unwrap()
        .with_loopback_runtime_token(PUBLISH_TOKEN)
        .unwrap()
}

struct RevokePublishAfterOneStore<'a> {
    inner: &'a dyn SemanticTransport,
    authorization: &'a AuthorizationVerifier,
    policy_path: &'a Path,
    read_only_grants: Vec<AuthorizationGrant>,
    stores_succeeded: Cell<usize>,
    first_object: Cell<Option<ObjectId>>,
}

impl SemanticTransport for RevokePublishAfterOneStore<'_> {
    fn list_references(
        &self,
        repository_id: RepositoryId,
    ) -> Result<crate::repository::remote::RemoteReferences, RemoteTransportError> {
        self.inner.list_references(repository_id)
    }

    fn fetch_object(
        &self,
        repository_id: RepositoryId,
        id: ObjectId,
    ) -> Result<Option<Vec<u8>>, RemoteTransportError> {
        self.inner.fetch_object(repository_id, id)
    }

    fn store_object(
        &self,
        repository_id: RepositoryId,
        id: ObjectId,
        bytes: &[u8],
    ) -> Result<(), RemoteTransportError> {
        self.inner.store_object(repository_id, id, bytes)?;
        if self.stores_succeeded.get() == 0 {
            self.first_object.set(Some(id));
            save_policy(self.policy_path, &policy(self.read_only_grants.clone()));
            self.authorization.reload(self.policy_path).unwrap();
        }
        self.stores_succeeded
            .set(self.stores_succeeded.get().saturating_add(1));
        Ok(())
    }

    fn compare_and_swap_reference(
        &self,
        repository_id: RepositoryId,
        name: &str,
        expected: Option<RepositoryRevisionId>,
        new: RepositoryRevisionId,
    ) -> Result<(), RemoteTransportError> {
        self.inner
            .compare_and_swap_reference(repository_id, name, expected, new)
    }
}

#[test]
fn auth_17_authenticated_coordinator_publish_reopen_and_fetch_preserves_local_authority() {
    let fixture = Fixture::new();
    let source_objects = ObjectStore::new(fixture.source_root.join(".kat"));
    let source_refs = FileRefStore::new(fixture.source_root.join(".kat"));
    let source_accepted = source_refs.read_accepted().unwrap();
    let source_workspace_before = read_workspace_state(&fixture.source_root).unwrap();
    let publisher_semantic =
        HttpSemanticTransport::loopback(fixture.hub_url(), PUBLISH_TOKEN).unwrap();
    let publisher_workspace = publisher_workspace(&fixture);

    let coordinator = RemoteCoordinator::new(
        fixture.repository_id,
        "origin",
        &source_objects,
        &source_refs,
        &publisher_semantic,
        &publisher_workspace,
    );
    coordinator
        .publish("main", None, fixture.candidate_revision)
        .unwrap();
    assert!(matches!(
        coordinator.publish("main", None, fixture.candidate_revision),
        Err(RemoteError::Transport(
            RemoteTransportError::ReferenceConflict
        ))
    ));
    assert_eq!(
        fixture
            .hub_core()
            .list_references(fixture.repository_id)
            .unwrap()
            .named[0]
            .target,
        fixture.candidate_revision
    );
    assert_eq!(
        source_refs.read_ref("local/main").unwrap(),
        fixture.base_revision
    );
    assert_eq!(source_refs.read_accepted().unwrap(), source_accepted);
    assert_eq!(
        read_workspace_state(&fixture.source_root).unwrap(),
        source_workspace_before
    );
    drop(coordinator);
    drop(publisher_semantic);
    drop(publisher_workspace);

    // Credentials are runtime-only; reopening the saved workspace binding requires them again.
    let read_semantic = HttpSemanticTransport::loopback(fixture.hub_url(), READ_TOKEN).unwrap();
    let target_dir = tempfile::tempdir().unwrap();
    let target_root = target_dir.path();
    let target_init = init_repository(target_root).unwrap();
    fs::copy(
        fixture.source_root.join(".kat/repository.toml"),
        target_root.join(".kat/repository.toml"),
    )
    .unwrap();
    let target_backend = GitWorkspaceBackend::init(target_root).unwrap();
    fs::write(target_root.join("keep.txt"), "receiver working tree\n").unwrap();
    let target_snapshot = target_backend
        .create_snapshot(&[PathBuf::from("keep.txt")])
        .unwrap();
    let target_objects = ObjectStore::new(target_root.join(".kat"));
    let target_revision = RepositoryRevisionId::from_object_id(put_payload(
        &target_objects,
        CanonicalPayload::RepositoryRevision(RepositoryRevision {
            parents: vec![],
            semantic_state: SemanticStateId::from_object_id(target_init.state),
            workspace_snapshot: target_snapshot,
            semantic_change: None,
        }),
    ));
    let target_refs = FileRefStore::new(target_root.join(".kat"));
    target_refs
        .compare_and_swap_ref("local/main", None, target_revision)
        .unwrap();
    crate::repository::workspace::init_workspace(target_root, target_revision).unwrap();
    let accepted_before = target_refs.read_accepted().unwrap();
    let workspace_before = read_workspace_state(target_root).unwrap();
    let active_lineage_before = GitWorkspaceBackend::read_active_lineage_for_test(target_root);
    let working_tree_before = fs::read(target_root.join("keep.txt")).unwrap();
    let target_workspace =
        GitWorkspaceTransport::configure(target_root, fixture.git_url(), fixture.binding).unwrap();
    drop(target_workspace);
    let target_workspace = GitWorkspaceTransport::open(target_root)
        .unwrap()
        .with_loopback_runtime_token(READ_TOKEN)
        .unwrap();
    let fetcher = RemoteCoordinator::new(
        fixture.repository_id,
        "origin",
        &target_objects,
        &target_refs,
        &read_semantic,
        &target_workspace,
    );
    let fetched = fetcher.fetch().unwrap();
    assert_eq!(fetched.updated.len(), 1);
    assert_eq!(fetched.updated[0].target, fixture.candidate_revision);
    assert_eq!(
        target_refs.read_ref("remotes/origin/main").unwrap(),
        fixture.candidate_revision
    );
    assert_eq!(target_refs.read_ref("local/main").unwrap(), target_revision);
    assert_eq!(target_refs.read_accepted().unwrap(), accepted_before);
    assert_eq!(read_workspace_state(target_root).unwrap(), workspace_before);
    assert_eq!(
        GitWorkspaceBackend::read_active_lineage_for_test(target_root),
        active_lineage_before
    );
    assert_eq!(
        fs::read(target_root.join("keep.txt")).unwrap(),
        working_tree_before
    );
    assert!(
        target_workspace
            .local_snapshot_available(&fixture.base_snapshot)
            .unwrap()
    );
    assert!(
        target_workspace
            .local_snapshot_available(&fixture.candidate_snapshot)
            .unwrap()
    );
    for id in fixture.semantic_objects {
        assert_eq!(
            target_objects.get(id).unwrap(),
            source_objects.get(id).unwrap()
        );
    }
}

#[test]
fn auth_18_partial_upload_read_only_wrong_repository_revocation_and_retry() {
    let fixture = Fixture::new();
    let source_objects = ObjectStore::new(fixture.source_root.join(".kat"));
    let source_refs = FileRefStore::new(fixture.source_root.join(".kat"));
    let accepted_before = source_refs.read_accepted().unwrap();
    let workspace_before = read_workspace_state(&fixture.source_root).unwrap();
    let active_lineage_before =
        GitWorkspaceBackend::read_active_lineage_for_test(&fixture.source_root);
    let working_tree_before = fs::read(fixture.source_root.join("published.txt")).unwrap();
    let publisher_semantic =
        HttpSemanticTransport::loopback(fixture.hub_url(), PUBLISH_TOKEN).unwrap();
    let publisher_workspace = publisher_workspace(&fixture);
    let read_only_grants = vec![
        grant(READ_TOKEN, fixture.repository_id, AuthorizationScope::Read),
        grant(
            PUBLISH_TOKEN,
            fixture.repository_id,
            AuthorizationScope::Read,
        ),
        grant(
            WRONG_REPOSITORY_TOKEN,
            fixture.wrong_repository_id,
            AuthorizationScope::Publish,
        ),
    ];
    let failing_semantic = RevokePublishAfterOneStore {
        inner: &publisher_semantic,
        authorization: &fixture.authorization,
        policy_path: &fixture.policy_path,
        read_only_grants,
        stores_succeeded: Cell::new(0),
        first_object: Cell::new(None),
    };
    let interrupted = RemoteCoordinator::new(
        fixture.repository_id,
        "origin",
        &source_objects,
        &source_refs,
        &failing_semantic,
        &publisher_workspace,
    )
    .publish("main", None, fixture.candidate_revision);
    assert!(matches!(
        interrupted,
        Err(RemoteError::Transport(RemoteTransportError::Other(message)))
            if message == "remote permission denied"
    ));
    assert_eq!(failing_semantic.stores_succeeded.get(), 1);
    let uploaded = failing_semantic.first_object.get().unwrap();
    let read_semantic = HttpSemanticTransport::loopback(fixture.hub_url(), READ_TOKEN).unwrap();
    assert!(
        read_semantic
            .fetch_object(fixture.repository_id, uploaded)
            .unwrap()
            .is_some()
    );
    assert!(
        publisher_workspace
            .remote_snapshot_available(&fixture.candidate_snapshot)
            .unwrap()
    );
    assert!(
        fixture
            .hub_core()
            .list_references(fixture.repository_id)
            .unwrap()
            .named
            .is_empty()
    );
    assert_eq!(
        source_refs.read_ref("local/main").unwrap(),
        fixture.base_revision
    );
    assert_eq!(source_refs.read_accepted().unwrap(), accepted_before);
    assert_eq!(
        read_workspace_state(&fixture.source_root).unwrap(),
        workspace_before
    );
    assert_eq!(
        GitWorkspaceBackend::read_active_lineage_for_test(&fixture.source_root),
        active_lineage_before
    );
    assert_eq!(
        fs::read(fixture.source_root.join("published.txt")).unwrap(),
        working_tree_before
    );

    let wrong_semantic =
        HttpSemanticTransport::loopback(fixture.hub_url(), WRONG_REPOSITORY_TOKEN).unwrap();
    assert!(
        wrong_semantic
            .list_references(fixture.repository_id)
            .is_err()
    );
    let wrong_workspace = GitWorkspaceTransport::open(&fixture.source_root)
        .unwrap()
        .with_loopback_runtime_token(WRONG_REPOSITORY_TOKEN)
        .unwrap();
    assert!(
        wrong_workspace
            .remote_snapshot_available(&fixture.candidate_snapshot)
            .is_err()
    );

    let read_workspace = GitWorkspaceTransport::open(&fixture.source_root)
        .unwrap()
        .with_loopback_runtime_token(READ_TOKEN)
        .unwrap();
    let read_only_publish = RemoteCoordinator::new(
        fixture.repository_id,
        "origin",
        &source_objects,
        &source_refs,
        &read_semantic,
        &read_workspace,
    )
    .publish("main", None, fixture.candidate_revision);
    assert!(matches!(
        read_only_publish,
        Err(RemoteError::Transport(RemoteTransportError::Other(_)))
    ));
    assert!(
        fixture
            .hub_core()
            .list_references(fixture.repository_id)
            .unwrap()
            .named
            .is_empty()
    );

    fixture.reload_policy(vec![grant(
        WRONG_REPOSITORY_TOKEN,
        fixture.wrong_repository_id,
        AuthorizationScope::Publish,
    )]);
    assert!(
        read_semantic
            .list_references(fixture.repository_id)
            .is_err()
    );
    assert!(
        publisher_semantic
            .list_references(fixture.repository_id)
            .is_err()
    );
    assert!(
        publisher_workspace
            .remote_snapshot_available(&fixture.candidate_snapshot)
            .is_err()
    );
    assert!(
        fixture
            .hub_core()
            .list_references(fixture.repository_id)
            .unwrap()
            .named
            .is_empty()
    );

    fixture.reload_policy(fixture.full_grants());
    let retry = RemoteCoordinator::new(
        fixture.repository_id,
        "origin",
        &source_objects,
        &source_refs,
        &publisher_semantic,
        &publisher_workspace,
    );
    retry
        .publish("main", None, fixture.candidate_revision)
        .unwrap();
    assert!(matches!(
        retry.publish("main", None, fixture.candidate_revision),
        Err(RemoteError::Transport(
            RemoteTransportError::ReferenceConflict
        ))
    ));
    assert_eq!(
        fixture
            .hub_core()
            .list_references(fixture.repository_id)
            .unwrap()
            .named[0]
            .target,
        fixture.candidate_revision
    );
    assert_eq!(
        source_refs.read_ref("local/main").unwrap(),
        fixture.base_revision
    );
    assert_eq!(source_refs.read_accepted().unwrap(), accepted_before);
    assert_eq!(
        read_workspace_state(&fixture.source_root).unwrap(),
        workspace_before
    );
    assert_eq!(
        GitWorkspaceBackend::read_active_lineage_for_test(&fixture.source_root),
        active_lineage_before
    );
    assert_eq!(
        fs::read(fixture.source_root.join("published.txt")).unwrap(),
        working_tree_before
    );
}
