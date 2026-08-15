# Phase 15 Implementation Plan: Artifact Re-accountability — `kat account`

> Part of the [v0.2 master plan](../implementation-plan.md).

## Purpose

Phase 15 delivers the **Artifact accountability UX** pillar of v0.2.0. v0.1's `kat artifacts` computes `CURRENT`/`STALE`/`UNACCOUNTED` from `represents`/`derived-from` baselines, but re-accounting a stale artifact requires the awkward **unlink + link** ceremony. Phase 15 introduces a **first-class semantic operation** — `AccountArtifact` — that establishes current accountability baselines for an artifact's existing explicit accountability relations, **without implying physical-content verification** (matching the v0.1 `CURRENT` clarification in `docs/prototype-design.md`).

**This is the only v0.2 phase expected to change the canonical format** — it is the first likely `spec/canonical-format.cddl` change. It therefore deserves careful design (Step 15.1) and full spec/vector/engine/CLI propagation. The operation name and encoding are **not decided in this plan**; they are design outputs of Step 15.1.

---

## 1. Design Space (decisions to freeze in Step 15.1)

### 1.1 Operation semantics

- Meaning: _"the artifact has been reviewed/reconciled against its current direct accountability baselines"_ — the artifact's `represents`/`derived-from` relations are re-baselined to the current upstream versions.
- It must be a **canonical semantic operation** (like `UpdateElement`/`Link`), not hidden unlink/link sugar — the artifact's relations must not be destroyed/recreated; only baselines move.
- It must **not imply physical file-content verification** (v0.1 semantic; keep `UNACCOUNTED`/`CURRENT`/`STALE` meanings intact).
- Interaction with `kat change` sessions (Phase 14): `account` must be usable as an operation inside a multi-operation change.

### 1.2 Canonical encoding (design decision)

Options to evaluate and freeze:

- New operation kind in the existing `Operation` union (e.g. `AccountArtifact`) carrying:
  - `artifact ElementId`
  - the accountability `RelationshipId`s it re-baselines (or a rule: "all direct accountability relations of the artifact")
  - the current upstream `ObjectId`s (baseline evidence)
- and/or an **immutable evidence object** (new object kind) recording the baselines for auditability.

The design must specify exactly what the CDDL gains (new op kind number / new object kind), what the engine produces, and how `analyze_artifact_accountability` (v0.1) consumes the new baselines. Any change must keep v0.1 repositories decodable where possible (or define a migration/versioning stance — the canonical envelope has a schema version).

### 1.3 Invariants

- Preconditions: artifact exists and is `Active`; its accountability relations exist and are active; baselines are current (or the operation is exactly what makes them current).
- Result: after `AccountArtifact`, the artifact's direct accountability relations are re-baselined to current upstream `ObjectId`s; `kat artifacts` reports them `CURRENT` (unless upstream changed again).
- Invariant set extended (single-state-delta style) for the account delta.

---

## 2. Work Breakdown & Implementation Steps

> Each step is atomic: implement, add tests, then **validate** (`cargo test`, `cargo fmt --check`, `cargo clippy -D warnings`) before starting the next step. Commit after validation.

### Step 15.1 — Design & freeze `AccountArtifact`

- Produce a design section / doc: operation name, canonical encoding (op kind + any evidence object), semantics, invariants, baseline rules, `kat artifacts` interaction, session (Phase 14) integration.
- Update `spec/canonical-format.cddl`, `docs/canonical-format.md`, `docs/operations.md`, `docs/invariants.md` with the frozen design.
- Add golden/negative **vectors** (`spec/vectors/valid/`, `invalid/`) for the new encoding, with externally-derived ObjectIds (encoder must not be the sole oracle).
- **Design approval gate** before implementation proceeds.
- **Validation**: `cargo test` pass (vector conformance), fmt/clippy clean; docs committed.

### Step 15.2 — Engine: `AccountArtifact` pipeline

- Following the v0.1 typestate pattern: `AccountArtifactInput`, `apply_account_artifact` (preconditions, baseline capture), `validate_account_artifact_ontology` (reuses existing validation), `validate_account_artifact_invariants`, `ValidatedAccountArtifact`, revision/persist/publish stages — mirroring `apply_deprecate_element` / `apply_link_element` structure in `src/repository/change.rs`.
- `expected_version`-style concurrency for the artifact's current versions.
- **Tests** (`tests/change.rs`): preconditions (not found, not active, no accountability relations), baseline capture, invariant failures, single-state delta, persist identity, CAS publish, reopen.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 15.3 — Query: `analyze_artifact_accountability` re-baseline

- Update the v0.1 `analyze_artifact_accountability` (in `src/repository/query.rs`) to consume `AccountArtifact` baselines: after `account`, relations are `CURRENT`; upstream change → `STALE` again (relink re-baseline test from v0.1 must still pass; `account` becomes the first-class path).
- **Tests** (`tests/query.rs`): account → CURRENT; upstream update → STALE; second account → CURRENT; non-mutation.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 15.4 — CLI: `kat account`

- `src/cli.rs` + `src/main.rs`: `kat account <artifact-id-or-prefix> [--description "..."]` (prefix resolution per Phase 11); runs the engine pipeline; prints stable IDs (`element_id`, `change_id`, `change_revision_id`, `state_id`, baselines).
- Usable inside a `kat change` session (Phase 14) as an operation.
- **Tests** (`tests/cli.rs`): account end-to-end; stale→current via CLI; prefix input; outside repo; no-mutation of unrelated objects.
- Update `docs/cli.md`, `docs/operations.md`.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 15.5 — Acceptance verification & Phase 15 closure

- End-to-end acceptance flow in `tests/cli.rs` (`phase15_acceptance_cli_flow_end_to_end`): init → create artifact + implementation → `represents` link → `kat artifacts` CURRENT → update implementation → `kat artifacts` STALE → `kat account <artifact>` → `kat artifacts` CURRENT (no unlink/link) → fresh reopen → `kat history` shows the `AccountArtifact` change → `kat validate` clean.
- Spec consistency: new vectors conformance; v0.1 vectors still pass (no regressions).
- All Definition-of-Done items checked. `cargo test`, `cargo fmt --check`, `cargo clippy -D warnings` clean. **Phase 15 Frozen.**

---

## 3. Acceptance Scenario

```text
kat create artifact --title "styles.css"
kat create implementation --title "Responsive layout"
kat link represents <artifact> <implementation>
kat artifacts               # CURRENT
kat update <implementation> --title "Responsive layout v2"
kat artifacts               # STALE (upstream changed)
kat account <artifact>      # first-class re-accountability
kat artifacts               # CURRENT again — no unlink/link ceremony
kat history --oneline       # one entry: account artifact
```

---

## 4. Definition of Done for Phase 15

- [ ] `AccountArtifact` (or the frozen name) is a canonical semantic operation with frozen encoding in `spec/canonical-format.cddl`; vectors added and conformant.
- [ ] Full engine typestate pipeline implemented (apply/ontology/invariants/revision/persist/publish) with tests.
- [ ] `analyze_artifact_accountability` consumes new baselines; `account` replaces unlink/link re-accountability as the first-class path; v0.1 behavior preserved.
- [ ] `kat account <id>` CLI implemented, including prefix resolution and Phase 14 session integration.
- [ ] No physical-content verification implied; `CURRENT`/`STALE`/`UNACCOUNTED` semantics preserved.
- [ ] `docs/canonical-format.md`, `docs/operations.md`, `docs/invariants.md`, `docs/cli.md` updated.
- [ ] All steps validated (`cargo test`, `fmt --check`, `clippy -D warnings`) and committed atomically.
