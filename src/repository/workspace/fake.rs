use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::domain::identity::{MaterializationId, WorkspaceSnapshotId};
use crate::domain::workspace::{
    BackendConsistency, MaterializationResolution, PhysicalChanges, WorkingState, WorkspaceBackend,
    WorkspaceBackendError,
};
use crate::encoding::hash::{
    hash_directory_materialization, hash_file_materialization, hash_workspace_snapshot,
};

/// An in-memory fake backend for testing Workspace abstraction without Git.
///
/// It stores snapshots as simple maps from `PathBuf` to `Vec<u8>`.

#[derive(Clone, Debug, PartialEq)]
pub enum FakeEntry {
    File { content: Vec<u8>, executable: bool },
    Symlink { target: String },
}

pub struct FakeWorkspaceBackend {
    /// Simulates the physical working tree.
    pub working_tree: RwLock<HashMap<PathBuf, FakeEntry>>,

    /// Simulates the snapshot storage (like Git's object database).
    pub snapshots: RwLock<HashMap<WorkspaceSnapshotId, HashMap<PathBuf, FakeEntry>>>,

    /// Counter to generate unique snapshot IDs.
    #[allow(dead_code)]
    next_id: RwLock<u64>,
}

impl Default for FakeWorkspaceBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeWorkspaceBackend {
    pub fn new() -> Self {
        Self {
            working_tree: RwLock::new(HashMap::new()),
            snapshots: RwLock::new(HashMap::new()),
            next_id: RwLock::new(1),
        }
    }
}

impl WorkspaceBackend for FakeWorkspaceBackend {
    fn inspect_working_state(
        &self,
        base: &WorkspaceSnapshotId,
    ) -> Result<WorkingState, WorkspaceBackendError> {
        let snaps = self.snapshots.read().unwrap();
        let base_tree = snaps
            .get(base)
            .ok_or_else(|| WorkspaceBackendError::SnapshotNotFound(base.clone()))?;
        let current_tree = self.working_tree.read().unwrap();

        let mut changes = crate::domain::workspace::PhysicalChanges {
            added: Vec::new(),
            modified: Vec::new(),
            deleted: Vec::new(),
            untracked: Vec::new(), // Fake doesn't distinguish untracked vs added yet
            ignored: Vec::new(),
        };

        for (p, b_data) in base_tree.iter() {
            if let Some(c_data) = current_tree.get(p) {
                if c_data != b_data {
                    changes.modified.push(p.clone());
                }
            } else {
                changes.deleted.push(p.clone());
            }
        }
        for p in current_tree.keys() {
            if !base_tree.contains_key(p) {
                changes.added.push(p.clone());
            }
        }

        changes.added.sort();
        changes.modified.sort();
        changes.deleted.sort();
        Ok(WorkingState {
            changes,
            backend_consistency: BackendConsistency::Consistent,
        })
    }

    fn create_snapshot(
        &self,
        tracked_paths: &[PathBuf],
    ) -> Result<WorkspaceSnapshotId, WorkspaceBackendError> {
        let tree = self.working_tree.read().unwrap().clone();

        let mut entries: Vec<(String, u8, MaterializationId)> = tracked_paths
            .iter()
            .filter_map(|p| {
                tree.get(p).map(|entry| match entry {
                    FakeEntry::File {
                        content,
                        executable,
                    } => (
                        p.to_string_lossy().to_string(),
                        if *executable { b'X' } else { b'F' },
                        crate::encoding::hash::hash_file_materialization(*executable, content),
                    ),
                    FakeEntry::Symlink { target } => (
                        p.to_string_lossy().to_string(),
                        b'S',
                        crate::encoding::hash::hash_symlink_materialization(target.as_bytes()),
                    ),
                })
            })
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        let snapshot_id = hash_workspace_snapshot(&entries);

        // Fake backend just stores the selected files as the snapshot.
        let mut snap_tree = HashMap::new();
        for p in tracked_paths {
            if let Some(bytes) = tree.get(p) {
                snap_tree.insert(p.clone(), bytes.clone());
            }
        }

        self.snapshots
            .write()
            .unwrap()
            .insert(snapshot_id.clone(), snap_tree);
        Ok(snapshot_id)
    }

    fn materialize_snapshot(&self, id: &WorkspaceSnapshotId) -> Result<(), WorkspaceBackendError> {
        let snaps = self.snapshots.read().unwrap();
        if let Some(tree) = snaps.get(id) {
            let mut current = self.working_tree.write().unwrap();
            *current = tree.clone();
            Ok(())
        } else {
            Err(WorkspaceBackendError::SnapshotNotFound(id.clone()))
        }
    }

