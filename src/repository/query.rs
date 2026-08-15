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

use std::collections::{HashMap, HashSet};

use crate::domain::change::ChangeRevision;
use crate::domain::element::{KnowledgeElementVersion, Lifecycle};
use crate::domain::identity::{ElementId, ObjectId, RelationshipId, RepositoryId, SoftwareId};
use crate::domain::operation::Operation;
use crate::domain::property::PropertyValue;
use crate::encoding::decode::DecodingError;
use crate::encoding::decode_canonical;
use crate::encoding::object::{CanonicalObject, CanonicalPayload, ObjectKind};
use crate::repository::object_store::{ObjectStore, ObjectStoreError};
use crate::repository::open::Repository;
use crate::repository::ref_store::{RefStore, RefStoreError};
use crate::repository::validation::repository::validate_repository;

/// Direction traversed when following a relationship in an origin trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TraversalDirection {
    /// Traversed from relationship source to relationship target.
    Forward,
    /// Traversed from relationship target to relationship source.
    Backward,
}

/// A single step in a trace origin path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceStep {
    /// Element identity where the step originates.
    pub from_element_id: ElementId,
    /// Stable relationship identity traversed.
    pub relationship_id: RelationshipId,
    /// Ontology type identifier of the relationship.
    pub relationship_type_id: String,
    /// Direction the relationship was traversed relative to its canonical definition.
    pub direction: TraversalDirection,
    /// Element identity reached by this step.
    pub to_element_id: ElementId,
}

/// A single sequence of trace steps connecting a root element to an origin root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TracePath {
    /// Ordered steps from root toward origin.
    pub steps: Vec<TraceStep>,
}

/// Result of tracing a knowledge element back to its authoritative origins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceResult {
    /// The root ElementId queried.
    pub root_element_id: ElementId,
    /// Discovered origin paths. Empty if root element is itself an authoritative origin.
    pub paths: Vec<TracePath>,
}

/// Classifies a relationship type for origin tracing.
///
/// Returns `Some(direction)` indicating which direction to traverse the edge to move
/// toward origin, or `None` if the relationship type does not participate in origin tracing.
pub fn origin_traversal_direction(relationship_type_id: &str) -> Option<TraversalDirection> {
    let short_or_qualified = relationship_type_id
        .rsplit('/')
        .next()
        .unwrap_or(relationship_type_id);
    match short_or_qualified {
        "motivates" => Some(TraversalDirection::Backward),
        "derived-from" | "derived_from" => Some(TraversalDirection::Forward),
        "realizes" => Some(TraversalDirection::Forward),
        "represents" => Some(TraversalDirection::Forward),
        "validates" => Some(TraversalDirection::Forward),
        "restricts" => Some(TraversalDirection::Backward),
        "addresses" => Some(TraversalDirection::Forward),
        "supersedes" => Some(TraversalDirection::Forward),
        "guides" => Some(TraversalDirection::Backward),
        "depends-on" | "depends_on" => None,
        _ => None,
    }
}

/// A single step in an impact propagation path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactStep {
    /// Element identity where the impact propagates from.
    pub from_element_id: ElementId,
    /// Stable relationship identity traversed.
    pub relationship_id: RelationshipId,
    /// Ontology type identifier of the relationship.
    pub relationship_type_id: String,
    /// Direction the relationship was traversed relative to its canonical definition.
    pub direction: TraversalDirection,
    /// Element identity reached by this step.
    pub to_element_id: ElementId,
}

/// A sequence of steps explaining why an element is impacted by a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactPath {
    /// Ordered steps from change root toward impacted target.
    pub steps: Vec<ImpactStep>,
}

/// An element impacted by a change, with its type, lifecycle, and rationale paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactedElement {
    /// Identity of the impacted element.
    pub element_id: ElementId,
    /// Ontology type identifier of the element.
    pub type_id: String,
    /// Current lifecycle state.
    pub lifecycle: crate::domain::element::Lifecycle,
    /// Paths explaining how impact propagated to this element.
    pub paths: Vec<ImpactPath>,
}

/// Categorized result of analyzing potential change consequences from a root element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactResult {
    /// Directly changed element ID(s).
    pub directly_changed: Vec<ElementId>,
    /// Semantically affected active knowledge elements.
    pub semantically_affected: Vec<ImpactedElement>,
    /// Artifact elements affected through traceability.
    pub affected_artifacts: Vec<ImpactedElement>,
}

