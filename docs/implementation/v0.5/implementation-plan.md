# KAT v0.5 Implementation Plan

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
- Initialize managed Git storage.
- Adopt existing Git repository.
- Maintain the backend mapping between `WorkspaceSnapshotId` and the immutable Git commit representing that physical snapshot.
- Generate synthetic physical commits where necessary.
- Materialize a snapshot safely.
- Inspect tracked working state.
- Distinguish ordinary edits from backend mismatch.
- Protect referenced objects from Git GC.
- Support physical ancestry queries.
- Support physical three-way merge primitives.
- Establish hidden/internal ref strategy if required.
- Verify exact snapshot content after materialization.
### Tests
- fresh non-Git project, existing Git project
- tracked modification, tracked addition, tracked deletion
- untracked content, ignored content, empty workspace
- snapshot reuse
- same tree across different Git metadata
- external checkout, external reset, Git HEAD moved independently, detached HEAD
- missing object, GC/reachability
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
- Implement `Workspace`: identity, `base_repository_revision`, semantic draft state, physical backend association.
- Handle behaviors: open workspace, reload workspace, clean workspace.
- Detect semantic-only modified, physical-only modified, combined modified.
- Guarantee base never silently moves.
### Tests
- persistence/restart tests.
- tracked modification detected.
- untracked/ignored file excluded.
- unchanged workspace reuses snapshot.
- direct backend movement classified separately.
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
- `collaboration-workflow.md`
### Traceability
- Adherence to v0.4 porcelain architecture.
### Preconditions
- Phase 9 complete.
### Design constraints
- Conform to existing CLI philosophy.
### Implementation work
- Evaluate and expose: `status`, `commit`, `switch`, `reconcile`.
- Define and validate porcelain contracts for `sync`/`push`.
- Implement local commands fully.
- For every porcelain action, specify: human output, `--json` output, preconditions, failure diagnostics, atomic boundary, interaction with dirty workspace.
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
### Implementation work
- Define exactly what the remote abstraction transports: `RepositoryRevision` objects, semantic objects, shared references / heads, workspace snapshot availability metadata.
- Operations: discover refs/heads, has object, fetch object(s), publish object(s), compare-and-swap ref, verify completeness.
### Tests
- In-memory fake remote testing for all operations.
### Regression checks
- `cargo test --workspace`, `cargo fmt --check`, `cargo clippy ... -D warnings`.
### Failure / recovery cases
- Transport-level failures generically: unavailable transport, partial object availability, duplicate object publication, stale CAS expectation, missing referenced object.
### Acceptance criteria
- Abstraction handles full DAG transport robustly.
### Artifacts produced
- `Remote` interface.
### Out of scope for this phase
- Actual KAT Hub or Git network calls.

## Phase 12: KAT Hub and Git remote integration
### Goal
Implement the actual remote sync via KAT Hub/Git.
### Design inputs
- `remote-model.md`
### Traceability
- Changing Git remote URL does not alter canonical state.
- Shared reference never points to an incomplete `RepositoryRevision`.
### Preconditions
- Phase 11 complete.
### Design constraints
- Remote sync is purely transport and CAS; no semantic reinterpretation.
### Implementation work
- Publish protocol:
  1. Ensure physical snapshot available remotely.
  2. Upload required semantic immutable objects.
  3. Upload `RepositoryRevision`.
  4. Verify completeness.
  5. CAS shared reference/head (the **visibility boundary**).
### Tests
- **Fetch semantics**: fetch missing immutable objects, remote heads, fetch does not move local reference, fetch does not move workspace base, fetch is idempotent, partial fetch failure leaves accepted local state unchanged.
- **Publication failures**: Git upload fails, semantic upload fails, `RepositoryRevision` upload fails, verification fails, CAS loses race, network drops after objects uploaded, retry publication, duplicate upload.
### Regression checks
- `cargo test --workspace`, `cargo fmt --check`, `cargo clippy ... -D warnings`.
### Failure / recovery cases
- Protocol must cleanly fail before CAS if any upload step fails.
### Acceptance criteria
- Multi-user scenarios across the implemented semantic and physical remote transports pass.
### Artifacts produced
- KAT Hub / Git remote implementation.
### Out of scope for this phase
- Auth mechanisms (assumes basic setup for now).

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
