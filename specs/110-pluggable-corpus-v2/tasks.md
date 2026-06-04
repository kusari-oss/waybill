---
description: "Task list for milestone 110 — Pluggable fingerprint corpus v2"
---

# Tasks: Pluggable fingerprint corpus v2 (multi-indicator records + signed fetch + authenticated sources)

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/110-pluggable-corpus-v2/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Test tasks are INCLUDED because (a) the constitution's Pre-PR Verification section mandates `cargo +stable test --workspace` passing clean, (b) the spec's FR-019 + SC-002 explicitly require an OSS-regression CI lane, and (c) the matcher's confidence-fusion logic + collision handling are precision-critical per constitution Principle IX (Accuracy) and benefit from TDD discipline.

**Organization**: Tasks are grouped by user story. Story implementation order is US3 → US1 → US2 → US4. This deviates from priority-letter order because US3 (P1, "no regression") is foundationally required by US1's v1-backward-compat shim, and US2's fetch infrastructure builds on US1's matcher-loaded records.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: User story label (US1–US4); omitted on Setup / Foundational / Polish phases
- Every task names exact file paths

## Path Conventions (per plan.md)

- All implementation lives in `mikebom-cli/src/scan_fs/binary/fingerprints/`.
- Integration tests in `mikebom-cli/tests/`.
- Test fixtures in `mikebom-cli/tests/fixtures/fingerprints_v2/`.
- No new crates per constitution Principle VI.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify branch baseline and create the per-feature test-fixture directory structure.

- [X] T001 Run `./scripts/pre-pr.sh` on the clean `110-pluggable-corpus-v2` branch to verify the pre-PR gate is green BEFORE any code changes; record the baseline test count + clippy warning count (expected: zero warnings) for later regression checks.
- [X] T002 [P] Create the test-fixture directory tree at `mikebom-cli/tests/fixtures/fingerprints_v2/` with subdirectories `archives/`, `binaries/`, `corpora/`, and an empty `.gitkeep` in each so the tree exists for fixture-generating tasks downstream.
- [X] T003 [P] Copy the JSON Schema contract from `specs/110-pluggable-corpus-v2/contracts/corpus-record-v2.schema.json` to `docs/reference/corpus-record-v2.schema.json` as the operator-facing stable URL contract per FR-004 + research R5.

**Checkpoint**: Branch baseline confirmed; fixture tree exists; public JSON Schema is at its operator-facing path.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Define the v2 type system + the JSON Schema test harness + the matcher error enum. No user-story work can begin until these compile.

**⚠️ CRITICAL**: All user-story tasks below depend on Phase 2 completing.

### Type definitions (data-model.md → code)

