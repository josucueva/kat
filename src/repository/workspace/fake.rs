use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::domain::identity::{MaterializationId, PhysicalCandidateId, WorkspaceSnapshotId};
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

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FakeEntry {
    File { content: Vec<u8>, executable: bool },
    Symlink { target: String },
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FakePhysicalCandidateState {
    pub provisional_tree: HashMap<PathBuf, FakeEntry>,
    pub conflicts: Vec<FakeMaterializationConflictState>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FakeMaterializationConflictState {
    pub path: PathBuf,
    pub base: Option<FakeEntry>,
    pub local: Option<FakeEntry>,
    pub other: Option<FakeEntry>,
}

pub struct FakeWorkspaceBackend {
    pub project_root: Option<PathBuf>,
    /// Simulates the physical working tree.
    pub working_tree: RwLock<HashMap<PathBuf, FakeEntry>>,

    /// Simulates the snapshot storage (like Git's object database).
    pub snapshots: RwLock<HashMap<WorkspaceSnapshotId, HashMap<PathBuf, FakeEntry>>>,

    /// Simulates the provisional physical merge states.
    pub provisional_merges: RwLock<HashMap<PhysicalCandidateId, HashMap<PathBuf, FakeEntry>>>,

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
            project_root: None,
            working_tree: RwLock::new(HashMap::new()),
            snapshots: RwLock::new(HashMap::new()),
            provisional_merges: RwLock::new(HashMap::new()),
            next_id: RwLock::new(1),
        }
    }

    pub fn with_root(path: &Path) -> Self {
        Self {
            project_root: Some(path.to_path_buf()),
            working_tree: RwLock::new(HashMap::new()),
            snapshots: RwLock::new(HashMap::new()),
            provisional_merges: RwLock::new(HashMap::new()),
            next_id: RwLock::new(1),
        }
    }

    pub fn get_candidate_state(
        &self,
        id: &crate::domain::identity::PhysicalCandidateId,
    ) -> Result<FakePhysicalCandidateState, WorkspaceBackendError> {
        if let Some(root) = &self.project_root {
            let candidates_dir = root.join(".kat/physical/fake/candidates");
            let state_path = candidates_dir.join(format!("{}.json", id));
            let json =
                std::fs::read_to_string(&state_path).map_err(WorkspaceBackendError::Io)?;
            serde_json::from_str(&json)
                .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))
        } else {
            // For purely in-memory tests, we reconstruct the state from `provisional_merges`.
            let merges = self.provisional_merges.read().unwrap();
            let tree = merges.get(id).ok_or_else(|| {
                WorkspaceBackendError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Candidate not found in memory",
                ))
            })?;
            Ok(FakePhysicalCandidateState {
                provisional_tree: tree.clone(),
                conflicts: Vec::new(), // In-memory fallback doesn't store conflicts since tests don't assert it yet
            })
        }
    }
}

impl WorkspaceBackend for FakeWorkspaceBackend {
    fn persist_physical_candidate(
        &self,
        _workspace_id: &crate::domain::workspace::WorkspaceId,
        _candidate: &crate::domain::workspace::PhysicalReconciliationCandidate,
    ) -> Result<(), WorkspaceBackendError> {
        // For tests, fake backend candidate persistence is in-memory or not strictly required
        Ok(())
    }
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

