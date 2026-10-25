//! Credential-free named remote configuration for the CLI.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::identity::RepositoryId;
use crate::repository::http::HttpSemanticTransport;
use crate::repository::workspace::git_transport::{
    GitWorkspaceBinding, GitWorkspaceTransport, WorkspaceBackendId,
};

const REMOTE_CONFIG_VERSION: u32 = 1;

/// Persisted remote endpoints and their immutable repository/backend identity.
/// Runtime bearer tokens are deliberately not part of this structure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteConfig {
    version: u32,
    name: String,
    pub repository_id: RepositoryId,
    pub workspace_backend_id: WorkspaceBackendId,
    pub hub_url: String,
    pub git_url: String,
}

/// Errors reading, validating, or creating named remote configuration.
#[derive(Debug, thiserror::Error)]
pub enum RemoteConfigError {
    #[error(
        "remote name must contain only letters, digits, '.', '_' or '-' and cannot be '.' or '..'"
    )]
    InvalidName,
    #[error("remote configuration already exists and is immutable")]
    AlreadyExists,
    #[error("remote configuration is missing")]
    NotFound,
    #[error("remote configuration is invalid: {0}")]
    Invalid(String),
    #[error("remote configuration I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl RemoteConfig {
    /// Validates a proposed remote before any repository configuration is written.
    pub fn validate(name: &str, hub_url: &str, git_url: &str) -> Result<(), RemoteConfigError> {
        validate_name(name)?;
        HttpSemanticTransport::validate_endpoint(hub_url).map_err(invalid)?;
        GitWorkspaceTransport::validate_authenticated_endpoint(git_url).map_err(invalid)?;
        Ok(())
    }

    /// Creates a named remote configuration without storing credentials.
    pub fn add(
        project_root: &Path,
        name: &str,
        repository_id: RepositoryId,
        workspace_backend_id: WorkspaceBackendId,
        hub_url: String,
        git_url: String,
    ) -> Result<Self, RemoteConfigError> {
        Self::validate(name, &hub_url, &git_url)?;

        let config = Self {
            version: REMOTE_CONFIG_VERSION,
            name: name.to_owned(),
            repository_id,
            workspace_backend_id,
            hub_url,
            git_url,
        };
        let path = config_path(project_root, name)?;
        fs::create_dir_all(path.parent().expect("remote config has a parent"))?;
        let bytes = serde_json::to_vec_pretty(&config).map_err(invalid)?;
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(RemoteConfigError::AlreadyExists);
            }
            Err(error) => return Err(error.into()),
        };
        if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
            let _ = fs::remove_file(path);
            return Err(error.into());
        }
        Ok(config)
    }

    /// Reads and validates one named remote, including its local repository identity.
    pub fn open(
        project_root: &Path,
        name: &str,
        repository_id: RepositoryId,
    ) -> Result<Self, RemoteConfigError> {
        validate_name(name)?;
        let path = config_path(project_root, name)?;
        let bytes = fs::read(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                RemoteConfigError::NotFound
            } else {
                RemoteConfigError::Io(error)
            }
        })?;
        let config: Self = serde_json::from_slice(&bytes).map_err(invalid)?;
        if config.version != REMOTE_CONFIG_VERSION
            || config.name != name
            || config.repository_id != repository_id
        {
            return Err(RemoteConfigError::Invalid(
                "version, name, or repository identity does not match".into(),
            ));
        }
        HttpSemanticTransport::validate_endpoint(&config.hub_url).map_err(invalid)?;
        GitWorkspaceTransport::validate_authenticated_endpoint(&config.git_url).map_err(invalid)?;
        Ok(config)
    }

    /// Reads all named remotes in deterministic name order.
    pub fn list(
        project_root: &Path,
        repository_id: RepositoryId,
    ) -> Result<Vec<Self>, RemoteConfigError> {
        let directory = project_root.join(".kat/remotes");
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut remotes = Vec::new();
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_file()
                || entry.path().extension().is_none_or(|ext| ext != "json")
            {
                continue;
            }
            let name = entry
                .path()
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| RemoteConfigError::Invalid("remote filename is invalid".into()))?
                .to_owned();
            remotes.push(Self::open(project_root, &name, repository_id)?);
        }
        remotes.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(remotes)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn binding(&self) -> GitWorkspaceBinding {
        GitWorkspaceBinding {
            repository_id: self.repository_id,
            workspace_backend_id: self.workspace_backend_id,
        }
    }
}

fn config_path(project_root: &Path, name: &str) -> Result<PathBuf, RemoteConfigError> {
    validate_name(name)?;
    Ok(project_root
        .join(".kat/remotes")
        .join(format!("{name}.json")))
}

fn validate_name(name: &str) -> Result<(), RemoteConfigError> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(RemoteConfigError::InvalidName);
    }
    Ok(())
}

fn invalid(error: impl std::fmt::Display) -> RemoteConfigError {
    RemoteConfigError::Invalid(error.to_string())
}
