use crate::domain::conflict::{MaterializationConflictKind, SemanticConflictKind};
use crate::domain::identity::{ElementId, RelationshipId, RepositoryRevisionId};
use crate::domain::workspace::WorkspaceId;
use crate::repository::query::QueryError;
use sha2::{Digest, Sha256};
use std::path::Path;

pub struct ConflictSummary {
    pub base_revision: RepositoryRevisionId,
    pub target_revision: RepositoryRevisionId,
    pub semantic: Vec<SemanticConflictView>,
    pub physical: Vec<MaterializationConflictView>,
    pub validation_findings: Vec<ValidationFindingView>,
}

pub struct SemanticConflictView {
    pub conflict_id: String,
    pub kind: SemanticConflictKind,
    pub affected_elements: Vec<ElementId>,
    pub affected_relationships: Vec<RelationshipId>,
}

pub struct MaterializationConflictView {
    pub conflict_id: String,
    pub kind: MaterializationConflictKind,
    pub paths: Vec<std::path::PathBuf>,
}

pub struct ValidationFindingView {
    pub diagnostic: String,
}

pub fn project_conflicts(
    repo_root: &Path,
    workspace_id: &WorkspaceId,
) -> Result<ConflictSummary, QueryError> {
    let session_path = crate::repository::reconcile::session_path(repo_root, workspace_id);
    if !session_path.exists() {
        return Err(QueryError::NoActiveReconciliation);
    }

    // Read the session JSON directly since load_reconciliation_session requires an expected_base
    let json = std::fs::read_to_string(&session_path)
        .map_err(|e| QueryError::WorkspaceConflict(e.to_string()))?;
    let session: crate::repository::reconcile::ReconciliationSession = serde_json::from_str(&json)
        .map_err(|e| QueryError::WorkspaceConflict(format!("Invalid session: {}", e)))?;

    match session.state {
        crate::repository::reconcile::ReconciliationSessionState::PreparedClean { .. } => {
            Ok(ConflictSummary {
                base_revision: session.base_revision,
                target_revision: session.target_revision,
                semantic: Vec::new(),
                physical: Vec::new(),
                validation_findings: Vec::new(),
            })
        }
        crate::repository::reconcile::ReconciliationSessionState::Conflicted { candidate }
        | crate::repository::reconcile::ReconciliationSessionState::ConflictedMaterialized {
            candidate,
        } => {
            let mut semantic_views = Vec::new();
            // We want deterministic ordering. Let's rely on the canonical ordering
            // inside candidate (which should be sorted, but let's sort just in case).
            let sorted_semantics = candidate.semantic_conflicts.clone();

            for conflict in sorted_semantics {
                // Deterministic ID generation based on kind and elements/relationships
                let mut hasher = Sha256::new();
                hasher.update(serde_json::to_vec(&conflict.kind).unwrap_or_default());
                for e in &conflict.affected_elements {
                    hasher.update(e.as_uuid().into_bytes());
                }
                for r in &conflict.affected_relationships {
                    hasher.update(r.as_uuid().into_bytes());
                }
                let hash = hasher.finalize();
                let conflict_id = hex::encode(hash);

                semantic_views.push(SemanticConflictView {
                    conflict_id,
                    kind: conflict.kind,
                    affected_elements: conflict.affected_elements,
                    affected_relationships: conflict.affected_relationships,
                });
            }

            let mut physical_views = Vec::new();
            let mut sorted_physical = candidate.materialization_conflicts.clone();
            // Actually MaterializationConflict implements Ord, so we can sort them
            sorted_physical.sort();

            for conflict in sorted_physical {
                let mut hasher = Sha256::new();
                hasher.update(serde_json::to_vec(&conflict.kind).unwrap_or_default());
                for p in &conflict.paths {
                    hasher.update(p.to_string_lossy().as_bytes());
                }
                let hash = hasher.finalize();
                let conflict_id = hex::encode(hash);

                physical_views.push(MaterializationConflictView {
                    conflict_id,
                    kind: conflict.kind,
                    paths: conflict.paths,
                });
            }

            let mut validation_views = Vec::new();
            for finding in candidate.validation_findings {
                validation_views.push(ValidationFindingView {
                    diagnostic: finding.diagnostic,
                });
            }

            // To guarantee stable ordering of the conflict IDs themselves if Phase 8/9 didn't
            // sort the semantic conflicts fully, we can sort the projected views by their ID.
            semantic_views.sort_by(|a, b| a.conflict_id.cmp(&b.conflict_id));

            Ok(ConflictSummary {
                base_revision: session.base_revision,
                target_revision: session.target_revision,
                semantic: semantic_views,
                physical: physical_views,
                validation_findings: validation_views,
            })
        }
    }
}
