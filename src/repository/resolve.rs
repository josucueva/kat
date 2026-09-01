//! Type-scoped unique-prefix ID resolution layer.
//!
//! Resolves string input (either a full 36-character hyphenated UUID or a unique
//! hex prefix of at least 8 hexadecimal digits) to a canonical stable identity
//! (`ElementId` or `RelationshipId`) against the current accepted repository state.

use std::str::FromStr;


use crate::domain::identity::{ElementId, RelationshipId};
use crate::encoding::decode_canonical;
use crate::encoding::object::CanonicalPayload;
use crate::repository::open::Repository;

/// Errors returned by ID prefix resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
        InvalidIdentifier { input: String },

        PrefixTooShort { input: String },

        NotFound { input: String },

        Ambiguous {
        input: String,
        count: usize,
        candidates: Vec<String>,
        candidates_joined: String,
    },

        Repository(String),
}

/// Counts the number of hexadecimal digits (`0-9`, `a-f`, `A-F`) in `input`,
/// ignoring hyphens. Returns `Err(())` if non-hex/non-hyphen characters are present.
fn count_and_validate_hex_digits(input: &str) -> Result<usize, ()> {
    let mut count = 0;
    for ch in input.chars() {
        if ch.is_ascii_hexdigit() {
            count += 1;
        } else if ch != '-' {
            return Err(());
        }
    }
    Ok(count)
}

/// Checks whether the candidate UUID string starts with `input_prefix` (case-insensitive).
fn matches_prefix(candidate_uuid_str: &str, input_prefix: &str) -> bool {
    let cand = candidate_uuid_str.to_lowercase();
    let pref = input_prefix.to_lowercase();
    cand.starts_with(&pref)
}

use crate::domain::state::SemanticState;

/// Resolves string input to a full `ElementId` in a given `SemanticState`.
pub fn resolve_element_in_state(
    state: &SemanticState,
    input: &str,
) -> Result<ElementId, ResolveError> {
    let trimmed = input.trim();
    let hex_count =
        count_and_validate_hex_digits(trimmed).map_err(|_| ResolveError::InvalidIdentifier {
            input: trimmed.to_string(),
        })?;

    if hex_count < 8 {
        return Err(ResolveError::PrefixTooShort {
            input: trimmed.to_string(),
        });
    }

    if trimmed.len() == 36 && hex_count == 32 {
        let Ok(id) = ElementId::from_str(trimmed) else {
            return Err(ResolveError::InvalidIdentifier {
                input: trimmed.to_string(),
            });
        };
        if state
            .elements
            .binary_search_by(|e| e.element_id.cmp(&id))
            .is_ok()
        {
            return Ok(id);
        } else {
            return Err(ResolveError::NotFound {
                input: trimmed.to_string(),
            });
        }
    }

    let mut matches = Vec::new();
    for entry in &state.elements {
        let uuid_str = entry.element_id.to_string();
        if matches_prefix(&uuid_str, trimmed) {
            matches.push(entry.element_id);
        }
    }

    match matches.len() {
        0 => Err(ResolveError::NotFound {
            input: trimmed.to_string(),
        }),
        1 => Ok(matches[0]),
        count => {
            let candidates: Vec<String> = matches.iter().map(|id| id.to_string()).collect();
            let candidates_joined = candidates.join(", ");
            Err(ResolveError::Ambiguous {
                input: trimmed.to_string(),
                count,
                candidates,
                candidates_joined,
            })
        }
    }
}

/// Derives the shortest unique hex prefix (minimum length 8 hex digits) for an `ElementId` in `SemanticState`.
pub fn shortest_unique_element_prefix(state: &SemanticState, element_id: ElementId) -> String {
    let full_str = element_id.to_string();
    for len in 8..=full_str.len() {
        let prefix = &full_str[..len];
        let mut matches = 0;
        for entry in &state.elements {
            if matches_prefix(&entry.element_id.to_string(), prefix) {
                matches += 1;
                if matches > 1 {
                    break;
                }
            }
        }
        if matches == 1 {
            return prefix.to_string();
        }
    }
    full_str
}

/// Resolves string input to a full `RelationshipId` in a given `SemanticState`.
pub fn resolve_relationship_in_state(
    state: &SemanticState,
    input: &str,
) -> Result<RelationshipId, ResolveError> {
    let trimmed = input.trim();
    let hex_count =
        count_and_validate_hex_digits(trimmed).map_err(|_| ResolveError::InvalidIdentifier {
            input: trimmed.to_string(),
        })?;

    if hex_count < 8 {
        return Err(ResolveError::PrefixTooShort {
            input: trimmed.to_string(),
        });
    }

    if trimmed.len() == 36 && hex_count == 32 {
        let Ok(id) = RelationshipId::from_str(trimmed) else {
            return Err(ResolveError::InvalidIdentifier {
                input: trimmed.to_string(),
            });
        };
        if state
            .relationships
            .binary_search_by(|r| r.relationship_id.cmp(&id))
            .is_ok()
        {
            return Ok(id);
        } else {
            return Err(ResolveError::NotFound {
                input: trimmed.to_string(),
            });
        }
    }

    let mut matches = Vec::new();
    for entry in &state.relationships {
        let uuid_str = entry.relationship_id.to_string();
        if matches_prefix(&uuid_str, trimmed) {
            matches.push(entry.relationship_id);
        }
    }

    match matches.len() {
        0 => Err(ResolveError::NotFound {
            input: trimmed.to_string(),
        }),
        1 => Ok(matches[0]),
        count => {
            let candidates: Vec<String> = matches.iter().map(|id| id.to_string()).collect();
            let candidates_joined = candidates.join(", ");
            Err(ResolveError::Ambiguous {
                input: trimmed.to_string(),
                count,
                candidates,
                candidates_joined,
            })
        }
    }
}

