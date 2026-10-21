//! Controlled Git storage for KAT workspace snapshots.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use git2::{Direction, PushOptions, RemoteCallbacks, Repository};
use uuid::Uuid;

use crate::domain::identity::{RepositoryId, WorkspaceSnapshotId};
use crate::domain::workspace::{WorkspaceBackend, WorkspaceBackendError};
use crate::encoding::hash::hash_workspace_snapshot;
use crate::repository::metadata::RepositoryMetadata;
use crate::repository::remote::{RemoteTransportError, WorkspaceTransport};
use crate::repository::workspace::git::{GitWorkspaceBackend, collect_git_tree};

const BINDING_FILE: &str = "kat-workspace-binding";
const BINDING_REF: &str = "refs/kat/backend-binding";
const HOOK_FILE: &str = "pre-receive";

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GitWorkspaceBinding {
    pub repository_id: RepositoryId,
    pub workspace_backend_id: WorkspaceBackendId,
}

#[derive(Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedWorkspaceRemote {
    version: u32,
    binding: GitWorkspaceBinding,
    url: String,
}

/// Git snapshot transport pinned to one local repository and workspace backend.
pub struct GitWorkspaceTransport {
    project_root: PathBuf,
    binding: GitWorkspaceBinding,
    url: String,
}

impl GitWorkspaceTransport {
    /// Saves a create-only remote binding for a local KAT repository.
    pub fn configure(
        project_root: &Path,
        url: impl Into<String>,
        binding: GitWorkspaceBinding,
    ) -> Result<Self, RemoteTransportError> {
        let url = url.into();
        if url.trim().is_empty() {
            return Err(other("workspace Git remote URL is empty"));
        }
        verify_local_repository_binding(project_root, binding)?;

        let config_path = workspace_remote_config_path(project_root);
        let saved = SavedWorkspaceRemote {
            version: 1,
            binding,
            url,
        };
        let bytes = serde_json::to_vec(&saved).map_err(other)?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&config_path)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
                    let _ = fs::remove_file(&config_path);
                    return Err(other(error));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = read_saved_workspace_remote(&config_path)?;
                if existing != saved {
                    return Err(other("workspace Git remote binding is immutable"));
                }
            }
            Err(error) => return Err(other(error)),
        }

        Self::open(project_root)
    }

    /// Opens the saved binding after checking it against local repository metadata.
    pub fn open(project_root: &Path) -> Result<Self, RemoteTransportError> {
        let saved = read_saved_workspace_remote(&workspace_remote_config_path(project_root))?;
        if saved.version != 1 || saved.url.trim().is_empty() {
            return Err(other("workspace Git remote configuration is invalid"));
        }
        verify_local_repository_binding(project_root, saved.binding)?;
        GitWorkspaceBackend::open(project_root).map_err(other)?;
        Ok(Self {
            project_root: project_root.to_path_buf(),
            binding: saved.binding,
            url: saved.url,
        })
    }

    pub fn binding(&self) -> GitWorkspaceBinding {
        self.binding
    }

    fn list_snapshot_refs(
        &self,
        repo: &Repository,
        id: &WorkspaceSnapshotId,
    ) -> Result<Vec<(String, git2::Oid)>, RemoteTransportError> {
        self.verify_remote_binding()?;
        let prefix = snapshot_ref_prefix(id);
        let mut remote = repo.remote_anonymous(&self.url).map_err(other)?;
        remote.connect(Direction::Fetch).map_err(other)?;
        let advertised = remote
            .list()
            .map_err(other)?
            .iter()
            .filter_map(|head| {
                head.name()
                    .strip_prefix(&prefix)
                    .map(|suffix| (head.name().to_owned(), suffix.to_owned(), head.oid()))
            })
            .collect::<Vec<_>>();
        remote.disconnect().map_err(other)?;

        let mut refs = advertised
            .into_iter()
            .map(|(name, suffix, target)| {
                let oid = suffix
                    .parse::<git2::Oid>()
                    .map_err(|_| RemoteTransportError::SnapshotIntegrity(id.clone()))?;
                if oid.to_string() != suffix || oid != target {
                    return Err(RemoteTransportError::SnapshotIntegrity(id.clone()));
                }
                Ok((name, oid))
            })
            .collect::<Result<Vec<_>, _>>()?;
        refs.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(refs)
    }

    fn verify_remote_binding(&self) -> Result<(), RemoteTransportError> {
        let temp = TemporaryGitRepository::new()?;
        let repo = Repository::open_bare(temp.path()).map_err(other)?;
        let mut remote = repo.remote_anonymous(&self.url).map_err(other)?;
        remote.connect(Direction::Fetch).map_err(other)?;
        let target = remote
            .list()
            .map_err(other)?
            .iter()
            .find(|head| head.name() == BINDING_REF)
            .map(|head| head.oid());
        remote.disconnect().map_err(other)?;
        let target = target.ok_or_else(|| other("workspace Git backend binding is missing"))?;
        let refspec = format!("{BINDING_REF}:{BINDING_REF}");
        remote
            .fetch(&[refspec], None, Some("KAT workspace binding verification"))
            .map_err(other)?;
        let reference = repo.find_reference(BINDING_REF).map_err(other)?;
        if reference.kind() != Some(git2::ReferenceType::Direct)
            || reference.target() != Some(target)
        {
            return Err(other(
                "workspace Git backend binding ref changed during verification",
            ));
        }
        let blob = repo.find_blob(target).map_err(other)?;
        let contents = std::str::from_utf8(blob.content()).map_err(other)?;
        if contents != binding_contents(self.binding) {
            if let Some(actual) = parse_repository_id(contents)
                && actual != self.binding.repository_id
            {
                return Err(RemoteTransportError::RepositoryMismatch {
                    expected: self.binding.repository_id,
                    actual,
                });
            }
            return Err(other("workspace Git backend binding mismatch"));
        }
        Ok(())
    }

    fn fetch_snapshot_refs(
        &self,
        repo: &Repository,
        refs: &[(String, git2::Oid)],
    ) -> Result<(), RemoteTransportError> {
        let destinations = refs
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        self.fetch_snapshot_refs_to(repo, refs, &destinations)
    }

    fn fetch_snapshot_refs_to(
        &self,
        repo: &Repository,
        refs: &[(String, git2::Oid)],
        destinations: &[String],
    ) -> Result<(), RemoteTransportError> {
        if refs.len() != destinations.len() {
            return Err(other("snapshot ref transfer plan is inconsistent"));
        }
        let refspecs = refs
            .iter()
            .zip(destinations)
            .map(|((source, _), destination)| format!("{source}:{destination}"))
            .collect::<Vec<_>>();
        let mut remote = repo.remote_anonymous(&self.url).map_err(other)?;
        remote
            .fetch(&refspecs, None, Some("KAT workspace snapshot transfer"))
            .map_err(other)
    }
}

