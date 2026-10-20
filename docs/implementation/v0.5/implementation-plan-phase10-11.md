# Phase 10.11 — Full local collaboration E2E evaluation

**Status: complete.** E2E-01..10 pass in `tests/collaboration_json.rs`. A human CLI workflow and an older version-1 session recovery case pass. The Phase 10 gate passes: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.

## Goal and prerequisites

Freeze Phase 10 by exercising complete user workflows through the `kat` binary after [10.10](implementation-plan-phase10-10.md) passes. Test local collaboration behavior, including both human and JSON presentation where it matters. Reuse CLI test fixtures and real temporary Git-backed KAT repositories. Remote fetch/push, migration, and the distributed E2E matrix remain in Phases 12–14.

Before the E2E freeze, correct the current semantic resolver guard: it accepts only `ConflictedMaterialized`. A semantic-only conflict with a clean physical merge must allow `Conflicted → resolve semantic → PreparedClean → finalize` without exposing or modifying the working tree. Semantic resolution remains allowed after materialization too. Physical conflict resolution still requires materialization. When semantic resolution clears the last blocker, bind the prepared revision to the **clean reconciled physical snapshot** (persist or recover that snapshot across CLI restarts); do not create it from an unrelated working-tree state. Preserve session compatibility when extending persisted candidate state. This is a Phase 10.6 correction required by the semantic/physical separation, not a new collaboration operation.

### Session compatibility decision

Keep local `ReconciliationSession` format version 1 and add an optional clean physical snapshot ID to persisted candidate state. New semantic-only conflicted sessions must persist the exact clean physical reconciliation result. For an older v1 session without that field, recover the result only if the backend can deterministically reproduce and verify the same snapshot from the session's immutable input revisions. If that cannot be proven, reject the session explicitly; never substitute a snapshot of the mutable working tree. This additive rule avoids a format bump while making restart behavior unambiguous.

## Scenario matrix

Each scenario asserts the final `Workspace.base_revision`, physical tree, reconciliation session state, and NamedReferences where applicable. Compare immutable revision IDs and file bytes rather than relying on success text alone.

| ID | User workflow | Required outcome |
| --- | --- | --- |
| E2E-01 | Two divergent clean revisions → `reconcile` → `finalize` | PreparedClean becomes the workspace base; merged semantic and physical contents are present. |
| E2E-02 | Semantic conflict with clean physical merge → `reconcile` → `conflicts` → `resolve semantic` → `finalize`, with no `materialize` | Listed conflict ID resolves; selected semantic side and clean merged physical snapshot survive; the working tree is untouched until finalization. |
| E2E-03 | Physical conflict → `reconcile` → `conflicts` → `materialize` → edit working file → `resolve physical --accept working` → `finalize` | Edited file is captured in the prepared revision and remains after finalization. Durable physical candidate state and `refs/kat/candidates/<id>` are removed. The current physical resolver supports `working` only. |
| E2E-04 | Mixed semantic and physical conflicts → resolve semantic before or after `materialize`, attempt `finalize` while physical conflict remains, resolve physical, finalize | Semantic resolution is independent of materialization; first finalization is rejected without moving the base; finalization succeeds only when both conflict arrays and blocking findings clear. |
| E2E-05 | `reconcile` → `materialize` → `abort` | Exact original workspace bytes and base revision are restored; no active session remains; durable physical candidate state and `refs/kat/candidates/<id>` are removed. |
| E2E-06 | `reconcile` against OtherAhead → `advance` | Reconcile rejects the non-divergent target; advance moves the base and physical tree to the target. |
| E2E-07 | `switch` between unrelated valid revisions | Workspace base and physical tree agree with each selected revision; clean-state guard is honored. |
| E2E-08 | Physical or mixed conflict → `reconcile` → `materialize` → process exits → new process runs `conflicts` and `resolve` | Session, `PhysicalCandidateId`, conflict IDs, and backend candidate state survive restart; resolution continues. |
| E2E-09 | Restart the CLI during PreparedClean → `finalize` | Prepared revision remains available and finalization completes. |
| E2E-10 | Capture NamedReferences before each collaboration workflow; reconcile, resolve, finalize, abort, advance, and switch | NamedReferences remain byte-for-byte unchanged unless an explicit reference operation is invoked. |

## Execution order

1. Reuse the existing reconciliation and command-test builders to create the few revision graphs needed for all ten scenarios. Keep scenario setup local and deterministic; do not create a separate test framework.
2. Exercise commands as separate processes so persisted sessions, stdout, exit codes, and restart behavior are real. Assert at least one complete flow in JSON mode and one in human mode, with matching repository outcomes.
3. Assert failure boundaries: rejected `finalize`, stale/dirty workspace actions, and aborted materialization leave accepted revisions and named references unchanged. Inspect both `.kat/workspaces/<workspace-id>/physical-reconciliation/<candidate-id>/` and `refs/kat/candidates/<candidate-id>` for durability in E2E-08 and cleanup in E2E-03/05. These internal GC-protection refs are separate from the NamedReferences checked by E2E-10. Keep precise failure/recovery tests already present; add only missing workflow coverage.
4. Run the Phase 10 gate: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`. Record the ten scenario results and update the pending report and phase tracker before marking Phase 10 frozen.

## Freeze criterion

The semantic-only flow passes without materialization, all MI-COL-01..15 checks from 10.10 and E2E-01..10 pass, and physical candidate/GC-ref cleanup is verified. No test needs a remote service. Phase 11 begins only after the local machine interface and collaboration workflows are verified together.
