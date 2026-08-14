//! Semantic repository invariants for a candidate semantic change
//! (see `docs/invariants.md`).
//!
//! Step 1.4 validates the **candidate SemanticState** of a prepared
//! `CreateElement`, not repository persistence. At this stage V1/S1 are not in
//! the ObjectStore and the accepted ref is unchanged — persistence existence
//! checks belong to the persist step (1.6) and to repository open/integrity,
//! so they are deliberately absent here.
//!
//! Enforced (the minimal set for a coherent first Change):
//!
//! 1. candidate state remains structurally canonical (reuses `CanonicalValidate`);
//! 2. the created element has the expected `Active` lifecycle;
//! 3. the candidate references the prepared V1 correctly
//!    (`candidate.elements[E1] == element_version_id`);
//! 4. the derived V1 identity is still correct
//!    (`canonical_object_id(element) == element_version_id`);
//! 5. the base ontology reference is preserved;
//! 6. existing state content is preserved, so the candidate is exactly
//!    `base elements + E1 -> V1` (relationships unchanged).
//!
//! `InvariantError` may **wrap** a `CanonicalStructureError` (a valid semantic
//! candidate must also be canonically structured), but it never reimplements
//! those ordering rules. Ontology conformance is not repeated here (1.3).

use crate::domain::element::Lifecycle;
use crate::domain::identity::ObjectId;
use crate::encoding::canonical_object_id;
use crate::encoding::object::{CanonicalObject, CanonicalPayload};
use crate::encoding::validate::{CanonicalStructureError, CanonicalValidate};
use crate::repository::change::{
    PreparedElementCreation, PreparedElementDeprecation, PreparedElementUpdate,
};

/// Error reported when a candidate semantic change violates an invariant.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvariantError {
    /// The created element's lifecycle is not the expected `Active`.
    #[error("the created element is not in the Active lifecycle")]
    CreatedElementNotActive,
    /// The candidate's V1 ObjectId no longer matches the element's re-derived
    /// content identity (encode-then-hash).
    #[error("V1 content identity mismatch: expected {expected}, actual {actual}")]
    ElementVersionIdentityMismatch {
        /// The correctly re-derived ObjectId of the element version.
        expected: ObjectId,
        /// The ObjectId carried on the prepared creation.
        actual: ObjectId,
    },
    /// The candidate state does not map the created element to its new version.
    #[error("candidate state does not reference the created element version")]
    CandidateElementReferenceMismatch,
    /// The candidate changed the base ontology reference.
    #[error("candidate changed the base ontology reference")]
    OntologyVersionChanged,
    /// The candidate altered unrelated element content (removed, replaced, or
    /// added an entry beyond exactly `E1 -> V1`).
    #[error("candidate mutated unrelated element content")]
    UnexpectedElementMutation,
    /// The candidate altered the base relationships.
    #[error("candidate mutated the base relationships")]
    UnexpectedRelationshipMutation,
    /// The candidate state is not structurally canonical.
    #[error("candidate state is not canonically structured: {0}")]
    InvalidCanonicalStructure(#[from] CanonicalStructureError),
    /// The update changed the element's stable identity.
    #[error("the update changed the element identity")]
    UpdateIdentityChanged,
    /// The update changed the element's type (`UpdateElement` must not become
    /// an implicit Retype).
    #[error("the update changed the element type")]
    UpdateTypeChanged,
    /// A lifecycle involved in the update is not `Active` (updates apply only
    /// to active elements and must not change the lifecycle).
    #[error("the update lifecycle is not Active (previous or new)")]
    UpdateLifecycleChanged,
    /// The base state no longer maps the element to the prepared previous
    /// version (`previous_version_id == expected_version == base.elements[E]`
    /// no longer holds).
    #[error("the base state no longer maps the element to the prepared previous version")]
    UpdateBaseVersionMismatch,
    /// The candidate's Vn+1 ObjectId no longer matches the element's re-derived
    /// content identity (encode-then-hash).
    #[error("Vn+1 content identity mismatch: expected {expected}, actual {actual}")]
    UpdateVersionIdentityMismatch {
        /// The correctly re-derived ObjectId of the new version.
        expected: ObjectId,
        /// The ObjectId carried on the prepared update.
        actual: ObjectId,
    },
    /// The prepared update's new version is identical to the previous version
    /// (defensive invariant breach — the operation-level no-op is rejected at
    /// step 2.1 as `NoEffectiveChange`).
    #[error("the update's new version is identical to the previous version")]
    UpdateVersionUnchanged,
    /// The candidate state does not map the updated element to its new version.
    #[error("candidate state does not reference the updated element version")]
    UpdateCandidateReferenceMismatch,
    /// The deprecation changed element properties.
    #[error("the deprecation changed element properties")]
    DeprecationPropertiesChanged,
    /// The deprecation lifecycle transition is invalid.
    #[error(
        "the deprecation lifecycle transition is invalid (previous must be Active, new must be Deprecated)"
    )]
    DeprecationLifecycleInvalid,
    /// The candidate state does not map the deprecated element to its new version.
    #[error("candidate state does not reference the deprecated element version")]
    DeprecationCandidateReferenceMismatch,
}

