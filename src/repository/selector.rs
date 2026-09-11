use std::fmt;

use crate::domain::identity::RepositoryRevisionId;
use crate::repository::object_store::{ObjectStore, ObjectStoreError};
use crate::repository::ref_store::{FileRefStore, RefStoreError};

/// A selector for a RepositoryRevision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevisionSelector {
    /// A full canonical 64-character hex ID.
    Full(RepositoryRevisionId),
    /// A unique prefix of a canonical ID.
    Prefix(String),
    /// A named reference or branch name.
    Named(String),
}

impl RevisionSelector {
    /// Parses a raw string into a RevisionSelector.
    pub fn parse(input: &str) -> Self {
        if let Ok(id) = input.parse::<RepositoryRevisionId>() {
            Self::Full(id)
        } else if input.chars().all(|c| c.is_ascii_hexdigit()) && input.len() >= 8 {
            Self::Prefix(input.to_string())
        } else {
            Self::Named(input.to_string())
        }
    }
}

/// Errors when resolving a revision selector.
#[derive(Debug)]
pub enum SelectorError {
    /// The prefix matched multiple revisions.
    AmbiguousPrefix(String),
    /// The revision could not be found.
    NotFound(String),
    /// The ref store encountered an error.
    RefStore(RefStoreError),
    /// The object store encountered an error.
    ObjectStore(ObjectStoreError),
}

impl fmt::Display for SelectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AmbiguousPrefix(p) => write!(f, "ambiguous prefix: {}", p),
            Self::NotFound(name) => write!(f, "revision not found for selector: {}", name),
            Self::RefStore(e) => write!(f, "ref store error: {}", e),
            Self::ObjectStore(e) => write!(f, "object store error: {}", e),
        }
    }
}

impl std::error::Error for SelectorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::RefStore(e) => Some(e),
            Self::ObjectStore(e) => Some(e),
            _ => None,
        }
    }
}

impl From<RefStoreError> for SelectorError {
    fn from(err: RefStoreError) -> Self {
        Self::RefStore(err)
    }
}

impl From<ObjectStoreError> for SelectorError {
    fn from(err: ObjectStoreError) -> Self {
        Self::ObjectStore(err)
    }
}

/// Resolves a RevisionSelector to a concrete RepositoryRevisionId.
pub fn resolve_revision(
    selector: &RevisionSelector,
    ref_store: &FileRefStore,
    object_store: &ObjectStore,
) -> Result<RepositoryRevisionId, SelectorError> {
    match selector {
        RevisionSelector::Full(id) => {
            if object_store.exists(id.as_object_id())? {
                Ok(*id)
            } else {
                Err(SelectorError::NotFound(id.to_string()))
            }
        }
        RevisionSelector::Prefix(prefix) => {
            let matches = object_store.find_by_prefix(prefix)?;
            let mut rev_matches = Vec::new();

            for object_id in matches {
                #[allow(clippy::collapsible_if)]
                if let Ok(bytes) = object_store.get(object_id) {
                    #[allow(clippy::collapsible_if)]
                    if let Ok(canonical) = crate::encoding::decode::decode_canonical(&bytes) {
                        if canonical.object_kind()
                            == crate::encoding::object::ObjectKind::RepositoryRevision
                        {
                            rev_matches.push(RepositoryRevisionId::from_object_id(object_id));
                        }
                    }
                }
            }

            if rev_matches.is_empty() {
                Err(SelectorError::NotFound(prefix.clone()))
            } else if rev_matches.len() > 1 {
                Err(SelectorError::AmbiguousPrefix(prefix.clone()))
            } else {
                Ok(rev_matches[0])
            }
        }
        RevisionSelector::Named(name) => {
            if let Ok(id) = ref_store.read_ref(&format!("local/{}", name)) {
                return Ok(id);
            }

            #[allow(clippy::collapsible_if)]
            if let Some((remote, branch)) = name.split_once('/') {
                if let Ok(id) = ref_store.read_ref(&format!("remotes/{}/{}", remote, branch)) {
                    return Ok(id);
                }
            }

            Err(SelectorError::NotFound(name.clone()))
        }
    }
}
