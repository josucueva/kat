# KAT v0.4 Problems and Findings

## Status

Draft.

This document consolidates empirical findings from KAT v0.1 through v0.3.1 and the real-project experiments performed with the Task Management API, Feature Flag Service, and Statit mobile application.

Its purpose is to define the problems that should guide the next KAT iteration.

This document does not define the v0.4 feature set.

Possible solutions are recorded only as directions for investigation.

---

# 1. Purpose

KAT's core semantic model has reached the point where it can be evaluated through actual software-development workflows rather than only through canonical-format and repository-conformance tests.

The experiments indicate that the current semantic model is useful for:

- representing authoritative software knowledge;
- tracing rationale;
- analyzing impact;
- mapping semantic responsibilities to physical artifacts;
- distinguishing semantic accountability from physical verification;
- routing developers and agents toward relevant implementation areas.

The experiments also exposed operational problems.

The dominant issues are no longer primarily about missing ontology concepts or repository semantics.

They concern:

- cost of semantic authoring;
- CLI interaction efficiency;
- identifier plumbing;
- fragmented retrieval;
- machine consumption of CLI output;
- graph-quality diagnostics;
- preserving retrieved context;
- ensuring that KAT remains cheaper to maintain than repeated rediscovery.

The next iteration should address these problems while preserving KAT's existing philosophy and invariants.

---

# 2. Evidence Base

The findings in this document are grounded in several implementation and empirical milestones.

## 2.1 Task Management API Experiment

The first real-project experiment modeled a small Node.js / Express application.

Final semantic model:

```text
40 active elements
84 relationships
7 accepted revisions
6 artifacts
```

Approximately 131 staged semantic operations were performed across accepted revisions.

This experiment exposed several important problems:

- ontology rules were not discoverable through the CLI at the time;
- the agent inspected internal `.kat/objects` data to recover ontology information;
- two invalid relationship attempts occurred before discovering endpoint rules;
- unrestricted trace output could reach approximately 19 KB;
- the distinction between mechanical validation and Validation evidence was not initially clear;
- UUID-oriented authoring created friction;
- semantic modeling could become excessively detailed without explicit granularity guidance.

Several of these problems were addressed in v0.3 and v0.3.1.

---

## 2.2 Feature Flag Service Experiments

The Feature Flag Service was used to test semantic context discovery.

The first KAT-assisted discovery required:

```text
29 KAT queries
19 kat show calls
```

while still inspecting all source and test files in the small repository.

A subsequent retrieval-discipline experiment reduced this to:

```text
9 KAT queries
0 kat show calls
6 kat impact calls
```

while recovering effectively the same development context.

This showed that the graph itself was capable of useful semantic retrieval, but the interaction pattern could become unnecessarily fragmented.

---

## 2.3 Statit Graph Construction

The Statit Flutter application provided a substantially larger repository.

The frozen KAT graph contains:

```text
70 active Knowledge Elements
110 relationships
6 accepted ChangeRevisions
21 accounted Artifacts
```

Repository construction required approximately:

```text
6 change begin operations
70 create operations
110 link operations
21 artifact-accountability operations
6 change commit operations

Total: 213 CLI operations
```

The construction agent created an external Python orchestration script to manage the CLI authoring process.

This is direct evidence that manual command-by-command semantic construction does not scale comfortably at this graph size.

---

## 2.4 Statit Context-Discovery A/B Experiment

A new feature, configurable per-exercise rest timer presets, was used to compare normal repository discovery against KAT-first semantic discovery.

Baseline discovery:

```text
17 directory/tree operations
4 ordinary searches
14 physical files inspected
0 KAT queries
```

KAT-assisted discovery:

```text
1 directory/tree operation
2 ordinary searches
11 physical files inspected
10 KAT queries
```

The KAT-guided agent reached the correct architectural neighborhood through semantic paths involving:

```text
Rest Timer Countdown Presets

Session Plan Snapshotting
    -> SQLite Session Snapshot Schema
    -> Workout Persistence and Resume Engine

Routine Day Management Service
```

The KAT run required less broad structural exploration and fewer physical reads.

---

## 2.5 Statit Implementation A/B Experiment

