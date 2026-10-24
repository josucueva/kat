//! Authenticated Git smart HTTP access to one controlled KAT workspace backend.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::domain::identity::RepositoryId;
use crate::repository::auth::{AuthorizationAction, AuthorizationError, AuthorizationVerifier};
use crate::repository::workspace::git_transport::{ControlledGitBackend, GitWorkspaceBinding};

#[cfg(not(test))]
const MAX_PACK_BYTES: usize = 256 * 1024 * 1024;
#[cfg(test)]
const MAX_PACK_BYTES: usize = 1024;
const MAX_REQUESTS_PER_SECOND: usize = 100;

/// Authenticated Git smart HTTP gateway for one fixed controlled backend.
///
/// The caller owns the listener. Keep the backend private and expose this
/// gateway only behind the trusted TLS proxy described by the Phase 12.5
/// service contract.
pub struct GitHttpGateway {
    backend_path: PathBuf,
    backend_name: String,
    binding: GitWorkspaceBinding,
    authorization: Arc<AuthorizationVerifier>,
    request_window: Mutex<(Instant, usize)>,
}

impl GitHttpGateway {
    /// Opens and pins the configured backend after verifying its binding and
    /// create-only retention policy.
    pub fn open(
        backend_path: &Path,
        binding: GitWorkspaceBinding,
        authorization: Arc<AuthorizationVerifier>,
    ) -> Result<Self, crate::repository::remote::RemoteTransportError> {
        let backend = ControlledGitBackend::open(backend_path, binding)?;
        let path = std::fs::canonicalize(backend.path()).map_err(other)?;
        let backend_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| {
                !name.is_empty()
                    && *name != "."
                    && *name != ".."
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            })
            .ok_or_else(|| other("controlled Git backend path is invalid"))?
            .to_owned();
        Ok(Self {
            backend_path: path,
            backend_name,
            binding,
            authorization,
            request_window: Mutex::new((Instant::now(), 0)),
        })
    }

    /// Handles requests from a caller-owned `tiny_http` listener.
    pub fn serve(&self, server: &Server) {
        // shortcut: serial requests, add bounded workers when Git transfer concurrency is needed.
        for request in server.incoming_requests() {
            self.handle(request);
        }
    }

    fn handle(&self, mut request: Request) {
        if !self.allow_request() {
            respond_error(request, 429, "rate-limited", false);
            return;
        }
        let Some(route) = Route::parse(request.method(), request.url(), self.binding.repository_id)
        else {
            respond_error(request, 404, "not-found", false);
            return;
        };
        if let Err(error) = self.authorization.authorize_git_basic_headers(
            request.headers(),
            self.binding.repository_id,
            route.action,
        ) {
            respond_error(
                request,
                error.status_code(),
                authorization_code(error),
                true,
            );
            return;
        }
        if let Err(status) = validate_request(&request, route.operation) {
            respond_error(request, status, request_error_code(status), false);
            return;
        }
        let body = match read_request_body(&mut request, route.operation) {
            Ok(body) => body,
            Err(status) => {
                respond_error(request, status, request_error_code(status), false);
                return;
            }
        };
        if let Err(error) = self.authorization.authorize_git_basic_headers(
            request.headers(),
            self.binding.repository_id,
            route.action,
        ) {
            respond_error(
                request,
                error.status_code(),
                authorization_code(error),
                true,
            );
            return;
        }
        if ControlledGitBackend::open(&self.backend_path, self.binding).is_err() {
            respond_error(request, 503, "backend-unavailable", false);
            return;
        }
        let protocol = match git_protocol_header(request.headers()) {
            Ok(protocol) => protocol,
            Err(()) => {
                respond_error(request, 400, "invalid-request", false);
                return;
            }
        };
        match run_git_http_backend(
            &self.backend_path,
            &self.backend_name,
            route.operation,
            body,
            protocol,
        ) {
            Ok((content_type, body)) => respond(request, 200, content_type, body, Vec::new()),
            Err(()) => respond_error(request, 502, "git-backend-failed", false),
        }
    }

    fn allow_request(&self) -> bool {
        let Ok(mut window) = self.request_window.lock() else {
            return false;
        };
        if window.0.elapsed() >= Duration::from_secs(1) {
            *window = (Instant::now(), 0);
        }
        if window.1 >= MAX_REQUESTS_PER_SECOND {
            return false;
        }
        window.1 += 1;
        true
    }
}

