# Tasks: Kotlin + Swift Ecosystem Readers

**Input**: Design documents from `/specs/122-kotlin-swift-readers/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/{swift-lockfile-format,kotlin-dsl-extraction,kmp-source-set-annotation}.md ✓, quickstart.md ✓

**Tests**: Per spec FRs + quickstart's negative-test runbook, integration tests are the principal validation mechanism. ~17 test functions across three US phases + a polish-phase negative-test suite cover acceptance scenarios + edge cases + cross-platform fixtures + fail-closed behavior.

**Single-PR scope**: ~900 LoC production + ~500 LoC tests + ~250 LoC docs per plan.md estimate. Natural cut-points (if review pushes back on diff size): US3 (T025-T027) is deferrable — the polyglot fixture exercises composition that US1 + US2 ship independently. The PR title in T032 already mentions the MVP cut-point.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

Single-project Rust CLI layout (`mikebom-cli/`). Affected paths:

- `mikebom-cli/src/scan_fs/package_db/swift/{mod,lockfile,manifest}.rs` (NEW MODULE — ~250 LoC)
- `mikebom-cli/src/scan_fs/package_db/kotlin_dsl/{mod,build_script,settings,version_catalog}.rs` (NEW MODULE — ~350 LoC)
- `mikebom-cli/src/scan_fs/package_db/mod.rs` (register both new readers in `read_all`)
- `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs` (one new C68 extractor each + table registration)
- `mikebom-cli/tests/scan_swift.rs` (NEW — US1 integration tests)
- `mikebom-cli/tests/scan_kotlin_dsl.rs` (NEW — US2 integration tests)
- `mikebom-cli/tests/scan_kmp_polyglot.rs` (NEW — US3 integration tests)
- `mikebom-cli/tests/fixtures/golden_inputs/{swift_package_resolved,kotlin_dsl_gradle,kmp_polyglot}/` (NEW fixtures)
- `docs/reference/sbom-format-mapping.md` (one new C68 row with Principle V audit)
- `docs/ecosystems.md` (new `## kotlin` + `## swift` sections + coverage-matrix entries)

---

## Phase 1: Setup

**Purpose**: Establish the two new module skeletons, the parity-catalog scaffolding for C68, and the docs row every subsequent task references. No production logic yet — just empty shells that compile + the C-row that gates the milestone-115 parity-catalog test.

- [X] T001 Create `mikebom-cli/src/scan_fs/package_db/swift/mod.rs` with a module-level doc-comment summarizing the reader (parses `Package.resolved` lockfiles; detects `Package.swift` presence only; emits `pkg:swift/<host>/<namespace>/<name>@<version>` per the purl-spec swift type), plus stub declarations `pub(super) mod lockfile;` + `pub(super) mod manifest;` + a stubbed `pub fn read(rootfs: &Path, exclude_set: &super::exclude_path::ExclusionSet) -> Vec<super::PackageDbEntry> { Vec::new() }`. Add `pub(super) mod swift;` to `mikebom-cli/src/scan_fs/package_db/mod.rs` (alongside the existing `gradle` / `nuget` / `pip` declarations). The stub MUST compile under `cargo +stable check -p mikebom`.

- [ ] T002 Create `mikebom-cli/src/scan_fs/package_db/kotlin_dsl/mod.rs` with a module-level doc-comment summarizing the reader (regex-extracts deps from `build.gradle.kts`; resolves `libs.<alias>` against `libs.versions.toml`; emits `pkg:maven/<group>/<name>@<version>` + workspace-root synthesis), plus stub declarations `pub(super) mod build_script;` + `pub(super) mod settings;` + `pub(super) mod version_catalog;` + a stubbed `pub fn read(rootfs: &Path, include_dev: bool, exclude_set: &super::exclude_path::ExclusionSet) -> Vec<super::PackageDbEntry> { let _ = include_dev; Vec::new() }`. Add `pub(super) mod kotlin_dsl;` to `mikebom-cli/src/scan_fs/package_db/mod.rs`. The stub MUST compile.

- [ ] T003 Add the C68 row to `docs/reference/sbom-format-mapping.md` per research.md § Decision 8 + contracts/kmp-source-set-annotation.md § "C68 parity-catalog row". The row follows the structure of the existing C67 row at lines 110+: CDX carrier, SPDX 2.3 carrier, SPDX 3 carrier, Principle V audit conclusion citing the native-field gap in all three formats + Constitution Principle X (Transparency) as the carve-out + the C64 / C67 storage-shape precedent. Then add three stub extractor registrations in `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs` using the existing `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` macros (one line each, mirroring the C67 pattern) AND register `C68` in the `EXTRACTORS` table in `mikebom-cli/src/parity/extractors/mod.rs` with `Directionality::SymmetricEqual, order_sensitive: false`. The milestone-115 `every_catalog_row_has_an_extractor` test at `parity/extractors/mod.rs:425` MUST pass on a fresh `cargo +stable test --lib parity::extractors` invocation.

---

## Phase 2: Foundational

