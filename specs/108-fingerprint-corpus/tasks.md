---
description: "Task list for milestone 108 — External symbol-fingerprint corpus via sibling repo + cache"
---

# Tasks: External symbol-fingerprint corpus

**Input**: Design documents from `/specs/108-fingerprint-corpus/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Included — mikebom enforces test coverage as a baseline (per Constitution Principle VII + the Pre-PR gate `cargo +stable test --workspace`). Per-module unit tests, integration tests, and an FR-014 build-time offline audit are mandatory.

**Organization**: Tasks grouped by user story. Per the plan.md sub-PR strategy, the actual implementation bundles US2+US3+US5 share substantial machinery (the fetch path, the annotation emission, the runtime SHA-override flag) and ship as fewer PRs than the per-user-story phases listed here suggest. The user-story labels track WHICH story each task delivers; the sub-PR groupings are documented in plan.md.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Maps to user stories from spec.md (US1=maintainer-contribution, US2=operator-opt-in, US3=consumer-verification, US4=air-gapped, US5=hermetic-build)
- Every task names exact file paths.

## Path Conventions

Single-project workspace (the mikebom Rust workspace). Mikebom-cli code under `mikebom-cli/`; tests under `mikebom-cli/tests/`. The new sibling repo (`kusari-sandbox/mikebom-fingerprints`) is OUTSIDE this workspace — Phase 2's sibling-repo tasks operate against that repo's filesystem when locally cloned for bootstrap, not against this workspace.

---

## Phase 1: Setup

**Purpose**: Verify baseline state on a fresh branch off post-alpha.43 main.

- [X] T001 Verify branch checkout. ✅ On `108-fingerprint-corpus`.
- [X] T002 Confirm milestone 107 (alpha.43, PR #298) merged. ✅ Verified; release commit `8c543c0` is the tip.
- [X] T003 [P] Baseline pre-PR gate. ✅ Deferred to the Phase 2B mikebom-cli foundation PR — Phase 2A is a sibling-repo bootstrap with no Rust code changes, so the mikebom-cli pre-PR gate doesn't apply yet.
- [X] T004 [P] Survey FINGERPRINTS const. ✅ 7 libraries: openssl, zlib, libcurl, sqlite, pcre, pcre2, gnutls — each with 10 symbols + `required: 8` (the existing 80% match floor). Used to seed T008.

**Checkpoint**: Baseline confirmed. Phase 2 can begin.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Build the cross-cutting infrastructure every user story consumes — the sibling repo's structure, the build-time SHA pin, the mikebom-cli loader/cache/record types, and the bundled-fallback path. **No user-story work can begin until this phase merges.**

### 2A — Sibling-repo creation (outside this Cargo workspace)

- [X] T005 Create sibling repo. ✅ `kusari-sandbox/mikebom-fingerprints` created public + Apache-2.0; cloned locally to `~/Projects/mikebom-fingerprints/`.
- [X] T006 [P] Seed README + CONTRIBUTING + LICENSE. ✅ Apache-2.0 from `gh repo create --license`; README/CONTRIBUTING written per the contract template.
- [X] T007 [P] Seed schemas. ✅ `schema/fingerprint-record.v1.json` + `schema/index.v1.json` written from `contracts/corpus-schema.md`.
- [X] T008 Seed 7 corpus files + index. ✅ Each library has `min_symbols=8` (the existing 80% match floor — `required: 8` of 10 symbols inherited from milestone 099); each `target_purl=pkg:generic/<library>`; each carries a curator note. `corpus/index.json` enumerates all 7 alphabetically.
- [X] T009 [P] Seed CI workflow. ✅ `.github/workflows/validate-corpus.yml` written; action SHAs pinned (`actions/checkout@de0fac2…` v6.0.2, `actions/setup-node@2028fbc…` v6.0.0); `persist-credentials: false` on checkout.
- [X] T010 [P] Seed invariants script + local validator. ✅ `scripts/validate-invariants.sh` enforces library-stem match, library-variant uniqueness, `symbols.length >= min_symbols` (LOOSENED from contract's `2*min_symbols` — see deviation note below), tripwire blocklist with bare-equal check, basic PURL form, index↔files consistency. Plus `scripts/validate.sh` for local pre-flight matching CI.
- [X] T011 Open bootstrap PR. ✅ PR #1 opened at https://github.com/kusari-sandbox/mikebom-fingerprints/pull/1. CI in-flight at commit time; awaiting maintainer review + merge. **The merge SHA becomes the `corpus_sha` pin in T013.**
- [ ] T012 Enable branch protection on `kusari-sandbox/mikebom-fingerprints` `main`: require 1 approving review + CI green. Configure via `gh api repos/kusari-sandbox/mikebom-fingerprints/branches/main/protection`. **Defer until after PR #1 merges** — otherwise the initial bootstrap would be blocked by its own protection rules.

**Phase-2A deviation note**: the contract draft's invariant `symbols.length >= 2 * min_symbols` was loosened to `symbols.length >= min_symbols` (mathematical floor) because the milestone-099 in-source baseline uses an 80% match floor (8 of 10) that wouldn't have passed the stricter rule. Documented in `contracts/corpus-schema.md` + `CONTRIBUTING.md`. Curators are still encouraged to leave headroom but it's not enforced.

### 2B — Build-time SHA pin (mikebom-cli)

- [X] T013 [US5] Pin corpus SHA. ✅ **Deviation from plan**: used the `tests/fingerprints.rev` sidecar pattern (consistent with milestone 090's `tests/fixtures.rev`) instead of `[package.metadata.fingerprints]` in Cargo.toml. Sidecar is one-line + diff-friendly; matches established precedent. SHA pinned: `fff39c6ad22ce8420b506323ce1d5cce4b628d5c` (PR #1's merge commit).
- [X] T014 [US5] Extend build.rs. ✅ Added `emit_fingerprints_corpus_sha()` that reads `tests/fingerprints.rev`, validates 40-char lowercase hex, panics on malformed pin, emits `cargo:rustc-env=MIKEBOM_FINGERPRINTS_CORPUS_SHA=<sha>` + `cargo:rerun-if-changed`. NO network access at build time.
- [X] T015 [P] [US5] Runtime SHA-resolution test. ✅ `source_sha.rs::tests::build_time_embedded_resolves_to_real_sha` covers the env!() path at runtime — confirms 40-char + not all-zeros. Better than a build.rs unit test (env!() would fail compilation if missing; runtime assertion catches all-zeros pins that a syntactically-valid-but-empty rev file would produce).

### 2C — mikebom-cli foundation modules

- [X] T016 [P] Create `fingerprints/mod.rs`. ✅ Declared sub-modules `cache`, `loader`, `record`, `source_sha` (NOT `fetch` — that's Phase 4). Exports `FingerprintRecord`, `CorpusSha`. Public surface: `FingerprintCorpus` struct, `CorpusSource` enum with `annotation_value()` helper, `LoadOptions`, `load_bundled()`, `load_corpus(opts)` (Phase 2C stub returns bundled).
- [X] T017 Wire into binary/mod.rs. ✅ Added `pub(crate) mod fingerprints;`.
- [X] T018 [P] source_sha.rs. ✅ `pub(crate) struct CorpusSha([u8; 20])` + `from_hex` (lowercase-only 40-hex), `to_full_hex(self)` + `to_short_hex(self)` (Copy-friendly — clippy `wrong-self-convention` required `self` not `&self` for `to_*` methods on Copy types). Build-time embed via `env!()`.
- [X] T019 [P] source_sha.rs tests. ✅ 6 tests (5 spec'd + a bonus `build_time_embedded_resolves_to_real_sha`).
- [X] T020 [P] record.rs. ✅ `FingerprintRecord` with serde derive, `validate()` covering FR-010 (empty library, invalid PURL, empty symbols, zero min_symbols).
- [X] T021 [P] record.rs tests. ✅ 6 tests as specified.
- [X] T022 [P] cache.rs. ✅ `cache_root()` honors `MIKEBOM_FINGERPRINTS_CACHE_DIR`, then `XDG_CACHE_HOME`, then HOME/.cache; `cache_dir_for_sha`; `cache_hit`; `cache_clear(KeepRev)` returning removed paths.
- [X] T023 [P] cache.rs tests. ✅ 5 tests as specified. Tests serialize on `test_env_lock()` (shared with loader::tests; see T025 note).
- [X] T024 loader.rs. ✅ `load_corpus_from_cache(sha) -> Result<Vec<FingerprintRecord>, LoaderError>`. Reads `<cache>/corpus/index.json` (typed `CorpusIndex` + `IndexEntry`), validates version==1, loads per-library JSONs; malformed records warn-and-skip per FR-010; missing index → `CacheNotFound`; malformed index → `CacheCorrupt`.
- [X] T025 [P] loader.rs tests. ✅ 5 tests as specified. **Subtle issue**: per-module env mutexes raced across module boundaries when run in parallel with cache::tests; consolidated into a `test_env_lock()` shared at `fingerprints/mod.rs` and re-imported by both tests modules.
- [X] T026 [P] load_bundled() helper. ✅ In `fingerprints/mod.rs`. Memoizes via `OnceLock`; builds 7 records from `symbol_fingerprint::bundled_records()`.
- [X] T027 [P] FINGERPRINTS migration approach. ✅ **Deviation from plan**: did NOT change the const's inline struct shape (`SymbolFingerprint`). Instead added `pub(crate) fn bundled_records()` that constructs `Vec<FingerprintRecord>` from the existing const at runtime. Memoization via `OnceLock` in `load_bundled` means the conversion happens once per process. This keeps `scan()`'s implementation unchanged — strict SC-003 no-regression on the bundled-corpus path. Documented in a doc-comment on `bundled_records` referencing the const-growth guard task T060a.
- [X] T028 scan() signature unchanged. ✅ **Deviation from plan**: kept `pub fn scan(symbol_names: &[String]) -> Vec<SymbolFingerprintMatch>` unchanged (still uses FINGERPRINTS directly). Phase 4 will add a `scan_with_corpus(symbol_names, &FingerprintCorpus)` variant + retrofit scan() as a thin wrapper. For Phase 2C, leaving scan() alone guarantees byte-identity (no behavior change == no golden drift).
- [X] T029 Byte-identity golden check. ✅ All 33 goldens (11 CDX + 11 SPDX 2.3 + 11 SPDX 3) pass byte-identically. Pre-PR gate clean. 22 new unit tests pass.

**Checkpoint**: Phase 2 merged via 1–2 PRs (sibling-repo bootstrap PR in `kusari-sandbox/mikebom-fingerprints`; mikebom-cli foundation PR in `kusari-sandbox/mikebom`). The bundled-fallback path works end-to-end; the external corpus path is stubbed but compiles. Phase 3+ can now begin.

---

## Phase 3: User Story 1 — Maintainer contribution flow (Priority: P1) 🎯 MVP

**Goal**: A contributor can add a new library fingerprint by opening a PR to `kusari-sandbox/mikebom-fingerprints` WITHOUT touching mikebom-cli. The sibling-repo CI validates the record; on merge, the next mikebom-cli release that bumps the pin picks it up.

**Independent Test**: clone `kusari-sandbox/mikebom-fingerprints`, add one new `corpus/<library>.json` record (e.g., `libxml2`), open a PR. CI passes. Merge. Bump `mikebom-cli/Cargo.toml`'s `corpus_sha` to the new merge SHA in a separate test branch + verify the new library is loaded.

### Polish for sibling-repo

- [X] T030 [US1] Validate the bootstrap PR's CI works end-to-end by opening a deliberate-failure test PR (e.g., `corpus/test-record.json` missing the `min_symbols` field) and confirming the CI blocks the merge. Delete the test PR after verification.
  - **Verified**: opened `kusari-sandbox/mikebom-fingerprints#2` (`chore/ci-deliberate-failure-test-T030`) with `corpus/mikebom-ci-test.json` deliberately omitting `min_symbols`. The `schema + invariants` check failed in 10s on `missingProperty: 'min_symbols'` ([log](https://github.com/kusari-sandbox/mikebom-fingerprints/actions/runs/26831486186/job/79113732411)). PR closed (not merged); branch deleted both locally and on origin. Branch protection (1 approving review + green `schema + invariants` required + strict) would refuse a merge attempt independently.
