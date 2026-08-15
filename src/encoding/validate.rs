//! Canonical structural validation, kept separate from encoding.
//!
//! These checks enforce the canonical structural rules from
//! `spec/canonical-format.cddl` and `docs/canonical-format.md` that are not
//! conveniently guaranteed by the Rust types themselves: ordering, uniqueness,
//! and minimal cardinality of canonical collections.
//!
//! This is deliberately distinct from semantic validation (ontology
//! conformance and invariants), which is out of scope until later steps.
//!
//! Three layers are kept separate:
//!
//! ```text
//! Rust type validity  !=  canonical structural validity  !=  semantic validity
//! ```

use std::cmp::Ordering;

use crate::domain::change::ChangeRevision;
use crate::domain::element::KnowledgeElementVersion;
use crate::domain::identity::{ElementId, ObjectId, RelationshipId};
use crate::domain::ontology::{OntologyVersion, RelationshipTypeDefinition};
use crate::domain::operation::Operation;
use crate::domain::property::PropertyValue;
use crate::domain::relationship::RelationshipVersion;
use crate::domain::state::SemanticState;
use crate::encoding::object::{CanonicalObject, CanonicalPayload};

/// Error reported when a canonical object violates a canonical structural rule.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CanonicalStructureError {
    /// SemanticState element entries are not sorted by element ID.
    #[error("SemanticState element entries must be sorted by element ID")]
    SemanticElementsUnordered,
    /// SemanticState contains two entries with the same element ID.
    #[error("SemanticState contains a duplicate element ID: {0}")]
    SemanticElementsDuplicate(ElementId),
    /// SemanticState relationship entries are not sorted by relationship ID.
    #[error("SemanticState relationship entries must be sorted by relationship ID")]
    SemanticRelationshipsUnordered,
    /// SemanticState contains two entries with the same relationship ID.
    #[error("SemanticState contains a duplicate relationship ID: {0}")]
    SemanticRelationshipsDuplicate(RelationshipId),
    /// Ontology element type definitions are not sorted by type ID.
    #[error("ontology element type definitions must be sorted by type ID")]
    OntologyElementTypesUnordered,
    /// Ontology contains two element type definitions with the same type ID.
    #[error("ontology contains a duplicate element type ID: {0}")]
    OntologyElementTypesDuplicate(String),
    /// Ontology relationship type definitions are not sorted by type ID.
    #[error("ontology relationship type definitions must be sorted by type ID")]
    OntologyRelationshipTypesUnordered,
    /// Ontology contains two relationship type definitions with the same type ID.
    #[error("ontology contains a duplicate relationship type ID: {0}")]
    OntologyRelationshipTypesDuplicate(String),
    /// An allowed-source-type list is not sorted.
    #[error("allowed source types must be sorted")]
    AllowedSourceTypesUnordered,
    /// An allowed-source-type list contains a duplicate.
    #[error("allowed source types contain a duplicate: {0}")]
    AllowedSourceTypesDuplicate(String),
    /// An allowed-target-type list is not sorted.
    #[error("allowed target types must be sorted")]
    AllowedTargetTypesUnordered,
    /// An allowed-target-type list contains a duplicate.
    #[error("allowed target types contain a duplicate: {0}")]
    AllowedTargetTypesDuplicate(String),
    /// ChangeRevision has more than one base state that is not sorted.
    #[error("ChangeRevision base states must be sorted by ObjectId when more than one is present")]
    ChangeBaseStatesUnordered,
    /// ChangeRevision has no base state.
    #[error("ChangeRevision must contain at least one base state")]
    ChangeBaseStatesEmpty,
    /// ChangeRevision dependencies are not sorted by ObjectId.
    #[error("ChangeRevision dependencies must be sorted by ObjectId")]
    ChangeDependenciesUnordered,
    /// ChangeRevision contains a duplicate dependency.
    #[error("ChangeRevision contains a duplicate dependency: {0}")]
    ChangeDependenciesDuplicate(ObjectId),
    /// ChangeRevision has no operations.
    #[error("ChangeRevision must contain at least one operation")]
    ChangeOperationsEmpty,
    /// A property map's keys are not in canonical order.
    #[error("property map keys must be in canonical order")]
    PropertyKeysUnordered,
    /// A property map contains a duplicate key.
    #[error("property map contains a duplicate key: {0}")]
    PropertyKeysDuplicate(String),
    /// AccountArtifact reconciliations are not sorted by relationship ID.
    #[error("AccountArtifact reconciliations must be sorted by relationship ID")]
    AccountReconciliationsUnordered,
    /// AccountArtifact reconciliations contain a duplicate relationship ID.
    #[error("AccountArtifact reconciliations contain a duplicate relationship ID: {0}")]
    AccountReconciliationsDuplicate(RelationshipId),
}

