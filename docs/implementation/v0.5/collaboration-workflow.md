# KAT v0.5 Collaboration Workflow

## Purpose

This document defines the user-facing collaboration workflow for KAT v0.5.

KAT coordinates semantic state, physical workspace state, repository history,
and remote collaboration through one repository-level workflow.

Users work with `RepositoryRevision`s rather than managing semantic state and
Git state independently.

This document defines:

- local work;
- local commit;
- synchronization;
- publication;
- repository movement;
- divergence;
- reconciliation;
- named references.

It does not define exact CLI syntax, Git plumbing, remote protocols, or
conflict object representation.

---

## 1. User model

A contributor works from one `RepositoryRevision`.

```text
RepositoryRevision
├── SemanticState
└── WorkspaceSnapshot
```

The workspace may then evolve semantically, physically, or in both dimensions.

```text
RepositoryRevision
        ↓
     Workspace
      /      \
semantic    physical
  work        work
      \      /
       ↓    ↓
new RepositoryRevision
```

KAT coordinates both dimensions.

Git remains an implementation backend and is not part of the normal
user-facing version-control workflow.

---

## 2. Core workflow

The normal collaboration lifecycle is:

```text
clone/open
    ↓
inspect
    ↓
work
    ↓
check
    ↓
commit locally
    ↓
sync
    ↓
push or reconcile
```

Local repository evolution does not require network access.

Remote publication is separate from local commit.

---

## 3. Local work

A workspace has an explicit base `RepositoryRevision`.

From that base, a contributor may perform:

### Semantic-only work

```text
SemanticState changes
WorkspaceSnapshot unchanged
```

Examples include Requirement, Constraint, or Design Decision evolution.

### Physical-only work

```text
SemanticState unchanged
WorkspaceSnapshot changes
```

Examples include documentation, configuration, or non-semantic physical
changes.

### Combined work

```text
SemanticState changes
WorkspaceSnapshot changes
```

This is common when implementation knowledge and physical artifacts evolve
together.

Physical edits never implicitly mutate semantic knowledge.

---

## 4. Status and health

`status` should expose the contributor's current position and working state.

At minimum it should make visible:

```text
workspace base
local accepted head
semantic modifications
physical modifications
remote relationship
workspace consistency
```

`check` remains responsible for repository health.

Collaboration does not introduce a separate health model.

Artifact semantic staleness and physical materialization drift remain distinct
accountability findings.

---

## 5. Local commit

A local commit creates a new accepted `RepositoryRevision`.

For example:

```text
Workspace based on R42

semantic result = S43
physical result = W43

        ↓

R43
├── parent = R42
├── semantic_state = S43
└── workspace_snapshot = W43
```

The new revision becomes accepted local history.

It is not automatically published remotely.

After a successful commit, the workspace is based on the new revision.

---

## 6. Synchronization

Synchronization updates the local repository's knowledge of shared repository
history.

Conceptually:

```text
sync
    ↓
discover remote heads
fetch missing repository state
compare local and remote history
```

Synchronization does not silently:

- move the active workspace;
- reconcile divergent revisions;
- discard local history.

After synchronization, the relationship may be:

```text
up to date
local ahead
remote ahead
diverged
```

---

## 7. Repository movement

Moving to another accepted software state operates on a complete
`RepositoryRevision`.

Conceptually:

```text
switch R43
```

moves both:

```text
SemanticState
WorkspaceSnapshot
```

together.

This is not equivalent to a Git checkout.

Local work must not be silently destroyed during repository movement.

---

## 8. Publication

Publication makes accepted local repository history visible to collaborators.

Conceptually:

```text
push
```

coordinates:

```text
physical snapshot availability
semantic object availability
RepositoryRevision availability
shared reference update
```

A revision must not become shared before all state required to reconstruct it is
remotely available.

Concurrent remote history must never be silently overwritten.

---

## 9. Divergence

Concurrent accepted histories are valid.

```text
        R0
       /  \
      RA  RB
```

This represents divergence, not conflict.

Both histories remain preserved.

If one history is simply a descendant of the other, reconciliation is not
required.

If neither is an ancestor of the other, KAT may reconcile them.

---

## 10. Reconciliation

Reconciliation attempts to produce a common descendant of divergent accepted
histories.

```text
        R0
       /  \
      RA  RB
       \  /
        RC
```

KAT evaluates independently:

```text
semantic reconciliation
physical reconciliation
```

Possible results are:

```text
clean reconciliation
reconciliation with health consequences
blocked reconciliation
```

A blocked reconciliation preserves explicit semantic or materialization
conflicts until resolution.

No competing accepted history is silently discarded.

---

## 11. Conflict interaction

Conflicts are first-class reconciliation state.

Users must be able to inspect:

```text
common base
competing alternatives
affected identities or materializations
reason automatic reconciliation failed
```

Conflict resolution is explicit.

A final reconciliation revision is accepted only after all blocking conflicts
are resolved.

Staleness, impact, validation findings, and GraphQuality findings are not
automatically conflicts.

---

## 12. Named references

Named references are optional human-readable pointers to
`RepositoryRevision`s.

Conceptually:

```text
main       -> R42
release    -> R37
experiment -> RA
```

They are movable names over immutable history.

They do not own or contain repository history.

A workspace is based on a revision, not inherently on a named branch.

This keeps the collaboration model closer to bookmark-oriented workflows than
mandatory branch-oriented workflows.

---

## 13. Git boundary

Normal KAT collaboration should not require direct Git version-control
operations.

Users should not need to coordinate:

```text
git pull
git checkout
git merge
git push
```

with KAT semantic state.

KAT uses the Git backend internally for physical storage, transport, and merge
capabilities.

Direct backend movement that causes the physical workspace to stop matching the
KAT workspace base must be detected and must not silently change semantic state.

---

## 14. Core command responsibilities

The expected collaboration intentions are:

```text
clone
    create a local repository and coherent workspace

status
    inspect local and collaboration state

author
    express semantic evolution

check
    evaluate repository health

commit
    create an accepted local RepositoryRevision

sync
    discover and fetch shared history

switch
    activate another RepositoryRevision

reconcile
    join divergent accepted histories

push
    publish accepted local history
```

Exact command names and grammar remain subject to CLI design and empirical
validation.

---

## 15. Collaboration principles

The workflow follows these principles:

1. Local work is possible without a remote.
2. Commit and publication are separate.
3. Semantic and physical state move together through `RepositoryRevision`.
4. Fetching shared history does not silently alter active work.
5. Divergence is normal and does not imply conflict.
6. Reconciliation preserves all accepted histories.
7. Conflicts require explicit resolution.
8. Git remains subordinate to KAT.
9. Named references are optional coordination aids, not mandatory history
   containers.

---

## Summary

KAT collaboration operates on complete software revisions.

```text
shared RepositoryRevision
        ↓
local workspace
        ↓
semantic and/or physical work
        ↓
local RepositoryRevision
        ↓
sync
        ↓
push or reconcile
```

Users do not independently coordinate semantic knowledge and Git history.

KAT owns the complete repository workflow while Git provides the physical
storage backend underneath it.