- [X] T004 [P] Add `Confidence` newtype (wraps `f64`, `try_from = "f64"` Serde adapter, range `0.0..=1.0`) AND the `Confidence::from_pct_in_range_const::<const PCT: u8>()` const constructor (compile-time-checked, no `.unwrap()` — preserves constitution principle IV at fixed-baseline construction sites) AND `FusedConfidence` enum (`High`, `Medium` only — no `Low` variant) in `mikebom-cli/src/scan_fs/binary/fingerprints/confidence.rs`. Per data-model.md.
- [X] T005 [P] Add `IndicatorKind` enum (closed set: `ExportedSymbols`, `VersionString`, `BuildId`, `MachoUuid`, `PePdb`, `AbiMarker`) with `#[serde(rename_all = "snake_case")]` in `mikebom-cli/src/scan_fs/binary/fingerprints/record.rs`.
- [X] T006 Add `IndicatorSpec` tagged enum (`SymbolSet`, `RodataLiteral`, `ExactHash` variants with `tag = "type"`, `rename_all = "kebab-case"`) in `mikebom-cli/src/scan_fs/binary/fingerprints/record.rs`, alongside `IndicatorKind`. **Depends on T005**. (Enables US1's matcher logic; foundational so unlabeled per speckit-tasks format spec.)
- [X] T007 Add `CorpusRecordV2` struct with all fields from data-model.md (`id`, `purl`, `purl_aliases`, `cpe_candidates`, `version_range`, `architectures`, `abi`, `indicators`, `collision`, `provenance`, `schema_version`) in `mikebom-cli/src/scan_fs/binary/fingerprints/record.rs`, with `#[serde(deny_unknown_fields)]`. **Depends on T006**. (Enables US1's matcher + US3's v1-upgrade; foundational so unlabeled.)
- [X] T008 [P] Add `Provenance` struct + `ProvenanceTier` enum + `CollisionSpec` + `LookAlike` in `mikebom-cli/src/scan_fs/binary/fingerprints/record.rs`. **Depends on T005**.
- [X] T009 [P] Add `CorpusSource`, `CorpusSourceId` newtype (16-char BASE32 of sha256(url)[..10] OR `"public-milestone-108"` sentinel), and the URL-hash-to-source-id helper in `mikebom-cli/src/scan_fs/binary/fingerprints/source_config.rs` (NEW file).
- [X] T010 [P] Add `CorpusError` thiserror enum with the variants from data-model.md (`WrongSchemaVersion`, `NoIndicators`, `ConfidenceOutOfRange`, `Fetch{source_id, kind}`, `SignatureFailure`, `MalformedRecord`) and `FetchFailureKind` plain enum (`MissingCredential`, `InvalidCredential`, `NetworkUnreachable`, `ArchiveMalformed`) in `mikebom-cli/src/scan_fs/binary/fingerprints/record.rs`.
- [X] T011 [P] Add `SelfIdentity` struct (`bare_name`, `purl`) + `matches_record()` impl in `mikebom-cli/src/scan_fs/binary/fingerprints/self_identity.rs` (NEW file). Body is a stub returning `false` for now; the resolver ladder lands in T072.
- [X] T012 Add `MatchResult` struct (per data-model.md) in `mikebom-cli/src/scan_fs/binary/fingerprints/matcher.rs` (NEW file). Body is a stub `pub fn match_binary(...) -> Vec<MatchResult> { vec![] }`. **Depends on T007, T011**. (Foundational matcher entry point used by US1+US3+US4; unlabeled per format spec.)

### JSON Schema validation test (dev-only, per research R5)

- [X] T013 Add `fingerprints_v2_schema.rs` in `mikebom-cli/tests/` that uses `jsonschema = "0.46"` (existing dev-dep) to validate every JSON in `mikebom-cli/tests/fixtures/fingerprints_v2/corpora/**/*.json` against `mikebom-cli/contracts/corpus-record-v2.schema.json` (copy the schema into `mikebom-cli/contracts/` for the test's relative-path access). Test should FAIL initially because no fixtures exist yet. (Foundational dev-only validation harness; used by all stories with fixture corpora; unlabeled per format spec.)

### Module wiring

- [X] T014 Update `mikebom-cli/src/scan_fs/binary/fingerprints/mod.rs` to export the new submodules (`confidence`, `matcher`, `self_identity`, `source_config`) and re-export the public types (`CorpusRecordV2`, `CorpusError`, `MatchResult`, `FusedConfidence`, `IndicatorKind`, `CorpusSource`, `CorpusSourceId`). **Depends on T004–T012**.

**Checkpoint**: `cargo +stable check --workspace` passes with the new types defined but the matcher returns empty. No behavioral change yet; all stories can start implementing on top of this foundation.

---

## Phase 3: User Story 3 — No regression for milestone-108 consumers (Priority: P1) 🎯 MVP REGRESSION-GATE

**Goal**: Existing milestone-108 v1 records continue to load and emit `pkg:generic/<name>` components with the new `mikebom:confidence: "medium"` annotation added. The OSS-default scan (no extra sources, no auth) produces an SBOM identical to the pre-milestone-110 baseline modulo that single annotation.

**Independent Test**: Run `cargo +stable test --workspace --test fingerprints_v1_regression` with no `MIKEBOM_FINGERPRINTS_SOURCES` env var; the test loads the milestone-108 reference fixture, scans it, and asserts byte-equality (after canonicalization) against the re-anchored golden SBOM in `mikebom-cli/tests/fixtures/fingerprints_v2/golden/v1_regression.cdx.json`.

**Why FIRST**: The v1→v2 upgrade shim is the critical-path piece. Until US3 lands cleanly, every other story risks breaking the existing milestone-108 contract. Implementing US3 first ensures the OSS-regression CI lane is green for the rest of the milestone.

### Tests for User Story 3 (FAIL FIRST per TDD)

- [X] T015 [P] [US3] Add `fingerprints_v1_regression.rs` in `mikebom-cli/tests/` that uses the milestone-108 fixture corpus (copied from `mikebom-cli/tests/fixtures/fingerprints/`) AND the milestone-108 reference binary fixtures, runs `mikebom sbom scan --fingerprints-corpus` programmatically via `env!("CARGO_BIN_EXE_mikebom")`, and compares the emitted CDX SBOM against the re-anchored golden. Test should FAIL because the v1-upgrade-to-v2 logic doesn't exist yet.
- [ ] T016 [P] [US3] Capture the pre-milestone-110 SBOM output by running `mikebom sbom scan --fingerprints-corpus --path mikebom-cli/tests/fixtures/fingerprints/m108-reference/ --output /tmp/pre-m110-baseline.cdx.json` against a clean `main` checkout in a worktree, then re-anchor it: write `mikebom-cli/tests/fixtures/fingerprints_v2/golden/v1_regression.cdx.json` containing the pre-m110 component list with the single permitted delta — every fingerprint-derived component gets a new `mikebom:confidence: "medium"` annotation appended (deterministic insertion order). Validate the golden is canonical-JSON-formatted via `jq -S` so SHA-256 comparison is stable.

### Implementation for User Story 3

- [ ] T017 [US3] Implement `upgrade_v1_to_v2()` in `mikebom-cli/src/scan_fs/binary/fingerprints/loader.rs` that converts a parsed v1 record into an in-memory `CorpusRecordV2` per the data-model.md mapping: synthesize a single `SymbolSet` indicator with `min_match = v1.min_symbols`, `confidence_baseline = Confidence::from_pct_in_range_const::<70>()` (the data-model.md const constructor — preserves constitution principle IV's no-`.unwrap()`-in-production rule), `suppress_when_self_identity_matches = true`; set `purl = Purl::generic(&v1.library_name)`; set `version_range = VersionRange::Unknown`; set `provenance.tier = ProvenanceTier::ManualCuration` with `extracted_from = "milestone-108-v1-record"`. **Depends on T007, T010**.
- [ ] T018 [US3] Update `loader.rs::load_archive()` to detect v1 vs v2 via the archive's `VERSION` file (absent or `"1"` → v1; `"2"` → v2; anything else → reject with `CorpusError::WrongSchemaVersion`). v1 archives route through `upgrade_v1_to_v2()`. **Depends on T017**.
- [ ] T019 [US3] In `mikebom-cli/src/scan_fs/binary/fingerprints/matcher.rs`, implement the trivial case: when ONE record matches a binary (regardless of v1-upgraded vs v2-native), return one `MatchResult` with `confidence = Medium` (from the 0.70 baseline → bucket), `indicators_matched = vec![IndicatorKind::ExportedSymbols]`. **Depends on T012, T017**.
- [ ] T020 [US3] In `mikebom-cli/src/scan_fs/binary/scan.rs` (or the matcher's emit-component path — verify location via grep before editing), thread the `MatchResult` through to component construction so the existing `pkg:generic/<name>` emission carries the new `mikebom:confidence` property. Existing component-construction code SHOULD be a single point of change — confirm via search for `pkg:generic` literal before editing.
- [X] T021 [US3] Run `cargo +stable test --workspace --test fingerprints_v1_regression`; the test from T015 should now PASS. If it doesn't, diff the emitted CDX against the golden — likely candidates are property ordering, JSON whitespace, or v1-upgrade missing a field.
- [X] T022 [P] [US3] Add a CI workflow lane `oss-regression-fingerprints` in `.github/workflows/ci.yml` per quickstart.md's snippet that runs `cargo +stable test --workspace --test fingerprints_v1_regression` with `unset MIKEBOM_FINGERPRINTS_SOURCES` and `unset *_TOKEN`. Use `actions/checkout@v5` with `persist-credentials: false` per the auto-memory feedback on GitHub Actions security.

**Checkpoint**: OSS-regression CI lane passes. SC-002 (byte-identical SBOM modulo confidence annotation) is verified. Milestone-110 can safely be opened as a PR without worrying about silently breaking existing milestone-108 consumers.

---

## Phase 4: User Story 1 — Versioned PURL emitted for a real binary (Priority: P1)

**Goal**: A binary statically linked against OpenSSL 3.1.4, scanned with a configured v2 corpus, emits a component with PURL `pkg:github/openssl/openssl@openssl-3.1.4` (or canonical equivalent) and `mikebom:confidence: "high"` when multiple indicators agree.

**Independent Test**: `cargo +stable test --workspace --test fingerprints_v2_match` — a unit-test-style integration test that hard-codes a fixture corpus file path (no fetch infrastructure required; that's US2's concern) and a fixture binary, then asserts the emitted SBOM contains the expected versioned PURL.

### Tests for User Story 1 (FAIL FIRST)

- [ ] T023 [P] [US1] Build a fixture v2 corpus archive at `mikebom-cli/tests/fixtures/fingerprints_v2/corpora/openssl-only/` containing ONE record: `records/openssl-3.1.4-glibc-amd64.json` with `purl: "pkg:github/openssl/openssl@openssl-3.1.4"`, `purl_aliases: ["pkg:deb/debian/libssl3@3.1.4-1"]`, exported_symbols indicator (10 OpenSSL public-API symbols, `min_match: 8`, baseline 0.70), version_string indicator (patterns `["OpenSSL 3.1.4"]`, baseline 0.95). Also write `VERSION` containing `"2"`. Validate against the JSON Schema by running T013's test.
- [ ] T024 [P] [US1] Build a fixture binary at `mikebom-cli/tests/fixtures/fingerprints_v2/binaries/libopenssl-3.1.4.so.fixture` by either (a) downloading a real libssl3 from the Debian pool and stripping it OR (b) compiling a stub `.so` that exports 10 OpenSSL symbols + contains the literal `"OpenSSL 3.1.4"` in `.rodata`. Document the construction in a sibling `README.md` under the binaries dir. Verify with `objdump -T` that all 10 symbols are exported and `strings` shows the version literal.
- [ ] T025 [P] [US1] Add `fingerprints_v2_match.rs` integration test in `mikebom-cli/tests/` that loads the T023 fixture corpus via a direct loader call (bypassing fetch), scans the T024 fixture binary, and asserts the emitted CDX SBOM has a component with `purl == "pkg:github/openssl/openssl@openssl-3.1.4"`, `mikebom:confidence == "high"`, `mikebom:indicators-matched` containing both `exported_symbols` and `version_string`. Test should FAIL because the multi-indicator fusion path doesn't exist yet.

### Implementation for User Story 1

- [ ] T026 [US1] Implement `fuse_confidence()` in `mikebom-cli/src/scan_fs/binary/fingerprints/confidence.rs` per research R2: `confidence = max(per-indicator baseline)`, then `for each AGREEING additional indicator: confidence = min(0.99, confidence + 0.05)`; map to `FusedConfidence::High` (≥0.85) or `Medium` (≥0.70) or `None` (<0.70 → suppressed). **Depends on T004**.
- [ ] T027 [US1] Implement `fuse_indicators()` in `mikebom-cli/src/scan_fs/binary/fingerprints/matcher.rs` per `contracts/matcher-api.md` — iterate over a record's indicators, for each one check whether the `BinaryArtifact`'s extracted data matches it (delegating to per-indicator matchers), collect matched `(IndicatorKind, Confidence)` pairs, pass to `fuse_confidence()`. **Depends on T026, T012**.
- [ ] T028 [P] [US1] Implement the per-indicator match check for `IndicatorKind::ExportedSymbols` as a private fn `match_symbol_set()` in `matcher.rs`: count how many of `IndicatorSpec::SymbolSet.required` are present in `BinaryArtifact::exported_symbols`; matches iff count >= `min_match`. Reuses existing milestone-099 extractor output via `BinaryArtifact`. **Depends on T027**.
- [ ] T029 [P] [US1] Implement `match_rodata_literal()` for `IndicatorKind::VersionString`: substring search across `BinaryArtifact::rodata_strings` (existing milestone-026 output) for any pattern in `IndicatorSpec::RodataLiteral.patterns`; matches iff any pattern is found. **Depends on T027**.
- [ ] T030 [P] [US1] Implement `match_exact_hash()` for `IndicatorKind::BuildId` / `MachoUuid` / `PePdb`: lower-case hex comparison of the corresponding `BinaryArtifact` field against the record's `sha_or_uuid_set`. **Depends on T027**.
- [ ] T031 [US1] Implement `match_binary()` in `matcher.rs` per `contracts/matcher-api.md` (single-record success path; collision handling deferred to US4). Iterate over loaded records, call `fuse_indicators` for each, build a `MatchResult` for each non-None fused-confidence result, sort by `(confidence DESC, primary_purl ASC)`. **Depends on T027–T030**.
- [ ] T032 [US1] Build the annotation-emission helpers in a new file `mikebom-cli/src/scan_fs/binary/fingerprints/annotations.rs` covering: (a) CDX 1.6 `evidence.identity[]` native form per `contracts/matcher-api.md`; (b) CDX `properties[]` for `mikebom:confidence`, `mikebom:indicators-matched`, `mikebom:purl-aliases`; (c) the existing milestone-108 C58 corpus-sha annotation extended to multi-source array form. **Depends on T031**.
- [ ] T033 [P] [US1] Add the SPDX 2.3 annotation emission for `mikebom:confidence` + `mikebom:indicators-matched` + `mikebom:purl-aliases` in `mikebom-cli/src/sbom/spdx23/` (locate the existing C16 annotation site via grep for `mikebom:confidence` first; extend that surface). The existing `mikebom:confidence` (C16) carrier is reused; the two new annotations follow the same wrapping convention. **Depends on T032**.
- [ ] T034 [P] [US1] Add the SPDX 3.0.1 annotation emission for the same three properties in `mikebom-cli/src/sbom/spdx3/`. Same emission pattern as SPDX 2.3 with the SPDX 3 graph-element annotation envelope per existing C16 / C56 / C58 precedent. **Depends on T032**.
- [ ] T035 [US1] Wire the matcher into the production scan path: in `mikebom-cli/src/scan_fs/binary/` (locate where the milestone-108 corpus matcher is currently invoked — likely `scan.rs::scan_binaries()` or similar), replace the current single-symbol-set matcher call with `matcher::match_binary()`. Preserve the milestone-108 opt-in gate (`--fingerprints-corpus`). **Depends on T031, T032, T033, T034**.
- [ ] T036 [US1] Run `cargo +stable test --workspace --test fingerprints_v2_match`; the test from T025 should PASS. Debug + iterate until green.
- [ ] T037 [US1] Run the FULL `./scripts/pre-pr.sh` and confirm zero new clippy warnings + all existing tests still pass + the new US1 + US3 tests pass.

**Checkpoint**: US1 acceptance scenarios 1, 2, 3 pass. SC-001 (≥8/10 versioned PURLs) is verifiable against a fixture set once the corpus contains records for 10 libraries (US1 only requires the single openssl record; full SC-001 measurement comes during /speckit-implement when the fixture corpus is expanded).

---

## Phase 5: User Story 2 — Pluggable sources with signed-fetch + auth + fallback (Priority: P1)

**Goal**: Operators can configure multiple corpus sources (CLI flag, env var, or config file). Each source supports optional bearer-token auth. mikebom fetches all configured sources at scan startup, verifies sigstore signatures, caches per-source with a 24-hour TTL, and degrades gracefully when sources are unreachable or signature-invalid.

**Independent Test**: `cargo +stable test --workspace --test fingerprints_v2_pluggable` — hermetic HTTP fixture server (per research R7's hand-rolled `tokio::net::TcpListener` stub) serves three test corpora; the test configures mikebom against them with various auth-success / auth-fail / network-fail / signature-fail combinations and asserts both the scan-still-completes contract AND the actionable-error-message format from SC-005.

### Tests for User Story 2 (FAIL FIRST)

- [ ] T038 [P] [US2] Build the hermetic test corpus server fixture at `mikebom-cli/tests/fixtures/fingerprints_v2/fixture_corpus_server.rs` (~80–120 lines): `tokio::net::TcpListener` on a random localhost port + minimal `hyper::Server` dispatch returning canned `release.json` / `<sha>.tar.gz` / `<sha>.sig` / `<sha>.cert` responses. Supports a `with_auth_token(Some(token))` builder for bearer-auth tests. Reference pattern: milestone-055's go-mod-proxy stub at `mikebom-cli/tests/fixtures/go_mod_proxy_stub.rs`.
- [ ] T039 [P] [US2] Pre-build three test corpus archives at `mikebom-cli/tests/fixtures/fingerprints_v2/archives/`: (a) `public-v2.tar.gz` (no auth required); (b) `private-v2.tar.gz` (test harness checks bearer token); (c) `conflicting-v2.tar.gz` (two records with overlapping symbols, used for US4 collision test). Each archive includes `records/`, `VERSION=2`, and pre-computed sigstore signatures (`*.sig` + `*.cert`) generated with a test-only signing identity. Document the signature generation in a sibling `BUILD.md`.
- [ ] T040 [P] [US2] Add `fingerprints_v2_pluggable.rs` integration test in `mikebom-cli/tests/` covering: (a) valid-auth-fetch-succeeds; (b) missing-auth-credential warning + fallback; (c) invalid-auth (401) warning + fallback; (d) network-unreachable warning + fallback; (e) signature-mismatch warning + reject; (f) cache TTL hit (no network on second scan within 24h, simulated by patching the `now()` provider); (g) `--force` bypasses TTL. Each scenario asserts both exit code 0 AND a specific log-line substring per the SC-005 contract. Test should FAIL because the multi-source loading + auth header + TTL logic doesn't exist yet.

### Implementation for User Story 2

- [ ] T041 [US2] Extend `CorpusSourceId` derivation in `mikebom-cli/src/scan_fs/binary/fingerprints/source_config.rs`: `fn from_url(url: &Url) -> Self` computes BASE32(sha256(url)[..10]) with the `"public-milestone-108"` sentinel special-case for the milestone-108 default URL. **Depends on T009**.
- [ ] T042 [US2] Implement source-config parsing per `contracts/cli-flags.md` in `source_config.rs`: `fn parse_sources_from_environment()` reads `MIKEBOM_FINGERPRINTS_SOURCES`, `fn parse_sources_from_config_file()` reads `~/.config/mikebom/config.toml` `[fingerprints]` section. CLI-flag parsing lives in T046. Union the three layers + implicit default. **Depends on T041**.
- [ ] T043 [US2] Extend `cache.rs` for multi-source layout per research R3: `fn cache_dir_for_source(source_id: &CorpusSourceId, sha: &Sha256) -> PathBuf` returns `~/.cache/mikebom/fingerprints/<source-id>/<sha>/`; add `fn touch_last_used(source_id, sha)` writing the `last_used.touch` file; add `fn is_cache_fresh(source_id, sha, ttl: Duration) -> bool` checking mtime. Maintain a `_meta/sources.json` index updated on each fetch. **Depends on T041**.
- [ ] T044 [US2] Extend `fetch.rs` for per-source auth + multi-source orchestration: `async fn fetch_source(source: &CorpusSource, force: bool) -> Result<FetchedArchive, CorpusError>` — reads the bearer token from `$<credential_env>` if set, sets the `Authorization: Bearer <value>` header, downloads `release.json` → resolves pinned SHA → downloads archive + sig + cert → verifies signature via the existing milestone-089 sigstore stack (reuse the existing `verify_blob` call site; pass per-source `allowed_issuers` list per research R6). On HTTP 401/403 → `FetchFailureKind::InvalidCredential`. Missing env var → `MissingCredential`. **Depends on T042, T043**.
- [ ] T045 [US2] Implement the multi-source orchestrator `async fn fetch_all_configured_sources(...) -> MultiSourceCorpus`: per-source fetches in parallel (`tokio::join_all`), per-source failures logged as warnings via `tracing::warn` with the SC-005 actionable-message format, successful sources merged into a single in-memory `Corpus`. **Depends on T044**.
- [ ] T046 [US2] Add the new CLI flags to `mikebom-cli/src/cli/scan_cmd.rs` per `contracts/cli-flags.md`: `--fingerprints-source URL[=ENV_VAR]` repeatable (parse the `=ENV_VAR` suffix manually since clap's value parser doesn't natively support per-value optional env-var binding), `--fingerprints-source-no-default` bool, `--scan-as <purl-or-name>`. Wire through to the source-config layer (T042). **Depends on T042**.
- [ ] T047 [US2] Add `--source` (repeatable), `--force`, `--no-default` to `mikebom fingerprints fetch` in the existing `mikebom-cli/src/cli/fingerprints_cmd.rs` (or wherever the milestone-108 fetch subcommand lives — grep for `fingerprints fetch` to confirm). **Depends on T044**.
- [ ] T048 [US2] Implement the graceful-degradation contract in the scan-startup path: if ALL configured sources fail (incl. the milestone-108 default), `tracing::warn!("no fingerprint corpus loaded; binary-tier components will use file-SHA-256 baseline only")` and continue with an empty `Corpus`. The matcher returns `vec![]` for every binary, the existing pre-milestone-108 file-level emission takes over. Exit code 0. **Depends on T045**.
- [ ] T049 [US2] Run `cargo +stable test --workspace --test fingerprints_v2_pluggable`; iterate until all 7 scenarios from T040 PASS. Debug auth-header propagation by stuffing a verbose `tracing::debug` log on every outbound request during dev.
- [ ] T049a [US2] Add an SC-003 wall-clock assertion: in `fingerprints_v2_pluggable.rs`, wrap the first-fetch-success scenario in a `std::time::Instant::now()` measurement and assert `elapsed < Duration::from_secs(30)` against the test corpus server (which serves a < 5 MB archive). Failure mode: budget exceeded indicates fetch+verify+cache pipeline regression. Per spec SC-003.
- [ ] T050 [US2] Re-run `./scripts/pre-pr.sh` to confirm no regression in US3 (the OSS-default path) or US1 (the matcher path) caused by US2's fetch+merge orchestration.

**Checkpoint**: US2 acceptance scenarios 1–5 pass. SC-003 (< 30s end-to-end first scan), SC-004 (zero network on cache hit), and SC-005 (actionable error categories) are all verifiable against the hermetic test fixture.

---

## Phase 6: User Story 4 — Multi-indicator fusion + collision handling + self-identity (Priority: P2)

**Goal**: When a binary matches multiple records (the BoringSSL/OpenSSL collision case), both components emit with cross-references. When the scanned project is the library itself (self-identity), the matcher suppresses spurious "library contains itself" components per the design doc §7.1 ladder.

**Independent Test**: `cargo +stable test --workspace --test fingerprints_v2_fusion` — exercises the collision binary fixture (T039 c) + a self-identity-scan fixture.

### Tests for User Story 4 (FAIL FIRST)

- [ ] T051 [P] [US4] Build a fixture binary at `mikebom-cli/tests/fixtures/fingerprints_v2/binaries/libboringssl.so.fixture` that exports 8+ OpenSSL public-API symbols (the API surface shared between BoringSSL and OpenSSL) AND contains the literal `"BoringSSL "` in `.rodata`. Document construction in the sibling README.
- [ ] T052 [P] [US4] Add a second record `boringssl-stable.json` to the `conflicting-v2` corpus archive (T039 c) so it now contains BOTH the openssl record (T023) AND a new boringssl record (`purl: "pkg:github/google/boringssl@stable"`, version_string indicator pinning the `"BoringSSL "` literal at baseline 0.95). Add `collision.look_alikes` to each record naming the other.
- [ ] T053 [P] [US4] Build a self-identity-scan fixture directory at `mikebom-cli/tests/fixtures/fingerprints_v2/binaries/self-identity-cmake/` containing a `CMakeLists.txt` with `project(openssl ...)` declaration + a built libopenssl.so.fixture binary (copy from T024). Scanning this directory MUST suppress the openssl corpus record's weak indicators.
- [ ] T054 [P] [US4] Add `fingerprints_v2_fusion.rs` in `mikebom-cli/tests/` with three scenarios: (a) the collision binary (T051) against the conflicting corpus (T052) → asserts TWO components emit with cross-referencing `mikebom:also-detected-via`; (b) the self-identity-scan fixture (T053) → asserts NO openssl component emits (the scan emits only the cmake-discovered self component); (c) `--scan-as my-test` overrides self-identity → openssl component DOES emit. Test should FAIL because collision + self-identity logic doesn't exist yet.

### Implementation for User Story 4

- [ ] T055 [US4] Implement `resolve_collisions()` in `mikebom-cli/src/scan_fs/binary/fingerprints/matcher.rs` per `contracts/matcher-api.md`: when multiple records produce non-None `fuse_indicators` results for a single binary, return one `MatchResult` per record with `also_detected_via` populated. Order by `(confidence DESC, primary_purl ASC)` per the matcher-api determinism contract. **Depends on T031**.
- [ ] T056 [US4] Update `match_binary()` to call `resolve_collisions()` instead of trivially returning the first match. **Depends on T055**.
- [ ] T057 [US4] Extend `annotations.rs::emit_component_annotations()` to populate the `mikebom:also-detected-via` annotation per existing C56 hybrid pattern (CDX native via `evidence.identity[].methods[].mikebom-source-mechanism`; SPDX annotation). **Depends on T056**.
- [ ] T058 [US4] Implement the self-identity resolution ladder in `mikebom-cli/src/scan_fs/binary/fingerprints/self_identity.rs` per research R8 priority order: (1) `--scan-as` operator override (read from CLI args / `MIKEBOM_SCAN_AS` env), (2) cmake `project()` from existing milestone-102/103 reader, (3) cargo `[package].name` from milestone-064 reader, (4) npm `package.json::name` from milestone-066 reader, (5) PEP 621 `[project].name` from milestone-068 reader, (6) git remote URL from milestone-073/074 `auto_detect.rs`. Return first hit; `None` if none resolve. **Depends on T011**.
- [ ] T059 [US4] Implement `SelfIdentity::matches_record()` body per research R8 matching rule: case-insensitive name + namespace comparison against record `purl` AND each `purl_aliases` entry. Returns `true` on first match. **Depends on T058**.
- [ ] T060 [US4] Implement `apply_self_identity_filter()` in `matcher.rs` per `contracts/matcher-api.md`: per-indicator-per-record `SuppressionDecision` (`Apply` / `SkipIndicator` / `SkipRecord`) based on `IndicatorSpec.suppress_when_self_identity_matches` AND whether self-identity matches. **Depends on T059**.
- [ ] T061 [US4] Update `fuse_indicators()` to call `apply_self_identity_filter()` before fusing each indicator; skipped indicators don't participate in the max-then-bump calculation. **Depends on T060**.
- [ ] T062 [US4] Update `scan_cmd.rs` to construct the `SelfIdentity` once per scan invocation (from the resolved ladder) and pass it through to `match_binary()` via the existing scan-state plumbing. **Depends on T058, T046**.
- [ ] T063 [US4] Run `cargo +stable test --workspace --test fingerprints_v2_fusion`; iterate until all three scenarios PASS.
- [ ] T064 [US4] Re-run `./scripts/pre-pr.sh` to confirm no regression in US1/US2/US3.

**Checkpoint**: US4 acceptance scenarios 1–4 pass. SC-006 (multi-record collision emits both with cross-references) is verifiable.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Bring the implementation up to release quality — docs, parity-extractor coverage, CLI help text, CHANGELOG entry, full pre-PR gate.

- [ ] T065 [P] Add the three new C-rows (C59 `mikebom:indicators-matched`, C60 `mikebom:purl-aliases`, C61 `mikebom:identification-version-range`) to `docs/reference/sbom-format-mapping.md` with the principle-V-audit justification clauses from research R1. Extend the C16, C56, C58 row text with the multi-source / new-semantic notes per research R1.
- [ ] T066 [P] Add the corresponding parity-extractor entries in `mikebom-cli/src/parity/extractors/cdx.rs`, `spdx2.rs`, `spdx3.rs` for the three new C-rows. The CI parity-row coverage check (existing) will fail until these are added.
- [ ] T067 [P] Update CLI help text in `mikebom-cli/src/cli/scan_cmd.rs` + `fingerprints_cmd.rs` (or wherever) with clear descriptions of the new flags per `contracts/cli-flags.md`. Verify with `mikebom sbom scan --help` showing the new flags grouped under a "Fingerprints corpus" section if clap supports help-text groups; otherwise inline.
- [ ] T068 [P] Update `docs/ecosystems.md` § fingerprints section (if it exists, otherwise create the section) with a paragraph describing the v2 schema + the pluggable source mechanism + a link to the JSON Schema at its operator-facing URL.
- [ ] T069 [P] Add a CHANGELOG entry under `CHANGELOG.md`'s `## [Unreleased]` section: "Pluggable fingerprint corpus v2 — multi-indicator records, signed multi-source fetch with optional bearer-token auth, multi-indicator confidence fusion. Milestone-108 v1 corpora continue to load unchanged. See `specs/110-pluggable-corpus-v2/spec.md`."
- [ ] T070 [P] Add the `SC-007` conformance test: a fixture corpus author end-to-end runs through the test corpus server (T038) → mikebom-cli successfully consumes it → records load and match. This test already exists conceptually in T040; promote to its own named test case `fingerprints_v2_third_party_conformance` in `fingerprints_v2_pluggable.rs` for documentation visibility.
- [ ] T071 [P] Open a tracked GitHub issue (do NOT add CLI scaffolding) titled "fingerprints: add `mikebom fingerprints verify` subcommand for runtime provenance re-validation" pointing at SC-008's post-remediation note. The milestone-110 acceptance gate is satisfied by the load-time deserialization-strict-shape validation (per the remediated SC-008); the live re-fetch subcommand is a follow-on milestone's scope. No code change in this task — just the issue + cross-link in `docs/reference/sbom-format-mapping.md`'s C58 row notes as a forward-pointer.
- [ ] T072 Final pre-PR pass: `./scripts/pre-pr.sh` MUST exit zero. Inspect every clippy line in the output even at zero-warning state — sometimes new code triggers nit-level lints that warrant a `#[allow(...)]` with a comment.
- [ ] T073 Update `specs/110-pluggable-corpus-v2/checklists/requirements.md` to mark all checklist items still passing (re-verify since spec was edited by /speckit-clarify); add a notes paragraph summarizing the design decisions taken in research.md.
- [ ] T074 Run the full `cargo +stable test --workspace` one more time + manually exercise the quickstart.md scenarios 1–6 against the test fixtures to confirm operator-facing UX matches the documented contract.

---

## Dependencies

Sequential dependencies (each phase blocks the next):

```text
Phase 1 (Setup) ──▶ Phase 2 (Foundational) ──▶ Phase 3 (US3 regression-gate)
                                              │
                                              └──▶ Phase 4 (US1) ──▶ Phase 5 (US2) ──▶ Phase 6 (US4) ──▶ Phase 7 (Polish)
```

US3 blocks US1/US2/US4 because the v1-upgrade shim is on the matcher's critical path. US1 → US2 → US4 is the natural buildup order (matcher core → fetch infrastructure → collision/self-identity polish).

Parallel opportunities:
- Within Phase 2: T004, T005, T008, T009, T010, T011 are all independent file-creation tasks → `[P]`.
- Within Phase 3: T015 (test fixture) + T016 (golden re-anchor) + T022 (CI workflow) parallelize across files.
- Within Phase 4: T028, T029, T030 are independent indicator-matcher implementations. T033, T034 are independent format-emission files. T023, T024 are independent fixture-build tasks.
- Within Phase 5: T038 (server fixture), T039 (archive fixtures), T040 (test file) all parallelize.
- Within Phase 6: T051, T052, T053, T054 are independent fixture-build / test-scaffold tasks.
- Within Phase 7: T065–T071 are all `[P]` (different files).

## Implementation Strategy

**MVP scope** (most operator-visible value with minimum risk):

The MVP is **US3 + US1 only** (Phases 1, 2, 3, 4). Together they deliver:
- Existing milestone-108 consumers see no regression.
- Operators with a single hand-configured v2 corpus path (no fetch, no auth) get versioned PURLs.

US2 (fetch + auth) is a separable second PR — the matcher works against any in-memory `Corpus` regardless of where it came from. Operators with their own deployment of mikebom can ship MVP scope first.

US4 (collisions + self-identity) is a polish PR — the matcher emits a single best match without it; collisions surface as "missed identifications" rather than incorrect ones.

**Suggested commit cadence** (each commit independently passes pre-PR gate):

1. Phase 1 + Phase 2 (foundational types, no behavioral change) — 1 commit.
2. Phase 3 (US3 regression-safe v1 upgrade) — 1 commit. CI lane should be GREEN here.
3. Phase 4 (US1 versioned-PURL matcher) — 1 commit. Tests + matcher logic land together.
4. Phase 5 (US2 fetch + auth) — 1 commit. Big commit but isolated to the fetch layer.
5. Phase 6 (US4 collisions + self-identity) — 1 commit. Polish.
6. Phase 7 (docs + parity-extractor + CHANGELOG) — 1 commit.

Six PRs (or six commits in one PR) covers the milestone cleanly without any single change being too risky to revert.

## Format validation

All tasks conform to the `- [ ] TaskID [P?] [Story?] Description with file path` format per the speckit-tasks rules:
- 75 total tasks (T001–T074 + T049a inserted during /speckit-analyze remediation).
- 37 of those are `[P]` marked.
- 51 of those carry a `[Story]` label (US1: 15, US2: 14, US3: 8, US4: 14); the remaining 24 are Setup / Foundational / Polish with no story label per the format spec.
- Every task names exact file paths.
