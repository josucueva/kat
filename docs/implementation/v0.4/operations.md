# KAT v0.4 Operations

## Status

Draft.

This document defines the semantic operations and queries available in KAT v0.4.

It is derived from:

- the v0.4 findings and problems;
- the v0.4 requirements;
- the v0.4 use cases.

This document defines what KAT can do at the semantic repository level.

It does not define:

- CLI syntax;
- reference-resolution syntax;
- workflow-local aliases;
- batch submission formats;
- structured machine-output formats;
- presentation modes;
- agent-specific behavior.

Those concerns are specified separately.

---

# 1. Operation Categories

KAT v0.4 defines two operation categories:

1. Mutation Operations
2. Query Operations

Mutation Operations participate in semantic evolution and may change repository state.

Query Operations inspect repository state without modifying it.

---

# 2. Mutation Operations

The mutation operation set remains unchanged from v0.3.1.

KAT v0.4 defines the following mutation operations:

```text
CreateElement
UpdateElement
DeprecateElement
SupersedeElement
Link
Unlink
AccountArtifact
```

These operations continue to participate in explicit Change semantics.

No new mutation operation is introduced by v0.4.

---

# 3. CreateElement

## Purpose

Create a new Knowledge Element with a new stable ElementId.

## Inputs

Conceptually:

```text
element_type
title
description?
properties?
```

## Semantic Effect

A new immutable `KnowledgeElementVersion` is created.

A new stable `ElementId` is assigned.

The candidate SemanticState selects:

```text
ElementId -> KnowledgeElementVersion ObjectId
```

## Preconditions

- the element type exists in the active ontology;
- the element representation is structurally valid;
- properties conform to the canonical PropertyValue model;
- the repository is writable through the current Change workflow.

## Result

The operation produces:

```text
new ElementId
new KnowledgeElementVersion ObjectId
updated candidate SemanticState
```

## Failure Conditions

- unknown element type;
- invalid element representation;
- invalid property representation;
- repository or draft failure.

---

# 4. UpdateElement

## Purpose

Create a new immutable version of an existing Knowledge Element while preserving its stable ElementId.

## Inputs

Conceptually:

```text
target ElementId
updated title?
updated description?
updated properties?
```

## Semantic Effect

If the candidate state contains:

```text
ElementId -> old ObjectId
```

then the operation creates a new `KnowledgeElementVersion` and updates the candidate state to:

```text
ElementId -> new ObjectId
```

The previous version remains immutable.

## Preconditions

- the target element exists in the working candidate state;
- the new version is structurally valid;
- the target remains compatible with the active ontology.

## Result

```text
same ElementId
new KnowledgeElementVersion ObjectId
updated candidate SemanticState
```

## Failure Conditions

- unknown target element;
- invalid new version;
- repository or draft failure.

---

# 5. DeprecateElement

## Purpose

Create a new version of an existing Knowledge Element whose lifecycle state is `Deprecated`.

## Semantic Effect

Stable identity is preserved.

The candidate SemanticState selects the newly created deprecated version.

## Preconditions

- the target element exists in the working candidate state;
- the lifecycle transition is valid.

## Result

```text
same ElementId
new deprecated KnowledgeElementVersion
updated candidate SemanticState
```

## Failure Conditions

- unknown target element;
- invalid lifecycle transition;
- structural validation failure.

---

# 6. SupersedeElement

## Purpose

Represent explicit supersession of an existing Design Decision according to KAT's existing semantics.

## Semantic Effect

The operation preserves the established supersession behavior, including:

- creation of the relevant new element version;
- lifecycle evolution where applicable;
- explicit semantic supersession relation.

## Preconditions

- source and target satisfy the current supersession rules;
- relevant elements exist in the working candidate state;
- ontology constraints are satisfied.

## Result

The candidate SemanticState reflects the supersession operation and any associated relationship/version changes.

## Failure Conditions

- unknown element;
- invalid element type;
- invalid supersession relation;
- candidate validation failure.

---

# 7. Link

## Purpose

Create an explicit semantic Relationship between two Knowledge Elements.

## Inputs

Conceptually:

```text
relationship_type
source ElementId
target ElementId
description?
```

## Semantic Effect

A new stable `RelationshipId` and immutable `RelationshipVersion` are created.

