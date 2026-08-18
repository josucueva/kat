//! Advisory Graph Quality Diagnostics (GQ-01 to GQ-04) and Aggregated Check Porcelain.

use crate::domain::identity::ElementId;
use crate::repository::open::Repository;
use crate::repository::query::{
    QueryError, list_elements, show_element, analyze_artifact_accountability, ArtifactAccountabilityStatus,
};
use crate::repository::validation::repository::{ValidationReport, validate_repository};

/// Severity level for graph quality diagnostic findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum QualitySeverity {
    /// Advisory diagnostic finding (does NOT cause process exit code 1 or repository invalidation).
    Advisory,
}

/// A single advisory graph quality diagnostic finding.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GraphQualityFinding {
    /// Rule identifier (e.g. "GQ-01", "GQ-02", "GQ-03", "GQ-04").
    pub rule_id: String,
    /// Severity level (Advisory).
    pub severity: QualitySeverity,
    /// Target element identity (if target-specific).
    pub target_element_id: Option<ElementId>,
    /// Human-oriented description of the finding.
    pub message: String,
    /// Supporting details or context lines.
    pub details: Vec<String>,
}

/// Aggregated graph quality diagnostic report.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GraphQualityReport {
    /// Total count of advisory findings detected.
    pub total_findings: usize,
    /// List of diagnostic findings.
    pub findings: Vec<GraphQualityFinding>,
}

/// Aggregated result for porcelain `kat check`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CheckReport {
    /// True if 0 mechanical invariant or ontology violations exist.
    pub repository_clean: bool,
    /// Mechanical validation report (mechanical consistency).
    pub mechanical_validation: ValidationReport,
    /// Advisory graph quality findings report.
    pub graph_quality: GraphQualityReport,
}

/// Runs advisory graph quality rule checks against the accepted repository state.
pub fn analyze_graph_quality(
    repository: &Repository,
) -> Result<GraphQualityReport, QueryError> {
    let elements = list_elements(repository, Default::default())?;
    let mut findings = Vec::new();

    for elem_summary in &elements {
        let view = show_element(repository, elem_summary.element_id)?;
        let total_rels = view.relationships.incoming.len() + view.relationships.outgoing.len();

        // GQ-01: Isolated Unlinked Element
        if total_rels == 0 {
            findings.push(GraphQualityFinding {
                rule_id: "GQ-01".to_string(),
                severity: QualitySeverity::Advisory,
                target_element_id: Some(elem_summary.element_id),
                message: format!(
                    "Element {} ({}) has no incoming or outgoing relationships",
                    elem_summary.element_id, view.element.type_id
                ),
                details: vec!["Consider linking this element to upstream requirements or downstream realizations".to_string()],
            });
        }

        // GQ-02: Requirement Lacks Realization Path
        let type_id = view.element.type_id.as_str();
        if type_id == "kat.core/requirement" || type_id == "kat.core/user-story" || type_id == "kat.core/use-case" {
            let has_realization = view.relationships.incoming.iter().any(|rel| {
                rel.relationship_type_id == "kat.core/realizes"
            });
            if !has_realization {
                findings.push(GraphQualityFinding {
                    rule_id: "GQ-02".to_string(),
                    severity: QualitySeverity::Advisory,
                    target_element_id: Some(elem_summary.element_id),
                    message: format!(
                        "Requirement {} has no active incoming realization path via kat.core/realizes",
                        elem_summary.element_id
                    ),
                    details: vec!["Link an implementation element to this requirement using 'kat link --type kat.core/realizes'".to_string()],
                });
            }
        }

        // GQ-03: Implementation Lacks Verification
        if type_id == "kat.core/implementation" || type_id == "kat.core/code" || type_id == "kat.core/service" {
            let has_verification = view.relationships.incoming.iter().any(|rel| {
                rel.relationship_type_id == "kat.core/validates"
            });
            if !has_verification {
                findings.push(GraphQualityFinding {
                    rule_id: "GQ-03".to_string(),
                    severity: QualitySeverity::Advisory,
                    target_element_id: Some(elem_summary.element_id),
                    message: format!(
                        "Implementation {} has no active incoming verification link via kat.core/validates",
                        elem_summary.element_id
                    ),
                    details: vec!["Link a test element to this implementation using 'kat link --type kat.core/validates'".to_string()],
                });
            }
        }
    }

    // GQ-04: Unaccounted Artifact File Link
    if let Ok(account_report) = analyze_artifact_accountability(repository) {
        for artifact in &account_report.artifacts {
            if artifact.status == ArtifactAccountabilityStatus::Unaccounted || artifact.status == ArtifactAccountabilityStatus::Stale {
                findings.push(GraphQualityFinding {
                    rule_id: "GQ-04".to_string(),
                    severity: QualitySeverity::Advisory,
                    target_element_id: Some(artifact.artifact_element_id),
                    message: format!(
                        "Artifact {} has accountability status: {:?}",
                        artifact.artifact_element_id, artifact.status
                    ),
                    details: vec!["Update artifact accountability coverage using 'kat account'".to_string()],
                });
            }
        }
    }

    Ok(GraphQualityReport {
        total_findings: findings.len(),
        findings,
    })
}

/// Performs comprehensive mechanical repository check and advisory graph quality analysis.
pub fn run_check(repository: &Repository) -> Result<CheckReport, QueryError> {
    let mechanical_validation = validate_repository(repository)?;
    let repository_clean = mechanical_validation.violations.is_empty();
    let graph_quality = analyze_graph_quality(repository)?;

    Ok(CheckReport {
        repository_clean,
        mechanical_validation,
        graph_quality,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::init::init_repository;

    #[test]
    fn run_check_on_clean_repo_returns_clean_with_gq_findings() {
        let dir = tempfile::tempdir().unwrap();
        let _repo = init_repository(dir.path()).unwrap();
        let opened = crate::repository::open::open_repository(dir.path()).unwrap();

        let report = run_check(&opened).unwrap();
        assert!(report.repository_clean);
        assert_eq!(report.graph_quality.total_findings, 0);
    }
}
