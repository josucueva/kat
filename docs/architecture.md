# Architecture

## Purpose

This document defines the logical architecture of KAT.

The architecture translates the conceptual model of KAT into a set of components, responsibilities, and boundaries.

It defines how KAT:

* Manages authoritative semantic state
* Preserves accepted semantic history
* Applies semantic changes
* Enforces ontology and invariants
* Supports traceability and validation
* Persists immutable semantic objects
* Materializes artifacts
* Reconciles artifact divergence
* Supports collaborative evolution
* Exposes its behavior through interfaces

The architecture does not define specific programming languages, filesystem layouts, databases, user interface technologies, or network protocols.

Physical implementation details are defined separately by the prototype and canonical format specifications.

## Architectural Principles

The architecture follows the core principles of KAT.

### Specification-First Authority

The authoritative state of the software is represented by the semantic model.

Artifacts may represent, implement, validate, or materialize that knowledge, but they do not independently redefine it.

### Controlled Semantic Mutation

Authoritative semantic state may only change through the KAT change process.

No interface, plugin, materializer, persistence component, or reconciliation mechanism may bypass the semantic change path to directly modify accepted semantic state.

### Immutable Persistence

Persisted canonical semantic objects and historical states are immutable.

Evolution creates new objects and versions rather than modifying historical objects in place.

### Stable Semantic Identity

Semantic identity is independent from immutable object identity.

A knowledge element may evolve through multiple immutable versions while preserving the same semantic identity.

### Content-Addressed Versions

Immutable canonical objects are identified by content-derived identities.

The Object ID of a canonical object is derived from its canonical representation.

Stable semantic identities remain independent from these content-derived Object IDs.

### Atomic Publication

A candidate semantic state becomes authoritative only after the complete Change has been applied and successfully validated.

Acceptance advances both:

* The accepted semantic state
* The accepted Change head

These must advance together as one repository-level transition.

### Derived Data Is Rebuildable

Indexes, caches, projections, and other query acceleration structures are derived from canonical repository data.

Loss of derived data must not imply loss of authoritative knowledge or accepted history.

### Architecture Independence

Core KAT architecture must not depend on a particular software architecture, programming language, framework, or artifact structure.

## Architectural Overview

KAT is organized around a Semantic Repository containing immutable canonical objects and mutable repository references.

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
        |                                     |
        v                                     v
  Ontology Engine                      Invariant Engine
        |                                     |
        +------------------+------------------+
                           |
                           v
                  Canonical Persistence
                           |
                 +---------+---------+
                 |                   |
                 v                   v
          Immutable Objects      Repository Refs
                                     |
                                     v
                            accepted / working

                           |
                           v
                     Derived Indexes
```

The accepted repository reference identifies both the accepted Semantic State and the accepted Change head.

Materialization and collaboration operate through the same Semantic Repository but do not bypass the Change model.

## Interface Layer

The Interface Layer exposes KAT behavior to users and external tools.

Possible interfaces include:

* Command-line interface
* Programmatic API
* Editor integration
* Automation
* External tooling

Interfaces are responsible for:

* Receiving user intent
* Collecting operation inputs
* Presenting results
* Reporting validation and conflict information

Interfaces must not contain authoritative semantic logic.

The same semantic behavior should remain available regardless of the interface used.

## Application Layer

The Application Layer coordinates KAT use cases.

It receives requests from interfaces and delegates them to the appropriate engine.

Examples include:

* Create or update knowledge
* Trace origin
* Analyze impact
* Validate consistency
* Inspect history
* Materialize affected artifacts
* Reconcile collaborative changes

The Application Layer does not directly mutate persisted semantic objects or repository references.

## Semantic Repository

The Semantic Repository is the central logical boundary through which KAT accesses repository state.

It provides controlled access to:

* Accepted semantic state
* Accepted Change head
* Working states
* Knowledge Element Versions
* Relationship Versions
* Change Revisions
* Ontology Versions
* Repository invariants
* Materialization information
* Collaboration state

The Semantic Repository separates semantic behavior from physical persistence.

Conceptually:

```text
KAT Components
      |
      v
