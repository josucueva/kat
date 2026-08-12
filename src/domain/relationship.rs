//! Relationship versions (see `spec/canonical-format.cddl`,
//! `relationship-version`).

use crate::domain::identity::{ElementId, RelationshipId};
use crate::domain::property::PropertyValue;

/// One immutable version of a semantic relationship.
///
/// Relationships reference stable semantic identities, not specific versions;
/// the active SemanticState determines which versions are current.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct RelationshipVersion {
    /// Stable identity of the relationship, unchanged across versions.
    pub relationship_id: RelationshipId,
    /// Stable identity of the source element.
    pub source_element_id: ElementId,
    /// Ontology relationship type identifier (e.g. `kat.core/addresses`).
    pub relationship_type: String,
    /// Stable identity of the target element.
    pub target_element_id: ElementId,
    /// Relationship-specific semantic properties (canonical ordered pairs).
    pub properties: Vec<(String, PropertyValue)>,
}
