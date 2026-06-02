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

- [ ] T001 Verify branch checkout: confirm `git branch --show-current` returns `108-fingerprint-corpus` (the script-created branch).
- [ ] T002 Confirm milestone 107's full release (alpha.43, PR #298) has merged to `main` AND the v0.1.0-alpha.43 tag is present. Rebase the 108 branch on the post-107 `main` head if needed.
- [ ] T003 [P] Run baseline pre-PR gate: `./scripts/pre-pr.sh` MUST pass clean on the rebased branch. Document the baseline scan-time for SC-004's <100ms cache-hit overhead comparison.
- [ ] T004 [P] Survey the existing `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs::FINGERPRINTS` const: run `grep -nE '^\s*SymbolFingerprint\s*\{' mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs` to record the 7 library names + their existing N=10 default. Used by Phase 2 (sibling-repo seed) to drive content extraction.

**Checkpoint**: Baseline confirmed. Phase 2 can begin.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Build the cross-cutting infrastructure every user story consumes — the sibling repo's structure, the build-time SHA pin, the mikebom-cli loader/cache/record types, and the bundled-fallback path. **No user-story work can begin until this phase merges.**

### 2A — Sibling-repo creation (outside this Cargo workspace)

- [ ] T005 Create the public Apache-2.0 repo `kusari-sandbox/mikebom-fingerprints` via `gh repo create kusari-sandbox/mikebom-fingerprints --public --description "Symbol-fingerprint corpus consumed by mikebom; see github.com/kusari-sandbox/mikebom milestone 108" --license Apache-2.0`. Clone locally to `~/Projects/mikebom-fingerprints/` for the seed PR.
- [ ] T006 [P] Seed `~/Projects/mikebom-fingerprints/README.md`, `CONTRIBUTING.md`, `LICENSE` (Apache-2.0) per `contracts/sibling-repo-bootstrap.md`.
- [ ] T007 [P] Seed `~/Projects/mikebom-fingerprints/schema/fingerprint-record.v1.json` + `schema/index.v1.json` verbatim from `contracts/corpus-schema.md`'s "Example record" and "corpus/index.json schema (v1)" sections.
- [ ] T008 Seed the 7 corpus files (`corpus/openssl.json`, `corpus/zlib.json`, `corpus/libcurl.json`, `corpus/sqlite.json`, `corpus/pcre.json`, `corpus/pcre2.json`, `corpus/gnutls.json`) by extracting from `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs::FINGERPRINTS` per T004's survey. Each file conforms to `schema/fingerprint-record.v1.json` with an explicit `min_symbols` per record (default `10` to match existing in-source behavior; curator notes per `contracts/sibling-repo-bootstrap.md` "Initial content"). Write `corpus/index.json` listing all 7.
- [ ] T009 [P] Seed `~/Projects/mikebom-fingerprints/.github/workflows/validate-corpus.yml` per `contracts/sibling-repo-bootstrap.md` "validate-corpus.yml". Pin all action SHAs (security memory: never interpolate `${{ }}` in run blocks; `persist-credentials: false` on checkout).
- [ ] T010 [P] Seed `~/Projects/mikebom-fingerprints/scripts/validate-invariants.sh` enforcing: `symbols.length >= 2 * min_symbols`; `(library, variant)` uniqueness; common-prefix-tripwire blocklist with `# tripwire-ok: <reason>` override; `index.json` ↔ `corpus/*.json` consistency.
- [ ] T011 Open the bootstrap PR `feat: seed corpus + schema + CI` against the new repo. Verify CI green. Merge. **Record the merge-commit SHA** — it's the `corpus_sha` pin in T013.
- [ ] T012 Enable branch protection on `kusari-sandbox/mikebom-fingerprints` `main`: require 1 approving review + CI green. Configure via `gh api repos/kusari-sandbox/mikebom-fingerprints/branches/main/protection`.

### 2B — Build-time SHA pin (mikebom-cli)