Both discovery results were then used as implementation starting contexts.

Both implementations converged on essentially the same production-code surface.

The baseline required no additional files outside its original 14-file context.

The KAT-assisted implementation required one additional file:

```text
routine_repository.dart
```

However, this file had already been surfaced during KAT discovery and was accidentally omitted from the final context summary.

The implementation did not require broad repository rediscovery and did not require additional KAT queries.

This suggests that KAT's observed benefit occurred mainly during context acquisition rather than during the code-editing phase itself.

---

# 3. Findings

---

# F-001: Semantic Authoring Exposes Too Much Identity Plumbing

## Observation

Element and relationship authoring requires users to operate directly on UUID-based element identities.

Current element resolution accepts:

```text
full UUID
UUID prefix of at least 8 hexadecimal characters
```

It does not accept:

```text
element titles
semantic aliases
artifact paths
human-defined local references
```

Relationship creation therefore requires commands such as:

```bash
kat link realizes <implementation-uuid> <requirement-uuid>
```

rather than references based on semantic meaning.

## Evidence

The current `kat link` grammar is:

```text
kat link <RELATIONSHIP_TYPE> <SOURCE_ELEMENT_ID> <TARGET_ELEMENT_ID>
```

where both endpoints are UUID identities.

Statit graph construction required 110 such link operations.

The construction agent created an external Python script partly to retain returned UUID values and reuse them during subsequent relationship construction.

## Impact

Identity bookkeeping introduces cost unrelated to semantic modeling itself.

The user or agent must repeatedly perform:

```text
create semantic object
    ↓
capture UUID
    ↓
store UUID externally
    ↓
reuse UUID in later operations
```

This:

- increases authoring command count;
- increases agent/tool context requirements;
- encourages external orchestration;
- makes manual authoring error-prone;
- leaks repository implementation mechanics into semantic interaction.

## Principle Affected

Semantic interaction should expose meaning, not repository mechanics.

Stable UUID identity remains appropriate internally.

The problem is requiring normal semantic workflows to manipulate that identity directly.

## What Already Works

- UUID identity is stable and unambiguous.
- Prefix resolution reduces the need to type full UUIDs.
- ambiguity is correctly detected;
- short unsafe prefixes are rejected.

The problem is therefore interaction ergonomics, not identity semantics.

## Problem Boundary

This finding does not imply that:

- ElementId should stop being UUID-based;
- titles should become canonical identities;
- ObjectId semantics should change;
- immutable versions should change.

## Candidate Directions

Investigate:

- semantic references;
- transaction-local aliases;
- richer identifier resolution;
- reusable authoring references;
- batch authoring.

Exact syntax is not yet decided.

---

# F-002: Non-Trivial Graph Construction Encourages External Orchestration

## Observation

The current CLI models semantic mutations as individual commands.

This becomes expensive when constructing a non-trivial graph.

## Evidence

The Statit graph required approximately:

```text
213 mutation-related CLI operations
```

for:

```text
70 elements
110 relationships
21 artifact baselines
6 Changes
```

The construction agent responded by creating a Python orchestration script around the KAT CLI.

All actual KAT mutations still used supported KAT commands, but the script became necessary to make the workflow manageable.

## Impact

This indicates that the effective authoring abstraction is lower-level than the user's conceptual operation.

The user often thinks in terms of:

```text
model the workout subsystem
```

but the CLI requires:

```text
begin
create
create
create
link
link
link
link
...
commit
```

External scripting:

- increases setup cost;
- duplicates functionality each agent/user may rebuild independently;
- requires parsing CLI output;
- increases failure surface;
- weakens KAT's self-contained workflow.

## Principle Affected

The cost of maintaining semantic knowledge must remain lower than the repeated cost of rediscovering it.

If authoring and maintaining the semantic repository becomes too expensive, KAT loses its economic value even if retrieval works well.

## What Already Works

The Change transaction model is conceptually appropriate.

A user can stage many operations and commit them as one coherent semantic Change.

The issue is that each staged operation still requires a separate high-friction CLI interaction.

## Problem Boundary

This does not imply that KAT should automatically generate semantic graphs.

