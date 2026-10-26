use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::domain::identity::{
    ChangeRevisionId, RepositoryRevisionId, SemanticStateId, WorkspaceSnapshotId,
};
use crate::domain::revision::RepositoryRevision;
use crate::domain::workspace::{BackendConsistency, ReconciliationStatus, WorkspaceBackend};
use crate::encoding::cbor::canonical_bytes;
use crate::encoding::object::{CanonicalObject, CanonicalPayload};
use crate::repository::error::RepositoryError;
use crate::repository::open::{Repository, open_repository};
use crate::repository::remote::{RemoteError, verify_revision_closure};
use crate::repository::session::has_draft_session;
use crate::repository::workspace::git::GitWorkspaceBackend;
use crate::repository::workspace::state::{WorkspaceStateError, read_workspace_state};
use crate::repository::workspace::{
    WorkspaceError, open_workspace, update_workspace_base, workspace_status,
};

static NEXT_RECORD_TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),
    #[error("Workspace error: {0}")]
    Workspace(#[from] WorkspaceError),
    #[error("Workspace backend error: {0}")]
    Backend(#[from] crate::domain::workspace::WorkspaceBackendError),
    #[error("Object store error: {0}")]
    ObjectStore(#[from] crate::repository::object_store::ObjectStoreError),
    #[error("Reference store error: {0}")]
    RefStore(#[from] crate::repository::ref_store::RefStoreError),
    #[error("Canonical encoding error: {0}")]
    Encoding(#[from] crate::encoding::validate::CanonicalStructureError),
    #[error("Revision closure verification failed: {0}")]
    Closure(#[from] RemoteError),
    #[error("Workspace state error: {0}")]
    WorkspaceState(#[from] WorkspaceStateError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid record path {path}: {reason}")]
    InvalidPath { path: String, reason: String },
    #[error("there are no semantic or tracked physical changes to record")]
    NoChanges,
    #[error("cannot record while a semantic Change draft is open")]
    DraftOpen,
    #[error("cannot record while a reconciliation is active")]
    ReconciliationActive,
    #[error("the physical backend does not match the workspace base")]
    BackendMismatch,
    #[error("accepted semantic state changed while recording; retry kat record")]
    SemanticHeadChanged,
    #[error("workspace base changed while recording; retry kat record")]
    WorkspaceBaseChanged,
    #[error("accepted semantic state differs from the workspace base but has no Change head")]
    MissingSemanticChange,
    #[error("a pending record transaction is malformed: {0}")]
    InvalidTransaction(String),
    #[error("record failed ({operation}) and recovery failed ({recovery})")]
    RecoveryFailed { operation: String, recovery: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecordOutcome {
    pub revision_id: RepositoryRevisionId,
    pub parent_revision_id: RepositoryRevisionId,
    pub semantic_state_id: SemanticStateId,
    pub workspace_snapshot_id: String,
    pub semantic_change: Option<ChangeRevisionId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PendingRecord {
    version: u32,
    workspace_id: String,
    old_base: RepositoryRevisionId,
    old_snapshot: WorkspaceSnapshotId,
    old_lineage: String,
    new_revision: Option<RepositoryRevisionId>,
    new_snapshot: Option<WorkspaceSnapshotId>,
    new_lineage: Option<String>,
}

fn transaction_path(root: &Path) -> PathBuf {
    root.join(".kat/work/record/pending.json")
}

fn write_transaction(
    root: &Path,
    transaction: &PendingRecord,
    create: bool,
) -> Result<(), RecordError> {
    let path = transaction_path(root);
    fs::create_dir_all(path.parent().expect("record transaction has a parent"))?;
    let bytes = serde_json::to_vec(transaction)
        .map_err(|error| RecordError::InvalidTransaction(error.to_string()))?;
    let temp = path.with_file_name(format!(
        "pending.tmp-{}-{}",
        std::process::id(),
        NEXT_RECORD_TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        if create {
            fs::hard_link(&temp, &path)?;
            fs::remove_file(&temp)
        } else {
            fs::rename(&temp, &path)
        }
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(temp);
        return Err(RecordError::Io(error));
    }
    Ok(())
}

fn read_transaction(root: &Path) -> Result<Option<PendingRecord>, RecordError> {
    let path = transaction_path(root);
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let transaction: PendingRecord = serde_json::from_slice(&bytes)
        .map_err(|error| RecordError::InvalidTransaction(error.to_string()))?;
    if transaction.version != 1 {
        return Err(RecordError::InvalidTransaction(format!(
            "unsupported version {}",
            transaction.version
        )));
    }
    Ok(Some(transaction))
}

fn load_verified_revision(
    repository: &Repository,
    backend: &GitWorkspaceBackend,
    revision_id: RepositoryRevisionId,
) -> Result<RepositoryRevision, RecordError> {
    let revision = repository.read_revision(revision_id)?;
    let snapshots = verify_revision_closure(revision_id, |id| {
        repository.object_store().get(id).map_err(RemoteError::from)
    })?;
    for snapshot in snapshots {
        if !backend.verify_snapshot_integrity(&snapshot)? {
            return Err(
                crate::domain::workspace::WorkspaceBackendError::SnapshotIntegrity(snapshot).into(),
            );
        }
    }
    Ok(revision)
}

fn recover_transaction(
    root: &Path,
    repository: &Repository,
    backend: &GitWorkspaceBackend,
) -> Result<Option<RecordOutcome>, RecordError> {
    let Some(transaction) = read_transaction(root)? else {
        return Ok(None);
    };
    let workspace = read_workspace_state(root)?;
    if workspace.id.0 != transaction.workspace_id {
        return Err(RecordError::InvalidTransaction(
            "workspace identity changed while a record was pending".into(),
        ));
    }

    if workspace.base_revision == transaction.old_base {
        let old_lineage = git2::Oid::from_str(&transaction.old_lineage)
            .map_err(|error| RecordError::InvalidTransaction(error.to_string()))?;
        backend.activate_lineage(&transaction.old_snapshot, old_lineage)?;
        fs::remove_file(transaction_path(root))?;
        return Ok(None);
    }

    let Some(new_revision_id) = transaction.new_revision else {
        return Err(RecordError::InvalidTransaction(
            "workspace moved before the candidate revision was recorded".into(),
        ));
    };
    let Some(new_snapshot) = transaction.new_snapshot else {
        return Err(RecordError::InvalidTransaction(
            "candidate revision has no snapshot identity".into(),
        ));
    };
    let Some(new_lineage) = transaction.new_lineage else {
        return Err(RecordError::InvalidTransaction(
            "candidate revision has no physical lineage".into(),
        ));
    };
    if workspace.base_revision != new_revision_id {
        return Err(RecordError::InvalidTransaction(
            "workspace base matches neither side of the pending record".into(),
        ));
    }

    let revision = load_verified_revision(repository, backend, new_revision_id)?;
    if revision.workspace_snapshot != new_snapshot {
        return Err(RecordError::InvalidTransaction(
            "candidate snapshot does not match its revision".into(),
        ));
    }
    let new_lineage = git2::Oid::from_str(&new_lineage)
        .map_err(|error| RecordError::InvalidTransaction(error.to_string()))?;
    backend.activate_lineage(&new_snapshot, new_lineage)?;
    fs::remove_file(transaction_path(root))?;
    Ok(Some(outcome(new_revision_id, &revision)))
}

fn outcome(revision_id: RepositoryRevisionId, revision: &RepositoryRevision) -> RecordOutcome {
    RecordOutcome {
        revision_id,
        parent_revision_id: revision.parents[0],
        semantic_state_id: revision.semantic_state,
        workspace_snapshot_id: revision.workspace_snapshot.to_hex(),
        semantic_change: revision.semantic_change,
    }
}

pub fn record_revision(
    root: &Path,
    explicit_paths: &[PathBuf],
) -> Result<RecordOutcome, RecordError> {
    let repository = open_repository(root)?;
    let backend = GitWorkspaceBackend::open(root)?;
    if let Some(recovered) = recover_transaction(root, &repository, &backend)? {
        return Ok(recovered);
    }

    let workspace = open_workspace(root, &backend)?;
    let parent = repository.read_revision(workspace.base_revision)?;
    let status = workspace_status(root, &backend)?;
    if has_draft_session(root) {
        return Err(RecordError::DraftOpen);
    }
    if status.status.reconciliation != ReconciliationStatus::None {
        return Err(RecordError::ReconciliationActive);
    }
    if status.status.backend_consistency != BackendConsistency::Consistent {
        return Err(RecordError::BackendMismatch);
    }
    if repository.accepted.state != parent.semantic_state.as_object_id()
        && repository.accepted.change.is_none()
    {
        return Err(RecordError::MissingSemanticChange);
    }

    let old_lineage = backend
        .active_lineage()
        .ok_or(RecordError::BackendMismatch)?;
    let mut tracked = BTreeSet::new();
    for path in backend.snapshot_paths(&parent.workspace_snapshot)? {
        validate_snapshot_path(&path)?;
        if tracked_path_exists(root, &path)? {
            tracked.insert(path);
        }
    }
    for path in explicit_paths {
        let path = validate_explicit_path(root, &backend, path)?;
        tracked.insert(path);
    }

    let mut transaction = PendingRecord {
        version: 1,
        workspace_id: workspace.id.0.clone(),
        old_base: workspace.base_revision,
        old_snapshot: parent.workspace_snapshot.clone(),
        old_lineage: old_lineage.to_string(),
        new_revision: None,
        new_snapshot: None,
        new_lineage: None,
    };
    write_transaction(root, &transaction, true)?;

    let result = (|| {
        let current_workspace = read_workspace_state(root)?;
        if current_workspace.id != workspace.id
            || current_workspace.base_revision != workspace.base_revision
        {
            fs::remove_file(transaction_path(root))?;
            return Err(RecordError::WorkspaceBaseChanged);
        }
        let snapshot = backend.create_snapshot(&tracked.into_iter().collect::<Vec<_>>())?;
        let new_lineage = backend
            .active_lineage()
            .ok_or(RecordError::BackendMismatch)?;
        backend.activate_lineage(&parent.workspace_snapshot, old_lineage)?;

        let semantic_changed = repository.accepted.state != parent.semantic_state.as_object_id();
        if !semantic_changed && snapshot == parent.workspace_snapshot {
            return Err(RecordError::NoChanges);
        }
        let semantic_change = if semantic_changed {
            repository
                .accepted
                .change
                .map(ChangeRevisionId::from_object_id)
                .ok_or(RecordError::MissingSemanticChange)?
                .into()
        } else {
            None
        };
        let revision = RepositoryRevision {
            parents: vec![workspace.base_revision],
            semantic_state: SemanticStateId::from_object_id(repository.accepted.state),
            workspace_snapshot: snapshot.clone(),
            semantic_change,
        };
        let bytes = canonical_bytes(&CanonicalObject {
            payload: CanonicalPayload::RepositoryRevision(revision.clone()),
        })?;
        let revision_id =
            RepositoryRevisionId::from_object_id(repository.object_store().put(&bytes)?);
        load_verified_revision(&repository, &backend, revision_id)?;

        if repository.ref_store().read_accepted()? != repository.accepted {
            return Err(RecordError::SemanticHeadChanged);
        }
        let current_workspace = read_workspace_state(root)?;
        if current_workspace.id != workspace.id
            || current_workspace.base_revision != workspace.base_revision
        {
            return Err(RecordError::WorkspaceBaseChanged);
        }

        transaction.new_revision = Some(revision_id);
        transaction.new_snapshot = Some(snapshot.clone());
        transaction.new_lineage = Some(new_lineage.to_string());
        write_transaction(root, &transaction, false)?;
        backend.activate_lineage(&snapshot, new_lineage)?;
        update_workspace_base(root, &workspace.id, revision_id)?;
        let _ = fs::remove_file(transaction_path(root));
        Ok(outcome(revision_id, &revision))
    })();

    match result {
        Ok(outcome) => Ok(outcome),
        Err(operation) => match recover_transaction(root, &repository, &backend) {
            Ok(Some(recovered)) => Ok(recovered),
            Ok(None) => Err(operation),
            Err(recovery) => Err(RecordError::RecoveryFailed {
                operation: operation.to_string(),
                recovery: recovery.to_string(),
            }),
        },
    }
}

fn validate_snapshot_path(path: &Path) -> Result<(), RecordError> {
    let text = path
        .to_str()
        .ok_or_else(|| invalid_path(path, "path is not valid UTF-8"))?;
    if text.is_empty()
        || text
            .split(['/', '\\'])
            .any(|part| part == "." || part == "..")
    {
        return Err(invalid_path(
            path,
            "path is empty or contains a traversal component",
        ));
    }
    for component in path.components() {
        match component {
            Component::Normal(name)
                if !name.to_str().is_some_and(|name| {
                    name.eq_ignore_ascii_case(".kat") || name.eq_ignore_ascii_case(".git")
                }) => {}
            _ => {
                return Err(invalid_path(
                    path,
                    "repository metadata or unsafe component",
                ));
            }
        }
    }
    Ok(())
}

fn validate_explicit_path(
    root: &Path,
    backend: &GitWorkspaceBackend,
    path: &Path,
) -> Result<PathBuf, RecordError> {
    validate_snapshot_path(path)?;
    let components = path.components().collect::<Vec<_>>();
    let mut current = root.to_path_buf();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current)
            .map_err(|error| invalid_path(path, &format!("parent path is unavailable: {error}")))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(invalid_path(
                path,
                "parent components must be workspace directories",
            ));
        }
    }
    let metadata = fs::symlink_metadata(root.join(path))
        .map_err(|error| invalid_path(path, &format!("path must exist: {error}")))?;
    if !metadata.file_type().is_file() && !metadata.file_type().is_symlink() {
        return Err(invalid_path(
            path,
            "path must be a regular file or symlink; directories are not expanded",
        ));
    }
    if backend.is_ignored(path)? {
        return Err(invalid_path(path, "Git ignore rules exclude this path"));
    }
    let mut normalized = PathBuf::new();
    for component in components {
        normalized.push(component.as_os_str());
    }
    Ok(normalized)
}

fn tracked_path_exists(root: &Path, path: &Path) -> Result<bool, RecordError> {
    let components = path.components().collect::<Vec<_>>();
    let mut current = root.to_path_buf();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(invalid_path(
                    path,
                    "tracked path parent components must be workspace directories",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        }
    }
    match fs::symlink_metadata(root.join(path)) {
        Ok(metadata) if metadata.file_type().is_file() || metadata.file_type().is_symlink() => {
            Ok(true)
        }
        Ok(_) => Err(invalid_path(
            path,
            "tracked path is not a regular file or symlink",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn invalid_path(path: &Path, reason: &str) -> RecordError {
    RecordError::InvalidPath {
        path: path.display().to_string(),
        reason: reason.to_string(),
    }
}
