#![cfg(unix)]

//! Runs remote workflows in separate `kat` processes against authenticated
//! Hub and Git HTTP services terminated by a local TLS test proxy.

use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

use kat::domain::identity::{
    RepositoryId, RepositoryRevisionId, SemanticStateId, WorkspaceSnapshotId,
};
use kat::domain::revision::RepositoryRevision;
use kat::domain::workspace::WorkspaceBackend;
use kat::encoding::canonical_bytes;
use kat::encoding::decode::decode_canonical;
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::repository::auth::{
    AuthorizationAudience, AuthorizationGrant, AuthorizationPolicy, AuthorizationScope,
    AuthorizationVerifier,
};
use kat::repository::git_http::GitHttpGateway;
use kat::repository::http::HubHttpService;
use kat::repository::hub::TrustedHubSemanticTransport;
use kat::repository::init::init_repository;
use kat::repository::metadata::RepositoryMetadata;
use kat::repository::object_store::ObjectStore;
use kat::repository::open::open_repository;
use kat::repository::ref_store::{AcceptedRef, FileRefStore};
use kat::repository::remote::SemanticTransport;
use kat::repository::workspace::git::GitWorkspaceBackend;
use kat::repository::workspace::git_transport::{
    ControlledGitBackend, GitWorkspaceBinding, WorkspaceBackendId,
};
use kat::repository::workspace::state::read_workspace_state;
use kat::repository::workspace::{init_workspace, update_workspace_base};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose,
};
use rustls::StreamOwned;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::{ServerConfig, ServerConnection};
use tiny_http::Server;

const READ_TOKEN: &str = "rrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrr";
const PUBLISH_TOKEN: &str = "ppppppppppppppppppppppppppppppppppppppppppp";
const INVALID_TOKEN: &str = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
type CasHook = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;

struct RunningHttpServer {
    address: SocketAddr,
    server: Arc<Server>,
    thread: Option<JoinHandle<()>>,
}

impl RunningHttpServer {
    fn start(serve: impl FnOnce(&Server) + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = Arc::new(Server::from_listener(listener, None).unwrap());
        let serving_server = Arc::clone(&server);
        let thread = thread::spawn(move || serve(&serving_server));
        Self {
            address,
            server,
            thread: Some(thread),
        }
    }
}

impl Drop for RunningHttpServer {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

struct TlsProxy {
    address: SocketAddr,
    stopping: Arc<AtomicBool>,
    hook: CasHook,
    thread: Option<JoinHandle<()>>,
}

impl TlsProxy {
    fn start(backend: SocketAddr, tls: Arc<ServerConfig>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stopping = Arc::new(AtomicBool::new(false));
        let hook = Arc::new(Mutex::new(None));
        let serving = Arc::clone(&stopping);
        let request_hook = Arc::clone(&hook);
        let thread = thread::spawn(move || {
            while !serving.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((client, _)) => {
                        let tls = Arc::clone(&tls);
                        let hook = Arc::clone(&request_hook);
                        thread::spawn(move || serve_tls_request(client, backend, tls, hook));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            address,
            stopping,
            hook,
            thread: Some(thread),
        }
    }

    fn set_before_reference_cas(&self, hook: impl FnOnce() + Send + 'static) {
        *self.hook.lock().unwrap() = Some(Box::new(hook));
    }
}

impl Drop for TlsProxy {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

fn serve_tls_request(
    client: TcpStream,
    backend: SocketAddr,
    tls: Arc<ServerConfig>,
    hook: CasHook,
) {
    let Ok(connection) = ServerConnection::new(tls) else {
        return;
    };
    let mut stream = StreamOwned::new(connection, client);
    let request = match read_http_request(&mut stream) {
        Ok(request) => request,
        Err(_) => return,
    };
    if request
        .windows(b"/reference-cas ".len())
        .any(|window| window == b"/reference-cas ")
        && let Some(hook) = hook.lock().unwrap().take()
    {
        hook();
    }
    let Ok(mut upstream) = TcpStream::connect(backend) else {
        return;
    };
    if upstream.write_all(&request).is_err()
        || upstream.shutdown(std::net::Shutdown::Write).is_err()
    {
        return;
    }
    let mut response = Vec::new();
    if upstream.read_to_end(&mut response).is_ok() {
        if let Some(header_end) = response.windows(4).position(|window| window == b"\r\n\r\n") {
            response.splice(
                header_end + 2..header_end + 2,
                b"Connection: close\r\n".iter().copied(),
            );
        }
        let _ = stream.write_all(&response);
        stream.conn.send_close_notify();
        let _ = stream.flush();
    }
}

fn read_http_request(stream: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut request = Vec::new();
    let mut buffer = [0; 8192];
    let header_end = loop {
        if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
        if request.len() > 64 * 1024 {
            return Err(std::io::Error::other(
                "test proxy request headers too large",
            ));
        }
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "test proxy request ended before headers",
            ));
        }
        request.extend_from_slice(&buffer[..read]);
    };
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let content_length = headers.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    });
    let chunked = headers.lines().any(|line| {
        line.split_once(':').is_some_and(|(key, value)| {
            key.eq_ignore_ascii_case("transfer-encoding")
                && value
                    .split(',')
                    .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"))
        })
    });
    if chunked && content_length.is_some() {
        return Err(std::io::Error::other(
            "test proxy request has conflicting body lengths",
        ));
    }
    let request_length = if chunked {
        loop {
            if let Some(length) = chunked_request_length(&request, header_end)? {
                break length;
            }
            if request.len() >= 16 * 1024 * 1024 {
                return Err(std::io::Error::other("test proxy request body too large"));
            }
            let read = stream.read(&mut buffer)?;
            if read == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "test proxy request ended before chunked body completed",
                ));
            }
            request.extend_from_slice(&buffer[..read]);
        }
    } else {
        header_end
            .checked_add(content_length.unwrap_or(0))
            .ok_or_else(|| std::io::Error::other("test proxy request body is too large"))?
    };
    if request_length > 16 * 1024 * 1024 {
        return Err(std::io::Error::other("test proxy request body too large"));
    }
    while request.len() < request_length {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "test proxy request ended before body",
            ));
        }
        request.extend_from_slice(&buffer[..read]);
    }
    request.truncate(request_length);
    Ok(request)
}

