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
use crate::domain::identity::{
    ElementId, ObjectId, OntologyId, RelationshipId, RepositoryId, SoftwareId,
};
use crate::domain::ontology::OntologyVersion;
use crate::domain::operation::Operation;
use crate::domain::property::PropertyValue;
use crate::domain::relationship::RelationshipVersion;
use crate::encoding::decode::DecodingError;
use crate::encoding::decode_canonical;
use crate::encoding::hash::canonical_object_id;
use crate::encoding::object::{CanonicalObject, CanonicalPayload, ObjectKind};
use crate::repository::object_store::{ObjectStore, ObjectStoreError};
use crate::repository::open::Repository;
use crate::repository::ref_store::{RefStore, RefStoreError};
use crate::repository::session::{DraftSessionError, read_draft_session};
use crate::repository::validation::repository::{
    ValidationReport, validate_repository, validate_repository_state,
};

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

/// Node in a deduplicated hierarchical tree view of an origin trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceTreeNode {
    /// Element identity of this node.
    pub element_id: ElementId,
    /// Child edges connected to this node.
    pub children: Vec<TraceTreeEdge>,
}

/// Directed relationship edge connecting a parent node to a child node in a trace tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceTreeEdge {
    /// Canonical relationship identity.
    pub relationship_id: RelationshipId,
    /// Relationship type ID (e.g. `kat.core/realizes`).
    pub relationship_type_id: String,
    /// Traversal direction relative to canonical relationship definition.
    pub direction: TraversalDirection,
    /// Child node connected via this edge.
    pub target: TraceTreeNode,
}

impl TraceResult {
    /// Converts discrete origin trace paths into a deduplicated tree hierarchy.
    pub fn to_tree(&self) -> TraceTreeNode {
        let mut root = TraceTreeNode {
            element_id: self.root_element_id,
            children: Vec::new(),
        };

        for path in &self.paths {
            let mut current = &mut root;
            for step in &path.steps {
                let pos = current.children.iter().position(|child| {
                    child.relationship_id == step.relationship_id
                        && child.target.element_id == step.to_element_id
                });

                let index = match pos {
                    Some(idx) => idx,
                    None => {
                        current.children.push(TraceTreeEdge {
                            relationship_id: step.relationship_id,
                            relationship_type_id: step.relationship_type_id.clone(),
                            direction: step.direction,
                            target: TraceTreeNode {
                                element_id: step.to_element_id,
                                children: Vec::new(),
                            },
                        });
                        current.children.len() - 1
                    }
                };

                current = &mut current.children[index].target;
            }
        }

        root
    }
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

/// Repository-wide summary totals for artifact accountability.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArtifactAccountabilitySummary {
    pub total: usize,
    pub current: usize,
    pub stale: usize,
    pub unaccounted: usize,
}

/// Comprehensive report produced by `analyze_artifact_accountability`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactAccountabilityReport {
    /// Records for active Artifact elements matching query selection.
    pub artifacts: Vec<ArtifactAccountability>,
    /// Repository-wide totals (unaffected by filtering).
    pub repository_summary: ArtifactAccountabilitySummary,
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
    /// The specified type ID or query short name is not registered in the active ontology.
    #[error("unknown ontology type '{0}'")]
    UnknownOntologyType(String),
    /// The short type identifier query matches multiple registered types in the active ontology.
    #[error("ontology type query '{query}' is ambiguous (matches: {matches:?})")]
    AmbiguousOntologyType {
        /// The query string provided by the user.
        query: String,
        /// The matching canonical type IDs.
        matches: Vec<String>,
    },
    /// The specified max_depth parameter is invalid (must be >= 1).
    #[error("max depth must be greater than 0, got {0}")]
    InvalidMaxDepth(usize),
}

/// Detailed view of a single relationship attached to an element.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct RelationshipNeighborhood {
    /// Incoming relationships (where target_element_id == queried element).
    pub incoming: Vec<RelationshipView>,
    /// Outgoing relationships (where source_element_id == queried element).
    pub outgoing: Vec<RelationshipView>,
}

