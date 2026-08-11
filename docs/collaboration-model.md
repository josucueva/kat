# Collaboration Model

## Purpose

The collaboration model defines how multiple participants may evolve the same KAT software model without reducing collaboration to file-based merging.

Collaboration operates on semantic changes to authoritative software knowledge.

The model defines:

* Local and shared work
* Concurrent changes
* Change dependencies
* Compatibility
* Reconciliation
* Semantic conflicts
* Conflict resolution
* Accepted and working states
* Collaborative history

The collaboration model does not define network protocols, synchronization algorithms, distributed storage, locking strategies, or user interface behavior.

## Shared Semantic State

Collaboration begins from a semantic state known to the participants involved.

Conceptually:

```text
Shared Semantic State S0
        |
        +--------------------+
        |                    |
        v                    v
Developer A             Developer B
```

Participants may independently propose changes based on the same state.

```text
Shared Semantic State S0
        |
        +--------------------+
        |                    |
        v                    v
Change A                Change B
```

The shared semantic state remains authoritative until new changes are accepted.

## Local Change

A local change is a change being developed by a participant that has not yet become part of the accepted shared semantic history.

A local change follows the same change model as any other KAT change.

It may contain:

* One or more mutation operations
* Preconditions
* Postconditions
* Semantic effects
* Dependencies on previous changes

A local change may evolve while it remains local.

Once accepted into persistent shared history, its historical identity and meaning must remain traceable.

## Shared Change

A shared change is a change that has been incorporated into the collaborative history of the software model.

A shared change must satisfy the requirements of the change model and all required invariants.

Acceptance of a shared change produces or contributes to an accepted semantic state.

```text
Accepted State S0
        |
        | Change A
        v
Accepted State S1
```

A change becoming shared does not imply that every participant must immediately materialize its artifact effects.

## Concurrent Changes

Changes are concurrent when they are developed independently without one depending on the other.

Example:

```text
        State S0
       /        \
      v          v
 Change A      Change B
```

Concurrency does not imply conflict.

Two concurrent changes may be:

* Independent
* Compatible
* Order-dependent
* Conflicting

Their relationship must be determined from their semantics, dependencies, preconditions, effects, and invariants.

## Causality

Collaboration preserves causal relationships between changes.

If one change requires knowledge introduced by another, the dependent change cannot be meaningfully incorporated before its dependency.

Example:

```text
Change A:
Create Requirement R

Change B:
Create Design Decision D addressing R
```

Then:

```text
Change A
    |
    v
Change B
```

Change B causally depends on Change A.

Changes are ordered when causality requires it.

Independent changes do not require an arbitrary semantic ordering.

## Compatibility

Two changes are compatible when they can both become part of an accepted semantic state without violating:

* Their preconditions
* Their intended semantic effects
* Required relationships
* Ontology rules
* Invariants

Compatible changes may still require a particular application order.

Example:

```text
Change A:
Create Requirement R

Change B:
Update unrelated Constraint C
```

These changes may be independent and compatible.

Another example:

```text
Change A:
Create Requirement R

Change B:
Create Decision D addressing R
```

These changes may be compatible but order-dependent because Change B depends on Change A.

## Reconciliation

Reconciliation is the process of determining how multiple changes can be incorporated into a shared semantic state.

Conceptually:

```text
Change A
     \
      \
       > Reconciliation
      /
     /
Change B
```

Reconciliation evaluates:

* Change dependencies
* Preconditions
* Postconditions
* Semantic effects
* Compatibility
* Ontology rules
* Invariants

The result may be:

* Both changes can be accepted
* The changes require a specific order
* One change must be adjusted
* A semantic conflict exists

Reconciliation operates on semantic meaning rather than textual file differences.

## Compatible Reconciliation

When concurrent changes are compatible, KAT may incorporate both into a new shared semantic state.

```text
        State S0
       /        \
      v          v
 Change A      Change B
       \        /
        \      /
         v    v
        State S1
```

The resulting state must satisfy all required invariants.

The exact execution or synchronization mechanism used to produce the resulting state is outside the scope of this model.

## Conflict

A semantic conflict occurs when multiple changes cannot be incorporated into the same accepted semantic state without violating their intended semantics or the rules of the model.

Conflict detection should use the conflict definitions established by the change model.

Initial conflict types include:

* Write conflict
* Lifecycle conflict
* Dependency conflict
* Invariant conflict

A conflict is not defined merely by multiple participants changing the same artifact or file.

## Write Conflict

A write conflict occurs when concurrent changes assign incompatible values to the same semantic property.

Example:

```text
Change A:
Requirement.priority = High

Change B:
Requirement.priority = Critical
```

The conflict concerns the intended value of the Requirement property, not the textual location in which it may later be represented.

## Lifecycle Conflict

A lifecycle conflict occurs when one change alters the lifecycle of knowledge in a way that invalidates another change.

Example:

```text
Change A:
Deprecate Requirement R

Change B:
Update Requirement R
```

The changes cannot both be accepted without resolving the intended lifecycle of Requirement R.

## Dependency Conflict

A dependency conflict occurs when one change removes or invalidates knowledge required by another.

Example:

```text
Change A:
Deprecate Authentication Implementation

Change B:
Add Payment Implementation dependency on Authentication Implementation
```

The conflict is determined from the semantic dependency rather than artifact overlap.

## Invariant Conflict

An invariant conflict occurs when changes are individually valid but their combined state violates a required invariant.

