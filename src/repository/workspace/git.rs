use git2::Repository;
use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::identity::{MaterializationId, PhysicalCandidateId, WorkspaceSnapshotId};
use crate::domain::workspace::{
    BackendConsistency, MaterializationResolution, PhysicalChanges, WorkingState, WorkspaceBackend,
    WorkspaceBackendError,
};

/// A specific Git commit and ancestry context representing a snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendSnapshotRepresentation {
    pub commit: git2::Oid,
}
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct GitPhysicalCandidateState {
    pub version: u32,
    pub base_snapshot: String,
    pub local_snapshot: String,
    pub other_snapshot: String,
    pub provisional_tree: String,
    pub conflicts: Vec<GitConflictReconstructionData>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GitConflictReconstructionData {
    pub kind: crate::domain::conflict::MaterializationConflictKind,
    pub paths: Vec<PathBuf>,
    pub base: Option<GitCandidateEntry>,
    pub local: Option<GitCandidateEntry>,
    pub other: Option<GitCandidateEntry>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GitCandidateEntry {
    pub path: PathBuf,
    pub oid: String,
    pub mode: u32,
}

/// A WorkspaceBackend that uses a Git repository stored inside `.kat/physical/git/`.
pub struct GitWorkspaceBackend {
    _repo: Repository,
    _project_root: PathBuf,
    transient_candidates: std::sync::RwLock<
        std::collections::HashMap<PhysicalCandidateId, GitPhysicalCandidateState>,
    >,
}

impl GitWorkspaceBackend {
    /// Opens an existing KAT Git backend or creates a new one at the given project root.
    /// The physical git repository will be at `<project_root>/.kat/physical/git/`.
    pub fn open(project_root: &Path) -> Result<Self, WorkspaceBackendError> {
        let kat_git_dir = project_root.join(".kat/physical/git");

        // If it doesn't exist, we can't open it (init logic handles creation).
        if !kat_git_dir.exists() {
            return Err(WorkspaceBackendError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "KAT physical Git repository not found",
            )));
        }

        let repo = Repository::open(&kat_git_dir).map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to open Git repository: {}",
                e
            )))
        })?;
        repo.set_workdir(project_root, false).map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to set Git workdir: {}",
                e
            )))
        })?;

        Ok(Self {
            _repo: repo,
            _project_root: project_root.to_path_buf(),
            transient_candidates: std::sync::RwLock::new(std::collections::HashMap::new()),
        })
    }

    /// Initializes a new GitWorkspaceBackend at the given project root.
    pub fn init(project_root: &Path) -> Result<Self, WorkspaceBackendError> {
        let kat_git_dir = project_root.join(".kat/physical/git");

        let repo = Repository::init_bare(&kat_git_dir).map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to initialize Git repository: {}",
                e
            )))
        })?;
        repo.set_workdir(project_root, false).map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to set Git workdir: {}",
                e
            )))
        })?;

        Ok(Self {
            _repo: repo,
            _project_root: project_root.to_path_buf(),
            transient_candidates: std::sync::RwLock::new(std::collections::HashMap::new()),
        })
    }

    /// Adopts an existing Git repository using Managed Import and Cutover.
    pub fn adopt(project_root: &Path) -> Result<Self, WorkspaceBackendError> {
        let kat_git_dir = project_root.join(".kat/physical/git");
        let original_git_dir = project_root.join(".git");
        let backup_git_dir = project_root.join(".kat/adoption-backup/original-git");

        if kat_git_dir.exists() {
            if original_git_dir.exists() {
                // Interrupted adoption: clean up partial physical git and retry
                std::fs::remove_dir_all(&kat_git_dir).map_err(WorkspaceBackendError::Io)?;
            } else {
                return Err(WorkspaceBackendError::Io(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "KAT physical Git repository already exists",
                )));
            }
        }
        if !original_git_dir.exists() {
            return Self::init(project_root);
        }

        // 1. Inspect / Pre-validate (Check if we can open it)
        if let Err(e) = Repository::open(&original_git_dir) {
            return Err(WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to inspect original Git repository: {}",
                e
            ))));
        }

        // 2. Import: Copy the .git directory to .kat/physical/git
        if let Err(e) = copy_dir_all(&original_git_dir, &kat_git_dir) {
            // Clean up partial import
            let _ = fs::remove_dir_all(&kat_git_dir);
            return Err(WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to import Git repository: {}",
                e
            ))));
        }

        // 3. Validate the imported repository
        let repo = match Repository::open(&kat_git_dir) {
            Ok(r) => r,
            Err(e) => {
                let _ = fs::remove_dir_all(&kat_git_dir);
                return Err(WorkspaceBackendError::Io(std::io::Error::other(format!(
                    "Imported Git repository failed validation: {}",
                    e
                ))));
            }
        };

        repo.set_workdir(project_root, false).map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to set Git workdir for adopted repo: {}",
                e
            )))
        })?;

        // 4. Snapshot / Verify Working Tree Equivalence
        // TODO: In Phase 3.4 we will create the initial WorkspaceSnapshot here.

        fs::create_dir_all(backup_git_dir.parent().unwrap()).map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to create backup directory: {}",
                e
            )))
        })?;

        if let Ok(head) = repo.head()
            && let Some(target) = head.target()
        {
            Self::write_active_lineage(project_root, target)?;
        }

        if let Err(e) = fs::rename(&original_git_dir, &backup_git_dir) {
            let _ = fs::remove_dir_all(&kat_git_dir);
            return Err(WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to cut over Git repository: {}",
                e
            ))));
        }

        Ok(Self {
            _repo: repo,
            _project_root: project_root.to_path_buf(),
            transient_candidates: std::sync::RwLock::new(std::collections::HashMap::new()),
        })
    }

    /// Saves a representation mapping: WorkspaceSnapshotId -> GitCommitOid[]
    pub fn record_snapshot_representation(
        project_root: &Path,
        snapshot_id: &WorkspaceSnapshotId,
        commit: git2::Oid,
    ) -> Result<(), WorkspaceBackendError> {
        let index_path = project_root.join(".kat/physical/snapshots.json");
        let mut index: std::collections::HashMap<String, Vec<String>> = if index_path.exists() {
            let content = std::fs::read_to_string(&index_path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };

        let id_str = snapshot_id.to_hex();
        let commit_str = commit.to_string();
        let entries = index.entry(id_str).or_default();
        if !entries.contains(&commit_str) {
            entries.push(commit_str);
        }

        if let Some(parent) = index_path.parent() {
            std::fs::create_dir_all(parent).map_err(WorkspaceBackendError::Io)?;
        }
        let content = serde_json::to_string_pretty(&index).unwrap();
        std::fs::write(index_path, content).map_err(WorkspaceBackendError::Io)?;

        // Also create a ref to protect from GC
        if let Ok(repo) = git2::Repository::open(project_root.join(".kat/physical/git")) {
            let ref_name = format!("refs/kat/lineage/{}/{}", snapshot_id.to_hex(), commit);
            let _ = repo.reference(&ref_name, commit, true, "KAT physical lineage protection");
        }

        Ok(())
    }

    /// Loads the representation mapping for a given WorkspaceSnapshotId
    pub fn get_snapshot_representations(
        project_root: &Path,
        id: &WorkspaceSnapshotId,
    ) -> Vec<BackendSnapshotRepresentation> {
        let index_path = project_root.join(".kat/physical/snapshots.json");
        let content = match fs::read_to_string(&index_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let index: std::collections::HashMap<String, Vec<String>> =
            match serde_json::from_str(&content) {
                Ok(idx) => idx,
                Err(_) => return Vec::new(),
            };

        let id_hex = id.to_hex();
        if let Some(oids_hex) = index.get(&id_hex) {
            oids_hex
                .iter()
                .filter_map(|h| git2::Oid::from_str(h).ok())
                .map(|commit| BackendSnapshotRepresentation { commit })
                .collect()
        } else {
            Vec::new()
        }
    }

    fn active_lineage_path(project_root: &Path) -> PathBuf {
        project_root.join(".kat/physical/active_lineage")
    }

    fn read_active_lineage(project_root: &Path) -> Option<git2::Oid> {
        let path = Self::active_lineage_path(project_root);
        if let Ok(content) = fs::read_to_string(&path) {
            git2::Oid::from_str(content.trim()).ok()
        } else {
            None
        }
    }

    #[doc(hidden)]
    pub fn read_active_lineage_for_test(root: &Path) -> Option<git2::Oid> {
        Self::read_active_lineage(root)
    }

    fn write_active_lineage(
        project_root: &Path,
        oid: git2::Oid,
    ) -> Result<(), WorkspaceBackendError> {
        let path = Self::active_lineage_path(project_root);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Write atomically
        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, oid.to_string()).map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to write active lineage: {}",
                e
            )))
        })?;
        fs::rename(&temp_path, &path).map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to persist active lineage: {}",
                e
            )))
        })?;
        Ok(())
    }

    /// Checks if the backend representation of `ancestor` is an ancestor of `descendant`
    /// in the underlying Git history.
    pub fn is_ancestor(
        &self,
        ancestor: &BackendSnapshotRepresentation,
        descendant: &BackendSnapshotRepresentation,
    ) -> Result<bool, WorkspaceBackendError> {
        self._repo
            .graph_descendant_of(descendant.commit, ancestor.commit)
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e.to_string())))
    }

    /// Returns the physical merge base (common ancestor) of two representations.
    pub fn physical_merge_base(
        &self,
        a: &BackendSnapshotRepresentation,
        b: &BackendSnapshotRepresentation,
    ) -> Result<BackendSnapshotRepresentation, WorkspaceBackendError> {
        let base_oid = self
            ._repo
            .merge_base(a.commit, b.commit)
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e.to_string())))?;
        Ok(BackendSnapshotRepresentation { commit: base_oid })
    }
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

