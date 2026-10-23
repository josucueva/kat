# KAT v0.5 Implementation Plan

## Status
Phase 10, Phase 11, Phase 12.0, Phase 12.1, local Phase 12.2, the trusted local Hub semantic core, and the Phase 12.4 Hub/Git integration acceptance are complete locally. The Phase 12.5 contract is frozen and Step 12.5.1 (shared authorization verifier and HTTP boundary tests) is complete at `d991db4`; the authenticated Hub API/client and paired Git gateway remain pending. Remote CLI wiring, deployment authentication, production retention proof, and storage durability acceptance remain pending.

## Progress Log
| Phase | Commit | Notes |
|---|---|---|
| Phase 0 | `N/A` | Complete. Consistency resolved. |
| Phase 1 | `c806322` | Complete. RepositoryRevision structure added. |
| Phase 2 | `c806322` | Complete. WorkspaceSnapshot abstraction added. |
| Phase 3 | `3d1ed40` | Complete. GitWorkspaceBackend implementation added, physical identity enforced. |
| Phase 4..8 | `N/A` | Complete. Workspace integration, remote reference, diverge logic, semantic reconciliation. |
| Phase 9 | `a8a0f0a` | Complete. Physical reconciliation and conflicts. |
| Phase 10.1..10.9 | `21b8275` | Complete. Local collaboration commands. |
| Phase 10.10 | `7676223` | Complete. Versioned JSON collaboration interface; MI-COL checks pass in `tests/collaboration_json.rs`. |
| Phase 10.11 | `7676223` | Complete. E2E-01..10 pass through separate CLI processes; session compatibility, physical snapshot binding, candidate cleanup, and NamedReference preservation checked. Phase 10 frozen. |
| Phase 11 | `c5d8029`, `9c9d01c`, `a065de8` | Complete. Step 11.1 satisfies the planned transport abstraction criteria; parent-first uploads, complete unnamed-tip discovery, structured fetch partial outcomes, and retention contract are covered. Source review found no distinct 11.2 workflow in the plan. Remote-backed workflows and live retention enforcement are Phase 12 work. Verification reported: 631 workspace tests and 16 remote tests pass. |
| Phase 12.0 | `6f03dd3`, `0ee8b36` | Transport contract frozen in [phase12-transport-design.md](phase12-transport-design.md). KAT-controlled retention and repository binding are specified; deployment-level backend verification remains pending. |
| Phase 12.1 | `2989582` | Complete. Local snapshot representation recording fails closed unless the embedded Git repository and direct protection ref verify; mapping writes are atomic and reads validate protection refs. Workspace Git tests and the full verification gate pass. |
| Phase 12.2.1 | `facf800`, `32d1de8` | Complete. A repository/backend-bound bare Git server provisions create-only lineage refs and rejects updates, deletion, and non-lineage writes. Loopback `git daemon` verifies the receive policy. |
| Phase 12.2.2 | `1f5d8e2` | Complete. The Git `WorkspaceTransport` persists an immutable remote binding, verifies the server's repository/backend marker, transfers lineage refs, and recomputes `WorkspaceSnapshotId` from remote trees before exposing local representations. Seven loopback transport tests pass. |
| Phase 12.2.3 | `517e102` | Complete locally. The loopback tests cover partial publication/retry, concurrent identical publication, rejected in-flight publication during aggressive GC, and read-back/fetch after GC. Hub publication and deployment-level durability proof remain pending. |
| Phase 12.3 | `8a02b56` | Complete locally. Added a file-backed `SemanticTransport` with immutable canonical objects, repository-scoped named-ref CAS, fixed controlled-Git binding, closure/tree verification before CAS, and complete unnamed-tip discovery. Five Hub acceptance tests pass; full gate: 649 tests, build, fmt, and Clippy pass. This is trusted in-process code, not a network service. |
| Phase 12.4 | `bd3eb73` | Complete locally. A loopback end-to-end acceptance test publishes a semantic and physical revision through `RemoteCoordinator`, the file-backed Hub, and `GitWorkspaceTransport`, then reopens both transports and fetches it into a second initialized KAT repository. It covers missing physical content, backend mismatch, interrupted upload/retry, stale CAS, and preservation of the receiving repository's local refs, accepted state, workspace base, active lineage, and working tree. Full gate: 650 tests, build, fmt, and Clippy pass. Network and deployment guarantees remain pending. |
| Phase 12.5 design | `899aa57`, `0b1f5c8` | Contract frozen. Defines repository-bound read/publish grants, semantic and Git audiences, expiry/revocation, secret handling, independent Git discovery/pack authorization, and AUTH-01..18. Shared verifier and HTTP boundary tests are next; Hub API and Git gateway follow. |
| Phase 12.5.1 | `d991db4` | Complete. Added hash-only, repository-bound read/publish grants with separate audiences, per-request expiry/revocation/scope checks, constant-time token digest comparison, fail-closed policy reload, and loopback HTTP boundary tests. Full gate: 654 tests, build, fmt, and Clippy pass. |

## 1. Objective

To implement KAT v0.5, introducing repository-level version control, collaboration, and workspace capabilities across semantic and physical software state. This plan acts as a **controlled translation from the frozen design into executable engineering work**, with explicit checks that the implementation has not drifted from KAT's philosophy or from the v0.4 guarantees.

## 2. Normative design inputs

The plan declares which documents constrain v0.5. 

At minimum, the following documents must be respected:
- `collaboration-invariants.md`
- `repository-revision-model.md`
- `workspace-model.md`
- `reconciliation-model.md`
- `conflict-model.md`
- `remote-model.md`
- `git-workspace-backend.md`
- `collaboration-workflow.md`
- `artifact-materialization-model.md`
- `reference-model.md`
- `reconciliation-rules.md`
- `canonical-impact-audit.md`

And it must also respect the already frozen v0.4 foundation:
- first principles
- ontology
- operations model
- `ChangeRevision`
- `SemanticState`
- canonical format
- artifact accountability
- porcelain architecture
- machine interface

## 3. Scope

The implementation plan explicitly distinguishes:
- **Normative**: must be implemented as specified.
- **Deferred**: explicitly out of v0.5.
- **Open implementation choice**: may be decided during implementation without changing semantics.

