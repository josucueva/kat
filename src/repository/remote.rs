//! Transport-independent collaboration over semantic objects and workspace snapshots.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::domain::change::ChangeRevision;
use crate::domain::identity::{
    ChangeRevisionId, ObjectId, RepositoryId, RepositoryRevisionId, SemanticStateId,
    WorkspaceSnapshotId,
};
use crate::domain::operation::Operation;
use crate::domain::reference::{NamedReference, RemoteReference};
use crate::domain::revision::RepositoryRevision;
use crate::domain::state::SemanticState;
use crate::encoding::decode::{DecodingError, decode_canonical};
use crate::encoding::hash::object_id;
use crate::encoding::object::{CanonicalObject, CanonicalPayload, ObjectKind};
use crate::repository::object_store::{ObjectStore, ObjectStoreError};
use crate::repository::ref_store::{FileRefStore, RefStoreError};

/// A point-in-time view of a remote's references and unnamed revision tips.
///
/// `unnamed_heads` are derived observations, not mutable references. The
/// coordinator verifies their complete closures before returning them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RemoteReferences {
    /// Mutable remote references, each of which can be published with CAS.
    pub named: Vec<NamedReference>,
    /// Unnamed tips derived from the remote's known revision graph.
    pub unnamed_heads: Vec<RepositoryRevisionId>,
}

/// Transport failures shared by the semantic and workspace transports.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RemoteTransportError {
    /// The transport is temporarily unavailable.
    #[error("transport unavailable")]
    Unavailable,
    /// The endpoint belongs to a different logical KAT repository.
    #[error("repository identity mismatch: expected {expected}, found {actual}")]
    RepositoryMismatch {
        /// The repository identity requested by KAT.
        expected: RepositoryId,
        /// The repository identity reported by the endpoint.
        actual: RepositoryId,
    },
    /// The endpoint rejected object bytes that do not match their identity.
    #[error("object integrity check failed for {0}")]
    ObjectIntegrity(ObjectId),
    /// The endpoint could not verify a physical snapshot.
    #[error("workspace snapshot integrity check failed for {0:?}")]
    SnapshotIntegrity(WorkspaceSnapshotId),
    /// A compare-and-swap found a different current reference value.
    #[error("remote reference changed concurrently")]
    ReferenceConflict,
    /// A transport-specific failure with diagnostic context.
    #[error("transport failure: {0}")]
    Other(String),
}

/// The semantic side of a logical remote.
pub trait SemanticTransport {
    /// Lists named references and read-only unnamed revision tips for a repo.
    fn list_references(
        &self,
        repository_id: RepositoryId,
    ) -> Result<RemoteReferences, RemoteTransportError>;

    /// Returns immutable canonical object bytes, or `None` if the object is absent.
    fn fetch_object(
        &self,
        repository_id: RepositoryId,
        id: ObjectId,
    ) -> Result<Option<Vec<u8>>, RemoteTransportError>;

    /// Stores canonical object bytes under `id`; duplicate publication is idempotent.
    fn store_object(
        &self,
        repository_id: RepositoryId,
        id: ObjectId,
        bytes: &[u8],
    ) -> Result<(), RemoteTransportError>;

    /// Moves a named reference only when its current target equals `expected`.
    fn compare_and_swap_reference(
        &self,
        repository_id: RepositoryId,
        name: &str,
        expected: Option<RepositoryRevisionId>,
        new: RepositoryRevisionId,
    ) -> Result<(), RemoteTransportError>;
}

/// The physical side of a logical remote, addressed only by KAT snapshot IDs.
pub trait WorkspaceTransport {
    /// Reports whether the local snapshot is present and passes backend checks.
    fn local_snapshot_available(
        &self,
        id: &WorkspaceSnapshotId,
    ) -> Result<bool, RemoteTransportError>;