/// Validates the candidate-state invariants of a prepared `CreateElement`.
///
/// Pure: it performs **no** persistence and **no** publication, and it does
/// not mutate `prepared`.
pub fn validate_create_element_invariants(
    prepared: &PreparedElementCreation,
) -> Result<(), InvariantError> {
    let element = &prepared.element;
    let candidate = &prepared.candidate_state;
    let base = &prepared.context.base_state;

    // 1. Candidate remains structurally canonical (reuses CanonicalValidate).
    candidate.validate_canonical_structure()?;

    // 2. The created element has the expected Active lifecycle.
    if element.lifecycle != Lifecycle::Active {
        return Err(InvariantError::CreatedElementNotActive);
    }

    // 3. The derived V1 identity is still correct.
    let derived = canonical_object_id(&CanonicalObject {
        payload: CanonicalPayload::KnowledgeElementVersion(element.clone()),
    })
    .expect("the element was canonically encoded at apply time");
    if derived != prepared.element_version_id {
        return Err(InvariantError::ElementVersionIdentityMismatch {
            expected: derived,
            actual: prepared.element_version_id,
        });
    }

    // 4. The candidate references the prepared V1 correctly.
    let created = candidate
        .elements
        .iter()
        .find(|e| e.element_id == element.element_id)
        .ok_or(InvariantError::CandidateElementReferenceMismatch)?;
    if created.version != prepared.element_version_id {
        return Err(InvariantError::CandidateElementReferenceMismatch);
    }

    // 5. The candidate is `base elements + exactly E1 -> V1`: removing the
    //    added entry must recover the base exactly (no removal, replacement,
    //    unrelated insertion, or version change).
    let mut without_created = candidate.elements.clone();
    let idx = without_created
        .iter()
        .position(|e| e.element_id == element.element_id)
        .expect("the created entry was found above");
    without_created.remove(idx);
    if without_created != base.elements {
        return Err(InvariantError::UnexpectedElementMutation);
    }

    // 6. The base ontology reference is preserved.
    if candidate.ontology_version != base.ontology_version {
        return Err(InvariantError::OntologyVersionChanged);
    }

    // 7. The base relationships are preserved.
    if candidate.relationships != base.relationships {
        return Err(InvariantError::UnexpectedRelationshipMutation);
    }

    Ok(())
}