impl WorkspaceBackend for GitWorkspaceBackend {
    fn inspect_working_state(
        &self,
        base: &WorkspaceSnapshotId,
    ) -> Result<WorkingState, WorkspaceBackendError> {
        let representations = Self::get_snapshot_representations(&self._project_root, base);
        if representations.is_empty() {
            return Err(WorkspaceBackendError::SnapshotNotFound(base.clone()));
        }

        let active_oid = Self::read_active_lineage(&self._project_root).ok_or_else(|| {
            WorkspaceBackendError::Io(std::io::Error::other("Active lineage not found"))
        })?;

        let is_consistent = representations.iter().any(|r| r.commit == active_oid);
        let backend_consistency = if is_consistent {
            BackendConsistency::Consistent
        } else {
            BackendConsistency::Mismatch(format!(
                "Active lineage is at {}, but expected one of {:?}",
                active_oid,
                representations.iter().map(|r| r.commit).collect::<Vec<_>>()
            ))
        };

        let base_commit_oid = if is_consistent {
            active_oid
        } else {
            representations[0].commit
        };

        let commit = self._repo.find_commit(base_commit_oid).map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to find commit: {}",
                e
            )))
        })?;
        let tree = commit.tree().map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!("Failed to find tree: {}", e)))
        })?;

        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.include_untracked(true);
        diff_opts.include_ignored(true);
        diff_opts.recurse_untracked_dirs(true);

        let diff = self
            ._repo
            .diff_tree_to_workdir(Some(&tree), Some(&mut diff_opts))
            .map_err(|e| {
                WorkspaceBackendError::Io(std::io::Error::other(format!(
                    "Failed to diff tree: {}",
                    e
                )))
            })?;

        let mut changes = crate::domain::workspace::PhysicalChanges {
            added: Vec::new(),
            modified: Vec::new(),
            deleted: Vec::new(),
            untracked: Vec::new(),
            ignored: Vec::new(),
        };

        for delta in diff.deltas() {
            let old_file = delta.old_file();
            let new_file = delta.new_file();
            let path = new_file.path().unwrap_or_else(|| old_file.path().unwrap());

            // .kat exclusion
            if path.starts_with(".kat") {
                continue;
            }

            match delta.status() {
                git2::Delta::Added => changes.added.push(path.to_path_buf()),
                git2::Delta::Deleted => changes.deleted.push(path.to_path_buf()),
                git2::Delta::Modified | git2::Delta::Typechange => {
                    changes.modified.push(path.to_path_buf())
                }
                git2::Delta::Untracked => changes.untracked.push(path.to_path_buf()),
                git2::Delta::Ignored => changes.ignored.push(path.to_path_buf()),
                git2::Delta::Renamed => {
                    changes.added.push(new_file.path().unwrap().to_path_buf());
                    if let Some(old_path) = old_file.path() {
                        changes.deleted.push(old_path.to_path_buf());
                    }
                }
                _ => {}
            }
        }

        changes.added.sort();
        changes.modified.sort();
        changes.deleted.sort();
        changes.untracked.sort();
        changes.ignored.sort();

        Ok(WorkingState {
            changes,
            backend_consistency,
        })
    }

    fn create_snapshot(
        &self,
        tracked_paths: &[PathBuf],
    ) -> Result<WorkspaceSnapshotId, WorkspaceBackendError> {
        // 1. Validate and normalize desired tracked state
        let mut normalized_paths = Vec::new();
        for p in tracked_paths {
            if p.is_absolute() {
                return Err(WorkspaceBackendError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Absolute paths not allowed in tracked_paths",
                )));
            }
            let p_str = p.to_string_lossy();
            if p_str.contains("..") {
                return Err(WorkspaceBackendError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Paths containing '..' not allowed in tracked_paths",
                )));
            }
            if p.starts_with(".kat") {
                return Err(WorkspaceBackendError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Paths in .kat/ are not allowed in tracked_paths",
                )));
            }
            normalized_paths.push(p.clone());
        }
        normalized_paths.sort();

        for i in 1..normalized_paths.len() {
            if normalized_paths[i - 1] == normalized_paths[i] {
                return Err(WorkspaceBackendError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Duplicate tracked path detected: {:?}", normalized_paths[i]),
                )));
            }
        }

        // 2. Tree construction independent of user index
        let mut index = self._repo.index().map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to open index: {}",
                e
            )))
        })?;
        index.clear().map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to clear index: {}",
                e
            )))
        })?;

        for path in &normalized_paths {
            index.add_path(path).map_err(|e| {
                WorkspaceBackendError::Io(std::io::Error::other(format!(
                    "Failed to add path {:?} to index: {}",
                    path, e
                )))
            })?;
        }
        let tree_oid = index.write_tree().map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to write tree: {}",
                e
            )))
        })?;
        let tree = self._repo.find_tree(tree_oid).map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to find written tree: {}",
                e
            )))
        })?;

        // 3. Lineage-aware synthetic parent
        let mut parents = Vec::new();
        let mut base_snapshot_id = None;

        if let Some(active_oid) = Self::read_active_lineage(&self._project_root)
            && let Ok(parent_commit) = self._repo.find_commit(active_oid)
        {
            if let Ok(parent_tree) = parent_commit.tree() {
                let mut parent_entries = Vec::new();
                if collect_git_tree(
                    &self._repo,
                    &parent_tree,
                    Path::new(""),
                    &mut parent_entries,
                )
                .is_ok()
                {
                    parent_entries.sort_by(|a, b| a.0.cmp(&b.0));
                    base_snapshot_id = Some(crate::encoding::hash::hash_workspace_snapshot(
                        &parent_entries,
                    ));
                }
            }
            parents.push(parent_commit);
        }

        // Compute KAT identity of the new tree
        let mut entries = Vec::new();
        collect_git_tree(&self._repo, &tree, Path::new(""), &mut entries)?;
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let computed_id = crate::encoding::hash::hash_workspace_snapshot(&entries);

        // Check if we can reuse the current lineage's representation
        if let Some(base_id) = base_snapshot_id
            && computed_id == base_id
        {
            // The physical state is unchanged according to KAT identity
            // Reuse the active lineage representation exactly
            let _ = self._repo.set_head_detached(parents[0].id());
            return Ok(computed_id);
        }

        // Create synthetic commit
        let sig = git2::Signature::now("KAT", "kat@localhost").map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to create signature: {}",
                e
            )))
        })?;

        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
        let commit_oid = self
            ._repo
            .commit(
                None, // Create commit without updating HEAD automatically to bypass parent checks
                &sig,
                &sig,
                "KAT synthetic snapshot",
                &tree,
                &parent_refs,
            )
            .map_err(|e| {
                WorkspaceBackendError::Io(std::io::Error::other(format!(
                    "Failed to create synthetic commit: {}",
                    e
                )))
            })?;

        // 5. Post-create identity verification
        let verify_commit = self._repo.find_commit(commit_oid).map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to re-read commit: {}",
                e
            )))
        })?;
        let verify_tree = verify_commit.tree().map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to re-read tree: {}",
                e
            )))
        })?;
        let mut verify_entries = Vec::new();
        collect_git_tree(
            &self._repo,
            &verify_tree,
            Path::new(""),
            &mut verify_entries,
        )?;
        verify_entries.sort_by(|a, b| a.0.cmp(&b.0));
        let verify_id = crate::encoding::hash::hash_workspace_snapshot(&verify_entries);

        if verify_id != computed_id {
            return Err(WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Post-create identity verification failed. Derived: {:?}, Verified: {:?}",
                computed_id, verify_id
            ))));
        }

        // 6. Persist representation mapping
        Self::record_snapshot_representation(&self._project_root, &computed_id, commit_oid)?;

        // 7. Update active lineage to the newly created physical commit
        Self::write_active_lineage(&self._project_root, commit_oid)?;
        let _ = self._repo.set_head_detached(commit_oid);

        Ok(computed_id)
    }

    fn materialize_snapshot(&self, id: &WorkspaceSnapshotId) -> Result<(), WorkspaceBackendError> {
        // 1. Get target representation
        let reps = Self::get_snapshot_representations(&self._project_root, id);
        if reps.is_empty() {
            return Err(WorkspaceBackendError::SnapshotNotFound(id.clone()));
        }

        // Use the representation if unique, otherwise reject ambiguity
        let target_oid = if reps.len() == 1 {
            reps[0].commit
        } else {
            return Err(WorkspaceBackendError::AmbiguousBackendRepresentation(
                id.clone(),
            ));
        };
        let target_commit = self._repo.find_commit(target_oid).map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to find target commit: {}",
                e
            )))
        })?;
        let target_tree = target_commit.tree().map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to find target tree: {}",
                e
            )))
        })?;

        // 2. Prepare the index to represent our current physical lineage
        let mut index = self._repo.index().map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to open index: {}",
                e
            )))
        })?;

        index.clear().map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::other(format!(
                "Failed to clear index: {}",
                e
            )))
        })?;

        if let Some(active_oid) = Self::read_active_lineage(&self._project_root)
            && let Ok(active_commit) = self._repo.find_commit(active_oid)
            && let Ok(active_tree) = active_commit.tree()
        {
            let _ = index.read_tree(&active_tree);
        }
        let _ = index.write();

        // 3. Safe materialization
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.safe(); // Prevents overwriting local modifications

        self._repo
            .checkout_tree(target_tree.as_object(), Some(&mut checkout))
            .map_err(|e| {
                WorkspaceBackendError::Io(std::io::Error::other(format!(
                    "Safe materialization failed (likely incompatible working state): {}",
                    e
                )))
            })?;

        // 4. Recompute / verify resulting KAT physical state
        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.include_untracked(true);
        diff_opts.include_ignored(true);
        diff_opts.recurse_untracked_dirs(true);

        let diff = self
            ._repo
            .diff_tree_to_workdir(Some(&target_tree), Some(&mut diff_opts))
            .map_err(|e| {
                WorkspaceBackendError::Io(std::io::Error::other(format!(
                    "Failed to diff tree for verification: {}",
                    e
                )))
            })?;

        let mut has_changes = false;
        for delta in diff.deltas() {
            let path = delta
                .new_file()
                .path()
                .unwrap_or_else(|| delta.old_file().path().unwrap());
            if !path.starts_with(".kat") {
                has_changes = true;
                break;
            }
        }

        if has_changes {
            return Err(WorkspaceBackendError::Io(std::io::Error::other(
                "Working directory is not clean after materialization",
            )));
        }

        // 5. Update active lineage and HEAD
        Self::write_active_lineage(&self._project_root, target_oid)?;
        let _ = self._repo.set_head_detached(target_oid);

        Ok(())
    }

    fn compare_snapshots(
        &self,
        left: &WorkspaceSnapshotId,
        right: &WorkspaceSnapshotId,
    ) -> Result<PhysicalChanges, WorkspaceBackendError> {
        let mut changes = PhysicalChanges {
            added: Vec::new(),
            modified: Vec::new(),
            deleted: Vec::new(),
            untracked: Vec::new(),
            ignored: Vec::new(),
        };

        if left == right {
            return Ok(changes);
        }

        let left_reps = Self::get_snapshot_representations(&self._project_root, left);
        let right_reps = Self::get_snapshot_representations(&self._project_root, right);

        if left_reps.is_empty() {
            return Err(WorkspaceBackendError::SnapshotNotFound(left.clone()));
        }
        // First valid representation requires verifying snapshot integrity
        if !self.verify_snapshot_integrity(left)? {
            return Err(WorkspaceBackendError::SnapshotIntegrity(left.clone()));
        }

        if right_reps.is_empty() {
            return Err(WorkspaceBackendError::SnapshotNotFound(right.clone()));
        }
        if !self.verify_snapshot_integrity(right)? {
            return Err(WorkspaceBackendError::SnapshotIntegrity(right.clone()));
        }

        let left_commit = self
            ._repo
            .find_commit(left_reps[0].commit)
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e.to_string())))?;
        let right_commit = self
            ._repo
            .find_commit(right_reps[0].commit)
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e.to_string())))?;

        let left_tree = left_commit.tree().unwrap();
        let right_tree = right_commit.tree().unwrap();

        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.include_untracked(true);

        let diff = self
            ._repo
            .diff_tree_to_tree(Some(&left_tree), Some(&right_tree), Some(&mut diff_opts))
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e.to_string())))?;

        for delta in diff.deltas() {
            let path = delta
                .new_file()
                .path()
                .unwrap_or_else(|| delta.old_file().path().unwrap());
            if path.starts_with(".kat") {
                continue;
            }
            let pb = path.to_path_buf();
            match delta.status() {
                git2::Delta::Added => changes.added.push(pb),
                git2::Delta::Deleted => changes.deleted.push(pb),
                git2::Delta::Modified
                | git2::Delta::Typechange
                | git2::Delta::Renamed
                | git2::Delta::Copied => changes.modified.push(pb),
                git2::Delta::Untracked => changes.untracked.push(pb),
                git2::Delta::Ignored => changes.ignored.push(pb),
                _ => changes.modified.push(pb),
            }
        }

        changes.added.sort();
        changes.modified.sort();
        changes.deleted.sort();
        changes.untracked.sort();
        changes.ignored.sort();

        changes.added.dedup();
        changes.modified.dedup();
        changes.deleted.dedup();
        changes.untracked.dedup();
        changes.ignored.dedup();

        Ok(changes)
    }

    fn resolve_materialization(
        &self,
        path: &Path,
        snapshot: &WorkspaceSnapshotId,
    ) -> Result<MaterializationResolution, WorkspaceBackendError> {
        if path.starts_with(".kat") {
            return Ok(MaterializationResolution::NotFound);
        }

        let reps = Self::get_snapshot_representations(&self._project_root, snapshot);
        if reps.is_empty() {
            return Err(WorkspaceBackendError::SnapshotNotFound(snapshot.clone()));
        }

        let commit_oid = reps[0].commit;
        let commit = self
            ._repo
            .find_commit(commit_oid)
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e.to_string())))?;
        let tree = commit
            .tree()
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e.to_string())))?;

        let mut entries = Vec::new();
        collect_git_tree(&self._repo, &tree, Path::new(""), &mut entries)?;

        let target_str = path
            .to_str()
            .ok_or_else(|| WorkspaceBackendError::UnsupportedPathEncoding(path.to_path_buf()))?;

        // Check for exact file/symlink match
        for (p, type_byte, mat_id) in &entries {
            if p == target_str {
                if *type_byte == b'S' {
                    return Ok(MaterializationResolution::Symlink(*mat_id));
                } else {
                    return Ok(MaterializationResolution::File(*mat_id));
                }
            }
        }

        // Check for directory match
        let mut children = Vec::new();
        let mut is_dir = false;

        for (p, type_byte, mat_id) in entries {
            if target_str.is_empty() || (p.starts_with(&target_str.to_string()) && p != target_str)
            {
                is_dir = true;
                children.push((p, type_byte, mat_id));
            }
        }

        if is_dir {
            children.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(MaterializationResolution::Directory(
                crate::encoding::hash::hash_directory_materialization(&children),
            ))
        } else {
            Ok(MaterializationResolution::NotFound)
        }
    }

    fn resolve_working_materialization(
        &self,
        path: &Path,
    ) -> Result<MaterializationResolution, WorkspaceBackendError> {
        crate::repository::workspace::fs_resolve_working_materialization(&self._project_root, path)
    }

    fn verify_snapshot_integrity(
        &self,
        id: &WorkspaceSnapshotId,
    ) -> Result<bool, WorkspaceBackendError> {
        let representations = Self::get_snapshot_representations(&self._project_root, id);
        if representations.is_empty() {
            return Err(WorkspaceBackendError::SnapshotNotFound(id.clone()));
        }

        for rep in representations {
            let commit = self._repo.find_commit(rep.commit).map_err(|e| {
                WorkspaceBackendError::Io(std::io::Error::other(format!(
                    "Failed to find commit {}: {}",
                    rep.commit, e
                )))
            })?;
            let tree = commit.tree().map_err(|e| {
                WorkspaceBackendError::Io(std::io::Error::other(format!(
                    "Failed to get tree for commit {}: {}",
                    rep.commit, e
                )))
            })?;

            let mut entries = Vec::new();
            collect_git_tree(&self._repo, &tree, Path::new(""), &mut entries)?;
            entries.sort_by(|a, b| a.0.cmp(&b.0));

            let computed_id = crate::encoding::hash::hash_workspace_snapshot(&entries);
            if computed_id.as_bytes() != id.as_bytes() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn reconcile_physical(
        &self,
        base: &WorkspaceSnapshotId,
        local: &WorkspaceSnapshotId,
        other: &WorkspaceSnapshotId,
    ) -> Result<crate::domain::workspace::PhysicalReconciliationResult, WorkspaceBackendError> {
        use crate::domain::conflict::{MaterializationConflict, MaterializationConflictKind};
        use crate::domain::identity::PhysicalCandidateId;
        use crate::domain::workspace::{
            PhysicalReconciliationCandidate, PhysicalReconciliationResult,
        };
        use std::collections::{BTreeMap, BTreeSet};
        use std::path::PathBuf;

        let b_reps = Self::get_snapshot_representations(&self._project_root, base);
        let l_reps = Self::get_snapshot_representations(&self._project_root, local);
        let o_reps = Self::get_snapshot_representations(&self._project_root, other);

        let b_commit = self
            ._repo
            .find_commit(
                b_reps
                    .first()
                    .ok_or_else(|| WorkspaceBackendError::SnapshotNotFound(base.clone()))?
                    .commit,
            )
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;
        let l_commit = self
            ._repo
            .find_commit(
                l_reps
                    .first()
                    .ok_or_else(|| WorkspaceBackendError::SnapshotNotFound(local.clone()))?
                    .commit,
            )
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;
        let o_commit = self
            ._repo
            .find_commit(
                o_reps
                    .first()
                    .ok_or_else(|| WorkspaceBackendError::SnapshotNotFound(other.clone()))?
                    .commit,
            )
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;

        let b_tree = b_commit
            .tree()
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;
        let l_tree = l_commit
            .tree()
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;
        let o_tree = o_commit
            .tree()
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;

        let mut merge_opts = git2::MergeOptions::new();
        merge_opts.find_renames(false);
        let index = self
            ._repo
            .merge_trees(&b_tree, &l_tree, &o_tree, Some(&merge_opts))
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;

        let mut entries_to_keep = BTreeMap::new();
        let mut conflicts = Vec::new();
        let mut state_conflicts = Vec::new();
        let mut paths_handled = BTreeSet::new();

        if index.has_conflicts() {
            let index_conflicts = index
                .conflicts()
                .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;
            for c in index_conflicts {
                let conflict =
                    c.map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;

                let path_bytes = conflict
                    .our
                    .as_ref()
                    .or(conflict.their.as_ref())
                    .or(conflict.ancestor.as_ref())
                    .unwrap()
                    .path
                    .clone();
                let path = PathBuf::from(std::str::from_utf8(&path_bytes).unwrap());

                paths_handled.insert(path.clone());

                let kind = match (&conflict.ancestor, &conflict.our, &conflict.their) {
                    (Some(_), None, Some(_)) | (Some(_), Some(_), None) => {
                        MaterializationConflictKind::DeleteModify
                    }
                    (Some(_), None, None) => {
                        continue;
                    }
                    (_, Some(l), Some(o)) => {
                        if l.mode != o.mode {
                            MaterializationConflictKind::TypeChange
                        } else {
                            MaterializationConflictKind::Content
                        }
                    }
                    (None, Some(l), None) => {
                        // Libgit2 emitted a conflict due to a D/F collision with another path.
                        // According to Kat semantics, this is a clean add by local. We let the
                        // subsequent PathCollision logic detect the D/F conflict naturally.
                        entries_to_keep.insert(path.clone(), (l.id, l.mode as i32));
                        continue;
                    }
                    (None, None, Some(o)) => {
                        // Same as above, clean add by other.
                        entries_to_keep.insert(path.clone(), (o.id, o.mode as i32));
                        continue;
                    }
                    _ => MaterializationConflictKind::Content,
                };

                conflicts.push(MaterializationConflict {
                    kind: kind.clone(),
                    paths: vec![path.clone()],
                });

                let entry_state = |e: &Option<git2::IndexEntry>| {
                    e.as_ref().map(|x| GitCandidateEntry {
                        path: path.clone(),
                        oid: x.id.to_string(),
                        mode: x.mode,
                    })
                };

                state_conflicts.push(GitConflictReconstructionData {
                    kind: kind.clone(),
                    paths: vec![path.clone()],
                    base: entry_state(&conflict.ancestor),
                    local: entry_state(&conflict.our),
                    other: entry_state(&conflict.their),
                });

                if let Some(base_entry) = conflict.ancestor {
                    entries_to_keep.insert(path, (base_entry.id, base_entry.mode as i32));
                }
            }
        }

        for entry in index.iter() {
            // Unconflicted entries have stage == 0, which corresponds to bits 12-13 being 0
            if (entry.flags & 0x3000) == 0 {
                let path = PathBuf::from(std::str::from_utf8(&entry.path).unwrap());
                if !paths_handled.contains(&path) {
                    entries_to_keep.insert(path, (entry.id, entry.mode as i32));
                }
            }
        }

        let mut clean_paths: Vec<_> = entries_to_keep.keys().cloned().collect();
        clean_paths.sort();

        let mut paths_to_remove = BTreeSet::new();
        for i in 0..clean_paths.len() {
            let p1 = &clean_paths[i];
            for j in (i + 1)..clean_paths.len() {
                let p2 = &clean_paths[j];
                if p2.starts_with(p1) {
                    conflicts.push(MaterializationConflict {
                        kind: MaterializationConflictKind::PathCollision,
                        paths: vec![p1.clone(), p2.clone()],
                    });

                    let b1 = b_tree.get_path(p1).ok().map(|e| GitCandidateEntry {
                        path: p1.clone(),
                        oid: e.id().to_string(),
                        mode: e.filemode() as u32,
                    });
                    let l1 = l_tree.get_path(p1).ok().map(|e| GitCandidateEntry {
                        path: p1.clone(),
                        oid: e.id().to_string(),
                        mode: e.filemode() as u32,
                    });
                    let o1 = o_tree.get_path(p1).ok().map(|e| GitCandidateEntry {
                        path: p1.clone(),
                        oid: e.id().to_string(),
                        mode: e.filemode() as u32,
                    });

                    let b2 = b_tree.get_path(p2).ok().map(|e| GitCandidateEntry {
                        path: p2.clone(),
                        oid: e.id().to_string(),
                        mode: e.filemode() as u32,
                    });
                    let l2 = l_tree.get_path(p2).ok().map(|e| GitCandidateEntry {
                        path: p2.clone(),
                        oid: e.id().to_string(),
                        mode: e.filemode() as u32,
                    });
                    let o2 = o_tree.get_path(p2).ok().map(|e| GitCandidateEntry {
                        path: p2.clone(),
                        oid: e.id().to_string(),
                        mode: e.filemode() as u32,
                    });

                    state_conflicts.push(GitConflictReconstructionData {
                        kind: MaterializationConflictKind::PathCollision,
                        paths: vec![p1.clone(), p2.clone()],
                        base: b1,
                        local: l1,
                        other: o1,
                    });
                    state_conflicts.push(GitConflictReconstructionData {
                        kind: MaterializationConflictKind::PathCollision,
                        paths: vec![p1.clone(), p2.clone()],
                        base: b2,
                        local: l2,
                        other: o2,
                    });

                    paths_to_remove.insert(p1.clone());
                    paths_to_remove.insert(p2.clone());
                } else {
                    break;
                }
            }
        }

        if !paths_to_remove.is_empty() {
            for p in paths_to_remove {
                entries_to_keep.remove(&p);
                if let Ok(b_entry) = b_tree.get_path(&p) {
                    entries_to_keep.insert(p.clone(), (b_entry.id(), b_entry.filemode()));
                }
            }
        }

        let prov_tree_oid =
            build_git_tree_recursive(&self._repo, &entries_to_keep, std::path::Path::new(""))?;

        if conflicts.is_empty() {
            let prov_tree = self
                ._repo
                .find_tree(prov_tree_oid)
                .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;
            let mut entries = Vec::new();
            collect_git_tree(
                &self._repo,
                &prov_tree,
                std::path::Path::new(""),
                &mut entries,
            )?;
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let new_snapshot_id = crate::domain::identity::WorkspaceSnapshotId::new(
                crate::encoding::hash::hash_workspace_snapshot(&entries).into_bytes(),
            );

            let sig = self
                ._repo
                .signature()
                .unwrap_or_else(|_| git2::Signature::now("kat", "kat@kat").unwrap());
            self._repo
                .commit(
                    None, // Do not update HEAD or references
                    &sig,
                    &sig,
                    "KAT Reconciliation Clean",
                    &prov_tree,
                    &[&l_commit, &o_commit],
                )
                .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;

            Ok(PhysicalReconciliationResult::Clean {
                snapshot: new_snapshot_id,
            })
        } else {
            conflicts.sort_by(|a, b| a.paths.cmp(&b.paths).then_with(|| a.kind.cmp(&b.kind)));

            let phys_id = PhysicalCandidateId::new();

            let state = GitPhysicalCandidateState {
                version: 1,
                base_snapshot: base.to_hex(),
                local_snapshot: local.to_hex(),
                other_snapshot: other.to_hex(),
                provisional_tree: prov_tree_oid.to_string(),
                conflicts: state_conflicts,
            };

            self.transient_candidates
                .write()
                .unwrap()
                .insert(phys_id, state);

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
    fn persist_physical_candidate(
        &self,
        workspace_id: &crate::domain::workspace::WorkspaceId,
        candidate: &crate::domain::workspace::PhysicalReconciliationCandidate,
    ) -> Result<(), WorkspaceBackendError> {
        let candidates_dir = self
            ._project_root
            .join(".kat/workspaces")
            .join(&workspace_id.0)
            .join("physical-reconciliation")
            .join(candidate.provisional.to_string());
        std::fs::create_dir_all(&candidates_dir).map_err(WorkspaceBackendError::Io)?;
        let state_path = candidates_dir.join("git.json");
        let state = self
            .transient_candidates
            .read()
            .unwrap()
            .get(&candidate.provisional)
            .cloned()
            .ok_or_else(|| {
                WorkspaceBackendError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Transient candidate state not found",
                ))
            })?;

        let json = serde_json::to_string_pretty(&state)
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;
        std::fs::write(&state_path, json.as_bytes()).map_err(WorkspaceBackendError::Io)?;

        let prov_tree_oid = git2::Oid::from_str(&state.provisional_tree)
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;
        let prov_tree = self
            ._repo
            .find_tree(prov_tree_oid)
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;

        let sig = self
            ._repo
            .signature()
            .unwrap_or_else(|_| git2::Signature::now("kat", "kat@kat").unwrap());
        let commit_oid = self
            ._repo
            .commit(
                None,
                &sig,
                &sig,
                "KAT Provisional Tree Protection",
                &prov_tree,
                &[],
            )
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;

        let ref_name = format!("refs/kat/candidates/{}", candidate.provisional);
        self._repo
            .reference(
                &ref_name,
                commit_oid,
                true,
                "KAT physical candidate protection",
            )
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;

        Ok(())
    }
}

impl GitWorkspaceBackend {
    pub fn get_candidate_state(
        &self,
        workspace_id: &crate::domain::workspace::WorkspaceId,
        id: &crate::domain::identity::PhysicalCandidateId,
    ) -> Result<GitPhysicalCandidateState, WorkspaceBackendError> {
        let state_path = self
            ._project_root
            .join(".kat/workspaces")
            .join(&workspace_id.0)
            .join("physical-reconciliation")
            .join(id.to_string())
            .join("git.json");
        let json =
            std::fs::read_to_string(&state_path).map_err(WorkspaceBackendError::Io)?;
        serde_json::from_str(&json).map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))
    }
}

