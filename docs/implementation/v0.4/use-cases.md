# KAT v0.4 Use Cases

## Status

Draft.

This document defines the primary user-facing workflows for KAT v0.4.

The use cases are derived from the v0.4 requirements and the empirical findings gathered through the Task Management API, Feature Flag Service, and Statit experiments.

The use cases describe:

- actor intent;
- preconditions;
- normal interaction flow;
- expected outcomes;
- failure conditions;
- relevant requirements.

They do not yet freeze:

- final CLI command names;
- exact command syntax;
- structured-output schemas;
- alias/reference syntax;
- batch-authoring format;
- graph-quality diagnostic algorithms.

Those belong to the operations and CLI design stages.

---

# 1. Actors

## Human User

A developer, architect, maintainer, or reviewer interacting directly with KAT.

The Human User may:

- author semantic knowledge;
- evolve the repository;
- inspect context;
- validate repository state;
- investigate graph quality;
- use human-oriented CLI output.

---

## Machine Client

A deterministic external program that interacts with KAT.

Examples include:

- shell scripts;
- CI tooling;
- IDE integrations;
- repository automation;
- future agent integrations.

A Machine Client requires stable structured input/output and shall not depend on parsing human presentation text.

---

## Agent

An AI-based software-development agent using supported KAT interfaces.

The Agent is not a special semantic actor in KAT core.

For v0.4 purposes, an Agent behaves primarily as a Machine Client that also interprets semantic results.

Agent-specific policy, prompting, and skills remain outside the v0.4 core.

---

# 2. Use Case Summary

| ID | Name | Primary Actor | Requirement Area |
|---|---|---|---|
| UC-0401 | Author a Semantic Subsystem | Human User / Machine Client | Authoring Efficiency |
| UC-0402 | Reference Newly Created Knowledge | Human User / Machine Client | Authoring Efficiency |
| UC-0403 | Author a Large Change Efficiently | Human User / Machine Client | Authoring Efficiency |
| UC-0404 | Resolve a Semantic Reference | Human User / Machine Client | Authoring / CLI Consistency |
| UC-0405 | Retrieve Semantic Development Context | Human User / Machine Client | Retrieval Efficiency |
| UC-0406 | Retrieve Context from Multiple Entry Points | Human User / Machine Client | Retrieval Efficiency |
| UC-0407 | Inspect Why Context Was Retrieved | Human User / Machine Client | Retrieval / Provenance |
| UC-0408 | Expand from Semantic Context into Physical Code | Human User / Agent | Retrieval / Artifact Routing |
| UC-0409 | Consume KAT Query Results Programmatically | Machine Client | Machine Interaction |
| UC-0410 | Consume Mutation Results Programmatically | Machine Client | Machine Interaction |
| UC-0411 | Inspect Graph Quality | Human User / Machine Client | Graph Quality |
| UC-0412 | Investigate a Graph-Quality Finding | Human User | Graph Quality |
| UC-0413 | Perform Concise Human Inspection | Human User | CLI Efficiency |
| UC-0414 | Recover from Ambiguous or Invalid References | Human User / Machine Client | CLI Consistency |
| UC-0415 | Inspect Repository Semantics Without Internal Access | Human User / Agent | Supported Interface Boundary |

---

# 3. UC-0401: Author a Semantic Subsystem

## Goal

Create a coherent group of Knowledge Elements and relationships representing part of the software without requiring external identifier bookkeeping.

## Primary Actors

- Human User
- Machine Client

## Preconditions

- A KAT repository exists.
- The repository can be opened successfully.
- The active OntologyVersion is available.
- No conflicting local draft prevents the requested authoring workflow.

## Trigger

The actor wants to represent a new semantic subsystem or responsibility.

Examples:

```text
Workout persistence
Authentication
Feature evaluation
Database migration policy
API request validation
```

## Main Flow

