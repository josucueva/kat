//! Workspace backend definitions and implementations.

pub mod fake;
pub mod git;
pub mod state;

use std::path::Path;

use crate::domain::identity::RepositoryRevisionId;
use crate::domain::workspace::{
    BackendConsistency, Workspace, WorkspaceBackend, WorkspaceId, WorkspaceStatus,
};
use crate::repository::error::RepositoryError;
use crate::repository::open::open_repository;
use crate::repository::session::{DraftSessionState, has_draft_session, read_draft_session};
use crate::repository::workspace::git::GitWorkspaceBackend;
use crate::repository::workspace::state::{
    WorkspaceStateError, read_workspace_state, write_workspace_state_atomic,
};

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("Workspace state error: {0}")]
    State(#[from] WorkspaceStateError),
    #[error("Backend error: {0}")]
    Backend(#[from] crate::domain::workspace::WorkspaceBackendError),
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),
    #[error("Workspace snapshot not found for revision {0:?}")]
    SnapshotNotFound(RepositoryRevisionId),
}

/// Initializes a new Workspace in the repository.
pub fn init_workspace(
    repo_root: &Path,
    base_revision: RepositoryRevisionId,
) -> Result<Workspace, WorkspaceError> {
    let ws = Workspace {
        id: WorkspaceId(uuid::Uuid::new_v4().to_string()),
        base_revision,
    };
    write_workspace_state_atomic(repo_root, &ws)?;
    Ok(ws)
}

/// Opens the existing Workspace.
pub fn open_workspace(repo_root: &Path) -> Result<Workspace, WorkspaceError> {
    Ok(read_workspace_state(repo_root)?)
}

/// Computes the combined status of the workspace relative to its base_revision.
pub fn workspace_status(repo_root: &Path) -> Result<WorkspaceStatus, WorkspaceError> {
    let ws = open_workspace(repo_root)?;

    // Check semantic modification
    let semantic_modified = if has_draft_session(repo_root) {
        read_draft_session(repo_root).is_ok_and(|session| session.status == DraftSessionState::Open)
    } else {
        false
    };

    // Check physical modification
    let repo = open_repository(repo_root)?;
    let revision = repo.read_revision(ws.base_revision)?;
    let backend = GitWorkspaceBackend::open(repo_root)?;

    let state = backend.inspect_working_state(&revision.workspace_snapshot)?;

    if let BackendConsistency::Mismatch(desc) = state.backend_consistency {
        return Ok(WorkspaceStatus::BackendMismatch(desc));
    }

    let physical_modified = !state.changes.is_clean();

    match (semantic_modified, physical_modified) {
        (false, false) => Ok(WorkspaceStatus::Clean),
        (true, false) => Ok(WorkspaceStatus::SemanticModified),
        (false, true) => Ok(WorkspaceStatus::PhysicalModified),
        (true, true) => Ok(WorkspaceStatus::CombinedModified),
    }
}
