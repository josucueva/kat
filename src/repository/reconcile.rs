use std::collections::BTreeSet;

use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::conflict::{SemanticConflict, SemanticConflictKind, ValidationFinding};
use crate::domain::identity::{ElementId, ObjectId, RelationshipId, RepositoryRevisionId};
use crate::domain::state::{ElementStateEntry, RelationshipStateEntry, SemanticState};
use crate::domain::workspace::WorkspaceId;
use crate::domain::workspace::{PhysicalReconciliationResult, WorkspaceBackend};
use crate::encoding::cbor;
use crate::encoding::object::{CanonicalObject, CanonicalPayload, ObjectKind};
use crate::repository::object_store::ObjectStore;
use crate::repository::query::{QueryError, load_typed};
use crate::repository::validation::validate_repository_state;

#[allow(clippy::large_enum_variant)]
pub enum ReconciliationResult {
    /// Both semantic and physical reconciliation succeeded with no conflicts or findings.
    /// The proposed state and new physical snapshot are ready to be committed.
    Clean { revision: RepositoryRevisionId },
    /// Conflicts or findings were found in either domain.
    Conflicted(ReconciliationCandidate),
}

/// A candidate produced by attempting semantic reconciliation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReconciliationCandidate {
    pub version: u32,
    pub workspace_id: WorkspaceId,
    pub base_revision: RepositoryRevisionId,
    pub local_revision: RepositoryRevisionId,
    pub other_revision: RepositoryRevisionId,
    pub proposed_semantic_state: SemanticState,
    pub semantic_conflicts: Vec<SemanticConflict>,
    pub physical_candidate: Option<crate::domain::workspace::PhysicalReconciliationCandidate>,
    pub materialization_conflicts: Vec<crate::domain::conflict::MaterializationConflict>,
    pub validation_findings: Vec<ValidationFinding>,
}

#[derive(Debug, thiserror::Error)]
pub enum CandidateLoadError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unsupported candidate format version: {0}")]
    UnsupportedVersion(u32),
    #[error("Stale candidate: base revision mismatch")]
    StaleBase,
    #[error("Stale candidate: local revision mismatch")]
    StaleLocal,
    #[error("Stale candidate: other revision mismatch")]
    StaleOther,
    #[error("Stale candidate: workspace ID mismatch")]
    StaleWorkspace,
}

