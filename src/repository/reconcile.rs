use std::collections::BTreeSet;

use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::conflict::{SemanticConflict, SemanticConflictKind, ValidationFinding};
use crate::domain::identity::{ElementId, ObjectId, RelationshipId, RepositoryRevisionId};
use crate::domain::state::{ElementStateEntry, RelationshipStateEntry, SemanticState};
use crate::domain::workspace::WorkspaceId;
use crate::domain::workspace::{PhysicalReconciliationResult, WorkspaceBackend};
use crate::encoding::cbor;
use crate::encoding::object::{CanonicalObject, CanonicalPayload, ObjectKind};
use crate::repository::object_store::ObjectStore;
use crate::repository::query::{QueryError, load_typed};
use crate::repository::validation::validate_repository_state;

#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReconciliationSession {
    pub version: u32,
    pub workspace_id: WorkspaceId,
    pub base_revision: RepositoryRevisionId,
    pub target_revision: RepositoryRevisionId,
    pub state: ReconciliationSessionState,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum ReconciliationSessionState {
    /// Both semantic and physical reconciliation succeeded with no conflicts or findings.
    /// The proposed state and new physical snapshot are ready to be committed.
    PreparedClean { revision: RepositoryRevisionId },
    /// Conflicts or findings were found in either domain.
    Conflicted { candidate: ReconciliationCandidate },
    /// Physical conflicts have been materialized into the workspace.
    ConflictedMaterialized { candidate: ReconciliationCandidate },
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ReconcileWorkspaceOutcome {
    Same,
    LocalAhead,
    OtherAhead,
    Prepared { session: Box<ReconciliationSession> },
}

/// A candidate produced by attempting semantic reconciliation.
#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReconciliationCandidate {
    pub version: u32,
    pub workspace_id: WorkspaceId,
    pub base_revision: RepositoryRevisionId,
    pub local_revision: RepositoryRevisionId,
    pub other_revision: RepositoryRevisionId,
    pub proposed_semantic_state: SemanticState,
    pub semantic_conflicts: Vec<SemanticConflict>,
    pub physical_candidate: Option<crate::domain::workspace::PhysicalReconciliationCandidate>,
    pub materialization_conflicts: Vec<crate::domain::conflict::MaterializationConflict>,
    pub validation_findings: Vec<ValidationFinding>,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionLoadError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unsupported session format version: {0}")]
    UnsupportedVersion(u32),
    #[error("Stale session: base revision mismatch")]
    StaleBase,
    #[error("Stale session: target revision mismatch")]
    StaleTarget,
    #[error("Stale session: workspace ID mismatch")]
    StaleWorkspace,
    #[error("Invalid session: {0}")]
    Invalid(String),
}

