//! Repository-level errors, composing the lower layers.

use std::path::PathBuf;

use crate::domain::identity::ObjectId;
use crate::encoding::decode::DecodingError;
use crate::encoding::validate::CanonicalStructureError;
use crate::encoding::object::ObjectKind;
use crate::repository::metadata::MetadataError;
use crate::repository::object_store::ObjectStoreError;
use crate::repository::ref_store::RefStoreError;

/// Error produced by repository-level operations.
#[derive(Debug)]
pub enum RepositoryError {
    /// A KAT repository already exists at the given `.kat` path.
        AlreadyExists(PathBuf),
    /// No KAT repository exists at the given `.kat` path.
        NotFound(PathBuf),
    /// A repository metadata failure.
        Metadata(MetadataError),
    /// An object store failure.
        ObjectStore(ObjectStoreError),
    /// A ref store failure.
        RefStore(RefStoreError),
    /// A canonical encoding failure.
        Encoding(CanonicalStructureError),
    /// A canonical decoding failure.
        Decoding(DecodingError),
    /// A referenced object has a different canonical kind than expected.
        UnexpectedObjectKind {
        /// The canonical kind the reference required.
        expected: ObjectKind,
        /// The canonical kind the stored object actually has.
        actual: ObjectKind,
    },
    /// The accepted ChangeRevision's result state does not match the
    /// accepted SemanticState (the repository head is internally inconsistent).
        AcceptedChangeStateMismatch {
        /// ObjectId of the accepted ChangeRevision.
        change: ObjectId,
        /// ObjectId of the accepted SemanticState.
        expected: ObjectId,
        /// ObjectId the ChangeRevision actually results in.
        actual: ObjectId,
    },
    /// An underlying filesystem failure.
        Io(std::io::Error),
}

impl std::fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyExists(_0) => write!(f, "a KAT repository already exists at {}", _0.display()),
            Self::NotFound(_0) => write!(f, "no KAT repository found at {}", _0.display()),
            Self::Metadata(_0) => write!(f, "repository metadata error: {_0}"),
            Self::ObjectStore(_0) => write!(f, "object store error: {_0}"),
            Self::RefStore(_0) => write!(f, "ref store error: {_0}"),
            Self::Encoding(_0) => write!(f, "encoding error: {_0}"),
            Self::Decoding(_0) => write!(f, "decoding error: {_0}"),
            Self::UnexpectedObjectKind { expected, actual, .. } => write!(f, "expected object kind {expected}, found {actual}"),
            Self::AcceptedChangeStateMismatch { change, expected, actual, .. } => write!(f, "accepted change {change} results in state {actual}, but the accepted state is {expected}"),
            Self::Io(_0) => write!(f, "repository I/O error: {_0}"),
        }
    }
}

impl std::error::Error for RepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Metadata(err) => Some(err),
            Self::ObjectStore(err) => Some(err),
            Self::RefStore(err) => Some(err),
            Self::Encoding(err) => Some(err),
            Self::Decoding(err) => Some(err),
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<MetadataError> for RepositoryError {
    fn from(err: MetadataError) -> Self {
        Self::Metadata(err)
    }
}
impl From<ObjectStoreError> for RepositoryError {
    fn from(err: ObjectStoreError) -> Self {
        Self::ObjectStore(err)
    }
}
impl From<RefStoreError> for RepositoryError {
    fn from(err: RefStoreError) -> Self {
        Self::RefStore(err)
    }
}
impl From<CanonicalStructureError> for RepositoryError {
    fn from(err: CanonicalStructureError) -> Self {
        Self::Encoding(err)
    }
}
impl From<DecodingError> for RepositoryError {
    fn from(err: DecodingError) -> Self {
        Self::Decoding(err)
    }
}
impl From<std::io::Error> for RepositoryError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}