#[derive(Clone, Copy)]
struct Route {
    action: AuthorizationAction,
    operation: GitOperation,
}

impl Route {
    fn parse(method: &Method, url: &str, expected: RepositoryId) -> Option<Self> {
        let (path, query) = url.split_once('?').unwrap_or((url, ""));
        let rest = path.strip_prefix("/v1/repositories/")?;
        let (repository, path) = rest.split_once("/git/")?;
        let actual = repository.parse::<RepositoryId>().ok()?;
        if actual.to_string() != repository || actual != expected {
            return None;
        }

        match (method, path, query) {
            (Method::Get, "info/refs", "service=git-upload-pack") => Some(Self {
                action: AuthorizationAction::GitFetch,
                operation: GitOperation::DiscoverUpload,
            }),
            (Method::Get, "info/refs", "service=git-receive-pack") => Some(Self {
                action: AuthorizationAction::GitPush,
                operation: GitOperation::DiscoverReceive,
            }),
            (Method::Post, "git-upload-pack", "") => Some(Self {
                action: AuthorizationAction::GitFetch,
                operation: GitOperation::UploadPack,
            }),
            (Method::Post, "git-receive-pack", "") => Some(Self {
                action: AuthorizationAction::GitPush,
                operation: GitOperation::ReceivePack,
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum GitOperation {
    DiscoverUpload,
    UploadPack,
    DiscoverReceive,
    ReceivePack,
}

impl GitOperation {
    fn is_post(self) -> bool {
        matches!(self, Self::UploadPack | Self::ReceivePack)
    }

    fn expected_content_type(self) -> &'static str {
        match self {
            Self::DiscoverUpload => "application/x-git-upload-pack-advertisement",
            Self::UploadPack => "application/x-git-upload-pack-result",
            Self::DiscoverReceive => "application/x-git-receive-pack-advertisement",
            Self::ReceivePack => "application/x-git-receive-pack-result",
        }
    }

    fn request_content_type(self) -> &'static str {
        match self {
            Self::UploadPack => "application/x-git-upload-pack-request",
            Self::ReceivePack => "application/x-git-receive-pack-request",
            Self::DiscoverUpload | Self::DiscoverReceive => "",
        }
    }

    fn path_info(self, backend_name: &str) -> String {
        match self {
            Self::DiscoverUpload => format!("/{backend_name}/info/refs"),
            Self::UploadPack => format!("/{backend_name}/git-upload-pack"),
            Self::DiscoverReceive => format!("/{backend_name}/info/refs"),
            Self::ReceivePack => format!("/{backend_name}/git-receive-pack"),
        }
    }

    fn query_string(self) -> &'static str {
        match self {
            Self::DiscoverUpload => "service=git-upload-pack",
            Self::DiscoverReceive => "service=git-receive-pack",
            Self::UploadPack | Self::ReceivePack => "",
        }
    }
}

fn validate_request(request: &Request, operation: GitOperation) -> Result<(), u16> {
    if operation.is_post() {
        let values = request
            .headers()
            .iter()
            .filter(|header| header.field.equiv("Content-Type"))
            .collect::<Vec<_>>();
        if values.len() != 1
            || values[0]
                .value
                .as_str()
                .split(';')
                .next()
                .is_none_or(|value| {
                    !value
                        .trim()
                        .eq_ignore_ascii_case(operation.request_content_type())
                })
        {
            return Err(415);
        }
    } else if request.body_length().is_some_and(|length| length != 0) {
        return Err(400);
    }
    if request_body_too_large(request.body_length(), 0) {
        return Err(413);
    }
    Ok(())
}

fn read_request_body(request: &mut Request, operation: GitOperation) -> Result<Vec<u8>, u16> {
    if !operation.is_post() {
        return Ok(Vec::new());
    }
    // shortcut: buffers packs up to 256 MiB, stream through temporary files if deployments need larger packs.
    let mut body = Vec::new();
    request
        .as_reader()
        .take((MAX_PACK_BYTES as u64) + 1)
        .read_to_end(&mut body)
        .map_err(|_| 400u16)?;
    if request_body_too_large(None, body.len()) {
        return Err(413);
    }
    Ok(body)
}

fn request_body_too_large(length: Option<usize>, read: usize) -> bool {
    length.is_some_and(|length| length > MAX_PACK_BYTES) || read > MAX_PACK_BYTES
}

fn git_protocol_header(headers: &[Header]) -> Result<Option<&str>, ()> {
    let mut values = headers
        .iter()
        .filter(|header| header.field.equiv("Git-Protocol"));
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(());
    }
    match value.value.as_str() {
        "version=1" | "version=2" => Ok(Some(value.value.as_str())),
        _ => Err(()),
    }
}

fn run_git_http_backend(
    backend_path: &Path,
    backend_name: &str,
    operation: GitOperation,
    body: Vec<u8>,
    protocol: Option<&str>,
) -> Result<(&'static str, Vec<u8>), ()> {
    let parent = backend_path.parent().ok_or(())?;
    let path_info = operation.path_info(backend_name);
    let mut command = Command::new("git");
    command
        .arg("http-backend")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").ok_or(())?)
        .env("GIT_PROJECT_ROOT", parent)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "REQUEST_METHOD",
            if operation.is_post() { "POST" } else { "GET" },
        )
        .env("PATH_INFO", path_info)
        .env("QUERY_STRING", operation.query_string())
        .env("REMOTE_USER", "kat")
        .env("CONTENT_LENGTH", body.len().to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if operation.is_post() {
        command.env("CONTENT_TYPE", operation.request_content_type());
    }
    if let Some(protocol) = protocol {
        command.env("HTTP_GIT_PROTOCOL", protocol);
    }
    // `env_clear` above is required; these request values are all derived from
    // the authenticated fixed-route request and do not contain client paths.
    let mut child = command.spawn().map_err(|_| ())?;
    let stdout = child.stdout.take().ok_or(())?;
    let reader = thread::spawn(move || {
        let mut output = Vec::new();
        stdout
            .take((MAX_PACK_BYTES as u64) + 64 * 1024 + 1)
            .read_to_end(&mut output)
            .map(|_| output)
    });
    let mut stdin = child.stdin.take().ok_or(())?;
    stdin.write_all(&body).map_err(|_| ())?;
    drop(stdin);
    let status = child.wait().map_err(|_| ())?;
    let output = reader.join().map_err(|_| ())?.map_err(|_| ())?;
    if !status.success() || output.len() > MAX_PACK_BYTES + 64 * 1024 {
        return Err(());
    }
    parse_cgi_response(&output, operation.expected_content_type())
}

fn parse_cgi_response(
    output: &[u8],
    expected_content_type: &'static str,
) -> Result<(&'static str, Vec<u8>), ()> {
    let (header_end, separator_len) = output
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| (position, 4))
        .or_else(|| {
            output
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|position| (position, 2))
        })
        .ok_or(())?;
    let headers = std::str::from_utf8(&output[..header_end]).map_err(|_| ())?;
    let mut content_type = None;
    let mut status = 200;
    for line in headers.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let (name, value) = line.split_once(':').ok_or(())?;
        let value = value.trim();
        if name.eq_ignore_ascii_case("Content-Type") {
            if content_type.replace(value).is_some() {
                return Err(());
            }
        } else if name.eq_ignore_ascii_case("Status") {
            status = value
                .split_ascii_whitespace()
                .next()
                .and_then(|status| status.parse::<u16>().ok())
                .ok_or(())?;
        }
    }
    if status != 200 || content_type != Some(expected_content_type) {
        return Err(());
    }
    Ok((
        expected_content_type,
        output[header_end + separator_len..].to_vec(),
    ))
}

