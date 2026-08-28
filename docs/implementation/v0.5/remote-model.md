# KAT v0.5 Remote Model

## Purpose

This document defines how KAT repositories collaborate across machines and
storage locations.

A remote allows a local KAT repository to discover and exchange accepted
repository history with other copies of the same logical repository.

KAT repository state contains two coordinated dimensions:

- semantic repository state;
- physical workspace state.

These dimensions may use different storage and transport systems.

The remote model therefore defines:

- repository identity;
- remote identity and location;
- semantic and physical remote responsibilities;
- shared repository heads;
- fetch;
- publication;
- synchronization;
- clone;
- object availability;
- remote consistency;
- divergence;
- offline operation;
- failure behavior.

This document does not define:

- Git plumbing;
- Git ref layouts;
- HTTP API endpoints;
- authentication protocols;
- authorization rules;
- concrete CLI syntax;
- review workflows;
- reconciliation algorithms;
- federation between different KAT repositories.

---

## 1. Motivation

A local KAT repository must be able to evolve independently.

For example:

```text
Developer A
    local repository

Developer B
    local repository

Shared remote
```

Both developers may create accepted local `RepositoryRevision`s without
immediate network access.

Later, they must be able to exchange those revisions without losing:

- semantic knowledge;
- physical workspace history;
- repository ancestry;
- Artifact accountability context;
- competing accepted histories.

The remote model therefore treats synchronization as exchange of immutable
repository objects and repository references.

It does not treat synchronization as replacement of one user's repository state
with another.

---

## 2. Repository identity

A KAT repository has stable repository identity.

Conceptually:

```text
RepositoryId
```

`RepositoryId` identifies the logical KAT repository independently from where
that repository is stored.

Therefore:

```text
repository identity != remote location
```

For example, the same repository may move from:

```text
GitHub
```

to:

```text
self-hosted Git
```

without becoming a different KAT repository.

Likewise, the semantic remote may move between KAT Hub instances without
changing repository identity.

---

## 3. Local repository and remote repository

A local repository is an independently usable copy of KAT repository history.

It contains enough repository state to support local work.

Conceptually:

```text
LocalRepository
├── RepositoryId
├── semantic objects
├── RepositoryRevision history
├── local repository view
├── local heads
├── workspaces
└── physical workspace backend
```

A remote is another location through which repository history may be exchanged.

A remote is not the semantic definition of the repository.

It is a collaboration endpoint.

---

## 4. Remote

A remote associates a logical KAT repository with one or more transport
locations.

Conceptually:

```text
Remote {
    repository_id
    semantic_transport
    workspace_transport
}
```

The exact representation is defined separately.

A remote may use:

```text
semantic transport
    -> KAT Hub

workspace transport
    -> Git hosting
```

The two transports may be hosted by the same service or different services.

---

## 5. Semantic remote

The semantic remote stores and transfers KAT-native repository information.

Its responsibilities include preserving and exposing:

- immutable semantic objects;
- `SemanticState`s;
- `ChangeRevision`s;
- `RepositoryRevision`s;
- shared repository heads;
- repository-level metadata required for synchronization.

A KAT Hub is the expected initial semantic remote service.

The semantic remote does not need to store physical project blobs if the
physical workspace is delegated to another backend.

---

## 6. Workspace remote

The workspace remote stores and transfers versioned physical project state.

Its responsibilities include:

- immutable physical content;
- physical workspace snapshots;
- physical ancestry required by the workspace backend;
- retrieval of physical snapshots referenced by KAT repository history.

The initial physical backend may use Git.

The physical remote may therefore be hosted by:

- GitHub;
- GitLab;
- a self-hosted Git server;
- a future KAT Hub physical-storage service.

The remote model does not depend on a specific Git hosting provider.

---

## 7. One logical remote may span multiple hosts

A KAT remote is a logical collaboration endpoint.

Its semantic and physical transports may resolve to different hosts.

For example:

```text
Remote origin

RepositoryId:
    R

Semantic:
    kat.example.com/repositories/R

Workspace:
    github.com/example/project.git
```

This is still one logical KAT remote.

The fact that the two dimensions use different hosts is an implementation and
deployment detail.

---

## 8. Remote hosting does not define authority

Neither the semantic host nor the physical host independently defines the
complete repository state.

The complete accepted software state is represented by:

```text
RepositoryRevision
```

which binds:

```text
SemanticState
+
WorkspaceSnapshot
```

The remote's responsibility is to preserve and distribute those immutable
objects and the references that make accepted repository history visible.

---