Semantic Repository
      |
      v
Persistence Implementation
```

The persistence implementation may change without changing repository semantics.

## Accepted Repository State

The repository maintains two related accepted references:

```text
Accepted Repository State

    state  -> SemanticState
    change -> ChangeRevision | none
```

The accepted Semantic State defines the authoritative software knowledge.

The accepted Change head identifies the current root of accepted semantic history.

For the initial repository:

```text
state  -> S0
change -> none
```

After accepting Change Revision C1:

```text
state  -> S1
change -> C1
```

The repository must preserve the invariant:

```text
accepted.change.result_state
            ==
accepted.state
```

whenever an accepted Change head exists.

The accepted Change head does not itself define intended software state.

It provides accepted-history reachability.

## Change Engine

The Change Engine is the only component responsible for producing new authoritative semantic states.

A Change may contain one or more mutation operations.

The Change Engine coordinates:

1. Loading the accepted repository state
2. Selecting the base Semantic State
3. Checking operation preconditions
4. Applying semantic operations
5. Producing new immutable object versions
6. Constructing a candidate Semantic State
7. Validating ontology rules
8. Evaluating operation postconditions
9. Evaluating required invariants
10. Persisting candidate canonical objects
11. Creating and persisting the Change Revision
12. Publishing the new accepted Semantic State and Change head together

Conceptually:

```text
Accepted Repository State

state  -> S0
change -> C0
        |
        v
Proposed Change
        |
        v
Check Preconditions
        |
        v
Apply Operations
        |
        v
Candidate State S1
        |
        +--> Ontology Validation
        +--> Postconditions
        +--> Invariants
        |
        v
Persist Canonical Objects
        |
        v
Create Change Revision C1
        |
        v
Atomic Publication
        |
        v

state  -> S1
change -> C1
```

If any required validation fails, the accepted repository state remains unchanged.

## Atomic Publication

Changes are atomic at the level of meaningful semantic evolution.

A partially applied Change must never become accepted repository state.

For example:

```text
Change:
Replace Decision A with Decision B

Operations:
    Create B
    Link B
    Supersede A
    Update affected relationships
```

The accepted state must never expose only some of these operations.

KAT therefore prepares and validates a complete candidate state before publication.

Before publication:

```text
accepted:

    state  -> S0
    change -> C0
```

After successful publication:

```text
accepted:

    state  -> S1
    change -> C1
```

The accepted Semantic State and accepted Change head must advance together.

Publication should use compare-and-swap semantics against the expected previously accepted repository state.

Conceptually:

```text
compare_and_swap(

    expected:
        state  = S0
        change = C0

    new:
        state  = S1
        change = C1
)
```

If the expected accepted state no longer matches, publication must fail rather than overwrite concurrent repository evolution.

If KAT fails before publication, the previously accepted repository state remains authoritative.

The exact physical mechanism used to make publication atomic is an implementation concern.

## Semantic Model Engine

The Semantic Model Engine provides the logical representation of semantic states.

It is responsible for:

* Loading Semantic State composition
* Resolving stable identities to immutable versions
* Reading knowledge elements
* Reading relationships
* Constructing candidate states
* Comparing semantic states
* Providing semantic data to other engines

A Semantic State logically maps semantic identities to immutable object versions.

Example:

```text
Semantic State S10

Elements:
    req-123 -> Requirement Version R4
    dec-021 -> Decision Version D2
    imp-904 -> Implementation Version I6

Relationships:
    rel-44 -> Relationship Version L3
    rel-87 -> Relationship Version L1
