//! Draft change transaction session: `.kat/work/change/session.json`.
//!
//! Stores private, local, non-canonical state for open multi-operation change
//! transactions (`kat change begin/status/commit/abort`).
//!
//! NORMATIVE CONTRACT & FORMAT BOUNDARY:
//! `.kat/work/change/session.json` is strictly private, local, and non-canonical.
//! Future KAT versions may alter, extend, or replace its internal representation
//! without repository-format compatibility obligations. Canonical immutable objects
//! reside exclusively in `.kat/objects/` under deterministic SHA-256 addresses.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::domain::change::ChangeRevision;
use crate::domain::element::KnowledgeElementVersion;
use crate::domain::identity::{ChangeId, ObjectId};
use crate::domain::operation::Operation;
use crate::domain::relationship::RelationshipVersion;
use crate::domain::state::SemanticState;
use crate::encoding::cbor::canonical_bytes;
use crate::encoding::decode::decode_canonical;
use crate::encoding::object::{CanonicalObject, CanonicalPayload};
use crate::repository::open::Repository;
use crate::repository::ref_store::RefStore;

/// Supported draft session file format version (v1).
pub const DRAFT_SESSION_VERSION: u32 = 1;

/// Status of an open draft change session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftSessionState {
    /// Active open session ready for staging or commit.
    Open,
    /// Stale session whose base_state no longer matches refs/accepted.
    Stale,
}

impl DraftSessionState {
    pub const fn as_str(self) -> &'static str {
        match self {
            DraftSessionState::Open => "open",
            DraftSessionState::Stale => "stale",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "open" => Some(DraftSessionState::Open),
            "stale" => Some(DraftSessionState::Stale),
            _ => None,
        }
    }
}

/// Errors produced during draft session operations.
#[derive(Debug, thiserror::Error)]
pub enum DraftSessionError {
    /// A draft session is already open in the repository.
    #[error("a draft change transaction is already open at .kat/work/change/session.json")]
    AlreadyExists,
    /// No open draft session exists.
    #[error("no open draft change transaction found at .kat/work/change/session.json")]
    NotFound,
    /// Attempted to modify or commit a stale session.
    #[error(
        "draft session is stale because accepted head moved since begin; use 'kat change abort' to clear"
    )]
    StaleSession,
    /// An underlying filesystem failure.
    #[error("draft session I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Draft session file format or decoding error.
    #[error("invalid draft session file: {0}")]
    Invalid(String),
}

/// An open draft change transaction session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftSession {
    /// Format version.
    pub schema_version: u32,
    /// Session status (Open or Stale).
    pub status: DraftSessionState,
    /// Base accepted state ObjectId at `begin` time.
    pub base_state_id: ObjectId,
    /// Base accepted ChangeRevision ObjectId at `begin` time, if any.
    pub base_change_id: Option<ObjectId>,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// Change description.
    pub description: Option<String>,
    /// Staged operations in application order.
    pub operations: Vec<Operation>,
    /// Staged knowledge element versions.
    pub staged_element_versions: Vec<KnowledgeElementVersion>,
    /// Staged relationship versions.
    pub staged_relationship_versions: Vec<RelationshipVersion>,
    /// Candidate working state after applying all staged operations.
    pub working_state: SemanticState,
}

/// Returns the path to `.kat/work/change/`.
pub fn draft_session_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".kat").join("work").join("change")
}

/// Returns the path to `.kat/work/change/session.json`.
pub fn draft_session_path(repo_root: &Path) -> PathBuf {
    draft_session_dir(repo_root).join("session.json")
}

/// Returns true if an open draft session exists.
pub fn has_draft_session(repo_root: &Path) -> bool {
    draft_session_path(repo_root).exists()
}

/// Opens a new draft change session on the accepted state $S_n$.
pub fn begin_draft_session(
    repository: &Repository,
    description: Option<String>,
) -> Result<DraftSession, DraftSessionError> {
    let root = repository.root_dir();
    if has_draft_session(root) {
        return Err(DraftSessionError::AlreadyExists);
    }

    let accepted = repository
        .ref_store()
        .read_accepted()
        .map_err(|e| DraftSessionError::Invalid(format!("failed to read accepted ref: {e}")))?;
    let store = repository.object_store();

    // Load current accepted SemanticState
    let state_bytes = store.get(accepted.state).map_err(|e| {
        DraftSessionError::Invalid(format!("failed to load base accepted state: {e}"))
    })?;
    let canonical_state = decode_canonical(&state_bytes).map_err(|e| {
        DraftSessionError::Invalid(format!("failed to decode base accepted state: {e}"))
    })?;

    let working_state = match canonical_state.payload {
        CanonicalPayload::SemanticState(state) => state,
        _ => {
            return Err(DraftSessionError::Invalid(
                "base accepted state object is not a SemanticState".to_string(),
            ));
        }
    };

    let session = DraftSession {
        schema_version: DRAFT_SESSION_VERSION,
        status: DraftSessionState::Open,
        base_state_id: accepted.state,
        base_change_id: accepted.change,
        created_at: "2026-08-15T00:00:00Z".to_string(),
        description,
        operations: Vec::new(),
        staged_element_versions: Vec::new(),
        staged_relationship_versions: Vec::new(),
        working_state,
    };

    write_draft_session_atomic(root, &session)?;
    Ok(session)
}

