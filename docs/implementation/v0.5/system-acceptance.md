# KAT v0.5 System Acceptance

**Status:** Coverage audit and scenario specification are active; Phase 13 migration/bootstrap and SYS-01 are complete locally. SYS-02..SYS-05 remain pending.

## Purpose and boundary

Phase 14 is the system-acceptance milestone already assigned by the v0.5
implementation plan. This document makes its coverage and execution criteria
specific. It does not add a new phase or change the Phase 11 transport
contracts.

The goal is to exercise KAT through separate `kat` CLI processes and persistent,
isolated repositories, then inspect both command results and durable repository
state. The existing loopback Hub and Git test services are suitable for local
acceptance. TLS deployment, production retention, durability, and performance
remain separate deployment gates.

Phase 13 migration and root-workspace bootstrap are implemented and locally
verified. `kat record` is implemented and SYS-01 passes through separate CLI
processes with persistent-state assertions. Phase 14 still requires `kat clone`
and SYS-02..SYS-05 before it can be declared complete.

## Existing coverage audit

“Existing coverage” identifies component and feature tests; they are not
substitutes for the integrated scenarios below. Only the gates with explicit
evidence in the status table are marked passed.

| Area | Existing coverage | Remaining system-level gap |
| --- | --- | --- |
| A. Repository lifecycle | `tests/init.rs`, `tests/migration_cli.rs`, `tests/open.rs`, `tests/cli.rs`, and `tests/v04_porcelain.rs` cover initialization, explicit migration, reopening, inspection, semantic authoring, and persistence. | One user journey does not yet combine semantic and physical evolution, validation, and reopening a complete sequence of `RepositoryRevision`s. |
| B. Semantic evolution | `tests/change.rs`, `tests/query.rs`, and CLI flows in `tests/cli.rs` cover element creation/update/lifecycle, relationships, trace, impact, validation, accountability, and multi-operation Changes. | These flows are not combined with accepted physical revisions and remote collaboration in one software-development journey. |
| C. Physical evolution | `tests/workspace.rs`, `tests/workspace_git.rs`, `tests/reconcile_physical.rs`, and `tests/reconcile_conformance.rs` cover snapshot identity, content and mode changes, protection refs, and physical merges. | The audited CLI has no command that captures ordinary working-tree edits into a new local `RepositoryRevision`; tests construct snapshots/revisions through repository APIs. |
| D. Workspace behavior | `tests/collaboration_json.rs` covers the Phase 10 E2E-01..10 workflows through CLI invocations, including restart, conflict resolution, abort, advance, switch, and reference preservation. | These workflows use prepared revision fixtures; they do not begin with ordinary local physical development and end in a remotely shared revision. |
| E. Remote collaboration | `tests/remote.rs`, `tests/hub.rs`, `tests/workspace_git_transport.rs`, `tests/remote_cli.rs`, and `tests/remote_cli_e2e.rs` cover closure transfer, authentication, CAS, partial outcomes, retry, and separate CLI processes. | `remote_cli_e2e` creates peer repositories with the test-only `clone_at_base` helper. There is no `kat clone` command to create a same-`RepositoryId` developer repository through the supported CLI. The test also seeds RepositoryRevisions directly rather than having peers create them through the CLI. |
| F. Reconciliation | `tests/reconcile_cmd.rs`, `tests/reconcile_conformance.rs`, `tests/reconcile_physical.rs`, `tests/reconcile_session.rs`, and `tests/collaboration_json.rs` cover semantic, physical, mixed, and topology cases. | No existing scenario starts with two CLI-created developers who independently evolve, fetch, reconcile both conflict classes, and verify the final shared history. |
| G. Failure recovery | `tests/remote.rs`, `tests/hub.rs`, `tests/workspace_git.rs`, and `tests/workspace_git_transport.rs` cover interrupted transfer, CAS, integrity failures, ref loss, GC, and recovery. | The failures are mostly injected at component boundaries. A scenario should verify authority and recovery across CLI process exits and service reopen. |
| H. Security | Authorization and remote CLI tests cover grants, revocation, binding checks, credential handling, and fail-closed behavior on loopback. | Production TLS/proxy isolation, direct backend access, server retention, and deployment durability are not local system-acceptance claims. |

### User-case traceability

