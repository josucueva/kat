//! The Change Engine: authoritative, controlled mutation of semantic state
//! (see `docs/prototype-design.md`, "Change Application Flow", and
//! `docs/architecture.md`, "Change Engine").
//!
//! The engine is the only path through which new authoritative semantic
//! states are produced. It composes, in order:
//!
//! ```text
//! prepare    (resolve accepted, load base state + ontology)
//!     ↓
//! apply      (build operations, check preconditions, apply to a candidate)
//!     ↓
//! validate   (ontology conformance + invariants — semantic validity)
//!     ↓
//! materialize (encode + persist V1, S1, C1 — immutable objects)
//!     ↓
//! publish    (compare-and-swap on refs/accepted)
//! ```
//!
//! Phase 0 established the separation of the three validation layers:
//! **encoding validity** (`encoding`), **repository integrity** (`open`),
//! and **semantic validity** (`repository::validation` + preconditions here).
//! This module is the orchestration boundary; it must not own ontology or
//! invariant semantics (those live in `repository::validation`).
//!
//! Step 1.1 is the smallest first piece: **prepare only**. It resolves the
//! accepted repository state and loads the base SemanticState and its
//! OntologyVersion into a reusable [`ChangeContext`]. It performs **no**
//! mutation, no persistence, and no publication.

use std::cmp::Ordering;

use crate::domain::element::{KnowledgeElementVersion, Lifecycle};
use crate::domain::identity::ElementId;
use crate::domain::identity::ObjectId;
use crate::domain::ontology::OntologyVersion;
use crate::domain::property::PropertyValue;
use crate::domain::state::{ElementStateEntry, SemanticState};
use crate::encoding::canonical_object_id;
use crate::encoding::cbor::cmp_encoded_text;
use crate::encoding::decode::DecodingError;
use crate::encoding::decode_canonical;
use crate::encoding::error::EncodingError;
use crate::encoding::object::{CanonicalObject, CanonicalPayload, ObjectKind};
use crate::repository::object_store::{ObjectStore, ObjectStoreError};
use crate::repository::open::Repository;
use crate::repository::ref_store::AcceptedRef;
use crate::repository::validation::invariant::{
    InvariantError, validate_create_element_invariants as validate_candidate_invariants,
};
use crate::repository::validation::ontology::{OntologyError, validate_element_type};

/// A failing operation-level precondition (operation *application* condition,
/// near the Change Engine rather than in `repository::validation`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PreconditionError {
    /// The ElementId already appears in the base state. In v0.1 a state maps
    /// one semantic ID to its current version, so a present ID — active,
    /// deprecated, or superseded — cannot be created; reuse/resurrection is an
    /// explicit operation's concern, not `CreateElement`'s.
    #[error("element {0} already exists in the base state")]
    ElementAlreadyExists(ElementId),
}

