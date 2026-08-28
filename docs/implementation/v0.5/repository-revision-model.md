# KAT v0.5 Repository Revision Model

## Purpose

This document defines the repository-level versioning model that binds
semantic state and physical workspace state into one coherent software
revision.

It defines:

- `RepositoryRevision`;
- `WorkspaceSnapshot`;
- repository ancestry;
- semantic-only, physical-only, and combined revisions;
- the relationship between `RepositoryRevision`, `SemanticState`, and
  `ChangeRevision`;
- repository heads;
- workspace bases;
- structural sharing;
- revision validity;
- the boundary between repository semantics and physical storage backends.

This document does not define:

- CLI commands;
- Git implementation details;
- remote synchronization;
- reconciliation algorithms;
- conflict resolution;
- KAT Hub protocols.

---

## 1. Motivation

KAT models software as more than source code.

A `SemanticState` represents the authoritative semantic state of the software,
but it does not represent the complete physical project.

A physical workspace snapshot represents the project files at a point in time,
but it does not represent the authoritative semantic knowledge of the software.

Neither one is sufficient to represent a complete version of a KAT repository.

KAT therefore introduces `RepositoryRevision` as the repository-level
version-control unit.

A `RepositoryRevision` binds:

```text
SemanticState
    +
WorkspaceSnapshot
    =
RepositoryRevision
```

The semantic and physical dimensions remain independently versioned, while the
repository revision records which versions belong together as one coherent
software state.

---

## 2. Core concepts

### 2.1 SemanticState

`SemanticState` remains the authoritative semantic snapshot of the repository.

It identifies the active immutable versions of Knowledge Elements and
Relationships for one semantic state.

It describes what the software means.

It does not describe the complete physical workspace.

---

### 2.2 WorkspaceSnapshot

A `WorkspaceSnapshot` identifies one immutable version of the versioned physical
project workspace.

It describes the project-owned physical content needed to reconstruct that
workspace.

Conceptually:

```text
WorkspaceSnapshot
    tracked project content
    at one immutable point in history
```

The storage mechanism is backend-specific.

A Git-backed implementation may represent a `WorkspaceSnapshot` using an
immutable Git object or commit.

Git-specific concepts such as branches or `HEAD` are not part of the
`WorkspaceSnapshot` semantics.

---

### 2.3 RepositoryRevision

A `RepositoryRevision` binds one `SemanticState` and one `WorkspaceSnapshot`
into one coherent version of the software repository.

Conceptually:

```text
RepositoryRevision
    parents[]
    semantic_state
    workspace_snapshot
    semantic_change?
```

A `RepositoryRevision` is immutable.

It is the version-control and collaboration unit for the complete KAT-managed
software repository.

---

## 3. Conceptual structure

The conceptual model is:

```text
RepositoryRevision {
    parents: RepositoryRevisionId[]
    semantic_state: SemanticStateId
    workspace_snapshot: WorkspaceSnapshotId
    semantic_change: ChangeRevisionId | none
}
```

This is a domain model.

The exact canonical representation, field encoding, and object identity are
defined only after the repository-revision semantics are frozen.

---

## 4. Parent revisions

`parents` identifies the direct repository revisions from which a revision was
derived.

An ordinary revision normally has one parent:

```text
R0 -> R1 -> R2
```

The initial repository revision has no parents:

```text
R0
parents = []
```

A reconciliation revision may have multiple parents:

```text
        R0
       /  \
      RA  RB
       \  /
        RC
```

with:

```text
RC.parents = [RA, RB]
```

Repository ancestry therefore forms a directed acyclic graph.

---

## 5. Semantic state

`semantic_state` identifies the complete authoritative semantic state associated
with the repository revision.

For example:

```text
R42
    semantic_state = S42
```

`S42` determines the active versions of the semantic identities visible in
that revision.

A `RepositoryRevision` does not duplicate the contents of the `SemanticState`.

It references the immutable state object.

---

## 6. Workspace snapshot

`workspace_snapshot` identifies the immutable physical workspace associated
with the repository revision.

For example:

```text
R42
    workspace_snapshot = W42
```

The snapshot represents the versioned physical project at that revision.

It does not imply that every tracked file is represented by an Artifact
Knowledge Element.

Physical tracking and semantic Artifact modeling remain separate concerns.

---

## 7. Semantic change

`semantic_change` identifies the explicit semantic evolution associated with
the repository revision when semantic evolution occurred.

For example:

```text
R1
    semantic_state = S1

R2
    semantic_state = S2
    semantic_change = C2
```

where `C2` explains how semantic state `S1` evolved into `S2`.

A repository revision may have no semantic change when only the physical
workspace changed.

Therefore:

```text
RepositoryRevision != ChangeRevision
```

A `RepositoryRevision` represents a complete software version.

