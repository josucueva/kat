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

fn workspace_state_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".kat").join("workspace")
}

fn workspace_state_path(repo_root: &Path) -> PathBuf {
    workspace_state_dir(repo_root).join("state.json")
}

pub fn read_workspace_state(repo_root: &Path) -> Result<Workspace, WorkspaceStateError> {
    let path = workspace_state_path(repo_root);
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
    let dir = workspace_state_dir(repo_root);
    fs::create_dir_all(&dir)?;
    let tmp = dir.join("state.json.tmp");
    let target = workspace_state_path(repo_root);
    let json =
        serde_json::to_string(workspace).map_err(|e| WorkspaceStateError::Parse(e.to_string()))?;

    let mut file = File::create(&tmp)?;
    file.write_all(json.as_bytes())?;
    file.sync_all()?;
    fs::rename(tmp, target)?;
    Ok(())
}
