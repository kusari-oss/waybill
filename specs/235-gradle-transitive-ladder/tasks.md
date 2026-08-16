---

description: "Task list for m235 — Gradle Transitive Dependency Resolution Ladder"

---

# Tasks: Gradle Transitive Dependency Resolution Ladder

**Input**: Design documents from `/specs/235-gradle-transitive-ladder/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Unit tests for parsers + regex extractors + timeout handling. Integration tests against 6 fixture Gradle projects. Golden CDX/SPDX 2.3/SPDX 3 fixtures for US1 + US3 happy paths (per research R7). Parity extractor test picks up the new C-row automatically. Subprocess timeout test uses a synthetic shell-script wrapper (no real Gradle required). `WAYBILL_TEST_REAL_GRADLE=1` env var gates optional real-Gradle integration tests.

**Organization**: 6 phases. Phases 3–6 map to US1/US2/US3/US4 respectively. Phase 2 (Foundational) carries the shared enums, structs, CLI flags, and lightweight direct-dep extractor that ALL three ladder stories consume.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

## Path Conventions

Repository root is `/Users/mlieberman/Projects/mikebom/`. All paths below are repo-relative. Rust source lives under `waybill-cli/src/`; fixtures under `waybill-cli/tests/fixtures/`; docs under `docs/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Directory scaffolding + module registration + baseline compile.

- [X] T001 Create module files scaffold: touch empty `waybill-cli/src/scan_fs/package_db/gradle/subprocess.rs`, `cache_reader.rs`, `static_parser.rs`, `version_catalog.rs`, `tier.rs`, `ladder.rs` (contents added in later tasks; empty stubs first so `mod.rs` can reference them from T003).
- [X] T002 Create fixture directory scaffold: `waybill-cli/tests/fixtures/golden_inputs/gradle/{wrapper_single_subproject,wrapper_multi_subproject,no_wrapper_with_lockfile,no_wrapper_warm_cache,cold_clone_static_only,mixed_tier}/` — empty dirs; fixture files created in per-story tasks.
- [X] T003 Register new submodules in `waybill-cli/src/scan_fs/package_db/gradle/mod.rs`: add `pub(super) mod subprocess;`, `pub(super) mod cache_reader;`, `pub(super) mod static_parser;`, `pub(super) mod version_catalog;`, `pub(super) mod tier;`, `pub(super) mod ladder;` after the existing `pub(super) mod lockfile;` line. Verify `cargo +stable check -p waybill` passes with empty stubs.

**Checkpoint**: Module scaffold exists, workspace still compiles. Move to Phase 2.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared types (enums, structs, CLI flags) + lightweight direct-dep extractor helper. All ladder stories depend on these.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T004 [P] Implement `GradleResolutionTier` and `GradleFallbackReason` enums in `waybill-cli/src/scan_fs/package_db/gradle/tier.rs` per data-model.md §Enum sections. Add `as_annotation_str(&self)` methods returning kebab-case strings. Add unit tests covering serialize round-trip.
- [X] T005 [P] Implement `GradleResolvedGraph`, `SubprojectRoot`, `GradleScanSummary`, `MavenCoord` structs in `waybill-cli/src/scan_fs/package_db/gradle/ladder.rs` per data-model.md §Struct sections. Include Debug/Clone derives. Include `EdgeScope` enum (Runtime, Test, Buildscript, Optional) that maps to existing `LifecycleScope` for CDX/SPDX emission.
- [X] T006 [P] Implement `GradleCliFlags` struct in `waybill-cli/src/cli/scan_cmd.rs` alongside the existing `ScanArgs` struct. Add the 5 flags per data-model.md §GradleCliFlags — `clap`'s `requires = "gradle_resolve"` handles R8 stale-flag validation; `value_parser!(u64).range(1..)` handles the zero-timeout error. Wire the `GradleCliFlags` into `ScanArgs` via `#[command(flatten)]` on a new field (matches the m076 `EnrichArgs` pattern referenced by `sbom_cmd.rs:8`).
- [X] T007 Implement lightweight direct-dep extractor helper `pub(super) fn extract_direct_coords(project_dir: &Path) -> Vec<MavenCoord>` in `waybill-cli/src/scan_fs/package_db/gradle/static_parser.rs`. Covers ONLY the direct-string-coord regex patterns (`implementation "g:a:v"` and `implementation("g:a:v")`). Skips version-catalog references (US3 full-parse handles those). Used by US2 to seed its declared-coords list before cache lookup.
- [X] T008 [P] Add shell-metacharacter validation helper `pub(super) fn validate_configuration_name(name: &str) -> Result<(), GradleFlagsError>` in `waybill-cli/src/cli/scan_cmd.rs` (near the `GradleCliFlags` struct). Rejects names containing spaces, semicolons, backticks, `$`, `|`, `&`, `>`, `<`. Post-parse validation in the scan command calls this on each `--gradle-extra-configurations` value; error is a clap-style user-visible message.
- [X] T009 [P] Add integration hook in `waybill-cli/src/scan_fs/package_db/gradle/mod.rs::read()` to call `ladder::resolve(project_dir, &flags)` AFTER the existing m106 lockfile pass. The ladder result's components + edges get merged into the return `Vec<PackageDbEntry>`. When m106 emitted entries (lockfile present), the ladder's transitive edges ADD to the existing entries (per FR-009 non-regression); when m106 emitted nothing, the ladder's components become the sole gradle output for that project.

