# KAT v0.5 Collaboration Invariants

## COLL-01: Complete software revision

Every accepted `RepositoryRevision` identifies exactly one
`SemanticState` and one `WorkspaceSnapshot`.

A `RepositoryRevision` represents one coherent version of the software.

---

## COLL-02: Independent evolution dimensions

Semantic state and physical workspace state evolve independently.

A repository revision may therefore be:

- semantic-only;
- physical-only;
- semantic and physical.

Changing one dimension does not implicitly change the other.

---

## COLL-03: RepositoryRevision is repository authority

The active state of a KAT repository is defined by its active
`RepositoryRevision`.

Neither the physical backend state nor a `SemanticState` independently
defines the complete active software revision.

---

## COLL-04: Stable identity is independent from version

Knowledge Elements and Relationships retain stable identities while
their immutable versions evolve independently.

Related Requirements, Implementations, Artifacts, Validations, and
physical contents are not required to advance versions together.

---

## COLL-05: Physical tracking is distinct from semantic modeling

The versioned physical workspace and the semantic Artifact model are
different concerns.

A physically tracked file does not automatically become an Artifact
Knowledge Element.

An Artifact provides explicit semantic meaning and accountability over
physical project content.

---

## COLL-06: Work has an explicit base

Every collaborative workspace and Change is based on an explicit
`RepositoryRevision`.

The base revision determines the semantic and physical state against
which the work was produced.

---

## COLL-07: Divergence is valid

Multiple descendant `RepositoryRevision`s may coexist from the same
base revision.

Divergence is not itself a conflict.

---

## COLL-08: Reconciliation preserves history

Reconciliation of divergent revisions MUST preserve every input history.

A reconciled revision is a descendant of every reconciled head.

No concurrent history or semantic alternative may be silently discarded.

---

## COLL-09: Reconciliation is semantic and physical

Semantic reconciliation and physical/materialization reconciliation are
distinct processes.

A clean result in one domain does not imply a clean result in the other.

A publishable reconciled `RepositoryRevision` must bind the resolved
semantic and physical results.

---

## COLL-10: Conflict means incompatible concurrent effects

A conflict exists only when concurrent effects cannot both be preserved
in one valid resulting repository revision without explicit resolution.

Concurrent difference or modification of the same entity is not
automatically a conflict.

KAT MUST distinguish at least:

- semantic conflicts;
- materialization conflicts.

---

## COLL-11: No silent conflict resolution

KAT MUST NOT resolve conflicts through implicit last-writer-wins or
silent replacement.

Competing alternatives remain reachable until an explicit resolution is
recorded.

Conflict resolution itself becomes explicit repository evolution.

---

## COLL-12: Conflict is distinct from consequence

A reconciliation may be mechanically valid while producing consequences
such as:

- stale Artifacts;
- evidence gaps;
- GraphQuality findings;
- validations requiring review;
- increased impact.

These are repository-health or accountability findings, not conflicts,
unless the concurrent effects themselves cannot coexist.

---

## COLL-13: Physical backend is subordinate to KAT

A physical version-control backend such as Git may store, transfer,
materialize, diff, and merge physical workspace snapshots.

Its branches, HEAD, refs, or other mutable state do not define KAT
repository authority.

KAT binds repository revisions to immutable physical snapshots.

---

## COLL-14: Collaboration never silently loses knowledge

Accepted semantic versions, competing alternatives, Changes, and
reconciliation decisions remain historically reachable according to
KAT's immutable history model.

Collaboration must preserve the knowledge of what changed, what
competed, and how the resulting state was produced.
