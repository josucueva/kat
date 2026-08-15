# Repository Model

## Purpose

The repository model defines the conceptual boundary of a software system managed by KAT.

A KAT repository contains the knowledge, rules, history, and references required to represent and evolve a software system according to the KAT model.

The repository provides a boundary for:

* Authoritative software knowledge
* Semantic state
* Accepted change history
* Ontology
* Invariants
* Working changes
* Materialization information
* Artifact references
* Collaboration state

The repository model does not define filesystem layout, database structure, network protocols, serialization formats, or storage technology.

## Repository

A KAT Repository is the unit through which KAT manages the knowledge and evolution of a software system.

Conceptually:

```text
KAT Repository
        |
        +--> Software
        |
        +--> Semantic Model
        |
        +--> Ontology
        |
        +--> Invariants
        |
        +--> Change History
        |
        +--> Working State
        |
        +--> Materialization Information
        |
        +--> Artifact References
```

The repository is not the software itself.

It is the KAT-managed environment in which the software's authoritative knowledge, evolution, and relationships are maintained.

## Software Boundary

A repository manages one logical software system.

The Software entity represents the system whose knowledge is maintained by the repository.

```text
KAT Repository
        |
        | manages
        v
Software
```

The exact physical boundaries of the software are not determined by KAT.

A software system may be implemented as:

* A monolith
* Multiple services
* Multiple applications
* Libraries
* Infrastructure components
* Other architectural structures

These may all belong to the same repository when they form part of the same logical software system.

The criteria for splitting or combining software systems across repositories may be refined later.

## Authoritative Repository State

A repository maintains an accepted semantic state representing the currently authoritative knowledge of the software.

It also maintains an accepted change head identifying the latest accepted Change Revision associated with that state.

Conceptually:

```text
Repository
    |
    v
Accepted Repository State
    |
    +--> Accepted Semantic State
    |
    +--> Accepted Change Head
```

The accepted semantic state defines the currently authoritative software knowledge.

The accepted change head identifies the current root of accepted semantic history.

For the initial repository state, no accepted Change may exist yet:

```text
Accepted Repository State
    |
    +--> Semantic State S0
    |
    +--> Change Head: none
```

After an accepted change:

```text
Accepted Repository State
    |
    +--> Semantic State S1
    |
    +--> Change Head C1
```

The accepted semantic state must satisfy:

* The repository ontology
* Required invariants
* Applicable consistency rules
* Accepted changes and their postconditions

Working or proposed changes do not redefine either the authoritative semantic state or accepted history until they are accepted.

The accepted semantic state and accepted change head must advance together as one repository-level transition.

## Working State

A repository may contain semantic work that has not yet become part of the accepted state.

Working state may contain:

* Local changes
* Proposed changes
* Changes awaiting validation
* Changes awaiting reconciliation
* Unresolved collaborative work

Conceptually:

```text
Accepted State
      |
      +--> Working Change A
      |
      +--> Working Change B
```

Working state and accepted state are distinct.

Incomplete or unresolved work must not silently become authoritative.

## Ontology

A repository is associated with an ontology that defines the semantic vocabulary available to the software model.

The ontology defines:

* Knowledge element types
* Relationship types
* Valid source and target combinations
* Basic relationship semantics

The repository must support the KAT core ontology.

A repository may also use ontology extensions when additional domain, architecture, or technology-specific concepts are required.

```text
Repository Ontology
        |
        +--> KAT Core Ontology
        |
        +--> Repository Extensions
```

Extensions must remain compatible with the rules and principles of the core ontology.

## Invariants

A repository maintains the invariants required for its accepted semantic states.

These may include:

### Core Invariants

Rules required by KAT itself.

Examples include:

* Stable knowledge identity
* Relationship validity
* Historical traceability
* Authority of the semantic model

### Repository Invariants

Rules that apply specifically to the software being managed.

Example:

```text
Every accepted payment Requirement
must have at least one Implementation.
```

Repository-specific invariants may strengthen the rules of the model but must not contradict required KAT invariants.

## Change History

A repository preserves the semantic evolution of the software through changes.

```text
Semantic State S0
        |
        | Change A
        v
Semantic State S1
        |
        | Change B
        v
Semantic State S2
```

Change history preserves, when applicable:

* Change identity
* Semantic operations
* Causal dependencies
* Affected knowledge
* Preconditions
* Postconditions
* Supersession or compensation
* Relationships between semantic states

History represents the evolution of authoritative software knowledge rather than a sequence of file modifications.

Independent changes do not require an artificial semantic ordering when no causal relationship exists between them.

The repository maintains an accepted change head identifying the latest accepted Change Revision associated with the accepted semantic state.

Conceptually:

```text
Accepted Change Head
        |
        v
Change B
        |
        +--> base state: S1
        |
        +--> result state: S2
```

Together with the accepted semantic state:

```text
Accepted Repository State

    state  -> S2
    change -> Change B
```

This allows KAT to distinguish accepted Change Revisions from Changes or Change Revisions that may exist in repository storage but were never accepted.

The accepted change head does not replace causal dependencies between Changes.

It provides a repository root from which accepted semantic history can be identified.

The change graph may still contain causal branching or reconciliation and does not require a globally linear history.

## Repository State

The state of a repository consists of the information necessary to understand the current accepted software knowledge and its evolution.

Conceptually:

```text
Repository State

    Accepted Semantic State
    Accepted Change Head
    Ontology
    Invariants
    Change History
    Working State
    Materialization State
```

Not every part of repository state is authoritative software knowledge.

For example:

* The accepted semantic state represents authoritative software knowledge.
* The accepted change head identifies the current root of accepted semantic history.
* Change history represents the evolution of authoritative software knowledge.
* Working state represents proposed evolution.
* Materialization state describes the relationship between knowledge and artifacts.

