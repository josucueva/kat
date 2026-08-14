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

use crate::domain::change::ChangeRevision;
use crate::domain::element::{KnowledgeElementVersion, Lifecycle};
use crate::domain::identity::{ChangeId, ElementId, ObjectId};
use crate::domain::ontology::OntologyVersion;
use crate::domain::operation::Operation;
use crate::domain::property::PropertyValue;
use crate::domain::state::{ElementStateEntry, SemanticState};
use crate::encoding::cbor::cmp_encoded_text;
use crate::encoding::decode::DecodingError;
use crate::encoding::decode_canonical;
use crate::encoding::error::EncodingError;
use crate::encoding::object::{CanonicalObject, CanonicalPayload, ObjectKind};
use crate::encoding::{canonical_bytes, canonical_object_id};
use crate::repository::object_store::{ObjectStore, ObjectStoreError};
use crate::repository::open::Repository;
use crate::repository::ref_store::{AcceptedRef, RefStore, RefStoreError};
use crate::repository::validation::invariant::{
    InvariantError, validate_create_element_invariants as validate_candidate_invariants,
    validate_update_element_invariants as validate_update_candidate_invariants,
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
    /// The ElementId is not present in the base state (an update targets an
    /// existing element).
    #[error("element {0} not found in the base state")]
    ElementNotFound(ElementId),
    /// The base state maps the element to a different version than the
    /// operation's `expected_version` (element-level optimistic concurrency).
    #[error("element {element_id} is at version {actual}, but the operation expected {expected}")]
    VersionMismatch {
        /// Stable identity of the element being updated.
        element_id: ElementId,
        /// The version the operation was prepared against (`expected_version`).
        expected: ObjectId,
        /// The version the base state actually maps the element to.
        actual: ObjectId,
    },
    /// The element's current version is not `Active`; update does not perform
    /// lifecycle transitions (deprecation/supersession are explicit operations).
    #[error("element {0} is not active and cannot be updated")]
    ElementNotActive(ElementId),
    /// The update patch contains no properties to change.
    #[error("update contains no properties to change")]
    EmptyUpdate,
    /// The (non-empty) update patch produces a content-identical version
    /// (`Vn+1` ObjectId == `Vn`); the change would not evolve the state.
    #[error(
        "update produces no effective change (the new version is identical to the current version)"
    )]
    NoEffectiveChange,
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
    /// The ObjectStore returned a different ObjectId than the identity derived
    /// during preparation. This is an integrity/programming failure and must
    /// not be silently accepted.
    #[error("persisted identity mismatch for {kind}: expected {expected}, actual {actual}")]
    PersistenceIdentityMismatch {
        /// The canonical kind that was persisted.
        kind: ObjectKind,
        /// The identity derived at preparation time.
        expected: ObjectId,
        /// The identity the ObjectStore content-addressed from the bytes.
        actual: ObjectId,
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
    /// The accepted repository head changed since this change was prepared
    /// (compare-and-swap conflict). The change's immutable objects remain
    /// stored but unreferenced; prepare against the new head and retry.
    #[error(
        "accepted repository state changed since this change was prepared; re-prepare against the new head and retry"
    )]
    Conflict,
    /// A ref store failure during publication, other than a CAS conflict.
    #[error("ref store error: {0}")]
    RefStore(#[from] RefStoreError),
    /// The prepared change is internally inconsistent at the publication
    /// boundary: its `ChangeRevision.result_state` does not match the prepared
    /// `state_id`. Construction (1.5) and persistence (1.6) guarantee these
    /// agree, so a violation here is an integrity/programming failure and the
    /// repository must not make such a Change authoritative.
    #[error(
        "cannot publish inconsistent change: result_state {actual} does not match prepared state {expected}"
    )]
    PublicationStateMismatch {
        /// The prepared candidate SemanticState ObjectId (`state_id`).
        expected: ObjectId,
        /// The ObjectId the ChangeRevision claims as its result state.
        actual: ObjectId,
    },
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

/// Application-level input for an `UpdateElement` operation.
///
/// The user/application representation, not yet a canonical object. The patch
/// is the **subset of properties to change** (`operations.md`: "Properties to
/// change"); the engine merges it onto the element's current full property set
/// to construct the immutable `Vn+1`.
///
/// `expected_version` is an explicit input with element-level optimistic
/// concurrency semantics: the engine requires the base state to still map
/// `element_id` to exactly `expected_version` before applying. This is distinct
/// from (and complements) the publication CAS, which protects the
/// repository-level base state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateElementInput {
    /// Stable identity of the element being updated.
    pub element_id: ElementId,
    /// The version the caller observed/expects to be current (`Vn`); the engine
    /// rejects the update if the base state maps the element elsewhere.
    pub expected_version: ObjectId,
    /// The properties to change, as an unordered application key/value list.
    /// Keys are normalized into canonical order; duplicates are rejected;
    /// unspecified properties are preserved.
    pub properties: Vec<(String, PropertyValue)>,
}

/// A logically-prepared element update: the previous version, the new Active
/// knowledge element version, its content identity, and the candidate
/// SemanticState it maps to.
///
/// Distinct from a full [`ChangeRevision`](crate::domain::change::ChangeRevision):
/// nothing here is persisted and no accepted ref is published. `Sn+1` exists
/// only because `Vn+1`'s ObjectId could be derived from its canonical bytes
/// (hashing is not persistence). The prepared value carries the decoded
/// previous version so step 2.3 can verify identity/type/lifecycle preservation
/// and the single-state-delta invariant without reloading anything.
#[derive(Debug)]
pub struct PreparedElementUpdate {
    /// The change context the update was applied against (unchanged).
    pub context: ChangeContext,
    /// The decoded current version `Vn` (base state's mapping for `E`).
    pub previous_element: KnowledgeElementVersion,
    /// ObjectId of the current version `Vn`.
    pub previous_version_id: ObjectId,
    /// The `expected_version` supplied to the operation (element-level
    /// optimistic concurrency); the invariant layer verifies it equals the base
    /// mapping for `E`.
    pub expected_version: ObjectId,
    /// The constructed Active `KnowledgeElementVersion` `Vn+1` (identity, type,
    /// and lifecycle preserved; patch merged onto the current properties).
    pub element: KnowledgeElementVersion,
    /// The content identity of `element` (SHA-256 of its canonical bytes).
    pub element_version_id: ObjectId,
    /// The candidate SemanticState: base with exactly `E -> Vn` replaced by
    /// `E -> Vn+1` (nothing else changed).
    pub candidate_state: SemanticState,
}

