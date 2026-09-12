# Feature Specification: Batched, observable dependency enrichment

**Feature Branch**: `839-batch-enrichment`
**Created**: 2026-09-11
**Status**: Draft
**Input**: User description: "Batch deps.dev version enrichment behind a flag, with progress output and a local disk cache, so large scans stop reading as hangs (issue #766)"

## Clarifications

### Session 2026-09-12

- Q: Is concurrency in scope, or only batching? → A: Both — add bounded concurrency to the per-component path as well (Option A), so the fallback and the no-flag default both improve independently of an unstable upstream surface.
- Q: What bounds cache entry lifetime? → A: Honour the response's own `Cache-Control: max-age` (Option A), defaulting to one hour when the header is absent, with an operator flag to extend deliberately.
  - Investigated rather than assumed. An earlier draft of this spec asserted that a pinned version's metadata is immutable. **That premise is false** and the evidence is recorded under Assumptions: deps.dev serves `cache-control: public, max-age=3600` on a fully pinned version record, states that it re-scans packages continuously, offers no `ETag`/`Last-Modified` for cheap revalidation, and derives Go licences from a scanner whose output moves when the scanner does.
- Q: FR-016 cannot be satisfied against this API — how should it read? → A: Reword from "MUST NOT request" to "MUST NOT retain or persist" (Option A). deps.dev offers no field mask on GetVersion or GetVersionBatch, so what is sent is the service's choice; what is kept is waybill's.
- Q: What does progress report while a batch is in flight? → A: Size batches below the service ceiling and issue them concurrently (Option D). **The size was subsequently corrected from ~500 to 100 by measurement** — see below.
  - The original ~500 was reasoned from progress granularity and blast radius, without measuring. Measurement (2026-09-12, `measurements/`) found the real constraint is different and stronger: the batch endpoint **pages at exactly 100**, and pages are serial because each needs the previous page's token. A 500-entry batch is five sequential round-trips and measures ~3× slower than five concurrent 100-entry batches; a 5000-entry batch is fifty, and is slower still. The direction of the original answer was right; the number and the reason were both wrong.
- Q: Must degraded enrichment be annotated in the SBOM? → A: Yes, document-scope (Option B) — one annotation per scan naming the degradation mode and the count of affected components. Closes a Principle XI / XII.3 MUST that no requirement previously carried.
- Q: Is progress output triggered by work count or by elapsed time? → A: Elapsed time (Option A) — the reported defect is silence over time, not volume of work, so the trigger is measured in the same unit as the complaint. FR-009 and US2's scenarios are reworded from work-count to elapsed time, resolving a tension with SC-004.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A large scan finishes in a reasonable time (Priority: P1)

An operator scans a large monorepo without opting out of network enrichment. Today waybill contacts the upstream metadata service once per component, in sequence. On a 7,592-component repository that is 7,592 sequential network round-trips and roughly 17 minutes of wall time. The operator sees no output during that period and reasonably concludes the tool has hung, so they kill it and get no SBOM at all.

After this change, waybill groups those lookups into a small number of batch requests. The same scan completes in a time the operator will wait for, and the enrichment data — licences and source links — is unchanged.

