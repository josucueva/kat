# KAT v0.3 Implementation Plan

This document is the master tracker for **KAT v0.3** (Phases 17–21). It follows the same workflow as v0.1 and v0.2: every step is **atomic**, is **validated before the next step** (`cargo test`, `cargo fmt --check`, `cargo clippy -D warnings`), and is committed after validation. Per-phase detail lives in dedicated files linked from the [Phases](#phases) index. The top-level master tracker is `docs/implementation/master-plan.md`.

Status: **v0.3.0 COMPLETED** — All Phases (17–21) implemented and verified (399 tests passing).

---

## v0.3.0 Release Purpose

> **Make KAT's semantic model easier to understand, inspect, and evolve without requiring users to know repository internals or discover semantic rules through trial and error.**

v0.3 focuses on **Semantic Discoverability and Inspection**. It takes the lessons learned from the real-project evaluation (`docs/implementation/v0.3/experiment.md` and `docs/implementation/v0.3/experiment-analysis.md`) and addresses CLI discoverability, query usability, validation clarity, and Change-authoring ergonomics without expanding the core ontology prematurely.

Five pillars:

1. **Ontology Discovery** — inspect active element types, relationship types, and allowed endpoint combinations directly from the CLI (`kat ontology`, `kat ontology show`).
2. **Scalable Traceability Inspection** — default to collapsed semantic tree/graph output for `trace` and `impact`, with `--paths` for exhaustive path enumeration and `--max-depth` bounding.
3. **Validation Clarity** — explicitly distinguish mechanical violations from mechanically unverified constraints, and surface linked validation evidence separately.
4. **Change Authoring UX & Draft Inspection** — surface clear transaction-mode feedback on mutations, and expand `kat change status` to report candidate effects, validation status, and expected artifact staleness.
5. **Artifact Accountability Inspection** — enhance `kat artifacts` inspection (`--stale`, per-artifact detail) to clearly report recorded vs current target version differences.

---

## Authoritative Sources

The implementation must not independently redefine semantics. Ground every decision in:

| Concern | Normative source |
| :--- | :--- |
| Structural schema (object kinds, envelopes, operations, ordering) | `spec/canonical-format.cddl` |
| Encoding and semantics rules (deterministic CBOR, hashing, validation) | `docs/canonical-format.md` |
| Physical design, repository layout, phases, error categories | `docs/prototype-design.md` |
| Semantic operations, change model, invariants | `docs/specification/operations.md`, `docs/specification/change-model.md`, `docs/specification/domain-model.md` |
| Frozen specification set (v0.2) | `docs/specification/` (`repository-model.md`, `materialization-model.md`, `collaboration-model.md`, `ontology.md`) |
| Vision, Requirements roadmap, and Use Cases | `docs/vision/` (`philosophy.md`, `requirements.md`, `architecture.md`, `use-cases.md`) |

**v0.3 semantic guardrail:** every v0.3 feature must preserve the canonical format and invariants. Read-side features (Phases 17–19, 21) do not mutate the repository. Change-authoring features (Phase 20) operate strictly through the existing v0.2 multi-operation draft Change framework.

---

## v0.3.0 Scope & Priorities

| Priority | Capability | Phases |
| :--- | :--- | :--- |
| **P0** | Ontology Discovery (`kat ontology`, `kat ontology show`) | 17 |
| **P0/P1** | Scalable Trace & Impact Inspection (collapsed tree default, `--paths`, `--max-depth`) | 18 |
| **P1** | Validation Clarity (distinguish mechanical violations, unverified constraints, validation evidence) | 19 |
| **P1** | Change Authoring UX & Draft Inspection (`kat change status` candidate effects, transaction feedback) | 20 |
| **P2** | Artifact Accountability Inspection (`kat artifacts --stale`, per-artifact baseline diffs) | 21 |
| **P2** | Real-Project Evaluation (re-evaluate workflow and ergonomics on a real codebase) | 21 |
| **Out of scope** | New core ontology relationships (e.g. `enforces`, `validates -> Design Decision`) | Deferred |
| **Out of scope** | Executable constraint language, semantic merge, physical artifact verification | Deferred |

---

## Phases

Detailed, per-phase plans live in dedicated files so this master stays a lean tracker; each phase file links back here.

| Phase | File | Status |
| :--- | :--- | :--- |
| Phase 17 — Ontology Discovery (`kat ontology`, `kat ontology show`) | [implementation-plan-phase17.md](implementation-plan-phase17.md) | **completed** |
| Phase 18 — Scalable Query Inspection (`kat trace` / `impact` tree rendering & bounds) | [implementation-plan-phase18.md](implementation-plan-phase18.md) | **completed** |
| Phase 19 — Validation Result Classification & Coverage | [implementation-plan-phase19.md](implementation-plan-phase19.md) | **completed** |
| Phase 20 — Change UX & Draft Inspection | [implementation-plan-phase20.md](implementation-plan-phase20.md) | **completed** |
| Phase 21 — Accountability Inspection & v0.3 Release Acceptance | [implementation-plan-phase21.md](implementation-plan-phase21.md) | **completed** |

---

## Progress Log

| Date | Milestone / step completed | Notes |
| :--- | :--- | :--- |
| 2026-08-16 | v0.3 planning documents created | Master tracker + Phase 17–21 plans drafted from real-world experiment findings. |
| 2026-08-16 | Phase 17 — Ontology Discovery | Implemented `inspect_ontology`, `show_ontology_type`, CLI wiring, default & compact formatting, and acceptance tests. |
| 2026-08-17 | Phase 18 — Scalable Query Inspection | Implemented Query Engine `--max-depth <N>` evaluation bounding, `TraceResult::to_tree()` ASCII tree rendering, `--paths` explicit path list view, and end-to-end acceptance tests. |
| 2026-08-17 | Phase 19 — Validation Classification & Coverage | Implemented `kat validate` diagnostic classification (Mechanical Violations vs Unverified Constraints), `kat.core/validates` evidence tracking, `kat validate --coverage`, and acceptance tests. |
| 2026-08-17 | Phase 20 — Change UX & Draft Inspection | Implemented `inspect_draft_session`, `DraftSessionView`, `CandidateEffectSummary`, `kat change status` multi-section rendering, transaction feedback, and acceptance tests. |
| 2026-08-17 | Phase 21 — Accountability Inspection & v0.3 Acceptance | Implemented `ArtifactFilter`, `analyze_artifact_accountability_filtered`, `kat artifacts --stale`, `kat artifacts <id>`, and acceptance tests. |

---

## v0.3 Non-Goals (do not build yet)

```text
new core ontology relationships (enforces, validates -> design-decision)
executable constraint language / rule engine
branching & multi-head accepted states
distributed repositories & remote sync
automatic semantic merge
physical file content hashing / drift detection
code generation / materialization plugins
automatic Change granularity scoring
semantic diff & AI change explanation
```
