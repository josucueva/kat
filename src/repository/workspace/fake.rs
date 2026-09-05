use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::domain::identity::{MaterializationId, WorkspaceSnapshotId};
use crate::domain::workspace::{
    BackendConsistency, MaterializationResolution, WorkingState, WorkspaceBackend,
    WorkspaceBackendError,
};
use crate::encoding::hash::{
    hash_directory_materialization, hash_file_materialization, hash_workspace_snapshot,
};

/// An in-memory fake backend for testing Workspace abstraction without Git.
///
/// It stores snapshots as simple maps from `PathBuf` to `Vec<u8>`.
pub struct FakeWorkspaceBackend {
    /// Simulates the physical working tree.
    pub working_tree: RwLock<HashMap<PathBuf, Vec<u8>>>,

    /// Simulates the snapshot storage (like Git's object database).
    pub snapshots: RwLock<HashMap<WorkspaceSnapshotId, HashMap<PathBuf, Vec<u8>>>>,

    /// Counter to generate unique snapshot IDs.
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
    fn inspect_working_state(&self, base: &WorkspaceSnapshotId) -> Result<WorkingState, WorkspaceBackendError> {
        let snaps = self.snapshots.read().unwrap();
        let base_tree = snaps
            .get(base)
            .ok_or_else(|| WorkspaceBackendError::SnapshotNotFound(base.clone()))?;
        let current_tree = self.working_tree.read().unwrap();

        let mut diff = WorkingState {
            backend_consistency: BackendConsistency::Consistent, // Fake backend doesn't simulate mismatch yet
            added: Vec::new(),
            modified: Vec::new(),
            deleted: Vec::new(),
            untracked: Vec::new(), // Fake doesn't distinguish untracked vs added yet
            ignored: Vec::new(),
        };

        for (p, b_data) in base_tree {
            if let Some(c_data) = current_tree.get(p) {
                if c_data != b_data {
                    diff.modified.push(p.clone());
                }
            } else {
                diff.deleted.push(p.clone());
            }
        }
        for (p, _) in current_tree.iter() {
            if !base_tree.contains_key(p) {
                diff.added.push(p.clone());
            }
        }

        diff.added.sort();
        diff.modified.sort();
        diff.deleted.sort();
        Ok(diff)
    }

    fn create_snapshot(&self, tracked_paths: &[PathBuf]) -> Result<WorkspaceSnapshotId, WorkspaceBackendError> {
        let tree = self.working_tree.read().unwrap().clone();
        
        let mut entries: Vec<(String, u8, MaterializationId)> = tracked_paths.iter().filter_map(|p| {
            tree.get(p).map(|bytes| {
                // Fake doesn't support exec bit tracking, so just use `false`
                (p.to_string_lossy().to_string(), b'F', hash_file_materialization(false, bytes))
            })
        }).collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        let snapshot_id = hash_workspace_snapshot(&entries);

        // Fake backend just stores the selected files as the snapshot.
        let mut snap_tree = HashMap::new();
        for p in tracked_paths {
            if let Some(bytes) = tree.get(p) {
                snap_tree.insert(p.clone(), bytes.clone());
            }
        }
        
        self.snapshots.write().unwrap().insert(snapshot_id.clone(), snap_tree);
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
    ) -> Result<Vec<PathBuf>, WorkspaceBackendError> {
        let snaps = self.snapshots.read().unwrap();
        let base_tree = snaps
            .get(base)
            .ok_or_else(|| WorkspaceBackendError::SnapshotNotFound(base.clone()))?;
        let target_tree = snaps
            .get(target)
            .ok_or_else(|| WorkspaceBackendError::SnapshotNotFound(target.clone()))?;

        let mut diff = Vec::new();
        for (p, b_data) in base_tree {
            if target_tree.get(p) != Some(b_data) {
                diff.push(p.clone());
            }
        }
        for (p, _) in target_tree {
            if !base_tree.contains_key(p) && !diff.contains(p) {
                diff.push(p.clone());
            }
        }

        diff.sort();
        Ok(diff)
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
        if let Some(bytes) = tree.get(path) {
            return Ok(MaterializationResolution::File(hash_file_materialization(false, bytes)));
        }

        // Check if it's a directory (i.e. if any files exist under this path prefix)
        let mut is_dir = false;
        let mut children = Vec::new();
        
        for (p, bytes) in tree {
            if p.starts_with(path) && p != path {
                is_dir = true;
                let child_mat_id = hash_file_materialization(false, bytes);
                children.push((p.to_string_lossy().to_string(), b'F', child_mat_id));
            }
        }

        if is_dir {
            children.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(MaterializationResolution::Directory(hash_directory_materialization(&children)))
        } else {
            Ok(MaterializationResolution::NotFound)
        }
    }