1. The actor begins a semantic Change.
2. The actor creates one or more Knowledge Elements.
3. KAT provides a deterministic way to reference those newly created elements during the active authoring workflow.
4. The actor creates relationships between:
   - newly created elements;
   - existing repository elements;
   - combinations of both.
5. KAT applies operations sequentially to the working candidate state.
6. KAT validates each operation and the resulting candidate state according to existing Change semantics.
7. The actor inspects the staged Change.
8. The actor commits the Change.
9. KAT publishes one accepted ChangeRevision.

## Example Semantic Workflow

```text
create Requirement "Session Plan Snapshotting"

create Design Decision "SQLite Session Snapshot Schema"

create Implementation "Workout Persistence and Resume Engine"

link Design Decision addresses Requirement

link Design Decision guides Implementation

link Implementation realizes Requirement
```

The actor should not need to manually copy UUIDs between these operations.

## Expected Result

The semantic subsystem is represented through:

- canonical Knowledge Element identities;
- explicit relationships;
- one coherent accepted ChangeRevision.

## Alternative Flow: Existing Element Participation

The actor references an already accepted Knowledge Element when constructing the new subsystem.

KAT resolves that reference deterministically and uses the corresponding ElementId.

## Failure Conditions

- unknown semantic reference;
- ambiguous semantic reference;
- ontology-invalid relationship;
- missing referenced element;
- candidate validation failure;
- stale draft;
- repository integrity failure.

## Postconditions

On successful commit:

```text
accepted.state = candidate result state
accepted.change = committed ChangeRevision
```

All persistent identities remain canonical UUID-based identities.

## Requirements

- REQ-0401
- REQ-0402
- REQ-0403
- REQ-0404
- REQ-0405
- REQ-0406
- REQ-0460
- REQ-0463
- REQ-0464
- REQ-0470

---

# 4. UC-0402: Reference Newly Created Knowledge

## Goal

Use a newly created Knowledge Element in subsequent operations of the same authoring workflow without manually retrieving and re-entering its stable UUID.

## Primary Actors

- Human User
- Machine Client

## Preconditions

- A Change authoring workflow is active.
- At least one Knowledge Element has been created during that workflow.

## Trigger

A later operation needs to refer to an element created earlier in the same workflow.

## Main Flow

1. The actor creates a Knowledge Element.
2. KAT returns or exposes a deterministic workflow-level reference.
3. The actor uses that reference in a later operation.
4. KAT resolves the workflow reference to the corresponding stable ElementId.
5. The operation is staged normally.

## Example

Conceptually:

```text
create Requirement
    -> workflow reference: requirement-a

create Implementation
    -> workflow reference: implementation-a

link implementation-a realizes requirement-a
```

The exact reference syntax is not defined by this use case.

## Expected Result

The actor can construct dependent operations without external UUID storage.

## Constraints

The workflow-level reference:

- shall not replace ElementId;
- shall not create ambiguous persistent identity;
- shall resolve deterministically;
- shall not survive beyond its defined lifetime unless later design explicitly makes it persistent.

## Failure Conditions

- unknown workflow reference;
- duplicate reference declaration;
- reference used outside its valid lifetime;
- reference resolves to an invalid endpoint for the requested operation.

## Requirements

- REQ-0401
- REQ-0404
- REQ-0405
- REQ-0425
- REQ-0471

---

# 5. UC-0403: Author a Large Change Efficiently

## Goal

Stage a large ordered group of semantic operations without requiring hundreds of manually orchestrated CLI invocations.

## Primary Actors

- Human User
- Machine Client

## Preconditions

- A writable repository exists.
- The actor has a coherent semantic Change to represent.

## Trigger

The actor needs to perform a significant semantic construction or revision.

Examples:

```text
initially model a subsystem
introduce a new architecture responsibility
connect a previously sparse semantic region
add a set of Validation evidence
perform a large traceability refinement
```

## Main Flow