**Checkpoint**: Shared substrate compiles. `GradleCliFlags` visible in `waybill sbom scan --help`. US1 + US2 + US3 + US4 can proceed in parallel.

---

## Phase 3: User Story 1 — Subprocess Resolution (Priority: P1) 🎯 MVP

**Goal**: When `--gradle-resolve` is passed and a Gradle wrapper is discoverable, waybill invokes `./gradlew :sub:dependencies --no-daemon --configuration <c>` per subproject × configuration and parses the resolved graph.

**Independent Test**: Fixture `wrapper_single_subproject` scan with `--gradle-resolve` produces an SBOM whose CDX `dependencies[]` array contains the transitive edge from the direct dep to a known-transitive dep. Byte-equivalent against the golden CDX fixture. Matches SC-001.

**Depends on**: Phase 2 (`GradleCliFlags`, `GradleResolutionTier`, `GradleResolvedGraph`, ladder integration hook).

### Subprocess parser + timeout (T010–T015)

- [X] T010 [US1] Implement wrapper discovery + tool-availability check in `waybill-cli/src/scan_fs/package_db/gradle/subprocess.rs::discover_wrapper(project_dir: &Path) -> Option<PathBuf>`. Prefers `project_dir/gradlew` (POSIX) or `project_dir/gradlew.bat` (Windows via `#[cfg(windows)]`); falls back to `which::which("gradle")` — actually **DO NOT** add the `which` crate; use `std::env::split_paths(&std::env::var_os("PATH")?)` and check each dir manually. Returns `None` if neither found → the ladder maps this to `GradleFallbackReason::MissingTool`.
- [X] T011 [US1] Implement subproject enumeration in `waybill-cli/src/scan_fs/package_db/gradle/subprocess.rs::enumerate_subprojects(wrapper: &Path, project_dir: &Path, timeout_secs: u64) -> Result<Vec<String>, SubprocessOutcome>`. Runs `./gradlew projects --no-daemon --quiet`. Parses the tree-format output to extract lines matching `+--- Project ':<name>'` or `\--- Project ':<name>'`. Returns `vec![""]` (single-project) if no subprojects listed. Uses the subprocess-with-timeout helper from T012.
- [X] T012 [US1] Implement subprocess-with-timeout helper `pub(super) fn spawn_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<Output, SubprocessOutcome>` in `waybill-cli/src/scan_fs/package_db/gradle/subprocess.rs`. Mirrors the `golang/go_mod_graph.rs::run_go_mod_graph` pattern at `waybill-cli/src/scan_fs/package_db/golang/go_mod_graph.rs:81-158`: spawn via `Command::stdin(Stdio::null()).spawn()`, use `std::thread` + `std::sync::mpsc` for output-collection + wait, on timeout send SIGTERM then SIGKILL after 2s grace via `Child::kill()`. Returns `SubprocessOutcome::Timeout` on timeout.
- [X] T013 [US1] Implement ASCII-tree parser `pub(super) fn parse_dependencies_output(output: &str, config: &str) -> Result<Vec<ParsedDepEntry>, SubprocessOutcome>` in `waybill-cli/src/scan_fs/package_db/gradle/subprocess.rs` per research R3. Line-oriented state machine: skip until `<config> - <description>` header, then parse `<indent>+--- <coord>` lines. Coord regex handles the four shapes: `g:a:v`, `g:a:v -> resolved`, `g:a -> resolved`, and trailing `(*)` / `(c)` markers. `(c)` lines skipped entirely; `(*)` lines record the coord but don't descend. Reconstruct parent-child edges by depth tracking. Return `Vec<ParsedDepEntry { coord: MavenCoord, depth: usize, edge_marker: EdgeMarker }>`.
- [X] T014 [US1] Add unit tests in `waybill-cli/src/scan_fs/package_db/gradle/subprocess.rs::tests` for the ASCII-tree parser using at least 4 hand-crafted fixture strings: (a) simple `implementation` with one transitive; (b) `(*)` deduplication marker; (c) `(c)` constraint marker; (d) `->` resolution override. Each test asserts exact `ParsedDepEntry` vec output.
- [X] T015 [US1] Implement the top-level entry `pub fn resolve_via_subprocess(project_dir: &Path, flags: &GradleCliFlags) -> Result<GradleResolvedGraph, SubprocessOutcome>` per the contract at `specs/235-gradle-transitive-ladder/contracts/gradle-subprocess.md`. Sequences: discover_wrapper → enumerate_subprojects → per-subproject-per-configuration invoke `./gradlew :<sub>:dependencies --configuration <c> --no-daemon --quiet` (add `--daemon` iff `flags.gradle_daemon`; add buildscript config when `flags.gradle_resolve_buildscript`) → aggregate parsed output → construct `GradleResolvedGraph { tier: Subprocess, ... }`.

