# Repository Model

## Purpose

The repository model defines the conceptual boundary of a software system managed by KAT.

A KAT repository contains the knowledge, rules, history, and references required to represent and evolve a software system according to the KAT model.

The repository provides a boundary for:

* Authoritative software knowledge
* Accepted semantic state and history
* Active ontology and domain constraints
* Local draft transaction state
* Artifact accountability state

The repository model does not define filesystem layout, database structure, network protocols, serialization formats, or storage technology.

---

## Repository Boundary

A KAT Repository is the unit through which KAT manages the knowledge and evolution of a software system.

Conceptually:

```text
KAT Repository
        |
        +--> Software System
        |
        +--> Accepted Repository State
        |       |-- SemanticState (Sn)
        |       +-- ChangeRevision Head (Cn)
        |
        +--> Persistent Repository Knowledge
        |       |-- Active Ontology (OntologyVersion)
        |       +-- Immutable Canonical Object Store
        |
        +--> Local Working State (Draft Session)
        |
        +--> Derived State (Indexes & Accountability Reports)
```

The repository is not the software itself.

It is the KAT-managed environment in which the software's authoritative knowledge, evolution, and relationships are maintained.

---

## Software System Boundary

A repository manages one logical software system.

The Software entity represents the system whose knowledge is maintained by the repository.

```text
KAT Repository
        |
        | manages
        v
Software System
```

The physical boundaries of the software system are not determined by KAT. A software system may be realized as a monolith, microservices, libraries, configuration files, or infrastructure definitions. All these elements belong to the same repository when they form part of the same logical software system.

---

## Repository Identity

A repository has a stable `RepositoryId` (UUIDv4) independent from its physical location on disk or in version control.

A managed software system has a stable `SoftwareId` (UUIDv4).

Moving the repository, copying its storage, or changing its filesystem path does not by itself redefine the software knowledge represented by it.

Canonical identity metadata is recorded by repository layout metadata.

---

## Authoritative Repository State

A repository maintains an accepted semantic state $S_n$ representing the currently authoritative software knowledge.

It also maintains an accepted change head $C_n$ identifying the latest accepted `ChangeRevision` associated with that state.

Conceptually:

```text
Repository
    |
    v
Accepted Repository State
    |
    +--> Accepted Semantic State (Sn)
    +--> Accepted Change Head (Cn)
```

For the initial repository state after initialization:

```text
Accepted Repository State (S0)
    |
    +--> SemanticState S0
    +--> Change Head: none
```

After accepting Change Revision $C_1$:

```text
Accepted Repository State (S1)
    |
    +--> SemanticState S1
    +--> Change Head C1
```

The accepted semantic state $S_n$ must satisfy:
* The active `OntologyVersion`
* Level 1 domain invariants
* Accepted changes and their postconditions

Working or draft changes do not redefine either the authoritative semantic state or accepted history until successfully accepted.

---

## Accepted State Invariant

The core structural invariant governing accepted repository state is:

```text
if accepted.change != none:
    accepted.change.result_state == accepted.state
```

For fresh initialization ($S_0$), `accepted.change == none`.

For any subsequent accepted transition $n \ge 1$:
* `accepted.change` is a valid canonical `ChangeRevision` $C_n$.
* `accepted.change.result_state` matches `accepted.state` ($S_n$).

The accepted state reference and accepted change head are published together as one atomic repository-level transition. The accepted `SemanticState` reference may remain unchanged ($S_{n+1} = S_n$) when the accepted Change has no net state effect (such as `AccountArtifact`).

---

## Local Draft State

A repository may contain at most **one local unaccepted draft Change** session at any time.

The local draft:
* is initialized from an explicit accepted state ($S_n$);
* maintains an ordered sequence of staged mutation operations;
* accumulates a candidate working semantic state $S_{\text{working}}$;
* is strictly local and not part of accepted history;
* may be committed (atomically accepted) or aborted (discarded);
* becomes stale if the repository's accepted state advances concurrently.

```text
Accepted Repository State (Sn)
      |
      +--> Local Draft Session (S_working)
             |-- Staged Op 1 (CreateElement)
             |-- Staged Op 2 (Link)
             +-- Staged Op 3 (AccountArtifact)
```

