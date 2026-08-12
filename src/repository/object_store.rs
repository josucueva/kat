//! Immutable content-addressed object store: `.kat/objects/<ObjectId>`.
//!
//! The store holds **bytes**, never `CanonicalObject`s. A higher layer later
//! composes `canonical_bytes(object)` + `put(&bytes)`; this module has no CBOR
//! or repository-metadata knowledge.
//!
//! Semantics:
//!
//! * `put` derives the ObjectId from the bytes and **never overwrites** an
//!   existing object. If the destination already exists it verifies the
//!   existing bytes; a mismatch is an `Integrity` error. Concurrent writers of
//!   the same bytes are harmless (one canonical object, both succeed).
//! * `get` reads the object and verifies its bytes hash to the requested
//!   ObjectId (integrity on read for a single object).
//! * `exists` is purely physical: it checks whether `objects/<id>` exists
//!   without reading or hashing.
//!
//! Layout (v0.1): flat `objects/<64 lowercase hex>`, no fan-out, packing,
//! or garbage collection.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::domain::identity::ObjectId;
use crate::encoding::hash::object_id;

/// Process-global counter for unique temporary file names.
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// Error produced by the object store.
#[derive(Debug, thiserror::Error)]
pub enum ObjectStoreError {
    /// The requested object is absent from the store.
    #[error("object not found: {0}")]
    NotFound(ObjectId),
    /// An object's bytes do not hash to the ObjectId they are stored under.
    #[error("object integrity mismatch: expected {expected}, actual {actual}")]
    Integrity {
        /// The ObjectId the object was expected to have.
        expected: ObjectId,
        /// The ObjectId its actual bytes hash to.
        actual: ObjectId,
    },
    /// An underlying filesystem failure.
    #[error("object store I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Immutable content-addressed object store rooted at a `.kat` directory.
#[derive(Debug)]
pub struct ObjectStore {
    root: PathBuf,
}

impl ObjectStore {
    /// Creates a store rooted at `root` (the `.kat` directory). No filesystem
    /// side effects occur until an operation touches it.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    /// The canonical destination path for an ObjectId.
    fn path_for(&self, id: ObjectId) -> PathBuf {
        self.objects_dir().join(id.to_string())
    }

    /// Stores `bytes`, returning their content-derived ObjectId.
    ///
    /// If the object already exists its bytes are verified (never overwritten);
    /// a concurrent writer publishing the same bytes is harmless.
    pub fn put(&self, bytes: &[u8]) -> Result<ObjectId, ObjectStoreError> {
        let id = object_id(bytes);
        let dest = self.path_for(id);

        // Already present: verify and return without rewriting.
        if dest.exists() {
            let actual = object_id(&fs::read(&dest)?);
            return if actual == id {
                Ok(id)
            } else {
                Err(ObjectStoreError::Integrity {
                    expected: id,
                    actual,
                })
            };
        }

        // Write to a unique temporary file, flush it, then publish atomically.
        fs::create_dir_all(self.objects_dir())?;
        fs::create_dir_all(self.tmp_dir())?;

        let tmp_path = self.tmp_dir().join(format!(
            "put-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));

        {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }

        // A concurrent writer may have published the same object first; a
        // failed rename (or an overwrite with identical bytes) is harmless.
        if fs::rename(&tmp_path, &dest).is_err() {
            let _ = fs::remove_file(&tmp_path);
        }

        // Verify the published object (or the concurrently published one).
        let actual = object_id(&fs::read(&dest)?);
        if actual != id {
            return Err(ObjectStoreError::Integrity {
                expected: id,
                actual,
            });
        }
        Ok(id)
    }

    /// Reads an object's bytes, verifying they hash to `id`.
    pub fn get(&self, id: ObjectId) -> Result<Vec<u8>, ObjectStoreError> {
        let path = self.path_for(id);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ObjectStoreError::NotFound(id));
            }
            Err(e) => return Err(ObjectStoreError::Io(e)),
        };
        let actual = object_id(&bytes);
        if actual != id {
            return Err(ObjectStoreError::Integrity {
                expected: id,
                actual,
            });
        }
        Ok(bytes)
    }

