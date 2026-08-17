# Architecture

## Purpose

This document defines the logical architecture of KAT.

The architecture translates the conceptual model of KAT into a set of components, responsibilities, and boundaries.

It defines how KAT:

* Manages authoritative semantic state
* Preserves accepted semantic history
* Applies semantic changes via atomic publication
* Enforces active ontology rules and core invariants
* Supports traceability, impact analysis, and validation
* Persists immutable content-addressed semantic objects
* Supports artifact accountability
* Supports local draft evolution and stale-base protection
* Defines extension boundaries for future materialization and distributed collaboration
* Exposes its behavior through interfaces

The architecture does not define specific programming languages, filesystem layouts, databases, user interface technologies, or network protocols.

Physical implementation details are specified separately by the prototype design and canonical format specifications.

---

## Architectural Principles

The architecture follows the core principles of KAT.

### Specification-First Authority
The authoritative state of the software is represented by the semantic model. Artifacts may represent or derive from that knowledge, but they do not independently redefine it.

### Controlled Semantic Mutation
Authoritative semantic state may only change through the KAT Change process. No interface, plugin, persistence component, or tool may bypass the Change path to directly modify accepted semantic state.

### Immutable Persistence
Persisted canonical semantic objects and historical states are immutable. Evolution creates new objects and versions rather than modifying historical objects in place.

### Stable Semantic Identity
Semantic identity is independent from immutable object identity. A knowledge element retains its 36-character UUID identity (`ElementId`) across updates, deprecations, and supersessions.

### Content-Addressed Versions
Immutable canonical objects are identified by content-derived identities (`ObjectId` SHA-256 hashes). Stable semantic identities remain independent from content-derived Object IDs.

### Atomic Publication
A candidate semantic state becomes authoritative only after the complete Change has been applied and successfully validated. Acceptance advances the accepted repository state reference and accepted Change head together in one atomic transition.

### Derived Data Is Rebuildable
Indexes, projections, and query acceleration structures are derived from canonical repository data. Loss of derived data does not imply loss of authoritative knowledge or accepted history.

### Architecture Independence
Core KAT architecture does not depend on a particular software architecture, programming language, framework, or artifact structure.

---

## Architectural Overview

KAT is organized around a Semantic Repository containing immutable canonical objects, repository metadata, and mutable repository references.

Conceptually:

```text
                         Interfaces
                    CLI / API / Editors
                           |
                           v
                    Application Layer
                           |
        +------------------+------------------+
        |                  |                  |
        v                  v                  v
   Change Engine      Query Engine      Validation Engine
        |                  |                  |
        +------------------+------------------+
                           |
                           v
                  Semantic Repository
                           |
        +------------------+------------------+
        |                  |                  |
        v                  v                  v
  Ontology Service  Draft Session Service Canonical Persistence
                                              |
                                    +---------+---------+
                                    |                   |
                                    v                   v
                             Immutable Objects      Repository Refs
                                                        |
                                                        v
                                               accepted state / change
                                               local draft session

                           |
                           v
                     Derived Projections
```

---

## Interface Layer

The Interface Layer exposes KAT behavior to users and external tools (CLI, programmatic API, editor integrations).

Interfaces are responsible for receiving user intent, collecting operation inputs, presenting formatted results, and reporting validation or conflict errors. Interfaces must not contain authoritative semantic logic.

---

## Application Layer

The Application Layer coordinates KAT use cases by delegating requests to the appropriate core engine:

* Create, update, deprecate, or supersede knowledge
* Trace origin (`kat trace`)
* Analyze impact (`kat impact`)
* Validate consistency (`kat validate`)
* Inspect history (`kat history`)
* Review artifact accountability (`kat artifacts`)
* Reconcile artifact accountability baselines (`kat account`)
* Manage local draft Changes (`kat change begin/status/commit/abort`)
* Inspect active ontology and type capabilities (`kat ontology`, `kat ontology show`)

The Application Layer does not directly mutate persisted canonical objects or repository references.

---

## Semantic Repository

The Semantic Repository is the central logical boundary through which KAT accesses repository state.

It provides controlled access to:
* Accepted `SemanticState` ($S_n$)
* Accepted `ChangeRevision` head ($C_n$)
* Canonical objects (`KnowledgeElementVersion`, `RelationshipVersion`, `ChangeRevision`, `SemanticState`, `OntologyVersion`)
* Active `OntologyVersion`
* Local draft session state ($S_{\text{working}}$)
* Repository metadata (`RepositoryId`, `SoftwareId`, format version)