The candidate SemanticState selects:

```text
RelationshipId -> RelationshipVersion ObjectId
```

## Preconditions

- the relationship type exists in the active ontology;
- source element exists;
- target element exists;
- source type is allowed for the relationship;
- target type is allowed for the relationship;
- the relationship does not violate duplicate-triple rules.

## Result

```text
new RelationshipId
new RelationshipVersion ObjectId
updated candidate SemanticState
```

## Failure Conditions

- unknown relationship type;
- missing source;
- missing target;
- invalid source type;
- invalid target type;
- duplicate relationship triple;
- structural validation failure.

---

# 8. Unlink

## Purpose

Remove an active Relationship from the candidate SemanticState.

## Inputs

Conceptually:

```text
RelationshipId
```

## Semantic Effect

The selected RelationshipVersion is removed from the candidate SemanticState.

The canonical Relationship object itself remains immutable in object storage.

## Preconditions

- the relationship exists in the working candidate state.

## Result

```text
updated candidate SemanticState
```

## Failure Conditions

- unknown relationship;
- repository or draft failure.

---

# 9. AccountArtifact

## Purpose

Record semantic reconciliation between an Artifact and the current versions of its direct accountability targets.

## Inputs

Conceptually:

```text
artifact_id
reconciliations[]
```

where each reconciliation contains:

```text
relationship_id
expected_relationship_version
target_element_id
reconciled_target_version
```

## Semantic Effect

`AccountArtifact` does not modify SemanticState.

It is recorded in the accepted `ChangeRevision` and updates the effective accountability baseline for the Artifact.

## Preconditions

- the Artifact exists in the working candidate state;
- the Artifact is Active;
- the element type is `kat.core/artifact`;
- the Artifact has at least one direct accountability relationship;
- all direct accountability relationships are reconciled;
- reconciliation relationship versions match the candidate state;
- reconciliation target versions match the candidate state;
- at least one effective target baseline changes.

## Result

The resulting ChangeRevision records the new accountability baseline.

The Change may result in:

```text
result_state == base_state
```

when `AccountArtifact` is the only operation.

## Failure Conditions

- unknown Artifact;
- Artifact not active;
- Artifact has no direct accountability relationship;
- incomplete reconciliation;
- stale relationship version;
- stale target version;
- invalid reconciliation ordering or duplication;
- no baseline change.

---

# 10. Mutation Invariants

All mutation operations preserve the following semantics.

## 10.1 Explicit Change Membership

Mutations occur through the existing Change workflow.

## 10.2 Ordered Evaluation

Operations are evaluated in declared order.

## 10.3 Working Candidate State

Each operation observes the state produced by all preceding operations in the same Change.

## 10.4 Candidate Validation

The resulting candidate must satisfy mechanical repository invariants before acceptance.

## 10.5 Atomic Acceptance

Accepted repository state changes only after successful Change commit.

## 10.6 Immutability

Existing canonical object versions are never modified.

New semantic versions produce new ObjectIds.

---

# 11. Query Operations

KAT v0.4 defines the following query operations:

```text
List
Show
Status
Trace
Impact
History
ArtifactAccountability
Ontology
Validate
Context
GraphQuality
```

The first nine preserve their existing semantic role.

`Context` and `GraphQuality` are new v0.4 query capabilities.

---

# 12. List

## Purpose

List active Knowledge Elements in the accepted SemanticState.

## Inputs

Conceptually:

```text
optional element-type filter
optional other supported filters
```

## Result

A deterministic ordered collection of matching active Knowledge Elements.

## State Semantics

Operates on accepted repository state.

---

# 13. Show

## Purpose

Inspect one active Knowledge Element and its selected version.

## Inputs

```text
ElementId
```

## Result

The query exposes the selected Knowledge Element version and relevant semantic information associated with that element.

This may include its active incoming and outgoing relationships.

## State Semantics

Operates on accepted repository state.

---

# 14. Status

## Purpose

Inspect repository status and, where applicable, explicit draft status.

## Result

Repository status may include:

```text
accepted state information
active draft presence
staged operation count
candidate effect
candidate accountability
candidate validation
```

## State Semantics

Accepted repository information and draft information remain explicitly distinguished.

---

