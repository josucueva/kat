//! Controlled Git storage for KAT workspace snapshots.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use git2::Repository;
use uuid::Uuid;

use crate::domain::identity::RepositoryId;
use crate::repository::remote::RemoteTransportError;

const BINDING_FILE: &str = "kat-workspace-binding";
const HOOK_FILE: &str = "pre-receive";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceBackendId(Uuid);

impl WorkspaceBackendId {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for WorkspaceBackendId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GitWorkspaceBinding {
    pub repository_id: RepositoryId,
    pub workspace_backend_id: WorkspaceBackendId,
}

/// A bare Git repository with a fixed KAT binding and create-only lineage refs.
pub struct ControlledGitBackend {
    path: PathBuf,
    binding: GitWorkspaceBinding,
}

impl ControlledGitBackend {
    /// Creates and configures a new KAT-controlled bare repository.
    pub fn provision(
        path: &Path,
        binding: GitWorkspaceBinding,
    ) -> Result<Self, RemoteTransportError> {
        if path.exists() {
            return Err(RemoteTransportError::Other(format!(
                "controlled Git backend path already exists: {}",
                path.display()
            )));
        }
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(other)?;
        }
        fs::create_dir(path).map_err(other)?;

        let result = (|| {
            let repo = Repository::init_bare(path).map_err(other)?;
            fs::write(path.join(BINDING_FILE), binding_contents(binding)).map_err(other)?;
            fs::write(path.join("git-daemon-export-ok"), b"").map_err(other)?;

            let hooks_path = path.join("hooks");
            fs::create_dir_all(&hooks_path).map_err(other)?;
            let hook_path = hooks_path.join(HOOK_FILE);
            let mut hook = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&hook_path)
                .map_err(other)?;
            hook.write_all(hook_contents(binding).as_bytes())
                .map_err(other)?;
            hook.sync_all().map_err(other)?;
            set_executable(&hook_path)?;

            let mut config = repo.config().map_err(other)?;
            config
                .set_str(
                    "core.hooksPath",
                    hooks_path
                        .to_str()
                        .ok_or_else(|| other("controlled Git backend path is not valid UTF-8"))?,
                )
                .map_err(other)?;
            config
                .set_bool("receive.denyDeletes", true)
                .map_err(other)?;
            Ok::<(), RemoteTransportError>(())
        })();

        if let Err(error) = result {
            let _ = fs::remove_dir_all(path);
            return Err(error);
        }

        Self::open(path, binding)
    }

    /// Opens a backend only when its repository binding and retention policy match.
    pub fn open(path: &Path, expected: GitWorkspaceBinding) -> Result<Self, RemoteTransportError> {
        let repo = Repository::open_bare(path).map_err(other)?;
        let contents = fs::read_to_string(path.join(BINDING_FILE)).map_err(other)?;
        let actual_repository = parse_repository_id(&contents)
            .ok_or_else(|| other("controlled Git backend binding is invalid"))?;
        if actual_repository != expected.repository_id {
            return Err(RemoteTransportError::RepositoryMismatch {
                expected: expected.repository_id,
                actual: actual_repository,
            });
        }
        if contents != binding_contents(expected) {
            return Err(other("workspace backend binding mismatch"));
        }
        if fs::read_to_string(path.join("hooks").join(HOOK_FILE)).map_err(other)?
            != hook_contents(expected)
        {
            return Err(other("controlled Git lineage policy has changed"));
        }
        verify_executable(&path.join("hooks").join(HOOK_FILE))?;
        if !path.join("git-daemon-export-ok").is_file() {
            return Err(other("controlled Git backend is not exported"));
        }

        let config = repo.config().map_err(other)?;
        let hooks_path = path
            .join("hooks")
            .to_str()
            .ok_or_else(|| other("controlled Git backend path is not valid UTF-8"))?
            .to_owned();
        if config.get_string("core.hooksPath").map_err(other)? != hooks_path
            || !config.get_bool("receive.denyDeletes").map_err(other)?
        {
            return Err(other("controlled Git receive policy is not active"));
        }

        Ok(Self {
            path: path.to_path_buf(),
            binding: expected,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn binding(&self) -> GitWorkspaceBinding {
        self.binding
    }
}

fn binding_contents(binding: GitWorkspaceBinding) -> String {
    format!(
        "version=1\nrepository_id={}\nworkspace_backend_id={}\n",
        binding.repository_id, binding.workspace_backend_id
    )
}

fn hook_contents(binding: GitWorkspaceBinding) -> String {
    format!(
        "#!/bin/sh\n\
set -eu\n\
binding_file=\"$(git rev-parse --absolute-git-dir)/{BINDING_FILE}\"\n\
grep -Fqx 'version=1' \"$binding_file\"\n\
grep -Fqx 'repository_id={}' \"$binding_file\"\n\
grep -Fqx 'workspace_backend_id={}' \"$binding_file\"\n\
while IFS=' ' read -r old new ref; do\n\
    case \"$ref\" in\n\
        refs/kat/lineage/*) ;;\n\
        *) echo 'KAT workspace backend accepts only lineage refs' >&2; exit 1 ;;\n\
    esac\n\
    zero=\"$(printf '%0*d' \"${{#old}}\" 0)\"\n\
    if [ \"$old\" != \"$zero\" ] || [ \"$new\" = \"$zero\" ]; then\n\
        echo 'KAT lineage refs are create-only' >&2\n\
        exit 1\n\
    fi\n\
    git cat-file -e \"$new^{{commit}}\"\ndone\n",
        binding.repository_id, binding.workspace_backend_id
    )
}

fn parse_repository_id(contents: &str) -> Option<RepositoryId> {
    contents
        .lines()
        .find_map(|line| line.strip_prefix("repository_id="))?
        .parse()
        .ok()
}

fn other(error: impl std::fmt::Display) -> RemoteTransportError {
    RemoteTransportError::Other(error.to_string())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), RemoteTransportError> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path).map_err(other)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(other)
}

#[cfg(unix)]
fn verify_executable(path: &Path) -> Result<(), RemoteTransportError> {
    use std::os::unix::fs::PermissionsExt;

    if fs::metadata(path).map_err(other)?.permissions().mode() & 0o111 == 0 {
        return Err(other("controlled Git receive hook is not executable"));
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), RemoteTransportError> {
    Err(other(
        "controlled Git receive hooks currently require a Unix-like host",
    ))
}

#[cfg(not(unix))]
fn verify_executable(_path: &Path) -> Result<(), RemoteTransportError> {
    Err(other(
        "controlled Git receive hooks currently require a Unix-like host",
    ))
}