1. The actor begins a Change.
2. The actor submits multiple ordered semantic operations through a supported KAT authoring mechanism.
3. KAT processes operations according to their explicit order.
4. Newly created elements may be referenced by later operations in the same authoring workflow.
5. KAT constructs `S_working` incrementally.
6. Operation-level and candidate-state rules are applied normally.
7. The actor inspects the candidate Change before acceptance.
8. The actor commits once.
9. KAT creates one ChangeRevision covering the complete operation sequence.

## Expected Result

Large authoring workflows remain:

```text
explicit
ordered
validated
atomic at acceptance
```

while requiring substantially less interaction overhead than v0.3.1.

## Failure Flow

If operation `N` is invalid:

1. KAT identifies the failing operation.
2. KAT reports the semantic reason.
3. KAT does not silently reorder or reinterpret operations.
4. The accepted repository state remains unchanged.

The exact recovery semantics for partially supplied batch input shall be defined during operation design.

## Constraints

Large-change authoring shall not introduce implicit:

- merge;
- branch;
- synchronization;
- best-effort partial accepted commits.

## Requirements

- REQ-0402
- REQ-0403
- REQ-0404
- REQ-0425
- REQ-0480
- SC-0401

---

# 6. UC-0404: Resolve a Semantic Reference

## Goal

Identify an existing Knowledge Element without requiring a full UUID when an unambiguous supported reference is available.

## Primary Actors

- Human User
- Machine Client

## Preconditions

- The repository is open.
- The requested object exists in the relevant repository state.

## Trigger

A command requires an element reference.

## Main Flow

1. The actor provides a supported reference.
2. KAT determines the reference class.
3. KAT resolves the reference deterministically.
4. KAT returns or operates on the unique stable ElementId.

## Existing Supported Forms

At minimum, v0.4 shall continue to support:

```text
full UUID
UUID prefix
```

Additional reference classes may be introduced during design.

## Ambiguous Flow

If more than one element matches:

1. KAT does not select automatically.
2. KAT reports ambiguity.
3. KAT provides stable identities sufficient to disambiguate.

## Unknown Flow

If no element matches:

1. KAT reports an unknown reference.
2. KAT does not silently interpret the value as another reference class when such interpretation would be unsafe.

## Expected Result

Reference behavior is consistent across commands where practical.

## Requirements

- REQ-0405
- REQ-0440
- REQ-0441
- REQ-0471

---

# 7. UC-0405: Retrieve Semantic Development Context

## Goal

Retrieve a bounded, semantically organized neighborhood around a Knowledge Element to support software understanding and development planning.

## Primary Actors

- Human User
- Machine Client
- Agent

## Preconditions

- A KAT repository exists.
- The repository has an accepted state.
- The requested entry point resolves uniquely.

## Trigger

The actor needs to understand the semantic context surrounding a Requirement, Design Decision, Implementation, Artifact, or other Knowledge Element.

## Main Flow

1. The actor provides one semantic entry point.
2. KAT resolves the entry point.
3. KAT traverses the accepted semantic graph according to deterministic context-retrieval rules.
4. KAT applies explicit bounds.
5. KAT groups returned knowledge by semantic role.
6. KAT preserves relevant relationship paths explaining why each item was included.
7. KAT returns a structured context result.
8. The actor uses the result to guide further semantic or physical inspection.

## Potential Result Categories

Where relevant, the result may distinguish:

```text
root
intent / provenance
requirements
constraints
design decisions
implementations
artifacts
validation evidence
dependencies
consequences
```

The exact result schema remains a design decision.

## Expected Result

The actor obtains useful context without manually composing many:

```text
show
trace
impact
artifacts
```

queries.

## Constraints

The result must not claim:

- complete physical dependency coverage;
- code correctness;
- Artifact file-content verification;
- probabilistically inferred relationships.

## Failure Conditions

- unresolved entry point;
- ambiguous entry point;
- invalid traversal bound;
- repository integrity failure.

## Requirements

- REQ-0410
- REQ-0411
- REQ-0412
- REQ-0413
- REQ-0414
- REQ-0415
- REQ-0416
- REQ-0418
- SC-0402
- SC-0404