Example:

```text
Constraint:
Payment must have at least one provider.

Change A:
Remove Stripe provider.

Change B:
Remove PayPal provider.
```

Each change may be valid independently.

Their combination violates the constraint.

## Working State

A working state represents semantic work that has not yet been accepted as part of the authoritative shared state.

A working state may contain:

* Local changes
* Proposed changes
* Unvalidated changes
* Changes awaiting reconciliation

A working state does not replace the accepted semantic state.

This distinction allows collaboration to proceed without treating incomplete work as authoritative.

## Accepted State

An accepted state is a semantic state recognized as authoritative within the shared software history.

An accepted state must satisfy:

* Ontology rules
* Required invariants
* Change preconditions and postconditions
* Required consistency rules

Unresolved semantic conflicts must not silently become part of an accepted state.

Whether KAT may explicitly represent a conflicted working state remains an open question.

## Conflict Resolution

Conflict resolution is the process of producing an acceptable semantic outcome from conflicting changes.

Resolution may involve:

* Selecting one intended value
* Modifying one or more changes
* Creating a new change that reconciles competing intent
* Replacing or superseding conflicting knowledge
* Rejecting a proposed change

Conflict resolution should preserve the semantic intent involved where possible.

Resolution must produce a state that satisfies the ontology and required invariants before becoming accepted.

## Resolution as Knowledge

A meaningful conflict resolution may itself represent new software knowledge.

For example:

```text
Change A:
Use REST

Change B:
Use gRPC
```

A resolution might introduce:

```text
Design Decision:
Use gRPC for internal communication while preserving REST externally.
```

This is not merely a mechanical merge.

It is a new design decision and should be represented as such.

## Rejected Changes

A proposed change may be rejected during collaboration.

Rejection means that the change does not become part of the accepted semantic state.

If the change has already entered persistent collaborative history, its rejection or replacement should remain traceable rather than silently removing its existence.

The exact lifecycle of rejected local changes is outside the scope of this model.

## Superseding Collaborative Changes

A later change may replace or counteract knowledge introduced by an earlier shared change.

This follows the normal change and history models.

Example:

```text
Change A:
Introduce REST API design.

Change B:
Supersede REST design with gRPC.
```

Both changes remain historically traceable.

Collaboration does not require rewriting earlier accepted history in order to represent later evolution.

## Collaboration and Artifacts

Collaboration is defined primarily over authoritative semantic changes.

Artifact modifications may occur during local work, but artifact overlap does not by itself determine whether semantic changes conflict.

For example, two developers may modify the same source file while changing unrelated semantic knowledge.

Conversely, two developers may modify different artifacts while introducing conflicting semantic changes.

Therefore:

```text
Artifact conflict != Semantic conflict
```

Artifact differences should be interpreted through their relationship to authoritative knowledge.

The materialization and reconciliation models govern how artifact changes are associated with semantic changes.

## Collaboration and Materialization

Accepted semantic changes may produce artifact effects.

Participants may materialize those effects independently or through shared tooling.

Conceptually:

```text
Shared Semantic Change
        |
        v
Affected Knowledge
        |
        v
Artifact Effects
        |
        v
Materialization
```

The state of materialized artifacts does not determine whether the semantic change itself is authoritative.

Artifact consistency remains a separate concern.

## Collaboration History

Collaborative history records the semantic evolution produced by accepted changes.

History should preserve:

* Change identity
* Causal dependencies
* Knowledge affected
* Relationships between changes
* Reconciliation outcomes when relevant
* Supersession or compensation
* The semantic states produced by accepted evolution

History should preserve causality rather than requiring every independent change to be interpreted as part of one artificial linear sequence.

The exact physical representation of collaborative history is an implementation concern.

## Collaboration Guarantees

A collaboration mechanism compatible with KAT should preserve the following properties:

* Authoritative knowledge remains specification-first.
* Changes retain stable identity once persisted.
* Causal dependencies remain traceable.
* Compatible changes may coexist.
* Semantic conflicts remain explicit.
* Conflicts are not reduced to file overlap.
* Required invariants hold for accepted states.
* Conflict resolution does not silently erase history.
* Artifact modifications do not independently redefine authoritative knowledge.
* Collaboration semantics remain independent from the synchronization technology used.

## Core Flow

The collaboration model can be summarized as:

```text
              Accepted State S0
                     |
          +----------+----------+
          |                     |
          v                     v
      Change A               Change B
          |                     |
          +----------+----------+
                     |
                     v
               Reconciliation
                     |
          +----------+----------+
          |                     |
          v                     v
      Compatible             Conflict
          |                     |
          v                     v
   Validate Result       Resolve Meaning
          |                     |
          +----------+----------+
                     |
                     v
              Accepted State S1
```

## Open Questions

The following questions remain intentionally unresolved:

* Can KAT persist unresolved conflicted working states?
* When does a local change become shared?
* Can a shared change be modified before final acceptance?
* How are participants identified?
* How are permissions and authorization represented?
* Can reconciliation involve more than two concurrent changes?
* How are changes exchanged between repositories?
* How are missing causal dependencies obtained?
* How are concurrent changes discovered?
* How are equivalent independently created changes recognized?
* How are rejected changes represented in persistent history?
* How are artifact-level conflicts coordinated with semantic conflicts?
* What synchronization model should be used between repositories?
* What guarantees are required when collaboration occurs offline?