These roles must remain distinguishable.

## Artifacts

Artifacts associated with the software may exist within or outside the physical storage used by the KAT repository.

The repository maintains references necessary to relate those artifacts to software knowledge.

```text
Repository
    |
    | references
    v
Artifact
    |
    | represents / derived_from
    v
Knowledge
```

An artifact's location is not its semantic identity.

Moving or renaming an artifact must not by itself create a new knowledge identity.

Artifact references may include information necessary for materialization, traceability, validation, and divergence detection.

The exact representation of artifact references is an implementation concern.

## Materialization State

The repository maintains enough information to understand how artifacts relate to the authoritative semantic model.

This may include:

* Which knowledge an artifact represents
* Which knowledge an artifact derives from
* Relevant materialization relationships
* Whether an artifact is consistent, outdated, incomplete, or divergent
* The semantic state or knowledge from which an artifact was materialized, when known

Materialization information does not make artifacts authoritative.

## Collaboration State

A repository may contain information necessary to support collaborative semantic evolution.

This may include:

* Working changes
* Shared changes
* Causal dependencies
* Reconciliation state
* Conflict information

The repository must distinguish collaborative working state from accepted authoritative state.

The exact synchronization mechanism between repositories is outside the scope of this model.

## Repository Validity

A repository is semantically valid when its accepted state satisfies all required rules.

At minimum:

* The semantic model conforms to the active ontology.
* Required invariants are satisfied.
* Relationships reference valid knowledge elements.
* Accepted changes satisfy their required conditions.
* Historical traceability required by KAT is preserved.
* The accepted change head, when present, references an accepted Change Revision whose resulting semantic state is the accepted semantic state.

Conceptually:

```text
accepted.change.result_state
            ==
accepted.state
```

For the initial repository state:

```text
accepted.state  = S0
accepted.change = none
```

Artifact divergence does not necessarily make the authoritative semantic state invalid.

Instead, it indicates inconsistency between the semantic state and its materialization and must remain identifiable.

## Repository Identity

A repository should have a stable identity independent from its physical location.

Moving the repository, copying its storage, or changing its filesystem path does not by itself redefine the software knowledge represented by it.

The exact repository identity format is an implementation concern.

## Repository and Artifact Storage

The conceptual repository boundary is different from the physical storage boundary.

For example:

```text
Conceptual KAT Repository
        |
        +--> Semantic knowledge
        +--> History
        +--> Rules
        +--> Artifact references

Physical environment
        |
        +--> KAT internal storage
        +--> Source directories
        +--> Generated artifacts
        +--> External resources
```

KAT should therefore not assume that every artifact must physically exist inside a specific repository directory.

This distinction allows KAT to manage software whose artifacts are distributed across different locations or systems.

## Repository and Version Control

A KAT repository is not defined by file-based version control.

External version control systems may be used to transport, store, or collaborate on artifacts, but their file history does not replace the semantic history maintained by KAT.

Conceptually:

```text
KAT Repository
    |
    +--> Semantic evolution
    +--> Knowledge history
    +--> Traceability

Artifact Version Control
    |
    +--> File evolution
```

The two systems may integrate, but they represent different forms of evolution.

## Repository Lifecycle

A repository conceptually moves through several lifecycle activities.

### Initialization

A repository is established with the information required to begin representing a software system.

This includes at least:

* Repository identity
* Software identity
* Core ontology
* Required core invariants
* Initial semantic state
* An accepted repository state referencing the initial semantic state with no accepted Change head

Conceptually:

```text
Accepted Repository State

    state  -> S0
    change -> none
```

### Evolution

Authoritative software knowledge evolves through accepted changes.

When a Change is accepted, the accepted semantic state and accepted change head advance together.

Conceptually:

```text
Before:

    state  -> S0
    change -> C0

After accepting C1:

    state  -> S1
    change -> C1
```

### Materialization

Knowledge may be realized through artifacts.

### Collaboration

Participants may develop and reconcile changes to the shared semantic model.

### Validation

The repository's semantic state may be evaluated against ontology rules, invariants, and consistency requirements.

These activities do not define a mandatory sequential workflow.

## Core Rules

The repository model follows these rules:

* A repository manages one logical software system.
* The repository is not the software itself.
* The accepted semantic state is authoritative.
* The repository maintains both an accepted semantic state and an accepted change head.
* The accepted semantic state and accepted change head advance together when a Change is accepted.
* The accepted change head identifies accepted semantic history but does not itself define intended software state.
* Working state must remain distinguishable from accepted state.
* Semantic history records knowledge evolution rather than file evolution.
* The repository ontology defines the vocabulary available to the semantic model.
* Required invariants constrain accepted semantic states.
* Artifacts remain traceable to knowledge without becoming authoritative.
* Artifact location does not define artifact identity.
* Repository identity is independent from physical location.
* Collaboration and materialization state do not independently redefine authoritative knowledge.
* Physical storage is an implementation concern.

## Open Questions

The following questions remain intentionally unresolved:

* Can one repository manage more than one logical software system?
* Can repositories reference knowledge maintained by other repositories?
* How are repository dependencies represented?
* How are ontology extensions installed or versioned?
* How are repository-specific invariants defined and evolved?
* How is the initial semantic state created?
* How are repositories copied or replicated without confusing repository identity?
* Can a repository have multiple accepted semantic states simultaneously?
* If multiple accepted semantic states are supported in the future, does each accepted state maintain its own accepted change head?
* How are repository boundaries determined for large systems?
* How are external artifacts referenced?
* Which parts of repository state must be persisted?
* Which parts may be derived from other repository information?
* How are repositories exchanged between participants?
