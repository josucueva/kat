# Phase 10.10 — Collaboration JSON machine interfaces

## Goal and boundary

Expose the existing local collaboration commands through the v0.4 versioned machine interface. Human and JSON modes invoke the same repository operations and have the same state changes and exit status. This phase adds presentation and contract checks; it does not add reconciliation, remote, or migration behavior. Phase 10.11 follows with full user-workflow E2E tests.

## Existing contract and implementation

- Keep `interface_schema_version = 1` and the existing `CommonResultEnvelope<T>`: `kat_version`, `interface_schema_version`, `success`, `repository_id`, `accepted_state_id`, `data`, `error`. `data` is non-null only on success; `error` is non-null only on failure. Do not replace `data` with `result` or add a second top-level envelope. Put collaboration context in `data`.
- `MachinePresenter` already emits the envelope. `status --json` serializes `RepositoryStatus`, including its optional `workspace`; `abort --json`, `advance --json`, and `switch --json` already use the presenter, though their payloads need review. `finalize --json` currently prints ad hoc JSON. `reconcile`, `conflicts`, `materialize`, and `resolve` lack `--json` options.
- Keep the v0.4 `status` payload fields and add only compatible collaboration fields if needed. Keep `abort --json`'s existing `aborted` and `type` fields for both draft and reconciliation cases. Machine clients must tolerate additive fields.
- JSON object key order is not contractual. All set-like arrays must have deterministic order. The envelope's `accepted_state_id` remains the accepted **semantic** state ID, not a workspace or repository revision ID.

## Command payload contract

All fields below live under `data`. IDs are full canonical IDs. Use `workspace_id`, `base_revision`, `target_revision`, `prepared_revision`, `from_revision`, and `to_revision` consistently where applicable. These are public `RepositoryRevisionId` fields; do not mechanically rename them with the v0.4 `_object_id` suffix used for raw `ObjectId` fields. Nullable fields use `null`; collections use `[]`. Do not expose internal candidate blobs or Git objects.

| Command | Required result fields and states |
| --- | --- |
| `status --json` | Preserve existing `RepositoryStatus`; `workspace` reports its ID, base revision, semantic/physical/backend/reconciliation status. No workspace remains `null`. |
| `reconcile <target> --json` | `status` = `same`, `local_ahead`, `prepared_clean`, or `conflicted`; `workspace_id`, `base_revision`, `target_revision`. Clean adds `prepared_revision`; conflicted adds `semantic_conflicts`, `physical_conflicts`, `validation_findings` counts. `other_ahead` remains a rejected operation with an error envelope and nonzero exit, matching human mode. |
| `conflicts --json` | `workspace_id`, `base_revision`, `target_revision`, `semantic_conflicts`, `physical_conflicts`, `validation_findings` as **three separate arrays**. Each conflict includes the ID accepted by `resolve`, its kind, and affected IDs or paths; findings include their diagnostic. Prepared clean returns empty arrays. |
| `materialize --json` | `workspace_id`, `base_revision`, `target_revision`, `status` = `materialized`, `already_materialized`, or `prepared_clean`, matching the current successful no-op cases. |
| `resolve semantic|physical ... --json` | `workspace_id`, `domain`, `conflict_id`, `accepted`, `status` = `conflicted` or `prepared_clean`; include `prepared_revision` when clean. Reflect the actual remaining session state; preserve current rejection of unsupported physical alternatives. |
| `abort --json` | Preserve `aborted` and `type`; reconciliation result also includes `workspace_id` and `base_revision` (the restored workspace base). Draft abort keeps its current shape. |
| `finalize --json` | `workspace_id`, `from_revision`, `to_revision`. Only `PreparedClean` succeeds; refs do not move. |
| `advance <target> --json` | `workspace_id`, `from_revision`, `to_revision`, `advanced`. For `Same`, `advanced: false` and both revisions equal; preserve successful no-op behavior. |
| `switch <target> --json` | `workspace_id`, `from_revision`, `to_revision`, `switched`. For `Same`, `switched: false` and both revisions equal. |

