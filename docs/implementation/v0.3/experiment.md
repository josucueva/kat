# EXPERIMENT.md — KAT-Driven Development Retrospective

This document is the project retrospective for the **Task Management API** developed with
KAT (Knowledge Architecture Toolkit) as the repository for authoritative software
knowledge. Everything reported below is based on what was actually observed during the
development session, with the KAT repository (`.kat/`) as the source of evidence.

---

## 1. System Summary

The implemented system is a small **Task Management REST API** for personal projects,
written in **Node.js (v22) with Express 4**.

Functional surface:

- **Projects**: create, list, get, delete.
- **Tasks**: create inside a project, list (with filtering), get, update (title,
  description, priority, status), delete.
- **Task model**: `title`, `description`, `priority` (`low`/`medium`/`high`), `status`
  (`open`/`completed`), timestamps, and an owning `projectId`.
- **Filtering**: `GET /projects/:projectId/tasks?status=open|completed&priority=low|medium|high`
  (combinable; invalid values → `400`).
- **Completion/reopening**: status transitions via `PATCH /tasks/:id`.
- **Deletion protection**: deleting a project that still has tasks → `409 Conflict`;
  the store refuses the deletion and the project stays intact.
- **Persistence**: a single JSON file (`data/tasks.json`, overridable via `DATA_FILE`)
  written atomically (temp file + rename). Data survives restarts (verified end-to-end).

Architectural choices that matter:

- **Store abstraction**: `MemoryStore` and `JsonStore` implement one interface, and the
  Express app is built by `createApp(store)` — so routes are independent of the storage
  mechanism, and tests run against in-memory storage while production uses JSON files.
- **Lightweight by constraint**: no database server, no ORM, no message bus — one
  dependency (`express`); tests use Node's built-in `node:test` runner and `fetch`.

Verification performed:

- `npm test` → **18/18 tests pass**.
- End-to-end smoke test against the real server: created a project and tasks, filtered by
  priority, verified `409` on protected deletion, restarted the server, and confirmed the
  data was still present.

---

## 2. KAT Repository Summary

Final accepted state (as reported by `kat status`, `kat list`, `kat validate`, `kat artifacts`):

**Knowledge elements: 40** (all `active`)

| Type              | Count | Examples                                                                                                                              |
| ----------------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `intent`          | 1     | "Provide a lightweight personal Task Management API"                                                                                  |
| `requirement`     | 12    | REST API, persistence, project/task CRUD, filtering, deletion protection, tests, simplicity                                           |
| `constraint`      | 5     | priority enum, reopen allowed, protected deletion, one project per task, no unnecessary infra                                         |
| `design-decision` | 7     | Node+Express, JSON-file persistence, store abstraction, explicit status model, query-param filtering, PATCH updates, HTTP error codes |
| `implementation`  | 4     | Express routes, JSON-file store, request validation, entry point                                                                      |
| `artifact`        | 6     | `src/app.js`, `src/store.js`, `src/server.js`, `test/api.test.js`, `package.json`, `README.md`                                        |
| `validation`      | 5     | test suite passes, persistence verified, priority/status transitions, deletion protection, filtering                                  |

**Relationships: 84** across the ontology types `motivates`, `addresses`, `restricts`,
`guides`, `realizes`, `represents`, `derived-from`, `validates`.

**Important Design Decisions recorded:**

1. Use Node.js with the Express web framework.
2. Persist data to a JSON file with atomic writes.
3. Separate persistence behind a store abstraction.
4. Model task status as explicit `open`/`completed` states (reopen = supported transition).
5. Filter tasks through query parameters on the list endpoint.
6. Use PATCH for partial task updates.
7. Use conventional HTTP status codes for errors (`400`/`404`/`409`).

**Important semantic relationships established** (canonical directions):