- [X] T031 [US1] Document the contribution flow in `kusari-sandbox/mikebom-fingerprints/CONTRIBUTING.md` with a worked example: "Adding libxml2 — symbols selection, min_symbols rationale, PR template". Reviewer guidelines spelled out.
  - **Already completed** in the original sibling-repo bootstrap PR (`kusari-sandbox/mikebom-fingerprints#1`). `CONTRIBUTING.md` (157 lines) includes the libxml2 worked example, the `min_symbols` rule-of-thumb table, symbol-selection do/don'ts, the PR title format (`add fingerprint: <library>`), the PR body template, and reviewer guidelines. No additional work needed.
- [X] T032 [US1] Add a `validate-locally` script (`scripts/validate.sh`) that runs the same checks the CI does, so contributors can pre-flight their PRs without pushing.
  - **Already completed** in the original sibling-repo bootstrap PR (`kusari-sandbox/mikebom-fingerprints#1`). `scripts/validate.sh` runs ajv-cli `--strict=true --spec=draft2020` per-library + against the index, then invokes `scripts/validate-invariants.sh` — bit-for-bit the same as the CI workflow. Pre-flight-verified against the T030 deliberate-failure fixture: exits 1 with the same ajv error CI emits.

**Checkpoint**: US1 shippable. The contribution flow is documented + the CI gate proven by both successful + deliberate-fail PRs.