It also does not imply introducing AI-based authoring into the core.

The problem exists even for deterministic human or scripted workflows.

## Candidate Directions

Investigate:

- change-local aliases;
- multi-operation authoring input;
- batch creation/linking;
- declarative transaction input;
- structured stdin;
- improved command composability.

---

# F-003: Semantic Retrieval Is Effective but Can Be Fragmented

## Observation

Existing traversal primitives can recover useful semantic context, but inefficient command strategies can require many queries.

## Evidence

Feature Flag discovery initially used:

```text
29 KAT queries
19 kat show calls
```

A later disciplined run recovered effectively equivalent context with:

```text
9 KAT queries
6 kat impact calls
0 kat show calls
```

The semantic graph had not materially changed between these tests.

The difference came primarily from query strategy.

## Impact

This suggests that users and agents currently need to manually orchestrate several lower-level read operations to construct a useful semantic neighborhood.

Typical interaction can become:

```text
list
impact
show
show
show
trace
artifacts
...
```

when the real user goal is closer to:

```text
give me the semantic context around this responsibility
```

Fragmentation:

- consumes more commands;
- increases output volume;
- creates more opportunities for duplicated retrieval;
- makes it easier to miss already-retrieved context;
- forces agents to manually assemble result sets.

## Principle Affected

KAT should make authoritative software knowledge easier to understand than rediscovering the same knowledge from physical artifacts.

## What Already Works

`kat impact` proved to be a high-value primitive.

`kat trace` and `--max-depth` provide meaningful bounded traversal.

The experiments suggest that the underlying graph traversal semantics are largely adequate.

The problem is therefore aggregation and interaction efficiency rather than a missing graph model.

## Problem Boundary

This finding does not justify changing relationship semantics or adding new ontology concepts.

It also does not yet prove that a specific command such as `kat context` is required.

## Candidate Directions

Investigate:

- deterministic context aggregation;
- reusable query DTOs;
- bounded semantic-neighborhood projections;
- context-oriented CLI output;
- query composition.

---

# F-004: CLI Output Is Human-Oriented but Not Machine-Efficient

## Observation

KAT currently provides human-readable terminal output and `--compact` variants, but no structured machine-readable output.

There is no `--json` mode.

## Evidence

Current output modes include:

```text
formatted human-readable text
compact text
collapsed trace trees
arrow-joined compact paths
tables
```

Agents and external scripts must parse these textual formats.

The Statit construction helper parsed creation output to recover generated UUID values.

Earlier unrestricted trace output reached approximately:

```text
19 KB
```

for a single query before v0.3 introduced collapsed rendering and traversal bounding.

## Impact

Human-formatted output creates unnecessary overhead for:

- agents;
- scripts;
- IDE integrations;
- future extensions;
- automated testing of CLI behavior.

It also increases token consumption because textual formatting and repeated labels are optimized for terminal readability rather than structured consumption.

Parsing presentation text is brittle.

## Principle Affected

Canonical semantic information should be available independently of a particular presentation format.

## What Already Works

v0.3 significantly improved trace scalability through:

- collapsed tree rendering;
- `--max-depth`;
- `--compact`;
- explicit `--paths`.

These changes addressed output explosion but not structured consumption.

## Problem Boundary

This does not imply replacing human-readable CLI output.

Human-readable output remains necessary.

The requirement is separation between presentation-oriented output and machine-oriented output.

## Candidate Directions

Investigate a stable structured output mode, potentially JSON, across query and mutation commands.

---

# F-005: Mechanical Validity Does Not Guarantee Semantic Graph Usefulness

## Observation

KAT correctly validates structural and ontology invariants, but a graph can be mechanically valid while being weak for semantic retrieval.

## Evidence

Before Statit's semantic provenance refinement:

- the graph reported zero mechanical violations;
- important functional requirements were not connected to the product Intent;
- some major semantic entry points produced weak provenance paths.

After adding legitimate `motivates` relationships, top-level product intent traversal became substantially more useful.

No previous mechanical invariant had been violated.

Current validation detects conditions such as:

```text
unknown relationship type
invalid source type
invalid target type
duplicate relationship triple
missing endpoint
```

