# KAT v0.5 Conflict Model

## Purpose

This document defines how KAT represents conflicts produced during
reconciliation.

A conflict is a first-class repository condition.

It is not merely a CLI error and it is not equivalent to divergence,
difference, staleness, impact, or repository-health findings.

This document defines:

- semantic conflicts;
- materialization conflicts;
- conflict identity;
- conflict inputs and alternatives;
- affected semantic and physical identities;
- conflict lifecycle;
- conflict preservation;
- explicit conflict resolution;
- resolution provenance;
- the relationship between conflicts, reconciliation, and repository history.

This document does not define:

- reconciliation algorithms;
- physical merge implementation details;
- Git-specific conflict formats;
- remote synchronization;
- KAT Hub protocols;
- CLI syntax;
- repository policy.

---

## 1. Motivation

Concurrent repository evolution may produce states that cannot be automatically
reconciled.

For example:

```text
Base R0

Alice:
    Update Requirement R1 -> R1a

Bob:
    Update Requirement R1 -> R1b
```

or:

```text
Alice:
    modify src/auth/service.rs

Bob:
    modify the same physical region incompatibly
```

KAT must not:

```text
choose Alice automatically
choose Bob automatically
choose the most recent revision
choose the remote revision
choose the local revision
```

Instead, the competing alternatives must remain preserved until an explicit
resolution is produced.

A conflict therefore represents unresolved concurrent evolution.

---

## 2. Core definition

A conflict exists when concurrent effects cannot both be preserved in one valid
resulting repository state without explicit resolution.

Conceptually:

```text
Conflict
    common base
    competing alternatives
    affected identities
    reason automatic reconciliation failed
```

Conflict does not mean that one alternative is invalid in isolation.

Both alternatives may be individually valid.

The conflict arises from their attempted composition.

---

## 3. Conflict is not divergence

The following concepts remain distinct:

```text
Divergence
    multiple repository histories exist

Difference
    repository states are not identical

Conflict
    concurrent effects cannot both be preserved automatically

Consequence
    reconciliation succeeds but repository health changes
```

For example:

```text
        R0
       /  \
      RA  RB
```

is divergent.

It is not a conflict unless reconciliation of `RA` and `RB` discovers
incompatible effects.

---

## 4. Conflict domains

KAT distinguishes at least two conflict domains:

```text
SemanticConflict
MaterializationConflict
```

These domains are independent.

A reconciliation may contain:

```text
semantic conflicts only
materialization conflicts only
both
neither
```

A clean result in one domain does not imply a clean result in the other.

---

# Semantic conflicts

## 5. SemanticConflict

A `SemanticConflict` represents incompatible concurrent semantic effects.

Conceptually:

```text
SemanticConflict {
    conflict_id
    base
    alternatives[]
    affected_elements[]
    affected_relationships[]
    related_operations[]
    kind
    diagnostic
}
```

This is a conceptual structure.

The exact canonical representation is defined only after the model is frozen.

---

## 6. Semantic conflict identity

A semantic conflict must have stable identity within the reconciliation state
that contains it.

Conflict identity must not depend only on presentation text.

The identity should derive from deterministic conflict inputs such as:

- common semantic base;
- competing repository revisions;
- affected semantic identities;
- competing operations;
- conflict kind.

The exact ObjectId or canonical representation is defined separately.

---

## 7. Common semantic base

Every semantic conflict is evaluated relative to a known semantic base when one
exists.

For example:

```text
S0
├── Change A -> SA
└── Change B -> SB
```

A conflict records that both alternatives originated from `S0`.

The base is necessary to explain:

- what both contributors observed;
- what each changed;
- why the effects no longer compose.

---

## 8. Semantic alternatives

A semantic conflict preserves every competing semantic alternative involved in
the conflict.

For example:

```text
Base:
    R1v1

Alternative A:
    R1v2a

Alternative B:
    R1v2b
```

Neither alternative is discarded before resolution.

The conflict must remain inspectable even if one alternative is eventually
selected.

---

## 9. Affected semantic identities

A semantic conflict identifies the stable semantic identities directly involved.