# 15. Trace

## Purpose

Trace a Knowledge Element toward its semantic origin or rationale.

## Inputs

Conceptually:

```text
root ElementId
optional max depth
```

## Semantics

Trace preserves the existing v0.3 traversal rules:

- accepted-state isolation;
- deterministic traversal;
- path-local cycle prevention;
- optional traversal bound;
- shared-prefix tree representation;
- explicit exhaustive paths when requested at the presentation layer.

## Result

A deterministic trace result containing semantic paths from the root toward relevant origin/rationale elements.

## State Semantics

Operates on accepted repository state.

---

# 16. Impact

## Purpose

Analyze semantic consequences reachable from a Knowledge Element.

## Inputs

Conceptually:

```text
root ElementId
optional max depth
```

## Semantics

Impact preserves the existing v0.3 traversal model:

- accepted-state isolation;
- deterministic traversal;
- bounded relationship expansion;
- category-oriented result.

## Result

A deterministic set of semantically affected elements and relationships.

## State Semantics

Operates on accepted repository state.

---

# 17. History

## Purpose

Inspect accepted semantic evolution associated with a Knowledge Element or repository object.

## Inputs

Conceptually:

```text
target identity
```

## Result

The relevant accepted ChangeRevisions and semantic versions, ordered according to repository history semantics.

## State Semantics

Operates on accepted history.

---

# 18. ArtifactAccountability

## Purpose

Inspect semantic accountability between Artifacts and the versions of the semantic targets they represent or derive from.

## Result

For each relevant Artifact:

```text
CURRENT
STALE
UNACCOUNTED
```

along with the accountability relationships and version baselines needed to explain that status.

A repository-wide accountability summary remains distinct from any applied result filtering.

## Important Meaning

```text
CURRENT
```

means semantic target-version alignment.

It does not mean physical file contents have been verified.

## State Semantics

Operates on accepted SemanticState and accepted accountability history.

---

# 19. Ontology

## Purpose

Inspect the active OntologyVersion used as repository context.

## Result

Ontology inspection exposes:

```text
OntologyId
OntologyVersion ObjectId
element types
relationship types
allowed source types
allowed target types
```

## Important Property

Ontology is repository context.

It is not part of SemanticState.

---

# 20. Validate

## Purpose

Evaluate repository correctness and evidence classifications.

## Result Categories

### Mechanical Violations

Repository conditions that KAT can prove mechanically invalid.

Examples include:

```text
unknown relationship type
invalid relationship source type
invalid relationship target type
duplicate relationship triple
missing endpoint
```

### Mechanically Unverified Constraints

Active natural-language Constraints for which KAT has no executable mechanical evaluator.

### Validation Evidence Coverage

Evidence coverage for element categories that are valid targets of `kat.core/validates` under the active ontology.

## Important Distinction

```text
evidence-backed
!=
mechanically verified
```

## State Semantics

Operates on accepted repository state.

---

# 21. Context

## Status

New in v0.4.

## Purpose

Retrieve a bounded semantic neighborhood around one or more Knowledge Element entry points.

The operation exists to reduce the need for actors to manually assemble development context through repeated low-level queries.

## Inputs

Conceptually:

```text
root ElementId(s)
retrieval bounds
optional semantic category filters
```

Exact interaction syntax is defined separately.

## Semantics

`Context` is a deterministic projection over:

```text
accepted SemanticState
active OntologyVersion
explicit Relationships
explicit retrieval bounds
```

It does not use probabilistic inference.

## Result

A `ContextResult` shall preserve, where relevant:

```text
roots
related Knowledge Elements
semantic categories
relevant Relationships
root/path provenance
Artifact anchors
retrieval bounds
truncation state
```

## Semantic Roles

The result may distinguish categories such as:

```text
provenance / intent
requirements
constraints
design decisions
implementations
artifacts
validation evidence
dependencies
consequences
```

The exact category derivation belongs to `context-model.md`.

## Multiple Roots

The query may accept multiple roots.

If the same semantic object is reachable from multiple roots, the result may deduplicate the object while preserving root/path provenance.

## Artifact Semantics

Artifacts returned by Context are semantic routing anchors.

The operation does not claim that returned Artifacts represent the complete physical dependency set required for a software change.