/// Classifies a relationship type for impact propagation.
///
/// Returns `Some(direction)` indicating which direction impact propagates through the edge
/// when a source or target changes, or `None` if the relationship is excluded from impact propagation.
pub fn impact_propagation_direction(relationship_type_id: &str) -> Option<TraversalDirection> {
    let short_or_qualified = relationship_type_id
        .rsplit('/')
        .next()
        .unwrap_or(relationship_type_id);
    match short_or_qualified {
        "motivates" => Some(TraversalDirection::Forward),
        "derived-from" | "derived_from" => Some(TraversalDirection::Backward),
        "realizes" => Some(TraversalDirection::Backward),
        "represents" => Some(TraversalDirection::Backward),
        "validates" => Some(TraversalDirection::Backward),
        "restricts" => Some(TraversalDirection::Forward),
        "addresses" => Some(TraversalDirection::Backward),
        "guides" => Some(TraversalDirection::Forward),
        "depends-on" | "depends_on" => Some(TraversalDirection::Backward),
        "supersedes" => None,
        _ => None,
    }
}

/// Status of an active Artifact element's alignment with upstream authoritative knowledge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactAccountabilityStatus {
    /// All direct accountability baselines match current active upstream versions.
    Current,
    /// At least one direct baseline differs from the current active upstream version.
    Stale,
    /// The active Artifact has no direct accountability relationships (represents / derived-from).
    Unaccounted,
}

/// Alignment baseline of an Artifact with one direct upstream authoritative element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactBaseline {
    /// Stable identity of the accountability relationship (represents / derived-from).
    pub relationship_id: RelationshipId,
    /// Relationship type ID.
    pub relationship_type: String,
    /// Upstream element ID.
    pub upstream_element_id: ElementId,
    /// Upstream element type ID.
    pub upstream_type_id: String,
    /// Upstream version object ID when the accountability relationship was introduced.
    pub baseline_version: ObjectId,
    /// Current active version object ID of the upstream element.
    pub current_version: ObjectId,
    /// True if `current_version != baseline_version`.
    pub is_stale: bool,
}

/// Accountability record for one active Artifact element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactAccountability {
    /// Identity of the Artifact element.
    pub artifact_element_id: ElementId,
    /// Type ID of the artifact element (`kat.core/artifact`).
    pub artifact_type_id: String,
    /// Title property if present.
    pub title: Option<String>,
    /// Accountability status.
    pub status: ArtifactAccountabilityStatus,
    /// Detailed baseline records for direct accountability relationships.
    pub baselines: Vec<ArtifactBaseline>,
}

/// Comprehensive report produced by `analyze_artifact_accountability`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactAccountabilityReport {
    /// Records for all active Artifact elements in the accepted state.
    pub artifacts: Vec<ArtifactAccountability>,
}

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

/// Detailed view of a single relationship attached to an element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipView {
    /// Canonical relationship identity.
    pub relationship_id: RelationshipId,
    /// Fully qualified relationship type ID (e.g. `kat.core/addresses`).
    pub relationship_type_id: String,
    /// Source element ID of the relationship link.
    pub source_element_id: ElementId,
    /// Target element ID of the relationship link.
    pub target_element_id: ElementId,
    /// ID of the other endpoint connected to the queried element.
    pub other_element_id: ElementId,
    /// Title of the other endpoint element (if present in properties).
    pub other_title: Option<String>,
}

/// 1-hop relationship neighborhood surrounding an element in the current accepted state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RelationshipNeighborhood {
    /// Incoming relationships (where target_element_id == queried element).
    pub incoming: Vec<RelationshipView>,
    /// Outgoing relationships (where source_element_id == queried element).
    pub outgoing: Vec<RelationshipView>,
}

/// The currently accepted version of one element, including its local relationship neighborhood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementView {
    /// Stable identity of the element (the queried ElementId).
    pub element_id: ElementId,
    /// ObjectId of the currently accepted KnowledgeElementVersion.
    pub version_id: ObjectId,
    /// The decoded, kind-verified KnowledgeElementVersion.
    pub element: KnowledgeElementVersion,
    /// 1-hop incoming and outgoing relationships present in the accepted state.
    pub relationships: RelationshipNeighborhood,
}

