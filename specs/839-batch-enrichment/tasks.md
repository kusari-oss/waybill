# Tasks: Batched, observable dependency enrichment

**Feature**: `839-batch-enrichment` · **Issue**: #766
**Spec**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Format: `[ID] [P?] [Story] Description`

`[P]` = parallelisable (different file, no dependency on incomplete work).
`[USn]` = the user story the task serves. Setup, Foundational and Polish
carry no story label.

## Path Conventions

All paths are repository-relative. Production code lives under
`waybill-cli/src/enrich/`; CLI flags in `waybill-cli/src/cli/scan_cmd.rs`.

## Note on story ordering

US2 (progress) is implemented **before** US1 (throughput) even though both
are P1. Progress is independent of both other stories, and shipping it
first means every later measurement is observable while it runs rather
than reconstructed afterwards. This is the plan's phase sequencing, not a
re-prioritisation — US1 remains the reported defect.

---

## Phase 1: Setup

- [ ] T001 Record the pre-change enrichment baseline — wall-clock time and upstream request count — for a repository of roughly 7,500 enrichable components, and write the numbers into `specs/839-batch-enrichment/baseline.md`. Every SC-001/SC-002/SC-008 claim is measured against this, and reconstructing it after the code changes is impossible.
- [ ] T002 Add the new CLI flags to `waybill-cli/src/cli/scan_cmd.rs`: `--enrich-batch`, `--enrich-cache-max-age <secs>`, `--enrich-no-cache`. Document each as subordinate to `--offline`, matching the existing `--no-deps-dev*` doc comments at `scan_cmd.rs:972-1056`. All three live in one file, so this is one task rather than three parallel ones.

---

## Phase 2: Foundational (blocking prerequisites)

- [ ] T003 [P] Create the shared enrichment-request key in `waybill-cli/src/enrich/request_key.rs`, carrying system, name and version with per-ecosystem name normalisation (PyPI per PEP 503, NuGet lowercased, Maven `group:artifact`). Both the batch path and the per-component path must build this identically, or they will populate and miss different cache entries for the same package (contract C-6.2).
- [ ] T004 [P] Unit-test per-ecosystem normalisation in `waybill-cli/src/enrich/request_key.rs`. A normalisation bug surfaces as a response entry with no data — indistinguishable from "deps.dev does not carry this package" — so it degrades silently and needs its own test rather than being caught downstream.
- [ ] T005 [P] Remove `advisory_keys` from `VersionInfo` in `waybill-cli/src/enrich/deps_dev_client.rs:21` and from the three test fixtures in `waybill-cli/src/enrich/depsdev_source.rs:412,427,457` (FR-016a). It is parsed and never read; caching it would persist the field most obviously mutable after publication for data that reaches no output.
- [ ] T006 Create the degradation record in `waybill-cli/src/enrich/degradation.rs` — the set of modes encountered (batch-unavailable, throttled, wholly-unavailable) plus the count of components left unenriched (FR-017a/b/c). Multiple modes in one run must all be recorded, not just the first.
- [ ] T007 Add the document-scope annotation for the degradation record: a new row in `docs/reference/sbom-format-mapping.md` **and** the matching entry in `waybill-cli/src/parity/extractors/mod.rs::EXTRACTORS`, plus a per-format arm in `waybill-cli/src/parity/extractors/cdx.rs`, `waybill-cli/src/parity/extractors/spdx2.rs` and `waybill-cli/src/parity/extractors/spdx3.rs` — **all in the same change**. Adding the catalog row ahead of the extractors fails `every_catalog_row_has_an_extractor` (`waybill-cli/src/parity/extractors/mod.rs:725`) and `holistic_parity`.
- [ ] T008 Emit the degradation annotation at document scope across all three formats, following the wiring an existing doc-scope annotation already uses: CDX in `waybill-cli/src/generate/cyclonedx/metadata.rs` (wired from `builder.rs`), SPDX 2.3 in `waybill-cli/src/generate/spdx/annotations.rs` (wired from `document.rs`), SPDX 3 in `waybill-cli/src/generate/spdx/v3_annotations.rs` (wired from `v3_document.rs`). Emit nothing when the phase was not degraded; an explicit "no degradation" marker is noise on every clean scan.
- [ ] T009 [P] Test the degradation annotation in `waybill-cli/src/enrich/degradation.rs`: a single mode, two modes in one run (FR-017c), and the empty case emitting nothing.

