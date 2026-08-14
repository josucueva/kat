# Ontology

## Purpose

The ontology defines the types of software knowledge represented by KAT and the relationships that may exist between them.

It provides a shared semantic vocabulary for the KAT model.

The ontology defines:

* Knowledge element types
* Relationship types
* Valid source and target types for relationships
* Basic semantic meaning of each relationship

The ontology does not define storage structures, database schemas, programming language types, or architecture-specific concepts.

## Principles

The ontology follows the core principles of KAT.

### Software knowledge is broader than source code

The ontology represents knowledge across different levels of software development, including intent, requirements, constraints, decisions, implementations, artifacts, and validation.

Source code is represented as an artifact rather than as the software model itself.

### The specification is authoritative

Intent, requirements, constraints, design decisions, and their relationships form the knowledge that defines the intended software state.

Implementation, artifacts, and validation remain traceable to that authoritative knowledge.

### Knowledge must remain traceable

Relationships are explicit and typed so that knowledge can be followed across different abstraction levels and through software evolution.

### Knowledge must be actionable

Relationships are not only descriptive.

They provide semantic information that can be used by KAT for tracing, impact analysis, validation, explanation, and evolution.

### The ontology is architecture-independent

The core ontology does not assume a particular software architecture, framework, programming language, or deployment model.

Concepts such as Controller, Service, Repository, Port, Adapter, Microservice, or Database are therefore not core KAT element types.

Such concepts may later be represented through extensions to the ontology.

# Knowledge Element

A Knowledge Element is an identifiable unit of software knowledge represented by KAT.

All knowledge elements have a stable identity.

The initial core knowledge element types are:

```text
Knowledge Element

    Intent
    Requirement
    Constraint
    Design Decision
    Implementation
    Artifact
    Validation
```

These types represent different roles within the software knowledge model. They do not define a mandatory development sequence.

# Element Types

## Intent

Intent represents the motivation, purpose, or desired outcome behind software knowledge.

Examples:

```text
Reduce payment failures.

Allow users to authenticate securely.

Provide immediate checkout confirmation.
```

Intent commonly provides the origin for requirements and decisions.

## Requirement

A Requirement describes a capability, behavior, or property that the software is expected to satisfy.

Requirements describe what is expected from the system without requiring a specific implementation.

Examples:

```text
Users must be able to reset their password.

Payments must support refunds.

The API must return a response within an accepted time.
```

## Constraint

A Constraint represents a rule, limitation, or condition that restricts possible software states or decisions.

Examples:

```text
Payment data must not be stored unencrypted.

Checkout must not wait for payment settlement.

The system must comply with a regulatory requirement.
```

A constraint may restrict different kinds of knowledge and is therefore not limited to design decisions.

## Design Decision

A Design Decision represents a chosen approach for addressing requirements or working within constraints.

A decision should preserve both the chosen approach and the reasoning behind it.

Examples:

```text
Use asynchronous payment processing.

Use PostgreSQL for persistence.

Use token-based authentication.
```

## Implementation

Implementation represents the concrete realization of intended software behavior or design.

Implementation is a semantic representation of how software knowledge is realized.

Examples may include:

```text
Payment processing component

Authentication mechanism

Persistence implementation

Refund workflow
```

Implementation is not equivalent to a source file.

One implementation may be represented by multiple artifacts.

## Artifact

An Artifact is a concrete representation, realization, or output associated with software knowledge.

Examples:

```text
Source file

OpenAPI document

Test

Configuration file

Deployment definition

Executable

Documentation
```

Artifacts do not independently define the intended state of the software.

## Validation

Validation represents evidence used to determine whether software knowledge or its realization satisfies expected requirements, constraints, or properties.

Examples:

```text
Integration test result

Performance measurement

Security analysis

Manual verification
```

A validation element represents evidence or a validation result.

The mechanism that produces that evidence, such as a test file, may itself be represented as an Artifact.

# Relationship Types

Relationships are directed and typed.

A relationship has:

```text
Source Element
Relationship Type
Target Element
```

For example:

```text
Intent
    motivates
        Requirement
```

The ontology defines which source and target types are valid for each relationship.

## motivates

Indicates that one knowledge element provides a reason or motivation for another.

Initial valid relationships:

```text
Intent -> Requirement
Intent -> Design Decision
```

Example:

```text
Intent:
Reduce checkout abandonment

    motivates

Requirement:
Checkout must complete without waiting for payment settlement
```

## addresses

Indicates that a Design Decision responds to a Requirement.

Valid relationship:

```text
Design Decision -> Requirement
```

Example:

```text
Design Decision:
Use asynchronous payment processing

    addresses

Requirement:
Checkout must not wait for payment settlement
```

## restricts

Indicates that a Constraint limits the valid possibilities for another knowledge element.

Initial valid relationships:

```text
Constraint -> Requirement
Constraint -> Design Decision
Constraint -> Implementation
```

Example:

```text
Constraint:
Payment information must remain encrypted

    restricts

Implementation:
Payment persistence
```

## guides

Indicates that a Design Decision influences how an Implementation is realized.