## 9. Shared heads

A remote exposes one or more shared repository heads.

A shared head identifies an accepted `RepositoryRevision` visible through that
remote.

For example:

```text
shared heads:
    RA
    RB
```

Multiple shared heads are valid.

They represent divergent accepted repository histories.

The remote must not automatically discard one head merely because another
revision was published later.

---

## 10. Remote heads are RepositoryRevision references

Remote collaboration operates on `RepositoryRevision`s.

A remote head therefore points to:

```text
RepositoryRevisionId
```

not directly to:

```text
SemanticStateId
```

or:

```text
Git branch
```

This preserves the semantic and physical binding established by the repository
revision model.

---

## 11. Git branches are not KAT remote heads

When Git is used as the workspace backend, Git branches may exist for backend
storage or interoperability.

They do not define KAT repository authority.

KAT must not interpret:

```text
Git branch main moved
```

as:

```text
KAT shared head moved
```

unless that movement occurs through an explicit KAT repository operation.

A KAT shared head always identifies a complete `RepositoryRevision`.

---

## 12. Immutable object exchange

Remote synchronization should primarily exchange immutable objects.

Conceptually:

```text
local known objects
remote known objects

        ↓

discover missing objects

        ↓

transfer missing objects
```

This applies independently to:

```text
KAT semantic/repository objects
physical workspace objects
```

Immutable object transfer does not itself modify accepted repository heads.

---

## 13. Fetch

Fetch imports remote repository knowledge into the local repository view.

Conceptually:

```text
fetch(remote)
```

may:

1. discover remote shared heads;
2. discover missing `RepositoryRevision`s;
3. fetch missing semantic objects;
4. fetch or make available required physical objects;
5. update local knowledge of the remote view.

Fetch does not automatically change the active workspace base.

---

## 14. Fetch does not automatically integrate

Suppose the local repository has:

```text
local head:
    RA
```

and fetch discovers:

```text
remote head:
    RB
```

If:

```text
RA != RB
```

KAT does not automatically replace `RA` with `RB`.

Instead, the local repository may now know:

```text
local history:
    RA

remote-visible history:
    RB
```

If the two are divergent, that divergence becomes explicit.

Reconciliation is a separate process.

---

## 15. Fast-forward case

If the fetched remote head is a descendant of the local known head:

```text
R0 -> R1 -> R2
```

with:

```text
local:
    R1

remote:
    R2
```

then no divergence exists between those histories.

The local repository may update its view of the shared head to `R2`.

The active workspace does not necessarily move automatically.

Workspace movement remains explicit.

---

## 16. Local-ahead case

If the local head is a descendant of the remote shared head:

```text
R0 -> R1 -> R2
```

with:

```text
remote:
    R1

local:
    R2
```

the local repository contains accepted work not yet visible remotely.

This is not a conflict.

It means the local repository is ahead of the shared remote history.

---

## 17. Divergent case

If neither head is an ancestor of the other:

```text
        R0
       /  \
      RA  RB
```

the repository histories are divergent.

Fetch must preserve both.

Conceptually:

```text
local repository view:
    heads = [RA, RB]
```

The remote model does not automatically reconcile them.

Reconciliation remains governed by the reconciliation model.

---

## 18. Publication

Publication makes a locally accepted repository revision visible through a
remote.

Conceptually:

```text
publish(R43, remote)
```

does not merely upload a reference.

Before the revision becomes a shared head, the remote must be able to
reconstruct all state required by that revision.

This includes both semantic and physical dimensions.

---

## 19. Publication completeness

A `RepositoryRevision` must not become a shared accepted remote head unless the
objects required to reconstruct it are remotely available.

For:

```text
R43
├── semantic_state = S43
└── workspace_snapshot = W43
```

the remote must be able to resolve:

```text
R43
S43
all semantic objects required by S43
W43
all physical objects required by W43
```

and any other immutable repository objects required by the revision.

---

## 20. Physical-first availability

When semantic and physical state use separate remotes, publication must avoid
creating a shared repository revision whose physical snapshot is unavailable.

Conceptually:

```text
1. make physical snapshot remotely available
2. make semantic/repository objects remotely available
3. verify complete RepositoryRevision
4. publish shared head
```

The exact network protocol is defined separately.

The important invariant is that shared-head visibility occurs only after the
revision is complete.

---

## 21. Semantic upload failure after physical upload

Suppose physical objects are successfully uploaded but semantic publication
fails.

Then:

```text
physical objects exist remotely
shared KAT head remains unchanged
```

This does not corrupt repository state.

