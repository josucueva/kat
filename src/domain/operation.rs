//! Semantic mutation operations (see `spec/canonical-format.cddl`, the
//! six operation encodings, and `docs/prototype-design.md`,
//! "Operation Representation").

use crate::domain::identity::{ElementId, ObjectId, RelationshipId};

/// A semantic mutation operation contained in a ChangeRevision.
///
/// The canonical numeric operation identifiers (`1`..`6`) are assigned
/// explicitly by the encoder (step 0.4); this enum does not hard-code them.
/// Operation order inside a Change is semantically meaningful and is preserved
/// by `Vec<Operation>` in `ChangeRevision`.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum Operation {
    /// `1` — introduce a new knowledge element version.
    CreateElement {
        /// ObjectId of the new KnowledgeElementVersion.
        new_version: ObjectId,
    },
    /// `2` — replace an element's current version.
    UpdateElement {
        /// Stable element identity.
        element_id: ElementId,
        /// Deterministic precondition: the version currently active.
        expected_version: ObjectId,
        /// ObjectId of the new KnowledgeElementVersion.
        new_version: ObjectId,
    },
    /// `3` — mark an element deprecated.
    DeprecateElement {
        /// Stable element identity.
        element_id: ElementId,
        /// Deterministic precondition: the version currently active.
        expected_version: ObjectId,
        /// ObjectId of the new (deprecated) KnowledgeElementVersion.
        new_version: ObjectId,
    },
    /// `4` — introduce a new relationship version.
    Link {
        /// ObjectId of the new RelationshipVersion.
        new_relationship_version: ObjectId,
    },
    /// `5` — remove a relationship from the resulting state.
    Unlink {
        /// Stable relationship identity.
        relationship_id: RelationshipId,
        /// Deterministic precondition: the version currently active.
        expected_version: ObjectId,
    },
    /// `6` — supersede an element with a replacement, adding a successor link.
    Supersede {
        /// Stable identity of the element being superseded.
        existing_element: ElementId,
        /// Deterministic precondition: the version currently active.
        expected_existing_version: ObjectId,
        /// Stable identity of the replacement element.
        replacement_element: ElementId,
        /// ObjectId of the replacement KnowledgeElementVersion.
        replacement_version: ObjectId,
        /// ObjectId of the superseding RelationshipVersion linking them.
        superseding_relationship: ObjectId,
    },
    /// `7` — re-baseline direct accountability relationships of an artifact.
    AccountArtifact {
        /// Stable artifact element identity.
        artifact_id: ElementId,
        /// List of relationship baseline reconciliations.
        reconciliations: Vec<RelationshipReconciliation>,
    },
}

/// Individual relationship baseline reconciliation entry for `AccountArtifact`.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct RelationshipReconciliation {
    /// Stable relationship identity.
    pub relationship_id: RelationshipId,
    /// Expected relationship version currently active in state.
    pub expected_relationship_version: ObjectId,
    /// Target element identity.
    pub target_element_id: ElementId,
    /// Exact target KnowledgeElementVersion ObjectId reconciled against.
    pub reconciled_target_version: ObjectId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn all_operation_variants_constructible() {
        let object_id = |n: u8| ObjectId::from_bytes([n; 32]);
        let element = ElementId::from_uuid(Uuid::from_u128(1));
        let relationship = RelationshipId::from_uuid(Uuid::from_u128(2));

        let operations = [
            Operation::CreateElement {
                new_version: object_id(1),
            },
            Operation::UpdateElement {
                element_id: element,
                expected_version: object_id(2),
                new_version: object_id(3),
            },
            Operation::DeprecateElement {
                element_id: element,
                expected_version: object_id(3),
                new_version: object_id(4),
            },
            Operation::Link {
                new_relationship_version: object_id(5),
            },
            Operation::Unlink {
                relationship_id: relationship,
                expected_version: object_id(6),
            },
            Operation::Supersede {
                existing_element: element,
                expected_existing_version: object_id(4),
                replacement_element: element,
                replacement_version: object_id(7),
                superseding_relationship: object_id(8),
            },
        ];

        assert_eq!(operations.len(), 6);
    }
}
