# KAT v0.5 Workspace Model

## Purpose

This document defines the local workspace model used to evolve a KAT repository.

A KAT workspace is the local environment in which a contributor works from an accepted `RepositoryRevision` toward a possible future revision.

The workspace coordinates two independently evolving dimensions:

- semantic working state;
- physical working state.

It defines:

- the workspace base;
- accepted state versus working state;
- semantic draft state;
- physical working state;
- tracked, untracked, and ignored physical content;
- workspace cleanliness and modification;
- physical snapshot materialization;
- physical backend consistency;
- Artifact materialization drift;
- local workspace persistence.

This document does not define:

- reconciliation between divergent repository revisions;
- semantic or materialization conflict resolution;
- remote synchronization;
- KAT Hub protocols;
- remote publication;
- concrete Git commands;
- CLI syntax.

---

## 1. Motivation

A `RepositoryRevision` represents an immutable accepted version of the complete software repository.

Development does not happen directly inside an accepted revision.

A contributor starts from an accepted `RepositoryRevision`, modifies semantic knowledge, physical project content, or both, and eventually produces another accepted revision.

Conceptually:

```text
RepositoryRevision R42
        |
        v
     Workspace
        |
        +-- semantic working state
        |
        +-- physical working state
        |
        v
possible RepositoryRevision R43
```

The workspace is therefore mutable local state positioned between immutable repository revisions.

It must always retain enough information to determine where the work started and how the current working state differs from that base.

---

## 2. Core concept

A workspace is grounded in exactly one base `RepositoryRevision`.

Conceptually:

```text
Workspace
    base_revision
    semantic_working_state
    physical_working_state
```

The base revision establishes:

```text
semantic base
    = base_revision.semantic_state

physical base
    = base_revision.workspace_snapshot
```

The workspace then records or derives changes relative to those two bases.

The base revision itself remains immutable.

---

## 3. Workspace base

Every workspace MUST identify the `RepositoryRevision` from which its current work originated.

For example:

```text
Workspace W
    base_revision = R42
```

where:

```text
R42
├── semantic_state = S42
└── workspace_snapshot = P42
```

The workspace therefore begins with:

```text
semantic working state = S42
physical working state = P42
```

before local modifications occur.

The explicit base is required for:

- determining semantic changes;
- determining physical changes;
- Artifact accountability;
- divergence detection;
- future reconciliation;
- creating a new repository revision.

A workspace must not infer its repository base only from the current physical
backend state.

---

## 4. Accepted state and working state

Accepted repository state and local working state are different concepts.

### Accepted state

An accepted state is represented by an immutable `RepositoryRevision`.

```text
R42
├── SemanticState S42
└── WorkspaceSnapshot P42
```

### Working state

Working state contains local modifications derived from that revision.

```text
Workspace based on R42
├── semantic draft changes
└── physical working changes
```

Working state may be incomplete, invalid, or inconsistent while development is
in progress.

It does not become repository history until a new `RepositoryRevision` is
accepted.

---

## 5. Semantic working state

Semantic modifications occur through KAT semantic operations.

The workspace does not directly mutate the accepted `SemanticState`.

Instead, semantic work is represented through the existing draft Change model.

Conceptually:

```text
base SemanticState S42
        |
        v
DraftSession
    operations[]
        |
        v
candidate SemanticState
```

The existing KAT authoring model remains responsible for:

- opening a draft;
- staging semantic operations;
- workflow references;
- validating operations;
- producing a candidate semantic result.

The workspace model does not replace `DraftSession`.

It places that draft within an explicit repository-level base.

Conceptually:

```text
Workspace
    base_revision = R42
    draft_session = D | none
```

where the draft semantic base corresponds to:

```text
R42.semantic_state
```

---

## 6. Physical working state

The physical working state is the current project-owned filesystem state being
edited by the contributor.

It originates from the base revision's `WorkspaceSnapshot`.

Conceptually:

```text
WorkspaceSnapshot P42
        |
        v
materialized working tree
        |
        +-- modified tracked content
        +-- new tracked content
        +-- deleted tracked content
        +-- untracked content
        +-- ignored content
```

The physical working state is mutable.

The base `WorkspaceSnapshot` is immutable.

A future accepted repository revision receives a new immutable
`WorkspaceSnapshot` only when the physical versioned state differs from the
base.

---

## 7. Versioned physical content

The physical workspace must distinguish content that belongs to the versioned
project from local filesystem content that does not.

The physical backend is responsible for maintaining the distinction between:

```text
tracked
untracked
ignored
```