## 4. Hard architectural constraints

We must verify consistently that ownership remains strict:
- `RepositoryRevision` = complete software revision
- `SemanticState` = semantic state only
- `WorkspaceSnapshot` = physical state only
- `ChangeRevision` = explicit semantic evolution
- `Workspace` = mutable local state
- `Git` = subordinate physical backend
- `Head` / `NamedReference` = mutable references, not history
- `ReconciliationCandidate` = unresolved non-accepted state

*If any document contradicts that ownership, implementation should not begin until it is corrected.*

## 5. v0.4 compatibility constraints

For every existing concept, the classification for v0.5 is:

| Existing concept | v0.5 treatment |
| :--- | :--- |
| `SemanticState` | UNCHANGED |
| `ChangeRevision` | EXTENDED/validated for reconciliation |
| `DraftSession` | EXTENDED with RepositoryRevision base context |
| Artifact semantic accountability | EXTENDED with physical dimension |
| `ObjectId` | UNCHANGED |
| canonical CBOR | UNCHANGED rules |
| current accepted repository state | REPLACED/GENERALIZED by RepositoryRevision |
| physical versioning | NEW |
| Git backend | NEW |
| multiple heads | NEW |
| conflict state | NEW |

*Note: The migration compatibility constraint applies from Phase 1 onward. No phase may rewrite existing v0.4 canonical objects.*

## 6. Pre-implementation decisions

The following hard decisions must be explicitly resolved before code depends on them:

### Canonical representation
- `RepositoryRevision` exact canonical fields
- `WorkspaceSnapshotId` representation
- `MaterializationId` representation
- Physical Artifact baseline: explicit `MaterializationId` vs. derive from `RepositoryRevision`
- `ChangeRevision` semantics for reconciliation

### Git representation
- Synthetic Git commit behavior
- Internal repo layout
- Working tree association
- Reachability protection
- Existing Git adoption

### Conflict persistence
- Where unresolved conflicts (local persistent state) live
- Crash recovery mechanisms

---

## Phase 0: Design consistency and canonical foundation
### Goal
Compare all v0.5 documents against each other and against the existing architecture.
### Design inputs
- All normative documents listed in Section 2.
### Traceability
- Addresses foundational consistency prior to engineering execution.
### Preconditions
- None.
### Design constraints
- Identify contradictions, duplicate concepts, terminology drift, unresolved ownership, canonical-model conflicts, and philosophy inconsistencies.
### Implementation work
- Resolve decisions from Section 6.
- Resolve `WorkspaceSnapshot` identity semantics, equality semantics, and canonical/backend identity boundary.
- Define a deterministic merge-base policy for DAG ancestry (needed for Phase 7).
- Verify ownership (e.g. `SemanticState` = semantic state only).
### Tests
- N/A (Documentation phase).
### Regression checks
- N/A
### Failure / recovery cases
- N/A
### Acceptance criteria
- All contradictions resolved and canonical format confirmed. Phase 0 gate passed.
### Artifacts produced
- A **decision register** resolving:
  - exact `RepositoryRevision` canonical structure
  - `WorkspaceSnapshotId` representation
  - `MaterializationId` representation
  - Artifact physical baseline decision
  - `ChangeRevision` reconciliation decision
  - deterministic merge-base policy
  - Git repository layout decision
  - conflict persistence decision
  - interface schema compatibility decision
  *(Each must end with RESOLVED, DEFERRED WITH JUSTIFICATION, or NO CHANGE REQUIRED).*
- A **consistency report** stating that the normative documents were cross-checked and either no contradiction exists, or identifying the exact document corrected.
### Out of scope for this phase
- Writing Rust code.

## Phase 1: RepositoryRevision
### Goal
Establish the `RepositoryRevision` foundation.
### Design inputs
- `repository-revision-model.md`
- Phase 0 decision register
### Traceability
- COLL-01 Complete software revision
- COLL-02 Independent evolution dimensions
- COLL-03 RepositoryRevision is repository authority
### Preconditions
- Phase 0 complete.
### Design constraints
- Binds semantic state and physical snapshot.
### Implementation work
- Add `RepositoryRevisionId` newtype.
- Add `RepositoryRevision` domain type.
- Define parent cardinality rules.
- Bind exactly one `SemanticStateId`.
- Bind exactly one `WorkspaceSnapshotId`.
- Represent optional semantic `ChangeRevisionId`.
- Define initial revision rules.
- Define normal single-parent revision rules.
- Define reconciliation multi-parent rules.
- Add structural validation.
- Add canonical encoding.
- Add decoding if canonical decoder exists.
- Add storage/load support.
- Add `ObjectId` derivation.
- Add repository integrity validation.
### Tests
- initial revision
- semantic-only successor
- physical-only successor
- combined successor
- multi-parent successor
- invalid missing state
- invalid missing workspace snapshot
- invalid parent structure
- canonical deterministic bytes
- stable `ObjectId`
- unknown referenced object
### Regression checks
- all existing `SemanticState` and `ChangeRevision` vectors unchanged.
- `cargo test --workspace`, `cargo fmt --check`, `cargo clippy ... -D warnings`.
### Failure / recovery cases
- Load failures (e.g., referenced objects missing).
### Acceptance criteria
- Unit tests and canonical conformance tests pass.
### Artifacts produced
- `RepositoryRevision` domain types and encoder/decoder.
### Out of scope for this phase
- Git integration, workspace abstractions.

## Phase 2: WorkspaceSnapshot
### Goal
Establish the `WorkspaceSnapshot` abstraction.
### Design inputs
- `workspace-model.md`
- Phase 0 decision register
### Traceability
- Abstract physical state mapping independent of Git specifics.
### Preconditions
- Phase 1 complete.
### Design constraints
- Represents physical state only. Backend-neutral contract.
### Implementation work
- Define `WorkspaceBackend` required capabilities:
  - inspect working state
  - create immutable snapshot
  - materialize snapshot
  - compare snapshots
  - resolve Artifact materialization
  - verify snapshot integrity