### Ladder integration + fixture (T016–T018)

- [X] T016 [US1] Implement `pub(super) fn try_subprocess(project_dir: &Path, flags: &GradleCliFlags) -> Option<GradleResolvedGraph>` in `waybill-cli/src/scan_fs/package_db/gradle/ladder.rs`. Early-return `None` if `!flags.gradle_resolve`. Otherwise call `resolve_via_subprocess`; on `Ok(graph)` return `Some(graph)`; on any `SubprocessOutcome` variant, record the fallback reason in the ladder's per-project state and return `None` (letting US2/US3 try next).
- [ ] T017 [US1] Create fixture at `waybill-cli/tests/fixtures/golden_inputs/gradle/wrapper_single_subproject/` with:
  - `settings.gradle` containing `rootProject.name = "waybill-fixture-app"` (synthetic name per memory `feedback_fixture_synthetic_package_names`)
  - `build.gradle` declaring `implementation("com.example.waybillfixture:direct:1.0.0")` and `testImplementation("com.example.waybillfixture:test-only:2.0.0")`
  - `gradle/wrapper/gradle-wrapper.properties` pointing at a distributionUrl (real Gradle version, e.g., 8.11)
  - `gradlew` shell script that mocks the real wrapper's output — writes canned ASCII-tree text to stdout for `:dependencies` calls. The mock is deterministic and doesn't require a JDK; controlled via env var `WAYBILL_FIXTURE_GRADLE_TREE_OUTPUT` (test injects the canned tree).
- [ ] T018 [US1] Create integration test `waybill-cli/tests/gradle_ladder.rs::us1_wrapper_single_subproject_transitive_edge` that scans the T017 fixture with `--gradle-resolve --gradle-timeout-secs 30` and asserts: (a) emitted CDX contains a component for `direct:1.0.0` AND its transitive `com.example.waybillfixture:transitive:0.5.0` (encoded in the mock ASCII-tree output); (b) `dependencies[]` contains an edge from `pkg:maven/com.example.waybillfixture/direct@1.0.0` to `pkg:maven/com.example.waybillfixture/transitive@0.5.0`. Uses the `apply_fake_home_env` pattern from existing integration tests.

### Golden fixture (T019)

- [ ] T019 [US1] Emit golden CDX + SPDX 2.3 + SPDX 3 for the T017 fixture at `waybill-cli/tests/fixtures/golden_inputs/gradle/wrapper_single_subproject/expected.{cdx,spdx-2.3,spdx-3}.json`. Regenerate via `WAYBILL_UPDATE_CDX_GOLDENS=1 WAYBILL_UPDATE_SPDX_GOLDENS=1 WAYBILL_UPDATE_SPDX3_GOLDENS=1 cargo test --workspace gradle_ladder`. Follows the m190/m197 golden-fixture pattern. All 3 formats MUST byte-equal on re-scan.

