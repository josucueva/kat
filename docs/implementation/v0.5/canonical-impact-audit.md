# KAT v0.5 Canonical Impact Audit

## Purpose

This document identifies the canonical-format impact of the v0.5 collaboration model.

It determines which new concepts belong to KAT's immutable content-addressed repository model and which remain mutable, local, derived, or backend-specific state.

The audit covers:

* `RepositoryRevision`;
* `WorkspaceSnapshot`;
* Artifact materialization baselines;
* reconciliation state;
* conflicts;
* references;
* workspaces;
* Git backend metadata.

---

## 1. Canonical boundary

A concept should become canonical when it represents durable repository history that must remain:

* immutable;
* content-addressed;
* independently verifiable;
* transferable between repository copies;
* historically reachable.

Mutable coordination state, local working state, projections, and backend implementation metadata should remain outside the canonical object model.

---

## 2. RepositoryRevision

`RepositoryRevision` MUST become a canonical immutable object.

It represents one accepted version of the complete software repository.

Conceptually:

```text
RepositoryRevision {
    parents[]
    semantic_state
    workspace_snapshot
    semantic_change?
}
```

Its identity must be derived from its deterministic canonical representation using the existing KAT ObjectId mechanism.

This introduces a new canonical object kind.

---

## 3. WorkspaceSnapshot

`WorkspaceSnapshot` is a KAT repository concept, but its physical content is stored by the workspace backend.

The canonical model therefore needs a stable immutable reference to the physical snapshot without embedding Git-specific semantics into higher KAT objects.

Conceptually:

```text
WorkspaceSnapshotId
```

must identify one immutable physical workspace state.

For the Git backend, it may resolve to an immutable Git commit internally.

The exact representation must not depend on:

* Git branch names;
* Git HEAD;
* mutable refs.

Whether `WorkspaceSnapshot` itself is represented as a standalone KAT canonical object or as a typed immutable backend identifier remains an implementation decision.

The higher-level canonical model must remain backend-neutral.

---

## 4. Artifact materialization baseline

v0.5 requires Artifact accountability to preserve both:

```text
semantic baseline
physical materialization baseline
```

The physical baseline MUST be historically reproducible.

Therefore canonical accepted accountability state must contain enough information to determine the accounted `MaterializationId`.

Two approaches remain possible:

### Explicit materialization

```text
AccountArtifact
    semantic_baselines[]
    materialization_id
```

### Revision-derived materialization

```text
AccountArtifact
    accounted_repository_revision
```

with physical materialization derived from that revision's `WorkspaceSnapshot`.

The implementation must choose one representation before canonical format changes are finalized.

The canonical model MUST NOT derive historical accountability from the current working tree.

---

## 5. MaterializationId

`MaterializationId` represents immutable physical content resolved for an Artifact.

It must be:

* deterministic;
* immutable;
* equality-comparable;
* backend-neutral at the KAT domain boundary.

A Git implementation may derive it from Git objects, but Git object identity must not be treated as a general KAT `ObjectId` unless explicitly encoded as such.

---

## 6. Reconciliation result

A successful reconciliation produces normal canonical history:

```text
RepositoryRevision
    parents = [RA, RB, ...]
```

and, when semantic resolution requires explicit operations:

```text
ChangeRevision
```

No separate canonical reconciliation object is required for a completed reconciliation in v0.5.

The resulting multi-parent `RepositoryRevision` and semantic history are sufficient to preserve the accepted reconciliation result.

---

## 7. ReconciliationCandidate

`ReconciliationCandidate` MUST NOT be canonical accepted repository history in v0.5.

It represents mutable, incomplete collaboration state that may contain unresolved conflicts.

It belongs to workspace/collaboration state.

Conceptually:

```text
ReconciliationCandidate
    resolved effects
    semantic conflicts
    materialization conflicts
```

It may be persisted locally, but it is not part of accepted repository state.

Future versions may make transferable unresolved reconciliation state durable if required.

---

## 8. Conflicts

`SemanticConflict` and `MaterializationConflict` MUST NOT become Knowledge Elements.

For v0.5 they remain collaboration-layer state associated with a `ReconciliationCandidate`.

They are not accepted software state until resolved.

