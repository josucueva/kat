# KAT v0.4 Graph Quality Model

## Status

Draft.

This document defines the advisory graph quality diagnostic rules, finding structures, and integration semantics for KAT v0.4.

It is derived from:

- the v0.4 foundation documents ([`findings.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/findings.md), [`requirements.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/requirements.md), [`use-cases.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/use-cases.md), [`operations.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/operations.md), [`reference-model.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/reference-model.md));
- the interaction model ([`interaction-model.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/interaction-model.md)).

The central thesis of this document is:

> **Which small set of deterministic advisory findings materially helps `check` tell a developer that a mechanically valid graph may not route or explain software effectively?**

This document explicitly prevents `GraphQuality` from becoming an over-engineered, arbitrary graph-linting framework. It defines a small, evidence-driven set of 4 advisory diagnostic rules.

---

# 1. Fundamental Conceptual Distinctions

KAT v0.4 strictly separates repository status concepts:

$$\text{Mechanical Violation} \neq \text{Unverified Constraint} \neq \text{Stale Artifact} \neq \text{Graph Quality Finding}$$

```text
┌────────────────────────────────────────────────────────────────────────┐
│ MECHANICAL VIOLATION (kat validate) — FATAL                            │
│ Causes exit code 1. Broken repository integrity or illegal ontology    │
│ triples (e.g. unknown relationship type, disallowed endpoint types,   │
│ duplicate triples, missing endpoints).                                 │
├────────────────────────────────────────────────────────────────────────┤
│ UNVERIFIED CONSTRAINT (kat validate) — INFORMATIONAL                   │
│ Natural-language Constraint elements lacking an executable mechanical  │
│ evaluator. Reported for transparency (exit code 0).                    │
├────────────────────────────────────────────────────────────────────────┤
│ STALE ARTIFACT (kat artifacts) — ACCOUNTABILITY                       │
│ Artifact element whose recorded baseline target version differs from   │
│ current active element version in state. Fixed via kat account.        │
├────────────────────────────────────────────────────────────────────────┤
│ GRAPH QUALITY FINDING (kat quality) — ADVISORY                         │
│ Mechanically valid graph structures that may reduce traceability,      │
│ context retrieval, or explanatory usefulness (exit code 0).             │
└────────────────────────────────────────────────────────────────────────┘
```

> **Core Invariant**: A Graph Quality Finding shall **NEVER** prevent Change commit, invalidate a repository, or cause a non-zero CLI exit code. Quality findings are strictly advisory guidance.

---

# 2. Evidence-Driven Diagnostic Catalog

KAT v0.4 defines exactly 4 evidence-driven graph quality rules:

```text
GQ-01: IsolatedElement
GQ-02: RequirementWithoutRealizationPath
GQ-03: ImplementationWithoutArtifactRoute
GQ-04: DesignDecisionWithoutConsequencePath
```

---

## GQ-01: IsolatedElement

### Observed Condition
An active Knowledge Element exists in accepted state $S_{\text{accepted}}$ with **zero** active incoming or outgoing relationships.

### Rationale
An isolated element cannot participate in context retrieval (`Context`), rationale tracing (`Trace`), or impact analysis (`Impact`). It exists in semantic isolation.

### Ontology Rule
Evaluated for all active Knowledge Element types.

### Impact Explanation
> *"Element '{title}' has no relationships. It will not be reached during context retrieval or impact analysis."*

---

## GQ-02: RequirementWithoutRealizationPath

### Observed Condition
An active Requirement element (`kat.core/requirement`) has no active downstream realization path to an Implementation element (`kat.core/implementation`) via `kat.core/realizes` edges.

### Rationale
A requirement that is not linked to any realizing implementation cannot route developers or agents to physical code.

### Ontology Rule
Evaluated for active elements of type `kat.core/requirement`. Checks whether a path exists:

$$\text{Requirement} \xleftarrow{\text{kat.core/realizes}} \text{Implementation}$$

### Impact Explanation
> *"Requirement '{title}' has no realizing Implementation. Context queries rooted here will not route to implementation code."*

---

## GQ-03: ImplementationWithoutArtifactRoute

### Observed Condition
An active Implementation element (`kat.core/implementation`) has no active representation link to an Artifact element (`kat.core/artifact`) via `kat.core/represents` edges.

### Rationale
An implementation responsibility that is not represented by any Artifact anchor leaves a gap between semantic responsibility and physical source files.