- [ ] T013 [US5] Add `[package.metadata.fingerprints]` section to `mikebom-cli/Cargo.toml` with `corpus_sha = "<sha-from-T011>"`. This is the build-time-embedded SHA that mikebom-cli will resolve at compile time.
- [ ] T014 [US5] Add `mikebom-cli/build.rs` (or modify if it exists) to parse the `[package.metadata.fingerprints].corpus_sha` field via the workspace `toml = "0.8"` dep and emit `cargo:rustc-env=MIKEBOM_FINGERPRINTS_CORPUS_SHA=<sha>`. Add `cargo:rerun-if-changed=Cargo.toml` so cargo invalidates the cache when the pin changes.
- [ ] T015 [P] [US5] Add a `build.rs` unit test (separate `build_rs_tests.rs` file at the crate root) verifying that the env var is emitted with the expected format. Validates the pin-resolution mechanism without running an actual build.

### 2C — mikebom-cli foundation modules

- [ ] T016 Create `mikebom-cli/src/scan_fs/binary/fingerprints/mod.rs` declaring `pub(super) mod cache;`, `pub(super) mod fetch;`, `pub(super) mod loader;`, `pub(super) mod record;`, `pub(super) mod source_sha;`. Export the public surface (`CorpusSource`, `FingerprintCorpus`, `load_corpus(...)`).
- [ ] T017 Wire `mod fingerprints;` into `mikebom-cli/src/scan_fs/binary/mod.rs`.
- [ ] T018 [P] Create `mikebom-cli/src/scan_fs/binary/fingerprints/source_sha.rs` per `data-model.md`: `pub(super) struct CorpusSha([u8; 20])` with `from_hex()`, `to_full_hex()`, `to_short_hex()` (12-hex truncation). Resolve build-time-embedded SHA via `env!("MIKEBOM_FINGERPRINTS_CORPUS_SHA")` at module-init.
- [ ] T019 [P] Add 5 unit tests in `source_sha.rs::tests`: `from_hex_accepts_lowercase_40_hex`, `from_hex_rejects_wrong_length`, `from_hex_rejects_non_hex_chars`, `to_short_hex_truncates_to_12_chars`, `to_full_hex_lowercase_roundtrip`.
- [ ] T020 [P] Create `mikebom-cli/src/scan_fs/binary/fingerprints/record.rs` per `data-model.md`: `pub(super) struct FingerprintRecord` with `serde::Deserialize` derive; explicit validation in `validate(&self) -> Result<(), RecordValidationError>` checking the FR-010 defensive rules (non-empty library/symbols, valid PURL via `Purl::new`, `min_symbols > 0`).
- [ ] T021 [P] Add 6 unit tests in `record.rs::tests`: `parses_minimal_valid_record`, `parses_record_with_optional_fields`, `rejects_missing_required_field`, `rejects_invalid_purl_in_target_purl`, `rejects_zero_min_symbols`, `rejects_empty_symbols_list`.
- [ ] T022 [P] Create `mikebom-cli/src/scan_fs/binary/fingerprints/cache.rs` per `contracts/cache-layout.md`: `cache_root() -> PathBuf` (honors `MIKEBOM_FINGERPRINTS_CACHE_DIR` env override; falls back to `dirs::cache_dir().join("mikebom").join("fingerprints")`); `cache_dir_for_sha(&CorpusSha) -> PathBuf`; `cache_hit(&CorpusSha) -> bool`; `cache_clear(opt: KeepRev) -> Result<Vec<PathBuf>>` returning removed paths.
- [ ] T023 [P] Add 5 unit tests in `cache.rs::tests`: `cache_root_honors_env_override`, `cache_dir_for_sha_uses_full_40_hex`, `cache_hit_false_when_directory_absent`, `cache_clear_removes_all_when_no_keep`, `cache_clear_preserves_kept_sha`.
- [ ] T024 Create `mikebom-cli/src/scan_fs/binary/fingerprints/loader.rs` per `contracts/cache-layout.md`'s "Reader validation": `load_corpus_from_cache(sha) -> Result<FingerprintCorpus, LoaderError>` reading `<cache-dir>/corpus/index.json` + per-library JSONs. Records that fail individual validation skipped with `tracing::warn!`; corrupt `index.json` returns `LoaderError::CacheCorrupt`.
- [ ] T025 [P] Add 5 unit tests in `loader.rs::tests`: `loads_valid_cache_to_corpus`, `returns_cache_not_found_when_index_absent`, `returns_cache_corrupt_on_malformed_index_json`, `skips_malformed_records_warns_continues`, `parses_index_with_optional_digest_field`.
- [ ] T026 [P] Create the bundled-fallback construction path: a `pub(super) fn load_bundled() -> FingerprintCorpus` in `fingerprints/mod.rs` that returns the in-source `FINGERPRINTS` const as a `FingerprintCorpus { records: ..., source: CorpusSource::Bundled }`. The bundled records use the same `FingerprintRecord` shape as cached ones (one shared type for the matcher).
- [ ] T027 [P] Migrate `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs::FINGERPRINTS` from its current inline struct shape (`SymbolFingerprint`) to a `&'static [FingerprintRecord]` ARRAY of the new `record::FingerprintRecord` type. The 7 records' content is unchanged; only the wrapping type changes. This is the foundational refactor that lets `symbol_fingerprint::scan` consume both bundled + cached corpora through one code path.
- [ ] T028 Modify `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs::scan` signature: takes `&FingerprintCorpus` as an explicit parameter (instead of reading the inline `FINGERPRINTS` const). All existing callers (likely just one in `scan_fs/binary/scan.rs`) pass `&load_bundled()` for now; later phases swap in cached corpora.
- [ ] T029 Run `cargo +stable test --workspace` and the 33 byte-identity goldens. **SC-003 no-regression contract**: byte-identical goldens pre-and-post-refactor. If any golden diff appears, abort and root-cause.

