# Collaboration Model

## Purpose

The collaboration model defines how participants evolve a KAT software model without reducing collaboration to file-based merging.

Collaboration operates on semantic changes to authoritative software knowledge.

---

## Scope and v0.2 Boundary

KAT v0.2 implements only **local draft collaboration semantics**:
* One local draft session per repository.
* Explicit accepted base state $S_n$.
* Stale-base conflict detection at commit time.
* Atomic acceptance via CAS transition.
* No automatic multi-branch merge or rebase.

Distributed multi-participant collaboration, shared drafts, remote repository synchronization, semantic merge, and conflict reconciliation are future collaboration semantics.

---

## Current v0.2 Collaboration Model

### Accepted Repository State
Authoritative software knowledge is represented by the current accepted `SemanticState` $S_n$ and the latest accepted `ChangeRevision` head $C_n$. All participants reading the repository inspect this accepted state.

### Local Draft Session
A participant prepares changes within a local draft session ($S_{\text{working}}$):
* Initialized from an explicit accepted base state $S_n$.
* Maintains an ordered sequence of staged mutation operations.
* Accumulates candidate state $S_{\text{working}}$.
* Isolated from accepted history until committed or aborted.
* At most one local draft session exists per repository.

### Stale-Base Conflict Detection
If the accepted repository state advances ($S_n \to S_{n+1}$) while a participant is preparing a local draft based on $S_n$, the draft becomes stale.

KAT v0.2 does not attempt automatic semantic merging. Upon commit, stale-base detection rejects publication with an explicit conflict error, preserving repository consistency.

### Atomic Acceptance
Committing a draft is an atomic transition:
$$(S_n, C_n) \xrightarrow{\text{Commit Draft}} (S_{n+1}, C_{n+1})$$
If valid, the new canonical objects are written, the accepted head is updated, and the draft session is cleared. Unaccepted or rejected drafts leave accepted repository state unchanged.

---

## Collaboration Principles

### Semantic Conflict vs File Conflict
Collaboration in KAT is defined primarily over semantic knowledge changes, not file-level diffs:

```text
Artifact Conflict != Semantic Conflict
```

* Two developers may modify the same source file without introducing conflicting semantic knowledge.
* Conversely, two developers may modify separate source files while creating conflicting semantic requirements or design decisions.

### Causality and Dependencies
Causal relationships between changes are preserved through explicit dependencies (`ChangeRevision.dependencies`). If Change B addresses a Requirement introduced in Change A, Change B causally depends on Change A and cannot be accepted before Change A.

### Publication Order vs Causal Dependencies
KAT v0.2 preserves a single linear publication sequence of `ChangeRevision` objects through the accepted change head $C_n$. Causal dependencies recorded in `ChangeRevision.dependencies` capture semantic links and need not correspond one-to-one with publication sequence order.

---

## Future Distributed Collaboration

The following sections define conceptual models for future multi-participant distributed collaboration beyond KAT v0.2.

### Concurrent Proposals (Future Concept)
Two proposed changes are concurrent when developed independently from the same base state $S_0$ without a direct causal dependency between them.

```text
        Shared Base S0
       /              \
      v                v
 Proposal A        Proposal B
```

In future models, concurrent proposals may be evaluated as independent, compatible, order-dependent, or conflicting.

### Collaborative Reconciliation (Future Concept)
Collaborative reconciliation (semantic merge) is the process of evaluating concurrent proposals and determining how to combine their semantic operations into a new accepted state:

```text
Proposal A \
            +--> Collaborative Reconciliation --> Combined Accepted State S1
Proposal B /
```

This is distinct from Phase 15 `AccountArtifact` accountability reconciliation, which re-baselines artifact relationships in accepted history without mutating `SemanticState`.

### Semantic Conflict Categories (Future Concept)
Conflict categories are defined by [`docs/specification/change-model.md`](change-model.md); when applied to collaborative proposals, they include:

* **Write Conflict**: Concurrent proposals assign incompatible values to the same element property.
* **Lifecycle Conflict**: One proposal deprecates or supersedes an element while another proposal attempts to update or link to it.
* **Dependency Conflict**: One proposal removes knowledge required by another proposal's causal dependency.
* **Invariant Conflict**: Proposals are individually valid, but their combined state violates a Level 1 domain invariant or ontology rule.

### Conflict Resolution (Future Concept)
Resolving a semantic conflict involves producing a valid, accepted outcome through explicit intent, such as:
* Selecting one proposed value.
* Creating a new `Design Decision` element that reconciles competing intent.
* Modifying or withdrawing a proposal.

Resolution must yield a semantically valid state before publication.

---

## Artifact Collaboration Boundary

Artifact modifications may occur during local work, but physical file overlap does not determine semantic collaboration conflicts.

Artifact accountability is tracked semantically through `kat.core/represents`, `kat.core/derived-from`, and `AccountArtifact` operations. Physical file merging, line-level diff resolution, and VCS branch coordination remain external to KAT v0.2.

---

## Current v0.2 Guarantees

KAT v0.2 guarantees the following collaboration properties:

* **Specification-First Authority**: Authoritative knowledge remains defined by the accepted semantic model.
* **Single Draft Isolation**: At most one local draft session exists per repository, isolated from accepted state.
* **Accepted Read Stability**: Standard queries inspect accepted state $S_n$ without exposure to transient draft operations.
* **Stale-Base Protection**: Attempts to commit against an outdated base state are rejected.
* **Atomic Publication**: Accepted state and change head update together as one atomic transition.
* **No Unverified Merges**: Invalid or conflicting operations are never persisted into accepted history.

---

## Current v0.2 Core Flow

```text
                  Accepted State Sn
                         |
                         v
             Local Draft Session (S_working)
                         |
            +------------+------------+
            | stage mutation ops      |
            v                         v
   Base State Unchanged     Base State Advanced
            |                         |
            v                         v
   Validate Candidate       Stale-Base Rejected
            |                         |
            v                         v
   Accepted State S_{n+1}    Accepted State Sn (Unchanged)
```

---

## Future Research Questions

The following topics are intentionally outside KAT v0.2 and represent future collaboration research:

* What protocol will govern remote repository fetch/push and proposed change exchange?
* Can KAT persist unaccepted, conflicted proposed change graphs for collaborative review?
* How will participant identities and cryptographic signatures be attached to change revisions?
* How will semantic merge tools visualize and resolve structural conflicts interactively?
* What guarantees are required for offline distributed repository synchronization?