/// Reads the open draft session from `.kat/work/change/session.json`.
pub fn read_draft_session(repo_root: &Path) -> Result<DraftSession, DraftSessionError> {
    let path = draft_session_path(repo_root);
    if !path.exists() {
        return Err(DraftSessionError::NotFound);
    }

    let content = fs::read_to_string(&path)?;
    parse_draft_session_json(&content)
}

/// Writes the draft session atomically to `.kat/work/change/session.json`.
pub fn write_draft_session_atomic(
    repo_root: &Path,
    session: &DraftSession,
) -> Result<(), DraftSessionError> {
    let dir = draft_session_dir(repo_root);
    fs::create_dir_all(&dir)?;

    let target_path = draft_session_path(repo_root);
    let tmp_path = dir.join("session.json.tmp");

    let json = format_draft_session_json(session)?;
    {
        let mut file = File::create(&tmp_path)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(tmp_path, target_path)?;
    Ok(())
}

/// Marks an active session as stale (e.g. after a CAS conflict).
pub fn mark_draft_session_stale(repo_root: &Path) -> Result<DraftSession, DraftSessionError> {
    let mut session = read_draft_session(repo_root)?;
    session.status = DraftSessionState::Stale;
    write_draft_session_atomic(repo_root, &session)?;
    Ok(session)
}

/// Aborts the open draft session, removing `.kat/work/change/session.json`.
pub fn abort_draft_session(repo_root: &Path) -> Result<(), DraftSessionError> {
    let path = draft_session_path(repo_root);
    if !path.exists() {
        return Err(DraftSessionError::NotFound);
    }
    fs::remove_file(path)?;
    Ok(())
}

fn format_draft_session_json(session: &DraftSession) -> Result<String, DraftSessionError> {
    let mut ops_hex = Vec::new();
    for op in &session.operations {
        let dummy_change = ChangeRevision {
            change_id: ChangeId::from_uuid(uuid::Uuid::nil()),
            result_state: ObjectId::from_bytes([0; 32]),
            base_states: vec![session.base_state_id],
            dependencies: Vec::new(),
            description: None,
            operations: vec![op.clone()],
        };
        let obj = CanonicalObject {
            payload: CanonicalPayload::ChangeRevision(dummy_change),
        };
        let bytes = canonical_bytes(&obj)
            .map_err(|e| DraftSessionError::Invalid(format!("failed to encode operation: {e}")))?;
        ops_hex.push(format!("\"{}\"", hex::encode(bytes)));
    }

    let mut elem_versions_hex = Vec::new();
    for ev in &session.staged_element_versions {
        let obj = CanonicalObject {
            payload: CanonicalPayload::KnowledgeElementVersion(ev.clone()),
        };
        let bytes = canonical_bytes(&obj).map_err(|e| {
            DraftSessionError::Invalid(format!("failed to encode element version: {e}"))
        })?;
        elem_versions_hex.push(format!("\"{}\"", hex::encode(bytes)));
    }

    let mut rel_versions_hex = Vec::new();
    for rv in &session.staged_relationship_versions {
        let obj = CanonicalObject {
            payload: CanonicalPayload::RelationshipVersion(rv.clone()),
        };
        let bytes = canonical_bytes(&obj).map_err(|e| {
            DraftSessionError::Invalid(format!("failed to encode relationship version: {e}"))
        })?;
        rel_versions_hex.push(format!("\"{}\"", hex::encode(bytes)));
    }

    let obj = CanonicalObject {
        payload: CanonicalPayload::SemanticState(session.working_state.clone()),
    };
    let state_bytes = canonical_bytes(&obj)
        .map_err(|e| DraftSessionError::Invalid(format!("failed to encode working state: {e}")))?;
    let state_hex = hex::encode(state_bytes);

    let desc_str = match &session.description {
        Some(d) => format!("\"{}\"", escape_json_string(d)),
        None => "null".to_string(),
    };

    let base_change_str = match &session.base_change_id {
        Some(c) => format!("\"{}\"", c),
        None => "null".to_string(),
    };

    Ok(format!(
        "{{\n  \
         \"schema_version\": {},\n  \
         \"status\": \"{}\",\n  \
         \"base_state_id\": \"{}\",\n  \
         \"base_change_id\": {},\n  \
         \"created_at\": \"{}\",\n  \
         \"description\": {},\n  \
         \"operations\": [\n    {}\n  ],\n  \
         \"staged_element_versions\": [\n    {}\n  ],\n  \
         \"staged_relationship_versions\": [\n    {}\n  ],\n  \
         \"working_state\": \"{}\"\n\
         }}\n",
        session.schema_version,
        session.status.as_str(),
        session.base_state_id,
        base_change_str,
        escape_json_string(&session.created_at),
        desc_str,
        ops_hex.join(",\n    "),
        elem_versions_hex.join(",\n    "),
        rel_versions_hex.join(",\n    "),
        state_hex
    ))
}

fn parse_draft_session_json(json: &str) -> Result<DraftSession, DraftSessionError> {
    let schema_version = extract_json_int(json, "schema_version")?;
    if schema_version != u64::from(DRAFT_SESSION_VERSION) {
        return Err(DraftSessionError::Invalid(format!(
            "unsupported schema_version: {schema_version}"
        )));
    }

    let status_str = extract_json_string(json, "status")?;
    let status = DraftSessionState::from_str(&status_str)
        .ok_or_else(|| DraftSessionError::Invalid(format!("unsupported status: {status_str}")))?;

    let base_state_str = extract_json_string(json, "base_state_id")?;
    let base_state_id = base_state_str
        .parse()
        .map_err(|_| DraftSessionError::Invalid("malformed base_state_id".to_string()))?;

    let base_change_str = extract_json_nullable_string(json, "base_change_id")?;
    let base_change_id = match base_change_str {
        Some(s) => Some(
            s.parse()
                .map_err(|_| DraftSessionError::Invalid("malformed base_change_id".to_string()))?,
        ),
        None => None,
    };

    let created_at = extract_json_string(json, "created_at")?;
    let description = extract_json_nullable_string(json, "description")?;

    let ops_hex = extract_json_string_array(json, "operations")?;
    let mut operations = Vec::new();
    for op_h in ops_hex {
        let bytes = hex::decode(&op_h)
            .map_err(|_| DraftSessionError::Invalid("malformed operation hex".to_string()))?;
        let canonical = decode_canonical(&bytes)
            .map_err(|e| DraftSessionError::Invalid(format!("failed to decode operation: {e}")))?;
        match canonical.payload {
            CanonicalPayload::ChangeRevision(ch) => {
                if let Some(op) = ch.operations.into_iter().next() {
                    operations.push(op);
                }
            }
            _ => {
                return Err(DraftSessionError::Invalid(
                    "payload is not an Operation wrapper".to_string(),
                ));
            }
        }
    }

    let elem_versions_hex = extract_json_string_array(json, "staged_element_versions")?;
    let mut staged_element_versions = Vec::new();
    for ev_h in elem_versions_hex {
        let bytes = hex::decode(&ev_h).map_err(|_| {
            DraftSessionError::Invalid("malformed staged element version hex".to_string())
        })?;
        let canonical = decode_canonical(&bytes).map_err(|e| {
            DraftSessionError::Invalid(format!("failed to decode staged element version: {e}"))
        })?;
        match canonical.payload {
            CanonicalPayload::KnowledgeElementVersion(ev) => staged_element_versions.push(ev),
            _ => {
                return Err(DraftSessionError::Invalid(
                    "payload is not a KnowledgeElementVersion".to_string(),
                ));
            }
        }
    }

    let rel_versions_hex = extract_json_string_array(json, "staged_relationship_versions")?;
    let mut staged_relationship_versions = Vec::new();
    for rv_h in rel_versions_hex {
        let bytes = hex::decode(&rv_h).map_err(|_| {
            DraftSessionError::Invalid("malformed staged relationship version hex".to_string())
        })?;
        let canonical = decode_canonical(&bytes).map_err(|e| {
            DraftSessionError::Invalid(format!("failed to decode staged relationship version: {e}"))
        })?;
        match canonical.payload {
            CanonicalPayload::RelationshipVersion(rv) => staged_relationship_versions.push(rv),
            _ => {
                return Err(DraftSessionError::Invalid(
                    "payload is not a RelationshipVersion".to_string(),
                ));
            }
        }
    }

    let state_hex = extract_json_string(json, "working_state")?;
    let state_bytes = hex::decode(&state_hex)
        .map_err(|_| DraftSessionError::Invalid("malformed working_state hex".to_string()))?;
    let state_canonical = decode_canonical(&state_bytes)
        .map_err(|e| DraftSessionError::Invalid(format!("failed to decode working_state: {e}")))?;
    let working_state = match state_canonical.payload {
        CanonicalPayload::SemanticState(st) => st,
        _ => {
            return Err(DraftSessionError::Invalid(
                "payload is not a SemanticState".to_string(),
            ));
        }
    };

    Ok(DraftSession {
        schema_version: schema_version as u32,
        status,
        base_state_id,
        base_change_id,
        created_at,
        description,
        operations,
        staged_element_versions,
        staged_relationship_versions,
        working_state,
    })
}

fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn extract_json_string(json: &str, key: &str) -> Result<String, DraftSessionError> {
    let pattern = format!("\"{key}\":");
    let pos = json
        .find(&pattern)
        .ok_or_else(|| DraftSessionError::Invalid(format!("missing key '{key}'")))?;
    let rest = &json[pos + pattern.len()..].trim_start();
    if !rest.starts_with('"') {
        return Err(DraftSessionError::Invalid(format!(
            "key '{key}' must be a string"
        )));
    }
    let rest = &rest[1..];
    let end = rest
        .find('"')
        .ok_or_else(|| DraftSessionError::Invalid(format!("unterminated string for '{key}'")))?;
    Ok(rest[..end].to_string())
}

fn extract_json_nullable_string(
    json: &str,
    key: &str,
) -> Result<Option<String>, DraftSessionError> {
    let pattern = format!("\"{key}\":");
    let pos = json
        .find(&pattern)
        .ok_or_else(|| DraftSessionError::Invalid(format!("missing key '{key}'")))?;
    let rest = &json[pos + pattern.len()..].trim_start();
    if rest.starts_with("null") {
        Ok(None)
    } else if let Some(rest) = rest.strip_prefix('"') {
        let end = rest.find('"').ok_or_else(|| {
            DraftSessionError::Invalid(format!("unterminated string for '{key}'"))
        })?;
        Ok(Some(rest[..end].to_string()))
    } else {
        Err(DraftSessionError::Invalid(format!(
            "key '{key}' must be string or null"
        )))
    }
}

fn extract_json_int(json: &str, key: &str) -> Result<u64, DraftSessionError> {
    let pattern = format!("\"{key}\":");
    let pos = json
        .find(&pattern)
        .ok_or_else(|| DraftSessionError::Invalid(format!("missing key '{key}'")))?;
    let rest = &json[pos + pattern.len()..].trim_start();
    let num_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..num_end]
        .parse()
        .map_err(|_| DraftSessionError::Invalid(format!("invalid integer for '{key}'")))
}