    fn reconcile_physical(
        &self,
        base: &WorkspaceSnapshotId,
        local: &WorkspaceSnapshotId,
        other: &WorkspaceSnapshotId,
    ) -> Result<crate::domain::workspace::PhysicalReconciliationResult, WorkspaceBackendError> {
        use crate::domain::conflict::{MaterializationConflict, MaterializationConflictKind};
        use crate::domain::workspace::{
            PhysicalReconciliationCandidate, PhysicalReconciliationResult,
        };

        let snaps = self.snapshots.read().unwrap();
        let b = snaps
            .get(base)
            .ok_or_else(|| WorkspaceBackendError::SnapshotNotFound(base.clone()))?;
        let l = snaps
            .get(local)
            .ok_or_else(|| WorkspaceBackendError::SnapshotNotFound(local.clone()))?;
        let o = snaps
            .get(other)
            .ok_or_else(|| WorkspaceBackendError::SnapshotNotFound(other.clone()))?;

        let mut provisional = HashMap::new();
        let mut conflicts = Vec::new();
        let mut state_conflicts = Vec::new();
        let mut all_paths = BTreeSet::new();

        all_paths.extend(b.keys().cloned());
        all_paths.extend(l.keys().cloned());
        all_paths.extend(o.keys().cloned());

        let mut add_conflict = |path: &PathBuf, kind: MaterializationConflictKind| {
            conflicts.push(MaterializationConflict {
                kind,
                paths: vec![path.clone()],
            });
            state_conflicts.push(FakeMaterializationConflictState {
                path: path.clone(),
                base: b.get(path).cloned(),
                local: l.get(path).cloned(),
                other: o.get(path).cloned(),
            });
        };

        for path in &all_paths {
            let b_entry = b.get(path);
            let l_entry = l.get(path);
            let o_entry = o.get(path);

            if l_entry == o_entry {
                // Both branches did the same thing (or unchanged)
                if let Some(entry) = l_entry {
                    provisional.insert(path.clone(), entry.clone());
                }
            } else if l_entry == b_entry {
                // Local unchanged, Other changed
                if let Some(entry) = o_entry {
                    provisional.insert(path.clone(), entry.clone());
                }
            } else if o_entry == b_entry {
                // Other unchanged, Local changed
                if let Some(entry) = l_entry {
                    provisional.insert(path.clone(), entry.clone());
                }
            } else {
                // Conflict
                let kind = if b_entry.is_some() && (l_entry.is_none() || o_entry.is_none()) {
                    MaterializationConflictKind::DeleteModify
                } else if let (Some(le), Some(oe)) = (l_entry, o_entry) {
                    if std::mem::discriminant(le) != std::mem::discriminant(oe) {
                        MaterializationConflictKind::TypeChange
                    } else {
                        MaterializationConflictKind::Content
                    }
                } else {
                    MaterializationConflictKind::Content
                };

                add_conflict(path, kind);

                // Neutral rule: retain Base if it existed
                if let Some(entry) = b_entry {
                    provisional.insert(path.clone(), entry.clone());
                }
            }
        }

        // Tree structure validation
        // In Fake, entries are always non-directories (File/Symlink).
        // Thus, if `path_a` is a prefix of `path_b`, there's a tree collision
        // since `path_a` cannot be both a file and a parent directory.
        let mut provisional_paths: Vec<_> = provisional.keys().cloned().collect();
        provisional_paths.sort();

        let mut paths_to_remove = BTreeSet::new();
        for i in 0..provisional_paths.len() {
            let p1 = &provisional_paths[i];
            for j in (i + 1)..provisional_paths.len() {
                let p2 = &provisional_paths[j];
                if p2.starts_with(p1) {
                    conflicts.push(MaterializationConflict {
                        kind: MaterializationConflictKind::PathCollision,
                        paths: vec![p1.clone(), p2.clone()],
                    });
                    state_conflicts.push(FakeMaterializationConflictState {
                        path: p1.clone(),
                        base: b.get(p1).cloned(),
                        local: l.get(p1).cloned(),
                        other: o.get(p1).cloned(),
                    });
                    state_conflicts.push(FakeMaterializationConflictState {
                        path: p2.clone(),
                        base: b.get(p2).cloned(),
                        local: l.get(p2).cloned(),
                        other: o.get(p2).cloned(),
                    });

                    paths_to_remove.insert(p1.clone());
                    paths_to_remove.insert(p2.clone());
                } else {
                    // Not a prefix, since array is sorted, subsequent ones won't be either.
                    // Wait, sorting by PathBuf sorts by components.
                    // Example: "foo", "foo/bar", "fooz"
                    // So we can break early if not starts_with.
                    break;
                }
            }
        }

        // Remove conflicting paths from provisional state.
        for p in paths_to_remove {
            provisional.remove(&p);
            // Revert to Base if it existed, otherwise omit.
            if let Some(entry) = b.get(&p) {
                provisional.insert(p.clone(), entry.clone());
            }
        }

        if conflicts.is_empty() {
            // Clean merge.
            let mut entries: Vec<_> = provisional
                .iter()
                .map(|(p, entry)| match entry {
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
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));

            let new_snapshot_id = crate::encoding::hash::hash_workspace_snapshot(&entries);

            // Wait, we need to save this tree into snapshots so it can be materialized!
            // I need mutable access to `snapshots` but I only have `&self` and read guard.
            // I'll drop the read guard and write.
            drop(snaps);

            let mut w_snaps = self.snapshots.write().unwrap();
            w_snaps.insert(new_snapshot_id.clone(), provisional);

            Ok(PhysicalReconciliationResult::Clean {
                snapshot: new_snapshot_id,
            })
        } else {
            // Conflicted merge
            // Sort conflicts deterministically
            conflicts.sort_by(|a, b| a.paths.cmp(&b.paths).then_with(|| a.kind.cmp(&b.kind)));

            // Store provisional state
            let phys_id = PhysicalCandidateId::new();
            self.provisional_merges
                .write()
                .unwrap()
                .insert(phys_id, provisional.clone());

            let state = FakePhysicalCandidateState {
                provisional_tree: provisional,
                conflicts: state_conflicts,
            };

            if let Some(root) = &self.project_root {
                let candidates_dir = root.join(".kat/physical/fake/candidates");
                let _ = std::fs::create_dir_all(&candidates_dir);
                let state_path = candidates_dir.join(format!("{}.json", phys_id));
                let json = serde_json::to_string_pretty(&state).unwrap();
                let _ = std::fs::write(&state_path, json);
            }

            Ok(PhysicalReconciliationResult::Conflicted(
                PhysicalReconciliationCandidate {
                    base: base.clone(),
                    local: local.clone(),
                    other: other.clone(),
                    conflicts,
                    provisional: phys_id,
                },
            ))
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
