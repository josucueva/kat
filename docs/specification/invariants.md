# Invariants

## Purpose

Invariants define durable semantic properties that must remain true for any valid, accepted KAT knowledge state and its history.

They provide the normative domain rules governing the KAT semantic model.

## Identity Invariants

* Every knowledge element has a single, stable identity.
* The identity of a knowledge element must not change during its lifetime.
* Two distinct knowledge elements must not share the same identity.

## Lifecycle Invariants

* A deprecated element is no longer active in the current semantic state.
* A superseded element remains historically traceable and points to its successor.
* Deprecation or supersession must not erase the historical existence or properties of an element.

## Relationship Invariants

* Every accepted relationship references knowledge elements that exist in the accepted semantic state.
* An accepted semantic state contains at most one relationship for a given `(relationship_type, source_element_id, target_element_id)` triple.
* Every accepted relationship conforms to the ontology applicable to that accepted state.
* A relationship that is no longer active in the current state remains historically traceable through accepted change history.

## Traceability Invariants

* Knowledge relationships required for traceability remain navigable while present in the accepted semantic state.
* Evolution must not silently erase historical traceability.
* A knowledge element that depends on originating or supporting knowledge preserves enough information to trace that relationship.
* Traceability remains navigable across superseded and deprecated knowledge when historical explanation requires it.

## Authority Invariants

* The authoritative semantic model defines the intended state of the software.
* Physical artifacts must not independently redefine authoritative knowledge.
* An artifact that diverges from the authoritative semantic model remains identifiable as divergent until it is reconciled.
* Artifact modification must not silently produce an authoritative semantic change.

## Artifact Accountability Invariants

* Artifact accountability is defined exclusively through currently accepted direct `kat.core/represents` and `kat.core/derived-from` relationships.
* An artifact whose resolved accountability baseline differs from the current version of any direct accountability target is stale.
* An artifact with no direct accountability relationships in the current state is unaccounted.
* Re-accountability must be recorded explicitly in accepted change history.
* Re-accountability does not imply or require physical verification of disk files by KAT.
* Re-accountability does not delete, recreate, or alter the identity of accountability relationships.

## History Invariants

* Historical changes must not silently disappear.
* A persisted change remains identifiable in history.
* Reversing or compensating for a change must not erase the original change.
* History must preserve enough information to explain how the current semantic state was reached.