impl WorkspaceTransport for GitWorkspaceTransport {
    fn local_snapshot_available(
        &self,
        id: &WorkspaceSnapshotId,
    ) -> Result<bool, RemoteTransportError> {
        let backend = GitWorkspaceBackend::open(&self.project_root).map_err(other)?;
        match backend.verify_snapshot_integrity(id) {
            Ok(verified) => Ok(verified),
            Err(WorkspaceBackendError::SnapshotNotFound(_)) => Ok(false),
            Err(_) => Err(RemoteTransportError::SnapshotIntegrity(id.clone())),
        }
    }

    fn remote_snapshot_available(
        &self,
        id: &WorkspaceSnapshotId,
    ) -> Result<bool, RemoteTransportError> {
        let temp = TemporaryGitRepository::new()?;
        let repo = Repository::open_bare(temp.path()).map_err(other)?;
        let refs = self.list_snapshot_refs(&repo, id)?;
        if refs.is_empty() {
            return Ok(false);
        }
        self.fetch_snapshot_refs(&repo, &refs)?;
        verify_fetched_snapshot(&repo, id, &refs)?;
        Ok(true)
    }

    fn fetch_snapshot(&self, id: &WorkspaceSnapshotId) -> Result<(), RemoteTransportError> {
        let repo_path = self.project_root.join(".kat/physical/git");
        let repo = Repository::open_bare(repo_path).map_err(other)?;
        let refs = self.list_snapshot_refs(&repo, id)?;
        if refs.is_empty() {
            return Err(RemoteTransportError::SnapshotIntegrity(id.clone()));
        }
        let staging_prefix = format!("refs/kat/staging/{}/", Uuid::new_v4());
        let staged = refs
            .iter()
            .map(|(_, commit)| (format!("{staging_prefix}{commit}"), *commit))
            .collect::<Vec<_>>();
        let result = (|| {
            let destinations = staged
                .iter()
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>();
            self.fetch_snapshot_refs_to(&repo, &refs, &destinations)?;
            verify_fetched_snapshot(&repo, id, &staged)?;
            for (_, commit) in &refs {
                GitWorkspaceBackend::record_snapshot_representation(
                    &self.project_root,
                    id,
                    *commit,
                )
                .map_err(other)?;
            }
            Ok::<(), RemoteTransportError>(())
        })();
        for (name, _) in &staged {
            if let Ok(mut reference) = repo.find_reference(name) {
                let _ = reference.delete();
            }
        }
        result?;
        if !self.local_snapshot_available(id)? {
            return Err(RemoteTransportError::SnapshotIntegrity(id.clone()));
        }
        Ok(())
    }

