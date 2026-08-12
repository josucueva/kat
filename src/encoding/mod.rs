//! Canonical binary encoding: deterministic CBOR, SHA-256 object identity,
//! the canonical object envelope, and canonical structural validation.

pub mod cbor;
pub mod error;
pub mod hash;
pub mod object;
pub mod validate;

pub use cbor::canonical_bytes;
pub use error::EncodingError;
pub use hash::{canonical_object_id, object_id};
