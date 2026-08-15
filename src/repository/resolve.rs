//! Type-scoped unique-prefix ID resolution layer.
//!
//! Resolves string input (either a full 36-character hyphenated UUID or a unique
//! hex prefix of at least 8 hexadecimal digits) to a canonical stable identity
//! (`ElementId` or `RelationshipId`) against the current accepted repository state.

use std::str::FromStr;
use thiserror::Error;

use crate::domain::identity::{ElementId, RelationshipId};
use crate::encoding::decode_canonical;
use crate::encoding::object::CanonicalPayload;
use crate::repository::open::Repository;
use crate::repository::ref_store::RefStore;

/// Errors returned by ID prefix resolution.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResolveError {
    #[error("invalid identifier '{input}'")]
    InvalidIdentifier { input: String },

    #[error("identifier prefix '{input}' is too short (minimum 8 hex digits required)")]
    PrefixTooShort { input: String },

    #[error("identifier '{input}' not found in current accepted state")]
    NotFound { input: String },

    #[error("identifier prefix '{input}' is ambiguous ({count} matches: {candidates_joined})")]
    Ambiguous {
        input: String,
        count: usize,
        candidates: Vec<String>,
        candidates_joined: String,
    },

    #[error("repository error: {0}")]
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

/// Resolves string input to a full `ElementId` in the current accepted `SemanticState`.
pub fn resolve_element_id(repository: &Repository, input: &str) -> Result<ElementId, ResolveError> {
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

    // Fast path: exact full 36-char UUID parse
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

/// Resolves string input to a full `RelationshipId` in the current accepted `SemanticState`.
pub fn resolve_relationship_id(
    repository: &Repository,
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

    // Fast path: exact full 36-char UUID parse
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
