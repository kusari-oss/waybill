# Tasks: Say why deps.dev did not enrich a component

**Input**: `specs/1067-depsdev-enrichment-outcome/` (spec.md, plan.md, research.md, data-model.md, contracts/deps-dev-outcome.md, quickstart.md, measurements/)

**Tests**: requested (SC-001…SC-005). Write each test first and confirm it fails before implementing it. For enforcement tests, also show they fail with the feature switched off (the m1065/m1066 practice).

`E/` = `waybill-cli/src/enrich/`. Mock-server tests reuse `batch_tests`' helpers in `E/depsdev_source.rs` (`src(&server, batch)`, `keys(..)`, `entry(name, licence)`, around line 1261).

**Rules carried over from earlier milestones:**
- No new function named `walk_*` under `waybill-cli/src/scan_fs/` (the walker audit is name-based).
- Catalogue rows go on **one line**.
- Watch corpus runs by the run ID taken from the dispatch output.

## Phase 1: Setup

- [X] T001 Baseline: run `cargo test -p waybill --bin waybill enrich::` and `cargo test -p waybill --lib parity`, and record them green.

## Phase 2: Foundational — truthful absence (FR-010) and a three-way result

**Purpose**: today two batch-path defects record present components as absent (research R2). Every outcome below depends on "absent" being true.

- [X] T002 [P] In `E/depsdev_source.rs::batch_tests`, test **unanswered slot**: the mock batch response echoes only one of two requested keys. The unechoed key must not be cached as absent: after the call, the disk cache (`WAYBILL_DEPS_DEV_CACHE_DIR` set to a tempdir; see `persistence_tests` for the pattern) holds no `record: null` entry for it, and its result is not `Absent`.
- [X] T003 [P] In `batch_tests`, test **duplicate coordinate**: one chunk requests `serde@1.0.0` twice, and the mock answers it once with a licence. Both positions get the record, the request body holds the coordinate once, and no `null` is cached for it.
- [X] T003a [P] In `batch_tests`, test **merged spelling**: the mock batch answers `Flask` but not `flask` (the measured deps.dev behaviour). The `flask` position is retried with one per-key GET (mock `expect(1)`), and is never cached as absent.
- [X] T004 In `E/depsdev_source.rs`, introduce `enum LookupResult { Found(VersionInfo), Absent, Failed }` and make `fetch_many` return `Vec<LookupResult>`:
  - **per-key:** `Ok(Some)` → `Found`, `Ok(None)` (404) → `Absent`, `Err` → `Failed`;
  - **batch:** in `fetch_chunk_batched`, track one slot per *distinct* coordinate (`HashMap<key, Vec<usize>>`, never last-write-wins), and send each distinct coordinate once.
    - An echoed item with `version` → `Found`.
    - An echoed item without `version` → `Absent`.
    - An unanswered slot → retried with the per-key GET. This needs a per-slot fallback, not only today's per-chunk one. Measured rare: 0 of 925 corpus keys. It arises only from two spellings deps.dev merges, research R2. A failure of that GET → `Failed`.
  - **cache writes:** only `Found` and `Absent` reach the disk cache (disk format unchanged, research R4). The in-memory cache also keeps `Failed`, so a failure is neither re-fetched nor replayed as `Absent` later in the same scan (the pre-m1067 request count is preserved). `--offline` misses are `Unqueried`.
  - **callers:** adapt every caller (`enrich_components` and tests); content behaviour for `Found` is unchanged.
  - Run T002/T003 to green, plus all existing `enrich::` tests.
- [X] T005 [P] Add `pub(crate) fn is_placeholder_version(v: &str) -> bool` in `E/request_key.rs`: case-insensitive membership in `""`, `unknown`, `0.0.0-unknown`, `v0.0.0-unknown`, `noassertion`, `none`, `latest`. Add unit tests for every member, plus `0.0.0` → false.
  - **Do not** change the distribution-URL guard in `waybill-cli/src/scan_fs/mod.rs:2199` (analysis R1; its gap is filed separately).

**Checkpoint**: everything compiles, existing tests green, and absences are now only real.