fn fetch_element_title(
    store: &ObjectStore,
    state: &crate::domain::state::SemanticState,
    element_id: ElementId,
) -> Option<String> {
    let index = state
        .elements
        .binary_search_by(|e| e.element_id.cmp(&element_id))
        .ok()?;
    let entry = &state.elements[index];
    let version = match load_typed(store, entry.version, ObjectKind::KnowledgeElementVersion)
        .ok()?
        .payload
    {
        CanonicalPayload::KnowledgeElementVersion(ev) => ev,
        _ => return None,
    };
    version
        .properties
        .iter()
        .find(|(k, _)| k == "title")
        .and_then(|(_, v)| match v {
            PropertyValue::Text(t) => Some(t.clone()),
            _ => None,
        })
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

    let mut incoming = Vec::new();
    let mut outgoing = Vec::new();

    for rel_entry in &state.relationships {
        let rel_ver = match load_typed(
            repository.object_store(),
            rel_entry.version,
            ObjectKind::RelationshipVersion,
        ) {
            Ok(obj) => match obj.payload {
                CanonicalPayload::RelationshipVersion(v) => v,
                _ => continue,
            },
            Err(_) => continue,
        };

        if rel_ver.target_element_id == element_id {
            let other_title =
                fetch_element_title(repository.object_store(), &state, rel_ver.source_element_id);
            incoming.push(RelationshipView {
                relationship_id: rel_ver.relationship_id,
                relationship_type_id: rel_ver.relationship_type.clone(),
                source_element_id: rel_ver.source_element_id,
                target_element_id: rel_ver.target_element_id,
                other_element_id: rel_ver.source_element_id,
                other_title,
            });
        }
        if rel_ver.source_element_id == element_id {
            let other_title =
                fetch_element_title(repository.object_store(), &state, rel_ver.target_element_id);
            outgoing.push(RelationshipView {
                relationship_id: rel_ver.relationship_id,
                relationship_type_id: rel_ver.relationship_type.clone(),
                source_element_id: rel_ver.source_element_id,
                target_element_id: rel_ver.target_element_id,
                other_element_id: rel_ver.target_element_id,
                other_title,
            });
        }
    }

    incoming.sort_by_key(|a| a.relationship_id);
    outgoing.sort_by_key(|a| a.relationship_id);

    Ok(ElementView {
        element_id,
        version_id: entry.version,
        element,
        relationships: RelationshipNeighborhood { incoming, outgoing },
    })
}

/// Criteria for filtering knowledge elements in [`list_elements`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListFilter {
    /// Optional filter by type ID (e.g. `kat.core/requirement`).
    pub type_id: Option<String>,
    /// Optional filter by element lifecycle state.
    pub lifecycle: Option<crate::domain::element::Lifecycle>,
}

