use crate::domain::conflict::{MaterializationConflictKind, SemanticConflictKind};
use crate::domain::identity::{ElementId, RelationshipId, RepositoryRevisionId};
use crate::domain::workspace::WorkspaceId;
use crate::repository::query::QueryError;
use std::path::Path;

pub struct ConflictSummary {
    pub base_revision: RepositoryRevisionId,
    pub target_revision: RepositoryRevisionId,
    pub semantic: Vec<SemanticConflictView>,
    pub physical: Vec<MaterializationConflictView>,
    pub validation_findings: Vec<ValidationFindingView>,
}

#[derive(serde::Serialize)]
pub struct SemanticConflictView {
    pub conflict_id: String,
    pub kind: SemanticConflictKind,
    pub affected_elements: Vec<ElementId>,
    pub affected_relationships: Vec<RelationshipId>,
}

#[derive(serde::Serialize)]
pub struct MaterializationConflictView {
    pub conflict_id: String,
    pub kind: MaterializationConflictKind,
    pub paths: Vec<std::path::PathBuf>,
}

#[derive(serde::Serialize)]
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
            for conflict in candidate.semantic_conflicts {
                let mut affected_elements = conflict.affected_elements;
                let mut affected_relationships = conflict.affected_relationships;
                affected_elements.sort();
                affected_relationships.sort();
                semantic_views.push(SemanticConflictView {
                    conflict_id: conflict.id,
                    kind: conflict.kind,
                    affected_elements,
                    affected_relationships,
                });
            }

            let mut physical_views = Vec::new();
            for conflict in candidate.materialization_conflicts {
                let mut paths = conflict.paths;
                paths.sort();
                physical_views.push(MaterializationConflictView {
                    conflict_id: conflict.id,
                    kind: conflict.kind,
                    paths,
                });
            }

            let mut validation_views = Vec::new();
            for finding in candidate.validation_findings {
                validation_views.push(ValidationFindingView {
                    diagnostic: finding.diagnostic,
                });
            }

            semantic_views.sort_by(|a, b| a.conflict_id.cmp(&b.conflict_id));
            physical_views.sort_by(|a, b| a.conflict_id.cmp(&b.conflict_id));
            validation_views.sort_by(|a, b| a.diagnostic.cmp(&b.diagnostic));

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