The uploaded physical objects are immutable but not yet referenced by a new
shared KAT revision.

They may later become reachable or be collected according to backend storage
policy.

---

## 22. Shared-head publication failure

Suppose all objects required by `R43` are uploaded, but another contributor
changes the shared head before publication completes.

For example:

```text
expected shared head:
    R42

actual shared head:
    R44
```

KAT must not silently overwrite `R44`.

The publication attempt fails as a shared-head update.

The already uploaded immutable objects remain valid.

The local `R43` remains an accepted local revision.

The repository now has potential divergence:

```text
        R42
       /   \
     R43   R44
```

---

## 23. Compare-and-swap publication

Shared-head movement should use an expected-current-state check.

Conceptually:

```text
publish:
    expected = R42
    candidate = R43
```

The remote updates the shared head only if the currently visible head is still
`R42`.

Conceptually:

```text
CAS(R42 -> R43)
```

If the remote head changed concurrently, publication does not destroy that
concurrent history.

This enables optimistic multi-user collaboration without a global repository
lock.

---

## 24. Multiple-head publication

A remote may support multiple shared heads rather than forcing every concurrent
publication into one pointer.

For example:

```text
        R42
       /   \
     R43   R44
```

both may become visible accepted heads.

This allows divergence to be preserved as repository state.

Whether a named shared reference requires a single head is a separate
repository-policy or reference-model concern.

---

## 25. Fetch and publication are distinct

Fetch and publication serve different directions of collaboration.

```text
fetch
    remote -> local knowledge

publish
    local accepted history -> remote visibility
```

Neither operation implies automatic reconciliation.

A higher-level synchronization workflow may combine them.

---

## 26. Synchronization

Synchronization is the process of bringing local knowledge of repository
history and remote knowledge of repository history up to date.

Conceptually:

```text
sync(remote)
    fetch remote history
    discover local/remote relationship
    publish eligible local objects/revisions
    report resulting repository view
```

Synchronization does not imply that divergent heads become one revision.

If divergence exists, synchronization reports it.

Reconciliation is required to produce a common descendant.

---

## 27. Synchronization outcomes

Synchronization may result in states such as:

### Up to date

```text
local shared view = remote shared view
```

---

### Remote ahead

```text
local known head:
    R42

remote:
    R43
```

with `R43` descending from `R42`.

---

### Local ahead

```text
remote:
    R42

local:
    R43
```

with `R43` descending from `R42`.

---

### Diverged

```text
        R42
       /   \
     R43   R44
```

Both histories remain preserved.

---

### Incomplete remote state

A referenced repository object or physical snapshot cannot be retrieved.

This is a remote-integrity failure, not semantic divergence.

---

## 28. Workspace independence from synchronization

Synchronization must not silently change an active workspace's base revision.

Suppose:

```text
Workspace:
    base = R42
```

and synchronization discovers:

```text
remote head = R45
```

The workspace remains:

```text
base = R42
```

until an explicit workspace operation changes it.

This prevents remote activity from silently changing the semantic or physical
context of in-progress work.

---

## 29. Offline operation

A KAT repository must support local accepted evolution without requiring a
remote.

A contributor may:

```text
author semantic work
modify physical content
create local RepositoryRevision
inspect history
validate repository state
```

while offline.

Remote availability is required only for operations that exchange repository
state with other locations.

This preserves KAT's distributed collaboration model.

---

## 30. Clone

Clone creates a new local copy of an existing KAT repository.

Conceptually:

```text
clone(remote)
```

must discover:

```text
RepositoryId
repository metadata
shared heads
semantic repository history
workspace transport
```

and establish a local repository corresponding to the same logical
`RepositoryId`.

---

## 31. Clone produces one coherent initial workspace

A normal developer clone must establish one selected accepted
`RepositoryRevision` as its initial local workspace base.

For example:

```text
selected revision:
    R42

R42
├── semantic_state = S42
└── workspace_snapshot = W42
```

Clone then makes available:

```text
S42
W42
```

and materializes the physical workspace corresponding to `W42`.

The result is one coherent KAT workspace.

Clone must not independently select:

```text
latest semantic state
```

and:

```text
Git default branch
```

because those may not correspond to the same repository revision.

---

## 32. Clone and repository history

A normal clone should obtain enough semantic repository history to support KAT
inspection and collaboration.

Historical physical content may be fetched lazily if the physical backend
supports it.

At minimum:

- current selected `RepositoryRevision` must be complete;
- its physical workspace must be materializable;
- required semantic history must be available for supported local operations;
- referenced historical physical objects must remain retrievable from the
  remote when needed.