/// Resolves string input to a full `ElementId` in the current accepted `SemanticState`.
pub fn resolve_element_id(repository: &Repository, input: &str) -> Result<ElementId, ResolveError> {
    let accepted = repository
        .ref_store()
        .read_accepted()
        .map_err(|e| ResolveError::Repository(e.to_string()))?;
    let state_bytes = repository
        .object_store()
        .get(accepted.state)
        .map_err(|e| ResolveError::Repository(e.to_string()))?;
    let state_obj =
        decode_canonical(&state_bytes).map_err(|e| ResolveError::Repository(e.to_string()))?;
    let state = match state_obj.payload {
        CanonicalPayload::SemanticState(s) => s,
        _ => unreachable!("kind checked"),
    };
    resolve_element_in_state(&state, input)
}

/// Resolves string input to a full `RelationshipId` in the current accepted `SemanticState`.
pub fn resolve_relationship_id(
    repository: &Repository,
    input: &str,
) -> Result<RelationshipId, ResolveError> {
    let accepted = repository
        .ref_store()
        .read_accepted()
        .map_err(|e| ResolveError::Repository(e.to_string()))?;
    let state_bytes = repository
        .object_store()
        .get(accepted.state)
        .map_err(|e| ResolveError::Repository(e.to_string()))?;
    let state_obj =
        decode_canonical(&state_bytes).map_err(|e| ResolveError::Repository(e.to_string()))?;
    let state = match state_obj.payload {
        CanonicalPayload::SemanticState(s) => s,
        _ => unreachable!("kind checked"),
    };
    resolve_relationship_in_state(&state, input)
}

use crate::repository::session::DraftSession;

/// Resolves a reference string (UUID, unique hex prefix, or draft workflow handle e.g. `@name`)
/// against an open draft session and candidate working state.
pub fn resolve_element_in_draft_session(
    session: &DraftSession,
    input: &str,
) -> Result<ElementId, ResolveError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ResolveError::InvalidIdentifier {
            input: trimmed.to_string(),
        });
    }

    // 1. Check draft workflow references
    let handle_with_at = if trimmed.starts_with('@') {
        trimmed.to_string()
    } else {
        format!("@{trimmed}")
    };
    for binding in &session.workflow_references {
        if binding.handle.eq_ignore_ascii_case(trimmed)
            || binding.handle.eq_ignore_ascii_case(&handle_with_at)
        {
            return Ok(binding.target_element_id);
        }
    }

    // 2. Fall back to resolving against working state
    resolve_element_in_state(&session.working_state, trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::ObjectId;
    use crate::domain::state::SemanticState;
    use crate::repository::session::{DraftSession, DraftSessionState};

    #[test]
    fn resolve_element_in_draft_session_handles_workflow_references_and_hex_prefixes() {
        let mut session = DraftSession {
            schema_version: 2,
            status: DraftSessionState::Open,
            base_state_id: ObjectId::from_bytes([1; 32]),
            base_change_id: None,
            created_at: "2026-08-15T00:00:00Z".to_string(),
            description: None,
            operations: Vec::new(),
            staged_element_versions: Vec::new(),
            staged_relationship_versions: Vec::new(),
            working_state: SemanticState {
                ontology_version: ObjectId::from_bytes([2; 32]),
                elements: Vec::new(),
                relationships: Vec::new(),
            },
            workflow_references: Vec::new(),
        };

        let elem_id = ElementId::new();
        session
            .working_state
            .elements
            .push(crate::domain::state::ElementStateEntry {
                element_id: elem_id,
                version: ObjectId::from_bytes([3; 32]),
            });
        session.bind_workflow_reference("@req-auth", elem_id);

        // Resolve with @ prefix
        assert_eq!(
            resolve_element_in_draft_session(&session, "@req-auth").unwrap(),
            elem_id
        );

        // Resolve without @ prefix
        assert_eq!(
            resolve_element_in_draft_session(&session, "req-auth").unwrap(),
            elem_id
        );

        // Resolve with full UUID string
        assert_eq!(
            resolve_element_in_draft_session(&session, &elem_id.to_string()).unwrap(),
            elem_id
        );
    }
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIdentifier { input, .. } => write!(f, "invalid identifier '{input}'"),
            Self::PrefixTooShort { input, .. } => write!(f, "identifier prefix '{input}' is too short (minimum 8 hex digits required)"),
            Self::NotFound { input, .. } => write!(f, "identifier '{input}' not found in current accepted state"),
            Self::Ambiguous { input, count, candidates: _candidates, candidates_joined, .. } => write!(f, "identifier prefix '{input}' is ambiguous ({count} matches: {candidates_joined})"),
            Self::Repository(_0) => write!(f, "repository error: {_0}"),
        }
    }
}

impl std::error::Error for ResolveError {
}