A `ChangeRevision` represents explicit semantic evolution.

---

## 8. Independent evolution dimensions

Semantic state and physical workspace state evolve independently.

A repository revision may therefore take one of three forms.

### 8.1 Semantic-only revision

```text
R1
├── semantic: S1
└── physical: W1

        ↓

R2
├── semantic: S2
└── physical: W1
```

The semantic state changes while the physical workspace remains unchanged.

Examples include:

- creating or refining a Requirement;
- creating a Constraint;
- recording a Design Decision;
- updating semantic relationships;
- modifying semantic knowledge before physical implementation begins.

---

### 8.2 Physical-only revision

```text
R1
├── semantic: S1
└── physical: W1

        ↓

R2
├── semantic: S1
└── physical: W2
```

The physical workspace changes while the semantic state remains unchanged.

Examples may include:

- documentation changes;
- formatting changes;
- build or configuration changes without modeled semantic consequences;
- physical implementation work performed without changing semantic knowledge.

A physical-only revision may produce Artifact accountability or repository
health findings.

Those findings do not prevent the revision from being represented.

---

### 8.3 Combined revision

```text
R1
├── semantic: S1
└── physical: W1

        ↓

R2
├── semantic: S2
└── physical: W2
```

Both semantic and physical state change.

This is expected to be common during implementation work.

---

## 9. Independent element evolution

A repository revision does not imply that every semantic or physical entity
changed.

For example:

```text
R42
├── Requirement R1 -> R1v1
├── Implementation I1 -> I1v3
├── Artifact A1 -> A1v2
├── Artifact A2 -> A2v5
└── WorkspaceSnapshot -> W42
```

A later revision may change only one of those identities:

```text
R43
├── Requirement R1 -> R1v1
├── Implementation I1 -> I1v3
├── Artifact A1 -> A1v2
├── Artifact A2 -> A2v6
└── WorkspaceSnapshot -> W43
```

Requirements, Implementations, Artifacts, Validations, and physical contents
are independently versioned.

Related objects are not required to advance versions together.

---

## 10. Revision identity and immutability

A `RepositoryRevision` is immutable once created.

Changing any part of its repository-level content produces a different
repository revision.

This includes changes to:

- parent revisions;
- semantic state;
- workspace snapshot;
- associated semantic change.

Repository revisions therefore participate in KAT's immutable history model.

The exact ObjectId derivation is defined separately.

---

## 11. Structural sharing

Repository revisions reuse unchanged immutable objects.

For example:

```text
R1
├── S1
└── W1

R2
├── S2
└── W1
```

`R2` reuses `W1`.

Likewise:

```text
R2
├── S2
└── W1

R3
├── S2
└── W2
```

`R3` reuses `S2`.

The repository model therefore does not require copying unchanged semantic or
physical state between revisions.

---

## 12. Repository history

Repository history is a DAG of immutable `RepositoryRevision`s.

Ordinary history:

```text
R0 -> R1 -> R2 -> R3
```

Concurrent evolution:

```text
        R0
       /  \
      RA  RB
```

Reconciliation:

```text
        R0
       /  \
      RA  RB
       \  /
        RC
```

The repository DAG represents complete software evolution.

It is not only semantic history and not only physical file history.

---

## 13. Repository heads

A repository head is a visible accepted `RepositoryRevision` with no visible
accepted successor in the current repository view.

For example:

```text
        R0
       /  \
      RA  RB
```

both `RA` and `RB` may be heads.

Multiple heads are valid.

They represent divergence, not conflict.

Optional named references may later point to repository revisions, but names
do not define revision identity or ancestry.

---

## 14. Workspace base

A collaborative workspace is grounded in an explicit `RepositoryRevision`.

Conceptually:

```text
Workspace
    base_revision
    draft_semantic_change
    physical_working_state
```

The base revision establishes both:

```text
semantic base
    = base_revision.semantic_state

physical base
    = base_revision.workspace_snapshot
```

This gives KAT an explicit point from which local work originated.

The base revision is required for:

- divergence detection;
- change comparison;
- impact analysis;
- physical change comparison;
- future reconciliation.

---

## 15. Working state is not a RepositoryRevision

A developer may modify semantic or physical state after opening a workspace.

Those uncommitted modifications are working state.

They do not become repository history until a new `RepositoryRevision` is
accepted.

Conceptually:

```text
RepositoryRevision R42
        ↓
Workspace based on R42
        ↓
semantic draft + physical edits
        ↓
new accepted RepositoryRevision R43
```

This distinction preserves immutability of accepted revisions.

---

## 16. Artifact accountability interaction

Artifact accountability spans both semantic and physical state.

For an Artifact, KAT may need to evaluate two independent dimensions.

Semantic alignment:

```text
CURRENT
STALE
UNACCOUNTED
```

Physical materialization alignment may later include states such as:

```text
CURRENT
MODIFIED
MISSING
UNRESOLVED
```

For example:

```text
Artifact A
    represented Implementation = I1
    semantic baseline = I1v3
    physical baseline = P17
```

If:

```text
I1v3 -> I1v4
P17 unchanged
```

the Artifact may be semantically stale while physically unchanged.

If:

```text
I1v3 unchanged
P17 -> P18
```

the Artifact may remain semantically current while its physical
materialization has changed.

A `RepositoryRevision` provides the coordination point that associates a
semantic state with an exact physical workspace snapshot.

The exact Artifact accountability representation is defined separately.

---

## 17. Relationship to SemanticState

`SemanticState` remains the authority for semantic meaning.

`RepositoryRevision` does not replace it.

Instead:

```text
RepositoryRevision
        |
        v
SemanticState
```

The repository revision adds the repository-level association between semantic
meaning and physical materialization.

The same semantic state may appear in multiple repository revisions:

```text
R1 -> {S1, W1}
R2 -> {S1, W2}
R3 -> {S1, W3}
```

---

## 18. Relationship to WorkspaceSnapshot

`WorkspaceSnapshot` represents physical project state only.

It does not imply semantic authority.

The same workspace snapshot may appear in multiple repository revisions:

```text
R1 -> {S1, W1}
R2 -> {S2, W1}
R3 -> {S3, W1}
```

This allows semantic knowledge to evolve without forcing physical changes.

---

## 19. Relationship to ChangeRevision

`ChangeRevision` and `RepositoryRevision` represent different layers of
evolution.

```text
ChangeRevision
    explicit semantic evolution

RepositoryRevision
    complete software version
```

A semantic-only or combined repository revision may reference a
`ChangeRevision`.

A physical-only revision may keep the same `SemanticState` and have no new
semantic `ChangeRevision`.

The exact relationship between multi-base reconciliation Changes and
multi-parent `RepositoryRevision`s is defined by the reconciliation model.

---

## 20. Revision validity

A `RepositoryRevision` is structurally valid only if:

1. all referenced parent revisions exist;
2. the referenced `SemanticState` exists and is structurally valid;
3. the referenced `WorkspaceSnapshot` exists and is structurally valid;
4. the referenced `ChangeRevision`, when present, exists;
5. the semantic transition associated with the `ChangeRevision` is compatible
   with the revision;
6. all referenced immutable objects can be resolved.

Repository health findings do not automatically make a structurally valid
revision invalid.

Examples include:

- stale Artifacts;
- physical Artifact drift;
- evidence gaps;
- GraphQuality findings;
- validations requiring review.

Those belong to accountability, validation, policy, or health models.

---

## 21. Initial repository revision

A newly initialized KAT repository has an initial `RepositoryRevision`.

Conceptually:

```text
R0
├── parents: []
├── semantic_state: S0
├── workspace_snapshot: W0
└── semantic_change: none
```

`S0` represents the initial accepted semantic state.

`W0` represents the initial versioned physical workspace.

The precise initialization rules are defined separately.

---

## 22. Existing project adoption

KAT may be initialized over an existing software project.

KAT does not need to synthesize semantic history for physical history that
predates KAT adoption.

Instead:

```text
existing project history
        ↓
KAT adoption boundary
        ↓
R0
├── SemanticState S0
└── WorkspaceSnapshot W0
```

Historical physical repository data may remain available through the physical
backend.

It is not automatically interpreted as historical KAT semantic evolution.

---

## 23. Physical backend boundary

`WorkspaceSnapshot` is a KAT repository concept.

Git is one possible implementation backend.

The repository-revision model does not depend on:

- Git branches;
- Git `HEAD`;
- Git index semantics;
- Git remote-tracking branches;
- Git-specific merge commands;
- Git's user-facing workflow.

A Git-backed implementation may use Git objects to implement physical snapshots,
storage, transfer, and materialization.

Those mechanisms remain below the repository model.

---

## 24. Non-goals

This model does not define:

- physical tracked/untracked file rules;
- ignore behavior;
- Git command interoperability;
- physical merge algorithms;
- semantic reconciliation algorithms;
- conflict representation;
- conflict resolution;
- remote synchronization;
- KAT Hub storage;
- remote publication;
- repository policy;
- clone behavior;
- CLI syntax.

Those concerns build on this model.

---

## Summary

KAT version control operates on complete software revisions.

```text
       SemanticState
            \
             \
          RepositoryRevision
             /
            /
    WorkspaceSnapshot
```

A `RepositoryRevision` is the immutable repository-level unit that binds one
authoritative semantic state to one exact physical workspace snapshot.

Semantic knowledge and physical materialization evolve independently.

Repository history, collaboration, divergence, switching, and reconciliation
operate on `RepositoryRevision`s rather than on either dimension in isolation.