but does not detect:

```text
isolated element
Requirement with no realizing Implementation
Implementation with no representing Artifact
Design Decision with no consequence path
weak provenance
semantic redundancy
vague textual content
```

## Impact

Users may interpret:

```text
0 violations
```

as indicating a high-quality semantic repository.

That is too strong.

The actual meaning is:

```text
the repository satisfies currently enforceable mechanical invariants
```

A graph may still be operationally weak for:

- traceability;
- impact analysis;
- semantic routing;
- context discovery.

## Principle Affected

Validation should distinguish what KAT can prove mechanically from what can only be diagnosed heuristically.

## What Already Works

KAT already separates:

```text
mechanical violations
mechanically unverified Constraints
Validation evidence coverage
```

This provides a strong conceptual precedent for keeping graph-quality diagnostics separate from mechanical correctness.

## Problem Boundary

Semantic-quality warnings must not become arbitrary hard validity rules.

For example:

```text
Requirement without Implementation
```

may be legitimate during early design.

Likewise:

```text
Artifact without Validation
```

may be entirely valid.

## Candidate Directions

Investigate advisory semantic-quality diagnostics such as:

- isolated active elements;
- Requirements without realization paths;
- Implementations without Artifact mappings;
- Design Decisions without consequence paths;
- disconnected provenance;
- unusually high relationship fan-out.

These should remain advisory unless an actual invariant is violated.

---

# F-006: Selective Artifact Representation Is Effective

## Observation

KAT does not need to represent every repository file as an Artifact to provide useful physical context routing.

## Evidence

Statit contains more than 100 physical files but only 21 KAT Artifacts.

During the rest-timer experiment, several necessary files were not directly represented as KAT Artifacts.

Examples included:

```text
routine_models.dart
workout.dart
exercise_editor_sheet.dart
```

The KAT-guided agent still discovered these files by following local physical dependencies after KAT had routed it into the correct implementation neighborhood.

During implementation, no missing Artifact caused broad repository rediscovery.

## Impact

This validates an important scalability property.

KAT can operate as:

```text
semantic responsibility
    ↓
selected physical anchors
    ↓
local structural/code navigation
```

rather than:

```text
semantic graph
    ↓
complete copy of physical repository structure
```

This reduces graph maintenance cost and avoids duplicating information already available through source-navigation tools.

## Principle Supported

The semantic model should reduce physical complexity rather than reproduce it.

## What Already Works

The current Artifact model supports multiple physical artifacts representing a shared Implementation responsibility.

This provides semantic compression without losing access to implementation neighborhoods.

## Problem Boundary

Selective representation must not become excessive sparsity.

If an important physical responsibility cannot be located reliably from any semantic anchor, additional Artifact representation may still be appropriate.

## Candidate Directions

No immediate semantic change is required.

Future graph-quality tooling may help identify important Implementation responsibilities with weak or missing Artifact routes.

---

# F-007: Retrieved Context Can Be Lost During Manual Context Assembly

## Observation

Successful semantic retrieval does not guarantee that the final context set assembled by an agent preserves all retrieved information.

## Evidence

During the Statit KAT-assisted discovery:

```text
routine_repository.dart
```

was correctly surfaced through the `Routine Day Management Service`.

The agent correctly discussed it as potentially requiring modification.

However, it accidentally omitted the file from its final ready-to-implement context set.

During implementation, the file was rediscovered and was confirmed to require modification.

## Impact

The retrieval layer succeeded.

The failure occurred during manual result synthesis.

This indicates a second context problem:

```text
semantic retrieval
    ↓
candidate context assembly
```

KAT currently provides information for the first stage but leaves the second entirely to the consumer.

Manual assembly can:

- drop retrieved elements/artifacts;
- introduce inconsistent classifications;
- create duplicate context;
- lose provenance for why something was included.

## Principle Affected

Semantic retrieval should remain structurally usable after the query completes.

## What Already Works

Traversal operations expose the necessary graph information.

The problem is preservation and aggregation of that information into a stable result structure.

## Problem Boundary

KAT should not decide which files must be edited.

That remains a development decision.

