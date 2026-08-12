//! Canonical binary encoding: deterministic CBOR, SHA-256 object identity,
//! the canonical object envelope, canonical structural validation, and the
//! strict canonical decoder (step 0.10).

pub mod cbor;
pub mod decode;
pub mod error;
pub mod hash;
pub mod object;
pub mod validate;

pub use cbor::canonical_bytes;
pub use decode::{DecodingError, decode_canonical};
pub use error::EncodingError;
pub use hash::{canonical_object_id, object_id};