---

# 8. UC-0406: Retrieve Context from Multiple Entry Points

## Goal

Retrieve the combined relevant semantic neighborhood for a task that naturally intersects multiple existing semantic responsibilities.

## Primary Actors

- Human User
- Machine Client
- Agent

## Preconditions

- Two or more valid semantic entry points have been identified.

## Trigger

A development task cannot be represented adequately by a single existing Knowledge Element.

Example:

```text
rest timer behavior
+
session snapshotting
+
routine configuration
```

## Main Flow

1. The actor supplies multiple semantic entry points.
2. KAT resolves all entry points.
3. KAT traverses according to deterministic aggregation rules.
4. KAT combines overlapping semantic neighborhoods.
5. KAT avoids unnecessary duplicate result entries.
6. KAT preserves which root or roots contributed to each result.
7. KAT returns the combined context.

## Expected Result

The actor can reason about a cross-cutting change without manually merging several independent terminal outputs.

## Constraints

Aggregation shall not erase root provenance.

If an Artifact is reachable from two entry points, the result should be capable of expressing that fact.

## Failure Conditions

- one or more ambiguous roots;
- one or more unknown roots;
- invalid retrieval configuration.

Design shall determine whether partial success is permitted or whether root resolution is all-or-nothing.

## Requirements

- REQ-0410
- REQ-0412
- REQ-0413
- REQ-0418
- REQ-0450

---

# 9. UC-0407: Inspect Why Context Was Retrieved

## Goal

Understand the semantic path that caused an element or Artifact to appear in a retrieved context.

## Primary Actors

- Human User
- Machine Client

## Preconditions

- A semantic context result has been produced.

## Trigger

The actor needs to determine why a particular result is relevant.

## Main Flow

1. The actor identifies an item in the result.
2. KAT exposes the relationship path or paths connecting the item to the context root.
3. The path includes relationship semantics and stable identities.
4. The actor determines whether the item is:
   - rationale;
   - constraint;
   - consequence;
   - implementation responsibility;
   - Artifact anchor;
   - Validation evidence;
   - another semantically justified category.

## Example

```text
Session Plan Snapshotting
    <- addressed by
SQLite Session Snapshot Schema
    -> guides
Workout Persistence and Resume Engine
    <- represented by
workout_repository.dart
```

## Expected Result

Context retrieval remains explainable rather than appearing as an opaque recommendation.

## Requirements

- REQ-0412
- REQ-0413
- REQ-0423
- REQ-0453

---

# 10. UC-0408: Expand from Semantic Context into Physical Code

## Goal

Use KAT to locate a semantic implementation neighborhood and then use ordinary source-code tools to discover fine-grained physical dependencies.

## Primary Actors

- Human User
- Agent

## Preconditions

- Semantic retrieval has produced one or more Artifact anchors.

## Trigger

The actor needs to understand or implement the physical change.

## Main Flow

1. The actor retrieves semantic context.
2. KAT identifies relevant Implementation responsibilities and Artifact anchors.
3. The actor opens the mapped physical Artifacts.
4. The actor follows:
   - imports;
   - symbols;
   - references;
   - local search;
   - framework structure;
   - tests;
   - other physical dependencies.
5. Additional relevant files may be discovered that are not KAT Artifacts.
6. The actor uses these files as local physical expansion of the semantic context.

## Expected Result

KAT narrows the initial search area while ordinary development tooling completes physical discovery.

## Important Invariant

```text
unmodeled physical file
!=
KAT graph defect
```

A missing Artifact mapping is only a graph-quality problem when the semantic responsibility cannot be located reliably without broad rediscovery.

## Requirements

- REQ-0416
- REQ-0417
- REQ-0465

---

# 11. UC-0409: Consume KAT Query Results Programmatically

## Goal

Allow an external program to consume KAT query results without parsing terminal-formatted text.

## Primary Actor

Machine Client

## Preconditions

- KAT supports a structured output representation for the requested query.

## Trigger

