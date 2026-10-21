# Phase 12 Transport and Publication Design

## Status and scope

Status: Draft after focused architectural review; no concrete transport code
has been started. The retention contract and repository binding are specified
below, but Phase 12.0 is not frozen until the deployment backend passes the
operational acceptance gates.

This design defines the first concrete implementation of the frozen Phase 11
`SemanticTransport`, `WorkspaceTransport`, and `RemoteCoordinator` contracts.
It preserves KAT's authority model and the separate semantic and physical
transports. It does not define Hub endpoint paths, a wire schema, credentials,
or CLI syntax.

Phase 12 uses a KAT Hub semantic service and a Git smart-protocol remote for
physical workspace content. One repository-level binding pairs those two
services. Phase 12 v1 supports only a KAT-controlled Git backend whose ref
retention policy the Hub can enforce and verify. A generic Git URL or a
successful push is not sufficient. The current repository has no KAT Hub or
workspace remote configured, so this policy has not yet been exercised against
a deployment.

## Design decisions

1. KAT Hub is authoritative for KAT named references and stores canonical
   objects as immutable bytes, scoped by `RepositoryId` and addressed by
   `ObjectId`.
2. The Git remote is authoritative only for transport availability of physical
   snapshots. Git branches and `HEAD` never define KAT revisions or references.
3. A workspace snapshot can have multiple Git commit representations because
   `WorkspaceSnapshotId` hashes the tree contents, not a Git commit. Transfer
   and retain every verified local representation of a published snapshot.
4. Reuse the existing KAT snapshot-protection ref shape:
   `refs/kat/lineage/<WorkspaceSnapshotId-hex>/<Git-commit-OID>`.
   A ref is create-only and immutable. The configured Git server must enforce
   rejection of updates and deletion; a conflicting target is an integrity
   error.
5. Upload physical snapshot refs first, semantic dependencies second, and
   update one KAT named reference last with linearizable compare-and-swap.
6. Phase 12 remote semantic storage is append-only. The configured Git server
   retains lineage refs and runs Git garbage collection with those refs as
   reachability roots. Phase 12 does not delete KAT lineage refs or collect KAT
   semantic objects.
7. Each Hub repository has a fixed workspace-backend binding. Hub discovery and
   CAS inspect that bound backend; a publication request cannot select another
   Git destination. The client coordinator independently verifies the full
   closure before changing local tracking refs or requesting publication.

These decisions fill in transport details left open by
[`remote-model.md`](remote-model.md) and
[`git-workspace-backend.md`](git-workspace-backend.md); they do not change the
Phase 11 trait signatures or canonical formats.

## Durable snapshot guarantee

A successful push and remote ref read-back prove only that a ref currently
points to a commit. They do not prove the ref cannot later be deleted, or that
the Git host will retain its objects. Publication therefore requires all of
the following at the bound workspace backend:

1. The Hub provisions and controls the backend for Phase 12 v1. A stable
   `WorkspaceBackendId` in the Hub's repository configuration identifies the
   Git repository and the retention policy applied to it.
2. The server accepts only create-if-absent writes to
   `refs/kat/lineage/**`. Its server-side policy rejects updates and deletions
   of those refs from both ordinary users and the transport identity. Ref
   creation is acknowledged only after the commit, tree, and blob objects and
   the ref update have been durably stored under the backend's stated storage
   policy.
3. Before CAS, the Hub reads the exact ref back from that configured backend,
   fetches or otherwise inspects the target commit tree, and recomputes the
   `WorkspaceSnapshotId` using KAT's identity rules. A ref-name match or
   successful push alone is not verification.
4. The Hub checks its repository configuration and the KAT backend's
   enforcement status when it publishes the named reference. The Hub's
   deployment control plane provisions the lineage-ref policy and reports its
   active policy version to the publication service. If the Hub cannot inspect
   the bound backend or cannot establish that the policy is enforced, it must
   reject publication before CAS.
5. While a KAT named reference can reach a snapshot, the backend does not remove
   its lineage ref. Ordinary Git garbage collection may run because Git keeps
   objects reachable from refs in `refs/*`; no KAT cleanup job may remove the
   ref. Storage loss or privileged administrative deletion is an operational
   failure, not a consequence of normal Git GC.