**Checkpoint**: `cargo +stable test --workspace` green. Nothing below depends on an un-normalised key or an unannotated degradation path.

---

## Phase 3: User Story 2 — The operator can see that work is happening (P1)

**Goal**: A slow enrichment phase reads as progress, not as a stall.

**Independent test**: Run a scan with enrichment enabled and watch the
output. Progress appears before the phase ends and conveys how much
remains.

- [ ] T010 [US2] Create the time-triggered progress emitter in `waybill-cli/src/enrich/progress.rs`: first emission once the phase passes 10 seconds, then at intervals no longer than 10 seconds (FR-009/009a). The trigger is elapsed time, not completed count — a small component count behind a slow endpoint produces the same silence as a large one.
- [ ] T011 [P] [US2] Unit-test the emitter in `waybill-cli/src/enrich/progress.rs` with an injected clock: nothing emitted below the threshold, first line after it, and continued emission at interval. An injected clock rather than real sleeps, so the test is neither slow nor timing-flaky.
- [ ] T012 [US2] Wire the emitter into `enrich_components` at `waybill-cli/src/enrich/depsdev_source.rs:273`, reporting completed against a total known before the phase starts, at `tracing` INFO.
- [ ] T013 [US2] Ensure a zero-enrichable-component phase emits nothing at all (FR-010) — not a `0/0` line, which asserts that nothing happened.
- [ ] T014 [US2] Verify progress lands on stderr while the SBOM goes to `--output` (`main.rs:288-290` already routes `tracing` to stderr), satisfying US2 scenario 3 without a new writer.

**Checkpoint**: US2 is independently shippable here.

---

## Phase 4: User Story 1 — A large scan finishes in a reasonable time (P1)

**Goal**: The reported defect. A 7,592-component scan produces an SBOM
instead of being killed.

**Independent test**: Scan a several-thousand-component repository with
the batch path active; licence coverage and source-reference counts match
the per-component path, and wall time drops by an order of magnitude
against the T001 baseline.

### Block A — concurrency (FR-003a), fixes the default and the fallback

- [ ] T015 [US1] Replace the serial loop at `waybill-cli/src/enrich/depsdev_source.rs:273-295` with `chunks(CONCURRENT_REQUESTS)` + `tokio::task::JoinSet`, reusing the existing `CONCURRENT_REQUESTS = 8` from `waybill-cli/src/enrich/deps_dev_graph.rs:43` rather than introducing a second constant. deps.dev publishes no rate limit, so there is no advertised allowance to tune against (FR-003b).
- [ ] T016 [P] [US1] Test that enrichment content is byte-identical between the serial and concurrent paths for a fixed input set, in `waybill-cli/src/enrich/depsdev_source.rs`.
- [ ] T017 [US1] **Measurement checkpoint.** Record wall time and request count for concurrency alone against the T001 baseline, into `specs/839-batch-enrichment/baseline.md`. Both blocks of this phase claim SC-001; measuring them together makes it impossible to know which earned it — the failure mode recorded in `docs/development/perf-methodology.md`.

### Block B — batching (FR-001), behind `--enrich-batch`

