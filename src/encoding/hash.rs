//! SHA-256 object identity.
//!
//! [`object_id`] derives the immutable ObjectId of exact canonical bytes:
//!
//! ```text
//! ObjectId = SHA-256(exact canonical bytes)
//! ```
//!
//! This function performs no re-encoding, normalization, object-kind logic,
//! or filesystem behavior. `ObjectId` is always *derived*, never generated;
//! the object store hashes bytes it already holds.

use sha2::{Digest, Sha256};

use crate::domain::identity::ObjectId;
use crate::encoding::cbor::canonical_bytes;
use crate::encoding::error::EncodingError;
use crate::encoding::object::CanonicalObject;

/// Computes the ObjectId (SHA-256) of exact canonical bytes.
pub fn object_id(bytes: &[u8]) -> ObjectId {
    let digest = Sha256::digest(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    ObjectId::from_bytes(out)
}

/// Computes the ObjectId of a canonical object: encode, then hash.
///
/// Kept as the composition of [`canonical_bytes`] and [`object_id`] so the
/// primitives stay separable (the object store will hash already-encoded
/// bytes without going through an object).
pub fn canonical_object_id(object: &CanonicalObject) -> Result<ObjectId, EncodingError> {
    canonical_bytes(object).map(|bytes| object_id(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::element::{KnowledgeElementVersion, Lifecycle};
    use crate::domain::identity::ElementId;
    use crate::encoding::object::{CanonicalObject, CanonicalPayload};
    use uuid::Uuid;

    /// SHA-256 of the empty byte sequence (standard implementation sanity
    /// fixture; the KAT vectors remain the authoritative protocol tests).
    #[test]
    fn empty_bytes_hash_is_known_sha256() {
        assert_eq!(
            object_id(b"").to_string(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    /// SHA-256 of "abc" (standard NIST test vector).
    #[test]
    fn known_bytes_hash_is_known_sha256() {
        assert_eq!(
            object_id(b"abc").to_string(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn same_bytes_same_object_id() {
        let bytes = [0x01u8, 0x02, 0x03];
        assert_eq!(object_id(&bytes), object_id(&bytes));
    }

    #[test]
    fn one_byte_difference_changes_object_id() {
        assert_ne!(object_id(b"abc"), object_id(b"abd"));
    }

    #[test]
    fn canonical_object_id_equals_encode_then_hash() {
        let object = CanonicalObject {
            payload: CanonicalPayload::KnowledgeElementVersion(KnowledgeElementVersion {
                element_id: ElementId::from_uuid(Uuid::new_v4()),
                type_id: "kat.core/requirement".into(),
                lifecycle: Lifecycle::Active,
                properties: vec![],
            }),
        };
        let bytes = canonical_bytes(&object).unwrap();
        assert_eq!(canonical_object_id(&object).unwrap(), object_id(&bytes));
    }
}