- Implement the backend-neutral abstraction according to the identity and equality semantics resolved in Phase 0.
### Tests
- Property/invariant tests on the abstraction.
- Fake/In-memory backend testing.
### Regression checks
- `cargo test --workspace`, `cargo fmt --check`, `cargo clippy ... -D warnings`.
- Existing canonical vectors unchanged.
### Failure / recovery cases
- Missing referenced snapshot.
### Acceptance criteria
- Can be tested with an in-memory/fake backend before Git integration.
### Artifacts produced
- `WorkspaceBackend` traits and `WorkspaceSnapshot` structures.
### Out of scope for this phase
- Actual Git implementation.

## Phase 3: GitWorkspaceBackend
### Goal
Integrate Git as the subordinate physical backend.
### Design inputs
- `git-workspace-backend.md`
- Phase 0 decision register
### Traceability
- Ensures Git is strictly a backend, not the source of truth.
### Preconditions
- Phase 2 complete.
### Design constraints
- Git HEAD is not repository authority.
- Git commit identity != `RepositoryRevisionId`.
- Git branch != KAT reference.
### Implementation work
- [x] Initialize managed Git storage.
- [x] Adopt existing Git repository.
- [x] Maintain the backend mapping between `WorkspaceSnapshotId` and the immutable Git commit representing that physical snapshot.
- [x] Generate synthetic physical commits where necessary.
- [x] Materialize a snapshot safely.
- [x] Inspect tracked working state.
- [x] Distinguish ordinary edits from backend mismatch.
- [x] Protect referenced objects from Git GC.
- [x] Support physical ancestry queries.
- [x] Support physical three-way merge primitives.
- [x] Establish hidden/internal ref strategy if required.
- [x] Verify exact snapshot content after materialization.
### Tests
- [x] fresh non-Git project, existing Git project
- [x] tracked modification, tracked addition, tracked deletion
- [x] untracked content, ignored content, empty workspace
- [x] snapshot reuse
- [x] same tree across different Git metadata
- [x] external checkout, external reset, Git HEAD moved independently, detached HEAD
- [x] missing object, GC/reachability
### Regression checks
- `cargo test --workspace`, `cargo fmt --check`, `cargo clippy ... -D warnings`.
- Existing canonical vectors unchanged.
### Failure / recovery cases
- Missing Git binary, corrupted Git object store, concurrent external Git operations.
### Acceptance criteria
- Git integration tests pass for snapshot creation and reuse.
- **Two physical states with identical tracked content must resolve to equivalent WorkspaceSnapshot content identity regardless of mutable Git metadata.**
### Artifacts produced
- `GitWorkspaceBackend` implementation.
### Out of scope for this phase
- Remote fetching/pushing.

## Phase 4: Workspace integration
### Goal
Manage local workspace lifecycle and detect tracked physical changes.
### Design inputs
- `workspace-model.md`
### Traceability
- Workspace encapsulates mutable local state only.
### Preconditions
- Phase 3 complete.
### Design constraints
- Workspace base remains a `RepositoryRevision`.
- Ordinary file edits are not backend mismatch.
- Untracked files are not silently included.
### Implementation work
- [x] Implement `Workspace`: identity, `base_repository_revision`, semantic draft state, physical backend association.
- [x] Handle behaviors: open workspace, reload workspace, clean workspace.
- [x] Detect semantic-only modified, physical-only modified, combined modified.
- [x] Guarantee base never silently moves.
### Tests
- [x] persistence/restart tests.
- [x] tracked modification detected.
- [x] untracked/ignored file excluded.
- [x] unchanged workspace reuses snapshot.
- [x] direct backend movement classified separately.
### Regression checks
- `cargo test --workspace`, `cargo fmt --check`, `cargo clippy ... -D warnings`.
- Existing canonical vectors unchanged.
### Failure / recovery cases
- Interruption during modification state writes.
### Acceptance criteria
- Accurate working-state DTO exposed and workspace lifecycle behaves reliably.
### Artifacts produced
- `Workspace` management structures.
### Out of scope for this phase
- Multi-user conflict state.

## Phase 5: Artifact materialization accountability
### Goal
Extend Artifact accountability with physical drift tracking.
### Design inputs
- `artifact-materialization-model.md`
- Phase 0 decision register
### Traceability
- Accurate drift tracking across semantic and physical dimensions.
### Preconditions
- Phase 4 complete.
### Design constraints
- Physical file change does not create semantic `STALE`.
- Semantic dependency change does not create `MODIFIED`.
### Implementation work
- Resolve Artifact locator against accepted `WorkspaceSnapshot` and current working physical state.
- Compute `MaterializationId`.
- Compare semantic baseline and physical baseline.
- Produce independently: `SemanticAccountability` and `PhysicalMaterializationStatus`.
### Tests
- Full matrix: CURRENT/CURRENT, STALE/CURRENT, CURRENT/MODIFIED, STALE/MODIFIED, CURRENT/MISSING, STALE/MISSING.
- UNACCOUNTED + resolvable physical content.
- UNRESOLVED locator.
- **working-state accountability versus accepted-revision accountability**:
  - accepted R42 is semantic CURRENT / physical CURRENT.
  - working tree modified becomes semantic CURRENT / physical MODIFIED.
  - R42 itself remains CURRENT / CURRENT. (Mutable workspace drift does not retroactively alter accepted history).
### Regression checks
- `cargo test --workspace`, `cargo fmt --check`, `cargo clippy ... -D warnings`.
- Existing accountability invariants remain unchanged where physical state is unchanged.
### Failure / recovery cases
- Unresolvable locators handled safely without crashing.
### Acceptance criteria
- Matrix tests pass perfectly; semantic and physical drift tracked orthogonally.
### Artifacts produced
- Updated `ArtifactAccountabilityReport` logic.
### Out of scope for this phase
- Automated artifact generation.