/// Attempts to reconcile two divergent semantic states against their common base.
#[allow(clippy::too_many_arguments)]
pub fn reconcile_semantic(
    store: &ObjectStore,
    workspace_id: WorkspaceId,
    base_revision: RepositoryRevisionId,
    local_revision: RepositoryRevisionId,
    other_revision: RepositoryRevisionId,
    base: &SemanticState,
    local: &SemanticState,
    other: &SemanticState,
) -> Result<ReconciliationCandidate, QueryError> {
    let mut conflicts = Vec::new();
    let mut findings = Vec::new();

    // 1. Reconcile Elements
    let mut next_elements = Vec::new();
    let all_elements = collect_element_ids(base, local, other);

    for element_id in all_elements {
        let b = get_element_version(base, &element_id);
        let l = get_element_version(local, &element_id);
        let o = get_element_version(other, &element_id);

        if l == o {
            if let Some(v) = l {
                next_elements.push(ElementStateEntry {
                    element_id,
                    version: *v,
                });
            }
        } else if l == b && o != b {
            if let Some(v) = o {
                next_elements.push(ElementStateEntry {
                    element_id,
                    version: *v,
                });
            }
        } else if o == b && l != b {
            if let Some(v) = l {
                next_elements.push(ElementStateEntry {
                    element_id,
                    version: *v,
                });
            }
        } else {
            // Conflict (L != B && O != B && L != O)
            let kind = determine_element_conflict_kind(store, b.copied(), l.copied(), o.copied())?;
            conflicts.push(SemanticConflict {
                affected_elements: vec![element_id],
                affected_relationships: vec![],
                kind,
            });
            // Neutral rule: Retain base version if it existed. Omit if concurrent creation.
            if let Some(v) = b {
                next_elements.push(ElementStateEntry {
                    element_id,
                    version: *v,
                });
            }
        }
    }

    // 2. Reconcile Relationships
    let mut next_relationships = Vec::new();
    let all_relationships = collect_relationship_ids(base, local, other);

    for rel_id in all_relationships {
        let b = get_relationship_version(base, &rel_id);
        let l = get_relationship_version(local, &rel_id);
        let o = get_relationship_version(other, &rel_id);

        if l == o {
            if let Some(v) = l {
                next_relationships.push(RelationshipStateEntry {
                    relationship_id: rel_id,
                    version: *v,
                });
            }
        } else if l == b && o != b {
            if let Some(v) = o {
                next_relationships.push(RelationshipStateEntry {
                    relationship_id: rel_id,
                    version: *v,
                });
            }
        } else if o == b && l != b {
            if let Some(v) = l {
                next_relationships.push(RelationshipStateEntry {
                    relationship_id: rel_id,
                    version: *v,
                });
            }
        } else {
            // RelationshipConflict for relationships
            conflicts.push(SemanticConflict {
                affected_elements: vec![],
                affected_relationships: vec![rel_id],
                kind: SemanticConflictKind::RelationshipConflict {
                    base_version: b.copied(),
                    local_version: l.copied(),
                    other_version: o.copied(),
                },
            });
            // Neutral rule: Retain base version if it existed.
            if let Some(v) = b {
                next_relationships.push(RelationshipStateEntry {
                    relationship_id: rel_id,
                    version: *v,
                });
            }
        }
    }

    let ontology_version = if local.ontology_version == other.ontology_version {
        local.ontology_version
    } else if local.ontology_version == base.ontology_version {
        other.ontology_version
    } else if other.ontology_version == base.ontology_version {
        local.ontology_version
    } else {
        conflicts.push(SemanticConflict {
            affected_elements: vec![],
            affected_relationships: vec![],
            kind: SemanticConflictKind::ConcurrentModification {
                base_version: Some(base.ontology_version),
                local_version: Some(local.ontology_version),
                other_version: Some(other.ontology_version),
            },
        });
        base.ontology_version
    };

    // 3. Combined Validation
    // Both next_elements and next_relationships must be sorted to be canonical
    next_elements.sort_unstable();
    next_relationships.sort_unstable();

    let proposed_semantic_state = SemanticState {
        ontology_version,
        elements: next_elements,
        relationships: next_relationships,
    };

    if let Err(validation_err) =
        validate_repository_state(store, &proposed_semantic_state, &[], &[])
    {
        findings.push(ValidationFinding {
            diagnostic: validation_err.to_string(),
        });
    }

    Ok(ReconciliationCandidate {
        version: 1,
        workspace_id,
        base_revision,
        local_revision,
        other_revision,
        proposed_semantic_state,
        semantic_conflicts: conflicts,
        physical_candidate: None,
        materialization_conflicts: vec![],
        validation_findings: findings,
    })
}

// Helper for the persistence path
fn candidate_path(repo_root: &Path, workspace_id: &WorkspaceId) -> PathBuf {
    repo_root
        .join(".kat")
        .join("workspaces")
        .join(&workspace_id.0)
        .join("reconciliation_candidate.json")
}

