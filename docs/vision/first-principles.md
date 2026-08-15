# First Principles

First principles define the fundamental assumptions on which KAT is built.

They describe properties that should remain true regardless of the implementation, storage model, interface, architecture, or technologies used by KAT.

## 1. Software is more than its artifacts

Software is not equivalent to its source code or any other individual artifact.

A software system includes the intent, requirements, constraints, decisions, implementations, validations, relationships, and artifacts that give the system its purpose and behavior.

Artifacts are representations or realizations of this broader body of software knowledge.

## 2. The specification is authoritative

The intended state of software is defined by its specification and represented through KAT's semantic model.

The semantic model is the machine-processable representation of that authoritative knowledge.

```text
Specification
      |
      v
Semantic Model
      |
      v
Artifacts
```

Source code, documentation, tests, configurations, and other artifacts may realize or validate the specification, but they do not independently replace it as the source of truth.

## 3. Knowledge must remain traceable

Software knowledge must preserve meaningful relationships across different levels of abstraction and throughout the evolution of the system.

It should be possible to trace both forward and backward.

```text
Intent
    |
    v
Requirement
    |
    v
Decision
    |
    v
Implementation
    |
    v
Artifact
    |
    v
Validation
```

Traceability should make it possible to understand:

* Why something exists
* What originated it
* What depends on it
* What may be affected by changing it
* How it is validated

## 4. Software evolves through changes to knowledge

Software is continuously evolving.

Its evolution should be represented as meaningful changes to authoritative software knowledge rather than primarily as modifications to files.

A change transforms one accepted semantic state into another.

```text
Semantic State A
        |
        | Change
        v
Semantic State B
```

Changes to artifacts are consequences of this evolution or inputs that must be reconciled with the authoritative model.

## 5. Knowledge must be actionable

Knowledge captured by KAT must not exist only for documentation or visualization.

It should participate in the operation of the software lifecycle.

Captured knowledge should be usable for activities such as:

* Traceability
* Impact analysis
* Consistency validation
* Change analysis
* Explanation
* Materialization

A semantic model that can only be inspected but cannot influence software evolution is insufficient for KAT.

## 6. Evolution must preserve history

The current state of software is not sufficient to explain the software system.

The evolution that produced that state is also part of its knowledge.

KAT must preserve enough historical information to understand:

* What changed
* Why it changed
* What knowledge was affected
* What previous knowledge it depended on
* How the current state was reached

Later changes may supersede or counteract earlier changes, but they should not silently erase their historical existence.

## 7. Consistency must be explicit and verifiable

The validity of a semantic state should be determined through explicit knowledge, relationships, constraints, and validation rules.

KAT should not rely solely on implicit assumptions about whether the software is consistent.

When knowledge evolves, affected parts of the system should be identifiable and their consistency should be capable of reevaluation.

## 8. Artifacts must remain accountable to knowledge

Artifacts should be explainable through the authoritative semantic model.

When an artifact contains behavior, structure, or constraints that cannot be explained by the current knowledge, a divergence exists.

```text
Authoritative Knowledge
        |
        v
Expected Artifacts

        !=

Actual Artifacts
        |
        v
Divergence
```

Artifact divergence should be detectable and reconciled rather than silently becoming a new source of truth.

This principle allows KAT to remain specification-first while still supporting handwritten implementations and legacy software.

## 9. Human intent and decisions remain fundamental

KAT may assist in capturing, deriving, validating, or transforming software knowledge, but it does not replace the intent and decisions that define the software system.

Automation and AI may operate on the semantic model, but they are mechanisms for working with knowledge rather than authorities that define what the software should be.
