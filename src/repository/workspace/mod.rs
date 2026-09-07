//! Workspace backend definitions and implementations.

pub mod fake;
pub mod git;
pub mod state;

use std::path::Path;

use crate::domain::identity::{MaterializationId, RepositoryRevisionId};
use crate::domain::workspace::{
    MaterializationResolution, PhysicalWorkspaceState, SemanticWorkspaceState, Workspace,
    WorkspaceBackend, WorkspaceBackendError, WorkspaceId, WorkspaceStatus,
};
use crate::repository::error::RepositoryError;
use crate::repository::open::open_repository;
use crate::repository::session::{DraftSessionState, read_draft_session};
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
    // Verify the base_revision exists
    let repo = open_repository(repo_root)?;
    let revision = repo.read_revision(base_revision)?;

    // Verify SemanticState exists
    repo.object_store()
        .get(revision.semantic_state.as_object_id())
        .map_err(|_| WorkspaceError::SnapshotNotFound(base_revision))?;

    // Verify WorkspaceSnapshot exists and is structurally sound
    let backend = GitWorkspaceBackend::open(repo_root)?;
    if !backend.verify_snapshot_integrity(&revision.workspace_snapshot)? {
        return Err(WorkspaceError::SnapshotNotFound(base_revision));
    }

    let ws = Workspace {
        id: WorkspaceId(uuid::Uuid::new_v4().to_string()),
        base_revision,
    };
    write_workspace_state_atomic(repo_root, &ws)?;
    Ok(ws)
}

/// Opens the existing Workspace.
pub fn open_workspace(repo_root: &Path) -> Result<Workspace, WorkspaceError> {
    let ws = read_workspace_state(repo_root)?;

    // Verify the base_revision exists
    let repo = open_repository(repo_root)?;
    let revision = repo.read_revision(ws.base_revision)?;

    // Verify SemanticState exists
    repo.object_store()
        .get(revision.semantic_state.as_object_id())
        .map_err(|_| WorkspaceError::SnapshotNotFound(ws.base_revision))?;

    // Verify WorkspaceSnapshot exists and is structurally sound
    let backend = GitWorkspaceBackend::open(repo_root)?;
    if !backend.verify_snapshot_integrity(&revision.workspace_snapshot)? {
        return Err(WorkspaceError::SnapshotNotFound(ws.base_revision));
    }

    Ok(ws)
}

/// Computes the combined status of the workspace relative to its base_revision.
pub fn workspace_status(repo_root: &Path) -> Result<WorkspaceStatus, WorkspaceError> {
    let ws = open_workspace(repo_root)?;
    let repo = open_repository(repo_root)?;
    let revision = repo.read_revision(ws.base_revision)?;

    // Check semantic modification
    let mut semantic = SemanticWorkspaceState::Clean;
    #[allow(clippy::collapsible_if)]
    if let Ok(session) = read_draft_session(repo_root) {
        if session.status == DraftSessionState::Open {
            if session.base_state_id != revision.semantic_state.as_object_id() {
                semantic = SemanticWorkspaceState::BaseMismatch(format!(
                    "Draft session semantic base mismatch: expected {}, got {}",
                    revision.semantic_state.as_object_id(),
                    session.base_state_id
                ));
            } else if !session.operations.is_empty() {
                semantic = SemanticWorkspaceState::Modified;
            }
        }
    }

    // Check physical modification
    let backend = GitWorkspaceBackend::open(repo_root)?;
    let state = backend.inspect_working_state(&revision.workspace_snapshot)?;

    let physical = if state.changes.is_clean() {
        PhysicalWorkspaceState::Clean
    } else {
        PhysicalWorkspaceState::Modified
    };

    Ok(WorkspaceStatus {
        semantic,
        physical,
        backend_consistency: state.backend_consistency,
    })
}

/// Helper function to resolve the materialization of a path against the current physical working tree using standard filesystem APIs.
pub fn fs_resolve_working_materialization(
    project_root: &Path,
    target_path: &Path,
) -> Result<MaterializationResolution, WorkspaceBackendError> {
    if target_path.starts_with(".kat") {
        return Ok(MaterializationResolution::NotFound);
    }

    let full_path = project_root.join(target_path);
    let metadata = match std::fs::symlink_metadata(&full_path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(MaterializationResolution::NotFound);
        }
        Err(e) => return Err(WorkspaceBackendError::Io(e)),
    };

    if metadata.is_symlink() {
        let target = std::fs::read_link(&full_path).map_err(WorkspaceBackendError::Io)?;
        let target_str = target
            .to_str()
            .ok_or_else(|| WorkspaceBackendError::UnsupportedPathEncoding(target.clone()))?;
        let mat_id = crate::encoding::hash::hash_symlink_materialization(target_str.as_bytes());
        return Ok(MaterializationResolution::Symlink(mat_id));
    }

    if metadata.is_file() {
        use std::os::unix::fs::PermissionsExt;
        let is_exec = (metadata.permissions().mode() & 0o111) != 0;
        let bytes = std::fs::read(&full_path).map_err(WorkspaceBackendError::Io)?;
        let mat_id = crate::encoding::hash::hash_file_materialization(is_exec, &bytes);
        return Ok(MaterializationResolution::File(mat_id));
    }

    if metadata.is_dir() {
        let mut entries = Vec::new();
        collect_fs_tree(project_root, target_path, &mut entries)?;
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let mat_id = crate::encoding::hash::hash_directory_materialization(&entries);
        return Ok(MaterializationResolution::Directory(mat_id));
    }

    Ok(MaterializationResolution::NotFound)
}

fn collect_fs_tree(
    project_root: &Path,
    dir: &Path,
    entries: &mut Vec<(String, u8, MaterializationId)>,
) -> Result<(), WorkspaceBackendError> {
    let full_path = project_root.join(dir);
    let read_dir = match std::fs::read_dir(&full_path) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(WorkspaceBackendError::Io(e)),
    };

    for entry in read_dir {
        let entry = entry.map_err(WorkspaceBackendError::Io)?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == ".kat" || name_str == ".git" {
            continue; // Ignore repository markers
        }

        let path = dir.join(&name);
        let path_str = path
            .to_str()
            .ok_or_else(|| WorkspaceBackendError::UnsupportedPathEncoding(path.clone()))?
            .to_string();

        let metadata = entry.metadata().map_err(WorkspaceBackendError::Io)?;
        if metadata.is_symlink() {
            let target = std::fs::read_link(entry.path()).map_err(WorkspaceBackendError::Io)?;
            let target_str = target.to_str().ok_or_else(|| {
                WorkspaceBackendError::UnsupportedPathEncoding(target.clone())
            })?;
            let mat_id = crate::encoding::hash::hash_symlink_materialization(target_str.as_bytes());
            entries.push((path_str, b'S', mat_id));
        } else if metadata.is_dir() {
            collect_fs_tree(project_root, &path, entries)?;
        } else if metadata.is_file() {
            use std::os::unix::fs::PermissionsExt;
            let is_exec = (metadata.permissions().mode() & 0o111) != 0;
            let bytes = std::fs::read(entry.path()).map_err(WorkspaceBackendError::Io)?;
            let mat_id = crate::encoding::hash::hash_file_materialization(is_exec, &bytes);
            let type_byte = if is_exec { b'X' } else { b'F' };
            entries.push((path_str, type_byte, mat_id));
        }
    }
    Ok(())
}