For example:

```text
affected_elements:
    R1
```

or:

```text
affected_elements:
    DD1
    DD2
    DD3
```

A conflict may also identify affected Relationship identities where the conflict
concerns graph structure.

This allows conflict inspection to operate on semantic identities rather than
serialized state fragments.

---

## 10. Related operations

When explicit semantic operations are available, a semantic conflict records or
references the operations that produced the competing effects.

For example:

```text
Alternative A:
    UpdateElement R1

Alternative B:
    DeprecateElement R1
```

This is more informative than storing only two resulting element versions.

KAT can then explain:

```text
one history attempts to update R1
while another deprecates R1
```

rather than only:

```text
R1 differs
```

---

## 11. Semantic conflict kinds

The initial model should distinguish conflict kinds by semantic cause.

The exact enumeration may evolve, but the following categories are useful.

### 11.1 Concurrent version conflict

Concurrent Changes produce incompatible versions of the same stable semantic
identity.

Example:

```text
A:
    Update R1 -> R1a

B:
    Update R1 -> R1b
```

Initial v0.5 behavior may conservatively require explicit resolution.

---

### 11.2 Lifecycle conflict

Concurrent Changes apply incompatible lifecycle effects.

Example:

```text
A:
    Deprecate R1

B:
    Update R1
```

Another example:

```text
A:
    Deprecate I1

B:
    create new relationships requiring active I1
```

---

### 11.3 Supersession conflict

Concurrent Changes produce incompatible supersession claims.

Example:

```text
A:
    DD1 superseded by DD2

B:
    DD1 superseded by DD3
```

Whether both may coexist depends on KAT supersession semantics.

If one coherent result cannot preserve both intended effects, the interaction is
a semantic conflict.

---

### 11.4 Relationship conflict

Concurrent Changes produce incompatible graph effects.

Example:

```text
A:
    Link relationship X

B:
    Unlink relationship X
```

or concurrent operations produce relationships that violate semantic or
lifecycle constraints when combined.

---

### 11.5 Combined validity conflict

Each Change is valid independently, but their composition violates semantic
validity.

Example:

```text
apply(S0, A) -> valid
apply(S0, B) -> valid
apply(S0, A + B) -> invalid
```

The resulting conflict arises from the combined state rather than direct
same-element modification.

---

### 11.6 Dependency conflict

A Change may depend on semantic assumptions that another Change invalidates in a
way that prevents preserving both intended effects.

This is stronger than ordinary impact.

If the dependency change only requires review, validation, or re-accounting, it
is not a blocking conflict.

It becomes a conflict only when both intended effects cannot coexist.

---

## 12. Same-element change is not sufficient

Concurrent modification of the same stable semantic identity is a conflict
candidate, not a universal definition of conflict.

Future reconciliation strategies may determine that some same-element effects
commute or can be combined deterministically.

The conflict model therefore defines incompatibility by effects, not by
identity overlap alone.

The initial implementation may remain conservative.

---

# Materialization conflicts

## 13. MaterializationConflict

A `MaterializationConflict` represents incompatible concurrent physical effects.

Conceptually:

```text
MaterializationConflict {
    conflict_id
    base_materialization
    alternatives[]
    affected_locators[]
    kind
    diagnostic
}
```

The exact representation is backend-neutral.

Git may detect the physical conflict, but Git conflict structures do not define
the KAT domain model.

---

## 14. Physical alternatives

A materialization conflict preserves:

```text
base
left alternative
right alternative
```

where applicable.

For a file conflict:

```text
base:
    Blob B0

left:
    Blob BA

right:
    Blob BB
```

The alternatives remain available until resolution.

No alternative is silently overwritten.

---

## 15. Affected physical locations

A materialization conflict identifies the project location or physical set
whose concurrent modifications could not be composed.

Examples:

```text
src/auth/service.rs
```

```text
src/auth/
```

The physical locator representation should remain compatible with the
workspace/materialization model.

---

## 16. Materialization conflict kinds

Possible initial kinds include:

### 16.1 Content conflict

Concurrent modifications to the same physical content cannot be automatically
merged.

