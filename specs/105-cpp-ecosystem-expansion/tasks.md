---
description: "Task list for milestone 105 — C/C++ Ecosystem Expansion (Phase 2)"
---

# Tasks: C/C++ Ecosystem Expansion (Phase 2)

**Input**: Design documents from `/specs/105-cpp-ecosystem-expansion/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Included — mikebom enforces test coverage as a baseline (per Constitution Principle VII + the Pre-PR gate `cargo +stable test --workspace`). Per-reader contract tests, per-format goldens, and integration tests against real corpora are all mandatory.

**Organization**: Tasks grouped by user story. Phase 1 (Setup) and Phase 2 (Foundational) MUST complete before any user story phase. The 6 user story phases can ship as 6 separate PRs after Phase 2 lands.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Maps to user stories from spec.md (e.g., US1, US2, US3, US4, US5, US6)
- Every task names exact file paths.

## Path Conventions

Single-project workspace (the mikebom Rust workspace). All source under `mikebom-cli/`; all tests under `mikebom-cli/tests/`.

---

## Phase 1: Setup

**Purpose**: Verify baseline state and resolve the in-flight prerequisite (PR #272).

- [X] T001 Verify branch checkout: confirm `git branch --show-current` returns `105-cpp-ecosystem-expansion` (the script-created branch).
- [X] T002 Confirm PR #272 (alpha.41 source-mechanism annotation) has merged to `main`, then rebase the 105 branch on the post-merge `main` head. **If #272 is still open, complete its review/merge first** — milestone 105 builds directly on the C55 parity row + 7 existing closed-enum values that #272 introduces. ✅ #272 merged 2026-05-28T00:43:39Z; rebased clean onto origin/main (615cebc); planning artifacts committed as 2e113e3.
- [X] T003 [P] Run baseline pre-PR gate: `./scripts/pre-pr.sh` MUST pass clean on the rebased branch before any work begins. Document the baseline scan-time for the existing golden fixtures (used as the SC-009 ≤5% comparator). ✅ Baseline = **2:36.64 wall-clock** for full clippy + workspace tests.

**Checkpoint**: Baseline confirmed clean. Phase 2 can begin.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Cross-cutting infrastructure used by every user story: the URL-sanitization helper refactor, the `find_package` target collector, the dedup pipeline + its parity rows, and the C56/C57 catalog entries.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### 2A — URL sanitization helper refactor (FR-016)

- [X] T004 Move `sanitize_userinfo` and its private `SanitizedUrl` struct from `mikebom-cli/src/binding/identifiers/auto_detect.rs` to a new public module `mikebom-cli/src/identifiers/sanitize.rs`. Collapse the struct into a `pub fn sanitize_userinfo(url: &str) -> Cow<'_, str>` return signature. Module-private helpers like `redact_userinfo_for_log` move with it. ✅ New module at `mikebom-cli/src/identifiers/sanitize.rs`; declared as `pub mod identifiers;` in `lib.rs` (NOT main.rs — auto_detect.rs lives in the lib tree). Behavioral refinement: empty-userinfo inputs (`https://@host/...`) now return `Cow::Borrowed` since there's no real credential to strip — preserves FR-016 log-gating semantics.
- [X] T005 [P] Bump the redaction-event log from `tracing::info!` to `tracing::warn!` in `mikebom-cli/src/identifiers/sanitize.rs` per FR-016. Update the log message structured fields per `contracts/credential-redaction.md`. ✅ Bumped at the 3 call sites in `auto_detect.rs` (the helper itself stays log-free; per-callsite logging preserves the `scheme=repo`/`scheme=git` context fields).
- [X] T006 Update the 2 existing production call sites in `mikebom-cli/src/binding/identifiers/auto_detect.rs` (`auto_detect_repo_identifier`, `auto_detect_build_tier_identifiers`) to import from the new path and consume the `Cow<'_, str>` return type. Behavior MUST be byte-identical to the pre-refactor version (apart from log level). ✅ Both functions updated; pre-PR gate passes clean.
- [X] T007 [P] Update the 9 existing unit tests in `mikebom-cli/src/binding/identifiers/auto_detect.rs` (lines 1214, 1225, 1241, 1248, 1256, 1264, 1274, 1291, 1293) to test the new public helper. Move them to a fresh test module in `mikebom-cli/src/identifiers/sanitize.rs`. ✅ 12 tests moved (8 sanitize_userinfo + 4 redact_userinfo_for_log); old test block in `auto_detect.rs` removed with a forwarding comment.

### 2B — `find_package` target collector (used by FR-008a)