The Semantic Repository separates semantic behavior from physical persistence implementations.

---

## Accepted Repository State

The repository maintains two related accepted references:

```text
Accepted Repository State
    state  -> SemanticState ObjectId
    change -> ChangeRevision ObjectId | none
```

* **Accepted SemanticState**: Defines the authoritative software knowledge ($S_n$).
* **Accepted Change Head**: Identifies the latest accepted `ChangeRevision` ($C_n$).

For a freshly initialized repository before any accepted Change:
```text
state  -> S0
change -> none
```

After accepting Change Revision $C_1$:
```text
state  -> S1
change -> C1
```

The repository maintains the normative invariant:
```text
if accepted.change != none:
    accepted.change.result_state == accepted.state
```

The accepted Change head provides accepted-history reachability; it does not itself define intended software state.

---

## Change Engine

The Change Engine is the component responsible for producing candidate states and executing accepted transitions.

A Change may contain one or more ordered mutation operations (`CreateElement`, `UpdateElement`, `DeprecateElement`, `SupersedeElement`, `Link`, `Unlink`, `AccountArtifact`).

The Change Engine distinguishes:
* **State-Mutating Operations** (`CreateElement`, `UpdateElement`, `DeprecateElement`, `SupersedeElement`, `Link`, `Unlink`): Change the selected element or relationship mappings of the candidate state and may produce new element or relationship version objects.
* **History-Only Operations** (`AccountArtifact`): Record explicit reconciliation baselines in accepted `ChangeRevision` history without altering candidate state identity mappings at that step (and leaving $S_{\text{result}} = S_{\text{base}}$ for standalone/history-only Changes).

For staged multi-operation Changes, operations are evaluated sequentially against candidate state $S_{\text{working}}$:

```text
Accepted State Sn
      |
      v
Draft Change Session
      |-- Op 1 -> S1
      |-- Op 2 -> S2
      +-- Op 3 -> S_working
      |
      v
Validate Candidate S_working
      |
      v
Atomic Publication -> (S_{n+1}, C_{n+1})
```

If validation fails or the accepted base state changes concurrently, publication is rejected and accepted state remains unchanged.

---

## Atomic Publication

Publishing a draft Change into accepted history is an atomic repository-level transition:

$$ (S_n, C_n) \xrightarrow{\text{Commit Draft}} (S_{n+1}, C_{n+1}) $$

Guarantees:
1. **Atomic Transition**: The accepted state reference and accepted Change head are published together atomically. The `SemanticState` ObjectId may remain unchanged ($S_{n+1} = S_n$) for a history-only or net-zero Change (such as `AccountArtifact`).
2. **Conflict Rejection**: A stale base state rejects publication.
3. **State Preservation**: Accepted repository state remains unchanged on failure.

---

## Draft Session Service

The Draft Session Service manages local transaction staging:
* Enforces at most **one active local draft session** per repository.
* Binds the draft session to an explicit accepted base state ($S_n$).
* Maintains ordered staged operations and accumulates candidate state $S_{\text{working}}$.
* Validates candidate consistency prior to commit.
* Cleans up session state upon successful publication (`commit`) or explicit cancellation (`abort`).

Transient draft operations do not pollute the canonical object store or alter accepted history until successfully committed.

---

## Semantic Model Service

The Semantic Model Service provides logical access to semantic states.

It is responsible for:
* Loading `SemanticState` compositions.
* Resolving stable UUID identities (`ElementId`, `RelationshipId`) to selected version ObjectIDs.
* Reading knowledge element versions and relationship versions.
* Constructing candidate working states ($S_{\text{working}}$).

A `SemanticState` maps selected element and relationship identities to ObjectIDs, including `Deprecated` and `Superseded` element versions selected by that state.

---

## Canonical Object Model

The canonical repository store contains immutable content-addressed objects:

```text
Knowledge Element Version
Relationship Version
Ontology Version
Change Revision
Semantic State
```

### Knowledge Element Version
Represents one immutable version snapshot of a knowledge element, containing its stable `ElementId`, ontology `type_id`, `lifecycle` state (`Active`, `Deprecated`, `Superseded`), and properties map.

### Relationship Version
Represents one immutable version snapshot of a typed semantic relationship, containing its stable `RelationshipId`, `type_id`, `source_element_id`, `target_element_id`, and properties map.