### Tracked

Tracked content belongs to the versioned physical project.

It participates in `WorkspaceSnapshot`s.

Examples include:

- source code;
- tests;
- project documentation;
- configuration;
- build definitions;
- scripts;
- versioned assets;
- dependency manifests;
- dependency lockfiles.

### Untracked

Untracked content exists in the local workspace but is not yet part of the
versioned physical project.

Untracked content does not automatically enter a `WorkspaceSnapshot`.

### Ignored

Ignored content is intentionally excluded from physical versioning.

Examples may include:

- build output;
- caches;
- temporary files;
- local editor state;
- generated transient content;
- credentials or local secrets.

The exact ignore mechanism is backend-specific.

---

## 8. Physical tracking and Artifact modeling

Physical tracking and semantic Artifact modeling remain independent.

A tracked file is part of the physical project.

An Artifact Knowledge Element gives selected physical content explicit semantic
meaning and accountability.

Therefore:

```text
tracked physical content
    does not imply
Artifact Knowledge Element
```

Most project files may be physically tracked without appearing as Artifacts in
the semantic graph.

Likewise, Artifact accountability must not require every tracked file to be
semantically modeled.

---

## 9. WorkspaceSnapshot

A `WorkspaceSnapshot` identifies one immutable version of the tracked physical
project.

Conceptually:

```text
WorkspaceSnapshot
    complete tracked physical state
```

The workspace model treats `WorkspaceSnapshotId` as an opaque immutable
identity.

The concrete physical backend determines how that identity maps to stored
physical content.

For a Git-backed implementation, a snapshot may correspond to an immutable Git
object or commit.

Git-specific representation is not part of the workspace domain model.

---

## 10. Materialization

Opening or switching a workspace requires materializing the physical snapshot
associated with its base revision.

Conceptually:

```text
RepositoryRevision R42
    workspace_snapshot = P42

            |
            v

materialize P42

            |
            v

local physical working tree
```

After materialization:

```text
workspace.base_revision = R42
```

and the clean physical working state corresponds to `P42`.

Materialization must not alter the semantic state independently.

The semantic and physical bases come from the same `RepositoryRevision`.

---

## 11. Clean workspace

A workspace is completely clean when neither dimension differs from its base.

Conceptually:

```text
Workspace W
    base = R42

semantic:
    no draft semantic changes

physical:
    tracked state matches R42.workspace_snapshot
```

This means:

```text
working semantic state = R42.semantic_state
working physical state = R42.workspace_snapshot
```

Untracked or ignored local files do not necessarily make the repository-level
workspace modified because they are outside the current versioned physical
snapshot.

The exact user-facing status terminology is defined separately.

---

## 12. Semantic modification

A workspace is semantically modified when its active draft represents semantic
operations relative to the base semantic state.

For example:

```text
base:
    R42.semantic_state = S42

draft:
    Update Requirement R1
```

The accepted `SemanticState S42` remains unchanged.

The draft describes a possible successor semantic state.

Semantic modification does not imply physical modification.

---

## 13. Physical modification

A workspace is physically modified when its tracked physical working state
differs from the base `WorkspaceSnapshot`.

Examples include:

- tracked file content changed;
- tracked file created;
- tracked file deleted;
- tracked file moved or renamed;
- tracked physical metadata changed when the backend considers that metadata
  versioned.

Physical modification does not implicitly mutate semantic knowledge.

---

## 14. Combined modification

A workspace may contain both semantic and physical changes.

For example:

```text
Workspace based on R42

semantic:
    Update Implementation I1

physical:
    modify src/auth/service.rs
```

The two changes are related only when KAT knowledge explicitly represents such
a relationship.

KAT must not infer semantic meaning solely from physical modification.

---

## 15. Physical-only work

A workspace may contain physical modifications without semantic modifications.

For example:

```text
semantic:
    unchanged

physical:
    README.md modified
```

or:

```text
semantic:
    unchanged

physical:
    source implementation modified
```

Both are representable.

The second case may cause Artifact accountability findings if the modified
physical content is represented by an Artifact.

Physical-only work is not automatically invalid.

---

## 16. Semantic-only work

A workspace may contain semantic modifications while its physical state remains
unchanged.

For example:

```text
semantic:
    Requirement R1 updated

physical:
    unchanged
```

This is valid working state.

It may cause impact, staleness, validation, or other repository-health findings
without requiring immediate physical modification.

---

## 17. Artifact physical materialization

An Artifact may identify physical project content through its locator.

Conceptually:

