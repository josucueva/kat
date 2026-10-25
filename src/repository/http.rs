//! Authenticated HTTPS adapter for the trusted file-backed Hub.

use std::io::Read;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tiny_http::{Header, Method as ServerMethod, Request, Response, Server, StatusCode};

use crate::domain::identity::{ObjectId, RepositoryId, RepositoryRevisionId};
use crate::domain::reference::NamedReference;
use crate::encoding::decode::decode_canonical;
use crate::encoding::hash::object_id;
use crate::repository::auth::{
    AuthorizationAction, AuthorizationError, AuthorizationVerifier, valid_token,
};
use crate::repository::hub::TrustedHubSemanticTransport;
use crate::repository::remote::{RemoteReferences, RemoteTransportError, SemanticTransport};

const API_VERSION: u32 = 1;
const MAX_OBJECT_BYTES: usize = 64 * 1024 * 1024;
const MAX_CAS_BYTES: usize = 64 * 1024;
const MAX_JSON_RESPONSE_BYTES: u64 = 1024 * 1024;
// shortcut: one shared process window; add proxy-side per-client quotas for larger deployments.
const MAX_REQUESTS_PER_SECOND: usize = 100;

/// Authenticated HTTP boundary for one provisioned Hub repository.
///
/// The caller owns the listener. Bind it only behind the trusted TLS proxy or
/// on its private side; this type does not provide TLS termination.
pub struct HubHttpService {
    hub: TrustedHubSemanticTransport,
    authorization: Arc<AuthorizationVerifier>,
    request_window: Mutex<(Instant, usize)>,
}

impl HubHttpService {
    /// Creates the API with the shared verifier used by the paired Git gateway.
    ///
    /// Sharing the `Arc` keeps grant reload and revocation consistent across both
    /// service boundaries.
    pub fn new(
        hub: TrustedHubSemanticTransport,
        authorization: Arc<AuthorizationVerifier>,
    ) -> Self {
        Self {
            hub,
            authorization,
            request_window: Mutex::new((Instant::now(), 0)),
        }
    }

    /// Handles requests from a caller-owned `tiny_http` listener.
    pub fn serve(&self, server: &Server) {
        for request in server.incoming_requests() {
            self.handle(request);
        }
    }

