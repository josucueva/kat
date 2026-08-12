//! Structural-invalid cases (see `spec/vectors/invalid/structural/`).
//!
//! Each value here is constructible as a Rust value but is not canonically
//! encodable: `canonical_bytes` must reject it with the specific
//! `CanonicalStructureError` (fail-closed — it must never silently repair or
//! normalize the input).

use uuid::Uuid;

use kat::domain::change::ChangeRevision;
use kat::domain::element::{KnowledgeElementVersion, Lifecycle};
use kat::domain::identity::{ChangeId, ElementId, ObjectId, OntologyId, RelationshipId};
use kat::domain::ontology::{ElementTypeDefinition, OntologyVersion};
use kat::domain::operation::Operation;
use kat::domain::property::PropertyValue;
use kat::domain::state::{ElementStateEntry, RelationshipStateEntry, SemanticState};
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::encoding::validate::CanonicalStructureError;
use kat::encoding::{EncodingError, canonical_bytes};

fn element_id(n: u8) -> ElementId {
    ElementId::from_uuid(Uuid::from_u128(n as u128))
}

fn relationship_id(n: u8) -> RelationshipId {
    RelationshipId::from_uuid(Uuid::from_u128(n as u128))
}

fn object_id(n: u8) -> ObjectId {
    ObjectId::from_bytes([n; 32])
}

fn assert_rejected(object: CanonicalObject, expected: CanonicalStructureError) {
    assert_eq!(
        canonical_bytes(&object),
        Err(EncodingError::InvalidCanonicalStructure(expected.clone())),
        "expected rejection with {expected:?}"
    );
}

#[test]
fn unsorted_semantic_state_rejected() {
    let object = CanonicalObject {
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
    assert_rejected(object, CanonicalStructureError::SemanticElementsUnordered);
}

#[test]
fn duplicate_element_id_rejected() {
    let object = CanonicalObject {
        payload: CanonicalPayload::SemanticState(SemanticState {
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
        }),
    };
    assert_rejected(
        object,
        CanonicalStructureError::SemanticElementsDuplicate(element_id(1)),
    );
}

#[test]
fn duplicate_relationship_id_rejected() {
    let object = CanonicalObject {
        payload: CanonicalPayload::SemanticState(SemanticState {
            ontology_version: object_id(0),
            elements: vec![],
            relationships: vec![
                RelationshipStateEntry {
                    relationship_id: relationship_id(1),
                    version: object_id(1),
                },
                RelationshipStateEntry {
                    relationship_id: relationship_id(1),
                    version: object_id(2),
                },
            ],
        }),
    };
    assert_rejected(
        object,
        CanonicalStructureError::SemanticRelationshipsDuplicate(relationship_id(1)),
    );
}

#[test]
fn unsorted_ontology_definitions_rejected() {
    let object = CanonicalObject {
        payload: CanonicalPayload::OntologyVersion(OntologyVersion {
            ontology_id: OntologyId::from_uuid(Uuid::new_v4()),
            element_types: vec![
                ElementTypeDefinition {
                    type_id: "kat.core/requirement".into(),
                    name: "Requirement".into(),
                },
                ElementTypeDefinition {
                    type_id: "kat.core/constraint".into(),
                    name: "Constraint".into(),
                },
            ],
            relationship_types: vec![],
        }),
    };
    assert_rejected(
        object,
        CanonicalStructureError::OntologyElementTypesUnordered,
    );
}

#[test]
fn duplicate_property_key_rejected() {
    // Not representable as a JSON object; constructed directly as ordered pairs.
    let object = CanonicalObject {
        payload: CanonicalPayload::KnowledgeElementVersion(KnowledgeElementVersion {
            element_id: element_id(1),
            type_id: "kat.core/requirement".into(),
            lifecycle: Lifecycle::Active,
            properties: vec![
                ("key".to_string(), PropertyValue::Integer(1)),
                ("key".to_string(), PropertyValue::Integer(2)),
            ],
        }),
    };
    assert_rejected(
        object,
        CanonicalStructureError::PropertyKeysDuplicate("key".to_string()),
    );
}

#[test]
fn empty_operations_rejected() {
    let object = CanonicalObject {
        payload: CanonicalPayload::ChangeRevision(ChangeRevision {
            change_id: ChangeId::from_uuid(Uuid::new_v4()),
            base_states: vec![object_id(1)],
            result_state: object_id(2),
            operations: vec![],
            dependencies: vec![],
            description: None,
        }),
    };
    assert_rejected(object, CanonicalStructureError::ChangeOperationsEmpty);
}

#[test]
fn empty_base_states_rejected() {
    let object = CanonicalObject {
        payload: CanonicalPayload::ChangeRevision(ChangeRevision {
            change_id: ChangeId::from_uuid(Uuid::new_v4()),
            base_states: vec![],
            result_state: object_id(2),
            operations: vec![Operation::CreateElement {
                new_version: object_id(3),
            }],
            dependencies: vec![],
            description: None,
        }),
    };
    assert_rejected(object, CanonicalStructureError::ChangeBaseStatesEmpty);
}

#[test]
fn unsorted_dependencies_rejected() {
    let object = CanonicalObject {
        payload: CanonicalPayload::ChangeRevision(ChangeRevision {
            change_id: ChangeId::from_uuid(Uuid::new_v4()),
            base_states: vec![object_id(1)],
            result_state: object_id(2),
            operations: vec![Operation::CreateElement {
                new_version: object_id(3),
            }],
            dependencies: vec![object_id(9), object_id(8)],
            description: None,
        }),
    };
    assert_rejected(object, CanonicalStructureError::ChangeDependenciesUnordered);
}

#[test]
fn duplicate_dependency_rejected() {
    let object = CanonicalObject {
        payload: CanonicalPayload::ChangeRevision(ChangeRevision {
            change_id: ChangeId::from_uuid(Uuid::new_v4()),
            base_states: vec![object_id(1)],
            result_state: object_id(2),
            operations: vec![Operation::CreateElement {
                new_version: object_id(3),
            }],
            dependencies: vec![object_id(8), object_id(8)],
            description: None,
        }),
    };
    assert_rejected(
        object,
        CanonicalStructureError::ChangeDependenciesDuplicate(object_id(8)),
    );
}