Accepted history preserves their alternatives through the parent `RepositoryRevision`s and the resulting reconciliation revision.

Therefore no new canonical conflict object kind is required for the initial implementation.

---

## 9. References

The following are mutable repository metadata and MUST NOT be canonical content-addressed objects:

```text
Head
NamedReference
RemoteReference
```

They point to immutable `RepositoryRevisionId`s.

Changing a reference does not create or mutate repository history.

Reference updates require their own persistence and synchronization semantics but remain outside canonical object identity.

---

## 10. Workspace

`Workspace` is mutable local state.

It MUST NOT become a canonical repository object.

Workspace state includes information such as:

```text
workspace identity
base RepositoryRevision
DraftSession
working physical state
backend association
reconciliation candidate
```

Its persistence remains local collaboration metadata.

---

## 11. Git backend metadata

Git-specific state MUST remain outside the canonical KAT model.

Examples include:

```text
Git repository path
Git remote URLs
Git refs
Git HEAD
Git index
Git commit mappings
backend reachability refs
credentials
```

Canonical KAT objects may reference an immutable `WorkspaceSnapshotId`, but they must not depend on mutable Git state.

---

## 12. ChangeRevision impact

The existing `ChangeRevision` model already supports plural base states.

This is compatible with semantic reconciliation.

However, v0.5 must verify whether the relationship between:

```text
RepositoryRevision.parents[]
```

and:

```text
ChangeRevision.base_states[]
```

is sufficiently precise for multi-parent reconciliation.

No canonical `ChangeRevision` change should be made unless reconciliation tests demonstrate that the existing representation is insufficient.

---

## 13. SemanticState impact

`SemanticState` remains unchanged in role.

It continues to represent authoritative semantic state only.

Physical workspace identity MUST NOT be added directly to `SemanticState`.

The semantic and physical dimensions are joined at `RepositoryRevision`.

---

## 14. Canonical format changes required

v0.5 definitely requires canonical support for:

```text
RepositoryRevision
```

It likely requires canonical or typed immutable representation for:

```text
WorkspaceSnapshotId
MaterializationId
```

It may require modification of Artifact accountability representation depending on the final physical-baseline design.

The following do not require new canonical object kinds for v0.5:

```text
Workspace
Head
NamedReference
RemoteReference
ReconciliationCandidate
SemanticConflict
MaterializationConflict
Git backend metadata
```

---

## 15. Canonical invariance requirements

The following must hold:

### CAN-COLL-01

Changing mutable references MUST NOT change canonical `RepositoryRevision` bytes or ObjectIds.

### CAN-COLL-02

Changing local workspace metadata MUST NOT change accepted repository objects.

### CAN-COLL-03

Changing Git remote location MUST NOT change repository identity or accepted canonical history.

### CAN-COLL-04

Reconciliation conflict state MUST NOT affect canonical accepted history until an explicit resolution produces accepted objects.

### CAN-COLL-05

The same semantic state and physical workspace snapshot with the same ancestry and semantic-change reference MUST produce the same `RepositoryRevision` identity.

### CAN-COLL-06

Git-specific mutable state MUST NOT participate in KAT canonical hashing.

---

## 16. Required implementation decisions

Before updating `canonical-format.cddl`, v0.5 must settle:

1. the exact canonical structure of `RepositoryRevision`;
2. the representation of `WorkspaceSnapshotId`;
3. the representation of `MaterializationId`;
4. whether Artifact physical accountability stores `MaterializationId` directly or derives it from a referenced `RepositoryRevision`;
5. whether existing `ChangeRevision` semantics require modification for reconciliation.

No other collaboration concept currently requires canonical-format expansion.

---

## Summary

The v0.5 canonical boundary is:

```text
Canonical accepted history
├── existing semantic objects
├── SemanticState
├── ChangeRevision
└── RepositoryRevision
        └── immutable WorkspaceSnapshot reference
```

Artifact accountability additionally requires a durable physical materialization baseline.

Outside the canonical boundary remain:

```text
Workspace
references
remote views
reconciliation candidates
unresolved conflicts
Git backend metadata
```

The primary canonical addition for v0.5 is therefore `RepositoryRevision`.

The remaining canonical design work is limited to the exact representation of physical snapshot and Artifact materialization identities.
