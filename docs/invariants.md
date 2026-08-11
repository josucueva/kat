# Invariants

## Purpose

Invariants define conditions that must remain true for an accepted KAT semantic state.

They provide the basis for consistency validation and semantic conflict detection.

An invariant describes a property of the model that must not be violated by a successful change.

## Identity Invariants

* Every knowledge element must have a stable identity.
* The identity of a knowledge element must not change during its lifetime.
* Two distinct knowledge elements must not share the same identity.

## Relationship Invariants

* Every relationship must reference existing knowledge elements.
* Every relationship must have a defined relationship type.
* The source and target element types must be valid for the relationship according to the ontology.
* A relationship that is no longer active may remain historically traceable.
* Invalid relationships must not become part of an accepted semantic state.

## Lifecycle Invariants

* A deprecated element must not be treated as active.
* A superseded element must remain historically traceable.
* A superseded element must preserve its relationship to its successor.
* Deprecation or supersession must not erase the historical existence of an element.

## Change Invariants

* A change must satisfy its required preconditions before it can be successfully applied.
* A successful change must satisfy its required postconditions.
* A successful change must produce a valid semantic state.
* A change that violates a required invariant must not become part of the accepted semantic state.
* A change must not introduce semantic effects that cannot be identified or traced.

## Traceability Invariants

* Knowledge relationships required for traceability must remain navigable while they are part of the accepted semantic state.
* Evolution must not silently remove historical traceability.
* A knowledge element that depends on originating or supporting knowledge must preserve enough information to trace that relationship.
* Traceability must remain possible across superseded and deprecated knowledge when historical explanation requires it.

## Authority Invariants

* The authoritative semantic model defines the intended state of the software.
* Artifacts must not independently redefine authoritative knowledge.
* An artifact that diverges from the authoritative semantic model must remain identifiable as divergent until it is reconciled.
* Artifact modification must not silently produce an authoritative semantic change.
* Reconciliation of artifact divergence must result in an explicit change to authoritative knowledge when the intended software state is modified.

## Validation Invariants

* An accepted semantic state must satisfy all required invariants.
* Validation must evaluate the semantic model without silently modifying it.
* A reported consistency violation must identify the affected knowledge when that information is available.
* Validation evidence must reference existing knowledge elements that it validates.

## History Invariants

* Historical changes must not silently disappear.
* A persisted change must remain identifiable in history.
* Reversing or compensating for a change must not erase the original change.
* The relationship between historical changes and the semantic states they affected must remain traceable.
* History must preserve enough information to explain how the current semantic state was reached.
