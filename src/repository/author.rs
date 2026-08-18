#![allow(clippy::unnecessary_lazy_evaluations)]

//! Declarative Authoring Compiler (`kat author`).
//!
//! NORMATIVE ARCHITECTURAL GUARDRAIL:
//! `author` deterministically compiles explicit user/agent claims into dependency-valid
//! canonical KAT mutation sequences. It NEVER uses LLM/probabilistic guessing or inference
//! to invent relationships or properties.

use std::collections::HashMap;

use crate::domain::identity::ElementId;
use crate::domain::property::PropertyValue;
use crate::repository::change::{
    AccountArtifactInput, ChangeError, CreateElementInput, DeprecateElementInput, LinkElementInput,
    StagedOperationInput, SupersedeElementInput, UnlinkElementInput, UpdateElementInput,
    stage_batch_operations_into_session,
};
use crate::repository::open::Repository;
use crate::repository::resolve::{resolve_element_in_draft_session, resolve_relationship_id};
use crate::repository::session::{begin_draft_session, has_draft_session, read_draft_session};

/// Declarative claim supported by the authoring compiler.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthorClaim {
    /// Create a new knowledge element.
    #[serde(alias = "CreateElement")]
    CreateElement {
        /// Ontology type ID (e.g. `kat.core/requirement`).
        #[serde(alias = "type")]
        type_id: String,
        /// Title property.
        title: String,
        /// Optional description property.
        #[serde(default)]
        description: Option<String>,
        /// Optional workflow reference handle (e.g. `@req-auth`).
        #[serde(default)]
        handle: Option<String>,
    },
    /// Link two knowledge elements via relationship.
    #[serde(alias = "LinkElement")]
    LinkElement {
        /// Source element reference (UUID, prefix, or `@handle`).
        source_ref: String,
        /// Relationship type ID (e.g. `kat.core/realizes`).
        #[serde(alias = "relationship_type")]
        relationship_type_id: String,
        /// Target element reference (UUID, prefix, or `@handle`).
        target_ref: String,
    },
    /// Unlink an existing relationship.
    #[serde(alias = "UnlinkElement")]
    UnlinkElement {
        /// Relationship reference.
        relationship_ref: String,
    },
    /// Re-baseline artifact accountability.
    #[serde(alias = "AccountArtifact")]
    AccountArtifact {
        /// Relative path to artifact file.
        artifact_path: String,
        /// Accountable element reference.
        element_ref: String,
    },
    /// Patch properties on an existing element.
    #[serde(alias = "UpdateElement")]
    UpdateElement {
        /// Target element reference.
        element_ref: String,
        /// New title property value.
        #[serde(default)]
        title: Option<String>,
        /// New description property value.
        #[serde(default)]
        description: Option<String>,
    },
    /// Deprecate an active element.
    #[serde(alias = "DeprecateElement")]
    DeprecateElement {
        /// Target element reference.
        element_ref: String,
    },
    /// Supersede an existing element with a replacement element.
    #[serde(alias = "SupersedeElement")]
    SupersedeElement {
        /// Existing element reference to supersede.
        existing_ref: String,
        /// Replacement element ontology type ID.
        #[serde(alias = "replacement_type")]
        replacement_type_id: String,
        /// Replacement element title.
        replacement_title: String,
        /// Optional workflow handle for replacement element.
        #[serde(default)]
        handle: Option<String>,
    },
}

/// Legacy externally-tagged DTO for backward compatibility with v0.4.0 JSON shapes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub enum AuthorClaimLegacy {
    CreateElement {
        #[serde(alias = "type")]
        type_id: String,
        title: String,
        description: Option<String>,
        handle: Option<String>,
    },
    LinkElement {
        source_ref: String,
        #[serde(alias = "relationship_type")]
        relationship_type_id: String,
        target_ref: String,
    },
    UnlinkElement {
        relationship_ref: String,
    },
    AccountArtifact {
        artifact_path: String,
        element_ref: String,
    },
    UpdateElement {
        element_ref: String,
        title: Option<String>,
        description: Option<String>,
    },
    DeprecateElement {
        element_ref: String,
    },
    SupersedeElement {
        existing_ref: String,
        #[serde(alias = "replacement_type")]
        replacement_type_id: String,
        replacement_title: String,
        handle: Option<String>,
    },
}

