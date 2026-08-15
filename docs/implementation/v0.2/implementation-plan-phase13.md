# Phase 13 Implementation Plan: Multi-Operation Change — Design

> Part of the [v0.2 master plan](../implementation-plan.md).

## Purpose

Phase 13 is a **design-only** phase. It produces a frozen design for staging **several semantic operations into one `ChangeRevision`**, published atomically — the "Meaningful changes" pillar of v0.2.0. No implementation happens here (implementation is Phase 14, which may only begin after this design is approved and frozen).

The v0.1 experiment revealed that one conceptual change often became several independent revisions:

> "Add reduced-motion support" → `Update Requirement R3` + `Update Implementation I3` + `Update Validation V1` + re-account `styles.css`

That contradicts the spirit of the existing change model — `docs/change-model.md` already states that _a Change may contain multiple semantic Operations that together represent one meaningful evolution_. Phase 13 designs how to finally expose that capability without weakening semantics.

**Canonical impact (expected): NONE.** The canonical format already represents a `ChangeRevision` as an array of operations (`operations` is a list, CDDL unchanged). Phase 13 must confirm this and must not change `spec/canonical-format.cddl`.

---

## 1. Design Space & Required Decisions

The design document (see Step 13.4) must answer, at minimum:

### 1.1 Staging model

- **Working candidate state**: a staged change holds a _working semantic candidate_ derived from the accepted state $S_n$.
- Decision: does the session operate **in memory only** (per `kat change begin ... commit` invocation sequence in one CLI lifetime) or is it **persisted on disk** (survives process restarts, e.g. a `changes/` directory with a session file)?
  - Recommended default to evaluate: **persisted session file** so a real workflow can interleave `kat change status` / edits across commands; but this adds a new on-disk artifact that is NOT a canonical object — its format is private, local, and explicitly non-canonical. The design must weigh simplicity (v0.2 KISS) vs. the real-experiment workflow.
- No accepted intermediate states: a change session never publishes anything until `commit`.

### 1.2 Transaction lifecycle & CLI

- `kat change begin --description "..."` → opens a session on accepted $S_n$.
- Subsequent operations target the **working candidate**: `kat update <id> ...`, `kat deprecate <id>`, `kat create ...`, `kat link ...`, `kat unlink ...`, (Phase 15) `kat account ...`.
- `kat change status` → session summary (description, operation count, per-operation summary, candidate validation status).
- `kat change commit` → validate the **whole candidate**, build **one** `ChangeRevision` with `operations = [O1, O2, ..., On]`, persist, publish once via CAS → accepted $S_{n+1}$.
- `kat change abort` → discard the session; repository untouched.
- Do not confuse `kat change status` (staged change) with `kat status` (repository dashboard).

### 1.3 Atomicity & failure semantics

- Atomicity: one revision, one persist, one CAS. Either the whole change is accepted or none of it is.
- Failure semantics:
  - An operation that fails preconditions mid-session → the session remains valid/abortable; previously applied ops in the session are retained or rolled back (design decision; recommend: keep session consistent, report the failing op).
  - Whole-candidate validation (ontology + invariants) fails at `commit` → commit rejected, session preserved for correction, or aborted (design decision).
  - CAS conflict at publish (concurrent writer won) → commit fails with `Conflict`; session is not silently merged (consistent with v0.1 conflict behaviour).

### 1.4 Engine API design

- How the existing typestate pipeline is reused: per-operation apply/ontology/invariant stages already exist (`apply_create_element`, `apply_update_element`, …). The session applies each op to the **working candidate** instead of a fresh base, then runs whole-candidate invariants once.
- `expected_version` semantics inside a session: each op's `expected_version` resolves against the **working candidate** state (op 2 sees the effect of op 1), preserving the v0.1 optimistic-concurrency meaning within the session.
- Interaction rules to define: operations on the same element within one session (e.g. `update` then `deprecate`), duplicate operations, and ordering determinism of `operations` in the final revision.
- The final revision construction reuses `prepare_change_revision`-style logic generalized to `Vec<ValidatedOperation>` → one `ChangeRevision`; persist order and identity verification extended to all objects.

### 1.5 CLI interaction & persistence of sessions

- Full UX for `begin/status/commit/abort` (output shapes, error messages, exit codes).
- Where/when a session file is written/locked/cleaned; concurrency between sessions (design decision: at most one open session per repository? recommended default).

### 1.6 Documentation updates

- `docs/change-model.md` (multi-op semantics, working candidate), `docs/operations.md` (session-level workflow), `docs/cli.md` (`kat change *`), `docs/implementation/` tracker.

---

## 2. Work Breakdown & Implementation Steps

> Each step is atomic and validated before the next. Phase 13 produces **documents only** — no `src/` changes.

### Step 13.1 — Staging & session design

- Write the staging-model section of the design doc: working candidate, in-memory vs on-disk session (with a recommendation), session identity, open-session constraints, lifecycle (`begin`/`status`/`commit`/`abort`).
- **Validation**: design doc section complete and self-consistent; no code.

### Step 13.2 — Engine & atomicity design

- Write the engine-API and atomicity section: per-op application to the working candidate, `expected_version` semantics within a session, whole-candidate ontology + invariant validation, one-revision/persist/CAS, failure and conflict semantics, operation interaction rules.
- Cross-check against `docs/change-model.md`, `docs/invariants.md`, `spec/canonical-format.cddl` (confirm no canonical change needed; if a change IS needed, this phase must surface it as a blocker for approval).
- **Validation**: design doc section complete; no code.

### Step 13.3 — CLI & UX design

- Write the CLI/UX section: `kat change begin/status/commit/abort` interface, output shapes, exit codes, error mapping, session-file format (if on-disk) and its non-canonical status.
- **Validation**: design doc section complete; no code.

### Step 13.4 — Design review, freeze & docs

- Consolidate the full design into `docs/v0-2-multi-op-change-design.md`; update `docs/change-model.md`, `docs/operations.md`, `docs/cli.md` to reflect the frozen design.
- **Design approval gate**: the design must be reviewed and approved before Phase 14 implementation begins. No implementation in Phase 13.
- **Validation**: docs committed; design frozen.

---

## 3. Acceptance Scenario (design review)

```text
A reader can answer, from the design doc alone:
  - where the working candidate lives (memory or disk) and its format status
  - how operations see each other's effects (expected_version semantics)
  - what happens on: op precondition failure, whole-candidate invariant failure, CAS conflict at commit
  - the exact CLI surface for begin/status/commit/abort
  - confirmation that spec/canonical-format.cddl is unchanged (or a flagged blocker)
```

---

## 4. Definition of Done for Phase 13

- [x] `docs/v0-2-multi-op-change-design.md` written and covers: staging model, transaction lifecycle, atomicity, failure semantics, engine API, CLI UX, session persistence, operation interaction rules.
- [x] Canonical format impact confirmed NONE (or blocker surfaced for approval).
- [x] `docs/change-model.md`, `docs/operations.md`, `docs/cli.md` updated to the frozen design.
- [ ] Design approved before Phase 14 begins.
- [x] No `src/` changes in Phase 13.