The problem is preserving the retrieved semantic neighborhood, not predicting code modifications.

## Candidate Directions

Investigate:

- structured context-result DTOs;
- deterministic grouping of retrieved semantic roles;
- context-oriented query output;
- machine-readable result preservation.

---

# F-008: KAT Reduces Discovery Cost, Not the Inherent Size of a Change

## Observation

The Statit experiment showed that KAT reduced the work needed to find the correct implementation context, but both implementations ultimately modified essentially the same production-code surface.

## Evidence

Discovery:

```text
Baseline:
17 directory/tree operations
4 searches
14 physical files inspected

KAT:
1 directory/tree operation
2 searches
11 physical files inspected
```

Implementation:

Both implementations converged on approximately the same nine production files.

The KAT implementation required no broad repository rediscovery.

## Impact

This refines the expected KAT value proposition.

KAT should not be expected to reduce:

```text
the number of physical files inherently required by a feature
```

Instead, it should reduce:

```text
the cost of discovering which files and responsibilities matter
```

## Principle Supported

KAT is a semantic routing and knowledge repository layer, not a replacement for implementation work or structural source-code navigation.

## Candidate Formulation

KAT acts as a semantic routing layer that reduces repository-wide discovery by directing developers and agents toward high-value implementation neighborhoods.

Once there, ordinary structural navigation completes the physical context.

---

# F-009: Correct CLI Behavior Should Reduce Dependence on Agent-Specific Rules

## Observation

Several agent mistakes observed during experiments were preventable through better base-tool ergonomics.

Examples included:

- excessive `show` calls;
- internal `.kat` inspection before ontology discovery existed;
- redundant traversal;
- manual UUID bookkeeping;
- manually assembling context sets;
- parsing human-formatted output.

## Impact

It would be possible to address these through agent-specific prompts or skills.

However, doing so before improving the base tool would encode workarounds around KAT's current interaction weaknesses.

## Principle Affected

The correct workflow should be the easiest workflow.

## Direction

KAT core should first become:

```text
consistent
discoverable
efficient
deterministic
machine-consumable
difficult to misuse
```

Only after this interaction model is stable should agent-specific extensions encode best practices around it.

---

# 4. Cross-Cutting Design Principles Emerging From the Findings

The experiments suggest several principles that may deserve explicit inclusion in KAT's design documentation.

## 4.1 Semantic Interaction Should Expose Meaning, Not Repository Mechanics

Internal identity, hashing, revisioning, canonical CBOR, and immutable storage are essential KAT implementation mechanisms.

Normal user interaction should primarily expose:

```text
Intent
Requirement
Constraint
Design Decision
Implementation
Artifact
Validation
Change
semantic context
```

Users should not need to reason continuously about internal storage or identity mechanics.

---

## 4.2 Semantic Maintenance Must Be Economically Sustainable

The cost of authoring and maintaining semantic knowledge must remain lower than the repeated cost of rediscovering that knowledge.

This is a practical constraint on KAT's usefulness.

A semantically powerful graph that is prohibitively expensive to maintain does not satisfy KAT's objective.

---

## 4.3 Semantic Compression Is Desirable

KAT should not reproduce the entire physical repository.

A smaller set of semantic responsibilities and selected Artifact anchors should provide useful routes into the physical implementation.

Ordinary structural tools remain responsible for fine-grained physical navigation.

---

## 4.4 Mechanical Validity and Semantic Quality Are Different

KAT should continue to distinguish:

```text
what is invalid
what is unverified
what is weak or suspicious
```

Semantic-quality diagnostics should not silently become mechanical invariants.

---

## 4.5 Efficient Retrieval Should Be Deterministic

A useful semantic context query should not require AI reasoning inside KAT.

Aggregation should be based on explicit graph structure, traversal rules, ontology semantics, and repository state.

Agent-specific interpretation can exist above this deterministic core.

---

# 5. Problem Prioritization

Based on repeated empirical evidence, the problems appear to have the following priority.

## P0 - Authoring Efficiency

Includes:

```text
F-001 identity plumbing
F-002 external orchestration
```

Why P0:

The semantic repository must be constructed before any downstream value can exist.

