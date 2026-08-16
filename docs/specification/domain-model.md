# Domain Model

## Purpose

This document defines the fundamental domain entities, structural abstractions, and conceptual relationships that compose the KAT semantic model.

The domain model describes how software knowledge is represented, versioned, connected, traced, and evolved over time.

---

## Core Structural Entities

KAT models software systems through distinct structural entities, separating logical identity from content-addressed version snapshots.

### Repository
The top-level management unit governing one logical software system. A repository maintains stable identity (`RepositoryId`), active ontology, accepted semantic state, change history, and local transaction draft state.

### Software
A `Software` entity identifies the logical software system managed by a KAT repository. A software system is composed of its specification, implementation, artifacts, validation evidence, and the relationships between them.

### Knowledge Element
A logical unit of software knowledge (e.g. a specific requirement or design decision). A knowledge element has a stable 36-character hyphenated UUID identity (`ElementId`) that persists across all version revisions of that element.

### Knowledge Element Version
An immutable, content-addressed version snapshot of a knowledge element at a specific point in time. Each version is identified by its 32-byte CBOR hash (`ObjectId`), belongs to an `ElementId`, and records a `lifecycle` state (`Active`, `Deprecated`, or `Superseded`).

### Relationship
A logical semantic edge connecting a source element to a target element. A relationship has a stable 36-character hyphenated UUID identity (`RelationshipId`) that persists across all relationship version revisions.

### Relationship Version
An immutable, content-addressed version snapshot of a relationship. Each version is identified by its 32-byte CBOR hash (`ObjectId`), belongs to a `RelationshipId`, and records the relationship type (`type_id`), source and target element identities, and relationship properties.

### Semantic State
An immutable composition of software knowledge at a specific point in evolution. A `SemanticState` ($S_n$) maps selected `ElementId`s to specific `KnowledgeElementVersion` ObjectIds, and selected `RelationshipId`s to specific `RelationshipVersion` ObjectIds. `SemanticState` is a snapshot reference, not a mutable object.

### Change
A logical evolution of software knowledge from one semantic state to another. A Change has a stable 36-character hyphenated UUID identity (`ChangeId`).

### Change Revision
A `ChangeRevision` is an immutable canonical record of a Change revision, identified by its 32-byte CBOR hash (`ObjectId`). When accepted, it becomes part of accepted repository history. A `ChangeRevision` records its `change_id`, an ordered sequence of canonical mutation operations, `base_states` references, `result_state` reference, and `dependencies`.

### Ontology Version
An immutable canonical object defining the active knowledge element types, relationship types, and allowed source/target type combinations for a semantic state.

---

## Identity and Versioning Boundaries

A foundational principle of the KAT domain model is the explicit separation of stable identity from immutable version content:

* $\text{ElementId (UUID)} \neq \text{KnowledgeElementVersion ObjectId (SHA-256)}$
* $\text{RelationshipId (UUID)} \neq \text{RelationshipVersion ObjectId (SHA-256)}$
* $\text{ChangeId (UUID)} \neq \text{ChangeRevision ObjectId (SHA-256)}$

A `KnowledgeElement` or `Relationship` retains its stable UUID across updates, deprecations, and supersessions. Revisions create new immutable version objects without altering the underlying entity identity.

---

## Knowledge Categories

The domain model categorizes software knowledge into core semantic roles. Detailed relationship admissibility and type definitions are owned by [`docs/specification/ontology.md`](ontology.md).

### Intent
`Intent` represents the motivation, purpose, or desired outcome behind a software element or change, answering *"Why does this exist?"*.

### Requirement
A `Requirement` describes a desired capability, behavior, or property that the software system should satisfy, without dictating a specific implementation.

### Constraint
A `Constraint` expresses a semantic restriction, limitation, or condition that restricts possible software states or decisions. A `Constraint` element represents semantic knowledge; whether it can be mechanically verified depends on whether executable validation rules exist.

### Design Decision
A `Design Decision` represents a chosen solution or approach for addressing requirements and constraints, preserving both the choice and the rationale behind it.

### Implementation
`Implementation` represents the semantic realization of intended software behavior and design (e.g., payment processing component, refund workflow). Implementation is a semantic concept, not a source code file.

