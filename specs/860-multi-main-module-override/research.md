# Phase 0 research — multi-main-module root override

Every finding below was read from the tree on 2026-09-13, not recalled.

## R1 — Three emitter call sites, one shared helper

`apply_main_module_drop_or_demote` is called from exactly three places:

| format | site |
|--------|------|
| CycloneDX | `waybill-cli/src/generate/cyclonedx/builder.rs:591` |
| SPDX 2.3 | `waybill-cli/src/generate/spdx/document.rs:425` |
| SPDX 3 | `waybill-cli/src/generate/spdx/v3_document.rs:65` |

**Decision**: change behaviour inside the helper and its return type, not
at the three sites. FR-009 requires identical policy across formats, and
milestone 149 consolidated these sites precisely so the policy lives in
one place.

**Alternative considered**: patch each emitter. Rejected — it is the
shape m149 removed, and three copies is how the formats drift.

## R2 — The parity catalog row already exists, and documents the decision we are superseding

`C102` (`waybill:demoted-from-main-module`) is registered at
`waybill-cli/src/parity/extractors/mod.rs:438` as `SymmetricEqual`, with
a long entry in `docs/reference/sbom-format-mapping.md:147`.

That entry states, as documented behaviour:

> Per US1 clarification Option A (recorded 2026-06-29), the demoted
> entry has NO outbound `dependsOn` edges in the wire output — its
> dep-graph topology is re-anchored on the operator-override root via
> milestone-084 logic.

**FR-007 makes that sentence false.** The catalog row is documentation
of emitted shape, and the shape changes.

**Decision**: update the C102 row in the same change as the behaviour.
No new row is needed — the annotation itself is unchanged, only the
surrounding description of edge topology.

**Consequence**: the repo's `every_catalog_row_has_an_extractor` gate
will not catch a stale *description*; only a missing extractor. So this
is a review-discipline item, listed explicitly in tasks rather than left
to a gate.

## R3 — SPDX 3 aliases PURLs specifically to serve re-anchoring

`v3_document.rs:318-324` aliases every dropped main-module PURL to the
synthesized root IRI so that dependency edges sourced at those PURLs get
rewritten to source from the new root (issue #229, mirroring the
milestone-084 CDX logic).

The C102 row documents the side effect this produces:

> **Subject-routing divergence vs CDX + SPDX 2.3**: … the demote
> annotation rides with the alias and ends up emitted with
> `subject = synth_root_iri` … rather than `subject = demoted_entry_iri`.
> … A future milestone could split `package_iri_by_purl` into separate
> annotation-routing vs relationship-routing maps to align subject
> behavior; deferred per the milestone-149 scope.

**Finding**: the alias exists *to serve re-anchoring*. FR-007 removes
re-anchoring for this path, so the alias may no longer be needed — which
would align the SPDX 3 annotation subject with CDX and SPDX 2.3 as a
side effect, closing the divergence m149 deferred.

**Decision**: treat this as in-scope but *verified, not assumed*. A task
checks whether the alias is still required for any other purpose before
removing it. If it is required, the divergence stays and is re-documented
rather than silently carried.

**Alternative considered**: remove the alias on the strength of the
reasoning above. Rejected — the alias predates m149 (issue #229) and may
serve paths this feature does not touch.

## R4 — The redirected-PURL set changes meaning, not shape

`DropOrDemoteResult.redirected_main_module_purls` currently drives edge
*removal* and *re-anchoring*. Under FR-007/FR-008 the same set identifies
which components the root should *depend on*.

**Decision**: keep the set, rename it to reflect the new meaning, and
change the consumers. A rename is cheap and a misleading name here
caused real confusion when reading this code during #863 triage.

## R5 — The N>1 INFO diagnostic disappears

`root_selector.rs:548`, `:573` and `:609` emit `tracing::info!`
diagnostics, including the milestone-149 FR-013 no-op notice for the
multi-main-module case. Under convergence that no-op no longer exists.

**Decision**: replace rather than delete. Constitution Principle X
(Transparency) wants operator-visible reasoning; a scan that retains 16
modules under an override should say so once, at INFO, with the count.

## R6 — CORRECTED: four corpus targets DO exercise N=1

**The original finding was wrong.** It claimed no corpus target has
exactly one main module, and that both N=1 and the FR-011 collision
therefore needed synthetic tests.

The measurement counted components in `components[]` carrying
`waybill:component-role = main-module`. At N=1 the module is promoted
*out* of `components[]` into `metadata.component`, so the count came back
zero for every single-module target. The method could only ever see N>1.

Corrected, from the milestone-860 regeneration (run 34770618930):

| N | targets |
|---|---------|
| 16 | maven-guice |
| 10 | rust-ripgrep |
| 4 | python-flask |
| **1** | **go-cobra, npm-express, pants-example-golang, pants-example-javascript** |
| 0 | image-postgres16, pants-example-django, pants-example-jvm, pants-example-python |

So **seven** targets are affected, not three, and the N=1 convergence
path has four real regression targets rather than none.

The four N=1 targets each gain exactly the project's own module —
`pkg:golang/github.com/spf13/cobra@v1.9.1` for go-cobra,
`pkg:npm/express@5.1.0` for npm-express. Naming the subject previously
deleted the scanned project from its own SBOM.

The synthetic N=1 test (T020) is still worth keeping: it runs the real
emitter and pins the behaviour independently of whether a corpus target
happens to exercise it.

**FR-011 remains uncovered by the corpus** — no target names a root
matching a module's PURL — so that one is genuinely synthetic-only.

### Why this matters beyond the number

This is the third measurement error in this feature's vicinity where the
method could not observe what it claimed to measure. Counting a
promoted-out component in the array it was promoted out of returns zero
and looks like an answer.