- [X] T008 Extend `mikebom-cli/src/scan_fs/package_db/cmake.rs::parse_cmake_file` to **collect** every `find_package(<target> ...)` target name into a new `ScanContext::find_package_targets: BTreeSet<String>` field (case-folded). Component emission from `find_package` MUST remain disabled — preserves milestone 102's FR-007. The existing regression test `find_package_does_not_emit_components` at `cmake.rs:411` MUST continue passing. ✅ Implemented as `pub fn collect_find_package_targets(scan_root: &Path) -> BTreeSet<String>` (separate fn, not threaded through a ScanContext struct — simpler and avoids API churn on `read()`). FR-007 regression test still passes; new `collect_does_not_emit_components_invariant` test explicitly asserts the read/collect separation. `#[allow(dead_code)]` on the public fn until US6/T089 wires the consumer.
- [X] T009 [P] Extend `cmake.rs` to additionally collect `add_library(<alias>::<alias> ALIAS <target>)` declarations into the same set (target-alias resolution for FR-008a). Dynamic aliases set inside macro/function bodies remain not chased. ✅ ALIAS regex handles namespaced + plain forms; both the namespace prefix AND the alias target name go into the set so submodule classification matches either form.
- [X] T010 [P] Add unit tests in `mikebom-cli/src/scan_fs/package_db/cmake.rs` (or a fresh `cmake_find_package_collector_tests.rs`) covering: (a) basic `find_package` collection, (b) `ALIAS` resolution, (c) `find_package_does_not_emit_components` still green. ✅ 7 new tests: `collect_find_package_basic`, `_case_folded`, `_add_library_alias_namespaced`, `_add_library_alias_unnamespaced`, `_combined_find_package_and_alias`, `_returns_empty_when_no_cmake`, `_does_not_emit_components_invariant`. All passing.

### 2C — `SourceMechanism` enum + DetectionRecord + DedupResult types

