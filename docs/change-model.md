# Change Model

## Purpose

The change model defines how KAT represents the evolution of software knowledge.

KAT follows a specification-first model. Changes to intent, requirements, constraints, decisions, and their relationships define the authoritative evolution of the software system.

Artifacts such as source code, tests, documentation, configuration, API descriptions, and deployment definitions may change as a consequence of this evolution, but they do not replace the semantic model as the source of truth.

The change model defines:

* What constitutes a change
* How changes affect semantic state
* How changes propagate to dependent knowledge
* How artifact effects relate to authoritative changes
* How changes depend on or conflict with other changes

The change model does not define distributed synchronization, branching, or remote collaboration.

## Authoritative Model

The semantic model is authoritative.

Intent, requirements, constraints, decisions, and their relationships define the intended state of the software.

Artifacts represent, implement, validate, or materialize that state, but they do not independently redefine it.

Software evolution should therefore be represented primarily as changes to authoritative knowledge.

## Change

A change represents an evolution of authoritative software knowledge from one semantic state to another.

A change may modify:

* Intent
* Requirements
* Constraints
* Design decisions
* Relationships between knowledge elements
* Other specification-level knowledge

A change may be realized through one or more mutation operations.

Example:

```text
Change:
Require MFA for authenticated users

Semantic modifications:
    Update authentication requirement
    Introduce MFA constraint
    Create supporting design decision
    Link requirement to decision
```

The change describes the meaningful evolution of the software.

The operations describe how KAT applies that evolution to the semantic model.

## Change Identity

Every change has a stable identity.

The identity of a change is independent from:

* Its description
* Its position in history
* The state it produces
* The identities of the operations it contains

A change may evolve during local work, but once it becomes part of persistent history its identity must continue to refer to the same historical change.

The exact identity format is an implementation concern and is not defined by this model.

## Operation

An operation is the smallest semantic mutation recognized by KAT.

Mutation operations include:

* Create
* Update
* Deprecate
* Link
* Unlink
* Supersede

A change may contain one or more operations.

Example:

```text
Change:
Replace REST interface with gRPC

Operations:
    Create new interface knowledge
    Link design decision to new interface
    Supersede previous interface
    Update dependent relationships
```

Operations modify the authoritative semantic model.

Operations on artifacts are not authoritative semantic operations unless their effects are reconciled back into the semantic model.

## Semantic State

A semantic state is the authoritative representation of software knowledge at a particular point in its evolution.

It contains the currently accepted intent, requirements, constraints, decisions, relationships, and other knowledge represented by KAT.

Applying a valid change produces a new semantic state.

```text
Semantic State A
        |
        | Change
        v
Semantic State B
```

Artifacts may be materialized from a semantic state, but they are not themselves the semantic state.

```text
Semantic State
     |
     +--> Source code
     +--> Tests
     +--> Documentation
     +--> Configuration
```

## Preconditions

A precondition is a condition that must be true before a change or operation can be applied.

Examples:

```text
Element exists.
Element is active.
Relationship exists.
Relationship does not already exist.
Element type is compatible with the operation.
```

If a required precondition is not satisfied, the operation cannot be applied normally.

Preconditions provide a deterministic basis for detecting incompatible changes.

## Atomicity

A change is the atomic unit of meaningful software evolution recognized by KAT.

A single change may contain several mutation operations.

Example:

```text
Change:
Replace Decision A with Decision B

Operations:
    Create Decision B
    Link Requirement to Decision B
    Supersede Decision A
    Update affected relationships
```

Intermediate operation results may not represent a complete or valid software state.

The system should therefore either apply the complete change or preserve the previous authoritative semantic state.

## Postconditions

A postcondition describes a condition that must be true after a change or operation has been successfully applied.

Example:

```text
Operation:
Supersede Decision A with Decision B

Postconditions:
    Decision A is superseded.
    Decision B is active.
    Decision A references Decision B as its successor.
```

Postconditions define the expected semantic result of an operation or change.

## Change Propagation

A change may affect other knowledge elements through their semantic relationships.

Example:

```text
Requirement changed
        |
        v
Design Decision affected
        |
        v
Implementation affected
        |
        v
Validation affected
```

KAT should distinguish between:

* The element directly changed
* Knowledge elements semantically affected by the change
* Artifacts derived from affected knowledge

Impact propagation does not imply that every affected element must change.

It identifies elements whose validity, consistency, or implementation may need to be reevaluated.

## Effects

An effect is a consequence of a change.

Effects may occur at different levels.

### Semantic Effects

Semantic effects are consequences that affect related software knowledge.

Example:

```text
Requirement changed
    ->
Design Decision requires review
```

### Validation Effects

Validation effects occur when previously valid knowledge must be reevaluated.