/// The currently accepted version of one element, including its local relationship neighborhood.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    max_depth: Option<usize>,
) -> Result<TraceResult, QueryError> {
    if let Some(0) = max_depth {
        return Err(QueryError::InvalidMaxDepth(0));
    }

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
        max_depth,
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
    max_depth: Option<usize>,
) {
    let mut expanded_any = false;

    if max_depth.is_none_or(|limit| current_path.len() < limit) {
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
                    max_depth,
                );

                current_path.pop();
                visited_rels.remove(&entry.relationship_id);
            }
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
    max_depth: Option<usize>,
) -> Result<ImpactResult, QueryError> {
    if let Some(0) = max_depth {
        return Err(QueryError::InvalidMaxDepth(0));
    }

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
        max_depth,
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
    max_depth: Option<usize>,
) {
    if max_depth.is_none_or(|limit| current_path.len() < limit) {
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
                    max_depth,
                );

                current_path.pop();
                visited_rels.remove(&entry.relationship_id);
            }
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
            if let Operation::AccountArtifact {
                reconciliations, ..
            } = op
            {
                for recon in reconciliations {
                    if recon.relationship_id == relationship_id {
                        return Ok(recon.reconciled_target_version);
                    }
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

    let mut total_current = 0;
    let mut total_stale = 0;
    let mut total_unaccounted = 0;

    for a in &artifact_records {
        match a.status {
            ArtifactAccountabilityStatus::Current => total_current += 1,
            ArtifactAccountabilityStatus::Stale => total_stale += 1,
            ArtifactAccountabilityStatus::Unaccounted => total_unaccounted += 1,
        }
    }

    let repository_summary = ArtifactAccountabilitySummary {
        total: artifact_records.len(),
        current: total_current,
        stale: total_stale,
        unaccounted: total_unaccounted,
    };

    Ok(ArtifactAccountabilityReport {
        artifacts: artifact_records,
        repository_summary,
    })
}

/// Filtering criteria for artifact accountability evaluation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArtifactFilter {
    /// If true, returns only artifacts whose accountability status is `Stale`.
    pub stale_only: bool,
    /// If provided, filters accountability analysis to the artifact matching this ElementId.
    pub target_artifact_id: Option<ElementId>,
}

/// Evaluates artifact accountability with filtering options (`stale_only`, `target_artifact_id`).
pub fn analyze_artifact_accountability_filtered(
    repository: &Repository,
    filter: ArtifactFilter,
) -> Result<ArtifactAccountabilityReport, QueryError> {
    let full_report = analyze_artifact_accountability(repository)?;
    let repository_summary = full_report.repository_summary;

    let filtered_artifacts = full_report
        .artifacts
        .into_iter()
        .filter(|rec| {
            if filter.stale_only && rec.status != ArtifactAccountabilityStatus::Stale {
                return false;
            }
            if let Some(target_id) = filter.target_artifact_id
                && rec.artifact_element_id != target_id
            {
                return false;
            }
            true
        })
        .collect();

    Ok(ArtifactAccountabilityReport {
        artifacts: filtered_artifacts,
        repository_summary,
    })
}

/// Breakdown of element and relationship counts in the accepted state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConsistencyCounts {
    /// Total invariant or ontology rule violations detected.
    pub violations: usize,
    /// Number of unverified natural-language constraints.
    pub unverified_constraints: usize,
}

/// Breakdown of artifact accountability divergence checks.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AccountabilityCounts {
    /// Artifacts whose baseline relationships are current.
    pub current: usize,
    /// Artifacts with upstream knowledge version divergence.
    pub stale: usize,
    /// Active artifacts with no explicit accountability evidence.
    pub unaccounted: usize,
}

/// Summary of the latest accepted change revision, if any change has been published.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LatestChangeSummary {
    /// Object ID of the latest change revision.
    pub revision_id: ObjectId,
    /// Primary operation kind (e.g. `create_element`, `update_element`).
    pub operation_kind: String,
    /// Description associated with the change revision.
    pub description: Option<String>,
}

/// Read-only snapshot summary of the repository's current accepted state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

// ---------------------------------------------------------------------------
// Ontology Discovery Query DTOs & Functions (Phase 17, Step 17.1)
// ---------------------------------------------------------------------------

/// Summary view of the active ontology.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntologySummary {
    /// Stable semantic identity of the active ontology.
    pub ontology_id: OntologyId,
    /// Content-addressed Object ID of the active `OntologyVersion`.
    pub ontology_version_id: ObjectId,
    /// Registered element type definitions, sorted by canonical `type_id`.
    pub element_types: Vec<ElementTypeSummary>,
    /// Registered relationship type definitions, sorted by canonical `type_id`.
    pub relationship_types: Vec<RelationshipTypeSummary>,
}

/// Summary of a registered element type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementTypeSummary {
    /// Fully qualified element type ID (e.g. `kat.core/requirement`).
    pub type_id: String,
    /// Human-readable element type name (e.g. `"Requirement"`).
    pub name: String,
}

/// Summary of a registered relationship type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipTypeSummary {
    /// Fully qualified relationship type ID (e.g. `kat.core/motivates`).
    pub type_id: String,
    /// Human-readable relationship type name (e.g. `"Motivates"`).
    pub name: String,
    /// Allowed source element type IDs, sorted alphabetically.
    pub allowed_source_types: Vec<String>,
    /// Allowed target element type IDs, sorted alphabetically.
    pub allowed_target_types: Vec<String>,
}

/// Detailed inspection view of an element or relationship type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OntologyTypeView {
    /// Detailed element type view.
    Element(ElementTypeView),
    /// Detailed relationship type view.
    Relationship(RelationshipTypeView),
}

