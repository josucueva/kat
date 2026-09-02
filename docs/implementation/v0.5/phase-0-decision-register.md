# KAT v0.5 Phase 0 Decision Register

## Purpose
This document records the exact architectural and canonical representation decisions required by Phase 0 of the v0.5 implementation plan. These decisions resolve implementation details left open by the normative models and establish a strict foundation for Phase 1.

---

## DEC-001 RepositoryRevision canonical structure
**Context**: `repository-revision-model.md` and `canonical-impact-audit.md` require a deterministic canonical representation.
**Decision**: `RepositoryRevision` will be encoded in CBOR using the standard KAT object hashing mechanism. Its canonical fields are:
*   `parents`: Array of `ObjectId` (references to parent `RepositoryRevision`s).
    - 0 for the initial RepositoryRevision
    - 1 for ordinary evolution
    - >1 for reconciliation
    - duplicate parent IDs forbidden
    - deterministic canonical ordering required (parents must be canonically ordered by full `RepositoryRevisionId` so that input order does not choose a winner or alter the resulting revision hash).
*   `semantic_state`: `ObjectId` (reference to `SemanticState`).
*   `workspace_snapshot`: `WorkspaceSnapshotId` (see DEC-002).
*   `semantic_change`: Optional `ObjectId` (reference to `ChangeRevision`).
**Status**: APPROVE WITH CLARIFICATION

## DEC-002 WorkspaceSnapshot identity representation
**Context**: `workspace-model.md` specifies `WorkspaceSnapshotId` must be backend-neutral at the KAT boundary. Identical physical content must resolve to equivalent identity.
**Decision**: `WorkspaceSnapshotId` represents the **KAT deterministic identity of tracked physical content**. 
*   Same physical contents -> same `WorkspaceSnapshotId`.
*   Different Git commits carrying those contents -> allowed, they resolve to the same `WorkspaceSnapshotId`.
The `GitWorkspaceBackend` maintains the internal mapping `WorkspaceSnapshotId -> GitCommitOid`. Git commit identity is backend metadata, isolating KAT from mutable Git author/timestamp metadata.
**Status**: RESOLVED

## DEC-003 MaterializationId representation
**Context**: `artifact-materialization-model.md` requires deterministic physical identity for Artifacts, and backend neutrality.
**Decision**: `MaterializationId` is defined semantically as the **KAT deterministic digest of the normalized resolved materialization**.
*   For a file: `MaterializationId = H(type || bytes)`
*   For a directory: `MaterializationId = H(canonical ordered entries(relative locator, type, materialization identity))`
Git can optimize resolution by exploiting Git blob/tree identities internally, but the identity semantic avoids tying Artifact accountability to Git forever.
**Status**: RESOLVED

## DEC-004 Artifact physical baseline decision
**Context**: `canonical-impact-audit.md` outlines two choices for storing an Artifact's physical baseline: explicit `MaterializationId` or a referenced `RepositoryRevision`.
**Decision**: Artifact physical baselines will store the **explicit `MaterializationId`**. 
The baseline directly records the exact physical materialization that was reviewed, while remaining independent of the `RepositoryRevision` used to observe it. This supports natural working-state comparisons (e.g., accounted M17 vs working M18 -> MODIFIED).
**Status**: RESOLVED

## DEC-005 ChangeRevision reconciliation decision
**Context**: `canonical-impact-audit.md` questions whether `ChangeRevision` needs modification for multi-parent reconciliation.
**Decision**: The existing `ChangeRevision` model already supports plural `base_states[]`. This is structurally sufficient.
*   **Rule**: A multi-parent `RepositoryRevision` does not necessarily imply a new `ChangeRevision`. If reconciliation is physical-only and semantic state remains unchanged, `RC.semantic_change = none`.
**Status**: APPROVE WITH CLARIFICATION

## DEC-006 Deterministic merge-base policy
**Context**: `reconciliation-model.md` requires deterministic common ancestor discovery.
**Decision**: KAT will use a conservative v0.5 merge-base policy:
*   Find the set of best common ancestors of all reconciliation heads.
*   If exactly one exists: use it.
*   If multiple incomparable best common ancestors exist: reconciliation is not automatically attempted in v0.5; return an explicit ambiguous-merge-base condition.
*   KAT MUST NOT choose a merge base based solely on ObjectId ordering.
**Status**: RESOLVED

## DEC-007 Git repository layout decision
**Context**: `git-workspace-backend.md` suggests an internal `.kat/` Git repository to avoid dual-authority confusion.
**Decision**: KAT will use a managed internal Git repository located at `.kat/physical/git/`. The project root acts as the physical working tree.
**Status**: RESOLVED

### DEC-007A Existing Git adoption
**Context**: The implementation plan calls for adopting an existing Git repository.
**Decision**: Whether to continue using `.git/`, import/copy its state into `.kat/physical/git/`, or convert `.git/` into the managed internal repository is deferred.
**Status**: DEFERRED UNTIL PHASE 3 (must be resolved before Git backend implementation).

## DEC-008 Conflict persistence decision
**Context**: `conflict-model.md` and `canonical-impact-audit.md` dictate that conflicts are not canonical history.
**Decision**: `ReconciliationCandidate` (and its enclosed Semantic and Materialization conflicts) will be persisted locally in `.kat/workspace/reconciliation_candidate.json` (or equivalent structured storage).
*   **Explicit requirement**: The persisted representation is local implementation state and is not canonical KAT serialization. Field ordering, JSON encoding, etc., must not influence any `ObjectId`.
**Status**: RESOLVED

## DEC-009 Interface schema compatibility decision
**Context**: `canonical-impact-audit.md` requires assessing existing command DTOs.
**Decision**: New collaboration commands and extensions (like `status`) will use `interface_schema_version = 1`. New physical drift and conflict tracking fields will be added as optional fields.
*   **Phase 10 compatibility test requirement**: Existing v0.4 machine consumers/fixtures must continue to parse the extended responses according to the documented compatibility contract. If not, schema version 2 may be required.
**Status**: RESOLVED CONDITIONALLY
