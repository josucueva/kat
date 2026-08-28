# KAT v0.5 Git Workspace Backend

## Purpose

This document defines how KAT uses Git as the initial physical workspace
backend.

Git provides storage, versioning, diffing, merging, and transport capabilities
for the physical project workspace.

KAT remains the authority for:

- `RepositoryRevision`;
- workspace base state;
- semantic state;
- collaboration history;
- reconciliation;
- conflict semantics;
- repository heads.

Git is therefore an implementation backend, not the user-facing repository
model.

This document defines:

- `GitWorkspaceBackend`;
- the relationship between `WorkspaceSnapshot` and Git objects;
- physical workspace tracking;
- snapshot creation;
- snapshot materialization;
- physical change detection;
- backend consistency;
- Git object reachability;
- physical reconciliation;
- remote Git transport;
- adoption of existing Git repositories;
- the abstraction boundary between KAT and Git.

This document does not define:

- semantic reconciliation;
- semantic conflict rules;
- KAT Hub protocols;
- repository policy;
- collaboration CLI syntax;
- authentication;
- authorization;
- federation.

---

## 1. Motivation

KAT requires complete physical workspace versioning in addition to semantic
versioning.

A KAT repository must preserve project-owned physical content such as:

```text
source code
tests
documentation
configuration
scripts
assets
dependency manifests
lockfiles
```

Not all physical content needs to be represented as an Artifact Knowledge
Element.

KAT therefore requires a physical workspace backend capable of:

```text
tracking project content
creating immutable snapshots
materializing historical snapshots
detecting local physical changes
comparing snapshots
merging divergent physical histories
transferring physical objects
preserving physical history
```

Git already provides these capabilities.

KAT uses Git for that physical layer while keeping KAT repository semantics
independent from Git's user-facing version-control model.

---

## 2. Architectural boundary

The intended layering is:

```text
RepositoryRevision
        |
        v
WorkspaceSnapshot
        |
        v
WorkspaceBackend
        |
        v
GitWorkspaceBackend
        |
        v
Git object database and transport
```

KAT operates in terms of:

```text
RepositoryRevisionId
WorkspaceSnapshotId
physical working state
MaterializationId
```

The Git backend translates those concepts into Git storage operations.

Higher KAT layers must not depend directly on:

```text
Git branches
Git HEAD
Git index state
remote-tracking branches
Git merge commands
Git conflict-marker representation
```

---

## 3. WorkspaceBackend abstraction

KAT should define a backend-neutral workspace abstraction.

Conceptually:

```text
WorkspaceBackend
    inspect_working_state
    create_snapshot
    materialize_snapshot
    compare
    merge
    resolve_materialization
    ensure_snapshot_available
    fetch
    publish
```

The exact Rust trait is defined during implementation.

The abstraction must expose KAT concepts rather than Git-specific structures.

For example:

```text
create_snapshot(...)
    -> WorkspaceSnapshotId
```

not:

```text
create_git_commit(...)
    -> GitCommitOid
```

at the repository-model boundary.

---

## 4. GitWorkspaceBackend

`GitWorkspaceBackend` is the initial implementation of `WorkspaceBackend`.

It is responsible for:

- tracked physical content;
- untracked and ignored content detection;
- immutable physical snapshot storage;
- physical ancestry;
- working-tree comparison;
- physical diffing;
- physical merge support;
- remote object transfer;
- physical object reachability.

It must not determine:

- active KAT `RepositoryRevision`;
- active semantic state;
- semantic Change history;
- semantic conflicts;
- KAT repository heads.

---

## 5. KAT owns workspace authority

The active KAT workspace is defined by:

```text
Workspace
    base_revision = R
```

where:

```text
R.workspace_snapshot = W
```

The Git backend materializes `W`.

Git does not independently determine which physical state belongs to the active
workspace.

Conceptually:

```text
KAT Workspace
    base = R42

R42
    workspace_snapshot = W42

Git backend
    materializes W42
```

This direction of authority must not be reversed.

---

## 6. Git HEAD is not KAT authority

Git may internally require references, commits, or HEAD-like state to implement
a working tree.

Those values are backend state.

They do not define the active KAT repository revision.

KAT must never derive:

```text
active SemanticState
```

from:

```text
current Git branch
current Git HEAD
```