impl From<AuthorClaimLegacy> for AuthorClaim {
    fn from(legacy: AuthorClaimLegacy) -> Self {
        match legacy {
            AuthorClaimLegacy::CreateElement {
                type_id,
                title,
                description,
                handle,
            } => AuthorClaim::CreateElement {
                type_id,
                title,
                description,
                handle,
            },
            AuthorClaimLegacy::LinkElement {
                source_ref,
                relationship_type_id,
                target_ref,
            } => AuthorClaim::LinkElement {
                source_ref,
                relationship_type_id,
                target_ref,
            },
            AuthorClaimLegacy::UnlinkElement { relationship_ref } => {
                AuthorClaim::UnlinkElement { relationship_ref }
            }
            AuthorClaimLegacy::AccountArtifact {
                artifact_path,
                element_ref,
            } => AuthorClaim::AccountArtifact {
                artifact_path,
                element_ref,
            },
            AuthorClaimLegacy::UpdateElement {
                element_ref,
                title,
                description,
            } => AuthorClaim::UpdateElement {
                element_ref,
                title,
                description,
            },
            AuthorClaimLegacy::DeprecateElement { element_ref } => {
                AuthorClaim::DeprecateElement { element_ref }
            }
            AuthorClaimLegacy::SupersedeElement {
                existing_ref,
                replacement_type_id,
                replacement_title,
                handle,
            } => AuthorClaim::SupersedeElement {
                existing_ref,
                replacement_type_id,
                replacement_title,
                handle,
            },
        }
    }
}

/// Parses a JSON payload of authoring claims, attempting normative internally-tagged `"kind"`
/// format first, and falling back to legacy v0.4.0 externally-tagged shapes if needed.
pub fn parse_author_claims_json(text: &str) -> Result<Vec<AuthorClaim>, serde_json::Error> {
    match serde_json::from_str::<Vec<AuthorClaim>>(text) {
        Ok(claims) => Ok(claims),
        Err(normative_err) => {
            if let Ok(legacy_claims) = serde_json::from_str::<Vec<AuthorClaimLegacy>>(text) {
                Ok(legacy_claims.into_iter().map(AuthorClaim::from).collect())
            } else {
                Err(normative_err)
            }
        }
    }
}

/// Result produced by compiling and staging authoring claims.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuthorBatchResult {
    /// Count of claims processed.
    pub claims_processed: usize,
    /// Count of primitive operations staged.
    pub operations_staged: usize,
    /// Workflow reference handle bindings created or updated during compilation.
    pub workflow_references: HashMap<String, ElementId>,
}

