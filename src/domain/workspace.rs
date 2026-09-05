//! Domain representations for physical workspace semantics.

use std::path::{Path, PathBuf};
use crate::domain::identity::{MaterializationId, WorkspaceSnapshotId};

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

/// Represents the physical working state diff against the snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingState {
    pub backend_consistency: BackendConsistency,
    pub added: Vec<PathBuf>,
    pub modified: Vec<PathBuf>,
    pub deleted: Vec<PathBuf>,
    pub untracked: Vec<PathBuf>,
    pub ignored: Vec<PathBuf>,
}

/// The required capabilities of a physical workspace backend independent of Git.
///
/// Implementations (e.g., GitWorkspaceBackend or FakeWorkspaceBackend) satisfy
/// this trait to provide snapshot capabilities while keeping KAT decoupled from
/// the underlying physical storage mechanism.
pub trait WorkspaceBackend {
    /// Inspects the current physical working tree, identifying modifications.
    fn inspect_working_state(&self, base: &WorkspaceSnapshotId) -> Result<WorkingState, WorkspaceBackendError>;

    /// Creates an immutable physical snapshot representing exactly the `tracked_paths`.
    /// The backend must NOT guess or include other files (like untracked files).
    fn create_snapshot(&self, tracked_paths: &[PathBuf]) -> Result<WorkspaceSnapshotId, WorkspaceBackendError>;

    /// Modifies the physical working tree to match the given snapshot exactly.
    fn materialize_snapshot(&self, id: &WorkspaceSnapshotId) -> Result<(), WorkspaceBackendError>;

    /// Compares two snapshots to find the differing file paths.
    /// (Returns structural diffs, placeholder returning `Vec<PathBuf>`).
    fn compare_snapshots(
        &self,
        base: &WorkspaceSnapshotId,
        target: &WorkspaceSnapshotId,
    ) -> Result<Vec<PathBuf>, WorkspaceBackendError>;

    /// Resolves the canonical identity of a specific path within a snapshot.
    fn resolve_materialization(
        &self,
        path: &Path,
        snapshot: &WorkspaceSnapshotId,
    ) -> Result<MaterializationResolution, WorkspaceBackendError>;

    /// Verifies the internal structural integrity of the referenced snapshot.
    fn verify_snapshot_integrity(&self, id: &WorkspaceSnapshotId) -> Result<bool, WorkspaceBackendError>;
}