- [ ] T018 [P] [US1] Define the batch request and response types in `waybill-cli/src/enrich/deps_dev_batch.rs` for `POST /v3alpha/versionbatch`, including `page_token` on the request and `next_page_token` on the response. **First re-verify the response page size** by running `python3 specs/839-batch-enrichment/measurements/pagecheck.py` (FR-005a-i): it was 100 on 2026-09-12, is undocumented, and FR-005a's chosen size depends on it. If it has moved, FR-005a moves with it.
- [ ] T019 [US1] Implement chunking in `waybill-cli/src/enrich/deps_dev_batch.rs` at **100** entries (FR-005a) — the observed response page size — with **5000** as a hard ceiling that is clamped or rejected, never sent (C-1.1/C-1.1a). Three numbers, easily conflated: 5000 is what the service accepts, 100 is what it returns per page, and 100 is therefore what waybill sends. Anything in between is accepted and then silently split into serial pages, measuring ~3–4× slower for identical coverage.
- [ ] T020 [P] [US1] Test chunking in `waybill-cli/src/enrich/deps_dev_batch.rs`: 1,001 requests produce eleven chunks at the default size, none exceeds it, and a configured size above 5000 is clamped or rejected. Assert the default equals the page size, so a future change to one forces a deliberate change to the other rather than a silent regression into pagination.
- [ ] T021 [US1] Implement the pagination loop in `waybill-cli/src/enrich/deps_dev_batch.rs`, continuing while `next_page_token` is **non-empty** and reusing the initial request body verbatim apart from `page_token` (C-2.1/C-2.3).
- [ ] T022 [P] [US1] Test pagination termination in `waybill-cli/src/enrich/deps_dev_batch.rs`. At the FR-005a size this path never runs in production, so it must be forced by a fixture — an unexercised defensive path rots, and this one exists only because the page size is undocumented and may move. deps.dev returns `"nextPageToken": ""` on the final page rather than omitting the field, so `Option<String>` yields `Some("")` and a presence check never terminates. Assert that a single-page fixture with an empty token stops after one request, and that a two-page fixture yields the union of both.
- [ ] T023 [US1] Match responses to requests by the echoed `responses[].request.versionKey`, never by array position (C-3.1). The echo is **uncanonicalized** per the API docs, so match against what was sent rather than re-deriving a key through waybill's own canonicalisation (C-3.2).
- [ ] T024 [P] [US1] Test identity matching in `waybill-cli/src/enrich/deps_dev_batch.rs` with a fixture mixing a hit, a miss (entry present, `version` absent) and a reordered response: exactly the hit is enriched, the miss is left alone, and neither fails the scan.
- [ ] T025 [US1] Issue batches concurrently in `waybill-cli/src/enrich/deps_dev_batch.rs`, under the same ceiling as the per-component path (FR-005b/C-1.1b), so finer chunking costs no wall-clock time relative to fewer, larger requests.
- [ ] T026 [US1] Implement fallback in `waybill-cli/src/enrich/deps_dev_batch.rs`: on transport error, non-200 or unparseable body, fall back to the **concurrent** per-component path, bounded by the same ceiling (C-4.1/4.2/4.3). Falling back to a sequential path would reproduce #766, which matters because `v3alpha` is documented as liable to change incompatibly.
- [ ] T027 [P] [US1] Test fallback in `waybill-cli/src/enrich/deps_dev_batch.rs`: an injected batch transport failure produces the same enrichment content as the per-component path for the same input.
- [ ] T028 [US1] Record the degradation mode from `waybill-cli/src/enrich/degradation.rs` (T006) in `waybill-cli/src/enrich/deps_dev_batch.rs` whenever the batch path falls back, and in `waybill-cli/src/enrich/depsdev_source.rs` when the phase degrades, so the emitted SBOM carries the FR-017a annotation.
- [ ] T029 [US1] **Measurement checkpoint.** Record batching into `specs/839-batch-enrichment/baseline.md` against both the T001 baseline and the T017 concurrency figure. Verify SC-001 (≤2 min), SC-002 (≥99% fewer requests — expect ~99.8%) and SC-003 (identical licence and source-reference counts) by comparison, not by assertion.

**Checkpoint**: the reported defect is fixed by both routes — flag on and flag off.

---

## Phase 5: User Story 3 — A repeated scan does not re-fetch unchanged data (P2)

**Goal**: A repeat scan within the freshness window issues far fewer
requests and produces an identical SBOM.

**Independent test**: Scan twice; the second issues ≥90% fewer requests
and emits identical enrichment content.

