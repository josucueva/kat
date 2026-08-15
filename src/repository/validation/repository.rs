//! Repository-wide semantic consistency validation (`kat validate`).

use std::collections::{HashMap, HashSet};

use crate::domain::element::Lifecycle;
use crate::domain::identity::{ElementId, RelationshipId};
use crate::domain::property::PropertyValue;

use crate::encoding::decode_canonical;
use crate::encoding::object::{CanonicalObject, CanonicalPayload, ObjectKind};
use crate::repository::object_store::ObjectStore;
use crate::repository::open::Repository;
use crate::repository::query::QueryError;
use crate::repository::ref_store::RefStore;

/// Category/kind of a mechanically decidable validation violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValidationViolationKind {
    /// Relationship type is not defined in the active ontology.
    UnknownRelationshipType,
    /// Current element type of source is not allowed for this relationship type.
    RelationshipSourceTypeNotAllowed,
    /// Current element type of target is not allowed for this relationship type.
    RelationshipTargetTypeNotAllowed,
    /// Semantic triple `(type, source, target)` occurs more than once in accepted state.
    DuplicateRelationshipTriple,
    /// Relationship source or target element is not present in accepted state.
    MissingEndpointElement,
}

/// A mechanically decidable semantic violation reported by `kat validate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationViolation {
    /// Category/kind of violation.
    pub kind: ValidationViolationKind,
    /// Relationship identity involved, if applicable.
    pub relationship_id: Option<RelationshipId>,
    /// Element identities directly participating in the violation.
    pub affected_element_ids: Vec<ElementId>,
    /// Human-readable diagnostic description.
    pub message: String,
}

/// Active natural-language Constraint element reported as unverified in KAT v0.1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedConstraint {
    /// Identity of the constraint element.
    pub constraint_element_id: ElementId,
    /// Title property of the constraint, if present.
    pub title: Option<String>,
    /// Element identities targeted by valid outgoing `kat.core/restricts` relationships.
    pub constrained_element_ids: Vec<ElementId>,
}

/// Comprehensive report produced by `validate_repository`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    /// Mechanically decidable semantic violations (causes exit code 1 if non-empty).
    pub violations: Vec<ValidationViolation>,
    /// Active constraint knowledge elements without executable rules (informational, exit code 0).
    pub unverified_constraints: Vec<UnverifiedConstraint>,
}

use crate::domain::element::KnowledgeElementVersion;
use crate::domain::relationship::RelationshipVersion;
use crate::domain::state::SemanticState;

/// Performs repository-wide semantic consistency validation over the current accepted state.
pub fn validate_repository(repository: &Repository) -> Result<ValidationReport, QueryError> {
    let accepted = repository.ref_store().read_accepted()?;

    let state = match load_typed(
        repository.object_store(),
        accepted.state,
        ObjectKind::SemanticState,
    )?
    .payload
    {
        CanonicalPayload::SemanticState(state) => state,
        _ => unreachable!("kind verified by load_typed"),
    };

    validate_repository_state(repository.object_store(), &state, &[], &[])
}