fn chunked_request_length(request: &[u8], body_start: usize) -> std::io::Result<Option<usize>> {
    let mut cursor = body_start;
    loop {
        let Some(line_end) = request[cursor..]
            .windows(2)
            .position(|window| window == b"\r\n")
            .map(|index| cursor + index)
        else {
            return Ok(None);
        };
        let size = String::from_utf8_lossy(&request[cursor..line_end]);
        let size = size.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size, 16)
            .map_err(|_| std::io::Error::other("test proxy request has invalid chunk size"))?;
        cursor = line_end + 2;
        if size == 0 {
            loop {
                let Some(trailer_end) = request[cursor..]
                    .windows(2)
                    .position(|window| window == b"\r\n")
                    .map(|index| cursor + index)
                else {
                    return Ok(None);
                };
                if trailer_end == cursor {
                    return Ok(Some(cursor + 2));
                }
                cursor = trailer_end + 2;
            }
        }
        let Some(chunk_end) = cursor.checked_add(size) else {
            return Err(std::io::Error::other(
                "test proxy request chunk is too large",
            ));
        };
        let Some(framed_end) = chunk_end.checked_add(2) else {
            return Err(std::io::Error::other(
                "test proxy request chunk is too large",
            ));
        };
        if request.len() < framed_end {
            return Ok(None);
        }
        if &request[chunk_end..chunk_end + 2] != b"\r\n" {
            return Err(std::io::Error::other(
                "test proxy request chunk is malformed",
            ));
        }
        cursor = chunk_end + 2;
    }
}

struct Fixture {
    _http_hub: RunningHttpServer,
    _http_git: RunningHttpServer,
    hub_tls: TlsProxy,
    _git_tls: TlsProxy,
    source_root: PathBuf,
    target_root: PathBuf,
    winner_root: PathBuf,
    hub_root: PathBuf,
    policy_path: PathBuf,
    ca_file: PathBuf,
    repository_id: RepositoryId,
    binding: GitWorkspaceBinding,
    base_revision: RepositoryRevisionId,
    base_snapshot: WorkspaceSnapshotId,
    authorization: Arc<AuthorizationVerifier>,
    _temp: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source_root = temp.path().join("source");
        fs::create_dir_all(&source_root).unwrap();
        let initialized = run_kat(&source_root, &["init"], None, None);
        assert!(
            initialized.status.success(),
            "init failed: {}",
            String::from_utf8_lossy(&initialized.stderr)
        );
        let source_repository = open_repository(&source_root).unwrap();
        let source_workspace = read_workspace_state(&source_root).unwrap();
        let base_revision = source_workspace.base_revision;
        let base_snapshot = source_repository
            .read_revision(base_revision)
            .unwrap()
            .workspace_snapshot;
        let source_refs = FileRefStore::new(source_root.join(".kat"));
        source_refs
            .compare_and_swap_ref("local/main", None, base_revision)
            .unwrap();

        let binding = GitWorkspaceBinding {
            repository_id: source_repository.metadata.repository_id,
            workspace_backend_id: WorkspaceBackendId::new(),
        };
        let workspace_remote = temp.path().join("workspace.git");
        ControlledGitBackend::provision(&workspace_remote, binding).unwrap();
        let hub_root = temp.path().join("hub");
        let hub =
            TrustedHubSemanticTransport::provision(&hub_root, binding, &workspace_remote).unwrap();
        let policy_path = temp.path().join("authorization.json");
        write_policy(
            &policy_path,
            source_repository.metadata.repository_id,
            &[READ_TOKEN, PUBLISH_TOKEN],
        );
        let authorization = Arc::new(AuthorizationVerifier::open(&policy_path).unwrap());

        let hub_auth = Arc::clone(&authorization);
        let http_hub = RunningHttpServer::start(move |server| {
            HubHttpService::new(hub, hub_auth).serve(server)
        });
        let git_auth = Arc::clone(&authorization);
        let git_path = workspace_remote.clone();
        let http_git = RunningHttpServer::start(move |server| {
            GitHttpGateway::open(&git_path, binding, git_auth)
                .unwrap()
                .serve(server)
        });

        let ca_file = temp.path().join("remote-test-ca.pem");
        let tls = Arc::new(test_tls_config(&ca_file));
        let hub_tls = TlsProxy::start(http_hub.address, Arc::clone(&tls));
        let git_tls = TlsProxy::start(http_git.address, tls);

        let target_root = temp.path().join("target");
        let winner_root = temp.path().join("winner");
        clone_at_base(&source_root, &target_root, base_revision, &base_snapshot);
        clone_at_base(&source_root, &winner_root, base_revision, &base_snapshot);

