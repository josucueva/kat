# Materialization Model

## Purpose

The materialization model defines how authoritative software knowledge is realized through concrete artifacts.

KAT follows a specification-first model. The semantic model defines the intended state of the software, while artifacts represent, implement, validate, or materialize that knowledge.

Materialization defines the relationship between those two levels.

---

## Scope and v0.2 Boundary

Materialization is a conceptual model for how semantic knowledge may be realized as artifacts.

KAT v0.2 does not implement materializers, physical artifact verification, automatic divergence detection, or reverse artifact-to-specification inference.

Instead, KAT v0.2 implements **semantic artifact accountability**: tracking direct semantic relationships (`kat.core/represents`, `kat.core/derived-from`) and explicitly reconciling accountability baselines in accepted history.

---

## Semantic Knowledge and Artifacts

Materialization flows conceptually from authoritative knowledge toward artifacts.

```text
Authoritative Knowledge (Intent / Requirement / Constraint / Decision)
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

Materialization does not make the produced artifact authoritative.

Changes to a physical artifact do not independently redefine the intended software state.

---

## Implementation vs Artifact

Implementation and Artifact represent different levels of the software model.

### Implementation
`Implementation` represents semantic knowledge about how intended software behavior or design is realized.

Examples:
* Payment processing component
* Authentication mechanism
* Refund workflow
* Persistence implementation

Implementation is a semantic concept and is not tied to a specific file or physical representation.

### Artifact
An `Artifact` is a concrete representation or output associated with software knowledge.

Examples:
* Source files
* OpenAPI documents
* Test files
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

---

## Artifact Relationships

Artifacts remain traceable to the knowledge they represent or originate from through explicit typed relationships in the ontology.

### represents
`represents` asserts that an Artifact element is a concrete representation of an `Implementation` element.

```text
Artifact -> Implementation
```

Example:

```text
payment_service.rs (Artifact)
    represents
Payment Processing (Implementation)
```

The artifact is a concrete representation of that implementation concept.

### derived-from
`derived-from` indicates that an Artifact element originated from, was produced by, or was shaped by software knowledge.

```text
Artifact -> Requirement
Artifact -> Constraint
Artifact -> Design Decision
Artifact -> Implementation
```

Example:

```text
openapi.yaml (Artifact)
    derived-from
Expose refund operation (Requirement)
```

An artifact may both `represents` an implementation concept and `derived-from` related requirements or design decisions.

---

## Materialization Modes (Future Conceptual Taxonomy)

KAT recognizes different conceptual ways in which artifacts may be materialized:

* **Deterministic Materialization**: The artifact is produced automatically from software knowledge using defined materialization rules.
* **Assisted Materialization**: Software knowledge is used to propose an artifact, but human review or modification completes the process.
* **Manual Materialization**: A developer manually creates or modifies an artifact according to authoritative software knowledge.

> **v0.2 Model Boundary**: KAT v0.2 does not persist materialization mode metadata in canonical objects. Materialized does not imply automatically generated; all artifacts are modeled uniformly as `kat.core/artifact` elements.

---

## Artifact Accountability

KAT v0.2 does not determine physical artifact correctness or inspect physical file contents.

Instead, it evaluates semantic accountability between `Artifact` elements and the target knowledge elements they directly reference.

Accountability status is categorized as:

* `CURRENT`: All direct accountability baselines match current target element versions.
* `STALE`: At least one direct accountability baseline differs from the current target element version, or a target element's lifecycle state has become invalid (`Deprecated` or `Superseded`).
* `UNACCOUNTED`: No direct accountability relationship exists for the artifact in the current accepted state $S_n$.

`CURRENT` status indicates semantic baseline alignment; it does not imply that the physical artifact has been inspected or verified on disk.

---

## Accountability Baselines & AccountArtifact

When a direct accountability relationship (`kat.core/represents`, `kat.core/derived-from`) is created, its initial accepted baseline is established by the target element version selected by that state.

When target elements evolve ($V_{\text{initial}} \to V_{\text{next}}$), the artifact's accountability status becomes `STALE`.

To acknowledge the target element evolution and re-baseline the artifact, a user submits an `AccountArtifact` operation:

```text
Target Element Update (V1 -> V2)
        |
        v
Artifact Status becomes STALE
        |
        v
User invokes AccountArtifact
        |
        v
New Baseline (V2) Recorded in Accepted History
        |
        v
Artifact Status returns to CURRENT
```

The `AccountArtifact` operation records explicit target version reconciliations in accepted `ChangeRevision` history without mutating the candidate `SemanticState` $S_{\text{working}}$.

---

## Physical Artifact Divergence

KAT v0.2 does not inspect physical artifact contents and therefore does not determine whether a physical artifact has diverged from the semantic model.

Physical divergence may exist externally (e.g. uncommitted local code edits), but KAT only evaluates and reports semantic accountability status from accepted relationships and baselines.

---

## Materialization and Validation

Materialization and validation are separate concerns in the KAT model.

* **Materialization** answers: *How is this knowledge concretely represented or realized?*
* **Validation** answers: *Does that realization or knowledge satisfy expected requirements, constraints, or properties?*

```text
Knowledge
    |
    v
Materialization (Artifact)
    |
    v
Validation (Validation Result)
```

Successfully materializing or accounting an artifact does not by itself prove that the resulting artifact is valid.

---

## Materialization and Semantic Change

Semantic changes to authoritative knowledge may create artifact accountability effects:

```text
Semantic Change (e.g. Require MFA)
        |
        v
Target Requirement / Decision Updated
        |
        v
Accountability status of related Artifacts becomes STALE
        |
        v
External source files updated physically
        |
        v
AccountArtifact committed -> Baselines Reconciled
```

Artifact updates do not directly mutate authoritative knowledge. Any intended change to software specification must be submitted as a formal Change through the KAT change model.

---

## Future Materialization Capabilities

The following capabilities represent potential future extensions to the materialization model:

* **Materialization Rules & Generators**: Defining executable materialization rules or plugins (e.g., OpenAPI generators, Infrastructure-as-Code emitters) driven by semantic state.
* **Physical Inspection & Verification**: Tools for hashing or analyzing physical source files to detect physical artifact drift against recorded baselines.
* **Reverse Reconciliation**: Tooling to infer or propose semantic specification changes from observed physical code diffs.

---

## Core Rules

The materialization model enforces the following normative rules:

* Artifacts are semantic knowledge elements, not authoritative specifications.
* `represents` and `derived-from` provide direct artifact accountability.
* Materialized does not imply automatically generated.
* KAT v0.2 does not inspect physical artifact contents.
* Accountability status is distinct from physical verification.
* Semantic changes may make artifact accountability `STALE`.
* `AccountArtifact` explicitly reconciles accepted accountability baselines in history.
* `AccountArtifact` does not change authoritative `SemanticState`.
* Physical artifact changes do not directly mutate authoritative knowledge.
* Any intended semantic evolution must pass through the normal Change model.

---

## Future Research Questions

The following topics remain open for future research:

* How will executable materialization plugins be configured and versioned?
* How can physical artifact verification bridges integrate with build systems and CI/CD pipelines?
* What protocol will govern automated reverse reconciliation from source code diffs to proposed semantic Changes?
