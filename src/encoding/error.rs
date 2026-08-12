//! Encoding errors (step 0.4).
//!
//! Only the variants actually reachable with the current in-memory writer are
//! defined. Additional categories (`WriterFailure`, `IntegerOutOfRange`,
//! `LengthOutOfRange`, ...) are added when their corresponding failure modes
//! exist; creating unreachable variants now would fail `-D warnings`.

use crate::encoding::validate::CanonicalStructureError;

/// Error produced while encoding canonical objects.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodingError {
    /// The object is not structurally canonical and was refused.
    #[error("cannot encode structurally non-canonical object: {0}")]
    InvalidCanonicalStructure(CanonicalStructureError),
}