        Self {
            _http_hub: http_hub,
            _http_git: http_git,
            hub_tls,
            _git_tls: git_tls,
            source_root,
            target_root,
            winner_root,
            hub_root,
            policy_path,
            ca_file,
            repository_id: source_repository.metadata.repository_id,
            binding,
            base_revision,
            base_snapshot,
            authorization,
            _temp: temp,
        }
    }

    fn hub_url(&self) -> String {
        format!("https://localhost:{}", self.hub_tls.address.port())
    }

    fn git_url(&self) -> String {
        format!(
            "https://localhost:{}/v1/repositories/{}/git",
            self._git_tls.address.port(),
            self.repository_id
        )
    }

    fn configure_remote(&self, root: &Path, token: Option<&str>) {
        let backend_id = self.binding.workspace_backend_id.to_string();
        let hub_url = self.hub_url();
        let git_url = self.git_url();
        let output = run_kat(
            root,
            &[
                "remote",
                "add",
                "origin",
                "--hub-url",
                &hub_url,
                "--git-url",
                &git_url,
                "--workspace-backend-id",
                &backend_id,
                "--json",
            ],
            token,
            Some(&self.ca_file),
        );
        assert!(
            output.status.success(),
            "remote add failed: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }

    fn revoke_read(&self) {
        write_policy(&self.policy_path, self.repository_id, &[PUBLISH_TOKEN]);
        self.authorization.reload(&self.policy_path).unwrap();
    }

    fn restore_read(&self) {
        write_policy(
            &self.policy_path,
            self.repository_id,
            &[READ_TOKEN, PUBLISH_TOKEN],
        );
        self.authorization.reload(&self.policy_path).unwrap();
    }
}

#[test]
fn authenticated_cli_publish_fetch_sync_and_partial_failure_use_separate_processes() {
    let fixture = Fixture::new();
    fixture.configure_remote(&fixture.source_root, Some(PUBLISH_TOKEN));

    let initial_push = run_kat(
        &fixture.source_root,
        &["push", "origin", "main", "--json"],
        Some(PUBLISH_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        initial_push.status.success(),
        "initial push failed: {}{}",
        String::from_utf8_lossy(&initial_push.stdout),
        String::from_utf8_lossy(&initial_push.stderr)
    );
    let initial_result: serde_json::Value = serde_json::from_slice(&initial_push.stdout).unwrap();
    assert_eq!(
        initial_result["data"]["expected_remote_target"],
        serde_json::Value::Null
    );
    assert_eq!(
        initial_result["data"]["published_target"],
        fixture.base_revision.to_string()
    );

    fixture.configure_remote(&fixture.target_root, None);
    fixture.configure_remote(&fixture.winner_root, None);
    let target_before = authority(&fixture.target_root);
    let first_fetch = run_kat(
        &fixture.target_root,
        &["fetch", "origin", "--json"],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        first_fetch.status.success(),
        "fetch failed: {}",
        String::from_utf8_lossy(&first_fetch.stdout)
    );
    let fetch_result: serde_json::Value = serde_json::from_slice(&first_fetch.stdout).unwrap();
    assert_eq!(fetch_result["data"]["updated"][0]["name"], "main");
    assert_eq!(authority(&fixture.target_root), target_before);
    assert_eq!(
        FileRefStore::new(fixture.target_root.join(".kat"))
            .read_ref("remotes/origin/main")
            .unwrap(),
        fixture.base_revision
    );

    let winner_revision = put_revision(
        &ObjectStore::new(fixture.winner_root.join(".kat")),
        vec![fixture.base_revision],
        SemanticStateId::from_object_id(
            open_repository(&fixture.winner_root)
                .unwrap()
                .accepted
                .state,
        ),
        fixture.base_snapshot.clone(),
    );
    let winner_workspace = read_workspace_state(&fixture.winner_root).unwrap();
    update_workspace_base(&fixture.winner_root, &winner_workspace.id, winner_revision).unwrap();
    let winner_push = run_kat(
        &fixture.winner_root,
        &["push", "origin", "winner", "--json"],
        Some(PUBLISH_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        winner_push.status.success(),
        "winner push failed: {}",
        String::from_utf8_lossy(&winner_push.stdout)
    );

    fs::write(
        fixture.source_root.join("source.txt"),
        "physical revision\n",
    )
    .unwrap();
    let source_backend = GitWorkspaceBackend::open(&fixture.source_root).unwrap();
    let next_snapshot = source_backend
        .create_snapshot(&[PathBuf::from("source.txt")])
        .unwrap();
    let source_repository = open_repository(&fixture.source_root).unwrap();
    let source_semantic = SemanticStateId::from_object_id(source_repository.accepted.state);
    let race_revision = put_revision(
        source_repository.object_store(),
        vec![fixture.base_revision],
        source_semantic,
        next_snapshot.clone(),
    );
    let next_revision = put_revision(
        source_repository.object_store(),
        vec![race_revision],
        source_semantic,
        next_snapshot,
    );
    let source_workspace = read_workspace_state(&fixture.source_root).unwrap();
    update_workspace_base(&fixture.source_root, &source_workspace.id, next_revision).unwrap();
    let source_before_push = authority(&fixture.source_root);

    let hub_root = fixture.hub_root.clone();
    let repository_id = fixture.repository_id;
    let base_revision = fixture.base_revision;
    fixture.hub_tls.set_before_reference_cas(move || {
        let hub = TrustedHubSemanticTransport::open(&hub_root, repository_id).unwrap();
        hub.compare_and_swap_reference(repository_id, "main", Some(base_revision), race_revision)
            .unwrap();
    });
    let stale_push = run_kat(
        &fixture.source_root,
        &["push", "origin", "main", "--json"],
        Some(PUBLISH_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(!stale_push.status.success());
    let stale_result: serde_json::Value = serde_json::from_slice(&stale_push.stdout).unwrap();
    assert_eq!(stale_result["error"]["code"], "REMOTE_REFERENCE_CONFLICT");
    let hub = TrustedHubSemanticTransport::open(&fixture.hub_root, fixture.repository_id).unwrap();
    assert_eq!(
        hub.list_references(fixture.repository_id)
            .unwrap()
            .named
            .iter()
            .find(|reference| reference.name == "main")
            .unwrap()
            .target,
        race_revision
    );

    let retry_push = run_kat(
        &fixture.source_root,
        &["push", "origin", "main", "--json"],
        Some(PUBLISH_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        retry_push.status.success(),
        "retry push failed: {}",
        String::from_utf8_lossy(&retry_push.stdout)
    );
    let retry_result: serde_json::Value = serde_json::from_slice(&retry_push.stdout).unwrap();
    assert_eq!(
        retry_result["data"]["expected_remote_target"],
        race_revision.to_string()
    );
    assert_eq!(authority(&fixture.source_root), source_before_push);

    let mismatch_root = fixture._temp.path().join("mismatched-backend");
    clone_at_base(
        &fixture.source_root,
        &mismatch_root,
        fixture.base_revision,
        &fixture.base_snapshot,
    );
    let wrong_backend_id = WorkspaceBackendId::new().to_string();
    let mismatch_remote = run_kat(
        &mismatch_root,
        &[
            "remote",
            "add",
            "origin",
            "--hub-url",
            &fixture.hub_url(),
            "--git-url",
            &fixture.git_url(),
            "--workspace-backend-id",
            &wrong_backend_id,
            "--json",
        ],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        mismatch_remote.status.success(),
        "remote configuration failed: {}",
        String::from_utf8_lossy(&mismatch_remote.stdout)
    );
    let mismatch_before = authority(&mismatch_root);
    let mismatch_fetch = run_kat(
        &mismatch_root,
        &["fetch", "origin", "--json"],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(!mismatch_fetch.status.success());
    let mismatch_result: serde_json::Value =
        serde_json::from_slice(&mismatch_fetch.stdout).unwrap();
    assert_eq!(mismatch_result["error"]["code"], "REMOTE_BINDING_MISMATCH");
    assert_eq!(authority(&mismatch_root), mismatch_before);

    let mut tracking_lock = fixture.target_root.join(".kat/refs/remotes/origin/winner");
    let mut extension = tracking_lock.extension().unwrap_or_default().to_os_string();
    extension.push(".lock");
    tracking_lock.set_extension(extension);
    fs::write(&tracking_lock, "injected tracking CAS conflict").unwrap();
    let target_before_sync = authority(&fixture.target_root);
    let partial_sync = run_kat(
        &fixture.target_root,
        &["sync", "origin", "--json"],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        !partial_sync.status.success(),
        "expected partial tracking failure, got: {}",
        String::from_utf8_lossy(&partial_sync.stdout)
    );
    let partial_result: serde_json::Value = serde_json::from_slice(&partial_sync.stdout).unwrap();
    assert_eq!(
        partial_result["error"]["code"],
        "FETCH_PARTIAL_TRACKING_UPDATE"
    );
    assert_eq!(
        partial_result["error"]["details"]["fetch"]["updated"][0]["name"],
        "main"
    );
    assert_eq!(
        partial_result["error"]["details"]["fetch"]["failed"][0]["name"],
        "winner"
    );
    let partial_fetch = &partial_result["error"]["details"]["fetch"];
    for field in ["updated", "unchanged", "removed", "failed", "unnamed_heads"] {
        assert!(
            partial_fetch[field].is_array(),
            "missing {field}: {partial_fetch}"
        );
    }
    assert_eq!(authority(&fixture.target_root), target_before_sync);
    let human_partial_sync = run_kat(
        &fixture.target_root,
        &["sync", "origin"],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(!human_partial_sync.status.success());
    let human_error = String::from_utf8_lossy(&human_partial_sync.stderr);
    assert!(human_error.contains("unchanged main"), "{human_error}");
    assert!(human_error.contains("failed winner"), "{human_error}");
    assert!(human_error.contains("main: OtherAhead"), "{human_error}");
    assert_eq!(authority(&fixture.target_root), target_before_sync);
    fs::remove_file(tracking_lock).unwrap();

    let full_sync = run_kat(
        &fixture.target_root,
        &["sync", "origin", "--json"],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        full_sync.status.success(),
        "sync failed: {}",
        String::from_utf8_lossy(&full_sync.stdout)
    );
    let full_result: serde_json::Value = serde_json::from_slice(&full_sync.stdout).unwrap();
    assert!(
        full_result["data"]["topology"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["reference"] == "main" && item["state"] == "OtherAhead")
    );
    assert_eq!(authority(&fixture.target_root), target_before_sync);
    let before_non_fast_forward = authority(&fixture.target_root);
    let remote_main_before_non_fast_forward = hub
        .list_references(fixture.repository_id)
        .unwrap()
        .named
        .iter()
        .find(|reference| reference.name == "main")
        .unwrap()
        .target;
    let non_fast_forward_push = run_kat(
        &fixture.target_root,
        &["push", "origin", "main", "--json"],
        Some(PUBLISH_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(!non_fast_forward_push.status.success());
    let non_fast_forward_result: serde_json::Value =
        serde_json::from_slice(&non_fast_forward_push.stdout).unwrap();
    assert_eq!(
        non_fast_forward_result["error"]["code"],
        "PUSH_NON_FAST_FORWARD"
    );
    assert_eq!(authority(&fixture.target_root), before_non_fast_forward);
    assert_eq!(
        hub.list_references(fixture.repository_id)
            .unwrap()
            .named
            .iter()
            .find(|reference| reference.name == "main")
            .unwrap()
            .target,
        remote_main_before_non_fast_forward
    );
    let before_old_push = authority(&fixture.target_root);
    let old_push = run_kat(
        &fixture.target_root,
        &["push", "origin", "old", "--json"],
        Some(PUBLISH_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        old_push.status.success(),
        "old push failed: {}",
        String::from_utf8_lossy(&old_push.stdout)
    );
    assert_eq!(authority(&fixture.target_root), before_old_push);
    let before_advance = authority(&fixture.target_root);
    assert_eq!(before_advance.workspace_base, fixture.base_revision);

    let advance = run_kat(
        &fixture.target_root,
        &["advance", &next_revision.to_string(), "--json"],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        advance.status.success(),
        "advance failed: {}",
        String::from_utf8_lossy(&advance.stdout)
    );
    let before_same_sync = authority(&fixture.target_root);
    let same_sync = run_kat(
        &fixture.target_root,
        &["sync", "origin", "--json"],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        same_sync.status.success(),
        "sync failed: {}",
        String::from_utf8_lossy(&same_sync.stdout)
    );
    let same_result: serde_json::Value = serde_json::from_slice(&same_sync.stdout).unwrap();
    assert!(
        same_result["data"]["topology"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["reference"] == "main" && item["state"] == "Same")
    );
    assert!(
        same_result["data"]["topology"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["reference"] == "old" && item["state"] == "LocalAhead")
    );
    assert!(
        same_result["data"]["topology"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["reference"] == "winner" && item["state"] == "Diverged")
    );
    assert_eq!(authority(&fixture.target_root), before_same_sync);

    let before_read_only_push = hub
        .list_references(fixture.repository_id)
        .unwrap()
        .named
        .iter()
        .find(|reference| reference.name == "main")
        .unwrap()
        .target;
    let read_only_push = run_kat(
        &fixture.target_root,
        &["push", "origin", "main", "--json"],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(!read_only_push.status.success());
    let read_only_result: serde_json::Value =
        serde_json::from_slice(&read_only_push.stdout).unwrap();
    assert_eq!(
        read_only_result["error"]["code"],
        "REMOTE_PERMISSION_DENIED"
    );
    assert_eq!(
        hub.list_references(fixture.repository_id)
            .unwrap()
            .named
            .iter()
            .find(|reference| reference.name == "main")
            .unwrap()
            .target,
        before_read_only_push
    );

    fixture.revoke_read();
    let before_revoked_fetch = authority(&fixture.target_root);
    let invalid_credential = run_kat(
        &fixture.target_root,
        &["fetch", "origin", "--json"],
        Some(INVALID_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(!invalid_credential.status.success());
    let invalid_result: serde_json::Value =
        serde_json::from_slice(&invalid_credential.stdout).unwrap();
    assert_eq!(
        invalid_result["error"]["code"],
        "REMOTE_AUTHENTICATION_FAILED"
    );
    assert!(!String::from_utf8_lossy(&invalid_credential.stdout).contains(INVALID_TOKEN));
    assert!(!String::from_utf8_lossy(&invalid_credential.stderr).contains(INVALID_TOKEN));
    assert_eq!(authority(&fixture.target_root), before_revoked_fetch);
    let revoked_fetch = run_kat(
        &fixture.target_root,
        &["fetch", "origin", "--json"],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(!revoked_fetch.status.success());
    let revoked_result: serde_json::Value = serde_json::from_slice(&revoked_fetch.stdout).unwrap();
    assert_eq!(
        revoked_result["error"]["code"],
        "REMOTE_AUTHENTICATION_FAILED"
    );
    assert!(!String::from_utf8_lossy(&revoked_fetch.stdout).contains(READ_TOKEN));
    assert!(!String::from_utf8_lossy(&revoked_fetch.stderr).contains(READ_TOKEN));
    assert_eq!(authority(&fixture.target_root), before_revoked_fetch);
    fixture.restore_read();
    let retry_fetch = run_kat(
        &fixture.target_root,
        &["fetch", "origin", "--json"],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        retry_fetch.status.success(),
        "authorized retry failed: {}",
        String::from_utf8_lossy(&retry_fetch.stdout)
    );
}

#[test]
fn authenticated_clone_creates_a_coherent_persistent_workspace() {
    let fixture = Fixture::new();
    fixture.configure_remote(&fixture.source_root, None);
    let pushed = run_kat(
        &fixture.source_root,
        &["push", "origin", "main", "--json"],
        Some(PUBLISH_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        pushed.status.success(),
        "{}",
        String::from_utf8_lossy(&pushed.stdout)
    );

    let destination = fixture._temp.path().join("clone");
    let cloned = run_kat(
        &fixture.source_root,
        &[
            "clone",
            "origin",
            "main",
            destination.to_str().unwrap(),
            "--json",
        ],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        cloned.status.success(),
        "clone failed: {}{}",
        String::from_utf8_lossy(&cloned.stdout),
        String::from_utf8_lossy(&cloned.stderr)
    );
    let clone_result: serde_json::Value = serde_json::from_slice(&cloned.stdout).unwrap();
    assert_eq!(
        clone_result["data"]["repository_id"],
        fixture.repository_id.to_string()
    );
    assert_eq!(
        clone_result["data"]["revision_id"],
        fixture.base_revision.to_string()
    );

    let source = open_repository(&fixture.source_root).unwrap();
    let clone = open_repository(&destination).unwrap();
    assert_eq!(clone.metadata.repository_id, source.metadata.repository_id);
    assert_eq!(clone.metadata.software_id, source.metadata.software_id);
    assert_eq!(clone.accepted, source.accepted);
    let workspace = read_workspace_state(&destination).unwrap();
    assert_eq!(workspace.base_revision, fixture.base_revision);
    let refs = FileRefStore::new(destination.join(".kat"));
    assert_eq!(refs.read_ref("local/main").unwrap(), fixture.base_revision);
    assert_eq!(
        refs.read_ref("remotes/origin/main").unwrap(),
        fixture.base_revision
    );
    let backend = GitWorkspaceBackend::open(&destination).unwrap();
    assert!(
        backend
            .verify_snapshot_integrity(&fixture.base_snapshot)
            .unwrap()
    );
    assert!(
        backend
            .inspect_working_state(&fixture.base_snapshot)
            .unwrap()
            .changes
            .is_clean()
    );
    let remote_config = fs::read_to_string(destination.join(".kat/remotes/origin.json")).unwrap();
    assert!(!remote_config.contains(READ_TOKEN));
    assert!(
        run_kat(
            &destination,
            &["status", "--json"],
            Some(READ_TOKEN),
            Some(&fixture.ca_file)
        )
        .status
        .success(),
        "clone must reopen in a new CLI process"
    );

    let missing_destination = fixture._temp.path().join("missing-reference-clone");
    let missing = run_kat(
        &fixture.source_root,
        &[
            "clone",
            "origin",
            "does-not-exist",
            missing_destination.to_str().unwrap(),
            "--json",
        ],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(!missing.status.success());
    let missing_result: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert_eq!(missing_result["error"]["code"], "CLONE_REFERENCE_NOT_FOUND");
    assert!(!missing_destination.exists());
    assert!(fs::read_dir(fixture._temp.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".kat-clone-")
    }));
}

#[test]
fn sys_02_cli_clone_and_reconcile_fail_closed_before_publication() {
    let fixture = Fixture::new();
    fixture.configure_remote(&fixture.source_root, None);
    let initial_push = run_kat(
        &fixture.source_root,
        &["push", "origin", "main", "--json"],
        Some(PUBLISH_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        initial_push.status.success(),
        "{}",
        String::from_utf8_lossy(&initial_push.stdout)
    );

    let developer_a = fixture._temp.path().join("developer-a");
    let developer_b = fixture._temp.path().join("developer-b");
    for destination in [&developer_a, &developer_b] {
        let cloned = run_kat(
            &fixture.source_root,
            &["clone", "origin", "main", destination.to_str().unwrap()],
            Some(READ_TOKEN),
            Some(&fixture.ca_file),
        );
        assert!(
            cloned.status.success(),
            "clone failed: {}{}",
            String::from_utf8_lossy(&cloned.stdout),
            String::from_utf8_lossy(&cloned.stderr)
        );
        assert_eq!(
            read_workspace_state(destination).unwrap().base_revision,
            fixture.base_revision
        );
    }

    let begin_a = run_kat(
        &developer_a,
        &[
            "change",
            "begin",
            "--description",
            "Add service requirement",
        ],
        None,
        None,
    );
    assert!(
        begin_a.status.success(),
        "{}",
        String::from_utf8_lossy(&begin_a.stderr)
    );
    let requirement = run_kat(
        &developer_a,
        &[
            "create",
            "requirement",
            "--title",
            "The service keeps its data",
        ],
        None,
        None,
    );
    assert!(
        requirement.status.success(),
        "{}",
        String::from_utf8_lossy(&requirement.stderr)
    );
    let requirement_id = output_field(&requirement.stdout, "element_id");
    for args in [
        vec!["commit"],
        vec!["record"],
        vec!["push", "origin", "main"],
    ] {
        let output = run_kat(
            &developer_a,
            &args,
            Some(PUBLISH_TOKEN),
            Some(&fixture.ca_file),
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let revision_a = read_workspace_state(&developer_a).unwrap().base_revision;

    let begin_b = run_kat(
        &developer_b,
        &[
            "change",
            "begin",
            "--description",
            "Add implementation module",
        ],
        None,
        None,
    );
    assert!(
        begin_b.status.success(),
        "{}",
        String::from_utf8_lossy(&begin_b.stderr)
    );
    let implementation = run_kat(
        &developer_b,
        &[
            "create",
            "implementation",
            "--title",
            "Persistent storage module",
        ],
        None,
        None,
    );
    assert!(
        implementation.status.success(),
        "{}",
        String::from_utf8_lossy(&implementation.stderr)
    );
    let implementation_id = output_field(&implementation.stdout, "element_id");
    let commit = run_kat(&developer_b, &["commit"], None, None);
    assert!(
        commit.status.success(),
        "{}",
        String::from_utf8_lossy(&commit.stderr)
    );
    fs::create_dir_all(developer_b.join("src")).unwrap();
    fs::write(
        developer_b.join("src/module.rs"),
        "pub fn durable() -> bool { true }\n",
    )
    .unwrap();
    let recorded = run_kat(&developer_b, &["record", "src/module.rs"], None, None);
    assert!(
        recorded.status.success(),
        "{}",
        String::from_utf8_lossy(&recorded.stderr)
    );
    let revision_b = read_workspace_state(&developer_b).unwrap().base_revision;

    let fetch = run_kat(
        &developer_b,
        &["fetch", "origin"],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        fetch.status.success(),
        "{}",
        String::from_utf8_lossy(&fetch.stderr)
    );
    let remote_revision = FileRefStore::new(developer_b.join(".kat"))
        .read_ref("remotes/origin/main")
        .unwrap();
    let sync = run_kat(
        &developer_b,
        &["sync", "origin", "--json"],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        sync.status.success(),
        "{}",
        String::from_utf8_lossy(&sync.stdout)
    );
    let sync_result: serde_json::Value = serde_json::from_slice(&sync.stdout).unwrap();
    assert!(
        sync_result["data"]["topology"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| { item["reference"] == "main" && item["state"] == "Diverged" })
    );
    assert_eq!(
        read_workspace_state(&developer_b).unwrap().base_revision,
        revision_b
    );

    let reconcile = run_kat(
        &developer_b,
        &["reconcile", &remote_revision.to_string(), "--json"],
        None,
        None,
    );
    assert!(
        reconcile.status.success(),
        "{}",
        String::from_utf8_lossy(&reconcile.stdout)
    );
    let prepared: serde_json::Value = serde_json::from_slice(&reconcile.stdout).unwrap();
    assert_eq!(prepared["data"]["status"], "prepared_clean");
    let finalize = run_kat(&developer_b, &["finalize"], None, None);
    assert!(
        finalize.status.success(),
        "{}",
        String::from_utf8_lossy(&finalize.stderr)
    );
    let merge_revision = read_workspace_state(&developer_b).unwrap().base_revision;
    let merge = open_repository(&developer_b)
        .unwrap()
        .read_revision(merge_revision)
        .unwrap();
    assert!(merge.parents.contains(&revision_a));
    assert!(merge.parents.contains(&revision_b));

    let before_push = authority(&developer_b);
    let remote_before_push =
        TrustedHubSemanticTransport::open(&fixture.hub_root, fixture.repository_id)
            .unwrap()
            .list_references(fixture.repository_id)
            .unwrap()
            .named
            .into_iter()
            .find(|reference| reference.name == "main")
            .unwrap()
            .target;
    assert_eq!(remote_before_push, revision_a);
    let push = run_kat(
        &developer_b,
        &["push", "origin", "main", "--json"],
        Some(PUBLISH_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(!push.status.success());
    let push_result: serde_json::Value = serde_json::from_slice(&push.stdout).unwrap();
    assert_eq!(
        push_result["error"]["code"],
        "PUSH_WORKSPACE_STATE_MISMATCH"
    );
    assert_eq!(authority(&developer_b), before_push);
    let remote_after_push =
        TrustedHubSemanticTransport::open(&fixture.hub_root, fixture.repository_id)
            .unwrap()
            .list_references(fixture.repository_id)
            .unwrap()
            .named
            .into_iter()
            .find(|reference| reference.name == "main")
            .unwrap()
            .target;
    assert_eq!(remote_after_push, remote_before_push);
    let accepted = open_repository(&developer_b).unwrap().accepted;
    assert_ne!(accepted.state, merge.semantic_state.as_object_id());
    assert_eq!(
        read_workspace_state(&developer_a).unwrap().base_revision,
        revision_a
    );
    assert_eq!(
        read_workspace_state(&developer_b).unwrap().base_revision,
        merge_revision
    );
    assert!(
        run_kat(&developer_a, &["show", &requirement_id], None, None)
            .status
            .success()
    );
    assert!(
        run_kat(&developer_b, &["show", &implementation_id], None, None)
            .status
            .success()
    );
}

#[test]
fn sys_03_two_cli_developers_resolve_semantic_and_physical_conflicts() {
    let fixture = Fixture::new();
    fixture.configure_remote(&fixture.source_root, None);
    let begin = run_kat(
        &fixture.source_root,
        &["change", "begin", "--description", "Seed conflict targets"],
        None,
        None,
    );
    assert!(begin.status.success());
    let created = run_kat(
        &fixture.source_root,
        &["create", "requirement", "--title", "Original requirement"],
        None,
        None,
    );
    assert!(created.status.success());
    let requirement_id = output_field(&created.stdout, "element_id");
    assert!(
        run_kat(&fixture.source_root, &["commit"], None, None)
            .status
            .success()
    );
    fs::create_dir_all(fixture.source_root.join("src")).unwrap();
    fs::write(
        fixture.source_root.join("src/module.rs"),
        "pub fn value() -> u8 { 0 }\n",
    )
    .unwrap();
    assert!(
        run_kat(
            &fixture.source_root,
            &["record", "src/module.rs"],
            None,
            None
        )
        .status
        .success()
    );
    let seed_push = run_kat(
        &fixture.source_root,
        &["push", "origin", "main"],
        Some(PUBLISH_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        seed_push.status.success(),
        "{}",
        String::from_utf8_lossy(&seed_push.stderr)
    );

    let developer_a = fixture._temp.path().join("conflict-a");
    let developer_b = fixture._temp.path().join("conflict-b");
    for destination in [&developer_a, &developer_b] {
        let cloned = run_kat(
            &fixture.source_root,
            &["clone", "origin", "main", destination.to_str().unwrap()],
            Some(READ_TOKEN),
            Some(&fixture.ca_file),
        );
        assert!(
            cloned.status.success(),
            "{}",
            String::from_utf8_lossy(&cloned.stderr)
        );
        assert_eq!(
            fs::read(destination.join("src/module.rs")).unwrap(),
            b"pub fn value() -> u8 { 0 }\n"
        );
    }

    let base_revision = read_workspace_state(&developer_a).unwrap().base_revision;
    for (root, title, body, reference) in [
        (
            &developer_a,
            "Requirement chosen by A",
            "pub fn value() -> u8 { 1 }\n",
            "main",
        ),
        (
            &developer_b,
            "Requirement chosen by B",
            "pub fn value() -> u8 { 2 }\n",
            "developer-b",
        ),
    ] {
        assert!(
            run_kat(root, &["change", "begin"], None, None)
                .status
                .success()
        );
        let update = run_kat(
            root,
            &["update", &requirement_id, "--title", title],
            None,
            None,
        );
        assert!(
            update.status.success(),
            "{}",
            String::from_utf8_lossy(&update.stderr)
        );
        assert!(run_kat(root, &["commit"], None, None).status.success());
        fs::write(root.join("src/module.rs"), body).unwrap();
        let record = run_kat(root, &["record", "src/module.rs"], None, None);
        assert!(
            record.status.success(),
            "{}",
            String::from_utf8_lossy(&record.stderr)
        );
        let push = run_kat(
            root,
            &["push", "origin", reference],
            Some(PUBLISH_TOKEN),
            Some(&fixture.ca_file),
        );
        assert!(
            push.status.success(),
            "{}{}",
            String::from_utf8_lossy(&push.stdout),
            String::from_utf8_lossy(&push.stderr)
        );
    }
    let revision_a = read_workspace_state(&developer_a).unwrap().base_revision;
    let revision_b = read_workspace_state(&developer_b).unwrap().base_revision;
    assert_ne!(revision_a, revision_b);

    let fetched = run_kat(
        &developer_a,
        &["fetch", "origin"],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        fetched.status.success(),
        "{}",
        String::from_utf8_lossy(&fetched.stderr)
    );
    let synchronized = run_kat(
        &developer_a,
        &["sync", "origin", "--json"],
        Some(READ_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        synchronized.status.success(),
        "{}",
        String::from_utf8_lossy(&synchronized.stdout)
    );
    let topology: serde_json::Value = serde_json::from_slice(&synchronized.stdout).unwrap();
    assert!(
        topology["data"]["topology"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| { item["reference"] == "developer-b" && item["state"] == "Diverged" })
    );
    let target = FileRefStore::new(developer_a.join(".kat"))
        .read_ref("remotes/origin/developer-b")
        .unwrap();
    let reconcile = run_kat(
        &developer_a,
        &["reconcile", &target.to_string(), "--json"],
        None,
        None,
    );
    assert!(
        reconcile.status.success(),
        "{}",
        String::from_utf8_lossy(&reconcile.stdout)
    );
    let conflicts = run_kat(&developer_a, &["conflicts", "--json"], None, None);
    let conflicts: serde_json::Value = serde_json::from_slice(&conflicts.stdout).unwrap();
    let semantic_id = conflicts["data"]["semantic_conflicts"][0]["conflict_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let physical_id = conflicts["data"]["physical_conflicts"][0]["conflict_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let before_failed_finalize = authority(&developer_a);
    let blocked_finalize = run_kat(&developer_a, &["finalize", "--json"], None, None);
    assert!(!blocked_finalize.status.success());
    assert_eq!(authority(&developer_a), before_failed_finalize);

    let semantic_resolution = run_kat(
        &developer_a,
        &[
            "resolve",
            "semantic",
            &semantic_id,
            "--accept",
            "local",
            "--json",
        ],
        None,
        None,
    );
    assert!(
        semantic_resolution.status.success(),
        "{}",
        String::from_utf8_lossy(&semantic_resolution.stdout)
    );
    let still_blocked = run_kat(&developer_a, &["finalize", "--json"], None, None);
    assert!(!still_blocked.status.success());
    assert_eq!(authority(&developer_a), before_failed_finalize);

    assert!(
        run_kat(&developer_a, &["materialize"], None, None)
            .status
            .success()
    );
    fs::write(
        developer_a.join("src/module.rs"),
        "pub fn value() -> u8 { 1 }\n",
    )
    .unwrap();
    let physical_resolution = run_kat(
        &developer_a,
        &[
            "resolve",
            "physical",
            &physical_id,
            "--accept",
            "working",
            "--json",
        ],
        None,
        None,
    );
    assert!(
        physical_resolution.status.success(),
        "{}",
        String::from_utf8_lossy(&physical_resolution.stdout)
    );
    let finalize = run_kat(&developer_a, &["finalize"], None, None);
    assert!(
        finalize.status.success(),
        "{}",
        String::from_utf8_lossy(&finalize.stderr)
    );

    let merged_id = read_workspace_state(&developer_a).unwrap().base_revision;
    let merged = open_repository(&developer_a)
        .unwrap()
        .read_revision(merged_id)
        .unwrap();
    assert!(merged.parents.contains(&revision_a));
    assert!(merged.parents.contains(&revision_b));
    assert_eq!(
        merged.semantic_state.as_object_id(),
        open_repository(&developer_a).unwrap().accepted.state
    );
    assert_eq!(
        fs::read(developer_a.join("src/module.rs")).unwrap(),
        b"pub fn value() -> u8 { 1 }\n"
    );
    assert_eq!(
        read_workspace_state(&developer_b).unwrap().base_revision,
        revision_b
    );
    assert_eq!(
        FileRefStore::new(developer_a.join(".kat"))
            .read_ref("local/main")
            .unwrap(),
        base_revision
    );

    let published = run_kat(
        &developer_a,
        &["push", "origin", "main"],
        Some(PUBLISH_TOKEN),
        Some(&fixture.ca_file),
    );
    assert!(
        published.status.success(),
        "{}{}",
        String::from_utf8_lossy(&published.stdout),
        String::from_utf8_lossy(&published.stderr)
    );
    let remote_main = TrustedHubSemanticTransport::open(&fixture.hub_root, fixture.repository_id)
        .unwrap()
        .list_references(fixture.repository_id)
        .unwrap()
        .named
        .into_iter()
        .find(|item| item.name == "main")
        .unwrap()
        .target;
    assert_eq!(remote_main, merged_id);
    assert!(
        run_kat(&developer_a, &["show", &requirement_id], None, None)
            .status
            .success()
    );
}

fn output_field(output: &[u8], key: &str) -> String {
    String::from_utf8_lossy(output)
        .lines()
        .find_map(|line| {
            let (name, value) = line.trim().split_once(':')?;
            (name == key).then(|| value.trim().to_owned())
        })
        .unwrap_or_else(|| {
            panic!(
                "missing {key} in output: {}",
                String::from_utf8_lossy(output)
            )
        })
}

#[derive(Debug, PartialEq, Eq)]
struct Authority {
    accepted: Vec<u8>,
    workspace_base: RepositoryRevisionId,
    local_ref: RepositoryRevisionId,
    active_lineage: Option<String>,
    working_tree: Vec<(PathBuf, Vec<u8>)>,
}

fn authority(root: &Path) -> Authority {
    open_repository(root).unwrap();
    let workspace = read_workspace_state(root).unwrap();
    let refs = FileRefStore::new(root.join(".kat"));
    let mut working_tree = Vec::new();
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == ".kat" || entry.file_type().unwrap().is_dir() {
            continue;
        }
        if entry.file_type().unwrap().is_file() {
            working_tree.push((
                entry.path().strip_prefix(root).unwrap().to_path_buf(),
                fs::read(entry.path()).unwrap(),
            ));
        }
    }
    working_tree.sort_by(|left, right| left.0.cmp(&right.0));
    Authority {
        accepted: fs::read(root.join(".kat/refs/accepted")).unwrap(),
        workspace_base: workspace.base_revision,
        local_ref: refs.read_ref("local/main").unwrap(),
        active_lineage: fs::read_to_string(root.join(".kat/physical/active_lineage")).ok(),
        working_tree,
    }
}

fn put_revision(
    store: &ObjectStore,
    parents: Vec<RepositoryRevisionId>,
    semantic_state: SemanticStateId,
    workspace_snapshot: WorkspaceSnapshotId,
) -> RepositoryRevisionId {
    let bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::RepositoryRevision(RepositoryRevision {
            parents,
            semantic_state,
            workspace_snapshot,
            semantic_change: None,
        }),
    })
    .unwrap();
    RepositoryRevisionId::from_object_id(store.put(&bytes).unwrap())
}

fn clone_at_base(
    source_root: &Path,
    target_root: &Path,
    base_revision: RepositoryRevisionId,
    base_snapshot: &WorkspaceSnapshotId,
) {
    fs::create_dir_all(target_root).unwrap();
    init_repository(target_root).unwrap();
    let source = open_repository(source_root).unwrap();
    let mut metadata = RepositoryMetadata::read(&target_root.join(".kat/repository.toml")).unwrap();
    metadata.repository_id = source.metadata.repository_id;
    metadata
        .write(&target_root.join(".kat/repository.toml"))
        .unwrap();
    let state =
        decode_canonical(&source.object_store().get(source.accepted.state).unwrap()).unwrap();
    let ontology = match state.payload {
        CanonicalPayload::SemanticState(state) => state.ontology_version,
        _ => panic!("accepted object is not a SemanticState"),
    };
    for id in [
        source.accepted.state,
        ontology,
        base_revision.as_object_id(),
    ] {
        target_store(target_root)
            .put(&source.object_store().get(id).unwrap())
            .unwrap();
    }
    let target_refs = FileRefStore::new(target_root.join(".kat"));
    let old = target_refs.read_accepted().unwrap();
    target_refs
        .compare_and_swap_accepted(
            &old,
            &AcceptedRef {
                state: source.accepted.state,
                change: source.accepted.change,
            },
        )
        .unwrap();
    let backend = GitWorkspaceBackend::init(target_root).unwrap();
    let snapshot = backend.create_snapshot(&[]).unwrap();
    assert_eq!(&snapshot, base_snapshot);
    target_refs
        .compare_and_swap_ref("local/main", None, base_revision)
        .unwrap();
    init_workspace(target_root, base_revision).unwrap();
}

fn target_store(root: &Path) -> ObjectStore {
    ObjectStore::new(root.join(".kat"))
}

fn write_policy(path: &Path, repository_id: RepositoryId, tokens: &[&str]) {
    let grants = tokens
        .iter()
        .map(|token| {
            AuthorizationGrant::from_token(
                token,
                repository_id,
                if *token == READ_TOKEN {
                    AuthorizationScope::Read
                } else {
                    AuthorizationScope::Publish
                },
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
        })
        .collect();
    let policy = AuthorizationPolicy::new(grants).unwrap();
    fs::write(path, serde_json::to_vec(&policy).unwrap()).unwrap();
}

fn test_tls_config(ca_file: &Path) -> ServerConfig {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut ca_params = CertificateParams::default();
    ca_params.distinguished_name = DistinguishedName::new();
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "KAT test CA");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
    ];
    let ca_key = KeyPair::generate().unwrap();
    let ca = ca_params.self_signed(&ca_key).unwrap();
    fs::write(ca_file, ca.pem()).unwrap();

    let mut server_params = CertificateParams::new(vec!["localhost".into()]).unwrap();
    server_params.distinguished_name = DistinguishedName::new();
    server_params
        .distinguished_name
        .push(DnType::CommonName, "localhost");
    server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    server_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    let server_key = KeyPair::generate().unwrap();
    let server_certificate = server_params.signed_by(&server_key, &ca, &ca_key).unwrap();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(server_key.serialize_der()));
    ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![server_certificate.der().clone(), ca.der().clone()],
            key,
        )
        .unwrap()
}

fn run_kat(
    root: &Path,
    args: &[&str],
    token: Option<&str>,
    ca_file: Option<&Path>,
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kat"));
    command
        .args(args)
        .current_dir(root)
        .env_remove("KAT_REMOTE_TOKEN")
        .env_remove("KAT_REMOTE_CA_FILE")
        .env_remove("SSL_CERT_FILE");
    if let Some(token) = token {
        command.env("KAT_REMOTE_TOKEN", token);
    }
    if let Some(path) = ca_file {
        command.env("KAT_REMOTE_CA_FILE", path);
    }
    command.output().expect("kat CLI process starts")
}
