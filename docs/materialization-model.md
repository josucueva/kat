# Materialization Model

## Purpose

The materialization model defines how authoritative software knowledge is realized through concrete artifacts.

KAT follows a specification-first model. The semantic model defines the intended state of the software, while artifacts represent, implement, validate, or materialize that knowledge.

Materialization defines the relationship between those two levels.

The materialization model describes:

* What materialization means
* How knowledge relates to artifacts
* How generated and handwritten artifacts are treated
* How artifact provenance is preserved
* How artifact consistency and divergence are represented
* How reconciliation returns artifact differences to the authoritative semantic model

The model does not define specific code generators, template engines, file formats, build systems, or programming languages.

## Materialization

Materialization is the process by which software knowledge is realized or represented through one or more concrete artifacts.

Conceptually:

```text
Authoritative Knowledge
        |
        v
Implementation Knowledge
        |
        v
Materialization
        |
        v
Artifacts
```

Materialization does not imply that an artifact must be automatically generated.

An artifact may be generated, assisted, or manually created while still being a materialization of software knowledge.

## Authority

Materialization flows from authoritative knowledge toward artifacts.

```text
Semantic Model
      |
      | materialize
      v
Artifact
```

Materialization does not make the produced artifact authoritative.

Changes to an artifact do not independently redefine the intended software state.

The reverse flow is handled through reconciliation.

```text
Materialization

Knowledge
    |
    v
Artifact


Reconciliation

Artifact Difference
    |
    v
Proposed Knowledge Change
```

## Implementation and Artifact

Implementation and Artifact represent different levels of the software model.

### Implementation

Implementation represents semantic knowledge about how intended software behavior or design is realized.

Examples:

* Payment processing component
* Authentication mechanism
* Refund workflow
* Persistence implementation

Implementation is not tied to a specific file or physical representation.

### Artifact

An artifact is a concrete representation associated with software knowledge.

Examples:

* Source files
* OpenAPI documents
* Tests
* Configuration files
* Deployment definitions
* Executables
* Documentation

One implementation may be represented through multiple artifacts.

```text
Implementation
      |
      +--> Source file
      +--> Configuration
      +--> Test
```

One artifact may also represent or derive from multiple knowledge elements.

## Materialization Modes

KAT recognizes different ways in which artifacts may be materialized.

### Deterministic Materialization

The artifact is produced automatically from software knowledge using defined materialization rules.

```text
Semantic Model
      |
      v
Materializer
      |
      v
Artifact
```

Examples may include generated API descriptions, configuration, or source code.

### Assisted Materialization

Software knowledge is used to produce or propose an artifact, but human review or modification is part of the process.

```text
Semantic Model
      |
      v
Proposed Artifact
      |
      v
Human Review
      |
      v
Artifact
```

The mechanism used to produce the proposal does not affect the authority of the semantic model.

### Manual Materialization

A developer manually creates or modifies an artifact according to the authoritative software knowledge.

```text
Semantic Model
      |
      | guides
      v
Developer
      |
      v
Artifact
```

Manual artifacts remain subject to the same traceability and consistency expectations as generated artifacts.

Materialized does not mean generated.

## Artifact Relationships

Artifacts must remain traceable to the knowledge they represent or originate from.

Two relationships are used for different purposes.

### represents

`represents` expresses semantic correspondence between an artifact and a knowledge element.

Example:

```text
payment_service.rs
    represents
Payment Processing Implementation
```

The artifact is a concrete representation of that implementation.

### derived_from

`derived_from` expresses provenance.

It indicates that an artifact was produced or shaped using a knowledge element as an input.

Example:

```text
openapi.yaml
    derived_from
Refund API Requirement
```

An artifact may both represent one element and derive from others.

Example:

```text
openapi.yaml
    represents
Refund API Interface

openapi.yaml
    derived_from
Refund Requirement
```

The exact relationship vocabulary may be refined as the ontology evolves.

## Materialization Provenance

A materialized artifact should preserve enough information to identify the knowledge responsible for its existence or form.

An artifact may be traceable to:

* Requirements
* Constraints
* Design Decisions
* Implementations
* Other relevant knowledge

Conceptually:

```text
Artifact
    |
    +-- represents -----> Implementation
    |
    +-- derived_from ---> Requirement
    |
    +-- derived_from ---> Design Decision
    |
    +-- derived_from ---> Constraint
```

Materialization provenance supports:

* Traceability
* Explanation
* Impact analysis
* Divergence detection
* Historical analysis
* Reproducibility

## Materialization Cardinality

Materialization is not limited to one-to-one relationships.

One knowledge element may correspond to multiple artifacts.

```text
Implementation
    |
    +--> Artifact A
    +--> Artifact B
    +--> Artifact C
```

One artifact may also correspond to multiple knowledge elements.

```text
Artifact
    |
    +--> Requirement A
    +--> Requirement B
    +--> Design Decision
```

The materialization model therefore supports many-to-many relationships between knowledge and artifacts.

## Artifact State

An artifact may have a state relative to the authoritative semantic model.

### Consistent

The artifact agrees with the currently accepted software knowledge it represents or derives from.

### Outdated

The authoritative knowledge relevant to the artifact has changed and the artifact has not yet been updated or rematerialized.

Example:

```text
Requirement changed
        |
        v
Existing artifact no longer reflects current knowledge
```

### Incomplete