/// Validates the candidate-state invariants of a prepared `UpdateElement`.
///
/// The central rule (normative, from the Phase 2 design):
///
/// > For `UpdateElement(E, Vn, Vn+1)`, the candidate Semantic State MUST differ
/// > from the base Semantic State **only** in the version mapping for `E`, which
/// > changes from `Vn` to `Vn+1`.
///
/// Checks, in order:
///
/// 1. candidate is structurally canonical (`CanonicalValidate`);
/// 2. `previous_element.element_id == element.element_id` (identity preserved);
/// 3. `previous_version_id == expected_version == base.elements[E].version`;
/// 4. `element.type_id == previous_element.type_id` (no implicit Retype);
/// 5. both lifecycles are `Active`;
/// 6. `element_version_id == canonical_object_id(element)`;
/// 7. `element_version_id != previous_version_id` (defensive — step 2.1 rejects
///    the no-op at the operation level as `NoEffectiveChange`);
/// 8. candidate maps `E -> element_version_id`;
/// 9. removing E's entry from the candidate and from the base yields identical
///    sets (no removal, replacement, unrelated insertion, or unrelated version
///    change);
/// 10. candidate ontology version equals base ontology version;
/// 11. relationships unchanged.
///
/// Pure: **no** persistence and **no** publication; does not mutate `prepared`.
pub fn validate_update_element_invariants(
    prepared: &PreparedElementUpdate,
) -> Result<(), InvariantError> {
    let element = &prepared.element;
    let previous = &prepared.previous_element;
    let base = &prepared.context.base_state;
    let candidate = &prepared.candidate_state;

    // 1. Candidate remains structurally canonical (reuses CanonicalValidate).
    candidate.validate_canonical_structure()?;

    // 2. Identity preserved: Vn+1.element_id == Vn.element_id == E.
    if element.element_id != previous.element_id {
        return Err(InvariantError::UpdateIdentityChanged);
    }
    let e = element.element_id;

    // 3. previous_version_id == expected_version == base.elements[E].version.
    let base_version = base
        .elements
        .iter()
        .find(|entry| entry.element_id == e)
        .map(|entry| entry.version)
        .ok_or(InvariantError::UnexpectedElementMutation)?;
    if prepared.previous_version_id != prepared.expected_version
        || prepared.previous_version_id != base_version
    {
        return Err(InvariantError::UpdateBaseVersionMismatch);
    }

    // 4. Type preserved (Update must not become an implicit Retype).
    if element.type_id != previous.type_id {
        return Err(InvariantError::UpdateTypeChanged);
    }

    // 5. Both lifecycles are Active.
    if element.lifecycle != Lifecycle::Active || previous.lifecycle != Lifecycle::Active {
        return Err(InvariantError::UpdateLifecycleChanged);
    }

    // 6. Vn+1 content identity is still correct (encode-then-hash).
    let derived = canonical_object_id(&CanonicalObject {
        payload: CanonicalPayload::KnowledgeElementVersion(element.clone()),
    })
    .expect("the element was canonically encoded at apply time");
    if derived != prepared.element_version_id {
        return Err(InvariantError::UpdateVersionIdentityMismatch {
            expected: derived,
            actual: prepared.element_version_id,
        });
    }

    // 7. Vn+1 must differ from Vn (defensive invariant breach).
    if prepared.element_version_id == prepared.previous_version_id {
        return Err(InvariantError::UpdateVersionUnchanged);
    }

    // 8. The candidate maps E -> Vn+1.
    let updated = candidate
        .elements
        .iter()
        .find(|entry| entry.element_id == e)
        .ok_or(InvariantError::UpdateCandidateReferenceMismatch)?;
    if updated.version != prepared.element_version_id {
        return Err(InvariantError::UpdateCandidateReferenceMismatch);
    }

    // 9. Single-state delta: the candidate and base element sets differ only
    //    in E's entry. Removing E's entry from both must yield identical sets —
    //    no removal, replacement, unrelated insertion, or unrelated version
    //    change.
    let without_updated: Vec<_> = candidate
        .elements
        .iter()
        .filter(|entry| entry.element_id != e)
        .cloned()
        .collect();
    let without_base: Vec<_> = base
        .elements
        .iter()
        .filter(|entry| entry.element_id != e)
        .cloned()
        .collect();
    if without_updated != without_base {
        return Err(InvariantError::UnexpectedElementMutation);
    }

    // 10. The base ontology reference is preserved.
    if candidate.ontology_version != base.ontology_version {
        return Err(InvariantError::OntologyVersionChanged);
    }

    // 11. The base relationships are preserved.
    if candidate.relationships != base.relationships {
        return Err(InvariantError::UnexpectedRelationshipMutation);
    }

    Ok(())
}