**Checkpoint**: Phase 2 merged via 1–2 PRs (sibling-repo bootstrap PR in `kusari-sandbox/mikebom-fingerprints`; mikebom-cli foundation PR in `kusari-sandbox/mikebom`). The bundled-fallback path works end-to-end; the external corpus path is stubbed but compiles. Phase 3+ can now begin.

---

## Phase 3: User Story 1 — Maintainer contribution flow (Priority: P1) 🎯 MVP

**Goal**: A contributor can add a new library fingerprint by opening a PR to `kusari-sandbox/mikebom-fingerprints` WITHOUT touching mikebom-cli. The sibling-repo CI validates the record; on merge, the next mikebom-cli release that bumps the pin picks it up.

**Independent Test**: clone `kusari-sandbox/mikebom-fingerprints`, add one new `corpus/<library>.json` record (e.g., `libxml2`), open a PR. CI passes. Merge. Bump `mikebom-cli/Cargo.toml`'s `corpus_sha` to the new merge SHA in a separate test branch + verify the new library is loaded.

### Polish for sibling-repo

- [ ] T030 [US1] Validate the bootstrap PR's CI works end-to-end by opening a deliberate-failure test PR (e.g., `corpus/test-record.json` missing the `min_symbols` field) and confirming the CI blocks the merge. Delete the test PR after verification.
- [ ] T031 [US1] Document the contribution flow in `kusari-sandbox/mikebom-fingerprints/CONTRIBUTING.md` with a worked example: "Adding libxml2 — symbols selection, min_symbols rationale, PR template". Reviewer guidelines spelled out.
- [ ] T032 [US1] Add a `validate-locally` script (`scripts/validate.sh`) that runs the same checks the CI does, so contributors can pre-flight their PRs without pushing.

**Checkpoint**: US1 shippable. The contribution flow is documented + the CI gate proven by both successful + deliberate-fail PRs.

---

## Phase 4: User Story 2 — Operator opt-in to external corpus (Priority: P1)