## Phase 3: User Story 1 — a consumer can tell why (P1) 🎯 MVP

**Goal**: each component in the six ecosystems that deps.dev did not enrich carries C191 with the right outcome. The document carries C192.

**Independent test**: a mocked deps.dev serving one found, one 404, one `["non-standard"]`, one transport error and one unsupported-ecosystem component. The outcomes match the contract, in all three formats.

### Tests for User Story 1

- [ ] T006 [P] [US1] In a new test module `E/depsdev_source.rs::outcome_tests`, with the mock server, both `batch = true` and `batch = false`, run `enrich_components` over components and assert `extra_annotations["waybill:deps-dev-outcome"]`:
  - `cargo` found with `MIT` → none;
  - found with `licenses: []` → none (matched; FR-003);
  - 404 / batch item without version → `absent`;
  - `licenses: ["non-standard"]` → `declined-invalid-license`, and the string `non-standard` appears in no annotation value. Repeat for a component that already carries a lockfile licence: still `declined-invalid-license` (analysis U2);
  - transport error (mock 500 on GET; batch failure falling back to a GET 500) → `transport-failure`;
  - `pkg:deb/...` → none (FR-002a).
- [ ] T007 [P] [US1] Same module: a component looked up in two passes keeps only the final outcome. Simulate an initial pass absent, then a post-graph pass found → no annotation.
- [ ] T008 [P] [US1] Same module: with `DepsDevSource::new(client, /* offline */ true)` and with enrichment disabled, no component gets C191 (FR-007).
- [ ] T009 [P] [US1] Generate-layer test (new file `waybill-cli/tests/deps_dev_outcome_emission.rs`, or a unit test beside the C158 emission). A scan context with components carrying C191 plus one deb component, on an online run, must give:
  - **CycloneDX:** C191 on the right components, and C192 equal to `{"absent":1,"declined-invalid-license":1,"not-queried:unsupported-ecosystem":1,"transport-failure":1}` (keys sorted);
  - **SPDX 2.3 and SPDX 3:** the same values;
  - **offline or disabled:** neither C191 nor C192;
  - **the SC-005 invariant:** for every key except `not-queried:unsupported-ecosystem`, the C192 count equals the number of components carrying that C191 value (analysis C1).

### Implementation for User Story 1

- [ ] T010 [US1] In `E/depsdev_source.rs::enrich_components` / `apply_version_info`, classify each looked-up component:
  - `Found` with ≥1 licence string, all failing `SpdxExpression::try_canonical` → `declined-invalid-license`. Log the rejected strings at debug only.
  - Any other `Found` → matched: remove `waybill:deps-dev-outcome`.
  - `Absent` → `absent`.
  - `Failed` → `transport-failure`.

  Write the value into `extra_annotations` per pass, and only when the source is online (FR-007). Keep `matched`/`enriched` counters and the log line unchanged (FR-008).
- [ ] T011 [US1] Carry an "online deps.dev pass ran" flag to generate, mirroring how `enrichment_degraded` (C158) reaches it (`cli/scan_cmd.rs` around 3843/5009 → `generate/mod.rs:94`).
- [ ] T012 [US1] Compute and emit C192 `waybill:deps-dev-outcomes` at document scope in the three emitters, next to C158 (`generate/cyclonedx/metadata.rs` ~371, `generate/spdx/annotations.rs` ~538, `generate/spdx/v3_annotations.rs` ~556):
  - count C191 values over the final components, plus components whose PURL type is outside the six ecosystems as `not-queried:unsupported-ecosystem`;
  - include non-zero counts only; serialise as canonical JSON with sorted keys;
  - emit only when the flag from T011 is set and at least one count exists.