/// Applies a single `UpdateElement` to `context`, producing a candidate only.
///
/// Step 2.1 is confined to **operation application**: preconditions, loading
/// the current version `Vn`, merging the property patch, constructing `Vn+1`,
/// deriving its content identity, and building the candidate SemanticState. It
/// deliberately does **not** perform ontology conformance (2.2) or invariant
/// validation (2.3), and it never persists or publishes.
///
/// Preconditions (in order):
///
/// ```text
/// element exists in base state
/// current version == expected_version
/// current version decodes as KnowledgeElementVersion
/// current lifecycle == Active
/// patch is not empty
/// ```
///
/// The patch is merged onto the current full property set (unspecified
/// properties preserved), the result is canonicalized, `Vn+1` is built, and a
/// `NoEffectiveChange` is rejected when `Vn+1`'s ObjectId equals `Vn`.
///
/// Requires `repository` to load and decode the current version `Vn` from the
/// ObjectStore (the context carries only its ObjectId).
pub fn apply_update_element(
    repository: &Repository,
    context: ChangeContext,
    input: UpdateElementInput,
) -> Result<PreparedElementUpdate, ChangeError> {
    let base_state = &context.base_state;

    // Precondition: the element must exist in the base state.
    let entry = base_state
        .elements
        .iter()
        .find(|e| e.element_id == input.element_id)
        .ok_or(ChangeError::Precondition(
            PreconditionError::ElementNotFound(input.element_id),
        ))?;

    // Precondition: current version == expected_version (element-level
    // optimistic concurrency).
    let previous_version_id = entry.version;
    if previous_version_id != input.expected_version {
        return Err(ChangeError::Precondition(
            PreconditionError::VersionMismatch {
                element_id: input.element_id,
                expected: input.expected_version,
                actual: previous_version_id,
            },
        ));
    }

    // Precondition: the current version loads, decodes canonically, and is a
    // KnowledgeElementVersion (missing -> NotFound; wrong kind -> rejected).
    let previous_element = match load_typed(
        repository.object_store(),
        previous_version_id,
        ObjectKind::KnowledgeElementVersion,
    )?
    .payload
    {
        CanonicalPayload::KnowledgeElementVersion(element) => element,
        _ => unreachable!("kind verified by load_typed"),
    };

    // Precondition: the current lifecycle is Active.
    if previous_element.lifecycle != Lifecycle::Active {
        return Err(ChangeError::Precondition(
            PreconditionError::ElementNotActive(input.element_id),
        ));
    }

    // Precondition: the patch is not empty.
    if input.properties.is_empty() {
        return Err(ChangeError::Precondition(PreconditionError::EmptyUpdate));
    }

    // Merge the patch onto the current full property set (preserving
    // unspecified properties), canonicalizing the result. Duplicate patch keys
    // are rejected inside the merge.
    let properties = merge_property_patch(&previous_element.properties, input.properties)?;

    // Construct Vn+1: identity, type, and lifecycle are preserved (the current
    // version is Active by precondition).
    let element = KnowledgeElementVersion {
        element_id: input.element_id,
        type_id: previous_element.type_id.clone(),
        lifecycle: Lifecycle::Active,
        properties,
    };

    // Derive Vn+1's content identity (encode + SHA-256). No persistence occurs.
    let element_version_id = canonical_object_id(&CanonicalObject {
        payload: CanonicalPayload::KnowledgeElementVersion(element.clone()),
    })?;

    // Reject a no-op: if Vn+1 is content-identical to Vn, nothing changed.
    if element_version_id == previous_version_id {
        return Err(ChangeError::Precondition(
            PreconditionError::NoEffectiveChange,
        ));
    }

    // Candidate state: replace exactly `E -> Vn` with `E -> Vn+1`, nothing else.
    let mut elements = base_state.elements.clone();
    let index = elements
        .iter()
        .position(|e| e.element_id == input.element_id)
        .expect("element presence precondition checked above");
    elements[index].version = element_version_id;

    let candidate_state = SemanticState {
        ontology_version: base_state.ontology_version,
        elements,
        relationships: base_state.relationships.clone(),
    };

    Ok(PreparedElementUpdate {
        context,
        previous_element,
        previous_version_id,
        expected_version: input.expected_version,
        element,
        element_version_id,
        candidate_state,
    })
}

/// Application-level input for a `DeprecateElement` operation (step 3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeprecateElementInput {
    /// Stable identity of the element to deprecate.
    pub element_id: ElementId,
    /// Expected current version ObjectId (`Vn`) for element-level optimistic
    /// concurrency.
    pub expected_version: ObjectId,
}

/// A logically-prepared `DeprecateElement` operation: the operation has been
/// applied to `context` to construct a candidate `KnowledgeElementVersion`
/// `Vn+1` with `lifecycle: Deprecated` and a candidate `SemanticState`, but
/// no validation (3.2/3.3), revision (3.4), persistence (3.5), or publication
/// (3.6) has occurred yet.
#[derive(Debug)]
pub struct PreparedElementDeprecation {
    /// The change context the deprecation was prepared against.
    pub context: ChangeContext,
    /// The previous version `Vn` loaded from the base state.
    pub previous_element: KnowledgeElementVersion,
    /// Content identity of `previous_element` (`Vn`).
    pub previous_version_id: ObjectId,
    /// Expected current version ObjectId (`Vn`) supplied by caller.
    pub expected_version: ObjectId,
    /// The newly-constructed version `Vn+1` (`lifecycle: Deprecated`).
    pub element: KnowledgeElementVersion,
    /// Content identity of `element` (`Vn+1`).
    pub element_version_id: ObjectId,
    /// Candidate `SemanticState`: base with `E -> Vn` replaced by `E -> Vn+1`.
    pub candidate_state: SemanticState,
}

