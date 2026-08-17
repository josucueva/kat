# KAT v0.4 Requirements

## Status

Draft.

This document defines the requirements for KAT v0.4.

The requirements are derived from the problems and findings identified through the v0.1 to v0.3.1 implementation work and the real-project experiments.

The document defines what KAT v0.4 must achieve.

It does not yet define:

- final CLI syntax;
- exact command names;
- exact structured-output schemas;
- exact alias/reference mechanisms;
- implementation architecture;
- canonical-format changes.

Those decisions belong to later design stages.

---

# 1. Goal

KAT v0.4 shall improve the efficiency, consistency, usability, and maintainability of semantic repository workflows without expanding the semantic model unnecessarily.

The primary focus is reducing operational friction around the semantic capabilities that already exist.

v0.4 should make KAT:

```text
easier to author
easier to query
easier to consume programmatically
harder to misuse
cheaper to maintain
```

while preserving the existing principles of:

- specification-first authority;
- explicit semantic evolution;
- immutable canonical objects;
- stable identity;
- deterministic behavior;
- selective Artifact representation;
- separation between semantic knowledge and physical repository structure.

---

# 2. Scope

KAT v0.4 focuses on four areas:

1. Authoring Efficiency
2. Retrieval Efficiency
3. Machine Interaction
4. Graph Quality Guidance

Cross-cutting requirements apply to all four areas.

---

# 3. Authoring Efficiency

## REQ-0401: Reduce Manual Stable-Identifier Bookkeeping

KAT shall allow users to perform normal multi-operation semantic authoring without requiring them to manually retain and repeatedly re-enter full stable identifiers for newly created elements.

The mechanism used to satisfy this requirement shall preserve the existing stable identity model.

### Rationale

Current authoring frequently requires:

```text
create element
    ↓
capture UUID
    ↓
store UUID externally
    ↓
reuse UUID in link operations
```

At non-trivial graph sizes this creates significant operational friction.

### Constraints

This requirement shall not:

- replace UUID-based ElementId identity;
- make titles canonical identities;
- weaken ambiguity detection;
- change ObjectId semantics.

---

## REQ-0402: Support Efficient Multi-Operation Authoring

KAT shall support efficient construction of Changes containing many semantic operations.

The user shall not be forced to rely on external orchestration scripts merely to perform routine multi-element and multi-relationship authoring.

### Rationale

The Statit graph required approximately 213 CLI mutation operations and led the construction agent to create an external Python orchestration layer.

### Constraints

Efficient authoring shall preserve:

- ordered operation semantics;
- candidate-state validation;
- atomic Change commit behavior;
- explicit Change boundaries;
- stale-draft detection.

---

## REQ-0403: Preserve Explicit Change Semantics

Any authoring-efficiency mechanism introduced in v0.4 shall operate through the existing semantic Change model.

Batching or higher-level authoring must not bypass accepted ChangeRevision creation.

### Invariant

```text
authoring convenience
    must not bypass
semantic evolution semantics
```

---

## REQ-0404: Allow In-Workflow Reuse of Newly Created Elements

Within an active authoring workflow, a newly created element shall be referable by subsequent operations without requiring the user to manually resolve its stable identifier externally.

### Example semantic workflow

```text
create Requirement
create Implementation
link Implementation realizes Requirement
```

The workflow should be expressible directly without external UUID bookkeeping.

### Note

This requirement does not prescribe whether the solution uses:

- temporary aliases;
- references;
- handles;
- variables;
- another deterministic mechanism.

---

## REQ-0405: Authoring References Shall Be Unambiguous

Any non-UUID reference mechanism introduced by v0.4 shall resolve deterministically.

When a reference cannot be resolved uniquely, KAT shall fail explicitly rather than guess.

### Required behavior

Resolution failures shall distinguish at least:

```text
unknown reference
ambiguous reference
invalid reference
```

---

## REQ-0406: Authoring Shall Remain Usable Without AI

All core authoring workflows introduced in v0.4 shall remain deterministic and fully usable by humans, scripts, and ordinary CLI clients without requiring an AI agent.

---

