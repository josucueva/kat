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
*   **POR-13**: Finalization moves `Workspace.base_revision` but never implicitly moves a `NamedReference`.
*   **POR-14**: A `ReconciliationSession` is bound to immutable revision identities; later movement of the reference originally used as the target does not alter the active session.
*   **POR-15**: Finalize is permitted only for a `PreparedClean` session. Conflict resolution must produce a prepared immutable revision before authority can move.

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

A workspace may hold an active reconciliation session. This ensures both clean and conflicted preparations are persistent, resumable, and bound to stable historical targets.

```rust
pub struct ReconciliationSession {
    pub version: u32,
    pub workspace_id: WorkspaceId,
    pub base_revision: RepositoryRevisionId,
    pub target_revision: RepositoryRevisionId,
    pub state: ReconciliationSessionState,
}

pub enum ReconciliationSessionState {
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

This enforces POR-14 (session bounded to immutable identities) and POR-15 (`kat finalize` operates exclusively on a `PreparedClean` state).

### 2.3 Session State Transitions

```text
NONE
 |
 | kat reconcile
 v
 +-----------------------------+
 |                             |
 v                             v
PREPARED_CLEAN             CONFLICTED
 |                             |
 | kat finalize                | kat materialize
 |                             v
 |                        CONFLICTED_MATERIALIZED
 |                             |
 |                        kat resolve
 |                             |
 |                     +-------+-------+
 |                     |               |
 |                 unresolved       resolved
 |                     |               |
 |                     v               v
 |                 CONFLICTED      PREPARED_CLEAN
 |                                     |
 +-------------------------------------+
                   |
              kat finalize
                   |
                   v
                  NONE

Any active state
      |
   kat abort
      |
      v
     NONE
```

### 2.4 Reconcile Divergence Handling

`kat reconcile` is strictly semantic preparation for diverged histories.

*   `Same` -> reports already synchronized.
*   `LocalAhead` -> reports no incoming reconciliation required.
*   `OtherAhead` -> does not create a session; suggests `kat advance <target>`.
*   `Diverged { one common ancestor }` -> reconciliation permitted.
*   `AmbiguousMergeBase` -> reconciliation rejected with explicit bases.
*   `Unrelated` -> reconciliation rejected.

### 2.5 Fast-forward / Advance vs. Switch

We strictly distinguish three workspace movement vectors:
*   **Switching workspaces:** `kat workspace switch <workspace-id>` changes `.kat/current-workspace` only.
*   **Switching revisions:** `kat switch <revision-or-ref>` changes the revision of the current workspace, demanding a clean current state.
*   **Advancing the workspace:** `kat advance <revision-or-ref>` is a restricted form of movement where the target is a strict descendant of the current base.

## 3. Command Surface Area

The formal Phase 10 porcelain vocabulary is:

*   **`kat status`**: Summarizes the explicit multi-dimensional state (Workspace ID, Base, Semantic, Physical, Backend, and Collaboration/Reconciliation status).
*   **`kat reconcile <target>`**: Resolves the target, asserts `Diverged` (rejecting `Same` or prompting advance for `OtherAhead`), invokes the engine, and saves a `ReconciliationSession`.
*   **`kat conflicts`**: Lists active conflicts in the current `ReconciliationSession`.
*   **`kat materialize`**: Exposes provisional physical state for human resolution.
*   **`kat resolve semantic <id>` / `kat resolve physical <id>`**: Modifies the provisional candidate state.
*   **`kat finalize`**: Consumes a `PreparedClean` session, advancing the `Workspace.base_revision`. It does *not* implicitly move named references.
*   **`kat abort`**: Discards the current `ReconciliationSession` and its backend physical candidate, reverting cleanly to the workspace base.
*   **`kat advance <target>`**: Moves workspace base to a strict descendant.
*   **`kat switch <target>`**: Moves workspace to another revision under clean-state rules.

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