The authoritative mapping is:

```text
Workspace
    -> RepositoryRevision
    -> WorkspaceSnapshot
```

---

## 7. Internal Git repository

The preferred KAT-managed configuration is an internal Git repository rather
than exposing an ordinary project-root `.git` repository as the primary user
interface.

Conceptually:

```text
project/
├── .kat/
│   ├── repository metadata
│   ├── semantic object store
│   ├── workspace metadata
│   └── physical/
│       └── git/
├── src/
├── tests/
├── ...
```

The visible project directory acts as the KAT physical working tree.

The Git repository under `.kat/` provides backend storage.

The exact on-disk layout is an implementation detail and may change.

---

## 8. Why Git should be internal

Keeping Git below KAT reduces the risk of two competing version-control
authorities.

Normal users should not need to reason about:

```text
KAT revision
Git branch
Git HEAD
semantic state
physical commit
```

simultaneously.

Instead, the normal model is:

```text
I am working from RepositoryRevision R42.
```

KAT coordinates both:

```text
R42.semantic_state
R42.workspace_snapshot
```

The Git representation remains internal.

---

## 9. Existing Git repositories

KAT must support adoption of projects that already contain Git history.

For example:

```text
project/
├── .git/
├── src/
└── ...
```

KAT adoption must not require reconstructing semantic history for historical Git
commits.

The KAT adoption boundary establishes the first KAT repository revision.

Conceptually:

```text
existing Git history
        |
        v
current physical project state
        |
        v
KAT adoption boundary
        |
        v
RepositoryRevision R0
```

Pre-KAT Git history remains physical legacy history.

It is not automatically interpreted as KAT semantic evolution.

---

## 10. Adoption strategies

The implementation may support more than one adoption strategy.

### 10.1 Managed migration

KAT takes control of the physical repository backend and moves or converts the
existing Git repository into its managed backend layout.

The project becomes a normal KAT-managed workspace.

### 10.2 Existing Git interoperability

KAT temporarily uses the existing Git repository as its physical backend.

This may simplify early adoption but introduces greater risk of external Git
state movement.

If supported, KAT must detect backend movement that no longer matches the
workspace's expected base.

The preferred long-term mode remains KAT-managed physical control.

---

## 11. WorkspaceSnapshot identity

`WorkspaceSnapshotId` is a KAT workspace-domain identity.

Higher KAT layers should treat it as opaque.

The Git backend maps it to immutable Git physical state.

Conceptually:

```text
WorkspaceSnapshotId W42
        |
        v
Git snapshot object G42
```

KAT must not make movable Git references part of `WorkspaceSnapshot`
identity.

---

## 12. Git commit versus Git tree

Git provides at least two useful immutable physical concepts:

```text
tree
    exact filesystem snapshot

commit
    filesystem tree + parentage + metadata
```

A Git tree is sufficient to identify physical project contents.

A Git commit additionally provides:

- ancestry;
- native Git reachability;
- compatibility with Git fetch and push;
- physical merge bases;
- easier Git debugging and tooling.

For the initial backend, KAT should prefer using a Git commit as the backend
representation of a `WorkspaceSnapshot`.

This decision is backend-specific.

The KAT repository model still sees only `WorkspaceSnapshotId`.

---

## 13. Synthetic physical commits

KAT may create backend Git commits specifically to preserve physical workspace
history.

These Git commits are physical implementation objects.

They are not KAT `RepositoryRevision`s.

Conceptually:

```text
RepositoryRevision R42
    |
    +-- semantic_state = S42
    |
    +-- workspace_snapshot = W42
                                |
                                v
                         Git commit G42
```

The two objects have different semantics.

---

## 14. Physical ancestry

Where practical, synthetic Git commit ancestry should correspond to physical
repository evolution.

For ordinary evolution:

```text
KAT:
    R1 -> R2

Git:
    G1 -> G2
```

where:

```text
R1.workspace_snapshot -> G1
R2.workspace_snapshot -> G2
```

For reconciliation:

```text
KAT:
    RA ----\
            RC
    RB ----/

Git:
    GA ----\
            GC
    GB ----/
```

when both physical histories actually participate in the resulting physical
snapshot.

This correspondence is useful for physical three-way merge and transport.

It does not make Git ancestry authoritative for KAT history.