/// Detailed inspection view of a registered element type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementTypeView {
    /// Fully qualified element type ID (e.g. `kat.core/implementation`).
    pub type_id: String,
    /// Human-readable element type name (e.g. `"Implementation"`).
    pub name: String,
    /// Outgoing relationship capabilities (where this element type is an allowed source).
    pub outgoing: Vec<RelationshipCapability>,
    /// Incoming relationship capabilities (where this element type is an allowed target).
    pub incoming: Vec<RelationshipCapability>,
}

/// Detailed inspection view of a registered relationship type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipTypeView {
    /// Fully qualified relationship type ID (e.g. `kat.core/realizes`).
    pub type_id: String,
    /// Human-readable relationship type name (e.g. `"Realizes"`).
    pub name: String,
    /// Allowed source element type IDs, sorted alphabetically.
    pub allowed_source_types: Vec<String>,
    /// Allowed target element type IDs, sorted alphabetically.
    pub allowed_target_types: Vec<String>,
}

/// A single relationship capability connecting an element type to a counterpart type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipCapability {
    /// Fully qualified relationship type ID (e.g. `kat.core/realizes`).
    pub relationship_type_id: String,
    /// Fully qualified counterpart element type ID (e.g. `kat.core/requirement`).
    pub counterpart_type_id: String,
}

/// Loads the active `OntologyVersion` associated with the repository state context.
pub fn active_ontology(repository: &Repository) -> Result<(ObjectId, OntologyVersion), QueryError> {
    let accepted = repository.ref_store().read_accepted()?;
    let state_bytes = repository.object_store().get(accepted.state)?;
    let state_obj = decode_canonical(&state_bytes)?;
    let state = match state_obj.payload {
        CanonicalPayload::SemanticState(state) => state,
        _ => {
            return Err(QueryError::UnexpectedObjectKind {
                expected: ObjectKind::SemanticState,
                actual: state_obj.object_kind(),
            });
        }
    };

    let ontology_version_id = state.ontology_version;
    let ontology_bytes = repository.object_store().get(ontology_version_id)?;
    let ontology_obj = decode_canonical(&ontology_bytes)?;
    let ontology = match ontology_obj.payload {
        CanonicalPayload::OntologyVersion(ontology) => ontology,
        _ => {
            return Err(QueryError::UnexpectedObjectKind {
                expected: ObjectKind::OntologyVersion,
                actual: ontology_obj.object_kind(),
            });
        }
    };

    Ok((ontology_version_id, ontology))
}

/// Inspects the active `OntologyVersion` associated with the repository state context,
/// producing a summary view of registered element types and relationship types.
pub fn inspect_ontology(repository: &Repository) -> Result<OntologySummary, QueryError> {
    let (ontology_version_id, ontology) = active_ontology(repository)?;

    let mut element_types: Vec<ElementTypeSummary> = ontology
        .element_types
        .iter()
        .map(|def| ElementTypeSummary {
            type_id: def.type_id.clone(),
            name: def.name.clone(),
        })
        .collect();
    element_types.sort_by(|a, b| a.type_id.cmp(&b.type_id));

    let mut relationship_types: Vec<RelationshipTypeSummary> = ontology
        .relationship_types
        .iter()
        .map(|def| {
            let mut allowed_source_types = def.allowed_source_types.clone();
            allowed_source_types.sort();
            let mut allowed_target_types = def.allowed_target_types.clone();
            allowed_target_types.sort();

            RelationshipTypeSummary {
                type_id: def.type_id.clone(),
                name: def.name.clone(),
                allowed_source_types,
                allowed_target_types,
            }
        })
        .collect();
    relationship_types.sort_by(|a, b| a.type_id.cmp(&b.type_id));

    Ok(OntologySummary {
        ontology_id: ontology.ontology_id,
        ontology_version_id,
        element_types,
        relationship_types,
    })
}