| Use case | Existing test evidence | System scenario |
| --- | --- | --- |
| UC-001 Create Knowledge Element | `tests/cli.rs::phase1_acceptance_cli_flow_end_to_end`, `kat_create_cli_flow_end_to_end`; `tests/change.rs` | SYS-01 |
| UC-002 Create Design Decision | `tests/cli.rs::kat_link_cli_flow_end_to_end`, `phase9_acceptance_cli_flow_end_to_end`; semantic operation tests | SYS-01 |
| UC-003 Link Knowledge Elements | `tests/cli.rs::kat_link_cli_flow_end_to_end`, `phase5_acceptance_cli_flow_end_to_end` | SYS-01 |
| UC-004 Trace Origin | `tests/cli.rs::phase7_acceptance_cli_flow_end_to_end` | SYS-01 |
| UC-005 Analyze Impact | `tests/cli.rs::phase8_acceptance_cli_flow_end_to_end` | SYS-01 |
| UC-006 Validate Consistency | `tests/cli.rs::phase9_acceptance_cli_flow_end_to_end`, `tests/validation_demo.rs` | SYS-01 |
| UC-007 Review Artifact Accountability | `tests/cli.rs::phase10_acceptance_cli_flow_end_to_end`, Phase 15 accountability flows | SYS-01, SYS-03 |
| UC-008 Author a Multi-Operation Change | `tests/cli.rs::phase14_acceptance_cli_flow_end_to_end`, `tests/change.rs`, `tests/v04_porcelain.rs` | SYS-01, SYS-02 |

### Invariant traceability

| Invariant | Existing focused evidence | Required system assertion |
| --- | --- | --- |
| Immutable objects and content-addressed identity | `tests/vector_conformance.rs`, `tests/open.rs`, `tests/remote.rs` | Reopen and verify previously accepted bytes and IDs after each scenario and failure. |
| Semantic authority | `tests/change.rs`, `tests/reference.rs`, `tests/remote_cli_e2e.rs` | File edits and fetch do not change accepted semantic state; only explicit semantic operations do. |
| Explicit workspace evolution | `tests/workspace.rs`, `tests/collaboration_json.rs` | Workspace base changes only after an allowed explicit operation; compare the persisted base before and after each rejected operation. |
| Physical identity | `tests/workspace_git.rs`, `tests/workspace_git_transport.rs` | Equivalent tracked trees retain one `WorkspaceSnapshotId`; verify after transfer and reopen. |
| Conflict preservation and explicit resolution | `tests/reconcile_physical.rs`, `tests/collaboration_json.rs` | Each semantic and physical conflict stays visible until resolved; final ancestry includes both input heads. |
| Reference safety | `tests/reference.rs`, `tests/remote.rs`, `tests/remote_cli_e2e.rs` | A failed or stale CAS leaves the newer named reference unchanged. |
| Remote closure integrity and retention | `tests/hub.rs`, `tests/workspace_git_transport.rs`, `tests/remote.rs` | Published references resolve to complete semantic and physical closures after transport/service reopen and tested GC. |
| Fetch isolation | `tests/remote.rs`, `tests/remote_cli_e2e.rs` | Fetch changes remote-tracking observations only; local refs, accepted state, workspace base, active lineage, and working-tree bytes remain unchanged. |

## User-path prerequisites

The current tests establish transport behavior using helpers that bypass two
user-facing transitions required by the proposed scenarios. Root initialization
and migration are implemented and locally tested; the contracts are defined in
[collaboration-workflow.md](collaboration-workflow.md). The remaining work is:

1. `kat record [PATH ...]` creates a complete local revision
   from the current accepted semantic state and tracked physical tree, with
   explicit paths required for newly tracked files. Explicit paths must be
   workspace-relative existing regular files or symlinks, must not name
   ignored files or repository metadata, and directories are not expanded.
   Previously tracked missing files are deletions. It must cover
   semantic-only, physical-only, and combined revisions.
   Multiple semantic commits before one record must preserve the dependency
   chain to every ChangeRevision, and the recorded head must pass full closure
   verification with its result state matching the new revision.
2. The remote model specifies clone, but the CLI exposes no `kat clone` command.
   Current remote CLI tests use a test helper to copy repository identity,
   semantic objects, physical state, refs, and workspace base into peer
   directories.

These are not test-harness gaps. Until the supported user workflows exist (or
the project explicitly scopes them out and documents another supported path),
the multi-developer and complete software-lifecycle scenarios cannot be claimed
as user-level system acceptance. The remaining gaps are in `src/cli.rs` and
`src/main.rs::run_commit`; `tests/remote_cli_e2e.rs::clone_at_base` supplies
the peer bootstrap through test-only repository APIs. A fixture that recreates
the test helper would only repeat component integration evidence.