---

### 16.2 Delete/modify conflict

One history deletes physical content while another modifies it.

---

### 16.3 Rename or move conflict

Concurrent physical changes move or rename the same materialization in
incompatible ways.

---

### 16.4 Tree conflict

Concurrent physical changes cannot produce one unambiguous project tree.

The physical backend may expose more detailed conflict information.

KAT may preserve that detail below the domain-level conflict kind.

---

## 17. Git does not define KAT conflict semantics

When Git is the physical backend, Git may detect and help resolve physical
conflicts.

For example:

```text
Git merge machinery
        |
        v
physical conflict information
        |
        v
MaterializationConflict
```

KAT wraps the backend result into its own reconciliation state.

Users should not need to interpret Git index stages or Git-specific conflict
metadata to understand a KAT conflict.

---

# Conflict lifecycle

## 18. Conflict creation

Conflicts are produced by reconciliation.

Conceptually:

```text
ReconciliationCandidate
    semantic_conflicts[]
    materialization_conflicts[]
```

A conflict belongs to a specific reconciliation context.

It is not an independent mutation of accepted repository history.

---

## 19. Unresolved conflict

A conflict begins unresolved.

An unresolved conflict preserves:

- common base;
- competing alternatives;
- affected identities or materializations;
- related operations when available;
- conflict kind;
- diagnostic context.

An unresolved blocking conflict prevents creation of a final accepted
reconciliation revision.

---

## 20. Conflict inspection

Conflict state must be inspectable before resolution.

A contributor must be able to determine:

```text
what changed
which histories produced the alternatives
which semantic or physical identities are affected
why automatic reconciliation failed
what alternatives are currently preserved
```

The exact CLI projection is defined later.

---

## 21. Explicit resolution

A conflict can be resolved only through an explicit decision that produces a
coherent resulting effect.

A resolution may:

```text
select one alternative
combine alternatives
replace both with a new result
remove an effect
introduce additional semantic operations
introduce new physical content
```

Resolution must not happen implicitly because of command execution order.

---

## 22. Semantic conflict resolution

Semantic conflict resolution should produce explicit semantic evolution.

For example:

```text
R1v2a
R1v2b
        |
        v
resolution
        |
        v
R1v3
```

The semantic operations required to produce `R1v3` become part of the
reconciliation Change where appropriate.

Conceptually:

```text
SemanticConflict
        |
        v
Resolution
        |
        v
semantic operations
        |
        v
reconciled SemanticState
```

---

## 23. Materialization conflict resolution

A materialization conflict resolution produces resolved physical content.

For example:

```text
base B0
left BA
right BB
      |
      v
manual or backend-assisted resolution
      |
      v
resolved BC
```

The resulting `WorkspaceSnapshot` contains `BC`.

The original alternatives remain historically reachable through the parent
repository revisions.

---

## 24. Conflict resolution provenance

KAT must preserve enough information to explain how a conflict was resolved.

Conceptually, resolution provenance may include:

```text
conflict identity
chosen/synthesized resolution
resulting semantic operations
resulting physical materialization
reconciliation revision
```

The exact metadata representation is defined separately.

Resolution provenance must not require rewriting the input histories.

---

## 25. Resolved conflicts remain historical knowledge

Resolving a conflict does not erase the conflict.

Suppose:

```text
        R0
       /  \
      RA  RB
       \  /
        RC
```

and `RA` and `RB` produced competing versions.

After `RC` resolves them, history still shows:

```text
RA existed
RB existed
their effects conflicted
RC contains the resolution
```

This follows KAT's immutable knowledge-preservation model.

---

## 26. Conflict state is not accepted software state

An unresolved reconciliation candidate is not itself an accepted
`RepositoryRevision`.

Conceptually:

```text
RA + RB
   |
   v
ReconciliationCandidate
   |
   +-- resolved effects
   +-- unresolved conflicts
```

Only after all blocking conflicts are resolved can KAT produce:

```text
RC
```

as an accepted repository revision.

---

# Conflict versus consequences

## 27. Staleness is not conflict

Artifact staleness does not automatically represent a conflict.

