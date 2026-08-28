# KAT v0.5 Reconciliation Model

## Purpose

This document defines how KAT reconciles divergent `RepositoryRevision`s into
a new coherent repository revision.

Reconciliation operates on complete software revisions.

It therefore considers two distinct but coordinated dimensions:

- semantic evolution;
- physical/materialization evolution.

This document defines:

- divergence;
- common ancestry;
- reconciliation inputs;
- semantic reconciliation;
- materialization reconciliation;
- automatic composition;
- reconciliation candidates;
- reconciliation outcomes;
- multi-parent repository revisions;
- reconciliation Changes;
- advisory consequences;
- history preservation.

This document does not define:

- detailed conflict object schemas;
- conflict resolution UX;
- Git merge implementation details;
- remote synchronization;
- KAT Hub protocols;
- CLI syntax;
- repository policy enforcement.

---

## 1. Motivation

Multiple contributors may evolve the same KAT repository independently.

For example:

```text
        R0
       /  \
      RA  RB
```

`RA` and `RB` may both be valid accepted `RepositoryRevision`s derived from
the same base.

Their coexistence is divergence.

It is not automatically a conflict.

KAT must determine whether the effects represented by both histories can be
preserved in one coherent successor revision.

That process is reconciliation.

Conceptually:

```text
        R0
       /  \
      RA  RB
       \  /
        RC
```

where:

```text
RC.parents = [RA, RB]
```

and `RC` preserves the accepted effects of both histories whenever possible.

---

## 2. Core definition

Reconciliation is the process of attempting to construct one valid
`RepositoryRevision` from two or more divergent repository histories while
preserving their intended semantic and physical effects.

Conceptually:

```text
reconcile(base, left, right)
    -> ReconciliationResult
```

A successful reconciliation produces:

```text
RepositoryRevision RC
    parents = [RA, RB]
    semantic_state = SC
    workspace_snapshot = WC
```

where `SC` and `WC` represent the reconciled semantic and physical results.

---

## 3. Divergence

Repository revisions diverge when they descend from a common ancestor through
different accepted histories.

For example:

```text
        R0
       /  \
      RA  RB
```

Divergence means:

```text
RA != RB
```

and neither revision is an ancestor of the other.

Divergence alone does not imply:

- semantic conflict;
- materialization conflict;
- invalid state;
- repository corruption.

It only means that multiple accepted histories currently exist.

---

## 4. Common ancestry

Reconciliation is evaluated relative to shared ancestry.

For two divergent revisions:

```text
RA
RB
```

KAT determines a common ancestor:

```text
R0
```

Conceptually:

```text
base  = R0
left  = RA
right = RB
```

The common ancestor establishes the state from which both histories evolved.

It provides:

```text
base semantic state
    = R0.semantic_state

base physical snapshot
    = R0.workspace_snapshot
```

Reconciliation must not treat related histories as unrelated snapshots when
their ancestry is available.

---

## 5. Reconciliation inputs

A reconciliation attempt operates on complete repository revisions.

Conceptually:

```text
ReconciliationInput {
    base
    inputs[]
}
```

For the initial v0.5 model:

```text
base
left
right
```

is sufficient.

Each input provides:

```text
RepositoryRevision
├── ancestry
├── SemanticState
├── WorkspaceSnapshot
└── associated semantic Change history
```

The reconciliation algorithm may use both resulting states and the explicit
Changes that produced them.

---

## 6. Reconciliation is not snapshot merging alone

KAT must not reduce reconciliation to:

```text
diff SemanticState A
diff SemanticState B
merge serialized values
```

When explicit semantic history exists, reconciliation should consider:

- semantic operations;
- stable identities;
- immutable versions;
- lifecycle transitions;
- relationships;
- Change dependencies;
- ontology and invariants.

Similarly, physical reconciliation may use the physical backend's knowledge of
common ancestry and physical changes.

The resulting repository revision must coordinate both dimensions.

---

## 7. Semantic reconciliation

Semantic reconciliation attempts to preserve the semantic effects of all input
histories in one valid resulting `SemanticState`.

Conceptually:

```text
S0
├── Change A -> SA
└── Change B -> SB

reconcile_semantic(S0, A, B)
    -> SC
```