**Checkpoint US1 complete**: `--gradle-resolve` on a project with a wrapper produces an SBOM with transitive edges matching Gradle's own view. SC-001 verified.

---

## Phase 4: User Story 2 — Cache Reader (Priority: P2)

**Goal**: When US1 didn't fire (opt-out, missing wrapper, subprocess timeout), reconstruct the graph from `~/.gradle/caches/modules-2/` cached POMs and `.module` files.

**Independent Test**: Fixture `no_wrapper_warm_cache` scan (no `--gradle-resolve`, no `./gradlew`, but the fixture ships a mock Gradle cache tree) produces an SBOM whose transitive edges match what US1 would have produced. Matches SC-002.

**Depends on**: Phase 2 (shared types). Independent of US1 completion.

- [ ] T020 [P] [US2] Implement `pub(super) fn discover_cache() -> Result<GradleCache, GradleCacheError>` in `waybill-cli/src/scan_fs/package_db/gradle/cache_reader.rs` per contracts/gradle-cache-reader.md §step 1-2. Checks `$GRADLE_USER_HOME/caches/modules-2/` first (via `std::env::var_os("GRADLE_USER_HOME")`), then `$HOME/.gradle/caches/modules-2/`. Enumerates `metadata-2.*` subdirs and picks the highest-numbered. Test-hook: `WAYBILL_TEST_GRADLE_CACHE=<path>` env var override, respected in `#[cfg(test)]` builds only.
- [ ] T021 [P] [US2] Implement POM parser `pub(super) fn parse_cached_pom(pom_path: &Path) -> Result<Vec<MavenCoord>, GradleCacheError>` in `waybill-cli/src/scan_fs/package_db/gradle/cache_reader.rs` using `quick-xml` (workspace dep; matches `maven.rs` pattern). Extracts `<dependencies>/<dependency>` entries. Returns a Vec of resolved `MavenCoord`.
- [ ] T022 [P] [US2] Implement `.module` (Gradle Module Metadata) parser `pub(super) fn parse_cached_module(module_path: &Path) -> Result<Vec<MavenCoord>, GradleCacheError>` in `waybill-cli/src/scan_fs/package_db/gradle/cache_reader.rs` using `serde_json`. Reads `variants[]`, picks the `runtime` variant (or `apiElements`+`runtimeElements` merged if `runtime` absent). Extracts `dependencies[]` entries.
- [ ] T023 [US2] Implement transitive walker `pub(super) fn walk_transitives(cache: &GradleCache, seeds: &[MavenCoord]) -> Result<(Vec<PackageDbEntry>, Vec<Edge>), GradleCacheError>` in `waybill-cli/src/scan_fs/package_db/gradle/cache_reader.rs`. BFS from seeds, prefer `.module` over `.pom` for each coord, cycle detection via `HashSet<MavenCoord>`, edge assembly. On >30% cache-miss threshold, return `GradleCacheError::InsufficientCoverage`.
- [ ] T024 [US2] Implement cache-freshness comparison `pub(super) fn cache_freshness(project_dir: &Path, cache_entries: &[PathBuf]) -> CacheFreshness` in `waybill-cli/src/scan_fs/package_db/gradle/cache_reader.rs`. Compares newest cache-entry mtime vs `build.gradle(.kts)` mtime. Returns `Fresh` if cache is newer, `Stale` otherwise. `CacheFreshness` is a small enum in this file.
- [ ] T025 [US2] Implement the top-level entry `pub fn resolve_via_cache(project_dir: &Path, declared_coords: &[MavenCoord]) -> Result<GradleResolvedGraph, GradleCacheError>` per contracts/gradle-cache-reader.md. Sequences: discover_cache → walk_transitives(seeds=declared_coords) → cache_freshness → construct `GradleResolvedGraph { tier: Cache, ... }` with freshness stored as an internal field to be surfaced via the annotation writer later.
- [ ] T026 [US2] Implement `pub(super) fn try_cache(project_dir: &Path, flags: &GradleCliFlags) -> Option<GradleResolvedGraph>` in `waybill-cli/src/scan_fs/package_db/gradle/ladder.rs`. Calls `static_parser::extract_direct_coords(project_dir)` to seed. Calls `resolve_via_cache`. On `Ok(graph)` returns `Some(graph)`; on any `GradleCacheError`, records the fallback reason and returns `None`.
- [ ] T027 [US2] Create fixture at `waybill-cli/tests/fixtures/golden_inputs/gradle/no_wrapper_warm_cache/` with: `settings.gradle` + `build.gradle` (declares `implementation "com.example.waybillfixture:root:1.0.0"`) + a synthetic Gradle cache tree at `<fixture>/fake-gradle-user-home/caches/modules-2/metadata-2.107/descriptors/com.example.waybillfixture/root/1.0.0/root-1.0.0.pom` declaring one transitive dep, plus its cached POM at `.../descriptors/com.example.waybillfixture/leaf/2.0.0/leaf-2.0.0.pom`. All coords use synthetic names.
- [ ] T028 [US2] Integration test `waybill-cli/tests/gradle_ladder.rs::us2_no_wrapper_warm_cache_transitive_edge` scans T027 fixture with `WAYBILL_TEST_GRADLE_CACHE=<fixture>/fake-gradle-user-home/caches/modules-2/` in env. Asserts: (a) tier annotation is `cache`; (b) transitive edge from root to leaf exists in emitted CDX; (c) `waybill:cache-freshness = "fresh"` since the cache mtime > `build.gradle` mtime (test sets both explicitly).