- [ ] T013 [US1] Parity rows in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs`: **C191** at component scope (the macro pattern of an existing per-component annotation row) and **C192** at document scope, both `SymmetricEqual`.
  - Add the new extractor functions to the explicit `use` lists in `mod.rs`.
  - Keep the extractor table sorted by `row_id` (the parity test enforces it).
- [ ] T014 [US1] Add C191 and C192 rows to `docs/reference/sbom-format-mapping.md`, **one line each**, after C190, from `contracts/deps-dev-outcome.md`. Run `cargo test -p waybill --lib parity` and `--test sbom_format_mapping_coverage`. Run T006–T009 to green; show T006 fails with T010's classification removed (local edit, reverted).

**Checkpoint**: US1 complete.

## Phase 4: User Story 2 — no requests for placeholder versions (P1)

**Goal**: placeholder versions are not sent to deps.dev, and are recorded as `not-queried:incomplete-coordinate`.

- [ ] T015 [P] [US2] Test in `outcome_tests`: components at `v0.0.0-unknown` (golang), `0.0.0-unknown` (cargo), `unknown` (maven) and `` (empty). The mock server receives **no** request for them (`expect(0)` on matching mocks), and each carries `not-queried:incomplete-coordinate`. A `0.0.0` component **is** queried.
- [ ] T016 [US2] In `E/request_key.rs::EnrichmentKey::from_purl_parts`, or at its call site in `enrich_components` (around `E/depsdev_source.rs:746-758`): when the ecosystem is one of the six and `is_placeholder_version(version)`, create no key and record `not-queried:incomplete-coordinate` on the component (online only). Keep the existing `None` for unsupported ecosystems. Run T015 to green.

## Phase 5: User Story 3 — unchanged when nothing is missing (P1)

- [ ] T017 [P] [US3] Test in `outcome_tests`: every component found (some with no new licence) → no C191, and the generate test from T009's harness shows no C192.
- [ ] T018 [US3] Verify byte-identity: the in-repo golden suites and `cargo test -p waybill --test public_corpus` pass unchanged locally. The goldens are offline, so FR-007 makes this hold by construction.

## Phase 6: Polish & Cross-Cutting

- [ ] T019 [P] Add an Unreleased entry to `CHANGELOG.md` covering:
  - C191/C192 and the outcome values;
  - placeholder versions no longer sent to deps.dev;
  - the two batch-path defects that cached present components as absent (anyone with an older disk cache: stale absences expire by `max_age`, default 1 hour);
  - offline scans unchanged.
- [ ] T020 Run `./scripts/pre-pr.sh > /tmp/prepr.log 2>&1; echo EXIT=$?`. Require `EXIT=0`, `Walker-audit allow-list check: OK`, the passed line, and no failing `test result`.
- [ ] T021 Push, then dispatch the read-only public-corpus run (run ID from the dispatch output). Expect no golden change (SC-004).
- [ ] T022 Live checks with the built binary:
  - run `measurements/probe_outcomes.sh` on opentelemetry-go and express, plus quickstart §1–§2;
  - append to `measurements/README.md`: placeholder requests 28 → 0 (SC-002), C192 counts equal the C191 tallies (SC-005), express has no C191.
- [ ] T023 Open the PR (closes #1058; references #1118), merge when green, then run `cargo clean`.

## Dependencies & Execution Order

- **Phase order:** Phase 1 → Phase 2 (T002–T005) → US1 (T006–T014) → US2 (T015–T016) → US3 (T017–T018) → Polish.
- **Measurements behind Phase 2:** the batch fallback design rests on two committed probes: `measurements/batch_echo.txt` and `echo_mismatch_corpus.txt` (analysis U1).
- **Why Phase 2 is first:** T004 changes `fetch_many`'s return type, which every later task builds on. T005 is independent of T004.
- **Story order:** US2 depends on US1's annotation plumbing (T010). US3 is verification only.

### Parallel opportunities

- **Phase 2:** T002 ∥ T003 (separate tests), then T004; T005 ∥ T004 (different files).
- **US1:** T006 ∥ T007 ∥ T008 ∥ T009 (separate tests); then T010 → T011 → T012 → T013 → T014.
- **Polish:** T019 ∥ T020 prep.

## Implementation Strategy

- **MVP:** Phase 2 and US1. The truthful-absence fixes plus the outcome signal.
- **Ship in one PR:** US2 and US3 come along. US2 is small, and it removes the dominant measured noise (28 of 30 misses on opentelemetry-go).