/// Resolves `query` against the active `OntologyVersion` and returns a detailed view.
///
/// `query` can be an exact canonical type_id (e.g. `kat.core/requirement`) or a short type
/// identifier (e.g. `requirement`).
pub fn show_ontology_type(
    repository: &Repository,
    query: &str,
) -> Result<OntologyTypeView, QueryError> {
    let (_ontology_version_id, ontology) = active_ontology(repository)?;

    // 1. Exact canonical type_id match.
    let resolved_type_id = if ontology.element_types.iter().any(|e| e.type_id == query)
        || ontology
            .relationship_types
            .iter()
            .any(|r| r.type_id == query)
    {
        query.to_string()
    } else {
        // 2. Short identifier match.
        let matches: Vec<String> = ontology
            .element_types
            .iter()
            .map(|e| e.type_id.as_str())
            .chain(
                ontology
                    .relationship_types
                    .iter()
                    .map(|r| r.type_id.as_str()),
            )
            .filter(|type_id| {
                let short = type_id.rsplit('/').next().unwrap_or(type_id);
                short == query
            })
            .map(String::from)
            .collect();

        match matches.len() {
            1 => matches[0].clone(),
            0 => return Err(QueryError::UnknownOntologyType(query.to_string())),
            _ => {
                let mut sorted_matches = matches;
                sorted_matches.sort();
                return Err(QueryError::AmbiguousOntologyType {
                    query: query.to_string(),
                    matches: sorted_matches,
                });
            }
        }
    };

    // Check if resolved_type_id is an element type.
    if let Some(elem_def) = ontology
        .element_types
        .iter()
        .find(|e| e.type_id == resolved_type_id)
    {
        let mut outgoing = Vec::new();
        let mut incoming = Vec::new();

        for rel_def in &ontology.relationship_types {
            if rel_def.allowed_source_types.contains(&resolved_type_id) {
                for target in &rel_def.allowed_target_types {
                    outgoing.push(RelationshipCapability {
                        relationship_type_id: rel_def.type_id.clone(),
                        counterpart_type_id: target.clone(),
                    });
                }
            }
            if rel_def.allowed_target_types.contains(&resolved_type_id) {
                for source in &rel_def.allowed_source_types {
                    incoming.push(RelationshipCapability {
                        relationship_type_id: rel_def.type_id.clone(),
                        counterpart_type_id: source.clone(),
                    });
                }
            }
        }

        outgoing.sort_by(|a, b| {
            a.relationship_type_id
                .cmp(&b.relationship_type_id)
                .then_with(|| a.counterpart_type_id.cmp(&b.counterpart_type_id))
        });
        incoming.sort_by(|a, b| {
            a.relationship_type_id
                .cmp(&b.relationship_type_id)
                .then_with(|| a.counterpart_type_id.cmp(&b.counterpart_type_id))
        });

        return Ok(OntologyTypeView::Element(ElementTypeView {
            type_id: elem_def.type_id.clone(),
            name: elem_def.name.clone(),
            outgoing,
            incoming,
        }));
    }

    // Check if resolved_type_id is a relationship type.
    if let Some(rel_def) = ontology
        .relationship_types
        .iter()
        .find(|r| r.type_id == resolved_type_id)
    {
        let mut allowed_source_types = rel_def.allowed_source_types.clone();
        allowed_source_types.sort();
        let mut allowed_target_types = rel_def.allowed_target_types.clone();
        allowed_target_types.sort();

        return Ok(OntologyTypeView::Relationship(RelationshipTypeView {
            type_id: rel_def.type_id.clone(),
            name: rel_def.name.clone(),
            allowed_source_types,
            allowed_target_types,
        }));
    }

    Err(QueryError::UnknownOntologyType(query.to_string()))
}

// ---------------------------------------------------------------------------
// Draft Session Inspection (Phase 20)
// ---------------------------------------------------------------------------

/// Detailed overview of an operation staged into an open draft change session.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StagedOperationDetail {
    pub index: usize,
    pub operation_kind: String,
    pub target_id: String,
    pub title: Option<String>,
    pub summary: String,
}

/// Delta metrics for candidate working state compared to base accepted state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CandidateEffectSummary {
    pub total_elements: usize,
    pub total_relationships: usize,
    pub elements_created: usize,
    pub elements_updated: usize,
    pub elements_deprecated: usize,
    pub elements_superseded: usize,
    pub relationships_created: usize,
    pub relationships_unlinked: usize,
}

/// Inspection view of an open draft change session.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DraftSessionView {
    pub base_state_id: ObjectId,
    pub base_change_id: Option<ObjectId>,
    pub created_at: String,
    pub description: Option<String>,
    pub status: String,
    pub staged_operations: Vec<StagedOperationDetail>,
    pub candidate_effect: CandidateEffectSummary,
    pub candidate_validation: ValidationReport,
    pub accountability_total_artifacts: usize,
    pub accountability_stale_artifacts: usize,
    pub accountability_reconciled_in_draft: usize,
}

fn get_element_version_info(
    store: &ObjectStore,
    session: &crate::repository::session::DraftSession,
    version_id: ObjectId,
) -> (Option<String>, String, Option<String>) {
    if let Some(v) = session.staged_element_versions.iter().find(|e| {
        canonical_object_id(&CanonicalObject {
            payload: CanonicalPayload::KnowledgeElementVersion((*e).clone()),
        })
        .map(|id| id == version_id)
        .unwrap_or(false)
    }) {
        let title = v.properties.iter().find_map(|(k, val)| {
            if k == "title" {
                if let PropertyValue::Text(t) = val {
                    Some(t.clone())
                } else {
                    None
                }
            } else {
                None
            }
        });
        return (Some(v.element_id.to_string()), v.type_id.clone(), title);
    }

    if let Ok(obj) = load_typed(store, version_id, ObjectKind::KnowledgeElementVersion)
        && let CanonicalPayload::KnowledgeElementVersion(v) = obj.payload
    {
        let title = v.properties.iter().find_map(|(k, val)| {
            if k == "title" {
                if let PropertyValue::Text(t) = val {
                    Some(t.clone())
                } else {
                    None
                }
            } else {
                None
            }
        });
        return (Some(v.element_id.to_string()), v.type_id, title);
    }

    (None, "unknown".to_string(), None)
}

