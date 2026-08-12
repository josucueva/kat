//! Ontology versions (see `spec/canonical-format.cddl`, `ontology-version`).

use crate::domain::identity::OntologyId;

/// One immutable version of the repository ontology.
///
/// Element and relationship type definitions are ordered vectors (matching
/// the canonical sorted-array form), not sets or maps, so that malformed
/// input such as unsorted or duplicate definitions remains observable to the
/// canonical validator.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct OntologyVersion {
    /// Stable identity of the ontology, unchanged across versions.
    pub ontology_id: OntologyId,
    /// Available knowledge element types, canonically sorted by `type_id`.
    pub element_types: Vec<ElementTypeDefinition>,
    /// Available relationship types, canonically sorted by `type_id`.
    pub relationship_types: Vec<RelationshipTypeDefinition>,
}

/// Definition of one knowledge element type.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ElementTypeDefinition {
    /// Stable textual type identifier (e.g. `kat.core/requirement`).
    pub type_id: String,
    /// Human-readable name.
    pub name: String,
}

/// Definition of one relationship type.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct RelationshipTypeDefinition {
    /// Stable textual relationship identifier (e.g. `kat.core/addresses`).
    pub type_id: String,
    /// Human-readable name.
    pub name: String,
    /// Allowed source element types, canonically sorted, unique.
    pub allowed_source_types: Vec<String>,
    /// Allowed target element types, canonically sorted, unique.
    pub allowed_target_types: Vec<String>,
}