    /// Checks whether `objects/<id>` exists, without reading or hashing it.
    pub fn exists(&self, id: ObjectId) -> Result<bool, ObjectStoreError> {
        Ok(self.path_for(id).try_exists()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    /// Creates an ObjectStore rooted at a fresh auto-cleaned temp directory.
    fn store() -> (tempfile::TempDir, ObjectStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(dir.path());
        (dir, store)
    }

    #[test]
    fn put_returns_correct_object_id() {
        let (_dir, store) = store();
        let bytes = b"hello canonical object";
        let id = object_id(bytes);
        assert_eq!(store.put(bytes).unwrap(), id);
    }

    #[test]
    fn put_stores_under_exact_lowercase_path() {
        let (dir, store) = store();
        let bytes = b"content";
        let id = store.put(bytes).unwrap();
        let path = dir.path().join("objects").join(id.to_string());
        assert!(path.is_file());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn get_returns_exact_original_bytes() {
        let (_dir, store) = store();
        let bytes = b"round trip";
        let id = store.put(bytes).unwrap();
        assert_eq!(store.get(id).unwrap(), bytes);
    }

    #[test]
    fn exists_reflects_put() {
        let (_dir, store) = store();
        let id = object_id(b"existence");
        assert!(!store.exists(id).unwrap());
        let id = store.put(b"existence").unwrap();
        assert!(store.exists(id).unwrap());
    }

    #[test]
    fn put_same_bytes_twice_is_idempotent() {
        let (_dir, store) = store();
        let bytes = b"idempotent";
        let first = store.put(bytes).unwrap();
        let second = store.put(bytes).unwrap();
        assert_eq!(first, second);
        // Exactly one object file exists.
        assert_eq!(fs::read_dir(store.objects_dir()).unwrap().count(), 1);
    }

    #[test]
    fn put_same_bytes_concurrently_produces_one_object() {
        let (_dir, store) = store();
        let store = Arc::new(store);
        let bytes = vec![0xabu8; 4096];
        let id = object_id(&bytes);

        let mut handles = Vec::new();
        for _ in 0..8 {
            let store = Arc::clone(&store);
            let bytes = bytes.clone();
            handles.push(thread::spawn(move || store.put(&bytes).unwrap()));
        }
        for handle in handles {
            assert_eq!(handle.join().unwrap(), id);
        }

        assert_eq!(store.get(id).unwrap(), bytes);
        assert_eq!(fs::read_dir(store.objects_dir()).unwrap().count(), 1);
    }

    #[test]
    fn tampered_object_rejected_by_get() {
        let (_dir, store) = store();
        let bytes = b"tamper me";
        let id = store.put(bytes).unwrap();
        let path = store.path_for(id);
        fs::write(&path, b"tampered!").unwrap();

        match store.get(id) {
            Err(ObjectStoreError::Integrity { expected, actual }) => {
                assert_eq!(expected, id);
                assert_eq!(actual, object_id(b"tampered!"));
            }
            other => panic!("expected Integrity, got {other:?}"),
        }
    }

    #[test]
    fn put_existing_correct_object_does_not_rewrite() {
        let (_dir, store) = store();
        let bytes = b"no rewrite";
        let id = store.put(bytes).unwrap();
        let path = store.path_for(id);
        let before = fs::metadata(&path).unwrap().modified().unwrap();

        assert_eq!(store.put(bytes).unwrap(), id);

        let after = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(before, after, "existing object must not be rewritten");
    }

    #[test]
    fn put_existing_corrupted_object_reports_integrity() {
        let (_dir, store) = store();
        let bytes = b"original";
        let id = object_id(bytes);
        let path = store.path_for(id);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"corrupted").unwrap();

        match store.put(bytes) {
            Err(ObjectStoreError::Integrity { expected, actual }) => {
                assert_eq!(expected, id);
                assert_eq!(actual, object_id(b"corrupted"));
            }
            other => panic!("expected Integrity, got {other:?}"),
        }
    }

    #[test]
    fn get_missing_object_returns_not_found() {
        let (_dir, store) = store();
        let id = object_id(b"absent");
        match store.get(id) {
            Err(ObjectStoreError::NotFound(found)) => assert_eq!(found, id),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }
}