## Phase 6: Heads and references
### Goal
Establish mutable references.
### Design inputs
- `reference-model.md`
### Traceability
- Heads and NamedReferences are mutable pointers, not history.
### Preconditions
- Phase 5 complete.
### Design constraints
- Moving a `NamedReference` does not alter `RepositoryRevision` ObjectIds.
### Implementation work
- Compute visible heads, multiple heads, unnamed heads.
- Implement full `RepositoryRevisionId` selector, unique prefix selector, named reference selector.
- Differentiate local vs remote-observed references.
- Implement CAS reference movement.
- Handle ambiguous prefix failure, stale expected-target failure.
### Tests
- moving main changes no canonical object.
- deleting a name deletes no history.
- workspace remains on R42 when main moves to R43.
- two concurrent updates to main: one CAS succeeds, one fails.
### Regression checks
- `cargo test --workspace`, `cargo fmt --check`, `cargo clippy ... -D warnings`.
### Failure / recovery cases
- CAS collision on reference update.
### Acceptance criteria
- References update correctly, independent of history.
### Artifacts produced
- `Head` and `NamedReference` domain types and store.
### Out of scope for this phase
- Syncing references remotely.

## Phase 7: Local divergence
### Goal
Detect when local workspace has diverged from references.
### Design inputs
- `workspace-model.md`, `collaboration-workflow.md`
### Traceability
- Detect without mutating accepted state.
### Preconditions
- Phase 6 complete.
### Design constraints
- Divergence must be computed from the `RepositoryRevision` DAG, not Git ancestry.
### Implementation work
- Implement revision ancestry comparison (same, local ahead, other ahead, diverged).
- Implement common ancestor discovery (using deterministic merge-base policy established in Phase 0).
- Implement visible head computation for multiple local accepted heads.
### Tests
- linear ancestry, siblings, deep divergence, multi-parent ancestry, already reconciled histories, unrelated/invalid ancestry.
### Regression checks
- `cargo test --workspace`, `cargo fmt --check`, `cargo clippy ... -D warnings`.
### Failure / recovery cases
- Handle fragmented local DAG (missing referenced objects).
### Acceptance criteria
- Divergence states accurately derived purely from KAT DAG.
### Artifacts produced
- Divergence analysis API.
### Out of scope for this phase
- Executing merges.

## Phase 8: Semantic reconciliation
### Goal
Reconcile divergent semantic histories.
### Design inputs
- `reconciliation-rules.md`, `reconciliation-model.md`
### Traceability
- Idempotency and consistency of the semantic rules engine.
### Preconditions
- Phase 7 complete.
### Design constraints
- Input order does not choose a winner.
- Automatic composition is deterministic.
- Both parents preserved on success.
### Implementation work
- Find semantic common base.
- Identify concurrent Change effects.
- Classify operation interactions (compose AUTO interactions, deduplicate IDEMPOTENT interactions).
- Preserve AUTO + CONSEQUENCE findings.
- Emit `SemanticConflict` candidates.
- Apply both operation orders where required.
- Compare canonical results.
- Revalidate composed `SemanticState`.
- Create reconciliation `ChangeRevision` when required.
- Construct semantic reconciliation result.
### Tests
- Enumeration of all frozen rule classes: Create/Create different, Create/Create identical, Create/Create conflicting same identity, Update/Update different, Update/Update same identity, Update/Deprecate same identity, Deprecate/Deprecate equivalent, Link/Link independent, Link/Unlink same relationship, Supersede/Supersede competing, Account/Account different, Account/Update dependency -> consequence, combined invariant failure, input-order determinism.
### Regression checks
- `cargo test --workspace`, `cargo fmt --check`, `cargo clippy ... -D warnings`.
### Failure / recovery cases
- Semantic conflict cleanly returns unresolved candidate without mutating accepted state.
### Acceptance criteria
- Reconciliation rules strictly implemented, order-independent.
- **AUTO + CONSEQUENCE must successfully reconcile while preserving the resulting health/accountability finding.**
### Artifacts produced
- Semantic reconciler logic.
### Out of scope for this phase
- Physical file merge, conflict resolution UI.

## Phase 9: Physical reconciliation and conflicts
### Goal
Handle physical merge and conflict state.
### Design inputs
- `reconciliation-model.md`
- `conflict-model.md`
- `git-workspace-backend.md`
### Traceability
- Conflict resolution never deletes parent histories.
- Unresolved conflicts do not leak into accepted states.
### Preconditions
- Phase 8 complete.
### Design constraints
- No `RepositoryRevision` becomes accepted while unresolved conflicts remain.
### Implementation work
- 9.1 physical three-way reconciliation.
- 9.2 conflict classification.
- 9.3 `ReconciliationCandidate` persistence.
- 9.4 explicit resolution.
- 9.5 accepted reconciled `RepositoryRevision` creation.
### Tests
- independent file edits.
- same-file clean Git merge.
- content conflict.
- delete/modify.
- rename/move where supported.
- semantic clean + physical conflict.
- semantic conflict + physical clean.
- both conflict.
- restart with unresolved candidate.
- abort reconciliation.
- resolve then commit.
- **RA and RB ObjectIds remain unchanged before and after resolution.**
### Regression checks
- `cargo test --workspace`, `cargo fmt --check`, `cargo clippy ... -D warnings`.
### Failure / recovery cases
- Interrupted candidate persistence, resolving crashes.
### Acceptance criteria
- Conflict state safely serialized; no accepted state created with conflicts.
### Artifacts produced
- Full reconciler engine and conflict tracker.
### Out of scope for this phase
- Interactive merge tooling integration.

## Phase 10: Collaboration porcelain
### Goal
Provide CLI tools for collaboration.
### Design inputs
- `collaboration-workflow.md`, `collaboration-porcelain.md`
### Traceability
- Adherence to v0.4 porcelain architecture and POR-01 through POR-12 invariants.
### Preconditions
- Phase 9 complete.
### Design constraints
- Conform to existing CLI philosophy and the strict command surface area (reconcile != publish).
### Implementation work
- 10.1 Reconciliation session/workspace state model.
- 10.2 Collaboration status/query APIs (`kat status` updates).
- 10.3 Reconcile porcelain (`kat reconcile`).
- 10.4 Inspect conflicts (`kat conflicts`).
- 10.5 Candidate materialization (applying physical conflicts to working tree).
- 10.6 Semantic/physical resolution workflows (`kat resolve`).
- 10.7 Abort workflow (`kat abort`).
- 10.8 Finalize/accept workflows (`kat finalize` or `kat accept`).
- 10.9 Switch/advance workflows (`kat switch`, explicit advance).
- 10.10 JSON machine interfaces.
- 10.10 execution plan: [implementation-plan-phase10-10.md](implementation-plan-phase10-10.md).
- 10.11 Full E2E collaboration tests.
- 10.11 execution plan: [implementation-plan-phase10-11.md](implementation-plan-phase10-11.md).
### Tests
- Local end-to-end usability scenarios mirroring real workflow.
### Regression checks
- `cargo test --workspace`, `cargo fmt --check`, `cargo clippy ... -D warnings`.
- CLI machine-contract fixtures unchanged unless deliberately extended.
### Failure / recovery cases
- Operations that require a clean workspace must reject incompatible dirty state with deterministic diagnostics and without altering accepted state.
### Acceptance criteria
- Workflows work end-to-end locally, producing JSON and human output properly.
### Artifacts produced
- CLI subcommands.
### Out of scope for this phase
- Remote-backed execution of `sync`/`push` (completed in Phase 12).