High authoring cost directly threatens adoption and the economic viability of KAT.

---

## P0 - CLI and Retrieval Efficiency

Includes:

```text
F-003 fragmented retrieval
F-004 machine-inefficient output
F-007 context preservation
```

Why P0:

The graph already contains useful information.

The primary remaining issue is extracting that information with fewer interactions and less processing overhead.

---

## P1 - Graph Quality Diagnostics

Includes:

```text
F-005 mechanically valid but weak graphs
```

Why P1:

This materially affects retrieval quality, but it should be implemented carefully because useful graph shape is partly contextual and cannot always be expressed as a hard invariant.

---

## Validated Design Property

```text
F-006 selective Artifact representation
F-008 discovery-cost reduction
```

These are not problems to fix.

They are properties that future work should preserve.

---

# 6. Constraints on Future Solutions

Any v0.4 solution should preserve the following existing semantics.

## 6.1 Stable Identity

ElementId, RelationshipId, ChangeId, and related stable identities remain UUID-based.

Interaction improvements must not weaken identity semantics.

## 6.2 Immutable Canonical Objects

Object identity remains SHA-256 over deterministic canonical representation.

No CLI ergonomics improvement should change canonical object identity semantics unnecessarily.

## 6.3 Explicit Changes

Semantic evolution remains represented through explicit Change operations and accepted ChangeRevisions.

Batch authoring should preserve this transaction model rather than bypass it.

## 6.4 Specification-First Authority

KAT remains a semantic software repository.

It must not become:

- a filesystem index;
- a source-code graph;
- a Git replacement;
- a passive AST builder;
- an AI-first development tool.

## 6.5 Selective Physical Representation

KAT should not require every physical repository file to become an Artifact.

## 6.6 Deterministic Core

Core query and authoring behavior should remain deterministic and usable without an AI agent.

---

# 7. Deferred Extension Layer

Agent-specific integration remains a valid future direction.

A future Agent Extension could provide:

- agent rules;
- skills;
- reusable prompts;
- tool wrappers;
- recommended retrieval strategies;
- model-specific integration;
- MCP or similar interfaces.

For example, it could teach agents to:

```text
start from a small number of semantic entry points
prefer aggregated/impact traversal over repeated show
never inspect .kat internals
treat Artifact mappings as routing anchors
use physical search only when semantic retrieval leaves a concrete gap
distinguish CURRENT accountability from physical verification
```

However, this should be built on top of a stable and efficient KAT core.

Agent extensions should encode good usage of KAT rather than compensate for avoidable core CLI weaknesses.

---

# 8. Open Questions for v0.4 Definition

The findings justify investigation of the following questions.

## Authoring

1. How should users reference elements without manually managing UUIDs?
2. Should semantic references be persistent or transaction-local?
3. Is change-local aliasing sufficient?
4. Should KAT support batch/declarative authoring?
5. How can bulk authoring preserve clear Change semantics?
6. How should machine clients receive newly created identities?

## Retrieval

7. Is a deterministic aggregated context query required?
8. What graph neighborhood should such a query include?
9. Should context aggregation distinguish rationale, implementation, artifacts, validation, and dependents?
10. What should default traversal bounds be?
11. How should context results preserve provenance?

## Output

12. Should all CLI commands support structured output?
13. What output schema stability guarantees should KAT provide?
14. Should structured output be JSON, CBOR diagnostic form, or another representation?
15. How should human output and machine output evolve independently?

## Graph Quality

16. Which graph-quality conditions are useful enough to diagnose?
17. Which should be warnings versus informational observations?
18. How can KAT avoid imposing one "ideal graph shape" on every software architecture?
19. Can diagnostics remain ontology-extension-safe rather than hardcoding core types?

---

# 9. Next Step

The next step is to transform these empirically supported problems into explicit v0.4 requirements.

The sequence should remain:

```text
experimental findings
    ↓
problem definition
    ↓
design principles and constraints
    ↓
v0.4 requirements
    ↓
use cases
    ↓
operations
    ↓
CLI and semantic design
    ↓
implementation plan
```

No v0.4 command syntax or canonical-model change should be frozen until the requirements are derived from these findings.