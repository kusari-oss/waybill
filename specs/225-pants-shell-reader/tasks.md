---
description: "Task list for feature 225 (Pants shell reader)"
---

# Tasks: Pants shell reader

**Input**: Design documents from `/specs/225-pants-shell-reader/`
**Prerequisites**: plan.md ✅, spec.md ✅ (3 user stories, 12 FRs, 6 SCs), research.md ✅ (5 items), data-model.md ✅ (4 module-private types + 2 config helpers), contracts/build-file-dsl-schema.md ✅, contracts/c145-waybill-pants-target.md ✅, quickstart.md ✅

**Tests**: Tests ARE included — every reader shipped since m002 has test coverage per Constitution Principle VII, and the regex BUILD-DSL extractor + target-address resolver + cross-target dedup introduce failure modes that only tests can audit.

**Organization**: Tasks grouped by user story. Follows m224's shape with one net-new phase for the C145 parity-catalog work (memory `feedback_sbom_format_mapping_extractor_gate` requires row + 3 extractors + tests to land together).

## Format: `[TaskID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3
- File paths absolute or repo-relative from `/Users/mlieberman/Projects/mikebom`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Prepare the module directory + register it.

- [X] T001 Create module directory `waybill-cli/src/scan_fs/package_db/pants_shell/` with 5 empty stub files (`mod.rs`, `build_dsl.rs`, `target_resolver.rs`, `config.rs`, `component_emit.rs`), each carrying only a `//! Milestone 225: <purpose>` doc-comment.
- [X] T002 Register the new module: add `pub mod pants_shell;` to `waybill-cli/src/scan_fs/package_db/mod.rs` alphabetically (between `pants_jvm` and `pip`). Verify with `cargo +stable build -p waybill --bin waybill` — should compile clean (readers do nothing yet).

**Checkpoint**: Empty pants_shell module registered. Compile clean.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Define the regex BUILD-DSL extractor + `Coordinate`-equivalent types + `pants.toml` `[shellcheck]`/`[shfmt]`/`[shunit2]` config parser + target-address resolver. All three user stories depend on these.

**⚠️ CRITICAL**: US1/US2/US3 all depend on these types + helpers.