/// Applies a single `DeprecateElement` to `context`, producing a candidate only.
///
/// Preconditions (in order):
/// ```text
/// element exists in base state
/// current version == expected_version
/// current version decodes as KnowledgeElementVersion
/// current lifecycle == Active
/// ```
///
/// Constructs `Vn+1` preserving identity, type, and properties with `lifecycle: Deprecated`.
pub fn apply_deprecate_element(
    repository: &Repository,
    context: ChangeContext,
    input: DeprecateElementInput,
) -> Result<PreparedElementDeprecation, ChangeError> {
    let base_state = &context.base_state;

    // Precondition: the element must exist in the base state.
    let entry = base_state
        .elements
        .iter()
        .find(|e| e.element_id == input.element_id)
        .ok_or(ChangeError::Precondition(
            PreconditionError::ElementNotFound(input.element_id),
        ))?;

    // Precondition: current version == expected_version.
    let previous_version_id = entry.version;
    if previous_version_id != input.expected_version {
        return Err(ChangeError::Precondition(
            PreconditionError::VersionMismatch {
                element_id: input.element_id,
                expected: input.expected_version,
                actual: previous_version_id,
            },
        ));
    }

    // Precondition: load and decode current version.
    let previous_element = match load_typed(
        repository.object_store(),
        previous_version_id,
        ObjectKind::KnowledgeElementVersion,
    )?
    .payload
    {
        CanonicalPayload::KnowledgeElementVersion(element) => element,
        _ => unreachable!("kind verified by load_typed"),
    };

    // Precondition: current lifecycle must be Active.
    if previous_element.lifecycle != Lifecycle::Active {
        return Err(ChangeError::Precondition(
            PreconditionError::ElementNotActive(input.element_id),
        ));
    }

    // Construct Vn+1: identity, type, and properties are preserved; lifecycle = Deprecated.
    let element = KnowledgeElementVersion {
        element_id: input.element_id,
        type_id: previous_element.type_id.clone(),
        lifecycle: Lifecycle::Deprecated,
        properties: previous_element.properties.clone(),
    };

    // Derive Vn+1's content identity. No persistence occurs.
    let element_version_id = canonical_object_id(&CanonicalObject {
        payload: CanonicalPayload::KnowledgeElementVersion(element.clone()),
    })?;

    // Candidate state: replace E -> Vn with E -> Vn+1.
    let mut elements = base_state.elements.clone();
    let index = elements
        .iter()
        .position(|e| e.element_id == input.element_id)
        .expect("element presence precondition checked above");
    elements[index].version = element_version_id;

    let candidate_state = SemanticState {
        ontology_version: base_state.ontology_version,
        elements,
        relationships: base_state.relationships.clone(),
    };

    Ok(PreparedElementDeprecation {
        context,
        previous_element,
        previous_version_id,
        expected_version: input.expected_version,
        element,
        element_version_id,
        candidate_state,
    })
}

/// Applies the step 2.2 ontology-conformance stage to a prepared element
/// update: the newly constructed `Vn+1.type_id` must exist in the base
/// `OntologyVersion`.
///
/// This **reuses** [`validate_element_type`] — there is no Update-specific
/// ontology semantics. It validates `prepared.element.type_id` (the newly
/// built `Vn+1`), not any independently supplied type. Because step 2.1
/// preserves the type, this effectively proves the updated version remains
/// conformant with the authoritative base ontology. The validator uses **only**
/// `prepared.context.ontology` — the ontology loaded from the repository's
/// `base_state.ontology_version` — never a global core. Step 2.2 enforces this
/// single rule (`Vn+1.type_id ∈ base ontology.element_types`) and nothing else:
/// no property-schema validation, no invariant validation (2.3), no
/// persistence, no ChangeRevision, no CAS.
pub fn validate_update_element_ontology(
    prepared: PreparedElementUpdate,
) -> Result<PreparedElementUpdate, ChangeError> {
    validate_element_type(&prepared.context.ontology, &prepared.element.type_id)?;
    Ok(prepared)
}

/// Applies the step 3.2 ontology-conformance stage to a prepared element
/// deprecation: the newly constructed `Vn+1.type_id` must exist in the base
/// `OntologyVersion`.
///
/// This **reuses** [`validate_element_type`]. It validates `prepared.element.type_id`
/// (the newly built `Vn+1`), proving the deprecated version remains conformant
/// with the base ontology. Purely preparatory; no persistence or CAS.
pub fn validate_deprecate_element_ontology(
    prepared: PreparedElementDeprecation,
) -> Result<PreparedElementDeprecation, ChangeError> {
    validate_element_type(&prepared.context.ontology, &prepared.element.type_id)?;
    Ok(prepared)
}

/// A `PreparedElementUpdate` that has passed the Phase 2 semantic validation
/// pipeline (ontology conformance 2.2 + invariant validation 2.3).
///
/// The wrapped [`PreparedElementUpdate`] is not exposed for mutation and is
/// consumed only by step 2.4 (`prepare_update_revision`), so a `ChangeRevision`
/// cannot be constructed from an unvalidated update through the normal API. The
/// type system guarantees the pipeline is not bypassable.
#[derive(Debug)]
pub struct ValidatedElementUpdate {
    prepared: PreparedElementUpdate,
}

impl ValidatedElementUpdate {
    /// Borrows the underlying prepared update (read-only; it remains validated
    /// and cannot be moved out to construct a revision directly).
    pub fn prepared(&self) -> &PreparedElementUpdate {
        &self.prepared
    }
}