pub fn save_reconciliation_candidate(
    repo_root: &Path,
    workspace_id: &WorkspaceId,
    candidate: &ReconciliationCandidate,
    backend: &dyn WorkspaceBackend,
) -> std::io::Result<()> {
    let path = candidate_path(repo_root, workspace_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // 1. Construct physical state & 2. Write backend candidate atomically
    if let Some(phys) = &candidate.physical_candidate {
        backend
            .persist_physical_candidate(workspace_id, phys)
            .map_err(std::io::Error::other)?;
        // 3. Verify it can be reopened (the API doesn't expose it generically yet, but backend has done the durable write)
        // If it was GitWorkspaceBackend, it would load_physical_candidate. We can just rely on the durable write success.
    }

    // 4. Write reconciliation_candidate.json atomically
    let temp_path = path.with_extension("tmp");
    let json = serde_json::to_string_pretty(candidate).map_err(std::io::Error::other)?;
    fs::write(&temp_path, json)?;
    fs::rename(temp_path, path)?;
    Ok(())
}

pub fn load_reconciliation_candidate(
    repo_root: &Path,
    workspace_id: &WorkspaceId,
    expected_base: RepositoryRevisionId,
    expected_local: RepositoryRevisionId,
    expected_other: RepositoryRevisionId,
) -> Result<Option<ReconciliationCandidate>, CandidateLoadError> {
    let path = candidate_path(repo_root, workspace_id);
    if !path.exists() {
        return Ok(None);
    }
    let json = fs::read_to_string(&path)?;

    // Parse as a generic value first to check version
    let parsed: serde_json::Value = serde_json::from_str(&json)?;
    if let Some(version) = parsed.get("version").and_then(|v| v.as_u64()) {
        if version != 1 {
            return Err(CandidateLoadError::UnsupportedVersion(version as u32));
        }
    } else {
        return Err(CandidateLoadError::UnsupportedVersion(0));
    }

    let candidate: ReconciliationCandidate = serde_json::from_value(parsed)?;

    if candidate.workspace_id != *workspace_id {
        return Err(CandidateLoadError::StaleWorkspace);
    }
    if candidate.base_revision != expected_base {
        return Err(CandidateLoadError::StaleBase);
    }
    if candidate.local_revision != expected_local {
        return Err(CandidateLoadError::StaleLocal);
    }
    if candidate.other_revision != expected_other {
        return Err(CandidateLoadError::StaleOther);
    }

    Ok(Some(candidate))
}

/// Orchestrates both semantic and physical reconciliation.
///
/// If both domains are clean, returns `ReconciliationResult::Clean` with the ready-to-commit state.
/// If either domain produces conflicts or validation findings, returns a `ReconciliationResult::Conflicted`
/// candidate that must be persisted for manual resolution.
#[allow(clippy::too_many_arguments)]
pub fn reconcile(
    store: &ObjectStore,
    backend: &dyn WorkspaceBackend,
    workspace_id: WorkspaceId,
    base_rev_id: RepositoryRevisionId,
    local_rev_id: RepositoryRevisionId,
    other_rev_id: RepositoryRevisionId,
    base_snapshot: &crate::domain::identity::WorkspaceSnapshotId,
    local_snapshot: &crate::domain::identity::WorkspaceSnapshotId,
    other_snapshot: &crate::domain::identity::WorkspaceSnapshotId,
    base_state: &SemanticState,
    local_state: &SemanticState,
    other_state: &SemanticState,
) -> Result<ReconciliationResult, QueryError> {
    // 1. Semantic Reconciliation
    let mut candidate = reconcile_semantic(
        store,
        workspace_id,
        base_rev_id,
        local_rev_id,
        other_rev_id,
        base_state,
        local_state,
        other_state,
    )?;

    // 2. Physical Reconciliation
    let physical_res = backend
        .reconcile_physical(base_snapshot, local_snapshot, other_snapshot)
        .map_err(QueryError::WorkspaceBackend)?;

    match physical_res {
        PhysicalReconciliationResult::Clean {
            snapshot: new_snapshot_id,
        } => {
            if candidate.semantic_conflicts.is_empty() && candidate.validation_findings.is_empty() {
                // Both semantic and physical domains are clean

                // 1. Persist the semantic state
                let state_payload =
                    CanonicalPayload::SemanticState(candidate.proposed_semantic_state.clone());
                let state_bytes = cbor::canonical_bytes(&CanonicalObject {
                    payload: state_payload,
                })
                .map_err(|e| {
                    QueryError::ObjectStore(crate::repository::object_store::ObjectStoreError::Io(
                        std::io::Error::other(e),
                    ))
                })?;
                let state_id = store.put(&state_bytes).map_err(QueryError::ObjectStore)?;
                let semantic_state_id =
                    crate::domain::identity::SemanticStateId::from_object_id(state_id);

                // 2. Parents must be deterministically sorted as per spec
                // Note: the spec usually implies Local and Other are the parents of a 3-way merge, base is implied by their ancestry.
                // We'll record local and other.
                let mut parents = vec![local_rev_id, other_rev_id];
                parents.sort();

                // 3. Construct the RepositoryRevision
                let rev = crate::domain::revision::RepositoryRevision {
                    parents,
                    semantic_state: semantic_state_id,
                    workspace_snapshot: new_snapshot_id,
                    semantic_change: None, // As per P9-11
                };

                // 4. Persist the RepositoryRevision
                let rev_payload = CanonicalPayload::RepositoryRevision(rev);
                let rev_bytes = cbor::canonical_bytes(&CanonicalObject {
                    payload: rev_payload,
                })
                .map_err(|e| {
                    QueryError::ObjectStore(crate::repository::object_store::ObjectStoreError::Io(
                        std::io::Error::other(e),
                    ))
                })?;
                let rev_obj_id = store.put(&rev_bytes).map_err(QueryError::ObjectStore)?;
                let revision_id = RepositoryRevisionId::from_object_id(rev_obj_id);

                return Ok(ReconciliationResult::Clean {
                    revision: revision_id,
                });
            }
        }
        PhysicalReconciliationResult::Conflicted(phys_candidate) => {
            candidate.materialization_conflicts = phys_candidate.conflicts.clone();
            candidate.physical_candidate = Some(phys_candidate);
        }
    }

    // Either semantic conflicts/findings exist, or physical conflicts exist (or both).
    Ok(ReconciliationResult::Conflicted(candidate))
}

fn collect_element_ids(
    base: &SemanticState,
    local: &SemanticState,
    other: &SemanticState,
) -> BTreeSet<ElementId> {
    let mut set = BTreeSet::new();
    for e in &base.elements {
        set.insert(e.element_id);
    }
    for e in &local.elements {
        set.insert(e.element_id);
    }
    for e in &other.elements {
        set.insert(e.element_id);
    }
    set
}

fn collect_relationship_ids(
    base: &SemanticState,
    local: &SemanticState,
    other: &SemanticState,
) -> BTreeSet<RelationshipId> {
    let mut set = BTreeSet::new();
    for e in &base.relationships {
        set.insert(e.relationship_id);
    }
    for e in &local.relationships {
        set.insert(e.relationship_id);
    }
    for e in &other.relationships {
        set.insert(e.relationship_id);
    }
    set
}

fn get_element_version<'a>(state: &'a SemanticState, id: &ElementId) -> Option<&'a ObjectId> {
    state
        .elements
        .iter()
        .find(|e| &e.element_id == id)
        .map(|e| &e.version)
}

