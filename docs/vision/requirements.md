# KAT Product Requirements

## Purpose

This document defines the product-level functional requirements and release roadmap for the Knowledge Architecture Tool (KAT).

Requirements describe what KAT fundamentally accomplishes as a specification-first semantic repository. Release milestones describe the progressive capability increments demonstrated across releases.

---

## Primary System Goal

> **KAT must provide a specification-first semantic repository in which software knowledge is represented as authoritative, versioned, traceable, and evolvable independently from its physical artifacts.**

---

## Requirement Model

KAT requirements are structured around product capabilities rather than single-release feature lists:

```text
Project Primary Goal
    |
    +--> Core Product Requirements (R-001 .. R-009)
    |
    +--> Release Capability Increments
            |
            +--> v0.1: Semantic Repository Foundation
            +--> v0.2: Usable Semantic Evolution
            +--> Future: Distributed Collaboration & Automation
```

* **Product Requirements** define long-lived functional boundaries.
* **Release Milestones** define the specific degree of capability achieved in each version.

---

## Core Product Requirements

### R-001 Authoritative Knowledge Representation

KAT must represent software knowledge independently from physical artifacts. At minimum, the core semantic model must support:
* Intent (*Why does this exist?*)
* Requirements (*What is expected?*)
* Constraints (*What restricts state or decisions?*)
* Design Decisions (*What solution was chosen and why?*)
* Implementations (*What realizes intended behavior?*)
* Artifacts (*What concrete outputs represent or derive from knowledge?*)
* Validation evidence (*What evidence verifies expected properties?*)

#### Milestones
* **v0.1**: Implemented core ontology and immutable `KnowledgeElementVersion` storage.
* **v0.2**: Preserved ontology model; introduced rich identity discovery (`list`, `show`, unique prefix resolution).
* **v0.3**: Active ontology vocabulary and admissibility rules become directly discoverable (`kat ontology`, `kat ontology show`).

---

### R-002 Identity, Immutability, and History

KAT must preserve stable logical identities (`ElementId`, `RelationshipId`, `ChangeId`) distinct from content-addressed version object hashes (`ObjectId`). Accepted change history must remain immutable and traceable.

#### Milestones
* **v0.1**: Established content-addressed canonical object store and single-operation publication history.
* **v0.2**: Multi-operation `ChangeRevision` objects, compact tabular rendering, element history filtering (`history --element`), and explicit `AccountArtifact` history logging.
* **v0.3**: Preserved identity and history invariants across discovery, query inspection, validation, and draft UX.

---

### R-003 Explicit Semantic Evolution

KAT must evolve authoritative software knowledge through explicit Changes. A Change may contain one or more ordered mutation operations (`CreateElement`, `UpdateElement`, `DeprecateElement`, `SupersedeElement`, `Link`, `Unlink`, `AccountArtifact`) that together express one meaningful software evolution. Acceptance must be atomic: a Change becomes authoritative as a whole or not at all.

#### Milestones
* **v0.1**: Single-operation publication per `ChangeRevision`.
* **v0.2**: Staged multi-operation local draft sessions ($S_{\text{working}}$), candidate state validation, atomic publication with stale-base rejection.
* **v0.3**: Draft Changes expose clearer candidate effects, transaction-mode feedback, and candidate accountability previews in `kat change status`.

---

### R-004 Traceability and Provenance

KAT must navigate semantic relationships across abstraction levels and historical evolution. Traceability must explain why an element exists, what addresses or realizes it, and what artifacts depend on it.

#### Milestones
* **v0.1**: Origin trace (`trace`) using relationship-specific provenance traversal policies.
* **v0.2**: Enhanced CLI query formatting and relationship display.
* **v0.3**: Trace inspection scales through collapsed tree rendering, optional exhaustive path rendering (`--paths`), and bounded traversal presentation (`--max-depth`).

---

### R-005 Impact Analysis

KAT must identify knowledge elements and artifacts that may be affected by semantic evolution, distinguishing directly changed elements from transitively affected knowledge and accountable artifacts.

#### Milestones
* **v0.1**: Impact analysis (`impact`) using relationship-specific impact propagation policies.
* **v0.2**: Partitioned impact reporting (`Directly Changed Elements`, `Semantically Affected Elements`, `Accountable Artifacts`).
* **v0.3**: Impact results gain collapsed tree rendering and bounded propagation presentation (`--max-depth`).

---

### R-006 Semantic Consistency Validation

KAT must mechanically validate all structural and domain rules for which executable semantics are defined. Rules that cannot be mechanically verified (such as semantic `Constraint` elements without executable rules) must remain identifiable as unverified rather than being silently assumed compliant.

#### Milestones
* **v0.1**: Accepted state structural and ontology validity checking (`validate`).
* **v0.2**: Pre-commit candidate-state validation for staged multi-operation local drafts.
* **v0.3**: Validation distinguishes mechanical violations, mechanically unverified constraints, and linked validation evidence coverage (`kat validate --coverage`).

---

### R-007 Artifact Accountability

KAT must preserve direct semantic accountability between Artifact elements and the knowledge they reference (`kat.core/represents`, `kat.core/derived-from`). KAT must determine whether recorded baselines remain aligned with target knowledge versions (`CURRENT`, `STALE`, `UNACCOUNTED`). Artifact accountability must not imply physical file verification, and physical edits must not independently redefine authoritative state.

#### Milestones
* **v0.1**: Direct accountability relationships, initial relationship baselines, and artifact accountability analysis.
* **v0.2**: Explicit `AccountArtifact` reconciliation and refined `CURRENT` / `STALE` / `UNACCOUNTED` status reporting.
* **v0.3**: Artifact-accountability inspection exposes baseline vs current target version differences more clearly (`kat artifacts --stale`, per-artifact detail).
* **Future**: Optional physical artifact verification bridges and file drift detection.

