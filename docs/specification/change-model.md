# Change Model

## Purpose

The change model defines how KAT represents the evolution of software knowledge.

KAT follows a specification-first model. Changes to intent, requirements, constraints, decisions, and their relationships define the authoritative evolution of the software system.

Artifacts such as source code, tests, documentation, configuration, API descriptions, and deployment definitions may change as a consequence of this evolution, but they do not replace the semantic model as the source of truth.

The change model defines:

* What constitutes a change
* How changes affect semantic state and history
* How changes propagate to dependent knowledge
* How artifact effects relate to authoritative changes
* How changes depend on or conflict with other changes

---

## Authoritative Model

The semantic model is authoritative.

Intent, requirements, constraints, decisions, and their relationships define the intended state of the software.

Artifacts represent, implement, validate, or derive from that state, but they do not independently redefine it.

Software evolution is represented primarily as changes to authoritative knowledge.

---

## Change

A Change is a meaningful unit of semantic evolution.

An accepted Change contains one or more ordered mutation operations that together express one coherent evolution of software knowledge. A draft Change may temporarily contain zero operations before its first staged mutation.

A Change may modify:
* Intent
* Requirements
* Constraints
* Design decisions
* Relationships between knowledge elements
* Artifact accountability baselines

A Change may exist locally as mutable draft work before acceptance. Once accepted, its historical record is immutable.

---

## ChangeRevision

A `ChangeRevision` is the immutable canonical representation of an accepted Change.

It records:
* the ordered mutation operations;
* its base semantic state references (v0.2 normal publication uses one accepted base state; multiple bases are reserved for future composition/merge semantics);
* its resulting semantic state reference;
* its optional human-readable description;
* its causal `ChangeRevision` dependencies.

---

## Change Identity

A logical Change may have a stable `ChangeId` (UUID).

An accepted `ChangeRevision` has an immutable content identity (`ObjectId`) derived from its canonical representation.

The logical identity of a Change is distinct from the content identity of any particular `ChangeRevision`.

---

## Operations

An operation is the smallest semantic mutation recognized by KAT.

Canonical mutation operations include:
* `CreateElement`
* `UpdateElement`
* `DeprecateElement`
* `Link`
* `Unlink`
* `SupersedeElement`
* `AccountArtifact`

Mutation operations contribute semantic effects to a Change. They may modify the candidate `SemanticState`, record accepted semantic history, or both.

Operational contracts, preconditions, and postconditions are defined in [`docs/operations.md`](operations.md).

---

## Operation Ordering

Operations within a Change are strictly ordered.

Each operation is evaluated against the candidate working state $S_{\text{working}}$ produced by all preceding successful operations in the same Change.

The operation sequence is preserved in the accepted `ChangeRevision`.

---

## Semantic State

A `SemanticState` is the immutable, authoritative representation of the currently accepted software knowledge at a particular point in its evolution.

It contains the accepted knowledge element versions, relationship versions, and `OntologyVersion` reference selected by that state, including elements whose lifecycle may be `Active`, `Deprecated`, or `Superseded`.

Every accepted Change produces a new `ChangeRevision`. A Change may produce a content-distinct resulting `SemanticState`, or it may record semantic evolution whose resulting state is content-identical to its base state.

```text
Semantic State A
        |
        | Change (produces ChangeRevision)
        v
Semantic State B (where State B may equal State A)
```

---

## Accepted State

The repository's accepted state identifies:
* the currently accepted `SemanticState`;
* the latest accepted `ChangeRevision`, when one exists.

When a latest accepted `ChangeRevision` exists, its `result_state` reference must identify the currently accepted `SemanticState`:

```text
accepted.change.result_state == accepted.state
```

Initialization is the only accepted state with no associated `ChangeRevision`.

---

## SemanticState Identity vs ChangeRevision Identity

`SemanticState` identity and `ChangeRevision` identity are independent.

A Change may produce a result state whose content differs from its base state:

```text
result_state != base_state
```

A Change may also record accepted semantic evolution while producing a result state content-identical to its base state:

```text
result_state == base_state
```

In both cases, acceptance creates a distinct immutable `ChangeRevision`.

`AccountArtifact` is one example of an operation that records explicit semantic acknowledgment of accountability baselines without modifying the candidate `SemanticState`. A multi-operation Change may also produce net-zero state effects even when individual operations modify the working candidate during evaluation.

---

## Draft Changes & Candidate Staging

A Change may exist locally as an unaccepted draft.

A draft:
* has an explicit accepted base state;
* accumulates ordered operations;
* maintains a working candidate state $S_{\text{working}}$;
* is not part of accepted history;
* may be committed or aborted.

