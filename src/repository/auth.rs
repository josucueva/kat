//! Shared repository-scoped authorization for the Hub API and workspace Git gateway.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tiny_http::Header;

use crate::domain::identity::RepositoryId;

const POLICY_VERSION: u32 = 1;
const MIN_TOKEN_LEN: usize = 43;

/// Service boundary that a capability is allowed to reach.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthorizationAudience {
    /// The KAT Hub semantic HTTP API.
    SemanticApi,
    /// The paired Git smart HTTP gateway.
    WorkspaceGit,
}

/// Capabilities assigned to one opaque repository token.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthorizationScope {
    /// Discover references and download immutable content.
    Read,
    /// Read, upload immutable content, and update named refs through CAS.
    Publish,
}

/// A request operation whose permission is checked at the service boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorizationAction {
    /// Discover named references and unnamed tips.
    DiscoverReferences,
    /// Download canonical bytes.
    FetchObject,
    /// Upload canonical bytes.
    StoreObject,
    /// Move one named KAT reference through compare-and-swap.
    CompareAndSwapReference,
    /// Read physical Git snapshots through `git-upload-pack`.
    GitFetch,
    /// Publish physical Git snapshots through `git-receive-pack`.
    GitPush,
}

impl AuthorizationAction {
    fn audience(self) -> AuthorizationAudience {
        match self {
            Self::DiscoverReferences
            | Self::FetchObject
            | Self::StoreObject
            | Self::CompareAndSwapReference => AuthorizationAudience::SemanticApi,
            Self::GitFetch | Self::GitPush => AuthorizationAudience::WorkspaceGit,
        }
    }

    fn requires_publish(self) -> bool {
        matches!(
            self,
            Self::StoreObject | Self::CompareAndSwapReference | Self::GitPush
        )
    }
}

/// A persisted grant contains only a SHA-256 token verifier, never the token.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationGrant {
    token_sha256: String,
    repository_id: RepositoryId,
    scope: AuthorizationScope,
    audiences: BTreeSet<AuthorizationAudience>,
    expires_at_unix_seconds: u64,
}

impl AuthorizationGrant {
    /// Creates an operator-managed grant record without retaining the raw token.
    pub fn from_token(
        token: &str,
        repository_id: RepositoryId,
        scope: AuthorizationScope,
        audiences: impl IntoIterator<Item = AuthorizationAudience>,
        expires_at_unix_seconds: u64,
    ) -> Result<Self, AuthorizationError> {
        validate_token(token)?;
        let audiences = audiences.into_iter().collect::<BTreeSet<_>>();
        if audiences.is_empty() {
            return Err(AuthorizationError::InvalidPolicy);
        }
        Ok(Self {
            token_sha256: hex::encode(Sha256::digest(token.as_bytes())),
            repository_id,
            scope,
            audiences,
            expires_at_unix_seconds,
        })
    }

    fn digest(&self) -> Result<[u8; 32], AuthorizationError> {
        if self.token_sha256.len() != 64
            || !self
                .token_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(AuthorizationError::InvalidPolicy);
        }
        let decoded =
            hex::decode(&self.token_sha256).map_err(|_| AuthorizationError::InvalidPolicy)?;
        decoded
            .try_into()
            .map_err(|_| AuthorizationError::InvalidPolicy)
    }
}

/// Versioned, operator-managed authorization policy. It contains token hashes only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationPolicy {
    version: u32,
    grants: Vec<AuthorizationGrant>,
}

impl AuthorizationPolicy {
    /// Builds a policy from operator-provisioned grants.
    pub fn new(grants: Vec<AuthorizationGrant>) -> Result<Self, AuthorizationError> {
        let policy = Self {
            version: POLICY_VERSION,
            grants,
        };
        policy.validate()?;
        Ok(policy)
    }

    fn validate(&self) -> Result<(), AuthorizationError> {
        if self.version != POLICY_VERSION {
            return Err(AuthorizationError::InvalidPolicy);
        }
        let mut digests = BTreeSet::new();
        for grant in &self.grants {
            if grant.audiences.is_empty() || !digests.insert(grant.digest()?) {
                return Err(AuthorizationError::InvalidPolicy);
            }
        }
        Ok(())
    }
}

/// Shared fail-closed grant verifier for the semantic API and Git gateway.
pub struct AuthorizationVerifier {
    policy: RwLock<Option<AuthorizationPolicy>>,
}

impl AuthorizationVerifier {
    /// Opens and validates an operator-managed JSON policy file.
    pub fn open(path: &Path) -> Result<Self, AuthorizationError> {
        Ok(Self {
            policy: RwLock::new(Some(read_policy(path)?)),
        })
    }