/// Applies the step 2.3 invariant stage to a prepared element update.
///
/// Validates the **candidate SemanticState** invariants — the normative
/// single-state-delta rule plus identity/type/lifecycle preservation, Vn+1
/// identity + reference, and candidate coherence — via
/// [`validate_update_candidate_invariants`]. It does **not** require
/// persistence (Vn+1/Sn+1 are not yet in the ObjectStore — that is 2.5 and
/// repository open/integrity), and it never publishes. Pure: no side effects.
///
/// Returns a [`ValidatedElementUpdate`] so that step 2.4 can only construct a
/// `ChangeRevision` from a validated candidate.
pub fn validate_update_element_invariants(
    prepared: PreparedElementUpdate,
) -> Result<ValidatedElementUpdate, ChangeError> {
    validate_update_candidate_invariants(&prepared)?;
    Ok(ValidatedElementUpdate { prepared })
}

/// A `PreparedElementDeprecation` that has passed the Phase 3 semantic validation
/// pipeline (ontology conformance 3.2 + invariant validation 3.3).
#[derive(Debug)]
pub struct ValidatedElementDeprecation {
    prepared: PreparedElementDeprecation,
}

impl ValidatedElementDeprecation {
    /// Borrows the underlying prepared deprecation (read-only).
    pub fn prepared(&self) -> &PreparedElementDeprecation {
        &self.prepared
    }
}