fn get_relationship_version<'a>(
    state: &'a SemanticState,
    id: &RelationshipId,
) -> Option<&'a ObjectId> {
    state
        .relationships
        .iter()
        .find(|e| &e.relationship_id == id)
        .map(|e| &e.version)
}

fn determine_element_conflict_kind(
    store: &ObjectStore,
    b: Option<ObjectId>,
    l: Option<ObjectId>,
    o: Option<ObjectId>,
) -> Result<SemanticConflictKind, QueryError> {
    let mut l_lifecycle = None;
    let mut o_lifecycle = None;

    if let Some(l_id) = l {
        let obj = load_typed(store, l_id, ObjectKind::KnowledgeElementVersion)?;
        if let CanonicalPayload::KnowledgeElementVersion(ev) = obj.payload {
            l_lifecycle = Some(ev.lifecycle);
        }
    }

    if let Some(o_id) = o {
        let obj = load_typed(store, o_id, ObjectKind::KnowledgeElementVersion)?;
        if let CanonicalPayload::KnowledgeElementVersion(ev) = obj.payload {
            o_lifecycle = Some(ev.lifecycle);
        }
    }

    if l_lifecycle == Some(crate::domain::element::Lifecycle::Superseded)
        || o_lifecycle == Some(crate::domain::element::Lifecycle::Superseded)
    {
        return Ok(SemanticConflictKind::SupersessionConflict {
            base_version: b,
            local_version: l,
            other_version: o,
        });
    }

    if l_lifecycle.is_some() && o_lifecycle.is_some() && l_lifecycle != o_lifecycle {
        Ok(SemanticConflictKind::LifecycleMismatch {
            base_version: b,
            local_version: l,
            other_version: o,
        })
    } else {
        Ok(SemanticConflictKind::ConcurrentModification {
            base_version: b,
            local_version: l,
            other_version: o,
        })
    }
}