fn authorization_code(error: AuthorizationError) -> &'static str {
    match error {
        AuthorizationError::Unauthenticated => "unauthorized",
        AuthorizationError::Forbidden => "forbidden",
        AuthorizationError::RepositoryNotFound => "not-found",
        AuthorizationError::PolicyUnavailable => "service-unavailable",
        AuthorizationError::InvalidPolicy => "internal-error",
    }
}

fn request_error_code(status: u16) -> &'static str {
    match status {
        400 => "invalid-request",
        413 => "request-too-large",
        415 => "unsupported-media-type",
        _ => "request-failed",
    }
}

fn respond_error(request: Request, status: u16, code: &'static str, challenge: bool) {
    respond(
        request,
        status,
        "text/plain; charset=utf-8",
        format!("{code}\n").into_bytes(),
        if challenge {
            vec![
                Header::from_bytes(
                    "WWW-Authenticate",
                    "Basic realm=\"KAT workspace\", charset=\"UTF-8\"",
                )
                .unwrap(),
            ]
        } else {
            Vec::new()
        },
    );
}

fn respond(
    request: Request,
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    extra_headers: Vec<Header>,
) {
    let mut response = Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", content_type).unwrap())
        .with_header(Header::from_bytes("Connection", "close").unwrap());
    for header in extra_headers {
        response.add_header(header);
    }
    let _ = request.respond(response);
}

