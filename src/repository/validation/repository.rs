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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UnverifiedConstraint {
    /// Identity of the constraint element.
    pub constraint_element_id: ElementId,
    /// Title property of the constraint, if present.
    pub title: Option<String>,
    /// Element identities targeted by valid outgoing `kat.core/restricts` relationships.
    pub constrained_element_ids: Vec<ElementId>,
}

/// Linked validation evidence element targeting a subject.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ValidationEvidenceInfo {
    /// Identity of the `kat.core/validation` element.
    pub validation_element_id: ElementId,
    /// Title of the validation element, if present.
    pub title: Option<String>,
}

/// Verification status and linked evidence details for a Constraint element.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConstraintVerificationDetail {
    /// Identity of the constraint element.
    pub constraint_id: ElementId,
    /// Title property of the constraint, if present.
    pub title: Option<String>,
    /// Element identities targeted by valid outgoing `kat.core/restricts` relationships.
    pub constrained_element_ids: Vec<ElementId>,
    /// Whether KAT mechanically verified this constraint via executable rule (always `false` in KAT).
    pub is_mechanically_verified: bool,
    /// Linked validation evidence elements targeting this constraint via `validates` relationships.
    pub validation_evidence: Vec<ValidationEvidenceInfo>,
}

/// Summary of evidence coverage for a single knowledge element category.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CategoryCoverageSummary {
    /// Canonical type ID of the category (e.g. `kat.core/constraint`).
    pub category_type: String,
    /// Total count of active elements in this category.
    pub total_count: usize,
    /// Count of elements backed by at least one linked validation evidence element.
    pub evidence_backed_count: usize,
    /// Count of elements with zero linked validation evidence elements.
    pub uncovered_count: usize,
}

/// Detail for an active knowledge element that has zero linked validation evidence elements.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UncoveredElementDetail {
    /// Identity of the uncovered element.
    pub element_id: ElementId,
    /// Canonical type ID of the element.
    pub type_id: String,
    /// Title property of the element, if present.
    pub title: Option<String>,
}

/// Comprehensive report produced by `validate_repository`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ValidationReport {
    /// Mechanically decidable semantic violations (causes exit code 1 if non-empty).
    pub violations: Vec<ValidationViolation>,
    /// Active constraint knowledge elements without executable rules (informational, exit code 0).
    pub unverified_constraints: Vec<UnverifiedConstraint>,
    /// Detailed verification status and evidence details per constraint.
    pub constraint_details: Vec<ConstraintVerificationDetail>,
    /// Category-level evidence coverage statistics.
    pub category_summaries: Vec<CategoryCoverageSummary>,
    /// Active knowledge elements without linked validation evidence.
    pub uncovered_elements: Vec<UncoveredElementDetail>,
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
    let mut valid_validates_evidence: HashMap<ElementId, Vec<ValidationEvidenceInfo>> =
        HashMap::new();

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

        // Track valid validates evidence elements for target subjects
        if short_rel_type == "validates"
            && rel_ontology_valid
            && endpoints_valid
            && let Some(src) = source_elem
        {
            let src_short_type = src.type_id.rsplit('/').next().unwrap_or(&src.type_id);
            if src_short_type == "validation" && src.lifecycle == Lifecycle::Active {
                let val_title = src.properties.iter().find_map(|(k, v)| {
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
                valid_validates_evidence
                    .entry(rel_v.target_element_id)
                    .or_default()
                    .push(ValidationEvidenceInfo {
                        validation_element_id: rel_v.source_element_id,
                        title: val_title,
                    });
            }
        }
    }

    // Collect unverified constraints, detailed verification info, and evidence coverage for active elements
    let mut unverified_constraints = Vec::new();
    let mut constraint_details = Vec::new();
    let mut category_stats: HashMap<String, (usize, usize)> = HashMap::new();
    let mut uncovered_elements = Vec::new();

    for entry in &state.elements {
        let Some(elem_v) = loaded_elements.get(&entry.element_id) else {
            continue;
        };

        if elem_v.lifecycle != Lifecycle::Active {
            continue;
        }

        let canonical_type = elem_v.type_id.clone();
        let short_type = canonical_type.rsplit('/').next().unwrap_or(&canonical_type);

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

        let mut evidence = valid_validates_evidence
            .get(&entry.element_id)
            .cloned()
            .unwrap_or_default();
        evidence.sort_by_key(|e| e.validation_element_id);

        let is_evidence_backed = !evidence.is_empty();

        if short_type == "constraint" {
            let constrained_element_ids = valid_restricts_targets
                .get(&entry.element_id)
                .cloned()
                .unwrap_or_default();

            unverified_constraints.push(UnverifiedConstraint {
                constraint_element_id: entry.element_id,
                title: title.clone(),
                constrained_element_ids: constrained_element_ids.clone(),
            });

            constraint_details.push(ConstraintVerificationDetail {
                constraint_id: entry.element_id,
                title: title.clone(),
                constrained_element_ids,
                is_mechanically_verified: false, // Critical invariant: evidence-backed != mechanically verified
                validation_evidence: evidence,
            });
        }

        let validatable_target_types: std::collections::HashSet<String> = ontology
            .relationship_types
            .iter()
            .find(|r| r.type_id == "kat.core/validates" || r.type_id.ends_with("/validates"))
            .map(|r| r.allowed_target_types.iter().cloned().collect())
            .unwrap_or_else(|| {
                let mut set = std::collections::HashSet::new();
                set.insert("kat.core/requirement".to_string());
                set.insert("kat.core/constraint".to_string());
                set.insert("kat.core/implementation".to_string());
                set
            });

        // Only elements whose type is an allowed target of validates relationships are eligible for evidence coverage tracking
        let is_coverage_eligible = validatable_target_types.contains(&canonical_type)
            || validatable_target_types.contains(short_type);

        if is_coverage_eligible {
            let entry_stat = category_stats
                .entry(canonical_type.clone())
                .or_insert((0, 0));
            entry_stat.0 += 1;
            if is_evidence_backed {
                entry_stat.1 += 1;
            } else {
                uncovered_elements.push(UncoveredElementDetail {
                    element_id: entry.element_id,
                    type_id: canonical_type,
                    title,
                });
            }
        }
    }

    let mut category_summaries: Vec<CategoryCoverageSummary> = category_stats
        .into_iter()
        .map(
            |(category_type, (total_count, evidence_backed_count))| CategoryCoverageSummary {
                category_type,
                total_count,
                evidence_backed_count,
                uncovered_count: total_count - evidence_backed_count,
            },
        )
        .collect();
    category_summaries.sort_by(|a, b| a.category_type.cmp(&b.category_type));

    uncovered_elements.sort_by(|a, b| (&a.type_id, a.element_id).cmp(&(&b.type_id, b.element_id)));
    constraint_details.sort_by_key(|c| c.constraint_id);

    Ok(ValidationReport {
        violations,
        unverified_constraints,
        constraint_details,
        category_summaries,
        uncovered_elements,
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