Where possible, KAT composes the semantic operations represented by both
histories.

A successful semantic reconciliation requires that the resulting semantic
state is mechanically valid.

---

## 8. Operation-based reconciliation

Explicit `ChangeRevision`s provide the preferred basis for semantic
reconciliation.

Conceptually:

```text
Change A
    operations = [A1, A2, ...]

Change B
    operations = [B1, B2, ...]
```

KAT evaluates whether those concurrent effects can be composed against their
common semantic base.

This preserves information that would be lost by comparing final states only.

Examples include:

- which stable identity was intentionally updated;
- whether an element was deprecated;
- whether a relationship was explicitly linked or unlinked;
- whether one Design Decision superseded another;
- which semantic dependencies were assumed.

---

## 9. Independent semantic effects

Concurrent semantic effects should reconcile automatically when they are
independent and valid in combination.

Example:

```text
Base S0

Change A
    Update Requirement R1

Change B
    Update Implementation I2
```

where:

```text
R1 != I2
```

If neither change invalidates assumptions required by the other, both effects
may be composed.

The resulting state contains both updates.

---

## 10. Commutativity as a strong automatic-reconciliation signal

For concurrent Changes `A` and `B`, a strong signal of independence is:

```text
apply(apply(S0, A), B)
    ==
apply(apply(S0, B), A)
```

provided both application orders are valid.

If both orders:

- succeed;
- produce the same canonical semantic state;

then the Changes are strong candidates for deterministic automatic
reconciliation.

This criterion is not the only possible reconciliation rule, but it provides
a useful correctness foundation for clearly independent operations.

---

## 11. Composition must be revalidated

Individual validity does not imply combined validity.

For example:

```text
apply(S0, A) -> valid
apply(S0, B) -> valid
```

does not guarantee:

```text
apply(S0, A + B) -> valid
```

KAT must validate the composed semantic result against:

- structural rules;
- ontology constraints;
- lifecycle rules;
- repository invariants.

If the combined result is mechanically invalid, reconciliation is not clean.

---

## 12. Same-identity modification

Concurrent modifications of the same stable identity are not automatically
conflicts.

For example:

```text
A:
    Update R1

B:
    Update R1
```

This is a conflict candidate.

The reconciliation model must determine whether both intended effects can be
preserved.

For the initial v0.5 implementation, KAT may conservatively classify concurrent
same-element updates as requiring explicit resolution rather than attempting
property-level semantic merging.

More advanced semantic merge strategies may be added later without changing
the repository model.

---

## 13. Lifecycle interactions

Some concurrent operations interact through element lifecycle rather than
through direct value modification.

Examples include:

```text
A:
    Deprecate R1

B:
    Update R1
```

or:

```text
A:
    Supersede DD1 with DD2

B:
    Supersede DD1 with DD3
```

These interactions must be evaluated semantically.

A clean physical merge does not make them semantically compatible.

---

## 14. Relationship interactions

Concurrent relationship operations may be independent or incompatible.

For example:

```text
A:
    Link I1 realizes R1

B:
    Link V1 validates R1
```

may compose cleanly.

Other combinations may interact, for example:

```text
A:
    Link I1 realizes R1

B:
    Unlink the same relationship
```

The reconciliation model evaluates effects over stable semantic identities and
relationship identities, not only serialized graph differences.

---

## 15. Dependency-aware reconciliation

A Change may have been authored under semantic assumptions that changed in
another concurrent history.

For example:

```text
Change A
    updates Requirement R1

Change B
    was authored while depending on R1v1
```

After `A`:

```text
R1v1 -> R1v2
```

`B` may still be mechanically applicable.

This does not automatically create a conflict.

Instead, KAT may determine that the reconciled result has semantic
consequences requiring review.

Dependency changes therefore participate in reconciliation analysis without
being treated as conflicts by default.

---

## 16. Materialization reconciliation

Physical/materialization reconciliation operates independently from semantic
reconciliation.

Conceptually:

```text
W0
├── physical evolution A -> WA
└── physical evolution B -> WB

reconcile_materialization(W0, WA, WB)
    -> WC
```

The physical backend may provide:

- physical diffing;
- ancestry;
- file-tree comparison;
- three-way merge;
- physical conflict detection.

The resulting `WorkspaceSnapshot WC` must represent one coherent physical
project state.

---

## 17. Semantic and materialization results are independent

Possible combinations include:

### Semantic clean, physical clean

```text
semantic:
    clean

materialization:
    clean
```

A reconciled repository revision may be produced.

### Semantic conflict, physical clean

```text
semantic:
    unresolved

materialization:
    clean
```

The reconciliation is not publishable.

### Semantic clean, physical conflict

```text
semantic:
    clean

materialization:
    unresolved
```

The reconciliation is not publishable.

### Semantic conflict, physical conflict

```text
semantic:
    unresolved

materialization:
    unresolved
```

Both domains require resolution.

Neither domain may infer correctness from the other.

---

## 18. Reconciliation candidate

A reconciliation attempt may produce useful state before it is fully
publishable.

Conceptually:

```text
ReconciliationCandidate {
    base
    inputs[]
    semantic_result
    materialization_result
    semantic_conflicts[]
    materialization_conflicts[]
    findings[]
}
```

This is a conceptual model.

The exact object representation is defined later.

A candidate may contain:

- automatically reconciled semantic effects;
- automatically merged physical content;
- unresolved conflicts;
- repository-health consequences.

This allows reconciliation to be inspectable rather than reduced to a single
success or failure result.

---

## 19. Reconciliation outcomes

A reconciliation attempt has three important outcome classes.

### 19.1 Clean reconciliation

Both semantic and materialization effects reconcile successfully.

```text
semantic:
    resolved

physical:
    resolved

mechanical validation:
    valid
```

A successor `RepositoryRevision` may be produced.

---

### 19.2 Reconciliation with consequences

Both semantic and materialization state reconcile mechanically, but the result
contains repository-health consequences.

Examples include:

- stale Artifacts;
- physical Artifact drift;
- evidence gaps;
- validations requiring review;
- GraphQuality findings;
- increased semantic impact.

These findings do not themselves make the reconciliation a conflict.

Conceptually:

```text
reconciliation:
    resolved

repository health:
    findings present
```

Publication policy is defined separately.

---

### 19.3 Blocked reconciliation

At least one semantic or materialization effect cannot be preserved without
explicit resolution.

Conceptually:

```text
semantic conflicts > 0
or
materialization conflicts > 0
```

No final accepted reconciled revision may be produced until blocking conflicts
are resolved.

---

## 20. Conflict boundary

A conflict exists when concurrent effects cannot both be preserved in one valid
result without explicit resolution.

Difference alone is insufficient.

Examples that are not automatically conflicts:

- different elements changed;
- one Requirement changed while an affected Artifact remained unchanged;
- a valid reconciliation causes an Artifact to become stale;
- validation evidence becomes outdated;
- GraphQuality findings appear.

These are differences or consequences.

Detailed conflict representation is defined in `conflict-model.md`.

---

## 21. Advisory consequences

After semantic and materialization reconciliation succeeds, KAT evaluates the
resulting repository state.

The result may contain findings such as:

```text
Artifact semantic status:
    STALE

Artifact materialization status:
    MODIFIED

Validation:
    review required

GraphQuality:
    findings present
```

These findings belong to accountability and repository health.

They remain distinct from reconciliation conflicts.

---

## 22. History preservation

Reconciliation must preserve all input histories.

Given:

```text
        R0
       /  \
      RA  RB
```

successful reconciliation produces:

```text
        R0
       /  \
      RA  RB
       \  /
        RC
```

where:

```text
RC.parents = [RA, RB]
```

KAT must not rewrite the history into:

```text
R0 -> RA -> RC
```

while silently discarding `RB`.

Likewise, semantic alternatives that required explicit resolution remain
historically reachable.

---

## 23. Reconciliation does not erase alternatives

Suppose concurrent history produced:

```text
R1v2a
R1v2b
```

and reconciliation resolves them by producing:

```text
R1v3
```

The resulting history must preserve that:

```text
R1v2a existed
R1v2b existed
R1v3 resolved their divergence
```

