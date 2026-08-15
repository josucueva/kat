# Phase 12 Implementation Plan: Output Modes — shared compact/full rendering, `history --oneline/--limit/--element`, compact read commands

> Part of the [v0.2 master plan](../implementation-plan.md).

## Purpose

Phase 12 delivers the **Concise interaction** pillar of v0.2.0. Read-side commands grow a **shared presentation model** — `default` (useful everyday output) vs `--compact` (minimum information necessary to identify and understand the result) — so that common queries are readable at a glance. It is a **cross-cutting CLI capability**, not seven unrelated `--short` implementations.

Phase 12 is **strictly presentation-layer**: no repository mutation, no canonical-format change, no query-semantics change. Default output must remain faithful to v0.1 (a regression suite pins it).

---

## 1. Frozen Design & Semantics

### 1.1 Shared presentation model

- Every read command has a `default` mode and a `--compact` mode:

  ```
  default     useful everyday output
  --compact   minimum information necessary to identify and understand the result
  ```

- `--verbose` is **deferred** (later release); the model must leave room for it. `--quiet` is **not** defined (in CLI conventions quiet means "suppress output", not "compact") — we use `--compact` only.
- Renderer architecture is a thin presentation layer that keeps query results (already structured Rust types) separate from rendering. This is the seam where a future `--json` renderer could attach (P2, deferred — we do **not** promise `--json` in v0.2.0).
- Flags are per-command (`kat history --compact`); a future global `kat config output.mode compact` is explicitly **not** introduced in v0.2 (command flags are enough).

### 1.2 `kat history` — default, `--oneline`, `--limit`, `--element`

- **Default** becomes shorter than v0.1: one change per entry (change-revision ID prefix + operation summary + description). The v0.1 detailed per-revision block moves to a future `--verbose` (kept conceptually; not implemented now).
- **`--oneline`** (git-like; the name is used deliberately because users know it from Git):

  ```
  aec57b12  update element      Require reduced-motion support
  92bc8120  link                Link implementation to requirement
  12a86c03  create element      Responsive layout implementation
  ```

- **`--limit N`**: truncate to the `N` most recent revisions (N ≥ 1; malformed N → exit 1).
- **`--element <id>`**: filter to revisions that touch the given element (any operation whose `element_id`/`relationship_id` involves the element — exact membership defined in the step; element may be specified as a unique prefix per Phase 11 resolution). Ordering stays newest-first, deterministic (existing DFS).
- Flags compose: `kat history --oneline --limit 10 --element <prefix>`.

### 1.3 Compact forms for the other read commands

| Command         | Default (unchanged)                       | `--compact`                                                                                                                 |
| :-------------- | :---------------------------------------- | :-------------------------------------------------------------------------------------------------------------------------- |
| `kat trace`     | existing tree                             | chain arrows: `styles.css -> Responsive implementation -> Responsive requirement -> Portal intent`; multiple paths numbered |
| `kat impact`    | existing 3-bucket detail                  | flat table `TYPE / ID / TITLE` (`direct`, `semantic`, `artifact`)                                                           |
| `kat validate`  | existing report                           | `0 violations, 2 unverified constraints`                                                                                    |
| `kat artifacts` | existing detail                           | `STATUS / ARTIFACT` table (`current`, `stale`, `unaccounted`)                                                               |
| `kat status`    | existing dashboard                        | `12 elements · 10 relationships · 0 violations · 1 stale artifact`                                                          |
| `kat show`      | existing detail (+ Phase 11 neighborhood) | `7af83d1c  requirement  active  User must authenticate using MFA`                                                           |

- `kat list` (Phase 11) is already compact by default; no change here.

---

## 2. Work Breakdown & Implementation Steps

> Each step is atomic: implement, add tests, then **validate** (`cargo test`, `cargo fmt --check`, `cargo clippy -D warnings`) before starting the next step. Commit after validation.

### Step 12.1 — Presentation renderer module

- New `src/present/` module (or `src/cli/present.rs`): shared rendering helpers — stable table renderer (column alignment), one-line renderers, and a `Compact` flag plumbing convention through read commands.
- Refactor existing read commands to route rendering through the module **without changing any default output**. Pin current default output with golden CLI tests (regression suite) so Phase 12 defaults stay byte-identical to v0.1.
- **Tests**: golden default-output tests for `show`/`history`/`trace`/`impact`/`validate`/`artifacts`/`status` (baseline before feature work); table-renderer unit tests (alignment, empty input, wrapping).
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 12.2 — `kat history` default + `--oneline`