Incomplete or staged draft operations do not redefine authoritative state ($S_n$) until committed.

---

## Atomic Acceptance

Publishing a draft Change into accepted repository history is an atomic transition:

$$ (S_n, C_n) \xrightarrow{\text{Commit Draft}} (S_{n+1}, C_{n+1}) $$

If the accepted state $S_n$ changed during draft preparation, acceptance is rejected with a conflict error.

Acceptance guarantees:
1. **Atomic Publication**: Either both the accepted state reference and accepted change head update to $(S_{n+1}, C_{n+1})$ or neither does.
2. **Conflict Rejection**: A stale base state rejects acceptance.
3. **State Preservation**: Accepted repository state remains unchanged on failure.

---

## Core Invariants vs Domain Constraints

A repository maintains the invariants required for its accepted semantic states.

### Core Invariants
Structural and semantic rules defined by KAT specification and mechanically enforced across all repositories:
* Stable knowledge identity (`ElementId`, `RelationshipId`)
* Domain relationship validity and type compatibility
* Immutable historical traceability
* Authority of accepted semantic state over source files

### Domain Constraints
Semantic restrictions expressed as `kat.core/constraint` knowledge elements within the software model (e.g. *"Payment data must not be stored unencrypted"*). Whether KAT can mechanically verify a constraint depends on whether executable validation semantics are available.

---

## Change History & Publication Order

A repository preserves the semantic evolution of software knowledge through an immutable accepted sequence of `ChangeRevision` objects.

```text
SemanticState S0
        |
        | Change C1 (base: S0, result: S1)
        v
SemanticState S1
        |
        | Change C2 (base: S1, result: S2)
        v
SemanticState S2
```

Accepted revisions have a single linear publication order through the accepted change head $C_n$.

Causal dependencies recorded in `ChangeRevision.dependencies` reflect semantic causal links between changes and need not correspond one-to-one with linear publication order.

KAT v0.2 does not implement branching, merge, or multi-head accepted history.

---

## Repository State Layering

The state of a KAT repository is categorized into four distinct functional layers:

1. **Accepted Repository State** (Authoritative):
   - Current `SemanticState` reference ($S_n$)
   - Current `ChangeRevision` head ($C_n$)

2. **Persistent Repository Knowledge** (Immutable Storage):
   - Active `OntologyVersion`
   - Immutable canonical object store (containing element versions, relationship versions, change revisions, ontology versions)

3. **Local Working State** (Transient):
   - Local draft transaction session

4. **Derived State** (Computed Read-Side Views):
   - Identity lookup indexes
   - Trace origin paths and impact propagation graphs
   - Artifact accountability reports (`CURRENT`, `STALE`, `UNACCOUNTED`)

---

## Artifact Knowledge Representation

Artifacts associated with the software are represented as semantic knowledge elements of type `kat.core/artifact`.

```text
Artifact Element (kat.core/artifact)
    |
    |-- kat.core/represents   --> Implementation Element
    +-- kat.core/derived-from --> Requirement / Constraint / Decision / Implementation
```

An artifact element's identity (`ElementId` UUID) is independent of its physical filesystem path or locator. Moving or renaming a source file does not by itself redefine its semantic knowledge identity.

Physical artifact location or external resource addressing is not defined by the core repository model.

---

## Artifact Accountability State

Artifact accountability is a derived query state evaluated against the current accepted semantic state $S_n$ and accepted change history.

Accountability is derived from:
* Currently accepted direct `kat.core/represents` and `kat.core/derived-from` relationships.
* Resolved accountability reconciliation baselines (from initial relationship acceptance or subsequent `AccountArtifact` operations).
* Current target element version selected by $S_n$.

Artifact accountability status is categorized as:
* `CURRENT`: All direct accountability baselines match current target element versions.
* `STALE`: At least one direct accountability baseline differs from the current target element version, or a target element's lifecycle state has become invalid (`Deprecated` or `Superseded`).
* `UNACCOUNTED`: No direct accountability relationship exists for the artifact in $S_n$.