fn build_git_tree_recursive(
    repo: &git2::Repository,
    entries: &std::collections::BTreeMap<std::path::PathBuf, (git2::Oid, i32)>,
    current_prefix: &std::path::Path,
) -> Result<git2::Oid, WorkspaceBackendError> {
    let mut tb = repo
        .treebuilder(None)
        .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;

    let mut groups: std::collections::BTreeMap<
        std::ffi::OsString,
        std::collections::BTreeMap<std::path::PathBuf, (git2::Oid, i32)>,
    > = std::collections::BTreeMap::new();

    for (path, val) in entries {
        let rel_path = path.strip_prefix(current_prefix).unwrap();
        let mut components = rel_path.components();
        if let Some(comp) = components.next() {
            let comp_os = comp.as_os_str().to_os_string();
            if components.next().is_none() {
                tb.insert(comp_os.to_str().unwrap(), val.0, val.1)
                    .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;
            } else {
                groups
                    .entry(comp_os)
                    .or_default()
                    .insert(path.clone(), *val);
            }
        }
    }

    for (dir_name, sub_entries) in groups {
        let subtree_oid =
            build_git_tree_recursive(repo, &sub_entries, &current_prefix.join(&dir_name))?;
        tb.insert(dir_name.to_str().unwrap(), subtree_oid, 0o040000)
            .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))?;
    }

    tb.write()
        .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e)))
}