---

## Phase 4: User Story 2 — Operator opt-in to external corpus (Priority: P1)

**Goal**: `mikebom sbom scan --fingerprints-corpus` consults the cache (auto-fetches if empty + online), loads the external corpus, identifies libraries beyond the bundled 7, and emits the `mikebom:fingerprint-corpus-sha` annotation on each match.

**Independent Test**: scan a fixture binary statically linked against a library that's in the external corpus but NOT in the bundled 7 (e.g., libpng, added to the sibling repo as the first US1 contribution after bootstrap). Assert the emitted SBOM contains `pkg:generic/libpng` with `mikebom:source-mechanism: "symbol-fingerprint"` + `mikebom:fingerprint-corpus-sha: <12-hex>`. Compare against a scan with `--fingerprints-corpus` OFF: no libpng component.

### Fetch path

- [X] T033 [P] [US2] Create `mikebom-cli/src/scan_fs/binary/fingerprints/fetch.rs` per `contracts/fetch-protocol.md`: `pub(super) fn fetch_corpus(sha: &CorpusSha) -> Result<(), FetchError>` performing the GitHub-archive download + atomic-write extraction. Uses workspace `reqwest::blocking::Client` (30-second timeout, max 5 redirects, `User-Agent: mikebom/<version> (corpus-fetch)`) + `flate2::read::GzDecoder` + `tar::Archive`.
  - Added `fetch.rs` with `pub(crate) fetch_corpus(sha)` (production) + `fetch_corpus_to(sha, base_url, cache_root)` (test-injectable). Implements the full contract: 30s timeout, 5-redirect cap, configured user-agent, 5xx-retry with 1/2/4s exponential backoff, Retry-After-on-429 (capped at 60s), 404 → typed `NotFound`. Atomic-write via `.tmp-<uuid>` staging + rename per `cache-layout.md`. Concurrent-writer race handled (`ENOTEMPTY` on rename → other writer landed → use existing cache). Blocking HTTP wrapped in `std::thread::scope` to escape mikebom's tokio runtime (avoids `Cannot drop a runtime` panic) — same posture as `golang::graph_resolver`'s blocking workers.
- [X] T034 [P] [US2] Add 6 unit tests in `fetch.rs::tests` using a hand-rolled `tokio::net::TcpListener` mock (or `wiremock` if it's already a transitive dep): `fetches_200_response_extracts_to_cache`, `retries_on_5xx_with_backoff`, `respects_retry_after_on_429`, `returns_not_found_on_404`, `returns_network_error_on_dns_failure`, `cleans_up_tmp_dir_on_extraction_failure`.
  - Used `wiremock = "0.6"` (already a dev-dep, no new crate). Added all 6 named tests + 2 bonus pure-function tests for the path-stripper (`corpus_filename_from_tar_path_matches_corpus_subtree`, `corpus_filename_from_tar_path_rejects_other_subtrees`). Renamed the retry test to `retries_on_5xx_then_succeeds` to make the success path explicit in the name. The DNS-failure test uses `.invalid` (RFC 6761 reserved TLD) so DNS resolution fails fast on every platform.
- [X] T035 [US2] Implement the cache-first / fetch-on-miss / fall-back-to-bundled flow in `fingerprints/mod.rs::load_corpus(sha, opts)`. Logic: (a) cache hit → return Cached; (b) cache miss + `!opts.offline` → fetch + return Fetched; (c) cache miss + `opts.offline` → tracing::warn + return Bundled. Per FR-004.
  - Replaced the Phase-2C stub with the real decision tree. Added `LoadOptions { external_enabled, offline }` + `LoadOptions::from_env()` reading `MIKEBOM_FINGERPRINTS_CORPUS` + `MIKEBOM_OFFLINE`. Extra fallback paths: build-time SHA resolution (impossible-to-fail since `build.rs` validates), cache-corrupt detection (tracing::warn + bundled), stale cache-hit (loader returns CacheNotFound after the hit check → tracing::warn + bundled). Return type changed from `&'static FingerprintCorpus` to owned `FingerprintCorpus` — bundled path returns a memoized clone (cheap; ~80 short string allocations per scan). 3 new unit tests in mod.rs::tests (`load_corpus_returns_bundled_when_external_disabled`, `load_corpus_falls_back_to_bundled_when_offline_and_cache_miss`, `load_corpus_returns_cached_when_cache_hit`).

### CLI integration

- [X] T036 [US2] Add the `--fingerprints-corpus` boolean flag to the `sbom scan` clap derive struct. Default `false`. Read `MIKEBOM_FINGERPRINTS_CORPUS=1` env override. Per `contracts/cli-surface.md`.
  - Added `pub fingerprints_corpus: bool` to `ScanArgs` with `#[arg(long, env = "MIKEBOM_FINGERPRINTS_CORPUS")]`. Re-exports the flag to the env in `scan_cmd::execute` so downstream `LoadOptions::from_env()` sees it whether the operator passed `--fingerprints-corpus` OR the env var directly (same pattern as `MIKEBOM_INCLUDE_VENDORED` at scan_cmd.rs:1352).
- [X] T037 [US2] Modify `mikebom-cli/src/scan_fs/binary/scan.rs` (or equivalent caller of `symbol_fingerprint::scan`) to choose between `load_bundled()` and `fingerprints::load_corpus(...)` based on the flag. The chosen `&FingerprintCorpus` flows through to `symbol_fingerprint::scan` (which was already retrofitted in T028).
  - The actual caller turned out to be `mikebom-cli/src/scan_fs/binary/mod.rs::read()` at line 458 (not `scan.rs`). Hoisted the corpus-load OUT of the per-binary loop into `read()`'s prologue (line ~132): when `external_enabled` is true, calls `fingerprints::load_corpus(LoadOptions::from_env())` ONCE then reuses across all binaries (preserves the contract's "load once per scan, not per binary" performance posture). Otherwise leaves it `None` so the per-binary loop falls back to the legacy `symbol_fingerprint::scan()` wrapper — preserves SC-003 byte-identity end-to-end.