    /// Creates a verifier from an already provisioned policy.
    pub fn from_policy(policy: AuthorizationPolicy) -> Result<Self, AuthorizationError> {
        policy.validate()?;
        Ok(Self {
            policy: RwLock::new(Some(policy)),
        })
    }

    /// Reloads the policy. A failed read or validation clears the active policy.
    pub fn reload(&self, path: &Path) -> Result<(), AuthorizationError> {
        match read_policy(path) {
            Ok(replacement) => {
                let mut active = self
                    .policy
                    .write()
                    .map_err(|_| AuthorizationError::PolicyUnavailable)?;
                *active = Some(replacement);
                Ok(())
            }
            Err(error) => {
                if let Ok(mut active) = self.policy.write() {
                    *active = None;
                }
                Err(error)
            }
        }
    }

    /// Checks token, repository, audience, scope, and expiry on one request.
    pub fn authorize(
        &self,
        token: &str,
        repository_id: RepositoryId,
        audience: AuthorizationAudience,
        action: AuthorizationAction,
    ) -> Result<(), AuthorizationError> {
        if !valid_token(token) || audience != action.audience() {
            return Err(AuthorizationError::Unauthenticated);
        }
        let digest = Sha256::digest(token.as_bytes());
        let policy = self
            .policy
            .read()
            .map_err(|_| AuthorizationError::PolicyUnavailable)?;
        let policy = policy
            .as_ref()
            .ok_or(AuthorizationError::PolicyUnavailable)?;

        let mut matching_grant = None;
        for grant in &policy.grants {
            let grant_digest = grant.digest()?;
            if bool::from(digest.as_slice().ct_eq(grant_digest.as_slice())) {
                matching_grant = Some(grant);
            }
        }
        let grant = matching_grant.ok_or(AuthorizationError::Unauthenticated)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if grant.expires_at_unix_seconds <= now || !grant.audiences.contains(&audience) {
            return Err(AuthorizationError::Unauthenticated);
        }
        if grant.repository_id != repository_id {
            return Err(AuthorizationError::RepositoryNotFound);
        }
        if action.requires_publish() && grant.scope != AuthorizationScope::Publish {
            return Err(AuthorizationError::Forbidden);
        }
        Ok(())
    }

    /// Extracts a Bearer token from an HTTP request and applies this verifier.
    pub fn authorize_bearer_headers(
        &self,
        headers: &[Header],
        repository_id: RepositoryId,
        action: AuthorizationAction,
    ) -> Result<(), AuthorizationError> {
        let token = bearer_token(headers)?;
        self.authorize(
            token,
            repository_id,
            AuthorizationAudience::SemanticApi,
            action,
        )
    }

    /// Extracts the Git smart HTTP `kat:<token>` Basic credential and applies
    /// the shared workspace-Git grant policy.
    pub fn authorize_git_basic_headers(
        &self,
        headers: &[Header],
        repository_id: RepositoryId,
        action: AuthorizationAction,
    ) -> Result<(), AuthorizationError> {
        let token = basic_git_token(headers)?;
        self.authorize(
            &token,
            repository_id,
            AuthorizationAudience::WorkspaceGit,
            action,
        )
    }
}

/// Safe authorization failures. These values never contain a credential.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AuthorizationError {
    /// Missing, malformed, unknown, expired, revoked, or wrong-audience token.
    #[error("authentication required")]
    Unauthenticated,
    /// Valid read-only grant attempted a publish operation.
    #[error("permission denied")]
    Forbidden,
    /// The token is not bound to the requested repository.
    #[error("repository not found")]
    RepositoryNotFound,
    /// The policy file is unavailable or failed validation/reload.
    #[error("authorization policy unavailable")]
    PolicyUnavailable,
    /// The operator-managed policy is malformed or contains duplicate tokens.
    #[error("authorization policy invalid")]
    InvalidPolicy,
}

impl AuthorizationError {
    /// HTTP status defined by the Phase 12.5 contract.
    pub const fn status_code(self) -> u16 {
        match self {
            Self::Unauthenticated => 401,
            Self::Forbidden => 403,
            Self::RepositoryNotFound => 404,
            Self::PolicyUnavailable => 503,
            Self::InvalidPolicy => 500,
        }
    }
}

fn read_policy(path: &Path) -> Result<AuthorizationPolicy, AuthorizationError> {
    let bytes = fs::read(path).map_err(|_| AuthorizationError::PolicyUnavailable)?;
    let policy: AuthorizationPolicy =
        serde_json::from_slice(&bytes).map_err(|_| AuthorizationError::InvalidPolicy)?;
    policy.validate()?;
    Ok(policy)
}

