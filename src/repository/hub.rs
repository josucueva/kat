//! Trusted, file-backed KAT Hub semantic service core.
//!
//! This transport is deliberately local and in-process. It exposes no network
//! listener; the caller must restrict the Hub data root to trusted processes.
//! The library does not configure operating-system ACLs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::domain::identity::{ObjectId, RepositoryId, RepositoryRevisionId};
use crate::domain::reference::NamedReference;
use crate::encoding::decode::decode_canonical;
use crate::encoding::object::CanonicalPayload;
use crate::repository::object_store::{ObjectStore, ObjectStoreError};
use crate::repository::ref_store::{FileRefStore, RefStoreError};
use crate::repository::remote::{
    RemoteError, RemoteReferences, RemoteTransportError, SemanticTransport, verify_revision_closure,
};
use crate::repository::workspace::git_transport::{ControlledGitBackend, GitWorkspaceBinding};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HubRepositoryConfig {
    version: u32,
    binding: GitWorkspaceBinding,
    workspace_git_path: PathBuf,
}

/// A trusted local KAT Hub semantic service, bound to one repository and its
/// controlled Git workspace backend.
///
/// The Hub stores exact canonical bytes and named references under its data
/// root. It is intended to run only inside a trusted deployment process; an
/// HTTP or other remote adapter must add authentication before exposing it.
pub struct TrustedHubSemanticTransport {
    root: PathBuf,
    config: HubRepositoryConfig,
}

impl TrustedHubSemanticTransport {
    /// Creates or reopens a repository binding after verifying the controlled
    /// Git backend's active create-only retention policy.
    pub fn provision(
        root: &Path,
        binding: GitWorkspaceBinding,
        workspace_git_path: &Path,
    ) -> Result<Self, RemoteTransportError> {
        fs::create_dir_all(root).map_err(other)?;
        let root = fs::canonicalize(root).map_err(other)?;
        let workspace_git_path = fs::canonicalize(workspace_git_path).map_err(other)?;
        ControlledGitBackend::open(&workspace_git_path, binding)?;

        let config = HubRepositoryConfig {
            version: 1,
            binding,
            workspace_git_path,
        };
        let config_path = config_path(&root, binding.repository_id);
        fs::create_dir_all(config_path.parent().expect("Hub config has a parent"))
            .map_err(other)?;
        let bytes = serde_json::to_vec(&config).map_err(other)?;
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
                if read_config(&config_path)? != config {
                    return Err(other("Hub repository binding is immutable"));
                }
            }
            Err(error) => return Err(other(error)),
        }

        Ok(Self { root, config })
    }

    /// Opens an existing trusted local Hub repository and rechecks its fixed
    /// backend binding and active retention policy.
    pub fn open(root: &Path, repository_id: RepositoryId) -> Result<Self, RemoteTransportError> {
        let root = fs::canonicalize(root).map_err(other)?;
        let config = read_config(&config_path(&root, repository_id))?;
        if config.version != 1 {
            return Err(other("unsupported Hub repository config version"));
        }
        if config.binding.repository_id != repository_id {
            return Err(RemoteTransportError::RepositoryMismatch {
                expected: repository_id,
                actual: config.binding.repository_id,
            });
        }
        ControlledGitBackend::open(&config.workspace_git_path, config.binding)?;
        Ok(Self { root, config })
    }

    fn verify_binding(
        &self,
        repository_id: RepositoryId,
    ) -> Result<ControlledGitBackend, RemoteTransportError> {
        if repository_id != self.config.binding.repository_id {
            return Err(RemoteTransportError::RepositoryMismatch {
                expected: repository_id,
                actual: self.config.binding.repository_id,
            });
        }
        let current = read_config(&config_path(&self.root, repository_id))?;
        if current != self.config {
            return Err(other("Hub repository binding changed after open"));
        }
        ControlledGitBackend::open(&current.workspace_git_path, current.binding)
    }

    fn objects(&self) -> ObjectStore {
        ObjectStore::new(
            repository_path(&self.root, self.config.binding.repository_id).join(".kat"),
        )
    }

    fn refs(&self) -> FileRefStore {
        FileRefStore::new(
            repository_path(&self.root, self.config.binding.repository_id).join(".kat"),
        )
    }

    fn verify_candidate(
        &self,
        target: RepositoryRevisionId,
        backend: &ControlledGitBackend,
    ) -> Result<(), RemoteTransportError> {
        let objects = self.objects();
        let snapshots =
            verify_revision_closure(target, |id| read_object(&objects, id)).map_err(|error| {
                other(format!(
                    "repository revision closure is incomplete: {error}"
                ))
            })?;
        for snapshot in snapshots {
            if !backend.verify_snapshot_integrity(&snapshot)? {
                return Err(RemoteTransportError::SnapshotIntegrity(snapshot));
            }
        }
        Ok(())
    }

    fn revision_inventory(
        &self,
    ) -> Result<BTreeMap<RepositoryRevisionId, Vec<RepositoryRevisionId>>, RemoteTransportError>
    {
        let objects = self.objects();
        let ids = objects.find_by_prefix("").map_err(other)?;
        let mut revisions = BTreeMap::new();
        for id in ids {
            let bytes = objects.get(id).map_err(other)?;
            if let CanonicalPayload::RepositoryRevision(revision) =
                decode_canonical(&bytes).map_err(other)?.payload
            {
                revisions.insert(RepositoryRevisionId::from_object_id(id), revision.parents);
            }
        }
        Ok(revisions)
    }
}

