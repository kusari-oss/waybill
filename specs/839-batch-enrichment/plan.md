# Implementation Plan: Batched, observable dependency enrichment

**Branch**: `839-batch-enrichment` | **Date**: 2026-09-12 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/839-batch-enrichment/spec.md` · Issue #766

## Summary

Enrichment contacts deps.dev once per component, in sequence. On a
7,592-component repository that is 7,592 round-trips and ~17 minutes of
total silence, so operators kill the scan and get no SBOM at all.

Three changes, in priority order: batch lookups through
`POST /v3alpha/versionbatch` behind a flag; make the per-component path —
which is both the default and the fallback — concurrent so the defect is
not reachable by either route; and emit time-triggered progress so a slow
phase reads as work rather than as a hang. A disk cache follows at P2.

Research settled two things the spec had wrong or open. The upstream
record for a pinned version **is** mutable and carries its own one-hour
`Cache-Control`, so the cache must expire (R1). And the batch API offers
**no field mask**, so FR-016 as written could not be satisfied against
this service (R3); it was flagged rather than reinterpreted, and has
since been reworded from "request" to "retain or persist".

A later clarification round also resized batches from the 5000 ceiling to
~500 (FR-005a): at the ceiling a large scan is two requests and the
progress count freezes through each one, which would have had US1's
batching silently defeat US2's observability while every test passed.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain. No nightly.
**Primary Dependencies**: Existing only — `reqwest` (already the deps.dev
transport), `tokio` (`JoinSet`, already used for concurrent deps.dev
fetches at `deps_dev_graph.rs:135`), `serde`/`serde_json`, `tracing`,
`anyhow`, `clap`. **Zero new Cargo dependencies.**
**Storage**: `$HOME/.cache/waybill/deps-dev/`, sibling to the existing
`$HOME/.cache/waybill/clearly-defined/` (R5).
**Testing**: `cargo +stable test --workspace`. Batch parsing, pagination
termination, identity matching and cache expiry are unit-testable against
recorded fixtures; no network in tests.
**Target Platform**: All three host platforms; nothing platform-specific.
**Project Type**: Single Rust workspace; changes confined to
`waybill-cli/src/enrich/` plus flag definitions in `cli/scan_cmd.rs`.
**Performance Goals**: SC-001 ≤2 min enrichment for ~7,500 components
(from ~17 min); SC-002 ≥99% fewer requests; SC-008 the non-bulk default
materially faster than 17 min.
**Constraints**: FR-003b — concurrency ceiling stays at the existing
`CONCURRENT_REQUESTS = 8`; deps.dev publishes no rate limit, so there is
no advertised allowance to tune against (R4). FR-012a — freshness comes
from the response, not from a constant.
**Scale/Scope**: Service ceiling is 5000 entries per batch (R2); waybill
sends ~500 (FR-005a), so a 7,592-component scan is ~16 concurrent
requests rather than 7,592 serial ones.

## Constitution Check

*GATE: must pass before Phase 0. Re-checked after Phase 1.*

| Principle | Assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | PASS. Zero new dependencies; nothing new links. |
| **II. eBPF-Only Observation** | N/A. No observation path touched. |
| **III. Fail Closed** | PASS. FR-004/004a/017 require the scan to complete with reduced enrichment rather than fail; FR-012c refuses to serve an entry past its bound rather than guess it is still good. |
| **IV. Type-Driven Correctness** | PASS. `clippy::unwrap_used` applies; the `nextPageToken` trap (R2) is exactly the class of bug a typed wrapper should make unrepresentable — see C-2. |
| **V. Specification Compliance** | PASS. No emitted-format change. |
| **VI. Three-Crate Architecture** | PASS. All changes inside `waybill-cli`. |
| **VII. Test Isolation** | PASS, with care. The disk cache is per-user shared state, so tests MUST point it at a per-test temp dir, never `$HOME`. Mirrors how `clearly_defined_disk_cache` tests already work. |
| **VIII. Completeness** | PASS. FR-006 exists precisely to stop partial pagination from under-enriching silently. |
| **IX. Accuracy** | PASS, and R1 is why. A never-expiring cache would have served stale licence data indefinitely with no signal, which is an accuracy violation dressed as a performance win. |
| **X. Transparency** | PASS as of FR-017a. See the gate finding below, which is now closed. |
| **XI. Enrichment** | PASS as of FR-017a. |
| **XII. External Data Source Enrichment** | PASS for XII.1/XII.3/XII.4. XII.2's provenance channel is unchanged by this feature; whether a cached value's *retrieval time* belongs in that annotation is recorded as open, low-impact. |

### Gate finding (RESOLVED): degraded enrichment must be annotated

Principle XI: *"If an enrichment source is unavailable, the SBOM MUST
still be emitted with the enrichment fields omitted **and a transparency
annotation (Principle X) noting the gap**."* Principle XII.3 repeats it.

The spec requires the scan to survive (FR-004, FR-017) and SC-006 says it
must not "silently under-enrich without saying so" — but no functional
requirement mandates the annotation, and SC-006's "without saying so" is
the only place the obligation appears at all. Three of this feature's own
edge cases produce exactly the degraded state the constitution is talking
about: batch endpoint unavailable, upstream throttling, enrichment wholly
unavailable.

This was a constitutional MUST with no requirement behind it. Recorded
here rather than resolved here, since amending the spec is not the plan's
call — and **subsequently resolved** in the clarify session of
2026-09-12 as **FR-017a/b/c**: a document-scope annotation naming the
degradation mode and the affected-component count, with every mode
recorded when more than one occurs.

Implementation obligation this creates: a new document-scope annotation
needs a matching row in `docs/reference/sbom-format-mapping.md` **and** a
matching entry in `waybill-cli/src/parity/extractors/mod.rs::EXTRACTORS`
with a per-format arm in that directory's `cdx.rs`, `spdx2.rs` and
`spdx3.rs`, or `every_catalog_row_has_an_extractor`
(`extractors/mod.rs:725`) and `holistic_parity` both fail. Adding the doc
row ahead of the emission code is the known way to break the build here.

(An earlier draft of this plan gave the path as
`waybill-cli/src/generate/parity/…`, which does not exist. It was taken
from a note rather than from the tree; `/speckit-analyze` caught it.)

A second, smaller question falls out of caching: XII.2 requires data be
annotated with its provenance ("license from deps.dev"). Data served from
a local cache is still from deps.dev, but it was retrieved at some earlier
time. Whether the retrieval time belongs in the annotation is a judgment
the spec should make rather than the implementation.

**Gate result: PASS with one recorded gap.** No violation requires a
Complexity Tracking entry — the gap is a missing requirement, not an
architectural exception.

## Project Structure

### Documentation (this feature)

```text
specs/839-batch-enrichment/
├── spec.md              # complete; 3 clarifications integrated
├── plan.md              # this file
├── research.md          # R1–R7, complete
├── data-model.md        # request, record, cache entry, progress, batch page
├── quickstart.md        # operator-facing: flags, cache, measurement
└── contracts/
    ├── batch-client.md      # C-1..C-4: chunking, pagination, matching, fallback
    └── enrichment-cache.md  # C-5..C-8: key, freshness, corruption, isolation