- `intent → motivates → requirement` (12 links).
- `design-decision → addresses → requirement`.
- `constraint → restricts → requirement | design-decision | implementation`.
- `design-decision → guides → implementation`.
- `implementation → realizes → requirement`.
- `artifact → represents → implementation`; `artifact → derived-from → requirement|constraint|design-decision|implementation`.
- `validation → validates → constraint | implementation | requirement`.

**Accepted Changes: 7 revisions**, consisting of 4 semantic changes and 3 artifact
reconciliation revisions:

| Revision       | Description                                                                          |
| -------------- | ------------------------------------------------------------------------------------ |
| `6a48e7d8c934` | Initial semantic model (52 ops) — intent, requirements, constraints, DDs, links      |
| `378b4eeee34c` | Record core implementation (64 ops) — implementations, artifacts, validations, links |
| `d659934691c9` | Evolve deletion-protection semantics (9 ops) — HTTP 409 refinement                   |
| `a645a6d558b3` | `account` artifact — `src/app.js` reconciled                                         |
| `6c0ec2e0e58b` | `account` artifact — `src/store.js` reconciled                                       |
| `555ed7d5f18b` | Evolve task-filtering semantics (6 ops) — concrete query parameters                  |
| `f01f24511f20` | `account` artifact — `src/app.js` reconciled                                         |

**Final validation result:** `kat validate` → **0 violations, 5 unverified constraints**
(the 5 unverified constraints are those with "no executable validation rule" — e.g. the
priority enum and deletion protection. These are enforced by implementation and validated
by tests rather than by a KAT-executable rule; this is informational, not a defect).

**Final artifact-accountability state:** `kat artifacts` → **6 current, 0 stale, 0 unaccounted**.

**No unfinished draft Change:** `kat change status` → "no open draft change transaction".

---

## 3. Traceability Questions

### Q1. Why does the task-completion implementation exist?

**KAT query:** `kat trace 56adfdb8-6539-46e7-851b-8e8b86f11e06`

The task-completion behavior is implemented by the routes implementation
**"Express REST API routes for projects and tasks"** (`56adfdb8`). The trace shows the
semantic path that explains its existence (Path 2 in the trace output):

```
56adfdb8  [implementation] Express REST API routes for projects and tasks
   ← kat.core/guides ←
915c96d4  [design-decision] "Model task status as explicit open and completed states"
   → kat.core/addresses →
6f63af13  [requirement] "Support marking tasks completed and reopening them"
   ← kat.core/motivates ←
d224a20d  [intent] "Provide a lightweight personal Task Management API"
```

The requirement is further constrained by **"A completed task may be reopened"**
(`7d88019a`): `6f63af13 ← kat.core/restricts ← 7d88019a`. In short: the completion
implementation exists because the Intent (a lightweight personal task manager) motivates
the requirement to mark tasks completed and reopen them, the Design Decision to model
status as explicit `open`/`completed` states addresses that requirement, and that Decision
guides the routes implementation; the reopen constraint restricts the same requirement.

### Q2. What depends on the requirement that a project with existing tasks cannot be deleted?

**KAT query:** `kat impact c4039918-ae9d-4d2d-9c2b-28f562b9cbbd --compact`

Final impact analysis (7 revisions in) reports:

- **Directly relevant:** `c4039918` [requirement] "Reject deletion of a project that still has tasks".
- **Semantically affected elements:**
  - `56adfdb8` [implementation] "Express REST API routes for projects and tasks" (via `realizes`, `addresses`, `guides`).
  - `7e8b5bf3` [design-decision] "Use conventional HTTP status codes for errors" (via `addresses`).
  - `440abd03` [validation] "Project deletion protection verified" (via `validates`).
  - `849f866e` [validation] "Automated test suite passes (18 tests)" (indirect).
  - `02f58acf` [validation] "Task filtering by status and priority verified" (indirect).
- **Accountable artifact:** `012d3257` "src/app.js - API route definitions" (via
  `represents` of the routes implementation and `derived-from` of the error-codes decision).

### Q3. Which artifact is responsible for persistence behavior, and what authoritative knowledge is it accountable to?