fn get_relationship_version_info(
    store: &ObjectStore,
    session: &crate::repository::session::DraftSession,
    version_id: ObjectId,
) -> (String, Option<String>, Option<String>) {
    if let Some(v) = session.staged_relationship_versions.iter().find(|r| {
        canonical_object_id(&CanonicalObject {
            payload: CanonicalPayload::RelationshipVersion((*r).clone()),
        })
        .map(|id| id == version_id)
        .unwrap_or(false)
    }) {
        return (
            v.relationship_type.clone(),
            Some(v.source_element_id.to_string()),
            Some(v.target_element_id.to_string()),
        );
    }

    if let Ok(obj) = load_typed(store, version_id, ObjectKind::RelationshipVersion)
        && let CanonicalPayload::RelationshipVersion(v) = obj.payload
    {
        return (
            v.relationship_type,
            Some(v.source_element_id.to_string()),
            Some(v.target_element_id.to_string()),
        );
    }

    ("unknown".to_string(), None, None)
}

/// Inspects an active open draft change session, returning detailed session metadata,
/// staged operation summaries, candidate effect deltas, candidate validation preview,
/// and artifact accountability preview.
pub fn inspect_draft_session(
    repository: &Repository,
) -> Result<Option<DraftSessionView>, QueryError> {
    let session = match read_draft_session(repository.root_dir()) {
        Ok(s) => s,
        Err(DraftSessionError::NotFound) => return Ok(None),
        Err(err) => {
            return Err(QueryError::ObjectStore(ObjectStoreError::Io(
                std::io::Error::other(err.to_string()),
            )));
        }
    };

    let store = repository.object_store();

    let mut staged_operations = Vec::new();
    let mut effect = CandidateEffectSummary {
        total_elements: session.working_state.elements.len(),
        total_relationships: session.working_state.relationships.len(),
        elements_created: 0,
        elements_updated: 0,
        elements_deprecated: 0,
        elements_superseded: 0,
        relationships_created: 0,
        relationships_unlinked: 0,
    };
    let mut reconciled_count = 0;

    for (idx, op) in session.operations.iter().enumerate() {
        let index = idx + 1;
        match op {
            Operation::CreateElement { new_version } => {
                effect.elements_created += 1;
                let (elem_id, type_id, title) =
                    get_element_version_info(store, &session, *new_version);
                let short_type = type_id.rsplit('/').next().unwrap_or(&type_id);
                let title_str = title.as_deref().unwrap_or("-");
                staged_operations.push(StagedOperationDetail {
                    index,
                    operation_kind: "CreateElement".to_string(),
                    target_id: elem_id.unwrap_or_default(),
                    title: title.clone(),
                    summary: format!("[{short_type}] \"{title_str}\""),
                });
            }
            Operation::UpdateElement {
                element_id,
                new_version,
                ..
            } => {
                effect.elements_updated += 1;
                let (_, type_id, title) = get_element_version_info(store, &session, *new_version);
                let short_type = type_id.rsplit('/').next().unwrap_or(&type_id);
                let title_str = title.as_deref().unwrap_or("-");
                staged_operations.push(StagedOperationDetail {
                    index,
                    operation_kind: "UpdateElement".to_string(),
                    target_id: element_id.to_string(),
                    title: title.clone(),
                    summary: format!("[{short_type}] \"{title_str}\""),
                });
            }
            Operation::DeprecateElement {
                element_id,
                new_version,
                ..
            } => {
                effect.elements_deprecated += 1;
                let (_, type_id, title) = get_element_version_info(store, &session, *new_version);
                let short_type = type_id.rsplit('/').next().unwrap_or(&type_id);
                let title_str = title.as_deref().unwrap_or("-");
                staged_operations.push(StagedOperationDetail {
                    index,
                    operation_kind: "DeprecateElement".to_string(),
                    target_id: element_id.to_string(),
                    title: title.clone(),
                    summary: format!("[{short_type}] \"{title_str}\""),
                });
            }
            Operation::Supersede {
                existing_element,
                replacement_element,
                ..
            } => {
                effect.elements_superseded += 1;
                effect.elements_created += 1;
                staged_operations.push(StagedOperationDetail {
                    index,
                    operation_kind: "SupersedeElement".to_string(),
                    target_id: existing_element.to_string(),
                    title: None,
                    summary: format!("supersede {existing_element} -> {replacement_element}"),
                });
            }
            Operation::Link {
                new_relationship_version,
            } => {
                effect.relationships_created += 1;
                let (rel_type, src, tgt) =
                    get_relationship_version_info(store, &session, *new_relationship_version);
                let short_rel = rel_type.rsplit('/').next().unwrap_or(&rel_type);
                staged_operations.push(StagedOperationDetail {
                    index,
                    operation_kind: "LinkKnowledgeElements".to_string(),
                    target_id: src.clone().unwrap_or_default(),
                    title: None,
                    summary: format!(
                        "[{short_rel}] {} -> {}",
                        src.as_deref().unwrap_or("-"),
                        tgt.as_deref().unwrap_or("-")
                    ),
                });
            }
            Operation::Unlink {
                relationship_id, ..
            } => {
                effect.relationships_unlinked += 1;
                staged_operations.push(StagedOperationDetail {
                    index,
                    operation_kind: "UnlinkRelationship".to_string(),
                    target_id: relationship_id.to_string(),
                    title: None,
                    summary: format!("unlink relationship {relationship_id}"),
                });
            }
            Operation::AccountArtifact {
                artifact_id,
                reconciliations,
            } => {
                effect.elements_updated += 1;
                reconciled_count += reconciliations.len();
                staged_operations.push(StagedOperationDetail {
                    index,
                    operation_kind: "AccountArtifact".to_string(),
                    target_id: artifact_id.to_string(),
                    title: None,
                    summary: format!(
                        "[kat.core/artifact] {artifact_id} (reconciled {} edges)",
                        reconciliations.len()
                    ),
                });
            }
        }
    }

    let candidate_validation = validate_repository_state(
        store,
        &session.working_state,
        &session.staged_element_versions,
        &session.staged_relationship_versions,
    )?;

    let acc_report = analyze_candidate_artifact_accountability(repository, &session)?;

    Ok(Some(DraftSessionView {
        base_state_id: session.base_state_id,
        base_change_id: session.base_change_id,
        created_at: session.created_at.clone(),
        description: session.description.clone(),
        status: session.status.as_str().to_string(),
        staged_operations,
        candidate_effect: effect,
        candidate_validation,
        accountability_total_artifacts: acc_report.repository_summary.total,
        accountability_stale_artifacts: acc_report.repository_summary.stale,
        accountability_reconciled_in_draft: reconciled_count,
    }))
}