/// Compiles and stages a batch of authoring claims into a draft session.
pub fn compile_and_stage_claims(
    repository: &Repository,
    claims: &[AuthorClaim],
) -> Result<AuthorBatchResult, ChangeError> {
    let root = repository.root_dir();
    let mut session = if has_draft_session(root) {
        read_draft_session(root).map_err(|e| {
            ChangeError::RefStore(crate::repository::ref_store::RefStoreError::Parse(
                e.to_string(),
            ))
        })?
    } else {
        begin_draft_session(repository, Some("porcelain authoring batch".to_string())).map_err(
            |e| {
                ChangeError::RefStore(crate::repository::ref_store::RefStoreError::Parse(
                    e.to_string(),
                ))
            },
        )?
    };

    let mut staged_inputs = Vec::new();
    let mut created_handles = HashMap::new();
    let mut auto_handle_counter = 1;

    for claim in claims {
        match claim {
            AuthorClaim::CreateElement {
                type_id,
                title,
                description,
                handle,
            } => {
                let element_id = ElementId::new();
                let handle_str = match handle {
                    Some(h) => {
                        let formatted = if h.starts_with('@') {
                            h.clone()
                        } else {
                            format!("@{h}")
                        };
                        if created_handles.contains_key(&formatted)
                            || session
                                .workflow_references
                                .iter()
                                .any(|b| b.handle == formatted)
                        {
                            return Err(ChangeError::RefStore(
                                crate::repository::ref_store::RefStoreError::Parse(format!(
                                    "duplicate workflow reference handle: {formatted}"
                                )),
                            ));
                        }
                        formatted
                    }
                    None => {
                        let auto_h = format!("@elem-{auto_handle_counter}");
                        auto_handle_counter += 1;
                        auto_h
                    }
                };

                session.bind_workflow_reference(&handle_str, element_id);
                created_handles.insert(handle_str, element_id);

                let mut properties =
                    vec![("title".to_string(), PropertyValue::Text(title.clone()))];
                if let Some(desc) = description {
                    properties.push(("description".to_string(), PropertyValue::Text(desc.clone())));
                }

                staged_inputs.push(StagedOperationInput::CreateElement(CreateElementInput {
                    element_id,
                    type_id: type_id.clone(),
                    properties,
                }));
            }
            AuthorClaim::LinkElement {
                source_ref,
                relationship_type_id,
                target_ref,
            } => {
                let source_element_id = resolve_element_in_draft_session(&session, source_ref)
                    .map_err(|e| {
                        ChangeError::RefStore(crate::repository::ref_store::RefStoreError::Parse(
                            e.to_string(),
                        ))
                    })?;
                let target_element_id = resolve_element_in_draft_session(&session, target_ref)
                    .map_err(|e| {
                        ChangeError::RefStore(crate::repository::ref_store::RefStoreError::Parse(
                            e.to_string(),
                        ))
                    })?;

                let relationship_id = crate::domain::identity::RelationshipId::new();
                staged_inputs.push(StagedOperationInput::LinkElement(LinkElementInput {
                    relationship_id,
                    relationship_type_id: relationship_type_id.clone(),
                    source_element_id,
                    target_element_id,
                    properties: Vec::new(),
                }));
            }
            AuthorClaim::UnlinkElement { relationship_ref } => {
                let relationship_id = resolve_relationship_id(repository, relationship_ref)
                    .map_err(|e| {
                        ChangeError::RefStore(crate::repository::ref_store::RefStoreError::Parse(
                            e.to_string(),
                        ))
                    })?;
                let rel_entry = session
                    .working_state
                    .relationships
                    .iter()
                    .find(|r| r.relationship_id == relationship_id)
                    .ok_or_else(|| {
                        ChangeError::Precondition(
                            crate::repository::change::PreconditionError::RelationshipAlreadyExists(
                                relationship_id,
                            ),
                        )
                    })?;

                staged_inputs.push(StagedOperationInput::UnlinkElement(UnlinkElementInput {
                    relationship_id,
                    expected_version: rel_entry.version,
                }));
            }
            AuthorClaim::AccountArtifact {
                artifact_path: _,
                element_ref,
            } => {
                let artifact_id =
                    resolve_element_in_draft_session(&session, element_ref).map_err(|e| {
                        ChangeError::RefStore(crate::repository::ref_store::RefStoreError::Parse(
                            e.to_string(),
                        ))
                    })?;

                staged_inputs.push(StagedOperationInput::AccountArtifact(
                    AccountArtifactInput { artifact_id },
                ));
            }
            AuthorClaim::UpdateElement {
                element_ref,
                title,
                description,
            } => {
                let element_id =
                    resolve_element_in_draft_session(&session, element_ref).map_err(|e| {
                        ChangeError::RefStore(crate::repository::ref_store::RefStoreError::Parse(
                            e.to_string(),
                        ))
                    })?;
                let elem_entry = session
                    .working_state
                    .elements
                    .iter()
                    .find(|e| e.element_id == element_id)
                    .ok_or_else(|| {
                        ChangeError::Precondition(
                            crate::repository::change::PreconditionError::ElementNotFound(
                                element_id,
                            ),
                        )
                    })?;

                let mut properties = Vec::new();
                if let Some(t) = title {
                    properties.push(("title".to_string(), PropertyValue::Text(t.clone())));
                }
                if let Some(d) = description {
                    properties.push(("description".to_string(), PropertyValue::Text(d.clone())));
                }

                staged_inputs.push(StagedOperationInput::UpdateElement(UpdateElementInput {
                    element_id,
                    expected_version: elem_entry.version,
                    properties,
                }));
            }
            AuthorClaim::DeprecateElement { element_ref } => {
                let element_id =
                    resolve_element_in_draft_session(&session, element_ref).map_err(|e| {
                        ChangeError::RefStore(crate::repository::ref_store::RefStoreError::Parse(
                            e.to_string(),
                        ))
                    })?;
                let elem_entry = session
                    .working_state
                    .elements
                    .iter()
                    .find(|e| e.element_id == element_id)
                    .ok_or_else(|| {
                        ChangeError::Precondition(
                            crate::repository::change::PreconditionError::ElementNotFound(
                                element_id,
                            ),
                        )
                    })?;

                staged_inputs.push(StagedOperationInput::DeprecateElement(
                    DeprecateElementInput {
                        element_id,
                        expected_version: elem_entry.version,
                    },
                ));
            }
            AuthorClaim::SupersedeElement {
                existing_ref,
                replacement_type_id,
                replacement_title,
                handle,
            } => {
                let existing_element_id = resolve_element_in_draft_session(&session, existing_ref)
                    .map_err(|e| {
                        ChangeError::RefStore(crate::repository::ref_store::RefStoreError::Parse(
                            e.to_string(),
                        ))
                    })?;
                let existing_version = session
                    .working_state
                    .elements
                    .iter()
                    .find(|e| e.element_id == existing_element_id)
                    .ok_or_else(|| {
                        ChangeError::Precondition(
                            crate::repository::change::PreconditionError::ElementNotFound(
                                existing_element_id,
                            ),
                        )
                    })?
                    .version;

                let replacement_element_id = ElementId::new();
                if let Some(h) = handle {
                    let norm = if h.starts_with('@') {
                        h.clone()
                    } else {
                        format!("@{h}")
                    };
                    session.bind_workflow_reference(&norm, replacement_element_id);
                    created_handles.insert(norm, replacement_element_id);
                }

                let relationship_id = crate::domain::identity::RelationshipId::new();
                let properties = vec![(
                    "title".to_string(),
                    PropertyValue::Text(replacement_title.clone()),
                )];

                staged_inputs.push(StagedOperationInput::SupersedeElement(
                    SupersedeElementInput {
                        existing_element_id,
                        expected_existing_version: existing_version,
                        replacement_element_id,
                        replacement_type_id: replacement_type_id.clone(),
                        replacement_properties: properties,
                        relationship_id,
                    },
                ));
            }
        }
    }

    let (staged_ops, _updated_session) =
        stage_batch_operations_into_session(repository, staged_inputs)?;

    Ok(AuthorBatchResult {
        claims_processed: claims.len(),
        operations_staged: staged_ops.len(),
        workflow_references: created_handles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::init::init_repository;
    use crate::repository::open::open_repository;

    #[test]
    fn compile_and_stage_claims_creates_elements_and_links_with_handles() {
        let temp = tempfile::tempdir().unwrap();
        init_repository(temp.path()).unwrap();
        let repository = open_repository(temp.path()).unwrap();

        let claims = vec![
            AuthorClaim::CreateElement {
                type_id: "kat.core/requirement".to_string(),
                title: "Auth Req".to_string(),
                description: Some("Must support JWT".to_string()),
                handle: Some("@req-auth".to_string()),
            },
            AuthorClaim::CreateElement {
                type_id: "kat.core/implementation".to_string(),
                title: "Auth Module".to_string(),
                description: None,
                handle: Some("@imp-auth".to_string()),
            },
            AuthorClaim::LinkElement {
                source_ref: "@imp-auth".to_string(),
                relationship_type_id: "kat.core/realizes".to_string(),
                target_ref: "@req-auth".to_string(),
            },
        ];

        let result = compile_and_stage_claims(&repository, &claims).unwrap();
        assert_eq!(result.claims_processed, 3);
        assert_eq!(result.operations_staged, 3);
        assert!(result.workflow_references.contains_key("@req-auth"));
        assert!(result.workflow_references.contains_key("@imp-auth"));
    }
}
