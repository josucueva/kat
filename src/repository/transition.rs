use crate::domain::identity::RepositoryRevisionId;
use crate::domain::workspace::{
    BackendConsistency, PhysicalWorkspaceState, ReconciliationStatus, SemanticWorkspaceState,
    WorkspaceBackend,
};
use crate::repository::error::RepositoryError;
use crate::repository::open::open_repository;
use crate::repository::session::has_draft_session;
use crate::repository::workspace::{
    WorkspaceError, open_workspace, update_workspace_base, workspace_status,
};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum TransitionError {
    #[error("Not in a workspace: {0}")]
    Workspace(#[from] WorkspaceError),
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),
    #[error("Cannot transition: Workspace is not clean.")]
    NotClean,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Encoding error: {0}")]
    Encoding(#[from] crate::encoding::validate::CanonicalStructureError),
    #[error("Decoding error: {0}")]
    Decoding(#[from] crate::encoding::DecodingError),
    #[error("Object store error: {0}")]
    ObjectStore(#[from] crate::repository::object_store::ObjectStoreError),
    #[error("Materialization or Verification failed, rolled back: {0}")]
    MaterializationFailed(String),
}

pub fn transition_workspace_to_revision<B: WorkspaceBackend>(
    repo_root: &Path,
    backend: &B,
    target_revision_id: RepositoryRevisionId,
) -> Result<(), TransitionError> {
    let repo = open_repository(repo_root)?;
    let ws = open_workspace(repo_root, backend)?;
    let status = workspace_status(repo_root, backend)?;

    if has_draft_session(repo_root) {
        return Err(TransitionError::NotClean);
    }

    if status.status.semantic != SemanticWorkspaceState::Clean {
        return Err(TransitionError::NotClean);
    }

    if status.status.physical != PhysicalWorkspaceState::Clean {
        return Err(TransitionError::NotClean);
    }

    if status.status.backend_consistency != BackendConsistency::Consistent {
        return Err(TransitionError::NotClean);
    }

    if status.status.reconciliation != ReconciliationStatus::None {
        return Err(TransitionError::NotClean);
    }

    let store = repo.object_store();

    // Load target revision
    let target_obj = store.get(target_revision_id.as_object_id())?;
    let target_rev = match crate::encoding::decode_canonical(&target_obj)?.payload {
        crate::encoding::object::CanonicalPayload::RepositoryRevision(r) => r,
        _ => {
            return Err(TransitionError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Target is not a revision",
            )));
        }
    };

    // Load current revision for rollback
    let base_obj = store.get(ws.base_revision.as_object_id())?;
    let base_rev = match crate::encoding::decode_canonical(&base_obj)?.payload {
        crate::encoding::object::CanonicalPayload::RepositoryRevision(r) => r,
        _ => {
            return Err(TransitionError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Base is not a revision",
            )));
        }
    };

    // Attempt materialization
    if let Err(e) = backend.materialize_snapshot(&target_rev.workspace_snapshot) {
        let _ = backend.materialize_snapshot(&base_rev.workspace_snapshot);
        return Err(TransitionError::MaterializationFailed(e.to_string()));
    }

    if !backend
        .verify_snapshot_integrity(&target_rev.workspace_snapshot)
        .unwrap_or(false)
    {
        let _ = backend.materialize_snapshot(&base_rev.workspace_snapshot);
        return Err(TransitionError::MaterializationFailed(
            "Snapshot integrity verification failed".to_string(),
        ));
    }

    update_workspace_base(repo_root, &ws.id, target_revision_id)?;

    Ok(())
}
