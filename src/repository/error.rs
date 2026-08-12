//! Repository-level errors, composing the lower layers.

use std::path::PathBuf;

use crate::domain::identity::ObjectId;
use crate::encoding::decode::DecodingError;
use crate::encoding::error::EncodingError;
use crate::encoding::object::ObjectKind;
use crate::repository::metadata::MetadataError;
use crate::repository::object_store::ObjectStoreError;
use crate::repository::ref_store::RefStoreError;

/// Error produced by repository-level operations.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    /// A KAT repository already exists at the given `.kat` path.
    #[error("a KAT repository already exists at {0}")]
    AlreadyExists(PathBuf),
    /// No KAT repository exists at the given `.kat` path.
    #[error("no KAT repository found at {0}")]
    NotFound(PathBuf),
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
    /// A canonical decoding failure.
    #[error("decoding error: {0}")]
    Decoding(#[from] DecodingError),
    /// A referenced object has a different canonical kind than expected.
    #[error("expected object kind {expected}, found {actual}")]
    UnexpectedObjectKind {
        /// The canonical kind the reference required.
        expected: ObjectKind,
        /// The canonical kind the stored object actually has.
        actual: ObjectKind,
    },
    /// The accepted ChangeRevision's result state does not match the
    /// accepted SemanticState (the repository head is internally inconsistent).
    #[error(
        "accepted change {change} results in state {actual}, but the accepted state is {expected}"
    )]
    AcceptedChangeStateMismatch {
        /// ObjectId of the accepted ChangeRevision.
        change: ObjectId,
        /// ObjectId of the accepted SemanticState.
        expected: ObjectId,
        /// ObjectId the ChangeRevision actually results in.
        actual: ObjectId,
    },
    /// An underlying filesystem failure.
    #[error("repository I/O error: {0}")]
    Io(#[from] std::io::Error),
}