    fn handle(&self, mut request: Request) {
        if !self.allow_request() {
            respond_error(request, 429, "rate-limited");
            return;
        }

        let Some(route) = Route::parse(request.method(), request.url()) else {
            respond_error(request, 404, "not-found");
            return;
        };
        let repository_id = route.repository_id();
        let action = route.action();
        if let Err(error) =
            self.authorization
                .authorize_bearer_headers(request.headers(), repository_id, action)
        {
            respond_error(request, error.status_code(), authorization_code(error));
            return;
        }

        let response = match route {
            Route::References { repository_id } => self.list_references(repository_id),
            Route::FetchObject { repository_id, id } => self.fetch_object(repository_id, id),
            Route::StoreObject { repository_id, id } => {
                self.store_object(repository_id, id, &mut request)
            }
            Route::CompareAndSwap { repository_id } => {
                self.compare_and_swap(repository_id, &mut request)
            }
        };
        match response {
            Ok((status, content_type, body)) => respond(request, status, content_type, body),
            Err(error) => respond_error(request, error.status, error.code),
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

    fn list_references(
        &self,
        repository_id: RepositoryId,
    ) -> Result<(u16, &'static str, Vec<u8>), ServiceError> {
        let references = self
            .hub
            .list_references(repository_id)
            .map_err(ServiceError::from_transport)?;
        let body = serde_json::to_vec(&ReferencesResponse::from(references))
            .map_err(|_| ServiceError::internal())?;
        if body.len() > MAX_JSON_RESPONSE_BYTES as usize {
            return Err(ServiceError::new(413, "response-too-large"));
        }
        Ok((200, "application/json", body))
    }

    fn fetch_object(
        &self,
        repository_id: RepositoryId,
        id: ObjectId,
    ) -> Result<(u16, &'static str, Vec<u8>), ServiceError> {
        match self
            .hub
            .fetch_object(repository_id, id)
            .map_err(ServiceError::from_transport)?
        {
            Some(bytes) if bytes.len() <= MAX_OBJECT_BYTES => {
                Ok((200, "application/octet-stream", bytes))
            }
            Some(_) => Err(ServiceError::new(413, "response-too-large")),
            None => Err(ServiceError::new(404, "not-found")),
        }
    }

    fn store_object(
        &self,
        repository_id: RepositoryId,
        id: ObjectId,
        request: &mut Request,
    ) -> Result<(u16, &'static str, Vec<u8>), ServiceError> {
        require_content_type(request, "application/octet-stream")?;
        let bytes = read_request_body(request, MAX_OBJECT_BYTES)?;
        if object_id(&bytes) != id || decode_canonical(&bytes).is_err() {
            return Err(ServiceError::new(422, "invalid-object"));
        }
        self.hub
            .store_object(repository_id, id, &bytes)
            .map_err(ServiceError::from_transport)?;
        Ok((204, "application/json", Vec::new()))
    }

    fn compare_and_swap(
        &self,
        repository_id: RepositoryId,
        request: &mut Request,
    ) -> Result<(u16, &'static str, Vec<u8>), ServiceError> {
        require_content_type(request, "application/json")?;
        let body = read_request_body(request, MAX_CAS_BYTES)?;
        let cas: CasRequest =
            serde_json::from_slice(&body).map_err(|_| ServiceError::new(422, "invalid-request"))?;
        self.hub
            .compare_and_swap_reference(repository_id, &cas.name, cas.expected, cas.new)
            .map_err(ServiceError::from_transport)?;
        Ok((204, "application/json", Vec::new()))
    }
}

/// HTTPS client implementing the frozen semantic transport interface.
///
/// The bearer capability is held only in memory. Endpoint URLs must be HTTPS;
/// the only HTTP exception is an internal literal-loopback test constructor.
pub struct HttpSemanticTransport {
    base_url: String,
    token: String,
    agent: ureq::Agent,
}

impl HttpSemanticTransport {
    /// Validates a remote Hub URL without requiring a runtime credential.
    pub fn validate_endpoint(base_url: &str) -> Result<(), RemoteTransportError> {
        validate_base_url(base_url, false)
    }

    /// Creates an HTTPS client with a runtime bearer capability.
    pub fn new(base_url: &str, token: &str) -> Result<Self, RemoteTransportError> {
        Self::build(base_url, token, false)
    }

    fn build(
        base_url: &str,
        token: &str,
        allow_loopback_http: bool,
    ) -> Result<Self, RemoteTransportError> {
        validate_base_url(base_url, allow_loopback_http)?;
        if !valid_token(token) {
            return Err(configuration_error());
        }
        let https_only = !allow_loopback_http;
        let mut builder = ureq::Agent::config_builder()
            .https_only(https_only)
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(30)));
        if let Some(ca_file) = std::env::var_os("KAT_REMOTE_CA_FILE") {
            let pem = std::fs::read(ca_file).map_err(|_| configuration_error())?;
            let certificate =
                ureq::tls::Certificate::from_pem(&pem).map_err(|_| configuration_error())?;
            let tls = ureq::tls::TlsConfig::builder()
                .root_certs(vec![certificate].into())
                .build();
            builder = builder.tls_config(tls);
        }
        let agent = builder.build().new_agent();
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            token: token.to_owned(),
            agent,
        })
    }

    fn request_url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    fn authorization_header(&self) -> String {
        format!("Bearer {}", self.token)
    }

    fn add_request_headers<T>(&self, request: ureq::RequestBuilder<T>) -> ureq::RequestBuilder<T> {
        request
            .header("Authorization", &self.authorization_header())
            .header("Accept-Encoding", "identity")
            .header("Connection", "close")
    }

    fn check_response(
        response: ureq::http::Response<ureq::Body>,
        expected_status: u16,
    ) -> Result<ureq::http::Response<ureq::Body>, RemoteTransportError> {
        if response.status().as_u16() == expected_status {
            Ok(response)
        } else {
            Err(transport_status(response.status().as_u16()))
        }
    }

    fn repository_path(repository_id: RepositoryId) -> String {
        format!("/v1/repositories/{repository_id}")
    }
}