- [X] T011 Create `mikebom-cli/src/scan_fs/dedup.rs` with the data shapes from `data-model.md`: `enum SourceMechanism` (13 variants per the plan; 7 existing + 6 new), `struct DetectionRecord`, `struct DedupResult`, `struct DedupedComponent`. Derive `Debug`, `Clone`, `PartialEq`, `Eq`. Provide `SourceMechanism::canonical_str() -> &'static str` returning the C55 closed-enum value. ✅ Module created. DetectionRecord/DedupedComponent drop PartialEq/Eq derives (PackageDbEntry doesn't implement them); tests assert field-by-field. SourceMechanism derives Ord; variant declaration order is load-bearing (VcpkgManifest<VcpkgClassic per US5 scenario 2).
- [X] T012 Implement `dedup::precedence_rank(record: &DetectionRecord) -> u8` per the two-stage table in `data-model.md` (Tier > PURL specificity). Smaller value wins. Total order; no ties unsettled at this stage. ✅ Returns u16 (packs tier << 8 \| purl_rank). 3 tiers (manifest=0, mixed=1, filesystem=2) × 4 PURL-specificity ranks. Stage 3 tie-break uses enum Ord (replaces canonical_str lex tie-break — see T011 note).
- [X] T013 Implement `dedup::dedup(records: Vec<DetectionRecord>) -> DedupResult` per the algorithm in `contracts/dedup-precedence.md`. Sort → group-by canonical PURL → winner-selection → sorted losers list. ✅ Uses BTreeMap for deterministic group iteration; per-group sort by (precedence_rank, source_mechanism enum order); losers sorted by enum order for `mikebom:also-detected-via` determinism.
- [X] T014 [P] Unit tests in `mikebom-cli/src/scan_fs/dedup.rs` covering the precedence table: (a) manifest-mode > filesystem-derived, (b) PURL-specificity tie-break within a tier, (c) lexicographic discriminant tie-break as the safety net. ✅ 7 tests: `precedence_manifest_outranks_filesystem`, `_outranks_mixed_outranks_filesystem`, `_purl_specificity_beats_within_same_tier`, `_lex_tie_break_when_tier_and_purl_match`, `single_reader_emits_no_also_detected_via`, `dedup_is_input_order_invariant` (3 input orderings × 2 groups × deterministic output), `canonical_str_covers_all_13_variants`. All passing.

### 2D — Parity wiring for C56 + C57 (R6 / R7)

- [ ] T015 Add `pub fn c56_cdx(component: &Value) -> BTreeSet<String>` to `mikebom-cli/src/parity/extractors/cdx.rs` per `contracts/dedup-precedence.md`'s parity-extractor contract (reads `evidence.identity[0].methods[].mikebom-source-mechanism` minus the first entry; returns BTreeSet of losers).
- [ ] T016 [P] Add `pub fn c56_spdx23(package: &Value) -> BTreeSet<String>` to `mikebom-cli/src/parity/extractors/spdx2.rs` (reads `mikebom:also-detected-via` annotation; parses as JSON array; flattens).
- [ ] T017 [P] Add `pub fn c56_spdx3(package: &Value) -> BTreeSet<String>` to `mikebom-cli/src/parity/extractors/spdx3.rs` (same shape as SPDX 2.3).
- [ ] T018 Add `pub fn c57_cdx(component: &Value) -> BTreeSet<String>` to `cdx.rs` — extracts `mikebom:build-reference` from CDX properties. Mirror the existing `c55_cdx` pattern.
- [ ] T019 [P] Add `pub fn c57_spdx23(package: &Value) -> BTreeSet<String>` to `spdx2.rs`.
- [ ] T020 [P] Add `pub fn c57_spdx3(package: &Value) -> BTreeSet<String>` to `spdx3.rs`.
- [ ] T021 Wire the new C56 + C57 ParityExtractor rows in `mikebom-cli/src/parity/extractors/mod.rs`: add to the three `use` statements (cdx/spdx2/spdx3 imports) and append two `ParityExtractor { row_id, label, cdx, spdx23, spdx3, directional: SymmetricEqual, order_sensitive: false }` entries after the existing C55 row at line 307. Mirror the PR #272 wiring pattern exactly.

### 2E — CDX hybrid emission for `also-detected-via` (R1 + R7)

- [ ] T022 Extend `mikebom-cli/src/generate/cyclonedx/evidence.rs` (around lines 31–94) to consume a `DedupedComponent::also_detected_via` list. For each detection record, emit a `methods[]` entry with `{technique: "manifest-analysis", confidence, mikebom-source-mechanism: <value>}`. Winning entry first (confidence 0.95), losing entries follow (confidence 0.85). Use the existing `serde_json::json!()` macro construction — additive change only, no struct rewrites.
- [ ] T023 [P] Wire the C56 parity extractor for CDX to read from the native `evidence.identity[0].methods[*].mikebom-source-mechanism` path only (committed: research R1's recommended evidence-only path for CDX). Do NOT emit a redundant `mikebom:also-detected-via` property on CDX components — the native evidence-identity field is the sole CDX home for this signal. SPDX 2.3 and SPDX 3.0.1 emit the `mikebom:also-detected-via` annotation as their sole home (no native equivalent per research R1). The parity round-trip test guarantees byte-identity across the two emission paths. Document this asymmetry in `docs/reference/sbom-format-mapping.md`'s C56 row narrative.

### 2F — Dedup determinism test (SC-010)

- [ ] T024 Create fixture `mikebom-cli/tests/fixtures/golden_inputs/dedup_collision/` containing a synthetic project tree where two distinct readers (e.g., `git-submodule` + `conan-recipe`) both match the same canonical PURL. Include a `.gitmodules`, a `conanfile.txt`, and a populated `third_party/<name>/` directory with a fake `.git/modules/<name>/HEAD`.
- [ ] T025 Create the determinism integration test `mikebom-cli/tests/dedup_precedence_determinism.rs`: scan the `dedup_collision` fixture once normally, then 100 times with a seeded-shuffled `walkdir::IntoIter` ordering. Assert byte-identical SBOM output across all 101 runs. Covers SC-010.

### 2G — Polyglot robustness (SC-008)

- [ ] T026 Fix the npm v1 package-lock abort surfaced during gRPC testing: change `mikebom-cli/src/scan_fs/package_db/npm/walk.rs` (or the npm dispatcher entry point) from `Err(...)` to `tracing::warn!` + skip. The scan MUST continue when a single npm reader encounters an unsupported lockfile version. Verify by re-running the gRPC scan that previously aborted with "0/1/2: package-lock.json v1 not supported".
- [ ] T027 [P] Add regression test `mikebom-cli/tests/polyglot_legacy_lockfile_robustness.rs` that creates a tmp tree containing both a valid C/C++ manifest AND a deliberately-bad `package-lock.json` (lockfileVersion=1), scans it, and asserts the C/C++ component appears in the output (i.e. the scan did not abort).

**Checkpoint**: Phase 2 complete. The dedup pipeline + sanitization helper + find_package collector + parity rows are wired and tested. The 6 user stories can now ship in parallel.

---

## Phase 3: User Story 1 — CPM.cmake (Priority: P1) 🎯 MVP

**Goal**: Modern CMake projects using `cpmaddpackage(...)` emerge with real PURLs + versions + `cpm-cmake` source-mechanism.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/golden_inputs/cpm_cmake/`; assert components `pkg:github/fmtlib/fmt@12.1.0`, `pkg:github/gabime/spdlog@1.17.0`, `pkg:github/lefticus/tools@main` all carry `mikebom:source-mechanism: "cpm-cmake"`.

### Fixture + tests

- [ ] T028 [P] [US1] Create fixture tree `mikebom-cli/tests/fixtures/golden_inputs/cpm_cmake/` with 4 sub-fixtures matching US1 acceptance scenarios: `with_git_tag/`, `version_only/`, `rolling/`, `mixed/` (each containing a synthetic CMakeLists.txt or Dependencies.cmake).
- [ ] T029 [P] [US1] Add 4 contract tests in `mikebom-cli/src/scan_fs/package_db/cmake.rs` (or a new test module): `source_mechanism_annotation_cpm_git_tag`, `_cpm_version_only`, `_cpm_rolling`, `_cpm_mixed_with_fetchcontent`. Mirror PR #272 test pattern.

### Implementation

- [ ] T030 [P] [US1] Extend `mikebom-cli/src/scan_fs/package_db/cmake.rs` with a `parse_cpm_block` function recognizing `cpmaddpackage(...)`, `cpmfindpackage(...)`, `cpmdeclarepackage(...)` call sites. Mirror the existing `parse_fetch_block` shape (lines 99–180). Extract `NAME`, `VERSION`, `GIT_TAG`, `GITHUB_REPOSITORY`, `GIT_REPOSITORY` arguments.
- [ ] T031 [US1] Implement PURL derivation per `contracts/cpm-cmake.md` table: `pkg:github/<org>/<repo>@<tag>`, `pkg:git+https://<sanitized-url>@<tag>`, or `pkg:generic/<name>@<version>` based on input combinations. Every URL passes through `sanitize_userinfo` first (FR-016).
- [ ] T032 [US1] Emit annotations: `mikebom:source-mechanism: "cpm-cmake"`, `mikebom:source-files`, `mikebom:download-url` (when GIT_REPOSITORY present), `mikebom:resolver-step: "cpm-no-version"` (when version is unknown), `mikebom:resolver-step: "cpm-rolling-tag"` (when GIT_TAG is `main`/`master`/`HEAD`).
- [ ] T033 [US1] Add `SourceMechanism::CpmCmake` to the enum (already declared in T011); wire `parse_cpm_block` output into the cmake reader's dispatch so each call site produces a `DetectionRecord`.

### Catalog + goldens

- [ ] T034 [US1] Update `docs/reference/sbom-format-mapping.md`'s C55 row to add `cpm-cmake` to the documented closed-enum list. Include the Constitution Principle V audit one-liner (no native field across CDX/SPDX 2.3/SPDX 3 — annotation is the correct home).
- [ ] T035 [US1] Generate CDX/SPDX 2.3/SPDX 3 byte-identity goldens for the cpm_cmake fixture: `mikebom-cli/tests/fixtures/golden/{cyclonedx/cpm-cmake.cdx.json, spdx-2.3/cpm-cmake.spdx.json, spdx-3/cpm-cmake.spdx3.json}`. Use `MIKEBOM_UPDATE_*_GOLDENS=1` to regen.
- [ ] T036 [US1] Run `./scripts/pre-pr.sh` clean. Open PR titled `feat(c/c++): CPM.cmake reader extension (US1 of milestone 105)`.

**Checkpoint**: US1 is shippable as a standalone PR. Modern CPM-using C++ projects scan cleanly.

---

## Phase 4: User Story 2 — `conanfile.py` (Priority: P1)

**Goal**: Modern Conan 2.x projects with `conanfile.py` emit per-recipe components with correct `lifecycle-scope` tagging.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/golden_inputs/conanfile_py/`; assert `pkg:conan/zlib@1.3.1`, `pkg:conan/openssl@3.0.0`, `pkg:conan/cmake@3.27.7` emerge with correct `lifecycle-scope` values.

### Fixture + tests

- [ ] T037 [P] [US2] Create fixture tree `mikebom-cli/tests/fixtures/golden_inputs/conanfile_py/` with 4 sub-fixtures per US2 scenarios: `class_attr/`, `method_form/`, `mixed_kinds/`, `dual_recipes/` (one mixing `conanfile.txt` AND `conanfile.py`).
- [ ] T038 [P] [US2] Add 4 contract tests in `mikebom-cli/src/scan_fs/package_db/conan.rs`: `source_mechanism_annotation_conan_py_class_attr`, `_conan_py_method`, `_conan_py_lifecycle_scope`, `_conan_py_dedup_with_txt`.

### Implementation

- [ ] T039 [P] [US2] Extend `mikebom-cli/src/scan_fs/package_db/conan.rs` with `parse_py_recipe` function. Regex/AST-light line-oriented parser per `contracts/conanfile-py.md`. Recognize: `requires = (...)`, `build_requires = (...)`, `tool_requires = (...)`, `self.requires(...)`, `self.tool_requires(...)`. Support tuple/list/multiline forms.
- [ ] T040 [US2] Implement lifecycle-scope tagging per FR-004 + the table in `contracts/conanfile-py.md`: `requires` → `runtime`; `build_requires` + `tool_requires` → `build`.
- [ ] T041 [US2] Implement conditional-guard handling: when `self.requires(...)` appears inside `if self.settings.os == "Linux":` (or similar guard), emit the component with a `mikebom:lifecycle-scope-guard` annotation containing the guard source string. Best-effort — do NOT attempt to evaluate the condition.
- [ ] T042 [US2] Implement skip-and-warn for dynamic requires: when `self.requires(...)` argument is a non-literal (f-string, function call, variable reference), emit `tracing::warn!` naming the file + line and skip the dep. Test coverage: at least one skip-and-warn case in the fixtures.
- [ ] T043 [US2] Add `conanfile.py` to the conan reader's file-pattern dispatch (in addition to existing `conanfile.txt`). The existing `conan-recipe` SourceMechanism value is reused (no new enum value needed — alpha.41 already covers it).

### Catalog + goldens

- [ ] T044 [US2] Generate CDX/SPDX 2.3/SPDX 3 goldens for the conanfile_py fixture. (No `docs/reference/sbom-format-mapping.md` update needed — `conan-recipe` is already documented from alpha.41.)
- [ ] T045 [US2] Run `./scripts/pre-pr.sh` clean. Open PR titled `feat(c/c++): conanfile.py support (US2 of milestone 105)`.

**Checkpoint**: US2 is shippable. Modern Conan 2.x projects scan cleanly.

---

## Phase 5: User Story 3 — `west.yml` (Priority: P2)

**Goal**: Zephyr applications emit one component per west-managed module with the manifest-pinned revision.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/golden_inputs/west/`; assert each project's PURL resolves correctly per its `remote:` / `defaults.remote:`. Plus: Zephyr v4.4.0 main-repo integration test yields ≥79 C/C++ components.

### Fixture + tests

- [ ] T046 [P] [US3] Create fixture tree `mikebom-cli/tests/fixtures/golden_inputs/west/` with 4 sub-fixtures per US3 scenarios: `basic/`, `multi_remote/`, `groups/`, `imports/` (the `imports/` fixture verifies the deferred-import warn-and-continue behavior).
- [ ] T047 [P] [US3] Add contract tests in a new file `mikebom-cli/src/scan_fs/package_db/west.rs` test module: `source_mechanism_annotation_zephyr_west`, `west_multi_remote_routing`, `west_groups_exclude_flag`, `west_imports_deferred`.

### Implementation

- [ ] T048 [P] [US3] Create new module `mikebom-cli/src/scan_fs/package_db/west.rs`. Define `WestManifest`, `WestProject`, `WestRemote`, `WestDefaults`, `WestImport` structs with `#[derive(Deserialize)]` per `data-model.md`.
- [ ] T049 [US3] Implement `parse_manifest(path: &Path) -> Result<WestManifest>` using `serde_yaml::from_str` (already a direct dep per research R2). Validate: every project MUST have `name` + `revision`; warn-and-skip otherwise (FR-013).
- [ ] T050 [US3] Implement `resolve_remote_url` per `contracts/west-yml.md`: `project.remote` (or `defaults.remote`) → `remotes[].url_base` → composed URL → `sanitize_userinfo` (FR-016).
- [ ] T051 [US3] Implement PURL derivation: `pkg:github/<org>/<repo>@<rev>` for GitHub-hosted; `pkg:git+https://<sanitized>@<rev>` otherwise.
- [ ] T052 [US3] Emit annotations per `contracts/west-yml.md`: `mikebom:source-mechanism: "zephyr-west"`, `mikebom:source-files`, `mikebom:download-url`, `mikebom:groups` (when non-empty).
- [ ] T053 [US3] Add the new `--exclude-group <name>` CLI flag to the `scan` subcommand in `mikebom-cli/src/cli/scan_cmd.rs` (or equivalent). Repeatable via `clap`'s `Args`-derive. Plumb the value through to the west reader as a filter set.
- [ ] T054 [US3] Wire `west` into the reader dispatcher at `mikebom-cli/src/scan_fs/package_db/mod.rs`. File-pattern trigger: `west.yml` at scan root or under `.west/`.
- [ ] T055 [US3] Log a `tracing::info!` event for each unfollowed `import:` directive so operators see what's being skipped (per US3 edge case).

### Catalog + goldens + integration

- [ ] T056 [US3] Update `docs/reference/sbom-format-mapping.md`'s C55 row to add `zephyr-west` with the Principle V audit one-liner.
- [ ] T057 [US3] Generate goldens for the west fixture.
- [ ] T058 [US3] Add integration test `mikebom-cli/tests/transitive_parity_cpp_phase2.rs::zephyr_v4_4_0_west_modules`: clones `zephyr v4.4.0` shallow (using milestone-090's split-test-fixtures cache infrastructure if practical, else a fresh shallow clone per CI run), scans, asserts ≥79 C/C++ components with `mikebom:source-mechanism: "zephyr-west"`. Skip-gates the test if the network is unavailable.
- [ ] T059 [US3] Run `./scripts/pre-pr.sh` clean. Open PR titled `feat(c/c++): Zephyr west.yml reader (US3 of milestone 105)`.

**Checkpoint**: US3 is shippable. Zephyr applications scan cleanly.

---

## Phase 6: User Story 4 — `idf_component.yml` (Priority: P2)

**Goal**: esp-idf projects emit one component per Espressif Component Registry dependency with `pkg:idf/` PURL + source-URL fallback annotation.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/golden_inputs/idf_component/`; assert `pkg:idf/espressif/mdns@1.4.2` etc. Plus: scan a representative esp-idf sample → ≥20 components.

### Fixture + tests

- [ ] T060 [P] [US4] Create fixture tree `mikebom-cli/tests/fixtures/golden_inputs/idf_component/` with 4 sub-fixtures per US4 scenarios: `exact/`, `locked/` (with a `dependencies.lock` file), `multi/`, `local/`.
- [ ] T061 [P] [US4] Add contract tests in `mikebom-cli/src/scan_fs/package_db/idf_component.rs`: `source_mechanism_annotation_idf_registry`, `idf_lockfile_resolution`, `idf_multi_manifest_union`, `idf_local_path`.

### Implementation

- [ ] T062 [P] [US4] Create new module `mikebom-cli/src/scan_fs/package_db/idf_component.rs`. Define `IdfComponentManifest`, `IdfDependency` enum with serde derives per `data-model.md`.
- [ ] T063 [US4] Implement `parse_manifest` using `serde_yaml::from_str`. Handle three dependency forms: registry exact (`namespace/name: "version"`), registry range (`namespace/name: "^1.2.0"`), local path (`name: {path: ...}`), git source (`name: {git: ..., version: ...}`).
- [ ] T064 [US4] Implement PURL derivation per `contracts/idf-component-yml.md` + clarification Q2: `pkg:idf/<ns>/<name>@<version>` for registry deps; `pkg:generic/<name>` for local; `pkg:git+https://<sanitized>@<rev>` for git.
- [ ] T065 [US4] Implement lockfile lookup for range-spec resolution: if a `dependencies.lock` exists in the same directory, parse it and substitute the exact version. Absent lockfile → preserve range string with `mikebom:requirement-range` annotation.
- [ ] T066 [US4] Implement the source-URL fallback annotation per clarification Q2: every registry-form component MUST carry `mikebom:download-url` (best-effort source resolution — `repository:` field in the manifest, `url:` field, or placeholder `https://components.espressif.com/<ns>/<name>`).
- [ ] T067 [US4] Multi-manifest union: when the same canonical PURL appears in multiple `idf_component.yml` files under the scan root, the existing dedup pipeline (Phase 2) handles deduplication. Verify behavior in fixture `multi/`.
- [ ] T068 [US4] Wire `idf_component` into the reader dispatcher. File-pattern trigger: any `idf_component.yml` anywhere under the scan root.

### Catalog + goldens + integration

- [ ] T069 [US4] Update `docs/reference/sbom-format-mapping.md`'s C55 row to add `idf-component` and `idf-component-local` with the Principle V audit one-liner.
- [ ] T070 [US4] Generate goldens for the idf_component fixture.
- [ ] T071 [US4] Add integration test `mikebom-cli/tests/transitive_parity_cpp_phase2.rs::esp_idf_components`: scan a checked-in mini esp-idf sample (NOT a fresh clone of upstream — the registry components are pinned to specific versions for determinism), assert ≥20 components emerge.
- [ ] T072 [US4] Run `./scripts/pre-pr.sh` clean. Open PR titled `feat(c/c++): Espressif idf_component.yml reader (US4 of milestone 105)`.

**Checkpoint**: US4 is shippable. esp-idf projects scan cleanly.

---

## Phase 7: User Story 5 — vcpkg classic mode (Priority: P3)

**Goal**: vcpkg classic-mode installations emit one component per installed port.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/golden_inputs/vcpkg_classic/`; assert `pkg:vcpkg/zlib@1.3.1` etc.

### Fixture + tests

- [ ] T073 [P] [US5] Create fixture tree `mikebom-cli/tests/fixtures/golden_inputs/vcpkg_classic/` with 2 sub-fixtures: `single_triplet/` and `multi_triplet/`. (US5 scenario 2's classic+manifest collision case lives in `dedup_collision/vcpkg_both/` — created in Phase 2 T024.)
- [ ] T074 [P] [US5] Add contract tests in `mikebom-cli/src/scan_fs/package_db/vcpkg.rs`: `source_mechanism_annotation_vcpkg_classic`, `vcpkg_classic_multi_triplet`.

### Implementation

- [ ] T075 [P] [US5] Extend `mikebom-cli/src/scan_fs/package_db/vcpkg.rs` with `parse_classic_install` function. Regex over filenames matching `^([^_]+)_(.+?)_([^_]+)\.list$` (port name has no `_`; version may have `.`; triplet is well-defined).
- [ ] T076 [US5] PURL derivation: `pkg:vcpkg/<name>@<version>` (same shape as vcpkg-manifest for dedup compatibility).
- [ ] T077 [US5] Emit annotations: `mikebom:source-mechanism: "vcpkg-classic"`, `mikebom:source-files`, `mikebom:target-arch` (the triplet).
- [ ] T078 [US5] Triplet deduplication: when the same `<name>@<version>` appears under multiple triplets, the dedup pipeline collapses by canonical PURL; the resulting component's `mikebom:target-arch` becomes a comma-joined list of triplets.
- [ ] T079 [US5] Wire `vcpkg-classic` into the reader dispatcher. File-pattern trigger: `**/vcpkg/installed/<triplet>/vcpkg/info/*.list`.

### Catalog + goldens

- [ ] T080 [US5] Update `docs/reference/sbom-format-mapping.md`'s C55 row to add `vcpkg-classic`.
- [ ] T081 [US5] Generate goldens for the vcpkg_classic fixture.
- [ ] T082 [US5] Run `./scripts/pre-pr.sh` clean. Open PR titled `feat(c/c++): vcpkg classic mode reader (US5 of milestone 105)`.

**Checkpoint**: US5 is shippable. vcpkg classic-mode projects scan cleanly.

---

## Phase 8: User Story 6 — git-submodule reader (Priority: P3)

**Goal**: `.gitmodules` entries emerge as components pinned to checked-out HEAD revisions, with `mikebom:build-reference` annotation reflecting `find_package` correlation.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/golden_inputs/git_submodule/`; assert components are present with correct SHAs + `build-reference` values. Plus: gRPC v1.69.0 integration test yields ≥16 submodule components.

### Fixture + tests

- [ ] T083 [P] [US6] Create fixture tree `mikebom-cli/tests/fixtures/golden_inputs/git_submodule/` with 6 sub-fixtures per US6 scenarios + edge cases: `populated/`, `uninitialized/`, `name_mismatch/`, `target_alias/`, `with_creds/`, `multiple_submodules/`. Each contains a synthetic `.gitmodules`, populated (or empty) submodule directory, and a synthetic `.git/modules/<name>/HEAD` file containing the desired SHA or branch ref.
- [ ] T084 [P] [US6] Add contract tests in `mikebom-cli/src/scan_fs/package_db/git_submodule.rs`: `source_mechanism_annotation_git_submodule_populated`, `_uninitialized`, `_name_mismatch_declared_only`, `_alias_declared_and_used`, `_credentials_redacted`.

### Implementation

- [ ] T085 [P] [US6] Create new module `mikebom-cli/src/scan_fs/package_db/git_submodule.rs`. Implement `.gitmodules` INI parser (state-machine, no `git2` crate). Output: `Vec<SubmoduleEntry>`.
- [ ] T086 [US6] Implement HEAD revision resolution per `contracts/git-submodule.md` (no `git` subprocess): `.git/modules/<name>/HEAD` → either a 40-char SHA directly, or a `ref: refs/heads/<branch>` requiring `.git/modules/<name>/refs/heads/<branch>` lookup, or fallback to `.git/modules/<name>/packed-refs`. If unresolved → `None` (FR-009).
- [ ] T087 [US6] Pass every URL through `sanitize_userinfo` (FR-016 — uses the Phase 2 helper) before PURL or annotation construction.
- [ ] T088 [US6] PURL derivation: `pkg:github/<org>/<repo>@<rev>` for `github.com` URLs; `pkg:gitlab/<org>/<repo>@<rev>` for `gitlab.com` if the `pkg:gitlab/` ecosystem is in the PURL spec (research at implementation time; otherwise fall back to `pkg:git+https://`); `pkg:git+https://<sanitized>@<rev>` for all others. SSH URLs (`git@host:path`) normalized to `pkg:git+ssh://` form.
- [ ] T089 [US6] Implement `mikebom:build-reference` annotation per FR-008a. Read the `ScanContext::find_package_targets` set (populated by Phase 2 T008 + T009). Compute `last_path_segment = submodule.path.file_name().to_lowercase()`. If `find_package_targets` contains it → `"declared-and-used"`; else → `"declared-only"`. Output is order-independent.
- [ ] T090 [US6] Uninitialized submodule path: emit component with `version: "unknown"` + `mikebom:resolver-step: "uninitialized-submodule"` annotation (FR-009). Scan MUST NOT fail.
- [ ] T091 [US6] Wire `git_submodule` into the reader dispatcher. File-pattern trigger: `.gitmodules` at the scan root only (NOT nested per edge case).

### Catalog + goldens + integration

- [ ] T092 [US6] Update `docs/reference/sbom-format-mapping.md`'s C55 row to add `git-submodule`. Document C57 (`mikebom:build-reference`) as a new row with the closed enum `declared-and-used` / `declared-only` and the Principle V audit (per research R3, this is the new annotation that supersedes the originally-intended `mikebom:linkage-kind` reuse).
- [ ] T093 [US6] Generate goldens for the git_submodule fixture.
- [ ] T094 [US6] Add integration test `mikebom-cli/tests/transitive_parity_cpp_phase2.rs::grpc_v1_69_0_submodule_count`: clone gRPC v1.69.0 shallow with `--recurse-submodules --depth 1`, scan, assert ≥16 components with `mikebom:source-mechanism: "git-submodule"`.
- [ ] T095 [US6] Run `./scripts/pre-pr.sh` clean. Open PR titled `feat(c/c++): git-submodule reader + find_package correlation (US6 of milestone 105)`.

**Checkpoint**: US6 is shippable. Large submodule-based C++ projects scan cleanly.

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Final cleanup, documentation, performance check, and the `pkg:idf/` PURL-spec registration follow-up.

- [ ] T096 [P] Finalize `docs/reference/sbom-format-mapping.md` C55 row: confirm all 13 closed-enum values listed (7 from alpha.41 + 6 new from milestone 105). Confirm C56 row (`mikebom:also-detected-via`) and C57 row (`mikebom:build-reference`) are present with Principle V audit clauses. Cross-check against the spec's FR-010 enumeration.
- [ ] T097 [P] File a package-url spec ecosystem-registration request for `pkg:idf/` against `https://github.com/package-url/purl-spec/issues` (link the milestone 105 spec in the issue body). Track the response; if the ecosystem name is rejected, plan a fast-follow milestone to switch idf-component PURLs to `pkg:generic/` with namespace qualifiers.
- [ ] T098 Update `CLAUDE.md` "Recent Changes" section with milestone 105: list the 6 new readers + 2 new annotations + 1 dedup pipeline + Yocto deferred to follow-on.
- [ ] T099 SC-009 performance check: re-run the existing golden-fixture scan suite, compare wall-clock to the baseline captured in T003. If delta exceeds 5%, profile and optimize the slow reader; do NOT ship until under threshold.
- [ ] T100 SC-006 parity round-trip check: run the full parity test suite (`cargo +stable test --workspace -- parity::`). Every byte-identity comparison MUST pass for all 13 source-mechanism values + the C56/C57 rows.
- [ ] T100a FR-012 offline-mode audit: a build-time test in `mikebom-cli/tests/offline_mode_audit_phase2.rs` (or a `build.rs` lint hook) that greps the new reader modules — `mikebom-cli/src/scan_fs/package_db/{cmake,conan,vcpkg,west,idf_component,git_submodule}.rs` plus `mikebom-cli/src/scan_fs/dedup.rs` — for `reqwest::`, `tokio::net::`, `hyper::`, `Command::new("curl"`, `Command::new("wget"`, `Command::new("git"` (the helper is for HEAD-revision lookup, NOT a subprocess shell-out per research Assumptions). Any match fails the build. Asserts FR-012's offline-only guarantee independently of the implementations' own claims.
- [ ] T101 [P] Run the quickstart.md scenarios end-to-end against fresh checkouts of a CPM project, Zephyr v4.4.0, gRPC v1.69.0, and an esp-idf sample. Confirm each scan produces the expected component counts per the quickstart's claims.
- [ ] T102 Final pre-PR gate clean on the integration branch combining all six US PRs (assuming the project's release workflow merges them into a single alpha milestone release).
- [ ] T103 Cut the next alpha release (likely `v0.1.0-alpha.42` — confirm the actual version at cut time per the standing release process; intervening hotfixes may have consumed alpha.42 already). Bump the workspace version, regenerate the alpha-release goldens, and open the release PR. Same shape as the existing alpha.41 release flow.

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 (Setup)**: No external blockers; needs PR #272 merged.
- **Phase 2 (Foundational)**: Blocks all user story phases. Every US uses the sanitization helper (FR-016), the dedup pipeline (FR-015), or both. US6 also uses the find_package collector.
- **Phases 3–8 (User Stories)**: All depend on Phase 2 completion. Once Phase 2 lands, US1–US6 can ship in any order. Recommended order: US1 → US2 → US3 → US4 → US5 → US6 (priority order).
- **Phase 9 (Polish)**: Depends on all 6 user stories being complete and merged.

### User story dependencies (within Phase 2's foundations)

- **US1 (CPM.cmake)**: uses sanitize_userinfo. Independent of other US.
- **US2 (conanfile.py)**: uses dedup pipeline (for the .txt + .py dedup case). Independent of other US.
- **US3 (west.yml)**: uses sanitize_userinfo + dedup pipeline. Independent.
- **US4 (idf_component.yml)**: uses sanitize_userinfo + dedup pipeline + the C55 enum extension. Independent.
- **US5 (vcpkg classic)**: uses dedup pipeline. Independent.
- **US6 (git-submodule)**: uses sanitize_userinfo + dedup pipeline + the find_package collector (Phase 2 T008-T010). The find_package collector is the only US6-specific Phase 2 prerequisite.

### Parallel opportunities

- **Within Phase 2**: 2A (sanitization refactor), 2B (find_package collector), 2C (dedup types), 2D (parity wiring), and 2F (determinism test fixture) are largely independent of each other and can be parallelized across multiple developers / Agent invocations. 2E (CDX hybrid emission) needs 2C. 2G (polyglot robustness) is fully independent — could be done first as a quick win.
- **Within each US phase**: fixture creation [P] + initial implementation [P] + catalog doc updates [P] + contract test stubs [P] can all run in parallel. Goldens generation [non-P] depends on implementation being complete.
- **Across user stories**: Phases 3–8 are independent of each other — different developers can take different US in parallel after Phase 2 lands.

### Within a user story

- Fixtures (T028, T037, T046, T060, T073, T083) can be created in parallel with contract test stubs.
- Implementation tasks come after fixtures (so the tests have something to scan).
- Goldens generation is the LAST step before pre-PR gate.

---

## Parallel Example: Phase 2

```bash
# Three developer tracks running concurrently:

# Track A — sanitization refactor (2A)
T004 → T006
T005 (parallel with T006)
T007 (parallel with T006)

# Track B — find_package collector (2B)
T008 → T010
T009 (parallel with T010)

# Track C — dedup pipeline + parity (2C + 2D + 2E)
T011 → T012 → T013 → T014
T015, T016, T017, T018, T019, T020 (all parallel)
T021 (after T015–T020)
T022 → T023 (sequential)

# Track D — quick wins (independent)
T026 → T027 (polyglot robustness)
T024 → T025 (dedup determinism fixture + test)
```

---

## Implementation Strategy

### MVP first (User Story 1 only)

1. Phase 1 (Setup) — verify baseline.
2. Phase 2 (Foundational) — sanitization helper + dedup pipeline + parity rows + find_package collector.
3. Phase 3 (US1 — CPM.cmake) — ship as PR.
4. **STOP and VALIDATE**: scan the cpp-best-practices/cmake_template project; confirm fmt/spdlog/CLI11/Catch2/FTXUI emerge with real PURLs+versions.
5. Cut alpha.42-rc1 with US1 only if useful for downstream validation.

### Incremental delivery (recommended)

1. Phase 1 + Phase 2 → foundation ready (single PR or 2–3 sub-PRs).
2. US1 (CPM.cmake) → demo against real project → ship.
3. US2 (conanfile.py) → ship.
4. US3 (west.yml) → demo against Zephyr v4.4.0 → ship.
5. US4 (idf_component.yml) → demo against esp-idf sample → ship.
6. US5 (vcpkg classic) → ship.
7. US6 (git-submodule) → demo against gRPC v1.69.0 → ship.
8. Phase 9 (Polish) → cut alpha.42 release.

### Parallel team strategy

If multiple developers / multiple Agent runs:

1. Phase 1 + Phase 2 — one developer (or paired) ships first.
2. Once Phase 2 merges, the 6 user stories distribute:
   - Developer A: US1 + US2 (modern cmake/conan)
   - Developer B: US3 + US4 (embedded RTOS readers)
   - Developer C: US5 + US6 (vcpkg-classic + submodule)
3. PRs land in priority order; Phase 9 consolidates.

---

## Notes

- **[P] tasks** = different files, no incomplete dependencies.
- **[Story] label** is REQUIRED for tasks in Phases 3–8; absent for Setup/Foundational/Polish phases.
- **Every per-reader phase produces a single PR** following the milestone-102 PR #272 shape (reader extension + fixtures + goldens + pre-PR clean).
- **PR #272 prerequisite**: this milestone assumes PR #272 (the alpha.41 source-mechanism annotation work) is merged to main. If it has not merged when work begins, complete its merge first.
- **Constitution principles**: every PR runs `./scripts/pre-pr.sh` clean per the Pre-PR Verification gate. Tests that use `.unwrap()` MUST be guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]`.
- **No new Cargo dependencies**: `serde_yaml` is already direct (research R2); all other crates needed are in the workspace closure.
- **Commit after each task or logical group**. Stop at any checkpoint to validate independently.
- **Yocto / OpenSTLinux**: explicitly OUT of scope per the clarification session. A follow-on milestone (likely 106) covers it.