fn extract_json_string_array(json: &str, key: &str) -> Result<Vec<String>, DraftSessionError> {
    let pattern = format!("\"{key}\":");
    let pos = json
        .find(&pattern)
        .ok_or_else(|| DraftSessionError::Invalid(format!("missing key '{key}'")))?;
    let rest = &json[pos + pattern.len()..].trim_start();
    if !rest.starts_with('[') {
        return Err(DraftSessionError::Invalid(format!(
            "key '{key}' must be an array"
        )));
    }
    let array_end = rest
        .find(']')
        .ok_or_else(|| DraftSessionError::Invalid(format!("unterminated array for '{key}'")))?;
    let body = &rest[1..array_end];

    let mut result = Vec::new();
    let mut cursor = body;
    while let Some(quote_start) = cursor.find('"') {
        let after_quote = &cursor[quote_start + 1..];
        let quote_end = after_quote.find('"').ok_or_else(|| {
            DraftSessionError::Invalid(format!("unterminated string in array '{key}'"))
        })?;
        result.push(after_quote[..quote_end].to_string());
        cursor = &after_quote[quote_end + 1..];
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_session_round_trip() {
        let state = SemanticState {
            ontology_version: ObjectId::from_bytes([1; 32]),
            elements: Vec::new(),
            relationships: Vec::new(),
        };

        let session = DraftSession {
            schema_version: 1,
            status: DraftSessionState::Open,
            base_state_id: ObjectId::from_bytes([2; 32]),
            base_change_id: Some(ObjectId::from_bytes([3; 32])),
            created_at: "2026-08-15T12:00:00Z".to_string(),
            description: Some("Test draft".to_string()),
            operations: Vec::new(),
            staged_element_versions: Vec::new(),
            staged_relationship_versions: Vec::new(),
            working_state: state,
        };

        let json = format_draft_session_json(&session).unwrap();
        let parsed = parse_draft_session_json(&json).unwrap();
        assert_eq!(session, parsed);
    }
}