pub fn collect_git_tree(
    repo: &git2::Repository,
    tree: &git2::Tree,
    prefix: &Path,
    entries: &mut Vec<(String, u8, MaterializationId)>,
) -> Result<(), WorkspaceBackendError> {
    for entry in tree.iter() {
        let name = std::str::from_utf8(entry.name_bytes()).map_err(|_| {
            WorkspaceBackendError::UnsupportedPathEncoding(
                prefix.join(String::from_utf8_lossy(entry.name_bytes()).as_ref()),
            )
        })?;

        if name == ".kat" && prefix == Path::new("") {
            continue;
        }

        let path = prefix.join(name);
        let path_str = path
            .to_str()
            .ok_or_else(|| WorkspaceBackendError::UnsupportedPathEncoding(path.clone()))?
            .to_string();

        match entry.filemode() {
            0o100644 => {
                let obj = entry
                    .to_object(repo)
                    .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e.to_string())))?;
                let blob = obj.as_blob().unwrap();
                let mat_id =
                    crate::encoding::hash::hash_file_materialization(false, blob.content());
                entries.push((path_str, b'F', mat_id));
            }
            0o100755 => {
                let obj = entry
                    .to_object(repo)
                    .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e.to_string())))?;
                let blob = obj.as_blob().unwrap();
                let mat_id = crate::encoding::hash::hash_file_materialization(true, blob.content());
                entries.push((path_str, b'X', mat_id));
            }
            0o120000 => {
                let obj = entry
                    .to_object(repo)
                    .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e.to_string())))?;
                let blob = obj.as_blob().unwrap();
                let mat_id = crate::encoding::hash::hash_symlink_materialization(blob.content());
                entries.push((path_str, b'S', mat_id));
            }
            0o040000 => {
                let obj = entry
                    .to_object(repo)
                    .map_err(|e| WorkspaceBackendError::Io(std::io::Error::other(e.to_string())))?;
                let subtree = obj.as_tree().unwrap();
                collect_git_tree(repo, subtree, &path, entries)?;
            }
            0o160000 => {
                return Err(WorkspaceBackendError::UnsupportedPhysicalEntryType);
            }
            mode => {
                return Err(WorkspaceBackendError::Io(std::io::Error::other(format!(
                    "Unsupported git file mode: {:o}",
                    mode
                ))));
            }
        }
    }
    Ok(())
}