/// Applies the step 3.3 invariant stage to a prepared element deprecation.
///
/// Validates candidate-state invariants via
/// [`validate_deprecate_element_invariants`].
pub fn validate_deprecate_element_invariants(
    prepared: PreparedElementDeprecation,
) -> Result<ValidatedElementDeprecation, ChangeError> {
    crate::repository::validation::invariant::validate_deprecate_element_invariants(&prepared)?;
    Ok(ValidatedElementDeprecation { prepared })
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
///
/// Returns a [`ValidatedElementCreation`] so that a `ChangeRevision` can only
/// be constructed from a candidate that has passed ontology + invariant
/// validation (the type system enforces this; a raw `PreparedElementCreation`
/// cannot be used to prepare a revision).
pub fn validate_create_element_invariants(
    prepared: PreparedElementCreation,
) -> Result<ValidatedElementCreation, ChangeError> {
    validate_candidate_invariants(&prepared)?;
    Ok(ValidatedElementCreation { prepared })
}

/// A `CreateElement` candidate that has passed the Phase 1 semantic validation
/// pipeline (ontology conformance 1.3 + invariant validation 1.4).
///
/// The wrapped [`PreparedElementCreation`] is not exposed for mutation and is
/// consumed only by [`prepare_change_revision`], so a `ChangeRevision` cannot
/// be constructed through the normal API from an unvalidated candidate. The
/// type system guarantees the pipeline is not bypassable.
#[derive(Debug)]
pub struct ValidatedElementCreation {
    prepared: PreparedElementCreation,
}

impl ValidatedElementCreation {
    /// Borrows the underlying prepared creation (read-only; it remains
    /// validated and cannot be moved out to construct a revision directly).
    pub fn prepared(&self) -> &PreparedElementCreation {
        &self.prepared
    }
}

/// A logically-prepared ChangeRevision: the derived candidate-state ObjectId,
/// the full `ChangeRevision`, and its content identity.
///
/// Still purely preparatory: V1/S1/C1 ObjectIds are known but **no object is
/// persisted** and the accepted ref is unchanged.
#[derive(Debug)]
pub struct PreparedChangeRevision {
    /// The validated element creation this change wraps.
    pub creation: PreparedElementCreation,
    /// ObjectId of the candidate SemanticState (`S1`), derived from its
    /// canonical bytes.
    pub state_id: ObjectId,
    /// The ChangeRevision (`C1`).
    pub change: ChangeRevision,
    /// ObjectId of `change`, derived from its canonical bytes.
    pub change_revision_id: ObjectId,
}

/// Constructs the `ChangeRevision` for a validated `CreateElement`, deriving
/// all content identities. Step 1.5 remains **purely preparatory**: it
/// computes `S1` and `C1` ObjectIds but persists and publishes nothing.
///
/// `change_id` and `description` are supplied by the caller (application/CLI
/// layer); the engine stays deterministic. `dependencies` is the accepted
/// Change head (`context.accepted.change`), recording causal ancestry without
/// hardcoding "first Change" semantics:
///
/// ```text
/// accepted.change == none      -> dependencies == []
/// accepted.change == Some(Cn)  -> dependencies == [Cn]
/// ```
///
/// `result_state` and `change_revision_id` are derived here (encode-then-hash),
/// which also exercises canonical structural validation.
pub fn prepare_change_revision(
    validated: ValidatedElementCreation,
    change_id: ChangeId,
    description: Option<String>,
) -> Result<PreparedChangeRevision, ChangeError> {
    let creation = validated.prepared;

    // S1 ObjectId derived from the candidate state's canonical bytes.
    let state_id = canonical_object_id(&CanonicalObject {
        payload: CanonicalPayload::SemanticState(creation.candidate_state.clone()),
    })?;

    // Dependencies = the accepted Change head (canonically ordered; at most
    // one at this linear stage).
    let dependencies: Vec<ObjectId> = creation.context.accepted.change.into_iter().collect();

    let change = ChangeRevision {
        change_id,
        base_states: vec![creation.context.base_state_id],
        result_state: state_id,
        operations: vec![Operation::CreateElement {
            new_version: creation.element_version_id,
        }],
        dependencies,
        description,
    };

    // C1 ObjectId derived from the ChangeRevision's canonical bytes.
    let change_revision_id = canonical_object_id(&CanonicalObject {
        payload: CanonicalPayload::ChangeRevision(change.clone()),
    })?;

    Ok(PreparedChangeRevision {
        creation,
        state_id,
        change,
        change_revision_id,
    })
}

/// A logically-prepared Update ChangeRevision: the derived candidate-state
/// ObjectId, the full `ChangeRevision`, and its content identity.
///
/// Still purely preparatory: Vn+1/Sn+1/Cn+1 ObjectIds are known but **no object
/// is persisted** and the accepted ref is unchanged.
#[derive(Debug)]
pub struct PreparedUpdateChangeRevision {
    /// The prepared element update this change wraps.
    pub update: PreparedElementUpdate,
    /// ObjectId of the candidate SemanticState (`Sn+1`), derived from its
    /// canonical bytes.
    pub state_id: ObjectId,
    /// The ChangeRevision (`Cn+1`).
    pub change: ChangeRevision,
    /// ObjectId of `change`, derived from its canonical bytes.
    pub change_revision_id: ObjectId,
}

/// Constructs the `ChangeRevision` for a validated `UpdateElement`, deriving
/// all content identities. Step 2.4 remains **purely preparatory**: it
/// computes `Sn+1` and `Cn+1` ObjectIds but persists and publishes nothing.
///
/// `change_id` and `description` are supplied by the caller (application/CLI
/// layer); the engine stays deterministic. `dependencies` is the accepted
/// Change head (`context.accepted.change`), recording causal ancestry without
/// hardcoding "first Change" semantics:
///
/// ```text
/// accepted.change == none      -> dependencies == []
/// accepted.change == Some(Cn)  -> dependencies == [Cn]
/// ```
///
/// `result_state` and `change_revision_id` are derived here (encode-then-hash),
/// which also exercises canonical structural validation.
pub fn prepare_update_change_revision(
    validated: ValidatedElementUpdate,
    change_id: ChangeId,
    description: Option<String>,
) -> Result<PreparedUpdateChangeRevision, ChangeError> {
    let update = validated.prepared;

    // Sn+1 ObjectId derived from the candidate state's canonical bytes.
    let state_id = canonical_object_id(&CanonicalObject {
        payload: CanonicalPayload::SemanticState(update.candidate_state.clone()),
    })?;

    // Dependencies = the accepted Change head (canonically ordered; at most
    // one at this linear stage).
    let dependencies: Vec<ObjectId> = update.context.accepted.change.into_iter().collect();

    let change = ChangeRevision {
        change_id,
        base_states: vec![update.context.base_state_id],
        result_state: state_id,
        operations: vec![Operation::UpdateElement {
            element_id: update.element.element_id,
            expected_version: update.previous_version_id,
            new_version: update.element_version_id,
        }],
        dependencies,
        description,
    };

    // Cn+1 ObjectId derived from the ChangeRevision's canonical bytes.
    let change_revision_id = canonical_object_id(&CanonicalObject {
        payload: CanonicalPayload::ChangeRevision(change.clone()),
    })?;

    Ok(PreparedUpdateChangeRevision {
        update,
        state_id,
        change,
        change_revision_id,
    })
}

/// A logically-prepared Deprecate ChangeRevision: the derived candidate-state
/// ObjectId, the full `ChangeRevision`, and its content identity.
///
/// Still purely preparatory: Vn+1/Sn+1/Cn+1 ObjectIds are known but **no object
/// is persisted** and the accepted ref is unchanged.
#[derive(Debug)]
pub struct PreparedDeprecateChangeRevision {
    /// The prepared element deprecation this change wraps.
    pub deprecation: PreparedElementDeprecation,
    /// ObjectId of the candidate SemanticState (`Sn+1`).
    pub state_id: ObjectId,
    /// The ChangeRevision (`Cn+1`).
    pub change: ChangeRevision,
    /// ObjectId of `change`.
    pub change_revision_id: ObjectId,
}

/// Constructs the `ChangeRevision` for a validated `DeprecateElement`, deriving
/// all content identities. Step 3.4 remains **purely preparatory**.
pub fn prepare_deprecate_change_revision(
    validated: ValidatedElementDeprecation,
    change_id: ChangeId,
    description: Option<String>,
) -> Result<PreparedDeprecateChangeRevision, ChangeError> {
    let deprecation = validated.prepared;

    // Sn+1 ObjectId derived from the candidate state's canonical bytes.
    let state_id = canonical_object_id(&CanonicalObject {
        payload: CanonicalPayload::SemanticState(deprecation.candidate_state.clone()),
    })?;

    // Dependencies = the accepted Change head.
    let dependencies: Vec<ObjectId> = deprecation.context.accepted.change.into_iter().collect();

    let change = ChangeRevision {
        change_id,
        base_states: vec![deprecation.context.base_state_id],
        result_state: state_id,
        operations: vec![Operation::DeprecateElement {
            element_id: deprecation.element.element_id,
            expected_version: deprecation.previous_version_id,
            new_version: deprecation.element_version_id,
        }],
        dependencies,
        description,
    };

    // Cn+1 ObjectId derived from the ChangeRevision's canonical bytes.
    let change_revision_id = canonical_object_id(&CanonicalObject {
        payload: CanonicalPayload::ChangeRevision(change.clone()),
    })?;

    Ok(PreparedDeprecateChangeRevision {
        deprecation,
        state_id,
        change,
        change_revision_id,
    })
}

/// A prepared change whose immutable objects have been materialized into the
/// ObjectStore (V1, S1, C1), but which has **not** been published.
///
/// The accepted ref is untouched — the new objects are unreferenced (an
/// intentionally-valid, harmless state). Step 1.7 publication will require
/// this type so a Change cannot be published before its objects are persisted.
#[derive(Debug)]
pub struct PersistedChange {
    /// The prepared change whose objects were just persisted.
    pub prepared: PreparedChangeRevision,
}

/// Materializes a prepared, validated change's immutable objects into the
/// ObjectStore in reference order — `V1`, `S1`, then `C1`:
///
/// ```text
/// C1 -> S1 -> V1
/// ```
///
/// Each `ObjectStore::put` returns the content-derived ObjectId (the store
/// hashes the bytes itself), which is verified against the identity derived at
/// preparation time. A mismatch is an integrity/programming failure and is
/// rejected. Step 1.6 does **not** publish: `refs/accepted` is left exactly as
/// it was. No rollback/GC — objects persisted before a failure remain as
/// unreachable immutable objects (harmless; reclaimable by a future GC).
pub fn persist_prepared_change(
    repository: &Repository,
    prepared: PreparedChangeRevision,
) -> Result<PersistedChange, ChangeError> {
    let store = repository.object_store();

    // V1
    let v1_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::KnowledgeElementVersion(prepared.creation.element.clone()),
    })?;
    let v1_id = store.put(&v1_bytes)?;
    if v1_id != prepared.creation.element_version_id {
        return Err(identity_mismatch(
            ObjectKind::KnowledgeElementVersion,
            prepared.creation.element_version_id,
            v1_id,
        ));
    }

    // S1
    let s1_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::SemanticState(prepared.creation.candidate_state.clone()),
    })?;
    let s1_id = store.put(&s1_bytes)?;
    if s1_id != prepared.state_id {
        return Err(identity_mismatch(
            ObjectKind::SemanticState,
            prepared.state_id,
            s1_id,
        ));
    }

    // C1
    let c1_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::ChangeRevision(prepared.change.clone()),
    })?;
    let c1_id = store.put(&c1_bytes)?;
    if c1_id != prepared.change_revision_id {
        return Err(identity_mismatch(
            ObjectKind::ChangeRevision,
            prepared.change_revision_id,
            c1_id,
        ));
    }

    Ok(PersistedChange { prepared })
}

