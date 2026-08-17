These results are quite valuable because the experiment did more than confirm that KAT works: it exposed where the current model is strong, where the CLI is weak, and where the **development workflow itself creates semantic friction**. I would not accept every recommendation in `EXPERIMENT.md` at face value, though. Some are genuine KAT gaps; others are misunderstandings of capabilities KAT already has or would weaken the ontology if implemented directly. 

## Overall result

The experiment gives good evidence that the central KAT hypothesis is viable:

```text
Specification
    ↓
Semantic evolution
    ↓
Implementation / Artifacts

with

Trace + Impact + History + Accountability
```

The strongest evidence is not that the final repository had 40 elements and 84 relationships. It is that the agent actually used semantic information during development:

* `impact` influenced the scope of a change before it was made;
* ontology validation prevented invalid relationships;
* multi-operation Changes preserved meaningful evolution as units;
* `trace` reconstructed rationale from implementation back toward intent;
* artifact accountability reacted automatically when represented knowledge versions changed;
* history preserved why requirements became more specific. 

That is much more meaningful than simply proving that KAT can store a graph.

---

# 1. The clearest real defect: ontology discoverability

This is the strongest result of the whole experiment.

The agent tried to determine what relationships were legal and ended up:

> decoding the binary ontology blob

after failed `kat link` attempts. 

That is unacceptable for normal use.

The ontology is effectively part of the language through which users interact with KAT. If users can execute:

```text
kat link validates ...
kat link realizes ...
kat link guides ...
```

then they must be able to ask KAT what those predicates mean and what endpoints they allow.

I would make this the first concrete v0.3 capability.

Something along the lines of:

```text
kat ontology
kat ontology relationships
kat ontology show kat.core/realizes
kat ontology show kat.core/design-decision
```

For example:

```text
$ kat ontology show kat.core/realizes

kat.core/realizes

Source:
  kat.core/implementation

Targets:
  kat.core/requirement

Meaning:
  An Implementation semantically realizes a Requirement.
```

And:

```text
$ kat ontology relationships --source implementation

TYPE                  TARGET
realizes              requirement
depends-on            implementation
```

This is not merely CLI polish. It makes the **semantic language discoverable**.

I would mark this **P0 for v0.3**.

---

# 2. I would reject the proposed `enforces` relationship for now

The experiment proposes:

```text
Implementation -- enforces --> Constraint
```

because the agent initially tried:

```text
Implementation -- realizes --> Constraint
```

and KAT correctly rejected it. 

I don't think the evidence establishes a missing ontology concept yet.

You already have:

```text
Constraint -- restricts --> Implementation
```

which expresses that the implementation is subject to that constraint.

The fact that humans sometimes say:

> "this implementation enforces this constraint"

doesn't necessarily mean KAT needs another canonical edge in the opposite direction.

Adding both:

```text
Constraint     -- restricts --> Implementation
Implementation -- enforces  --> Constraint
```

creates a risk of semantically redundant relationships that users would then need to keep consistent.

The experiment mostly showed a **discoverability problem**:

> the agent did not know that `restricts` was the canonical modeling direction.

Once it discovered the ontology, it modeled the relationship correctly.

So I would record this as:

```text
Observation:
Users naturally search for an Implementation -> Constraint predicate.

Current interpretation:
Potential ontology ergonomics issue, not yet evidence for a new core relationship.

Action:
Improve ontology discovery and documentation first.
Re-evaluate after more real projects.
```

That is exactly the kind of thing another experiment should confirm before changing the core ontology.

---

# 3. Same caution with `validates -> Design Decision`

The agent wanted:

```text
Validation -- validates --> Design Decision
```

and proposed expanding `validates`. 

Again, I would **not change the ontology yet**.

There are at least two distinct concepts:

```text
Validation proves/checks a property of something

Evidence evaluates whether a design decision was a good choice
```

Those are not necessarily the same semantic relation.

For example:

```text
benchmark result
    evaluates
database architecture decision
```

might make sense in the future, while:

```text
test result
    validates
requirement
```

has a stronger conformance meaning.

Expanding `validates` to decisions could blur that distinction.

This is a legitimate observation to retain, but it belongs in an **ontology research backlog**, not necessarily v0.3 implementation.

---

# 4. One proposed Change-workflow improvement is already supported

The experiment says:

> “allow `kat account` reconciliations to be staged into an open Change”

because its three reconciliations became separate revisions. 

But based on the v0.2 model we froze, **`AccountArtifact` is already a mutation operation and can participate in an open draft Change**.