# 4. Retrieval Efficiency

## REQ-0410: Support Efficient Semantic Neighborhood Retrieval

KAT shall provide a way to retrieve a useful bounded semantic neighborhood around one or more semantic entry points without requiring a user to manually compose a large sequence of low-level queries.

### Rationale

Existing `impact`, `trace`, `show`, `artifacts`, and related queries are individually useful, but agents currently have to assemble their results manually.

---

## REQ-0411: Aggregated Retrieval Shall Be Deterministic

Any semantic context aggregation introduced by v0.4 shall be derived from:

- accepted repository state;
- active ontology;
- explicit relationships;
- explicit traversal rules;
- explicit bounds.

It shall not require probabilistic or AI-based inference.

---

## REQ-0412: Retrieval Shall Preserve Semantic Role

Aggregated semantic retrieval shall preserve why information was returned.

A consumer should be able to distinguish, where applicable:

```text
requirements
constraints
design decisions
implementations
artifacts
validation evidence
incoming dependencies
outgoing consequences
```

The result shall not flatten all retrieved nodes into an undifferentiated set.

---

## REQ-0413: Retrieval Shall Preserve Provenance

For information included in an aggregated retrieval result, KAT shall preserve sufficient relationship information to explain how that information is connected to the entry point.

### Example

A consumer should be able to distinguish:

```text
Requirement
    -> realized by
Implementation
    -> represented by
Artifact
```

from a merely neighboring Artifact with no relevant path.

---

## REQ-0414: Retrieval Shall Support Explicit Bounds

Semantic neighborhood retrieval shall provide deterministic mechanisms for limiting expansion.

Bounds may include concepts such as:

- relationship depth;
- direction;
- semantic role;
- result category.

The exact interface is deferred.

---

## REQ-0415: Retrieval Shall Avoid Unnecessary Result Explosion

Default retrieval behavior shall favor concise, useful semantic context rather than exhaustive graph expansion.

Exhaustive traversal may remain available explicitly where appropriate.

### Rationale

Earlier unrestricted trace behavior produced approximately 19 KB of output for a small project.

---

## REQ-0416: Retrieval Shall Not Claim Complete Physical Dependency Coverage

KAT shall continue to treat Artifact mappings as semantic routing anchors rather than a complete physical dependency graph.

Semantic retrieval must not imply that every relevant physical source file is represented in KAT.

---

## REQ-0417: Retrieval Shall Support Local Physical Expansion

The retrieval model shall remain compatible with the workflow:

```text
semantic retrieval
    ↓
Artifact anchor
    ↓
ordinary source navigation
    ↓
additional physical implementation detail
```

v0.4 shall not require all physical dependencies to become KAT Artifacts.

---

## REQ-0418: Retrieval Results Shall Be Stable Enough for Reuse

A semantic retrieval result shall be representable in a form that can be preserved and consumed without requiring the client to manually reconstruct the same context from multiple formatted command outputs.

### Rationale

The Statit experiment showed that a correctly retrieved file could still be lost during manual context-set assembly.

---

# 5. Machine Interaction

## REQ-0420: Provide Structured Machine-Readable Output

KAT shall provide a structured machine-readable representation for core CLI query results.

The representation shall not require consumers to parse human-formatted terminal output.

### Applies at minimum to

- element inspection;
- semantic traversal;
- artifact accountability;
- validation results;
- ontology inspection;
- repository status;
- aggregated semantic retrieval introduced by v0.4, if any.

---

## REQ-0421: Human and Machine Output Shall Be Separable

Human-readable CLI presentation and machine-readable output shall be treated as separate presentation concerns.

Improving human formatting shall not require machine clients to change parsers unnecessarily.

---

## REQ-0422: Structured Output Shall Preserve Semantic Identity

Machine-readable results shall expose canonical stable identifiers where relevant.

Human-friendly presentation references may be included, but shall not replace stable identities in structured results.

---

## REQ-0423: Structured Output Shall Preserve Relationship Semantics

Machine-readable traversal and context results shall expose sufficient relationship information to reconstruct the semantic paths represented by the result.

---