**KAT queries:** `kat artifacts`, `kat trace 9cec3c64-c837-46a9-a442-8e05e95121cb --compact`, `kat show 9cec3c64...`

The artifact responsible for persistence behavior is **`src/store.js - persistence code`**
(`9cec3c64`). Its accountability baselines (from `kat artifacts`) are:

- `represents` → `99822b8d` [implementation] "JSON-file backed store with atomic writes".
- `derived-from` → `75b186e0` [design-decision] "Persist data to a JSON file with atomic writes".
- `derived-from` → `48f68bbf` [design-decision] "Separate persistence behind a store abstraction".

Its trace (`kat trace 9cec3c64 --compact`) shows the authoritative knowledge it answers to:

```
src/store.js - persistence code
  -> Persist data to a JSON file with atomic writes
     -> Persist data between application executions        (REQ-2)
        -> Provide a lightweight personal Task Management API   (Intent)
  -> Separate persistence behind a store abstraction
     -> Keep the implementation simple and lightweight     (REQ-12)
  -> JSON-file backed store with atomic writes
     -> Tasks belong to exactly one project                (CON-4)
     -> Deleting a project with existing tasks is rejected (CON-3)
     -> Avoid unnecessary infrastructure                   (CON-5)
```

So `src/store.js` is accountable to REQ-2 (persistence), REQ-12 (simplicity), CON-3
(deletion protection), CON-4 (task ownership), and CON-5 (no heavy infrastructure), via
its two design decisions and its represented implementation.

### Q4. Choose one element that changed during development. How did it evolve?

**Element chosen:** `c4039918` [requirement] "Reject deletion of a project that still has tasks".

**KAT query:** `kat history --element c4039918-ae9d-4d2d-9c2b-28f562b9cbbd`

- **Original meaning** (created in Change 1, revision `6a48e7d8c934`, version `c016f5c697cc`):
  "Deleting a project that still contains tasks must be rejected to avoid silently losing
  task data." This was deliberately broad, mirroring the initial product constraint.
- **What changed** (Change 3, revision `d659934691c9`, new version `acee9f9d831a`):
  "Deleting a project that still contains tasks must be rejected with an **HTTP 409
  Conflict response** so that existing task data is never silently lost. The project
  remains intact after the rejected attempt."
- **Why it changed:** during implementation the concrete enforcement decision was made —
  the store refuses deletion by throwing `PROJECT_HAS_TASKS`, and the route maps that to
  `409 Conflict`. The requirement was refined to record this decision as authoritative
  knowledge rather than leaving the mechanism implicit in code.
- **Which accepted Change recorded it:** Change `be27a9c3-71ca-4e76-875f-5b7b28749f83`
  ("Evolve deletion-protection semantics...", revision `d659934691c9`), a 9-operation
  change that also updated CON-3, IMPL-1, IMPL-2, and added the deletion-protection
  validation evidence — a good example of one meaningful evolution staged together.

The constraint **"Deleting a project with existing tasks is rejected"** (`2eebc264`)
evolved in the same Change in the same direction (store-level guard + `409 Conflict`).

---

## 4. Artifact Accountability Observations

Two genuine `STALE` events occurred, both caused by evolving the implementations that
artifacts represent. In both cases the physical artifact was inspected and did **not**
require modification, because the code had been written to the refined behavior from the
start and the semantic model was catching up to decisions already made at implementation
time.

**Stale event 1 — after Change 3 (deletion-protection evolution)**

- **Artifacts that became stale:** `src/app.js` (`012d3257`) and `src/store.js` (`9cec3c64`).
- **Target knowledge that changed:** `56adfdb8` (routes implementation) and `99822b8d`
  (store implementation) were updated to record the HTTP-409 / `PROJECT_HAS_TASKS`
  enforcement; `c4039918` and `2eebc264` were updated.
- **Did the physical artifact require modification?** No. Verified via search that
  `src/app.js` maps `PROJECT_HAS_TASKS` → `409` and `src/store.js` throws
  `PROJECT_HAS_TASKS` — the artifacts already implemented the evolved semantics.