    fn verify_snapshot_integrity(&self, id: &WorkspaceSnapshotId) -> Result<bool, WorkspaceBackendError> {
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
        
        backend.working_tree.write().unwrap().insert(PathBuf::from("test.txt"), b"hello".to_vec());
        let snap1 = backend.create_snapshot(&[PathBuf::from("test.txt")]).unwrap();

        backend.working_tree.write().unwrap().insert(PathBuf::from("test.txt"), b"world".to_vec());
        let snap2 = backend.create_snapshot(&[PathBuf::from("test.txt")]).unwrap();

        let diff = backend.compare_snapshots(&snap1, &snap2).unwrap();
        assert_eq!(diff, vec![PathBuf::from("test.txt")]);

        backend.materialize_snapshot(&snap1).unwrap();
        assert_eq!(backend.working_tree.read().unwrap().get(&PathBuf::from("test.txt")).unwrap(), b"hello");
    }

    #[test]
    fn test_materialization_resolution() {
        let backend = FakeWorkspaceBackend::new();
        backend.working_tree.write().unwrap().insert(PathBuf::from("dir/file.txt"), b"content".to_vec());
        let snap = backend.create_snapshot(&[PathBuf::from("dir/file.txt")]).unwrap();

        let res_file = backend.resolve_materialization(Path::new("dir/file.txt"), &snap).unwrap();
        assert!(matches!(res_file, MaterializationResolution::File(_)));

        let res_dir = backend.resolve_materialization(Path::new("dir"), &snap).unwrap();
        assert!(matches!(res_dir, MaterializationResolution::Directory(_)));

        let res_none = backend.resolve_materialization(Path::new("missing"), &snap).unwrap();
        assert!(matches!(res_none, MaterializationResolution::NotFound));
    }

    #[test]
    fn test_cross_backend_identity_invariants() {
        // Fake backend A
        let backend_a = FakeWorkspaceBackend::new();
        backend_a.working_tree.write().unwrap().insert(PathBuf::from("src/a.rs"), b"X".to_vec());
        backend_a.working_tree.write().unwrap().insert(PathBuf::from("src/b.rs"), b"Y".to_vec());
        let snap_a = backend_a.create_snapshot(&[PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")]).unwrap();
        let mat_a = backend_a.resolve_materialization(Path::new("src/a.rs"), &snap_a).unwrap();

        // Fake backend B
        let backend_b = FakeWorkspaceBackend::new();
        backend_b.working_tree.write().unwrap().insert(PathBuf::from("src/a.rs"), b"X".to_vec());
        backend_b.working_tree.write().unwrap().insert(PathBuf::from("src/b.rs"), b"Y".to_vec());
        let snap_b = backend_b.create_snapshot(&[PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")]).unwrap();
        let mat_b = backend_b.resolve_materialization(Path::new("src/a.rs"), &snap_b).unwrap();

        // Snapshots must be identical
        assert_eq!(snap_a, snap_b);

        // Resolved materializations must be identical
        assert_eq!(mat_a, mat_b);
    }
}
