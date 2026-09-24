//! Domain representations for physical workspace semantics.

use crate::domain::identity::{MaterializationId, RepositoryRevisionId, WorkspaceSnapshotId};
use std::path::{Path, PathBuf};

/// A unique identity for a local workspace.
#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceId(pub String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticWorkspaceState {
    Clean,
    Modified,
    BaseMismatch(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PhysicalWorkspaceState {
    Clean,
    Modified,
}

/// The overall combined divergence status of a workspace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceStatus {
    pub semantic: SemanticWorkspaceState,
    pub physical: PhysicalWorkspaceState,
    pub backend_consistency: BackendConsistency,
}

/// The core domain entity for a workspace, representing its immutable base revision.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub base_revision: RepositoryRevisionId,
}

/// Errors originating from workspace backend operations.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceBackendError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Snapshot not found: {0:?}")]
    SnapshotNotFound(WorkspaceSnapshotId),

    #[error("Integrity check failed for snapshot: {0:?}")]
    SnapshotIntegrity(WorkspaceSnapshotId),

    #[error("Materialization resolution failed: {0}")]
    Resolution(String),

    #[error("Unsupported physical entry type encountered (e.g. gitlink/submodule)")]
    UnsupportedPhysicalEntryType,

    #[error("Ambiguous backend representation for snapshot: {0:?}")]
    AmbiguousBackendRepresentation(WorkspaceSnapshotId),

    #[error("Path encoding is not valid UTF-8: {0:?}")]
    UnsupportedPathEncoding(PathBuf),
}

/// Information about a materialized entity (file, directory, or symlink) inside a snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaterializationResolution {
    /// A regular file and its identity.
    File(MaterializationId),
    /// A directory and its identity.
    Directory(MaterializationId),
    /// A symlink and its identity (based on target bytes).
    Symlink(MaterializationId),
    /// Path did not exist in the snapshot.
    NotFound,
}

/// Represents the alignment between the underlying Git repository state and the KAT base.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendConsistency {
    /// The backend matches the expected KAT physical base.
    Consistent,
    /// The backend base has been moved independently of KAT (e.g., via `git checkout`).
    Mismatch(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalChanges {
    pub added: Vec<PathBuf>,
    pub modified: Vec<PathBuf>,
    pub deleted: Vec<PathBuf>,
    pub untracked: Vec<PathBuf>,
    pub ignored: Vec<PathBuf>,
}

impl PhysicalChanges {
    pub fn is_clean(&self) -> bool {
        self.added.is_empty()
            && self.modified.is_empty()
            && self.deleted.is_empty()
            && self.untracked.is_empty()
    }
}

/// Represents the physical working state diff against the snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingState {
    pub changes: PhysicalChanges,
    pub backend_consistency: BackendConsistency,
}

/// A provisional physical reconciliation result that contains conflicts.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PhysicalReconciliationCandidate {
    pub base: WorkspaceSnapshotId,
    pub local: WorkspaceSnapshotId,
    pub other: WorkspaceSnapshotId,
    pub conflicts: Vec<crate::domain::conflict::MaterializationConflict>,
    /// Backend-specific opaque handle to the provisional merge result.
    pub provisional: crate::domain::identity::PhysicalCandidateId,
}

/// The result of a physical reconciliation operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PhysicalReconciliationResult {
    /// The backend successfully created a merged physical snapshot without conflicts.
    Clean { snapshot: WorkspaceSnapshotId },
    /// The backend encountered physical conflicts and produced a provisional state.
    Conflicted(PhysicalReconciliationCandidate),
}

/// The required capabilities of a physical workspace backend independent of Git.
///
/// Implementations (e.g., GitWorkspaceBackend or FakeWorkspaceBackend) satisfy
/// this trait to provide snapshot capabilities while keeping KAT decoupled from
/// the underlying physical storage mechanism.
pub trait WorkspaceBackend {
    /// Inspects the current physical working tree, identifying modifications.
    fn inspect_working_state(
        &self,
        base: &WorkspaceSnapshotId,
    ) -> Result<WorkingState, WorkspaceBackendError>;

    /// Creates an immutable physical snapshot representing exactly the `tracked_paths`.
    /// The backend must NOT guess or include other files (like untracked files).
    fn create_snapshot(
        &self,
        tracked_paths: &[PathBuf],
    ) -> Result<WorkspaceSnapshotId, WorkspaceBackendError>;

    /// Modifies the physical working tree to match the given snapshot exactly.
    fn materialize_snapshot(&self, id: &WorkspaceSnapshotId) -> Result<(), WorkspaceBackendError>;

    /// Compares two snapshots to find the differing file paths.
    /// Returns the structural physical changes (added, modified, deleted).
    fn compare_snapshots(
        &self,
        base: &WorkspaceSnapshotId,
        target: &WorkspaceSnapshotId,
    ) -> Result<PhysicalChanges, WorkspaceBackendError>;

    /// Resolves the canonical identity of a specific path within a snapshot.
    fn resolve_materialization(
        &self,
        path: &Path,
        snapshot_id: &WorkspaceSnapshotId,
    ) -> Result<MaterializationResolution, WorkspaceBackendError>;

    /// Resolves the canonical identity of a specific path in the current mutable working tree.
    fn resolve_working_materialization(
        &self,
        path: &Path,
    ) -> Result<MaterializationResolution, WorkspaceBackendError>;

    /// Verifies the internal structural integrity of the referenced snapshot.
    fn verify_snapshot_integrity(
        &self,
        id: &WorkspaceSnapshotId,
    ) -> Result<bool, WorkspaceBackendError>;

    /// Attempts to physically reconcile `other` into `local` using `base` as the common ancestor.
    /// This should not modify KAT semantic state or heads. If successful, produces a new physical
    /// snapshot representing the merge. If conflicted, returns the `MaterializationConflict` details.
    fn reconcile_physical(
        &self,
        base: &WorkspaceSnapshotId,
        local: &WorkspaceSnapshotId,
        other: &WorkspaceSnapshotId,
    ) -> Result<PhysicalReconciliationResult, WorkspaceBackendError>;

    /// Persists the physical candidate state to durable storage under the workspace boundary,
    /// and establishes any backend-specific GC protections (e.g. Git refs).
    fn persist_physical_candidate(
        &self,
        workspace_id: &WorkspaceId,
        candidate: &PhysicalReconciliationCandidate,
    ) -> Result<(), WorkspaceBackendError>;
}