- **What was changed:** nothing in the code.
- **Reconciliation:** `kat account 012d3257...` (revision `a645a6d558b3`) and
  `kat account 9cec3c64...` (revision `6c0ec2e0e58b`), each with a description recording
  the verification that the artifact aligns.

**Stale event 2 — after Change 4 (filtering evolution)**

- **Artifact that became stale:** `src/app.js` (`012d3257`).
- **Target knowledge that changed:** `56adfdb8` (routes implementation) was updated to
  record the concrete `status`/`priority` query-parameter filtering; `15a43505` and
  `b727c2b6` were updated.
- **Did the physical artifact require modification?** No. Verified via search that
  `src/app.js` reads `req.query.status`/`req.query.priority`, validates them (400), and
  passes them to the store.
- **What was changed:** nothing in the code.
- **Reconciliation:** `kat account 012d3257...` (revision `f01f24511f20`).

These events were real, not manufactured: the implementations genuinely received new
versions, KAT flagged the affected artifacts, and reconciliation was performed only after
inspecting the physical files.

---

## 5. Problems Encountered

1. **"Implementation realizes Constraint" was rejected by the ontology.**
   - Trying to do: model that an implementation enforces a constraint (e.g. the store
     enforces "Deleting a project with existing tasks is rejected").
   - What KAT did: `kat link realizes <impl> <constraint>` failed with
     "ontology conformance error: relationship type kat.core/realizes does not allow
     target element type kat.core/constraint".
   - Why difficult: the task brief listed "Implementations realizing Requirements" but no
     canonical relationship from implementation to constraint; the CLI help only gives
     examples. It took trial-and-error (and reading the binary ontology blob) to learn
     that `restricts` is constraint→implementation and that no
     implementation→constraint relationship exists.
   - How I proceeded: used `restricts` (constraint restricts implementation) for all
     constraint-to-implementation connections and dropped the invalid direction.

2. **The ontology is not discoverable through the CLI.**
   - Trying to do: enumerate the valid relationship types and their allowed
     source/target element types before creating links.
   - What KAT did: no `kat ontology`/`kat schema` command exists; the ontology lives in a
     binary blob under `.kat/objects/` with no human-readable dump command.
   - Why difficult: I had to hex-dump the blob and manually decode type names and allowed
     pairs; two link attempts failed before I had the full picture.
   - How I proceeded: read the binary ontology, decoded all 10 relationship types and
     their allowed pairs, and used the decoded table for the rest of the session.

3. **`validates` cannot target Design Decisions.**
   - Trying to do: link the filtering validation evidence to the filtering design
     decision `b727c2b6`.
   - What KAT did: rejected with "ontology conformance error: relationship type
     kat.core/validates does not allow target element type kat.core/design-decision".
   - Why difficult: it is reasonable to want validation evidence for a decision; the
     restriction is not documented anywhere user-visible.
   - How I proceeded: linked the validation only to the requirement and implementation.

4. **`node --test test/` failed on Windows.**
   - Trying to do: run the test suite via the npm `test` script using `node --test test/`.
   - What happened: `MODULE_NOT_FOUND: Cannot find module '...\test'` — Node tried to load
     the directory as a module instead of discovering tests.
   - Why difficult: the trailing-slash form is a common pattern but breaks here on
     Windows/Node 22.
   - How I proceeded: changed the script to `node --test`, which uses default discovery.

5. **Full `kat trace` output is verbose and got redirected to a file.**
   - Trying to do: trace the routes implementation to understand its origins.
   - What KAT did: returned a 19 KB multi-path dump that the environment wrote to a
     scratch file rather than the terminal.
   - Why difficult: hard to scan; paths are repeated with only the `--compact` variant
     being concise.
   - How I proceeded: used `kat trace ... --compact` for subsequent traces.

