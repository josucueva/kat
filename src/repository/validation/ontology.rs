//! Ontology conformance: whether knowledge objects conform to the repository
//! ontology (an element's `type_id` exists in the base `OntologyVersion`).
//!
//! The validator uses **only the ontology referenced by the base state**
//! (`context.base_state.ontology_version`), loaded into the [`ChangeContext`];
//! it never falls back to a global/hardcoded core ontology. The repository
//! state determines the active ontology version.
//!
//! Phase 1 (step 1.3) enforces the single minimal rule:
//!
//! ```text
//! element.type_id must exist in ontology.element_types
//! ```
//!
//! Nothing else. Relationship source/target validation, constraint semantics,
//! property schema validation, ontology inheritance/extensions, and
//! architecture-specific rules are deliberately out of scope for 1.3
//! (relationship and invariant enforcement land with their own later steps).

use crate::domain::ontology::OntologyVersion;

/// Error reported when a knowledge object does not conform to the ontology.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OntologyError {
    /// The element type is not defined in the ontology.
        UnknownElementType(String),
    /// The relationship type is not defined in the ontology.
        UnknownRelationshipType(String),
    /// The relationship's source element type is not allowed for this relationship type.
        RelationshipSourceTypeNotAllowed {
        /// The relationship type being validated.
        relationship_type: String,
        /// The source element type.
        source_type: String,
        /// Allowed source types according to ontology.
        allowed_sources: Vec<String>,
        /// Allowed target types according to ontology.
        allowed_targets: Vec<String>,
    },
    /// The relationship's target element type is not allowed for this relationship type.
        RelationshipTargetTypeNotAllowed {
        /// The relationship type being validated.
        relationship_type: String,
        /// The target element type.
        target_type: String,
        /// Allowed source types according to ontology.
        allowed_sources: Vec<String>,
        /// Allowed target types according to ontology.
        allowed_targets: Vec<String>,
    },
}

/// Validates that `type_id` is a defined element type in `ontology`.
///
/// The lookup is independent of storage order (canonically ordered or not); at
/// v0.1 scale a linear scan is sufficient. Only this single conformance rule
/// is enforced.
pub fn validate_element_type(
    ontology: &OntologyVersion,
    type_id: &str,
) -> Result<(), OntologyError> {
    if ontology.element_types.iter().any(|t| t.type_id == type_id) {
        Ok(())
    } else {
        Err(OntologyError::UnknownElementType(type_id.to_string()))
    }
}

/// Validates that `relationship_type` is defined in `ontology` and that
/// `source_type` and `target_type` are allowed source and target element types.
pub fn validate_relationship(
    ontology: &OntologyVersion,
    relationship_type: &str,
    source_type: &str,
    target_type: &str,
) -> Result<(), OntologyError> {
    let rel_def = ontology
        .relationship_types
        .iter()
        .find(|r| r.type_id == relationship_type)
        .ok_or_else(|| OntologyError::UnknownRelationshipType(relationship_type.to_string()))?;

    if !rel_def
        .allowed_source_types
        .iter()
        .any(|s| s == source_type)
    {
        return Err(OntologyError::RelationshipSourceTypeNotAllowed {
            relationship_type: relationship_type.to_string(),
            source_type: source_type.to_string(),
            allowed_sources: rel_def.allowed_source_types.clone(),
            allowed_targets: rel_def.allowed_target_types.clone(),
        });
    }

    if !rel_def
        .allowed_target_types
        .iter()
        .any(|t| t == target_type)
    {
        return Err(OntologyError::RelationshipTargetTypeNotAllowed {
            relationship_type: relationship_type.to_string(),
            target_type: target_type.to_string(),
            allowed_sources: rel_def.allowed_source_types.clone(),
            allowed_targets: rel_def.allowed_target_types.clone(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::OntologyId;
    use crate::repository::init::initial_core_ontology;
    use uuid::Uuid;

    fn core_ontology() -> OntologyVersion {
        initial_core_ontology(OntologyId::from_uuid(Uuid::nil()))
    }

    #[test]
    fn known_core_element_types_are_accepted() {
        let ontology = core_ontology();
        for type_id in [
            "kat.core/requirement",
            "kat.core/constraint",
            "kat.core/implementation",
            "kat.core/artifact",
            "kat.core/design-decision",
            "kat.core/intent",
            "kat.core/validation",
        ] {
            validate_element_type(&ontology, type_id).unwrap();
        }
    }

    #[test]
    fn unknown_element_type_is_rejected() {
        let ontology = core_ontology();
        assert_eq!(
            validate_element_type(&ontology, "kat.core/not-a-real-type"),
            Err(OntologyError::UnknownElementType(
                "kat.core/not-a-real-type".into()
            ))
        );
    }

    #[test]
    fn known_core_supersedes_relationship_accepted() {
        let ontology = core_ontology();
        validate_relationship(
            &ontology,
            "kat.core/supersedes",
            "kat.core/design-decision",
            "kat.core/design-decision",
        )
        .unwrap();
    }

    #[test]
    fn unknown_relationship_type_rejected() {
        let ontology = core_ontology();
        assert_eq!(
            validate_relationship(
                &ontology,
                "kat.core/bogus-rel",
                "kat.core/design-decision",
                "kat.core/design-decision"
            ),
            Err(OntologyError::UnknownRelationshipType(
                "kat.core/bogus-rel".into()
            ))
        );
    }

    #[test]
    fn supersedes_with_invalid_source_type_rejected() {
        let ontology = core_ontology();
        assert_eq!(
            validate_relationship(
                &ontology,
                "kat.core/supersedes",
                "kat.core/requirement",
                "kat.core/design-decision"
            ),
            Err(OntologyError::RelationshipSourceTypeNotAllowed {
                relationship_type: "kat.core/supersedes".into(),
                source_type: "kat.core/requirement".into(),
                allowed_sources: vec!["kat.core/design-decision".into()],
                allowed_targets: vec!["kat.core/design-decision".into()],
            })
        );
    }

    #[test]
    fn supersedes_with_invalid_target_type_rejected() {
        let ontology = core_ontology();
        assert_eq!(
            validate_relationship(
                &ontology,
                "kat.core/supersedes",
                "kat.core/design-decision",
                "kat.core/requirement"
            ),
            Err(OntologyError::RelationshipTargetTypeNotAllowed {
                relationship_type: "kat.core/supersedes".into(),
                target_type: "kat.core/requirement".into(),
                allowed_sources: vec!["kat.core/design-decision".into()],
                allowed_targets: vec!["kat.core/design-decision".into()],
            })
        );
    }
}

impl std::fmt::Display for OntologyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownElementType(_0) => write!(f, "unknown element type: {_0}"),
            Self::UnknownRelationshipType(_0) => write!(f, "unknown relationship type: {_0}"),
            Self::RelationshipSourceTypeNotAllowed { relationship_type, source_type, allowed_sources, allowed_targets, .. } => write!(f, "relationship type '{relationship_type}' does not allow source element type '{source_type}' (requires source in {allowed_sources:?}, target in {allowed_targets:?})"),
            Self::RelationshipTargetTypeNotAllowed { relationship_type, target_type, allowed_sources, allowed_targets, .. } => write!(f, "relationship type '{relationship_type}' does not allow target element type '{target_type}' (requires source in {allowed_sources:?}, target in {allowed_targets:?})"),
        }
    }
}

impl std::error::Error for OntologyError {
}