/// A prepared update change whose immutable objects have been materialized into
/// the ObjectStore (Vn+1, Sn+1, Cn+1), but which has **not** been published.
///
/// The accepted ref is untouched — the new objects are unreferenced (an
/// intentionally-valid, harmless state). Step 2.6 publication will require
/// this type so an Update Change cannot be published before its objects are
/// persisted.
#[derive(Debug)]
pub struct PersistedUpdateChange {
    /// The prepared update change whose objects were just persisted.
    pub prepared: PreparedUpdateChangeRevision,
}

/// Materializes a prepared, validated update change's immutable objects into
/// the ObjectStore in reference order — `Vn+1`, `Sn+1`, then `Cn+1`:
///
/// ```text
/// Cn+1 -> Sn+1 -> Vn+1
/// ```
///
/// Each `ObjectStore::put` returns the content-derived ObjectId (the store
/// hashes the bytes itself), which is verified against the identity derived at
/// preparation time. A mismatch is an integrity/programming failure and is
/// rejected. Step 2.5 does **not** publish: `refs/accepted` is left exactly as
/// it was. No rollback/GC — objects persisted before a failure remain as
/// unreachable immutable objects (harmless; reclaimable by a future GC).
pub fn persist_prepared_update_change(
    repository: &Repository,
    prepared: PreparedUpdateChangeRevision,
) -> Result<PersistedUpdateChange, ChangeError> {
    let store = repository.object_store();

    // Vn+1
    let v_next_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::KnowledgeElementVersion(prepared.update.element.clone()),
    })?;
    let v_next_id = store.put(&v_next_bytes)?;
    if v_next_id != prepared.update.element_version_id {
        return Err(identity_mismatch(
            ObjectKind::KnowledgeElementVersion,
            prepared.update.element_version_id,
            v_next_id,
        ));
    }

    // Sn+1
    let s_next_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::SemanticState(prepared.update.candidate_state.clone()),
    })?;
    let s_next_id = store.put(&s_next_bytes)?;
    if s_next_id != prepared.state_id {
        return Err(identity_mismatch(
            ObjectKind::SemanticState,
            prepared.state_id,
            s_next_id,
        ));
    }

    // Cn+1
    let c_next_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::ChangeRevision(prepared.change.clone()),
    })?;
    let c_next_id = store.put(&c_next_bytes)?;
    if c_next_id != prepared.change_revision_id {
        return Err(identity_mismatch(
            ObjectKind::ChangeRevision,
            prepared.change_revision_id,
            c_next_id,
        ));
    }

    Ok(PersistedUpdateChange { prepared })
}

/// A prepared deprecate change whose immutable objects have been materialized into
/// the ObjectStore (`Vn+1`, `Sn+1`, `Cn+1`), but which has **not** been published.
#[derive(Debug)]
pub struct PersistedDeprecateChange {
    /// The prepared deprecate change whose objects were just persisted.
    pub prepared: PreparedDeprecateChangeRevision,
}

/// Materializes a prepared, validated deprecate change's immutable objects into
/// the ObjectStore in reference order: `Vn+1`, `Sn+1`, then `Cn+1`.
///
/// Leaves `refs/accepted` untouched.
pub fn persist_prepared_deprecate_change(
    repository: &Repository,
    prepared: PreparedDeprecateChangeRevision,
) -> Result<PersistedDeprecateChange, ChangeError> {
    let store = repository.object_store();

    // Vn+1
    let v_next_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::KnowledgeElementVersion(prepared.deprecation.element.clone()),
    })?;
    let v_next_id = store.put(&v_next_bytes)?;
    if v_next_id != prepared.deprecation.element_version_id {
        return Err(identity_mismatch(
            ObjectKind::KnowledgeElementVersion,
            prepared.deprecation.element_version_id,
            v_next_id,
        ));
    }

    // Sn+1
    let s_next_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::SemanticState(prepared.deprecation.candidate_state.clone()),
    })?;
    let s_next_id = store.put(&s_next_bytes)?;
    if s_next_id != prepared.state_id {
        return Err(identity_mismatch(
            ObjectKind::SemanticState,
            prepared.state_id,
            s_next_id,
        ));
    }

    // Cn+1
    let c_next_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::ChangeRevision(prepared.change.clone()),
    })?;
    let c_next_id = store.put(&c_next_bytes)?;
    if c_next_id != prepared.change_revision_id {
        return Err(identity_mismatch(
            ObjectKind::ChangeRevision,
            prepared.change_revision_id,
            c_next_id,
        ));
    }

    Ok(PersistedDeprecateChange { prepared })
}

