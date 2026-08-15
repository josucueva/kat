# KAT v0.2 Implementation Plan

This document is the master tracker for **KAT v0.2** (Phases 11–16). It follows the same workflow as v0.1: every step is **atomic**, is **validated before the next step** (`cargo test`, `cargo fmt --check`, `cargo clippy -D warnings`), and is committed after validation. Per-phase detail lives in dedicated files linked from the [Phases](#phases) index. The top-level, version-agnostic tracker is `docs/implementation/implementation-plan.md`.

Status: **v0.2.0 in planning** — planning documents drafted; **no v0.2 code has been written yet**. v0.1 is complete and released (v0.1.0 / v0.1.1, 346 tests passing).

---

## v0.2.0 Release Purpose

> **KAT v0.2.0 should make semantic repositories substantially easier to navigate, evolve, and operate while preserving the explicit, deterministic semantics established in v0.1.**

v0.2 is **not** "KAT gets more intelligent." It is: _KAT becomes practical to use repeatedly on a real evolving project._ It reduces the **operational** and **cognitive** cost of working with an existing semantic model, without weakening semantics.

Four pillars (from the v0.1 review — see `docs/v0-1-release-acceptance-review.md` and the v0.1 release evaluation):

1. **Discovery** — find knowledge without external UUID bookkeeping; inspect local semantic neighborhoods quickly.
2. **Concise interaction** — let users choose between compact and detailed output; make common queries readable at a glance.
3. **Meaningful changes** — group several semantic operations into one `ChangeRevision`; reduce command ceremony during real evolution.
4. **Artifact accountability UX** — replace unlink/link re-accountability with an explicit semantic operation.

## Authoritative Sources

The implementation must not independently redefine semantics. Ground every decision in:

| Concern                                                                | Normative source                                                   |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Structural schema (object kinds, envelopes, operations, ordering)      | `spec/canonical-format.cddl`                                       |
| Encoding and semantics rules (deterministic CBOR, hashing, validation) | `docs/canonical-format.md`                                         |
| Physical design, repository layout, phases, error categories           | `docs/prototype-design.md`                                         |
| Semantic operations, change model, invariants                          | `docs/operations.md`, `docs/change-model.md`, `docs/invariants.md` |
| v0.1 release guarantees (frozen semantics, acceptance matrix)          | `docs/v0-1-release-acceptance-review.md`                           |
| CLI presentation conventions (v0.1)                                    | `docs/cli.md`, `docs/cli-presentation.md`                          |

**v0.2 semantic guardrail:** every v0.2 feature must preserve the v0.1 canonical format and invariants unless a phase explicitly designs and freezes a canonical change (Phase 15 is the only phase expected to touch `spec/canonical-format.cddl`). Read-side features (Phases 11–12) must not mutate the repository.

## v0.2.0 Scope & Priorities

| Priority          | Capability                                                               | Phases       |
| :---------------- | :----------------------------------------------------------------------- | :----------- |
| **P0**            | `kat list` (removes immediate UUID bookkeeping)                          | 11           |
| **P0**            | unique-prefix ID resolution (improves virtually every command)           | 11           |
| **P0**            | relationships in `kat show` (local semantic navigation)                  | 11           |
| **P0**            | compact/read output modes (reduces cognitive load across the CLI)        | 12           |
| **P1**            | multi-operation Changes (removes evolution bureaucracy)                  | 13–14        |
| **P1**            | history `--oneline` / `--limit` / `--element` (needed as repos grow)     | 12           |
| **P1**            | first-class artifact re-accountability (`kat account`)                   | 15           |
| **P2 (deferred)** | structured `--json` output (renderer architecture only, no JSON promise) | 12 (arch)    |
| **P3 (research)** | executable constraint evaluation                                         | out of scope |

## Phases

Detailed, per-phase plans live in dedicated files so this master stays a lean tracker; each phase file links back here.

| Phase                                                  | File                                                             | Status  |
| ------------------------------------------------------ | ---------------------------------------------------------------- | ------- |
| Phase 11 — Discovery (`kat list`, ID prefixes, `show`) | [implementation-plan-phase11.md](implementation-plan-phase11.md) | **complete** |
| Phase 12 — Output Modes (compact/full rendering)       | [implementation-plan-phase12.md](implementation-plan-phase12.md) | **complete** |
| Phase 13 — Multi-Operation Change Design               | [implementation-plan-phase13.md](implementation-plan-phase13.md) | **design complete** |
| Phase 14 — Multi-Operation Changes (`kat change`)      | [implementation-plan-phase14.md](implementation-plan-phase14.md) | planned |
| Phase 15 — Artifact Re-accountability (`kat account`)  | [implementation-plan-phase15.md](implementation-plan-phase15.md) | planned |
| Phase 16 — Real-Project Evaluation                     | [implementation-plan-phase16.md](implementation-plan-phase16.md) | planned |

Phase ordering rationale: Discovery (11) fixes the biggest UX pain first and is purely read-side; Output Modes (12) generalizes presentation across the CLI; the multi-operation Change design (13) and implementation (14) are the largest workflow change; `account` (15) is the only canonical-format change and is therefore isolated last among feature phases; Evaluation (16) repeats the real-world experiment and decides v0.3 from evidence.

## Progress Log

| Date       | Milestone / step completed      | Notes                                                                                                                   |
| ---------- | ------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| 2026-08-15 | v0.2 planning documents created | Master tracker + Phase 11–16 plans drafted from the v0.1 review (real-world experiment findings). No v0.2 code written. |
| 2026-08-15 | Step 11.1 — `list_elements` query function | Implemented `ListFilter` and `list_elements` in `src/repository/query.rs` with type and lifecycle filters. Unit tests added. |
| 2026-08-15 | Step 11.2 — `kat list` CLI command | Implemented `kat list [<type>] [--type <type>] [--lifecycle <state>]` with compact table renderer in `src/main.rs`. |
| 2026-08-15 | Step 11.3 — Unique-prefix ID resolution | Implemented type-scoped resolvers in `src/repository/resolve.rs` with $\ge 8$ hex digit rule and 0/1/>1 deterministic outcomes. |
| 2026-08-15 | Step 11.4 — CLI prefix wiring | Wired unique prefix resolution into `show`, `update`, `deprecate`, `supersede`, `link`, `unlink`, `trace`, `impact`. |
| 2026-08-15 | Step 11.5 — Relationship-aware `kat show` | Extended `ElementView` with 1-hop `RelationshipNeighborhood` and rendered compact `Relationships` table in `kat show`. |
| 2026-08-15 | Step 11.6 — Phase 11 closure | Added end-to-end acceptance flow test `phase11_acceptance_cli_flow_end_to_end` in `tests/cli.rs`. Updated assets and `install.sh`. |
| 2026-08-15 | Step 12.1 — Shared output-mode CLI flags | Added `--compact` flag across all read subcommands (`status`, `show`, `history`, `trace`, `impact`, `validate`, `artifacts`). |
| 2026-08-15 | Step 12.2 — `kat history` options | Added `--oneline`, `--limit N`, `--element <id-or-prefix>` flags with `history_entry_touches_element` filter in `src/repository/query.rs`. |
| 2026-08-15 | Step 12.3–12.6 — Compact renderers & Phase 12 closure | Implemented compact renderers across read commands and added `phase12_acceptance_cli_flow_end_to_end` in `tests/cli.rs`. 255 tests passing. |
| 2026-08-15 | Phase 13 — Multi-Operation Change Design | Authored `docs/v0-2-multi-op-change-design.md` detailing staging, candidate state $S_{\text{working}}$, `.kat/draft.json` storage, failure modes, read query isolation, and CLI UX. |

## v0.2 Non-Goals (do not build yet)

Everything in the v0.1 non-goals list, plus, for v0.2:

```
branching
distributed repositories
remote synchronization
semantic merge
automatic conflict resolution
AI extraction
source parsing
filesystem watchers
automatic artifact reconciliation
new architecture-specific ontology
generic graph query language
plugin system
structured --json output        (deferred to P2; renderer architecture only)
executable constraints          (deferred to P3 / research — execution trust, environment,
                                 result persistence, security, reproducibility, evidence)
```

The v0.1 experiment did not show a need for any of these; v0.2 exists to make the existing model _practical_, not larger.