## REQ-0424: Structured Output Shall Be Deterministic

Given the same accepted repository state, ontology, command arguments, and KAT version, structured query output shall be deterministic except for presentation-independent fields explicitly defined otherwise.

---

## REQ-0425: Mutation Results Shall Be Machine-Consumable

Mutation commands shall expose their created or affected semantic identities in a reliable machine-readable form.

External tools shall not need to extract UUIDs from human-formatted prose.

---

## REQ-0426: Compact Human Output Shall Remain Available

Structured output shall not replace concise human CLI modes.

KAT shall continue to support efficient terminal-oriented inspection for ordinary users.

---

# 6. Graph Quality Guidance

## REQ-0430: Distinguish Mechanical Validity From Graph Quality

KAT shall not treat advisory semantic-quality conditions as mechanical repository violations unless they correspond to an explicit invariant.

### Required conceptual distinction

```text
invalid
!=
unverified
!=
operationally weak
```

---

## REQ-0431: Provide Advisory Graph-Quality Diagnostics

KAT shall provide advisory diagnostics for graph structures that are mechanically valid but may reduce traceability, retrieval quality, or semantic usefulness.

### Candidate diagnostic classes include

```text
isolated active elements
Requirements with no useful realization path
Implementations with no Artifact representation
Design Decisions with no semantic consequence path
weak or missing provenance toward Intent
```

The final diagnostic set shall be defined during design.

---

## REQ-0432: Graph-Quality Diagnostics Shall Not Enforce a Universal Graph Shape

Quality diagnostics shall account for legitimate differences in software maturity, architecture, and modeling scope.

For example:

```text
Requirement without Implementation
```

may be valid during early specification work.

Therefore such findings shall not automatically invalidate the repository.

---

## REQ-0433: Diagnostics Shall Be Ontology-Aware

Where possible, graph-quality diagnostics shall derive relevant relationship and type semantics from the active ontology rather than hardcoding assumptions about only `kat.core`.

This is required to preserve ontology extensibility.

---

## REQ-0434: Diagnostics Shall Explain Their Basis

Each advisory diagnostic shall identify:

- the affected element or relationship;
- the observed graph condition;
- why the condition may matter.

Diagnostics shall not produce unexplained quality scores.

---

## REQ-0435: Graph Quality Shall Remain Distinct From Validation Evidence

Quality diagnostics shall not be conflated with:

- mechanical validation;
- mechanically unverified Constraints;
- Validation evidence coverage.

Each represents a different semantic concern.

---

# 7. CLI Consistency and Guidance

## REQ-0440: Common References Shall Resolve Consistently

Commands that operate on semantic elements should follow a consistent reference-resolution model where practical.

A user should not need to learn unrelated identifier behavior for each command.

---

## REQ-0441: Ambiguity Shall Be Explicit

Where shorthand or semantic references are supported, KAT shall never silently choose between multiple valid candidates.

Ambiguity errors shall expose enough information for the user to select the intended object.

---

## REQ-0442: The CLI Shall Guide Users Toward Efficient Operations

When an operation is valid but likely to produce excessive output or inefficient repeated interaction, KAT should provide concise actionable guidance where appropriate.

### Example class

If an exhaustive traversal produces a very large result, the CLI may suggest:

```text
bounded depth
compact output
aggregated retrieval
```

without preventing explicit exhaustive use.

---

## REQ-0443: Normal Workflows Shall Not Require Internal Repository Inspection

All semantic information necessary for normal authoring and retrieval workflows shall be accessible through supported KAT interfaces.

Users and agents shall not need to inspect:

```text
.kat/objects
binary canonical objects
internal refs
```

to understand repository semantics.

---

# 8. Resource Efficiency

## REQ-0450: Common Retrieval Workflows Shall Minimize Redundant Output

KAT should avoid requiring repeated transmission of the same semantic information across multiple commands when a deterministic aggregated result can provide it once.

---

## REQ-0451: Machine Output Shall Avoid Presentation Overhead

Structured output should avoid unnecessary human-presentation content when used by machine clients.

---