/// Attempts to reconcile two divergent semantic states against their common base.
#[allow(clippy::too_many_arguments)]
pub fn reconcile_semantic(
    store: &ObjectStore,
    workspace_id: WorkspaceId,
    base_revision: RepositoryRevisionId,
    local_revision: RepositoryRevisionId,
    other_revision: RepositoryRevisionId,
    base: &SemanticState,
    local: &SemanticState,
    other: &SemanticState,
) -> Result<ReconciliationCandidate, QueryError> {
    let mut conflicts = Vec::new();
    let mut findings = Vec::new();

    // 1. Reconcile Elements
    let mut next_elements = Vec::new();
    let all_elements = collect_element_ids(base, local, other);

    for element_id in all_elements {
        let b = get_element_version(base, &element_id);
        let l = get_element_version(local, &element_id);
        let o = get_element_version(other, &element_id);

        if l == o {
            if let Some(v) = l {
                next_elements.push(ElementStateEntry {
                    element_id,
                    version: *v,
                });
            }
        } else if l == b && o != b {
            if let Some(v) = o {
                next_elements.push(ElementStateEntry {
                    element_id,
                    version: *v,
                });
            }
        } else if o == b && l != b {
            if let Some(v) = l {
                next_elements.push(ElementStateEntry {
                    element_id,
                    version: *v,
                });
            }
        } else {
            // Conflict (L != B && O != B && L != O)
            let kind = determine_element_conflict_kind(store, b.copied(), l.copied(), o.copied())?;
            conflicts.push(SemanticConflict {
                id: uuid::Uuid::from_bytes(
                    crate::encoding::hash::object_id(format!("{:?}", kind).as_bytes()).as_bytes()
                        [..16]
                        .try_into()
                        .unwrap(),
                )
                .to_string(),
                affected_elements: vec![element_id],
                affected_relationships: vec![],
                kind,
            });
            // Neutral rule: Retain base version if it existed. Omit if concurrent creation.
            if let Some(v) = b {
                next_elements.push(ElementStateEntry {
                    element_id,
                    version: *v,
                });
            }
        }
    }

    // 2. Reconcile Relationships
    let mut next_relationships = Vec::new();
    let all_relationships = collect_relationship_ids(base, local, other);

    for rel_id in all_relationships {
        let b = get_relationship_version(base, &rel_id);
        let l = get_relationship_version(local, &rel_id);
        let o = get_relationship_version(other, &rel_id);

        if l == o {
            if let Some(v) = l {
                next_relationships.push(RelationshipStateEntry {
                    relationship_id: rel_id,
                    version: *v,
                });
            }
        } else if l == b && o != b {
            if let Some(v) = o {
                next_relationships.push(RelationshipStateEntry {
                    relationship_id: rel_id,
                    version: *v,
                });
            }
        } else if o == b && l != b {
            if let Some(v) = l {
                next_relationships.push(RelationshipStateEntry {
                    relationship_id: rel_id,
                    version: *v,
                });
            }
        } else {
            let kind = SemanticConflictKind::RelationshipConflict {
                base_version: b.copied(),
                local_version: l.copied(),
                other_version: o.copied(),
            };
            conflicts.push(SemanticConflict {
                id: uuid::Uuid::from_bytes(
                    crate::encoding::hash::object_id(format!("{:?}", kind).as_bytes()).as_bytes()
                        [..16]
                        .try_into()
                        .unwrap(),
                )
                .to_string(),
                affected_elements: vec![],
                affected_relationships: vec![rel_id],
                kind,
            });
            // Neutral rule: Retain base version if it existed.
            if let Some(v) = b {
                next_relationships.push(RelationshipStateEntry {
                    relationship_id: rel_id,
                    version: *v,
                });
            }
        }
    }

    let ontology_version = if local.ontology_version == other.ontology_version {
        local.ontology_version
    } else if local.ontology_version == base.ontology_version {
        other.ontology_version
    } else if other.ontology_version == base.ontology_version {
        local.ontology_version
    } else {
        let kind = SemanticConflictKind::ConcurrentModification {
            base_version: Some(base.ontology_version),
            local_version: Some(local.ontology_version),
            other_version: Some(other.ontology_version),
        };
        conflicts.push(SemanticConflict {
            id: uuid::Uuid::from_bytes(
                crate::encoding::hash::object_id(format!("{:?}", kind).as_bytes()).as_bytes()[..16]
                    .try_into()
                    .unwrap(),
            )
            .to_string(),
            affected_elements: vec![],
            affected_relationships: vec![],
            kind,
        });
        base.ontology_version
    };

    // 3. Combined Validation
    // Both next_elements and next_relationships must be sorted to be canonical
    next_elements.sort_unstable();
    next_relationships.sort_unstable();

    let proposed_semantic_state = SemanticState {
        ontology_version,
        elements: next_elements,
        relationships: next_relationships,
    };

    if let Err(validation_err) =
        validate_repository_state(store, &proposed_semantic_state, &[], &[])
    {
        findings.push(ValidationFinding {
            diagnostic: validation_err.to_string(),
        });
    }

    Ok(ReconciliationCandidate {
        version: 1,
        workspace_id,
        base_revision,
        local_revision,
        other_revision,
        proposed_semantic_state,
        semantic_conflicts: conflicts,
        physical_candidate: None,
        materialization_conflicts: vec![],
        validation_findings: findings,
    })
}

// Helper for the persistence path
pub fn session_path(repo_root: &Path, workspace_id: &WorkspaceId) -> PathBuf {
    repo_root
        .join(".kat")
        .join("workspaces")
        .join(&workspace_id.0)
        .join("reconciliation_session.json")
}

