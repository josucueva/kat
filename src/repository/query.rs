//! Read-side queries over the accepted repository head (steps 1.8-1.9).
//!
//! Queries are strictly read-only: they never mutate the object store or
//! `refs/accepted`. Both queries resolve the **current** accepted ref at query
//! time (a point-in-time read, so a handle that just published a change sees
//! the new head without reopening):
//!
//! * [`show_element`] resolves an ElementId to its current version and decodes
//!   the [`KnowledgeElementVersion`].
//! * [`history`] reconstructs the accepted Change history by following
//!   ChangeRevision dependencies from the accepted head.
//!
//! This is the read counterpart of `change.rs`: the engine mutates through
//! prepared/persisted/published typestates; queries only observe.

use std::collections::HashMap;

use crate::domain::change::ChangeRevision;
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
    /// The accepted ChangeRevision's result state does not match the accepted
    /// SemanticState read from the live ref (the head is internally
    /// inconsistent; the open-time snapshot may be stale).
    #[error(
        "accepted change {change} results in state {actual}, but the accepted state is {expected}"
    )]
    AcceptedChangeStateMismatch {
        /// ObjectId of the accepted ChangeRevision.
        change: ObjectId,
        /// ObjectId of the accepted SemanticState.
        expected: ObjectId,
        /// ObjectId the ChangeRevision actually results in.
        actual: ObjectId,
    },
    /// The ChangeRevision dependency graph contains a cycle. Content-addressed
    /// storage makes a genuine cycle unconstructible through the normal store
    /// (each dependency ObjectId is the hash of its target's content), so this
    /// is defense-in-depth against a non-conforming store implementation: a
    /// revision reached while still on the traversal stack is rejected rather
    /// than traversed forever.
    #[error("history contains a dependency cycle at revision {0}")]
    HistoryCycle(ObjectId),
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

/// Loads `id` from the store and requires it to be a ChangeRevision.
fn load_change(store: &ObjectStore, id: ObjectId) -> Result<ChangeRevision, QueryError> {
    match load_typed(store, id, ObjectKind::ChangeRevision)?.payload {
        CanonicalPayload::ChangeRevision(change) => Ok(change),
        _ => unreachable!("kind verified by load_typed"),
    }
}

/// One ChangeRevision in the accepted history, with its content identity.
#[derive(Debug)]
pub struct HistoryEntry {
    /// ObjectId of the ChangeRevision.
    pub revision_id: ObjectId,
    /// The decoded, kind-verified ChangeRevision.
    pub change: ChangeRevision,
}

/// Reconstructs the accepted Change history by following ChangeRevision
/// ancestry from the current accepted head, **newest first** (`cli.md` does
/// not specify an order; the accepted head is the natural entry point, so
/// newest-first makes the traversal direct).
///
/// ```text
/// refs/accepted (current)
///     ↓ accepted.change
/// ChangeRevision head
///     ↓ dependencies (canonical stored order)
/// ancestors…
/// ```
///
/// History is inferred from the dependency graph alone — never from object
/// timestamps or filesystem order. Traversal is deterministic: head first,
/// then each revision's dependencies depth-first in their canonical stored
/// order, skipping already-visited revisions (shared ancestry appears once).
/// A revision still on the current traversal stack is a cycle and is rejected
/// with [`QueryError::HistoryCycle`] instead of looping forever.
///
/// Integrity per revision: the object must exist, decode canonically, and be
/// a ChangeRevision. The accepted head must also satisfy
/// `change.result_state == accepted.state`, re-verified against the live ref
/// (the open-time snapshot may be stale). No semantic merge/history
/// validation beyond that. Read-only: objects and refs are never mutated.
pub fn history(repository: &Repository) -> Result<Vec<HistoryEntry>, QueryError> {
    let accepted = repository.ref_store().read_accepted()?;
    let Some(head) = accepted.change else {
        return Ok(Vec::new());
    };

    // The accepted head is validated against the live ref: the open-time
    // snapshot may be stale, so the result-state relationship is re-verified
    // here rather than trusting `open_repository`'s earlier check.
    let head_change = load_change(repository.object_store(), head)?;
    if head_change.result_state != accepted.state {
        return Err(QueryError::AcceptedChangeStateMismatch {
            change: head,
            expected: accepted.state,
            actual: head_change.result_state,
        });
    }

    let mut states = HashMap::new();
    let mut entries = Vec::new();
    visit(repository, head, &mut states, &mut entries)?;
    Ok(entries)
}

/// Depth-first traversal state of one ChangeRevision ObjectId.
#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    /// Not yet reached by the traversal.
    Unseen,
    /// Currently on the depth-first stack; an edge back to it is a cycle.
    Visiting,
    /// Fully visited; its ancestry was emitted and it can be skipped.
    Visited,
}

/// Depth-first walk emitting `revision_id` **before** its dependencies (so the
/// accepted head is first), recursing into dependencies in their canonical
/// stored order.
fn visit(
    repository: &Repository,
    revision_id: ObjectId,
    states: &mut HashMap<ObjectId, VisitState>,
    entries: &mut Vec<HistoryEntry>,
) -> Result<(), QueryError> {
    match states
        .get(&revision_id)
        .copied()
        .unwrap_or(VisitState::Unseen)
    {
        // Shared ancestry: already emitted through another path.
        VisitState::Visited => return Ok(()),
        // Still on the traversal stack: malformed cycle, reject rather than
        // traversing forever.
        VisitState::Visiting => return Err(QueryError::HistoryCycle(revision_id)),
        VisitState::Unseen => {}
    }
    states.insert(revision_id, VisitState::Visiting);

    let change = load_change(repository.object_store(), revision_id)?;
    // Grab the dependency list before moving `change` into the entry; the
    // entry is still emitted first (pre-order: head first, newest first).
    let dependencies = change.dependencies.clone();
    entries.push(HistoryEntry {
        revision_id,
        change,
    });

    for dependency in &dependencies {
        visit(repository, *dependency, states, entries)?;
    }

    states.insert(revision_id, VisitState::Visited);
    Ok(())
}