/// Enumerates knowledge elements present in the current accepted `SemanticState`,
/// returning `ElementView`s ordered deterministically by `ElementId`.
///
/// Filters compose (`type_id` AND `lifecycle`). If both are `None`, all active,
/// deprecated, and superseded elements are returned.
///
/// Read-only: objects and refs are never mutated.
pub fn list_elements(
    repository: &Repository,
    filter: ListFilter,
) -> Result<Vec<ElementView>, QueryError> {
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

    let mut views = Vec::new();
    for entry in &state.elements {
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

        if filter
            .type_id
            .as_ref()
            .is_some_and(|t| element.type_id != *t)
        {
            continue;
        }

        if filter.lifecycle.is_some_and(|l| element.lifecycle != l) {
            continue;
        }

        views.push(ElementView {
            element_id: entry.element_id,
            version_id: entry.version,
            element,
            relationships: RelationshipNeighborhood::default(),
        });
    }

    Ok(views)
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

/// Determines if a [`HistoryEntry`] touches a specific element ID in any of its contained operations.
pub fn history_entry_touches_element(
    repository: &Repository,
    entry: &HistoryEntry,
    target_element_id: ElementId,
) -> Result<bool, QueryError> {
    let store = repository.object_store();
    for op in &entry.change.operations {
        let matches = match op {
            Operation::CreateElement { new_version } => {
                matches!(
                    load_typed(store, *new_version, ObjectKind::KnowledgeElementVersion).map(|o| o.payload),
                    Ok(CanonicalPayload::KnowledgeElementVersion(ev)) if ev.element_id == target_element_id
                )
            }
            Operation::UpdateElement { element_id, .. }
            | Operation::DeprecateElement { element_id, .. } => *element_id == target_element_id,
            Operation::Supersede {
                existing_element,
                replacement_element,
                ..
            } => {
                *existing_element == target_element_id || *replacement_element == target_element_id
            }
            Operation::Link {
                new_relationship_version,
            } => {
                matches!(
                    load_typed(store, *new_relationship_version, ObjectKind::RelationshipVersion).map(|o| o.payload),
                    Ok(CanonicalPayload::RelationshipVersion(rv)) if rv.source_element_id == target_element_id || rv.target_element_id == target_element_id
                )
            }
            Operation::Unlink {
                expected_version, ..
            } => {
                matches!(
                    load_typed(store, *expected_version, ObjectKind::RelationshipVersion).map(|o| o.payload),
                    Ok(CanonicalPayload::RelationshipVersion(rv)) if rv.source_element_id == target_element_id || rv.target_element_id == target_element_id
                )
            }
            Operation::AccountArtifact {
                artifact_id,
                reconciliations,
            } => {
                *artifact_id == target_element_id
                    || reconciliations
                        .iter()
                        .any(|r| r.target_element_id == target_element_id)
            }
        };
        if matches {
            return Ok(true);
        }
    }
    Ok(false)
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

/// Traces an element back to its authoritative origins in the current accepted semantic state.
///
/// ```text
/// refs/accepted (current)
///     ↓
/// SemanticState
///     ↓ binary search root_element_id
/// KnowledgeElementVersion
///     ↓ deterministic path exploration over accepted relationships
/// TraceResult { root_element_id, paths }
/// ```
///
/// Traversal is pure read-only over the accepted semantic state. It follows origin-classified
/// relationship types in their respective origin traversal directions. Edges are evaluated
/// in canonical accepted-state relationship order (`RelationshipId` order). Cycle detection
/// tracks visited relationship IDs per path branch to guarantee finite, deterministic exploration.
pub fn trace_origin(
    repository: &Repository,
    root_element_id: ElementId,
) -> Result<TraceResult, QueryError> {
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

    // Verify root_element_id presence in accepted state
    if !state
        .elements
        .iter()
        .any(|e| e.element_id == root_element_id)
    {
        return Err(QueryError::ElementNotFound(root_element_id));
    }

    // Load relationship versions for all relationships in accepted state
    let mut loaded_rel_versions = HashMap::new();
    for entry in &state.relationships {
        let rel = match load_typed(
            repository.object_store(),
            entry.version,
            ObjectKind::RelationshipVersion,
        )?
        .payload
        {
            CanonicalPayload::RelationshipVersion(rel) => rel,
            _ => unreachable!("kind verified by load_typed"),
        };
        loaded_rel_versions.insert(entry.relationship_id, rel);
    }

    let mut paths = Vec::new();
    let mut current_path = Vec::new();
    let mut visited_rels = HashSet::new();

    explore_origin_paths(
        root_element_id,
        &state.relationships,
        &loaded_rel_versions,
        &mut current_path,
        &mut visited_rels,
        &mut paths,
    );

    Ok(TraceResult {
        root_element_id,
        paths,
    })
}

fn explore_origin_paths(
    current_element_id: ElementId,
    state_relationships: &[crate::domain::state::RelationshipStateEntry],
    loaded_rel_versions: &HashMap<RelationshipId, crate::domain::relationship::RelationshipVersion>,
    current_path: &mut Vec<TraceStep>,
    visited_rels: &mut HashSet<RelationshipId>,
    paths: &mut Vec<TracePath>,
) {
    let mut expanded_any = false;

    // state_relationships is canonically sorted by RelationshipId.
    // Iterating over state_relationships preserves canonical relationship order.
    for entry in state_relationships {
        if visited_rels.contains(&entry.relationship_id) {
            continue;
        }

        let Some(rel_v) = loaded_rel_versions.get(&entry.relationship_id) else {
            continue;
        };

        let Some(direction) = origin_traversal_direction(&rel_v.relationship_type) else {
            continue;
        };

        let next_element_id = match direction {
            TraversalDirection::Forward => {
                if rel_v.source_element_id == current_element_id {
                    Some(rel_v.target_element_id)
                } else {
                    None
                }
            }
            TraversalDirection::Backward => {
                if rel_v.target_element_id == current_element_id {
                    Some(rel_v.source_element_id)
                } else {
                    None
                }
            }
        };

        if let Some(next_id) = next_element_id {
            expanded_any = true;

            let step = TraceStep {
                from_element_id: current_element_id,
                relationship_id: entry.relationship_id,
                relationship_type_id: rel_v.relationship_type.clone(),
                direction,
                to_element_id: next_id,
            };

            visited_rels.insert(entry.relationship_id);
            current_path.push(step);

            explore_origin_paths(
                next_id,
                state_relationships,
                loaded_rel_versions,
                current_path,
                visited_rels,
                paths,
            );

            current_path.pop();
            visited_rels.remove(&entry.relationship_id);
        }
    }

    if !expanded_any && !current_path.is_empty() {
        paths.push(TracePath {
            steps: current_path.clone(),
        });
    }
}

/// Analyzes the potential impact of a change to `root_element_id` in the current accepted state.
///
/// ```text
/// refs/accepted (current)
///     ↓
/// SemanticState
///     ↓ binary search root_element_id
/// KnowledgeElementVersion
///     ↓ deterministic path exploration over accepted relationships (impact rules)
/// ImpactResult { directly_changed, semantically_affected, affected_artifacts }
/// ```
///
/// Traversal is pure read-only over the accepted semantic state. It follows impact-classified
/// relationship types in their respective impact propagation directions. Edges are evaluated
/// in canonical accepted-state relationship order (`RelationshipId` order). Cycle detection
/// tracks visited relationship IDs per path branch to guarantee finite, deterministic exploration.
/// Target elements are filtered to `Lifecycle::Active` and partitioned into `semantically_affected`
/// vs `affected_artifacts`.
pub fn analyze_impact(
    repository: &Repository,
    root_element_id: ElementId,
) -> Result<ImpactResult, QueryError> {
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

    // Verify root_element_id presence in accepted state
    if !state
        .elements
        .iter()
        .any(|e| e.element_id == root_element_id)
    {
        return Err(QueryError::ElementNotFound(root_element_id));
    }

    // Load element versions and relationship versions in accepted state
    let mut loaded_element_versions = HashMap::new();
    for entry in &state.elements {
        let elem = match load_typed(
            repository.object_store(),
            entry.version,
            ObjectKind::KnowledgeElementVersion,
        )?
        .payload
        {
            CanonicalPayload::KnowledgeElementVersion(elem) => elem,
            _ => unreachable!("kind verified by load_typed"),
        };
        loaded_element_versions.insert(entry.element_id, elem);
    }

    let mut loaded_rel_versions = HashMap::new();
    for entry in &state.relationships {
        let rel = match load_typed(
            repository.object_store(),
            entry.version,
            ObjectKind::RelationshipVersion,
        )?
        .payload
        {
            CanonicalPayload::RelationshipVersion(rel) => rel,
            _ => unreachable!("kind verified by load_typed"),
        };
        loaded_rel_versions.insert(entry.relationship_id, rel);
    }

    // Explore impact paths
    let mut raw_impacted_paths: HashMap<ElementId, Vec<ImpactPath>> = HashMap::new();
    let mut current_path = Vec::new();
    let mut visited_rels = HashSet::new();

    explore_impact_paths(
        root_element_id,
        &state.relationships,
        &loaded_rel_versions,
        &mut current_path,
        &mut visited_rels,
        &mut raw_impacted_paths,
    );

    let mut semantically_affected = Vec::new();
    let mut affected_artifacts = Vec::new();

    // Preserve canonical element ordering when emitting impacted elements
    for entry in &state.elements {
        if entry.element_id == root_element_id {
            continue;
        }

        let Some(paths) = raw_impacted_paths.remove(&entry.element_id) else {
            continue;
        };

        let Some(elem_v) = loaded_element_versions.get(&entry.element_id) else {
            continue;
        };

        // Filter propagated target elements to Active lifecycle
        if elem_v.lifecycle != crate::domain::element::Lifecycle::Active {
            continue;
        }

        let impacted = ImpactedElement {
            element_id: entry.element_id,
            type_id: elem_v.type_id.clone(),
            lifecycle: elem_v.lifecycle,
            paths,
        };

        let short_type = elem_v.type_id.rsplit('/').next().unwrap_or(&elem_v.type_id);
        if short_type == "artifact" {
            affected_artifacts.push(impacted);
        } else {
            semantically_affected.push(impacted);
        }
    }

    Ok(ImpactResult {
        directly_changed: vec![root_element_id],
        semantically_affected,
        affected_artifacts,
    })
}

fn explore_impact_paths(
    current_element_id: ElementId,
    state_relationships: &[crate::domain::state::RelationshipStateEntry],
    loaded_rel_versions: &HashMap<RelationshipId, crate::domain::relationship::RelationshipVersion>,
    current_path: &mut Vec<ImpactStep>,
    visited_rels: &mut HashSet<RelationshipId>,
    raw_impacted_paths: &mut HashMap<ElementId, Vec<ImpactPath>>,
) {
    for entry in state_relationships {
        if visited_rels.contains(&entry.relationship_id) {
            continue;
        }

        let Some(rel_v) = loaded_rel_versions.get(&entry.relationship_id) else {
            continue;
        };

        let Some(direction) = impact_propagation_direction(&rel_v.relationship_type) else {
            continue;
        };

        let next_element_id = match direction {
            TraversalDirection::Forward => {
                if rel_v.source_element_id == current_element_id {
                    Some(rel_v.target_element_id)
                } else {
                    None
                }
            }
            TraversalDirection::Backward => {
                if rel_v.target_element_id == current_element_id {
                    Some(rel_v.source_element_id)
                } else {
                    None
                }
            }
        };

        if let Some(next_id) = next_element_id {
            let step = ImpactStep {
                from_element_id: current_element_id,
                relationship_id: entry.relationship_id,
                relationship_type_id: rel_v.relationship_type.clone(),
                direction,
                to_element_id: next_id,
            };

            visited_rels.insert(entry.relationship_id);
            current_path.push(step);

            raw_impacted_paths
                .entry(next_id)
                .or_default()
                .push(ImpactPath {
                    steps: current_path.clone(),
                });

            explore_impact_paths(
                next_id,
                state_relationships,
                loaded_rel_versions,
                current_path,
                visited_rels,
                raw_impacted_paths,
            );

            current_path.pop();
            visited_rels.remove(&entry.relationship_id);
        }
    }
}

/// Traces history backward from accepted head to find the ChangeRevision where `relationship_id`
/// was first introduced, returning the target element's version ObjectId in that change's result state.
pub fn resolve_relationship_baseline_version(
    repository: &Repository,
    relationship_id: RelationshipId,
    target_element_id: ElementId,
) -> Result<ObjectId, QueryError> {
    let entries = history(repository)?;

    // `entries` is ordered from accepted head (newest) to oldest.
    // Walk newest to oldest: check if any revision contains an AccountArtifact operation
    // that explicitly reconciled relationship_id!
    for entry in &entries {
        for op in &entry.change.operations {
            if let Operation::AccountArtifact { reconciliations, .. } = op {
                if let Some(recon) = reconciliations
                    .iter()
                    .find(|r| r.relationship_id == relationship_id)
                {
                    return Ok(recon.reconciled_target_version);
                }
            }
        }
    }

    // Otherwise fallback to finding the introducing state
    let mut introducing_state = None;
    for entry in &entries {
        let state = match load_typed(
            repository.object_store(),
            entry.change.result_state,
            ObjectKind::SemanticState,
        )?
        .payload
        {
            CanonicalPayload::SemanticState(s) => s,
            _ => unreachable!("kind verified by load_typed"),
        };

        if state
            .relationships
            .iter()
            .any(|r| r.relationship_id == relationship_id)
        {
            introducing_state = Some(state);
        } else {
            break;
        }
    }

    let state = introducing_state.ok_or(QueryError::ElementNotFound(target_element_id))?;

    let elem_entry = state
        .elements
        .iter()
        .find(|e| e.element_id == target_element_id)
        .ok_or(QueryError::ElementNotFound(target_element_id))?;

    Ok(elem_entry.version)
}

/// Evaluates artifact accountability across all active `kat.core/artifact` elements
/// in the repository's current accepted state $S_n$.
pub fn analyze_artifact_accountability(
    repository: &Repository,
) -> Result<ArtifactAccountabilityReport, QueryError> {
    let accepted = repository.ref_store().read_accepted()?;
    let state = match load_typed(
        repository.object_store(),
        accepted.state,
        ObjectKind::SemanticState,
    )?
    .payload
    {
        CanonicalPayload::SemanticState(s) => s,
        _ => unreachable!("kind verified by load_typed"),
    };

    let mut artifact_records = Vec::new();

    for elem_entry in &state.elements {
        let el_version = match load_typed(
            repository.object_store(),
            elem_entry.version,
            ObjectKind::KnowledgeElementVersion,
        )?
        .payload
        {
            CanonicalPayload::KnowledgeElementVersion(v) => v,
            _ => unreachable!("kind verified by load_typed"),
        };
        if el_version.lifecycle != Lifecycle::Active {
            continue;
        }

        let short_or_qualified = el_version
            .type_id
            .rsplit('/')
            .next()
            .unwrap_or(&el_version.type_id);
        if short_or_qualified != "artifact" {
            continue;
        }

        let title = el_version
            .properties
            .iter()
            .find(|(k, _)| k == "title")
            .and_then(|(_, v)| match v {
                PropertyValue::Text(t) => Some(t.clone()),
                _ => None,
            });

        let mut baselines = Vec::new();

        for rel_entry in &state.relationships {
            let rel_version = match load_typed(
                repository.object_store(),
                rel_entry.version,
                ObjectKind::RelationshipVersion,
            )?
            .payload
            {
                CanonicalPayload::RelationshipVersion(v) => v,
                _ => unreachable!("kind verified by load_typed"),
            };
            if rel_version.source_element_id != el_version.element_id {
                continue;
            }

            let rel_type_short = rel_version
                .relationship_type
                .rsplit('/')
                .next()
                .unwrap_or(&rel_version.relationship_type);
            if rel_type_short != "represents"
                && rel_type_short != "derived-from"
                && rel_type_short != "derived_from"
            {
                continue;
            }

            let target_view = show_element(repository, rel_version.target_element_id)?;
            let current_version = target_view.version_id;

            let baseline_version = resolve_relationship_baseline_version(
                repository,
                rel_version.relationship_id,
                rel_version.target_element_id,
            )?;

            let is_stale = current_version != baseline_version
                || target_view.element.lifecycle != Lifecycle::Active;

            baselines.push(ArtifactBaseline {
                relationship_id: rel_version.relationship_id,
                relationship_type: rel_version.relationship_type.clone(),
                upstream_element_id: rel_version.target_element_id,
                upstream_type_id: target_view.element.type_id.clone(),
                baseline_version,
                current_version,
                is_stale,
            });
        }

        let status = if baselines.is_empty() {
            ArtifactAccountabilityStatus::Unaccounted
        } else if baselines.iter().any(|b| b.is_stale) {
            ArtifactAccountabilityStatus::Stale
        } else {
            ArtifactAccountabilityStatus::Current
        };

        artifact_records.push(ArtifactAccountability {
            artifact_element_id: el_version.element_id,
            artifact_type_id: el_version.type_id,
            title,
            status,
            baselines,
        });
    }

    Ok(ArtifactAccountabilityReport {
        artifacts: artifact_records,
    })
}

/// Breakdown of element and relationship counts in the accepted state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeCounts {
    /// Total elements in the accepted state across all lifecycles.
    pub total_elements: usize,
    /// Number of active elements.
    pub active_elements: usize,
    /// Number of deprecated elements.
    pub deprecated_elements: usize,
    /// Number of superseded elements.
    pub superseded_elements: usize,
    /// Total relationships in the accepted state.
    pub total_relationships: usize,
}

/// Breakdown of mechanical consistency rule checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsistencyCounts {
    /// Total invariant or ontology rule violations detected.
    pub violations: usize,
    /// Number of unverified natural-language constraints.
    pub unverified_constraints: usize,
}