fn other(error: impl std::fmt::Display) -> crate::repository::remote::RemoteTransportError {
    crate::repository::remote::RemoteTransportError::Other(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::Arc;

    use base64::Engine;
    use git2::{Cred, Direction, FetchOptions, PushOptions, RemoteCallbacks, Repository};

    use crate::domain::identity::RepositoryId;
    use crate::repository::auth::{
        AuthorizationAudience, AuthorizationGrant, AuthorizationPolicy, AuthorizationScope,
    };
    use crate::repository::remote::WorkspaceTransport;
    use crate::repository::workspace::git::GitWorkspaceBackend;
    use crate::repository::workspace::git_transport::{GitWorkspaceTransport, WorkspaceBackendId};

    const READ_TOKEN: &str = "rrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrr";
    const PUBLISH_TOKEN: &str = "ppppppppppppppppppppppppppppppppppppppppppp";

    struct Fixture {
        _root: tempfile::TempDir,
        backend: PathBuf,
        repository_id: RepositoryId,
        binding: GitWorkspaceBinding,
    }

    impl Fixture {
        fn new() -> Self {
            Self::for_repository(RepositoryId::new())
        }

        fn for_repository(repository_id: RepositoryId) -> Self {
            let root = tempfile::tempdir().unwrap();
            let backend = root.path().join("workspace.git");
            let binding = GitWorkspaceBinding {
                repository_id,
                workspace_backend_id: WorkspaceBackendId::new(),
            };
            ControlledGitBackend::provision(&backend, binding).unwrap();
            Self {
                _root: root,
                backend,
                repository_id,
                binding,
            }
        }

        fn verifier(&self, scope: AuthorizationScope) -> Arc<AuthorizationVerifier> {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let token = match scope {
                AuthorizationScope::Read => READ_TOKEN,
                AuthorizationScope::Publish => PUBLISH_TOKEN,
            };
            let mut grants = vec![
                AuthorizationGrant::from_token(
                    token,
                    self.repository_id,
                    scope,
                    [AuthorizationAudience::WorkspaceGit],
                    now + 300,
                )
                .unwrap(),
            ];
            if scope == AuthorizationScope::Publish {
                grants.push(
                    AuthorizationGrant::from_token(
                        READ_TOKEN,
                        self.repository_id,
                        AuthorizationScope::Read,
                        [AuthorizationAudience::WorkspaceGit],
                        now + 300,
                    )
                    .unwrap(),
                );
            }
            Arc::new(
                AuthorizationVerifier::from_policy(AuthorizationPolicy::new(grants).unwrap())
                    .unwrap(),
            )
        }
    }

    struct RunningGateway {
        address: String,
        socket_address: String,
        server: Arc<Server>,
        serving: Option<thread::JoinHandle<()>>,
    }

    impl RunningGateway {
        fn start(fixture: &Fixture, verifier: Arc<AuthorizationVerifier>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = Arc::new(Server::from_listener(listener, None).unwrap());
            let gateway = Arc::new(
                GitHttpGateway::open(&fixture.backend, fixture.binding, verifier).unwrap(),
            );
            let serving_server = Arc::clone(&server);
            let serving = thread::spawn(move || gateway.serve(&serving_server));
            Self {
                address: format!(
                    "http://{address}/v1/repositories/{}/git",
                    fixture.repository_id
                ),
                socket_address: address.to_string(),
                server,
                serving: Some(serving),
            }
        }

        fn stop(mut self) {
            self.server.unblock();
            self.serving.take().unwrap().join().unwrap();
        }
    }

    fn add_basic(token: &'static str) -> RemoteCallbacks<'static> {
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(move |_url, _username, _| Cred::userpass_plaintext("kat", token));
        callbacks
    }

    fn make_commit(repo: &Repository) -> git2::Oid {
        let signature = git2::Signature::now("KAT test", "kat@example.invalid").unwrap();
        let tree_id = repo.treebuilder(None).unwrap().write().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(None, &signature, &signature, "snapshot", &tree, &[])
            .unwrap()
    }

    fn advertised_refs(
        remote: &mut git2::Remote<'_>,
        token: &'static str,
    ) -> Vec<(String, git2::Oid)> {
        let connection = remote
            .connect_auth(Direction::Fetch, Some(add_basic(token)), None)
            .unwrap();
        connection
            .list()
            .unwrap()
            .iter()
            .map(|head| (head.name().to_owned(), head.oid()))
            .collect()
    }

    fn http_get(address: &str, path: &str, authorization: &str) -> String {
        use std::io::{Read, Write};

        let mut socket = std::net::TcpStream::connect(address).unwrap();
        write!(socket, "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nAuthorization: {authorization}\r\n\r\n").unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn real_git_client_uses_shared_grants_and_create_only_backend_policy() {
        let fixture = Fixture::new();
        let server = RunningGateway::start(&fixture, fixture.verifier(AuthorizationScope::Publish));
        let local_dir = tempfile::tempdir().unwrap();
        let local = Repository::init_bare(local_dir.path()).unwrap();
        let commit = make_commit(&local);
        local
            .reference("refs/heads/source", commit, true, "test source")
            .unwrap();
        let lineage = format!("refs/kat/lineage/{}/{}", "ab".repeat(32), commit);
        let mut remote = local.remote_anonymous(&server.address).unwrap();
        let connection = remote
            .connect_auth(Direction::Push, Some(add_basic(PUBLISH_TOKEN)), None)
            .unwrap();
        assert!(!connection.list().unwrap().is_empty());
        drop(connection);
        let mut push = PushOptions::new();
        push.remote_callbacks(add_basic(PUBLISH_TOKEN));
        remote
            .push(&[format!("refs/heads/source:{lineage}")], Some(&mut push))
            .unwrap();

        let target_dir = tempfile::tempdir().unwrap();
        let target = Repository::init_bare(target_dir.path()).unwrap();
        let mut read_remote = target.remote_anonymous(&server.address).unwrap();
        let mut fetch = FetchOptions::new();
        fetch.remote_callbacks(add_basic(READ_TOKEN));
        read_remote
            .fetch(
                &[format!("{lineage}:refs/kat/fetched")],
                Some(&mut fetch),
                Some("KAT read grant fetch test"),
            )
            .unwrap();
        assert_eq!(
            target.find_reference("refs/kat/fetched").unwrap().target(),
            Some(commit)
        );

        let mut remote = local.remote_anonymous(&server.address).unwrap();
        let connection = remote
            .connect_auth(Direction::Fetch, Some(add_basic(PUBLISH_TOKEN)), None)
            .unwrap();
        assert!(
            connection
                .list()
                .unwrap()
                .iter()
                .any(|head| head.name() == lineage)
        );
        drop(connection);

        let replacement = make_commit(&local);
        local
            .reference("refs/heads/replacement", replacement, true, "replacement")
            .unwrap();
        let expected = commit;
        let mut update = PushOptions::new();
        update.remote_callbacks(add_basic(PUBLISH_TOKEN));
        let _ = remote.push(
            &[format!("refs/heads/replacement:{lineage}")],
            Some(&mut update),
        );
        assert_eq!(
            advertised_refs(&mut remote, PUBLISH_TOKEN)
                .into_iter()
                .find(|(name, _)| name == &lineage)
                .map(|(_, oid)| oid),
            Some(expected)
        );

        let mut delete = PushOptions::new();
        delete.remote_callbacks(add_basic(PUBLISH_TOKEN));
        let _ = remote.push(&[format!(":{lineage}")], Some(&mut delete));
        assert_eq!(
            advertised_refs(&mut remote, PUBLISH_TOKEN)
                .into_iter()
                .find(|(name, _)| name == &lineage)
                .map(|(_, oid)| oid),
            Some(expected)
        );

        let mut non_lineage = PushOptions::new();
        non_lineage.remote_callbacks(add_basic(PUBLISH_TOKEN));
        let _ = remote.push(
            &["refs/heads/source:refs/heads/main".to_owned()],
            Some(&mut non_lineage),
        );
        assert!(
            advertised_refs(&mut remote, PUBLISH_TOKEN)
                .iter()
                .all(|(name, _)| name != "refs/heads/main")
        );
        server.stop();
    }

    #[test]
    fn read_grant_cannot_push_and_each_request_is_authenticated() {
        let fixture = Fixture::new();
        let server = RunningGateway::start(&fixture, fixture.verifier(AuthorizationScope::Read));
        let local_dir = tempfile::tempdir().unwrap();
        let local = Repository::init_bare(local_dir.path()).unwrap();
        let mut remote = local.remote_anonymous(&server.address).unwrap();
        assert!(
            remote
                .connect_auth(Direction::Fetch, Some(add_basic(READ_TOKEN)), None)
                .is_ok()
        );

        let commit = make_commit(&local);
        local
            .reference("refs/heads/source", commit, true, "test source")
            .unwrap();
        let lineage = format!("refs/kat/lineage/{}/{}", "cd".repeat(32), commit);
        let mut push = PushOptions::new();
        push.remote_callbacks(add_basic(READ_TOKEN));
        assert!(
            remote
                .push(&[format!("refs/heads/source:{lineage}")], Some(&mut push))
                .is_err()
        );
        server.stop();
    }

    #[test]
    fn kat_workspace_transport_uses_a_runtime_token_without_persisting_it() {
        let local_dir = tempfile::tempdir().unwrap();
        let initialized = crate::repository::init::init_repository(local_dir.path()).unwrap();
        GitWorkspaceBackend::init(local_dir.path()).unwrap();
        let fixture = Fixture::for_repository(initialized.repository_id);
        let server = RunningGateway::start(&fixture, fixture.verifier(AuthorizationScope::Publish));
        assert!(
            GitWorkspaceTransport::configure(
                local_dir.path(),
                "https://kat:secret@example.invalid/v1/repositories/test/git",
                fixture.binding
            )
            .is_err()
        );
        assert!(
            GitWorkspaceTransport::configure(
                local_dir.path(),
                "https://example.invalid/v1/repositories/test/git?token=secret",
                fixture.binding
            )
            .is_err()
        );
        let transport =
            GitWorkspaceTransport::configure(local_dir.path(), &server.address, fixture.binding)
                .unwrap()
                .with_loopback_runtime_token(PUBLISH_TOKEN)
                .unwrap();
        assert!(
            !transport
                .remote_snapshot_available(&crate::domain::identity::WorkspaceSnapshotId::new(
                    vec![7; 32]
                ))
                .unwrap()
        );
        let saved =
            std::fs::read_to_string(local_dir.path().join(".kat/physical/workspace-remote.json"))
                .unwrap();
        assert!(!saved.contains(PUBLISH_TOKEN));
        let embedded_git =
            Repository::open_bare(local_dir.path().join(".kat/physical/git")).unwrap();
        assert!(
            !embedded_git
                .config()
                .unwrap()
                .get_bool("http.followRedirects")
                .unwrap()
        );
        server.stop();
    }

    #[test]
    fn gateway_hides_wrong_repository_and_rejects_unauthenticated_discovery() {
        use std::io::{Read, Write};

        let fixture = Fixture::new();
        let server = RunningGateway::start(&fixture, fixture.verifier(AuthorizationScope::Read));
        let address = server.socket_address.as_str();
        let path = format!(
            "/v1/repositories/{}/git/info/refs?service=git-upload-pack",
            fixture.repository_id
        );
        let mut socket = std::net::TcpStream::connect(address).unwrap();
        write!(
            socket,
            "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 401"));
        assert!(!response.contains(READ_TOKEN));

        let mut socket = std::net::TcpStream::connect(address).unwrap();
        write!(socket, "POST /v1/repositories/{}/git/git-upload-pack HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/x-git-upload-pack-request\r\nContent-Length: 0\r\n\r\n", fixture.repository_id).unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 401"));
        assert!(!response.contains(READ_TOKEN));

        let credential =
            base64::engine::general_purpose::STANDARD.encode(format!("kat:{READ_TOKEN}"));
        let wrong_username =
            base64::engine::general_purpose::STANDARD.encode(format!("not-kat:{READ_TOKEN}"));
        let path = format!(
            "/v1/repositories/{}/git/info/refs?service=git-upload-pack",
            fixture.repository_id
        );
        assert!(
            http_get(address, &path, &format!("Basic {wrong_username}"))
                .starts_with("HTTP/1.1 401")
        );
        let mut socket = std::net::TcpStream::connect(address).unwrap();
        let oversized_body = vec![0; MAX_PACK_BYTES + 1];
        write!(socket, "POST /v1/repositories/{}/git/git-upload-pack HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nAuthorization: Basic {credential}\r\nContent-Type: application/x-git-upload-pack-request\r\nContent-Length: {}\r\n\r\n", fixture.repository_id, oversized_body.len()).unwrap();
        socket.write_all(&oversized_body).unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 413"));

        let wrong = RepositoryId::new();
        let path = format!("/v1/repositories/{wrong}/git/info/refs?service=git-upload-pack");
        let mut socket = std::net::TcpStream::connect(address).unwrap();
        write!(socket, "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nAuthorization: Basic {credential}\r\n\r\n").unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 404"));
        assert!(!response.contains(READ_TOKEN));
        server.stop();

        let expired_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(1);
        let expired_grant = AuthorizationGrant::from_token(
            READ_TOKEN,
            fixture.repository_id,
            AuthorizationScope::Read,
            [AuthorizationAudience::WorkspaceGit],
            expired_at,
        )
        .unwrap();
        let expired_verifier = Arc::new(
            AuthorizationVerifier::from_policy(
                AuthorizationPolicy::new(vec![expired_grant]).unwrap(),
            )
            .unwrap(),
        );
        let expired_server = RunningGateway::start(&fixture, expired_verifier);
        let expired_path = format!(
            "/v1/repositories/{}/git/info/refs?service=git-upload-pack",
            fixture.repository_id
        );
        let response = http_get(
            &expired_server.socket_address,
            &expired_path,
            &format!("Basic {credential}"),
        );
        assert!(response.starts_with("HTTP/1.1 401"));
        expired_server.stop();
    }

    #[test]
    fn hub_and_git_share_repository_grants_and_revocation_state() {
        let fixture = Fixture::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let grant = AuthorizationGrant::from_token(
            READ_TOKEN,
            fixture.repository_id,
            AuthorizationScope::Read,
            [
                AuthorizationAudience::SemanticApi,
                AuthorizationAudience::WorkspaceGit,
            ],
            now + 300,
        )
        .unwrap();
        let verifier = Arc::new(
            AuthorizationVerifier::from_policy(AuthorizationPolicy::new(vec![grant]).unwrap())
                .unwrap(),
        );
        let git_server = RunningGateway::start(&fixture, Arc::clone(&verifier));

        let hub_root = tempfile::tempdir().unwrap();
        let hub = crate::repository::hub::TrustedHubSemanticTransport::provision(
            hub_root.path(),
            fixture.binding,
            &fixture.backend,
        )
        .unwrap();
        let hub_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let hub_address = hub_listener.local_addr().unwrap();
        let hub_server = Arc::new(Server::from_listener(hub_listener, None).unwrap());
        let hub_service = Arc::new(crate::repository::http::HubHttpService::new(
            hub,
            Arc::clone(&verifier),
        ));
        let hub_serving_server = Arc::clone(&hub_server);
        let hub_thread = thread::spawn(move || hub_service.serve(&hub_serving_server));

        let git_path = format!(
            "/v1/repositories/{}/git/info/refs?service=git-upload-pack",
            fixture.repository_id
        );
        let git_auth = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("kat:{READ_TOKEN}"))
        );
        assert!(
            http_get(&git_server.socket_address, &git_path, &git_auth).starts_with("HTTP/1.1 200")
        );

        let hub_path = format!("/v1/repositories/{}/references", fixture.repository_id);
        let hub_auth = format!("Bearer {READ_TOKEN}");
        let response = http_get(&hub_address.to_string(), &hub_path, &hub_auth);
        assert!(response.starts_with("HTTP/1.1 200"));

        let revoked_policy = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            revoked_policy.path(),
            serde_json::to_vec(&AuthorizationPolicy::new(Vec::new()).unwrap()).unwrap(),
        )
        .unwrap();
        verifier.reload(revoked_policy.path()).unwrap();
        assert!(
            http_get(&git_server.socket_address, &git_path, &git_auth).starts_with("HTTP/1.1 401")
        );
        assert!(
            http_get(&hub_address.to_string(), &hub_path, &hub_auth).starts_with("HTTP/1.1 401")
        );

        hub_server.unblock();
        hub_thread.join().unwrap();
        git_server.stop();
    }

    #[test]
    fn route_and_request_validation_rejects_alternate_services_and_oversized_packs() {
        let repository_id = RepositoryId::new();
        assert!(
            Route::parse(
                &Method::Get,
                &format!(
                    "/v1/repositories/{repository_id}/git/info/refs?service=git-upload-pack&x=1"
                ),
                repository_id
            )
            .is_none()
        );
        assert!(
            Route::parse(
                &Method::Post,
                &format!("/v1/repositories/{repository_id}/git/objects/loose"),
                repository_id
            )
            .is_none()
        );
        assert!(request_body_too_large(Some(MAX_PACK_BYTES + 1), 0));
        assert!(request_body_too_large(None, MAX_PACK_BYTES + 1));
    }

    #[test]
    fn gateway_open_rejects_a_different_backend_binding() {
        let fixture = Fixture::new();
        let wrong_binding = GitWorkspaceBinding {
            workspace_backend_id: WorkspaceBackendId::new(),
            ..fixture.binding
        };
        assert!(
            GitHttpGateway::open(
                &fixture.backend,
                wrong_binding,
                fixture.verifier(AuthorizationScope::Publish)
            )
            .is_err()
        );
    }
}