An external tool invokes a KAT read operation.

## Main Flow

1. The Machine Client executes a query.
2. It requests machine-readable output.
3. KAT returns structured data.
4. The result contains:
   - stable semantic identifiers;
   - semantic type information;
   - operation-specific result data;
   - deterministic ordering;
   - relationship information when applicable.
5. The Machine Client consumes the result directly.

## Example Consumers

```text
shell automation
CI
IDE integration
future Agent Extension
test harness
repository reporting tool
```

## Expected Result

No human-text parsing is required.

## Failure Flow

Errors shall also be representable in a reliable structured form suitable for machine handling.

The exact error envelope remains a design question.

## Requirements

- REQ-0420
- REQ-0421
- REQ-0422
- REQ-0423
- REQ-0424
- REQ-0472
- SC-0403

---

# 12. UC-0410: Consume Mutation Results Programmatically

## Goal

Allow a Machine Client to perform semantic mutations and reliably obtain created or affected identities.

## Primary Actor

Machine Client

## Preconditions

- The requested mutation is valid.
- The actor is allowed to modify the repository.

## Trigger

A Machine Client creates or updates semantic knowledge.

## Main Flow

1. The client invokes a mutation.
2. KAT stages or accepts the mutation according to Change semantics.
3. KAT returns a structured mutation result.
4. The result exposes the stable identities needed for subsequent operations.
5. The client can use these identities or workflow references without parsing human output.

## Expected Result

External automation no longer needs logic such as:

```text
run kat create
parse text
extract UUID
store UUID
run kat link
```

## Requirements

- REQ-0425
- REQ-0401
- REQ-0404
- SC-0401
- SC-0403

---

# 13. UC-0411: Inspect Graph Quality

## Goal

Identify semantic graph structures that are mechanically valid but may weaken traceability, retrieval, accountability, or explanation.

## Primary Actors

- Human User
- Machine Client

## Preconditions

- The repository opens successfully.
- Mechanical repository validation can be performed.

## Trigger

The actor wants to evaluate the operational usefulness of the semantic graph.

## Main Flow

1. KAT evaluates advisory graph-quality rules against the accepted state and active ontology.
2. KAT produces zero or more findings.
3. Each finding identifies:
   - affected semantic object;
   - observed condition;
   - diagnostic class;
   - explanation of potential impact.
4. Findings do not cause repository invalidity unless an independent mechanical invariant is violated.
5. The actor reviews the findings and decides whether modeling changes are appropriate.

## Candidate Finding Types

Design may include findings such as:

```text
isolated active element

Requirement with no realization path

Implementation with no Artifact route

Design Decision with no semantic consequence path

weak provenance toward Intent
```

This use case does not freeze the final diagnostic catalog.

## Expected Result

The actor can distinguish:

```text
VALID BUT POSSIBLY WEAK
```

from:

```text
INVALID
```

## Requirements

- REQ-0430
- REQ-0431
- REQ-0432
- REQ-0433
- REQ-0434
- REQ-0435
- SC-0405

---

# 14. UC-0412: Investigate a Graph-Quality Finding

## Goal

Understand why KAT reported an advisory graph-quality condition before deciding whether to change the model.

## Primary Actor

Human User

## Preconditions

- A graph-quality finding exists.

## Trigger

The user wants to inspect a finding.

## Main Flow

1. The user selects a finding.
2. KAT identifies the affected element or relationship.
3. KAT presents the graph condition that triggered the finding.
4. KAT presents relevant nearby semantic paths where useful.
5. The user determines whether the condition is:
   - intentional;
   - temporary;
   - incomplete modeling;
   - a genuine graph-quality issue.
6. The user may choose to make a separate semantic Change.

## Example

```text
Finding:
Requirement has no realization path.

Element:
REQ-X: Offline Workout Resume

Observed:
No active path from this Requirement through an ontology-valid
realization relationship to an Implementation.

Impact:
Impact/context queries rooted here may not route toward physical implementation.
```