## System scenarios

Each scenario uses temporary persistent directories and invokes the real `kat`
binary in a fresh process for each CLI step. Fixtures may start the existing
loopback Hub and controlled Git services. Assertions inspect command exit status,
human/JSON output as applicable, and persisted refs, objects, workspace state,
Git tree, and working-tree bytes after the process exits.

Each scenario is executed through separate CLI processes; the test records
below identify which user-level workflows have passed.

### SYS-01: Build and reopen a software repository

**Preconditions:** Fresh directory; supported v0.5 workspace initialization and
local revision-capture workflows available.

**Steps:**

1. Run `kat init`; verify it creates the root revision and workspace base
   through the same bootstrap invariants used by v0.4 migration.
2. In a draft Change, create an Intent, Requirement, Constraint, Design
   Decision, Implementation, Artifact, and Validation. Link the decision to
   its requirement/constraint, the implementation and validation to their
   semantic targets, and the Artifact to the represented implementation.
   Run `kat commit`, then `kat check` and `kat validate` against the accepted
   semantic state. Before `kat record`, verify `status` reports that state as
   unrecorded and `push` refuses it. Record without paths; verify a
   semantic-only revision reuses the initial physical snapshot. Exercise update,
   supersede, deprecate, and accountability operations on suitable elements
   while preserving an active trace path; accept several Changes before one
   record and verify their full history remains available.
3. Add a tracked source file and an executable file by naming both paths to
   `kat record`. Verify this creates a physical-only revision with the expected
   bytes and executable mode. Then change semantic knowledge and a tracked file,
   commit the semantic Change, and record the combined revision.
4. Exit all processes. Reopen and inspect with `status`, `show`, `history`,
   `trace`, `impact`, `artifacts`, and `validate`.

**Expected:** Stable element and relationship identities; accepted semantic
state and history survive reopen; semantic-only, physical-only, and combined
revisions have the expected parents and component identities; tracked bytes
and executable mode match; read/validation
commands do not mutate accepted state; history explains the accepted Change.

**Current status:** Passed locally on 2026-10-25 by
`tests/system_acceptance.rs::sys_01_build_and_reopen_through_separate_cli_processes`.
The test uses fresh `kat` processes for each command and checks root bootstrap,
all three revision transitions, multiple semantic Changes before recording,
artifact accountability history, push refusal before recording, executable
mode, immutable named references, reopen behavior, and read-command isolation.

### SYS-02: Two developers evolve independently and converge

**Preconditions:** SYS-01 bootstrap revision published to a configured remote;
supported `kat clone` workflow available for two independent developer
directories; separate read/publish credentials as defined by the service
fixture.

**Steps:**

1. Developer A and Developer B clone the same named reference and verify the
   same initial `RepositoryId`, base revision, semantic state, and physical
   tree.
2. A creates and publishes a Requirement evolution. B independently changes
   an Implementation and a tracked source file from the common base.
3. B fetches and inspects topology, reconciles the independent histories,
   finalizes explicitly, and publishes using the ordinary fast-forward/CAS
   policy.
4. A fetches, advances or reconciles explicitly, and both repositories reopen
   at the same resulting revision.

**Expected:** Both independent changes and their histories are present in the
result; remote-tracking refs move only on fetch; neither fetch nor sync moves a
workspace base; the final physical tree and semantic state agree across users.

**Current status:** Blocked by the missing `kat clone` and `kat record`
workflows. Existing `remote_cli_e2e` seeds peer state and revisions with test
helpers.

### SYS-03: Resolve simultaneous semantic and physical conflicts

**Preconditions:** SYS-02 clone and local revision workflows available; two
developers start from the same revision.

**Steps:**

1. Developer A and Developer B independently update the same semantic element
   incompatibly and edit the same tracked file differently.
2. Both publish their descendant revisions under distinct named refs. A fetches
   B's ref and runs `sync`, then `reconcile`.
3. Inspect conflicts. Resolve the semantic conflict and physical conflict in
   separate explicit commands; attempt finalization while one remains.
4. Confirm the blocked finalization changes no authority. Resolve the remaining
   conflict, finalize, reopen, and inspect history, semantic content, and the
   physical tree.

**Expected:** Both conflict classes are reported independently; neither
disappears through ordering or finalization; final ancestry preserves both
input revisions; the resolved semantic choice and physical file content match
the explicit decisions.