Example:

```text
Constraint changed
    ->
Existing implementation may no longer satisfy it
```

### Artifact Effects

Artifact effects occur when materialized artifacts may need to be regenerated, modified, or reviewed.

Example:

```text
API requirement changed
    ->
OpenAPI definition affected
    ->
Server implementation affected
    ->
Integration tests affected
```

Artifact effects are consequences of authoritative changes. They do not independently redefine the semantic state.

## Artifact Divergence

An artifact divergence occurs when an artifact no longer agrees with the authoritative semantic state.

This may happen when a developer modifies an implementation, configuration, test, or other artifact directly.

Conceptually:

```text
Artifact modified independently
        |
        v
Divergence detected
        |
        v
Semantic difference identified
        |
        v
Authoritative change proposed
        |
        v
Semantic state reconciled
```

An artifact modification does not automatically become an authoritative software change.

If the modification introduces meaningful behavior, structure, or constraints that are not represented in the semantic model, that difference should be reconciled through an authoritative change.

The exact mechanism used to detect and reconcile divergence is outside the scope of this version.

## Consistency Between Knowledge and Artifacts

KAT should preserve consistency between the authoritative semantic state and its materialized artifacts.

An artifact may be:

* Consistent
* Outdated
* Incomplete
* Divergent

A divergent artifact contains behavior or structure that cannot be explained by the current semantic state.

Such divergence should be detected and reconciled rather than silently accepted as a new source of truth.

## Causality

A change may depend on one or more previous changes.

Example:

```text
Change A:
Create Requirement R

Change B:
Create Design Decision D for Requirement R
```

Change B depends on Change A because the knowledge required by B does not exist before A.

This dependency defines a causal relationship:

```text
Change A
    |
    v
Change B
```

Changes are ordered where causality requires ordering.

Independent changes do not require a semantic ordering relationship.

## Compatibility

Two changes are compatible when they can both be incorporated into a semantic state without:

* Violating their preconditions
* Producing contradictory modifications
* Invalidating required relationships
* Violating semantic invariants

Compatible changes may still require a specific application order.

Compatibility does not imply independence.

## Conflict

A conflict occurs when two or more changes cannot be incorporated into the same semantic state without violating their intended semantics or the rules of the model.

KAT initially distinguishes the following conflict types.

### Write Conflict

A write conflict occurs when multiple changes assign incompatible values to the same semantic property.

Example:

```text
Change A:
Requirement.priority = High

Change B:
Requirement.priority = Critical
```

### Lifecycle Conflict

A lifecycle conflict occurs when one change modifies the lifecycle of an element in a way that invalidates another change.

Example:

```text
Change A:
Deprecate Requirement R

Change B:
Update Requirement R
```

### Dependency Conflict

A dependency conflict occurs when one change invalidates an element or relationship required by another change.

Example:

```text
Change A:
Deprecate Authentication Service

Change B:
Add Payment Service dependency on Authentication Service
```

### Invariant Conflict

An invariant conflict occurs when changes are individually valid but their combined result violates a rule of the semantic model.

Example:

```text
Constraint:
Payment must have at least one provider.

Change A:
Remove Stripe.

Change B:
Remove PayPal.
```

Each change may be valid independently.

Together they leave the payment capability without a provider and therefore violate the constraint.

## History

Changes form the history of the authoritative software knowledge model.

History records what happened to the software and preserves the relationships between previous and current semantic states.

History should preserve:

* What changed
* Why it changed
* Which semantic operations were applied
* Which knowledge was affected
* Which previous state or changes it depended on

Changes that have become part of persistent history should not be silently removed or rewritten.

Artifact effects may be associated with the authoritative change that caused them, but semantic history remains primary.

## Reversal and Compensation

Reversing a change does not erase the original change from history.

Instead, a new authoritative change may counteract or supersede some or all of its effects.

Example:

```text
Change A:
Authentication requires MFA

Change B:
Mandatory MFA requirement removed
```

Both changes remain part of the software's semantic history.

The current semantic state reflects the effect of Change B, while historical traceability preserves Change A.

Artifacts may then be rematerialized or reconciled from the resulting semantic state.

The exact semantics of local undo operations are outside the scope of this model.

## Open Questions

The following questions remain intentionally unresolved:

* Are operations inside a change always ordered?
* Can a change depend on multiple previous changes?
* Can a change exist in an unresolved conflicting state?
* Should effects be explicitly declared or derived?
* How are changes compared for semantic equivalence?
* Can changes be partially applied?
* How are concurrent changes reconciled?
* How are semantic states identified?
* How are historical changes persisted?
* When does a local change become persistent history?
* How is artifact divergence detected?
* How is an implementation-originated difference converted into an authoritative change?