### Change Revision
A `ChangeRevision` is an immutable canonical record of a Change revision. It contains `change_id`, `base_states` references, ordered semantic operations, `dependencies`, `result_state` reference, and description. Local draft Changes evolve in transient session state; canonical `ChangeRevision` objects are produced when a Change is materialized for attempted acceptance.

### Semantic State
Represents one immutable composition of software knowledge. In the canonical format payload, `SemanticState` contains:
* `repository_id`
* Selected `ElementId` $\to$ `KnowledgeElementVersion` `ObjectId` mappings
* Selected `RelationshipId` $\to$ `RelationshipVersion` `ObjectId` mappings

Ontology version selection is maintained as repository state context.

### Ontology Version
Represents one immutable version of the ontology defining element types, relationship types, and allowed source/target endpoint combinations for a state.

---

## Repository References

Repository refs are mutable references pointing to immutable canonical objects.

The accepted reference identifies:
```text
accepted
    state  -> SemanticState ObjectId
    change -> ChangeRevision ObjectId | none
```

Refs allow advancing repository state without rewriting historical objects.

---

## Local Draft State

Local draft state represents transient, unaccepted work being staged in a local transaction session:

```text
local draft session
    base_state        -> SemanticState ObjectId
    staged_operations -> [Operation]
    candidate_state   -> S_working
```

Local draft session state is stored separately from repository refs and does not pollute canonical persistence until committed.

---

## Structural Sharing

`SemanticState` objects reuse unchanged immutable object versions across state transitions. If a Change modifies one Requirement, the new `SemanticState` references the new Requirement version while preserving unchanged decision, implementation, and relationship version ObjectIDs.

---

## Content Addressing

Canonical immutable objects are content-addressed using SHA-256 hashes of their deterministic CBOR encodings:

```text
Canonical Object Payload -> Deterministic CBOR Encoding -> SHA-256 -> ObjectId
```

Content addressing guarantees immutability, integrity verification, deduplication, and structural sharing, while keeping stable logical identities (`ElementId`, `RelationshipId`, `ChangeId`) independent.

---

## Ontology Service

The Ontology Service enforces structural and relationship type rules defined by the active `OntologyVersion`:
* Validates element type registration.
* Validates relationship type registration.
* Enforces allowed source and target element type combinations for directed relationships.

Ontology validation occurs during semantic mutation evaluation and during complete candidate-state validation before acceptance.

---

## Invariant Engine

The Invariant Engine enforces core spec-defined structural and domain rules across candidate and accepted states:
* Stable identity uniqueness (`ElementId`, `RelationshipId`).
* Domain relationship validity and lifecycle compatibility (e.g. relationship source/target existence).
* Immutable historical traceability invariants.
* Authority of accepted semantic state over source files.

Semantic `Constraint` elements expressed in the model represent domain knowledge; they are evaluated separately from spec-defined mechanical invariants.

---

## Validation Engine

The Validation Engine coordinates semantic consistency evaluation without mutating state.

Repository integrity and canonical envelope decoding are validated by the persistence layer upon loading objects. The semantic Validation Engine evaluates:
* Candidate state $S_{\text{working}}$ and accepted state $S_n$ semantic consistency.
* Active ontology type rules and relationship endpoint constraints.
* Lifecycle state rules.
* Core spec-defined structural and domain invariants.

The engine explicitly reports unverified semantic `Constraint` elements rather than assuming compliance.

---

## Query Engine

The Query Engine provides read-only semantic query operations over accepted repository state $S_n$:

* `List`: Filtered element discovery by type or lifecycle.
* `Show`: Element property view and incoming/outgoing relationship inspection.
* `Status`: Repository summary, latest accepted change, and state counts.
* `Trace`: Origin provenance path traversal following normative origin policies, supporting depth-bounding (`max_depth: Option<usize>`), path-local cycle prevention, and shared-prefix tree projection (`to_tree()`).
* `Impact`: Partitioned impact analysis (`Directly Changed Elements`, `Semantically Affected Elements`, `Accountable Artifacts`) with depth-bounded evaluation (`max_depth: Option<usize>`) and path-local cycle prevention.
* `History`: Accepted history traversal and element revision filtering (`history --element`).
* `ArtifactAccountability`: `CURRENT` / `STALE` / `UNACCOUNTED` status reporting (`kat artifacts`).
* `InspectOntology`: Active ontology discovery, element/relationship type summaries, endpoint admissibility, and capability views (`kat ontology`, `kat ontology show`).

