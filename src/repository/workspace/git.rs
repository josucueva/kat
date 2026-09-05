use std::path::{Path, PathBuf};
use std::fs;
use git2::Repository;

use crate::domain::identity::{MaterializationId, WorkspaceSnapshotId};
use crate::domain::workspace::{
    BackendConsistency, MaterializationResolution, WorkingState, WorkspaceBackend,
    WorkspaceBackendError,
};

/// A specific Git commit and ancestry context representing a snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendSnapshotRepresentation {
    pub commit: git2::Oid,
}

/// A WorkspaceBackend that uses a Git repository stored inside `.kat/physical/git/`.
pub struct GitWorkspaceBackend {
    _repo: Repository,
    _project_root: PathBuf,
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
            WorkspaceBackendError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to open Git repository: {}", e),
            ))
        })?;

        Ok(Self {
            _repo: repo,
            _project_root: project_root.to_path_buf(),
        })
    }

    /// Initializes a new GitWorkspaceBackend at the given project root.
    pub fn init(project_root: &Path) -> Result<Self, WorkspaceBackendError> {
        let kat_git_dir = project_root.join(".kat/physical/git");
        
        let repo = Repository::init_bare(&kat_git_dir).map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to initialize Git repository: {}", e),
            ))
        })?;

        Ok(Self {
            _repo: repo,
            _project_root: project_root.to_path_buf(),
        })
    }

    /// Adopts an existing Git repository using Managed Import and Cutover.
    pub fn adopt(project_root: &Path) -> Result<Self, WorkspaceBackendError> {
        let kat_git_dir = project_root.join(".kat/physical/git");
        let original_git_dir = project_root.join(".git");
        let backup_git_dir = project_root.join(".kat/adoption-backup/original-git");

        if kat_git_dir.exists() {
            return Err(WorkspaceBackendError::Io(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "KAT physical Git repository already exists",
            )));
        if !original_git_dir.exists() {
            return Self::init(project_root);
        }

        // 1. Inspect / Pre-validate (Check if we can open it)
        if let Err(e) = Repository::open(&original_git_dir) {
            return Err(WorkspaceBackendError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to inspect original Git repository: {}", e),
            )));
        }

        // 2. Import: Copy the .git directory to .kat/physical/git
        if let Err(e) = copy_dir_all(&original_git_dir, &kat_git_dir) {
            // Clean up partial import
            let _ = fs::remove_dir_all(&kat_git_dir);
            return Err(WorkspaceBackendError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to import Git repository: {}", e),
            )));
        }

        // 3. Validate the imported repository
        let repo = match Repository::open(&kat_git_dir) {
            Ok(r) => r,
            Err(e) => {
                let _ = fs::remove_dir_all(&kat_git_dir);
                return Err(WorkspaceBackendError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Imported Git repository failed validation: {}", e),
                )));
            }
        };

        // 4. Snapshot / Verify Working Tree Equivalence
        // TODO: In Phase 3.4 we will create the initial WorkspaceSnapshot here.
        
        // 5. Atomic Cutover
        fs::create_dir_all(backup_git_dir.parent().unwrap()).map_err(|e| {
            WorkspaceBackendError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create backup directory: {}", e),
            ))
        })?;

        if let Err(e) = fs::rename(&original_git_dir, &backup_git_dir) {
            let _ = fs::remove_dir_all(&kat_git_dir);
            return Err(WorkspaceBackendError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to cut over Git repository: {}", e),
            )));
        }

        Ok(Self {
            _repo: repo,
            _project_root: project_root.to_path_buf(),
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
            std::fs::create_dir_all(parent).map_err(|e| WorkspaceBackendError::Io(e))?;
        }
        let content = serde_json::to_string_pretty(&index).unwrap();
        std::fs::write(index_path, content).map_err(WorkspaceBackendError::Io)
    }

    /// Loads the representation mapping for a given WorkspaceSnapshotId
    pub fn get_snapshot_representations(
        project_root: &Path,
        snapshot_id: &WorkspaceSnapshotId,
    ) -> Vec<BackendSnapshotRepresentation> {
        let index_path = project_root.join(".kat/physical/snapshots.json");
        if !index_path.exists() {
            return Vec::new();
        }
        let content = match std::fs::read_to_string(&index_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let index: std::collections::HashMap<String, Vec<String>> = 
            serde_json::from_str(&content).unwrap_or_default();

        index.get(&snapshot_id.to_hex())
            .map(|commits| {
                commits.iter()
                    .filter_map(|c| git2::Oid::from_str(c).ok())
                    .map(|oid| BackendSnapshotRepresentation { commit: oid })
                    .collect()
            })
            .unwrap_or_default()
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
        _base: &WorkspaceSnapshotId,
    ) -> Result<WorkingState, WorkspaceBackendError> {
        // TODO: Implement proper Git statuses and BackendConsistency logic
        Ok(WorkingState {
            backend_consistency: BackendConsistency::Consistent,
            added: Vec::new(),
            modified: Vec::new(),
            deleted: Vec::new(),
            untracked: Vec::new(),
            ignored: Vec::new(),
        })
    }

    fn create_snapshot(&self, _tracked_paths: &[PathBuf]) -> Result<WorkspaceSnapshotId, WorkspaceBackendError> {
        // TODO: Implement temporary index, tree building, and KAT hashing
        unimplemented!()
    }

    fn materialize_snapshot(&self, _id: &WorkspaceSnapshotId) -> Result<(), WorkspaceBackendError> {
        // TODO: Safe materialization only
        unimplemented!()
    }

    fn compare_snapshots(
        &self,
        _left: &WorkspaceSnapshotId,
        _right: &WorkspaceSnapshotId,
    ) -> Result<Vec<PathBuf>, WorkspaceBackendError> {
        // TODO: git2 diff
        unimplemented!()
    }

    fn resolve_materialization(
        &self,
        _path: &Path,
        _snapshot: &WorkspaceSnapshotId,
    ) -> Result<MaterializationResolution, WorkspaceBackendError> {
        // TODO: Git tree traversal and materialization hashing
        unimplemented!()
    }

    fn verify_snapshot_integrity(
        &self,
        _id: &WorkspaceSnapshotId,
    ) -> Result<bool, WorkspaceBackendError> {
        // TODO: Recompute hash from Git tree
        unimplemented!()
    }
}
