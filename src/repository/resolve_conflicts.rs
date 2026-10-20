use crate::domain::revision::RepositoryRevision;
use crate::domain::workspace::{PhysicalReconciliationResult, WorkspaceBackend};
use crate::encoding::canonical_bytes;
use crate::encoding::decode_canonical;
use crate::encoding::object::{CanonicalObject, CanonicalPayload};
use crate::repository::open::open_repository;
use crate::repository::reconcile::{
    ReconciliationSession, ReconciliationSessionState, SessionLoadError,
    load_reconciliation_session, save_reconciliation_session,
};
use crate::repository::validation::repository::validate_repository_state;
use crate::repository::workspace::{WorkspaceError, open_workspace};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("Not in a workspace: {0}")]
    Workspace(#[from] WorkspaceError),
    #[error("Repository error: {0}")]
    Repository(#[from] crate::repository::error::RepositoryError),
    #[error("Workspace backend error: {0}")]
    Backend(#[from] crate::domain::workspace::WorkspaceBackendError),
    #[error("Session load error: {0}")]
    SessionLoad(#[from] SessionLoadError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("No active reconciliation session")]
    NoSession,
    #[error("Reconciliation session is stale: workspace base revision changed")]
    StaleSession,
    #[error("Session is not in ConflictedMaterialized state")]
    NotMaterialized,
    #[error("Conflict not found for the given identity")]
    ConflictNotFound,
    #[error("Invalid accept side: {0}")]
    InvalidAcceptSide(String),
    #[error("Query error: {0}")]
    Query(#[from] crate::repository::query::QueryError),
    #[error("Encoding error: {0:?}")]
    Encoding(crate::encoding::validate::CanonicalStructureError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticResolution {
    Base,
    Local,
    Other,
}

impl std::str::FromStr for SemanticResolution {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "base" => Ok(SemanticResolution::Base),
            "local" => Ok(SemanticResolution::Local),
            "other" => Ok(SemanticResolution::Other),
            _ => Err(format!("Invalid semantic resolution: {}", s)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhysicalResolution {
    Base,
    Local,
    Other,
    Working,
}

impl std::str::FromStr for PhysicalResolution {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "base" => Ok(PhysicalResolution::Base),
            "local" => Ok(PhysicalResolution::Local),
            "other" => Ok(PhysicalResolution::Other),
            "working" => Ok(PhysicalResolution::Working),
            _ => Err(format!("Invalid physical resolution: {}", s)),
        }
    }
}

pub fn resolve_semantic_conflict<B: WorkspaceBackend>(
    repo_root: &Path,
    conflict_id: &str,
    side: SemanticResolution,
    backend: &B,
) -> Result<ReconciliationSession, ResolveError> {
    let ws = open_workspace(
        repo_root,
        &crate::repository::workspace::git::GitWorkspaceBackend::open(repo_root)
            .map_err(crate::repository::workspace::WorkspaceError::Backend)?,
    )?;
    let mut session = load_reconciliation_session(repo_root, &ws.id, ws.base_revision)?
        .ok_or(ResolveError::NoSession)?;

    let materialized = matches!(
        session.state,
        ReconciliationSessionState::ConflictedMaterialized { .. }
    );
    let mut candidate = match session.state {
        ReconciliationSessionState::ConflictedMaterialized { candidate }
        | ReconciliationSessionState::Conflicted { candidate } => candidate,
        _ => return Err(ResolveError::NotMaterialized),
    };

    let conflict_idx = candidate
        .semantic_conflicts
        .iter()
        .position(|c| c.id == conflict_id)
        .ok_or(ResolveError::ConflictNotFound)?;

    let conflict = candidate.semantic_conflicts.remove(conflict_idx);

    // We need to resolve the semantic state according to the accept side.
    let repo = open_repository(repo_root)?;
    let rev_id = match side {
        SemanticResolution::Base => candidate.base_revision,
        SemanticResolution::Local => candidate.local_revision,
        SemanticResolution::Other => candidate.other_revision,
    };

    let rev_obj = repo
        .object_store()
        .get(rev_id.as_object_id())
        .map_err(|e| ResolveError::Io(std::io::Error::other(e)))?;
    let rev = match decode_canonical(&rev_obj)
        .map_err(|e| ResolveError::Io(std::io::Error::other(e)))?
        .payload
    {
        CanonicalPayload::RepositoryRevision(r) => r,
        _ => {
            return Err(ResolveError::Io(std::io::Error::other("Not a revision")));
        }
    };

    let state_obj = repo
        .object_store()
        .get(rev.semantic_state.as_object_id())
        .map_err(|e| ResolveError::Io(std::io::Error::other(e)))?;
    let state = match decode_canonical(&state_obj)
        .map_err(|e| ResolveError::Io(std::io::Error::other(e)))?
        .payload
    {
        CanonicalPayload::SemanticState(s) => s,
        _ => {
            return Err(ResolveError::Io(std::io::Error::other(
                "Not a semantic state",
            )));
        }
    };

    // Update candidate's proposed semantic state for all affected elements
    for element_id in conflict.affected_elements {
        let element_version = state.elements.iter().find(|e| e.element_id == element_id);
        candidate
            .proposed_semantic_state
            .elements
            .retain(|e| e.element_id != element_id);
        if let Some(ev) = element_version {
            candidate.proposed_semantic_state.elements.push(ev.clone());
        }
    }
    for relationship_id in conflict.affected_relationships {
        let version = state
            .relationships
            .iter()
            .find(|entry| entry.relationship_id == relationship_id);
        candidate
            .proposed_semantic_state
            .relationships
            .retain(|entry| entry.relationship_id != relationship_id);
        if let Some(version) = version {
            candidate
                .proposed_semantic_state
                .relationships
                .push(version.clone());
        }
    }
    candidate.proposed_semantic_state.elements.sort();
    candidate.proposed_semantic_state.relationships.sort();

    // Re-run validation
    let validation_report = validate_repository_state(
        repo.object_store(),
        &candidate.proposed_semantic_state,
        &[],
        &[],
    )?;
    candidate.validation_findings = validation_report
        .violations
        .into_iter()
        .map(|f| crate::domain::conflict::ValidationFinding {
            diagnostic: f.message,
        })
        .collect();

    // Check if fully resolved
    if let Some(new_candidate) = check_and_transition(repo_root, &ws.id, &candidate, backend)? {
        session.state = new_candidate;
    } else {
        session.state = if materialized {
            ReconciliationSessionState::ConflictedMaterialized { candidate }
        } else {
            ReconciliationSessionState::Conflicted { candidate }
        };
    }

    save_reconciliation_session(repo_root, &ws.id, &session, backend)?;

    Ok(session)
}

pub fn resolve_physical_conflict<B: WorkspaceBackend>(
    repo_root: &Path,
    conflict_id: &str,
    side: PhysicalResolution,
    backend: &B,
) -> Result<ReconciliationSession, ResolveError> {
    let ws = open_workspace(
        repo_root,
        &crate::repository::workspace::git::GitWorkspaceBackend::open(repo_root)
            .map_err(crate::repository::workspace::WorkspaceError::Backend)?,
    )?;
    let mut session = load_reconciliation_session(repo_root, &ws.id, ws.base_revision)?
        .ok_or(ResolveError::NoSession)?;

    let mut candidate = match session.state {
        ReconciliationSessionState::ConflictedMaterialized { candidate } => candidate,
        _ => return Err(ResolveError::NotMaterialized),
    };

    let conflict_idx = candidate
        .materialization_conflicts
        .iter()
        .position(|c| c.id == conflict_id)
        .ok_or(ResolveError::ConflictNotFound)?;

    // According to the user feedback:
    // apply/capture selected physical alternative -> verify materialization -> update backend provisional state -> remove conflict -> persist backend state -> persist reconciliation session

    // For now, capturing working state updates provisional state in our abstract design
    if side != PhysicalResolution::Working {
        return Err(ResolveError::InvalidAcceptSide("only 'working' is supported for physical conflicts currently in this implementation. Use physical tools (like git) to restore versions, then --accept working".into()));
    }
    candidate.materialization_conflicts.remove(conflict_idx);

    // Check if fully resolved
    if let Some(new_candidate) = check_and_transition(repo_root, &ws.id, &candidate, backend)? {
        session.state = new_candidate;
    } else {
        session.state = ReconciliationSessionState::ConflictedMaterialized { candidate };
    }

    save_reconciliation_session(repo_root, &ws.id, &session, backend)?;

    Ok(session)
}

fn check_and_transition<B: WorkspaceBackend>(
    repo_root: &Path,
    workspace_id: &crate::domain::workspace::WorkspaceId,
    candidate: &crate::repository::reconcile::ReconciliationCandidate,
    backend: &B,
) -> Result<Option<ReconciliationSessionState>, ResolveError> {
    // We assume all validation findings in v0.5 are blocking, so checking `validation_findings.is_empty()` is sufficient.
    if !candidate.semantic_conflicts.is_empty()
        || !candidate.materialization_conflicts.is_empty()
        || !candidate.validation_findings.is_empty()
    {
        return Ok(None);
    }

    let repo = open_repository(repo_root)?;

    let snapshot_id = if let Some(physical) = &candidate.physical_candidate {
        backend.snapshot_resolved_candidate(workspace_id, physical)
    } else if let Some(snapshot) = &candidate.clean_physical_snapshot {
        if !backend
            .verify_snapshot_integrity(snapshot)
            .map_err(|e| ResolveError::Io(std::io::Error::other(e)))?
        {
            return Err(ResolveError::Io(std::io::Error::other(
                "Clean physical snapshot failed integrity verification",
            )));
        }
        Ok(snapshot.clone())
    } else {
        // Older version-1 sessions omit the clean snapshot. Reconcile only their immutable inputs.
        let base = repo.read_revision(candidate.base_revision)?;
        let local = repo.read_revision(candidate.local_revision)?;
        let other = repo.read_revision(candidate.other_revision)?;
        match backend.reconcile_physical(
            &base.workspace_snapshot,
            &local.workspace_snapshot,
            &other.workspace_snapshot,
        )? {
            PhysicalReconciliationResult::Clean { snapshot }
                if backend.verify_snapshot_integrity(&snapshot)? =>
            {
                Ok(snapshot)
            }
            _ => Err(crate::domain::workspace::WorkspaceBackendError::Resolution(
                "Cannot recover the clean physical result from this older session".into(),
            )),
        }
    }
    .map_err(|e| ResolveError::Io(std::io::Error::other(e)))?;

    let state_obj = CanonicalObject {
        payload: CanonicalPayload::SemanticState(candidate.proposed_semantic_state.clone()),
    };
    let state_bytes = canonical_bytes(&state_obj).map_err(ResolveError::Encoding)?;
    let semantic_id = repo
        .object_store()
        .put(&state_bytes)
        .map_err(|e| ResolveError::Io(std::io::Error::other(e)))?;
    let semantic_state = crate::domain::identity::SemanticStateId::from_object_id(semantic_id);

    let mut parents = vec![candidate.local_revision, candidate.other_revision];
    parents.sort();
    let revision = RepositoryRevision {
        parents,
        semantic_state,
        semantic_change: None,
        workspace_snapshot: snapshot_id,
    };

    let rev_obj = CanonicalObject {
        payload: CanonicalPayload::RepositoryRevision(revision),
    };
    let rev_bytes = canonical_bytes(&rev_obj).map_err(ResolveError::Encoding)?;
    let rev_id = repo
        .object_store()
        .put(&rev_bytes)
        .map_err(|e| ResolveError::Io(std::io::Error::other(e)))?;
    let rev_id = crate::domain::identity::RepositoryRevisionId::from_object_id(rev_id);

    Ok(Some(ReconciliationSessionState::PreparedClean {
        revision: rev_id,
    }))
}