/// Canonical structural validation for values that must conform to the
/// canonical format's structural rules before they can be encoded.
pub trait CanonicalValidate {
    /// Validates this value's canonical structure, or returns the first
    /// structural violation found.
    fn validate_canonical_structure(&self) -> Result<(), CanonicalStructureError>;
}

/// Returns `unordered()` when any adjacent pair compares as `Greater`.
fn check_sorted<T, K: ?Sized + Ord, F, C>(
    items: &[T],
    key: F,
    cmp: C,
    unordered: impl Fn() -> CanonicalStructureError,
) -> Result<(), CanonicalStructureError>
where
    F: for<'a> Fn(&'a T) -> &'a K,
    C: Fn(&K, &K) -> Ordering,
{
    for pair in items.windows(2) {
        if cmp(key(&pair[0]), key(&pair[1])) == Ordering::Greater {
            return Err(unordered());
        }
    }
    Ok(())
}

/// Returns `duplicate()` on `Equal` or `unordered()` on `Greater`.
fn check_strictly_ascending<T, K: ?Sized + Ord, F, C>(
    items: &[T],
    key: F,
    cmp: C,
    unordered: impl Fn() -> CanonicalStructureError,
    duplicate: impl Fn(&T) -> CanonicalStructureError,
) -> Result<(), CanonicalStructureError>
where
    F: for<'a> Fn(&'a T) -> &'a K,
    C: Fn(&K, &K) -> Ordering,
{
    for pair in items.windows(2) {
        match cmp(key(&pair[0]), key(&pair[1])) {
            Ordering::Equal => return Err(duplicate(&pair[0])),
            Ordering::Greater => return Err(unordered()),
            Ordering::Less => {}
        }
    }
    Ok(())
}

/// Canonical ordering of two property map keys.
///
/// Keys are ordered by bytewise comparison of their **full deterministic CBOR
/// encodings** (RFC 8949 §4.2.1), not by the raw strings. Because the encoded
/// text-string header participates, this is not the same as plain string
/// comparison (e.g. `"z"` sorts before `"aa"`). The comparator is shared with
/// the encoder so validation and encoding can never disagree.
fn property_key_cmp(a: &str, b: &str) -> Ordering {
    crate::encoding::cbor::cmp_encoded_text(a, b)
}

/// Validates one property map: canonical key order, uniqueness, and the
/// canonical structure of every value.
fn validate_property_map(
    entries: &[(String, PropertyValue)],
) -> Result<(), CanonicalStructureError> {
    check_strictly_ascending(
        entries,
        |(key, _)| key.as_str(),
        property_key_cmp,
        || CanonicalStructureError::PropertyKeysUnordered,
        |(key, _)| CanonicalStructureError::PropertyKeysDuplicate(key.clone()),
    )?;
    entries
        .iter()
        .try_for_each(|(_, value)| value.validate_canonical_structure())
}

impl CanonicalValidate for PropertyValue {
    fn validate_canonical_structure(&self) -> Result<(), CanonicalStructureError> {
        match self {
            PropertyValue::List(values) => values
                .iter()
                .try_for_each(|value| value.validate_canonical_structure()),
            PropertyValue::Map(entries) => validate_property_map(entries),
            _ => Ok(()),
        }
    }
}

