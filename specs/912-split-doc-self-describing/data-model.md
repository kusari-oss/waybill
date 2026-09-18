# Phase 1 Data Model: Split-document resolve identity

**Feature**: `912-split-doc-self-describing` | **Date**: 2026-09-18

No persistent store. One new document-scope annotation, plus a namespace that
readers must start recording.

---

## Entity: Resolve identity

Which resolve a per-resolve split document represents.

| Aspect | Value |
|---|---|
| Scope | document |
| Cardinality | exactly one, normally |
| Present on | per-resolve split documents only |
| Absent on | unsplit documents, `--split=workspace`, `--split=directory`, anything produced before this feature |
| Carried by | all three formats identically (FR-004, parity gate) |

**Validation**

- Absent, never present-and-empty (R5). "The question does not apply" and
  "the answer is missing" must stay distinguishable, matching how the m868
  ownership annotation behaves on a scan with no Pex lockfile.
- Names the resolve **and** its Pants language namespace (FR-001a), so two
  resolves called `default` under `[python.resolves]` and `[jvm.resolves]`
  are distinguishable.
- Where the document also has a root component naming the resolve — the
  declared-Python case — the two agree (FR-005), asserted by a test rather
  than assumed.

**The merged-document case.** #919 means two same-named resolves from
different namespaces currently collapse into one document. Such a document
genuinely represents two resolves, and stating either alone would be false.
It records **both**, which is the only truthful option available, and is
self-correcting: once #919 lands, no document is ever in this state and the
plural form stops occurring naturally. A consumer seeing two identities is
seeing a real defect, not a shape it has to support long-term.

---

## Entity: Pants language namespace

Which Pants section declared a resolve — `[python.resolves]` or
`[jvm.resolves]`.

| Aspect | Today | After |
|---|---|---|
| Recorded | **nowhere** | on the entry, by the reader that read it |
| Inferable | only from members' PURL ecosystem | directly |
| Readers that must set it | — | Pex, coursier/JVM, uv-as-Pants-backend |

**Why not infer it** (R2): every resolve in the fixtures happens to be
single-ecosystem, so `pkg:pypi/*` implies Python and `pkg:maven/*` implies
JVM. That is a property of the fixtures, not a guarantee — a resolve is a
lockfile, and nothing forbids a reader emitting mixed types from one. An
inference that is correct only because no counterexample has been written yet
is the kind that fails silently when one is.

**Validation**

- Set by every reader that emits resolve membership. A reader that sets
  membership without a namespace produces an identity that cannot satisfy
  FR-001a.
- Does not change component membership's own value — membership stays the
  resolve name. This entity is what makes the *identity* unambiguous, and
  whether membership itself should also be qualified is #919's question, not
  this feature's.

---

## Entity: Document-scope statements, after this feature

Two statements that describe different things and must stay separable
(FR-001b, SC-009):

| Statement | Describes | Same across a repository's split documents? |
|---|---|---|
| resolve ownership (existing) | the **repository** — which resolves were declared, which discovered | yes, identical in every document |
| resolve identity (new) | **this document** — which resolve it is | no, different in each |

The current confusion is that only the first exists, so a reader holding one
document sees repository-wide facts and nothing document-scoped. Folding the
identity into the first would put a document-scoped fact inside a
repository-wide container and reproduce it.

---

## What this feature does not change

- Which components, edges, or resolves are discovered.
- Whether a discovered resolve gains an anchor — it does not (FR-006).
- The repository-wide ownership statement's value (FR-007).
- Component membership's encoding, which m911 set.
- `--split=resolve`'s grouping, which is #919.