In a multi-operation Change, operations are staged sequentially against $S_{\text{working}}$. Staging does not modify the accepted semantic state or accepted history. Later operations observe the candidate state produced by all successfully staged operations that precede them.

If the accepted repository advances away from the draft's base state, the draft becomes stale and cannot be accepted without an explicit future reconciliation mechanism. KAT v0.2 does not automatically rebase or merge stale drafts.

---

## Atomicity & Failure Semantics

A Change is accepted atomically. Either all of its operations become part of one accepted `ChangeRevision`, or none of them become accepted.

Failure of a staged operation does not affect previously staged operations in a draft session and does not modify accepted state.

Failure to accept a complete Change leaves accepted state unchanged. KAT does not partially accept a Change. A failure during candidate validation preserves the local draft session for correction. A publication failure due to an accepted-state CAS conflict marks the local draft as stale without altering accepted history.

A local draft Change becomes accepted history only after successful whole-candidate validation and atomic accepted-state publication.

---

## Preconditions and Postconditions

Operations define their own preconditions and postconditions in [`docs/operations.md`](operations.md).

A Change is valid only when its ordered operations can be applied sequentially against candidate state $S_{\text{working}}$ and the resulting candidate satisfies all required model invariants.

---

## Change Propagation & Effects

A change may affect other knowledge elements through their semantic relationships.

KAT distinguishes between:
* The element directly changed
* Knowledge elements semantically affected by the change
* Artifacts related to affected knowledge

### Semantic Effects

Semantic effects are consequences that affect related software knowledge (e.g. a requirement update requiring design decision review).

### Validation Effects

Validation effects identify validation knowledge or consistency obligations that may require reevaluation.

### Artifact Effects

Artifact effects occur when related physical artifacts (code, documentation, tests) require review as a consequence of authoritative semantic changes. Artifact effects do not independently redefine the semantic state.

---

## Artifact Accountability

KAT tracks artifact accountability through currently accepted direct `kat.core/represents` and `kat.core/derived-from` relationships.

An artifact accountability status is:
* `CURRENT`: Every direct accountability baseline matches the current target element version.
* `STALE`: At least one resolved direct accountability baseline differs from the current target element version.
* `UNACCOUNTED`: No direct accountability relationship exists for the artifact in the current state.

These statuses indicate baseline alignment in accepted history and do not imply physical inspection or verification of disk files by KAT.

---

## Physical Artifact Divergence

Physical divergence occurs when an artifact's actual file contents no longer agree with the intended software state expressed in the semantic model.

KAT v0.2 does not automatically detect or parse physical artifact divergence. Detecting and reconciling physical artifact divergence remains outside the scope of this version.

---

## Causality

An accepted `ChangeRevision` may depend on one or more earlier accepted `ChangeRevisions` when its semantic meaning requires knowledge introduced or modified by them.

Semantic dependency is distinct from repository publication order. Two Changes may be semantically independent even though their accepted revisions have a defined historical order in a repository.

---

## Compatibility and Conflict

Two changes are compatible when they can both be incorporated into a semantic state without violating preconditions, producing contradictory modifications, or violating semantic invariants.

A conflict occurs when two or more changes cannot be incorporated into the same semantic state without violating their intended semantics or model rules.

KAT defines semantic conflict concepts independently from any merge or conflict resolution mechanism. Automatic reconciliation remains outside KAT v0.2 scope. These categories define semantic concepts; v0.2 does not claim complete automatic detection of every conflict category.

Conflict types include:
* **Write Conflict**: Incompatible values assigned to the same property.
* **Lifecycle Conflict**: Lifecycle modification invalidates another operation.
* **Dependency Conflict**: Modification invalidates an element or relationship required by another change.
* **Invariant Conflict**: Combined result violates a model invariant (e.g. creating duplicate semantic relationship triples).

---

## History & Reversal

Changes form the immutable history of authoritative software knowledge.

History records:
* What changed
* Why it changed, when that rationale is recorded
* Which semantic operations were applied
* Which knowledge was affected
* Which previous state or changes it depended on

Reversing a change does not erase the original change from history. A new authoritative change counteracts or supersedes its effects, preserving historical traceability.

---

## Open Questions

The following questions remain intentionally unresolved:

* Should effects be explicitly declared or derived?
* How are changes compared for semantic equivalence?
* Can an accepted ChangeRevision have multiple base states in future merge scenarios?
* How are concurrent changes reconciled across distributed repositories?
* How is physical artifact divergence detected automatically?
* How is an implementation-originated difference converted into an authoritative change?