**Goal**: `mikebom sbom scan --fingerprints-corpus` consults the cache (auto-fetches if empty + online), loads the external corpus, identifies libraries beyond the bundled 7, and emits the `mikebom:fingerprint-corpus-sha` annotation on each match.

**Independent Test**: scan a fixture binary statically linked against a library that's in the external corpus but NOT in the bundled 7 (e.g., libpng, added to the sibling repo as the first US1 contribution after bootstrap). Assert the emitted SBOM contains `pkg:generic/libpng` with `mikebom:source-mechanism: "symbol-fingerprint"` + `mikebom:fingerprint-corpus-sha: <12-hex>`. Compare against a scan with `--fingerprints-corpus` OFF: no libpng component.

### Fetch path

- [ ] T033 [P] [US2] Create `mikebom-cli/src/scan_fs/binary/fingerprints/fetch.rs` per `contracts/fetch-protocol.md`: `pub(super) fn fetch_corpus(sha: &CorpusSha) -> Result<(), FetchError>` performing the GitHub-archive download + atomic-write extraction. Uses workspace `reqwest::blocking::Client` (30-second timeout, max 5 redirects, `User-Agent: mikebom/<version> (corpus-fetch)`) + `flate2::read::GzDecoder` + `tar::Archive`.
- [ ] T034 [P] [US2] Add 6 unit tests in `fetch.rs::tests` using a hand-rolled `tokio::net::TcpListener` mock (or `wiremock` if it's already a transitive dep): `fetches_200_response_extracts_to_cache`, `retries_on_5xx_with_backoff`, `respects_retry_after_on_429`, `returns_not_found_on_404`, `returns_network_error_on_dns_failure`, `cleans_up_tmp_dir_on_extraction_failure`.
- [ ] T035 [US2] Implement the cache-first / fetch-on-miss / fall-back-to-bundled flow in `fingerprints/mod.rs::load_corpus(sha, opts)`. Logic: (a) cache hit → return Cached; (b) cache miss + `!opts.offline` → fetch + return Fetched; (c) cache miss + `opts.offline` → tracing::warn + return Bundled. Per FR-004.

### CLI integration

- [ ] T036 [US2] Add the `--fingerprints-corpus` boolean flag to the `sbom scan` clap derive struct. Default `false`. Read `MIKEBOM_FINGERPRINTS_CORPUS=1` env override. Per `contracts/cli-surface.md`.
- [ ] T037 [US2] Modify `mikebom-cli/src/scan_fs/binary/scan.rs` (or equivalent caller of `symbol_fingerprint::scan`) to choose between `load_bundled()` and `fingerprints::load_corpus(...)` based on the flag. The chosen `&FingerprintCorpus` flows through to `symbol_fingerprint::scan` (which was already retrofitted in T028).

### SBOM annotation

- [ ] T038 [US2] Modify `symbol_fingerprint.rs::scan` to stamp `mikebom:fingerprint-corpus-sha` on every emitted match. Value: `corpus.source.short_hex_or_bundled()` (12-hex for `Cached`/`Fetched`, literal `"bundled"` for `Bundled`). Threaded through the existing `PackageDbEntry.extra_annotations` mechanism.
- [ ] T038a [US2] Implement FR-013 multi-record collision in `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs::scan`. When ≥2 corpus records match the same target binary (e.g., a vendor fork `variant` + the upstream library), emit one `PackageDbEntry` per matching record (no silent dedup) AND populate the `mikebom:also-detected-via` annotation on each, listing the OTHER matching records' library names. Reuses the milestone-105 dedup-pipeline annotation pattern. Adds 1 unit test `multi_record_match_emits_both_components_with_also_detected_via` and 1 fixture record in `mikebom-cli/tests/fixtures/fingerprint_corpus/variant_collision/` containing two records that match the same symbols list (one with `variant: "libressl"`, one without).
- [ ] T039 [P] [US2] Add a unit test in `symbol_fingerprint.rs::tests`: `emits_corpus_sha_annotation_for_bundled_matches` verifying the `bundled` sentinel; `emits_corpus_sha_annotation_for_cached_matches` verifying the 12-hex value.

### Network-gated integration test

- [ ] T040 [P] [US2] Add `mikebom-cli/tests/scan_fingerprint_corpus_external.rs` — end-to-end integration test gated behind `MIKEBOM_FINGERPRINTS_NETWORK_TESTS=1`. The test: fetch the corpus from the real sibling repo at the build-time-embedded SHA, run `mikebom sbom scan` against a synthetic binary fixture, assert the SBOM contains expected fingerprint-corpus-sha annotations. When the env gate is off, the test short-circuits with a `println!("skipped: MIKEBOM_FINGERPRINTS_NETWORK_TESTS not set")` and exits zero.

**Checkpoint**: US2 shippable. Operator opt-in works end-to-end with corpus fetch + cache + annotation. PR title (proposed): `feat(fingerprints): operator opt-in to external corpus + cache-first fetch + sha annotation (closes #208)`.

---

## Phase 5: User Story 3 — Consumer verifies corpus version (Priority: P2)

**Goal**: an SBOM consumer can inspect a fingerprint-derived component's `mikebom:fingerprint-corpus-sha`, resolve that SHA against the sibling repo, and identify the exact fingerprint record that produced the match.

**Independent Test**: take an SBOM emitted in Phase 4 + the annotation's SHA value, run `curl -fsSL https://github.com/kusari-sandbox/mikebom-fingerprints/archive/<sha>.tar.gz | tar xz`, find the matching `corpus/<library>.json`, confirm its symbol list matches what would have produced the match.

US3 is largely satisfied by Phase 4's annotation emission. This phase adds documentation + tests proving the verification path is end-to-end usable.

- [ ] T041 [P] [US3] Add a worked example to `quickstart.md` (already drafted in Phase 1 of this plan; verify the example resolves an annotation SHA back to the corpus record).
- [ ] T042 [US3] Add `mikebom-cli/tests/scan_fingerprint_corpus_annotation_provenance.rs` — end-to-end integration test asserting that the emitted `mikebom:fingerprint-corpus-sha` value is BOTH the 12-hex prefix of the build-time-embedded SHA from `env!("MIKEBOM_FINGERPRINTS_CORPUS_SHA")` AND a valid prefix of a real commit reachable on the sibling repo (gated behind `MIKEBOM_FINGERPRINTS_NETWORK_TESTS=1` for the second check; the first half runs offline).
- [ ] T043 [P] [US3] Document the annotation lookup recipe in `docs/reference/identifiers.md` (the existing milestone-073 identifiers doc) under a new "External corpus provenance" subsection.

**Checkpoint**: US3 shippable. PR title (proposed): `docs+test: annotation-provenance recipe + lookup integration test`.

---

## Phase 6: User Story 4 — Air-gapped operator pre-fetch (Priority: P2)

**Goal**: `mikebom fingerprints fetch` lets operators populate the cache on an internet-connected machine, ship it offline, and run scans without network access.

**Independent Test**: on machine A, run `mikebom fingerprints fetch`; tar the cache; restore on machine B; run `mikebom sbom scan --offline --fingerprints-corpus` on machine B against the same fixture binary used in Phase 4. Assert the SBOM is byte-identical to machine A's (modulo timestamps).

### Subcommand machinery

- [ ] T044 [US4] Create `mikebom-cli/src/cli/fingerprints_cmd.rs` per `contracts/cli-surface.md`. Three subcommands: `fetch [--corpus-rev <sha>]`, `cache-clear [--keep-rev <sha>]`, `list`. Each clap-derived; common error handling via `anyhow::Result`.
- [ ] T045 [US4] Wire `fingerprints` into the top-level subcommand routing in `mikebom-cli/src/cli/mod.rs`. Help text discoverability: `mikebom --help` lists `fingerprints` alongside `sbom`, `trace`, etc.

### Behavior implementation

- [ ] T046 [US4] Implement `fingerprints fetch` subcommand: validate SHA (or use embedded), check cache for hit (print "cache hit: <sha>"; exit 0), otherwise invoke `fetch::fetch_corpus(...)` from Phase 4. Print "fetched: <sha> → <cache-path>" on success; exit non-zero with categorized error per `contracts/cli-surface.md` exit-code table on failure.
- [ ] T047 [US4] Implement `fingerprints cache-clear`: validate `--keep-rev <sha>` if provided; iterate `<cache-root>/*`; remove all (or all except kept). Print removed paths on stdout; exit 0.
- [ ] T048 [US4] Implement `fingerprints list`: enumerate `<cache-root>/*` directories; print `<full-sha>  <records-count>  <mtime>` per cached SHA.

### Tests

- [ ] T049 [P] [US4] Add `mikebom-cli/tests/fingerprints_fetch_cmd.rs` — integration test for `mikebom fingerprints fetch` (network-gated via the same env var as Phase 4).
- [ ] T050 [P] [US4] Add `mikebom-cli/tests/fingerprints_cache_clear_cmd.rs` — integration test for `cache-clear` (uses tempdir + `MIKEBOM_FINGERPRINTS_CACHE_DIR` env override; fully offline).
- [ ] T051 [P] [US4] Add `mikebom-cli/tests/fingerprints_list_cmd.rs` — integration test for `list` (offline; uses tempdir).
- [ ] T052 [US4] Add an end-to-end air-gapped roundtrip test `mikebom-cli/tests/airgapped_fingerprint_roundtrip.rs`: stage 1 runs `mikebom fingerprints fetch --corpus-rev <hardcoded-test-sha>` in tempdir A; stage 2 tars the tempdir; stage 3 untars to tempdir B; stage 4 runs `mikebom sbom scan --offline --fingerprints-corpus --fingerprints-rev <same-sha>` against a fixture binary in tempdir B; stage 5 asserts the SBOM matches stage 1's expected output. Gated behind `MIKEBOM_FINGERPRINTS_NETWORK_TESTS=1`.

**Checkpoint**: US4 shippable. PR title (proposed): `feat(fingerprints): add fetch/cache-clear/list subcommands + air-gapped roundtrip`.

---

## Phase 7: User Story 5 — Hermetic build SHA pinning (Priority: P3)

**Goal**: Two operators running the same mikebom-cli binary get byte-identical SBOMs regardless of their local cache state. Runtime override via `--fingerprints-rev <sha>` is the only way to deviate.

**Independent Test**: build mikebom-cli with a known `corpus_sha = "<X>"`; run the same scan on two machines (one with empty cache, one with a NEWER SHA `<Y>` cached); verify both emit byte-identical SBOMs stamped with `<X>`. Then run again on machine B with `--fingerprints-rev <Y>`; verify the SBOM reflects `<Y>`.

US5 builds on Phase 2's build-time SHA pin (T013-T014). This phase adds the runtime-override flag + the reproducibility tests.

- [ ] T053 [US5] Add the `--fingerprints-rev <SHA>` flag to the `sbom scan` clap derive. Validation: 40-hex regex; exit non-zero on malformed values. Implicit dependency: requires `--fingerprints-corpus` (warn + ignore if absent per `contracts/cli-surface.md`).
- [ ] T054 [US5] Modify `fingerprints/mod.rs::load_corpus(...)` to accept an `Option<CorpusSha>` runtime-override parameter. When `Some(sha)`: use that SHA instead of the build-time-embedded one for both cache lookup AND the fetch URL. The SBOM annotation reflects the override value (not the build-time-embedded one).
- [ ] T055 [P] [US5] Add reproducibility integration test `mikebom-cli/tests/hermetic_build_pin.rs`: two-pass scan from a fresh tempdir cache (`MIKEBOM_FINGERPRINTS_CACHE_DIR` env override) — first pass with `--fingerprints-corpus` only (uses build-time-embedded SHA), second pass with `--fingerprints-rev <build-time-embedded-sha>` explicit. Both SBOMs MUST be byte-identical (modulo timestamps masked by `MIKEBOM_FIXED_TIMESTAMP`).
- [ ] T056 [P] [US5] Add an override-vs-embedded test in the same file: with `--fingerprints-rev <different-sha>`, the emitted SBOM's annotation MUST reflect `<different-sha>`'s 12-hex prefix, NOT the build-time-embedded one. Gated behind `MIKEBOM_FINGERPRINTS_NETWORK_TESTS=1` (the override SHA must actually be reachable on the sibling repo for the fetch to succeed in CI).

**Checkpoint**: US5 shippable. PR title (proposed): `feat(fingerprints): add --fingerprints-rev runtime override + reproducibility tests`.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Docs, FR-014 audit, SC-003 no-regression guard. Mirrors the milestone-106/107 polish PR shape.

- [ ] T057 Update `docs/reference/sbom-format-mapping.md`: add a new C-row (next available — likely C58 or C59) for the `mikebom:fingerprint-corpus-sha` annotation. Document its emission rule, value space (12-hex OR `bundled` sentinel), per-component placement, and cross-format mapping (CDX `properties[]`, SPDX 2.3 `annotations[]`, SPDX 3 graph-element Annotation).
- [ ] T058 [P] Update `docs/ecosystems.md` — the `## binary analysis` section (or equivalent existing place where milestone-099 was documented) MUST add a paragraph linking to the external-corpus opt-in flow + the `mikebom-fingerprints` sibling repo URL. NOT a new top-level section — this is an enhancement of an existing reader's docs, not a new ecosystem.
- [ ] T059 [P] FR-014 offline-mode audit: add `mikebom-cli/tests/offline_mode_audit_ecosystem_108.rs`. **Different shape than prior offline audits** because this milestone DOES make ONE network call (the corpus fetch). The audit's grep tripwire allowlist: `fingerprints/fetch.rs` is the ONLY file allowed to contain `reqwest::` strings; ALL OTHER files in `fingerprints/` MUST be free of `reqwest::` / `tokio::net::` / `hyper::` / `Command::new("curl"|"wget"|"http"` / `TcpStream` / `TcpListener` / `std::net::TcpStream/Listener`.
- [ ] T060 [P] SC-003 no-regression integration test: add `mikebom-cli/tests/scan_fingerprint_corpus_bundled.rs`. Scan a fixture binary with `--fingerprints-corpus` OFF (the default path). Assert the emitted SBOM is byte-identical (modulo timestamps) to what mikebom would have produced pre-milestone-108. The bundled 7-library corpus path MUST be unchanged.
- [ ] T060a SC-001 const-growth guard: add a code-comment header on `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs::FINGERPRINTS` declaring "**DO NOT ADD NEW LIBRARIES HERE**. Post-milestone-108, the source-of-truth corpus lives at `kusari-sandbox/mikebom-fingerprints`. New libraries go there; this const is the bundled fallback ONLY and stays at 7 entries unless an alpha release explicitly bumps the floor." Plus a unit test in `symbol_fingerprint.rs::tests` named `bundled_fingerprint_const_size_locked` asserting `FINGERPRINTS.len() == 7`. The test must include a doc-comment with the same instruction so a maintainer who fails it understands the lift required to legitimately increase the count.
- [ ] T061 [P] Update `CLAUDE.md` (auto-generated by `update-agent-context.sh`) with milestone-108 entries. Already done by `/speckit.plan`; this task verifies the additions are still present after Phase 6's work.
- [ ] T062 Run `./scripts/pre-pr.sh` clean. Open polish PR titled `docs+test: milestone 108 polish — sbom-format-mapping C-row + FR-014 audit + SC-003 no-regression`.

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