Query operations produce results that are semantically equivalent to querying canonical repository objects directly.

---

## Derived Projection Layer

The Derived Projection Layer contains rebuildable lookup structures designed to accelerate query operations (e.g. element indexes, relationship graph indexes, accountability status projections).

If a projection is lost or corrupted, KAT reconstructs it from canonical repository data and accepted roots.

---

## Artifact Accountability Architecture

Artifact accountability is an architectural capability built on semantic relationships and accepted history:
* **Direct Accountability Edges**: `kat.core/represents` and `kat.core/derived-from` relationships connecting `kat.core/artifact` elements to target knowledge elements.
* **Status Evaluation**: Evaluated dynamically by comparing recorded accountability baselines against target element versions in $S_n$ (`CURRENT`, `STALE`, `UNACCOUNTED`).
* **Baseline Reconciliation**: Executed via `AccountArtifact` operations through the Change Engine to log updated baselines in accepted `ChangeRevision` history without mutating `SemanticState`.

Physical source code files exist outside KAT internal repository storage; KAT does not inspect physical artifact file contents or perform file drift detection in v0.2.

---

## History and Causality

KAT v0.2 preserves a single accepted publication sequence of `ChangeRevision` objects through the accepted change head $C_n$.

Causal dependencies recorded in `ChangeRevision.dependencies` capture semantic links between changes independently of publication sequence order.

---

## Extension Boundaries (Future Architecture)

The following components represent future architecture boundaries beyond KAT v0.2:

### Future Materialization Extension
Generators, template engines, and materialization rules for producing physical source artifacts from authoritative semantic state.

### Future Physical Artifact Verification
File hashing engines and physical code analyzers for verifying disk contents against recorded accountability baselines.

### Future Distributed Collaboration Engine
Multi-participant Change proposal exchange, remote repository synchronization, multi-branch proposal graphs, semantic merge, and conflict reconciliation.

### Future Semantic Diff & Explain
Structural state comparison tools and AI-assisted change explanation interfaces.

---

## Determinism

Core semantic behavior is deterministic when given identical:
* Base `SemanticState`
* Ordered mutation operations
* Active `OntologyVersion`
* Core structural invariant rules

Canonical CBOR encoding guarantees identical `ObjectId` generation across implementations.

---

## v0.2 Architectural Scope

The KAT v0.2 architecture requires:
* Local repository management.
* Content-addressed canonical object storage (`KnowledgeElementVersion`, `RelationshipVersion`, `ChangeRevision`, `OntologyVersion`, `SemanticState`).
* Stable UUID identity separation (`ElementId`, `RelationshipId`, `ChangeId`).
* Single local draft session ($S_{\text{working}}$).
* Candidate-state validation and atomic publication with stale-base rejection.
* Active ontology enforcement and core invariant validation.
* Read-side queries (`list`, `show`, `status`, `trace`, `impact`, `history`, `artifacts`).
* Explicit `AccountArtifact` baseline reconciliation.

The v0.2 architecture does not include:
* Remote repository synchronization.
* Branching or multi-head accepted history.
* Automatic semantic merge.
* Code generation or materialization plugins.
* Physical file content inspection or drift detection.

---

## Core Architectural Rules

* Accepted `SemanticState` ($S_n$) is authoritative software knowledge.
* Accepted `ChangeRevision` head ($C_n$) provides accepted-history reachability.
* Authoritative state changes only through the Change Engine.
* Persisted canonical semantic objects are immutable.
* Stable semantic identities (`ElementId`) and content-derived `ObjectId`s are separate.
* A candidate state $S_{\text{working}}$ is validated before atomic publication.
* Accepted publication updates accepted state and change head together atomically; `SemanticState` `ObjectId` remains unchanged for history-only Changes (`AccountArtifact`).
* At most one local draft session exists per repository.
* Artifact accountability status (`CURRENT`, `STALE`, `UNACCOUNTED`) is semantic and baseline-based.
* Physical artifact modifications do not directly mutate authoritative knowledge; `AccountArtifact` reconciles baselines in accepted history.
* Derived projections must be rebuildable from canonical repository data.

---

## Future Research Questions

* When should persistent structural trees replace flat `SemanticState` manifests?
* How will format migrations be executed across canonical object stores?
* What protocol will govern remote object transfer between KAT repositories?
* How will executable materialization plugins register with the Application Layer?
* How will multi-participant conflict resolution graphs be represented for collaborative review?