- [ ] T030 [P] [US3] Create the disk cache in `waybill-cli/src/enrich/deps_dev_disk_cache.rs` at `$HOME/.cache/waybill/deps-dev/`, porting the mechanics already solved in `waybill-cli/src/enrich/clearly_defined_disk_cache.rs`: recorded negatives (`:30`), corrupt-entry handling (`:113`), key-hash collision refusal (`:131`), expiry-on-read (`:138`), best-effort writes (`:149`).
- [ ] T031 [US3] Parse `Cache-Control: max-age` from the upstream response and store it **per entry** alongside `retrieved_at` (C-7.2/7.3), defaulting to one hour when the header is absent or unparseable. Storing it per entry means a deps.dev policy change needs no migration. This requires surfacing response headers through `waybill-cli/src/enrich/deps_dev_client.rs`, which currently discards them.
- [ ] T032 [P] [US3] Test expiry in `waybill-cli/src/enrich/deps_dev_disk_cache.rs`: an entry with a one-hour bound and a two-hour-old timestamp reads as a miss; the same entry at thirty minutes reads as a hit.
- [ ] T033 [US3] Cache recorded absences in `waybill-cli/src/enrich/deps_dev_disk_cache.rs` under the same expiry rules (C-7.6). Without this, the long tail of packages deps.dev does not carry is re-requested every scan — on a large repository, most of the traffic this feature exists to remove.
- [ ] T034 [US3] Make writes in `waybill-cli/src/enrich/deps_dev_disk_cache.rs` atomic via write-then-rename (C-8.3), so an interrupted scan cannot leave a partial entry a later scan would read as valid.
- [ ] T035 [P] [US3] Test failure handling in `waybill-cli/src/enrich/deps_dev_disk_cache.rs`: a deliberately corrupted entry yields a miss and a completed scan; an unwritable cache directory yields a completed scan.
- [ ] T036 [US3] Implement `--enrich-cache-max-age` to extend the bound **at fetch time**, raising the `max_age` written into new entries — never by overriding the freshness check at read time (C-7.5). The operator's choice to accept staleness is then recorded in the data rather than applied invisibly on every later read.
- [ ] T037 [US3] Implement `--enrich-no-cache` (bypass both read and write) and document cache clearing as removing the directory (FR-014).
- [ ] T038 [P] [US3] Ensure every cache test points at a per-test temporary directory and never at the real `$HOME` (C-8.5, Constitution Principle VII). A test touching the real cache shares mutable state with the developer's own scans and with every other test.
- [ ] T039 [US3] Verify `--offline` behaviour (C-5.1/5.2, FR-015, SC-007): cache reads are permitted, no request of any kind is issued, and an expired entry under `--offline` is simply a miss that leaves the component unenriched rather than triggering a fetch.
- [ ] T040 [US3] **Measurement checkpoint** (SC-005). Scan the same repository twice inside the freshness window and record both request counts into `specs/839-batch-enrichment/baseline.md`; the second must issue at least 90% fewer. Confirm the two SBOMs carry identical enrichment content — a cache that is fast because it is serving the wrong thing would otherwise pass on request count alone.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T041 Sweep every Edge Case in the spec and confirm each yields a completed scan carrying the FR-017a annotation where it degraded (SC-006): batch endpoint unavailable, oversized batch, paginated response, omitted or reordered entries, upstream throttling, `--offline`, interruption mid-enrichment, zero enrichable components. Tests T027/T035 cover two of these; the rest have no single owning task and would otherwise go unverified.
- [ ] T042 [P] Publish the operator documentation from `specs/839-batch-enrichment/quickstart.md` into `docs/`, including the explanation of why the cache expires after an hour — the one-hour default will otherwise read as an oversight rather than as deps.dev's own stated policy.
- [ ] T043 Verify SC-004 by observation and record the observed interval in `specs/839-batch-enrichment/baseline.md`: on a scan of ~7,500 components, progress appears within 10 seconds of enrichment beginning and thereafter at intervals no longer than 10 seconds, with the batch path active. This is the criterion that batching at the 5000 ceiling would have broken while every automated test stayed green.
- [ ] T044 Run the full pre-PR gate — `cargo +stable clippy --workspace --all-targets` and `cargo +stable test --workspace`, both clean — and enumerate the per-suite results rather than grepping for failures.

