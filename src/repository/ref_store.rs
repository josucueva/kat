//! Mutable repository references, including the `accepted` ref with
//! compare-and-swap publication (see `docs/prototype-design.md`).
//!
//! `RefStore` provides **atomic storage semantics only**. It deliberately
//! does not interpret refs: the invariant
//! `accepted.change.result_state == accepted.state` belongs to repository
//! open/integrity validation and Change publication, not here.

use std::fmt;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::domain::identity::ObjectId;

/// Process-global counter for unique temporary ref file names.
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// The `accepted` repository ref: the authoritative SemanticState and the
/// accepted ChangeRevision head (absent for a freshly initialized repository).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedRef {
    /// ObjectId of the authoritative SemanticState.
    pub state: ObjectId,
    /// ObjectId of the accepted ChangeRevision head, if any.
    pub change: Option<ObjectId>,
}

impl AcceptedRef {
    /// Parses the physical ref file format: `state <64 hex>` followed by
    /// `change <64 hex>` (or `change none`).
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut state: Option<ObjectId> = None;
        let mut change: Option<Option<ObjectId>> = None;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let (key, value) = line
                .split_once(' ')
                .ok_or_else(|| format!("expected '<key> <value>', got '{line}'"))?;
            let value = value.trim();
            match key {
                "state" => state = Some(parse_object_id(value, "state")?),
                "change" => {
                    change = Some(if value == "none" {
                        None
                    } else {
                        Some(parse_object_id(value, "change")?)
                    });
                }
                other => return Err(format!("unknown ref field: {other}")),
            }
        }
        let state = state.ok_or_else(|| "missing 'state' field".to_string())?;
        Ok(Self {
            state,
            change: change.unwrap_or(None),
        })
    }
}

impl fmt::Display for AcceptedRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.change {
            Some(id) => write!(f, "state {}\nchange {}\n", self.state, id),
            None => write!(f, "state {}\nchange none\n", self.state),
        }
    }
}

fn parse_object_id(value: &str, field: &str) -> Result<ObjectId, String> {
    value
        .parse::<ObjectId>()
        .map_err(|_| format!("malformed {field} ObjectId: {value}"))
}

/// Error produced by the ref store.
#[derive(Debug, thiserror::Error)]
pub enum RefStoreError {
    /// The ref is absent (not yet initialized).
    #[error("repository ref not found")]
    NotFound,
    /// An underlying filesystem failure.
    #[error("repository ref I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The ref file is not in the expected format.
    #[error("malformed repository ref: {0}")]
    Parse(String),
    /// A compare-and-swap could not publish because the ref state did not
    /// match expectations (or another publication is in progress).
    #[error("accepted ref changed concurrently (compare-and-swap failed)")]
    Conflict,
}

/// Persistence for mutable repository refs.
pub trait RefStore {
    /// Reads the current `accepted` ref.
    fn read_accepted(&self) -> Result<AcceptedRef, RefStoreError>;

    /// Creates the initial `accepted` ref, failing if it already exists.
    fn init_accepted(&self, initial: &AcceptedRef) -> Result<(), RefStoreError>;

    /// Publishes `new` only when the current ref equals `expected`
    /// (compare-and-swap). On any other current value the ref is untouched
    /// and `Conflict` is returned.
    fn compare_and_swap_accepted(
        &self,
        expected: &AcceptedRef,
        new: &AcceptedRef,
    ) -> Result<(), RefStoreError>;
}

/// Filesystem ref store rooted at a `.kat` directory.
#[derive(Debug)]
pub struct FileRefStore {
    root: PathBuf,
}

impl FileRefStore {
    /// Creates a store rooted at `root` (the `.kat` directory).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn refs_dir(&self) -> PathBuf {
        self.root.join("refs")
    }

    fn accepted_path(&self) -> PathBuf {
        self.refs_dir().join("accepted")
    }

    fn lock_path(&self) -> PathBuf {
        self.refs_dir().join("accepted.lock")
    }

    /// Acquires the accepted-ref lock (exclusive create), serializing CAS
    /// writers. Returns `Conflict` if another publication holds the lock.
    fn acquire_lock(&self) -> Result<LockGuard, RefStoreError> {
        let path = self.lock_path();
        fs::create_dir_all(path.parent().expect("refs dir has a parent"))?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => Ok(LockGuard { path }),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(RefStoreError::Conflict),
            Err(e) => Err(RefStoreError::Io(e)),
        }
    }
}

