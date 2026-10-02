use crate::domain::identity::{ElementId, ObjectId, RelationshipId};

/// Categories of semantic conflicts identified during reconciliation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SemanticConflictKind {
    /// Both histories modified the same identity in incompatible ways.
    ConcurrentModification {
        base_version: Option<ObjectId>,
        local_version: Option<ObjectId>,
        other_version: Option<ObjectId>,
    },
    /// Concurrent operations applied incompatible lifecycle intent (e.g. Update vs Deprecate).
    LifecycleMismatch {
        base_version: Option<ObjectId>,
        local_version: Option<ObjectId>,
        other_version: Option<ObjectId>,
    },
    /// Both histories attempted to modify or interact with the same relationship identity in incompatible ways.
    RelationshipConflict {
        base_version: Option<ObjectId>,
        local_version: Option<ObjectId>,
        other_version: Option<ObjectId>,
    },
    /// Both histories applied supersession logic concurrently leading to incompatible targets.
    SupersessionConflict {
        base_version: Option<ObjectId>,
        local_version: Option<ObjectId>,
        other_version: Option<ObjectId>,
    },
    /// Concurrent operations established competing accountability baselines for an artifact.
    AmbiguousAccountability,
}

/// A first-class conflict representing unresolved concurrent evolution.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SemanticConflict {
    /// A stable local identifier for resolving this conflict during a session.
    pub id: String,
    /// Elements directly involved in or affected by the conflict.
    pub affected_elements: Vec<ElementId>,
    /// Relationships directly involved in or affected by the conflict.
    pub affected_relationships: Vec<RelationshipId>,
    /// The specific category of semantic incompatibility.
    pub kind: SemanticConflictKind,
}

/// A finding resulting from validating a composed semantic candidate.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ValidationFinding {
    pub diagnostic: String,
}

/// Categories of physical materialization conflicts identified during backend physical merge.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum MaterializationConflictKind {
    /// Both histories modified the same physical content incompatibly.
    Content,
    /// One history deleted the file, the other modified it.
    DeleteModify,
    /// Both histories renamed the file differently.
    RenameRename,
    /// Incompatible type changes (e.g. regular file vs symlink).
    TypeChange,
    /// Path collision (e.g. file vs directory).
    PathCollision,
    /// Complex tree-level structural conflicts.
    Tree,
}

/// A first-class conflict representing unresolved physical/materialization evolution.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct MaterializationConflict {
    /// A stable local identifier for resolving this conflict during a session.
    pub id: String,
    /// The specific category of physical incompatibility.
    pub kind: MaterializationConflictKind,
    /// Paths relative to the physical workspace root. Multiple paths may be involved (e.g., in renames).
    pub paths: Vec<std::path::PathBuf>,
}