All mutations follow the same interaction model:

```text
no draft
    mutation -> standalone Change

open draft
    mutation -> staged operation
```

So the three reconciliation revisions were not a KAT limitation. They were a workflow choice by the agent.

It could have done:

```text
kat change begin --description "Reconcile artifacts after deletion semantics"

kat account <app>
kat account <store>

kat change status
kat change commit
```

and obtained one meaningful reconciliation Change.

This is an important experimental finding nevertheless, because it means **the CLI did not make that capability obvious enough**.

So I would rewrite the finding as:

```text
Observed:
The agent performed related AccountArtifact reconciliations as separate Changes.

Actual capability:
KAT already permits AccountArtifact inside an open multi-operation Change.

Likely problem:
Transaction-mode discoverability / workflow guidance.

Potential improvement:
Make mutation behavior under an open draft clearer in help/status output.
```

That is more accurate.

---

# 5. The `unverified constraints` issue reveals a deeper UX ambiguity

This finding is particularly interesting.

KAT reported:

```text
0 violations
5 unverified constraints
```

while those constraints were also connected to `Validation` evidence and covered by tests. 

Semantically, KAT is correct.

A relationship such as:

```text
Validation -- validates --> Constraint
```

does **not** magically give KAT an executable rule capable of proving that Constraint.

So these are two different questions:

```text
Does this Constraint have validation evidence?
                !=
Can KAT mechanically evaluate this Constraint?
```

The agent conflated them.

That suggests the current presentation may be insufficiently explicit.

Instead of simply:

```text
Unverified Constraints: 5
```

I would consider terminology like:

```text
Mechanically Unverified Constraints: 5
```

and potentially show evidence separately:

```text
Constraint                          Mechanical Rule   Validation Evidence
Priority must be low/medium/high    none              1
Project with tasks cannot delete    none              1
```

That would make the distinction visible:

```text
semantic constraint
    ↓
executable rule?       validation evidence?
     yes/no                  yes/no
```

This could become a very useful future **validation coverage query**, but not exactly as suggested in the experiment.

The experiment proposed using incoming `validates` edges to determine "validation coverage." That is reasonable as an **evidence coverage** metric, but it must not be confused with mechanical validation.

---

# 6. The most important workflow finding: the semantics sometimes followed the code

This is, to me, the most interesting result.

The report says that after the semantic deletion requirement was refined, `app.js` and `store.js` became stale. The agent inspected them and discovered:

> the code already implemented the evolved semantics. 

The same happened with filtering.

That means the actual evolution was roughly:

```text
code already implements behavior
        ↓
semantic model later refined to describe it
        ↓
implementation version updated in KAT
        ↓
artifact becomes STALE
        ↓
inspection says artifact already matches
        ↓
AccountArtifact
```

This is valid, and KAT behaved correctly.

But philosophically it shows a mild inversion of the intended workflow:

```text
desired:
knowledge evolves
    ↓
artifact becomes stale
    ↓
artifact is updated
    ↓
reconcile

observed twice:
artifact behavior already exists
    ↓
knowledge catches up
    ↓
artifact becomes stale
    ↓
no code change necessary
    ↓
reconcile
```

This doesn't invalidate accountability. In fact, it proves that KAT correctly refuses to infer physical divergence.

But it tells us something about **how difficult specification-first discipline is even for an AI agent explicitly instructed to follow it**.

That deserves to be recorded as a major experiment observation.

I would run a future experiment specifically measuring:

```text
How often does:
    knowledge -> implementation

versus:
    implementation -> semantic catch-up
```

That may eventually influence tooling around Change planning.

---

# 7. The initial semantic model may be over-modeled

The first Change contained **52 operations**, and the second contained **64 operations**. 

For a small Task API, the final repository had:

```text
40 elements
84 relationships
```

That is not automatically excessive. But it is enough to raise a serious question:

> Is KAT encouraging useful semantic modeling, or encouraging exhaustive graph construction?

The prompt explicitly asked for Intent, Requirements, Constraints, Decisions, Implementations, Artifacts and Validation, so some of this density was induced by our experimental setup.

Still, this is something I would investigate before scaling.

The key metric should not be:

```text
more elements = better
```

It should be:

```text
Does this element or relationship answer a useful future question?
```

The most successful elements clearly did:

* deletion requirement;
* HTTP status decision;
* persistence decisions;
* routes implementation;
* persistence artifact.

But with 84 relationships, there are probably edges that added little practical value.

For the next experiment I would explicitly tell the agent:

> Model only knowledge whose absence would make rationale, impact, validation, or accountability meaningfully harder to recover.

Then compare repository size and usefulness.

---

# 8. Change granularity is genuinely unresolved

This one is real.

The agent successfully chose four meaningful semantic Changes:

```text
1. Initial semantic model
2. Core implementation
3. Deletion semantics refinement
4. Filtering semantics refinement
```

and the latter two are excellent examples of what KAT means by a meaningful Change. 

But the first two are huge:

```text
52 operations
64 operations
```

This exposes a conceptual question that KAT currently leaves entirely to the user:

> What is the semantic unit of evolution?

I would **not have KAT automatically reject or warn based simply on operation count**. A 70-operation migration could be genuinely one meaningful Change.

Instead, first improve guidance.

Perhaps documentation such as:

```text
A Change should answer one sentence:

"What meaningful evolution becomes true if this Change is accepted?"
```

Examples:

```text
Good:
"Introduce JSON persistence behind a storage abstraction."

Probably too broad:
"Implement the application."

Probably too narrow:
"Change the title property of one element."
```

Then more experiments can tell us whether tooling is needed.

---

# 9. Trace verbosity is a genuine query UX problem

A 19 KB trace for a small system is already concerning. 

This becomes much worse on a serious repository.

`--compact` helped, but the problem is structural: graph queries can have path explosion and repeated prefixes.

I would consider this a real v0.3 candidate.

Possible approaches:

```text
kat trace <id>
    summary / collapsed tree

kat trace <id> --paths
    exhaustive paths

kat trace <id> --max-depth N

kat trace <id> --type requirement

kat trace <id> --through <relationship>

kat trace <id> --compact
```

Most importantly, shared prefixes/subgraphs should ideally not be repeated as independent long paths.

---

# 10. PowerShell and Node issues are not KAT product findings

Two reported problems should be excluded from KAT planning:

* `node --test test/` failing on Windows;
* `2>/dev/null` failing in PowerShell. 

The second might justify making KAT documentation/examples shell-neutral if KAT itself provides Unix-specific examples, but in this experiment the problematic command appears to have been generated by the AI agent itself.

So I would classify them as:

```text
environment / agent execution noise
```

not KAT defects.

---

# What I think the experiment actually tells us

I would reduce the retrospective to five high-confidence findings:

| Finding                                                   |  Confidence | Likely action                                        |
| --------------------------------------------------------- | ----------: | ---------------------------------------------------- |
| Ontology is not discoverable                              |   Very high | v0.3 CLI capability                                  |
| Trace output does not scale well                          |        High | v0.3 query UX                                        |
| Mechanical validation vs validation evidence is confusing |        High | terminology/reporting refinement                     |
| Meaningful Change boundaries require judgment             | Medium-high | guidance + more experiments                          |
| Specification-first sequencing is difficult to maintain   |        High | investigate workflow, do not immediately add feature |

And three hypotheses requiring more evidence:

| Hypothesis                                                                  | Current disposition                            |
| --------------------------------------------------------------------------- | ---------------------------------------------- |
| Need `Implementation -> enforces -> Constraint`                             | Do not add yet                                 |
| `validates` should target Design Decision                                   | Investigate semantics first                    |
| Accountability baseline semantics need changing for same-Change Link+Update | Needs a targeted test before changing anything |

That last experiment recommendation about same-Change link/update is especially important. The report claims a potential staleness can be “silently avoided” when a relationship is introduced against a target updated in the same Change.  I would **not change the baseline to the pre-Change version** automatically. A newly accepted relationship should normally describe the candidate state being accepted, so baselining against an obsolete version would be suspicious. We should test concrete scenarios before touching that semantic rule.

## What I would make v0.3 right now

I would resist adding major semantic capabilities after one experiment.

A sensible next increment would be something like:

> **v0.3 - Semantic Discoverability and Inspection**

with a narrow goal:

```text
Make KAT's semantic model understandable and navigable
without external knowledge of its internal representation.
```

Potential scope:

```text
P0
  ontology discovery / inspection

P1
  scalable trace rendering and filtering
  clearer validation classification:
      violations
      mechanically-unverified constraints
      validation-evidence coverage

P2
  Change-authoring guidance / better draft UX
  improved mutation help showing draft-vs-standalone behavior
```

Then run the **same style of experiment again on a somewhat more complex project** before touching the ontology.

The experiment was successful precisely because it produced evidence that can contradict our initial intuitions. The next step should be to use that evidence conservatively, fixing obvious usability failures first and accumulating more data before expanding KAT's semantic language. 
