//! Read-side queries over the accepted repository head (step 1.8).
//!
//! Queries are strictly read-only: they never mutate the object store or
//! `refs/accepted`. [`show_element`] resolves the **current** accepted ref at
//! query time (a point-in-time read, so a handle that just published a change
//! sees the new element without reopening), loads the referenced
//! [`SemanticState`](crate::domain::state::SemanticState), resolves an
//! [`ElementId`] to its current version ObjectId, and decodes + kind-checks
//! the [`KnowledgeElementVersion`].
//!
//! This is the read counterpart of `change.rs`: the engine mutates through
//! prepared/persisted/published typestates; queries only observe.

use crate::domain::element::KnowledgeElementVersion;
use crate::domain::identity::{ElementId, ObjectId};
use crate::encoding::decode::DecodingError;
use crate::encoding::decode_canonical;
use crate::encoding::object::{CanonicalObject, CanonicalPayload, ObjectKind};
use crate::repository::object_store::{ObjectStore, ObjectStoreError};
use crate::repository::open::Repository;
use crate::repository::ref_store::{RefStore, RefStoreError};

/// Error produced by read-side queries.
#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    /// A ref store failure while reading the accepted head.
    #[error("ref store error: {0}")]
    RefStore(#[from] RefStoreError),
    /// An object store failure while loading a referenced object.
    #[error("object store error: {0}")]
    ObjectStore(#[from] ObjectStoreError),
    /// A referenced object failed strict canonical decoding.
    #[error("decoding error: {0}")]
    Decoding(#[from] DecodingError),
    /// A referenced object has a different canonical kind than expected.
    #[error("expected object kind {expected}, found {actual}")]
    UnexpectedObjectKind {
        /// The canonical kind the repository structure required.
        expected: ObjectKind,
        /// The canonical kind the stored object actually has.
        actual: ObjectKind,
    },
    /// The ElementId is not present in the accepted SemanticState.
    #[error("element {0} not found in the accepted state")]
    ElementNotFound(ElementId),
}

/// The currently accepted version of one element.
#[derive(Debug)]
pub struct ElementView {
    /// Stable identity of the element (the queried ElementId).
    pub element_id: ElementId,
    /// ObjectId of the currently accepted KnowledgeElementVersion.
    pub version_id: ObjectId,
    /// The decoded, kind-verified KnowledgeElementVersion.
    pub element: KnowledgeElementVersion,
}

/// Resolves the currently accepted version of `element_id` and decodes it.
///
/// ```text
/// refs/accepted (current)
///     ↓
/// SemanticState
///     ↓ binary search
/// ElementStateEntry { element_id, version }
///     ↓ ObjectStore::get + decode_canonical
/// KnowledgeElementVersion (kind-checked)
/// ```
///
/// The accepted ref is read at query time, so the view reflects the current
/// head (a handle that published a change sees the new element without
/// reopening). Read-only: objects and refs are never mutated.
pub fn show_element(
    repository: &Repository,
    element_id: ElementId,
) -> Result<ElementView, QueryError> {
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

    // The state's element entries are canonically sorted by ElementId, so a
    // binary search is the deterministic lookup.
    let entry = match state
        .elements
        .binary_search_by(|e| e.element_id.cmp(&element_id))
    {
        Ok(index) => &state.elements[index],
        Err(_) => return Err(QueryError::ElementNotFound(element_id)),
    };

    let element = match load_typed(
        repository.object_store(),
        entry.version,
        ObjectKind::KnowledgeElementVersion,
    )?
    .payload
    {
        CanonicalPayload::KnowledgeElementVersion(element) => element,
        _ => unreachable!("kind verified by load_typed"),
    };

    Ok(ElementView {
        element_id,
        version_id: entry.version,
        element,
    })
}

/// Loads `id` from the store (hash verified by `ObjectStore::get`), decodes
/// it canonically, and requires exactly `expected` kind.
fn load_typed(
    store: &ObjectStore,
    id: ObjectId,
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