**Current status:** The conflict mechanisms and local CLI flows exist. The
multi-user publication sequence is not yet exercised through user-created
repositories.

### SYS-04: Interrupt publication, restart, and retry

**Preconditions:** SYS-02 user setup available; disposable Hub/Git services
whose process and storage roots can be stopped/reopened; fault injection at
physical transfer, canonical-object transfer, and reference CAS boundaries.

**Steps:**

1. Capture authority and persistent state for the local repositories and Hub.
2. Interrupt one transfer after immutable content has moved and interrupt a
   separate publication immediately before remote CAS.
3. Stop clients and services; reopen all processes using the same persistent
   roots; run `fetch`, `sync`, and `push` as appropriate, then retry publication.
4. Fetch the successful revision into a second developer repository and
   inspect the full closure.

**Expected:** Failed publication never advances the named reference; partial
immutable content is safe to reuse; retries are idempotent; after successful
CAS the semantic and physical closure remains available; local and remote
authority before the retry matches the recorded state.

**Current status:** Component and loopback CLI tests cover interruption/retry,
revocation, CAS, and reopen separately. A full peer workflow with process and
service restarts is not yet established.

### SYS-05: Fail closed on repository-integrity faults

**Preconditions:** Disposable repositories only; one valid baseline captured
before fault injection.

**Steps:**

1. Remove a required local Git protection ref; in separate runs corrupt a
   canonical object, create an invalid named reference, and substitute a remote
   workspace backend binding.
2. Reopen the affected repository and invoke the operation that consumes the
   damaged state (open, status/validate, fetch, or push as appropriate).
3. Compare persisted accepted refs, local named refs, workspace base, active
   lineage, and working-tree bytes with the baseline. Restore the fault where
   supported, reopen, and verify recovery.

**Expected:** Each affected operation returns an explicit integrity or binding
error; invalid state is never advertised or published; no unrelated authority
moves; recoverable protection-ref failures recover only after the verified
representation is restored.

**Current status:** Focused local integrity tests exist. One composed
cross-process fault/recovery scenario remains to be run.

## Release gates

The current local score is **2/5**: SYS-01 and the regression/conformance
baseline have passed. SYS-02..SYS-05 remain pending; performance and deployment
gates are tracked separately.

| Gate | Evidence required | Status |
| --- | --- | --- |
| Local-1: lifecycle and critical CLI workflows | SYS-01 passes through real CLI processes and persistent state assertions. | Passed 2026-10-25 |
| Local-2: semantic and physical reconciliation | SYS-02 and SYS-03 preserve intended changes, conflicts, and ancestry. | Blocked by user-path prerequisites |
| Local-3: failure isolation | SYS-04 and SYS-05 show no silent authority movement and successful recovery where promised. | Not run |
| Local-4: remote publication integrity | Complete closure, CAS, fetch isolation, and retry pass through Hub and Git transports together. | Existing component evidence; system gate not run |
| Local-5: regression and conformance | `cargo test --workspace`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, including canonical vectors. | Passed 2026-10-25 |
| Readiness-1: performance | Memory, latency, and large-repository behavior characterized against stated limits. | Deferred |
| Deployment-1: infrastructure guarantees | TLS/proxy isolation, access control, retention/GC, durability, and recovery verified in a named disposable deployment environment. | Deferred |

The first five gates establish local system acceptance. Readiness-1 and
Deployment-1 are separate release-readiness work; passing local tests must not
be reported as deployment proof.

The regression gate passed on 2026-10-25 with `cargo test --workspace`,
`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
and generated-asset verification. SYS-01 is a separate process-level scenario;
the remaining integrated scenarios are not yet accepted.

## Stateful sequence testing

The audited test files contain deterministic invariant and conformance cases;
no dedicated stateful property-based system harness is present. After the
user-path prerequisites are available, add a reproducible operation-sequence
test that checks revision closure, immutable object identities, semantic
validation, and workspace authority after every operation and reopen. Add
generated/property-based sequences only if those deterministic scenarios leave
state-space interactions untested; do not add a new dependency just to duplicate
the current conformance suite.

## Execution order

1. Implement and accept same-identity `kat clone` for the multi-developer
   workflow; `kat record` and SYS-01 are complete locally.
2. Execute SYS-02..SYS-05 using the real CLI and persistent repositories,
   reusing existing loopback services and repository inspection helpers.
3. Fix discovered defects with focused regressions and record exact results in
   this tracker. Run the full regression/conformance gate after implementation
   changes; schedule deployment acceptance separately.