---

## Dependencies & Execution Order

### Phase dependencies

```
Setup (T001-T002)
   └─▶ Foundational (T003-T009)   ← blocks everything
          ├─▶ US2 progress (T010-T014)        independent
          ├─▶ US1 concurrency (T015-T017)     needs T003
          │      └─▶ US1 batching (T018-T029) needs concurrency for C-4.2 fallback
          └─▶ US3 cache (T030-T039)           needs T003 (key) + T031 (headers)
                 └─▶ Polish (T041-T044)
```

### User story dependencies

- **US2** is fully independent. It could ship alone.
- **US1 Block B depends on US1 Block A**, because contract C-4.2 requires the fallback target to be the concurrent path. Batching before concurrency would mean a batch failure falls back to the original defect.
- **US3 depends on T003** (shared key) and on **T031** (response headers, which the client discards today).

### Parallel opportunities

- T003, T004, T005 — three different files, no shared state.
- T011, T016, T020, T022, T024, T027 — all test tasks in files whose implementation task is already complete.
- T030 and T032/T035/T038 — cache implementation and its tests, once T003 lands.
- T042 can start any time after Phase 5.

### Sequencing constraint worth respecting

T017, T029 and T040 are measurement tasks, not implementation. Skipping them
leaves SC-001 attributable to "the work" rather than to concurrency or
batching specifically — and the recorded project failure in
`docs/development/perf-methodology.md` is exactly that: a full spec and
implementation cycle aimed at a subsystem that turned out not to be the
cost.

---

## Implementation strategy

**MVP**: Phases 1–3. Progress alone makes a slow scan legible, which is
half the reported experience, and it ships without touching the request
path at all.

**Second increment**: Phase 4 Block A. Concurrency fixes the default and
the fallback, with no dependency on an endpoint whose own documentation
says it may change incompatibly.

**Third**: Phase 4 Block B. Batching behind the flag, on top of a
concurrent path that already works.

**Fourth**: Phase 5. The cache is P2, and its value is easiest to
misjudge before the earlier phases have moved the baseline.


---

## Traceability

Every functional requirement maps to at least one task. Contract clauses
(C-n) cited in a task discharge the requirements those clauses implement.

| Requirement | Tasks |
|---|---|
| FR-001 batch retrieval | T018, T019, T025 |
| FR-002 flag-gated, not default | T002, T041 |
| FR-003 per-component path retained | T015 |
| FR-003a/b bounded concurrency | T015, T016 |
| FR-004 / FR-004a fallback, concurrent | T026, T027 |
| FR-005 service ceiling | T019, T020 |
| FR-005a batch size 100 (page boundary) | T018, T019, T020 |
| FR-005a-i re-verify page size | T018, T020 |
| FR-005b concurrent batches | T025 |
| FR-005c blast radius | T026 |
| FR-006 consume all pages | T021, T022 |
| FR-007 match by identity | T023, T024 |
| FR-008 batch ≡ per-component | T016, T027, T029 |
| FR-009 / FR-009a time-triggered progress | T010, T011, T012 |
| FR-010 silent when no work | T013 |
| FR-011 persistent cache | T030 |
| FR-012 / FR-012a/b/c freshness | T031, T032, T036 |
| FR-013 corrupt cache is a miss | T035 |
| FR-014 bypass or clear | T037 |
| FR-015 no request when offline | T039 |
| FR-016 / FR-016a retain nothing unused | T005 |
| FR-017 scan survives | T041 |
| FR-017a/b/c degradation annotation | T006, T007, T008, T009, T028 |

| Criterion | Verified by |
|---|---|
| SC-001 ≤2 min | T029 |
| SC-002 ≥99% fewer requests | T029 |
| SC-003 identical coverage | T029 |
| SC-004 progress within/every 10s | T043 |
| SC-005 repeat scan ≥90% fewer | T040 |
| SC-006 every failure mode completes | T041 |
| SC-007 nothing issued when offline | T039 |
| SC-008 non-batch path materially faster | T017 |
| SC-009 no stale data served | T032 |