The chosen or synthesized resolution does not erase the competing knowledge.

---

## 24. Reconciliation revision

A successful reconciliation produces a new multi-parent
`RepositoryRevision`.

Conceptually:

```text
RC {
    parents = [RA, RB]
    semantic_state = SC
    workspace_snapshot = WC
    semantic_change = CR | none
}
```

`SC` is the reconciled semantic state.

`WC` is the reconciled physical workspace snapshot.

`CR`, when present, explains the semantic evolution introduced specifically by
the reconciliation.

---

## 25. Relationship to ChangeRevision

A reconciliation `RepositoryRevision` and a reconciliation
`ChangeRevision` serve different purposes.

The repository revision records:

```text
which complete histories were joined
which semantic state resulted
which physical state resulted
```

The Change records:

```text
which explicit semantic operations were required
to produce the reconciled semantic result
```

This distinction matters because some semantic effects may already exist in the
input histories and require no new operation.

---

## 26. Reconciliation Change semantics

A reconciliation Change should not pretend that one concurrent history is the
sole base and the other is merely replayed afterward.

Conceptually, reconciliation is multi-base evolution.

The existing plural base-state capability in `ChangeRevision` is compatible
with this direction.

For example:

```text
ChangeRevision CR
    base_states = [SA, SB]
    operations = resolution/reconciliation operations
    result_state = SC
```

The exact canonical semantics are defined only after reconciliation behavior is
validated.

---

## 27. No-op semantic reconciliation

A reconciliation may require no additional semantic operations.

For example:

```text
RA:
    Create Requirement R1

RB:
    Create Design Decision D1
```

If both semantic histories compose automatically, the reconciled semantic state
may be derivable directly from the two histories.

The reconciliation `RepositoryRevision` still has two parents.

Whether such reconciliation requires an explicit zero-operation
`ChangeRevision` or `semantic_change = none` is left open until the
`ChangeRevision` integration is finalized.

---

## 28. Physical-only divergence

Two histories may diverge only physically.

For example:

```text
RA
    semantic = S1
    physical = WA

RB
    semantic = S1
    physical = WB
```

Semantic reconciliation is trivial:

```text
SC = S1
```

Materialization reconciliation still determines:

```text
WA + WB -> WC
```

The resulting revision remains a multi-parent reconciliation revision.

---

## 29. Semantic-only divergence

Two histories may diverge only semantically.

For example:

```text
RA
    semantic = SA
    physical = W1

RB
    semantic = SB
    physical = W1
```

Materialization reconciliation is trivial:

```text
WC = W1
```

Semantic reconciliation determines:

```text
SA + SB -> SC
```

The resulting revision remains a multi-parent reconciliation revision.

---

## 30. Combined divergence

Both semantic and physical dimensions may diverge:

```text
RA
├── SA
└── WA

RB
├── SB
└── WB
```

Reconciliation must coordinate:

```text
semantic:
    SA + SB -> SC

physical:
    WA + WB -> WC
```

before producing:

```text
RC
├── SC
└── WC
```

This is the general collaboration case.

---

## 31. Artifact accountability after reconciliation

Artifact accountability is evaluated against the reconciled semantic and
physical state.

For example, reconciliation may produce:

```text
Implementation I1:
    I1v3 -> I1v4

Artifact A1 physical content:
    unchanged
```

The resulting Artifact may be:

```text
semantic:
    STALE

physical:
    CURRENT
```

Alternatively:

```text
Implementation:
    unchanged

Artifact physical content:
    changed
```

may produce:

```text
semantic:
    CURRENT

physical:
    MODIFIED
```

These are reconciliation consequences, not necessarily conflicts.

---

## 32. Determinism

Automatic reconciliation must be deterministic.

Given identical:

- common ancestry;
- input repository revisions;
- semantic histories;
- physical backend state;
- ontology and structural rules;

KAT must produce the same automatic reconciliation result.

Unresolved human choices must be represented explicitly rather than hidden
inside nondeterministic reconciliation behavior.

---

## 33. Reconciliation and repository validity

A final reconciled `RepositoryRevision` must satisfy normal repository revision
validity requirements.

At minimum:

- all parents exist;
- reconciled `SemanticState` is structurally valid;
- reconciled `WorkspaceSnapshot` is valid;
- referenced semantic Change data is consistent;
- no blocking reconciliation conflict remains.

Repository-health findings may still exist according to existing KAT semantics.

---

## 34. Initial automatic reconciliation scope

The first implementation should prefer conservative, explainable
reconciliation.

Strong initial candidates for automatic semantic composition include:

```text
Create distinct element A
+
Create distinct element B
```

```text
Update element A
+
Update different element B
```

```text
independent Link operations
```

```text
Account Artifact A
+
unrelated semantic Update B
```

provided the composed result remains mechanically valid.

---

## 35. Initial conservative conflict scope

The first implementation may conservatively require explicit resolution for
cases such as:

```text
Update R1
+
Update R1
```

```text
Deprecate R1
+
Update R1
```

```text
Supersede DD1 with DD2
+
Supersede DD1 with DD3
```

or any composition whose combined semantic state violates repository
invariants.

This conservative scope may later be refined without changing the fundamental
reconciliation model.

---

## 36. Reconciliation evaluation scenarios

The reconciliation model should initially be tested against at least the
following scenarios.

### REC-01: Independent creation

```text
Base:
    S0

A:
    Create R1

B:
    Create DD1
```

Expected:

```text
automatic semantic reconciliation
```

---

### REC-02: Independent updates

```text
A:
    Update R1

B:
    Update I1
```

where the updates are semantically independent.

Expected:

```text
automatic semantic reconciliation
```

---

### REC-03: Same-element concurrent update

```text
A:
    Update R1 -> R1a

B:
    Update R1 -> R1b
```

Expected initial behavior:

```text
semantic conflict candidate
explicit resolution required
```

---

### REC-04: Lifecycle interaction

```text
A:
    Deprecate R1

B:
    Update R1
```

Expected:

```text
semantic conflict
```

---

### REC-05: Independent relationships

```text
A:
    Link I1 realizes R1

B:
    Link V1 validates R1
```

Expected:

```text
automatic semantic reconciliation
```

---

### REC-06: Combined invariant failure

```text
A alone:
    valid

B alone:
    valid

A + B:
    invalid
```

Expected:

```text
blocked semantic reconciliation
```

---

### REC-07: Clean reconciliation with consequences

```text
semantic reconciliation:
    valid

materialization reconciliation:
    valid

result:
    Artifact becomes stale
```

Expected:

```text
reconciliation succeeds
Artifact staleness reported as consequence
```

---

### REC-08: Physical-only divergence

```text
semantic:
    unchanged on both sides

physical:
    independently changed
```

Expected:

```text
semantic reconciliation trivial
physical reconciliation determines outcome
```

---

### REC-09: Semantic-only divergence

```text
physical:
    unchanged

semantic:
    independently changed
```

Expected:

```text
physical reconciliation trivial
semantic reconciliation determines outcome
```

---

## 37. Non-goals

This model does not define:

- detailed `SemanticConflict` representation;
- detailed `MaterializationConflict` representation;
- user conflict-resolution commands;
- text merge algorithms;
- Git merge plumbing;
- remote head synchronization;
- remote repository protocols;
- pull-request/review workflows;
- access control;
- policy enforcement;
- KAT Hub UI;
- branch or bookmark UX.

Those concerns build on this reconciliation model.

---

## Summary

Reconciliation joins divergent complete software histories.

```text
              R0
             /  \
            /    \
          RA      RB
           \      /
            \    /
              RC
```

Each input revision contains both:

```text
SemanticState
WorkspaceSnapshot
```

KAT reconciles those dimensions independently:

```text
semantic histories
        ↓
semantic reconciliation
        ↓
SC

physical histories
        ↓
materialization reconciliation
        ↓
WC
```

A successful result binds them again:

```text
RC
├── semantic_state = SC
├── workspace_snapshot = WC
└── parents = [RA, RB]
```

Divergence is not conflict.

Conflict exists only when concurrent effects cannot both be preserved without
explicit resolution.

Successful reconciliation may still produce Artifact accountability,
validation, impact, or GraphQuality consequences.

No accepted history or competing knowledge is silently discarded.