use std::str::FromStr;

/// Evaluates artifact accountability against a candidate draft session's `working_state`
/// and sequential staged operations.
pub fn analyze_candidate_artifact_accountability(
    repository: &Repository,
    session: &crate::repository::session::DraftSession,
) -> Result<ArtifactAccountabilityReport, QueryError> {
    let store = repository.object_store();

    let mut candidate_elements: std::collections::HashMap<ElementId, (String, ObjectId)> =
        std::collections::HashMap::new();
    for entry in &session.working_state.elements {
        let (_, type_id, _) = get_element_version_info(store, session, entry.version);
        candidate_elements.insert(entry.element_id, (type_id, entry.version));
    }

    let mut candidate_relationships: std::collections::HashMap<
        RelationshipId,
        (String, ElementId, ElementId, ObjectId),
    > = std::collections::HashMap::new();
    for entry in &session.working_state.relationships {
        let (rel_type, src_opt, tgt_opt) =
            get_relationship_version_info(store, session, entry.version);
        if let (Some(src_str), Some(tgt_str)) = (src_opt, tgt_opt)
            && let (Ok(src_id), Ok(tgt_id)) =
                (ElementId::from_str(&src_str), ElementId::from_str(&tgt_str))
        {
            candidate_relationships.insert(
                entry.relationship_id,
                (rel_type, src_id, tgt_id, entry.version),
            );
        }
    }

    let mut effective_baselines: std::collections::HashMap<RelationshipId, ObjectId> =
        std::collections::HashMap::new();
    for (&rel_id, (_, _, tgt_id, _)) in &candidate_relationships {
        if let Ok(base_ver) = resolve_relationship_baseline_version(repository, rel_id, *tgt_id) {
            effective_baselines.insert(rel_id, base_ver);
        } else if let Some((_, tgt_ver)) = candidate_elements.get(tgt_id) {
            effective_baselines.insert(rel_id, *tgt_ver);
        }
    }

    for op in &session.operations {
        if let Operation::AccountArtifact {
            reconciliations, ..
        } = op
        {
            for r in reconciliations {
                effective_baselines.insert(r.relationship_id, r.reconciled_target_version);
            }
        }
    }

    let mut artifact_records = Vec::new();

    for (&elem_id, (type_id, ver_id)) in &candidate_elements {
        let short_type = type_id.rsplit('/').next().unwrap_or(type_id);
        if short_type != "artifact" {
            continue;
        }

        let (_, _, title) = get_element_version_info(store, session, *ver_id);

        let mut baselines = Vec::new();
        let mut has_stale = false;
        let mut has_accountability_rel = false;

        for (&rel_id, (rel_type, src_id, tgt_id, _)) in &candidate_relationships {
            if *src_id != elem_id {
                continue;
            }

            let short_rel = rel_type.rsplit('/').next().unwrap_or(rel_type);
            if short_rel != "represents" && short_rel != "derived-from" {
                continue;
            }

            has_accountability_rel = true;

            let (tgt_type_id, current_tgt_ver) = match candidate_elements.get(tgt_id) {
                Some((t, v)) => (t.clone(), *v),
                None => {
                    has_stale = true;
                    ("unknown".to_string(), ObjectId::from_bytes([0; 32]))
                }
            };

            let recorded_baseline = effective_baselines
                .get(&rel_id)
                .copied()
                .unwrap_or(ObjectId::from_bytes([0; 32]));
            let is_stale = recorded_baseline != current_tgt_ver;

            if is_stale {
                has_stale = true;
            }

            baselines.push(ArtifactBaseline {
                relationship_id: rel_id,
                relationship_type: rel_type.clone(),
                upstream_element_id: *tgt_id,
                upstream_type_id: tgt_type_id,
                baseline_version: recorded_baseline,
                current_version: current_tgt_ver,
                is_stale,
            });
        }

        let status = if !has_accountability_rel {
            ArtifactAccountabilityStatus::Unaccounted
        } else if has_stale {
            ArtifactAccountabilityStatus::Stale
        } else {
            ArtifactAccountabilityStatus::Current
        };

        artifact_records.push(ArtifactAccountability {
            artifact_element_id: elem_id,
            artifact_type_id: type_id.clone(),
            title,
            status,
            baselines,
        });
    }

    artifact_records.sort_by_key(|a| a.artifact_element_id);

    let mut total_current = 0;
    let mut total_stale = 0;
    let mut total_unaccounted = 0;

    for a in &artifact_records {
        match a.status {
            ArtifactAccountabilityStatus::Current => total_current += 1,
            ArtifactAccountabilityStatus::Stale => total_stale += 1,
            ArtifactAccountabilityStatus::Unaccounted => total_unaccounted += 1,
        }
    }

    let repository_summary = ArtifactAccountabilitySummary {
        total: artifact_records.len(),
        current: total_current,
        stale: total_stale,
        unaccounted: total_unaccounted,
    };

    Ok(ArtifactAccountabilityReport {
        artifacts: artifact_records,
        repository_summary,
    })
}

