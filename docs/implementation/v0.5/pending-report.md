# v0.5 Implementation Pending Report

This report outlines the remaining phases required to complete the v0.5 implementation plan. **Phases 10 and 11 are frozen. The Phase 12.0 transport contract is frozen, and the local Phase 12.3 Hub semantic core is implemented. Network access and deployment acceptance remain pending.**

## What Has Been Completed

All core reconciliation, conflict resolution, topology traversal, and collaboration CLI workflows have been successfully implemented and tested. This includes:
- Phase 1: Architectural foundations
- Phase 2: Domain logic core
- Phase 3: Topology models
- Phase 4: `WorkspaceBackend` Git trait mapping
- Phase 5: Re-accountability logic
- Phase 6: Core Validation
- Phase 7: Topology implementation
- Phase 8: Semantic reconciliation
- Phase 9: Physical reconciliation and conflicts
- Phase 10.1–10.9: Core collaboration porcelain (`kat reconcile`, `kat resolve`, `kat switch`, `kat advance`, `kat finalize`, `kat abort`, `kat conflicts`, `kat materialize`)
- Phase 10.10: JSON machine interfaces for the collaboration commands, with v1 envelope contract and MI-COL checks.
- Phase 10.11: Ten local collaboration E2E scenarios through separate `kat` processes, covering clean and conflicted merges, semantic resolution without materialization, physical edits, abort, advance, switch, restart, and NamedReference preservation. The version-1 session format gained an optional clean physical snapshot ID; older sessions recover it from immutable inputs only. Candidate state and Git protection refs are checked across restart and after cleanup.
- Phase 11.1: Frozen transport abstraction with semantic/workspace separation, recursive closure verification, parent-first uploads, closure-complete unnamed-tip discovery, per-reference tracking CAS outcomes, and a documented retention contract. Reported verification: 631 workspace tests and 16 remote tests pass.
- Phase 12.3 (`8a02b56`): Added `TrustedHubSemanticTransport`, a file-backed local Hub core. It stores exact canonical object bytes by repository and object ID, persists KAT named refs with per-name CAS, fixes each repository to one controlled Git backend, and checks policy, semantic closure, and snapshot trees before CAS. Discovery advertises only verified unnamed tips and fails closed for invalid named refs. Five Hub acceptance tests cover interrupted upload/retry, missing dependencies, backend/policy drift, concurrent CAS, reopen, and coordinator publish/fetch. Full gate: 649 tests plus build, formatting, and Clippy passed.

## What Remains Pending

The remaining effort focuses on concrete remote integration, compatibility migration, and distributed end-to-end verification.

### Phase 11: Remote abstraction — Complete

Step 11.1 satisfies the Phase 11 acceptance criteria. The implementation plan contains no separate 11.2 repository workflow step; actual remote-backed `sync`/`push` is assigned to Phase 12. No additional wrapper around `RemoteCoordinator` is planned because it already coordinates the existing local stores and injected transports.

### Phase 12: KAT Hub and Git remote integration
**Goal:** Implement the actual remote sync via KAT Hub/Git.
- Design: [`phase12-transport-design.md`](phase12-transport-design.md), drafted in `6f03dd3` and reviewed/amended in `0ee8b36`, defines canonical-byte transfer, Git lineage refs, publication ordering, repository binding, and recovery. The transport contract is frozen independently of deployment proof. The retention guarantee requires a KAT-controlled Git backend with server-enforced create-only lineage refs, remote read-back/tree verification, and Hub-side verification of the bound backend before CAS.
- Phase 12.1 is complete in `2989582`: local snapshot recording requires the embedded Git repo and a verified direct protection ref, mapping updates are atomic, and lookups reject missing or mismatched protection refs. Missing-repository, conflicting/missing-ref, and recovery cases pass; the full build, test, formatting, and Clippy gate passed.
- Phase 12.2.1 is complete in `facf800` and tracked in `32d1de8`: the controlled bare backend binds one repository and backend ID, and its receive hook allows lineage-ref creation while rejecting updates, deletions, and non-lineage writes. Loopback tests cover binding mismatch, policy drift, and client mutation rejection.
- Phase 12.2.2 is complete in `1f5d8e2`: the concrete Git `WorkspaceTransport` saves one immutable destination, verifies the remote's advertised repository/backend binding marker, transfers only lineage refs, and recomputes `WorkspaceSnapshotId` from trees read back into a fresh repository. Seven loopback integration tests pass, including wrong binding/tree rejection and fetch without materialization. The full build, test, formatting, and Clippy gate passed.
- Phase 12.2.3 is complete locally in `517e102`: a failed second representation leaves the first immutable ref intact and retry succeeds; concurrent identical publications are idempotent; aggressive GC during a paused receive rejects the in-flight upload without creating a ref; retry then succeeds, and a published snapshot remains fetchable after `git gc --prune=now`. Invalid fetched trees leave no local representation or staging ref. The full build, test, formatting, and Clippy gate passed.
- Phase 12.3 is complete locally in `8a02b56`: the trusted Hub core implements `SemanticTransport`, immutable canonical storage, repository-scoped named-ref CAS, closure-verified discovery, fixed Git backend binding, and policy/closure/tree checks before CAS. Five Hub tests pass. Its `RemoteCoordinator` publish/fetch test uses a test `WorkspaceTransport`; the Hub independently verifies a real controlled bare Git repository. Full gate: 649 tests, build, fmt, and Clippy pass.
- The deployment trust boundary covers ordinary Git clients and supported maintenance operations. Unrestricted host administrators and underlying storage loss are trusted deployment risks; the service must state its durability and recovery assumptions.
- GitHub's fully qualified ref API and documented branch/tag rulesets do not prove retention for `refs/kat/*`. This repository's `origin` is only its project-source remote, not evidence of a configured KAT workspace host.
- Phase 12.2 local implementation is complete. Its loopback backend and transport are not deployment proof. Phase 12.3 verifies the controlled backend's local policy files/configuration and reads snapshot refs and trees before CAS, but this does not verify production service startup, host controls, backups, or storage durability.
- The Hub core is trusted in-process only. It has no network listener, endpoint schema, authentication, or CLI wiring. The Hub data root must be protected by deployment operating-system permissions; the library does not configure ACLs. Do not expose this core to untrusted processes.
- Remaining remote integration: add the authenticated service adapter/client or an operator-managed single-process deployment, exercise the Hub together with `GitWorkspaceTransport`, and wire remote CLI workflows.
- **Tests**: Fetch semantics, publication failures and race conditions, network drop retries.

### Phase 13: Migration and compatibility
**Goal:** Migrate v0.4 repositories to v0.5 seamlessly.
- Preserve every existing canonical object from v0.4 without mutation.
- Detect v0.4 repositories and transition to v0.5.
- Create first `RepositoryRevision` wrapper and initialize local workspace base.
- **Tests**: v0.4 repo without Git, v0.4 repo inside Git, clean/dirty physical workspace, interrupted/repeated migration.

### Phase 14: End-to-end evaluation
**Goal:** Validate full workflow between multiple users.
- Execute the distributed empirical scenario matrix, beyond the local Phase 10 E2E-01 through E2E-10 workflows.
- Includes scenarios for semantic conflicts, physical merge conflicts, combined evolution, publication races, and external Git movements.
- **Tests**: All E2E scenarios pass matching expected state exactly.

## Next Steps

Next, add an authenticated service adapter/client if Hub access crosses a process boundary, exercise the Hub together with `GitWorkspaceTransport`, and wire remote CLI workflows. Verify retention, access controls, and storage durability against the actual deployment; the local loopback tests are not production proof.