impl SemanticTransport for TrustedHubSemanticTransport {
    fn list_references(
        &self,
        repository_id: RepositoryId,
    ) -> Result<RemoteReferences, RemoteTransportError> {
        let backend = self.verify_binding(repository_id)?;
        let refs = self.refs();
        let mut named = refs.list_local_refs().map_err(other)?;
        named.sort_by(|left, right| left.name.cmp(&right.name));
        for reference in &named {
            refs.validate_ref_name(&format!("local/{}", reference.name))
                .map_err(other)?;
            self.verify_candidate(reference.target, &backend)?;
        }

        let revisions = self.revision_inventory()?;
        let parented = revisions
            .values()
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>();
        let named_targets = named
            .iter()
            .map(|reference| reference.target)
            .collect::<BTreeSet<_>>();
        let mut unnamed_heads = Vec::new();
        for candidate in revisions.keys().copied() {
            if parented.contains(&candidate) || named_targets.contains(&candidate) {
                continue;
            }
            // Incomplete or physically unavailable tips are not advertised.
            if self.verify_candidate(candidate, &backend).is_ok() {
                unnamed_heads.push(candidate);
            }
        }

        Ok(RemoteReferences {
            named: named
                .into_iter()
                .map(|reference| NamedReference {
                    name: reference.name,
                    target: reference.target,
                })
                .collect(),
            unnamed_heads,
        })
    }

    fn fetch_object(
        &self,
        repository_id: RepositoryId,
        id: ObjectId,
    ) -> Result<Option<Vec<u8>>, RemoteTransportError> {
        self.verify_repository(repository_id)?;
        let bytes = match self.objects().get(id) {
            Ok(bytes) => bytes,
            Err(ObjectStoreError::NotFound(_)) => return Ok(None),
            Err(ObjectStoreError::Integrity { .. }) => {
                return Err(RemoteTransportError::ObjectIntegrity(id));
            }
            Err(error) => return Err(other(error)),
        };
        decode_canonical(&bytes).map_err(other)?;
        Ok(Some(bytes))
    }

    fn store_object(
        &self,
        repository_id: RepositoryId,
        id: ObjectId,
        bytes: &[u8],
    ) -> Result<(), RemoteTransportError> {
        self.verify_repository(repository_id)?;
        if crate::encoding::hash::object_id(bytes) != id {
            return Err(RemoteTransportError::ObjectIntegrity(id));
        }
        decode_canonical(bytes).map_err(other)?;
        let stored = self.objects().put(bytes).map_err(other)?;
        if stored != id {
            return Err(RemoteTransportError::ObjectIntegrity(id));
        }
        Ok(())
    }

    fn compare_and_swap_reference(
        &self,
        repository_id: RepositoryId,
        name: &str,
        expected: Option<RepositoryRevisionId>,
        new: RepositoryRevisionId,
    ) -> Result<(), RemoteTransportError> {
        let backend = self.verify_binding(repository_id)?;
        let refs = self.refs();
        let path = format!("local/{name}");
        refs.validate_ref_name(&path).map_err(other)?;
        self.verify_candidate(new, &backend)?;
        refs.compare_and_swap_ref(&path, expected, new)
            .map_err(map_ref_error)
    }
}

impl TrustedHubSemanticTransport {
    fn verify_repository(&self, repository_id: RepositoryId) -> Result<(), RemoteTransportError> {
        if repository_id != self.config.binding.repository_id {
            return Err(RemoteTransportError::RepositoryMismatch {
                expected: repository_id,
                actual: self.config.binding.repository_id,
            });
        }
        if read_config(&config_path(&self.root, repository_id))? != self.config {
            return Err(other("Hub repository binding changed after open"));
        }
        Ok(())
    }
}

fn repository_path(root: &Path, repository_id: RepositoryId) -> PathBuf {
    root.join("repositories").join(repository_id.to_string())
}

fn config_path(root: &Path, repository_id: RepositoryId) -> PathBuf {
    repository_path(root, repository_id).join("hub-repository.json")
}

fn read_config(path: &Path) -> Result<HubRepositoryConfig, RemoteTransportError> {
    let bytes = fs::read(path).map_err(other)?;
    serde_json::from_slice(&bytes).map_err(other)
}

fn read_object(objects: &ObjectStore, id: ObjectId) -> Result<Vec<u8>, RemoteError> {
    match objects.get(id) {
        Ok(bytes) => Ok(bytes),
        Err(ObjectStoreError::NotFound(_)) => Err(RemoteError::MissingRemoteObject(id)),
        Err(ObjectStoreError::Integrity { actual, .. }) => Err(RemoteError::ObjectHashMismatch {
            expected: id,
            actual,
        }),
        Err(error) => Err(RemoteError::ObjectStore(error)),
    }
}

fn map_ref_error(error: RefStoreError) -> RemoteTransportError {
    match error {
        RefStoreError::Conflict => RemoteTransportError::ReferenceConflict,
        error => other(error),
    }
}

fn other(error: impl std::fmt::Display) -> RemoteTransportError {
    RemoteTransportError::Other(error.to_string())
}