## REQ-0452: Efficient Retrieval Shall Not Require Full-Graph Traversal

Common context-discovery operations should operate on bounded relevant graph regions where possible.

---

## REQ-0453: Resource Efficiency Shall Not Sacrifice Semantic Correctness

Output reduction, batching, and aggregation shall not hide semantic relationships required to interpret the returned information correctly.

---

# 9. Compatibility Requirements

## REQ-0460: Preserve Existing Stable Identity Semantics

v0.4 shall preserve the current UUID-based stable identity semantics for canonical semantic entities.

---

## REQ-0461: Preserve Object Identity Semantics

ObjectId shall continue to be derived from SHA-256 over deterministic canonical object bytes unless an independently justified canonical-format revision explicitly changes this in a future design.

v0.4 interaction improvements do not themselves justify such a change.

---

## REQ-0462: Preserve Immutable Revision Semantics

Accepted canonical object revisions shall remain immutable.

---

## REQ-0463: Preserve Accepted-State Semantics

Read operations shall continue to use accepted repository state unless explicitly defined as draft inspection.

Open drafts shall not silently affect accepted-state queries.

---

## REQ-0464: Preserve Draft Transaction Semantics

The existing single local draft transaction model shall remain authoritative unless v0.4 explicitly revises it through a separate collaboration/change-model decision.

Authoring efficiency shall not implicitly introduce branch, merge, or synchronization semantics.

---

## REQ-0465: Preserve Selective Artifact Representation

v0.4 shall not require every physical file to be modeled as a KAT Artifact.

---

## REQ-0466: Preserve Accountability Semantics

Artifact accountability shall continue to represent semantic target-version alignment.

It shall not be redefined as physical file verification.

---

## REQ-0467: Preserve Ontology Extensibility

New authoring, retrieval, output, and graph-quality behavior should remain compatible with active ontology extensions.

---

# 10. Determinism Requirements

## REQ-0470: Core v0.4 Behavior Shall Remain Deterministic

Core authoring, reference resolution, semantic retrieval, quality diagnostics, and machine output shall not depend on probabilistic inference.

---

## REQ-0471: Deterministic Failure Shall Be Preferred Over Guessing

If KAT cannot determine a unique semantic target or valid interpretation, it shall fail explicitly.

---

## REQ-0472: Ordering Shall Be Stable

Structured and human-readable results whose semantic content is set-like shall define deterministic ordering suitable for reproducible CLI use and testing.

---

# 11. Usability Requirements

## REQ-0480: Common Semantic Workflows Shall Require Fewer Manual Steps

v0.4 shall measurably reduce interaction overhead for at least:

```text
multi-element authoring
relationship construction
semantic context retrieval
machine consumption of query output
```

relative to v0.3.1.

---

## REQ-0481: Improvements Shall Be Measurable

v0.4 should be evaluated against reproducible workflows derived from the existing experiments.

Potential measurements include:

```text
CLI command count
identifier bookkeeping operations
failed/retry commands
semantic query count
output size
physical search count
external scripting required
```

---

## REQ-0482: Existing Simple Workflows Shall Remain Simple

Improvements for large graphs and automation shall not make basic commands substantially harder to use.

Examples include:

```text
kat create
kat show
kat link
kat validate
kat status
```

---

# 12. Non-Requirements

The following are explicitly not requirements for v0.4.

## NR-0401: Automatic Semantic Graph Generation

v0.4 is not required to infer or generate the semantic repository automatically from source code.

---

## NR-0402: AI-Assisted Core Behavior

v0.4 is not required to use LLMs or other AI systems for authoring, traversal, validation, or diagnostics.

---

## NR-0403: Agent-Specific Integration

Agent skills, rules, prompts, MCP integration, or model-specific extensions are deferred.

They may be developed later on top of the stable KAT interface.

---

## NR-0404: Complete Physical Dependency Modeling

KAT is not required to model imports, function calls, AST relationships, widget trees, or every physical file.

---

## NR-0405: Git Replacement Features

v0.4 does not introduce:

- branching;
- merge;
- synchronization;
- distributed repository semantics;
- Git-compatible version control.

