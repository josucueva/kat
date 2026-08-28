# KAT v0.5 Reference Model

## Purpose

This document defines how KAT identifies and refers to accepted
`RepositoryRevision`s during local and remote collaboration.

Repository history is immutable.

References are movable names or selectors over that immutable history.

This document defines:

- repository heads;
- named references;
- remote references;
- revision selectors;
- reference movement;
- workspace relationship to references.

It does not define Git refs, CLI syntax, remote transport, or repository policy.

---

## 1. Immutable history and movable references

A `RepositoryRevision` is immutable.

References do not contain history.

They only identify revisions within that history.

Conceptually:

```text
RepositoryRevision DAG
        |
        +-- R42
        +-- R43
        +-- R44

NamedReference
    main -> R44
```

Moving `main` does not mutate `R42`, `R43`, or `R44`.

---

## 2. Head

A head is a visible accepted `RepositoryRevision` with no visible accepted
successor in the current repository view.

For example:

```text
R0 -> R1 -> R2
```

has:

```text
head = R2
```

Divergent history may have multiple heads:

```text
        R0
       /  \
      RA  RB
```

with:

```text
heads = [RA, RB]
```

Multiple heads are valid and represent divergence, not conflict.

---

## 3. Repository view

Heads are evaluated relative to a repository view.

A local repository may know:

```text
local heads
remote-observed heads
```

without treating them as the same mutable state.

For example:

```text
local:
    RA

origin:
    RB
```

If `RA` and `RB` diverge, both remain visible.

---

## 4. NamedReference

A `NamedReference` is an optional human-readable name pointing to one accepted
`RepositoryRevision`.

Conceptually:

```text
NamedReference {
    name
    target
}
```

Example:

```text
main       -> R42
release    -> R37
experiment -> RA
```

A named reference is mutable.

Its target revision is immutable.

---

## 5. References do not own history

Repository revisions remain valid independently of named references.

For example:

```text
R40 -> R41 -> R42
```

may exist even if no persistent name points to `R41`.

Deleting or moving a reference does not rewrite repository ancestry.

This avoids branch-centric repository semantics.

---

## 6. Workspace and reference are distinct

A workspace is based on an explicit `RepositoryRevision`.

It is not inherently "on" a named reference.

For example:

```text
Workspace A
    base = R42

main
    -> R42
```

If `main` later moves to:

```text
main -> R45
```

the workspace remains based on `R42`.

Reference movement must not silently move active work.

---

## 7. Reference movement

Moving a named reference changes only its target.

For example:

```text
before:
    main -> R42

after:
    main -> R43
```

This is allowed when repository collaboration rules permit the update.

Reference movement must not mutate repository revisions.

Concurrent reference updates must not silently overwrite one another.

---

## 8. Remote references

A remote may expose named references and repository heads.

Conceptually:

```text
origin/main -> R42
origin heads:
    R42
    RX
```

Remote references represent the last known state observed from that remote.

They do not automatically change local references or workspace bases.

---

## 9. Local and remote references are separate

A local named reference and a remote-observed reference may point to different
revisions.

For example:

```text
main        -> R43
origin/main -> R42
```

This may indicate local accepted work not yet published.

The reverse:

```text
main        -> R42
origin/main -> R43
```

may indicate shared history has advanced.

KAT must preserve the distinction rather than silently synchronizing the two.

---

## 10. Reference update safety

Mutable shared references should use optimistic update semantics.

Conceptually:

```text
expected:
    main -> R42

candidate:
    main -> R43
```

The update succeeds only if the shared reference still points to `R42`.

If another contributor moved it first, KAT must not silently overwrite the new
target.

The accepted revisions remain preserved even if the reference update fails.

---

## 11. Unnamed heads

A repository head does not require a persistent human-readable name.

For example:

```text
        R0
       /  \
      RA  RB
```

may expose:

```text
main -> RA

unnamed head:
    RB
```

This allows KAT to preserve concurrent accepted history without forcing every
revision into a branch-like namespace.

A name may be attached later if useful.

---

## 12. Revision selectors

Users and internal workflows need deterministic ways to identify
`RepositoryRevision`s.

The model should support at least:

```text
full RepositoryRevisionId
unique RepositoryRevisionId prefix
named reference
workspace-relative selector
```

The exact selector grammar is defined separately.

---

## 13. Full revision identity

A full `RepositoryRevisionId` is the canonical revision reference.

It is immutable and globally unambiguous within the repository identity model.

Persistent systems requiring stable reference should retain the full revision
identity.

---

## 14. Prefix reference

A revision may be selected using a unique prefix of its immutable identifier.

The existing KAT reference principles should apply:

```text
minimum prefix length
uniqueness within the relevant repository view
fail if ambiguous
```

Prefix references are convenience selectors.

They are not persistent identity.

---

## 15. Named references are not identity

A named reference such as:

```text
main
```

may move.

Therefore:

```text
reference name != RepositoryRevision identity
```

A historical record that requires an exact revision must store the immutable
`RepositoryRevisionId`, not only the current name.

---

## 16. Workspace-relative references

KAT may provide temporary selectors relative to the current workspace.

Examples could include concepts such as:

```text
current workspace base
local head
parent revision
```

These are navigation conveniences.

They do not create persistent repository identity.

The exact selector vocabulary is deferred to CLI design.

---

## 17. Reference conflicts are not semantic conflicts

Two contributors may concurrently attempt to move the same named reference.

For example:

```text
Alice:
    main -> RA

Bob:
    main -> RB
```

This is a mutable-reference update conflict.

It does not imply that `RA` and `RB` have a semantic or materialization
conflict.

The repository histories may simply have diverged.

Reconciliation semantics remain separate.

---

## 18. Git boundary

KAT references are repository-level concepts.

They are not defined by Git branches or Git refs.

A Git-backed implementation may maintain internal Git refs for storage,
transport, or interoperability.

Those backend refs must not define KAT reference semantics.

Conceptually:

```text
KAT:
    main -> RepositoryRevision R42

R42:
    workspace_snapshot -> W42

Git backend:
    W42 -> Git physical snapshot
```

KAT does not define:

```text
main -> Git branch
```

as repository authority.

---

## 19. Initial v0.5 reference scope

The initial model requires only:

```text
RepositoryRevisionId
Head
NamedReference
RemoteReference
unique revision prefix
```

More advanced reference features are deferred.

These include:

```text
branch namespaces
revision expression languages
automatic reference tracking
release channels
reference aliases
cross-repository references
```

---

## Summary

KAT repository history is an immutable `RepositoryRevision` DAG.

```text
RepositoryRevision history
        |
        +-- heads
        |
        +-- optional named references
```

A head is an accepted visible tip of repository history.

A named reference is a movable human-readable pointer to one immutable
`RepositoryRevision`.

Workspaces are based on revisions, not inherently on named branches.

Local and remote references remain distinct until explicitly updated.

Concurrent reference movement never silently discards accepted repository
history.

Git refs may implement backend storage or interoperability, but KAT references
remain repository-level concepts.