The exact clone depth and storage policy are defined separately.

---

## 33. Remote physical snapshot reachability

Every `WorkspaceSnapshot` referenced by remotely visible KAT history must
remain retrievable.

If Git is the physical backend, KAT must ensure that referenced physical
snapshots are protected from ordinary backend garbage collection.

The specific mechanism is defined by the Git workspace backend model.

The remote model only requires retrievability.

---

## 34. Remote integrity

A remote repository view is internally consistent only if every visible shared
`RepositoryRevision` can resolve all immutable repository state required by
that revision.

For a visible head:

```text
R
```

the remote must be able to resolve at least:

```text
R
R.semantic_state
R.workspace_snapshot
R.parents
R.semantic_change, when present
```

and all transitively required immutable objects.

A broken reference constitutes remote-integrity failure.

---

## 35. Semantic and physical remote mismatch

Because semantic and physical state may be stored separately, one service may
be reachable while the other is unavailable.

For example:

```text
KAT Hub:
    available

Git host:
    unavailable
```

KAT may still fetch semantic repository metadata and discover repository heads.

However, a revision whose physical snapshot cannot currently be retrieved is
not fully materializable.

KAT must report this condition explicitly.

It must not silently substitute another physical snapshot.

---

## 36. Temporary unavailability is not repository corruption

The distinction must be preserved between:

```text
object does not exist remotely
```

and:

```text
remote service is temporarily unavailable
```

The former is an integrity problem.

The latter is a transport/availability problem.

The exact error model is defined separately.

---

## 37. Remote repository movement

A repository may change hosting location.

For example:

```text
workspace remote:
    GitHub

        ↓

workspace remote:
    self-hosted Git
```

or:

```text
semantic remote:
    KAT Hub A

        ↓

semantic remote:
    KAT Hub B
```

As long as:

```text
RepositoryId
```

remains the same and repository history is preserved, this does not create a
new KAT repository.

Remote locators are configuration, not identity.

---

## 38. Multiple remotes

A local KAT repository may eventually know multiple remotes.

For example:

```text
origin
company
backup
```

Each remote may expose a different repository view.

The local repository may track which heads were observed from each remote.

The exact multi-remote reference model is deferred.

The repository model must not assume there can only ever be one remote.

---

## 39. Same RepositoryId requirement

Ordinary synchronization and publication operate between copies of the same
logical KAT repository.

Therefore:

```text
local RepositoryId
    ==
remote RepositoryId
```

is required for normal same-repository collaboration.

If two repositories have different `RepositoryId`s, they are independent KAT
repositories.

Connecting knowledge across those repositories belongs to the future
federation model.

---

## 40. Forking versus federation

Creating an independent repository from an existing KAT repository is distinct
from normal synchronization.

A detached fork may receive:

```text
new RepositoryId
```

while preserving provenance to the origin repository.

Federation may later allow semantic references between independent
repositories.

Neither behavior is defined by the v0.5 remote model.

---

## 41. KAT Hub

A KAT Hub is a remote service capable of hosting the KAT-native portion of
repository collaboration.

The initial Hub may provide only:

```text
repository registration
RepositoryId lookup
immutable KAT object storage
RepositoryRevision storage
shared-head storage
object discovery
object transfer
atomic shared-head updates
```

A Hub does not initially need to provide:

- Git hosting;
- web-based review;
- CI;
- organizations;
- semantic visualization;
- federation.

Those may be added later without changing the remote model.

---

## 42. External Git hosting

A KAT Hub may delegate physical workspace storage to an external Git host.

For example:

```text
KAT Hub
    semantic/repository history

GitHub
    physical workspace history
```

The `RepositoryRevision` provides the consistency boundary between them.

For:

```text
R42
```

KAT records an immutable association with:

```text
SemanticState S42
WorkspaceSnapshot W42
```

It does not associate `S42` with a movable Git branch.

---

## 43. KAT-managed Git access

When Git is the workspace backend, normal KAT collaboration should operate
through KAT rather than direct Git synchronization commands.

KAT is responsible for coordinating:

```text
semantic fetch
physical fetch
RepositoryRevision discovery
workspace consistency
publication
divergence detection
```

Direct Git remote movement must not silently redefine KAT repository state.

The exact backend-interoperability rules are defined in the Git workspace
backend model.

---

## 44. Remote knowledge preservation

Remote collaboration must not use last-writer-wins semantics over accepted
repository history.

Suppose:

```text
Alice publishes RA
Bob independently publishes RB
```