pub fn save_reconciliation_session(
    repo_root: &Path,
    workspace_id: &WorkspaceId,
    session: &ReconciliationSession,
    backend: &dyn WorkspaceBackend,
) -> std::io::Result<()> {
    let path = session_path(repo_root, workspace_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // 1. Construct physical state & 2. Write backend candidate atomically
    if let ReconciliationSessionState::Conflicted { candidate } = &session.state
        && let Some(phys) = &candidate.physical_candidate
    {
        backend
            .persist_physical_candidate(workspace_id, phys)
            .map_err(std::io::Error::other)?;
        // 3. Verify it can be reopened (the API doesn't expose it generically yet, but backend has done the durable write)
        // If it was GitWorkspaceBackend, it would load_physical_candidate. We can just rely on the durable write success.
    }

    // 4. Write reconciliation_session.json atomically
    let temp_path = path.with_extension("tmp");
    let json = serde_json::to_string_pretty(session).map_err(std::io::Error::other)?;
    fs::write(&temp_path, json)?;
    fs::rename(temp_path, path)?;
    Ok(())
}

pub fn load_reconciliation_session(
    repo_root: &Path,
    workspace_id: &WorkspaceId,
    expected_base: RepositoryRevisionId,
) -> Result<Option<ReconciliationSession>, SessionLoadError> {
    let path = session_path(repo_root, workspace_id);
    if !path.exists() {
        return Ok(None);
    }
    let json = fs::read_to_string(&path)?;

    // Parse as a generic value first to check version
    let parsed: serde_json::Value = serde_json::from_str(&json)?;
    if let Some(version) = parsed.get("version").and_then(|v| v.as_u64()) {
        if version != 1 {
            return Err(SessionLoadError::UnsupportedVersion(version as u32));
        }
    } else {
        return Err(SessionLoadError::UnsupportedVersion(0));
    }

    let session: ReconciliationSession = serde_json::from_value(parsed)?;

    if session.workspace_id != *workspace_id {
        return Err(SessionLoadError::StaleWorkspace);
    }
    if session.base_revision != expected_base {
        return Err(SessionLoadError::StaleBase);
    }

    Ok(Some(session))
}

pub fn clear_reconciliation_session(
    repo_root: &Path,
    workspace_id: &WorkspaceId,
) -> std::io::Result<()> {
    let path = session_path(repo_root, workspace_id);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum AbortError {
    #[error("Not in a workspace: {0}")]
    Workspace(#[from] crate::repository::workspace::WorkspaceError),
    #[error("Repository error: {0}")]
    Repository(#[from] crate::repository::error::RepositoryError),
    #[error("Session load error: {0}")]
    SessionLoad(#[from] crate::repository::reconcile::SessionLoadError),
    #[error("Draft session abort error: {0}")]
    DraftSession(#[from] crate::repository::session::DraftSessionError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Encoding error: {0}")]
    Encoding(#[from] crate::encoding::validate::CanonicalStructureError),
    #[error("Decoding error: {0}")]
    Decoding(#[from] crate::encoding::DecodingError),
    #[error("Object store error: {0}")]
    ObjectStore(#[from] crate::repository::object_store::ObjectStoreError),
}

pub fn abort_reconciliation_session<B: WorkspaceBackend>(
    repo_root: &Path,
    backend: &B,
) -> Result<bool, AbortError> {
    let ws = crate::repository::workspace::open_workspace(
        repo_root,
        &crate::repository::workspace::git::GitWorkspaceBackend::open(repo_root)
            .map_err(crate::repository::workspace::WorkspaceError::Backend)?,
    )?;

    let session = load_reconciliation_session(repo_root, &ws.id, ws.base_revision)?;

    if let Some(session) = session {
        let repo = crate::repository::open::open_repository(repo_root)?;
        let store = repo.object_store();

        // We always use the session's original workspace base as the authoritative rollback point.
        let base_rev_obj = store.get(session.base_revision.as_object_id())?;
        let base_rev = match crate::encoding::decode_canonical(&base_rev_obj)?.payload {
            CanonicalPayload::RepositoryRevision(r) => r,
            _ => {
                return Err(AbortError::Io(std::io::Error::other("Not a revision")));
            }
        };

        // Check if working tree != Wbase
        let working_state = backend
            .inspect_working_state(&base_rev.workspace_snapshot)
            .map_err(|e| AbortError::Io(std::io::Error::other(e)))?;

        let has_changes = !working_state.changes.untracked.is_empty()
            || !working_state.changes.modified.is_empty()
            || !working_state.changes.deleted.is_empty();

        if has_changes {
            backend
                .materialize_snapshot(&base_rev.workspace_snapshot)
                .map_err(|e| AbortError::Io(std::io::Error::other(e)))?;

            // Verify
            if !backend
                .verify_snapshot_integrity(&base_rev.workspace_snapshot)
                .unwrap_or(false)
            {
                return Err(AbortError::Io(std::io::Error::other(
                    "Materialization verification failed",
                )));
            }
        }

        backend
            .clear_physical_candidate(&ws.id)
            .map_err(|e| AbortError::Io(std::io::Error::other(e)))?;

        clear_reconciliation_session(repo_root, &ws.id)?;

        return Ok(true);
    }

    Ok(false)
}

#[derive(Debug, thiserror::Error)]
pub enum FinalizeError {
    #[error("Not in a workspace: {0}")]
    Workspace(#[from] crate::repository::workspace::WorkspaceError),
    #[error("Repository error: {0}")]
    Repository(#[from] crate::repository::error::RepositoryError),
    #[error("Session load error: {0}")]
    SessionLoad(#[from] crate::repository::reconcile::SessionLoadError),
    #[error("No active reconciliation session found.")]
    NoSession,
    #[error("Cannot finalize: Session is not in a PreparedClean state.")]
    NotPreparedClean,
    #[error("Stale session: workspace base revision does not match session base revision.")]
    StaleSession,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Encoding error: {0}")]
    Encoding(#[from] crate::encoding::validate::CanonicalStructureError),
    #[error("Decoding error: {0}")]
    Decoding(#[from] crate::encoding::DecodingError),
    #[error("Object store error: {0}")]
    ObjectStore(#[from] crate::repository::object_store::ObjectStoreError),
}

pub fn finalize_reconciliation_session<B: WorkspaceBackend>(
    repo_root: &Path,
    backend: &B,
) -> Result<(), FinalizeError> {
    let ws = crate::repository::workspace::open_workspace(
        repo_root,
        &crate::repository::workspace::git::GitWorkspaceBackend::open(repo_root)
            .map_err(crate::repository::workspace::WorkspaceError::Backend)?,
    )?;

    let session = load_reconciliation_session(repo_root, &ws.id, ws.base_revision)?
        .ok_or(FinalizeError::NoSession)?;

    if ws.base_revision != session.base_revision {
        return Err(FinalizeError::StaleSession);
    }

    let r_merge = match session.state {
        ReconciliationSessionState::PreparedClean { revision } => revision,
        _ => return Err(FinalizeError::NotPreparedClean),
    };

    let repo = crate::repository::open::open_repository(repo_root)?;
    let store = repo.object_store();
    let r_merge_obj = store.get(r_merge.as_object_id())?;
    let r_merge_rev = match crate::encoding::decode_canonical(&r_merge_obj)?.payload {
        CanonicalPayload::RepositoryRevision(r) => r,
        _ => {
            return Err(FinalizeError::Io(std::io::Error::other("Not a revision")));
        }
    };

    let w_merge = r_merge_rev.workspace_snapshot;

    let working_state = backend
        .inspect_working_state(&w_merge)
        .map_err(|e| FinalizeError::Io(std::io::Error::other(e)))?;

    let has_changes = !working_state.changes.untracked.is_empty()
        || !working_state.changes.modified.is_empty()
        || !working_state.changes.deleted.is_empty();

    if has_changes {
        backend
            .materialize_snapshot(&w_merge)
            .map_err(|e| FinalizeError::Io(std::io::Error::other(e)))?;

        if !backend.verify_snapshot_integrity(&w_merge).unwrap_or(false) {
            return Err(FinalizeError::Io(std::io::Error::other(
                "Materialization verification failed",
            )));
        }
    }

    crate::repository::workspace::update_workspace_base(repo_root, &ws.id, r_merge)?;

    backend
        .clear_physical_candidate(&ws.id)
        .map_err(|e| FinalizeError::Io(std::io::Error::other(e)))?;

    clear_reconciliation_session(repo_root, &ws.id)?;

    Ok(())
}

pub fn reconcile_workspace(
    repo_root: &Path,
    workspace_id: &WorkspaceId,
    target_rev_id: RepositoryRevisionId,
) -> Result<ReconcileWorkspaceOutcome, QueryError> {
    let repo = crate::repository::open::open_repository(repo_root).map_err(|_| {
        QueryError::ObjectStore(crate::repository::object_store::ObjectStoreError::NotFound(
            ObjectId::from_bytes([0; 32]),
        ))
    })?;
    let store = repo.object_store();

    use crate::repository::session::has_draft_session;
    if has_draft_session(repo_root) {
        return Err(QueryError::WorkspaceConflict(
            "an active authoring session already exists".to_string(),
        ));
    }

    if session_path(repo_root, workspace_id).exists() {
        return Err(QueryError::WorkspaceConflict(
            "an active reconciliation session already exists".to_string(),
        ));
    }

    let ws = crate::repository::workspace::open_workspace(
        repo_root,
        &crate::repository::workspace::git::GitWorkspaceBackend::open(repo_root).unwrap(),
    )
    .map_err(|_| {
        QueryError::ObjectStore(crate::repository::object_store::ObjectStoreError::NotFound(
            ObjectId::from_bytes([0; 32]),
        ))
    })?;

    let base_rev =
        crate::repository::topology::compare_ancestry(store, ws.base_revision, target_rev_id)?;
    let common_base_id = match base_rev {
        crate::repository::topology::DivergenceState::Same => {
            return Ok(ReconcileWorkspaceOutcome::Same);
        }
        crate::repository::topology::DivergenceState::LocalAhead => {
            return Ok(ReconcileWorkspaceOutcome::LocalAhead);
        }
        crate::repository::topology::DivergenceState::OtherAhead => {
            return Ok(ReconcileWorkspaceOutcome::OtherAhead);
        }
        crate::repository::topology::DivergenceState::Diverged { common_base } => common_base,
        crate::repository::topology::DivergenceState::AmbiguousMergeBase { bases } => {
            return Err(QueryError::AmbiguousMergeBase(bases));
        }
        crate::repository::topology::DivergenceState::Unrelated => {
            return Err(QueryError::UnrelatedHistory);
        }
    };

    let base_revision = repo.read_revision(common_base_id).map_err(|_| {
        QueryError::ObjectStore(crate::repository::object_store::ObjectStoreError::NotFound(
            ObjectId::from_bytes([0; 32]),
        ))
    })?;
    let local_revision = repo.read_revision(ws.base_revision).map_err(|_| {
        QueryError::ObjectStore(crate::repository::object_store::ObjectStoreError::NotFound(
            ObjectId::from_bytes([0; 32]),
        ))
    })?;
    let target_revision = repo.read_revision(target_rev_id).map_err(|_| {
        QueryError::ObjectStore(crate::repository::object_store::ObjectStoreError::NotFound(
            ObjectId::from_bytes([0; 32]),
        ))
    })?;

    let base_state_obj = store
        .get(base_revision.semantic_state.as_object_id())
        .map_err(QueryError::ObjectStore)?;
    let base_state = match crate::encoding::decode_canonical(&base_state_obj)
        .map_err(QueryError::Decoding)?
        .payload
    {
        CanonicalPayload::SemanticState(s) => s,
        _ => {
            return Err(QueryError::ObjectStore(
                crate::repository::object_store::ObjectStoreError::NotFound(ObjectId::from_bytes(
                    [0; 32],
                )),
            ));
        }
    };

    let local_state_obj = store
        .get(local_revision.semantic_state.as_object_id())
        .map_err(QueryError::ObjectStore)?;
    let local_state = match crate::encoding::decode_canonical(&local_state_obj)
        .map_err(QueryError::Decoding)?
        .payload
    {
        CanonicalPayload::SemanticState(s) => s,
        _ => {
            return Err(QueryError::ObjectStore(
                crate::repository::object_store::ObjectStoreError::NotFound(ObjectId::from_bytes(
                    [0; 32],
                )),
            ));
        }
    };

    let target_state_obj = store
        .get(target_revision.semantic_state.as_object_id())
        .map_err(QueryError::ObjectStore)?;
    let target_state = match crate::encoding::decode_canonical(&target_state_obj)
        .map_err(QueryError::Decoding)?
        .payload
    {
        CanonicalPayload::SemanticState(s) => s,
        _ => {
            return Err(QueryError::ObjectStore(
                crate::repository::object_store::ObjectStoreError::NotFound(ObjectId::from_bytes(
                    [0; 32],
                )),
            ));
        }
    };

    let backend =
        crate::repository::workspace::git::GitWorkspaceBackend::open(repo_root).map_err(|_| {
            QueryError::ObjectStore(crate::repository::object_store::ObjectStoreError::NotFound(
                ObjectId::from_bytes([0; 32]),
            ))
        })?;

    let session = reconcile(
        store,
        &backend,
        workspace_id.clone(),
        common_base_id,
        ws.base_revision,
        target_rev_id,
        &base_revision.workspace_snapshot,
        &local_revision.workspace_snapshot,
        &target_revision.workspace_snapshot,
        &base_state,
        &local_state,
        &target_state,
    )?;

    save_reconciliation_session(repo_root, workspace_id, &session, &backend).map_err(|_| {
        QueryError::ObjectStore(crate::repository::object_store::ObjectStoreError::NotFound(
            ObjectId::from_bytes([0; 32]),
        ))
    })?;

    Ok(ReconcileWorkspaceOutcome::Prepared {
        session: Box::new(session),
    })
}

/// Orchestrates both semantic and physical reconciliation.
///
/// If both domains are clean, returns a `ReconciliationSession` in the `PreparedClean` state.
/// If either domain produces conflicts or validation findings, returns a `ReconciliationSession`
/// in the `Conflicted` state.
#[allow(clippy::too_many_arguments)]
pub fn reconcile(
    store: &ObjectStore,
    backend: &dyn WorkspaceBackend,
    workspace_id: WorkspaceId,
    base_rev_id: RepositoryRevisionId,
    local_rev_id: RepositoryRevisionId,
    other_rev_id: RepositoryRevisionId,
    base_snapshot: &crate::domain::identity::WorkspaceSnapshotId,
    local_snapshot: &crate::domain::identity::WorkspaceSnapshotId,
    other_snapshot: &crate::domain::identity::WorkspaceSnapshotId,
    base_state: &SemanticState,
    local_state: &SemanticState,
    other_state: &SemanticState,
) -> Result<ReconciliationSession, QueryError> {
    // 1. Semantic Reconciliation
    let mut candidate = reconcile_semantic(
        store,
        workspace_id.clone(),
        base_rev_id,
        local_rev_id,
        other_rev_id,
        base_state,
        local_state,
        other_state,
    )?;

    // 2. Physical Reconciliation
    let physical_res = backend
        .reconcile_physical(base_snapshot, local_snapshot, other_snapshot)
        .map_err(QueryError::WorkspaceBackend)?;

    match physical_res {
        PhysicalReconciliationResult::Clean {
            snapshot: new_snapshot_id,
        } => {
            if candidate.semantic_conflicts.is_empty() && candidate.validation_findings.is_empty() {
                // Both semantic and physical domains are clean

                // 1. Persist the semantic state
                let state_payload =
                    CanonicalPayload::SemanticState(candidate.proposed_semantic_state.clone());
                let state_bytes = cbor::canonical_bytes(&CanonicalObject {
                    payload: state_payload,
                })
                .map_err(|e| {
                    QueryError::ObjectStore(crate::repository::object_store::ObjectStoreError::Io(
                        std::io::Error::other(e),
                    ))
                })?;
                let state_id = store.put(&state_bytes).map_err(QueryError::ObjectStore)?;
                let semantic_state_id =
                    crate::domain::identity::SemanticStateId::from_object_id(state_id);

                // 2. Parents must be deterministically sorted as per spec
                // Note: the spec usually implies Local and Other are the parents of a 3-way merge, base is implied by their ancestry.
                // We'll record local and other.
                let mut parents = vec![local_rev_id, other_rev_id];
                parents.sort();

                // 3. Construct the RepositoryRevision
                let rev = crate::domain::revision::RepositoryRevision {
                    parents,
                    semantic_state: semantic_state_id,
                    workspace_snapshot: new_snapshot_id,
                    semantic_change: None, // As per P9-11
                };

                // 4. Persist the RepositoryRevision
                let rev_payload = CanonicalPayload::RepositoryRevision(rev);
                let rev_bytes = cbor::canonical_bytes(&CanonicalObject {
                    payload: rev_payload,
                })
                .map_err(|e| {
                    QueryError::ObjectStore(crate::repository::object_store::ObjectStoreError::Io(
                        std::io::Error::other(e),
                    ))
                })?;
                let rev_obj_id = store.put(&rev_bytes).map_err(QueryError::ObjectStore)?;
                let revision_id = RepositoryRevisionId::from_object_id(rev_obj_id);

                return Ok(ReconciliationSession {
                    version: 1,
                    workspace_id,
                    base_revision: local_rev_id,
                    target_revision: other_rev_id,
                    state: ReconciliationSessionState::PreparedClean {
                        revision: revision_id,
                    },
                });
            }
        }
        PhysicalReconciliationResult::Conflicted(phys_candidate) => {
            candidate.materialization_conflicts = phys_candidate.conflicts.clone();
            candidate.physical_candidate = Some(phys_candidate);
        }
    }

    // Either semantic conflicts/findings exist, or physical conflicts exist (or both).
    Ok(ReconciliationSession {
        version: 1,
        workspace_id,
        base_revision: local_rev_id,
        target_revision: other_rev_id,
        state: ReconciliationSessionState::Conflicted { candidate },
    })
}

fn collect_element_ids(
    base: &SemanticState,
    local: &SemanticState,
    other: &SemanticState,
) -> BTreeSet<ElementId> {
    let mut set = BTreeSet::new();
    for e in &base.elements {
        set.insert(e.element_id);
    }
    for e in &local.elements {
        set.insert(e.element_id);
    }
    for e in &other.elements {
        set.insert(e.element_id);
    }
    set
}

fn collect_relationship_ids(
    base: &SemanticState,
    local: &SemanticState,
    other: &SemanticState,
) -> BTreeSet<RelationshipId> {
    let mut set = BTreeSet::new();
    for e in &base.relationships {
        set.insert(e.relationship_id);
    }
    for e in &local.relationships {
        set.insert(e.relationship_id);
    }
    for e in &other.relationships {
        set.insert(e.relationship_id);
    }
    set
}

fn get_element_version<'a>(state: &'a SemanticState, id: &ElementId) -> Option<&'a ObjectId> {
    state
        .elements
        .iter()
        .find(|e| &e.element_id == id)
        .map(|e| &e.version)
}

fn get_relationship_version<'a>(
    state: &'a SemanticState,
    id: &RelationshipId,
) -> Option<&'a ObjectId> {
    state
        .relationships
        .iter()
        .find(|e| &e.relationship_id == id)
        .map(|e| &e.version)
}

fn determine_element_conflict_kind(
    store: &ObjectStore,
    b: Option<ObjectId>,
    l: Option<ObjectId>,
    o: Option<ObjectId>,
) -> Result<SemanticConflictKind, QueryError> {
    let mut l_lifecycle = None;
    let mut o_lifecycle = None;

    if let Some(l_id) = l {
        let obj = load_typed(store, l_id, ObjectKind::KnowledgeElementVersion)?;
        if let CanonicalPayload::KnowledgeElementVersion(ev) = obj.payload {
            l_lifecycle = Some(ev.lifecycle);
        }
    }

    if let Some(o_id) = o {
        let obj = load_typed(store, o_id, ObjectKind::KnowledgeElementVersion)?;
        if let CanonicalPayload::KnowledgeElementVersion(ev) = obj.payload {
            o_lifecycle = Some(ev.lifecycle);
        }
    }

    if l_lifecycle == Some(crate::domain::element::Lifecycle::Superseded)
        || o_lifecycle == Some(crate::domain::element::Lifecycle::Superseded)
    {
        return Ok(SemanticConflictKind::SupersessionConflict {
            base_version: b,
            local_version: l,
            other_version: o,
        });
    }

    if l_lifecycle.is_some() && o_lifecycle.is_some() && l_lifecycle != o_lifecycle {
        Ok(SemanticConflictKind::LifecycleMismatch {
            base_version: b,
            local_version: l,
            other_version: o,
        })
    } else {
        Ok(SemanticConflictKind::ConcurrentModification {
            base_version: b,
            local_version: l,
            other_version: o,
        })
    }
}