fn bearer_token(headers: &[Header]) -> Result<&str, AuthorizationError> {
    let mut authorization = headers
        .iter()
        .filter(|header| header.field.equiv("Authorization"));
    let value = authorization
        .next()
        .ok_or(AuthorizationError::Unauthenticated)?
        .value
        .as_str();
    if authorization.next().is_some() {
        return Err(AuthorizationError::Unauthenticated);
    }
    let mut parts = value.split_ascii_whitespace();
    let scheme = parts.next().ok_or(AuthorizationError::Unauthenticated)?;
    let token = parts.next().ok_or(AuthorizationError::Unauthenticated)?;
    if !scheme.eq_ignore_ascii_case("Bearer") || parts.next().is_some() || !valid_token(token) {
        return Err(AuthorizationError::Unauthenticated);
    }
    Ok(token)
}

fn basic_git_token(headers: &[Header]) -> Result<String, AuthorizationError> {
    let mut authorization = headers
        .iter()
        .filter(|header| header.field.equiv("Authorization"));
    let value = authorization
        .next()
        .ok_or(AuthorizationError::Unauthenticated)?
        .value
        .as_str();
    if authorization.next().is_some() {
        return Err(AuthorizationError::Unauthenticated);
    }
    let mut parts = value.split_ascii_whitespace();
    if !parts
        .next()
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("Basic"))
    {
        return Err(AuthorizationError::Unauthenticated);
    }
    let encoded = parts.next().ok_or(AuthorizationError::Unauthenticated)?;
    if parts.next().is_some() {
        return Err(AuthorizationError::Unauthenticated);
    }
    let credentials = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| AuthorizationError::Unauthenticated)?;
    let credentials =
        std::str::from_utf8(&credentials).map_err(|_| AuthorizationError::Unauthenticated)?;
    let (username, token) = credentials
        .split_once(':')
        .ok_or(AuthorizationError::Unauthenticated)?;
    if username != "kat" || !valid_token(token) {
        return Err(AuthorizationError::Unauthenticated);
    }
    Ok(token.to_owned())
}

fn validate_token(token: &str) -> Result<(), AuthorizationError> {
    if valid_token(token) {
        Ok(())
    } else {
        Err(AuthorizationError::Unauthenticated)
    }
}