from a common base.

The remote model must preserve both histories or reject an unsafe reference
update without deleting either revision.

It must never silently replace one accepted history because another arrived
later.

---

## 45. Remote references and repository history

Mutable remote references are discovery and collaboration mechanisms.

They do not alter immutable repository history.

Moving:

```text
shared ref:
    R42 -> R43
```

does not mutate `R42`.

Likewise, removing a mutable reference does not by itself erase immutable
repository history.

Retention and garbage-collection policy are defined separately.

---

## 46. Remote state and repository policy

A remote service may eventually enforce policies before allowing publication to
selected shared references.

Examples may include:

- required validation;
- no unresolved conflicts;
- Artifact-accountability requirements;
- review requirements.

Those are remote/project governance rules.

They are distinct from the fundamental remote consistency model.

---

## 47. Remote evaluation scenarios

The remote model should initially be validated against the following scenarios.

### REM-01: Clean clone

Remote:

```text
shared head = R10
```

Expected:

```text
local RepositoryId matches remote
R10 semantic state available
R10 physical snapshot available
workspace materialized from R10
```

---

### REM-02: Remote fast-forward

Local:

```text
R10
```

Remote:

```text
R10 -> R11
```

Expected:

```text
fetch discovers R11
no divergence
active workspace remains on its explicit base until moved
```

---

### REM-03: Local ahead

Remote:

```text
R10
```

Local:

```text
R10 -> R11
```

Expected:

```text
R11 remains valid local accepted history
eligible for publication
```

---

### REM-04: Concurrent publication

Base:

```text
R10
```

Alice:

```text
R10 -> RA
```

Bob:

```text
R10 -> RB
```

Expected:

```text
neither accepted history is silently lost
divergence becomes visible
```

---

### REM-05: Physical upload succeeds, semantic publication fails

Expected:

```text
shared KAT head unchanged
local RepositoryRevision preserved
remote physical objects may remain unreferenced
repository consistency preserved
```

---

### REM-06: Concurrent shared-head update

Client expects:

```text
remote = R10
```

but remote became:

```text
R11
```

before publication.

Expected:

```text
head update fails safely
R11 not overwritten
local candidate remains available
```

---

### REM-07: Semantic host available, workspace host unavailable

Expected:

```text
repository metadata may be fetched
physical materialization reported unavailable
no substitute WorkspaceSnapshot selected
```

---

### REM-08: Missing physical snapshot

Remote advertises:

```text
R10 -> W10
```

but `W10` cannot be resolved because required physical objects do not exist.

Expected:

```text
remote integrity failure
```

---

### REM-09: Workspace remains stable after fetch

Workspace:

```text
base = R10
```

Fetch discovers:

```text
R11
```

Expected:

```text
workspace base remains R10
remote knowledge updated
```

---

### REM-10: Repository moved to another physical host

Before:

```text
RepositoryId = K
workspace remote = Host A
```

After migration:

```text
RepositoryId = K
workspace remote = Host B
```

Expected:

```text
same logical KAT repository
history remains valid
```

---

## 48. Non-goals

This model does not define:

- concrete KAT Hub API endpoints;
- wire encoding;
- authentication;
- authorization;
- user accounts;
- organizations;
- Git protocol implementation;
- Git ref naming;
- physical pack transfer;
- GitHub-specific APIs;
- reconciliation implementation;
- conflict resolution;
- branches or bookmark UX;
- review requests;
- CI;
- federation;
- exact garbage-collection policy;
- exact CLI commands.

Those concerns build on this remote model.

---

## Summary

A KAT remote distributes complete repository history without becoming the
definition of that history.

```text
                   KAT Repository
                        |
                 RepositoryId
                        |
             RepositoryRevision DAG
                  /             \
                 /               \
        Semantic state       Physical state
              |                   |
              v                   v
          KAT Hub            Workspace remote
                             GitHub / GitLab /
                             self-hosted Git
```

Semantic and physical state may be hosted separately.

Consistency is preserved because every accepted `RepositoryRevision` binds one
immutable `SemanticState` to one immutable `WorkspaceSnapshot`.

Remote collaboration exchanges immutable objects and shared
`RepositoryRevision` references.

Fetching does not silently move local workspaces.

Publishing does not silently overwrite concurrent history.

Divergent accepted histories remain visible until reconciliation explicitly
produces a common descendant.

Repository identity remains independent from hosting location.

The KAT Hub therefore acts as the shared collaboration service for KAT-native
repository state, while physical workspace storage may initially be delegated to
existing Git infrastructure.