impl SemanticTransport for HttpSemanticTransport {
    fn list_references(
        &self,
        repository_id: RepositoryId,
    ) -> Result<RemoteReferences, RemoteTransportError> {
        let path = format!("{}/references", Self::repository_path(repository_id));
        let mut response = self
            .add_request_headers(self.agent.get(&self.request_url(&path)))
            .call()
            .map_err(|_| RemoteTransportError::Unavailable)?;
        if response.status().as_u16() != 200 {
            return Err(transport_status(response.status().as_u16()));
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(MAX_JSON_RESPONSE_BYTES)
            .read_to_vec()
            .map_err(|_| RemoteTransportError::Unavailable)?;
        let decoded: ReferencesResponse =
            serde_json::from_slice(&bytes).map_err(|_| invalid_response())?;
        if decoded.version != API_VERSION {
            return Err(invalid_response());
        }
        Ok(decoded.into())
    }

    fn fetch_object(
        &self,
        repository_id: RepositoryId,
        id: ObjectId,
    ) -> Result<Option<Vec<u8>>, RemoteTransportError> {
        let path = format!("{}/objects/{id}", Self::repository_path(repository_id));
        let mut response = self
            .add_request_headers(self.agent.get(&self.request_url(&path)))
            .call()
            .map_err(|_| RemoteTransportError::Unavailable)?;
        match response.status().as_u16() {
            404 => return Ok(None),
            200 => {}
            status => return Err(transport_status(status)),
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(MAX_OBJECT_BYTES as u64)
            .read_to_vec()
            .map_err(|_| RemoteTransportError::Unavailable)?;
        if object_id(&bytes) != id || decode_canonical(&bytes).is_err() {
            return Err(RemoteTransportError::ObjectIntegrity(id));
        }
        Ok(Some(bytes))
    }

    fn store_object(
        &self,
        repository_id: RepositoryId,
        id: ObjectId,
        bytes: &[u8],
    ) -> Result<(), RemoteTransportError> {
        if bytes.len() > MAX_OBJECT_BYTES {
            return Err(RemoteTransportError::Other(
                "canonical object exceeds the remote API size limit".into(),
            ));
        }
        if object_id(bytes) != id || decode_canonical(bytes).is_err() {
            return Err(RemoteTransportError::ObjectIntegrity(id));
        }
        let path = format!("{}/objects/{id}", Self::repository_path(repository_id));
        let response = self
            .add_request_headers(self.agent.put(&self.request_url(&path)))
            .header("Content-Type", "application/octet-stream")
            .send(bytes)
            .map_err(|_| RemoteTransportError::Unavailable)?;
        Self::check_response(response, 204).map(|_| ())
    }

    fn compare_and_swap_reference(
        &self,
        repository_id: RepositoryId,
        name: &str,
        expected: Option<RepositoryRevisionId>,
        new: RepositoryRevisionId,
    ) -> Result<(), RemoteTransportError> {
        let path = format!("{}/reference-cas", Self::repository_path(repository_id));
        let body = CasRequest {
            name: name.to_owned(),
            expected,
            new,
        };
        let body = serde_json::to_vec(&body)
            .map_err(|_| RemoteTransportError::Other("invalid CAS request".into()))?;
        let response = self
            .add_request_headers(self.agent.put(&self.request_url(&path)))
            .header("Content-Type", "application/json")
            .send(body)
            .map_err(|_| RemoteTransportError::Unavailable)?;
        Self::check_response(response, 204).map(|_| ())
    }
}

#[derive(Clone, Copy)]
enum Route {
    References {
        repository_id: RepositoryId,
    },
    FetchObject {
        repository_id: RepositoryId,
        id: ObjectId,
    },
    StoreObject {
        repository_id: RepositoryId,
        id: ObjectId,
    },
    CompareAndSwap {
        repository_id: RepositoryId,
    },
}

impl Route {
    fn parse(method: &ServerMethod, url: &str) -> Option<Self> {
        if url.contains(['?', '#']) {
            return None;
        }
        let mut parts = url.split('/');
        if !parts.next()?.is_empty() || parts.next()? != "v1" || parts.next()? != "repositories" {
            return None;
        }
        let repository_id = RepositoryId::from_str(parts.next()?).ok()?;
        match (method, parts.next()?, parts.next()) {
            (&ServerMethod::Get, "references", None) => Some(Self::References { repository_id }),
            (&ServerMethod::Get, "objects", Some(id)) if parts.next().is_none() => {
                Some(Self::FetchObject {
                    repository_id,
                    id: ObjectId::from_str(id).ok()?,
                })
            }
            (&ServerMethod::Put, "objects", Some(id)) if parts.next().is_none() => {
                Some(Self::StoreObject {
                    repository_id,
                    id: ObjectId::from_str(id).ok()?,
                })
            }
            (&ServerMethod::Put, "reference-cas", None) => {
                Some(Self::CompareAndSwap { repository_id })
            }
            _ => None,
        }
    }

    fn repository_id(self) -> RepositoryId {
        match self {
            Self::References { repository_id }
            | Self::FetchObject { repository_id, .. }
            | Self::StoreObject { repository_id, .. }
            | Self::CompareAndSwap { repository_id } => repository_id,
        }
    }

    fn action(self) -> AuthorizationAction {
        match self {
            Self::References { .. } => AuthorizationAction::DiscoverReferences,
            Self::FetchObject { .. } => AuthorizationAction::FetchObject,
            Self::StoreObject { .. } => AuthorizationAction::StoreObject,
            Self::CompareAndSwap { .. } => AuthorizationAction::CompareAndSwapReference,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferencesResponse {
    version: u32,
    named: Vec<ReferenceResponse>,
    unnamed_heads: Vec<RepositoryRevisionId>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceResponse {
    name: String,
    target: RepositoryRevisionId,
}

impl From<RemoteReferences> for ReferencesResponse {
    fn from(references: RemoteReferences) -> Self {
        Self {
            version: API_VERSION,
            named: references
                .named
                .into_iter()
                .map(|reference| ReferenceResponse {
                    name: reference.name,
                    target: reference.target,
                })
                .collect(),
            unnamed_heads: references.unnamed_heads,
        }
    }
}

impl From<ReferencesResponse> for RemoteReferences {
    fn from(references: ReferencesResponse) -> Self {
        Self {
            named: references
                .named
                .into_iter()
                .map(|reference| NamedReference {
                    name: reference.name,
                    target: reference.target,
                })
                .collect(),
            unnamed_heads: references.unnamed_heads,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CasRequest {
    name: String,
    expected: Option<RepositoryRevisionId>,
    new: RepositoryRevisionId,
}

struct ServiceError {
    status: u16,
    code: &'static str,
}

impl ServiceError {
    const fn new(status: u16, code: &'static str) -> Self {
        Self { status, code }
    }

    const fn internal() -> Self {
        Self::new(500, "internal-error")
    }

    fn from_transport(error: RemoteTransportError) -> Self {
        match error {
            RemoteTransportError::Unavailable => Self::new(503, "service-unavailable"),
            RemoteTransportError::RepositoryMismatch { .. } => Self::new(404, "not-found"),
            RemoteTransportError::ObjectIntegrity(_) => Self::new(422, "invalid-object"),
            RemoteTransportError::SnapshotIntegrity(_) => {
                Self::new(422, "invalid-workspace-snapshot")
            }
            RemoteTransportError::InvalidData => Self::new(422, "invalid-data"),
            RemoteTransportError::ReferenceConflict => Self::new(409, "reference-conflict"),
            RemoteTransportError::Other(_) => Self::internal(),
        }
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    version: u32,
    error: &'static str,
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

fn require_content_type(request: &Request, expected: &str) -> Result<(), ServiceError> {
    let mut values = request
        .headers()
        .iter()
        .filter(|header| header.field.equiv("Content-Type"));
    let Some(value) = values.next() else {
        return Err(ServiceError::new(415, "unsupported-media-type"));
    };
    if values.next().is_some()
        || value
            .value
            .as_str()
            .split(';')
            .next()
            .is_none_or(|actual| !actual.trim().eq_ignore_ascii_case(expected))
    {
        return Err(ServiceError::new(415, "unsupported-media-type"));
    }
    Ok(())
}

fn read_request_body(request: &mut Request, limit: usize) -> Result<Vec<u8>, ServiceError> {
    if request_body_too_large(request.body_length(), 0, limit) {
        return Err(ServiceError::new(413, "request-too-large"));
    }
    let mut bytes = Vec::new();
    request
        .as_reader()
        .take((limit as u64) + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ServiceError::new(400, "invalid-request"))?;
    if request_body_too_large(None, bytes.len(), limit) {
        return Err(ServiceError::new(413, "request-too-large"));
    }
    Ok(bytes)
}

fn request_body_too_large(
    declared_length: Option<usize>,
    read_length: usize,
    limit: usize,
) -> bool {
    declared_length.is_some_and(|length| length > limit) || read_length > limit
}

fn respond_error(request: Request, status: u16, code: &'static str) {
    let body = serde_json::to_vec(&ErrorResponse {
        version: API_VERSION,
        error: code,
    })
    .unwrap_or_else(|_| b"{\"version\":1,\"error\":\"internal-error\"}".to_vec());
    respond(request, status, "application/json", body);
}

fn respond(request: Request, status: u16, content_type: &'static str, body: Vec<u8>) {
    let response = Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", content_type).unwrap());
    let _ = request.respond(response);
}

fn transport_status(status: u16) -> RemoteTransportError {
    match status {
        401 => RemoteTransportError::Other("remote authentication required".into()),
        403 => RemoteTransportError::Other("remote permission denied".into()),
        404 => RemoteTransportError::Other("remote resource not found".into()),
        409 => RemoteTransportError::ReferenceConflict,
        413 => RemoteTransportError::Other("remote API size limit exceeded".into()),
        422 => RemoteTransportError::Other("remote rejected invalid KAT data".into()),
        429 => RemoteTransportError::Unavailable,
        _ => RemoteTransportError::Unavailable,
    }
}

fn invalid_response() -> RemoteTransportError {
    RemoteTransportError::Other("remote returned an invalid response".into())
}

fn configuration_error() -> RemoteTransportError {
    RemoteTransportError::Other("invalid remote HTTP configuration".into())
}

fn validate_base_url(
    base_url: &str,
    allow_loopback_http: bool,
) -> Result<(), RemoteTransportError> {
    if base_url.is_empty()
        || base_url.contains(['?', '#', '\\'])
        || base_url
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return Err(configuration_error());
    }
    let (scheme, _) = base_url.split_once("://").ok_or_else(configuration_error)?;
    let uri: ureq::http::Uri = base_url.parse().map_err(|_| configuration_error())?;
    if uri.scheme_str() != Some(scheme) {
        return Err(configuration_error());
    }
    let loopback_http = allow_loopback_http && scheme == "http";
    if scheme != "https" && !loopback_http {
        return Err(configuration_error());
    }
    let authority = uri
        .authority()
        .map(|authority| authority.as_str())
        .unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return Err(configuration_error());
    }
    if loopback_http && !is_literal_loopback(authority) {
        return Err(configuration_error());
    }
    Ok(())
}

fn is_literal_loopback(authority: &str) -> bool {
    authority
        .parse::<SocketAddr>()
        .map(|address| address.ip().is_loopback())
        .or_else(|_| {
            authority
                .parse::<IpAddr>()
                .map(|address| address.is_loopback())
        })
        .unwrap_or(false)
}

#[cfg(all(test, unix))]
#[path = "authenticated_remote_tests.rs"]
mod authenticated_remote_tests;

#[cfg(test)]
impl HttpSemanticTransport {
    fn loopback(base_url: &str, token: &str) -> Result<Self, RemoteTransportError> {
        Self::build(base_url, token, true)
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::thread;

    use git2::{Repository as GitRepository, Signature};

    use super::*;
    use crate::domain::identity::{SemanticStateId, WorkspaceSnapshotId};
    use crate::domain::revision::RepositoryRevision;
    use crate::encoding::canonical_bytes;
    use crate::encoding::hash::hash_workspace_snapshot;
    use crate::encoding::object::{CanonicalObject, CanonicalPayload};
    use crate::repository::auth::{
        AuthorizationAudience, AuthorizationGrant, AuthorizationPolicy, AuthorizationScope,
    };
    use crate::repository::init::init_repository;
    use crate::repository::object_store::ObjectStore;
    use crate::repository::workspace::git_transport::{
        ControlledGitBackend, GitWorkspaceBinding, WorkspaceBackendId,
    };

    const READ_TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const PUBLISH_TOKEN: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    struct Fixture {
        _repo_dir: tempfile::TempDir,
        _backend_dir: tempfile::TempDir,
        _hub_dir: tempfile::TempDir,
        repository_id: RepositoryId,
        ontology: ObjectId,
        state: ObjectId,
        revision: RepositoryRevisionId,
        objects: ObjectStore,
        hub: TrustedHubSemanticTransport,
    }

    impl Fixture {
        fn new() -> Self {
            let repo_dir = tempfile::tempdir().unwrap();
            let initialized = init_repository(repo_dir.path()).unwrap();
            let objects = ObjectStore::new(repo_dir.path().join(".kat"));
            let snapshot = hash_workspace_snapshot(&[]);

            let backend_dir = tempfile::tempdir().unwrap();
            let backend_path = backend_dir.path().join("workspace.git");
            let binding = GitWorkspaceBinding {
                repository_id: initialized.repository_id,
                workspace_backend_id: WorkspaceBackendId::new(),
            };
            ControlledGitBackend::provision(&backend_path, binding).unwrap();
            add_snapshot(&backend_path, &snapshot);

            let hub_dir = tempfile::tempdir().unwrap();
            let hub_root = hub_dir.path().join("hub");
            let hub =
                TrustedHubSemanticTransport::provision(&hub_root, binding, &backend_path).unwrap();
            let revision_bytes = canonical_bytes(&CanonicalObject {
                payload: CanonicalPayload::RepositoryRevision(RepositoryRevision {
                    parents: vec![],
                    semantic_state: SemanticStateId::from_object_id(initialized.state),
                    workspace_snapshot: snapshot.clone(),
                    semantic_change: None,
                }),
            })
            .unwrap();
            let revision =
                RepositoryRevisionId::from_object_id(objects.put(&revision_bytes).unwrap());
            for id in [
                initialized.ontology,
                initialized.state,
                revision.as_object_id(),
            ] {
                hub.store_object(initialized.repository_id, id, &objects.get(id).unwrap())
                    .unwrap();
            }
            hub.compare_and_swap_reference(initialized.repository_id, "main", None, revision)
                .unwrap();

            Self {
                _repo_dir: repo_dir,
                _backend_dir: backend_dir,
                _hub_dir: hub_dir,
                repository_id: initialized.repository_id,
                ontology: initialized.ontology,
                state: initialized.state,
                revision,
                objects,
                hub,
            }
        }
    }

    fn verifier(repository_id: RepositoryId, scope: AuthorizationScope) -> AuthorizationVerifier {
        let grant = |token| {
            AuthorizationGrant::from_token(
                token,
                repository_id,
                scope,
                [AuthorizationAudience::SemanticApi],
                u64::MAX,
            )
            .unwrap()
        };
        AuthorizationVerifier::from_policy(
            AuthorizationPolicy::new(vec![grant(READ_TOKEN), grant(PUBLISH_TOKEN)]).unwrap(),
        )
        .unwrap()
    }

    fn start_service(
        hub: TrustedHubSemanticTransport,
        authorization: Arc<AuthorizationVerifier>,
    ) -> (String, Arc<Server>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = Arc::new(Server::from_listener(listener, None).unwrap());
        let service = HubHttpService::new(hub, authorization);
        let serving_server = server.clone();
        let thread = thread::spawn(move || service.serve(&serving_server));
        (format!("http://{address}"), server, thread)
    }

    fn add_snapshot(path: &std::path::Path, snapshot: &WorkspaceSnapshotId) {
        let repository = GitRepository::open_bare(path).unwrap();
        let tree_id = repository.treebuilder(None).unwrap().write().unwrap();
        let tree = repository.find_tree(tree_id).unwrap();
        let signature = Signature::now("KAT HTTP test", "kat@example.invalid").unwrap();
        let commit = repository
            .commit(None, &signature, &signature, "empty", &tree, &[])
            .unwrap();
        repository
            .reference(
                &format!("refs/kat/lineage/{}/{commit}", snapshot.to_hex()),
                commit,
                false,
                "test retention ref",
            )
            .unwrap();
    }

    #[test]
    fn authenticated_http_client_reads_and_publishes_through_hub() {
        let fixture = Fixture::new();
        let (url, server, serving) = start_service(
            fixture.hub,
            Arc::new(verifier(fixture.repository_id, AuthorizationScope::Publish)),
        );
        let client = HttpSemanticTransport::loopback(&url, PUBLISH_TOKEN).unwrap();

        let references = client.list_references(fixture.repository_id).unwrap();
        assert_eq!(references.named.len(), 1);
        assert_eq!(references.named[0].name, "main");
        assert_eq!(references.named[0].target, fixture.revision);
        assert_eq!(
            client
                .fetch_object(fixture.repository_id, fixture.state)
                .unwrap()
                .unwrap(),
            fixture.objects.get(fixture.state).unwrap()
        );

        let other_dir = tempfile::tempdir().unwrap();
        let other_repository = init_repository(other_dir.path()).unwrap();
        let other_objects = ObjectStore::new(other_dir.path().join(".kat"));
        let extra_id = other_repository.ontology;
        let extra_bytes = other_objects.get(extra_id).unwrap();
        client
            .store_object(fixture.repository_id, extra_id, &extra_bytes)
            .unwrap();
        assert_eq!(
            client
                .fetch_object(fixture.repository_id, extra_id)
                .unwrap()
                .unwrap(),
            extra_bytes
        );
        client
            .compare_and_swap_reference(fixture.repository_id, "copy/main", None, fixture.revision)
            .unwrap();
        assert!(matches!(
            client.compare_and_swap_reference(
                fixture.repository_id,
                "main",
                None,
                fixture.revision
            ),
            Err(RemoteTransportError::ReferenceConflict)
        ));
        assert_eq!(
            client
                .list_references(fixture.repository_id)
                .unwrap()
                .named
                .iter()
                .filter(|reference| reference.name == "copy/main")
                .count(),
            1
        );

        server.unblock();
        serving.join().unwrap();
    }

    #[test]
    fn read_grant_rejects_upload_and_does_not_move_hub_references() {
        let fixture = Fixture::new();
        let (url, server, serving) = start_service(
            fixture.hub,
            Arc::new(verifier(fixture.repository_id, AuthorizationScope::Read)),
        );
        let client = HttpSemanticTransport::loopback(&url, READ_TOKEN).unwrap();
        let before = client.list_references(fixture.repository_id).unwrap();
        assert!(client.list_references(RepositoryId::new()).is_err());
        assert!(
            client
                .store_object(
                    fixture.repository_id,
                    fixture.ontology,
                    &fixture.objects.get(fixture.ontology).unwrap(),
                )
                .is_err()
        );
        assert!(
            client
                .compare_and_swap_reference(
                    fixture.repository_id,
                    "copy/main",
                    None,
                    fixture.revision,
                )
                .is_err()
        );
        assert_eq!(
            client.list_references(fixture.repository_id).unwrap(),
            before
        );
        server.unblock();
        serving.join().unwrap();
    }

    #[test]
    fn service_rejects_missing_auth_invalid_objects_and_query_credentials() {
        use std::io::{Read as _, Write as _};

        let fixture = Fixture::new();
        let repository_id = fixture.repository_id;
        let (url, server, serving) = start_service(
            fixture.hub,
            Arc::new(verifier(repository_id, AuthorizationScope::Publish)),
        );
        let address = url.strip_prefix("http://").unwrap();
        let mut socket = std::net::TcpStream::connect(address).unwrap();
        write!(
            socket,
            "GET /v1/repositories/{repository_id}/references HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 401"));

        let client = HttpSemanticTransport::loopback(&url, PUBLISH_TOKEN).unwrap();
        let arbitrary = [0x15; 3];
        let invalid_id = object_id(&arbitrary);
        let mut invalid_object = std::net::TcpStream::connect(address).unwrap();
        write!(
            invalid_object,
            "PUT /v1/repositories/{repository_id}/objects/{invalid_id} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nAuthorization: Bearer {PUBLISH_TOKEN}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
            arbitrary.len()
        )
        .unwrap();
        invalid_object.write_all(&arbitrary).unwrap();
        let mut invalid_response = String::new();
        invalid_object
            .read_to_string(&mut invalid_response)
            .unwrap();
        assert!(invalid_response.starts_with("HTTP/1.1 422"));
        assert!(
            client
                .fetch_object(repository_id, invalid_id)
                .unwrap()
                .is_none()
        );

        let missing_revision =
            RepositoryRevisionId::from_object_id(ObjectId::from_bytes([0x42; 32]));
        let cas_body = serde_json::to_vec(&CasRequest {
            name: "unpublished".into(),
            expected: None,
            new: missing_revision,
        })
        .unwrap();
        let mut invalid_cas = std::net::TcpStream::connect(address).unwrap();
        write!(
            invalid_cas,
            "PUT /v1/repositories/{repository_id}/reference-cas HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nAuthorization: Bearer {PUBLISH_TOKEN}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            cas_body.len()
        )
        .unwrap();
        invalid_cas.write_all(&cas_body).unwrap();
        let mut cas_response = String::new();
        invalid_cas.read_to_string(&mut cas_response).unwrap();
        assert!(cas_response.starts_with("HTTP/1.1 422"));
        assert_eq!(
            client.list_references(repository_id).unwrap().named.len(),
            1
        );

        let mut query = std::net::TcpStream::connect(address).unwrap();
        write!(
            query,
            "GET /v1/repositories/{repository_id}/references?token={PUBLISH_TOKEN} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nAuthorization: Bearer {PUBLISH_TOKEN}\r\n\r\n"
        )
        .unwrap();
        let mut query_response = String::new();
        query.read_to_string(&mut query_response).unwrap();
        assert!(query_response.starts_with("HTTP/1.1 404"));
        assert!(!query_response.contains(PUBLISH_TOKEN));
        server.unblock();
        serving.join().unwrap();
    }

    #[test]
    fn client_requires_https_and_rejects_url_credentials() {
        assert!(HttpSemanticTransport::new("http://example.invalid", PUBLISH_TOKEN).is_err());
        assert!(HttpSemanticTransport::new("http://127.0.0.1:8123", PUBLISH_TOKEN).is_err());
        assert!(
            HttpSemanticTransport::new("https://kat:secret@example.invalid", PUBLISH_TOKEN)
                .is_err()
        );
        assert!(
            HttpSemanticTransport::new("https://example.invalid?token=x", PUBLISH_TOKEN).is_err()
        );
        assert!(HttpSemanticTransport::new("https://example.invalid", "too-short").is_err());
    }

    #[test]
    fn loopback_http_exception_requires_literal_loopback_ip() {
        assert!(validate_base_url("http://127.0.0.1:8123", true).is_ok());
        assert!(validate_base_url("http://[::1]:8123", true).is_ok());
        assert!(validate_base_url("http://localhost:8123", true).is_err());
        assert!(validate_base_url("http://192.0.2.1:8123", true).is_err());
    }

    #[test]
    fn request_rate_window_fails_closed_at_the_limit() {
        let fixture = Fixture::new();
        let service = HubHttpService::new(
            fixture.hub,
            Arc::new(verifier(fixture.repository_id, AuthorizationScope::Read)),
        );
        for _ in 0..MAX_REQUESTS_PER_SECOND {
            assert!(service.allow_request());
        }
        assert!(!service.allow_request());
    }

    #[test]
    fn declared_and_streamed_request_sizes_obey_the_same_limit() {
        assert!(!request_body_too_large(Some(10), 0, 10));
        assert!(request_body_too_large(Some(11), 0, 10));
        assert!(!request_body_too_large(None, 10, 10));
        assert!(request_body_too_large(None, 11, 10));
    }
}