/// A persisted update change that has been atomically published as the
/// repository's accepted head (step 2.6).
///
/// Publication only moves `refs/accepted`; the immutable objects were already
/// materialized by persistence (step 2.5). `accepted` is the new head
/// `{ state: Sn+1, change: Some(Cn+1) }`, with `Cn+1.result_state == Sn+1`.
#[derive(Debug)]
pub struct PublishedUpdateChange {
    /// The persisted update change that was just published.
    pub persisted: PersistedUpdateChange,
    /// The new accepted repository head (`state: Sn+1`, `change: Some(Cn+1)`).
    pub accepted: AcceptedRef,
}

/// Publishes an already-persisted update change by atomically advancing the
/// accepted State and Change head — **and only if** the repository is still at
/// the accepted ref the change was prepared against.
///
/// The core is a single compare-and-swap:
///
/// ```text
/// expected = persisted.prepared.update.context.accepted
/// new      = { state: Sn+1, change: Some(Cn+1) }
/// compare_and_swap_accepted(expected, new)
/// ```
///
/// All semantic preparation, validation, encoding, hashing, and persistence
/// already happened before this step, so publication is intentionally trivial.
/// The API requires a [`PersistedUpdateChange`] — a raw `PreparedUpdateChangeRevision`
/// cannot reach it, so a change cannot be published before its immutable
/// objects exist in the ObjectStore (a compile-time pipeline guarantee).
///
/// Before the CAS, the publication-boundary invariant
/// `prepared.change.result_state == prepared.state_id` is verified fail-fast
/// (construction in 2.4 already guarantees it; this is a defensive check at
/// the point where the Change becomes authoritative).
///
/// On a CAS conflict the accepted ref is left as the concurrent winner and
/// this change's objects remain stored but unreferenced — that is the intended
/// concurrency outcome, not corruption, and nothing is rolled back.
pub fn publish_persisted_update_change(
    repository: &Repository,
    persisted: PersistedUpdateChange,
) -> Result<PublishedUpdateChange, ChangeError> {
    let prepared = &persisted.prepared;

    // Critical publication-boundary invariant: the ChangeRevision's result
    // state must be exactly the prepared SemanticState Sn+1. Construction (2.4)
    // and persistence (2.5) guarantee this by construction; this cheap check
    // runs at the point where the repository is about to make the Change
    // authoritative.
    if prepared.change.result_state != prepared.state_id {
        return Err(ChangeError::PublicationStateMismatch {
            expected: prepared.state_id,
            actual: prepared.change.result_state,
        });
    }

    // expected: the accepted ref the update was prepared against.
    // new: Sn+1 + Cn+1, built from the prepared identities, so by construction
    // new.state == Cn+1.result_state and new.change == Cn+1 ObjectId.
    let expected = &prepared.update.context.accepted;
    let new = AcceptedRef {
        state: prepared.state_id,
        change: Some(prepared.change_revision_id),
    };

    match repository
        .ref_store()
        .compare_and_swap_accepted(expected, &new)
    {
        Ok(()) => Ok(PublishedUpdateChange {
            persisted,
            accepted: new,
        }),
        Err(RefStoreError::Conflict) => Err(ChangeError::Conflict),
        Err(err) => Err(ChangeError::RefStore(err)),
    }
}

/// A persisted deprecate change that has been atomically published as the
/// repository's accepted head (step 3.6).
#[derive(Debug)]
pub struct PublishedDeprecateChange {
    /// The persisted deprecate change that was just published.
    pub persisted: PersistedDeprecateChange,
    /// The new accepted repository head (`state: Sn+1`, `change: Some(Cn+1)`).
    pub accepted: AcceptedRef,
}

/// Publishes an already-persisted deprecate change by atomically advancing the
/// accepted State and Change head.
pub fn publish_persisted_deprecate_change(
    repository: &Repository,
    persisted: PersistedDeprecateChange,
) -> Result<PublishedDeprecateChange, ChangeError> {
    let prepared = &persisted.prepared;

    if prepared.change.result_state != prepared.state_id {
        return Err(ChangeError::PublicationStateMismatch {
            expected: prepared.state_id,
            actual: prepared.change.result_state,
        });
    }

    let expected = &prepared.deprecation.context.accepted;
    let new = AcceptedRef {
        state: prepared.state_id,
        change: Some(prepared.change_revision_id),
    };

    match repository
        .ref_store()
        .compare_and_swap_accepted(expected, &new)
    {
        Ok(()) => Ok(PublishedDeprecateChange {
            persisted,
            accepted: new,
        }),
        Err(RefStoreError::Conflict) => Err(ChangeError::Conflict),
        Err(err) => Err(ChangeError::RefStore(err)),
    }
}

/// A persisted change that has been atomically published as the repository's
/// accepted head (step 1.7).
///
/// Publication only moves `refs/accepted`; the immutable objects were already
/// materialized by persistence (step 1.6). `accepted` is the new head
/// `{ state: S1, change: Some(C1) }`, with `C1.result_state == S1`.
#[derive(Debug)]
pub struct PublishedChange {
    /// The persisted change that was just published.
    pub persisted: PersistedChange,
    /// The new accepted repository head (`state: S1`, `change: Some(C1)`).
    pub accepted: AcceptedRef,
}