## Relationship to Existing Queries

```text
Show
    focused element inspection

Trace
    origin/rationale traversal

Impact
    consequence traversal

Context
    bounded semantic-neighborhood projection
```

## State Semantics

Operates on accepted repository state.

---

# 22. GraphQuality

## Status

New in v0.4.

## Purpose

Identify mechanically valid graph conditions that may weaken semantic traceability, retrieval, accountability, or explanatory usefulness.

## Nature

`GraphQuality` is advisory.

Its findings are not mechanical validation violations.

## Inputs

Conceptually:

```text
optional diagnostic filters
```

## Result

A deterministic collection of advisory findings.

Each finding shall identify:

```text
finding type
affected semantic object(s)
observed graph condition
explanation
```

Severity may be included if defined by the graph-quality model.

## Candidate Finding Classes

The initial design may investigate findings such as:

```text
IsolatedElement
RequirementWithoutRealizationPath
ImplementationWithoutArtifactRoute
DesignDecisionWithoutConsequencePath
WeakProvenance
```

This document does not freeze the final diagnostic catalog.

## Ontology Awareness

Graph-quality rules should derive semantic capabilities from the active ontology where practical.

They should not impose one rigid `kat.core` graph shape when ontology extensions provide other valid structures.

## Important Distinction

```text
GraphQuality finding
!=
MechanicalViolation
```

A finding may indicate:

```text
valid but potentially weak
```

rather than:

```text
invalid repository
```

## State Semantics

Operates on accepted repository state and active OntologyVersion.

---

# 23. Query Invariants

## 23.1 Read-Only

Query operations never modify repository state.

## 23.2 Accepted-State Isolation

Unless a query explicitly exists for draft inspection, query operations operate on accepted state.

## 23.3 Determinism

Given the same:

```text
accepted SemanticState
active OntologyVersion
query inputs
KAT version
```

the semantic result shall be deterministic.

## 23.4 Explicit Truncation

A bounded query shall indicate when traversal was truncated by its configured bounds.

## 23.5 Semantic Explanation

Where a query returns related semantic objects, sufficient relationship information should remain available to explain why they are related.

---

# 24. v0.4 Operation Delta

Compared with v0.3.1:

## Mutation Operations

No change.

```text
CreateElement
UpdateElement
DeprecateElement
SupersedeElement
Link
Unlink
AccountArtifact
```

## Existing Queries

Preserved:

```text
List
Show
Status
Trace
Impact
History
ArtifactAccountability
Ontology
Validate
```

## New Queries

Added:

```text
Context
GraphQuality
```

Therefore the v0.4 semantic operation delta is limited to two new read-only query capabilities.

---

# 25. Out of Scope for This Document

The following are intentionally not defined as operations:

```text
reference resolution
workflow-local references
batch authoring
structured input
structured output
structured errors
human presentation modes
agent rules
```

These are interaction or interface concerns.

They shall be specified separately.

Likewise, this document does not define new canonical objects for:

```text
ContextResult
GraphQualityFinding
workflow aliases
batch submissions
```

These may remain derived or interaction-layer representations unless later design proves persistent semantic representation necessary.

---

# 26. Canonical Model Impact

The v0.4 operation model does not currently require a new canonical object kind.

The canonical object set remains:

```text
KnowledgeElementVersion
RelationshipVersion
ChangeRevision
SemanticState
OntologyVersion
```

`Context` and `GraphQuality` produce derived query results.

They do not create accepted semantic repository objects.

---

# 27. Repository Model Impact

Accepted Repository State remains:

```text
Accepted Repository State
    state  -> SemanticState
    change -> ChangeRevision | none
```

No additional accepted-state reference is introduced by v0.4 operations.

---

# 28. Next Specification Stage

The operation surface is intentionally small.

Most v0.4 work now belongs to detailed models around these operations rather than additional semantic operations.

The next specification sequence is:

```text
operations.md
    ↓
reference-model.md
    ↓
authoring-model.md
    ↓
context-model.md
    ↓
graph-quality-model.md
    ↓
machine-interface.md
    ↓
cli.md
```

The immediate next document should be:

```text
reference-model.md
```

because reference and identity ergonomics are the highest-priority authoring problem and influence the later authoring and CLI design.