The finding does not require the user to add an Implementation if the Requirement is intentionally not implemented yet.

## Expected Result

Quality diagnostics remain explainable and advisory.

## Requirements

- REQ-0432
- REQ-0434
- REQ-0435

---

# 15. UC-0413: Perform Concise Human Inspection

## Goal

Inspect KAT interactively without receiving unnecessary result volume.

## Primary Actor

Human User

## Preconditions

- A supported KAT read operation is available.

## Trigger

The user wants a quick terminal-oriented answer.

## Main Flow

1. The user invokes a query.
2. KAT provides a concise default representation suitable for terminal use.
3. The user may explicitly request:
   - greater detail;
   - exhaustive traversal;
   - machine-readable output;
   - compact output;
   - bounded output.
4. KAT does not require the user to consume exhaustive data by default.

## Expected Result

Simple inspection remains simple even after machine output and aggregated retrieval are introduced.

## Requirements

- REQ-0415
- REQ-0426
- REQ-0442
- REQ-0450
- REQ-0482

---

# 16. UC-0414: Recover from Ambiguous or Invalid References

## Goal

Correct a failed semantic reference without guessing which object KAT intended.

## Primary Actors

- Human User
- Machine Client

## Preconditions

- The actor supplied a reference to a KAT operation.

## Trigger

The reference cannot be resolved uniquely.

## Ambiguous Flow

1. KAT detects multiple candidates.
2. KAT returns an ambiguity error.
3. Candidate stable identities are exposed.
4. Human-readable output may include useful titles/types.
5. The actor submits a more precise reference.

## Unknown Flow

1. KAT finds no valid candidate.
2. KAT reports an unknown reference.
3. No semantic operation is performed.

## Invalid Flow

1. The supplied value is not a valid reference form.
2. KAT reports an invalid-reference error.
3. KAT does not reinterpret the value unpredictably.

## Expected Result

Resolution remains deterministic and safe.

## Requirements

- REQ-0405
- REQ-0440
- REQ-0441
- REQ-0471

---

# 17. UC-0415: Inspect Repository Semantics Without Internal Access

## Goal

Use supported KAT interfaces to understand repository semantics without reading internal canonical-storage structures.

## Primary Actors

- Human User
- Agent
- Machine Client

## Preconditions

- The KAT repository can be opened.

## Trigger

The actor needs information about:

- ontology;
- elements;
- relationships;
- context;
- validation;
- accountability;
- graph quality;
- Changes.

## Main Flow

1. The actor invokes a supported KAT read operation.
2. KAT exposes the required semantic information.
3. The actor completes the workflow without opening or decoding:
   - `.kat/objects`;
   - internal refs;
   - canonical CBOR;
   - repository implementation metadata not intended as public interface.

## Expected Result

Normal workflows never require reverse engineering KAT's storage layer.

## Failure Condition

If a normal semantic workflow still requires internal repository inspection, the supported KAT interface is incomplete.

## Requirements

- REQ-0443
- REQ-0420
- REQ-0410
- REQ-0431

---

# 18. Cross-Use-Case Invariants

The following invariants apply across the v0.4 use cases.

## 18.1 Stable Identity Remains Canonical

Human-friendly or workflow-local references may improve interaction, but canonical persistent identity remains based on existing stable IDs.

```text
convenient reference
    -> resolves to
stable identity
```

not:

```text
convenient reference
    replaces
stable identity
```

---

## 18.2 Semantic Evolution Remains Explicit

All mutations continue to occur through semantic operations and Change semantics.

Efficiency features shall not create hidden repository mutations.

---

## 18.3 Accepted-State Isolation Remains Preserved

Normal queries operate on accepted state.

Draft inspection remains explicit.

---

## 18.4 Machine and Human Interfaces Represent the Same Semantics

Machine-readable output and human-readable output may differ in presentation but shall represent the same underlying operation result.

---

## 18.5 Quality Findings Are Not Violations

Unless an independently defined invariant is broken:

```text
quality finding
!=
mechanical violation
```

---

## 18.6 Artifact Retrieval Is Selective

KAT may route to selected physical Artifact anchors.

It does not claim complete physical-file coverage.

---

## 18.7 Core Workflows Remain Deterministic

No use case in this document depends on AI inference.

---

# 19. Deferred Use Cases

The following workflows remain outside v0.4.

## DU-0401: Generate a Semantic Graph Automatically

Deferred.

---

## DU-0402: Ask an AI Agent to Maintain the Graph Automatically

Deferred to a future extension layer.

---

## DU-0403: Merge Concurrent Semantic Histories

Deferred.

---

## DU-0404: Synchronize Distributed KAT Repositories

Deferred.

---

## DU-0405: Infer Physical Dependencies from Source Code

Deferred.

---

## DU-0406: Verify Physical Artifact Contents

Deferred.

---

## DU-0407: Generate Agent Rules or Skills

A future Agent Extension may provide this, but it is not part of KAT v0.4 core semantics.

---

# 20. Requirement Coverage

The principal requirement groups are covered as follows.

## Authoring Efficiency

Covered primarily by:

```text
UC-0401 Author a Semantic Subsystem
UC-0402 Reference Newly Created Knowledge
UC-0403 Author a Large Change Efficiently
UC-0404 Resolve a Semantic Reference
UC-0410 Consume Mutation Results Programmatically
UC-0414 Recover from Ambiguous or Invalid References
```

---

## Retrieval Efficiency

Covered primarily by:

```text
UC-0405 Retrieve Semantic Development Context
UC-0406 Retrieve Context from Multiple Entry Points
UC-0407 Inspect Why Context Was Retrieved
UC-0408 Expand from Semantic Context into Physical Code
UC-0413 Perform Concise Human Inspection
```

---

## Machine Interaction

Covered primarily by:

```text
UC-0409 Consume KAT Query Results Programmatically
UC-0410 Consume Mutation Results Programmatically
```

---

## Graph Quality Guidance

Covered primarily by:

```text
UC-0411 Inspect Graph Quality
UC-0412 Investigate a Graph-Quality Finding
```

---

## CLI Consistency

Covered primarily by:

```text
UC-0404 Resolve a Semantic Reference
UC-0413 Perform Concise Human Inspection
UC-0414 Recover from Ambiguous or Invalid References
UC-0415 Inspect Repository Semantics Without Internal Access
```

---

# 21. v0.4 Core Workflows

Although this document defines fifteen use cases, the v0.4 interaction model can be reduced to four principal workflows.

## Workflow A: Author

```text
begin Change
    ↓
create semantic knowledge
    ↓
reuse references directly
    ↓
relate knowledge
    ↓
inspect candidate
    ↓
commit
```

---

## Workflow B: Retrieve

```text
identify semantic entry point(s)
    ↓
retrieve bounded semantic context
    ↓
inspect provenance
    ↓
follow Artifact anchors
    ↓
perform local physical expansion
```

---

## Workflow C: Automate

```text
invoke KAT
    ↓
receive structured deterministic result
    ↓
consume stable identities and semantics
    ↓
perform next supported operation
```

---

## Workflow D: Improve the Graph

```text
validate repository
    ↓
inspect advisory graph quality
    ↓
investigate findings
    ↓
decide whether refinement is appropriate
    ↓
author explicit Change
```

---

# 22. Next Specification Stage

The next stage is to define the v0.4 operations required to support these use cases.

The operations design shall determine:

- which existing operations remain unchanged;
- which existing operations need stronger resolution/output behavior;
- which new operations are required;
- which capabilities are projections over existing repository semantics;
- which capabilities are purely CLI interaction mechanisms;
- whether any canonical semantic model change is actually necessary.

The expected sequence is:

```text
v0.4 findings
    ↓
v0.4 requirements
    ↓
v0.4 use cases
    ↓
v0.4 operations
    ↓
CLI semantics
    ↓
implementation design
```