---

## NR-0406: Semantic Merge

Automatic or manual semantic merging of concurrent KAT histories remains deferred.

---

## NR-0407: Ontology Expansion for v0.4 Ergonomics

New core Knowledge Element or relationship types shall not be added merely to solve CLI interaction problems.

---

## NR-0408: Physical File Verification

KAT v0.4 is not required to hash, inspect, compile, or otherwise verify physical Artifact contents.

---

# 13. Success Criteria

KAT v0.4 shall be considered successful when the following are demonstrated empirically.

## SC-0401: Authoring

A repository comparable to the Statit graph can be constructed without requiring external scripting solely for:

```text
UUID retention
relationship endpoint bookkeeping
basic operation batching
```

---

## SC-0402: Retrieval

A development-context discovery workflow comparable to the Statit experiment can recover the useful semantic neighborhood using substantially fewer fragmented semantic queries than v0.3.1.

---

## SC-0403: Structured Consumption

An external client can consume KAT query and mutation results without parsing human-oriented terminal text.

---

## SC-0404: Context Preservation

A retrieved semantic neighborhood can be represented structurally so clients do not need to manually reconstruct the result from independent command outputs.

---

## SC-0405: Graph Quality

KAT can identify at least representative mechanically valid but operationally weak graph conditions without converting those conditions into hard repository violations.

---

## SC-0406: Semantic Compatibility

Existing v0.3.1 repository semantics remain valid unless an explicit v0.4 design decision documents and justifies a change.

---

# 14. Open Design Questions

The following questions remain unresolved and shall be answered during v0.4 design.

## Authoring

1. What reference mechanism should replace manual UUID bookkeeping in common authoring workflows?
2. Should references exist only inside a draft Change or persist beyond it?
3. Should KAT support declarative batch input?
4. Should batch input operate through CLI arguments, stdin, or a file format?
5. How should newly created identities be exposed to machine clients?
6. Should existing `create` and `link` commands remain unchanged and compose with new mechanisms?

## Retrieval

7. Should v0.4 introduce a dedicated semantic-context operation?
8. What constitutes a semantic neighborhood?
9. Should retrieval accept more than one root?
10. How should incoming and outgoing semantics be grouped?
11. How should traversal bounds be represented?
12. How should Artifact anchors be distinguished from physical completeness?

## Machine Interaction

13. Which structured output format should be normative?
14. Should structured output be available globally or per command?
15. What compatibility guarantees should apply to structured schemas?
16. Should mutation and query result schemas share a common envelope?

## Graph Quality

17. Which advisory diagnostics provide sufficient value to include in v0.4?
18. Should graph-quality inspection be a separate operation from `validate`?
19. How should severity be represented?
20. Which diagnostics can be derived generically from ontology capabilities?
21. Which diagnostics, if any, require `kat.core`-specific semantics?

---

# 15. Requirement Dependencies

The major requirement groups relate as follows:

```text
Authoring Efficiency
        ↓
lower graph maintenance cost
        ↓
more sustainable semantic repositories

Retrieval Efficiency
        ↓
fewer fragmented queries
        ↓
lower context-discovery cost

Machine Interaction
        ↓
stable automation interface
        ↓
agents / scripts / future extensions

Graph Quality Guidance
        ↓
better semantic connectivity
        ↓
higher retrieval usefulness
```

Together:

```text
lower cost to create knowledge
        +
lower cost to retrieve knowledge
        +
better graph quality
        +
better machine consumption
        ↓
KAT becomes more useful than repeated semantic rediscovery
```

---

# 16. v0.4 Requirement Summary

v0.4 shall improve KAT without expanding its core purpose.

The target is not more semantic concepts.

The target is a better semantic repository interface.

The release should make this workflow practical:

```text
author authoritative knowledge efficiently
        ↓
evolve it through explicit Changes
        ↓
retrieve bounded semantic context efficiently
        ↓
route into physical implementation
        ↓
consume results reliably as human or machine
        ↓
identify weak graph structures without confusing them with invalidity
```

The next specification stage is to derive v0.4 use cases from these requirements.