/// Breakdown of artifact accountability divergence checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountabilityCounts {
    /// Artifacts whose baseline relationships are current.
    pub current: usize,
    /// Artifacts with upstream knowledge version divergence.
    pub stale: usize,
    /// Active artifacts with no explicit accountability evidence.
    pub unaccounted: usize,
}

/// Summary of the latest accepted change revision, if any change has been published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestChangeSummary {
    /// Object ID of the latest change revision.
    pub revision_id: ObjectId,
    /// Primary operation kind (e.g. `create_element`, `update_element`).
    pub operation_kind: String,
    /// Description associated with the change revision.
    pub description: Option<String>,
}

/// Read-only snapshot summary of the repository's current accepted state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryStatus {
    /// Repository identity.
    pub repository_id: RepositoryId,
    /// Software system identity.
    pub software_id: SoftwareId,
    /// Current accepted state Object ID.
    pub state_id: ObjectId,
    /// Current accepted change Object ID (if any).
    pub change_id: Option<ObjectId>,
    /// Base ontology Object ID.
    pub ontology_id: ObjectId,
    /// Summary of the latest change revision (if any).
    pub latest_change: Option<LatestChangeSummary>,
    /// Element and relationship counts.
    pub knowledge: KnowledgeCounts,
    /// Consistency validation counts.
    pub consistency: ConsistencyCounts,
    /// Artifact accountability counts.
    pub accountability: AccountabilityCounts,
}