Example:

```text
A:
    Update Implementation I1

B:
    no change to Artifact A1
```

After reconciliation:

```text
A1 semantic accountability = STALE
```

The semantic effects may still coexist perfectly.

This is a repository-health consequence.

---

## 28. Physical Artifact drift is not conflict

If an Artifact's represented physical materialization changes while its semantic
baseline remains unchanged:

```text
semantic:
    CURRENT

physical:
    MODIFIED
```

this is accountability state.

It is not automatically a reconciliation conflict.

---

## 29. Impact is not conflict

A concurrent Change may affect knowledge relied upon by another history.

For example:

```text
A:
    Update Requirement R1

B:
    work involving Implementation I1
```

If both effects can coexist, reconciliation may succeed and report impact.

Impact becomes conflict only if preserving both intended effects is impossible
without explicit resolution.

---

## 30. Validation findings are not conflict

A reconciled result may require validation to be rerun or may contain evidence
gaps.

These findings do not become semantic conflicts merely because collaboration
caused them.

---

## 31. GraphQuality findings are not conflict

GraphQuality remains advisory according to the existing KAT model.

A reconciliation that introduces a GraphQuality finding may still be a valid
reconciliation.

Repository policy may later make selected findings publication-blocking, but
that is distinct from conflict semantics.

---

# Conflict and repository policy

## 32. Conflict is objective, policy is governance

KAT must distinguish:

```text
Conflict
    concurrent effects cannot coexist without explicit resolution

Policy violation
    project rules do not permit publication of an otherwise representable state
```

For example:

```text
Artifact is stale
```

may be:

```text
not a conflict
```

while repository policy may say:

```text
publication forbidden while stale Artifacts exist
```

The conflict model does not define project governance.

---

# Conflict preservation

## 33. No last-writer-wins

KAT must never automatically resolve a conflict using:

- newest timestamp;
- latest publication;
- local preference;
- remote preference;
- contributor identity;
- arbitrary input ordering.

Automatic reconciliation must be based on deterministic semantic or physical
composition rules.

When those rules cannot preserve all effects, the conflict remains unresolved.

---

## 34. Input order does not determine the winner

Given:

```text
reconcile(RA, RB)
```

and:

```text
reconcile(RB, RA)
```

the ordering of equivalent reconciliation inputs must not cause one history to
silently dominate the other.

Presentation order may differ.

Conflict semantics must remain equivalent.

---

## 35. Alternatives remain reachable

All accepted repository revisions participating in a conflict remain reachable
through repository history.

Conflict resolution produces descendants.

It does not mutate or delete input revisions.

---

# Conflict interaction with ChangeRevision

## 36. Semantic resolution operations

When resolution requires semantic modification, those modifications should be
represented through the existing semantic operation model.

For example:

```text
Conflict:
    Update R1 -> R1a
    Update R1 -> R1b

Resolution:
    produce R1c
```

should result in explicit semantic evolution rather than direct mutation of a
`SemanticState`.

The exact operation sequence depends on the final reconciliation
`ChangeRevision` semantics.

---

## 37. Conflict objects are not Knowledge Elements

Conflict objects belong to the collaboration/reconciliation layer.

They are not automatically Knowledge Elements in the software ontology.

A conflict describes competing repository evolution.

It is not itself part of the software's authoritative domain knowledge.

---

## 38. Resolution may create new knowledge

Although the conflict itself is not a Knowledge Element, resolving it may
require creating or updating semantic knowledge.

For example, two conflicting Design Decisions may be resolved by:

```text
creating a new Design Decision
superseding both alternatives
```

Such evolution occurs through normal KAT semantic operations.

---

# Initial conflict scenarios

## 39. CONF-01: Same-element concurrent update

Base:

```text
R1v1
```

A:

```text
Update R1 -> R1v2a
```

B:

```text
Update R1 -> R1v2b
```

Initial expected behavior:

```text
SemanticConflict
kind = ConcurrentVersion
affected = R1
```

Both alternatives remain available.

---

## 40. CONF-02: Update versus deprecation

A:

```text
Update R1
```

B:

```text
Deprecate R1
```

Expected:

```text
SemanticConflict
kind = Lifecycle
```

unless future semantic rules establish a deterministic valid composition.

---

## 41. CONF-03: Competing supersession

A:

```text
Supersede DD1 with DD2
```

B:

```text
Supersede DD1 with DD3
```

Expected:

```text
SemanticConflict
kind = Supersession
```

if both supersession claims cannot be preserved under the ontology and lifecycle
rules.

---

## 42. CONF-04: Combined validity failure

A:

```text
valid independently
```

B:

```text
valid independently
```

Combined:

```text
invalid semantic state
```

Expected:

```text
SemanticConflict
kind = CombinedValidity
```

The conflict diagnostic should identify the violated rule or invariant.

---

## 43. CONF-05: Physical content conflict

Base:

```text
src/service.rs = B0
```

A:

```text
src/service.rs = BA
```

B:

```text
src/service.rs = BB
```

Physical backend cannot automatically merge the changes.

Expected:

```text
MaterializationConflict
kind = Content
```

with:

```text
base = B0
left = BA
right = BB
```

---

## 44. CONF-06: Delete versus modify

A:

```text
delete src/service.rs
```

B:

```text
modify src/service.rs
```

Expected:

```text
MaterializationConflict
kind = DeleteModify
```

---

## 45. CONF-07: Semantic conflict with clean materialization

A and B produce incompatible semantic effects but modify different files.

Expected:

```text
semantic:
    conflict

materialization:
    clean

reconciliation:
    blocked
```

---

## 46. CONF-08: Materialization conflict with clean semantics

A and B produce independent semantic effects but incompatible physical edits.

Expected:

```text
semantic:
    clean

materialization:
    conflict

reconciliation:
    blocked
```

---

## 47. CONF-09: Reconciliation consequence without conflict

A:

```text
Update Implementation I1
```

B:

```text
Artifact A1 unchanged
```

Combined state is mechanically valid.

Expected:

```text
reconciliation:
    succeeds

Artifact:
    semantic accountability = STALE
```

No `SemanticConflict` is created.

---

## 48. CONF-10: Explicit resolution

Given:

```text
SemanticConflict
    R1v2a
    R1v2b
```

Contributor resolves by producing:

```text
R1v3
```

Expected:

```text
conflict resolved explicitly
R1v2a historically reachable
R1v2b historically reachable
R1v3 present in reconciled state
resolution provenance preserved
```

---

## 49. Determinism

Conflict detection must be deterministic.

Given identical:

- common base;
- input repository revisions;
- semantic operations;
- semantic states;
- workspace snapshots;
- ontology;
- repository invariants;
- physical backend merge result;

KAT must detect the same conflicts.

Human resolution is intentionally not deterministic.

The chosen human resolution must instead be represented explicitly.

---

## 50. Non-goals

This model does not define:

- exact canonical conflict encoding;
- exact conflict ObjectId derivation;
- exact reconciliation candidate storage;
- Git index conflict representation;
- text conflict-marker syntax;
- automatic semantic property-level merging;
- user conflict-resolution commands;
- interactive merge tools;
- remote conflict synchronization;
- review workflows;
- repository policy.

Those concerns build on this model.

---

## Summary

Conflicts are first-class unresolved collaboration state.

```text
          R0
         /  \
        /    \
      RA      RB
        \    /
         \  /
   ReconciliationCandidate
        |
        +-- resolved semantic effects
        +-- resolved physical effects
        +-- SemanticConflict[]
        +-- MaterializationConflict[]
```

A conflict exists only when concurrent effects cannot both be preserved in one
valid result without explicit resolution.

KAT distinguishes:

```text
semantic conflict
materialization conflict
repository-health consequence
```

No conflict is resolved through silent last-writer-wins behavior.

Competing alternatives remain preserved until an explicit resolution is
recorded.

After resolution:

```text
RA ----\
        \
         RC
        /
RB ----/
```

the input histories and competing alternatives remain historically reachable.

Conflict resolution therefore becomes part of the explainable evolution of the
software repository rather than an erased implementation detail.