6. **Five constraints are reported as "unverified" with no clear required action.**
   - Trying to do: get a clean validation pass.
   - What KAT did: `kat validate` reports 0 violations but lists 5 "unverified
     constraints" (reason: no executable validation rule), e.g. the priority enum and
     deletion protection.
   - Why difficult: it is not obvious whether this indicates missing work or is an
     expected informational state for constraints enforced by code/tests.
   - How I proceeded: treated it as informational (the constraints are enforced by the
     implementation and covered by `validation` evidence) and documented it.

7. **Deciding Change boundaries.**
   - Trying to do: decide how to group semantic operations into Changes without
     over-fragmenting (the brief warns against a Change per trivial edit).
   - What KAT did: provided only begin/status/commit — no guidance on granularity.
   - Why difficult: grouping choices are judgment calls; e.g. whether the three
     artifact-account reconciliations should each be their own revision (they were, and
     each produced a distinct revision).
   - How I proceeded: grouped by meaningful evolution (initial model, core implementation,
     deletion-protection refinement, filtering refinement) and let `kat account`
     reconciliations be their own revisions.

8. **PowerShell vs. Unix shell conventions.**
   - Trying to do: suppress stderr with `2>/dev/null` in a one-liner.
   - What happened: PowerShell treated `2>/dev/null` as a redirection to `C:\dev\null`,
     which failed (no such path).
   - Why difficult: a trivial habit-based mistake, but it shows the CLI examples assume a
     Unix shell.
   - How I proceeded: dropped the redirection and re-ran the command normally.

---

## 6. Useful KAT Behaviors

1. **Impact analysis shaped the scope of a modification.** Before Change 3, `kat impact
c4039918` (deletion requirement) listed exactly the implementations, design decision,
   validations, and the `src/app.js` artifact that would be affected — confirming the
   planned scope before I opened the Change.

2. **Artifact accountability caught staleness automatically and at the right moment.**
   Updating IMPL-1/IMPL-2 in Change 3 flipped `src/app.js` and `src/store.js` to `STALE`;
   updating IMPL-1 in Change 4 flipped `src/app.js` again. Without `kat artifacts`, these
   would have gone unnoticed until a code review.

3. **Trace answered a "why does this exist" question in seconds.** `kat trace` on the
   routes implementation produced the semantic path to the Intent, making the rationale
   for the completion feature explicit (Q1 above).

4. **History preserved multi-operation Changes with rationale.** Change 3's 9 operations
   (2 requirements/constraints updated, 2 implementations updated, 1 validation created,
   4 links) were committed as one revision whose description records the rationale, so the
   whole evolution is inspectable as a unit.