- Shorten `kat history` default to one change per entry (keeping newest-first DFS order and dedup semantics).
- Add `--oneline` compact form.
- **Tests** (`tests/cli.rs`): default one-per-entry; `--oneline` exact rendering; multi-revision ordering; still no-mutation.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 12.3 — `kat history --limit` and `--element`

- Add `--limit N` (truncate newest-first; malformed/zero → exit 1) and `--element <id-or-prefix>` (filter to revisions touching the element; uses Phase 11 `ElementId` resolution).
- **Tests** (`tests/cli.rs`): `--limit 1/2/10`; `--limit 0` and non-numeric → exit 1; `--element` filters correctly (update/link/deprecate all counted); `--element` with prefix; combined flags; unknown element → exit 1.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 12.4 — `kat trace --compact`

- Compact chain rendering (arrow-joined, numbered multi-path). Default tree unchanged.
- **Tests** (`tests/cli.rs`): single path; multi-path numbering; cycle-safe still; no-mutation.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 12.5 — `kat impact --compact`

- Flat `TYPE / ID / TITLE` table preserving the 3 required categories (direct/semantic/artifact). Default detail unchanged.
- **Tests** (`tests/cli.rs`): category rows correct; empty artifacts bucket; no-mutation.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 12.6 — `kat validate --compact`

- One-line counts: `0 violations, 2 unverified constraints`. Exit codes unchanged (0 clean/unverified, 1 violations).
- **Tests** (`tests/cli.rs`): clean; violations; unverified-only; exit codes preserved.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 12.7 — `kat artifacts --compact`

- `STATUS / ARTIFACT` table (current/stale/unaccounted). Default detail unchanged; exit codes unchanged.
- **Tests** (`tests/cli.rs`): each status; all-current; stale/unaccounted exit 1.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 12.8 — `kat status --compact`

- One-line dashboard: `12 elements · 10 relationships · 0 violations · 1 stale artifact`. Default dashboard unchanged.
- **Tests** (`tests/cli.rs`): counts correct; empty repo; no-mutation.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 12.9 — Acceptance verification & Phase 12 closure

- End-to-end acceptance flow in `tests/cli.rs` (`phase12_acceptance_cli_flow_end_to_end`): build a multi-revision repo (create ×2, update, link, deprecate), then verify every `--compact`/`--oneline`/`--limit`/`--element` form and that **all default outputs match their pinned v0.1 golden outputs**; ObjectStore + accepted ref unchanged; fresh reopen reproducible.
- Update `docs/cli.md` and `docs/cli-presentation.md` with the shared presentation model.
- All Definition-of-Done items checked. `cargo test`, `cargo fmt --check`, `cargo clippy -D warnings` clean. **Phase 12 Frozen.**

---

## 3. Acceptance Scenario

```text
kat history --oneline --limit 5
kat history --element <requirement-prefix>
kat trace <artifact-prefix> --compact
kat impact <requirement-prefix> --compact
kat validate --compact
kat artifacts --compact
kat status --compact
```

---

## 4. Definition of Done for Phase 12

- [x] Shared presentation model (`default` + `--compact`) implemented across `history`, `trace`, `impact`, `validate`, `artifacts`, `status`, `show`; defaults byte-identical to v0.1 (pinned by golden tests).
- [x] `kat history` default is one-change-per-entry; `--oneline`, `--limit N`, `--element <id|prefix>` implemented and composable.
- [x] Compact forms for trace (chain), impact (table), validate (counts), artifacts (status table), status (one-liner) implemented with correct exit codes.
- [x] Renderer architecture leaves a seam for a future `--json` renderer (no JSON promise in v0.2.0); no `--verbose`/`--quiet`/global config added.
- [x] No repository mutation, no canonical-format change, no query-semantics change.
- [x] `docs/cli.md` and `docs/cli-presentation.md` document the presentation model.
- [x] All steps validated (`cargo test`, `fmt --check`, `clippy -D warnings`) and committed atomically.
