# Collaboration Porcelain and Workflow (v0.5 Phase 10)

This document specifies the user-facing collaboration workflows and porcelain commands for KAT v0.5, with a particular focus on divergence, reconciliation, conflict resolution, and authority transitions.

## 1. Principles and Invariants

These invariants govern all Phase 10 porcelain implementation:

*   **POR-01**: Porcelain never bypasses repository/domain invariants.
*   **POR-02**: Reconcile never implicitly publishes authority.
*   **POR-03**: Workspace base moves only through an explicit user action.
*   **POR-04**: Refs move only through explicit publication/update actions.
*   **POR-05**: Prepared reconciliation is resumable.
*   **POR-06**: Conflict resolution modifies provisional state only.
*   **POR-07**: Abort loses no accepted repository knowledge.
*   **POR-08**: Materialization is explicit and reversible.
*   **POR-09**: Status exposes semantic, physical, backend, and collaboration state independently.
*   **POR-10**: Machine output is deterministic and versioned.
*   **POR-11**: User references resolve to stable `RepositoryRevision` identities before mutation.
*   **POR-12**: No porcelain command invokes Git as a user-visible version-control authority.

## 2. Core Workflows

The core principle separating KAT's workflow from Git's is that **reconciliation is purely preparation**. It has no authority over the workspace's target or branch heads until explicitly finalized.

### 2.1 The Reconciliation State Machine

```text
workspace at Rlocal
        |
        v
compare against target ref/revision
        |
        +-- Same
        |
        +-- LocalAhead
        |
        +-- OtherAhead
        |
        +-- Diverged
                |
                v
           kat reconcile
                |
        +-------+-------+
        |               |
      clean          conflicted
        |               |
        v               v
prepared Rmerge   ReconciliationCandidate
        |               |
        |          inspect/status
        |               |
        |          materialize
        |               |
        |            resolve
        |               |
        +-------+-------+
                |
             finalize
                |
                v
        explicit authority move
```

### 2.2 Reconcile Session

A workspace may hold an active reconciliation session. This handles the requirement that both clean and conflicted preparations are persistent and resumable:

```rust
pub enum ReconciliationSession {
    /// Reconciliation resulted in a clean `RepositoryRevision` but is unpublished.
    PreparedClean { 
        revision: RepositoryRevisionId 
    },
    /// Reconciliation produced conflicts requiring resolution.
    Conflicted { 
        candidate: ReconciliationCandidate 
    },
}
```

This ensures `kat reconcile` is safely resumable and `kat abort` deterministically clears the state regardless of whether conflicts were present.

### 2.3 Fast-forward / Advance vs. Switch

We strictly distinguish:
*   **Switching the workspace context:** `kat switch <workspace-id>` or `kat switch-revision <revision-id>` inside a workspace.
*   **Advancing the workspace:** If the target is strictly ahead of the workspace base (`OtherAhead`), we do not produce a multi-parent merge. The user performs an explicit "advance" or "fast-forward" to move the workspace base to the descendant revision.

## 3. Command Surface Area

The minimal Phase 10 porcelain surface is:

*   **`kat status`**: Summarizes the explicit multi-dimensional state (Workspace ID, Base, Semantic state, Physical state, Backend status, and Collaboration/Reconciliation status).
*   **`kat switch <revision-or-ref>`**: Moves the workspace base to a completely different revision, demanding a clean current state. (Not to be confused with switching workspaces).
*   **`kat reconcile <target>`**: Resolves the target, asserts `Diverged` (rejecting `Same` or prompting advance for `OtherAhead`), computes common ancestors, invokes the engine, and saves a `ReconciliationSession`.
*   **`kat conflicts`**: Lists active conflicts in the current `ReconciliationSession`.
*   **`kat resolve semantic/physical <id>`**: Modifies the provisional candidate state.
*   **`kat abort`**: Discards the current `ReconciliationSession` and its backend physical candidate, reverting cleanly to the workspace base.
*   **`kat finalize`** (or `kat accept`/`kat commit`): Consumes a cleanly prepared or fully resolved `ReconciliationSession`, moving the workspace base and updating any associated references.

## 4. Implementation Phasing

*   **10.1** Reconciliation session/workspace state model (`ReconciliationSession`).
*   **10.2** Collaboration status/query APIs (`kat status` updates).
*   **10.3** Reconcile porcelain (`kat reconcile`).
*   **10.4** Inspect conflicts (`kat conflicts`).
*   **10.5** Candidate materialization (applying physical conflicts to working tree).
*   **10.6** Semantic/physical resolution workflows.
*   **10.7** Abort workflow.
*   **10.8** Finalize/accept workflows.
*   **10.9** Switch/advance workflows.
*   **10.10** JSON machine interfaces.
*   **10.11** Full E2E collaboration tests.
