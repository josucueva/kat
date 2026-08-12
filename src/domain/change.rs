//! Change revisions (see `spec/canonical-format.cddl`, `change-revision`).

use crate::domain::identity::{ChangeId, ObjectId};
use crate::domain::operation::Operation;

/// One immutable revision of a logical Change.
///
/// Operation order is semantically meaningful and is preserved. `base_states`
/// and `dependencies` are vectors; canonical ordering/duplicate rules are
/// enforced by the canonical validator rather than normalized at construction.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ChangeRevision {
    /// Stable identity of the logical Change, shared by its revisions.
    pub change_id: ChangeId,
    /// Base SemanticState ObjectIds (at least one for v0.1; single-base).
    pub base_states: Vec<ObjectId>,
    /// SemanticState ObjectId produced by this revision.
    pub result_state: ObjectId,
    /// Ordered semantic operations (at least one; order is semantic).
    pub operations: Vec<Operation>,
    /// Causal ChangeRevision ObjectIds; set-like, sorted, unique.
    pub dependencies: Vec<ObjectId>,
    /// Optional human-readable description (participates in the ObjectId).
    pub description: Option<String>,
}