---

## 15. Physical ancestry may differ from repository ancestry

KAT must not require a one-to-one Git commit for every
`RepositoryRevision`.

For example, semantic-only evolution may produce:

```text
R1
├── semantic = S1
└── physical = W1

R2
├── semantic = S2
└── physical = W1
```

Both repository revisions reuse the same physical snapshot.

Therefore:

```text
R1.workspace_snapshot
    ==
R2.workspace_snapshot
```

No new Git physical commit is required solely because a new KAT
`RepositoryRevision` exists.

The Git physical graph is therefore related to, but not identical to, the KAT
repository DAG.

---

## 16. Physical-only evolution

Physical-only evolution produces a new `WorkspaceSnapshot`.

For example:

```text
R1
├── S1
└── W1

R2
├── S1
└── W2
```

The Git backend creates the physical snapshot corresponding to `W2`.

A semantic `ChangeRevision` is not required solely because physical content
changed.

---

## 17. Tracked content

Git tracking semantics define which local physical content participates in KAT
`WorkspaceSnapshot`s.

Tracked content belongs to the physical versioned project.

Typical tracked content includes:

```text
source
tests
documentation
configuration
scripts
assets
dependency manifests
lockfiles
```

KAT does not require every tracked path to have an Artifact Knowledge Element.

---

## 18. Untracked content

Untracked Git content exists in the local filesystem but is not currently part
of the versioned physical project.

KAT must not silently include arbitrary untracked files in a
`WorkspaceSnapshot`.

This prevents accidental inclusion of:

```text
temporary files
build output
local data
credentials
editor files
```

A physical file must enter tracked physical state through an explicit backend
tracking decision.

The exact user-facing workflow is defined later.

---

## 19. Ignored content

Ignored content remains outside KAT physical snapshots.

The Git backend may use Git ignore semantics to determine ignored files.

KAT should not duplicate Git's ignore system without a concrete need.

A future KAT-specific ignore layer may be added only if KAT requires semantics
that cannot be represented by the backend.

---

## 20. Physical working-state inspection

The Git backend must be able to compare the current physical working state
against the workspace's expected base snapshot.

Given:

```text
Workspace
    base_revision = R42

R42.workspace_snapshot = W42
```

the backend evaluates physical changes relative to `W42`.

Possible physical changes include:

```text
modified tracked content
new tracked content
deleted tracked content
renamed or moved tracked content
untracked content
ignored content
```

These observations support KAT workspace status and snapshot creation.

---

## 21. Ordinary file editing

Normal physical editing does not change workspace authority.

For example:

```text
base snapshot = W42

working tree:
    W42 + local edits
```

remains a workspace based on `R42`.

The backend must distinguish this from moving the underlying Git repository to a
different history base.

---

## 22. Backend mismatch

A backend mismatch occurs when the Git repository's physical version-control
base is moved independently of the KAT workspace.

For example:

```text
KAT expects:
    W42 -> G42

backend currently based on:
    G51
```

This is different from:

```text
G42 + ordinary local edits
```

KAT must detect the mismatch.

It must not silently change:

```text
workspace.base_revision
semantic state
```

to follow Git.

---

## 23. Direct Git mutation

Operations equivalent to the following can create backend mismatch:

```text
checkout another commit
switch branches
hard reset
merge outside KAT
rebase outside KAT
rewrite physical ancestry
```

In a fully KAT-managed workspace, these operations should not be part of the
normal workflow.

If external Git manipulation still occurs, KAT must fail safely and preserve its
known repository base.

The exact recovery UX is defined later.

---

## 24. Read-only Git inspection

Read-only Git operations do not inherently threaten KAT consistency.

Examples include operations equivalent to:

```text
status
diff
log
show
```

However, KAT does not require users to use Git for normal inspection.

KAT should provide its own workspace-oriented projections.

---

## 25. Snapshot creation

Creating a physical snapshot captures the current tracked physical project state
as an immutable Git-backed snapshot.

Conceptually:

```text
working tracked state
        |
        v
GitWorkspaceBackend.create_snapshot
        |
        v
WorkspaceSnapshot W43
```

The operation must not modify semantic state.

A later KAT repository publication step binds `W43` to the appropriate
`SemanticState`.

---

## 26. Snapshot reuse