## Phase 11: Remote abstraction
### Step 11.1: Transport abstraction and hardening
- Status: Frozen after hardening and full verification.
- [x] Record named-reference CAS, recursive completeness, retry, and fetch authority contracts.
- [x] Add semantic and workspace transport traits with an in-memory coordinator.
- [x] Add fake-remote acceptance tests for closure verification, partial transfer, CAS, and local authority preservation.
- [x] Upload revision parents before children; require closure-complete unnamed-tip discovery and reject an advertised incomplete tip.
- [x] Preflight tracking names and namespace conflicts; report per-reference partial CAS outcomes after full content verification.
- [x] Require published-reference closure retention by contract; defer concrete retention/collection enforcement to Phase 12.
- Notes: Phase 11 is complete in this step. The plan has no separate repository-level workflow acceptance criteria for 11.2; the existing coordinator already composes local stores with injected transports. No network, Git remote commands, KAT Hub behavior, or live retention enforcement is added; those remain Phase 12 work.
### Goal
Implement abstractions for remote syncing.
### Design inputs
- `remote-model.md`
### Traceability
- Local state handles fetch without modifying base workspace.
### Preconditions
- Phase 10 complete.
### Design constraints
- Avoid locking the domain to HTTP at this layer.
- Keep repository coordination, semantic object transport, and physical snapshot transport as separate responsibilities.
- Fetch transfers and verifies content before updating remote-tracking observations. It never moves local named references, `refs/accepted`, a workspace base, or the working tree.
- Publish verifies the complete destination closure before compare-and-swapping one named remote reference.
- Discovery advertises only unnamed tips with complete revision, semantic, and physical dependency closure; the coordinator verifies every advertised closure and errors if a tip is incomplete.
- Fetch validates remote names and local tracking namespace conflicts before transfer. Independent per-reference CAS may partially succeed and returns an outcome that identifies updated, unchanged, removed, and failed refs.
- Successfully published named refs retain their recursively referenced revisions, semantic dependencies, and snapshots while reachable; concrete synchronization with collection is Phase 12 work.
### Implementation work
- Add a `RemoteCoordinator` over `SemanticTransport` and `WorkspaceTransport`.
- `SemanticTransport` lists named references and closure-complete read-only unnamed DAG tips, fetches/stores canonical bytes by `ObjectId`, and compare-and-swaps a named remote reference.
- Unnamed DAG tips are derived observations; do not add a mutable `Head` type or unnamed-head mutation operation.
- `WorkspaceTransport` checks local and remote snapshot availability, fetches snapshots, and publishes snapshots by `WorkspaceSnapshotId` without materializing the working tree.
- Recursively resolve and verify every parent `RepositoryRevision`, its semantic state and change dependencies, and every physical snapshot in the transferred ancestry. Verify hashes, canonical decoding, expected object kinds, and semantic-change/result-state consistency.
- Upload revision parents before children. Keep partially transferred immutable objects for retry. Do not update a named reference until semantic and physical destination completeness has been verified.
- Do not move shared references until the transport can retain their complete semantic and physical closure for the lifetime of reachability; Phase 11 specifies and fake-tests this contract, Phase 12 implements its storage enforcement.
### Tests
- In-memory fake transports cover reference discovery (including unnamed tips), object and snapshot transfer, idempotent duplicate transfer, recursive closure verification, and per-reference CAS.
- `REMOTE-11` covers an interrupted parent-first revision upload, incomplete unnamed-tip suppression, and explicit failure if discovery advertises that incomplete tip.
- Fetch tests prove invalid names fail before transfer, content failures do not update tracking references, later CAS failure reports earlier successful updates, and local authority never moves.
- Publish tests prove that missing dependencies or snapshots prevent CAS, partial uploads may remain, and stale CAS preserves the current remote reference and the local candidate.
- Successful publication tests verify that the fake transports retain the complete semantic and physical closure after CAS.
### Regression checks
- `cargo build`, `cargo test --workspace`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`.
### Failure / recovery cases
- Distinguish unavailable transports, missing objects/snapshots, integrity failures, and stale CAS expectations.
- Report tracking-ref CAS failures per reference without losing partial success information.
- Retry after partial immutable transfers; duplicate publication is idempotent.
### Acceptance criteria
- Fetch and publish handle complete revision DAGs, preserve all local authority invariants, and report per-reference update failures after content verification.
- Unnamed discovery never advertises an incomplete tip; a transport that does so causes an explicit fetch integrity error.
- The publication retention contract is documented and exercised by the fake transports; concrete enforcement is explicitly deferred to Phase 12.
### Artifacts produced
- `RemoteCoordinator`, `SemanticTransport`, `WorkspaceTransport`, and in-memory acceptance tests.
### Out of scope for this phase
- Actual KAT Hub or Git network calls.

## Phase 12: KAT Hub and Git remote integration
### Step 12.0: Concrete transport and publication design
- Status: Transport contract frozen; deployment acceptance pending.
- [x] Define semantic and Git-backed workspace transfer, reference visibility, retention, GC, repository binding, and failure/recovery behavior without changing Phase 11 interfaces.
- [ ] Select and configure a KAT-controlled workspace backend; verify remote ref read-back and recompute the tree-derived `WorkspaceSnapshotId` before CAS.
- [ ] Verify that the backend rejects lineage-ref update/deletion and preserves refs through Git GC; bind its fixed `WorkspaceBackendId` in the Hub repository record.
- Notes: The design contract is frozen independently of deployment proof. Stages 12.2.1 and 12.2.2 exercise a provisioned backend over loopback `git daemon`; this is not a production deployment. No KAT Hub or workspace remote is configured in the project repository. Its `origin` points to GitHub for the project source, not a KAT physical workspace remote. GitHub documentation does not establish retention of `refs/kat/*`; Phase 12 v1 therefore targets a KAT-controlled Git backend.
### Step 12.1: Local Git snapshot retention hardening
- Status: Complete (`2989582`).
- [x] Require the embedded repository and target commit to open before recording a representation.
- [x] Create protection refs without force and verify that each is a direct ref to the expected commit.
- [x] Persist mappings with atomic replacement and fail closed on mapping read/write errors.
- [x] Validate protection refs whenever representations are loaded; propagate integrity errors through workspace operations.
- [x] Cover missing repository, conflicting/missing protection refs, failed recording without representation exposure, and recovery after repair.
- Notes: An interrupted mapping write may leave an orphan protection ref, which is safe and retryable. If post-write ref verification fails, lookups reject the mapping until the ref is repaired. This step does not establish server-side ref immutability or GC retention.
### Step 12.2: Controlled physical transport and retention enforcement
- Status: Local implementation complete; deployment policy/durability proof pending. No Hub semantic reference publication in this step.
- Stage 12.2.1 — Controlled Git backend: Complete (`facf800`).
  - [x] Provision a bare Git repository bound to one KAT `RepositoryId` and `WorkspaceBackendId`.
  - [x] Enforce create-only `refs/kat/lineage/**` updates on the server; reject ref updates and deletion through supported Git operations.
  - [x] Test through a real loopback Git daemon; verify binding and receive-hook drift fail closed.
- Stage 12.2.2 — Concrete workspace transport: Complete (`1f5d8e2`).
  - [x] Persist one immutable local binding and verify the configured server's advertised `RepositoryId` and `WorkspaceBackendId` marker before transfer.
  - [x] Implement the frozen `WorkspaceTransport` contract; transfers use only KAT lineage refs and cannot select a destination per operation.
  - [x] Read refs and commits back into a fresh Git repository, recompute `WorkspaceSnapshotId`, and record local representations only after integrity verification.
  - [x] Verify publication, fetch without materializing, wrong repository/backend binding, and wrong tree identity in seven loopback integration tests.
- Stage 12.2.3 — Retention and failure validation: Complete locally (`517e102`).
  - [x] Verify per-ref push rejections are surfaced; a second representation can fail after the first remains safely published, then retry completes idempotently.
  - [x] Race identical publication calls and verify only one immutable lineage ref remains.
  - [x] Run aggressive GC during a paused receive; the server rejects the now-incomplete upload, creates no lineage ref, and a retry succeeds after GC.
  - [x] Run `git gc --prune=now` after publication, read the exact remote tree back, and fetch it into a second KAT repository.
  - [x] Confirm invalid fetched tree identity leaves no local representation or staging ref.
- Acceptance: local loopback tests prove that published snapshots remain fetchable after GC, client-visible mutation/deletion attempts fail, mismatched binding/tree identity fails closed, and retries are idempotent. Deployment policy and storage durability must still be verified before production Hub publication is accepted; Step 12.3 checks the local controlled-backend policy before CAS.
- Trust boundary: supported GC after publication preserves refs; aggressive concurrent GC may reject an in-flight upload, which leaves no lineage ref and requires retry. Unrestricted host administrators and underlying storage loss remain deployment assumptions; root-level ref/hook changes and physical storage loss are outside the guarantee.
### Goal
Implement the actual remote sync via KAT Hub/Git.
### Design inputs
- `remote-model.md`
- `git-workspace-backend.md`
- [Phase 12 transport and publication design](phase12-transport-design.md)
### Traceability
- Changing Git remote URL does not alter canonical state.
- Shared reference never points to an incomplete `RepositoryRevision`.
### Preconditions
- Phase 11 complete.
### Design constraints
- Remote sync is purely transport and CAS; no semantic reinterpretation.
- Keep the Phase 11 transport trait signatures and canonical formats unchanged.
- Store exact canonical bytes by `ObjectId`; never re-encode during transfer.
- Keep KAT named references in the semantic service. Git refs carry physical snapshots only.
- Publish physical snapshot refs before semantic revisions; advance one semantic named ref with linearizable CAS only after full destination verification against the repository's fixed workspace-backend binding.
- Retain published semantic content in append-only Hub storage and physical content through server-enforced create-only `refs/kat/lineage/<snapshot-id>/<git-oid>` roots. Hub CAS verifies the active backend policy and read-back tree; do not delete these roots or collect Hub semantic objects in Phase 12.
- Treat fetch as observation: only remote-tracking refs may change after full closure verification; accepted state, local named refs, workspace base, active lineage, and working tree remain unchanged.
### Implementation work
- Implement a Git `WorkspaceTransport` over the internal bare repository and the Hub-bound KAT-controlled Git backend; fetch and publish only KAT lineage refs and rebuild local snapshot mappings from verified refs.
- Implement or integrate the KAT Hub `SemanticTransport` service/client: repository-scoped immutable canonical objects, coherent discovery, unnamed-tip derivation with paired physical-closure checks, and per-name linearizable CAS that rejects missing or changed backend bindings and unverified retention policy.
- Compose the transports with `RemoteCoordinator` without adding a second repository coordination wrapper.
- Implement the publish sequence: verify local closure; publish and verify physical snapshot refs; upload semantic dependencies and revision parents before children; read back and verify the destination closure; CAS one named KAT ref as the visibility boundary.
- Implement fetch and sync workflow behavior: fetch updates only remote-tracking refs; sync reports topology relative to those observations without automatic publish, reconciliation, reference movement, or materialization.
- Keep Phase 12 remote storage append-only. Any later KAT-aware pruning must synchronize with reference CAS and protect in-flight publication.
### Tests
- Exercise the Git transport against a controlled bare remote: lineage ref push/fetch/read-back, multiple representations per snapshot, identity mismatch, idempotent retry, denied ref update/deletion, and concurrent `git gc` preservation.
- Test that local lineage-ref creation failure fails snapshot recording/export, that a mismatched physical backend cannot pass Hub verification, and that a missing or disabled retention policy prevents named-reference CAS.
- Exercise the Hub semantic transport: exact byte preservation, duplicate and mismatched object writes, repository mismatch, coherent discovery, incomplete closure, CAS success/race, and lost-response recovery.
- **Fetch semantics**: fetch missing immutable objects and physical snapshots, fetch does not move local reference or workspace base, fetch is idempotent, and partial failure leaves authority unchanged.
- **Publication failures**: Git upload fails, semantic upload fails, `RepositoryRevision` upload fails, destination verification fails, CAS loses race, response is lost after CAS, retry publication, and duplicate upload.
- Test that incomplete unnamed tips are omitted, incomplete named refs fail closed, and every advertised closure passes independent coordinator verification.
- Test fetch/push/sync preserve accepted state, local named refs, workspace base, active lineage, and the working tree as specified.
### Regression checks
- `cargo test --workspace`, `cargo fmt --check`, `cargo clippy ... -D warnings`.
### Failure / recovery cases
- Protocol must cleanly fail before CAS if any upload step fails.
### Acceptance criteria
- The concrete transports satisfy the frozen Phase 11 interfaces and move canonical objects and Git-backed snapshots without changing canonical identities.
- A KAT named ref advances only after the complete semantic and physical destination closure is verified; CAS never overwrites a concurrent target.
- Published closures survive concurrent Git GC and remain available under append-only Hub object retention.
- The Hub repository configuration fixes each `RepositoryId` to a `WorkspaceBackendId`; no publication request can select or silently switch the physical destination.
- The configured Git server rejects lineage-ref mutation/deletion and the Hub verifies the target ref/tree against the KAT snapshot ID before CAS.
- Fetch and sync preserve local authority; retry and ambiguous-CAS behavior match the failure/recovery design.
- The multi-user remote scenarios across semantic and physical transports pass.
### Artifacts produced
- KAT Hub semantic transport integration, Git workspace transport, and repository remote workflow integration.
### Step 12.3: Trusted Hub semantic service core
- Status: Complete locally (`8a02b56`).
- [x] Store exact, validated canonical bytes in repository-scoped append-only object namespaces.
- [x] Persist named references through per-name CAS and reject invalid names or repository mismatches.
- [x] Fix each Hub repository to one `WorkspaceBackendId` and controlled Git repository; reject binding changes.
- [x] Verify the active controlled-backend policy, recursive semantic closure, and every referenced snapshot tree before CAS.
- [x] Discover only closure-complete unnamed DAG tips; fail listing when a named ref or ref file is invalid.
- [x] Test interrupted upload/retry, missing semantic and physical dependencies, invalid backend binding, policy drift, CAS race, Hub reopen, and publish/fetch through `RemoteCoordinator` with a test workspace transport; Hub-side snapshot verification uses the controlled Git repository.
- Notes: This first Hub service boundary is a trusted in-process file-backed core. It has no network listener, endpoint schema, authentication, or CLI wiring. The deployment must restrict the Hub data root with operating-system permissions; the library does not set ACLs. Production retention, durability, and administrative trust remain unverified.
### Step 12.4: Hub and Git transport integration
- Status: Complete locally (`bd3eb73`).
- [x] Compose `RemoteCoordinator`, `TrustedHubSemanticTransport`, and `GitWorkspaceTransport` against the same controlled Git backend and publish one revision with both semantic and physical changes.
- [x] Reject publication when a required physical snapshot is absent or the Hub/workspace backend binding differs; verify an interrupted physical upload leaves the named reference unchanged and can be retried.
- [x] Verify stale CAS cannot advance the named reference, then reopen the Hub and Git transports and fetch the complete closure into a second initialized KAT repository.
- [x] Confirm fetch preserves the receiving repository's local named ref, accepted state, workspace base, active Git lineage, and working-tree bytes while transferring exact canonical objects and snapshots.
- Notes: The acceptance test uses a Unix loopback `git daemon` and the trusted in-process Hub core. It proves integrated local behavior only; it does not establish network authentication, production retention, or deployment durability.
### Not included in Steps 12.3–12.4
- Remote authentication, network endpoint/client, and production deployment policy proof. The local core must not be exposed to untrusted processes.

### Step 12.5: Remote service access and authorization
- Status: Contract frozen (`0b1f5c8`); network implementation in progress.
- [x] Define a versioned HTTPS JSON API and status mapping for the existing `SemanticTransport` operations.
- [x] Define shared repository-bound read/publish grants, semantic/Git audiences, mandatory expiry, out-of-band revocation, and fail-closed policy reload.
- [x] Require independent authorization of Git smart HTTP discovery and pack requests through the same verifier used by the Hub API.
- [x] Freeze the authorization matrix AUTH-01..18, publication closure gate, TLS boundary, and secret-redaction requirements.
- [x] Implement the shared authorization verifier and real HTTP request boundary tests (`d991db4`).
- [ ] Implement the authenticated Hub API and semantic client without changing `SemanticTransport`.
- [ ] Implement the paired Git smart HTTP gateway with repository binding and read/publish authorization.
- [ ] Pass AUTH-01..18 through integrated Hub/Git requests before adding remote CLI workflows.
- Notes: The frozen contract is recorded in [phase12-service-design.md](phase12-service-design.md). No network endpoints are included yet. Deployment grant provisioning, production retention, and storage durability still need implementation and evidence.

## Phase 13: Migration and compatibility
### Goal
Migrate v0.4 repositories to v0.5 seamlessly.
### Design inputs
- `canonical-impact-audit.md`
- `repository-revision-model.md`
- `git-workspace-backend.md`
- Phase 0 decision register
- v0.4 repository format and canonical specifications
### Traceability
- Existing canonical v0.4 objects remain byte-for-byte unchanged.
### Preconditions
- Phase 12 complete.
### Design constraints
- Migration compatibility constraint applies from Phase 1 onward. No phase may rewrite existing v0.4 canonical objects.
### Implementation work
- Detect v0.4 repository.
- Preserve every existing canonical object.
- Capture/adopt initial physical workspace.
- Create first `RepositoryRevision` wrapper.
- Initialize local workspace base.
- Initialize local head/reference metadata.
- Record repository format version transition.
### Tests
- v0.4 repo without Git, v0.4 repo already inside Git, clean physical workspace, dirty physical workspace, migration interrupted, migration repeated, post-migration v0.4 `ObjectId`s unchanged.
### Regression checks
- `cargo test --workspace`, `cargo fmt --check`, `cargo clippy ... -D warnings`.
- Post-migration v0.4 ObjectIds verified unchanged.
### Failure / recovery cases
- Interrupted migration can restart or roll back cleanly.
### Acceptance criteria
- v0.4 repository opens and migrates without mutating golden ObjectIds.
### Artifacts produced
- Migration subsystem.
### Out of scope for this phase
- Manual migration CLI flags.

## Phase 14: End-to-end evaluation
### Goal
Validate full workflow between multiple users.
### Design inputs
- Entire v0.5 feature set.
### Traceability
- Validates the entire collaboration and distributed version control feature set.
### Preconditions
- Phases 1-13 complete.
### Design constraints
- Must use empirical scenario matrix.
### Implementation work
- Execute scenario matrix.
### Tests
- E2E-01 Independent semantic collaboration (R0 -> RA, RB; sync; reconcile -> RC).
- E2E-02 same semantic identity conflict.
- E2E-03 physical-only divergence.
- E2E-04 physical merge conflict.
- E2E-05 semantic + physical combined evolution.
- E2E-06 Artifact becomes stale after clean reconcile.
- E2E-07 offline work then publication.
- E2E-08 CAS publication race.
- E2E-09 failed remote publication and retry.
- E2E-10 external Git movement detected.
### Regression checks
- `cargo test --workspace`, `cargo fmt --check`, `cargo clippy ... -D warnings`.
- ALL Canonical vectors.
### Failure / recovery cases
- Covered by the E2E failure scenarios, particularly E2E-08 and E2E-09.
### Acceptance criteria
- All E2E scenarios pass matching expected state exactly.
### Artifacts produced
- E2E test suite.
### Out of scope for this phase
- Adding new features based on test outcome.

---

## Regression strategy
- **A. Unit tests**: For deterministic local logic.
- **B. Canonical conformance tests**: Golden vectors, exact CBOR bytes. (Same logical `RepositoryRevision` -> same canonical bytes).
- **C. Regression tests**: Existing 660 tests pass. `v0.4` guarantees intact. *No v0.5 step may update existing golden vectors merely to make tests pass unless explicitly approved.* (Note: Migration compatibility constraint applies from Phase 1 onward).
- **D. Property / invariant tests**: e.g., input order doesn't choose a winner, fetch doesn't move workspace base.
- **E. Git integration tests**: Use real Git repos to test native tracking/reachability.
- **F. Artifact accountability tests**: Matrix (Semantic/Physical state combinations).
- **G. Reconciliation scenario tests**: Frozen scenarios evaluated on actual accepted revisions.
- **H. Crash / partial-state tests**: Verify `last accepted RepositoryRevision remains valid` on interruption.
- **I. End-to-end multi-user tests**: Simulate real divergence/sync across multiple clones.

## Philosophy consistency gates
At major milestones, verify:
1. **Specification-first**: Does physical Git state ever implicitly redefine semantic authority? (NO)
2. **Explicit evolution**: Are semantic changes produced through explicit semantic operations? (YES)
3. **Stable identity + immutable versions**: Does collaboration mutate existing accepted versions? (NO)
4. **Traceability**: Can reconciled history explain origin, conflict, and resolution? (YES)
5. **Semantic accountability**: Can physical changes be detected without pretending they changed semantic meaning? (YES)
6. **No silent knowledge loss**: Does any workflow implement last-writer-wins? (NO)
7. **KAT is not Git**: Can Git branch or HEAD movement define KAT repository authority? (NO)
8. **KAT is not a passive graph**: Does collaboration operate through explicit Changes rather than syncing graph snapshots? (YES)

## Canonical hard gates
Verify code structure boundary ownership:
- `SemanticState` -> no physical state.
- `RepositoryRevision` -> semantic + physical binding.
- `Git backend` -> no semantic decisions.
- `Workspace` -> mutable local state only.
- `NamedReference` -> no history ownership.
- `Conflict` -> no `KnowledgeElement` representation.
- `KAT Hub` -> no semantic reinterpretation.

## Machine-interface compatibility
- Existing command DTOs must be explicitly assessed for compatibility.
- Decide if new commands use `interface_schema_version = 1`.
- Ensure new collaboration fields are added safely as optional.

## Failure and recovery requirements
For every mutating operation, define:
- precondition
- prepared immutable state
- atomic visibility point (e.g. CAS/update local head)
- failure behavior
- recovery behavior

*The atomic boundary should be the mutable reference movement. Everything before it is immutable preparation.*

## Deferred scope
The following are explicitly excluded from v0.5:
- federation
- real-time CRDT collaboration
- GitHub PR integration
- full KAT Hub web UI
- organizations
- fine-grained permissions
- CI orchestration
- large-file/LFS architecture
- cross-repository semantic references
- property-level semantic auto-merge
- LLM-assisted reconciliation
- persistent global aliases
- full branch language

## Freeze criteria
- all v0.4 tests pass
- all new canonical vectors pass
- `fmt` passes
- `clippy` passes with zero warnings
- all collaboration invariants covered
- all reconciliation rules covered
- Artifact physical drift scenarios covered
- Git backend integration scenarios covered
- multi-user scenarios pass
- failure/atomicity scenarios pass
- no canonical hard-gate violation
- no philosophy inconsistency found
- machine interface reviewed
- migration path tested
- clean working tree
- version/package correctly updated
- **every normative collaboration invariant has at least one corresponding test or review gate**
- **every reconciliation rule has an executable scenario**
- **every Phase 0 decision is reflected in implementation or explicitly deferred**
- **normative v0.5 documentation reflects the final implemented semantics with no unresolved implementation/design contradiction**