---

### R-008 Safe Collaborative Evolution

KAT must permit multiple participants or repository instances to evolve semantic knowledge without reducing semantic conflict to file-level merging. Conflicting evolution must not silently enter accepted state.

#### Milestones
* **v0.1**: Not addressed.
* **v0.2**: Single local draft session per repository, stale-base conflict rejection at commit time, no automatic merge/rebase.
* **v0.3**: Preserved single local draft session model; enhanced candidate status and transaction-mode feedback.
* **Future**: Distributed repository synchronization, multi-branch proposal graphs, semantic merge, and conflict reconciliation.

---

### R-009 Materialization Boundary

KAT must permit authoritative semantic knowledge to be realized through concrete artifacts without transferring specification authority from the semantic model to those artifacts.

#### Milestones
* **v0.1 / v0.2 / v0.3**: Established conceptual materialization model and semantic artifact accountability boundary.
* **Future**: Materialization rules, generator plugins, reverse specification inference.

---

## Release Milestones

### v0.1 - Semantic Repository Foundation

#### Goal
Demonstrate that authoritative software knowledge can be represented, persisted, evolved, traced, analyzed, and validated independently from source code history.

#### Included Capabilities
* Canonical object storage for elements, relationships, changes, and ontologies.
* Single-operation Change publication.
* Command-line interface for `init`, `create`, `update`, `deprecate`, `supersede`, `link`, `unlink`, `show`, `trace`, `impact`, `validate`, `history`.
* Direct accountability relationships, initial relationship baselines, and artifact accountability analysis.

#### Explicitly Deferred
* Multi-operation Changes.
* Staged local draft sessions.
* Artifact accountability reconciliation (`AccountArtifact`).
* Distributed collaboration and branch merging.
* Materialization code generators.

---

### v0.2 - Usable Semantic Evolution

#### Goal
Make the semantic repository practical for deliberate multi-operation software evolution and explicit artifact accountability.

#### Included Capabilities
* **Discovery & Interaction**: `kat list`, `kat show`, `kat artifacts`, 8-character unique hex prefix resolution, compact table views.
* **Multi-Operation Evolution**: Single local draft session, staged operations, candidate state $S_{\text{working}}$, atomic publication, stale-base rejection.
* **Artifact Accountability**: `CURRENT` / `STALE` / `UNACCOUNTED` status evaluation via `kat artifacts`, plus explicit `kat account` / `AccountArtifact` baseline reconciliation in accepted `ChangeRevision` history.

#### Explicitly Deferred
* Distributed remote repository fetch/push.
* Branching and multi-head accepted history.
* Automatic semantic merge and conflict resolution.
* Physical file hashing or physical drift detection.
* Materialization generators/plugins.

---

### v0.3 - Semantic Discoverability and Inspection

#### Goal
Make KAT's semantic model easier to understand, inspect, and evolve without requiring users to know repository internals or discover semantic rules through trial and error.

#### Included Capabilities
* **Ontology Discovery**: Inspect active element types, relationship types, and endpoint combinations directly from the CLI (`kat ontology`, `kat ontology show`).
* **Scalable Traceability Inspection**: Collapsed tree rendering for `trace` and `impact`, optional exhaustive path rendering (`--paths`), and bounded traversal depth (`--max-depth`).
* **Validation Clarity**: Distinguish mechanical violations from mechanically unverified constraints, and surface linked validation evidence separately (`kat validate --coverage`).
* **Change Authoring UX**: Transaction-mode feedback for staged vs standalone mutations, and expanded `kat change status` candidate status rendering.
* **Artifact Accountability Inspection**: Filtered inspection of stale artifacts (`kat artifacts --stale`) and detailed baseline vs current target version differences.

#### Explicitly Deferred
* New core ontology relationships.
* Executable constraint rules.
* Distributed collaboration and semantic merge.
* Physical artifact verification.
* Materialization plugins.

---

## Release Requirements Matrix

| Requirement | v0.1 Foundation | v0.2 Usable Evolution | v0.3 Discoverability & Inspection | Future Scope |
| :--- | :--- | :--- | :--- | :--- |
| **R-001 Knowledge Representation** | Core ontology & versions | Enhanced discovery & lookup UX | Ontology discovery (`kat ontology`) | Domain extensions |
| **R-002 Identity & History** | Content-addressed store | Multi-op revisions, history filtering | Preserved identity & history invariants | Distributed graphs |
| **R-003 Semantic Evolution** | Single-operation Changes | Multi-op staged drafts, atomic publication | Transaction feedback & candidate status | Collaborative merge |
| **R-004 Traceability** | Provenance traversal policies | Enhanced CLI presentation | Collapsed tree rendering & `--max-depth` | Cross-repository trace |
| **R-005 Impact Analysis** | Propagation policies | Partitioned impact reporting | Collapsed impact trees & `--max-depth` | Change simulation |
| **R-006 Consistency Validation** | Structural & ontology rules | Candidate-state pre-commit validation | Mechanical vs unverified classification & coverage | Executable constraint engine |
| **R-007 Artifact Accountability** | Direct edges & baselines | `CURRENT / STALE / UNACCOUNTED`, `AccountArtifact` | `kat artifacts --stale` & baseline diffs | Physical drift verification |
| **R-008 Safe Collaboration** | Deferred | Local draft, stale-base rejection | Candidate status & transaction feedback | Remote sync & semantic merge |
| **R-009 Materialization Boundary** | Conceptual boundary | Conceptual boundary & accountability | Conceptual boundary & accountability | Generator plugins & reverse inference |

