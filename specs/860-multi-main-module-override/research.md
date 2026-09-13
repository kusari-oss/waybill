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

## R6 — No corpus target exercises N=1 or the identity collision

Measured across all eleven targets: main-module counts are 16
(maven-guice), 10 (rust-ripgrep), 4 (python-flask), and 0 for the other
eight. **No target has exactly one**, and none exhibits the FR-011
identity collision.

**Decision**: both cases need synthetic unit tests; the corpus cannot
cover them. This is the same trap as milestone 856, where a hand-built
fixture passed while production failed — so the N=1 test must run the
real emitter path, not a hand-assembled component vector.