    fn compare_snapshots(
        &self,
        base: &WorkspaceSnapshotId,
        target: &WorkspaceSnapshotId,
    ) -> Result<PhysicalChanges, WorkspaceBackendError> {
        let snaps = self.snapshots.read().unwrap();
        let base_tree = snaps
            .get(base)
            .ok_or_else(|| WorkspaceBackendError::SnapshotNotFound(base.clone()))?;
        let target_tree = snaps
            .get(target)
            .ok_or_else(|| WorkspaceBackendError::SnapshotNotFound(target.clone()))?;

        let mut changes = PhysicalChanges {
            added: Vec::new(),
            modified: Vec::new(),
            deleted: Vec::new(),
            untracked: Vec::new(),
            ignored: Vec::new(),
        };

        for (p, b_data) in base_tree.iter() {
            if let Some(t_data) = target_tree.get(p) {
                if b_data != t_data {
                    changes.modified.push(p.clone());
                }
            } else {
                changes.deleted.push(p.clone());
            }
        }
        for p in target_tree.keys() {
            if !base_tree.contains_key(p) {
                changes.added.push(p.clone());
            }
        }

        changes.added.sort();
        changes.modified.sort();
        changes.deleted.sort();

        Ok(changes)
    }

    fn resolve_materialization(
        &self,
        path: &Path,
        snapshot: &WorkspaceSnapshotId,
    ) -> Result<MaterializationResolution, WorkspaceBackendError> {
        let snaps = self.snapshots.read().unwrap();
        let tree = snaps
            .get(snapshot)
            .ok_or_else(|| WorkspaceBackendError::SnapshotNotFound(snapshot.clone()))?;

        // Simple check for exact file match
        if let Some(entry) = tree.get(path) {
            return Ok(match entry {
                FakeEntry::File {
                    content,
                    executable,
                } => MaterializationResolution::File(
                    crate::encoding::hash::hash_file_materialization(*executable, content),
                ),
                FakeEntry::Symlink { target } => MaterializationResolution::Symlink(
                    crate::encoding::hash::hash_symlink_materialization(target.as_bytes()),
                ),
            });
        }

        // Check if it's a directory (i.e. if any files exist under this path prefix)
        let mut is_dir = false;
        let mut children = Vec::new();

        for (p, bytes) in tree {
            if p.starts_with(path) && p != path {
                is_dir = true;
                let (child_mat_id, file_type) = match bytes {
                    FakeEntry::File {
                        content,
                        executable,
                    } => (
                        hash_file_materialization(*executable, content),
                        if *executable { b'X' } else { b'F' },
                    ),
                    FakeEntry::Symlink { target } => (
                        crate::encoding::hash::hash_symlink_materialization(target.as_bytes()),
                        b'S',
                    ),
                };
                let p_str = p
                    .to_str()
                    .ok_or_else(|| WorkspaceBackendError::UnsupportedPathEncoding(p.clone()))?
                    .to_string();
                children.push((p_str, file_type, child_mat_id));
            }
        }

        if is_dir {
            children.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(MaterializationResolution::Directory(
                hash_directory_materialization(&children),
            ))
        } else {
            Ok(MaterializationResolution::NotFound)
        }
    }

    fn resolve_working_materialization(
        &self,
        path: &Path,
    ) -> Result<MaterializationResolution, WorkspaceBackendError> {
        let tree = self.working_tree.read().unwrap();

        // Simple check for exact file match
        if let Some(entry) = tree.get(path) {
            return Ok(match entry {
                FakeEntry::File {
                    content,
                    executable,
                } => MaterializationResolution::File(
                    crate::encoding::hash::hash_file_materialization(*executable, content),
                ),
                FakeEntry::Symlink { target } => MaterializationResolution::Symlink(
                    crate::encoding::hash::hash_symlink_materialization(target.as_bytes()),
                ),
            });
        }

        // Check if it's a directory (i.e. if any files exist under this path prefix)
        let mut is_dir = false;
        let mut children = Vec::new();

        for (p, bytes) in tree.iter() {
            if p.starts_with(path) && p != path {
                is_dir = true;
                let (child_mat_id, file_type) = match bytes {
                    FakeEntry::File {
                        content,
                        executable,
                    } => (
                        hash_file_materialization(*executable, content),
                        if *executable { b'X' } else { b'F' },
                    ),
                    FakeEntry::Symlink { target } => (
                        crate::encoding::hash::hash_symlink_materialization(target.as_bytes()),
                        b'S',
                    ),
                };
                let p_str = p
                    .to_str()
                    .ok_or_else(|| WorkspaceBackendError::UnsupportedPathEncoding(p.clone()))?
                    .to_string();
                children.push((p_str, file_type, child_mat_id));
            }
        }

        if is_dir {
            children.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(MaterializationResolution::Directory(
                hash_directory_materialization(&children),
            ))
        } else {
            Ok(MaterializationResolution::NotFound)
        }
    }