```

Semantic States are immutable.

## Canonical Object Model

The canonical repository store contains the minimum immutable information required to preserve authoritative meaning and semantic history.

The canonical object types are:

```text
Knowledge Element Version
Relationship Version
Ontology Version
Change Revision
Semantic State
```

Repository metadata and repository refs are persisted repository information but are not immutable content-addressed canonical object types.

### Knowledge Element Version

Represents one immutable version of a knowledge element.

It contains:

* Stable semantic identity
* Ontology type
* Lifecycle state
* Semantic properties

Its immutable version identity is its content-derived Object ID.

Examples of ontology types include:

* Intent
* Requirement
* Constraint
* Design Decision
* Implementation
* Artifact
* Validation

Ontology types are not separate persistence object kinds.

### Relationship Version

Represents one immutable version of a typed semantic relationship.

It contains:

* Stable relationship identity
* Source element identity
* Relationship type
* Target element identity
* Relationship properties when applicable

Its immutable version identity is its content-derived Object ID.

Relationships remain first-class objects because they participate independently in traceability, validation, evolution, and conflict analysis.

### Change Revision

Represents one immutable revision of a semantic Change.

It contains information such as:

* Stable Change identity
* Base Semantic State references
* Ordered semantic operations
* Causal dependencies
* Resulting Semantic State
* Human-readable description when present

Standard operation preconditions and postconditions may be implied by the operation structures rather than redundantly persisted.

A Change may evolve through multiple revisions while it remains local.

Each persisted Change Revision is immutable and receives a distinct content-derived Object ID.

### Semantic State

Represents one immutable composition of software knowledge.

A Semantic State references:

* Knowledge Element Versions
* Relationship Versions
* Ontology Version

The state does not duplicate the complete contents of referenced objects.

The Semantic State does not contain history information or references back to its originating Change Revision.

### Ontology Version

Represents one immutable version of the ontology used to interpret and validate a Semantic State.

Ontology Versions are immutable so historical states can be interpreted using the semantic vocabulary that applied to them.

An Ontology has a stable semantic identity while each immutable Ontology Version has a content-derived Object ID.

## Repository References

Repository refs are mutable repository-level references to immutable canonical objects.

The accepted repository ref identifies:

```text
accepted

    state  -> SemanticState ObjectId
    change -> ChangeRevision ObjectId | none
```

Future working refs may identify proposed semantic and historical heads:

```text
working/alice
working/bob
```

Repository refs provide movement between immutable repository objects without rewriting historical objects.

Refs are not themselves canonical semantic objects.

## Structural Sharing

Semantic States reuse unchanged immutable object versions.

Example:

```text
State S1

req-1 -> R1
dec-1 -> D1
imp-1 -> I1


State S2

req-1 -> R2
dec-1 -> D1
imp-1 -> I1
```

Only the Requirement changed.

The unchanged Decision and Implementation versions are shared between both states.

The architecture requires structural sharing at the canonical object level.

The physical representation of a Semantic State may initially contain a complete mapping of semantic identities to Object IDs and later adopt a more efficient persistent structure.

## Semantic Identity and Object Identity

KAT distinguishes stable semantic identity from immutable object identity.

Example:

```text
Requirement semantic identity:
    UUID R

Immutable versions:
    ObjectId V1
    ObjectId V2
    ObjectId V3
```

Semantic identity answers:

> Which knowledge element is this?

Object identity answers:

> Which exact immutable representation of that element is this?

KAT uses stable semantic identities for evolving concepts and content-derived Object IDs for immutable representations.

The same distinction applies to:

* Relationships
* Changes
* Ontologies

Semantic States require only immutable Object IDs because each distinct Semantic State is itself a distinct immutable object.

## Content Addressing

Canonical immutable objects are content-addressed.

Conceptually:

```text
Logical Canonical Object
        |
        v
Canonical Encoding
        |
        v
Canonical Bytes
        |
        v
SHA-256
        |
        v
Object ID
```

The content-derived Object ID provides:

* Immutable version identity
* Integrity verification
* Deduplication
* Structural sharing
* Reproducibility
* A foundation for future repository transfer

Stable semantic identity must remain independent from content addressing.

For example:

```text
ElementId:
    UUID

ElementVersionId:
    SHA-256 ObjectId
