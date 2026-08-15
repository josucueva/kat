# Operations

## Purpose

Operations define the actions that can be performed on the KAT semantic model.

An operation represents a meaningful action involving software knowledge. Operations are defined independently from the interface used to invoke them.

Operations are divided into three categories:

* Mutation operations
* Query operations
* Validation operations

### Mutation Operations & Change Transactions

Mutation operations change the authoritative semantic state of the software.

Operations may be executed in **auto-commit mode** (one operation per change revision) or staged into a **multi-operation transaction** (`kat change begin` / `commit` / `abort`), producing a single `ChangeRevision` containing multiple operations published atomically. See [`docs/v0-2-multi-op-change-design.md`](v0-2-multi-op-change-design.md) for full transactional semantics.

Every successful mutation contributes to a change in the semantic model.

### Create

Creates a new knowledge element.

**Input:**

* Element type
* Properties

**Result:**

* A new knowledge element with a stable identity

**Example:**

```text
Create Requirement

Title:
"Support Apple Pay"

Description:
"Users must be able to pay using Apple Pay."
```

### Update

Changes one or more properties of an existing knowledge element.

**Input:**

* Element identity
* Properties to change

**Result:**

* The element contains the new values
* The previous state remains traceable through history

**Example:**

```text
Update Requirement

Element:
req-123

Change:
Priority: Medium -> High
```

### Deprecate

Marks a knowledge element as no longer active without removing its historical existence.

**Input:**

* Element identity
* Reason

**Result:**

* The element is marked as deprecated
* Its relationships and history remain traceable

Deprecation is preferred over deletion when the element has participated in the history of the software.

### Link

Creates a typed relationship between two knowledge elements.

**Input:**

* Source element
* Relationship type
* Target element

**Result:**

* A relationship is established between the elements

**Example:**

```text
Requirement
    |
    | addressed_by
    v
Design Decision
```

### Unlink

Removes an active relationship between two knowledge elements.

**Input:**

* Source element
* Relationship type
* Target element

**Result:**

* The relationship is no longer part of the current semantic state
* Its previous existence remains traceable through history

### Supersede

Replaces a knowledge element with another while preserving the relationship between the old and new elements.

**Input:**

* Existing element
* Replacement element
* Reason

**Result:**

* The existing element is marked as superseded
* The replacement becomes its successor
* Both elements remain traceable

**Example:**

```text
Design Decision A
"Use REST"
        |
        | superseded_by
        v
Design Decision B
"Use gRPC"
```

## Query Operations

Query operations inspect the semantic model without changing its state.

### Trace

Traverses relationships associated with a knowledge element.

Trace may operate forward or backward depending on the information being requested.

**Input:**

* Element identity
* Direction
* Optional relationship types

**Result:**

* Related knowledge elements
* Relationships connecting them
* Paths through which they are connected

**Example:**

```text
Implementation
    ^
    |
Design Decision
    ^
    |
Requirement
    ^
    |
Intent
```

### Impact

Identifies knowledge elements that may be affected by a change to another element.

Impact analysis follows relevant relationships from the selected element.

**Input:**

* Element identity
* Optional proposed change

**Result:**

* Directly affected elements
* Indirectly affected elements
* Relationships through which the impact was identified
* Affected artifacts when traceability is available

Impact represents potential consequences. It does not imply that every identified element becomes invalid or requires modification.

### Explain

Provides the knowledge necessary to understand why an element exists and how it relates to the software system.

**Input:**

* Element identity

**Result:**

An explanation derived from available information such as:

* Origin
* Intent
* Requirements
* Decisions
* Constraints
* Relationships
* Validation
* Relevant history

Explain does not create new knowledge. It presents knowledge already represented by KAT.

### History

Retrieves the evolution of a knowledge element or a selected part of the semantic model.

**Input:**

* Element identity or scope

**Result:**

* Changes affecting the selected knowledge
* Their causal or historical order when relevant
* Previous semantic states when available

History preserves the evolution of authoritative knowledge rather than only the current values of an element.

## Validation Operations

Validation operations evaluate the semantic model against defined rules without modifying its semantic state.

### Validate

Evaluates whether the selected semantic state satisfies the applicable consistency rules and constraints.

**Input:**

* Semantic model or selected scope
* Applicable validation rules

**Result:**

* Successful validations
* Violated rules
* Affected knowledge elements
* Information necessary to trace each violation

**Example:**

```text
Rule:
Every accepted Requirement must have an Implementation.

Result:
Violation

Element:
req-123

Missing:
Implementation
```

Validation reports the state of the model. It does not silently modify knowledge in order to satisfy a rule.

## Operation Properties

All operations follow these general properties.

### Identity

Operations that target existing knowledge elements use their stable identities rather than names or locations.

### Explicit Effects

The effect of an operation on the semantic model must be defined.

An operation must not introduce implicit semantic changes that cannot be identified or traced.

### Traceability

Changes produced through mutation operations must remain traceable as the software evolves.

### Validity

An operation may be rejected when its execution would violate required preconditions or produce a semantic state that violates rules required by the model.

### Authority

Mutation operations modify the authoritative semantic model.

Changes to artifacts are not equivalent to semantic operations unless their meaning is reconciled into the semantic model.

### Interface Independence

Operations describe semantic behavior, not user interface behavior.

The same operation may later be invoked through a CLI, API, editor, automation, or another interface without changing its semantics.

## Deferred Operations

The following operations are intentionally not defined in version 0.1:

* Merge
* Synchronize
* Branch
* Materialize
* Import
* Export

Their behavior depends on concepts that require further definition, particularly the collaboration and materialization models.