**Purpose**: Implement the shared entity types + the parsers each US phase depends on. US1 needs `SwiftLockfileEntry` + the PURL projection helper; US2 needs `VersionCatalog` + `KotlinDslEntry` + `KmpSourceSetTracker`; US3 needs nothing new (composes US1 + US2 via the dispatcher).

**⚠️ CRITICAL**: No user-story work begins until T004-T010 are complete.

- [X] T004 Implement `mikebom-cli/src/scan_fs/package_db/swift/lockfile.rs::SwiftLockfileEntry` struct per data-model.md § Entity 1 (`identity`, `location`, `version: Option<String>`, `revision: String`, `branch: Option<String>` fields, all `pub(super)`). No constructors yet — those land in T011 + T012.

- [X] T005 Implement `mikebom-cli/src/scan_fs/package_db/swift/lockfile.rs::project_purl(location: &str, version: &str) -> Result<mikebom_common::types::purl::Purl, SwiftLockfileError>` per contracts/swift-lockfile-format.md § "PURL projection rules" + research.md § Decision 3. Handle the five sub-rules in order: HTTPS-with-`.git`, HTTPS-without-`.git`, SSH-form via regex `^(?:(?P<user>[^@]+)@)?(?P<host>[^:]+):(?P<path>.+?)(?:\.git)?$`, deep-namespace URL-encoding with `%2F` for middle path segments, and the version-segment selection per FR-003 / clarification Q1 (full 40-char SHA when commit-pinned). Use `mikebom_common::types::purl::Purl::new` for final canonicalization. Define `pub(super) enum SwiftLockfileError` with `Io`, `ParseJson`, `UnknownVersion`, `MissingPinsArray`, `InvalidRevision`, `UnparseableLocation` variants per `contracts/swift-lockfile-format.md` § "Error semantics" using `thiserror`. Add `#[cfg(test)] mod tests` block exercising each PURL projection sub-rule including the SSH + deep-namespace + commit-pinned cases.

- [ ] T006 Implement `mikebom-cli/src/scan_fs/package_db/kotlin_dsl/version_catalog.rs::{VersionCatalog, ResolvedRef}` per data-model.md § Entity 2 + contracts/kotlin-dsl-extraction.md § "`libs.versions.toml` parsing". Function: `pub(super) fn parse(path: &Path) -> Result<VersionCatalog, CatalogError>` reads bytes, parses TOML via the workspace `toml` crate, reads `[versions]` into a temp `HashMap<String, String>`, walks `[libraries]` resolving `version.ref` references AND supporting BOTH the `module = "g:n"` form AND the split `group / name / version` form. Missing `version.ref` lookups emit `tracing::warn!` naming alias + catalog path; the entry DROPS. Define `pub(super) enum CatalogError` with `Io`, `ParseToml`, `MalformedLibraryEntry` variants via `thiserror`. Add `#[cfg(test)] mod tests` covering: pure `versions`+`libraries` happy path, `module` form, split-GAV form, missing-`version.ref` warn-and-drop, malformed `module` warn-and-drop.

- [ ] T007 Implement `mikebom-cli/src/scan_fs/package_db/kotlin_dsl/build_script.rs::{KotlinDslEntry, KotlinDepRaw}` per data-model.md § Entity 3. Compile the three regexes per contracts/kotlin-dsl-extraction.md § "`build.gradle.kts` dep declaration surface syntax" via `once_cell::sync::Lazy<Regex>` (or `std::sync::LazyLock` — workspace already pulls one). Function: `pub(super) fn extract_deps(content: &str, source_path: &Path) -> Vec<KotlinDslEntry>` walks the file content line-by-line, runs all three regexes per line, and collects matches into `KotlinDslEntry` records. Source-set tracking via brace-depth counting: track which `kotlin { sourceSets { <name> { dependencies { ... } } } }` block contains each line; populate `source_set: Option<String>` accordingly. Add `#[cfg(test)] mod tests` covering: fully-qualified-GAV form, catalog-alias form, named-arguments form, mixed-configurations file, KMP source-set block, top-level `dependencies` block with `source_set = None`, malformed-input warn-and-skip.

- [ ] T008 Implement `mikebom-cli/src/scan_fs/package_db/kotlin_dsl/build_script.rs::resolve_and_emit(entries: Vec<KotlinDslEntry>, catalog: Option<&VersionCatalog>, source_path: &Path, tracker: &mut KmpSourceSetTracker) -> Vec<super::super::PackageDbEntry>`. For each entry: resolve `CatalogAlias` via `catalog.libraries.get(&dotted_to_dashed(alias))`; resolve `PartialGav` via catalog lookup by name; emit one `PackageDbEntry` per resolved entry with the appropriate `pkg:maven/<group>/<name>@<version>` PURL, lifecycle-scope mapping per contracts/kotlin-dsl-extraction.md § "Dep-configuration → lifecycle-scope mapping", `mikebom:source-files` annotation, and `mikebom:sbom-tier = "design"` per clarification Q5 / FR-004. Source-set hits go through `tracker.record(purl.clone(), source_set.clone())`. Unresolvable entries emit `tracing::warn!` + drop. Add `#[cfg(test)] mod tests` covering: GAV-direct emission, catalog-resolved emission, partial-GAV with version-ref resolution, catalog-miss warn-and-drop, lifecycle-scope mapping for each dep-config family.

