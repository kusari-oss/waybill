# Research: Refresh the public-corpus goldens with verified drift

Feature: `840-refresh-corpus-goldens` · Spec: [spec.md](./spec.md) · Issue #763

The headline finding: **almost none of this feature needs new
infrastructure.** The generation path, the CI dispatch, the artifact
upload and most of the masking already exist and were built for exactly
this purpose by m195/m196. What does not exist is the *verification*
half, which the spec makes the point of the work.

---

## R1 — How goldens are regenerated

**Decision**: Reuse the existing dispatch. Build nothing.

`.github/workflows/public-corpus.yml` already exposes `workflow_dispatch`
with two inputs:

| input | type | default | purpose |
|---|---|---|---|
| `ref` | string | `main` | branch to run the corpus against |
| `regen_goldens` | boolean | `false` | regenerate in place and upload |

When `regen_goldens` is true the workflow sets
`WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1`, the harness writes actual
output over the goldens (`layer2_golden.rs:38`), and the job uploads the
whole fixture tree as the `corpus-goldens-regen` artifact
(`public-corpus.yml:122`).

So FR-002 and FR-003 — generate in the gate's own environment, never on
a maintainer's laptop — are **already satisfied by using the existing
path**. They are not new work; they are a constraint on how the existing
work is invoked. Recording them as requirements still matters: the
constraint is currently tribal knowledge, and the two most recent
lane failures in this repo (#818, #832) were both caused by ignoring
exactly this.

**Alternatives rejected**: adding a regen script, or regenerating
locally and committing. The second is what FR-003 forbids and what
`m196` FR-004a already forbade.

---

## R2 — What masking already exists, and what it implies for review

**Decision**: Goldens are stored **already masked**, so a plain
`git diff` of the fixture tree is largely normalised for free.

`layer2_golden.rs:51` applies `mask_nondeterministic` *before* writing
the golden. It currently neutralises:

- SPDX 2.3 per-annotation timestamps (rotate per scan via `Utc::now()`)
- SPDX 3 `/doc-` prefixed element IDs (per-scan identifier)
- SHA-256 / MD5 content hashes embedded in annotation `statement:`,
  `comment:` and `value:` JSON-in-string payloads

That last one was added by m196 for two reasons worth remembering: the
hashes rotate whenever an upstream image is re-published, producing drift
with no regression signal, and a raw SHA-256 in a fixture was once
flagged by a secret scanner as an API key.

**Implication for the plan**: FR-004/FR-005 are *mostly* already
delivered. The review diff is `git diff` over masked files. What remains
is R3.

---

## R3 — The one real gap: array ordering

**Decision**: Ordering must be normalised **at review time**, not by
changing emission.

SPDX 3 wraps output in a `@graph` array. Array order is not masked, and
masking cannot fix it — masking replaces values, it does not reorder.
The project's own hard-won note on this is explicit: mask
content-addressed IDs *and* `LC_ALL=C sort` before diffing, "else SPDX-3
array reordering fakes semantic hits".

This is the failure mode that makes a 147-merge diff unreviewable: a
reordered array presents as every element changing, burying whatever
genuinely changed underneath the volume.

**Chosen approach**: a review-time normaliser that sorts unordered
collections by a stable key before diffing. It operates on copies for
human review only — it never touches committed goldens, because changing
what is stored would change what the gate compares.

**Alternatives rejected**:
- *Sort at emission time.* Would make every golden churn on this change
  alone and alters shipped output to serve a test — the tail wagging the
  dog.
- *Sort inside `mask_nondeterministic`.* Same objection: it changes the
  stored artifact, and a reordered-but-equal golden would then silently
  compare equal, weakening the gate.
- *Review raw diffs.* Directly contradicts FR-004, and is the reason
  this has not already been done.

---

## R4 — Attributing deltas to causes

**Decision**: Attribute by *category* against the merge log, not
per-component.

The goldens were last written by `25bfbce` on 2026-07-21; roughly 147
merges have landed since. Attribution means: for each observable class
of change, name the merged change that produced it. One known example
accounts for a very large share on its own — m776 (#797) began emitting
source-provenance `externalReferences`, which touches essentially every
component that resolves a source URL.

Category granularity was deliberately deferred from clarification to
here (spec, Clarifications note). The working definition for the plan:
**a category is a repeated shape of change**, e.g. "every component
gained an `externalReferences` entry of type X". If a change appears
once and only once, it is not a category — it is an individual delta and
needs its own explanation, which is precisely the case FR-007 exists to
catch.

**Alternatives rejected**: per-component attribution (unreviewable at
this volume — thousands of components); per-target one-line summary (too
coarse to distinguish the regression case the feature exists to find).

---

## R5 — Proving the gate still has teeth

**Decision**: Mutate emission deliberately, confirm the lane fails, then
revert. Do not trust a passing lane as evidence of a working lane.

FR-010 and SC-003 require this because a refresh is indistinguishable
from a disable when judged only by "the lane is green". This repo has
already shipped the inverse mistake: a schema gate that passed because
its `$ref`s resolved to stubs, and so validated nothing.

**Alternatives rejected**: relying on the goldens' byte-identity nature
as self-evident proof. It is not — the harness could be skipping a
target, masking too aggressively, or short-circuiting on an error, and
every one of those presents as green.

---

## R6 — Where the procedure gets written

**Decision**: `docs/development/`, alongside the sibling perf-baseline
procedure.

FR-014 requires the method be findable by the next maintainer. There is
an existing home for this class of document — `docs/perf/refreshing-the-baseline.md`
does the same job for the bench baseline, and the two documents describe
the same hazard.

FR-015 forbids committing the *evidence* as a document; it says nothing
about the *procedure*. These are different artifacts: the evidence
describes one moment and belongs in the PR; the procedure is a standing
instruction and belongs in the tree.

**Alternatives rejected**: `docs/design-notes.md` — actively being
retired for accretion (#827), so adding to it would be moving in the
wrong direction.

---

## Open items carried into the plan

- **Which targets, exactly.** The last observed run showed ten failing
  of eleven. FR-001 scopes to "currently failing", so the definitive
  list comes from a dispatch at implementation time, not from this
  document.
- **Whether any target fails for a non-drift reason.** Unknown until the
  diffs are read. FR-012 / FR-012a govern the outcome if one is found.
- **`image-postgres16`.** Currently passing, therefore out of scope by
  FR-001 — but it is also the only image-tier target, so if it starts
  failing during this work the cause is likely upstream re-publication
  rather than emission drift, and R2's hash masking is the relevant
  context.