```

A change to canonical contents creates a new Object ID without creating a new semantic identity.

## Ontology Engine

The Ontology Engine determines whether semantic structures conform to the active ontology.

It is responsible for validating:

* Knowledge element types
* Relationship types
* Valid relationship source types
* Valid relationship target types
* Ontology extension rules

Example:

```text
Intent
    motivates
Requirement
```

may be valid, while:

```text
Artifact
    motivates
Requirement
```

may be rejected by the core ontology.

Ontology validation must occur before an invalid candidate Semantic State becomes accepted.

## Invariant Engine

The Invariant Engine evaluates required semantic invariants.

It is responsible for rules such as:

* Identity invariants
* Relationship invariants
* Lifecycle invariants
* Change invariants
* Traceability invariants
* Authority invariants
* Validation invariants
* History invariants
* Repository-specific invariants

The Invariant Engine operates above storage-level integrity constraints.

Storage integrity, canonical-format validity, ontology validity, and semantic invariants are distinct concerns.

## Validation Engine

The Validation Engine coordinates consistency evaluation without directly changing semantic state.

It may use:

* Ontology rules
* Invariants
* Repository-specific validation rules
* Validation evidence
* Traceability relationships

Validation produces reports describing:

* Successful validations
* Violated rules
* Affected knowledge
* Relevant traceability paths

Validation must not silently mutate the semantic model.

## Query Engine

The Query Engine provides read-only semantic operations.

It supports operations such as:

* Trace
* Impact
* Explain
* History
* Semantic state inspection
* Semantic diff

The Query Engine may use derived indexes for efficiency.

It must produce results that are semantically equivalent to querying canonical repository data directly.

## Derived Projection Layer

The Derived Projection Layer contains rebuildable representations designed to accelerate queries.

Possible projections include:

* Current element lookup
* Element type index
* Incoming relationship index
* Outgoing relationship index
* Traceability index
* Artifact path lookup
* Change dependency index
* State-to-Change history index
* Search index
* Validation cache

Derived projections are not authoritative.

Conceptually:

```text
Canonical Repository Data
        |
        v
     rebuild
        |
        v
Derived Projection
```

If a derived projection is lost or corrupted, KAT must be able to reconstruct it from canonical repository information and accepted repository roots.

The initial implementation may use only in-memory projections.

A database may later be used as a derived index without becoming the source of truth.

## Canonical Persistence

Canonical Persistence stores immutable canonical objects and repository-level metadata and refs.

Its responsibilities include:

* Persisting immutable content-addressed objects
* Loading objects by Object ID
* Verifying object integrity
* Persisting repository metadata
* Managing repository refs
* Supporting atomic accepted-state publication
* Preserving canonical data across executions

The logical architecture does not require a relational, graph, or document database.

Possible physical implementations include:

* Structured files
* Immutable object storage
* Embedded storage engines
* Hybrid object store and database
* Packed object storage

The physical implementation must preserve the canonical semantics defined by KAT.

## Canonical Format Boundary

The physical bytes of canonical objects are governed by a canonical repository format.

That format must define:

* Object envelope
* Object kinds
* Object schemas
* Deterministic encoding
* Object hashing
* Compatibility behavior

The architecture does not depend on the specific serialization technology, but all implementations of a repository format version must produce equivalent canonical object identities for equivalent canonical objects.

The concrete canonical format is specified separately.

## Repository Metadata

Repository Metadata contains information about the repository itself.

Examples include:

* Repository identity
* Software identity
* Repository format version
* Object encoding version
* Hash algorithm

Repository Metadata is distinct from semantic software state and from immutable canonical object history.

Moving or copying physical repository storage does not by itself redefine semantic identities.

## Persistence Integrity

Canonical persistence must support detection of corrupted or incomplete objects.

For content-addressed canonical objects, integrity verification compares:

```text
SHA-256(object bytes)
        ==