- [ ] T009 Implement `mikebom-cli/src/scan_fs/package_db/kotlin_dsl/mod.rs::KmpSourceSetTracker` per data-model.md § Entity 4. The `BTreeMap<Purl, BTreeSet<String>>` storage + `new()`, `record(purl, source_set)`, `finalize() -> Vec<(Purl, serde_json::Value::Array)>` methods. The `finalize` method projects each `BTreeSet<String>` into a `serde_json::Value::Array(Vec<Value::String>)` ready to stamp onto `PackageDbEntry::extra_annotations` under the `mikebom:kmp-source-set` key. Add `#[cfg(test)] mod tests` covering: empty tracker → empty finalize; single record → one-element array; multiple records with same PURL → deduped lex-sorted array; cross-PURL records produce independent arrays. **Pre-dedup duplication note** (per the FR-006 timing contract): every duplicate `PackageDbEntry` emitted by `build_script::resolve_and_emit` for the SAME canonical PURL MUST carry the SAME merged source-set array, so the milestone-105 `scan_fs::dedup` pipeline collapses them deterministically. `tracker.finalize()` is called AFTER all `KotlinDslEntry` records are recorded; the merged array is then stamped on EVERY pre-dedup duplicate (not just one) so the dedup pipeline preserves the merged value regardless of which duplicate it picks as the canonical entry.

- [ ] T010 Implement `mikebom-cli/src/scan_fs/package_db/kotlin_dsl/settings.rs::{SettingsScript, parse}` per contracts/kotlin-dsl-extraction.md § "`settings.gradle.kts` parsing". `SettingsScript { root_project_name: Option<String>, includes: Vec<String> }`. Function `pub(super) fn parse(path: &Path) -> Result<SettingsScript, SettingsError>` reads the file + extracts `rootProject.name = "..."` (single capture) + every `include(":module1", ":module2")` declaration (regex with repeated string captures). Missing fields are non-fatal (`root_project_name = None`; `includes = vec![]` if no `include(...)` found). Add `#[cfg(test)] mod tests` covering: full settings file with both `rootProject.name` + `include`, missing `rootProject.name` (falls back to None), multi-arg `include`, malformed file warn-and-degraded.

**Checkpoint**: After T004-T010, the foundational types + parsers exist; US1/US2/US3 work can begin.

---

## Phase 3: User Story 1 — Swift Package Manager reader (Priority: P1) 🎯 MVP

**Goal**: A `mikebom sbom scan --path <swift-project>` against a SwiftPM project emits every `Package.resolved` `pins[]` entry as a `pkg:swift/...` SBOM component.

**Independent Test**: Run `cargo +stable test --test scan_swift` and verify the four US1 acceptance scenarios pass (`pkg:swift/<host>/<ns>/<name>@<version>` emission; `.git` suffix stripping; commit-pinned full-SHA mode; Package.swift-without-Package.resolved warn-and-skip).

### Implementation for User Story 1

- [X] T011 [US1] Implement `mikebom-cli/src/scan_fs/package_db/swift/lockfile.rs::read_package_resolved(path: &Path) -> Result<Vec<SwiftLockfileEntry>, SwiftLockfileError>` per contracts/swift-lockfile-format.md § "Per-version schema". Read bytes via `std::fs::read`, parse JSON via `serde_json::from_slice` into a top-level `serde_json::Value`, read the `version` integer, dispatch per Decision 2: v1 reads `object.pins[]`; v2/v3 read top-level `pins[]`. Each pin produces one `SwiftLockfileEntry` after validating the 40-char hex regex on `state.revision`. Unknown versions return `Err(SwiftLockfileError::UnknownVersion { ... })`. Add `#[cfg(test)] mod tests` covering: v1 happy path, v2 happy path, v3 with `originHash` ignored, unknown-version err, missing-pins err, invalid-revision-skips-entry-but-continues.

- [X] T012 [US1] Implement `mikebom-cli/src/scan_fs/package_db/swift/manifest.rs::detect(path: &Path) -> bool` per FR-002 / clarification Q3 — returns `true` iff the file exists + is at least 0 bytes (no content parsing). Document the contract explicitly in the doc-comment: "Package.swift content is NEVER parsed in v0.1". Add `#[cfg(test)] mod tests` covering: file present → true, file absent → false, file exists but is a directory → false.