    fn verify_snapshot_integrity(
        &self,
        id: &WorkspaceSnapshotId,
    ) -> Result<bool, WorkspaceBackendError> {
        let snaps = self.snapshots.read().unwrap();
        Ok(snaps.contains_key(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fake_workspace_snapshot_flow() {
        let backend = FakeWorkspaceBackend::new();

        backend.working_tree.write().unwrap().insert(
            PathBuf::from("test.txt"),
            FakeEntry::File {
                content: b"hello".to_vec(),
                executable: false,
            },
        );
        let snap1 = backend
            .create_snapshot(&[PathBuf::from("test.txt")])
            .unwrap();

        backend.working_tree.write().unwrap().insert(
            PathBuf::from("test.txt"),
            FakeEntry::File {
                content: b"world".to_vec(),
                executable: false,
            },
        );
        let snap2 = backend
            .create_snapshot(&[PathBuf::from("test.txt")])
            .unwrap();

        let diff = backend.compare_snapshots(&snap1, &snap2).unwrap();
        assert_eq!(diff.modified, vec![PathBuf::from("test.txt")]);
        assert!(diff.added.is_empty());
        assert!(diff.deleted.is_empty());

        backend.materialize_snapshot(&snap1).unwrap();
        assert_eq!(
            backend
                .working_tree
                .read()
                .unwrap()
                .get(&PathBuf::from("test.txt"))
                .unwrap(),
            &FakeEntry::File {
                content: b"hello".to_vec(),
                executable: false
            }
        );
    }

    #[test]
    fn test_materialization_resolution() {
        let backend = FakeWorkspaceBackend::new();
        backend.working_tree.write().unwrap().insert(
            PathBuf::from("dir/file.txt"),
            FakeEntry::File {
                content: b"content".to_vec(),
                executable: false,
            },
        );
        let snap = backend
            .create_snapshot(&[PathBuf::from("dir/file.txt")])
            .unwrap();

        let res_file = backend
            .resolve_materialization(Path::new("dir/file.txt"), &snap)
            .unwrap();
        assert!(matches!(res_file, MaterializationResolution::File(_)));

        let res_dir = backend
            .resolve_materialization(Path::new("dir"), &snap)
            .unwrap();
        assert!(matches!(res_dir, MaterializationResolution::Directory(_)));

        let res_none = backend
            .resolve_materialization(Path::new("missing"), &snap)
            .unwrap();
        assert!(matches!(res_none, MaterializationResolution::NotFound));
    }

    #[test]
    fn test_cross_backend_identity_invariants() {
        // Fake backend A
        let backend_a = FakeWorkspaceBackend::new();
        backend_a.working_tree.write().unwrap().insert(
            PathBuf::from("src/a.rs"),
            FakeEntry::File {
                content: b"X".to_vec(),
                executable: false,
            },
        );
        backend_a.working_tree.write().unwrap().insert(
            PathBuf::from("src/b.rs"),
            FakeEntry::File {
                content: b"Y".to_vec(),
                executable: false,
            },
        );
        let snap_a = backend_a
            .create_snapshot(&[PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")])
            .unwrap();
        let mat_a = backend_a
            .resolve_materialization(Path::new("src/a.rs"), &snap_a)
            .unwrap();

        // Fake backend B
        let backend_b = FakeWorkspaceBackend::new();
        backend_b.working_tree.write().unwrap().insert(
            PathBuf::from("src/a.rs"),
            FakeEntry::File {
                content: b"X".to_vec(),
                executable: false,
            },
        );
        backend_b.working_tree.write().unwrap().insert(
            PathBuf::from("src/b.rs"),
            FakeEntry::File {
                content: b"Y".to_vec(),
                executable: false,
            },
        );
        let snap_b = backend_b
            .create_snapshot(&[PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")])
            .unwrap();
        let mat_b = backend_b
            .resolve_materialization(Path::new("src/a.rs"), &snap_b)
            .unwrap();

        // Snapshots must be identical
        assert_eq!(snap_a, snap_b);

        // Resolved materializations must be identical
        assert_eq!(mat_a, mat_b);
    }
}