### Artifact
An `Artifact` is a concrete representation or output associated with software knowledge (e.g., source files, configuration files, test files, documentation). Artifacts concretely represent or derive from software knowledge. Some artifacts may participate in producing validation evidence, but Validation remains a distinct knowledge element.

### Validation
`Validation` represents evidence or results (e.g., test execution results, performance measurements) evaluating whether software knowledge or its realization satisfies expected properties. KAT records validation evidence; KAT does not necessarily execute the underlying test runner.

---

## Authoritative Knowledge

The intended state of the software is defined by its specification and represented through the semantic model.

Specification is a collective term for authoritative knowledge: intent, requirements, constraints, design decisions, and their relationships.

Implementation, artifacts, and validation remain connected to this authoritative knowledge through typed relationships, but artifacts do not independently redefine intended software state.

```text
Specification (Authoritative Knowledge)
        |
        | represented through
        v
Accepted Semantic State (Sn)
        |
        +--> Implementation
        +--> Artifacts
        +--> Validation Evidence
```

---

## Semantic Relationships

Relationships connect knowledge elements using canonical directed names defined by the active `OntologyVersion`.

Canonical core relationships include:

```text
Intent               -- motivates   --> Requirement / Design Decision
Design Decision      -- addresses   --> Requirement
Constraint           -- restricts   --> Requirement / Design Decision / Implementation
Design Decision      -- guides      --> Implementation
Implementation       -- realizes    --> Requirement
Artifact             -- represents  --> Implementation
Artifact             -- derived-from--> Requirement / Constraint / Decision / Implementation
Validation           -- validates   --> Requirement / Constraint / Implementation
Implementation       -- depends-on  --> Implementation
Design Decision      -- supersedes  --> Design Decision
```

Normative relationship endpoint rules and type admissibility are governed exclusively by `ontology.md`.

---

## Evolution

Software knowledge evolves through atomic, ordered semantic changes.

A Change contains ordered mutation operations (`CreateElement`, `UpdateElement`, `DeprecateElement`, `SupersedeElement`, `Link`, `Unlink`, `AccountArtifact`) that evolve candidate semantic knowledge or record semantic evolution metadata such as artifact accountability reconciliation.

```text
Accepted Semantic State S0
        |
        | Change Revision C1
        v
Accepted Semantic State S1
```

Physical artifact changes do not independently redefine authoritative knowledge. KAT v0.2 does not infer semantic divergence from physical artifact contents; artifact accountability is tracked separately through accepted relationships and baselines.

---

## Artifact Accountability

Artifacts remain traceable to authoritative knowledge through direct semantic relationships (`kat.core/represents`, `kat.core/derived-from`).

The domain model tracks artifact accountability state as a derived query property:
* **Accountability Baselines**: Recorded when direct accountability relationships are initially accepted and subsequently reconciled through `AccountArtifact`.
* **Reconciliation**: Executed via explicit `AccountArtifact` operations to re-baseline an artifact's direct accountability edges when target knowledge versions evolve.
* **Accountability Status**: Categorized as `CURRENT`, `STALE`, or `UNACCOUNTED`.

Accountability status reflects semantic baseline alignment and does not imply physical file verification on disk.

---

## Traceability

Traceability is the ability to navigate semantic relationships between software knowledge elements across abstraction levels and through historical evolution.

```text
Intent --motivates--> Requirement <--addresses-- Design Decision --guides--> Implementation <--represents-- Artifact
```

Traceability enables answering domain queries such as:
* *Why does this element exist?*
* *What requirements does this implementation realize?*
* *What design decisions guide this component?*
* *What artifacts are accountable to this requirement?*
* *How has this element evolved over accepted history?*

Query-specific graph traversal policies and directions (e.g. Origin Trace vs Impact Analysis) are defined by [`docs/specification/operations.md`](operations.md).

---

## Domain Boundaries

The KAT domain model enforces the following core boundaries:

* **Specification-First**: Artifacts represent or derive from knowledge; artifacts do not independently dictate authoritative specification.
* **Identity vs Location**: An `Artifact` element's `ElementId` UUID is its semantic identity; physical file paths are locators and do not define identity.
* **Semantic Constraint vs Executable Invariant**: A `Constraint` element expresses semantic restriction; KAT invariants are spec-level rules enforced mechanically.
* **Change vs Knowledge Element**: A `Change` is an evolution transaction containing operations; `Change` is not a `Knowledge Element` in the semantic graph.