    fn publish_snapshot(&self, id: &WorkspaceSnapshotId) -> Result<(), RemoteTransportError> {
        if !self.local_snapshot_available(id)? {
            return Err(RemoteTransportError::SnapshotIntegrity(id.clone()));
        }
        let reps = GitWorkspaceBackend::get_snapshot_representations(&self.project_root, id)
            .map_err(other)?;
        let repo =
            Repository::open_bare(self.project_root.join(".kat/physical/git")).map_err(other)?;
        let refs = reps
            .iter()
            .map(|rep| (snapshot_ref_name(id, rep.commit), rep.commit))
            .collect::<Vec<_>>();
        let advertised = self.list_snapshot_refs(&repo, id)?;
        for (name, target) in &refs {
            match advertised
                .iter()
                .find(|(advertised_name, _)| advertised_name == name)
            {
                Some((_, advertised_target)) if advertised_target == target => continue,
                Some(_) => return Err(RemoteTransportError::SnapshotIntegrity(id.clone())),
                None => {}
            }
            let refspec = format!("{name}:{name}");
            let mut remote = repo.remote_anonymous(&self.url).map_err(other)?;
            let mut rejection = None;
            {
                let mut callbacks = RemoteCallbacks::new();
                // libgit2 reports receive-hook rejections through this callback.
                callbacks.push_update_reference(|refname, status| {
                    if let Some(status) = status {
                        rejection = Some(format!("{refname}: {status}"));
                    }
                    Ok(())
                });
                let mut options = PushOptions::new();
                options.remote_callbacks(callbacks);
                remote.push(&[refspec], Some(&mut options)).map_err(other)?;
            }
            if let Some(rejection) = rejection {
                match self
                    .list_snapshot_refs(&repo, id)?
                    .into_iter()
                    .find(|(advertised_name, _)| advertised_name == name)
                {
                    Some((_, advertised_target)) if advertised_target == *target => continue,
                    Some(_) => return Err(RemoteTransportError::SnapshotIntegrity(id.clone())),
                    None => {
                        return Err(other(format!(
                            "remote rejected workspace lineage ref: {rejection}"
                        )));
                    }
                }
            }
        }

        if !self.remote_snapshot_available(id)? {
            return Err(RemoteTransportError::SnapshotIntegrity(id.clone()));
        }
        Ok(())
    }
}

struct TemporaryGitRepository(PathBuf);

impl TemporaryGitRepository {
    fn new() -> Result<Self, RemoteTransportError> {
        let path =
            std::env::temp_dir().join(format!("kat-workspace-verify-{}.git", Uuid::new_v4()));
        Repository::init_bare(&path).map_err(other)?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryGitRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn verify_local_repository_binding(
    project_root: &Path,
    binding: GitWorkspaceBinding,
) -> Result<(), RemoteTransportError> {
    let metadata =
        RepositoryMetadata::read(&project_root.join(".kat/repository.toml")).map_err(other)?;
    if metadata.repository_id != binding.repository_id {
        return Err(RemoteTransportError::RepositoryMismatch {
            expected: metadata.repository_id,
            actual: binding.repository_id,
        });
    }
    Ok(())
}

fn workspace_remote_config_path(project_root: &Path) -> PathBuf {
    project_root
        .join(".kat/physical")
        .join("workspace-remote.json")
}

fn read_saved_workspace_remote(path: &Path) -> Result<SavedWorkspaceRemote, RemoteTransportError> {
    let bytes = fs::read(path).map_err(other)?;
    serde_json::from_slice(&bytes).map_err(other)
}

fn snapshot_ref_prefix(id: &WorkspaceSnapshotId) -> String {
    format!("refs/kat/lineage/{}/", id.to_hex())
}

fn snapshot_ref_name(id: &WorkspaceSnapshotId, commit: git2::Oid) -> String {
    format!("{}{commit}", snapshot_ref_prefix(id))
}

fn verify_fetched_snapshot(
    repo: &Repository,
    id: &WorkspaceSnapshotId,
    refs: &[(String, git2::Oid)],
) -> Result<(), RemoteTransportError> {
    for (name, expected_oid) in refs {
        let reference = repo
            .find_reference(name)
            .map_err(|_| RemoteTransportError::SnapshotIntegrity(id.clone()))?;
        if reference.kind() != Some(git2::ReferenceType::Direct)
            || reference.target() != Some(*expected_oid)
        {
            return Err(RemoteTransportError::SnapshotIntegrity(id.clone()));
        }
        let commit = repo
            .find_commit(*expected_oid)
            .map_err(|_| RemoteTransportError::SnapshotIntegrity(id.clone()))?;
        let tree = commit
            .tree()
            .map_err(|_| RemoteTransportError::SnapshotIntegrity(id.clone()))?;
        let mut entries = Vec::new();
        collect_git_tree(repo, &tree, Path::new(""), &mut entries)
            .map_err(|_| RemoteTransportError::SnapshotIntegrity(id.clone()))?;
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        if hash_workspace_snapshot(&entries) != *id {
            return Err(RemoteTransportError::SnapshotIntegrity(id.clone()));
        }
    }
    Ok(())
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
            let binding_blob = repo
                .blob(binding_contents(binding).as_bytes())
                .map_err(other)?;
            repo.reference(
                BINDING_REF,
                binding_blob,
                true,
                "KAT workspace backend binding",
            )
            .map_err(other)?;
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
        let binding_ref = repo.find_reference(BINDING_REF).map_err(other)?;
        let binding_blob = binding_ref
            .target()
            .and_then(|target| repo.find_blob(target).ok())
            .ok_or_else(|| other("controlled Git backend binding ref is invalid"))?;
        if binding_blob.content() != binding_contents(expected).as_bytes() {
            return Err(other("controlled Git backend binding ref has changed"));
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
