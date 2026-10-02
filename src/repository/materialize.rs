use crate::domain::workspace::WorkspaceBackend;
use crate::repository::error::RepositoryError;
use crate::repository::open::open_repository;
use crate::repository::reconcile::{
    ReconciliationSession, ReconciliationSessionState, SessionLoadError,
    load_reconciliation_session, save_reconciliation_session,
};
use crate::repository::workspace::{WorkspaceError, open_workspace};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum MaterializeError {
    #[error("Not in a workspace: {0}")]
    Workspace(#[from] WorkspaceError),
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),
    #[error("Session load error: {0}")]
    SessionLoad(#[from] SessionLoadError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("No active reconciliation session")]
    NoSession,
    #[error("Reconciliation session is stale: workspace base revision changed")]
    StaleSession,
    #[error("Session is already clean, no conflicts to materialize")]
    PreparedClean,
    #[error("Conflicts are already materialized")]
    AlreadyMaterialized,
    #[error("Failed to open physical backend: {0}")]
    Backend(crate::domain::workspace::WorkspaceBackendError),
    #[error(
        "Materialization failed, but workspace was successfully restored to pre-materialization state: {original}"
    )]
    MaterializationFailed {
        original: crate::domain::workspace::WorkspaceBackendError,
        restored: bool,
    },
    #[error(
        "CRITICAL: Materialization failed AND recovery failed! Workspace is in an unknown state.\nOriginal error: {original}\nRecovery error: {recovery}"
    )]
    MaterializationRecoveryFailed {
        original: crate::domain::workspace::WorkspaceBackendError,
        recovery: crate::domain::workspace::WorkspaceBackendError,
    },
}

pub fn materialize_conflicts<B: WorkspaceBackend>(
    repo_root: &Path,
    backend: &B,
) -> Result<ReconciliationSession, MaterializeError> {
    let ws = open_workspace(
        repo_root,
        &crate::repository::workspace::git::GitWorkspaceBackend::open(repo_root)
            .map_err(crate::repository::workspace::WorkspaceError::Backend)?,
    )?;

    let mut session = load_reconciliation_session(repo_root, &ws.id, ws.base_revision)?
        .ok_or(MaterializeError::NoSession)?;

    if ws.base_revision != session.base_revision {
        return Err(MaterializeError::StaleSession);
    }

    let candidate = match &session.state {
        ReconciliationSessionState::PreparedClean { .. } => {
            return Err(MaterializeError::PreparedClean);
        }
        ReconciliationSessionState::ConflictedMaterialized { .. } => {
            return Err(MaterializeError::AlreadyMaterialized);
        }
        ReconciliationSessionState::Conflicted { candidate } => candidate.clone(),
    };

    // We only need to materialize if there is a physical candidate
    if let Some(phys) = &candidate.physical_candidate {
        // MAT-03: The restoration checkpoint is the physical snapshot associated with the session's base workspace revision.
        let repo = open_repository(repo_root)?;
        let base_rev_data = repo.read_revision(session.base_revision)?;
        let local_snapshot = base_rev_data.workspace_snapshot;

        // Try materialization
        if let Err(orig_err) = backend.materialize_candidate(&ws.id, phys) {
            // MAT-02: On materialization failure, KAT attempts to restore the exact pre-materialization snapshot.
            if let Err(rec_err) = backend.materialize_snapshot(&local_snapshot) {
                // MAT-05: If restoration fails or cannot be verified, KAT reports an explicit recovery failure and MUST NOT claim retry-safe state.
                return Err(MaterializeError::MaterializationRecoveryFailed {
                    original: orig_err,
                    recovery: rec_err,
                });
            } else {
                // MAT-04: If restoration succeeds and verifies, the session remains Conflicted and materialization is retryable.
                return Err(MaterializeError::MaterializationFailed {
                    original: orig_err,
                    restored: true,
                });
            }
        }
    }

    // MAT-01: Candidate materialization is not considered successful until the resulting physical state is verified.
    session.state = ReconciliationSessionState::ConflictedMaterialized { candidate };

    save_reconciliation_session(repo_root, &ws.id, &session, backend)?;

    Ok(session)
}