If tracked physical state is identical to the base `WorkspaceSnapshot`, KAT
should reuse the existing snapshot identity.

For example:

```text
base = W42
working tracked contents = W42

create_snapshot(...)
    -> W42
```

This preserves structural sharing and avoids artificial physical history.

---

## 27. Snapshot materialization

The backend must be able to reconstruct a physical working tree from a
`WorkspaceSnapshot`.

Conceptually:

```text
materialize(W42)
```

produces the tracked project state represented by `W42`.

Materialization must not independently choose a semantic state.

KAT determines the complete workspace base through the associated
`RepositoryRevision`.

---

## 28. Safe materialization

KAT must not silently destroy local physical work when materializing another
snapshot.

Before changing the physical base, the workspace layer must determine whether
local modifications can be safely preserved, rejected, or otherwise handled.

The exact switching workflow is defined in the collaboration workflow model.

The backend provides the capability to inspect and materialize state.

KAT determines when it is valid to do so.

---

## 29. Physical diff

The Git backend may provide physical differences between:

```text
WorkspaceSnapshot and WorkspaceSnapshot
```

or:

```text
WorkspaceSnapshot and working state
```

KAT may use those differences for:

- workspace status;
- Artifact materialization drift;
- reconciliation diagnostics;
- review;
- impact support.

Physical diffs do not themselves define semantic changes.

---

## 30. Artifact locator resolution

An Artifact may identify physical content through a locator.

For example:

```text
Artifact A
    locator = src/auth/service.rs
```

The Git backend must support resolving the locator against:

```text
accepted WorkspaceSnapshot
```

and, when required:

```text
current physical working state
```

The result is used to derive physical materialization identity.

---

## 31. Materialization identity

The backend must provide deterministic physical identity for Artifact
materialization.

Conceptually:

```text
resolve_materialization(locator, workspace_state)
    -> MaterializationId
```

For Git-backed content, possible mappings include:

```text
single file
    -> Git blob identity

directory
    -> Git tree identity

multiple paths
    -> deterministic aggregate identity
```

The KAT-facing result should remain a backend-neutral `MaterializationId`.

---

## 32. Working-tree materialization identity

Artifact drift must be detectable before a new physical snapshot is committed.

Therefore the backend must be able to compute materialization identity from the
current working tree.

For example:

```text
accounted materialization:
    M17

current working materialization:
    M18
```

If:

```text
M17 != M18
```

KAT can deterministically report physical drift.

This does not require a new KAT `RepositoryRevision` first.

---

## 33. Artifact physical accountability

The Git backend provides physical evidence required by KAT Artifact
accountability.

It does not determine semantic accountability.

Conceptually:

```text
GitWorkspaceBackend
    tells KAT:
        physical materialization changed

semantic model
    tells KAT:
        accounted semantic baseline changed

Artifact accountability
    combines both dimensions
```

This preserves:

```text
semantic CURRENT / STALE / UNACCOUNTED
```

separately from physical states such as:

```text
CURRENT / MODIFIED / MISSING / UNRESOLVED
```

---

## 34. Deleted Artifact materialization

If an Artifact locator resolved in its physical baseline but no longer resolves
in the working state, the backend reports the physical materialization as
missing.

Conceptually:

```text
baseline:
    M17

current:
    missing
```

KAT may surface:

```text
physical accountability = MISSING
```

The backend does not infer semantic meaning from the deletion.

---

## 35. Physical merge

During reconciliation, the Git backend attempts to reconcile divergent physical
histories relative to their common physical ancestry.

Conceptually:

```text
W0
├── WA
└── WB

merge(W0, WA, WB)
    -> physical result
```

Possible results:

```text
clean merged physical state
```

or:

```text
materialization conflicts
```

KAT wraps those results into the reconciliation model.

---

## 36. Git merge does not determine semantic reconciliation

A successful Git merge means only that the physical project state could be
combined according to backend rules.

It does not imply:

```text
semantic Changes are compatible
Artifacts remain current
validation remains sufficient
repository graph remains semantically valid
```

Similarly, a semantic reconciliation may succeed while Git reports physical
conflicts.

The two reconciliation dimensions remain independent.

---

## 37. MaterializationConflict translation

Backend physical conflicts must be translated into KAT
`MaterializationConflict` state.

For example:

```text
Git backend:
    base blob B0
    left blob BA
    right blob BB
    merge failed
```

becomes conceptually:

```text
MaterializationConflict
    kind = Content
    base = M0
    alternatives = [MA, MB]
    locator = src/service.rs
```

Git-specific conflict representation remains below the KAT conflict model.

---

## 38. Physical conflict resolution

A resolved physical conflict produces a new physical working result.

For example:

```text
base B0
left BA
right BB

        ↓

resolved BC
```

The backend stores `BC` as part of the resulting physical snapshot.

KAT records the reconciliation at the repository level.

The parent histories continue to preserve `BA` and `BB`.

---

## 39. Git remote transport

The Git backend may use standard Git-compatible remotes to transfer physical
objects.

Possible hosts include:

```text
GitHub
GitLab
self-hosted Git
future KAT Hub Git service
```

KAT's remote model determines which physical snapshots must be available.

The Git backend determines how those physical objects are transferred.

---

## 40. Fetching physical objects

Physical fetch makes remote Git objects required by known KAT
`WorkspaceSnapshot`s locally available.

KAT decides:

```text
which WorkspaceSnapshot is required
```

The Git backend performs:

```text
object discovery and transfer
```

Physical fetch does not independently move the KAT workspace base.

---

## 41. Publishing physical objects

Before a `RepositoryRevision` becomes a shared remote head, its referenced
physical snapshot must be available through the configured physical remote.

Conceptually:

```text
RepositoryRevision R43
    workspace_snapshot = W43

        ↓

GitWorkspaceBackend.publish(W43)
```

Only after remote physical availability is established may higher KAT layers
publish the complete repository revision.

---

## 42. Git branches and remote interoperability

A Git host may require refs to make physical commits reachable and transferable.

KAT may therefore maintain backend Git refs.

These refs are implementation mechanisms.

They do not become KAT repository heads or semantic branches.

For example, a backend may conceptually maintain:

```text
refs/kat/...
```

to protect or transport KAT-owned physical snapshots.

The exact Git ref namespace is intentionally not frozen in this document.

---

## 43. Physical object reachability

Every Git object required by a remotely visible KAT `WorkspaceSnapshot` must
remain retrievable.

KAT must prevent backend garbage collection from deleting physical snapshots
still referenced by live KAT repository history.

Possible implementation mechanisms include:

```text
protected internal Git refs
reachability roots
dedicated backend references
```

The exact mechanism is deferred.

The requirement is:

```text
KAT-referenced physical snapshot
    -> remains retrievable
```

---

## 44. Garbage collection boundary

Git may perform normal physical object garbage collection only when doing so
does not remove objects still required by KAT repository history.

KAT repository reachability and backend Git reachability are not assumed to be
identical.

The backend must bridge that difference.

---

## 45. External Git branches

Existing or interoperability Git branches may be present.

Their movement does not automatically create, delete, or move KAT
`RepositoryRevision`s.

For example:

```text
Git branch main:
    G42 -> G50
```

does not imply:

```text
KAT shared head:
    R42 -> R50
```

unless a KAT collaboration operation explicitly establishes that repository
transition.

---

## 46. CI and external tooling compatibility

Because the physical project remains Git-backed, external tooling may still
consume physical Git snapshots.

Examples include:

```text
CI systems
deployment systems
code review tools
GitHub
GitLab
IDEs
```

Such systems operate on the physical projection of the project.

They do not automatically understand KAT semantic state.

Future integrations may associate Git physical snapshots with
`RepositoryRevision`s to expose semantic context.

---

## 47. Mapping RepositoryRevision to physical snapshot

KAT must be able to determine the physical backend state corresponding to any
known `RepositoryRevision`.

Conceptually:

```text
RepositoryRevision R42
    workspace_snapshot = W42

GitWorkspaceBackend
    W42 -> G42
```

This mapping must be deterministic and persistent.

KAT must never need to guess which Git branch or current checkout corresponds to
`R42`.

---

## 48. Mapping is immutable

Once:

```text
W42 -> G42
```

is established, it must not later resolve to another Git physical snapshot.

Movable Git names may point to `G42`, but the KAT workspace snapshot binding is
immutable.

---

## 49. Backend metadata

The Git backend may require local metadata such as:

```text
Git repository location
remote URLs
backend snapshot mappings
protected refs
working-tree association
```

This metadata is implementation state.

It is not automatically canonical KAT semantic knowledge.

The exact persistence layout is defined during implementation.

---

## 50. RepositoryRevision creation interaction

Creating a new accepted `RepositoryRevision` coordinates semantic and physical
results.

Conceptually:

```text
Workspace
    base = R42

semantic candidate:
    S43

physical candidate:
    W43

        ↓

RepositoryRevision R43
    parent = R42
    semantic_state = S43
    workspace_snapshot = W43
```

The Git backend is responsible only for producing and preserving `W43`.

Higher KAT layers create and accept `R43`.

---

## 51. Atomic repository visibility

Git snapshot creation and semantic state creation may occur before final
repository publication because both are immutable.

KAT should make the new repository revision authoritative only after all
required objects exist.

Conceptually:

```text
prepare semantic state
prepare physical snapshot
validate
create RepositoryRevision
advance local KAT repository head
```

If preparation fails, the previous accepted repository revision remains
unchanged.

Unused immutable backend objects may be cleaned later.

---

## 52. Physical snapshot integrity

The backend must be able to verify that a referenced `WorkspaceSnapshot`
resolves to valid physical content.

A KAT repository revision whose physical snapshot cannot be resolved is not
fully reconstructable.

This is a repository/backend integrity condition.

It is distinct from:

```text
Artifact staleness
semantic conflict
GraphQuality
validation findings
```

---

## 53. Backend failure

Git backend failures must not silently mutate KAT repository state.

For example, if:

```text
physical snapshot creation fails
```

then KAT must not publish a repository revision referencing that failed
snapshot.

Likewise, if:

```text
physical materialization fails
```

KAT must not silently activate the semantic state of the target revision while
leaving another physical state active.

Semantic and physical activation remain coordinated through
`RepositoryRevision`.

---

## 54. Initial implementation strategy

The initial implementation should favor mature native Git behavior over
reimplementing Git internals.

A practical first backend may invoke the system Git implementation behind a
Rust abstraction.

Conceptually:

```text
WorkspaceBackend trait
        |
        v
GitWorkspaceBackend
        |
        v
native Git executable
```

This keeps the initial scope focused on KAT collaboration semantics.

The exact Rust integration mechanism is an implementation decision and may later
change to another Git library without altering the workspace model.

---

## 55. Backend replacement

The KAT repository model must not require Git permanently.

A future backend could theoretically implement:

```text
WorkspaceBackend
```

using another content-addressed physical store.

For example:

```text
NativeKatWorkspaceBackend
```

The following KAT concepts must remain stable across backend replacement:

```text
RepositoryRevision
Workspace
WorkspaceSnapshot
MaterializationId
MaterializationConflict
```

Backend-specific representations remain below those abstractions.

---

## 56. Initial backend scenarios

The Git backend should be evaluated against at least the following scenarios.

### GIT-01: Clean physical snapshot

Given:

```text
workspace base = W1
working tracked state unchanged
```

Expected:

```text
create_snapshot -> W1
```

No artificial physical revision is created.

---

### GIT-02: Tracked file modification

Given:

```text
W1
```

and:

```text
src/service.rs modified
```

Expected:

```text
working state reports physical modification
create_snapshot -> W2
W2 != W1
```

---

### GIT-03: Untracked file

Given:

```text
debug.log untracked
```

Expected:

```text
reported as untracked
not silently included in W2
```

---

### GIT-04: Ignored file

Given:

```text
build/output.bin ignored
```

Expected:

```text
excluded from WorkspaceSnapshot
```

---

### GIT-05: Artifact physical drift

Given:

```text
Artifact A
    locator = src/service.rs
    physical baseline = M1
```

When:

```text
src/service.rs changes
```

Expected:

```text
current materialization = M2
M2 != M1

semantic accountability unchanged
physical accountability = MODIFIED
```

---

### GIT-06: Semantic-only repository revision

Given:

```text
semantic S1 -> S2
physical unchanged W1
```

Expected:

```text
new RepositoryRevision
same WorkspaceSnapshot W1
no unnecessary new Git physical snapshot
```

---

### GIT-07: Physical-only repository revision

Given:

```text
semantic unchanged S1
physical W1 -> W2
```

