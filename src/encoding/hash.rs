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
use std::path::PathBuf;

use crate::domain::identity::{MaterializationId, ObjectId, WorkspaceSnapshotId};
use crate::encoding::cbor::canonical_bytes;
use crate::encoding::validate::CanonicalStructureError;
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
pub fn canonical_object_id(object: &CanonicalObject) -> Result<ObjectId, CanonicalStructureError> {
    canonical_bytes(object).map(|bytes| object_id(&bytes))
}

/// Computes the MaterializationId for a file (DEC-003).
pub fn hash_file_materialization(is_executable: bool, bytes: &[u8]) -> MaterializationId {
    let mut hasher = Sha256::new();
    hasher.update(b"KAT-MATERIALIZATION-FILE");
    let mode_byte = if is_executable { 1u8 } else { 0u8 };
    hasher.update(&[mode_byte]);
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    MaterializationId::from_bytes(out)
}

/// Computes the MaterializationId for a symlink (DEC-003).
pub fn hash_symlink_materialization(target_bytes: &[u8]) -> MaterializationId {
    let mut hasher = Sha256::new();
    hasher.update(b"KAT-MATERIALIZATION-SYMLINK");
    hasher.update(target_bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    MaterializationId::from_bytes(out)
}

/// Computes the MaterializationId for a directory (DEC-003).
///
/// Hashing algorithm: H(domain_tag || entry_count || ordered_entries).
/// The caller is responsible for providing strictly sorted canonical entries (by locator).
pub fn hash_directory_materialization(
    entries: &[(String, u8, MaterializationId)],
) -> MaterializationId {
    let mut hasher = Sha256::new();
    hasher.update(b"KAT-MATERIALIZATION-DIRECTORY");
    hasher.update(&(entries.len() as u32).to_le_bytes());
    
    for (locator, type_byte, id) in entries {
        let name_bytes = locator.as_bytes();
        hasher.update(&(name_bytes.len() as u32).to_le_bytes());
        hasher.update(name_bytes);
        hasher.update(&[*type_byte]);
        hasher.update(id.as_bytes());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    MaterializationId::from_bytes(out)
}

/// Computes the WorkspaceSnapshotId (DEC-002).
///
/// Hashing algorithm: H(domain_tag || entry_count || ordered_entries).
/// The caller is responsible for providing strictly sorted canonical tracked entries (by locator).
pub fn hash_workspace_snapshot(
    entries: &[(String, u8, MaterializationId)],
) -> WorkspaceSnapshotId {
    let mut hasher = Sha256::new();
    hasher.update(b"KAT-WORKSPACE-SNAPSHOT");
    hasher.update(&(entries.len() as u32).to_le_bytes());
    
    for (locator, type_byte, id) in entries {
        let name_bytes = locator.as_bytes();
        hasher.update(&(name_bytes.len() as u32).to_le_bytes());
        hasher.update(name_bytes);
        hasher.update(&[*type_byte]);
        hasher.update(id.as_bytes());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    WorkspaceSnapshotId::new(out.to_vec())
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

    #[test]
    fn materialization_hash_differentiates_type() {
        let file_id = hash_file_materialization(false, b"abc");
        let obj_id = object_id(b"abc");
        let symlink_id = hash_symlink_materialization(b"abc");
        let dir_id = hash_directory_materialization(&[]);
        
        assert_ne!(file_id.as_bytes(), obj_id.as_bytes());
        assert_ne!(file_id, symlink_id);
        assert_ne!(symlink_id.as_bytes(), obj_id.as_bytes());
        assert_ne!(file_id, dir_id);
        assert_ne!(symlink_id, dir_id);
    }

    #[test]
    fn materialization_hash_differentiates_executable_mode() {
        let file_normal = hash_file_materialization(false, b"abc");
        let file_exec = hash_file_materialization(true, b"abc");
        assert_ne!(file_normal, file_exec);
    }

    #[test]
    fn materialization_vs_workspace_snapshot_differentiates() {
        // Even with the same entries, a directory materialization != a workspace snapshot
        let file1_id = hash_file_materialization(false, b"1");
        let entries = vec![("a".to_string(), b'F', file1_id)];
        
        let dir_id = hash_directory_materialization(&entries);
        let snap_id = hash_workspace_snapshot(&entries);
        
        assert_ne!(dir_id.as_bytes(), snap_id.as_bytes());
    }

    #[test]
    fn directory_materialization_is_deterministic() {
        let file1_id = hash_file_materialization(false, b"1");
        let file2_id = hash_file_materialization(true, b"2");

        let entries1 = vec![
            ("a".to_string(), b'F', file1_id),
            ("b".to_string(), b'X', file2_id),
        ];

        let mut entries2 = entries1.clone();
        entries2.reverse(); 

        let dir1 = hash_directory_materialization(&entries1);
        let dir2 = hash_directory_materialization(&entries2);
        assert_ne!(dir1, dir2);
        
        let dir1_again = hash_directory_materialization(&entries1);
        assert_eq!(dir1, dir1_again);
    }
}