impl CanonicalValidate for KnowledgeElementVersion {
    fn validate_canonical_structure(&self) -> Result<(), CanonicalStructureError> {
        validate_property_map(&self.properties)
    }
}

impl CanonicalValidate for RelationshipVersion {
    fn validate_canonical_structure(&self) -> Result<(), CanonicalStructureError> {
        validate_property_map(&self.properties)
    }
}

impl CanonicalValidate for SemanticState {
    fn validate_canonical_structure(&self) -> Result<(), CanonicalStructureError> {
        check_strictly_ascending(
            &self.elements,
            |e| &e.element_id,
            Ord::cmp,
            || CanonicalStructureError::SemanticElementsUnordered,
            |e| CanonicalStructureError::SemanticElementsDuplicate(e.element_id),
        )?;
        check_strictly_ascending(
            &self.relationships,
            |r| &r.relationship_id,
            Ord::cmp,
            || CanonicalStructureError::SemanticRelationshipsUnordered,
            |r| CanonicalStructureError::SemanticRelationshipsDuplicate(r.relationship_id),
        )
    }
}

impl CanonicalValidate for RelationshipTypeDefinition {
    fn validate_canonical_structure(&self) -> Result<(), CanonicalStructureError> {
        check_strictly_ascending(
            &self.allowed_source_types,
            |t| t.as_str(),
            Ord::cmp,
            || CanonicalStructureError::AllowedSourceTypesUnordered,
            |t| CanonicalStructureError::AllowedSourceTypesDuplicate(t.clone()),
        )?;
        check_strictly_ascending(
            &self.allowed_target_types,
            |t| t.as_str(),
            Ord::cmp,
            || CanonicalStructureError::AllowedTargetTypesUnordered,
            |t| CanonicalStructureError::AllowedTargetTypesDuplicate(t.clone()),
        )
    }
}

impl CanonicalValidate for OntologyVersion {
    fn validate_canonical_structure(&self) -> Result<(), CanonicalStructureError> {
        check_strictly_ascending(
            &self.element_types,
            |d| d.type_id.as_str(),
            Ord::cmp,
            || CanonicalStructureError::OntologyElementTypesUnordered,
            |d| CanonicalStructureError::OntologyElementTypesDuplicate(d.type_id.clone()),
        )?;
        check_strictly_ascending(
            &self.relationship_types,
            |d| d.type_id.as_str(),
            Ord::cmp,
            || CanonicalStructureError::OntologyRelationshipTypesUnordered,
            |d| CanonicalStructureError::OntologyRelationshipTypesDuplicate(d.type_id.clone()),
        )?;
        self.relationship_types
            .iter()
            .try_for_each(|definition| definition.validate_canonical_structure())
    }
}

impl CanonicalValidate for ChangeRevision {
    fn validate_canonical_structure(&self) -> Result<(), CanonicalStructureError> {
        if self.base_states.is_empty() {
            return Err(CanonicalStructureError::ChangeBaseStatesEmpty);
        }
        // Multiple base states must be sorted; uniqueness is not required by
        // the CDDL.
        if self.base_states.len() > 1 {
            check_sorted(
                &self.base_states,
                |o| o,
                Ord::cmp,
                || CanonicalStructureError::ChangeBaseStatesUnordered,
            )?;
        }
        if self.operations.is_empty() {
            return Err(CanonicalStructureError::ChangeOperationsEmpty);
        }
        for op in &self.operations {
            if let Operation::AccountArtifact { reconciliations, .. } = op {
                check_strictly_ascending(
                    reconciliations,
                    |r| &r.relationship_id,
                    Ord::cmp,
                    || CanonicalStructureError::AccountReconciliationsUnordered,
                    |r| CanonicalStructureError::AccountReconciliationsDuplicate(r.relationship_id),
                )?;
            }
        }
        check_strictly_ascending(
            &self.dependencies,
            |o| o,
            Ord::cmp,
            || CanonicalStructureError::ChangeDependenciesUnordered,
            |o| CanonicalStructureError::ChangeDependenciesDuplicate(*o),
        )
    }
}