/// Publishes an already-persisted change by atomically advancing the accepted
/// State and Change head — **and only if** the repository is still at the
/// accepted ref the change was prepared against.
///
/// The core is a single compare-and-swap:
///
/// ```text
/// expected = persisted.prepared.creation.context.accepted
/// new      = { state: S1, change: Some(C1) }
/// compare_and_swap_accepted(expected, new)
/// ```
///
/// All semantic preparation, validation, encoding, hashing, and persistence
/// already happened before this step, so publication is intentionally trivial.
/// The API requires a [`PersistedChange`] — a raw `PreparedChangeRevision`
/// cannot reach it, so a change cannot be published before its immutable
/// objects exist in the ObjectStore (a compile-time pipeline guarantee).
///
/// Before the CAS, the publication-boundary invariant
/// `prepared.change.result_state == prepared.state_id` is verified fail-fast
/// (construction in 1.5 already guarantees it; this is a defensive check at
/// the point where the Change becomes authoritative). `new.state ==
/// C1.result_state` and `new.change == C1 ObjectId` then hold by construction,
/// because `new` is built from the prepared identities.
///
/// On a CAS conflict the accepted ref is left as the concurrent winner and
/// this change's objects remain stored but unreferenced — that is the intended
/// concurrency outcome, not corruption, and nothing is rolled back. The
/// repository-open integrity layer independently verifies the persisted
/// relationship later.
pub fn publish_persisted_change(
    repository: &Repository,
    persisted: PersistedChange,
) -> Result<PublishedChange, ChangeError> {
    let prepared = &persisted.prepared;

    // Critical publication-boundary invariant: the ChangeRevision's result
    // state must be exactly the prepared SemanticState S1. Construction (1.5)
    // and persistence (1.6) guarantee this by construction; this cheap check
    // runs at the point where the repository is about to make the Change
    // authoritative.
    if prepared.change.result_state != prepared.state_id {
        return Err(ChangeError::PublicationStateMismatch {
            expected: prepared.state_id,
            actual: prepared.change.result_state,
        });
    }

    // expected: the accepted ref the change was prepared against.
    // new: S1 + C1, built from the prepared identities, so by construction
    // new.state == C1.result_state and new.change == C1 ObjectId.
    let expected = &prepared.creation.context.accepted;
    let new = AcceptedRef {
        state: prepared.state_id,
        change: Some(prepared.change_revision_id),
    };

    match repository
        .ref_store()
        .compare_and_swap_accepted(expected, &new)
    {
        Ok(()) => Ok(PublishedChange {
            persisted,
            accepted: new,
        }),
        // The accepted head moved since preparation: surface the domain-level
        // conflict. The losing change's objects remain stored, unreferenced.
        Err(RefStoreError::Conflict) => Err(ChangeError::Conflict),
        Err(e) => Err(ChangeError::RefStore(e)),
    }
}

fn identity_mismatch(kind: ObjectKind, expected: ObjectId, actual: ObjectId) -> ChangeError {
    ChangeError::PersistenceIdentityMismatch {
        kind,
        expected,
        actual,
    }
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

/// Overlays an update **patch** onto a base property set.
///
/// The patch is the subset of properties to change (`operations.md`): keys
/// present in the patch replace the base value, new keys are added, and all
/// other (unspecified) base properties are preserved. The patch is normalized
/// first (canonical order + duplicate-key rejection, via [`normalize_properties`]),
/// then the merged result is re-canonicalized by encoded-key order.
fn merge_property_patch(
    base: &[(String, PropertyValue)],
    patch: Vec<(String, PropertyValue)>,
) -> Result<Vec<(String, PropertyValue)>, ChangeError> {
    let patch = normalize_properties(patch)?;
    let mut merged = base.to_vec();
    for (key, value) in patch {
        if let Some(slot) = merged.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = value;
        } else {
            merged.push((key, value));
        }
    }
    merged.sort_by(|a, b| cmp_encoded_text(&a.0, &b.0));
    Ok(merged)
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

    #[test]
    fn prepare_change_revision_first_change_has_no_dependencies() {
        let context = context_with_base(vec![]);
        let validated = validate_create_element_invariants(
            validate_create_element_ontology(
                apply_create_element(context, input(5, "kat.core/requirement", vec![])).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

        let change_id = crate::domain::identity::ChangeId::from_uuid(Uuid::from_u128(7));
        let revision = prepare_change_revision(validated, change_id, None).unwrap();

        // First Change: accepted.change == none -> dependencies == [].
        assert_eq!(revision.change.base_states, vec![object_id(1)]);
        assert!(revision.change.dependencies.is_empty());
        assert_eq!(revision.change.description, None);
        assert_eq!(revision.change.change_id, change_id);
        assert!(matches!(
            &revision.change.operations[0],
            Operation::CreateElement { new_version } if *new_version == revision.creation.element_version_id
        ));
        assert_eq!(revision.change.result_state, revision.state_id);
    }

    #[test]
    fn prepare_change_revision_records_accepted_change_head_as_dependency() {
        // A later Change: the accepted head is Some(Cn) -> dependencies == [Cn].
        let previous = object_id(50);
        let context = ChangeContext {
            accepted: AcceptedRef {
                state: object_id(1),
                change: Some(previous),
            },
            ..context_with_base(vec![])
        };
        let validated = validate_create_element_invariants(
            validate_create_element_ontology(
                apply_create_element(context, input(6, "kat.core/requirement", vec![])).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

        let change_id = crate::domain::identity::ChangeId::from_uuid(Uuid::from_u128(8));
        let revision = prepare_change_revision(validated, change_id, None).unwrap();

        assert_eq!(revision.change.dependencies, vec![previous]);
        assert_eq!(revision.change.base_states, vec![object_id(1)]);
    }

    #[test]
    fn prepare_change_revision_preserves_supplied_description() {
        let context = context_with_base(vec![]);
        let validated = validate_create_element_invariants(
            validate_create_element_ontology(
                apply_create_element(context, input(9, "kat.core/requirement", vec![])).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

        let change_id = crate::domain::identity::ChangeId::from_uuid(Uuid::from_u128(9));
        let revision =
            prepare_change_revision(validated, change_id, Some("create requirement".to_string()))
                .unwrap();
        assert_eq!(
            revision.change.description.as_deref(),
            Some("create requirement")
        );
    }
}