Object ID
```

A mismatch indicates repository corruption or integrity failure.

Integrity verification does not replace semantic validation.

A structurally intact object may still violate ontology rules or semantic invariants.

KAT therefore distinguishes:

```text
Encoding Validity
        |
        v
Repository Integrity
        |
        v
Semantic Validity
```

## Materialization Engine

The Materialization Engine realizes semantic knowledge through artifacts.

Its input is authoritative semantic knowledge.

Its output may include:

* Generated artifacts
* Assisted artifact proposals
* Materialization instructions
* Updated materialization metadata

Conceptually:

```text
Semantic Repository
        |
        v
Materialization Engine
        |
        v
Artifacts
```

Materialization does not directly modify authoritative semantic state.

Artifact effects remain consequences of semantic Changes.

## Artifact Boundary

Artifacts may exist inside or outside the physical KAT repository.

KAT maintains semantic references and provenance independently from physical artifact location.

An artifact path is therefore metadata, not semantic identity.

```text
Artifact Identity
    !=
Filesystem Path
```

Materialized artifact bytes do not need to be part of the canonical semantic object store.

## Divergence Detection

Artifact divergence occurs when an artifact no longer agrees with the authoritative semantic state.

A divergence detection mechanism may compare:

* Artifact state
* Materialization provenance
* Relevant semantic state
* Known artifact mappings

The exact detection mechanism is implementation-specific.

Detection must not directly mutate authoritative knowledge.

## Reconciliation

Reconciliation interprets artifact divergence and determines how it should be resolved.

Possible outcomes include:

* Modify the artifact to match existing knowledge
* Propose an authoritative semantic Change

Knowledge reconciliation must flow through the Change Engine.

```text
Artifact Divergence
        |
        v
Reconciliation
        |
        v
Proposed Change
        |
        v
Change Engine
        |
        v
Candidate Semantic State
```

No reconciliation component may bypass the Change Engine and directly modify accepted semantic state.

## Collaboration Engine

The Collaboration Engine coordinates semantic work created by multiple participants.

Its responsibilities may include:

* Working state management
* Change causality
* Compatibility analysis
* Reconciliation of concurrent Changes
* Semantic conflict detection
* Conflict resolution support
* Shared Change acceptance

The Collaboration Engine operates on semantic Changes and States rather than file differences.

Conceptually:

```text
        Accepted State S0
        /               \
       v                 v
   Change A           Change B
       \                 /
        \               /
         v             v
          Reconciliation
                |
                v
        Candidate State S1
```

The Collaboration Engine does not define transport or synchronization protocols.

## Working and Accepted States

The architecture distinguishes working repository state from accepted repository state.

Conceptually:

```text
accepted
    state  -> S10
    change -> C10

working/a
    state  -> S12
    change -> C12

working/b
    state  -> S14
    change -> C14
```

The accepted reference identifies authoritative repository state and the root of accepted history.

Working refs may identify proposed or incomplete semantic states and their corresponding Change heads.

Working states may contain unresolved changes that are not yet acceptable as authoritative state.

## Conflict Handling

Semantic conflicts are determined using:

* Change semantics
* Preconditions
* Dependencies
* Ontology rules
* Invariants
* Combined candidate state

Conflict is not determined only by artifact overlap.

The architecture may later support persistent conflicted working states.

Unresolved conflicts must not silently become part of accepted repository state.

## History

History is not stored as a separate canonical object.

It emerges from:

* Change Revisions
* Semantic States
* Change dependencies
* Accepted and working Change heads
* Stable semantic identities

Conceptually:

```text
S0 --C1--> S1 --C2--> S2
```

The accepted repository state provides the root:

```text
accepted.change -> C2
accepted.state  -> S2
```

Change dependencies may form causal structures such as:

```text
        C0
       /  \
     C1    C2
       \  /
        C3
```

History therefore supports causal structures without requiring one artificial global linear order.

The accepted Change head identifies accepted history reachability but does not eliminate or replace causal dependencies.

## Semantic Diff

Semantic diff is a derived operation.

Given two Semantic States, KAT may compare their semantic identity-to-version mappings.

Example:

```text
Requirement req-1:
    R4 -> R5

