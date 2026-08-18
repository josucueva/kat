//! Advisory Graph Quality Diagnostics (GQ-01 to GQ-04) and Aggregated Check Porcelain.

use crate::domain::identity::ElementId;
use crate::domain::property::PropertyValue;
use crate::repository::open::Repository;
use crate::repository::query::{
    ArtifactAccountabilityReport, QueryError, analyze_artifact_accountability, list_elements,
    show_element,
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
    /// Target element title (if present).
    pub target_title: Option<String>,
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
    /// Artifact accountability report (if available).
    pub artifact_accountability: Option<ArtifactAccountabilityReport>,
}

fn element_title(ev: &crate::domain::element::KnowledgeElementVersion) -> &str {
    for (k, v) in &ev.properties {
        if k.as_str() == "title"
            && let PropertyValue::Text(t) = v
        {
            return t.as_str();
        }
    }
    "<untitled>"
}

/// Runs advisory graph quality rule checks against the accepted repository state.
pub fn analyze_graph_quality(repository: &Repository) -> Result<GraphQualityReport, QueryError> {
    let elements = list_elements(repository, Default::default())?;
    let mut findings = Vec::new();

    for elem_summary in &elements {
        let view = show_element(repository, elem_summary.element_id)?;
        let total_rels = view.relationships.incoming.len() + view.relationships.outgoing.len();
        let title = element_title(&view.element).to_string();

        // GQ-01: Isolated Unlinked Element
        if total_rels == 0 {
            findings.push(GraphQualityFinding {
                rule_id: "GQ-01".to_string(),
                severity: QualitySeverity::Advisory,
                target_element_id: Some(elem_summary.element_id),
                target_title: Some(title.clone()),
                message: "has no incoming or outgoing relationships".to_string(),
                details: vec!["Consider linking this element to upstream requirements or downstream realizations".to_string()],
            });
        }

        // GQ-02: Requirement Lacks Realization Path
        let type_id = view.element.type_id.as_str();
        if type_id == "kat.core/requirement"
            || type_id == "kat.core/user-story"
            || type_id == "kat.core/use-case"
            || type_id == "kat.core/goal"
            || type_id == "requirement"
        {
            let has_realization = view
                .relationships
                .incoming
                .iter()
                .any(|rel| rel.relationship_type_id == "kat.core/realizes" || rel.relationship_type_id == "realizes");
            if !has_realization {
                findings.push(GraphQualityFinding {
                    rule_id: "GQ-02".to_string(),
                    severity: QualitySeverity::Advisory,
                    target_element_id: Some(elem_summary.element_id),
                    target_title: Some(title.clone()),
                    message: "has no active incoming realization path via kat.core/realizes".to_string(),
                    details: vec!["Link an implementation element to this requirement using 'kat link --type kat.core/realizes'".to_string()],
                });
            }
        }

        // GQ-03: Implementation Without Artifact Representation Route
        if type_id == "kat.core/implementation"
            || type_id == "kat.core/code"
            || type_id == "kat.core/service"
            || type_id == "kat.core/module"
            || type_id == "implementation"
        {
            let has_artifact_route = view
                .relationships
                .incoming
                .iter()
                .any(|rel| rel.relationship_type_id == "kat.core/represents" || rel.relationship_type_id == "represents");
            if !has_artifact_route {
                findings.push(GraphQualityFinding {
                    rule_id: "GQ-03".to_string(),
                    severity: QualitySeverity::Advisory,
                    target_element_id: Some(elem_summary.element_id),
                    target_title: Some(title.clone()),
                    message: "no modeled Artifact representation route".to_string(),
                    details: vec!["Link an artifact element to this implementation using 'kat link --type kat.core/represents'".to_string()],
                });
            }
        }

        // GQ-04: Design Decision Without Consequence Route
        if type_id == "kat.core/design-decision"
            || type_id == "kat.core/architecture"
            || type_id == "kat.core/decision"
            || type_id == "kat.core/model"
            || type_id == "design-decision"
        {
            let has_consequence = view
                .relationships
                .outgoing
                .iter()
                .any(|rel| {
                    rel.relationship_type_id == "kat.core/addresses"
                        || rel.relationship_type_id == "addresses"
                        || rel.relationship_type_id == "kat.core/guides"
                        || rel.relationship_type_id == "guides"
                });
            if !has_consequence {
                findings.push(GraphQualityFinding {
                    rule_id: "GQ-04".to_string(),
                    severity: QualitySeverity::Advisory,
                    target_element_id: Some(elem_summary.element_id),
                    target_title: Some(title.clone()),
                    message: "no consequence route through addresses or guides".to_string(),
                    details: vec![
                        "Link this design decision to a requirement using kat.core/addresses or to an implementation using kat.core/guides".to_string(),
                    ],
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
    let artifact_accountability = analyze_artifact_accountability(repository).ok();

    Ok(CheckReport {
        repository_clean,
        mechanical_validation,
        graph_quality,
        artifact_accountability,
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
