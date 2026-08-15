# KAT Documentation

This directory contains the specifications, architectural design documents, user guides, and implementation plans for the KAT software knowledge tracking system.

## Directory Structure

```text
docs/
├── specification/       # Frozen Normative Specification & Protocol Models
├── vision/              # Architectural Vision, Philosophy & Requirements
├── user-guide/          # CLI User Documentation & Installation Guides
└── implementation/      # Crate Design & Version Milestone Plans
```

---

## 1. Specification (`docs/specification/`)

The normative, protocol-level specifications defining KAT's data models, storage contracts, operational rules, and domain invariants:

* **[canonical-format.md](specification/canonical-format.md)**: Binary CBOR representation, envelope version 1, schema version 1, hashing, and structural validation.
* **[operations.md](specification/operations.md)**: Operations 1..7 operational contracts ($S_{\text{working}}$ candidate state mechanics) and query contracts.
* **[change-model.md](specification/change-model.md)**: Change evolution, draft staging, atomic publication, state identity vs revision identity.
* **[invariants.md](specification/invariants.md)**: Semantic invariants governing accepted software knowledge states.
* **[repository-model.md](specification/repository-model.md)**: Physical repository layout, content-addressed object storage, reference pointers, and storage integrity.
* **[domain-model.md](specification/domain-model.md)**: Conceptual domain entities, elements, relationships, and ontology bindings.
* **[ontology.md](specification/ontology.md)**: Core ontology definitions (`kat.core/*`) and relationship rules.
* **[materialization-model.md](specification/materialization-model.md)**: Mapping semantic states to physical software workspace files.
* **[collaboration-model.md](specification/collaboration-model.md)**: High-level concepts for distributed collaboration, branches, and conflict resolution.

---

## 2. Vision & Foundations (`docs/vision/`)

High-level rationale, architectural philosophy, and project requirements:

* **[architecture.md](vision/architecture.md)**: Overall system architecture and component interactions.
* **[first-principles.md](vision/first-principles.md)**: Foundational principles guiding KAT design.
* **[philosophy.md](vision/philosophy.md)**: Design philosophy and core motivation.
* **[concepts.md](vision/concepts.md)**: Core concepts overview.
* **[requirements.md](vision/requirements.md)**: Functional and non-functional system requirements.
* **[use-cases.md](vision/use-cases.md)**: Practical workflows and user scenarios.
* **[non-goals.md](vision/non-goals.md)**: Explicit non-goals for KAT.

---

## 3. User Guide (`docs/user-guide/`)

User-facing guides and CLI documentation:

* **[cli.md](user-guide/cli.md)**: Command-line interface reference and usage syntax.
* **[cli-presentation.md](user-guide/cli-presentation.md)**: Terminal presentation, formatting, and UX standards.
* **[install.md](user-guide/install.md)**: Building and installing KAT.

---

## 4. Implementation Details & Milestone Plans (`docs/implementation/`)

Technical crate design, phase roadmaps, and release review reports:

* **[master-plan.md](implementation/master-plan.md)**: Master implementation roadmap.
* **[prototype-design.md](implementation/prototype-design.md)**: Rust crate architecture, module structure, and storage layout.
* **[v0.1/](implementation/v0.1/)**: Phase 0..10 implementation plans and the v0.1 release acceptance review.
* **[v0.2/](implementation/v0.2/)**: Phase 11..16 implementation plans and feature design specifications (Phase 14 multi-op staging & Phase 15 artifact re-accountability).
