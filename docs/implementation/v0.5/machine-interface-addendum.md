# v0.5 local collaboration machine-interface addendum

This addendum extends the [v0.4 machine interface](../v0.4/machine-interface.md) for Phase 10.10. `interface_schema_version` remains `1`. Every collaboration `--json` result uses the existing `CommonResultEnvelope<T>` with `data` on success and `error` on failure. `accepted_state_id` still identifies the accepted semantic state. `RepositoryRevisionId` values use `base_revision`, `target_revision`, `prepared_revision`, `from_revision`, and `to_revision`; they are not renamed with the raw `ObjectId` `_object_id` suffix. JSON object key order is immaterial; set-like arrays are sorted.

## Exit and stream contract

| Outcome | Envelope | Exit |
| --- | --- | --- |
| Normal or successful no-op | `success: true`, non-null `data`, null `error` | 0 |
| Runtime, domain, or precondition failure | `success: false`, null `data`, non-null `error` | 1 |
| Usage/parser error with `--json` recognizable in raw arguments | One failure envelope on stdout | 2 |
| Usage/parser error before machine mode can be determined | Ordinary `clap` usage output permitted | 2 |

When machine mode is recognized, stdout contains exactly one UTF-8 JSON envelope and no prose. `--help` and `--version` retain normal `clap` behavior. A successful `reconcile` with conflicts means preparation succeeded; it does not mean finalization is allowed. `reconcile` against OtherAhead is a failure, matching human mode.

## `data` contracts

| Command | Payload |
| --- | --- |
| `status` | Existing `RepositoryStatus` fields, including optional `workspace`. No workspace is `null`. |
| `reconcile` | `status` (`same`, `local_ahead`, `prepared_clean`, `conflicted`), `workspace_id`, `base_revision`, `target_revision`; `prepared_clean` adds `prepared_revision`; `conflicted` adds numeric `semantic_conflicts`, `physical_conflicts`, `validation_findings`. |
| `conflicts` | `workspace_id`, `base_revision`, `target_revision`, separate `semantic_conflicts`, `physical_conflicts`, `validation_findings` arrays. Conflict entries carry the persisted `conflict_id` accepted by `resolve`. |
| `materialize` | `workspace_id`, `base_revision`, `target_revision`, `status` (`materialized`, `already_materialized`, `prepared_clean`). |
| `resolve semantic` / `resolve physical` | `workspace_id`, `domain`, `conflict_id`, `accepted`, `status` (`conflicted` or `prepared_clean`), nullable `prepared_revision`. |
| `abort` | Existing `aborted` and `type`; reconciliation abort additionally reports `workspace_id` and restored `base_revision`. Draft abort retains its existing payload. |
| `finalize` | `workspace_id`, `from_revision`, `to_revision`. |
| `advance` | Existing `advanced`, plus `workspace_id`, `from_revision`, `to_revision`; existing `target` on transition and `reason` on no-op remain. |
| `switch` | Existing `switched`, plus `workspace_id`, `from_revision`, `to_revision`; existing `target` on transition and `reason` on no-op remain. |

`conflicts` sorts semantic and physical entries by persisted conflict ID, each affected-ID/path list by its value, and validation findings by diagnostic. PreparedClean yields three empty arrays.

## Errors

New collaboration paths use stable `error.code` values: `NotInRepository`, `WorkspaceNotFound`, `WorkspaceError`, `InvalidTarget`, `ReconcileFailed`, `OtherAhead`, `NoActiveReconciliation`, `ConflictProjectionFailed`, `MaterializeFailed`, `SessionLoadFailed`, `InvalidAcceptSide`, `ResolveFailed`, and `FinalizeFailed`. Existing `abort`, `advance`, `switch`, and `status` codes remain unchanged for compatibility. Parser failures identified as JSON requests use `INVALID_ARGUMENTS`. All errors use the common envelope invariant; messages explain the failure and details may add context without changing the code.