    /// Reports whether the remote snapshot is retrievable and passes backend checks.
    fn remote_snapshot_available(
        &self,
        id: &WorkspaceSnapshotId,
    ) -> Result<bool, RemoteTransportError>;

    /// Fetches a remote snapshot into local immutable workspace storage.
    /// This must not materialize or switch the working tree.
    fn fetch_snapshot(&self, id: &WorkspaceSnapshotId) -> Result<(), RemoteTransportError>;

    /// Publishes a locally available snapshot into remote immutable storage.
    fn publish_snapshot(&self, id: &WorkspaceSnapshotId) -> Result<(), RemoteTransportError>;
}

/// Failures detected while coordinating repository content and references.
#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    /// A transport operation failed.
    #[error(transparent)]
    Transport(#[from] RemoteTransportError),
    /// Local immutable-object storage failed.
    #[error(transparent)]
    ObjectStore(#[from] ObjectStoreError),
    /// Local reference storage failed.
    #[error(transparent)]
    RefStore(#[from] RefStoreError),
    /// Canonical decoding rejected an object.
    #[error(transparent)]
    Decoding(#[from] DecodingError),
    /// A required semantic object is absent from the remote.
    #[error("remote object {0} is missing")]
    MissingRemoteObject(ObjectId),
    /// A transferred object does not hash to its requested identity.
    #[error("object hash mismatch: expected {expected}, got {actual}")]
    ObjectHashMismatch {
        /// The requested object identity.
        expected: ObjectId,
        /// The identity derived from transferred bytes.
        actual: ObjectId,
    },
    /// A canonical object reference points to the wrong object kind.
    #[error("expected canonical object kind {expected}, got {actual}")]
    UnexpectedObjectKind {
        /// The kind required by the referencing field.
        expected: ObjectKind,
        /// The decoded kind.
        actual: ObjectKind,
    },
    /// One object identity was referenced as two incompatible canonical kinds.
    #[error("object {0} is referenced with incompatible kinds")]
    ConflictingObjectKinds(ObjectId),
    /// An associated change does not produce its revision's semantic state.
    #[error("change {change} results in state {actual}, revision expects {expected}")]
    SemanticChangeStateMismatch {
        /// The associated semantic change.
        change: ChangeRevisionId,
        /// The revision's semantic state.
        expected: SemanticStateId,
        /// The change's result state.
        actual: ObjectId,
    },
    /// A workspace snapshot required by the revision closure is absent locally.
    #[error("local workspace snapshot {0:?} is missing")]
    MissingLocalSnapshot(WorkspaceSnapshotId),
    /// A workspace snapshot required by the revision closure is absent remotely.
    #[error("remote workspace snapshot {0:?} is missing")]
    MissingRemoteSnapshot(WorkspaceSnapshotId),
    /// A snapshot fetch completed without making a valid local snapshot available.
    #[error("fetched workspace snapshot {0:?} is still unavailable locally")]
    FetchedSnapshotUnavailable(WorkspaceSnapshotId),
    /// A snapshot publication completed without making a valid remote snapshot available.
    #[error("published workspace snapshot {0:?} is still unavailable remotely")]
    PublishedSnapshotUnavailable(WorkspaceSnapshotId),
    /// The remote returned the same named reference more than once.
    #[error("remote returned duplicate reference name {0}")]
    DuplicateRemoteReference(String),
    /// The remote returned an unnamed tip that is also a named reference target.
    #[error("remote returned revision {0} as both named and unnamed")]
    NamedAndUnnamedHead(RepositoryRevisionId),
    /// The remote returned the same unnamed DAG tip more than once.
    #[error("remote returned duplicate unnamed DAG tip {0}")]
    DuplicateUnnamedHead(RepositoryRevisionId),
    /// A remote-tracking ref changed while fetch was transferring content.
    #[error("remote-tracking reference {0} changed concurrently")]
    TrackingReferenceConflict(String),
    /// The destination did not retain the verified semantic closure.
    #[error("remote semantic closure differs from the verified local closure")]
    SemanticClosureMismatch,
    /// The destination did not retain the verified physical closure.
    #[error("remote physical closure differs from the verified local closure")]
    PhysicalClosureMismatch,
}

/// The verified result of a fetch. Unnamed heads are returned without creating refs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchOutcome {
    /// Remote references recorded in `refs/remotes/<remote>/…`.
    pub references: Vec<RemoteReference>,
    /// Verified unnamed DAG tips, derived from the remote view.
    pub unnamed_heads: Vec<RepositoryRevisionId>,
}

/// Coordinates fetch and publication without moving local repository authority.
pub struct RemoteCoordinator<'a> {
    repository_id: RepositoryId,
    remote_name: String,
    objects: &'a ObjectStore,
    refs: &'a FileRefStore,
    semantic: &'a dyn SemanticTransport,
    workspace: &'a dyn WorkspaceTransport,
}

impl<'a> RemoteCoordinator<'a> {
    /// Creates a coordinator over the existing local object and reference stores.
    pub fn new(
        repository_id: RepositoryId,
        remote_name: impl Into<String>,
        objects: &'a ObjectStore,
        refs: &'a FileRefStore,
        semantic: &'a dyn SemanticTransport,
        workspace: &'a dyn WorkspaceTransport,
    ) -> Self {
        Self {
            repository_id,
            remote_name: remote_name.into(),
            objects,
            refs,
            semantic,
            workspace,
        }
    }

    /// Fetches remote immutable content, verifies complete closures, then updates
    /// only the local remote-tracking references.
    pub fn fetch(&self) -> Result<FetchOutcome, RemoteError> {
        // Capture expected local observations before network work. A competing
        // fetch must not be overwritten by a stale response.
        let previous = self
            .refs
            .list_remote_refs()?
            .into_iter()
            .filter(|reference| reference.remote == self.remote_name)
            .map(|reference| (reference.name, reference.target))
            .collect::<BTreeMap<_, _>>();

        let remote = self.semantic.list_references(self.repository_id)?;
        let remote = validate_remote_references(remote)?;
        let roots = remote
            .named
            .iter()
            .map(|reference| {
                (
                    reference.target.as_object_id(),
                    ObjectKind::RepositoryRevision,
                )
            })
            .chain(
                remote
                    .unnamed_heads
                    .iter()
                    .map(|revision| (revision.as_object_id(), ObjectKind::RepositoryRevision)),
            )
            .collect::<Vec<_>>();

        let closure = walk_closure(
            &roots,
            |id| {
                self.semantic
                    .fetch_object(self.repository_id, id)?
                    .ok_or(RemoteError::MissingRemoteObject(id))
            },
            |id, bytes| {
                let stored = self.objects.put(bytes)?;
                if stored != id {
                    return Err(RemoteError::ObjectHashMismatch {
                        expected: id,
                        actual: stored,
                    });
                }
                Ok(())
            },
        )?;

        for snapshot in &closure.snapshots {
            if !self.workspace.local_snapshot_available(snapshot)? {
                if !self.workspace.remote_snapshot_available(snapshot)? {
                    return Err(RemoteError::MissingRemoteSnapshot(snapshot.clone()));
                }
                self.workspace.fetch_snapshot(snapshot)?;
                if !self.workspace.local_snapshot_available(snapshot)? {
                    return Err(RemoteError::FetchedSnapshotUnavailable(snapshot.clone()));
                }
            }
        }

        let named = remote
            .named
            .iter()
            .map(|reference| (reference.name.clone(), reference.target))
            .collect::<BTreeMap<_, _>>();
        for (name, target) in &named {
            let path = self.tracking_path(name);
            if let Err(error) =
                self.refs
                    .compare_and_swap_ref(&path, previous.get(name).copied(), *target)
            {
                if matches!(error, RefStoreError::Conflict) {
                    return Err(RemoteError::TrackingReferenceConflict(name.clone()));
                }
                return Err(error.into());
            }
        }
        for (name, target) in &previous {
            if !named.contains_key(name) {
                let path = self.tracking_path(name);
                if let Err(error) = self.refs.delete_ref(&path, *target) {
                    if matches!(error, RefStoreError::Conflict) {
                        return Err(RemoteError::TrackingReferenceConflict(name.clone()));
                    }
                    return Err(error.into());
                }
            }
        }

        Ok(FetchOutcome {
            references: named
                .into_iter()
                .map(|(name, target)| RemoteReference {
                    remote: self.remote_name.clone(),
                    name,
                    target,
                })
                .collect(),
            unnamed_heads: remote.unnamed_heads,
        })
    }

    /// Publishes a complete local revision closure, then CAS-updates one remote
    /// named reference. Partial immutable uploads are intentionally retained.
    pub fn publish(
        &self,
        name: &str,
        expected: Option<RepositoryRevisionId>,
        candidate: RepositoryRevisionId,
    ) -> Result<(), RemoteError> {
        let root = [(candidate.as_object_id(), ObjectKind::RepositoryRevision)];
        let closure = walk_closure(&root, |id| Ok(self.objects.get(id)?), |_, _| Ok(()))?;

        for snapshot in &closure.snapshots {
            if !self.workspace.local_snapshot_available(snapshot)? {
                return Err(RemoteError::MissingLocalSnapshot(snapshot.clone()));
            }
        }
        for snapshot in &closure.snapshots {
            if !self.workspace.remote_snapshot_available(snapshot)? {
                self.workspace.publish_snapshot(snapshot)?;
            }
            if !self.workspace.remote_snapshot_available(snapshot)? {
                return Err(RemoteError::PublishedSnapshotUnavailable(snapshot.clone()));
            }
        }

        // Keep the physical snapshot available before exposing semantic objects.
        // RepositoryRevision objects are written after their semantic closure.
        let mut objects = closure.objects.clone();
        objects.sort_by_key(|(id, kind)| (*kind == ObjectKind::RepositoryRevision, *id));
        for (id, kind) in &objects {
            if *kind != ObjectKind::RepositoryRevision {
                let bytes = self.objects.get(*id)?;
                self.semantic
                    .store_object(self.repository_id, *id, &bytes)?;
            }
        }
        for (id, kind) in &objects {
            if *kind == ObjectKind::RepositoryRevision {
                let bytes = self.objects.get(*id)?;
                self.semantic
                    .store_object(self.repository_id, *id, &bytes)?;
            }
        }

        // Re-read every destination object and verify its hash, canonical form,
        // kind, and associated ChangeRevision result state before CAS.
        let remote_closure = walk_closure(
            &root,
            |id| {
                self.semantic
                    .fetch_object(self.repository_id, id)?
                    .ok_or(RemoteError::MissingRemoteObject(id))
            },
            |_, _| Ok(()),
        )?;
        verify_same_closure(&closure, &remote_closure)?;
        for snapshot in &remote_closure.snapshots {
            if !self.workspace.remote_snapshot_available(snapshot)? {
                return Err(RemoteError::MissingRemoteSnapshot(snapshot.clone()));
            }
        }

        self.semantic
            .compare_and_swap_reference(self.repository_id, name, expected, candidate)?;
        Ok(())
    }

    fn tracking_path(&self, name: &str) -> String {
        format!("remotes/{}/{name}", self.remote_name)
    }
}

#[derive(Debug)]
struct RevisionClosure {
    objects: Vec<(ObjectId, ObjectKind)>,
    snapshots: Vec<WorkspaceSnapshotId>,
}

fn walk_closure(
    roots: &[(ObjectId, ObjectKind)],
    mut load: impl FnMut(ObjectId) -> Result<Vec<u8>, RemoteError>,
    mut accept: impl FnMut(ObjectId, &[u8]) -> Result<(), RemoteError>,
) -> Result<RevisionClosure, RemoteError> {
    let mut pending = roots.to_vec();
    let mut expected = HashMap::<ObjectId, ObjectKind>::new();
    let mut objects = Vec::new();
    let mut snapshots = BTreeSet::new();
    let mut change_results = HashMap::<ObjectId, ObjectId>::new();
    let mut revision_changes = Vec::<(ChangeRevisionId, SemanticStateId)>::new();

    while let Some((id, required_kind)) = pending.pop() {
        if let Some(previous_kind) = expected.get(&id) {
            if *previous_kind != required_kind {
                return Err(RemoteError::ConflictingObjectKinds(id));
            }
            continue;
        }
        expected.insert(id, required_kind);

        let bytes = load(id)?;
        let actual_id = object_id(&bytes);
        if actual_id != id {
            return Err(RemoteError::ObjectHashMismatch {
                expected: id,
                actual: actual_id,
            });
        }
        let object = decode_canonical(&bytes)?;
        let actual_kind = object.object_kind();
        if actual_kind != required_kind {
            return Err(RemoteError::UnexpectedObjectKind {
                expected: required_kind,
                actual: actual_kind,
            });
        }

        let (dependencies, snapshot) = object_dependencies(&object);
        pending.extend(dependencies);
        if let Some(snapshot) = snapshot {
            snapshots.insert(snapshot);
        }
        match &object.payload {
            CanonicalPayload::RepositoryRevision(revision) => {
                if let Some(change) = revision.semantic_change {
                    revision_changes.push((change, revision.semantic_state));
                }
            }
            CanonicalPayload::ChangeRevision(change) => {
                change_results.insert(id, change.result_state);
            }
            _ => {}
        }
        accept(id, &bytes)?;
        objects.push((id, actual_kind));
    }

    for (change, expected_state) in revision_changes {
        let actual = change_results
            .get(&change.as_object_id())
            .copied()
            .ok_or(RemoteError::MissingRemoteObject(change.as_object_id()))?;
        if actual != expected_state.as_object_id() {
            return Err(RemoteError::SemanticChangeStateMismatch {
                change,
                expected: expected_state,
                actual,
            });
        }
    }

    objects.sort_by_key(|(id, _)| *id);
    Ok(RevisionClosure {
        objects,
        snapshots: snapshots.into_iter().collect(),
    })
}

fn verify_same_closure(
    local: &RevisionClosure,
    remote: &RevisionClosure,
) -> Result<(), RemoteError> {
    if local.objects != remote.objects {
        return Err(RemoteError::SemanticClosureMismatch);
    }
    if local.snapshots != remote.snapshots {
        return Err(RemoteError::PhysicalClosureMismatch);
    }
    Ok(())
}

fn object_dependencies(
    object: &CanonicalObject,
) -> (Vec<(ObjectId, ObjectKind)>, Option<WorkspaceSnapshotId>) {
    let mut dependencies = Vec::new();
    let snapshot = match &object.payload {
        CanonicalPayload::KnowledgeElementVersion(_) => None,
        CanonicalPayload::RelationshipVersion(_) => None,
        CanonicalPayload::OntologyVersion(_) => None,
        CanonicalPayload::SemanticState(state) => {
            semantic_state_dependencies(state, &mut dependencies);
            None
        }
        CanonicalPayload::ChangeRevision(change) => {
            change_dependencies(change, &mut dependencies);
            None
        }
        CanonicalPayload::RepositoryRevision(revision) => {
            revision_dependencies(revision, &mut dependencies);
            Some(revision.workspace_snapshot.clone())
        }
    };
    (dependencies, snapshot)
}

fn semantic_state_dependencies(
    state: &SemanticState,
    dependencies: &mut Vec<(ObjectId, ObjectKind)>,
) {
    dependencies.push((state.ontology_version, ObjectKind::OntologyVersion));
    dependencies.extend(
        state
            .elements
            .iter()
            .map(|entry| (entry.version, ObjectKind::KnowledgeElementVersion)),
    );
    dependencies.extend(
        state
            .relationships
            .iter()
            .map(|entry| (entry.version, ObjectKind::RelationshipVersion)),
    );
}

fn change_dependencies(change: &ChangeRevision, dependencies: &mut Vec<(ObjectId, ObjectKind)>) {
    dependencies.extend(
        change
            .base_states
            .iter()
            .copied()
            .map(|id| (id, ObjectKind::SemanticState)),
    );
    dependencies.push((change.result_state, ObjectKind::SemanticState));
    dependencies.extend(
        change
            .dependencies
            .iter()
            .copied()
            .map(|id| (id, ObjectKind::ChangeRevision)),
    );
    for operation in &change.operations {
        match operation {
            Operation::CreateElement { new_version } => {
                dependencies.push((*new_version, ObjectKind::KnowledgeElementVersion));
            }
            Operation::UpdateElement {
                expected_version,
                new_version,
                ..
            }
            | Operation::DeprecateElement {
                expected_version,
                new_version,
                ..
            } => {
                dependencies.push((*expected_version, ObjectKind::KnowledgeElementVersion));
                dependencies.push((*new_version, ObjectKind::KnowledgeElementVersion));
            }
            Operation::Link {
                new_relationship_version,
            } => dependencies.push((*new_relationship_version, ObjectKind::RelationshipVersion)),
            Operation::Unlink {
                expected_version, ..
            } => dependencies.push((*expected_version, ObjectKind::RelationshipVersion)),
            Operation::Supersede {
                expected_existing_version,
                replacement_version,
                superseding_relationship,
                ..
            } => {
                dependencies.push((
                    *expected_existing_version,
                    ObjectKind::KnowledgeElementVersion,
                ));
                dependencies.push((*replacement_version, ObjectKind::KnowledgeElementVersion));
                dependencies.push((*superseding_relationship, ObjectKind::RelationshipVersion));
            }
            Operation::AccountArtifact {
                reconciliations, ..
            } => {
                for reconciliation in reconciliations {
                    dependencies.push((
                        reconciliation.expected_relationship_version,
                        ObjectKind::RelationshipVersion,
                    ));
                    dependencies.push((
                        reconciliation.reconciled_target_version,
                        ObjectKind::KnowledgeElementVersion,
                    ));
                }
            }
        }
    }
}

fn revision_dependencies(
    revision: &RepositoryRevision,
    dependencies: &mut Vec<(ObjectId, ObjectKind)>,
) {
    dependencies.extend(
        revision
            .parents
            .iter()
            .map(|id| (id.as_object_id(), ObjectKind::RepositoryRevision)),
    );
    dependencies.push((
        revision.semantic_state.as_object_id(),
        ObjectKind::SemanticState,
    ));
    if let Some(change) = revision.semantic_change {
        dependencies.push((change.as_object_id(), ObjectKind::ChangeRevision));
    }
}

fn validate_remote_references(
    mut references: RemoteReferences,
) -> Result<RemoteReferences, RemoteError> {
    references.named.sort_by(|a, b| a.name.cmp(&b.name));
    references.unnamed_heads.sort();

    let mut names = BTreeSet::new();
    let mut named_targets = BTreeSet::new();
    for reference in &references.named {
        if !names.insert(reference.name.clone()) {
            return Err(RemoteError::DuplicateRemoteReference(
                reference.name.clone(),
            ));
        }
        named_targets.insert(reference.target);
    }
    let mut unnamed = BTreeSet::new();
    for head in &references.unnamed_heads {
        if named_targets.contains(head) {
            return Err(RemoteError::NamedAndUnnamedHead(*head));
        }
        if !unnamed.insert(*head) {
            return Err(RemoteError::DuplicateUnnamedHead(*head));
        }
    }
    Ok(references)
}