```

### Source Code (repository root)

```text
waybill-cli/src/enrich/
├── deps_dev_client.rs        # MODIFIED — batch method; drop advisory_keys (FR-016a);
│                             #   surface response Cache-Control for FR-012a
├── deps_dev_batch.rs         # NEW — chunking, pagination, identity matching
├── deps_dev_disk_cache.rs    # NEW — ports clearly_defined_disk_cache mechanics
├── depsdev_source.rs         # MODIFIED — concurrent loop replaces the :273 serial one
└── progress.rs               # NEW — time-triggered emitter (FR-009/009a)

├── request_key.rs            # NEW — shared request identity + per-ecosystem
│                             #   name normalisation (C-6.2)
└── degradation.rs            # NEW — degradation record for FR-017a

waybill-cli/src/cli/scan_cmd.rs   # MODIFIED — new flags; all subordinate to --offline

waybill-cli/src/parity/extractors/     # MODIFIED — mod.rs EXTRACTORS entry plus
    {mod,cdx,spdx2,spdx3}.rs           #   a per-format arm, for the FR-017a annotation
docs/reference/sbom-format-mapping.md  # MODIFIED — catalog row, same change as above

waybill-cli/src/generate/cyclonedx/metadata.rs   # MODIFIED — doc-scope annotation emission
waybill-cli/src/generate/spdx/annotations.rs     #   (SPDX 2.3)
waybill-cli/src/generate/spdx/v3_annotations.rs  #   (SPDX 3)
```

## Phase sequencing

Ordered so each step is independently verifiable and nothing depends on
an unreviewed predecessor:

1. **Progress first (US2).** It is independent of both other stories,
   and shipping it first means every later measurement is observable
   while it runs rather than inferred afterwards.
2. **Concurrency (US1, FR-003a).** Fixes the default and the fallback.
   Measurable on its own against the 17-minute baseline.
3. **Batching (US1, FR-001).** Behind the flag. Builds on a concurrent
   path that already works, so a batch failure falls back to something
   fast rather than to the original bug.
4. **Disk cache (US3).** Last, because it is P2 and because its value is
   easiest to misjudge before steps 2 and 3 have moved the baseline.
5. **`advisory_keys` removal (FR-016a).** Independent; can land anywhere,
   but naturally belongs with step 4 since it is the field whose caching
   would be most obviously wrong.

Steps 2 and 3 both claim SC-001. Measuring them separately is the only
way to know which one earned it — and the m772 lesson recorded in
`docs/development/perf-methodology.md` is that flag-toggle attribution
lumps untoggled phases together and sends a whole cycle at the wrong
subsystem.

## Complexity Tracking

No constitutional violations requiring justification. Table intentionally
empty.