/// Performs semantic consistency validation over a given SemanticState (accepted or candidate working state).
pub fn validate_repository_state(
    store: &ObjectStore,
    state: &SemanticState,
    staged_elements: &[KnowledgeElementVersion],
    staged_relationships: &[RelationshipVersion],
) -> Result<ValidationReport, QueryError> {
    let ontology =
        match load_typed(store, state.ontology_version, ObjectKind::OntologyVersion)?.payload {
            CanonicalPayload::OntologyVersion(ontology) => ontology,
            _ => unreachable!("kind verified by load_typed"),
        };

    // Build lookup map of ontology relationship types by type_id and short name
    let mut ontology_rel_map = HashMap::new();
    for rel_def in &ontology.relationship_types {
        ontology_rel_map.insert(rel_def.type_id.clone(), rel_def.clone());
        let short_name = rel_def
            .type_id
            .rsplit('/')
            .next()
            .unwrap_or(&rel_def.type_id);
        ontology_rel_map.insert(short_name.to_string(), rel_def.clone());
    }

    // Build lookup map of ontology element type short names
    let mut element_type_map = HashMap::new();
    for el_def in &ontology.element_types {
        element_type_map.insert(el_def.type_id.clone(), el_def.clone());
        let short_name = el_def.type_id.rsplit('/').next().unwrap_or(&el_def.type_id);
        element_type_map.insert(short_name.to_string(), el_def.clone());
    }

    // Load all element versions in state (overlaying staged versions)
    let staged_elem_map: HashMap<ElementId, KnowledgeElementVersion> = staged_elements
        .iter()
        .map(|e| (e.element_id, e.clone()))
        .collect();

    let mut loaded_elements = HashMap::new();
    for entry in &state.elements {
        if let Some(staged) = staged_elem_map.get(&entry.element_id) {
            loaded_elements.insert(entry.element_id, staged.clone());
        } else {
            let elem = match load_typed(store, entry.version, ObjectKind::KnowledgeElementVersion)?
                .payload
            {
                CanonicalPayload::KnowledgeElementVersion(elem) => elem,
                _ => unreachable!("kind verified by load_typed"),
            };
            loaded_elements.insert(entry.element_id, elem);
        }
    }

    // Load all relationship versions in state (overlaying staged versions)
    let staged_rel_map: HashMap<RelationshipId, RelationshipVersion> = staged_relationships
        .iter()
        .map(|r| (r.relationship_id, r.clone()))
        .collect();

    let mut loaded_relationships = HashMap::new();
    for entry in &state.relationships {
        if let Some(staged) = staged_rel_map.get(&entry.relationship_id) {
            loaded_relationships.insert(entry.relationship_id, staged.clone());
        } else {
            let rel =
                match load_typed(store, entry.version, ObjectKind::RelationshipVersion)?.payload {
                    CanonicalPayload::RelationshipVersion(rel) => rel,
                    _ => unreachable!("kind verified by load_typed"),
                };
            loaded_relationships.insert(entry.relationship_id, rel);
        }
    }

    let mut violations = Vec::new();
    let mut seen_triples = HashSet::new();
    let mut valid_restricts_targets: HashMap<ElementId, Vec<ElementId>> = HashMap::new();

    // Iterate over relationships in canonical RelationshipId order
    for entry in &state.relationships {
        let Some(rel_v) = loaded_relationships.get(&entry.relationship_id) else {
            continue;
        };

        let source_elem = loaded_elements.get(&rel_v.source_element_id);
        let target_elem = loaded_elements.get(&rel_v.target_element_id);

        let mut endpoints_valid = true;
        let mut missing_affected = Vec::new();

        if source_elem.is_none() {
            endpoints_valid = false;
            missing_affected.push(rel_v.source_element_id);
        }
        if target_elem.is_none() {
            endpoints_valid = false;
            missing_affected.push(rel_v.target_element_id);
        }

        if !endpoints_valid {
            violations.push(ValidationViolation {
                kind: ValidationViolationKind::MissingEndpointElement,
                relationship_id: Some(entry.relationship_id),
                affected_element_ids: missing_affected,
                message: format!(
                    "relationship {} references non-existent endpoint element(s)",
                    entry.relationship_id
                ),
            });
        }

        let mut affected = Vec::new();
        if source_elem.is_some() {
            affected.push(rel_v.source_element_id);
        }
        if target_elem.is_some() {
            affected.push(rel_v.target_element_id);
        }

        // Check relationship type existence in active ontology
        let rel_def = ontology_rel_map.get(&rel_v.relationship_type);
        let mut rel_ontology_valid = true;

        if let Some(def) = rel_def {
            // Check source type compatibility
            if let Some(src) = source_elem {
                let src_type_allowed = def.allowed_source_types.iter().any(|t| {
                    t == &src.type_id || t.rsplit('/').next() == src.type_id.rsplit('/').next()
                });
                if !src_type_allowed {
                    rel_ontology_valid = false;
                    violations.push(ValidationViolation {
                        kind: ValidationViolationKind::RelationshipSourceTypeNotAllowed,
                        relationship_id: Some(entry.relationship_id),
                        affected_element_ids: affected.clone(),
                        message: format!(
                            "relationship {} type '{}' does not allow source element type '{}'",
                            entry.relationship_id, rel_v.relationship_type, src.type_id
                        ),
                    });
                }
            }

            // Check target type compatibility
            if let Some(tgt) = target_elem {
                let tgt_type_allowed = def.allowed_target_types.iter().any(|t| {
                    t == &tgt.type_id || t.rsplit('/').next() == tgt.type_id.rsplit('/').next()
                });
                if !tgt_type_allowed {
                    rel_ontology_valid = false;
                    violations.push(ValidationViolation {
                        kind: ValidationViolationKind::RelationshipTargetTypeNotAllowed,
                        relationship_id: Some(entry.relationship_id),
                        affected_element_ids: affected.clone(),
                        message: format!(
                            "relationship {} type '{}' does not allow target element type '{}'",
                            entry.relationship_id, rel_v.relationship_type, tgt.type_id
                        ),
                    });
                }
            }
        } else {
            rel_ontology_valid = false;
            violations.push(ValidationViolation {
                kind: ValidationViolationKind::UnknownRelationshipType,
                relationship_id: Some(entry.relationship_id),
                affected_element_ids: affected.clone(),
                message: format!(
                    "relationship {} uses unknown relationship type '{}'",
                    entry.relationship_id, rel_v.relationship_type
                ),
            });
        }

        // Check triple uniqueness
        let triple = (
            rel_v.relationship_type.clone(),
            rel_v.source_element_id,
            rel_v.target_element_id,
        );
        if !seen_triples.insert(triple) {
            violations.push(ValidationViolation {
                kind: ValidationViolationKind::DuplicateRelationshipTriple,
                relationship_id: Some(entry.relationship_id),
                affected_element_ids: affected.clone(),
                message: format!(
                    "relationship {} creates duplicate semantic triple ({}, {}, {})",
                    entry.relationship_id,
                    rel_v.relationship_type,
                    rel_v.source_element_id,
                    rel_v.target_element_id
                ),
            });
        }

        // Track valid restricts targets for unverified constraints
        let short_rel_type = rel_v
            .relationship_type
            .rsplit('/')
            .next()
            .unwrap_or(&rel_v.relationship_type);
        if short_rel_type == "restricts" && rel_ontology_valid && endpoints_valid {
            valid_restricts_targets
                .entry(rel_v.source_element_id)
                .or_default()
                .push(rel_v.target_element_id);
        }
    }

    // Collect unverified constraints for active kat.core/constraint elements
    let mut unverified_constraints = Vec::new();
    for entry in &state.elements {
        let Some(elem_v) = loaded_elements.get(&entry.element_id) else {
            continue;
        };

        let short_type = elem_v.type_id.rsplit('/').next().unwrap_or(&elem_v.type_id);
        if short_type == "constraint" && elem_v.lifecycle == Lifecycle::Active {
            let title = elem_v.properties.iter().find_map(|(k, v)| {
                if k == "title" {
                    if let PropertyValue::Text(t) = v {
                        Some(t.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            let constrained_element_ids = valid_restricts_targets
                .get(&entry.element_id)
                .cloned()
                .unwrap_or_default();

            unverified_constraints.push(UnverifiedConstraint {
                constraint_element_id: entry.element_id,
                title,
                constrained_element_ids,
            });
        }
    }

    Ok(ValidationReport {
        violations,
        unverified_constraints,
    })
}

fn load_typed(
    store: &ObjectStore,
    id: crate::domain::identity::ObjectId,
    expected: ObjectKind,
) -> Result<CanonicalObject, QueryError> {
    let bytes = store.get(id)?;
    let object = decode_canonical(&bytes)?;
    let actual = object.object_kind();
    if actual != expected {
        return Err(QueryError::UnexpectedObjectKind { expected, actual });
    }
    Ok(object)
}