/// Traversal direction filter for Context retrieval queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ContextDirection {
    /// Follow outgoing relationships from source to target.
    Downstream,
    /// Follow incoming relationships from target to source.
    Upstream,
    /// Follow both incoming and outgoing relationships.
    Both,
}

/// Category buckets for context elements.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CategorizedContext {
    pub requirements: Vec<ElementId>,
    pub realizations: Vec<ElementId>,
    pub verification: Vec<ElementId>,
    pub design: Vec<ElementId>,
    pub system: Vec<ElementId>,
}

/// Resolved physical source or artifact route for a context element.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PhysicalRoute {
    pub element_id: ElementId,
    pub path: String,
    pub role: String,
}

/// Context retrieval query response.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContextResult {
    /// Root element identities queried.
    pub roots: Vec<ElementId>,
    /// Unique elements in the retrieved context graph (including roots), ordered deterministically by ElementId.
    pub elements: Vec<ElementView>,
    /// Optional categorization by ontology type.
    pub categorized: Option<CategorizedContext>,
    /// Resolved physical file routes.
    pub physical_routes: Vec<PhysicalRoute>,
}

/// Retrieves point-in-time context for a set of root elements over the accepted state $S_{accepted}$.
pub fn retrieve_context(
    repository: &Repository,
    roots: &[ElementId],
    direction: ContextDirection,
    max_depth: Option<usize>,
    categorize: bool,
) -> Result<ContextResult, QueryError> {
    if let Some(depth) = max_depth {
        if depth == 0 {
            return Err(QueryError::InvalidMaxDepth(0));
        }
    }

    let accepted = repository.ref_store().read_accepted()?;
    let store = repository.object_store();

    let state_bytes = store.get(accepted.state)?;
    let state_canonical = decode_canonical(&state_bytes)?;
    let state = match state_canonical.payload {
        CanonicalPayload::SemanticState(s) => s,
        _ => {
            return Err(QueryError::UnexpectedObjectKind {
                expected: ObjectKind::SemanticState,
                actual: state_canonical.object_kind(),
            });
        }
    };

    let mut loaded_rel_versions: HashMap<RelationshipId, RelationshipVersion> = HashMap::new();
    for entry in &state.relationships {
        let bytes = store.get(entry.version)?;
        let canonical = decode_canonical(&bytes)?;
        if let CanonicalPayload::RelationshipVersion(rv) = canonical.payload {
            loaded_rel_versions.insert(entry.relationship_id, rv);
        }
    }

    let mut reached_elements: HashSet<ElementId> = HashSet::new();

    for &root in roots {
        reached_elements.insert(root);
        let mut path_visited_rels: HashSet<RelationshipId> = HashSet::new();
        explore_context_graph(
            root,
            &state.relationships,
            &loaded_rel_versions,
            direction,
            0,
            max_depth,
            &mut path_visited_rels,
            &mut reached_elements,
        );
    }

    let mut elements: Vec<ElementView> = Vec::new();
    let mut physical_routes: Vec<PhysicalRoute> = Vec::new();

    let mut sorted_reached: Vec<ElementId> = reached_elements.into_iter().collect();
    sorted_reached.sort();

    for elem_id in sorted_reached {
        let view = show_element(repository, elem_id)?;

        for (prop_key, prop_val) in &view.element.properties {
            let key_lower = prop_key.to_lowercase();
            if key_lower == "path"
                || key_lower == "file"
                || key_lower == "uri"
                || key_lower == "location"
            {
                if let PropertyValue::Text(val_str) = prop_val {
                    physical_routes.push(PhysicalRoute {
                        element_id: elem_id,
                        path: val_str.clone(),
                        role: "source".to_string(),
                    });
                }
            }
        }

        for rel in &view.relationships.outgoing {
            if rel.relationship_type_id == "kat.core/realizes"
                || rel.relationship_type_id == "kat.core/accounts-for"
            {
                if let Ok(target_view) = show_element(repository, rel.target_element_id) {
                    for (pk, pv) in &target_view.element.properties {
                        if pk.to_lowercase() == "path" || pk.to_lowercase() == "file" {
                            if let PropertyValue::Text(val_str) = pv {
                                physical_routes.push(PhysicalRoute {
                                    element_id: elem_id,
                                    path: val_str.clone(),
                                    role: "realizes".to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }

        elements.push(view);
    }

    let categorized = if categorize {
        let mut reqs = Vec::new();
        let mut reals = Vec::new();
        let mut verifs = Vec::new();
        let mut des = Vec::new();
        let mut sys = Vec::new();

        for ev in &elements {
            let t = ev.element.type_id.as_str();
            match t {
                "kat.core/requirement"
                | "kat.core/goal"
                | "kat.core/use-case"
                | "kat.core/user-story" => reqs.push(ev.element_id),
                "kat.core/implementation"
                | "kat.core/code"
                | "kat.core/module"
                | "kat.core/service" => reals.push(ev.element_id),
                "kat.core/test" | "kat.core/verification" | "kat.core/benchmark" => {
                    verifs.push(ev.element_id)
                }
                "kat.core/architecture"
                | "kat.core/design"
                | "kat.core/decision"
                | "kat.core/model" => des.push(ev.element_id),
                _ => sys.push(ev.element_id),
            }
        }

        Some(CategorizedContext {
            requirements: reqs,
            realizations: reals,
            verification: verifs,
            design: des,
            system: sys,
        })
    } else {
        None
    };

    Ok(ContextResult {
        roots: roots.to_vec(),
        elements,
        categorized,
        physical_routes,
    })
}

fn explore_context_graph(
    current: ElementId,
    state_rels: &[crate::domain::state::RelationshipStateEntry],
    loaded_rel_versions: &HashMap<RelationshipId, RelationshipVersion>,
    direction: ContextDirection,
    current_depth: usize,
    max_depth: Option<usize>,
    path_visited_rels: &mut HashSet<RelationshipId>,
    reached_elements: &mut HashSet<ElementId>,
) {
    if let Some(limit) = max_depth {
        if current_depth >= limit {
            return;
        }
    }

    for entry in state_rels {
        if path_visited_rels.contains(&entry.relationship_id) {
            continue;
        }

        let Some(rel_v) = loaded_rel_versions.get(&entry.relationship_id) else {
            continue;
        };

        let (next_id, allowed) = match direction {
            ContextDirection::Downstream => {
                if rel_v.source_element_id == current {
                    (Some(rel_v.target_element_id), true)
                } else {
                    (None, false)
                }
            }
            ContextDirection::Upstream => {
                if rel_v.target_element_id == current {
                    (Some(rel_v.source_element_id), true)
                } else {
                    (None, false)
                }
            }
            ContextDirection::Both => {
                if rel_v.source_element_id == current {
                    (Some(rel_v.target_element_id), true)
                } else if rel_v.target_element_id == current {
                    (Some(rel_v.source_element_id), true)
                } else {
                    (None, false)
                }
            }
        };

        if allowed {
            if let Some(target_id) = next_id {
                reached_elements.insert(target_id);

                path_visited_rels.insert(entry.relationship_id);
                explore_context_graph(
                    target_id,
                    state_rels,
                    loaded_rel_versions,
                    direction,
                    current_depth + 1,
                    max_depth,
                    path_visited_rels,
                    reached_elements,
                );
                path_visited_rels.remove(&entry.relationship_id);
            }
        }
    }
}
