//! Repository-level errors, composing the lower layers.

use std::path::PathBuf;

use crate::encoding::error::EncodingError;
use crate::repository::metadata::MetadataError;
use crate::repository::object_store::ObjectStoreError;
use crate::repository::ref_store::RefStoreError;

/// Error produced by repository-level operations.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    /// A KAT repository already exists at the given `.kat` path.
    #[error("a KAT repository already exists at {0}")]
    AlreadyExists(PathBuf),
    /// A repository metadata failure.
    #[error("repository metadata error: {0}")]
    Metadata(#[from] MetadataError),
    /// An object store failure.
    #[error("object store error: {0}")]
    ObjectStore(#[from] ObjectStoreError),
    /// A ref store failure.
    #[error("ref store error: {0}")]
    RefStore(#[from] RefStoreError),
    /// A canonical encoding failure.
    #[error("encoding error: {0}")]
    Encoding(#[from] EncodingError),
    /// An underlying filesystem failure.
    #[error("repository I/O error: {0}")]
    Io(#[from] std::io::Error),
}
