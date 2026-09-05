//! Repository revisions (see `spec/canonical-format.cddl`, `repository-revision`).

use crate::domain::identity::{ObjectId, RepositoryRevisionId, WorkspaceSnapshotId, SemanticStateId, ChangeRevisionId};

/// The core version-control unit binding the semantic state to the physical workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryRevision {
    /// Ordered canonical parents (0 for initial, 1 for normal, >1 for reconciliation).
    pub parents: Vec<RepositoryRevisionId>,
    /// The canonical identity of the accepted SemanticState.
    pub semantic_state: SemanticStateId,
    /// The deterministic identity of the tracked physical content.
    pub workspace_snapshot: WorkspaceSnapshotId,
    /// The canonical identity of the ChangeRevision that produced this state, if any.
    pub semantic_change: Option<ChangeRevisionId>,
}
