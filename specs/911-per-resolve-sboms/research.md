# Phase 0 Research: Per-resolve SBOMs for Pants monorepos

**Feature**: `911-per-resolve-sboms` | **Issue**: [#902](https://github.com/kusari-oss/waybill/issues/902) items 1, 3, 4
**Date**: 2026-09-17

Every claim below is traceable to a file and line in this workspace, per the
project's "measure before designing around it" rule.

---

## R1 — The load-bearing assumption is HALF TRUE, and that changes the plan

The spec's Assumptions say, flagged as unverified: *"Partitioning depends on
membership, not on anchors — the anchor is a convenience for graph
traversal."* FR-009a and SC-006a exist to test it. Tested:

**As a concept, true. As implemented, false.** The existing split does not
partition by membership at all — it does breadth-first traversal from a seed
component:

```rust
// split.rs:311
pub(crate) fn project_for_root(root: &SubprojectRoot, all_components, all_relationships)
    -> SplitProjection
// builds a from → [to] adjacency map, then BFS from root.purl_string
```

and the seed set comes from main-module components only:

```rust
// split.rs:187
pub(crate) fn enumerate_workspace_roots(...) -> Vec<SubprojectRoot> {
    resolved_components.iter().filter(|c| is_main_module(c))   // <- the split axis
```

A resolve anchor is not a main module, so **neither existing mode partitions a
Pants repository today** — which is what item 4 of #902 already observed. More
to the point for the clarify answer: a *discovered* resolve has no anchor
component at all, so there is nothing for BFS to start from, and no
`SubprojectRoot` can be constructed for it (`SubprojectRoot` requires a real
`Purl`).

**Decision**: `SplitMode::Resolve` selects by **membership filter**, not by
BFS from a seed. Components whose membership contains resolve R form R's
projection, plus the relationships whose endpoints are both in that set.

**Rationale**: it is the only strategy that works uniformly for declared and
discovered resolves, so there is one code path rather than two that must agree.
It also satisfies FR-013 directly — the operator's split and a consumer's own
walk use the same data, rather than one using membership and the other using
edges and hoping they match.

**Alternative rejected — BFS from the anchor where one exists, filter
otherwise.** Two strategies that must produce the same partition is a bug
waiting to happen, and after m910 scoped edges within a resolve the two would
*usually* agree, which is worse than never agreeing: the disagreement would be
rare and data-dependent.

**Consequence for the clarify answer**: the decision not to anchor discovered
resolves survives, but only because the projection strategy changes with it.
Had `SplitMode::Resolve` reused `project_for_root`, FR-009a would have failed
and the declared-vs-discovered answer would have had to be reopened. That
contingency is now discharged.

---

## R2 — Each sub-SBOM's root component is an unsolved question

Not raised in #902 and not visible from the spec, but it blocks Story 3.

A sub-SBOM's `metadata.component` is not set by the split. It is chosen at
emit time by milestone 127's root-selector ladder, which looks for the single
component carrying `waybill:component-role = "main-module"`. m215 hit this and
left the evidence in a comment at `split.rs:355-380`: sibling main-modules
pulled in by BFS confused the ladder, producing the wrong
`metadata.component.purl` for 23 of 25 sub-SBOMs on a real repository. The fix
was to *demote* siblings so the ladder sees exactly one.

For a resolve projection the problem is the mirror image: **there is no
main-module at all**. The resolve anchor is `pkg:generic/<resolve>` with no
such role, and a discovered resolve has no anchor component either. The ladder
would fall through to its synthetic-placeholder branch and name every
sub-SBOM something unhelpful.

**Decision**: the per-resolve projection promotes its own root explicitly
rather than relying on the ladder to infer one. For a declared resolve that is
the existing anchor component; for a discovered resolve the split synthesises
one for the emitted document.

**Open for the plan to settle, not research**: whether the synthesised root for
a discovered resolve is also worth emitting in the *unsplit* document. It is
not — that would be anchoring discovered resolves by the back door, which
FR-009 forbids. Noting it explicitly so the implementation does not drift into
it.

---

## R3 — Three readers emit this annotation, not one

`waybill:pants-resolve` (catalogue row C143) is written by:

| Reader | File |
|---|---|
| Pants Pex | `scan_fs/package_db/pants/lockfile.rs`, `pants/mod.rs`, `pants/resolve_classifier.rs` |
| Pants coursier/JVM | `scan_fs/package_db/pants_jvm/lockfile.rs`, `pants_jvm/resolve_classifier.rs` |
| **uv** | `scan_fs/package_db/pip/uv_lock.rs:227` |

The third is easy to miss. The uv reader stamps the annotation when a uv
lockfile is used as a Pants resolver backend (`uv_lock.rs:188`), threading
`pants_resolve_name` into every entry.

**Decision**: the array change applies at all three sites, and the plan's task
list enumerates them. A reader left on the scalar form would emit a value that
parses differently from its siblings — precisely the cross-reader
inconsistency #901 was about, reintroduced.

**How this was found**: `grep -rln "pants-resolve" scan_fs/package_db/`. Worth
repeating at implement time rather than trusting this list, per the project's
rule about re-verifying research greps.

---

## R4 — Deduplication: where to union, and what must not change

The merge is at `resolve/deduplicator.rs:213`:

```rust
for (key, value) in other.extra_annotations {
    best.extra_annotations.entry(key).or_insert(value);   // first wins
}
```

grouped by `(ecosystem, name, version, parent_purl)` (`deduplicator.rs:33-46`).

The milestone-109 comment above it explains why first-wins is right for the
case it was written for — source-tier and binary-tier evidence describing one
artifact, where the higher-confidence side should win. That reasoning is sound
and must survive.

**Decision**: a per-key merge policy, with union applied only to keys whose
value set is genuinely plural. Not a blanket change from `or_insert` to union:
`waybill:sbom-tier` or `waybill:source-type` unioned across a merge would
produce a value that is true of neither side.

**Alternative rejected — union every array-valued annotation.** Tempting,
because the encoding would tell you what to do. But `waybill:source-files` and
`waybill:file-paths` are already arrays and are already unioned elsewhere by a
dedicated pass (milestone 148); folding them into a generic rule risks
double-handling. An explicit list of plural keys is duller and safer.

---

## R5 — Doc-scope ownership annotation already exists and is counts-only

C161 `waybill:resolve-ownership`, added by m868, is a document-scope annotation
whose wire form is built at `pants/mod.rs:385-393`:

```
weak-classification=<N>;unanchored-lockfiles=<N>
```

Observed in the committed corpus goldens:

| target | value |
|---|---|
| pants-example-django | `weak-classification=1;unanchored-lockfiles=0` |
| pants-example-python | `weak-classification=1;unanchored-lockfiles=0` |
| pants-example-golang / javascript / jvm | absent |

So the carrier for FR-007 exists; it counts but does not name. The extractor
triple is registered at `parity/extractors/mod.rs:642` as `SymmetricEqual`,
`order_sensitive: false`.

**Decision**: extend C161's value to name the resolves in each category rather
than adding a second doc-scope row. One row, one place a consumer looks.

**Constraint the plan must respect**: the existing value is a
semicolon-delimited `key=value` string, not JSON. Adding names means either
changing that grammar or nesting a list inside it. Whichever is chosen, the
catalogue row's documented grammar and any consumer of the count form move
together — and per the project's own gate, a catalogue row and its extractors
must change in the same change or the parity tests fail.

---

## R6 — Scope of golden churn

Because the encoding changes for every Pants component, not only shared ones
(clarify decision 2), churn is proportional to Pants coverage:

| Corpus target | components carrying C143 |
|---|---:|
| pants-example-django | 34 |
| pants-example-jvm | 27 |
| pants-example-python | 11 |
| **total** | **72** |

Plus the crate-local Pants fixtures and any unit-test assertions on the scalar
form. Corpus goldens are CI-generated; the plan must not regenerate them
locally.

**Note for the tasks**: this is the second consecutive feature to churn Pants
goldens (m910 and #901 both did). The per-feature refresh cost is real and is
an argument for landing the whole of this feature before refreshing, rather
than per user story.

---

## R7 — What has NOT been verified

Stated plainly so nothing downstream treats these as settled:

- **The reported monorepo figures** (2,466 → 1,319; 20 of 24 resolves
  surviving) are from the issue author's repository and have not been
  reproduced here. The spec's Assumptions already say so. A first task should
  confirm them against current `main`, since #910 and #901 landed after they
  were taken — neither touches membership or dedup, so they are *expected*
  unchanged, and confirming that cheaply is better than assuming it.
- **Whether any consumer outside this repository reads `waybill:pants-resolve`
  today.** FR-006b requires the change be called out; how loudly depends on an
  answer nobody here has.
- **Behaviour when a package's membership is large.** The spec has it as an
  edge case; no measurement of the high end exists.

## Open items carried into Phase 1

None blocking. R1 and R2 both resolve into design decisions rather than
questions, and R2's discovery — that each sub-SBOM needs its root chosen
explicitly — is the one thing the spec did not anticipate.