- [X] T013 [US1] Implement the Swift reader entry point `mikebom-cli/src/scan_fs/package_db/swift/mod.rs::read(rootfs, exclude_set)` per contracts/swift-lockfile-format.md + research.md § Decision 7. Use `crate::scan_fs::walk::safe_walk` with a `WalkConfig` (max_depth 6, the existing default-descent skip predicate) to find every `Package.resolved` under the scan tree. For each found `Package.resolved`: call `lockfile::read_package_resolved(path)`; on success, project each `SwiftLockfileEntry` to a `PackageDbEntry` using `lockfile::project_purl` and stamping `mikebom:source-files = "<path-to-Package.resolved>"` + (when commit-pinned) `mikebom:source-type = "git"` + `mikebom:source-revision = "<sha>"`. On parse failure: emit `tracing::warn!` naming the path + the error; zero components contributed; walk continues. Also detect sibling `Package.swift` files via `manifest::detect`: when a directory has `Package.swift` but no sibling `Package.resolved`, emit `tracing::warn!` naming the unresolved manifest (FR-002 / FR-009 fail-closed). The reader honors `--exclude-path` via the existing `safe_walk` integration (FR-011). Skip `.build/` subtrees explicitly per the edge-case rule from spec.

- [X] T014 [US1] Register the new Swift reader in the `read_all` dispatcher at `mikebom-cli/src/scan_fs/package_db/mod.rs`. Add `out.extend(swift::read(rootfs, exclude_set));` after the existing `gradle::read` call (~line 1390-1395 per the plan's call-site reference). Run `cargo +stable test --workspace` and confirm zero pre-existing test failures (the new reader returns zero components against non-Swift fixtures, so existing golden tests stay byte-identical per SC-007).

- [X] T015 [US1] Create `mikebom-cli/tests/fixtures/golden_inputs/swift_package_resolved/` with a minimal SwiftPM project layout: `Package.swift` declaring `let package = Package(name: "demo-swift", products: [.library(name: "demo-swift", targets: ["demo-swift"])], dependencies: [.package(url: "https://github.com/apple/swift-argument-parser.git", from: "1.3.0"), .package(url: "https://github.com/Alamofire/Alamofire.git", from: "5.9.0")], targets: [.target(name: "demo-swift", dependencies: ["swift-argument-parser", "Alamofire"])])` + `Package.resolved` (v2 schema) declaring those same two deps with realistic 40-char SHAs. Also create a second fixture under `swift_commit_pinned/` containing a `Package.resolved` entry with `state.version` ABSENT but `state.revision` present (the commit-pinned-mode test case). Each fixture is in-tree (committed to git) so CI can scan them deterministically.

- [X] T016 [US1] Create `mikebom-cli/tests/scan_swift.rs` with test scaffolding (`use std::process::Command;`, `binary_path()`, `run_scan(root)` helper that invokes mikebom with `--no-deep-hash` / `--offline` / `MIKEBOM_FIXED_TIMESTAMP` env per the milestone-119 / milestone-113 precedent). Write the four US1 acceptance-scenario tests: `us1_as1_swift_argument_parser_emits_as_pkg_swift`; `us1_as2_dot_git_suffix_stripped_from_purl`; `us1_as3_commit_pinned_uses_full_sha_as_version_segment`; `us1_as4_package_swift_without_resolved_warns_and_emits_zero`. Each test scans the fixture, parses the emitted CDX, asserts the expected `pkg:swift/...` PURL presence + the `mikebom:source-revision` annotation shape + (for AS3) the `mikebom:source-type = "git"` annotation + (for AS4) verifies the stderr warn line names the unresolved manifest. Tests guard `.unwrap()` with the standard `#[cfg_attr(test, allow(clippy::unwrap_used))]` gate.

**Checkpoint**: After T011-T016, US1 is fully functional. SwiftPM projects produce non-empty SBOMs; the milestone's headline P1 promise (SC-002) is achieved. PR could ship here if review feedback pushes back on US2 / US3 diff size.

---

## Phase 4: User Story 2 — Kotlin DSL Gradle reader (Priority: P1)

**Goal**: A `mikebom sbom scan --path <kotlin-project>` against an Android-Studio-generated Kotlin DSL project emits every declared Maven dependency as a `pkg:maven/...` SBOM component, even when no `gradle.lockfile` is present.

**Independent Test**: Run `cargo +stable test --test scan_kotlin_dsl` and verify the seven US2 acceptance scenarios pass (build.gradle.kts dep emission; libs.versions.toml catalog resolution; lifecycle-scope mapping per dep-config; KMP source-set JSON-array; multi-module workspace synthesis; dynamic version preservation).

### Implementation for User Story 2

- [ ] T017 [US2] Implement the Kotlin DSL reader entry point `mikebom-cli/src/scan_fs/package_db/kotlin_dsl/mod.rs::read(rootfs, include_dev, exclude_set)`. Use `safe_walk` to find every `settings.gradle.kts` first; when multiple are found in the scan tree (the "two-deep nested Gradle workspaces" edge case from spec), treat ONLY the OUTERMOST one (the shortest directory path from `rootfs`) as the workspace root — inner `settings.gradle.kts` files are walked for their sibling `build.gradle.kts` discovery only and DO NOT emit additional `pkg:generic/...@0.0.0` workspace-root components. Implementation: sort discovered settings paths by depth ascending, take the first as canonical, treat all others as plain directories. Parse the canonical settings file via `settings::parse`. For each `build.gradle.kts` under the scan tree (multi-module workspaces have many): load the file content; call `build_script::extract_deps` for the entry vec; consult the workspace's `libs.versions.toml` (via the milestone-064 walk-up-from-`build.gradle.kts`-directory pattern — grep `cargo.rs` for `workspace_walkup` or `find_workspace_root` for the canonical function, OR duplicate the walk-up locally for kotlin_dsl); call `build_script::resolve_and_emit` with a single shared `KmpSourceSetTracker`. After all `build.gradle.kts` files are processed, call `tracker.finalize()` and STAMP each `(Purl, JSON-array)` pair onto **every** pre-dedup component whose canonical PURL matches — the milestone-105 dedup pipeline downstream collapses duplicates while preserving the stamped array (per FR-006 timing contract; see T009 note). Synthesize ONE workspace-root `PackageDbEntry` per detected outermost `settings.gradle.kts` (per FR-007 / clarification Q4 + the `kotlin_dsl::synthesize_workspace_root` helper landed in T018). Honor `include_dev`: when `false`, drop components carrying `mikebom:sbom-tier = "design"` (the design-tier gating per clarification Q5); when `true`, emit them.

- [ ] T018 [US2] Implement `mikebom-cli/src/scan_fs/package_db/kotlin_dsl/mod.rs::synthesize_workspace_root(settings: &SettingsScript, project_dir: &Path) -> PackageDbEntry` per contracts/kotlin-dsl-extraction.md § "Workspace-root emission". PURL `pkg:generic/<rootProject.name>@0.0.0` (falls back to the workspace directory name when `rootProject.name` is `None`); `mikebom:component-role = "workspace-root"`; `mikebom:source-files = "<path-to-settings.gradle.kts>"`; `lifecycle_scope = None`; `sbom_tier = Some("source")`. Add `#[cfg(test)] mod tests` covering: full SettingsScript happy path, missing `rootProject.name` falls back to dir name, `include(...)` modules produce sibling main-module entries.

- [ ] T019 [US2] Register the new Kotlin DSL reader in the `read_all` dispatcher at `mikebom-cli/src/scan_fs/package_db/mod.rs`. Add `out.extend(kotlin_dsl::read(rootfs, include_dev, exclude_set));` after the existing `gradle::read` call (~line 1389). Confirm zero pre-existing test failures via `cargo +stable test --workspace`.

- [ ] T020 [US2] Wire the `mikebom:kmp-source-set` annotation through the existing emitter at `mikebom-cli/src/generate/cyclonedx/builder.rs:965-973` (per-component properties). The existing `extra_annotations` serialization path already JSON-encodes `serde_json::Value::Array` values via `to_string`, so no emitter change is required IF T009 + T017 store the value correctly. Run `cargo +stable check -p mikebom` to confirm. (This task is a verify-not-implement step; if a code change IS required after running the test suite, the implementation belongs here.)

- [ ] T021 [US2] Create `mikebom-cli/tests/fixtures/golden_inputs/kotlin_dsl_gradle/` with a minimal Kotlin Android project: `settings.gradle.kts` declaring `rootProject.name = "demo-kts"` + `include(":app", ":shared")`; `gradle/libs.versions.toml` declaring `[versions] okhttp = "4.12.0", kotlin = "1.9.20", ktor = "2.3.7"` + `[libraries] okhttp = { module = "com.squareup.okhttp3:okhttp", version.ref = "okhttp" }, ktor-client-cio = { module = "io.ktor:ktor-client-cio-jvm", version.ref = "ktor" }`; `app/build.gradle.kts` declaring `dependencies { implementation(libs.okhttp); api("org.jetbrains.kotlin:kotlin-stdlib:1.9.20"); testImplementation("io.kotest:kotest-runner-junit5:5.8.0"); kapt("com.google.dagger:dagger-compiler:2.50") }`; `shared/build.gradle.kts` declaring a KMP `kotlin { sourceSets { commonMain { dependencies { implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.6.2") } }, jvmMain { dependencies { implementation(libs.ktor.client.cio) } } } }` block. All files in-tree (committed to git).

- [ ] T022 [US2] Create `mikebom-cli/tests/scan_kotlin_dsl.rs` with the test scaffolding mirroring `scan_swift.rs`. Write the seven US2 acceptance-scenario tests: `us2_as1_implementation_dep_emits_as_pkg_maven`; `us2_as2_libs_alias_resolves_through_version_catalog`; `us2_as3_dep_config_to_lifecycle_scope_mapping_covers_all_families` (verifies `implementation`→none, `testImplementation`→`test`, `debugImplementation`→`development`, `kapt`→`build` on the same fixture); `us2_as4_kmp_source_set_emits_json_array_lex_sorted`; `us2_as5_settings_kts_workspace_synthesizes_pkg_generic_root` (verify `pkg:generic/demo-kts@0.0.0` workspace-root + two sibling main-module components); `us2_as6_dynamic_version_preserves_verbatim_and_marks_design` (`+` and range `[1.0, 2.0)` cases); `us2_design_tier_gated_by_include_declared_deps` (run scan with `--no-include-declared-deps` + `--image` mode-like flag combination — verify deps absent; run with the auto-on `--path` mode — verify deps present).

- [ ] T023 [US2] Create the second fixture `mikebom-cli/tests/fixtures/golden_inputs/kotlin_dsl_dynamic_version/` containing only an `app/build.gradle.kts` with `implementation("io.example:lib:+")` and `implementation("io.example:lib2:[1.0, 2.0)")` (verbatim version preservation cases). Used by `us2_as6` test.

- [ ] T024 [US2] Add the `--exclude-path` cross-reader test to `mikebom-cli/tests/scan_kotlin_dsl.rs`: `us2_kotlin_dsl_honors_exclude_path` — scan the existing `kotlin_dsl_gradle/` fixture with `--exclude-path '**/shared'` and verify the `shared/` module's deps (the kotlinx-serialization-json dep + the ktor-client-cio dep) are absent from the emitted SBOM while `app/`'s deps still emit. Confirms FR-011 across the new reader. Also add `us2_nested_gradle_workspace_emits_only_outermost_workspace_root` test using a synthetic two-deep-nested fixture (`tempfile::tempdir()` ad-hoc construction; no in-tree fixture needed): outer `settings.gradle.kts` declares `include(":outer-app")`; the `outer-app/` directory contains its OWN `settings.gradle.kts` (the defensive coding pattern from real KMP samples). Verify the emitted SBOM contains EXACTLY ONE `pkg:generic/...@0.0.0` workspace-root component (the outer one); the inner workspace-root is NOT emitted (per the T017 outermost-only rule + the spec's "Two-deep nested Gradle workspaces" edge case).

**Checkpoint**: After T017-T024, US2 is fully functional. Android-Studio-default Kotlin DSL projects produce non-empty SBOMs; the milestone's other P1 promise (SC-001 + SC-004) is achieved. PR could ship here if review feedback pushes back on US3 diff size.

---

## Phase 5: User Story 3 — Kotlin Multiplatform polyglot monorepo (Priority: P2)

**Goal**: A `mikebom sbom scan --path <kmp-monorepo>` against a KMP polyglot project emits BOTH `pkg:maven/...` (Android side) AND `pkg:swift/...` (iOS side) components in one SBOM with no cross-ecosystem dedup collapse.

**Independent Test**: Run `cargo +stable test --test scan_kmp_polyglot` and verify the three US3 acceptance scenarios pass (polyglot emission; KMP source-set workspace-root → module → dep edge; cross-ecosystem-name-collision distinct components).

### Implementation for User Story 3

- [ ] T025 [US3] Create `mikebom-cli/tests/fixtures/golden_inputs/kmp_polyglot/` with the three-module KMP monorepo layout: `settings.gradle.kts` declaring `rootProject.name = "kmp-app"` + `include(":androidApp", ":shared")`; `androidApp/build.gradle.kts` declaring Android-side deps (`implementation("androidx.core:core-ktx:1.12.0")`); `shared/build.gradle.kts` declaring KMP source-set deps (`commonMain` declaring `kotlinx-serialization-json`); `iosApp/Package.swift` + `iosApp/Package.resolved` (v2 schema) declaring `Alamofire 5.9.0` + `swift-log 1.5.4`. The iOS side has no `build.gradle.kts` at all — it's a pure SwiftPM project nested under the KMP root.

- [ ] T026 [US3] Create `mikebom-cli/tests/scan_kmp_polyglot.rs` with the three US3 acceptance-scenario tests: `us3_as1_polyglot_scan_emits_both_pkg_maven_and_pkg_swift_components` (count maven + swift PURL families in emitted SBOM); `us3_as2_kmp_workspace_root_to_module_dep_edges_preserved` (verify `pkg:generic/kmp-app@0.0.0` → main-module → kotlinx-serialization-json edge AND main-module → Alamofire edge); `us3_as3_same_name_different_ecosystem_emits_two_distinct_components` (verify `pkg:maven/.../some-lib@1.0.0` AND `pkg:swift/github.com/.../some-lib@1.0.0` both emerge when the operator declares the same name on both sides — synthesize via an additional fixture entry).

- [ ] T027 [US3] Add `us3_no_swift_no_kotlin_byte_identical` regression test to `mikebom-cli/tests/scan_kmp_polyglot.rs`: scan a pure cargo project fixture (reuse the `kotlin_dsl_dynamic_version` fixture but rename the test to scan a synthesized `tempfile::tempdir()` with only a `Cargo.toml`); compare the emitted SBOM byte-for-byte against the pre-feature mikebom build of the same fixture (use `MIKEBOM_FIXED_TIMESTAMP` for determinism + strip `serialNumber`). Confirms SC-007.

**Checkpoint**: After T025-T027, US3 is fully functional. KMP polyglot monorepos produce one combined SBOM (SC-003). The milestone's full scope is complete.

---

## Phase 6: Polish

- [X] T028 Add the negative-test runbook tests from `quickstart.md` § "Negative-test runbook" to the appropriate integration test files: `swift_malformed_package_resolved_warns_and_continues` + `swift_unknown_schema_version_warns_and_continues` + `swift_ssh_url_emits_via_ssh_host` to `scan_swift.rs`; `kotlin_dsl_unparseable_build_script_warns_and_continues` + `kotlin_dsl_missing_catalog_alias_warns_and_drops` to `scan_kotlin_dsl.rs`. Each test verifies a `tracing::warn!` log line on stderr + zero-or-degraded components in the emitted SBOM + scan exit code 0.

- [X] T029 Add new `## kotlin` + `## swift` sections to `docs/ecosystems.md` after the existing `## yocto` section per the established per-ecosystem docs convention (mirrors the milestone-106 docs additions). Each section covers: discovery rules (which files mikebom looks for), PURL projection examples (one happy-path + one edge case per ecosystem), the dep-config → lifecycle-scope mapping (Kotlin only), the KMP source-set annotation (Kotlin only), workspace-member emission convention, and a pointer to `docs/user-guide/cli-reference.md`. Also add Kotlin DSL + SwiftPM rows to the Coverage matrix at the top of the file + pointer lines to the `## Directory exclusion (--exclude-path)` cross-cutting section per the milestone-113 / 118 ecosystem convention (formulaic single-string + anchor per the milestone-118 verification regex).

- [X] T030 Run `MIKEBOM_SKIP_DOCKER_INTEGRATION=1 ./scripts/pre-pr.sh` from the repo root. Verify clippy `--workspace --all-targets -D warnings` passes clean AND `cargo +stable test --workspace` passes clean (every suite `ok. N passed; 0 failed`). Per CLAUDE.md this is MANDATORY before opening any PR. Fix any clippy doc-list-continuation lints (the recent milestone-119 / 120 / 121 PRs all hit this lint on multi-line doc comments — proactively check that the new module + helper doc-comments are properly indented). **Additionally verify FR-013 / SC-006 (zero new Cargo deps)**: run `git diff main -- mikebom-cli/Cargo.toml mikebom-cli/Cargo.lock` and confirm the diff shows ZERO additions in the `[dependencies]` or `[dev-dependencies]` sections of `Cargo.toml`. Lockfile churn from existing-dep version selections IS acceptable; new dep entries are NOT — on any unexpected dep addition, abort the PR and investigate which task introduced it. **Additionally verify FR-012 (no network calls)** via a static check: `grep -rn -E 'reqwest::|tokio::net|hyper::|ureq::' mikebom-cli/src/scan_fs/package_db/swift/ mikebom-cli/src/scan_fs/package_db/kotlin_dsl/ | grep -v -E '#\[cfg\(test\)\]|//'` MUST return zero matches — the two new modules are pure file readers; no network primitives belong in either tree.

- [X] T031 Update `specs/122-kotlin-swift-readers/tasks.md` (this file) marking T001-T030 as `[X]` completed.

- [X] T032 Commit per CLAUDE.md commit protocol. Commit title: `feat(scan_fs): Kotlin DSL Gradle + Swift Package Manager ecosystem readers (milestone 122)`. Commit body summarizes: (a) two new readers — `scan_fs/package_db/swift/` parsing `Package.resolved` lockfiles (v1/v2/v3 schema) and emitting `pkg:swift/<host>/<ns>/<name>@<version>`; `scan_fs/package_db/kotlin_dsl/` regex-extracting `build.gradle.kts` deps + resolving `libs.versions.toml` catalogs + emitting `pkg:maven/...`; (b) one new annotation key (C68 `mikebom:kmp-source-set`) with full Principle V audit row in `docs/reference/sbom-format-mapping.md`; (c) zero new Cargo dependencies; (d) workspace-root synthesis as `pkg:generic/<rootProject.name>@0.0.0` per the milestone-106 uv / bun convention; (e) commit-pinned Swift PURL uses FULL 40-char SHA (clarification Q1); (f) KMP source-set provenance via JSON-encoded array (clarification Q2); (g) `Package.swift` detected but never parsed (clarification Q3); (h) `build.gradle.kts`-only-discovered components are design-tier gated by `--include-declared-deps` (clarification Q5); (i) full integration test suite covering US1 + US2 + US3 acceptance + negative-test runbook.

- [X] T033 Open PR. Title: `feat(scan_fs): Kotlin DSL Gradle + Swift Package Manager ecosystem readers (milestone 122)`. Body includes: (1) the milestone scope + the link to spec.md + the five clarifications applied during /speckit-clarify (Q1 full-SHA, Q2 JSON-array, Q3 `Package.swift` no-parse, Q4 `pkg:generic` workspace-root, Q5 design-tier gating); (2) `## Summary` listing both readers + the new C68 row + the docs additions + the zero-new-Cargo-deps posture; (3) `## Test plan` listing the 17 acceptance scenarios + 5 negative-test scenarios as manually-verified-on-this-PR checklist items; (4) `## Cross-platform sanity` checklist — Linux x86_64 + macOS aarch64 + Windows x86_64 CI lanes all green; (5) the Principle V audit conclusion citing C68 in `docs/reference/sbom-format-mapping.md`; (6) reference to the milestone-106 ecosystem-expansion precedent + cite the spec's Assumptions section calling out the deferred items (CocoaPods, Carthage, full Package.swift parsing).

---

## Dependencies & Execution Order

```text
Phase 1 Setup:           T001 → T002 → T003 (sequential — file-coordination on docs + extractor table)

Phase 2 Foundational:    T004 → T005 → T006 → T007 → T008 → T009 → T010 (sequential — type chain: SwiftLockfileEntry → PURL projection → VersionCatalog → KotlinDslEntry extraction → resolve_and_emit → KmpSourceSetTracker → SettingsScript)

Phase 3 US1 (P1, MVP):   T011 → T012 → T013 → T014 → T015 → T016
                                                                            ↓
                                                                       (US2 + US3 parallel branches open)

Phase 4 US2 (P1):        T017 → T018 → T019 → T020 → T021 → T022 → T023 → T024

Phase 5 US3 (P2):        T025 → T026 → T027

Phase 6 Polish:          T028 → T029 → T030 → T031 → T032 → T033
```

**Sequential chains**:

- T001 → T002 → T003 (Setup — file-coordinated edits on `mod.rs` + docs + extractor table)
- T004 → T005 → T006 → T007 → T008 → T009 → T010 (Foundational — entity-type + parser chain; each depends on its predecessor's public surface)
- T011 → T012 → T013 → T014 → T015 → T016 (US1 — production code chain → fixture → tests)
- T017 → T018 → T019 → T020 → T021 → T022 → T023 → T024 (US2 — same shape)
- T025 → T026 → T027 (US3 — fixture → tests → regression)
- T028 → T029 → T030 → T031 → T032 → T033 (Polish — linear ratchet)

**Parallel branches**: After Phase 2 checkpoint (T010 done), Phase 3 (US1) and Phase 4 (US2) can land concurrently — they share NO production files. Phase 5 (US3) is sequential after BOTH Phase 3 + Phase 4 complete because the polyglot tests require both readers to be live.

## Parallel Opportunities

Within Phase 2 Foundational (after T004 lands):

```text
# Production-code files are independent:
T005 [US1] — swift/lockfile.rs (PURL projection + error types)  } parallel-safe
T006 [US2] — kotlin_dsl/version_catalog.rs                       } parallel-safe
T007 [US2] — kotlin_dsl/build_script.rs (regex + entry types)    } depends on T006 for catalog ref shape
T009 [US2] — kotlin_dsl/mod.rs (KmpSourceSetTracker)             } parallel with T005-T007 (different file)
T010 [US2] — kotlin_dsl/settings.rs                              } parallel with T005-T007
```

Across user stories (after Phase 2 done):

```text
# US1 vs US2 production code is fully independent:
T011-T014 [US1] — swift/ module                                  } parallel branch
T017-T020 [US2] — kotlin_dsl/ module + dispatcher                } parallel branch
```

Within Polish phase (T028 onward), most tasks are sequential by dependency (pre-PR must pass before commit; commit must land before PR).

## Independent Test Criteria

Per spec's three user stories:

- **US1 (P1) — MVP**: Confirmed by T016's four acceptance tests (PURL emission; `.git` stripping; commit-pinned full SHA; warn-and-skip on missing lockfile) plus T028's three Swift negative tests.
- **US2 (P1)**: Confirmed by T022's seven acceptance tests (dep emission; catalog resolution; lifecycle-scope mapping; KMP source-set; workspace synthesis; dynamic versions; design-tier gating) plus T024's `--exclude-path` cross-reader regression plus T028's two Kotlin DSL negative tests.
- **US3 (P2)**: Confirmed by T026's three acceptance tests (polyglot composition; KMP workspace edges; cross-ecosystem distinct components) plus T027's byte-identity regression.

## Implementation Strategy

**Single-PR ship**: T001 → T033 in one PR. ~900 LoC production + ~500 LoC tests + ~250 LoC docs per plan.md estimate.

**MVP scope**: T001-T016 + T030-T033 (Setup + Foundational + US1 + Polish minus US2 + US3 phases). After this scope ships, the milestone's headline SwiftPM promise (SC-002) is closed; Android-Studio Kotlin DSL projects still produce empty SBOMs but the existing milestone-106 `gradle.lockfile` reader keeps working for projects that have dependency locking enabled.

**Cut-points** (per plan.md "Complexity Tracking"): if review feedback pushes back on diff size, US3 (T025-T027) defers to a follow-up PR — the polyglot fixture exercises composition that US1 + US2 already deliver independently. US2 alone (T017-T024) is also a natural cut-point because it shares no production code with US1.

**Format validation**: All 33 tasks above use the required checklist format — `- [ ]` checkbox + sequential ID (T001…T033) + optional [P] marker + [US1]/[US2]/[US3] label for user-story tasks (Setup + Foundational + Polish tasks have no story label) + description with exact file path(s).