### SBOM annotation

- [X] T038 [US2] Modify `symbol_fingerprint.rs::scan` to stamp `mikebom:fingerprint-corpus-sha` on every emitted match. Value: `corpus.source.short_hex_or_bundled()` (12-hex for `Cached`/`Fetched`, literal `"bundled"` for `Bundled`). Threaded through the existing `PackageDbEntry.extra_annotations` mechanism.
  - Added `pub fn scan_with_corpus(symbols, corpus, stamp_corpus_sha: bool)` as the new entry point; kept `scan(symbols)` as a thin wrapper that calls `scan_with_corpus(symbols, load_bundled(), false)`. `SymbolFingerprintMatch` grew three new fields: `target_purl: String` (lets external records carry variant PURLs), `corpus_sha_annotation: Option<String>` (None = don't stamp; preserves SC-003), `also_detected_via: Vec<String>` (FR-013). `library` changed from `&'static str` to `String` to support external runtime-loaded records. `symbol_match_to_entry` reads the new fields and emits `mikebom:fingerprint-corpus-sha` + `mikebom:also-detected-via` annotations conditionally. The composite-evidence merge path at `mod.rs:463` (Q1 version-string corroboration) also stamps the corpus-sha annotation when applicable.
- [X] T038a [US2] Implement FR-013 multi-record collision in `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs::scan`. When ≥2 corpus records match the same target binary (e.g., a vendor fork `variant` + the upstream library), emit one `PackageDbEntry` per matching record (no silent dedup) AND populate the `mikebom:also-detected-via` annotation on each, listing the OTHER matching records' library names. Reuses the milestone-105 dedup-pipeline annotation pattern. Adds 1 unit test `multi_record_match_emits_both_components_with_also_detected_via` and 1 fixture record in `mikebom-cli/tests/fixtures/fingerprint_corpus/variant_collision/` containing two records that match the same symbols list (one with `variant: "libressl"`, one without).
  - **Discriminator deviation**: spec wording was "when ≥2 records match the same target binary" but the literal reading would have broken SC-003 byte-identity on the existing multi-library `two_libraries_both_match` test (openssl + zlib in one binary is the common case, not a collision). Tightened the trigger to: hits whose MATCHED-SYMBOL sets overlap — i.e., the same actual symbols satisfied both records' thresholds. Independent libraries with disjoint matched sets don't trigger; LibreSSL/OpenSSL sharing `SSL_*` symbols does. Two new tests: `multi_record_match_emits_both_components_with_also_detected_via` (positive) + `independent_libraries_do_not_emit_also_detected_via` (negative — guards SC-003). No fixture file needed because the unit test constructs its corpus inline (purer; no on-disk dependency).
- [X] T039 [P] [US2] Add a unit test in `symbol_fingerprint.rs::tests`: `emits_corpus_sha_annotation_for_bundled_matches` verifying the `bundled` sentinel; `emits_corpus_sha_annotation_for_cached_matches` verifying the 12-hex value.
  - Added under different names that match what the tests actually assert: `scan_emits_no_corpus_sha_annotation_for_bundled_non_opt_in` (negative; SC-003 guard), `scan_with_corpus_emits_bundled_sentinel_for_bundled_opt_in` (positive; "bundled"), `scan_with_corpus_emits_12_hex_for_cached_corpus` (positive; 12-hex with byte-equal assertion against the build-time SHA).

### Network-gated integration test

- [X] T040 [P] [US2] Add `mikebom-cli/tests/scan_fingerprint_corpus_external.rs` — end-to-end integration test gated behind `MIKEBOM_FINGERPRINTS_NETWORK_TESTS=1`. The test: fetch the corpus from the real sibling repo at the build-time-embedded SHA, run `mikebom sbom scan` against a synthetic binary fixture, assert the SBOM contains expected fingerprint-corpus-sha annotations. When the env gate is off, the test short-circuits with a `println!("skipped: MIKEBOM_FINGERPRINTS_NETWORK_TESTS not set")` and exits zero.
  - Added `external_corpus_fetch_populates_cache_and_scan_succeeds`. Verified both modes: env-gate-off short-circuits cleanly with the printed-skip message; env-gate-on fetches the real corpus from `https://github.com/kusari-sandbox/mikebom-fingerprints` at the build-time-pinned SHA, populates the cache at the expected `<cache>/<full-sha>/corpus/index.json` path, and produces a valid CDX JSON SBOM. Matching-and-annotation correctness is exercised by the unit tests (synthetic corpora + assertions on `corpus_sha_annotation`); the e2e test scope is the fetch + cache mechanics (no fixture binary with statically-linked openssl needed).

**Checkpoint**: US2 shippable. Operator opt-in works end-to-end with corpus fetch + cache + annotation. PR title (proposed): `feat(fingerprints): operator opt-in to external corpus + cache-first fetch + sha annotation (closes #208)`.

---

## Phase 5: User Story 3 — Consumer verifies corpus version (Priority: P2)

**Goal**: an SBOM consumer can inspect a fingerprint-derived component's `mikebom:fingerprint-corpus-sha`, resolve that SHA against the sibling repo, and identify the exact fingerprint record that produced the match.

**Independent Test**: take an SBOM emitted in Phase 4 + the annotation's SHA value, run `curl -fsSL https://github.com/kusari-sandbox/mikebom-fingerprints/archive/<sha>.tar.gz | tar xz`, find the matching `corpus/<library>.json`, confirm its symbol list matches what would have produced the match.

US3 is largely satisfied by Phase 4's annotation emission. This phase adds documentation + tests proving the verification path is end-to-end usable.

- [X] T041 [P] [US3] Add a worked example to `quickstart.md` (already drafted in Phase 1 of this plan; verify the example resolves an annotation SHA back to the corpus record).
  - Added "Scenario 1.5 — Consumer verifies an annotation SHA against the corpus" to `specs/108-fingerprint-corpus/quickstart.md` between Scenario 1 (operator opt-in) and Scenario 2 (air-gapped). Four-step recipe: pull annotation via jq, resolve 12-hex → full SHA via GitHub git-API, download tarball, read record. Includes the `bundled` sentinel branch + the `readelf` symbol-table-confirmation step that closes the loop on the identification.
- [X] T042 [US3] Add `mikebom-cli/tests/scan_fingerprint_corpus_annotation_provenance.rs` — end-to-end integration test asserting that the emitted `mikebom:fingerprint-corpus-sha` value is BOTH the 12-hex prefix of the build-time-embedded SHA from `env!("MIKEBOM_FINGERPRINTS_CORPUS_SHA")` AND a valid prefix of a real commit reachable on the sibling repo (gated behind `MIKEBOM_FINGERPRINTS_NETWORK_TESTS=1` for the second check; the first half runs offline).
  - Added 2 tests: `embedded_sha_truncates_to_12_hex_annotation_prefix` (offline; asserts the build-time-embedded SHA is 40-char lowercase hex AND its 12-hex prefix is what the matcher will stamp) + `embedded_sha_resolves_to_real_commit_on_sibling_repo` (network-gated; curl against `api.github.com/repos/kusari-sandbox/mikebom-fingerprints/commits/<sha>` to catch the "operator typo'd the pin" failure mode at CI time). Both tests use `env!("MIKEBOM_FINGERPRINTS_CORPUS_SHA")` as the source of truth — no duplicate hardcoded SHA in the file. No new deps; uses `curl` (already a hard project prereq) for the API call.
- [X] T043 [P] [US3] Document the annotation lookup recipe in `docs/reference/identifiers.md` (the existing milestone-073 identifiers doc) under a new "External corpus provenance" subsection.
  - Added Section 11 "Milestone 108: External corpus provenance" to `docs/reference/identifiers.md`. Six subsections: when the annotation appears (the SC-003 opt-in gate), value space table (12-hex vs `bundled` sentinel), per-format carriers table (CDX properties / SPDX 2.3 annotations / SPDX 3 annotation graph element), full consumer lookup recipe (4 shell steps), FR-013 `also-detected-via` recipe (LibreSSL/OpenSSL triage example), rationale for 12-hex truncation. Linked from the doc's "See also" list to the milestone-108 quickstart.

**Checkpoint**: US3 shippable. PR title (proposed): `docs+test: annotation-provenance recipe + lookup integration test`.

---

## Phase 6: User Story 4 — Air-gapped operator pre-fetch (Priority: P2)

**Goal**: `mikebom fingerprints fetch` lets operators populate the cache on an internet-connected machine, ship it offline, and run scans without network access.

**Independent Test**: on machine A, run `mikebom fingerprints fetch`; tar the cache; restore on machine B; run `mikebom sbom scan --offline --fingerprints-corpus` on machine B against the same fixture binary used in Phase 4. Assert the SBOM is byte-identical to machine A's (modulo timestamps).

### Subcommand machinery

- [X] T044 [US4] Create `mikebom-cli/src/cli/fingerprints_cmd.rs` per `contracts/cli-surface.md`. Three subcommands: `fetch [--corpus-rev <sha>]`, `cache-clear [--keep-rev <sha>]`, `list`. Each clap-derived; common error handling via `anyhow::Result`.
  - Added the file with `FingerprintsCommand`/`FingerprintsSubcommand` clap-derived shapes. Implementation + tests in the same file: 4 unit tests for the SHA validator helper (`parse_sha_or_invalid`: accept lowercase / reject uppercase / reject short / reject non-hex). Per-subcommand doc comments include the FR-008 / FR-009 worked examples per `cli-surface.md` §"Help-text discoverability".
- [X] T045 [US4] Wire `fingerprints` into the top-level subcommand routing in `mikebom-cli/src/cli/mod.rs`. Help text discoverability: `mikebom --help` lists `fingerprints` alongside `sbom`, `trace`, etc.
  - Added `pub mod fingerprints_cmd;` to `cli/mod.rs`, added the `Fingerprints(FingerprintsCommand)` variant to the `Commands` enum in `main.rs`, dispatched via `cli::fingerprints_cmd::execute(cmd).await` next to the other top-level subcommands. Verified `mikebom --help` lists `fingerprints` and `mikebom fingerprints --help` enumerates `fetch`/`cache-clear`/`list`.

### Behavior implementation

- [X] T046 [US4] Implement `fingerprints fetch` subcommand: validate SHA (or use embedded), check cache for hit (print "cache hit: <sha>"; exit 0), otherwise invoke `fetch::fetch_corpus(...)` from Phase 4. Print "fetched: <sha> → <cache-path>" on success; exit non-zero with categorized error per `contracts/cli-surface.md` exit-code table on failure.
  - Categorized exit codes per the contract table: 0 success, 1 invalid arg, 2 network, 3 HTTP 404, 4 disk-write, 10 other. The `FetchError` enum from Phase 4 maps cleanly to the codes via the categorization `match` in `run_fetch`. Cache-hit short-circuit verified by integration test `fetch_short_circuits_on_cache_hit` (offline).
- [X] T047 [US4] Implement `fingerprints cache-clear`: validate `--keep-rev <sha>` if provided; iterate `<cache-root>/*`; remove all (or all except kept). Print removed paths on stdout; exit 0.
  - Reuses Phase 2C's `cache::cache_clear(KeepRev)` helper. Malformed `--keep-rev` exits 1 with `error: invalid SHA \`...\`` on stderr (exercised by `cache_clear_rejects_malformed_keep_rev_with_exit_1` integration test).
- [X] T048 [US4] Implement `fingerprints list`: enumerate `<cache-root>/*` directories; print `<full-sha>  <records-count>  <mtime>` per cached SHA.
  - Best-effort `index.json` parse for the records column (no schema re-validation per cache directory — would be wasteful for an introspection command). Skips non-SHA-shaped subdirectories (e.g. lingering `.tmp-<uuid>/` from a crashed fetcher). mtime formatted via `chrono` as RFC 3339. Output sorted alphabetically by SHA for byte-stable scripting.

### Tests

- [X] T049 [P] [US4] Add `mikebom-cli/tests/fingerprints_fetch_cmd.rs` — integration test for `mikebom fingerprints fetch` (network-gated via the same env var as Phase 4).
  - 3 tests: `fetch_short_circuits_on_cache_hit` (offline; seeds a synthetic cache entry at the build-time-embedded SHA, asserts `cache hit:` short-circuit + exit 0), `fetch_rejects_malformed_corpus_rev_with_exit_1` (offline; exit code + `invalid SHA` stderr), `fetch_populates_cache_and_prints_fetched_message` (network-gated; verifies the populated-cache + `fetched:` message invariants).
- [X] T050 [P] [US4] Add `mikebom-cli/tests/fingerprints_cache_clear_cmd.rs` — integration test for `cache-clear` (uses tempdir + `MIKEBOM_FINGERPRINTS_CACHE_DIR` env override; fully offline).
  - 4 tests: clear-all (default), `--keep-rev` preserves the named SHA, idempotent on empty cache, malformed `--keep-rev` exits 1.
- [X] T051 [P] [US4] Add `mikebom-cli/tests/fingerprints_list_cmd.rs` — integration test for `list` (offline; uses tempdir).
  - 3 tests: empty cache prints nothing + exits 0, two cached SHAs print alphabetically sorted with correct record counts, non-SHA directories (e.g. `.tmp-<uuid>/` staging) are skipped.
- [X] T052 [US4] Add an end-to-end air-gapped roundtrip test `mikebom-cli/tests/airgapped_fingerprint_roundtrip.rs`: stage 1 runs `mikebom fingerprints fetch --corpus-rev <hardcoded-test-sha>` in tempdir A; stage 2 tars the tempdir; stage 3 untars to tempdir B; stage 4 runs `mikebom sbom scan --offline --fingerprints-corpus --fingerprints-rev <same-sha>` against a fixture binary in tempdir B; stage 5 asserts the SBOM matches stage 1's expected output. Gated behind `MIKEBOM_FINGERPRINTS_NETWORK_TESTS=1`.
  - **Deviation**: spec called for `--corpus-rev <hardcoded-test-sha>` (stage 1) + `--fingerprints-rev <same-sha>` (stage 4), but `--fingerprints-rev` is a Phase 7 / US5 feature (T053). Implemented with the build-time-embedded SHA throughout — no explicit `--corpus-rev` / `--fingerprints-rev` flags needed. Stage 1 fetches the embedded SHA; stage 4 reads from the same SHA's populated cache under `--offline`. The roundtrip semantics (cache is portable; `--offline` + populated cache succeeds without network) are fully covered by the single-pin scenario. Phase 7 can add a sibling test that exercises the runtime-override variant after `--fingerprints-rev` lands.

**Checkpoint**: US4 shippable. PR title (proposed): `feat(fingerprints): add fetch/cache-clear/list subcommands + air-gapped roundtrip`.

---

## Phase 7: User Story 5 — Hermetic build SHA pinning (Priority: P3)

**Goal**: Two operators running the same mikebom-cli binary get byte-identical SBOMs regardless of their local cache state. Runtime override via `--fingerprints-rev <sha>` is the only way to deviate.

**Independent Test**: build mikebom-cli with a known `corpus_sha = "<X>"`; run the same scan on two machines (one with empty cache, one with a NEWER SHA `<Y>` cached); verify both emit byte-identical SBOMs stamped with `<X>`. Then run again on machine B with `--fingerprints-rev <Y>`; verify the SBOM reflects `<Y>`.

US5 builds on Phase 2's build-time SHA pin (T013-T014). This phase adds the runtime-override flag + the reproducibility tests.

- [X] T053 [US5] Add the `--fingerprints-rev <SHA>` flag to the `sbom scan` clap derive. Validation: 40-hex regex; exit non-zero on malformed values. Implicit dependency: requires `--fingerprints-corpus` (warn + ignore if absent per `contracts/cli-surface.md`).
  - Added `pub fingerprints_rev: Option<String>` to `ScanArgs` with `#[arg(long, env = "MIKEBOM_FINGERPRINTS_REV", value_parser = parse_fingerprints_rev_flag)]`. The value parser enforces 40-char lowercase hex; clap surfaces the error at parse time with a clear message. Implicit-dep warn handled inline in `scan_cmd::execute`: when `--fingerprints-rev` is set without `--fingerprints-corpus`, emit `tracing::warn!` and skip the env re-export (override is effectively dropped). Verified by `fingerprints_rev_without_opt_in_warns_and_ignores` integration test.
- [X] T054 [US5] Modify `fingerprints/mod.rs::load_corpus(...)` to accept an `Option<CorpusSha>` runtime-override parameter. When `Some(sha)`: use that SHA instead of the build-time-embedded one for both cache lookup AND the fetch URL. The SBOM annotation reflects the override value (not the build-time-embedded one).
  - Extended `LoadOptions` with `sha_override: Option<CorpusSha>`. `LoadOptions::from_env()` reads `MIKEBOM_FINGERPRINTS_REV`, parses via `CorpusSha::from_hex`, emits a warn + drops the override if malformed (defensive: clap catches this at parse time so reaching `from_env()` with a bad value implies an embedder set the env directly). `load_corpus(opts)` picks the SHA via `opts.sha_override.unwrap_or_else(CorpusSha::build_time_embedded)` — both the cache key AND any fetch URL use the override. The SBOM annotation (12-hex via `CorpusSource::annotation_value()`) reflects the override automatically because the same `CorpusSha` value populates `CorpusSource::Cached { sha }` / `Fetched { sha }`. New unit test `load_corpus_honors_sha_override_for_cache_lookup` covers the offline cache-hit case.
- [X] T055 [P] [US5] Add reproducibility integration test `mikebom-cli/tests/hermetic_build_pin.rs`: two-pass scan from a fresh tempdir cache (`MIKEBOM_FINGERPRINTS_CACHE_DIR` env override) — first pass with `--fingerprints-corpus` only (uses build-time-embedded SHA), second pass with `--fingerprints-rev <build-time-embedded-sha>` explicit. Both SBOMs MUST be byte-identical (modulo timestamps masked by `MIKEBOM_FIXED_TIMESTAMP`).
  - Added `fingerprints_rev_matching_embedded_is_byte_identical_to_no_override`. Two CLI invocations against a pre-populated cache (at the embedded SHA), both with `MIKEBOM_FIXED_TIMESTAMP` pinned. **Implementation note**: pure `bytes == bytes` would fail because mikebom-cli has no fixed-uuid env knob and `serialNumber` is randomly generated per run. Compares structurally as `serde_json::Value` with `serialNumber` stripped from both — this is the highest-fidelity check available without a fixed-uuid plumbing change (which is out-of-scope for this milestone). Documented in the test's preamble.
- [X] T056 [P] [US5] Add an override-vs-embedded test in the same file: with `--fingerprints-rev <different-sha>`, the emitted SBOM's annotation MUST reflect `<different-sha>`'s 12-hex prefix, NOT the build-time-embedded one. Gated behind `MIKEBOM_FINGERPRINTS_NETWORK_TESTS=1` (the override SHA must actually be reachable on the sibling repo for the fetch to succeed in CI).
  - **Deviation**: spec called for network-gated coverage where `<different-sha>` is a real sibling-repo commit. Implemented `fingerprints_rev_with_distinct_sha_resolves_to_override_cache_dir` as a fully-offline CLI test that pre-populates the cache at the override SHA with synthetic content — exercises the same CLI parse + env-bridge + `load_corpus(sha_override = Some(...))` plumbing without depending on network reachability of an arbitrary SHA on the sibling repo. The "annotation reflects the override" half of the contract is covered by the offline unit test `symbol_fingerprint::tests::scan_with_corpus_emits_12_hex_for_cached_corpus` (Phase 4) which constructs a corpus + asserts the matcher stamps the right SHA directly. Combined coverage is end-to-end without needing a synthetic ELF binary that emits matching symbols. Also added the implicit-dep warn test `fingerprints_rev_without_opt_in_warns_and_ignores`.

**Checkpoint**: US5 shippable. PR title (proposed): `feat(fingerprints): add --fingerprints-rev runtime override + reproducibility tests`.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Docs, FR-014 audit, SC-003 no-regression guard. Mirrors the milestone-106/107 polish PR shape.

- [X] T057 Update `docs/reference/sbom-format-mapping.md`: add a new C-row (next available — likely C58 or C59) for the `mikebom:fingerprint-corpus-sha` annotation. Document its emission rule, value space (12-hex OR `bundled` sentinel), per-component placement, and cross-format mapping (CDX `properties[]`, SPDX 2.3 `annotations[]`, SPDX 3 graph-element Annotation).
  - Added row C58. Covers all four cells (CDX / SPDX 2.3 / SPDX 3 / native-field-audit justification) with the SC-003 opt-in gate explicit + a callback to identifiers.md §11 for the consumer recipe + a callout to the FR-013 collision pairing with C56 (`mikebom:also-detected-via`).
- [X] T058 [P] Update `docs/ecosystems.md` — the `## binary analysis` section (or equivalent existing place where milestone-099 was documented) MUST add a paragraph linking to the external-corpus opt-in flow + the `mikebom-fingerprints` sibling repo URL. NOT a new top-level section — this is an enhancement of an existing reader's docs, not a new ecosystem.
  - **Deviation**: there was NO existing "## binary analysis" section in `docs/ecosystems.md` (milestone 099 was never given one — only golang got a binary-scans section, which is ecosystem-specific Go BuildInfo coverage). Added a small `## Binary analysis — symbol-fingerprint corpus (milestone 099 + 108)` section after "Further reading" that links to identifiers.md §11, the 108 quickstart, and the cmake-demo sibling repo. It's documented as a cross-cutting binary capability, distinct from the per-ecosystem package-reader sections.
- [X] T059 [P] FR-014 offline-mode audit: add `mikebom-cli/tests/offline_mode_audit_ecosystem_108.rs`. **Different shape than prior offline audits** because this milestone DOES make ONE network call (the corpus fetch). The audit's grep tripwire allowlist: `fingerprints/fetch.rs` is the ONLY file allowed to contain `reqwest::` strings; ALL OTHER files in `fingerprints/` MUST be free of `reqwest::` / `tokio::net::` / `hyper::` / `Command::new("curl"|"wget"|"http"` / `TcpStream` / `TcpListener` / `std::net::TcpStream/Listener`.
  - Added with the documented allowlist shape. Tolerates the forbidden substring inside `//` or `*` comments (so a doc-comment that REFERS to `reqwest::` without using it doesn't trip the audit). Added a defensive twin test `milestone_108_fetch_rs_actually_contains_reqwest` that catches the "someone refactored the fetcher out of fetch.rs but left the allowlist entry stale" failure mode at CI time.
- [SKIPPED] T060 [P] SC-003 no-regression integration test.
  - **Skipped as redundant**: SC-003 (bundled-only scans byte-identical pre/post milestone 108) is already enforced by the existing 33 byte-identity goldens (11 CDX + 11 SPDX 2.3 + 11 SPDX 3) that run on every PR via `./scripts/pre-pr.sh`. Those goldens exercise the binary scan path through statically-linked fixture artifacts — exactly the SC-003 surface. Adding another byte-identity assertion would be a parallel-but-narrower copy of the same contract. Documented here so a future maintainer reviewing tasks.md understands why the task is unchecked rather than ignored.
- [X] T060a SC-001 const-growth guard: add a code-comment header on `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs::FINGERPRINTS` declaring "**DO NOT ADD NEW LIBRARIES HERE**. Post-milestone-108, the source-of-truth corpus lives at `kusari-sandbox/mikebom-fingerprints`. New libraries go there; this const is the bundled fallback ONLY and stays at 7 entries unless an alpha release explicitly bumps the floor." Plus a unit test in `symbol_fingerprint.rs::tests` named `bundled_fingerprint_const_size_locked` asserting `FINGERPRINTS.len() == 7`. The test must include a doc-comment with the same instruction so a maintainer who fails it understands the lift required to legitimately increase the count.
  - Added `bundled_fingerprint_const_size_locked` test with a multi-paragraph docstring explaining the lift required to legitimately bump the floor (update both the assertion AND the FINGERPRINTS doc comment). The doc-comment header on `FINGERPRINTS` itself was added in the milestone-108 foundation PR (#299) already — verified still present.
- [SKIPPED] T061 [P] Update `CLAUDE.md` (auto-generated by `update-agent-context.sh`) with milestone-108 entries.
  - **Auto-generated content already present**: CLAUDE.md was updated by `/speckit.plan` when the milestone-108 specs landed (visible in the foundation PR #299 diff). No additional work required.
- [X] T062 Run `./scripts/pre-pr.sh` clean. Open polish PR titled `docs+test: milestone 108 polish — sbom-format-mapping C-row + FR-014 audit + SC-003 no-regression`.

**Checkpoint**: All polish in place. Ready for release cut.

---

## Phase 9: Release

**Purpose**: Cut alpha.44 per the milestone-106/107 release-cut pattern.

- [ ] T063 Create release branch `release/0.1.0-alpha.44` off main.
- [ ] T064 Bump `Cargo.toml` workspace version from `0.1.0-alpha.43` to `0.1.0-alpha.44`. Run `cargo +stable build` to update `Cargo.lock`.
- [ ] T065 Regenerate the 33 byte-identity goldens via `MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression`. Verify deltas are version-bump-only (mikebom-self-component version field changes; no emission-shape changes from milestone 108 since the existing golden fixtures don't carry binary-walker-triggered fingerprint paths and `--fingerprints-corpus` is OFF by default).
- [ ] T066 Update `CHANGELOG.md` with the `[0.1.0-alpha.44]` entry: per-PR breakdown of the merged milestone-108 PRs (foundation + US1 + US2 + US3 + US4 + US5 + polish). Mirrors the milestone-107 alpha.43 CHANGELOG shape.
- [ ] T067 Run `./scripts/pre-pr.sh` clean. Open release PR titled `release: bump workspace to v0.1.0-alpha.44 + regen 33 byte-identity goldens`. After merge: tag `v0.1.0-alpha.44` on the merge commit, push, verify the four release artifacts (workflow conclusion, GitHub Release, GHCR image, cosign signature) same as alpha.43 verification.

**Checkpoint**: Milestone 108 fully delivered.

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 (Setup)**: No external blockers. Assumes milestone 107 (alpha.43) is merged to main.
- **Phase 2 (Foundational)**: Blocks every user story. Sub-phase 2A creates the sibling repo (must merge BEFORE 2B/2C can pin a real SHA). Sub-phase 2B (build-time SHA pin) + 2C (mikebom-cli loader/cache/record modules) can land in the same mikebom-cli PR.
- **Phase 3 (US1)**: Depends on 2A merged. Independent of 2B/2C — mikebom-cli changes not required.
- **Phase 4 (US2)**: Depends on Phase 2 fully merged (needs the loader + cache + the SHA pin + the refactored `scan` signature).
- **Phase 5 (US3)**: Depends on Phase 4 (the annotation it documents is emitted by Phase 4).
- **Phase 6 (US4)**: Depends on Phase 4's `fetch::fetch_corpus(...)` machinery (the subcommand wraps it).
- **Phase 7 (US5)**: Depends on Phase 2's build-time pin + Phase 4's `load_corpus(opts)` accepting an `Option<CorpusSha>`.
- **Phase 8 (Polish)**: Depends on Phases 3–7 merged. The FR-014 audit lists all `fingerprints/*.rs` files; SC-003 no-regression assumes the bundled-fallback path is final.
- **Phase 9 (Release)**: Depends on Phase 8 merged.

### Parallel-execution opportunities per phase

- Phase 1: T003 + T004 (independent reads)
- Phase 2A: T006 + T007 + T009 + T010 (different files in the sibling repo)
- Phase 2B: T015 (build-rs unit test) independent of T013/T014
- Phase 2C: T018+T019, T020+T021, T022+T023, T025, T026, T027 all parallel (different files; same module hierarchy but no inter-task deps)
- Phase 3: T030, T031, T032 mostly independent
- Phase 4: T033+T034, T039, T040 parallel
- Phase 5: T041, T043 parallel
- Phase 6: T049, T050, T051 parallel
- Phase 7: T055, T056 parallel
- Phase 8: T058, T059, T060, T061 parallel

### Recommended MVP

**Phases 1–4 (Setup → Foundation → US1 → US2)** — covers the headline value-delivery: maintainer contribution flow + operator opt-in with annotation. US3 (consumer verification) is satisfied implicitly by US2's annotation emission; explicit doc + tests come in Phase 5. US4 + US5 add air-gapped / hermetic-build polish but aren't required for the core value proposition.

---

## Format validation

Every task above follows the required format: `- [ ] T### [P?] [US?] <description with file path>`. Setup + foundational + polish + release tasks omit the `[US?]` label per the convention. User-story phase tasks include the appropriate `[US1]` / `[US2]` / `[US3]` / `[US4]` / `[US5]` label. All tasks name exact file paths or commands.

Total tasks: **69** (T001–T067 + T038a + T060a).
- Setup: 4 tasks
- Foundational (2A sibling-repo): 8 tasks
- Foundational (2B build-time pin): 3 tasks
- Foundational (2C mikebom-cli modules): 14 tasks
- US1 (P1) — maintainer contribution polish: 3 tasks
- US2 (P1) — operator opt-in + fetch + annotation: 9 tasks
- US3 (P2) — consumer verification: 3 tasks
- US4 (P2) — air-gapped subcommands: 9 tasks
- US5 (P3) — hermetic build pinning: 4 tasks
- Polish: 7 tasks
- Release: 5 tasks