`reconcile`'s clean/conflicted statuses are successful preparation, even though a conflicted session cannot yet finalize. A rejected topology, invalid target, absent/stale session, dirty workspace, bad conflict ID, or backend failure uses `success: false`, `data: null`, a stable error `code`, human-readable `message`, structured `details` when useful, and a nonzero exit. Use existing error codes where already exposed; audit casing and document any new codes before asserting them in tests.

For usage/parser failures, apply the v0.4 exit-code rule: **exit 2**. If `--json` can be recognized from the raw invocation, the failure must emit exactly one `CommonResultEnvelope` error on stdout. If parsing fails before machine mode can be determined, ordinary `clap` usage-error behavior is allowed. Determine this boundary empirically in step 1; the binary currently starts with `Cli::parse()`, so parser errors occur before normal command dispatch. Once JSON mode is recognized, stdout contains only the envelope and stderr only diagnostics.

## Implementation order

1. **Freeze DTO examples and error map.** Check v0.4 machine-interface and CLI stream rules against the current command branches. Test malformed invocations with `--json` before/after a subcommand and with missing arguments to establish when raw arguments reliably identify machine mode. Record exact success, no-op, and failure payloads in the [v0.5 machine-interface addendum](machine-interface-addendum.md) before wiring new output. The current conflict projection hashes new display IDs while `resolve` looks up the stored conflict IDs. Correct this in the shared projection so both human and machine paths show IDs that `resolve` accepts.
2. **Add flags and presentation.** Add `--json` to the four missing command forms (including both `resolve` subcommands) and route all nine commands through `MachinePresenter`. Reuse existing `RepositoryStatus`, session, conflict, and workspace projections. Capture before/after revision IDs around transitions without changing their operations. Replace `finalize`'s ad hoc output. Refresh generated help, completions, and man pages.
3. **Verify semantic parity.** For every JSON branch, assert the same repository outcome and exit code as human mode, including no-ops and failures. Ensure no JSON path bypasses current clean-state, ancestry, session, or validation guards.
4. **Lock the machine contract.** Add focused CLI integration tests for the matrix below. Compare parsed JSON, not object key order; assert exact array ordering and one JSON value on stdout. Keep full multi-command collaboration journeys for 10.11.

## Acceptance matrix

| ID | Check |
| --- | --- |
| MI-COL-01 | `status --json` is deterministic and preserves its existing v1 fields. |
| MI-COL-02 | `reconcile --json` reports `prepared_clean` with the prepared revision. |
| MI-COL-03 | `reconcile --json` reports conflict and finding counts separately. |
| MI-COL-04 | `conflicts --json` keeps semantic, physical, and validation arrays separate; its IDs round-trip into `resolve`. |
| MI-COL-05 | `materialize --json` reports state changes and successful no-ops. |
| MI-COL-06 | Semantic resolution JSON reports the selected side and resulting session state. |
| MI-COL-07 | Physical resolution JSON reports `working` and resulting session state. |
| MI-COL-08 | `abort --json` covers reconciliation and draft paths without changing the draft contract. |
| MI-COL-09 | `finalize --json` uses the common envelope and reports old/new revision IDs. |
| MI-COL-10 | `advance --json` reports old/new IDs for transition and no-op. |
| MI-COL-11 | `switch --json` reports old/new IDs for transition and no-op. |
| MI-COL-12 | Runtime failures use the common error envelope and exit 1; usage/parser failures exit 2 and use the envelope whenever raw arguments identify `--json`. |
| MI-COL-13 | Every invocation recognized as `--json` emits exactly one JSON envelope on stdout, with no human text. |
| MI-COL-14 | IDs, conflicts, paths, and findings have stable ordering across repeated reads. |
| MI-COL-15 | `interface_schema_version` stays `1`; existing v0.4 DTOs and error invariants remain compatible. |

## Gate

Run `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`. Phase 10 remains open until 10.11's full collaboration E2E scenarios pass. After 10.10, update the pending report and Phase 10 tracker with the tested contract and any remaining CLI parser limitations.