/// Error produced by the Change Engine.
///
/// Only variants reachable by the engine are defined; further variants
/// (ontology, invariants) are added when the respective steps require them.
#[derive(Debug, thiserror::Error)]
pub enum ChangeError {
    /// An object store failure while loading a referenced object.
    #[error("object store error: {0}")]
    ObjectStore(#[from] ObjectStoreError),
    /// A referenced object failed strict canonical decoding.
    #[error("decoding error: {0}")]
    Decoding(#[from] DecodingError),
    /// A canonical object failed to encode (fail-closed).
    #[error("encoding error: {0}")]
    Encoding(#[from] EncodingError),
    /// A referenced object has a different canonical kind than expected.
    #[error("expected object kind {expected}, found {actual}")]
    UnexpectedObjectKind {
        /// The canonical kind the repository structure required.
        expected: ObjectKind,
        /// The canonical kind the stored object actually has.
        actual: ObjectKind,
    },
    /// An operation-level precondition was not satisfied.
    #[error("precondition violated: {0}")]
    Precondition(#[from] PreconditionError),
    /// The candidate violates ontology conformance.
    #[error("ontology conformance error: {0}")]
    Ontology(#[from] OntologyError),
    /// The candidate violates a semantic repository invariant.
    #[error("invariant violated: {0}")]
    Invariant(#[from] InvariantError),
    /// The application input contained a duplicate canonical property key.
    #[error("duplicate property key: {0}")]
    DuplicatePropertyKey(String),
}

/// The resolved context a change is prepared against: the accepted head, the
/// selected base SemanticState, and the OntologyVersion interpreting it.
///
/// Carrying the loaded base state and ontology avoids re-reading and
/// re-decoding them for each operation in the change.
#[derive(Debug)]
pub struct ChangeContext {
    /// The accepted repository head this change is based on. This is also the
    /// `expected` value later passed to the CAS publication step.
    pub accepted: AcceptedRef,
    /// ObjectId of the base SemanticState the change applies to.
    pub base_state_id: ObjectId,
    /// The decoded base SemanticState.
    pub base_state: SemanticState,
    /// The OntologyVersion that interprets the base state.
    pub ontology: OntologyVersion,
}

/// Resolves the accepted repository head and loads the base SemanticState and
/// its OntologyVersion into a [`ChangeContext`].
///
/// `prepare_only: no mutation, no persistence, no publication.` The accepted
/// ref and object store are left exactly as they were.
pub fn prepare_change(repository: &Repository) -> Result<ChangeContext, ChangeError> {
    let accepted = repository.accepted.clone();
    let base_state_id = accepted.state;

    let base_state = match load_typed(
        repository.object_store(),
        base_state_id,
        ObjectKind::SemanticState,
    )?
    .payload
    {
        crate::encoding::object::CanonicalPayload::SemanticState(state) => state,
        _ => unreachable!("kind verified by load_typed"),
    };

    let ontology = match load_typed(
        repository.object_store(),
        base_state.ontology_version,
        ObjectKind::OntologyVersion,
    )?
    .payload
    {
        crate::encoding::object::CanonicalPayload::OntologyVersion(ontology) => ontology,
        _ => unreachable!("kind verified by load_typed"),
    };

    Ok(ChangeContext {
        accepted,
        base_state_id,
        base_state,
        ontology,
    })
}

/// Loads `id` from the store (hash verified by `ObjectStore::get`), decodes
/// it canonically, and requires exactly `expected` kind.
fn load_typed(
    store: &ObjectStore,
    id: ObjectId,
    expected: ObjectKind,
) -> Result<CanonicalObject, ChangeError> {
    let bytes = store.get(id)?;
    let object = decode_canonical(&bytes)?;
    let actual = object.object_kind();
    if actual != expected {
        return Err(ChangeError::UnexpectedObjectKind { expected, actual });
    }
    Ok(object)
}

/// Application-level input for a `CreateElement` operation.
///
/// This is the user/application representation, not yet a canonical object.
/// The engine establishes the canonical representation (property ordering) and
/// consumes it; the caller supplies the stable [`ElementId`], so the engine
/// stays deterministic and the CLI/application layer decides when to generate
/// identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateElementInput {
    /// Stable identity of the new element.
    pub element_id: ElementId,
    /// Ontology element type identifier (e.g. `kat.core/requirement`).
    pub type_id: String,
    /// Semantic properties as an unordered application key/value list.
    /// Keys are normalized into canonical order; duplicates are rejected.
    pub properties: Vec<(String, PropertyValue)>,
}

/// A logically-prepared element creation: the new Active knowledge element
/// version, its content identity, and the candidate SemanticState it maps to.
///
/// Distinct from a full [`ChangeRevision`](crate::domain::change::ChangeRevision):
/// nothing here is persisted and no accepted ref is published. `S1` exists
/// only because `V1`'s ObjectId could be derived from its canonical bytes
/// (hashing is not persistence).
#[derive(Debug)]
pub struct PreparedElementCreation {
    /// The change context the element was created against (unchanged).
    pub context: ChangeContext,
    /// The constructed Active `KnowledgeElementVersion`.
    pub element: KnowledgeElementVersion,
    /// The content identity of `element` (SHA-256 of its canonical bytes).
    pub element_version_id: ObjectId,
    /// The candidate SemanticState: base plus `element_id -> element_version_id`
    /// inserted at the canonical position.
    pub candidate_state: SemanticState,
}

/// Applies a single `CreateElement` to `context`, producing a candidate only.
///
/// Step 1.2 is confined to **operation application**: it checks the
/// `ElementId`-uniqueness precondition, builds the new Active
/// `KnowledgeElementVersion` (normalizing property-key order and rejecting
/// duplicates), derives its content identity, and inserts it into a candidate
/// SemanticState at the canonical position.
///
/// It deliberately does **not** perform ontology conformance (1.3) or
/// invariant validation (1.4), and it never persists or publishes.
pub fn apply_create_element(
    context: ChangeContext,
    input: CreateElementInput,
) -> Result<PreparedElementCreation, ChangeError> {
    // Precondition: the ElementId must not already be present in the base
    // state (present at all — active, deprecated, or superseded — is rejected).
    let base_state = &context.base_state;
    if base_state
        .elements
        .iter()
        .any(|e| e.element_id == input.element_id)
    {
        return Err(ChangeError::Precondition(
            PreconditionError::ElementAlreadyExists(input.element_id),
        ));
    }

    // Normalize property keys into canonical order; reject duplicates.
    // Application input may be unordered; the constructed version is canonical.
    let properties = normalize_properties(input.properties)?;

    let element = KnowledgeElementVersion {
        element_id: input.element_id,
        type_id: input.type_id,
        lifecycle: Lifecycle::Active,
        properties,
    };

    // Derive V1's content identity (encode + SHA-256). No persistence occurs.
    let element_version_id = canonical_object_id(&CanonicalObject {
        payload: CanonicalPayload::KnowledgeElementVersion(element.clone()),
    })?;

    // Insert `element_id -> element_version_id` at the canonical position.
    let entry = ElementStateEntry {
        element_id: element.element_id,
        version: element_version_id,
    };
    let mut elements = base_state.elements.clone();
    let insertion_point = match elements.binary_search_by(|e| e.element_id.cmp(&entry.element_id)) {
        // The precondition above guarantees the id is not already present; an
        // `Ok(_)` position is therefore unreachable. Using an `Err` step is
        // the deterministic canonical insertion point.
        Ok(_) => unreachable!("element_id uniqueness precondition checked"),
        Err(pos) => pos,
    };
    elements.insert(insertion_point, entry);

    let candidate_state = SemanticState {
        ontology_version: base_state.ontology_version,
        elements,
        relationships: base_state.relationships.clone(),
    };

    Ok(PreparedElementCreation {
        context,
        element,
        element_version_id,
        candidate_state,
    })
}

/// Applies the step 1.3 ontology-conformance stage to a prepared element
/// creation: the element's `type_id` must exist in the base `OntologyVersion`.
///
/// The validator uses **only** `prepared.context.ontology` — the ontology
/// loaded from the repository's `base_state.ontology_version` — never a global
/// core ontology. Step 1.3 enforces this single rule and nothing else: no
/// invariant validation (1.4), no persistence, no ChangeRevision, no CAS.
pub fn validate_create_element_ontology(
    prepared: PreparedElementCreation,
) -> Result<PreparedElementCreation, ChangeError> {
    validate_element_type(&prepared.context.ontology, &prepared.element.type_id)?;
    Ok(prepared)
}

/// Applies the step 1.4 invariant stage to a prepared element creation.
///
/// Validates the **candidate SemanticState** invariants (structural canonical
/// form, Active lifecycle, correct V1 identity + reference, preserved ontology
/// and unrelated content). It does **not** require persistence (V1/S1 are not
/// yet in the ObjectStore — that is 1.6 and repository open/integrity), and it
/// never publishes. Pure: no side effects.
pub fn validate_create_element_invariants(
    prepared: PreparedElementCreation,
) -> Result<PreparedElementCreation, ChangeError> {
    validate_candidate_invariants(&prepared)?;
    Ok(prepared)
}

/// Sorts property keys into canonical order (bytewise comparison of their full
/// deterministic CBOR encodings, RFC 8949 §4.2.1) and rejects duplicate keys.
///
/// This *establishes* the canonical representation from possibly-unordered
/// application input; it is distinct from `canonical_bytes()` refusing to
/// repair an already-constructed canonical object.
fn normalize_properties(
    properties: Vec<(String, PropertyValue)>,
) -> Result<Vec<(String, PropertyValue)>, ChangeError> {
    let mut props = properties.into_iter().collect::<Vec<_>>();
    props.sort_by(|a, b| cmp_encoded_text(&a.0, &b.0));
    for pair in props.windows(2) {
        if cmp_encoded_text(&pair[0].0, &pair[1].0) == Ordering::Equal {
            return Err(ChangeError::DuplicatePropertyKey(pair[1].0.clone()));
        }
    }
    Ok(props)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::{ElementId, OntologyId};
    use crate::domain::state::ElementStateEntry;
    use crate::encoding::canonical_object_id;
    use crate::encoding::object::{CanonicalObject, CanonicalPayload};
    use crate::encoding::validate::CanonicalValidate;
    use crate::repository::init::{init_repository, initial_core_ontology};
    use crate::repository::open::open_repository;
    use uuid::Uuid;

    fn element_id(n: u128) -> ElementId {
        ElementId::from_uuid(Uuid::from_u128(n))
    }

    fn object_id(n: u8) -> ObjectId {
        ObjectId::from_bytes([n; 32])
    }

    /// A `ChangeContext` over a manually constructed base state. `accepted`
    /// references `object_id(1)`; the base state's `ontology_version` is `object_id(2)`.
    fn context_with_base(elements: Vec<ElementStateEntry>) -> ChangeContext {
        context_with_parts(
            elements,
            initial_core_ontology(OntologyId::from_uuid(Uuid::nil())),
        )
    }

    /// A `ChangeContext` whose active ontology is `ontology` (the authoritative,
    /// base-state-referenced ontology for validation, not a global core).
    fn context_with_parts(
        elements: Vec<ElementStateEntry>,
        ontology: OntologyVersion,
    ) -> ChangeContext {
        let base_state = SemanticState {
            ontology_version: object_id(2),
            elements,
            relationships: vec![],
        };
        ChangeContext {
            accepted: AcceptedRef {
                state: object_id(1),
                change: None,
            },
            base_state_id: object_id(1),
            base_state,
            ontology,
        }
    }

    fn input(
        id: u128,
        type_id: &str,
        properties: Vec<(&str, PropertyValue)>,
    ) -> CreateElementInput {
        CreateElementInput {
            element_id: element_id(id),
            type_id: type_id.to_string(),
            properties: properties
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        }
    }

    #[test]
    fn prepare_change_unit_loads_context() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let init = init_repository(root).unwrap();

        let repo = open_repository(root).unwrap();
        let context = prepare_change(&repo).unwrap();

        assert_eq!(context.accepted.state, init.state);
        assert_eq!(context.accepted.change, None);
        assert_eq!(context.base_state_id, init.state);
        assert_eq!(context.base_state.ontology_version, init.ontology);
        assert!(context.base_state.elements.is_empty());
        assert!(context.base_state.relationships.is_empty());
        assert_eq!(context.ontology.element_types.len(), 7);
        assert_eq!(context.ontology.relationship_types.len(), 10);
    }

    #[test]
    fn create_builds_active_element_with_canonical_properties_and_identity() {
        let context = context_with_base(vec![]);
        // Deliberately unordered application input.
        let prepared = apply_create_element(
            context,
            input(
                7,
                "kat.core/requirement",
                vec![
                    ("description", PropertyValue::Text("A requirement".into())),
                    ("title", PropertyValue::Text("The title".into())),
                ],
            ),
        )
        .unwrap();

        // Active lifecycle, supplied identity, supplied type.
        assert_eq!(prepared.element.lifecycle, Lifecycle::Active);
        assert_eq!(prepared.element.element_id, element_id(7));
        assert_eq!(prepared.element.type_id, "kat.core/requirement");

        // Properties canonicalized into canonical key order.
        let keys: Vec<&str> = prepared
            .element
            .properties
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        assert_eq!(keys, ["title", "description"]);

        // Content identity equals the independent encode-then-hash.
        let expected_id = canonical_object_id(&CanonicalObject {
            payload: CanonicalPayload::KnowledgeElementVersion(prepared.element.clone()),
        })
        .unwrap();
        assert_eq!(prepared.element_version_id, expected_id);

        // Candidate SemanticState: ontology unchanged, relationships unchanged,
        // exactly one element mapping E7 -> V1.
        assert_eq!(prepared.candidate_state.ontology_version, object_id(2));
        assert!(prepared.candidate_state.relationships.is_empty());
        assert_eq!(
            prepared.candidate_state.elements,
            vec![ElementStateEntry {
                element_id: element_id(7),
                version: expected_id,
            }]
        );

        // Base state and accepted ref untouched (no mutation, no publication).
        assert!(prepared.context.base_state.elements.is_empty());
        assert_eq!(prepared.context.accepted.change, None);
    }

    #[test]
    fn create_rejects_when_element_id_already_in_base_state() {
        let base = vec![ElementStateEntry {
            element_id: element_id(9),
            version: object_id(5),
        }];
        let context = context_with_base(base);

        let err = apply_create_element(
            context,
            input(
                9,
                "kat.core/requirement",
                vec![("title", PropertyValue::Text("dup".into()))],
            ),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            ChangeError::Precondition(PreconditionError::ElementAlreadyExists(id)) if id == element_id(9)
        ));
    }

    #[test]
    fn create_rejects_duplicate_property_key() {
        let context = context_with_base(vec![]);
        let err = apply_create_element(
            context,
            input(
                1,
                "kat.core/requirement",
                vec![
                    ("title", PropertyValue::Text("a".into())),
                    ("title", PropertyValue::Text("b".into())),
                ],
            ),
        )
        .unwrap_err();
        assert!(matches!(err, ChangeError::DuplicatePropertyKey(k) if k == "title"));
    }

    #[test]
    fn create_accepts_unknown_type_at_step_1_2() {
        // No ontology conformance yet (that is step 1.3); a bogus type reaches
        // the candidate successfully at 1.2, proving the layers are independent.
        let context = context_with_base(vec![]);
        let prepared =
            apply_create_element(context, input(4, "kat.core/not-a-real-type", vec![])).unwrap();
        assert_eq!(prepared.element.type_id, "kat.core/not-a-real-type");
    }

    #[test]
    fn create_inserts_into_nonempty_base_keeping_canonical_order() {
        // Base: E1 and E3 present; creating E2 must land between them.
        let base = vec![
            ElementStateEntry {
                element_id: element_id(1),
                version: object_id(1),
            },
            ElementStateEntry {
                element_id: element_id(3),
                version: object_id(3),
            },
        ];
        let context = context_with_base(base);
        let prepared =
            apply_create_element(context, input(2, "kat.core/requirement", vec![])).unwrap();

        let ids: Vec<u128> = prepared
            .candidate_state
            .elements
            .iter()
            .map(|e| e.element_id.as_uuid().as_u128())
            .collect();
        assert_eq!(ids, [1, 2, 3]);

        // Candidate is structurally canonical: sorted and unique.
        prepared
            .candidate_state
            .validate_canonical_structure()
            .unwrap();
    }

    #[test]
    fn ontology_validation_accepts_known_core_types() {
        for (type_id, id) in [
            ("kat.core/requirement", 1),
            ("kat.core/constraint", 2),
            ("kat.core/implementation", 3),
        ] {
            let prepared =
                apply_create_element(context_with_base(vec![]), input(id, type_id, vec![]))
                    .unwrap();
            let validated = validate_create_element_ontology(prepared).unwrap();
            assert_eq!(validated.element.type_id, type_id);
        }
    }

    #[test]
    fn ontology_validation_rejects_unknown_type() {
        let prepared = apply_create_element(
            context_with_base(vec![]),
            input(9, "kat.core/not-a-real-type", vec![]),
        )
        .unwrap();

        let err = validate_create_element_ontology(prepared).unwrap_err();
        assert!(matches!(
            err,
            ChangeError::Ontology(OntologyError::UnknownElementType(t))
                if t == "kat.core/not-a-real-type"
        ));
    }

    #[test]
    fn ontology_validation_uses_the_base_ontology_not_a_global_core() {
        // A custom authoritative ontology defining only "constraint" — no
        // "requirement". The context's loaded ontology must decide, not a
        // hardcoded core.
        let custom = crate::domain::ontology::OntologyVersion {
            ontology_id: OntologyId::from_uuid(Uuid::from_u128(99)),
            element_types: vec![crate::domain::ontology::ElementTypeDefinition {
                type_id: "kat.core/constraint".into(),
                name: "Constraint".into(),
            }],
            relationship_types: vec![],
        };

        // "requirement" is not in the authoritative (base) ontology -> rejected.
        let prepared = apply_create_element(
            ChangeContext {
                ontology: custom.clone(),
                ..context_with_base(vec![])
            },
            input(2, "kat.core/requirement", vec![]),
        )
        .unwrap();
        let err = validate_create_element_ontology(prepared).unwrap_err();
        assert!(matches!(
            err,
            ChangeError::Ontology(OntologyError::UnknownElementType(t))
                if t == "kat.core/requirement"
        ));
    }

    #[test]
    fn ontology_validation_does_not_mutate_the_prepared_creation() {
        let prepared = apply_create_element(
            context_with_base(vec![]),
            input(7, "kat.core/requirement", vec![]),
        )
        .unwrap();
        let element_before = prepared.element.clone();
        let candidate_before = prepared.candidate_state.clone();
        let context_before_ontology = prepared.context.ontology.clone();

        let validated = validate_create_element_ontology(prepared).unwrap();

        assert_eq!(validated.element, element_before);
        assert_eq!(validated.candidate_state, candidate_before);
        assert_eq!(validated.context.ontology, context_before_ontology);
    }
}