```text
Artifact A
    locator -> physical project content
```

The locator is resolved relative to a physical workspace state.

At an accepted repository revision:

```text
RepositoryRevision R42
    workspace_snapshot = P42

Artifact A
    locator = src/auth/service.rs

resolve(A, P42)
    -> materialization M42
```

The resulting physical materialization has an immutable identity that can be
used for accountability.

The workspace model does not require that this identity be a Git blob ID.

It should remain conceptually backend-neutral.

---

## 18. Materialization identity

KAT needs a deterministic way to determine whether the physical content
represented by an Artifact has changed.

Conceptually:

```text
MaterializationId
```

identifies the physical content resolved by an Artifact locator at a particular
workspace state.

The representation depends on the locator and physical backend.

For example, a future implementation may resolve:

```text
single file
    -> content identity

directory
    -> tree identity

logical physical set
    -> deterministic aggregate identity
```

The exact locator and materialization model is defined separately.

---

## 19. Artifact materialization drift

Artifact accountability must distinguish semantic alignment from physical
materialization alignment.

Suppose an Artifact was accounted when:

```text
semantic baseline:
    Implementation I1v3

physical baseline:
    Materialization M17
```

If the workspace changes the Artifact's represented physical content:

```text
M17 -> M18
```

while:

```text
I1v3 remains unchanged
```

the Artifact remains semantically aligned with its recorded semantic baseline
but its physical materialization has changed.

Conceptually:

```text
semantic:
    CURRENT

physical:
    MODIFIED
```

This state must not be reported as semantically `STALE`.

---

## 20. Semantic staleness without physical drift

The inverse is also possible.

Suppose:

```text
semantic baseline:
    I1v3

physical baseline:
    M17
```

and the semantic dependency advances:

```text
I1v3 -> I1v4
```

while physical content remains:

```text
M17
```

Then:

```text
semantic:
    STALE

physical:
    CURRENT
```

The physical materialization did not change, but it is now accounted against an
older semantic version.

---

## 21. Combined Artifact drift

Both dimensions may change independently.

For example:

```text
I1v3 -> I1v4
M17   -> M18
```

Then:

```text
semantic:
    STALE

physical:
    MODIFIED
```

Re-accounting may later establish a new baseline if the contributor determines
that `M18` correctly represents `I1v4`.

The exact `AccountArtifact` evolution is defined separately.

---

## 22. Missing physical materialization

An Artifact locator may stop resolving in the physical working state.

For example:

```text
Artifact A
    locator = src/auth/service.rs
```

but the file is deleted.

KAT must be able to distinguish this from ordinary content modification.

Conceptually, physical accountability may later include:

```text
MISSING
```

Likewise, a locator that cannot be deterministically resolved may produce:

```text
UNRESOLVED
```

The precise physical accountability state machine is defined separately.

---

## 23. Non-Artifact physical changes

Tracked physical content that is not represented by an Artifact still
participates in the physical workspace.

For example:

```text
src/internal/helper.rs
```

may be tracked and versioned without having an Artifact Knowledge Element.

Changing it produces a physical workspace modification.

It does not produce an Artifact accountability finding unless some modeled
Artifact resolves to that physical content.

This preserves the distinction between physical completeness and semantic
modeling.

---

## 24. Physical backend

The physical backend is responsible for capabilities such as:

- tracking physical project content;
- identifying tracked, untracked, and ignored content;
- creating immutable physical snapshots;
- materializing snapshots;
- comparing working state with a snapshot;
- storing physical history;
- physical diffing;
- physical merging;
- transferring physical objects.

KAT remains responsible for deciding which physical snapshot belongs to a
`RepositoryRevision`.

---

## 25. Git-backed workspace

Git is the initial candidate physical backend.

When Git is used:

```text
KAT
    owns RepositoryRevision
    owns workspace base
    owns semantic state
    owns collaboration semantics

Git backend
    stores physical content
    stores physical snapshots
    detects physical modifications
    performs physical storage operations
```

Git's user-facing version-control model is not KAT repository authority.

In particular:

```text
Git HEAD
Git branches
Git remote-tracking refs
```

must not independently define the active KAT repository state.

---

## 26. Backend consistency

The physical backend must correspond to the workspace's expected physical base.

Conceptually:

```text
Workspace
    base_revision = R42

R42.workspace_snapshot = P42
```

KAT expects the materialized workspace to originate from `P42`.

If the backend is moved independently to a different base snapshot, the
workspace enters a backend-mismatch condition.

For example:

