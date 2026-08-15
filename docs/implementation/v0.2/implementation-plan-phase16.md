# Phase 16 Implementation Plan: Real-Project Evaluation

> Part of the [v0.2 master plan](../implementation-plan.md).

## Purpose

Phase 16 closes v0.2 the same way v0.1 was validated: with a **real-world experiment** on an actual evolving project (the same web-application experiment that produced the v0.1 findings, or an equivalent realistic workload). It measures whether the v0.2.0 pillars actually delivered, and produces the **evidence base for the v0.3 decision**.

This is a **validation/research phase**, not a feature phase: no new product code is written (fixes found during evaluation are handled as normal validated commits and are in scope only if they are clear regressions/bugs).

---

## 1. Evaluation Objectives (measurement questions)

Repeat the experiment and measure specifically:

1. **Discovery** — Did `kat list` remove the need for external UUID bookkeeping? Was prefix resolution used, and did ambiguity ever bite?
2. **Concise interaction** — Did compact output make common queries faster to read/scan? Which commands got the most `--compact`/`--oneline` use?
3. **Meaningful changes** — Did multi-operation Changes reduce revision noise? How many conceptual changes became one revision vs. many?
4. **Artifact re-accountability** — Did `kat account` eliminate the unlink/link ceremony? Was the baseline semantics clear?
5. **Semantic integrity** — Did any new semantic ambiguity appear? Were invariants respected throughout? Did `kat validate` stay clean on the real repo?

Secondary observations worth capturing: CLI ergonomics (typos, confusion, help text), performance at scale (repo size, history length, query latency), and any missing capability that surfaced.

---

## 2. Work Breakdown & Implementation Steps

> Each step is atomic and validated before the next.

### Step 16.1 — Experiment setup & workload definition

- Define the experiment repository: a realistic evolving web-application model (requirements, constraints, decisions, implementations, validations, artifacts with `represents`/`derived-from`), targeting the scale that exposed v0.1's pains (e.g. 60–100+ revisions).
- Script the workload as a repeatable sequence of `kat` invocations (documented in the evaluation doc).
- **Validation**: workload script runs end-to-end on v0.2; captured baseline.

### Step 16.2 — Run the experiment & gather metrics

- Execute the workload through the v0.2 CLI, capturing: number of `kat list`/`show`/`trace`/`impact` calls (and their compact forms), revision counts with and without multi-op changes, `account` vs old unlink/link counts, `kat validate`/`kat artifacts` results at checkpoints.
- **Validation**: metrics recorded in `docs/v0-2-evaluation.md`; no product-code changes.

### Step 16.3 — Analysis & v0.3 recommendation

- Answer the five measurement questions with evidence; document any new semantic ambiguities or UX defects (as findings, with severity); note any observed regressions (fixed as normal validated commits).
- Produce a recommendation for v0.3 scope based on evidence (what to build next, what to defer, what to drop).
- **Validation**: `docs/v0-2-evaluation.md` complete; recommendation documented.

### Step 16.4 — v0.2.0 release acceptance & freeze

- Run the full regression suite (`cargo test`, `cargo fmt --check`, `cargo clippy -D warnings`), the v0.1 vector conformance suite, and the v0.2 acceptance scenarios (Phases 11–15).
- Freeze v0.2 semantics; prepare the v0.2.0 release (tag, version bump to 0.2.0, release notes).
- All Definition-of-Done items checked. **Phase 16 Frozen / v0.2.0 released.**

---

## 3. Deliverables

- `docs/v0-2-evaluation.md` — experiment setup, metrics, findings, v0.3 recommendation.
- v0.2.0 release (tag + `version.txt` + `Cargo.toml` bump + release notes).

---

## 4. Definition of Done for Phase 16

- [ ] Real-project experiment run against the v0.2 CLI with a documented, repeatable workload.
- [ ] All five measurement questions answered with evidence in `docs/v0-2-evaluation.md`.
- [ ] Regressions/bugs found are fixed via normal validated commits (or explicitly deferred with rationale).
- [ ] v0.3 recommendation produced from evidence (not from speculation).
- [ ] Full regression + vector conformance suites pass; v0.2.0 released and frozen.