/// Computes a concise, read-only summary of the repository's current accepted state.
pub fn repository_status(repository: &Repository) -> Result<RepositoryStatus, QueryError> {
    let accepted = &repository.accepted;

    let state_bytes = repository
        .object_store()
        .get(accepted.state)
        .map_err(QueryError::ObjectStore)?;
    let state_obj = decode_canonical(&state_bytes).map_err(QueryError::Decoding)?;
    let state = match state_obj.payload {
        CanonicalPayload::SemanticState(s) => s,
        _ => {
            return Err(QueryError::ObjectStore(ObjectStoreError::NotFound(
                accepted.state,
            )));
        }
    };

    let mut active_elements = 0;
    let mut deprecated_elements = 0;
    let mut superseded_elements = 0;

    for element_entry in &state.elements {
        let view = show_element(repository, element_entry.element_id)?;
        match view.element.lifecycle {
            Lifecycle::Active => active_elements += 1,
            Lifecycle::Deprecated => deprecated_elements += 1,
            Lifecycle::Superseded => superseded_elements += 1,
        }
    }

    let knowledge = KnowledgeCounts {
        total_elements: state.elements.len(),
        active_elements,
        deprecated_elements,
        superseded_elements,
        total_relationships: state.relationships.len(),
    };

    let val_report = validate_repository(repository)?;
    let consistency = ConsistencyCounts {
        violations: val_report.violations.len(),
        unverified_constraints: val_report.unverified_constraints.len(),
    };

    let art_report = analyze_artifact_accountability(repository)?;
    let mut current = 0;
    let mut stale = 0;
    let mut unaccounted = 0;

    for artifact in &art_report.artifacts {
        match artifact.status {
            ArtifactAccountabilityStatus::Current => current += 1,
            ArtifactAccountabilityStatus::Stale => stale += 1,
            ArtifactAccountabilityStatus::Unaccounted => unaccounted += 1,
        }
    }

    let accountability = AccountabilityCounts {
        current,
        stale,
        unaccounted,
    };

    let latest_change = if accepted.change.is_some() {
        let entries = history(repository)?;
        entries.first().map(|head| {
            let primary_op_kind = head
                .change
                .operations
                .first()
                .map(|op| match op {
                    crate::domain::operation::Operation::CreateElement { .. } => "create element",
                    crate::domain::operation::Operation::UpdateElement { .. } => "update element",
                    crate::domain::operation::Operation::DeprecateElement { .. } => {
                        "deprecate element"
                    }
                    crate::domain::operation::Operation::Supersede { .. } => "supersede element",
                    crate::domain::operation::Operation::Link { .. } => "link",
                    crate::domain::operation::Operation::Unlink { .. } => "unlink",
                    crate::domain::operation::Operation::AccountArtifact { .. } => {
                        "account artifact"
                    }
                })
                .unwrap_or("change")
                .to_string();

            LatestChangeSummary {
                revision_id: head.revision_id,
                operation_kind: primary_op_kind,
                description: head.change.description.clone(),
            }
        })
    } else {
        None
    };

    Ok(RepositoryStatus {
        repository_id: repository.metadata.repository_id,
        software_id: repository.metadata.software_id,
        state_id: accepted.state,
        change_id: accepted.change,
        ontology_id: state.ontology_version,
        latest_change,
        knowledge,
        consistency,
        accountability,
    })
}
