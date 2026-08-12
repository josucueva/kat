//! Canonical binary encoding: deterministic CBOR, SHA-256 object identity,
//! the canonical object envelope, and canonical structural validation.

pub mod cbor;
pub mod hash;
pub mod object;
pub mod validate;