**Why this priority**: This is the reported defect (#766). It converts a scan that produces nothing into one that produces a fully enriched SBOM. It is also the only story that changes what an operator ultimately *gets* rather than what they *see*.

**Independent Test**: Scan a repository with several thousand components with enrichment enabled and the batch path active. Confirm the emitted SBOM carries the same licence and source-link coverage as the per-component path, and that wall time falls by at least the SC-001 ratio against the T001 baseline.

**Acceptance Scenarios**:

1. **Given** a repository whose scan yields several thousand enrichable components, **When** the operator scans with enrichment enabled and the batch path active, **Then** an SBOM is produced whose licence coverage and source-reference count match the per-component path for the same input.
2. **Given** the same repository, **When** the operator compares wall time between the batch path and the *sequential* per-component path as it behaved before this change, **Then** the batch path meets the SC-001 ratio. The baseline is named explicitly because FR-003a also speeds up the per-component path, and comparing against the improved one would understate the batch gain while appearing to fail the criterion.
3. **Given** a scan where the metadata service returns data for only some of the requested components, **When** enrichment completes, **Then** components with data are enriched, components without are left unenriched, and neither case fails the scan.

---

### User Story 2 - The operator can see that work is happening (Priority: P1)

An operator runs any scan whose enrichment phase takes more than a few seconds. waybill reports what it is doing as it goes, so a slow phase is legible as progress rather than as a stall.

Today the final line printed before enrichment is `scan complete`, after which the process is silent until the SBOM is written. That silence is what turned a slow scan into a bug report: the reporter's log ends exactly where enrichment begins.

**Why this priority**: Equal to Story 1, because throughput alone does not fix the reported experience. Any phase that can run long without output will be read as a hang on a large enough input, and the batch path does not eliminate that possibility — it lowers the threshold at which it occurs. Shipping Story 1 without Story 2 leaves the same failure mode waiting for a bigger repository.

**Independent Test**: Run a scan with enrichment enabled and observe the output stream. Progress must be visible without waiting for the phase to finish, and must convey how much work remains.

**Acceptance Scenarios**:

1. **Given** a scan whose enrichment phase runs longer than the first-emission delay, **When** enrichment runs, **Then** waybill emits progress showing work completed against work total, and continues to do so at intervals no longer than that delay until the phase ends.
2. **Given** a scan whose enrichment phase completes within the first-emission delay, **When** enrichment runs, **Then** waybill emits no progress output at all. A phase nobody waited on needs no reassurance that it happened.
3. **Given** an operator who has redirected output to a file, **When** the scan runs, **Then** progress output does not corrupt or interleave badly with the rest of the diagnostic stream.

---

### User Story 3 - A repeated scan does not re-fetch unchanged data (Priority: P2)

An operator scans the same repository repeatedly — in a CI loop, while iterating on flags, or across several output formats. Metadata for a *pinned* package version does not change, so waybill reuses what it already retrieved instead of asking again.

**Why this priority**: Lower than P1 because it improves the repeat case rather than fixing the reported defect, and the first scan on a fresh machine is unaffected. It is genuinely valuable — repeated scans are the common development loop — and there is direct precedent in the sibling enricher, which already keeps an on-disk cache.

**Independent Test**: Scan a repository twice with enrichment enabled. The second scan performs materially fewer network requests than the first and produces an identical SBOM.

**Acceptance Scenarios**:

1. **Given** a repository scanned once with enrichment enabled, **When** it is scanned again, **Then** the second scan issues substantially fewer network requests and emits an SBOM with identical enrichment content.
2. **Given** a *fresh* cache entry for a package version, **When** that same version is requested again, **Then** the cached value is used without a network request. **Given** an entry past its freshness bound, **Then** it is re-fetched — the package version is pinned, but the upstream *record about* it is re-derived continuously and is not immutable.
3. **Given** an unreadable, corrupt or partially-written cache, **When** a scan runs, **Then** waybill falls back to fetching and the scan succeeds.
4. **Given** an operator who wants a cold measurement, **When** they request it, **Then** the cache can be bypassed or cleared.

---

### Edge Cases

- The batch endpoint is unavailable, returns an error, or returns a malformed body. Enrichment must degrade to the concurrent per-component path (FR-003a) rather than failing the scan or silently dropping enrichment for every component. Degrading to the *sequential* path would reproduce the defect this feature exists to fix, since the upstream surface is explicitly unstable. This is a degraded state and MUST be annotated per FR-017a.
- A batch request exceeds the service's documented maximum entries per request. Requests must be chunked so this cannot occur, including on repositories far larger than any currently tested.
- A batch response is paginated. All pages must be consumed, or the scan will silently under-enrich — a failure that produces a plausible-looking SBOM and is therefore worse than an error.
- A batch response omits entries that were requested, or returns them in a different order. Responses must be matched to requests by identity rather than by position.
- The upstream service throttles or rejects requests issued in parallel. Concurrency introduces a failure mode sequential fetching did not have, so the concurrent path must bound its own request rate and treat a throttling response as retryable rather than as an enrichment failure. Throttling that survives retry is a degraded state and MUST be annotated per FR-017a.
- The operator passes the flag that disables network access. No enrichment request of any kind may be issued, batch or otherwise.
- An upstream response carries no `Cache-Control` directive, or one that cannot be parsed. The one-hour default applies; enrichment must not fail and must not fall back to treating the entry as permanent.
- The scan is interrupted mid-enrichment. No partial or corrupt cache state may be left behind that would poison a later scan.
- A scan produces zero enrichable components. The phase must be a no-op without emitting progress output or making requests.
- Enrichment data differs between the batch and per-component paths for the same package version. The two paths must be verified to agree, since a silent divergence would make SBOM content depend on which path happened to run.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: waybill MUST retrieve version metadata for multiple components in a single upstream request when the batch path is active.
- **FR-002**: The batch path MUST be selectable by the operator via an explicit flag, and MUST NOT be the default until it has been exercised against real corpora.
- **FR-003**: waybill MUST retain the existing per-component retrieval path, and MUST use it when the batch path is not selected.
- **FR-003a**: The per-component retrieval path MUST issue its requests concurrently, under a bounded limit, rather than strictly in sequence. This path is the default (FR-002) and the fallback (FR-004), so leaving it sequential would leave the reported defect reachable by both routes.
- **FR-003b**: The concurrency limit MUST be bounded and MUST NOT be raised to a level that risks upstream throttling in pursuit of the SC-001 target. deps.dev publishes **no** rate limit and **no** documented throttling semantics, so the ceiling must be chosen conservatively by waybill rather than tuned up against an advertised allowance.
- **FR-004**: When a batch request fails, waybill MUST fall back to the per-component path for the affected components and complete the scan successfully.
- **FR-004a**: The fallback MUST use the concurrent per-component path, not a sequential one.
- **FR-005**: waybill MUST split batch requests so that no single request exceeds the upstream service's documented maximum number of entries. This is a hard service limit, not a tuning parameter — exceeding it is rejected outright.
- **FR-005a**: Batch size MUST be **100 entries** — the observed response page size. Above it the service returns 100 results plus a continuation token, and because each page requires the previous page's token, oversized batches serialise into sequential round-trips rather than saving them. Measured: batch=100 completes a workload ~3× faster than batch=500 and ~4× faster than batch=5000 (`measurements/`). Sizing at the page boundary also keeps the completed count moving often enough to satisfy US2, but that is a consequence, not the reason.
- **FR-005a-i**: The page size is **observed behaviour, not a documented contract** — deps.dev publishes no page size for `GetVersionBatch`. Implementations MUST re-verify it rather than assume it, and MUST remain correct if it changes (see FR-006).
- **FR-005b**: Batches MUST be issued concurrently, under the same bounded ceiling as the per-component path (FR-003b), so that finer chunking costs no wall-clock time relative to fewer, larger requests.
- **FR-005c**: A failed batch MUST cost only the components in that batch. Smaller batches are therefore also a blast-radius decision, not only a progress one: under FR-004a a failure falls back for 500 components rather than 5000.
- **FR-006**: waybill MUST consume all pages of a paginated batch response before considering enrichment complete. At the FR-005a size this path should never execute — it exists because the page size is undocumented and may change, and because stopping early produces a plausible-looking SBOM that is quietly under-enriched. A defensive path that is never exercised is a path that rots, so it MUST be covered by a test that forces pagination rather than left to chance.
- **FR-007**: waybill MUST match batch responses to their requests by package identity, not by ordering.
- **FR-008**: Enrichment content produced by the batch path MUST be identical to that produced by the per-component path for the same set of package versions.
- **FR-009**: waybill MUST emit progress output during enrichment, reporting completed and total work, once the phase has been running longer than a first-emission delay of 10 seconds. The trigger is **elapsed time, not work count**: the defect being fixed is an operator watching silence, and a small component count behind a slow or throttled endpoint produces exactly the same silence as a large one.
- **FR-009a**: Once emission has begun, waybill MUST continue to emit at intervals no longer than the first-emission delay, until the phase ends. A single line followed by renewed silence would reproduce the defect after a longer fuse.
- **FR-010**: waybill MUST NOT emit enrichment progress output when there is no enrichment work to do, nor when the phase completes within the first-emission delay.
- **FR-011**: waybill MUST persist retrieved version metadata across scans in a local cache.
- **FR-012**: waybill MUST use a cached entry in preference to a network request for the same package version, provided the entry is still within its freshness bound.
- **FR-012a**: The freshness bound MUST be taken from the upstream response's own `Cache-Control: max-age` directive, and MUST default to one hour when that directive is absent or unparseable. Reading the service's stated policy is more durable than any interval chosen here, and costs nothing.
- **FR-012b**: waybill MUST provide an operator flag to extend the freshness bound beyond what the upstream response specifies, for operators who knowingly prefer speed over currency. Extending MUST be an explicit act, never a default.
- **FR-012c**: An entry past its freshness bound MUST be treated as a miss and re-fetched. Serving it would silently substitute stale licence data for current data.
- **FR-013**: waybill MUST treat an unreadable or corrupt cache as a cache miss and continue the scan.
- **FR-014**: waybill MUST provide a way for an operator to bypass or clear the local cache.
- **FR-015**: waybill MUST NOT issue any enrichment request — batch, per-component, or cache-refresh — when network access is disabled.
- **FR-016**: waybill MUST NOT **retain or persist** upstream fields it does not consume. The earlier wording said "MUST NOT request", which this upstream makes impossible: deps.dev publishes no field mask on `GetVersion` or `GetVersionBatch`, so the full record arrives regardless — and the v3alpha record is larger than the v3 one. What is sent is the service's choice; what is kept is waybill's, and with a disk cache that choice becomes durable rather than momentary.
- **FR-016a**: The `advisoryKeys` field MUST NOT be requested or retained. It is deserialised today at `deps_dev_client.rs:21` and referenced nowhere outside test fixtures, so it already violates FR-016. It is also the field most obviously mutable after publication, so caching it would create staleness risk for data that reaches no output.
- **FR-017**: The scan MUST succeed when enrichment is wholly unavailable, emitting an SBOM without enrichment rather than failing.
- **FR-017a**: When the enrichment phase completes in a degraded state, waybill MUST emit a **document-scope** transparency annotation naming the degradation mode — batch endpoint unavailable, upstream throttled, or enrichment wholly unavailable — and the number of components left unenriched as a result. Constitution Principles XI and XII.3 both require a transparency annotation when an enrichment source degrades; before this requirement, nothing in the spec carried that obligation and a degraded run was indistinguishable from a clean one except by a component count nobody checks.
- **FR-017b**: The annotation MUST be document-scope, not per-component. It records one fact about the run; attaching it to every affected component would restate that fact thousands of times.
- **FR-017c**: A scan that degrades in more than one mode MUST record every mode that occurred, not only the first or the last. A run that was throttled *and* lost the batch endpoint is not adequately described by either alone.

### Key Entities

- **Enrichment request**: the identity of one package version for which metadata is sought — ecosystem, name, version.
- **Enrichment record**: the metadata retrieved for one package version. Today this is licences and source links. Anything not consumed by emission is out of scope per FR-016.
- **Cache entry**: a stored enrichment record keyed by package version identity, carrying the time it was retrieved and the freshness bound that applied. Not immutable: the artefact is pinned, the upstream record about it is not.
- **Progress report**: completed and total enrichment work at a point in time.
- **Degradation record**: the set of degradation modes encountered during the enrichment phase and the count of components left unenriched by them. One per scan; emitted as a document-scope annotation. Empty means the phase was not degraded, and is emitted as nothing rather than as an explicit "no degradation" marker.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With the dependency-graph path disabled (`--no-deps-dev-graph`), a scan completes enrichment at least **20× faster** with the batch path than sequentially. **MEASURED: 39.35×** (T029; 30,140ms → 766ms, medians of 3 and 5 samples, `batched=709 fell_back=0`, coverage identical at 584 licensed / 2264 externalRefs).
- **SC-002**: The number of upstream requests for such a scan falls by at least 98% compared with the per-component path. At the FR-005a size of 100, roughly 7,500 components becomes about 76 requests rather than 7,500 — a 99.0% reduction — so the criterion holds with margin at the size measurement selected.
- **SC-003**: Licence coverage and source-reference counts for such a scan are identical between the batch and per-component paths.
- **SC-004**: An operator watching a scan of that size sees evidence of progress within 10 seconds of enrichment beginning, and thereafter at intervals no longer than 10 seconds.
- **SC-005**: A second scan of an unchanged repository issues at least 90% fewer upstream requests than the first.
- **SC-006**: Every failure mode in Edge Cases yields a completed scan — enrichment may be reduced or absent, but the scan does not fail. "Does not silently under-enrich" is discharged by FR-017a: for each such failure mode, the emitted SBOM carries a document-scope annotation naming the mode and the affected-component count, so a degraded run is distinguishable from a clean one by inspection rather than by comparing component counts against an expectation nobody holds.
- **SC-007**: No scan issues an upstream request when network access is disabled.
- **SC-009**: No scan serves cached enrichment data older than the freshness bound the upstream response specified, unless the operator explicitly extended it.
- **SC-008**: Under the same conditions as SC-001, a scan with the batch path NOT selected — the default, and the fallback — completes enrichment at least **5× faster** than the sequential path. **MEASURED: 8.55×** (T029; enrichment phase 30,140ms → 3,524ms, medians of 3 and 5 samples, coverage identical). This criterion was briefly lowered to ≥3.5× on the strength of a figure produced by subtracting an unstable floor; the floor moved between 5.13s and 23.53s across runs on this repository, and three successive speedup figures derived from it were wrong. Measuring the phase directly restored the original bar.

## Assumptions

- The upstream service's batch interface is currently published under an unstable version label. Story 1 is therefore gated behind an operator flag (FR-002) and paired with a fallback (FR-004) so an upstream change degrades rather than breaks. Promoting it to the default is out of scope for this feature and should follow evidence from real use.
- "Progress output" means the existing diagnostic stream, not a new interactive UI. Issue #607 proposes per-phase progress diagnostics more broadly; this feature covers the enrichment phase only and should not foreclose that wider design.
- The local cache stores only data already retrievable from the public upstream service. It is a latency optimisation, not a new source of truth.
- **Upstream records are mutable, and this was verified rather than assumed** (2026-09-12). A pinned version record is served with `cache-control: public, max-age=3600`; the deps.dev FAQ states the service "re-scans each package at a constant rate", that data is fresh "to within an hour or so" for common packages and **staler for quiescent ones**, and that "there is no mechanism for users to trigger an update". No `ETag` or `Last-Modified` is offered, so change cannot be detected cheaply — only by re-fetching. Go licences are derived with the `licensecheck` scanner, so a pinned module's licence string can change when the scanner changes, with no change to the module.
- A *published, shared* cache or API — one operated by the project and consumed by others — is **out of scope for this feature but anticipated as future work**. It would make the project a supply-chain intermediary for licence facts, which needs its own design and governance discussion; it should not arrive as a side effect of a performance fix. The local cache specified here should therefore be built so its storage shape could later be served, rather than in a form that would have to be discarded.
- **The end-to-end gain is smaller than the licence-path gain, and the base scan is the floor.** Measured at the request layer, batching resolves 709 coordinates in 0.43s against 42.84s sequential — ~100×. But a scan also walks, parses and emits, which costs ~0.48s and no enrichment change touches it. So with the graph path disabled the projected end-to-end figure is ~1.0s against 38.33s, about 37×; with the graph path enabled it is ~7s against ~41s, about 6×. Quoting the ~100× as an end-to-end expectation would be the same category error as measuring a component and reporting it as a system.
- **The deps.dev dependency-graph path is out of scope, and becomes the dominant cost once this feature lands.** Measured (T001): the licence path this feature batches is 41.68s of a 41.11s scan; the graph path is a separate deps.dev call costing 5.87s. deps.dev publishes **no** batch method for dependencies — only `GetVersionBatch`, `GetFindingsBatch`, `GetProjectBatch` and `PurlLookupBatch`, and `…/dependenciesbatch` returns 404 — so the graph path cannot be batched at all. It already has an operator opt-out (`--no-deps-dev-graph`, shipped in m207), so no new flag is needed here. After batching, the graph path is roughly 93% of remaining enrichment time; making it faster (its concurrency is 8-way, and 16-way roughly doubled throughput in probing) is deliberately left to a separate piece of work.
- **Upstream cache state changes the baseline by ~10×, and every measurement must control for it.** Measured (T001, `baseline.md`): the identical scan of the identical tree with the identical binary takes **445.81s cold and 42.52s warm** — deps.dev serves `max-age=3600` from an edge, so a repeat scan inside the hour is served there. A comparison whose slow arm is cold and fast arm warm fabricates a ten-fold improvement out of nothing; this happened once while taking the baseline and voided a decomposition. Success criteria are therefore ratios against the **warm** figure, which is the reproducible one.
- **The ~17-minute baseline is not independently reproduced.** A clean-room measurement of the request path on 2026-09-12 (`measurements/`) extrapolates the sequential path to ~7 minutes for 7,592 components, not 17. The gap may be network, coordinate mix, upstream CDN warmth, or per-scan overhead outside the HTTP calls — it is unexplained. T001 settles it before any absolute figure is relied on; until then success criteria are expressed as ratios.
- The measurements cited throughout (7,592 components, ~17 minutes, 110 → 5,658 components with licences, 7,571 → 23,810 source references) come from a scan of the repository named in issue #766, taken on 2026-09-11 against the then-current `main`.
- The reported symptom in #766 is a hang. It is not: the scan completes in about 17 minutes. It is treated here as a defect of equal severity, because every operator kills it and therefore gets no SBOM.
