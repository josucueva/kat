//! Create a local developer workspace from a configured remote reference.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

use crate::domain::identity::{RepositoryId, RepositoryRevisionId};
use crate::domain::workspace::WorkspaceBackend;
use crate::encoding::decode_canonical;
use crate::encoding::object::CanonicalPayload;
use crate::repository::http::HttpSemanticTransport;
use crate::repository::metadata::RepositoryMetadata;
use crate::repository::object_store::ObjectStore;
use crate::repository::open::open_repository;
use crate::repository::ref_store::{AcceptedRef, FileRefStore};
use crate::repository::remote::RemoteCoordinator;
use crate::repository::remote_config::RemoteConfig;
use crate::repository::workspace::git::GitWorkspaceBackend;
use crate::repository::workspace::git_transport::GitWorkspaceTransport;
use crate::repository::workspace::{init_workspace, open_workspace};

static NEXT_CLONE_TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, thiserror::Error)]
pub enum CloneError {
    #[error("clone destination must have an existing parent and a new final path component")]
    InvalidDestination,
    #[error("clone destination cannot contain or replace its source repository")]
    OverlappingDestination,
    #[error("invalid remote reference: {0}")]
    InvalidReference(String),
    #[error("remote fetch completed with tracking-reference failures")]
    PartialFetch,
    #[error("remote {remote} has no named reference {reference}")]
    ReferenceNotFound { remote: String, reference: String },
    #[error("remote reference does not identify a RepositoryRevision")]
    InvalidRevision,
    #[error("clone destination already exists: {0}")]
    DestinationExists(String),
    #[error("clone failed: {0}")]
    Operation(String),
    #[error("clone failed ({operation}) and staging cleanup failed ({cleanup})")]
    CleanupFailed { operation: String, cleanup: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CloneOutcome {
    pub repository_id: RepositoryId,
    pub reference: String,
    pub revision_id: RepositoryRevisionId,
    pub destination: String,
}

/// Clone one configured remote reference into a new sibling staging directory.
///
/// The source repository supplies immutable repository metadata and the
/// credential-free remote binding. Remote content is fetched and verified
/// before the finished workspace is atomically renamed into place.
pub fn clone_repository(
    source_root: &Path,
    remote: &RemoteConfig,
    reference: &str,
    destination: &Path,
    token: &str,
) -> Result<CloneOutcome, CloneError> {
    let source = open_repository(source_root).map_err(operation)?;
    if remote.repository_id != source.metadata.repository_id {
        return Err(CloneError::Operation(
            "remote RepositoryId does not match the source repository".into(),
        ));
    }
    let source_workspace = GitWorkspaceTransport::open(source_root).map_err(operation)?;
    if source_workspace.binding() != remote.binding() || source_workspace.url() != remote.git_url {
        return Err(CloneError::Operation(
            "configured remote differs from the source repository's immutable workspace binding"
                .into(),
        ));
    }
    let refs = FileRefStore::new(source_root.join(".kat"));
    refs.validate_ref_name(&format!("local/{reference}"))
        .map_err(|error| CloneError::InvalidReference(error.to_string()))?;

    let source_root = fs::canonicalize(source_root).map_err(operation)?;
    let (destination, parent) = destination_path(destination)?;
    if destination.starts_with(&source_root) || source_root.starts_with(&destination) {
        return Err(CloneError::OverlappingDestination);
    }
    match fs::symlink_metadata(&destination) {
        Ok(_) => {
            return Err(CloneError::DestinationExists(
                destination.display().to_string(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(operation(error)),
    }

    let staging = create_staging_directory(&parent)?;
    let result = populate_clone(&staging, &source.metadata, remote, reference, token);
    if let Err(operation) = result {
        return match fs::remove_dir_all(&staging) {
            Ok(()) => Err(operation),
            Err(cleanup) => Err(CloneError::CleanupFailed {
                operation: operation.to_string(),
                cleanup: cleanup.to_string(),
            }),
        };
    }
    fs::rename(&staging, &destination).map_err(|error| {
        let cleanup = fs::remove_dir_all(&staging).err();
        match cleanup {
            Some(cleanup) => CloneError::CleanupFailed {
                operation: error.to_string(),
                cleanup: cleanup.to_string(),
            },
            None => operation(error),
        }
    })?;

    let result = (|| {
        let repository = open_repository(&destination).map_err(operation)?;
        let workspace = GitWorkspaceBackend::open(&destination).map_err(operation)?;
        open_workspace(&destination, &workspace).map_err(operation)?;
        let revision_id = FileRefStore::new(destination.join(".kat"))
            .read_ref(&format!("local/{reference}"))
            .map_err(operation)?;
        Ok(CloneOutcome {
            repository_id: repository.metadata.repository_id,
            reference: reference.to_owned(),
            revision_id,
            destination: destination.display().to_string(),
        })
    })();
    match result {
        Ok(outcome) => Ok(outcome),
        Err(error) => match fs::remove_dir_all(&destination) {
            Ok(()) => Err(error),
            Err(cleanup) => Err(CloneError::CleanupFailed {
                operation: error.to_string(),
                cleanup: cleanup.to_string(),
            }),
        },
    }
}

fn populate_clone(
    root: &Path,
    metadata: &RepositoryMetadata,
    remote: &RemoteConfig,
    reference: &str,
    token: &str,
) -> Result<(), CloneError> {
    let kat_dir = root.join(".kat");
    for directory in ["objects", "refs", "locks", "tmp"] {
        fs::create_dir_all(kat_dir.join(directory)).map_err(operation)?;
    }
    metadata
        .write(&kat_dir.join("repository.toml"))
        .map_err(operation)?;

    RemoteConfig::add(
        root,
        remote.name(),
        remote.repository_id,
        remote.workspace_backend_id,
        remote.hub_url.clone(),
        remote.git_url.clone(),
    )
    .map_err(operation)?;
    GitWorkspaceBackend::init(root).map_err(operation)?;
    GitWorkspaceTransport::configure(root, remote.git_url.clone(), remote.binding())
        .map_err(operation)?;
    let workspace =
        GitWorkspaceTransport::open_with_runtime_token(root, token).map_err(operation)?;
    let semantic = HttpSemanticTransport::new(&remote.hub_url, token).map_err(operation)?;
    let objects = ObjectStore::new(&kat_dir);
    let references = FileRefStore::new(&kat_dir);
    let fetched = RemoteCoordinator::new(
        metadata.repository_id,
        remote.name(),
        &objects,
        &references,
        &semantic,
        &workspace,
    )
    .fetch()
    .map_err(operation)?;
    if !fetched.failed.is_empty() {
        return Err(CloneError::PartialFetch);
    }

    let revision_id = references
        .read_ref(&format!("remotes/{}/{reference}", remote.name()))
        .map_err(|error| match error {
            crate::repository::ref_store::RefStoreError::NotFound => {
                CloneError::ReferenceNotFound {
                    remote: remote.name().to_owned(),
                    reference: reference.to_owned(),
                }
            }
            error => operation(error),
        })?;
    let bytes = objects.get(revision_id.as_object_id()).map_err(operation)?;
    let revision = match decode_canonical(&bytes).map_err(operation)?.payload {
        CanonicalPayload::RepositoryRevision(revision) => revision,
        _ => return Err(CloneError::InvalidRevision),
    };
    references
        .init_accepted(&AcceptedRef {
            state: revision.semantic_state.as_object_id(),
            change: revision.semantic_change.map(|change| change.as_object_id()),
        })
        .map_err(operation)?;
    references
        .compare_and_swap_ref(&format!("local/{reference}"), None, revision_id)
        .map_err(operation)?;

    let backend = GitWorkspaceBackend::open(root).map_err(operation)?;
    backend
        .materialize_snapshot(&revision.workspace_snapshot)
        .map_err(operation)?;
    init_workspace(root, revision_id).map_err(operation)?;
    let repository = open_repository(root).map_err(operation)?;
    let verified = repository.read_revision(revision_id).map_err(operation)?;
    if verified != revision
        || !backend
            .verify_snapshot_integrity(&revision.workspace_snapshot)
            .map_err(operation)?
    {
        return Err(CloneError::InvalidRevision);
    }
    Ok(())
}

fn destination_path(destination: &Path) -> Result<(PathBuf, PathBuf), CloneError> {
    let name = destination
        .file_name()
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        .ok_or(CloneError::InvalidDestination)?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent).map_err(|_| CloneError::InvalidDestination)?;
    Ok((parent.join(name), parent))
}

fn create_staging_directory(parent: &Path) -> Result<PathBuf, CloneError> {
    for _ in 0..100 {
        let id = NEXT_CLONE_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".kat-clone-{}-{id}", std::process::id()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(operation(error)),
        }
    }
    Err(CloneError::Operation(
        "could not allocate a clone staging directory".into(),
    ))
}

fn operation(error: impl std::fmt::Display) -> CloneError {
    CloneError::Operation(error.to_string())
}
