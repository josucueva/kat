use std::collections::{HashSet, VecDeque};

use crate::domain::identity::RepositoryRevisionId;
use crate::domain::revision::RepositoryRevision;
use crate::encoding::decode::decode_canonical;
use crate::encoding::object::{CanonicalPayload, ObjectKind};
use crate::repository::object_store::ObjectStore;
use crate::repository::query::QueryError;

/// Defines the topological relationship between two repository revisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DivergenceState {
    /// Both revisions are identical.
    Same,
    /// The local revision is a strict topological descendant of the other.
    LocalAhead,
    /// The other revision is a strict topological descendant of the local revision.
    OtherAhead,
    /// The revisions have diverged but share exactly one best common ancestor.
    Diverged { common_base: RepositoryRevisionId },
    /// The revisions have diverged and multiple incomparable best common ancestors exist.
    AmbiguousMergeBase { bases: Vec<RepositoryRevisionId> },
    /// The revisions have no common ancestor (disjoint histories).
    Unrelated,
}

fn load_revision(
    store: &ObjectStore,
    id: RepositoryRevisionId,
) -> Result<RepositoryRevision, QueryError> {
    let bytes = store
        .get(id.as_object_id())
        .map_err(QueryError::ObjectStore)?;
    let canonical = decode_canonical(&bytes).map_err(QueryError::Decoding)?;
    if canonical.object_kind() != ObjectKind::RepositoryRevision {
        return Err(QueryError::UnexpectedObjectKind {
            expected: ObjectKind::RepositoryRevision,
            actual: canonical.object_kind(),
        });
    }
    match canonical.payload {
        CanonicalPayload::RepositoryRevision(rev) => Ok(rev),
        _ => unreachable!(),
    }
}

/// Returns the subset of `revs` that are topological graph heads (i.e., have no successors in the set).
pub fn find_graph_heads(
    store: &ObjectStore,
    revs: &[RepositoryRevisionId],
) -> Result<Vec<RepositoryRevisionId>, QueryError> {
    let mut heads: HashSet<RepositoryRevisionId> = revs.iter().copied().collect();

    for &rev_id in revs {
        let rev = load_revision(store, rev_id)?;
        for parent in rev.parents {
            heads.remove(&parent);
        }
    }

    let mut result: Vec<_> = heads.into_iter().collect();
    // Return them in a deterministic order
    result.sort_by_key(|id| id.to_string());
    Ok(result)
}

/// Returns all ancestors reachable from a given revision.
fn get_ancestors(
    store: &ObjectStore,
    start: RepositoryRevisionId,
) -> Result<HashSet<RepositoryRevisionId>, QueryError> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    queue.push_back(start);
    visited.insert(start);

    while let Some(current_id) = queue.pop_front() {
        let rev = load_revision(store, current_id)?;
        for parent in rev.parents {
            if visited.insert(parent) {
                queue.push_back(parent);
            }
        }
    }

    Ok(visited)
}

/// Finds the best common ancestors of two revisions using a deterministic merge-base policy.
pub fn find_best_common_ancestors(
    store: &ObjectStore,
    a: RepositoryRevisionId,
    b: RepositoryRevisionId,
) -> Result<Vec<RepositoryRevisionId>, QueryError> {
    if a == b {
        return Ok(vec![a]);
    }

    let ancestors_a = get_ancestors(store, a)?;
    let ancestors_b = get_ancestors(store, b)?;

    let common_ancestors: HashSet<_> = ancestors_a.intersection(&ancestors_b).copied().collect();

    if common_ancestors.is_empty() {
        return Ok(Vec::new());
    }

    // A candidate is "best" if no other common ancestor has this candidate as an ancestor.
    let mut best = Vec::new();
    for &candidate in &common_ancestors {
        let mut has_descendant_in_common = false;
        for &other in &common_ancestors {
            if other != candidate {
                let other_ancestors = get_ancestors(store, other)?;
                if other_ancestors.contains(&candidate) {
                    has_descendant_in_common = true;
                    break;
                }
            }
        }

        if !has_descendant_in_common {
            best.push(candidate);
        }
    }

    best.sort_by_key(|id| id.to_string());
    Ok(best)
}

/// Compares the topology of two revisions and returns their divergence state.
pub fn compare_ancestry(
    store: &ObjectStore,
    local: RepositoryRevisionId,
    other: RepositoryRevisionId,
) -> Result<DivergenceState, QueryError> {
    if local == other {
        return Ok(DivergenceState::Same);
    }

    let best_ancestors = find_best_common_ancestors(store, local, other)?;

    if best_ancestors.is_empty() {
        return Ok(DivergenceState::Unrelated);
    }

    if best_ancestors.len() == 1 {
        let base = best_ancestors[0];
        if base == other {
            return Ok(DivergenceState::LocalAhead);
        } else if base == local {
            return Ok(DivergenceState::OtherAhead);
        } else {
            return Ok(DivergenceState::Diverged { common_base: base });
        }
    }

    Ok(DivergenceState::AmbiguousMergeBase {
        bases: best_ancestors,
    })
}
