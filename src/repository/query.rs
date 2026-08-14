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
use crate::domain::element::KnowledgeElementVersion;
use crate::domain::identity::{ElementId, ObjectId, RelationshipId};
use crate::encoding::decode::DecodingError;
use crate::encoding::decode_canonical;
use crate::encoding::object::{CanonicalObject, CanonicalPayload, ObjectKind};
use crate::repository::object_store::{ObjectStore, ObjectStoreError};
use crate::repository::open::Repository;
use crate::repository::ref_store::{RefStore, RefStoreError};

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