impl CanonicalValidate for CanonicalObject {
    fn validate_canonical_structure(&self) -> Result<(), CanonicalStructureError> {
        match &self.payload {
            CanonicalPayload::KnowledgeElementVersion(v) => v.validate_canonical_structure(),
            CanonicalPayload::RelationshipVersion(v) => v.validate_canonical_structure(),
            CanonicalPayload::ChangeRevision(v) => v.validate_canonical_structure(),
            CanonicalPayload::SemanticState(v) => v.validate_canonical_structure(),
            CanonicalPayload::OntologyVersion(v) => v.validate_canonical_structure(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::element::Lifecycle;
    use crate::domain::identity::{ChangeId, OntologyId};
    use crate::domain::ontology::ElementTypeDefinition;
    use crate::domain::operation::Operation;
    use crate::domain::state::{ElementStateEntry, RelationshipStateEntry};
    use uuid::Uuid;

    fn element_id(n: u8) -> ElementId {
        ElementId::from_uuid(Uuid::from_u128(n as u128))
    }

    fn relationship_id(n: u8) -> RelationshipId {
        RelationshipId::from_uuid(Uuid::from_u128(n as u128))
    }

    fn object_id(n: u8) -> ObjectId {
        ObjectId::from_bytes([n; 32])
    }

    fn element_with_properties(
        properties: Vec<(String, PropertyValue)>,
    ) -> KnowledgeElementVersion {
        KnowledgeElementVersion {
            element_id: element_id(1),
            type_id: "kat.core/requirement".to_string(),
            lifecycle: Lifecycle::Active,
            properties,
        }
    }

    fn relationship_type(
        type_id: &str,
        source: &[&str],
        target: &[&str],
    ) -> RelationshipTypeDefinition {
        RelationshipTypeDefinition {
            type_id: type_id.to_string(),
            name: type_id.to_string(),
            allowed_source_types: source.iter().map(|s| s.to_string()).collect(),
            allowed_target_types: target.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn change_revision(
        base_states: Vec<ObjectId>,
        operations: Vec<Operation>,
        dependencies: Vec<ObjectId>,
    ) -> ChangeRevision {
        ChangeRevision {
            change_id: ChangeId::from_uuid(Uuid::new_v4()),
            base_states,
            result_state: object_id(99),
            operations,
            dependencies,
            description: None,
        }
    }

    #[test]
    fn property_map_keys_sorted_canonical_pass() {
        // Canonical text-key order is length-first then bytewise, so "z"
        // (1 byte) sorts before "aa" (2 bytes), matching RFC 8949 §4.2.1
        // bytewise ordering of the deterministic CBOR encodings.
        let element = element_with_properties(vec![
            ("z".to_string(), PropertyValue::Integer(1)),
            ("aa".to_string(), PropertyValue::Integer(2)),
        ]);
        element.validate_canonical_structure().unwrap();
    }

    #[test]
    fn property_map_keys_bytewise_unsorted_fail() {
        // "aa" before "z" is NOT canonical: "z" must come first.
        let element = element_with_properties(vec![
            ("aa".to_string(), PropertyValue::Integer(1)),
            ("z".to_string(), PropertyValue::Integer(2)),
        ]);
        assert_eq!(
            element.validate_canonical_structure(),
            Err(CanonicalStructureError::PropertyKeysUnordered)
        );
    }

    #[test]
    fn property_map_keys_duplicate_fail() {
        let element = element_with_properties(vec![
            ("key".to_string(), PropertyValue::Integer(1)),
            ("key".to_string(), PropertyValue::Integer(2)),
        ]);
        assert_eq!(
            element.validate_canonical_structure(),
            Err(CanonicalStructureError::PropertyKeysDuplicate(
                "key".to_string()
            ))
        );
    }

    #[test]
    fn nested_property_maps_validated_recursively() {
        let nested = PropertyValue::Map(vec![
            ("b".to_string(), PropertyValue::Null),
            ("a".to_string(), PropertyValue::Null),
        ]);
        let element = element_with_properties(vec![(
            "x".to_string(),
            PropertyValue::Map(vec![("k".to_string(), nested)]),
        )]);
        assert_eq!(
            element.validate_canonical_structure(),
            Err(CanonicalStructureError::PropertyKeysUnordered)
        );
    }

    #[test]
    fn semantic_state_sorted_unique_pass() {
        let state = SemanticState {
            ontology_version: object_id(0),
            elements: vec![
                ElementStateEntry {
                    element_id: element_id(1),
                    version: object_id(1),
                },
                ElementStateEntry {
                    element_id: element_id(2),
                    version: object_id(2),
                },
            ],
            relationships: vec![RelationshipStateEntry {
                relationship_id: relationship_id(1),
                version: object_id(3),
            }],
        };
        state.validate_canonical_structure().unwrap();
    }

    #[test]
    fn semantic_state_unsorted_elements_fail() {
        let state = SemanticState {
            ontology_version: object_id(0),
            elements: vec![
                ElementStateEntry {
                    element_id: element_id(2),
                    version: object_id(2),
                },
                ElementStateEntry {
                    element_id: element_id(1),
                    version: object_id(1),
                },
            ],
            relationships: vec![],
        };
        assert_eq!(
            state.validate_canonical_structure(),
            Err(CanonicalStructureError::SemanticElementsUnordered)
        );
    }

    #[test]
    fn semantic_state_duplicate_element_id_fail() {
        // Duplicate element IDs are invalid even when the versions differ.
        let state = SemanticState {
            ontology_version: object_id(0),
            elements: vec![
                ElementStateEntry {
                    element_id: element_id(1),
                    version: object_id(1),
                },
                ElementStateEntry {
                    element_id: element_id(1),
                    version: object_id(9),
                },
            ],
            relationships: vec![],
        };
        assert_eq!(
            state.validate_canonical_structure(),
            Err(CanonicalStructureError::SemanticElementsDuplicate(
                element_id(1)
            ))
        );
    }

    #[test]
    fn ontology_sorted_unique_pass() {
        let ontology = OntologyVersion {
            ontology_id: OntologyId::from_uuid(Uuid::new_v4()),
            element_types: vec![
                ElementTypeDefinition {
                    type_id: "kat.core/constraint".into(),
                    name: "Constraint".into(),
                },
                ElementTypeDefinition {
                    type_id: "kat.core/requirement".into(),
                    name: "Requirement".into(),
                },
            ],
            relationship_types: vec![
                relationship_type(
                    "kat.core/addresses",
                    &["kat.core/design-decision"],
                    &["kat.core/requirement"],
                ),
                relationship_type(
                    "kat.core/validates",
                    &["kat.core/validation"],
                    &["kat.core/requirement"],
                ),
            ],
        };
        ontology.validate_canonical_structure().unwrap();
    }

    #[test]
    fn ontology_duplicate_element_type_fail() {
        let ontology = OntologyVersion {
            ontology_id: OntologyId::from_uuid(Uuid::new_v4()),
            element_types: vec![
                ElementTypeDefinition {
                    type_id: "kat.core/requirement".into(),
                    name: "a".into(),
                },
                ElementTypeDefinition {
                    type_id: "kat.core/requirement".into(),
                    name: "b".into(),
                },
            ],
            relationship_types: vec![],
        };
        assert_eq!(
            ontology.validate_canonical_structure(),
            Err(CanonicalStructureError::OntologyElementTypesDuplicate(
                "kat.core/requirement".to_string()
            ))
        );
    }

    #[test]
    fn ontology_allowed_types_sorted_unique() {
        relationship_type("kat.core/x", &["kat.core/a", "kat.core/b"], &["kat.core/c"])
            .validate_canonical_structure()
            .unwrap();

        let unsorted = relationship_type("kat.core/x", &["kat.core/b", "kat.core/a"], &[]);
        assert_eq!(
            unsorted.validate_canonical_structure(),
            Err(CanonicalStructureError::AllowedSourceTypesUnordered)
        );

        let duplicate = relationship_type("kat.core/x", &[], &["kat.core/a", "kat.core/a"]);
        assert_eq!(
            duplicate.validate_canonical_structure(),
            Err(CanonicalStructureError::AllowedTargetTypesDuplicate(
                "kat.core/a".to_string()
            ))
        );
    }

    #[test]
    fn change_revision_single_base_state_ok() {
        let change = change_revision(
            vec![object_id(1)],
            vec![Operation::CreateElement {
                new_version: object_id(2),
            }],
            vec![],
        );
        change.validate_canonical_structure().unwrap();
    }

    #[test]
    fn change_revision_multiple_base_states_must_be_sorted() {
        let change = change_revision(
            vec![object_id(2), object_id(1)],
            vec![Operation::CreateElement {
                new_version: object_id(3),
            }],
            vec![],
        );
        assert_eq!(
            change.validate_canonical_structure(),
            Err(CanonicalStructureError::ChangeBaseStatesUnordered)
        );
    }

    #[test]
    fn change_revision_duplicate_base_states_allowed() {
        // The CDDL requires base states to be sorted but not unique.
        let change = change_revision(
            vec![object_id(1), object_id(1)],
            vec![Operation::CreateElement {
                new_version: object_id(3),
            }],
            vec![],
        );
        change.validate_canonical_structure().unwrap();
    }

    #[test]
    fn change_revision_empty_base_states_fail() {
        let change = change_revision(
            vec![],
            vec![Operation::CreateElement {
                new_version: object_id(1),
            }],
            vec![],
        );
        assert_eq!(
            change.validate_canonical_structure(),
            Err(CanonicalStructureError::ChangeBaseStatesEmpty)
        );
    }

    #[test]
    fn change_revision_empty_operations_fail() {
        let change = change_revision(vec![object_id(1)], vec![], vec![]);
        assert_eq!(
            change.validate_canonical_structure(),
            Err(CanonicalStructureError::ChangeOperationsEmpty)
        );
    }

    #[test]
    fn change_revision_dependencies_sorted_unique() {
        let ok = change_revision(
            vec![object_id(1)],
            vec![Operation::CreateElement {
                new_version: object_id(2),
            }],
            vec![object_id(3), object_id(4)],
        );
        ok.validate_canonical_structure().unwrap();

        let unsorted = change_revision(
            vec![object_id(1)],
            vec![Operation::CreateElement {
                new_version: object_id(2),
            }],
            vec![object_id(4), object_id(3)],
        );
        assert_eq!(
            unsorted.validate_canonical_structure(),
            Err(CanonicalStructureError::ChangeDependenciesUnordered)
        );

        let duplicate = change_revision(
            vec![object_id(1)],
            vec![Operation::CreateElement {
                new_version: object_id(2),
            }],
            vec![object_id(3), object_id(3)],
        );
        assert_eq!(
            duplicate.validate_canonical_structure(),
            Err(CanonicalStructureError::ChangeDependenciesDuplicate(
                object_id(3)
            ))
        );
    }

    #[test]
    fn canonical_object_validates_its_payload() {
        let valid = CanonicalObject {
            payload: CanonicalPayload::KnowledgeElementVersion(element_with_properties(vec![(
                "a".to_string(),
                PropertyValue::Integer(1),
            )])),
        };
        assert!(valid.validate_canonical_structure().is_ok());

        // A structurally invalid payload fails through the envelope too.
        let invalid = CanonicalObject {
            payload: CanonicalPayload::SemanticState(SemanticState {
                ontology_version: object_id(0),
                elements: vec![
                    ElementStateEntry {
                        element_id: element_id(2),
                        version: object_id(2),
                    },
                    ElementStateEntry {
                        element_id: element_id(1),
                        version: object_id(1),
                    },
                ],
                relationships: vec![],
            }),
        };
        assert_eq!(
            invalid.validate_canonical_structure(),
            Err(CanonicalStructureError::SemanticElementsUnordered)
        );
    }
}