This state does not imply physical artifact inspection or verification.

---

## Collaboration Boundary

KAT v0.2 supports local draft transaction sessions and concurrent stale-draft detection.

Distributed synchronization, shared draft state, multi-repository remote fetch/push, branch merge, and conflict reconciliation are outside the current repository model and are defined as future collaboration concerns in [`docs/specification/collaboration-model.md`](collaboration-model.md).

---

## Repository Validity

A repository is semantically valid when its accepted state satisfies all required structural and domain rules:

1. The accepted `SemanticState` $S_n$ exists in store and is structurally valid.
2. The active `OntologyVersion` exists in store and is valid.
3. `accepted.change == none` only for the freshly initialized repository before any Change has been accepted.
4. If `accepted.change` is not `none`, `accepted.change.result_state == accepted.state`.
5. All element version and relationship version objects selected by $S_n$ exist in the object store and have expected kinds.
6. The accepted state $S_n$ conforms to the active ontology and Level 1 domain invariants.

Artifact accountability staleness does not make the authoritative semantic state invalid. Physical artifact divergence is outside KAT v0.2 and is not inferred from accountability status.

---

## Physical Storage Boundary

The conceptual repository boundary is distinct from the physical storage boundary:

```text
Conceptual KAT Repository
        |-- Semantic knowledge
        |-- Accepted history
        |-- Active ontology
        +-- Artifact accountability baselines

Physical Storage Boundary
        |-- Repository layout metadata
        |-- Immutable canonical object store
        |-- Local draft storage
        +-- Managed software source files & build outputs
```

`Artifact semantic identity (ElementId UUID) != physical filesystem path`. KAT does not require managed artifacts to physically reside within the internal repository storage directory.

---

## Version Control Boundary

A KAT repository is not defined by file-based version control (such as Git).

External version control systems may transport, store, or collaborate on source code files, but file-level commits do not replace or modify the semantic history maintained by KAT `ChangeRevision` objects.

```text
KAT Repository             External Version Control
  |                           |
  +-- Semantic evolution      +-- File line modifications
  +-- Knowledge history       +-- Blob tree commits
  +-- System traceability     +-- Workspace branches
```

---

## Repository Lifecycle

A KAT repository moves through defined lifecycle activities:

1. **Initialization**: Establishing `RepositoryId`, `SoftwareId`, active `OntologyVersion`, initial state $S_0$, and setting `accepted.change = none`.
2. **Draft Evolution**: Opening a draft session, staging mutation operations on $S_{\text{working}}$, and evaluating candidate consistency.
3. **Atomic Acceptance**: Validating candidate state $S_{\text{working}}$, persisting canonical objects, atomically publishing $(S_{n+1}, C_{n+1})$, and cleaning up local draft state.
4. **Validation**: Evaluating mechanical consistency and identifying unverified constraints on accepted state $S_n$.
5. **Artifact Accountability Analysis**: Evaluating direct accountability baselines against current target element versions to report `CURRENT`, `STALE`, or `UNACCOUNTED` status.

---

## Core Rules

The repository model enforces the following normative rules:

* A repository manages exactly one logical software system.
* The repository is not the software itself.
* The accepted semantic state $S_n$ is authoritative.
* A repository has at most one accepted `SemanticState` reference ($S_n$).
* A repository has at most one accepted `ChangeRevision` head ($C_n$).
* A repository has at most one local unaccepted draft session.
* A local draft session never changes accepted state until successful atomic acceptance.
* Accepted publication is an atomic, repository-level transition.
* Artifact location does not define artifact identity (`ElementId` UUID != file path).
* Artifact accountability staleness does not invalidate accepted semantic state.
* Repository identity (`RepositoryId` UUID) is independent of physical filesystem location.

---

## Future Research Questions

The following topics are intentionally outside KAT v0.2 and represent future research areas:

* Could future repository models support federated cross-repository knowledge references?
* How will ontology extensions be packaged, published, and versioned across repositories?
* How will distributed repositories synchronize accepted change revision graphs?
* What protocol will define remote artifact metadata bridges for build systems and CI/CD pipelines?