5. **Accountability reconciliations leave an auditable trail.** Each `kat account` call
   produced a revision with the verification description, so it is possible to see _why_
   an artifact was re-baselined (e.g. "artifact already aligns... no modification
   required") rather than just that it was.

6. **The ontology prevented an invalid semantic model.** The `realizes`→constraint
   rejection (Problem 1) stopped me from creating a relationship the ontology does not
   support, keeping the model conformant (0 violations).

---

## 7. Improvement Points

### Usability improvements

1. **Observed limitation:** relationship types and their allowed element-type pairs are
   not discoverable from the CLI (Problems 1–3); the ontology is a binary blob.
   **Proposed improvement:** add a `kat ontology` (or `kat schema`) command that prints
   element types and, for each relationship type, the allowed source/target types.
   **Why it helps:** developers would avoid trial-and-error link failures and could model
   correctly on the first attempt.

2. **Observed limitation:** full `kat trace` / `kat impact` output is verbose and was
   redirected to a file (Problem 5).
   **Proposed improvement:** default to a compact rendering when many paths exist, or
   print a summary with an option to expand.
   **Why it helps:** the core value (which elements are connected and via which
   relationship) is visible at a glance.

### Semantic-model improvements

3. **Observed limitation:** there is no canonical relationship from an implementation to
   a constraint it enforces (Problem 1); `restricts` only goes constraint→implementation.
   **Proposed improvement:** add an `enforces` relationship (implementation → constraint),
   complementary to `restricts`.
   **Why it helps:** "this code enforces that constraint" is a common, meaningful fact in
   real development, and today it must be shoehorned into the reverse direction or left
   implicit.

4. **Observed limitation:** `validates` cannot target a design-decision (Problem 3).
   **Proposed improvement:** allow `validates` to target design-decision (or add
   `evaluates` for decision validation evidence).
   **Why it helps:** decisions are exactly the things that benefit from evidence later
   (e.g. "we validated that query-param filtering meets the requirement"), and currently
   that evidence cannot attach to the decision.

### Query improvements

5. **Observed limitation:** no way to see which constraints have no validation coverage
   other than the "unverified constraints" line in `kat validate` (Problem 6).
   **Proposed improvement:** make `kat validate` (or a query) list constraints without any
   incoming `validates` relationship explicitly as a coverage report.
   **Why it helps:** teams could distinguish "constraint intentionally enforced by tests"
   from "constraint with no evidence at all".

### Change-workflow improvements

6. **Observed limitation:** there is no guidance or signal for appropriate Change
   granularity (Problem 7), and each `kat account` reconciliation becomes its own revision.
   **Proposed improvement:** allow `kat account` reconciliations to be staged into an open
   Change (so N reconciliations can be one revision), and optionally surface a suggestion
   when a Change grows very large.
   **Why it helps:** the revision history would better reflect human-intended groupings,
   reducing noise from mechanical reconciliations.

### Artifact-accountability improvements

7. **Observed limitation:** if an artifact's `derived-from`/`represents` link is created
   in the same Change that updates the target element, the baseline is set against the new
   version, so a would-be staleness is silently avoided.
   **Proposed improvement:** when a link to an element and an update of that element are in
   the same Change, emit a warning (or baseline against the pre-Change version).
   **Why it helps:** it makes reconciliation decisions explicit instead of letting
   staleness be masked by ordering.

---

## 8. Final Assessment

- **Did the semantic repository remain understandable as the implementation grew?**
  Yes. At 40 elements and 84 relationships it was still readable via `kat list`/`kat show`,
  and every element had a clear role. The ontology's type discipline kept the model tidy.

- **Was meaningful software evolution adequately represented through Changes?**
  Yes. Four semantic changes captured the real evolution: initial model, core
  implementation, deletion-protection refinement (409 semantics), and filtering
  refinement (query parameters). Each was a cohesive multi-operation unit with rationale.

- **Could the final implementation be traced back to authoritative knowledge?**
  Yes. Every artifact has an accountability baseline and every implementation traces back
  to requirements, constraints, decisions, and ultimately the Intent (Q1–Q3).

- **Did artifact accountability provide useful information?**
  Yes — it produced two genuine `STALE` events at the exact points where the represented
  implementations evolved, forcing an explicit "inspect then reconcile" step.

- **Did KAT expose information that would have been difficult to recover from source-code history alone?**
  Yes: the _rationale_ for behavior (why 409 for deletion, why the status model, why
  query-parameter filtering), the _impact_ of a requirement change before making it, and
  the _validation evidence_ tying tests to constraints. Git history records _what_
  changed in code; KAT recorded _why_ the knowledge changed.

- **What was the largest source of friction?**
  Ontology discoverability — learning the valid relationship types and their allowed
  targets by trial and error and by decoding a binary blob. The second largest was
  judgment about Change granularity and what constitutes "meaningful" evolution.

**Conclusion on practicality for a larger project.** The workflow is practical: the discipline of writing requirements/constraints/decisions first, then tying every artifact and validation back to them, produced a genuinely traceable system at modest cost, and accountability + impact analysis are high-value behaviors. Before scaling to a larger project, KAT should first improve **ontology discoverability** (a `kat ontology` command), add an **implementation→constraint enforcement relationship**, and provide **better guidance on Change granularity** — otherwise link errors and boundary decisions consume disproportionate attention as the model grows.
