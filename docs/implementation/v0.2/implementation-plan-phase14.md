# Phase 14 Implementation Plan — Multi-Operation Changes (`kat change`)

This document is the per-phase implementation plan for **Phase 14** of the KAT v0.2 roadmap. It implements multi-operation change transactions (`kat change begin`, `status`, `commit`, `abort`) following the frozen architectural design specification in [`docs/v0-2-multi-op-change-design.md`](../../v0-2-multi-op-change-design.md).

Parent tracker: [`implementation-plan.md`](implementation-plan.md)

---

## Executive Summary

Phase 14 exposes multi-operation `ChangeRevision` capability to users without modifying the canonical CDDL schema or protocol version. Multiple semantic mutation operations (`create`, `update`, `deprecate`, `supersede`, `link`, `unlink`) are staged into a private, local, non-canonical session file (`.kat/work/change/session.json`). On `kat change commit`, whole-candidate consistency validation executes, canonical objects are materialized into `.kat/objects`, and a single `ChangeRevision` containing all staged operations is atomically published to `refs/accepted`.

---

## Steps & Status

- [x] **Step 14.1 — Engine Session & Persistence (`src/repository/session.rs`)**
  - Implement `DraftSession`, `DraftSessionState`, `begin_draft_session`, `read_draft_session`, `write_draft_session_atomic`, `mark_draft_session_stale`, and `abort_draft_session`.
  - Wire hex CBOR serialization for `Operation`, `KnowledgeElementVersion`, `RelationshipVersion`, and `SemanticState`.

- [x] **Step 14.2 — Engine Sequential Staging (`src/repository/change.rs`)**
  - Implement `stage_operation_into_session`.
  - Validate preconditions and evaluate `expected_version` against $S_{\text{working}}$.
  - Update `working_state` and stage versions into session without polluting `.kat/objects`.

- [x] **Step 14.3 — Engine Whole-Candidate Commit & Single CAS (`src/repository/change.rs`)**
  - Implement `commit_draft_session`.
  - Validate whole-candidate consistency using `validate_repository_state`.
  - Materialize canonical objects into `.kat/objects`.
  - Perform single CAS update on `refs/accepted` publishing one `ChangeRevision`.

- [x] **Step 14.4 — CLI `kat change` Subcommands & Staged Output (`src/cli.rs`, `src/main.rs`)**
  - Wire `kat change begin`, `kat change status`, `kat change commit`, `kat change abort`.
  - Update mutation CLI commands (`create`, `update`, `deprecate`, `supersede`, `link`, `unlink`) to auto-stage onto active draft session when `.kat/work/change/session.json` exists.

- [x] **Step 14.5 — Acceptance Verification & Phase 14 Closure**
  - Added end-to-end acceptance test `phase14_acceptance_cli_flow_end_to_end` in `tests/cli.rs`.
  - Regenerated CLI assets and man pages via `cargo run --bin generate_assets`.
  - Verified local build and installed binary via `./install.sh`.
  - Verified full test suite passing cleanly (268 tests).