Expected:

```text
new RepositoryRevision
same SemanticState S1
new WorkspaceSnapshot W2
```

---

### GIT-08: External backend movement

Given:

```text
KAT base expects W1/G1
```

When backend is externally moved to:

```text
G2
```

Expected:

```text
backend mismatch detected
KAT base unchanged
semantic state unchanged
no silent adoption
```

---

### GIT-09: Ordinary local edit

Given:

```text
KAT base W1/G1
```

When:

```text
tracked file modified without changing Git ancestry
```

Expected:

```text
normal physical working modification
not backend mismatch
```

---

### GIT-10: Clean physical reconciliation

Given:

```text
W0
├── WA
└── WB
```

with independent physical edits.

Expected:

```text
Git backend produces WC
no MaterializationConflict
```

---

### GIT-11: Physical content conflict

Given incompatible concurrent edits.

Expected:

```text
backend preserves base/left/right alternatives
KAT receives MaterializationConflict
no silent winner
```

---

### GIT-12: Physical remote publication

Given:

```text
R2.workspace_snapshot = W2
```

Expected:

```text
W2 becomes remotely retrievable before R2 is published as shared KAT history
```

---

### GIT-13: Historical physical snapshot reachability

Given:

```text
R1 -> W1
R2 -> W2
```

and current development has advanced beyond both.

Expected:

```text
W1 and W2 remain retrievable while required by retained KAT history
```

---

### GIT-14: Existing Git project adoption

Given an existing Git repository with substantial previous history.

Expected:

```text
KAT establishes adoption revision R0
current physical state becomes W0
historical Git history is preserved physically
no synthetic historical semantic knowledge is created
```

---

## 57. Open implementation decisions

The following decisions are intentionally not frozen by this model.

### 57.1 Exact Git repository layout

Options include:

```text
internal bare repository + managed working tree
internal non-bare repository
controlled reuse of existing .git repository
```

The preferred direction is KAT-managed internal storage.

---

### 57.2 Exact WorkspaceSnapshotId representation

The KAT model requires an immutable backend-neutral snapshot identity.

Whether the first implementation directly wraps a Git commit OID or stores a
KAT-level mapping remains open.

---

### 57.3 Git hash algorithm exposure

KAT should not assume that Git's object identifier representation is equivalent
to KAT's canonical `ObjectId`.

The relationship between Git object IDs and KAT identifiers must be explicit.

---

### 57.4 Hidden Git ref layout

KAT may require protected internal refs for:

```text
remote transfer
physical reachability
garbage-collection safety
```

The exact ref namespace is deferred.

---

### 57.5 System Git versus Git library

Possible implementations include:

```text
native Git subprocess
Git library
pure-Rust Git implementation
```

The repository and workspace models must remain independent from this choice.

---

## 58. Non-goals

This model does not define:

- exact Git command sequences;
- exact Git ref names;
- exact Git repository directory layout;
- Git credential handling;
- GitHub API integration;
- Git LFS behavior;
- large-object storage;
- submodule semantics;
- sparse checkout;
- partial clone policy;
- user-facing branch semantics;
- named KAT refs;
- reconciliation UX;
- conflict-resolution CLI;
- KAT Hub API;
- CI integration behavior;
- repository policy.

Those concerns may be defined after the backend semantics are validated.

---

## Summary

Git is the initial physical workspace backend for KAT.

```text
KAT
 |
 +-- RepositoryRevision
 |       |
 |       +-- SemanticState
 |       |
 |       +-- WorkspaceSnapshot
 |                  |
 |                  v
 |          GitWorkspaceBackend
 |                  |
 |                  v
 |              Git objects
 |
 +-- collaboration semantics
 +-- repository heads
 +-- reconciliation
 +-- conflicts
```

Git provides:

```text
physical tracking
immutable physical storage
physical ancestry
diffing
merging
transport
```

KAT provides:

```text
complete software revision identity
semantic authority
workspace authority
semantic evolution
collaboration history
reconciliation semantics
conflict semantics
```

The physical backend must never become a second repository authority.

A KAT workspace is always based on an explicit `RepositoryRevision`, and every
accepted revision binds one semantic state to one immutable physical workspace
snapshot.

Git is therefore infrastructure underneath KAT, not the version-control model
presented to KAT users.