pub(crate) fn valid_token(token: &str) -> bool {
    token.len() >= MIN_TOKEN_LEN
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    use super::*;

    const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn grant(
        repository_id: RepositoryId,
        scope: AuthorizationScope,
        expiry: u64,
    ) -> AuthorizationGrant {
        AuthorizationGrant::from_token(
            TOKEN,
            repository_id,
            scope,
            [AuthorizationAudience::SemanticApi],
            expiry,
        )
        .unwrap()
    }

    fn write_policy(path: &Path, policy: &AuthorizationPolicy) {
        fs::write(path, serde_json::to_vec(policy).unwrap()).unwrap();
    }

    fn http_status(
        verifier: AuthorizationVerifier,
        repository_id: RepositoryId,
        authorization: Option<&str>,
        action: AuthorizationAction,
    ) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = tiny_http::Server::from_listener(listener, None).unwrap();
        let server_thread = thread::spawn(move || {
            let request = server.recv().unwrap();
            let status = verifier
                .authorize_bearer_headers(request.headers(), repository_id, action)
                .map_or_else(AuthorizationError::status_code, |_| 204);
            request
                .respond(tiny_http::Response::empty(tiny_http::StatusCode(status)))
                .unwrap();
        });

        let mut client = TcpStream::connect(address).unwrap();
        write!(client, "GET /v1/repositories/{repository_id}/references HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n").unwrap();
        if let Some(authorization) = authorization {
            write!(client, "Authorization: {authorization}\r\n").unwrap();
        }
        write!(client, "\r\n").unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        server_thread.join().unwrap();
        response
            .lines()
            .next()
            .and_then(|line| line.split_ascii_whitespace().nth(1))
            .and_then(|status| status.parse().ok())
            .unwrap()
    }

    #[test]
    fn authorization_is_enforced_on_real_http_requests() {
        let repository_id = RepositoryId::new();
        let valid = AuthorizationVerifier::from_policy(
            AuthorizationPolicy::new(vec![grant(
                repository_id,
                AuthorizationScope::Read,
                u64::MAX,
            )])
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            http_status(
                AuthorizationVerifier::from_policy(
                    AuthorizationPolicy::new(vec![grant(
                        repository_id,
                        AuthorizationScope::Read,
                        u64::MAX,
                    )])
                    .unwrap(),
                )
                .unwrap(),
                repository_id,
                None,
                AuthorizationAction::DiscoverReferences,
            ),
            401
        );
        assert_eq!(
            http_status(
                AuthorizationVerifier::from_policy(
                    AuthorizationPolicy::new(vec![grant(
                        repository_id,
                        AuthorizationScope::Read,
                        0,
                    )])
                    .unwrap(),
                )
                .unwrap(),
                repository_id,
                Some(&format!("Bearer {TOKEN}")),
                AuthorizationAction::DiscoverReferences,
            ),
            401
        );
        assert_eq!(
            http_status(
                AuthorizationVerifier::from_policy(
                    AuthorizationPolicy::new(vec![grant(
                        repository_id,
                        AuthorizationScope::Read,
                        u64::MAX,
                    )])
                    .unwrap(),
                )
                .unwrap(),
                RepositoryId::new(),
                Some(&format!("Bearer {TOKEN}")),
                AuthorizationAction::DiscoverReferences,
            ),
            404
        );
        assert_eq!(
            http_status(
                valid,
                repository_id,
                Some(&format!("Bearer {TOKEN}")),
                AuthorizationAction::StoreObject,
            ),
            403
        );
        let publish = AuthorizationVerifier::from_policy(
            AuthorizationPolicy::new(vec![grant(
                repository_id,
                AuthorizationScope::Publish,
                u64::MAX,
            )])
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            http_status(
                publish,
                repository_id,
                Some(&format!("bEaReR {TOKEN}")),
                AuthorizationAction::FetchObject,
            ),
            204
        );
    }

    #[test]
    fn failed_policy_reload_disables_all_grants_until_repaired() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("grants.json");
        let repository_id = RepositoryId::new();
        let initial = AuthorizationPolicy::new(vec![grant(
            repository_id,
            AuthorizationScope::Read,
            u64::MAX,
        )])
        .unwrap();
        write_policy(&path, &initial);
        let verifier = AuthorizationVerifier::open(&path).unwrap();

        fs::write(&path, b"invalid policy").unwrap();
        assert_eq!(
            verifier.reload(&path),
            Err(AuthorizationError::InvalidPolicy)
        );
        assert_eq!(
            verifier.authorize(
                TOKEN,
                repository_id,
                AuthorizationAudience::SemanticApi,
                AuthorizationAction::FetchObject,
            ),
            Err(AuthorizationError::PolicyUnavailable)
        );

        write_policy(&path, &initial);
        verifier.reload(&path).unwrap();
        assert_eq!(
            verifier.authorize(
                TOKEN,
                repository_id,
                AuthorizationAudience::SemanticApi,
                AuthorizationAction::FetchObject,
            ),
            Ok(())
        );

        write_policy(&path, &AuthorizationPolicy::new(vec![]).unwrap());
        verifier.reload(&path).unwrap();
        assert_eq!(
            verifier.authorize(
                TOKEN,
                repository_id,
                AuthorizationAudience::SemanticApi,
                AuthorizationAction::FetchObject,
            ),
            Err(AuthorizationError::Unauthenticated)
        );
    }

    #[test]
    fn one_grant_enforces_audience_and_publish_scope_on_both_boundaries() {
        let repository_id = RepositoryId::new();
        let verifier = AuthorizationVerifier::from_policy(
            AuthorizationPolicy::new(vec![
                AuthorizationGrant::from_token(
                    TOKEN,
                    repository_id,
                    AuthorizationScope::Read,
                    [
                        AuthorizationAudience::SemanticApi,
                        AuthorizationAudience::WorkspaceGit,
                    ],
                    u64::MAX,
                )
                .unwrap(),
            ])
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            verifier.authorize(
                TOKEN,
                repository_id,
                AuthorizationAudience::WorkspaceGit,
                AuthorizationAction::GitFetch,
            ),
            Ok(())
        );
        assert_eq!(
            verifier.authorize(
                TOKEN,
                repository_id,
                AuthorizationAudience::WorkspaceGit,
                AuthorizationAction::GitPush,
            ),
            Err(AuthorizationError::Forbidden)
        );
        assert_eq!(
            verifier.authorize(
                TOKEN,
                repository_id,
                AuthorizationAudience::WorkspaceGit,
                AuthorizationAction::FetchObject,
            ),
            Err(AuthorizationError::Unauthenticated)
        );
    }

    #[test]
    fn persisted_policy_contains_only_token_hashes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("grants.json");
        let policy = AuthorizationPolicy::new(vec![grant(
            RepositoryId::new(),
            AuthorizationScope::Read,
            u64::MAX,
        )])
        .unwrap();
        write_policy(&path, &policy);
        let policy = fs::read_to_string(path).unwrap();
        assert!(!policy.contains(TOKEN));
        assert!(policy.contains("token_sha256"));
    }
}
