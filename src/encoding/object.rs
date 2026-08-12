//! Canonical object envelope and object kinds
//! (see `spec/canonical-format.cddl`, `canonical-object`).

use crate::domain::change::ChangeRevision;
use crate::domain::element::KnowledgeElementVersion;
use crate::domain::ontology::OntologyVersion;
use crate::domain::relationship::RelationshipVersion;
use crate::domain::state::SemanticState;

/// Envelope protocol version for v0.1 (protocol constant, not a field).
pub const ENVELOPE_VERSION: u64 = 1;
/// Schema protocol version for v0.1 (protocol constant, not a field).
pub const SCHEMA_VERSION: u64 = 1;

/// Canonical object kind.
///
/// The numeric protocol identifiers (`1`..`5`) are assigned explicitly by the
/// encoder (step 0.4); this enum only distinguishes kinds, so that a
/// kind/payload mismatch is unrepresentable in ordinary Rust code.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ObjectKind {
    /// `1`
    KnowledgeElementVersion,
    /// `2`
    RelationshipVersion,
    /// `3`
    ChangeRevision,
    /// `4`
    SemanticState,
    /// `5`
    OntologyVersion,
}

/// Typed payload of a canonical object. Each variant carries exactly the
/// payload structure required by its object kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonicalPayload {
    /// A knowledge element version.
    KnowledgeElementVersion(KnowledgeElementVersion),
    /// A relationship version.
    RelationshipVersion(RelationshipVersion),
    /// A change revision.
    ChangeRevision(ChangeRevision),
    /// A semantic state.
    SemanticState(SemanticState),
    /// An ontology version.
    OntologyVersion(OntologyVersion),
}

/// An immutable canonical object: a typed envelope over one payload.
///
/// `envelope_version` and `schema_version` are v0.1 protocol constants
/// ([`ENVELOPE_VERSION`], [`SCHEMA_VERSION`]) and are not stored as fields,
/// which prevents inconsistent values (e.g. `schema_version = 99`) from being
/// constructed in application code. The ObjectId itself is derived from the
/// complete encoding and is never stored inside the object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalObject {
    /// The typed payload; the object kind is implied by this variant.
    pub payload: CanonicalPayload,
}

impl CanonicalObject {
    /// Returns the object kind implied by the payload variant.
    pub fn object_kind(&self) -> ObjectKind {
        match &self.payload {
            CanonicalPayload::KnowledgeElementVersion(_) => ObjectKind::KnowledgeElementVersion,
            CanonicalPayload::RelationshipVersion(_) => ObjectKind::RelationshipVersion,
            CanonicalPayload::ChangeRevision(_) => ObjectKind::ChangeRevision,
            CanonicalPayload::SemanticState(_) => ObjectKind::SemanticState,
            CanonicalPayload::OntologyVersion(_) => ObjectKind::OntologyVersion,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::change::ChangeRevision;
    use crate::domain::element::{KnowledgeElementVersion, Lifecycle};
    use crate::domain::identity::{ChangeId, ElementId, ObjectId, OntologyId, RelationshipId};
    use crate::domain::ontology::{ElementTypeDefinition, OntologyVersion};
    use crate::domain::operation::Operation;
    use crate::domain::relationship::RelationshipVersion;
    use crate::domain::state::{ElementStateEntry, RelationshipStateEntry, SemanticState};
    use uuid::Uuid;

    fn element_id(n: u8) -> ElementId {
        ElementId::from_uuid(Uuid::from_u128(n as u128))
    }

    fn object_id(n: u8) -> ObjectId {
        ObjectId::from_bytes([n; 32])
    }

    #[test]
    fn object_kind_is_derived_from_payload_variant() {
        let element = CanonicalObject {
            payload: CanonicalPayload::KnowledgeElementVersion(KnowledgeElementVersion {
                element_id: element_id(1),
                type_id: "kat.core/requirement".into(),
                lifecycle: Lifecycle::Active,
                properties: vec![],
            }),
        };
        assert_eq!(element.object_kind(), ObjectKind::KnowledgeElementVersion);

        let relationship = CanonicalObject {
            payload: CanonicalPayload::RelationshipVersion(RelationshipVersion {
                relationship_id: RelationshipId::from_uuid(Uuid::from_u128(2)),
                source_element_id: element_id(1),
                relationship_type: "kat.core/addresses".into(),
                target_element_id: element_id(2),
                properties: vec![],
            }),
        };
        assert_eq!(relationship.object_kind(), ObjectKind::RelationshipVersion);

        let change = CanonicalObject {
            payload: CanonicalPayload::ChangeRevision(ChangeRevision {
                change_id: ChangeId::from_uuid(Uuid::from_u128(3)),
                base_states: vec![object_id(1)],
                result_state: object_id(2),
                operations: vec![Operation::CreateElement {
                    new_version: object_id(3),
                }],
                dependencies: vec![],
                description: None,
            }),
        };
        assert_eq!(change.object_kind(), ObjectKind::ChangeRevision);

        let state = CanonicalObject {
            payload: CanonicalPayload::SemanticState(SemanticState {
                ontology_version: object_id(1),
                elements: vec![ElementStateEntry {
                    element_id: element_id(1),
                    version: object_id(2),
                }],
                relationships: vec![RelationshipStateEntry {
                    relationship_id: RelationshipId::from_uuid(Uuid::from_u128(4)),
                    version: object_id(3),
                }],
            }),
        };
        assert_eq!(state.object_kind(), ObjectKind::SemanticState);

        let ontology = CanonicalObject {
            payload: CanonicalPayload::OntologyVersion(OntologyVersion {
                ontology_id: OntologyId::from_uuid(Uuid::from_u128(5)),
                element_types: vec![ElementTypeDefinition {
                    type_id: "kat.core/requirement".into(),
                    name: "Requirement".into(),
                }],
                relationship_types: vec![],
            }),
        };
        assert_eq!(ontology.object_kind(), ObjectKind::OntologyVersion);
    }
}