/// Removes the lock file on drop, on every path (success, error, or panic).
struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl RefStore for FileRefStore {
    fn read_accepted(&self) -> Result<AcceptedRef, RefStoreError> {
        let text = match fs::read_to_string(self.accepted_path()) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(RefStoreError::NotFound);
            }
            Err(e) => return Err(RefStoreError::Io(e)),
        };
        AcceptedRef::parse(&text).map_err(RefStoreError::Parse)
    }

    fn init_accepted(&self, initial: &AcceptedRef) -> Result<(), RefStoreError> {
        fs::create_dir_all(self.refs_dir())?;
        let path = self.accepted_path();
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(initial.to_string().as_bytes())?;
                file.sync_all()?;
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(RefStoreError::Conflict),
            Err(e) => Err(RefStoreError::Io(e)),
        }
    }

    fn compare_and_swap_accepted(
        &self,
        expected: &AcceptedRef,
        new: &AcceptedRef,
    ) -> Result<(), RefStoreError> {
        // Serialize writers; the guard removes the lock file on drop.
        let _lock = self.acquire_lock()?;

        let current = self.read_accepted()?;
        if &current != expected {
            return Err(RefStoreError::Conflict);
        }

        // Write new contents to a temp file in the same directory so the
        // final rename is atomic, flush it, then atomically replace the ref.
        let tmp_path = self.refs_dir().join(format!(
            "accepted.tmp-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(new.to_string().as_bytes())?;
            file.sync_all()?;
        }
        fs::rename(&tmp_path, self.accepted_path())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    fn object_id(byte: u8) -> ObjectId {
        ObjectId::from_bytes([byte; 32])
    }

    fn store() -> (tempfile::TempDir, FileRefStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = FileRefStore::new(dir.path());
        (dir, store)
    }

    fn init(store: &FileRefStore) -> AcceptedRef {
        let initial = AcceptedRef {
            state: object_id(1),
            change: None,
        };
        store.init_accepted(&initial).unwrap();
        initial
    }

    #[test]
    fn accepted_ref_round_trip_change_none() {
        let (_dir, store) = store();
        let initial = init(&store);
        assert_eq!(store.read_accepted().unwrap(), initial);
    }

    #[test]
    fn accepted_ref_round_trip_with_change() {
        let (_dir, store) = store();
        let initial = init(&store);
        let new = AcceptedRef {
            state: object_id(2),
            change: Some(object_id(3)),
        };
        store.compare_and_swap_accepted(&initial, &new).unwrap();
        assert_eq!(store.read_accepted().unwrap(), new);
    }

    #[test]
    fn parse_accepts_change_none_and_change_head() {
        let none = AcceptedRef::parse(&format!("state {}\nchange none\n", object_id(1))).unwrap();
        assert_eq!(
            none,
            AcceptedRef {
                state: object_id(1),
                change: None
            }
        );

        let head = AcceptedRef::parse(&format!(
            "state {}\nchange {}\n",
            object_id(1),
            object_id(2)
        ))
        .unwrap();
        assert_eq!(
            head,
            AcceptedRef {
                state: object_id(1),
                change: Some(object_id(2))
            }
        );
    }

    #[test]
    fn parse_rejects_malformed_state_object_id() {
        assert!(AcceptedRef::parse("state not-a-hex\n").is_err());
    }

    #[test]
    fn parse_rejects_malformed_change_object_id() {
        assert!(
            AcceptedRef::parse(&format!("state {}\nchange not-a-hex\n", object_id(1))).is_err()
        );
    }

    #[test]
    fn cas_succeeds_when_expected_matches() {
        let (_dir, store) = store();
        let initial = init(&store);
        let new = AcceptedRef {
            state: object_id(2),
            change: Some(object_id(3)),
        };
        store.compare_and_swap_accepted(&initial, &new).unwrap();
        assert_eq!(store.read_accepted().unwrap(), new);
    }

    #[test]
    fn cas_fails_when_expected_is_stale() {
        let (_dir, store) = store();
        let initial = init(&store);
        let stale_expected = AcceptedRef {
            state: object_id(9),
            change: None,
        };
        let new = AcceptedRef {
            state: object_id(2),
            change: Some(object_id(3)),
        };
        let err = store
            .compare_and_swap_accepted(&stale_expected, &new)
            .unwrap_err();
        assert!(matches!(err, RefStoreError::Conflict));
        // The failed CAS must not modify the ref.
        assert_eq!(store.read_accepted().unwrap(), initial);
    }

    #[test]
    fn concurrent_cas_allows_only_one_winner() {
        let (_dir, store) = store();
        let initial = Arc::new(init(&store));
        let new = Arc::new(AcceptedRef {
            state: object_id(2),
            change: Some(object_id(3)),
        });
        let store = Arc::new(store);

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let expected = Arc::clone(&initial);
                let new = Arc::clone(&new);
                thread::spawn(move || store.compare_and_swap_accepted(&expected, &new).is_ok())
            })
            .collect();

        let results: Vec<bool> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let winners = results.into_iter().filter(|won| *won).count();
        assert_eq!(winners, 1, "exactly one CAS must win");
        assert_eq!(store.read_accepted().unwrap(), *new);
    }

    #[test]
    fn init_accepted_fails_if_already_initialized() {
        let (_dir, store) = store();
        let initial = init(&store);
        let err = store.init_accepted(&initial).unwrap_err();
        assert!(matches!(err, RefStoreError::Conflict));
    }

    #[test]
    fn temp_and_lock_files_cleaned_after_successful_publication() {
        let (_dir, store) = store();
        let initial = init(&store);
        let new = AcceptedRef {
            state: object_id(2),
            change: Some(object_id(3)),
        };
        store.compare_and_swap_accepted(&initial, &new).unwrap();

        let names: Vec<String> = fs::read_dir(store.refs_dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, vec!["accepted".to_string()]);
    }
}