Valid relationship:

```text
Design Decision -> Implementation
```

Example:

```text
Design Decision:
Use event-driven payment processing

    guides

Implementation:
Payment processing workflow
```

## realizes

Indicates that an Implementation provides a concrete realization of a Requirement.

Valid relationship:

```text
Implementation -> Requirement
```

Example:

```text
Implementation:
Refund workflow

    realizes

Requirement:
Payments must support refunds
```

`realizes` does not by itself prove that the requirement is correctly satisfied. That determination belongs to validation.

## represents

Indicates that an Artifact provides a concrete representation of another knowledge element.

Initial valid relationship:

```text
Artifact -> Implementation
```

Example:

```text
Artifact:
payment_service.rs

    represents

Implementation:
Payment processing component
```

## derived_from

Indicates that an Artifact originates from or is produced from represented software knowledge.

Initial valid relationships:

```text
Artifact -> Requirement
Artifact -> Constraint
Artifact -> Design Decision
Artifact -> Implementation
```

Example:

```text
Artifact:
openapi.yaml

    derived_from

Requirement:
Expose refund operation
```

The exact difference between `represents` and `derived_from` may be refined as the materialization model is defined.

## validates

Indicates that Validation provides evidence about another knowledge element.

Initial valid relationships:

```text
Validation -> Requirement
Validation -> Constraint
Validation -> Implementation
```

Example:

```text
Validation:
Refund integration test result

    validates

Requirement:
Payments must support refunds
```

## depends_on

Indicates that one knowledge element requires another for its meaning or realization.

Initial valid relationship:

```text
Implementation -> Implementation
```

Example:

```text
Refund Processing
    depends_on
Payment Provider Integration
```

Additional valid source and target combinations may be introduced when justified by the domain model.

## supersedes

Indicates that one knowledge element replaces another while preserving historical traceability.

Initial valid relationship:

```text
Design Decision -> Design Decision
```

Example:

```text
Design Decision:
Use gRPC

    supersedes

Design Decision:
Use REST
```

The relationship may later be generalized to additional knowledge element types if their lifecycle requires explicit supersession.

# Relationship Direction

Relationship direction follows the semantic meaning of the relationship.

For example:

```text
Intent
    motivates
        Requirement
```

and:

```text
Design Decision
    addresses
        Requirement
```

The direction does not determine how KAT may traverse the relationship.

Trace operations may navigate relationships in either direction.

Therefore:

```text
Relationship direction != trace direction
```

# Relationship Validity

A relationship is valid only when:

* The source element exists
* The target element exists
* The relationship type exists
* The source type is allowed for the relationship
* The target type is allowed for the relationship
* Any additional ontology rules are satisfied

For example:

```text
Intent -> motivates -> Requirement
```

may be valid, while:

```text
Artifact -> motivates -> Requirement
```

is not valid in the core ontology.

Invalid relationships must not silently become part of an accepted semantic state.

# Extensibility

The core ontology defines only concepts that are broadly applicable to software systems.

Architecture-specific or technology-specific concepts should be introduced through extensions rather than added directly to the core ontology.

For example:

```text
Core:

Implementation

Extension:

Implementation
    |
    +-- Service
    +-- Controller
    +-- Repository
    +-- Adapter
```

Another architecture may define:

```text
Implementation
    |
    +-- Process
    +-- Actor
    +-- Message Handler
```

Both can use the same core KAT semantics without requiring KAT itself to assume either architecture.

Extensions must preserve the semantics and invariants of the core ontology.

# Evolution

The ontology describes the vocabulary used by the semantic model.

Changes to software knowledge operate on instances of ontology types and relationships.

For example:

```text
Create Requirement

Create Design Decision

Link:
Design Decision
    addresses
Requirement
```

The change model determines how those operations evolve the semantic state.

The ontology determines whether the resulting elements and relationships are semantically valid.

# Initial Core Ontology

The initial ontology can be summarized as:

```text
Intent
    motivates
        Requirement

Intent
    motivates
        Design Decision

Design Decision
    addresses
        Requirement

Constraint
    restricts
        Requirement

Constraint
    restricts
        Design Decision

Constraint
    restricts
        Implementation

Design Decision
    guides
        Implementation

Implementation
    realizes
        Requirement

Artifact
    represents
        Implementation

Artifact
    derived_from
        Requirement
        Constraint
        Design Decision
        Implementation

Validation
    validates
        Requirement
        Constraint
        Implementation

Implementation
    depends_on
        Implementation

Design Decision
    supersedes
        Design Decision
```

This initial ontology is intentionally small.

New element types and relationships should be added only when they represent concepts that cannot be expressed clearly through the existing ontology.

# Origin Traversal Policy

Relationships carry semantic directionality that defines how KAT queries navigate the semantic graph to trace authoritative origin and provenance.

When performing **Origin Tracing** (`kat trace`), relationships participate in provenance navigation according to a normative direction relative to their canonical definitions:

| Relationship Type | Canonical Form | Origin Traversal Direction | Semantic Rationale |
| :--- | :--- | :--- | :--- |
| `kat.core/motivates` | Intent $\xrightarrow{\text{motivates}}$ Req / Decision | Target $\to$ Source (**Backward**) | Requirement / Decision is motivated by Intent |
| `kat.core/derived-from` | Artifact $\xrightarrow{\text{derived-from}}$ Auth Knowledge | Source $\to$ Target (**Forward**) | Artifact is derived from Requirement / Decision / Constraint |
| `kat.core/realizes` | Impl $\xrightarrow{\text{realizes}}$ Requirement | Source $\to$ Target (**Forward**) | Implementation realizes Requirement |
| `kat.core/represents` | Artifact $\xrightarrow{\text{represents}}$ Implementation | Source $\to$ Target (**Forward**) | Artifact represents Implementation |
| `kat.core/validates` | Validation $\xrightarrow{\text{validates}}$ Subject | Source $\to$ Target (**Forward**) | Validation validates Requirement / Constraint / Implementation |
| `kat.core/restricts` | Constraint $\xrightarrow{\text{restricts}}$ Req / Decision / Impl | Target $\to$ Source (**Backward**) | Element is restricted by Constraint |
| `kat.core/addresses` | Decision $\xrightarrow{\text{addresses}}$ Requirement | Source $\to$ Target (**Forward**) | Decision exists to address Requirement |
| `kat.core/supersedes` | Replacement $\xrightarrow{\text{supersedes}}$ Existing Decision | Source $\to$ Target (**Forward**) | Replacement decision supersedes old decision |
| `kat.core/guides` | Decision $\xrightarrow{\text{guides}}$ Implementation | Target $\to$ Source (**Backward**) | Implementation is guided by Decision |
| `kat.core/depends-on` | Impl $\xrightarrow{\text{depends-on}}$ Implementation | *Excluded (Non-Origin)* | Structural dependency (reserved for Impact Analysis) |

Relationships classified as *Excluded* (such as `kat.core/depends-on`) represent horizontal or operational dependencies rather than authoritative origin or rationale, and are omitted from Origin Tracing.

# Impact Propagation Policy

Impact Analysis (`kat impact`) answers: *If a knowledge element changes, what other current knowledge may be affected?*

Impact propagation travels through relationships according to a normative direction relative to their canonical definitions:

| Relationship Type | Canonical Form | Impact Propagation Direction | Semantic Rationale |
| :--- | :--- | :--- | :--- |
| `kat.core/motivates` | Intent $\xrightarrow{\text{motivates}}$ Req / Decision | Source $\to$ Target (**Forward**) | Changed Intent affects motivated Requirement / Decision |
| `kat.core/addresses` | Decision $\xrightarrow{\text{addresses}}$ Requirement | Target $\to$ Source (**Backward**) | Changed Requirement affects addressing Decision |
| `kat.core/restricts` | Constraint $\xrightarrow{\text{restricts}}$ Req / Decision / Impl | Source $\to$ Target (**Forward**) | Changed Constraint affects restricted elements |
| `kat.core/guides` | Decision $\xrightarrow{\text{guides}}$ Implementation | Source $\to$ Target (**Forward**) | Changed Decision affects guided Implementation |
| `kat.core/realizes` | Impl $\xrightarrow{\text{realizes}}$ Requirement | Target $\to$ Source (**Backward**) | Changed Requirement affects realizing Implementation |
| `kat.core/represents` | Artifact $\xrightarrow{\text{represents}}$ Implementation | Target $\to$ Source (**Backward**) | Changed Implementation affects representing Artifact |
| `kat.core/derived-from` | Artifact $\xrightarrow{\text{derived-from}}$ Auth Knowledge | Target $\to$ Source (**Backward**) | Changed Auth Knowledge affects derived Artifact |
| `kat.core/validates` | Validation $\xrightarrow{\text{validates}}$ Subject | Target $\to$ Source (**Backward**) | Changed Subject affects validating evidence |
| `kat.core/depends-on` | Impl A $\xrightarrow{\text{depends-on}}$ Impl B | Target $\to$ Source (**Backward**) | Changed Dependency B affects dependent Implementation A |
| `kat.core/supersedes` | Replacement $\xrightarrow{\text{supersedes}}$ Existing Decision | *Excluded (Non-Impact)* | Historical evolution relation (omitted from current impact) |

Impact Analysis categorizes impacted elements into three distinct buckets:
1. **Directly Changed Elements**: The root element(s) being modified.
2. **Semantically Affected Elements**: Non-artifact Active elements reached via impact propagation (`Requirement`, `Constraint`, `Design Decision`, `Implementation`, `Validation`, `Intent`).
3. **Affected Artifacts**: Active elements of type `kat.core/artifact` reached via impact propagation.

> **Lifecycle Policy Distinction**: Filtering reached target elements to `Lifecycle::Active` is an **Impact-specific query policy**. Because Impact Analysis identifies potential consequences for the *current accepted operational state*, historical (`Deprecated` or `Superseded`) targets are excluded from impact results. By contrast, **Trace Origin** retains all historical lifecycle states in trace paths to preserve full provenance history.