```text
expected base:
    P42

actual backend base:
    P51
```

KAT must not silently reinterpret this as:

```text
workspace.base_revision = R51
```

or infer another semantic state.

---

## 27. External backend mutation

Direct manipulation of the physical backend may change the workspace outside
KAT.

Examples for a Git implementation could include operations equivalent to:

- switching to another commit;
- resetting history;
- merging outside KAT;
- changing the physical ancestry.

Such external movement is outside the normal KAT workspace workflow.

When detected, KAT must preserve the existing semantic and repository base and
report that the physical backend no longer corresponds to it.

The resolution workflow is defined separately.

---

## 28. Ordinary physical editing is not backend mismatch

Editing project files does not constitute backend mismatch.

For example:

```text
base snapshot:
    P42

working tree:
    P42 + local edits
```

is normal workspace evolution.

Backend mismatch occurs when the physical workspace's version-control base or
ancestry is moved independently of the KAT workspace base.

This distinction allows normal source editing while preventing silent
repository-state movement.

---

## 29. Workspace persistence

Workspace metadata must survive across KAT command invocations.

At minimum, KAT must be able to recover:

```text
workspace identity
base RepositoryRevision
semantic draft state, when present
physical backend association
```

Additional implementation metadata may be persisted as required.

The exact on-disk representation is not defined by this model.

Existing `DraftSession` persistence remains responsible for semantic authoring
state until explicitly changed by a later design.

---

## 30. Multiple local workspaces

The repository model should permit multiple local workspaces to exist against
the same repository history.

For example:

```text
Repository history:
    R42

Workspace A:
    base = R42
    objective = Feature A

Workspace B:
    base = R42
    objective = Refactor B
```

Each workspace evolves independently.

They may later produce divergent `RepositoryRevision`s.

The exact workspace creation, naming, switching, and lifecycle UX is defined
separately.

---

## 31. Workspace and repository heads

A workspace base and a repository head are different concepts.

Suppose:

```text
Workspace base:
    R42
```

and another contributor publishes:

```text
R42 -> R43
```

The workspace remains based on `R42`.

It must not silently move to `R43`.

Instead:

```text
workspace base:
    R42

visible newer head:
    R43
```

represents potential divergence.

Synchronization or reconciliation behavior is defined separately.

---

## 32. Producing a new RepositoryRevision

A workspace may eventually produce a new accepted `RepositoryRevision`.

Conceptually:

```text
Workspace
    base = R42

semantic working result:
    S43

physical working result:
    P43

        |
        v

RepositoryRevision R43
    parent = R42
    semantic_state = S43
    workspace_snapshot = P43
```

Depending on what changed:

```text
semantic-only:
    S42 -> S43
    P42 -> P42

physical-only:
    S42 -> S42
    P42 -> P43

combined:
    S42 -> S43
    P42 -> P43
```

The exact publication operation is defined separately.

---

## 33. Workspace validity and repository health

A mutable workspace may temporarily contain:

- invalid semantic drafts;
- incomplete semantic work;
- unaccounted physical changes;
- stale Artifacts;
- missing Artifact materializations;
- failing validations;
- GraphQuality findings.

These conditions do not imply that the base `RepositoryRevision` is invalid.

The workspace is local mutable state under development.

The rules determining whether that working state may become an accepted
repository revision belong to validation and repository-policy models.

---

## 34. Non-goals

This model does not define:

- semantic reconciliation;
- physical merge behavior;
- conflict objects;
- conflict resolution;
- remote heads;
- KAT Hub storage;
- fetch or push protocols;
- GitHub integration;
- repository permissions;
- policy enforcement;
- exact Artifact locator syntax;
- exact materialization hashing;
- exact `AccountArtifact` encoding;
- CLI command names or syntax.

Those concerns build on the workspace model.

---

## Summary

A KAT workspace is mutable local state grounded in one immutable
`RepositoryRevision`.

```text
RepositoryRevision
    |
    +-- SemanticState
    |
    +-- WorkspaceSnapshot
    |
    v
Workspace
    |
    +-- semantic draft evolution
    |
    +-- physical working evolution
    |
    v
possible successor RepositoryRevision
```

The semantic and physical dimensions may evolve independently.

The workspace preserves their common repository base without allowing the
physical backend to redefine semantic authority.

Physical tracking remains separate from semantic Artifact modeling.

Artifact accountability observes both semantic baseline changes and physical
materialization changes without conflating them.

A backend such as Git may implement physical versioning, but KAT remains the
authority for the workspace base and for the complete repository state.