Implementation imp-2:
    unchanged

Relationship rel-3:
    removed
```

This structural difference may then be interpreted semantically.

Semantic diff is useful for inspection and analysis but does not replace Change history.

Changes remain the primary representation of meaningful evolution.

## Unreferenced Objects

Immutable objects may be persisted before the repository transition that references them becomes accepted.

If publication fails, such objects may remain unreferenced.

This does not invalidate the repository as long as the accepted repository references remain valid.

Examples include:

* Candidate Knowledge Element Versions
* Candidate Relationship Versions
* Candidate Semantic States
* Abandoned Change Revisions

Unreferenced objects may later be removed through garbage collection.

Garbage collection is not required for the initial architecture.

## Extension Boundary

KAT core should allow future extensions for:

* Ontology extensions
* Materializers
* Artifact analyzers
* Validators
* Importers
* Exporters
* Integration adapters

Extensions must operate through defined KAT interfaces.

They must not bypass ontology validation, invariants, or the Change Engine when authoritative knowledge is modified.

## Determinism

Core semantic behavior should be deterministic when provided with the same:

* Base Semantic State
* Change Revision
* Ontology
* Invariants

Canonical object representation must also be deterministic within a repository-format version so equivalent canonical objects produce the same Object ID.

Automation or AI may assist with:

* Proposing knowledge
* Suggesting relationships
* Generating artifacts
* Interpreting divergence

Such systems must not silently define semantic validity.

## Initial Architectural Scope

The initial KAT architecture requires:

* Local repository operation
* Immutable canonical semantic objects
* Stable semantic identities
* Content-addressed immutable object identities
* Semantic States
* Change Revisions
* Accepted Semantic State and Change head
* Change application
* Atomic accepted repository publication
* Ontology enforcement
* Invariant validation
* Basic traceability queries
* Persistent accepted history
* Rebuildable query projections

The initial architecture does not require:

* Remote repositories
* Network synchronization
* Automatic semantic merge
* Distributed locking
* CRDTs
* Graph database
* Persistent conflict states
* Garbage collection
* Object packing
* Full materialization automation
* AI integration

## Core Architectural Rules

The architecture follows these rules:

* The accepted Semantic State is authoritative software knowledge.
* The accepted Change head identifies the root of accepted semantic history.
* The accepted Semantic State and accepted Change head advance together.
* Authoritative state changes only through the Change Engine.
* Persisted canonical semantic objects are immutable.
* Stable semantic identity and immutable Object ID are separate.
* Immutable canonical Object IDs are derived from canonical contents.
* Semantic States reference immutable object versions.
* Semantic States do not contain their originating Change Revision.
* Unchanged versions may be structurally shared across Semantic States.
* A candidate Semantic State is validated before becoming accepted.
* Accepted repository publication is atomic.
* Repository refs are mutable; canonical historical objects are not.
* Canonical data must survive independently from derived indexes.
* Derived indexes must be rebuildable.
* Ontology rules and invariants are semantic concerns, not storage concerns.
* Materialization does not redefine authority.
* Artifact reconciliation must return through the normal Change path.
* Collaboration operates on semantic Changes rather than file diffs.
* Physical persistence technology does not define repository semantics.

## Open Questions

The following questions remain intentionally unresolved:

* When should persistent structural trees replace flat Semantic State manifests?
* Should a derived database be introduced after v0.1?
* How are repository format migrations performed?
* How are repository-specific invariants represented physically?
* How is materialization provenance persisted?
* When should Materialization become a canonical object type?
* Should unresolved conflicts become persistent canonical objects?
* How are canonical objects transferred between repositories?
* How are unreferenced objects detected and garbage-collected?
* How are canonical objects packed or compressed?
* How should extensions be discovered and loaded?
* How should repositories migrate if the canonical hash algorithm must change?
* How should multiple accepted heads be represented if KAT later supports more than one accepted semantic state?
