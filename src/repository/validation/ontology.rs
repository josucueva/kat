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
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OntologyError {
    /// The element type is not defined in the ontology.
    #[error("unknown element type: {0}")]
    UnknownElementType(String),
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
}