The KAT Hub's server-side repository record contains the immutable pairing:

```text
RepositoryId
semantic object/reference namespace
WorkspaceBackendId
workspace Git repository and retention policy
```

The local client selects a logical Hub remote, not a physical Git destination
per publication. The Hub uses its configured backend binding for discovery and
CAS, and the publication request has no destination override. The Hub's
independent verification against its configured backend fails if any required
snapshot is absent there, leaving the named reference unchanged. Once a shared
reference exists, the backend binding and retention policy cannot be edited in
place. Changing them requires an explicit migration that copies and verifies
all reachable snapshots before cutover; Phase 12 v1 does not implement
rebinding.

This guarantee relies on the KAT deployment controlling the Git server policy
and on its storage durability configuration. Git protocol alone cannot enforce
that policy against a host administrator. External providers are not Phase 12
v1 targets unless a provider-specific integration proves equivalent create-only
retention and lets the Hub verify that enforcement.

The [GitHub reference API](https://docs.github.com/en/rest/git/refs) documents
creation of fully qualified refs, while GitHub's
[ruleset documentation](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets)
describes creation, update, and deletion controls for branches and tags. Those
documents do not establish an immutable retention guarantee for KAT's custom
`refs/kat/*` namespace. The [Git `gc` manual](https://git-scm.com/docs/git-gc)
documents that refs keep their reachable objects from ordinary garbage
collection; it does not prevent a host from deleting a ref.

## Existing backend review findings

`GitWorkspaceBackend::create_snapshot` rereads the commit tree and verifies its
KAT snapshot identity before recording the representation. However,
`record_snapshot_representation` ignores failure to open the internal Git repo
and ignores failure to create the local lineage ref, then returns success.
`verify_snapshot_integrity` validates commits listed in `snapshots.json` but
does not require a corresponding protection ref. The happy-path reachability
test does not cover these failures.

Phase 12 implementation must not treat the mapping file or a successful local
snapshot check as proof that a commit is GC-protected. Before remote publication
is implemented, local representation recording must fail closed when it cannot
create the protection ref, and the publish path must verify the local commit
and derived snapshot identity before transfer. Add a regression test that
injects local ref-creation failure. This is an implementation prerequisite; it
does not require changing the frozen Phase 11 transport interface.

## Concrete transport responsibilities

### Semantic transport: KAT Hub

The Hub stores exact canonical object bytes under the repository and object
identities. A write is create-if-absent: the Hub checks that the bytes hash to
the supplied `ObjectId`, accepts an identical duplicate, and rejects different
bytes for an existing identity. Objects are not re-encoded during transfer.

The Hub stores named KAT references as `RepositoryRevisionId` targets. Listing
and compare-and-swap operate on one repository namespace. Name validation,
repository identity checks, and the expected-target comparison happen at the
service boundary. The CAS operation is linearizable and is the only operation
that makes a candidate revision visible through a shared KAT reference.

Discovery returns named references and derived unnamed DAG tips. The Hub
derives candidate tips from its repository revision inventory, verifies their
recursive semantic closure, and checks every referenced workspace snapshot
against the paired Git remote. It omits incomplete unnamed candidates. A named
reference with missing or invalid closure is an integrity error; it is never
silently hidden. The client coordinator still verifies every advertised
closure independently.

The Hub must confirm a candidate's semantic closure and physical snapshot
availability at CAS time, using the fixed repository-to-backend binding and
the active retention policy. The Hub's reference read/list operation must
provide a fresh, coherent view suitable for resolving an ambiguous CAS
response.

### Workspace transport: Git smart protocol

`GitWorkspaceBackend` continues to use the internal bare repository at
`.kat/physical/git/`. The workspace transport uses the configured Git remote
with Git's fetch/push protocol, restricted to KAT lineage refs. It does not
push or fetch ordinary branches as KAT state.

For each `WorkspaceSnapshotId`, local `.kat/physical/snapshots.json` supplies
candidate commit representations. Before publishing, the backend verifies
each commit exists, its local protection ref exists, and hashing its tree with
KAT's workspace identity rules yields the requested snapshot ID. It then
creates the corresponding ref on the configured KAT-controlled server. Git
transfers the commit, its parent commits, trees, and blobs. The Hub reads the
remote ref back and verifies the target tree before allowing CAS; this is a
content check, while the server policy above provides retention.

For fetch, the transport fetches all remote lineage refs for the requested
snapshot ID into the same local namespace. It verifies each fetched commit's
tree-derived snapshot ID, writes the local representation mapping, and creates
local KAT protection refs. These steps store immutable backend data only:
they do not checkout a commit, update the active lineage, materialize files,
or change a workspace base.

The local mapping file is a cache, not remote authority. On fetch, the Git refs
are the transfer record and the mapping is rebuilt from validated ref names and
commit targets. Mapping updates and protection-ref creation must be checked and
durable before `local_snapshot_available` reports success.

### Logical remote binding

A logical remote identifies a KAT Hub repository. The Hub's noncanonical
repository configuration binds that `RepositoryId` to one `WorkspaceBackendId`
and one Git repository with the required retention policy. Client-local config
may name the logical remote and credentials, but it cannot override the
physical destination for an individual publication. Changing the backend
binding does not change `RepositoryId`, object IDs, KAT refs, or workspace
bases; it requires an explicit migration, deferred beyond Phase 12 v1.
Credentials are not stored in canonical objects or repository identity
metadata. Exact configuration syntax is part of transport implementation,
not this design.

A Hub deployment that cannot reach and inspect its configured backend, or
cannot verify that its retention policy is active, cannot advertise
closure-complete unnamed tips or accept a reference publication. The Hub
repository record and backend policy are checked by the service at CAS, not
selected by the publication caller. Account and authorization design remains
out of scope.

## Operation sequences

### Fetch

1. The coordinator asks the Hub for a coherent reference/tip view.
2. Local tracking names and namespace conflicts are preflighted before
   transferring content.
3. Canonical semantic bytes are fetched by `ObjectId`; Git snapshot refs and
   objects are fetched by `WorkspaceSnapshotId`.
4. The coordinator verifies hashes, canonical decoding, object kinds,
   revision ancestry, semantic change/result-state consistency, and every
   physical snapshot.
5. Only after all required closures pass does it CAS-update remote-tracking
   refs independently, returning the Phase 11 `FetchOutcome` for any partial
   tracking update.

Fetch never moves a local named KAT reference, accepted state, workspace base,
active lineage, or working tree. Immutable objects fetched before a later
failure may remain locally available for retry.

### Publish / push

The caller selects a complete local `RepositoryRevision`, a KAT remote
reference name, and the previously observed remote target (`expected`).

1. Verify the complete source closure and every local physical snapshot.
2. Publish and verify each physical snapshot's KAT lineage refs first.
3. Store semantic dependencies, then `RepositoryRevision` parents before
   children. All object writes are idempotent.
4. Read the semantic destination back and independently verify the complete
   semantic closure. The Hub separately rechecks the exact physical refs and
   trees on the backend bound to this `RepositoryId`, and verifies that its
   create-only retention policy is active.
5. Ask the Hub to CAS the one named KAT reference from `expected` to the
   candidate. The Hub rechecks closure completeness and the backend binding
   before atomically recording the reference as a retention root.

The Git refs do not move the KAT shared reference. The Hub CAS does not move a
local reference or workspace. A stale expected target fails without automatic
overwrite; the caller fetches and decides what to publish next.

### Synchronize

Synchronization composes fetch with the existing repository topology queries
to report whether local history is current, ahead, behind, or diverged from a
remote-tracking reference. It does not automatically publish, reconcile,
advance a local named reference, or materialize a workspace. Those remain
explicit repository operations.

## Retention and garbage collection

The initial Hub object store is append-only: it does not delete KAT semantic
objects. This guarantees that a concurrent semantic collector cannot remove
objects between destination verification and reference CAS. The cost is that
objects left by failed or stale publications remain stored.

On the Git side, each KAT lineage ref is a normal Git reachability root. Git
`gc` may run concurrently with fetch or publication; it preserves the
referenced commit and its complete object ancestry while that ref exists. The
KAT-controlled server must reject ref updates and deletion, and the Hub must
verify the binding and active policy at CAS. The KAT transport never deletes
lineage refs. This guarantee does not extend to arbitrary third-party hosts or
privileged administrative deletion unless their service policy establishes
equivalent controls.

Phase 12 has no KAT-aware object or ref pruning. Any later reclamation feature
must coordinate collection with named-reference CAS: a collector may remove
content only after proving it is unreachable from every live KAT named ref and
every in-flight publication. It must not be added as an opportunistic Git or
Hub cleanup job that can race publication.

## Failure and recovery behavior

| Failure | Required result | Recovery |
| --- | --- | --- |
| Git upload stops partway | No Hub named ref moves; successfully pushed immutable lineage refs remain | Retry; verified refs are idempotent |
| Semantic object upload stops partway | No Hub named ref moves; stored canonical objects remain | Retry the same candidate |
| Destination closure is missing or invalid | No named ref moves; return an integrity error | Repair or re-transfer the missing immutable content, then retry |
| Snapshot ref exists but its commit tree hashes to another snapshot ID | Reject it; do not update local tracking refs or publish a KAT ref | Treat as remote integrity failure |
| Expected KAT ref target is stale | CAS fails; competing target is unchanged | Fetch, inspect divergence, then explicitly retry with a new expected value |
| CAS response is lost | Do not assume either success or failure | Read the named ref: candidate means success, old expected value permits retry, any other target is a conflict |
| Fetch fails after immutable transfer | Local refs and workspace authority stay unchanged; transferred objects may remain | Retry fetch |
| One local tracking-ref CAS fails after siblings succeeded | Successful siblings remain; failed ref is reported | Inspect `FetchOutcome.failed`, then retry that ref |
| Git remote is unavailable or deletes a protected KAT ref | No local authority moves; closure verification fails | Restore availability/ref or select a valid remote |
| Backend binding is missing, changed, or retention policy is not verifiably active | Hub CAS is rejected; named reference is unchanged | Restore the configured policy or complete an explicit backend migration |

Partial immutable transfers are not rolled back. Partial transfer is safe
because objects and lineage refs are immutable, idempotent, and not KAT named
references.

## Phase 12 acceptance criteria

- A local bare Git remote receives and fetches snapshot mappings using only
  `refs/kat/lineage/**`; branch movement has no effect on KAT references.
- Phase 12 v1 uses a KAT-controlled Git backend whose server rejects lineage
  ref update/deletion and whose Git GC preserves reachable objects.
- A remote ref is read back from the bound backend and its tree-derived
  `WorkspaceSnapshotId` is independently recomputed before CAS.
- The Hub rejects publication when the `RepositoryId` has no fixed workspace
  backend binding or the deployment control plane cannot verify the backend
  retention policy as active.
- Host-specific support is claimed only after proving custom ref advertisement,
  read-back, create-only policy enforcement, and GC retention on that host.
- Multiple Git commit representations of one workspace identity retain and
  round-trip; a mismatched tree is rejected.
- Fetch does not materialize content or change workspace base, active lineage,
  accepted state, or local named references.
- The Hub transport preserves exact canonical bytes, enforces immutable object
  IDs, validates repository identity, and implements linearizable per-ref CAS.
- Discovery omits incomplete unnamed tips and reports integrity failure for an
  incomplete named reference; the coordinator independently rechecks closures.
- Publication uploads physical refs before semantic revisions and advances the
  KAT named reference only after destination closure verification.
- Concurrent Git `gc` cannot remove a snapshot protected by a KAT lineage ref;
  Hub semantic objects remain present under the append-only Phase 12 policy.
- A local representation/ref-creation failure fails before publication; the
  regression suite covers the current `GitWorkspaceBackend` failure path.
- Interrupted uploads, CAS races, lost CAS responses, retries, duplicate
  transfers, and partial fetch tracking updates match the recovery table.
- Failed publication and fetch preserve local accepted state, named refs,
  workspace base, active lineage, and working tree.

## Explicitly deferred

- Hub HTTP endpoint paths and wire schema;
- authentication, authorization, and account management;
- CLI syntax and presentation for remote configuration, `fetch`, `push`, and
  `sync`;
- KAT-aware deletion, compaction, and garbage collection;
- Git-provider-specific administration or hosting APIs.
- Backend rebinding and migration; Phase 12 v1 rejects silent changes.
