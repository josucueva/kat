# Phase 12 Transport and Publication Design

## Status and scope

Status: Draft for Phase 12 implementation review; no concrete transport code
has been started.

This design defines the first concrete implementation of the frozen Phase 11
`SemanticTransport`, `WorkspaceTransport`, and `RemoteCoordinator` contracts.
It preserves KAT's authority model and the separate semantic and physical
transports. It does not define Hub endpoint paths, a wire schema, credentials,
or CLI syntax.

Phase 12 uses a KAT Hub semantic service and a Git smart-protocol remote for
physical workspace content. One logical remote binds those two locations.
The current repository has the transport client interface but no KAT Hub
service target; Phase 12 implementation must integrate an existing Hub service
or add that service as a separately deployable component.

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
   A ref is create-only and immutable. A conflicting target is an integrity
   error.
5. Upload physical snapshot refs first, semantic dependencies second, and
   update one KAT named reference last with linearizable compare-and-swap.
6. Phase 12 remote semantic storage is append-only. Git garbage collection may
   run normally because KAT lineage refs are persistent roots. Phase 12 does
   not delete KAT lineage refs or collect KAT semantic objects.
7. The Hub's discovery and CAS implementation is bound to the associated
   workspace remote so it can verify physical snapshot availability. The
   client coordinator independently verifies the full closure before changing
   local tracking refs or requesting publication.

These decisions fill in transport details left open by
[`remote-model.md`](remote-model.md) and
[`git-workspace-backend.md`](git-workspace-backend.md); they do not change the
Phase 11 trait signatures or canonical formats.

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
availability at CAS time. This is safe across the two hosts because published
physical snapshots are protected by immutable Git refs and Phase 12 does not
delete those refs. The Hub's reference read/list operation must provide a
fresh, coherent view suitable for resolving an ambiguous CAS response.

### Workspace transport: Git smart protocol

`GitWorkspaceBackend` continues to use the internal bare repository at
`.kat/physical/git/`. The workspace transport uses the configured Git remote
with Git's fetch/push protocol, restricted to KAT lineage refs. It does not
push or fetch ordinary branches as KAT state.

For each `WorkspaceSnapshotId`, local `.kat/physical/snapshots.json` supplies
the known commit representations. Before publishing, the backend verifies
each commit exists and that hashing its tree with KAT's workspace identity
rules yields the requested snapshot ID. It then pushes the corresponding
create-only lineage ref. Git transfers the commit, its parent commits, trees,
and blobs. After the push, the transport confirms that each remote ref names
the expected commit.

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

A logical remote pairs one semantic Hub location with one physical Git
location. That pairing is local, noncanonical configuration; changing either
location does not change `RepositoryId`, object IDs, KAT refs, or workspace
bases. Credentials are not stored in canonical objects or repository identity
metadata. Exact configuration syntax is part of transport implementation,
not this design.

The Hub's service configuration must associate each hosted `RepositoryId` with
its workspace remote so discovery and CAS can verify snapshots. A Hub deployment
that cannot reach or inspect its paired Git remote cannot claim closure-complete
unnamed-tip discovery or safe reference publication.
Phase 12's basic service setup must provide that read access; account and
authorization design remains out of scope.

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
   semantic closure; confirm every physical snapshot remains available.
5. Ask the Hub to CAS the one named KAT reference from `expected` to the
   candidate. The Hub rechecks closure completeness and atomically records the
   reference as a retention root.

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
`gc` may run concurrently with fetch or publication; it must preserve the
referenced commit and its complete object ancestry. The KAT transport never
deletes lineage refs. The Hub and the Git remote must reserve the KAT lineage
namespace from ordinary branch updates and ref deletion.

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

Partial immutable transfers are not rolled back. Partial transfer is safe
because objects and lineage refs are immutable, idempotent, and not KAT named
references.

## Phase 12 acceptance criteria

- A local bare Git remote receives and fetches snapshot mappings using only
  `refs/kat/lineage/**`; branch movement has no effect on KAT references.
- A supported Git host must advertise and retain the reserved KAT lineage ref
  namespace; compatibility is verified per host before that host is claimed.
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