**Checkpoint US2 complete**: Cache-based resolution works without a JDK. SC-002 verified.

---

## Phase 5: User Story 3 — Static Baseline (Priority: P3)

**Goal**: When neither wrapper nor cache is available, parse `build.gradle(.kts)` + `settings.gradle(.kts)` + `libs.versions.toml` to emit at least direct-dep components.

**Independent Test**: Fixture `cold_clone_static_only` scan (no wrapper, no cache, no lockfile) emits at least one component per `implementation(...)` line in the fixture's `build.gradle.kts`. Matches SC-003.

**Depends on**: Phase 2 (shared types) + T007 direct-dep extractor (foundation-level; already done).

- [ ] T029 [P] [US3] Implement subproject enumeration `pub(super) fn enumerate_subprojects_static(project_dir: &Path) -> Vec<PathBuf>` in `waybill-cli/src/scan_fs/package_db/gradle/static_parser.rs`. Reads `project_dir/settings.gradle` and `.kts`. Recognized patterns per contracts/gradle-static-parser.md §step 1: Groovy `include 'a', 'b'` and `include ":a"`; Kotlin `include("a", "b")` and `include(":a")`. Dynamic expressions log a warn and skip. Returns absolute subproject paths.
- [ ] T030 [P] [US3] Implement the full DSL regex table from contracts/gradle-static-parser.md §step 3 in `waybill-cli/src/scan_fs/package_db/gradle/static_parser.rs`. 7 pattern types × 2 DSLs × 10 configurations. Use `std::sync::OnceLock` for regex compile-once (matches gem.rs/alpm.rs pattern). Configuration → EdgeScope mapping per step 8. Unit tests cover each pattern shape against hand-crafted fixture strings.
- [ ] T031 [P] [US3] Implement `pub(super) fn resolve_libs_versions_toml(project_dir: &Path, key: &str) -> Option<MavenCoord>` in `waybill-cli/src/scan_fs/package_db/gradle/version_catalog.rs`. Delegates to the existing m122 reader at `waybill-cli/src/scan_fs/package_db/kotlin_dsl/version_catalog.rs` (verify path via grep; if the m122 helper isn't `pub`, promote it and add a small justification comment on the visibility change). Reads `project_dir/gradle/libs.versions.toml` or `project_dir/../gradle/libs.versions.toml`.
- [ ] T032 [US3] Implement platform BOM handling in `waybill-cli/src/scan_fs/package_db/gradle/static_parser.rs`. When the Groovy or Kotlin `platform(...)` regex matches, do NOT emit a component; instead attach `waybill:gradle-platform-import = <BOM coord>` as an annotation on the enclosing subproject's main-module `PackageDbEntry` (via the existing extra_annotations channel).
- [ ] T033 [US3] Implement the top-level entry `pub fn resolve_via_static_parse(project_dir: &Path) -> Result<GradleResolvedGraph, GradleStaticError>` per contracts/gradle-static-parser.md. Sequences: enumerate_subprojects_static → per-subproject find `build.gradle(.kts)` → run all 7×10×2 regex patterns → resolve version catalog refs → map configuration to EdgeScope → construct components with zero edges (US3 emits no transitives).
- [ ] T034 [US3] Implement `pub(super) fn try_static(project_dir: &Path) -> Option<GradleResolvedGraph>` in `waybill-cli/src/scan_fs/package_db/gradle/ladder.rs`. Calls `resolve_via_static_parse`. Any `GradleStaticError::NoSourceFiles` maps to `GradleFallbackReason::NoSourceFiles` and returns `None`; other error variants also `None` with appropriate reasons.
- [ ] T035 [US3] Create fixture at `waybill-cli/tests/fixtures/golden_inputs/gradle/cold_clone_static_only/` with: `settings.gradle.kts` (`include("app", "core")`), `app/build.gradle.kts` (uses `implementation("com.example.waybillfixture:app-dep:1.0.0")` + `testImplementation("com.example.waybillfixture:test-dep:2.0.0")` + version-catalog ref), `core/build.gradle.kts` (uses `api("com.example.waybillfixture:core-dep:3.0.0")`), `gradle/libs.versions.toml` (defines the `libs.spring.boot` entry the app references). All coords use synthetic names.
- [ ] T036 [US3] Integration test `waybill-cli/tests/gradle_ladder.rs::us3_cold_clone_static_only_direct_deps` scans T035 fixture without `--gradle-resolve`. Asserts: (a) tier annotation is `static`; (b) app-dep, test-dep, core-dep, and the version-catalog-resolved coord all appear as components; (c) test-dep has `scope: test` (CDX) / `TEST_DEPENDENCY_OF` (SPDX); (d) NO `dependencies[]` edges (US3 emits direct deps only).
- [ ] T037 [US3] Emit golden CDX + SPDX 2.3 + SPDX 3 for the T035 fixture at `waybill-cli/tests/fixtures/golden_inputs/gradle/cold_clone_static_only/expected.{cdx,spdx-2.3,spdx-3}.json`. Regenerate via the `WAYBILL_UPDATE_*_GOLDENS=1` env vars.

**Checkpoint US3 complete**: Cold-clone Gradle projects emit at least direct deps. SC-003 verified.

---

## Phase 6: User Story 4 — Transparency Annotations (Priority: P2)

**Goal**: Every scan touching a Gradle project carries the tier annotation; per-subproject annotations when mixed; fallback-reason annotation when a tier degraded.

**Independent Test**: Fixture scans across all 5 tier values produce SBOMs whose document-scope annotations name the correct tier; the `mixed_tier` fixture produces per-subproject annotations. Matches SC-004.

**Depends on**: Phase 2 (shared types) + US1/US2/US3 for producing the tier values.

- [ ] T038 [P] [US4] Implement `pub fn emit(summary: &GradleScanSummary) -> DocumentScopeAnnotations` in `waybill-cli/src/generate/gradle_annotations.rs` per contracts/gradle-annotations.md §Aggregation logic. Emits document-scope `waybill:gradle-resolution-tier` on every Gradle-touching scan; when `summary.aggregate_mixed`, additionally attaches `waybill:gradle-subproject-tier` to each Gradle main-module component; on non-empty fallback_history, emits `waybill:gradle-fallback-reason`; on cache tier, emits `waybill:cache-freshness` per component.
- [ ] T039 [US4] Wire `gradle_annotations::emit` into the three format emitters:
  - CDX: `metadata.properties[]` for doc-scope; component `properties[]` for per-component.
  - SPDX 2.3: `annotations[]` at document level for doc-scope; SpdxPackage-level `annotations[]` for per-component. Use the milestone-071 mikebom-annotation-comment-v1 envelope.
  - SPDX 3: `Annotation` elements with `subject` pointing at the SBOM or component IRI.
  Wire location: near the existing tier-annotation emission for m160 Go (`waybill:go-resolution-step`) — grep for that string to find the insertion point.
- [ ] T040 [US4] Add parity catalog row in `docs/reference/sbom-format-mapping.md` for `waybill:gradle-resolution-tier` with `SymmetricEqual` directionality across CDX / SPDX 2.3 / SPDX 3. Include an example entry showing the value shape. Row ID: next-available C-number (verify via grep for the highest existing C-row).
- [ ] T041 [US4] Implement parity extractor at `waybill-cli/src/parity/extractors/gradle_resolution_tier.rs` per contracts/gradle-annotations.md §Parity extractor contract. Register in `waybill-cli/src/parity/extractors/mod.rs::EXTRACTORS` array. Per memory `feedback_sbom_format_mapping_extractor_gate`, this MUST land in the same PR as the T040 catalog row addition else `every_catalog_row_has_an_extractor` + `holistic_parity` tests fail.
- [ ] T042 [US4] Create fixtures for the remaining 4 tier scenarios (structure-check only, no goldens per research R7):
  - `wrapper_multi_subproject/` — 3 subprojects, all via subprocess. Tests subproject enumeration.
  - `no_wrapper_with_lockfile/` — no wrapper, has `gradle.lockfile`. Verifies m106 FR-009 non-regression.
  - `mixed_tier/` — 3 subprojects: one with wrapper (subprocess), one no-wrapper-with-cache (cache), one cold (static). Verifies `mixed` annotation.
- [ ] T043 [US4] Integration tests in `waybill-cli/tests/gradle_ladder.rs` for the 3 additional scenarios (T042 fixtures) — structure-check assertions only:
  - `us4_wrapper_multi_subproject_tier_homogeneous` — all subprojects same tier; document-scope has `subprocess`; NO per-subproject annotations (per aggregation logic step 2).
  - `us4_no_wrapper_with_lockfile_m106_non_regression` — m106 still emits its flat list; ladder adds nothing; tier annotation is `lockfile-only`.
  - `us4_mixed_tier` — document-scope is `mixed`; each subproject's main-module carries the specific per-subproject tier annotation.
- [ ] T044 [US4] Integration test `waybill-cli/tests/gradle_ladder.rs::us4_timeout_fallback_records_reason` uses a synthetic fixture with a `./gradlew` shell script that sleeps 15 seconds. Scan with `--gradle-resolve --gradle-timeout-secs 3`. Asserts: (a) US1 was killed at 3s; (b) ladder degraded to US3 (no cache, has build.gradle); (c) tier annotation is `static`; (d) `waybill:gradle-fallback-reason` records `timeout` for the subproject.
- [ ] T044a [US4] Implement FR-014 per-scan INFO log summary in `waybill-cli/src/scan_fs/package_db/gradle/ladder.rs`. After processing all Gradle projects, emit a single `tracing::info!(...)` line naming each subproject and its winning tier — format `gradle-resolver: :app=subprocess, :lib=cache, :tests=static`. Wire the emission point at the end of the `read()` dispatcher in `gradle/mod.rs` so it fires exactly once per scan (even if multiple project dirs contributed). Integration test `waybill-cli/tests/gradle_ladder.rs::fr014_summary_log_emits_once_per_scan` uses `tracing_test::traced_test` (already a workspace dev-dep — verify) OR a scoped `tracing::Subscriber` in the test body to assert the summary appears exactly once with the expected format.

**Checkpoint US4 complete**: Transparency annotations verified across all tier permutations. SC-004 verified.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, pre-PR verification, spec close-out.

- [ ] T045 [P] Update `docs/ecosystems.md` (or the equivalent per-ecosystem coverage doc — verify path via grep) with a new subsection on Gradle transitive resolution. Cover: the ladder tiers, when each fires, how the operator opts in (`--gradle-resolve`), tie-in with the m106 lockfile reader.
- [ ] T046 [P] Update `CLAUDE.md` `## Active Technologies` to reference m235 (auto-added by `update-agent-context.sh claude` run during plan phase; verify the line is correctly formatted and not truncated per the m234 experience).
- [ ] T047 Run pre-PR gate locally: `./scripts/pre-pr.sh`. MUST pass clean (zero clippy warnings; every test suite green with `ok. N passed; 0 failed`). Per Constitution `Pre-PR Verification` section.
- [ ] T048 Open PR titled `feat(m235): Gradle transitive dependency resolution ladder` with description linking spec + plan + tasks + the SC-verification checklist. Include a "Test plan" section enumerating the fixtures + goldens + tier-annotation verification. Include a "Deferred" section naming T3 (network POM fetch) as out of scope.
- [X] T049 Add spec close-out note to `specs/235-gradle-transitive-ladder/spec.md` under a new `## Close-out (post-implementation)` section (per FR-010 tradition): (a) final CLI flag surface as landed; (b) any deviations from the plan; (c) link to the merged PR; (d) SC verification pass/fail per SC.
- [X] T050 Add `memory/reference_gradle_ladder.md` auto-memory entry: SoT locations (composite paths + CLI flags), which tier fires when, tie-in with m106.

---

## Dependencies

```
Phase 1 (Setup: T001–T003)
        │
        ▼
Phase 2 (Foundational: T004–T009)  ← BLOCKS all user stories
        │
        ├─── Phase 3 US1 (T010–T019) — MVP: subprocess resolution
        │
        ├─── Phase 4 US2 (T020–T028) — independent of US1
        │
        ├─── Phase 5 US3 (T029–T037) — independent of US1/US2
        │        │
        │        └─── (T007 in Phase 2 supplied by US3 helper)
        │
        └─── Phase 6 US4 (T038–T044) — depends on US1/US2/US3 for tier producers

Phase 7 (Polish: T045–T050) — after all user stories land
```

## Parallel execution examples

**Within Phase 2 (foundational)**: T004 (tier.rs), T005 (ladder.rs shared types), T006 (args.rs flags), T008 (validation helper) all touch different files with no cross-dependency. Ship 4-way parallel.

**Within Phase 3 (US1)**: T010–T014 all live in `subprocess.rs` — sequential. T017 fixture + T019 golden can be prepared in parallel with T010–T014 by a second contributor.

**Cross-story parallel**: US1 (T010–T019), US2 (T020–T028), US3 (T029–T037) are three independent branches after Phase 2. Three contributors can ship all three in parallel. US4 (T038–T044) starts after any single US lands (with degraded partial coverage until all three exist).

**Within Phase 6 (US4)**: T038 (annotation emitter) + T040 (docs row) + T041 (parity extractor) can start in parallel; T039 (three-format wire-up) depends on T038; T042 (fixtures) + T043/T044 (tests) can start in parallel with T038-T041 once the enums from Phase 2 exist.

## Implementation strategy — MVP scope

**MVP = US1 only** (Phases 1 + 2 + 3). Ships:
- Subprocess resolution behind `--gradle-resolve`
- All 5 CLI flags with validation
- ASCII-tree parser + timeout handling
- Ladder integration with fallback to `LockfileOnly` (m106) when US1 fails or is opt-out
- One golden fixture proving end-to-end shape

MVP does NOT ship: cache reader, static parser, mixed-tier aggregation. Those add coverage for scans without a JDK; without them, scans without `--gradle-resolve` fall back to m106 (existing behavior) — no regression.

**Incremental delivery**:

1. **Cut 1** — Phases 1 + 2 + US1. MVP. Merge-safe.
2. **Cut 2** — US2 cache reader. Adds no-JDK coverage.
3. **Cut 3** — US3 static baseline. Adds cold-clone coverage.
4. **Cut 4** — US4 transparency annotations + parity extractor + docs row.
5. **Cut 5** — Polish + spec close-out.

Alternative bundling: **US1 + US4 (annotations) in the same PR** — the annotations are cheap to add once and enable consumers to distinguish tiers even during the interim when only US1 is live. Recommended if the reviewer bandwidth supports the larger diff.

## Task summary

- **Total tasks**: 51 (T044a added post-analysis for FR-014 coverage)
- **Per phase**: Setup 3, Foundational 6, US1 10, US2 9, US3 9, US4 8, Polish 6
- **Per user story**: US1 = 10 tasks (MVP; largest), US2 = 9 tasks, US3 = 9 tasks, US4 = 8 tasks
- **Parallel-safe tasks marked [P]**: 12 across all phases
- **Golden fixture tasks**: 2 (T019 US1 golden, T037 US3 golden)
- **New CLI flags**: 5 (`--gradle-resolve`, `--gradle-resolve-buildscript`, `--gradle-daemon`, `--gradle-timeout-secs`, `--gradle-extra-configurations`)

## Format validation

All 50 tasks follow the required checklist format:
- Every task starts with `- [ ]`
- Every task has a sequential ID (T001–T050)
- Every task in Phases 3–6 carries a story label ([US1], [US2], [US3], [US4])
- Every task references a concrete file path OR a concrete action against a named artifact
- Tasks marked [P] confirm they touch different files with no ordering dependency
