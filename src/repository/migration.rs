//! Explicit v0.4-to-v0.5 migration and shared root-workspace bootstrap.

use std::io;
use std::path::Path;

use crate::domain::identity::RepositoryRevisionId;
use crate::domain::revision::RepositoryRevision;
use crate::domain::workspace::{Workspace, WorkspaceBackend, WorkspaceBackendError, WorkspaceId};
use crate::encoding::canonical_bytes;
use crate::encoding::object::{CanonicalObject, CanonicalPayload};
use crate::encoding::validate::CanonicalStructureError;
use crate::repository::error::RepositoryError;
use crate::repository::init::{
    InitResult, init_repository_for_bootstrap, resume_pending_bootstrap,
};
use crate::repository::metadata::{MetadataError, RepositoryMetadata, SUPPORTED_FORMAT_VERSION};
use crate::repository::object_store::ObjectStoreError;
use crate::repository::open::{open_repository, open_repository_for_migration};
use crate::repository::remote::{RemoteError, verify_revision_closure};
use crate::repository::workspace::git::GitWorkspaceBackend;
use crate::repository::workspace::state::{
    WorkspaceStateError, read_workspace_state, write_workspace_state_atomic,
};

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Encoding(#[from] CanonicalStructureError),
    #[error(transparent)]
    ObjectStore(#[from] ObjectStoreError),
    #[error(transparent)]
    Metadata(#[from] MetadataError),
    #[error(transparent)]
    Backend(#[from] WorkspaceBackendError),
    #[error(transparent)]
    WorkspaceState(#[from] WorkspaceStateError),
    #[error(transparent)]
    Remote(#[from] RemoteError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("existing workspace is based on {actual}, expected bootstrap revision {expected}")]
    WorkspaceBaseMismatch {
        expected: RepositoryRevisionId,
        actual: RepositoryRevisionId,
    },
    #[error(
        "repository format 2 is missing a valid workspace; refusing to mark migration complete"
    )]
    IncompleteCurrentRepository,
    #[error("workspace state did not persist the bootstrap revision before format upgrade")]
    IncompleteBootstrapWorkspace,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MigrationOutcome {
    Migrated(RepositoryRevisionId),
    AlreadyMigrated(RepositoryRevisionId),
}

/// Initializes a user-facing KAT repository, including its root revision and
/// workspace. The legacy marker keeps an interrupted bootstrap closed to
/// normal repository operations until `kat migrate` resumes it.
pub fn initialize_v05_repository(
    root: &Path,
) -> Result<(InitResult, RepositoryRevisionId), MigrationError> {
    let initialized = init_repository_for_bootstrap(root)?;
    let revision = finish_migration(root)?;
    Ok((initialized, revision))
}

/// Explicitly upgrades a v0.4 repository. A completed format-2 repository is
/// verified and reported without changing its state.
pub fn migrate_repository(root: &Path) -> Result<MigrationOutcome, MigrationError> {
    let metadata_path = root.join(".kat/repository.toml");
    let _ = resume_pending_bootstrap(root)?;
    let metadata = RepositoryMetadata::read_for_migration(&metadata_path)?;
    if metadata.format_version == SUPPORTED_FORMAT_VERSION {
        return Ok(MigrationOutcome::AlreadyMigrated(
            verify_completed_repository(root)?,
        ));
    }

    Ok(MigrationOutcome::Migrated(finish_migration(root)?))
}

fn finish_migration(root: &Path) -> Result<RepositoryRevisionId, MigrationError> {
    let repo = open_repository_for_migration(root)?;
    let backend = open_or_adopt_backend(root)?;
    let paths = backend.initial_snapshot_paths()?;
    let snapshot = backend.create_snapshot(&paths)?;
    if !backend.verify_snapshot_integrity(&snapshot)? {
        return Err(WorkspaceBackendError::SnapshotIntegrity(snapshot).into());
    }

    let revision = RepositoryRevision {
        parents: Vec::new(),
        semantic_state: crate::domain::identity::SemanticStateId::from_object_id(
            repo.accepted.state,
        ),
        workspace_snapshot: snapshot.clone(),
        semantic_change: repo
            .accepted
            .change
            .map(crate::domain::identity::ChangeRevisionId::from_object_id),
    };
    let bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::RepositoryRevision(revision),
    })?;
    let revision_id = RepositoryRevisionId::from_object_id(repo.object_store().put(&bytes)?);
    let closure_snapshots = verify_revision_closure(revision_id, |id| {
        repo.object_store().get(id).map_err(RemoteError::from)
    })?;
    if !closure_snapshots.contains(&snapshot) {
        return Err(WorkspaceBackendError::SnapshotIntegrity(snapshot).into());
    }

    let workspace = match read_workspace_state(root) {
        Ok(workspace) if workspace.base_revision == revision_id => workspace,
        Ok(workspace) => {
            return Err(MigrationError::WorkspaceBaseMismatch {
                expected: revision_id,
                actual: workspace.base_revision,
            });
        }
        Err(WorkspaceStateError::NotFound) => {
            let workspace = Workspace {
                id: WorkspaceId(uuid::Uuid::new_v4().to_string()),
                base_revision: revision_id,
            };
            write_workspace_state_atomic(root, &workspace)?;
            workspace
        }
        Err(error) => return Err(error.into()),
    };
    if workspace.base_revision != revision_id {
        return Err(MigrationError::WorkspaceBaseMismatch {
            expected: revision_id,
            actual: workspace.base_revision,
        });
    }
    if read_workspace_state(root)? != workspace {
        return Err(MigrationError::IncompleteBootstrapWorkspace);
    }
    if !backend.verify_snapshot_integrity(&snapshot)? {
        return Err(WorkspaceBackendError::SnapshotIntegrity(snapshot).into());
    }
    let persisted_revision = repo.read_revision(revision_id)?;
    if persisted_revision.semantic_state.as_object_id() != repo.accepted.state
        || persisted_revision.semantic_change
            != repo
                .accepted
                .change
                .map(crate::domain::identity::ChangeRevisionId::from_object_id)
    {
        return Err(MigrationError::IncompleteBootstrapWorkspace);
    }

    let mut upgraded = repo.metadata.clone();
    upgraded.format_version = SUPPORTED_FORMAT_VERSION;
    upgraded.write_atomic(&root.join(".kat/repository.toml"))?;

    Ok(revision_id)
}

fn open_or_adopt_backend(root: &Path) -> Result<GitWorkspaceBackend, MigrationError> {
    let private_backend = root.join(".kat/physical/git");
    if private_backend.exists() {
        return Ok(GitWorkspaceBackend::open(root)?);
    }

    let project_git = root.join(".git");
    if project_git.is_dir() {
        return Ok(GitWorkspaceBackend::adopt(root)?);
    }
    Ok(GitWorkspaceBackend::init(root)?)
}

fn verify_completed_repository(root: &Path) -> Result<RepositoryRevisionId, MigrationError> {
    let repo = open_repository(root)?;
    let workspace =
        read_workspace_state(root).map_err(|_| MigrationError::IncompleteCurrentRepository)?;
    let revision = repo
        .read_revision(workspace.base_revision)
        .map_err(|_| MigrationError::IncompleteCurrentRepository)?;
    let backend =
        GitWorkspaceBackend::open(root).map_err(|_| MigrationError::IncompleteCurrentRepository)?;
    if !backend
        .verify_snapshot_integrity(&revision.workspace_snapshot)
        .map_err(|_| MigrationError::IncompleteCurrentRepository)?
    {
        return Err(MigrationError::IncompleteCurrentRepository);
    }
    verify_revision_closure(workspace.base_revision, |id| {
        repo.object_store().get(id).map_err(RemoteError::from)
    })?;
    Ok(workspace.base_revision)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::change::ChangeRevision;
    use crate::domain::identity::{ChangeId, ChangeRevisionId, ElementId, RepositoryRevisionId};
    use crate::domain::operation::Operation;
    use crate::repository::init::init_repository;
    use crate::repository::metadata::{LEGACY_FORMAT_VERSION, RepositoryMetadata};
    use crate::repository::ref_store::{AcceptedRef, FileRefStore};
    use std::fs;

    fn set_format(root: &Path, format_version: u32) {
        let path = root.join(".kat/repository.toml");
        let mut metadata = RepositoryMetadata::read_for_migration(&path).unwrap();
        metadata.format_version = format_version;
        metadata.write_atomic(&path).unwrap();
    }

    #[test]
    fn migration_creates_root_workspace_and_excludes_repository_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("src/.git")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("src/.git/config"), "must not be captured").unwrap();
        fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(root.join("ignored.txt"), "ignored").unwrap();
        let initialized = init_repository(root).unwrap();
        set_format(root, LEGACY_FORMAT_VERSION);

        let outcome = migrate_repository(root).unwrap();
        let MigrationOutcome::Migrated(revision_id) = outcome else {
            panic!("expected a new migration")
        };
        let repo = open_repository(root).unwrap();
        let revision = repo.read_revision(revision_id).unwrap();
        let backend = GitWorkspaceBackend::open(root).unwrap();
        let paths = backend.initial_snapshot_paths().unwrap();

        assert_eq!(revision.parents, Vec::<RepositoryRevisionId>::new());
        assert_eq!(revision.semantic_state.as_object_id(), initialized.state);
        assert_eq!(revision.semantic_change, None);
        assert!(paths.contains(&Path::new("src/main.rs").to_path_buf()));
        assert!(!paths.iter().any(|path| {
            path.components()
                .any(|component| component.as_os_str() == ".git")
        }));
        assert!(!paths.iter().any(|path| {
            path.components()
                .any(|component| component.as_os_str() == ".kat")
        }));
        assert!(!paths.contains(&Path::new("ignored.txt").to_path_buf()));
        assert_eq!(
            read_workspace_state(root).unwrap().base_revision,
            revision_id
        );
    }

    #[test]
    fn migration_adopts_existing_git_project_and_preserves_its_tree() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(root.join("ignored.txt"), "ignored\n").unwrap();

        let git = git2::Repository::init(root).unwrap();
        let mut index = git.index().unwrap();
        index.add_path(Path::new("src/main.rs")).unwrap();
        index.add_path(Path::new(".gitignore")).unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = git.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("test", "test@example.com").unwrap();
        git.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        drop(index);
        drop(git);
        fs::write(root.join("src/main.rs"), "uncommitted migration state\n").unwrap();

        init_repository(root).unwrap();
        set_format(root, LEGACY_FORMAT_VERSION);
        let MigrationOutcome::Migrated(revision_id) = migrate_repository(root).unwrap() else {
            panic!("expected migration")
        };
        let repo = open_repository(root).unwrap();
        let revision = repo.read_revision(revision_id).unwrap();
        let physical = git2::Repository::open(root.join(".kat/physical/git")).unwrap();
        let representation =
            GitWorkspaceBackend::get_snapshot_representations(root, &revision.workspace_snapshot)
                .unwrap()
                .remove(0);
        let commit = physical.find_commit(representation.commit).unwrap();
        let tree = commit.tree().unwrap();
        let main = tree.get_path(Path::new("src/main.rs")).unwrap();
        let blob = physical.find_blob(main.id()).unwrap();

        assert_eq!(blob.content(), b"uncommitted migration state\n");
        assert!(tree.get_path(Path::new(".kat")).is_err());
        assert!(tree.get_path(Path::new(".git")).is_err());
        assert!(tree.get_path(Path::new("ignored.txt")).is_err());
        assert!(!root.join(".git").exists());
        assert!(root.join(".kat/adoption-backup/original-git").exists());
    }

    #[test]
    fn migration_verifies_the_full_accepted_change_chain() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let initialized = init_repository(root).unwrap();
        let store = crate::repository::object_store::ObjectStore::new(root.join(".kat"));
        let mut previous = None;
        let mut change_ids = Vec::new();
        for _ in 0..3 {
            let change = ChangeRevision {
                change_id: ChangeId::new(),
                base_states: vec![initialized.state],
                result_state: initialized.state,
                operations: vec![Operation::AccountArtifact {
                    artifact_id: ElementId::new(),
                    reconciliations: vec![],
                }],
                dependencies: previous.into_iter().collect(),
                description: None,
            };
            let bytes = canonical_bytes(&CanonicalObject {
                payload: CanonicalPayload::ChangeRevision(change),
            })
            .unwrap();
            let change_id = store.put(&bytes).unwrap();
            change_ids.push(change_id);
            previous = Some(change_id);
        }
        FileRefStore::new(root.join(".kat"))
            .compare_and_swap_accepted(
                &AcceptedRef {
                    state: initialized.state,
                    change: None,
                },
                &AcceptedRef {
                    state: initialized.state,
                    change: previous,
                },
            )
            .unwrap();
        let accepted_before = FileRefStore::new(root.join(".kat"))
            .read_accepted()
            .unwrap();
        let original_objects = fs::read_dir(root.join(".kat/objects"))
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                (path.clone(), fs::read(path).unwrap())
            })
            .collect::<Vec<_>>();
        set_format(root, LEGACY_FORMAT_VERSION);

        let MigrationOutcome::Migrated(revision_id) = migrate_repository(root).unwrap() else {
            panic!("expected migration")
        };
        let repo = open_repository(root).unwrap();
        let revision = repo.read_revision(revision_id).unwrap();
        assert_eq!(
            revision.semantic_change,
            Some(ChangeRevisionId::from_object_id(
                *change_ids.last().unwrap()
            ))
        );
        for change in &change_ids {
            assert!(store.get(*change).is_ok());
        }
        assert_eq!(repo.accepted, accepted_before);
        for (path, bytes) in original_objects {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }

        fs::remove_file(root.join(".kat/objects").join(change_ids[0].to_string())).unwrap();
        assert!(
            verify_revision_closure(revision_id, |id| {
                store.get(id).map_err(RemoteError::from)
            })
            .is_err()
        );
    }

    #[test]
    fn interrupted_migration_reuses_root_and_workspace_then_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("readme.md"), "project").unwrap();
        init_repository(root).unwrap();
        set_format(root, LEGACY_FORMAT_VERSION);

        let first = migrate_repository(root).unwrap();
        let MigrationOutcome::Migrated(first_revision) = first else {
            panic!("expected a new migration")
        };
        let first_workspace = read_workspace_state(root).unwrap();
        set_format(root, LEGACY_FORMAT_VERSION);

        assert_eq!(
            migrate_repository(root).unwrap(),
            MigrationOutcome::Migrated(first_revision)
        );
        assert_eq!(read_workspace_state(root).unwrap(), first_workspace);
        assert_eq!(
            migrate_repository(root).unwrap(),
            MigrationOutcome::AlreadyMigrated(first_revision)
        );
    }

    #[test]
    fn fresh_cli_bootstrap_uses_root_revision_and_workspace() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();

        let (_initialized, revision_id) = initialize_v05_repository(dir.path()).unwrap();
        let repo = open_repository(dir.path()).unwrap();
        let revision = repo.read_revision(revision_id).unwrap();

        assert!(revision.parents.is_empty());
        assert_eq!(revision.semantic_state.as_object_id(), repo.accepted.state);
        assert_eq!(revision.semantic_change, None);
        assert_eq!(
            read_workspace_state(dir.path()).unwrap().base_revision,
            revision_id
        );
    }

    #[test]
    fn interrupted_fresh_bootstrap_is_closed_and_resumes_through_migrate() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
        init_repository_for_bootstrap(dir.path()).unwrap();

        let error = open_repository(dir.path()).unwrap_err().to_string();
        assert!(error.contains("kat migrate"));
        assert!(read_workspace_state(dir.path()).is_err());

        assert!(matches!(
            migrate_repository(dir.path()).unwrap(),
            MigrationOutcome::Migrated(_)
        ));
        assert!(open_repository(dir.path()).is_ok());
    }
}