/// Validates the candidate-state invariants of a prepared `DeprecateElement`.
///
/// Steps:
/// 1. Candidate state is canonical.
/// 2. Identity preserved.
/// 3. Base version & expected version match.
/// 4. Type preserved.
/// 5. Properties preserved (deprecation alters ONLY lifecycle).
/// 6. Valid lifecycle transition (`Active -> Deprecated`).
/// 7. Content identity derived correctly.
/// 8. Vn+1 != Vn.
/// 9. Candidate state maps E -> Vn+1.
/// 10. Single-state delta rule (candidate minus E == base minus E).
/// 11. Base ontology and relationships preserved.
pub fn validate_deprecate_element_invariants(
    prepared: &PreparedElementDeprecation,
) -> Result<(), InvariantError> {
    let candidate = &prepared.candidate_state;
    let base = &prepared.context.base_state;
    let e = prepared.element.element_id;

    // 1. Candidate state is canonical.
    candidate.validate_canonical_structure()?;

    // 2. Identity preserved.
    if prepared.element.element_id != prepared.previous_element.element_id {
        return Err(InvariantError::UpdateIdentityChanged);
    }

    // 3. Base version and expected version match.
    let base_entry = base
        .elements
        .iter()
        .find(|entry| entry.element_id == e)
        .ok_or(InvariantError::UpdateBaseVersionMismatch)?;
    if base_entry.version != prepared.previous_version_id
        || base_entry.version != prepared.expected_version
    {
        return Err(InvariantError::UpdateBaseVersionMismatch);
    }

    // 4. Type preserved.
    if prepared.element.type_id != prepared.previous_element.type_id {
        return Err(InvariantError::UpdateTypeChanged);
    }

    // 5. Properties preserved.
    if prepared.element.properties != prepared.previous_element.properties {
        return Err(InvariantError::DeprecationPropertiesChanged);
    }

    // 6. Valid lifecycle transition (Active -> Deprecated).
    if prepared.previous_element.lifecycle != Lifecycle::Active
        || prepared.element.lifecycle != Lifecycle::Deprecated
    {
        return Err(InvariantError::DeprecationLifecycleInvalid);
    }

    // 7. Content identity derived correctly.
    let derived_id = canonical_object_id(&CanonicalObject {
        payload: CanonicalPayload::KnowledgeElementVersion(prepared.element.clone()),
    })
    .expect("the element was canonically encoded at apply time");
    if derived_id != prepared.element_version_id {
        return Err(InvariantError::UpdateVersionIdentityMismatch {
            expected: derived_id,
            actual: prepared.element_version_id,
        });
    }

    // 8. Vn+1 != Vn.
    if prepared.element_version_id == prepared.previous_version_id {
        return Err(InvariantError::UpdateVersionUnchanged);
    }

    // 9. Candidate state maps E -> Vn+1.
    let updated = candidate
        .elements
        .iter()
        .find(|entry| entry.element_id == e)
        .ok_or(InvariantError::DeprecationCandidateReferenceMismatch)?;
    if updated.version != prepared.element_version_id {
        return Err(InvariantError::DeprecationCandidateReferenceMismatch);
    }

    // 10. Single-state delta rule.
    let without_updated: Vec<_> = candidate
        .elements
        .iter()
        .filter(|entry| entry.element_id != e)
        .cloned()
        .collect();
    let without_base: Vec<_> = base
        .elements
        .iter()
        .filter(|entry| entry.element_id != e)
        .cloned()
        .collect();
    if without_updated != without_base {
        return Err(InvariantError::UnexpectedElementMutation);
    }

    // 11. Base ontology reference and relationships preserved.
    if candidate.ontology_version != base.ontology_version {
        return Err(InvariantError::OntologyVersionChanged);
    }
    if candidate.relationships != base.relationships {
        return Err(InvariantError::UnexpectedRelationshipMutation);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::element::KnowledgeElementVersion;
    use crate::domain::identity::{ElementId, ObjectId, OntologyId, RelationshipId};
    use crate::domain::state::{ElementStateEntry, RelationshipStateEntry, SemanticState};
    use crate::repository::change::{ChangeContext, CreateElementInput, apply_create_element};
    use crate::repository::init::initial_core_ontology;
    use crate::repository::ref_store::AcceptedRef;
    use uuid::Uuid;

    fn element_id(n: u128) -> ElementId {
        ElementId::from_uuid(Uuid::from_u128(n))
    }
    fn object_id(n: u8) -> ObjectId {
        ObjectId::from_bytes([n; 32])
    }

    /// A change context with the given base state; `accepted` references
    /// `object_id(1)`, base state uses `ontology_version = object_id(2)`.
    fn context(base: SemanticState) -> ChangeContext {
        ChangeContext {
            accepted: AcceptedRef {
                state: object_id(1),
                change: None,
            },
            base_state_id: object_id(1),
            base_state: base,
            ontology: initial_core_ontology(OntologyId::from_uuid(Uuid::nil())),
        }
    }

    /// Applies a CreateElement for `n` in the given base state, returning a
    /// valid prepared creation ready for mutation.
    fn prepared(base: SemanticState, n: u128) -> PreparedElementCreation {
        apply_create_element(
            context(base),
            CreateElementInput {
                element_id: element_id(n),
                type_id: "kat.core/requirement".into(),
                properties: vec![],
            },
        )
        .unwrap()
    }

    fn base_state(
        elements: Vec<ElementStateEntry>,
        relationships: Vec<RelationshipStateEntry>,
    ) -> SemanticState {
        SemanticState {
            ontology_version: object_id(2),
            elements,
            relationships,
        }
    }

    #[test]
    fn valid_prepared_creation_passes() {
        let p = prepared(base_state(vec![], vec![]), 5);
        validate_create_element_invariants(&p).unwrap();
    }

    #[test]
    fn non_active_lifecycle_fails() {
        let mut p = prepared(base_state(vec![], vec![]), 5);
        p.element = KnowledgeElementVersion {
            lifecycle: Lifecycle::Deprecated,
            ..p.element.clone()
        };
        assert_eq!(
            validate_create_element_invariants(&p),
            Err(InvariantError::CreatedElementNotActive)
        );
    }

    #[test]
    fn altered_element_version_id_fails() {
        let mut p = prepared(base_state(vec![], vec![]), 5);
        p.element_version_id = object_id(9);
        assert!(matches!(
            validate_create_element_invariants(&p),
            Err(InvariantError::ElementVersionIdentityMismatch {
                expected: _,
                actual
            }) if actual == object_id(9)
        ));
    }

    #[test]
    fn candidate_points_to_wrong_version_fails() {
        let mut p = prepared(base_state(vec![], vec![]), 5);
        p.candidate_state.elements[0].version = object_id(7);
        assert_eq!(
            validate_create_element_invariants(&p),
            Err(InvariantError::CandidateElementReferenceMismatch)
        );
    }

    #[test]
    fn candidate_missing_created_element_fails() {
        let mut p = prepared(base_state(vec![], vec![]), 5);
        p.candidate_state.elements.clear();
        assert_eq!(
            validate_create_element_invariants(&p),
            Err(InvariantError::CandidateElementReferenceMismatch)
        );
    }

    #[test]
    fn changed_ontology_reference_fails() {
        let mut p = prepared(base_state(vec![], vec![]), 5);
        p.candidate_state.ontology_version = object_id(8);
        assert_eq!(
            validate_create_element_invariants(&p),
            Err(InvariantError::OntologyVersionChanged)
        );
    }

    #[test]
    fn removed_existing_base_element_fails() {
        // Base already holds E9 -> v9; creating E5 must preserve E9.
        let base = base_state(
            vec![ElementStateEntry {
                element_id: element_id(9),
                version: object_id(9),
            }],
            vec![],
        );
        let mut p = prepared(base, 5);
        p.candidate_state
            .elements
            .retain(|e| e.element_id != element_id(9));
        assert_eq!(
            validate_create_element_invariants(&p),
            Err(InvariantError::UnexpectedElementMutation)
        );
    }

    #[test]
    fn changed_existing_base_element_version_fails() {
        let base = base_state(
            vec![ElementStateEntry {
                element_id: element_id(9),
                version: object_id(9),
            }],
            vec![],
        );
        let mut p = prepared(base, 5);
        p.candidate_state.elements[1].version = object_id(8);
        assert_eq!(
            validate_create_element_invariants(&p),
            Err(InvariantError::UnexpectedElementMutation)
        );
    }

    #[test]
    fn extra_unrelated_element_fails() {
        let base = base_state(
            vec![ElementStateEntry {
                element_id: element_id(9),
                version: object_id(9),
            }],
            vec![],
        );
        let mut p = prepared(base, 5);
        p.candidate_state.elements.push(ElementStateEntry {
            element_id: element_id(11),
            version: object_id(3),
        });
        // The pushed entry is appended (possibly out of order), so either a
        // canonical-structure or an unexpected-element error is raised.
        let err = validate_create_element_invariants(&p).unwrap_err();
        assert!(matches!(
            err,
            InvariantError::UnexpectedElementMutation
                | InvariantError::InvalidCanonicalStructure(_)
        ));
    }

    #[test]
    fn changed_relationships_fail() {
        let rel = RelationshipStateEntry {
            relationship_id: RelationshipId::from_uuid(Uuid::from_u128(3)),
            version: object_id(4),
        };
        let base = base_state(vec![], vec![rel.clone()]);
        let mut p = prepared(base, 5);
        p.candidate_state.relationships[0].version = object_id(6);
        assert_eq!(
            validate_create_element_invariants(&p),
            Err(InvariantError::UnexpectedRelationshipMutation)
        );
    }

    #[test]
    fn structurally_noncanonical_candidate_fails() {
        let mut p = prepared(base_state(vec![], vec![]), 5);
        // Force unsorted element entries: E5 at the front is < a newly
        // inserted E1 that sorts before it, so ordering breaks.
        p.candidate_state.elements.clear();
        p.candidate_state.elements.push(ElementStateEntry {
            element_id: element_id(7),
            version: p.element_version_id,
        });
        p.candidate_state.elements.push(ElementStateEntry {
            element_id: element_id(1),
            version: object_id(1),
        });
        assert!(matches!(
            validate_create_element_invariants(&p),
            Err(InvariantError::InvalidCanonicalStructure(_))
        ));
    }
}
