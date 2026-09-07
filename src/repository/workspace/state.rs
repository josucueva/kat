use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::domain::workspace::Workspace;

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceStateError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Workspace state not found")]
    NotFound,
    #[error("Failed to parse workspace state: {0}")]
    Parse(String),
}

fn current_workspace_pointer(repo_root: &Path) -> PathBuf {
    repo_root.join(".kat").join("current-workspace")
}

fn workspace_state_dir(repo_root: &Path, id: &str) -> PathBuf {
    repo_root.join(".kat").join("workspaces").join(id)
}

fn workspace_state_path(repo_root: &Path, id: &str) -> PathBuf {
    workspace_state_dir(repo_root, id).join("state.json")
}

pub fn read_workspace_state(repo_root: &Path) -> Result<Workspace, WorkspaceStateError> {
    let pointer = current_workspace_pointer(repo_root);
    if !pointer.exists() {
        return Err(WorkspaceStateError::NotFound);
    }
    let current_id = fs::read_to_string(&pointer)?;
    let current_id = current_id.trim();

    let path = workspace_state_path(repo_root, current_id);
    if !path.exists() {
        return Err(WorkspaceStateError::NotFound);
    }
    let json = fs::read_to_string(&path)?;
    serde_json::from_str(&json).map_err(|e| WorkspaceStateError::Parse(e.to_string()))
}

pub fn write_workspace_state_atomic(
    repo_root: &Path,
    workspace: &Workspace,
) -> Result<(), WorkspaceStateError> {
    let dir = workspace_state_dir(repo_root, &workspace.id.0);
    fs::create_dir_all(&dir)?;
    let tmp = dir.join("state.json.tmp");
    let target = workspace_state_path(repo_root, &workspace.id.0);
    let json =
        serde_json::to_string(workspace).map_err(|e| WorkspaceStateError::Parse(e.to_string()))?;

    let mut file = File::create(&tmp)?;
    file.write_all(json.as_bytes())?;
    file.sync_all()?;
    fs::rename(tmp, target)?;

    // Update current workspace pointer
    let pointer = current_workspace_pointer(repo_root);
    let mut ptr_file = File::create(&pointer)?;
    ptr_file.write_all(workspace.id.0.as_bytes())?;
    ptr_file.sync_all()?;

    Ok(())
}
