# Phase 14 Implementation Plan: Multi-Operation Changes — `kat change begin/status/commit/abort`

> Part of the [v0.2 master plan](../implementation-plan.md).

## Purpose

Phase 14 implements the frozen design from **Phase 13** (`docs/v0-2-multi-op-change-design.md`). It exposes the v0.1 capability that _a `ChangeRevision` may contain multiple semantic operations_ as a real user workflow: stage several operations into a working candidate, inspect them, then commit once — producing **one** `ChangeRevision` with `operations = [O1, …, On]` published atomically.

Phase 14 may **only begin after the Phase 13 design is approved and frozen**. Any deviation from the frozen design must be flagged and re-approved.

---

## 1. Frozen Semantics (from Phase 13 design)

- **Working candidate**: a session derives a working semantic candidate from the accepted state $S_n$ at `begin`. All operations apply to the working candidate (each op sees prior ops' effects; `expected_version` resolves against the working candidate).
- **No accepted intermediate states**: nothing is persisted to `ObjectStore` and `refs/accepted` is untouched until `commit`.
- **`kat change commit`**: validate the whole candidate (ontology + invariants) → construct **one** `ChangeRevision` (`operations = [O1..On]`, dependencies from the accepted head) → persist all new objects → single CAS publish → accepted $S_{n+1}$.
- **Failure semantics** (per design): op precondition failure keeps the session usable; whole-candidate validation failure at `commit` rejects the commit and preserves/aborts the session (per design); CAS conflict at publish → `Conflict`, no silent merge.
- **`kat change abort`**: discard the session; repository byte-for-byte unchanged.
- Canonical format is unchanged (confirmed in Phase 13); only the engine + CLI gain session support.

---

## 2. Work Breakdown & Implementation Steps

> Each step is atomic: implement, add tests, then **validate** (`cargo test`, `cargo fmt --check`, `cargo clippy -D warnings`) before starting the next step. Commit after validation.

### Step 14.1 — Engine: session data structures

- Per the design doc, add session types (e.g. `ChangeSession { id, base_accepted, working_candidate, description, operations: Vec<SessionOperation> }`) and the open/close operations in `src/repository/change.rs` (or a new `src/repository/session.rs`).
- Session file format (if the design chose on-disk sessions) — private, local, **explicitly non-canonical**.
- At most one open session per repository (per design); opening a second → error.
- **Tests**: open on fresh repo; open with existing session → rejected; abort cleans up; no ObjectStore/accepted-ref changes at any point.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 14.2 — Engine: apply operations to the working candidate

- Generalize the existing apply/ontology/invariant stages to run against the working candidate: `update/deprecate/create/link/unlink` inside a session reuse `apply_*`, `validate_*_ontology`, `validate_*_invariants` on the candidate state.
- Each op records its inputs; `expected_version` for each op resolves against the current working candidate.
- **Tests**: sequential ops on the same element (update then deprecate); ops seeing prior effects (link after create); duplicate/conflicting ops rejected per design; precondition failures leave session consistent.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 14.3 — Engine: whole-candidate validation + single revision/persist/publish

- `commit_change_session`: validate the whole candidate (ontology preserved, invariants over the multi-op delta — per the Phase 13 design), build **one** `ChangeRevision` with the ordered operation list, derive IDs (encode-then-hash), persist all new objects in reference order with identity verification, then one CAS publish.
- Reject commit on whole-candidate validation failure (session preserved or aborted per design).
- **Tests**: multi-op revision contents and ordering; dependencies from accepted head; persist identity checks; single CAS; conflict → `Conflict`, winner kept; accepted ref points to $S_{n+1}$; fresh reopen verifies the whole chain.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 14.4 — CLI: `kat change begin/status/commit/abort`

- `src/cli.rs` + `src/main.rs`: `kat change` subcommands.
  - `kat change begin --description "..."` → opens a session, prints session/description.
  - Mutation commands inside a session: `kat update/deprecate/create/link/unlink` detect an open session and apply to the working candidate (printing per-op results).
  - `kat change status` → session summary (description, operations, candidate validation status).
  - `kat change commit` → engine commit; prints stable IDs (`change_id`, `change_revision_id`, `state_id`).
  - `kat change abort` → discard; exit 0.
  - `kat change status`/`commit`/`abort` with no session → clear error, exit 1.
- **Tests** (`tests/cli.rs`): full begin→ops→status→commit flow; begin→abort; commit with validation failure → rejected; commit conflict; no-session errors; end-to-end history shows ONE revision with multiple operations.
- Update `docs/cli.md`.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 14.5 — Acceptance verification & Phase 14 closure

- End-to-end acceptance flow in `tests/cli.rs` (`phase14_acceptance_cli_flow_end_to_end`): init → begin "Add reduced-motion support" → update requirement → update implementation → deprecate validation → create new validation → status → commit → fresh reopen → accepted $S_{n+1}$ → `kat history` shows **one** revision `C_{n+1}` with `operations == [Update, Update, Deprecate, Create]` → all objects persisted → `kat show`/`list` reflect the final state → `kat validate` clean.
- Negative flows: `abort` leaves repo identical; commit rejected on invalid candidate; CAS conflict.
- Update `docs/change-model.md`, `docs/operations.md` if the design doc requires confirmation notes.
- All Definition-of-Done items checked. `cargo test`, `cargo fmt --check`, `cargo clippy -D warnings` clean. **Phase 14 Frozen.**

---

## 3. Acceptance Scenario

```text
kat change begin --description "Add reduced-motion support"
kat update <R3> --title "Require reduced-motion support"
kat update <I3> --title "Implement reduced-motion"
kat deprecate <V1>
kat create validation --title "Reduced-motion acceptance test"
kat change status          # 4 operations, candidate violations: 0
kat change commit          # one revision C_{n+1}, atomic publish

kat history --oneline      # exactly one new entry: change (4 operations)
kat validate               # clean
```

---

## 4. Definition of Done for Phase 14

- [ ] `kat change begin/status/commit/abort` implemented per the frozen Phase 13 design.
- [ ] Mutation commands apply to the working candidate inside a session (ops see prior effects; `expected_version` against the working candidate).
- [ ] Commit builds ONE `ChangeRevision` with all operations, persists, and publishes atomically; no accepted intermediate states.
- [ ] Failure semantics per design: op failure (session usable), whole-candidate validation failure at commit (rejected, session handled per design), CAS conflict (`Conflict`, no merge).
- [ ] `abort` leaves the repository byte-for-byte unchanged.
- [ ] Canonical format unchanged; session artifacts (if any) explicitly non-canonical and local.
- [ ] `docs/cli.md` documents `kat change *`; `docs/change-model.md`/`docs/operations.md` consistent.
- [ ] All steps validated (`cargo test`, `fmt --check`, `clippy -D warnings`) and committed atomically.