The artifact represents only part of the currently required software knowledge.

### Divergent

The artifact contains behavior, structure, or constraints that cannot be explained by or conflict with the authoritative semantic model.

Example:

```text
Artifact changed manually
        |
        v
Behavior introduced
        |
        v
No corresponding authoritative knowledge
```

Outdated and divergent are different states.

An outdated artifact reflects an older authoritative state.

A divergent artifact contains meaning that is not represented by the authoritative state.

## Materialization Inputs

A materialization should be traceable to the semantic knowledge used to produce or define the artifact.

Conceptually:

```text
Materialization

Inputs:
    Relevant semantic knowledge
    Materialization rules
    Target representation

Result:
    Artifact or artifacts
```

The model does not define how these inputs are physically stored.

The materialization should preserve enough information to determine whether an artifact still corresponds to the current semantic state.

## Materialization Rules

Materialization rules define how software knowledge maps to concrete representations.

Example:

```text
Requirement
+
Design Decision
+
Implementation
        |
        v
Materialization Rule
        |
        v
Artifact
```

Materialization rules are not necessarily part of the core KAT ontology.

Architecture-specific or technology-specific rules may be provided through extensions.

For example:

```text
KAT Core
    Materialization

Extensions
    OpenAPI Materializer
    Java Materializer
    Terraform Materializer
```

This preserves the architecture independence of the core model.

## Materialization Scope

Materialization may operate on different scopes.

Examples:

* Entire software system
* Selected knowledge element
* Selected implementation
* Selected capability
* Knowledge affected by a change

Conceptually:

```text
Change
    |
    v
Impact Analysis
    |
    v
Affected Knowledge
    |
    v
Materialization
    |
    v
Affected Artifacts
```

The materialization model does not require the entire software system to be materialized at once.

## Materialization and Validation

Materialization and validation are separate concerns.

Materialization answers:

> How is this knowledge concretely represented or realized?

Validation answers:

> Does that realization satisfy the expected requirements, constraints, or properties?

Conceptually:

```text
Knowledge
    |
    v
Materialization
    |
    v
Artifact
    |
    v
Validation
```

Successfully materializing an artifact does not prove that the resulting artifact is valid.

## Materialization and Change

Materialization normally does not create a new authoritative semantic change.

A change to authoritative knowledge may produce artifact effects.

Example:

```text
Change:
Require MFA

        |
        v

Semantic Effects:
Authentication design affected
Authentication implementation affected

        |
        v

Artifact Effects:
Authentication source outdated
API description outdated
Authentication tests outdated
```

Materialization may resolve those artifact effects by producing or updating the affected artifacts.

The resulting artifact modifications remain consequences of the original semantic change.

## Artifact Divergence

Artifact divergence occurs when an artifact no longer agrees with the authoritative semantic model.

This may occur when:

* A developer modifies an artifact manually
* External tooling modifies an artifact
* An artifact is created without corresponding knowledge
* Existing software is imported into KAT

A divergent artifact must not silently redefine authoritative knowledge.

The divergence should remain identifiable until it is resolved.

## Reconciliation

Reconciliation is the process of resolving a difference between an artifact and the authoritative semantic model.

Conceptually:

```text
Artifact Modification
        |
        v
Divergence
        |
        v
Semantic Difference Identified
        |
        v
Reconciliation
```

Reconciliation may produce different outcomes.

### Artifact Reconciliation

The artifact is changed to agree with the existing authoritative semantic model.

```text
Artifact Divergence
        |
        v
Artifact Updated
        |
        v
Consistent
```

### Knowledge Reconciliation

The artifact reveals an intentional change that should become part of the software specification.

In this case, reconciliation results in a proposed authoritative change.

```text
Artifact Difference
        |
        v
Proposed Change
        |
        v
Normal Change Process
        |
        v
New Semantic State
```

The artifact does not directly modify authoritative knowledge.

The change must pass through the normal KAT change process.

## Historical Traceability

Materialization should preserve enough history to explain the relationship between software knowledge and artifacts over time.

KAT should be able to determine, when information is available:

* Which knowledge caused an artifact to exist
* Which knowledge state an artifact corresponds to
* Which change caused an artifact to become outdated
* Which artifacts were affected by a semantic change
* Whether an artifact was generated, assisted, or manually maintained

The exact persistence mechanism for this information is outside the scope of the materialization model.

## Core Rules

The materialization model follows these rules:

* Materialization flows from authoritative knowledge toward artifacts.
* Materialized does not mean generated.
* Artifacts do not independently redefine authoritative knowledge.
* Implementation knowledge is distinct from its concrete artifacts.
* Artifact provenance must remain traceable.
* Materialization may be one-to-many or many-to-many.
* Materialization and validation are separate concerns.
* Artifact divergence must remain identifiable.
* Reconciliation is required when artifact meaning differs from authoritative knowledge.
* Changes to authoritative knowledge must pass through the normal change model.

## Open Questions

The following questions remain intentionally unresolved:

* How is artifact consistency determined?
* How is artifact divergence detected?
* How are materialization rules represented?
* How are materializers discovered or configured?
* How is partial materialization tracked?
* How are externally generated artifacts handled?
* How are materialization dependencies ordered?
* How is materialization provenance persisted?
* Can a materialization itself have a stable identity?
* How are artifacts associated with specific semantic states?
* How are conflicting artifact and semantic changes reconciled?