### Ontology Rule
Evaluated for active elements of type `kat.core/implementation`. Checks whether a link exists:

$$\text{Implementation} \xleftarrow{\text{kat.core/represents}} \text{Artifact}$$

### Impact Explanation
> *"Implementation '{title}' has no mapped Artifact anchor. Developers cannot navigate from this responsibility to physical source files."*

---

## GQ-04: DesignDecisionWithoutConsequencePath

### Observed Condition
An active Design Decision element (`kat.core/design-decision`) has no active outgoing relationship (`kat.core/guides`, `kat.core/addresses`) to an Implementation or Requirement.

### Rationale
An architectural decision that neither addresses a requirement nor guides an implementation provides no observable semantic consequence in the graph.

### Ontology Rule
Evaluated for active elements of type `kat.core/design-decision`. Checks whether outgoing links exist:

$$\text{Design Decision} \xrightarrow{\text{kat.core/addresses}} \text{Requirement} \quad \lor \quad \text{Design Decision} \xrightarrow{\text{kat.core/guides}} \text{Implementation}$$

### Impact Explanation
> *"Design Decision '{title}' has no outgoing consequence edges. Its architectural impact cannot be traced downstream."*

---

# 3. Graph Quality Severity Levels

Quality findings assign an advisory severity level:

- **`High`**: Directly breaks physical context routing (e.g. `GQ-03: ImplementationWithoutArtifactRoute`).
- **`Medium`**: Breaks semantic traceability or realization paths (e.g. `GQ-02: RequirementWithoutRealizationPath`, `GQ-04: DesignDecisionWithoutConsequencePath`).
- **`Low`**: Structural isolation that does not corrupt existing paths (e.g. `GQ-01: IsolatedElement`).

Severity levels guide developer prioritization; they do not alter command behavior.

---

# 4. Finding DTO Structure

```text
GraphQualityReport {
    repository_id: RepositoryId,
    accepted_state_id: ObjectId,
    total_findings_count: usize,
    findings: Vec<GraphQualityFinding>,
}

GraphQualityFinding {
    code: FindingCode, // IsolatedElement | RequirementWithoutRealizationPath | ...
    severity: QualitySeverity, // High | Medium | Low
    affected_element_ids: Vec<ElementId>,
    summary: String,
    impact_explanation: String,
}
```

Concrete serialization schemas for machine consumption are defined in [`machine-interface.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/machine-interface.md).

---

# 5. Integration into Porcelain Health Check (`check`)

The porcelain health operation (`check`) aggregates repository status into 4 distinct sections:

```text
========================================================================
                      KAT REPOSITORY HEALTH CHECK
========================================================================

1. MECHANICAL CONSISTENCY (kat validate)
   Status: PASS (0 violations)

2. EVIDENCE COVERAGE (kat validate --coverage)
   Validatable Categories: 4/4 covered (100%)
   Uncovered Elements: 0

3. ARTIFACT ACCOUNTABILITY (kat artifacts --stale)
   Total Artifacts: 21 | Current: 21 | Stale: 0 | Unaccounted: 0

4. ADVISORY GRAPH QUALITY (kat quality)
   Advisory Findings: 2 (0 High, 1 Medium, 1 Low)
   - [Medium] GQ-02: Requirement 'Offline Mode' has no realizing Implementation.
   - [Low]    GQ-01: Element 'Scratch Note' is isolated (0 edges).

Note: Advisory graph quality findings do not invalidate the repository.
```

This single porcelain operation answers *"Is my repository healthy?"* without forcing developers to learn four separate underlying command flags.

---

# 6. Invariants

## INV-QUAL-01: Non-Fatal Advisory Semantics
Graph quality evaluation is strictly advisory. Findings shall never cause CLI command failure or block `kat change commit`.

## INV-QUAL-02: Accepted-State Isolation
`GraphQuality` operates read-only over accepted repository state $S_{\text{accepted}}$.

## INV-QUAL-03: Ontology Extensibility
Diagnostic rules check relationship predicates dynamically against `OntologyVersion`, ensuring core rules operate cleanly alongside extension ontologies.

---

# 7. Next Specification Stage

The next document in the specification sequence is:

```text
docs/implementation/v0.4/machine-interface.md
```

It shall define:
- concrete JSON DTO schemas for porcelain commands (`context`, `status`, `author`, `check`, `commit`);
- concrete JSON DTO schemas for mutation responses;
- machine error envelope structures.