- [X] T003 In `waybill-cli/src/scan_fs/package_db/pants_shell/mod.rs`, add module-private types `ShellTargetKind` (closed enum with 4 variants + `lifecycle_scope()` + `as_dsl_name()` methods per data-model.md §"ShellTargetKind"), `TargetDeclaration { kind, name, source, start_line }`, `TargetSource { Single(String), Globs(Vec<String>) }`, `ResolvedTarget { address, kind, files, build_file }`, and `TargetParseError` (thiserror enum with 3 variants per data-model.md). Also declare the sub-modules: `mod build_dsl; mod target_resolver; mod config; mod component_emit;`.
- [X] T004 [P] In `waybill-cli/src/scan_fs/package_db/pants_shell/build_dsl.rs`, add `pub(crate) fn extract_targets(bytes: &[u8]) -> Vec<Result<TargetDeclaration, TargetParseError>>` per research.md §R2. Two regex patterns compiled via `OnceLock` (single-source shape + multi-sources shape). Handles single- and double-quoted string literals + trailing commas + arbitrary kwargs before/between/after `name=` / `source=` / `sources=[...]`. Add 12 unit tests: valid `shell_source` (name/source order both ways), valid `shell_sources` with 3-element `sources=[]` list, valid `shunit2_test`, valid `shunit2_tests` with default name (no `name=` kwarg), variable-reference source (returns `NonStringLiteralSource` err), concat source (returns err), missing `name` AND missing `source` (returns `MissingRequiredKwarg`), multi-line target spanning 3 lines, comment-line inside the target (ignored), 3 valid targets in a single BUILD-file blob (all 3 parse), unbalanced-parens (returns `UnbalancedParens`), zero recognized targets (returns empty vec).
- [X] T005 [P] In `waybill-cli/src/scan_fs/package_db/pants_shell/target_resolver.rs`, add `pub(crate) fn resolve_target(decl: &TargetDeclaration, build_file: &Path, scan_root: &Path) -> Option<ResolvedTarget>`. Computes target address per contracts §"Target address resolution" (BUILD file's parent dir relative to scan root + `name=` or dir basename fallback for globs). Resolves `TargetSource::Single(rel_path)` via `<build_dir>/<rel_path>`. Resolves `TargetSource::Globs(patterns)` via non-recursive glob for `*` / recursive for `**` — use the `globset` workspace dep (already in the closure per m113 `ExclusionSet`). Drops files that don't exist on disk with a WARN naming the target address + missing path. Add 8 unit tests: single-source resolves to 1 file, `sources=["*.sh"]` matches 3 files in a fixture dir, `sources=["**/*.sh"]` recurses, missing source file returns `ResolvedTarget` with empty `files` and WARN emitted, empty glob match emits INFO not WARN, root-level BUILD address is `<name>` no prefix, subdirectory BUILD address is `<subdir>:<name>`, missing-name-fallback uses dir basename.
- [X] T006 [P] In `waybill-cli/src/scan_fs/package_db/pants_shell/config.rs`, add module-private `ShellSetupConfig { shellcheck, shfmt, shunit2 }` + `ExternalToolSection { version: Option<String> }` Deserialize types per data-model.md §"ShellSetupConfig". Add `pub(crate) fn parse(bytes: &[u8]) -> Option<ShellSetupConfig>` returning `None` on any `toml::from_str` error (FR-004 fail-open). Add 4 unit tests: all 3 sections with `version` set, only `[shellcheck]` with `version`, `[shellcheck]` present but no `version` key, malformed TOML.
- [X] T007 [P] In `waybill-cli/src/scan_fs/package_db/pants_shell/component_emit.rs`, add `pub(crate) fn script_to_package_db_entry(file: &Path, target_addresses: &[String], kind: ShellTargetKind, scan_root: &Path) -> Option<PackageDbEntry>`. Streams SHA-256 over the file bytes (matches the m133 pattern at `file_tier/walker.rs`). Constructs PURL `pkg:generic/<url-encoded-basename>@<sha256[:12]>` per research.md §R3. Emits `waybill:pants-target` annotation with comma-separated lexically-sorted addresses. Emits `waybill:source-files` as a JSON-array-in-string per m080 convention. Sets `lifecycle_scope` from `kind.lifecycle_scope()`, `sbom_tier = Some("source")`, full sha256 in `hashes[]`. Returns None + WARN on I/O error or PURL construction failure. Add 5 unit tests: happy-path plain shell_source component, shunit2 component tags Development, multi-target dedup annotation is lex-sorted comma-sep, PURL is content-addressed and stable, missing file returns None.
- [X] T008 In `waybill-cli/src/scan_fs/package_db/pants_shell/component_emit.rs` (append), add `pub(crate) fn tool_to_package_db_entry(tool_name: &str, version: &str, pants_toml_path: &Path, scan_root: &Path) -> Option<PackageDbEntry>`. Constructs PURL `pkg:generic/<tool_name>@<version>` verbatim (preserving leading `v` prefix per R4). Sets `lifecycle_scope = Some(Development)`, `sbom_tier = Some("design")`, emits `waybill:source-file = pants.toml` annotation (m080 row). Empty `hashes[]`. Add 3 unit tests: shellcheck v0.9.0 happy path, shfmt 3.7.0 (no v prefix — preserved), version "" returns None with WARN.

**Checkpoint**: Types + BUILD-DSL extractor + target resolver + config parser + component-emit helpers all compile with green unit tests. Ready for the orchestrator wiring.

---

## Phase 3: New parity-catalog row C145 `waybill:pants-target`

**Purpose**: Add C145 to the parity catalog + register its 3 extractors. Per memory `feedback_sbom_format_mapping_extractor_gate`, adding one without the other fails `parity::extractors::tests::every_catalog_row_has_an_extractor` — both must land together.

**⚠️ CRITICAL**: US1 emissions include `waybill:pants-target`; without this phase, US1 integration tests won't pass the parity gate.

- [X] T009 [P] In `docs/reference/sbom-format-mapping.md`, add row C145 following the exact 5-column template of C143 (see line 183). Full contents per `specs/225-pants-shell-reader/contracts/c145-waybill-pants-target.md` §"Documentation contract". Insert alphabetically after C144 at line 184.
- [X] T010 [P] In `waybill-cli/src/parity/extractors/cdx.rs`, add `cdx_anno!(c145_cdx, "waybill:pants-target", component);` — mirror the C143 registration at line 859. Include the same "Milestone 225 US1" comment shape.
- [X] T011 [P] In `waybill-cli/src/parity/extractors/spdx2.rs`, add `spdx23_anno!(c145_spdx23, "waybill:pants-target", component);` — mirror the C143 registration at line 618.
- [X] T012 [P] In `waybill-cli/src/parity/extractors/spdx3.rs`, add `spdx3_anno!(c145_spdx3, "waybill:pants-target", component);` — mirror the C143 registration at line 678.
- [X] T013 In `waybill-cli/src/parity/extractors/mod.rs` (at the `EXTRACTORS` array, after the C144 entry), add the C145 `ParityExtractor { row_id, label, cdx: c145_cdx, spdx23: c145_spdx23, spdx3: c145_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false }` per contract §"Registration". Then verify the `c145_cdx` / `c145_spdx23` / `c145_spdx3` symbols are reachable — check the existing `use` block's import style (per-symbol named imports vs `use cdx::*` glob) and add named imports in the same style if named imports are the pattern. Confirm by running `cargo +stable build -p waybill --bin waybill` — should compile clean.
- [X] T014 Run `cargo +stable test -p waybill --bin waybill parity::extractors::tests` to confirm `every_catalog_row_has_an_extractor` + `holistic_parity` pass. Both should be green after T009+T010+T011+T012+T013 all land.

**Checkpoint**: C145 row + 3 extractors + parity gate all green. Emit path for `waybill:pants-target` is fully plumbed.

---

## Phase 4: User Story 1 — BUILD-declared shell scripts (Priority: P1) 🎯 MVP

**Goal**: `waybill sbom scan` against a Pants repo containing `BUILD` files with `shell_source` / `shell_sources` targets emits one `pkg:generic/*` file-tier component per resolved `.sh` file, each with SHA-256 hash + `waybill:pants-target` annotation.

**Independent Test**: Run the scan against `waybill-cli/tests/fixtures/pants_shell/minimal_scripts/`. Assert the emitted CDX has 2 components with `pkg:generic/waybill-fixture-*.sh@<sha[:12]>` PURLs, sha256 hashes matching the fixture files, `waybill:pants-target=scripts:<name>` on each, and correct lifecycle scope.

### Tests for User Story 1

- [X] T015 [P] [US1] Create synthetic fixture `waybill-cli/tests/fixtures/pants_shell/minimal_scripts/` per research.md §R5 fixture 1: `scripts/BUILD` with `shell_source(name="deploy", source="waybill-fixture-deploy.sh")` + `shell_sources(name="utils", sources=["waybill-fixture-*.sh"])`; 2 synthetic `.sh` files with distinct content. Add a comment-line inside the BUILD file to exercise T004's comment-tolerance path.
- [X] T016 [P] [US1] Create synthetic fixture `waybill-cli/tests/fixtures/pants_shell/glob_sources/` per research.md §R5 fixture 2: `helpers/BUILD` with `shell_sources(name="utils", sources=["*.sh"])` + 3 synthetic `waybill-fixture-*.sh` files.
- [X] T017 [P] [US1] Create integration test file `waybill-cli/tests/pants_shell_reader.rs` with 3 initial `#[test]` functions (bodies filled in T018-T020):
  - `us1_minimal_scripts_emits_2_components_with_sha256_and_target_annotation`
  - `us1_glob_sources_expands_to_3_components`
  - `us1_fr010_info_log_emits_all_six_structured_fields`
  Import helpers `bin()`, `run_scan()`, `read_cdx()`, `get_property()`, `strip_ansi()` mirroring `waybill-cli/tests/pants_coursier_jvm_reader.rs` verbatim.
- [X] T018 [US1] Implement `us1_minimal_scripts_emits_2_components_with_sha256_and_target_annotation` in `waybill-cli/tests/pants_shell_reader.rs`. Uses fixture from T015. **Emits BOTH formats in one scan invocation** (`--format cyclonedx-json --format spdx-2.3-json --output <fmt>=<path>`) per SC-001. Assert: (a) exit 0; (b) CDX contains 2 pants-shell components with `pkg:generic/waybill-fixture-*.sh@<sha[:12]>` PURLs, each with 1 sha256 hash + `waybill:pants-target=scripts:<name>` in properties[]; (c) SPDX 2.3 output contains 2 `packages[]` with matching `externalRefs[]` + `checksums[]`; (d) each script's SHA-256 in the CDX matches the actual file bytes (compute in-test via `sha2` crate).
- [X] T019 [US1] Implement `us1_glob_sources_expands_to_3_components` in `waybill-cli/tests/pants_shell_reader.rs`. Uses fixture from T016. Assert: 3 pants-shell components emitted, all carry `waybill:pants-target=helpers:utils`, all have distinct sha256 hashes (proves glob resolved to 3 files, not 1).
- [X] T020 [US1] Implement `us1_fr010_info_log_emits_all_six_structured_fields` in `waybill-cli/tests/pants_shell_reader.rs`. Uses fixture from T015. Subprocess with `RUST_LOG=info`; strip ANSI codes; assert stderr contains ALL SIX structured field names (`build_files_discovered=`, `build_files_parsed_ok=`, `build_files_skipped_corrupt=`, `shell_targets_found=`, `script_components_emitted=`, `tool_components_emitted=`).

### Implementation for User Story 1

- [X] T021 [US1] In `waybill-cli/src/scan_fs/package_db/pants_shell/mod.rs`, implement `fn discover_build_files(scan_root: &Path) -> Vec<PathBuf>` using `crate::scan_fs::walk::safe_walk` with a `WalkConfig::default()` and a visit closure that matches `path.file_name() == Some(OsStr::new("BUILD"))`. Returns absolute paths.
- [X] T022 [US1] In `waybill-cli/src/scan_fs/package_db/pants_shell/mod.rs`, implement `pub fn read(scan_root: &Path) -> Vec<PackageDbEntry>` per contracts §"Discovery + orchestration data flow":
  1. Discover BUILD files via T021's helper.
  2. If ZERO BUILD files AND no `pants.toml` at scan_root → return `Vec::new()` early (SC-003 byte-identity).
  3. For each BUILD file: read bytes; call `build_dsl::extract_targets`; for each Ok(decl) call `target_resolver::resolve_target`; for each resolved file call `component_emit::script_to_package_db_entry`. Increment counters per FR-010.
  4. Cross-file dedup pass (SC-006): group emitted entries by canonical `source_path`; when 2+ entries share a path, merge into one whose `waybill:pants-target` annotation is comma-sep lexically-sorted union.
  5. Read `pants.toml` if present, parse via `config::parse`; for each `[shellcheck]/[shfmt]/[shunit2]` section with `version` set, call `component_emit::tool_to_package_db_entry` and append.
  6. Emit FR-010 INFO log unless zero build files AND no pants.toml.
  7. Return combined `Vec<PackageDbEntry>`. Log module path: `waybill::scan_fs::package_db::pants_shell`.
- [X] T023 [US1] Wire the new reader into `waybill-cli/src/scan_fs/package_db/mod.rs::read_all()`. Add `out.extend(pants_shell::read(rootfs));` call after the existing `pants_jvm::read(rootfs)` call (m224). Follows the same "extend the aggregated result vector" pattern.

**Checkpoint**: Run `cargo +stable test -p waybill --test pants_shell_reader us1_`. Expect T018 + T019 + T020 all green. `waybill sbom scan --path waybill-cli/tests/fixtures/pants_shell/minimal_scripts/ --format cyclonedx-json --output /tmp/us1.cdx.json` produces the expected 2 components. **MVP shippable at this point.**

---

## Phase 5: User Story 2 — Pinned shell-tool inventory (Priority: P2)

**Goal**: `pants.toml` `[shellcheck]` / `[shfmt]` / `[shunit2]` `version` pins get emitted as design-tier `pkg:generic/*` components.

**Independent Test**: Run scan against `waybill-cli/tests/fixtures/pants_shell/with_shell_setup/`. Assert the CDX contains exactly 3 `pkg:generic/(shellcheck|shfmt|shunit2)@<version>` components plus the script component from the co-located BUILD file.

### Tests for User Story 2

- [X] T024 [P] [US2] Create synthetic fixture `waybill-cli/tests/fixtures/pants_shell/with_shell_setup/` per research.md §R5 fixture 3: `pants.toml` with all 3 subsystem sections (`[shellcheck] version = "v0.9.0"`, `[shfmt] version = "v3.7.0"`, `[shunit2] version = "2.1.8"`) + `scripts/BUILD` with one `shell_source` target + one `waybill-fixture-*.sh` file.
- [X] T025 [P] [US2] Add integration test `us2_pants_toml_pins_emit_design_tier_tool_components` to `waybill-cli/tests/pants_shell_reader.rs`. Uses fixture from T024. Assert: (a) 3 tool components with expected PURLs (`pkg:generic/shellcheck@v0.9.0`, `pkg:generic/shfmt@v3.7.0`, `pkg:generic/shunit2@2.1.8`); (b) each tool component has `waybill:sbom-tier=design` property AND `waybill:source-file=pants.toml`; (c) the script component from the same fixture is ALSO emitted (co-existence proof); (d) FR-010 log has `tool_components_emitted=3` AND `script_components_emitted=1`.
- [X] T026 [P] [US2] Add integration test `us2_no_version_key_emits_no_tool_component` to `waybill-cli/tests/pants_shell_reader.rs`. Uses a new sub-fixture `with_shell_setup_no_versions/pants.toml` containing `[shellcheck]` with no `version` key (only `known_versions = [...]`). Assert: zero tool components emitted; `tool_components_emitted=0` in FR-010 log. Regression guard for spec Acceptance Scenario 3.

### Implementation for User Story 2

- [X] T027 [US2] No new production code — US2 is entirely covered by T008 (tool_to_package_db_entry) + T022 step 5 (config parse + tool emission). Verify T025 + T026 pass. If T025 fails: check that `component_emit::tool_to_package_db_entry` runs for all 3 sections. If T026 fails: check that `config::parse` returns `None` for the missing-version tool section.

**Checkpoint**: T025 + T026 green. US2 done with zero new production LOC beyond US1's implementation.

---

## Phase 6: User Story 3 — shunit2 test scope tagging (Priority: P3)

**Goal**: `shunit2_test` / `shunit2_tests`-owned components carry `waybill:lifecycle-scope=development`; `shell_source` / `shell_sources`-owned components do not.

**Independent Test**: Run scan against `waybill-cli/tests/fixtures/pants_shell/shunit2_dev_scope/`. Assert shunit2-owned components tag as development, shell_source-owned components do not.

### Tests for User Story 3

- [X] T028 [P] [US3] Create synthetic fixture `waybill-cli/tests/fixtures/pants_shell/shunit2_dev_scope/` per research.md §R5 fixture 4: `tests/BUILD` with `shunit2_test(name="deploy-test", source="waybill-fixture-deploy-test.sh")` + `shunit2_tests(name="unit", sources=["*_test.sh"])` + `shell_source(name="fixture-setup", source="waybill-fixture-setup.sh")`. Three `.sh` files: `waybill-fixture-deploy-test.sh`, `waybill-fixture-x_test.sh` (matches the glob), `waybill-fixture-setup.sh` (the runtime one).
- [X] T029 [P] [US3] Add integration test `us3_shunit2_targets_tag_development_shell_source_targets_tag_runtime` to `waybill-cli/tests/pants_shell_reader.rs`. Uses fixture from T028. Assert: (a) 3 script components emitted; (b) the two shunit2-owned components (`waybill-fixture-deploy-test.sh` + `waybill-fixture-x_test.sh`) carry `waybill:lifecycle-scope=development`; (c) the shell_source-owned component (`waybill-fixture-setup.sh`) either has NO lifecycle-scope property OR has `runtime` explicitly — either is acceptable per the m179 Runtime-is-default convention.

### Implementation for User Story 3

- [X] T030 [US3] No new production code — US3 is entirely covered by T003 (`ShellTargetKind::lifecycle_scope()` classifier) + T007 (component_emit propagates it). Verify T029 passes. If it fails: check that `component_emit::script_to_package_db_entry` correctly sets `lifecycle_scope` from `kind.lifecycle_scope()`.

**Checkpoint**: T029 green.

---

## Phase 7: Edge cases + fail-open contracts (Cross-cutting)

**Purpose**: Cover FR-009 fail-open at file AND target grain + SC-005 malformed-BUILD partial parse + SC-006 multi-owner dedup + FR-011 zero-cost guarantee.

- [X] T031 [P] Create synthetic fixture `waybill-cli/tests/fixtures/pants_shell/missing_source_file/` per research.md §R5 fixture 5: `scripts/BUILD` declares `shell_source(name="broken", source="waybill-fixture-nonexistent.sh")` + one valid `shell_source(name="valid", source="waybill-fixture-real.sh")`; only the "valid" file exists on disk.
- [X] T032 [P] Add integration test `edge_missing_source_file_warns_and_continues` to `waybill-cli/tests/pants_shell_reader.rs`. Uses T031 fixture. Assert: exit 0; exactly 1 pants-shell component emitted (the valid one); stderr contains WARN naming the missing file's target address + path.
- [X] T033 [P] Create synthetic fixture `waybill-cli/tests/fixtures/pants_shell/malformed_build_partial/` per research.md §R5 fixture 6: `scripts/BUILD` contains 3 valid `shell_source` targets AND one syntactically-broken target (unclosed paren spanning multiple lines) + 3 corresponding valid `.sh` files.
- [X] T034 [P] Add integration test `edge_malformed_build_partial_emits_valid_targets` to `waybill-cli/tests/pants_shell_reader.rs`. Uses T033 fixture. Assert: exit 0 (SC-005 fail-open); 3 pants-shell components emitted (from the valid targets); WARN naming the broken target's line range; `build_files_parsed_ok=1` in FR-010 log (per-file counts as parsed_ok because at least one target extracted).
- [X] T035 [P] Create synthetic fixture `waybill-cli/tests/fixtures/pants_shell/dupe_target_owners/` per research.md §R5 fixture 7: `scripts/BUILD` has BOTH `shell_source(name="single", source="waybill-fixture-x.sh")` AND `shell_sources(name="glob", sources=["waybill-fixture-*.sh"])` matching the same file; only `waybill-fixture-x.sh` exists.
- [X] T036 [P] Add integration test `edge_dupe_target_owners_emit_one_component_with_merged_annotation` to `waybill-cli/tests/pants_shell_reader.rs`. Uses T035 fixture. Assert: exactly 1 pants-shell component emitted (SC-006 dedup); its `waybill:pants-target` property value is exactly `"scripts:glob,scripts:single"` (lex-sorted, comma-sep — verifies annotation-merge logic).
- [X] T037 [P] Add integration test `edge_no_pants_no_build_files_produces_no_reader_activity` to `waybill-cli/tests/pants_shell_reader.rs`. Uses `waybill-cli/tests/fixtures/pants_pex/minimal_python` (has 3rdparty/python but no BUILD files or pants.toml at the scan root). Assert: exit 0; zero pants-shell components; INFO log line `pants-shell reader complete` MUST NOT appear (reader returns early per FR-011 / SC-003).
- [X] T037a [P] Create synthetic fixture `waybill-cli/tests/fixtures/pants_shell/shell_command_ignored/` per finding C1 from `/speckit-analyze` (FR-012 regression guard): `scripts/BUILD` declares BOTH `shell_command(name="build", command="make all", tools=["make"], outputs=["target/binary"])` AND `shell_source(name="wrapper", source="waybill-fixture-wrapper.sh")`; one `waybill-fixture-wrapper.sh` file present. The `shell_command` target is legal Pants syntax but out-of-scope per FR-012.
- [X] T037b [P] Add integration test `edge_shell_command_targets_not_ingested` to `waybill-cli/tests/pants_shell_reader.rs`. Uses T037a fixture. Assert: exit 0; exactly ONE pants-shell component emitted (`waybill-fixture-wrapper.sh` from the `shell_source` target); zero components whose `waybill:pants-target` annotation contains `"scripts:build"` (the shell_command address). Regression guard for FR-012 (per finding C1 from `/speckit-analyze`).

---

## Phase 8: Docs + memory

- [X] T038 [P] Update `docs/ecosystems.md` — add `## pants (shell)` section covering: BUILD-file walker discovery, 4 recognized target types, PURL construction (`pkg:generic/<basename>@<sha[:12]>`), tool-pin discovery from `[shellcheck]/[shfmt]/[shunit2]`, multi-owner dedup contract, FR-010 log shape (6 fields), follow-ups. Cross-link to `specs/225-pants-shell-reader/quickstart.md`. Also add a row to the coverage-matrix table at the top of the file: `[pants (shell)](#pants-shell)` between the `[pants (JVM)]` and `[kotlin]` rows.
- [X] T039 [P] Update `README.md` supported-ecosystems table (add a row `**pants (shell)** *(225)*` between the `**pants (JVM)**` and `**vcpkg**` rows). Bump the "Thirteen production ecosystem readers" count to "Fourteen".
- [X] T040 [P] Add memory entry `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/reference_pants_shell_reader.md` documenting: module location (`scan_fs/package_db/pants_shell/`), the 4 recognized built-in target types, the regex-scoped DSL extraction rationale (no Python interpreter per Principle I), tool-pin schema (`[shellcheck]/[shfmt]/[shunit2] version = "..."`), C145 catalog row that shipped with this feature, cross-target dedup + annotation-merge semantics, follow-up milestones (`shell_command`, plugin-registered target types, nested pants.toml). Add corresponding line to `MEMORY.md` index.

---

## Phase 9: Pre-PR gate

- [X] T041 Run `./scripts/pre-pr.sh`. Confirm: (a) `cargo +stable clippy --workspace --all-targets` exit 0, zero warnings; (b) `cargo +stable test --workspace --no-fail-fast` — every suite reports `ok. N passed; 0 failed`. Report per-target counts per memory `feedback_prepr_gate_full_output`. Special attention: `pants_shell_reader` (11 integration tests expected: US1×3 + US2×2 + US3×1 + edge×5 — US1 T018/T019/T020; US2 T025/T026; US3 T029; edge T032/T034/T036/T037/T037b) + `parity::extractors::tests::every_catalog_row_has_an_extractor` + `holistic_parity` MUST pass (C145 gate).
- [X] T042 Verify no unintended goldens changed: `git status waybill-cli/tests/fixtures/` MUST show only additions under `pants_shell/` — no modifications to any existing golden. `git status waybill-cli/tests/` similarly — only additions. Any modification indicates a leaked side-effect on other readers.
- [X] T043 Run `cargo tree -p waybill --target x86_64-unknown-linux-gnu -e normal | grep -Ei 'openssl-sys|libz-sys|aws-lc-rs|aws-lc-sys|native-tls|mbedtls-sys|tough'` — expect zero output (Constitution Principle I regression guard).
- [X] T044 Locally walk `specs/225-pants-shell-reader/quickstart.md` §1 and §2 end-to-end against the largest fixture (`with_shell_setup` — 3 tool components + 1 script component). Confirm the FR-010 INFO log line appears with all 6 structured fields.

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (Phase 1)**: T001 → T002. Two sequential tasks; no parallelism possible.
- **Foundational (Phase 2)**: T003 must land first (types are consumed by every other task). T004 + T005 + T006 + T007 + T008 all `[P]` w.r.t. each other (5 different files) once T003 lands. All 5 must complete before US1 starts.
- **C145 parity work (Phase 3)**: T009 + T010 + T011 + T012 all `[P]` (4 different files). T013 depends on T010+T011+T012 (needs the extractor functions to exist to register them). T014 depends on T013 (runs the parity test). Phase 3 MUST complete before Phase 4's integration tests (which emit `waybill:pants-target` and trigger the parity gate).
- **US1 (Phase 4)**: T015 + T016 + T017 `[P]` (fixture creation + test scaffolding — different files). T021 → T022 → T023 sequential (each consumes the previous). Tests T018/T019/T020 depend on T023.
- **US2 (Phase 5)**: T024 + T025 + T026 all `[P]` (different files). T027 verification only.
- **US3 (Phase 6)**: T028 + T029 `[P]`. T030 verification only.
- **Phase 7**: T031/T033/T035/T037a fixtures `[P]`; T032/T034/T036/T037/T037b tests `[P]`. Pairing: T031↔T032, T033↔T034, T035↔T036, T037a↔T037b (each test depends on its paired fixture); T037 uses an external fixture (`pants_pex/minimal_python`) and pairs to nothing.
- **Phase 8**: T038 + T039 + T040 all `[P]`.
- **Phase 9**: T041 → T042 → T043 → T044 sequential.

### Story dependencies (visualized)

```text
Phase 1 (Setup) ──> Phase 2 (Foundational) ──> Phase 3 (C145 parity) ──> Phase 4 (US1 MVP) ──> Phase 7 (edge cases)
                                                                              │                     │
                                                                              ├──> Phase 5 (US2)    │
                                                                              └──> Phase 6 (US3)    │
                                                                                                    │
                                                                              Phase 8 (docs) <──────┤
                                                                                                    │
                                                                              Phase 9 (pre-PR) <────┘
```

### Parallel opportunities

- **Phase 2**: T004 + T005 + T006 + T007 in parallel (4 different files, all consume T003's types).
- **Phase 3**: T009 + T010 + T011 + T012 in parallel (4 different files).
- **Phase 4 setup half**: T015 + T016 + T017 in parallel (fixtures + skeleton).
- **Phase 7**: 4 fixture tasks in parallel + 5 test tasks in parallel (see pairing above; T037 is unpaired).
- **Phase 8**: all 3 tasks in parallel.

---

## Implementation Strategy

### MVP first (US1 only)

1. Complete Phase 1 (Setup — module skeleton + registration).
2. Complete Phase 2 (types + BUILD-DSL extractor + resolver + config + emit helpers).
3. Complete Phase 3 (C145 catalog row + 3 extractors + parity gate).
4. Complete Phase 4 (US1 — orchestrator + read_all wire + 3 integration tests).
5. **STOP + VALIDATE**: `cargo +stable test -p waybill --test pants_shell_reader us1_` should be all-green.
6. Deploy / demo MVP: `waybill sbom scan` now covers Pants shell scripts with SHA-256 fingerprints + target-address provenance.

### Incremental delivery after MVP

- Phase 5 (US2 tool inventory) — 1 fixture + 2 tests + verification.
- Phase 6 (US3 test-scope tagging) — 1 fixture + 1 test + verification.
- Phase 7 (edge-case coverage) — hardens SC-005 fail-open + SC-006 dedup + FR-011 zero-cost.
- Phase 8 (docs polish).
- Phase 9 (pre-PR gate).

Estimated total effort: **~1.5 focused work-days** (slightly larger than m224 due to the C145 parity work + BUILD-DSL parser which is more complex than m224's coord-string parser).

---

## Notes

- **One new parity-catalog row (C145).** Adding it + 3 extractor entries is unavoidable per memory `feedback_sbom_format_mapping_extractor_gate` — `waybill:pants-target` is a genuinely new concept vs m223's C143 `waybill:pants-resolve`.
- All script fixtures use synthetic `waybill-fixture-*.sh` names per memory `feedback_fixture_synthetic_package_names`. Never real filenames like `deploy.sh` that could match Advisory-DB heuristics.
- `[P]` tasks touch different files with no ordering dependency.
- Every US phase is independently shippable AFTER US1 (US2/US3 are additive refinements).
- The BUILD-DSL regex extractor is the single most fragile piece; T004's 12 unit tests are the primary safety net. Any regex bug should surface in T004 before the integration tests in Phase 4 run.
- Estimated production LOC: ~450 total (T003 ≈ 80; T004 ≈ 120; T005 ≈ 80; T006 ≈ 50; T007+T008 ≈ 120). Test LOC: ~400. Fixture LOC (BUILD + .sh + pants.toml): ~200. Parity work: ~40 LOC (1 row + 3 extractor lines). **Comparable to m224 in total, but with a different distribution (bigger extractor + parity budget, smaller emit helper because scripts have simpler fields than Maven coords).**
