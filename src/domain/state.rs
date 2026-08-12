//! Semantic states (see `spec/canonical-format.cddl`, `semantic-state`).

use crate::domain::identity::{ElementId, ObjectId, RelationshipId};

/// Logical mapping of one active element to its current version.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ElementStateEntry {
    /// Stable element identity.
    pub element_id: ElementId,
    /// ObjectId of the active KnowledgeElementVersion.
    pub version: ObjectId,
}

/// Logical mapping of one active relationship to its current version.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct RelationshipStateEntry {
    /// Stable relationship identity.
    pub relationship_id: RelationshipId,
    /// ObjectId of the active RelationshipVersion.
    pub version: ObjectId,
}

/// One immutable composition of software knowledge.
///
/// Represented as ordered vectors (matching the canonical sorted-array form)
/// rather than maps, so that malformed input such as unsorted or duplicate
/// entries remains observable to the canonical validator instead of being
/// silently normalized at construction.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct SemanticState {
    /// ObjectId of the OntologyVersion used to interpret this state.
    pub ontology_version: ObjectId,
    /// Active element mappings, canonically sorted by `element_id`, unique.
    pub elements: Vec<ElementStateEntry>,
    /// Active relationship mappings, canonically sorted by `relationship_id`, unique.
    pub relationships: Vec<RelationshipStateEntry>,
}
