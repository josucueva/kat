use crate::domain::identity::RepositoryRevisionId;

/// A human-readable name pointing to one accepted `RepositoryRevision`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedReference {
    pub name: String,
    pub target: RepositoryRevisionId,
}

/// A remote-observed reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteReference {
    pub remote: String,
    pub name: String,
    pub target: RepositoryRevisionId,
}
