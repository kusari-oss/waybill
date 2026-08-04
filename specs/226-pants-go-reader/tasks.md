---
description: "Task list for feature 226 (Pants Go reader)"
---

# Tasks: Pants Go reader

**Input**: Design documents from `/specs/226-pants-go-reader/`
**Prerequisites**: plan.md ✅, spec.md ✅ (3 user stories, 12 FRs, 6 SCs), research.md ✅ (5 items), data-model.md ✅ (4 module-private types + 2 config helpers + typed error enum), contracts/go-build-dsl-schema.md ✅, contracts/c145-broadening.md ✅, quickstart.md ✅

**Tests**: Tests ARE included — every reader shipped since m002 has test coverage per Constitution Principle VII, and the regex BUILD-DSL extractor + go_mod ownership inference + enrichment pass introduce failure modes that only tests can audit.

**Organization**: Tasks grouped by user story. Follows m225's shape closely; **NO parity-work phase** because C145 is broadened via a doc-only description update (per contracts/c145-broadening.md) with zero code changes to extractors.

## Format: `[TaskID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3
- File paths absolute or repo-relative from `/Users/mlieberman/Projects/mikebom`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Prepare the module directory + register it.

- [X] T001 Create module directory `waybill-cli/src/scan_fs/package_db/pants_go/` with 5 empty stub files (`mod.rs`, `build_dsl.rs`, `ownership_index.rs`, `config.rs`, `enrichment.rs`), each carrying only a `//! Milestone 226: <purpose>` doc-comment.
- [X] T002 Register the new module: add `pub mod pants_go;` to `waybill-cli/src/scan_fs/package_db/mod.rs` alphabetically (between `pants` and `pants_jvm`). Verify with `cargo +stable build -p waybill --bin waybill` — should compile clean (module contents do nothing yet).

**Checkpoint**: Empty pants_go module registered. Compile clean.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Define types + regex BUILD-DSL extractor for Go targets + ownership-index builder + `pants.toml` `[golang]` config parser. All three user stories depend on these.

**⚠️ CRITICAL**: US1/US2/US3 all depend on these types + helpers.

- [X] T003 In `waybill-cli/src/scan_fs/package_db/pants_go/mod.rs`, add module-private types `GoTargetKind` (closed enum with 4 variants per data-model.md §"GoTargetKind"), `GoTargetDeclaration { kind, name, import_path, main, start_line }`, `TargetAddress(String)` newtype with `Display` + `Ord`, `GoOwnershipIndex { go_mod_roots: BTreeMap<PathBuf, TargetAddress>, import_path_to_addresses: HashMap<String, Vec<TargetAddress>>, main_targets: Vec<(PathBuf, TargetAddress)>, package_targets: Vec<(PathBuf, TargetAddress)> }`, and `GoTargetParseError` (thiserror enum with 3 variants per data-model.md). Also declare the sub-modules: `mod build_dsl; mod ownership_index; mod config; mod enrichment;`.
- [X] T004 [P] In `waybill-cli/src/scan_fs/package_db/pants_go/build_dsl.rs`, add `pub(crate) fn extract_targets(bytes: &[u8]) -> Vec<Result<GoTargetDeclaration, GoTargetParseError>>` per research.md §R2 (reuse m225 pants_shell/build_dsl.rs's hybrid anchoring-regex + char-scan pattern). Two regex patterns: single-source shape (matches `go_mod` + `go_third_party_package` + `go_binary` + `go_package`), then per-kwarg extraction for `name=`, `import_path=`, `main=`. Add 12 unit tests: valid `go_mod`, valid `go_mod` default-name (no `name=`), valid `go_third_party_package` with both kwargs, valid `go_binary(main=".")`, valid `go_binary(main="./cmd/foo")`, valid `go_package` default-name, `go_third_party_package` missing `import_path=` (returns `MissingRequiredKwarg`), variable-reference `import_path=IMPORT_VAR` (returns `NonStringLiteralValue`), unbalanced-parens (returns `UnbalancedParens`), 3 valid targets in one BUILD blob (all 3 parse), comment-line inside target body (ignored), zero recognized targets (returns empty vec).
- [X] T005 [P] In `waybill-cli/src/scan_fs/package_db/pants_go/ownership_index.rs`, add `pub(crate) fn build_index(decls: &[(PathBuf /*build_file*/, GoTargetDeclaration)], scan_root: &Path) -> GoOwnershipIndex`. For each declaration: compute the target address per m225's `compute_address` convention (BUILD dir relative to scan root + name kwarg with defaults per R1); route into the appropriate index bucket per data-model.md §"GoOwnershipIndex" (`go_mod` → `go_mod_roots`; `go_third_party_package` → `import_path_to_addresses`; `go_binary` → `main_targets` with `main=` resolved to absolute path via `<build_dir>/<normalized_main>`; `go_package` → `package_targets` with the BUILD file's own directory as the package dir). Add 8 unit tests: single go_mod at 3rdparty/go, multi-go_mod deep + shallow (longest-prefix determinism), go_third_party_package with distinct import_paths, go_binary with `main="."`, go_binary with `main="./cmd/foo"`, go_binary with absolute-path main (WARN + skip), go_package address computation with default name, empty declarations list returns empty index.
- [X] T006 [P] In `waybill-cli/src/scan_fs/package_db/pants_go/config.rs`, add module-private `GoSetupConfig { golang: Option<GolangSection> }` + `GolangSection { expected_version: Option<String> }` Deserialize types per data-model.md §"GoSetupConfig". Add `pub(crate) fn parse(bytes: &[u8]) -> Option<GoSetupConfig>` returning `None` on any `toml::from_str` error. Add 4 unit tests: `[golang] expected_version = "1.21"`, `[golang]` present but no `expected_version`, missing `[golang]` section entirely, malformed TOML.
- [X] T007 [P] In `waybill-cli/src/scan_fs/package_db/pants_go/enrichment.rs`, add `pub(crate) fn collect_addresses_for_component(component: &ResolvedComponent, index: &GoOwnershipIndex) -> Vec<TargetAddress>`. Implements the R3+R4 matching algorithm: (a) `go_mod` longest-prefix match on `source_path`; (b) `import_path_to_addresses` direct lookup — **PURL→import_path reconstruction is `namespace().map(|n| format!("{n}/{}", name())).unwrap_or_else(|| name().to_string())` — join `pkg:golang/<namespace>/<name>@<v>` back into the Go module path (e.g., `pkg:golang/github.com/spf13/cobra@v1.6.0` → `"github.com/spf13/cobra"`); single-segment module paths without namespace are handled degenerately but are exceedingly rare in real Go modules** (per finding I1 from `/speckit-analyze`); (c) main-module role check + `main_targets` path match on `source_path.parent()`; (d) main-module role check + `package_targets` `starts_with` match. Returns lex-sorted deduped vec. Add 6 unit tests: single go_mod match returns 1 address, no match returns empty, multi-owner merge is sorted+deduped, main-module + go_binary(main=".") matches, main-module + go_package matches, third-party import_path direct match trumps go_mod-only.

**Checkpoint**: Types + BUILD-DSL extractor + ownership-index builder + config parser + enrichment helper all compile with green unit tests. Ready for the two entry-point wire-ups (US1 enrichment + US2 tool-pin emission).

---

## Phase 3: User Story 1 — Attach Pants target ownership to Go modules (Priority: P1) 🎯 MVP

**Goal**: Post-`read_all` enrichment pass walks BUILD files, builds ownership index, and injects `waybill:pants-target` annotations onto every matching `pkg:golang/*` component. Zero fabrication.

**Independent Test**: Run scan against `waybill-cli/tests/fixtures/pants_go/minimal_3rdparty_go/`. Assert 3 `pkg:golang/*` components each carry `waybill:pants-target=3rdparty/go:mod`.

### Tests for User Story 1

- [X] T008 [P] [US1] Create synthetic fixture `waybill-cli/tests/fixtures/pants_go/minimal_3rdparty_go/` per research.md §R5-analog: `3rdparty/go/BUILD` with `go_mod(name="mod")`, `3rdparty/go/go.mod` naming module `github.com/waybill-fixture/root`, `3rdparty/go/go.sum` with 3 third-party entries (use synthetic module names `github.com/waybill-fixture/foo v1.0.0`, `github.com/waybill-fixture/bar v2.1.3`, `github.com/waybill-fixture/baz v0.9.0` with plausible sha1-shaped h1: prefixes per go.sum format). Include a comment-line inside the BUILD file to exercise T004's comment-tolerance path.
- [X] T009 [P] [US1] Create synthetic fixture `waybill-cli/tests/fixtures/pants_go/explicit_third_party_targets/` per US1 scenario 2: `3rdparty/go/BUILD` declaring BOTH `go_mod(name="mod")` AND `go_third_party_package(name="foo", import_path="github.com/waybill-fixture/foo")`; `3rdparty/go/go.mod` + `3rdparty/go/go.sum` with a matching `github.com/waybill-fixture/foo v1.0.0` entry.
- [X] T010 [P] [US1] Create integration test file `waybill-cli/tests/pants_go_reader.rs` with 4 initial `#[test]` functions (bodies filled in T011-T014):
  - `us1_minimal_3rdparty_go_annotates_all_three_components`
  - `us1_explicit_third_party_target_merges_with_go_mod`
  - `us1_fr010_info_log_emits_all_six_structured_fields`
  - `us1_zero_fabrication_component_count_unchanged`
  Import helpers `bin()`, `run_scan()`, `read_cdx()`, `get_property()`, `strip_ansi()` mirroring `waybill-cli/tests/pants_shell_reader.rs` verbatim.
- [X] T011 [US1] Implement `us1_minimal_3rdparty_go_annotates_all_three_components` in `waybill-cli/tests/pants_go_reader.rs`. Uses fixture from T008. **Emits BOTH formats in one scan invocation** (`--format cyclonedx-json --format spdx-2.3-json --output <fmt>=<path>`) per SC-001. Assert: (a) exit 0; (b) CDX contains 3 `pkg:golang/github.com/waybill-fixture/*` components; (c) each has `waybill:pants-target=3rdparty/go:mod` property; (d) SPDX 2.3 output contains 3 packages with matching externalRefs; (e) SPDX 2.3 packages carry the same annotation via the m080 envelope shape (verify by parsing the `annotations[]` entries).
- [X] T012 [US1] Implement `us1_explicit_third_party_target_merges_with_go_mod` in `waybill-cli/tests/pants_go_reader.rs`. Uses fixture from T009. Assert: the `pkg:golang/github.com/waybill-fixture/foo@v1.0.0` component's `waybill:pants-target` value is exactly `"3rdparty/go:foo,3rdparty/go:mod"` (lex-sorted, comma-sep — SC-004 gate).
- [X] T013 [US1] Implement `us1_fr010_info_log_emits_all_six_structured_fields` in `waybill-cli/tests/pants_go_reader.rs`. Uses fixture from T008. Subprocess with `RUST_LOG=info`; strip ANSI codes; assert stderr contains ALL SIX structured field names: `build_files_discovered=`, `build_files_parsed_ok=`, `build_files_skipped_corrupt=`, `go_targets_found=`, `components_annotated=`, `toolchain_component_emitted=`.
- [X] T014 [US1] Implement `us1_zero_fabrication_component_count_unchanged` in `waybill-cli/tests/pants_go_reader.rs`. Uses fixture from T008. Runs waybill TWICE: (a) once with the fixture as-is (Pants BUILD file present) → count `pkg:golang/*` components; (b) once with the BUILD file temporarily renamed to `_BUILD_DISABLED` (via std::fs::rename in a tempdir copy) → count again. Assert both counts are IDENTICAL. Proves FR-012 / Principle IX: enrichment adds annotations only, never fabricates components.

### Implementation for User Story 1

- [X] T015 [US1] In `waybill-cli/src/scan_fs/package_db/pants_go/mod.rs`, implement `fn discover_build_files(scan_root: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf>` using `safe_walk` with `WalkConfig { max_depth: 32, should_skip: no-op, exclude_set }`. Copy the `path.file_name() == Some(OsStr::new("BUILD"))` matcher verbatim from m225's pants_shell (`discover_build_files` at pants_shell/mod.rs).
- [X] T016 [US1] In `waybill-cli/src/scan_fs/package_db/pants_go/mod.rs`, implement `pub fn enrich(scan_root: &Path, exclude_set: &ExclusionSet, components: &mut Vec<ResolvedComponent>)`. Steps: (1) discover BUILD files via T015; (2) early-return silently if zero BUILD files AND no `pants.toml` at scan_root; (3) for each BUILD file, read bytes, call `build_dsl::extract_targets`, accumulate `(build_file, GoTargetDeclaration)` pairs plus a `build_files_parsed_ok` / `build_files_skipped_corrupt` counter split (parsed_ok if at least one target parsed OR the file has zero recognized targets); (4) call `ownership_index::build_index` on the accumulated decls; (5) iterate `components.iter_mut()`, for each `pkg:golang/*` call `enrichment::collect_addresses_for_component`, if non-empty inject `waybill:pants-target` annotation via `extra_annotations.insert`; (6) FR-012 diagnostic: for each `import_path_to_addresses` key, check if any component's `pkg:golang/<import_path>@*` matched — if not, emit an INFO log naming the orphan import path; (7) emit FR-010 INFO summary log. Log module path: `waybill::scan_fs::package_db::pants_go`.
- [X] T017 [US1] In `waybill-cli/src/scan_fs/mod.rs`, add the enrichment call site immediately after `reconcile_design_source_tiers` at line ~1001 (before m148 canonicalization). Signature: `crate::scan_fs::package_db::pants_go::enrich(rootfs, exclude_set, &mut components);`. Add a "Milestone 226:" comment header explaining the placement rationale (after m191 reconciler so the component set is final; before m148 so any annotations flow through canonicalization). **Implementation-time verification per finding U2 from `/speckit-analyze`**: confirm m148 canonicalization at `scan_fs/mod.rs` preserves the `extra_annotations` map when it merges duplicate-PURL entries. If m148 drops annotations on collapsed duplicates, either (a) move the enrichment call to AFTER m148, or (b) file a small fix in the m148 pass to union the annotation maps of merged entries. Verify quickly with a grep for `extra_annotations` in the m148 code path before implementing T017.

**Checkpoint**: Run `cargo +stable test -p waybill --test pants_go_reader us1_`. Expect T011 + T012 + T013 + T014 all green. `waybill sbom scan --path waybill-cli/tests/fixtures/pants_go/minimal_3rdparty_go/ --format cyclonedx-json --output /tmp/us1.cdx.json` produces the expected 3 annotated components. **MVP shippable at this point.**

---

## Phase 4: User Story 2 — Inventory the pinned Go toolchain (Priority: P2)

**Goal**: `pants.toml` `[golang] expected_version` pins get emitted as design-tier `pkg:generic/go@<version>` components mirroring m225's shellcheck/shfmt pattern.

**Independent Test**: Run scan against `waybill-cli/tests/fixtures/pants_go/with_toolchain_pin/`. Assert the CDX contains one `pkg:generic/go@1.21` component with `waybill:sbom-tier=design` + `waybill:source-file=pants.toml`.

### Tests for User Story 2

- [X] T018 [P] [US2] Create synthetic fixture `waybill-cli/tests/fixtures/pants_go/with_toolchain_pin/`: `pants.toml` with `[golang] expected_version = "1.21"`, plus `3rdparty/go/BUILD` with `go_mod(name="mod")` + `3rdparty/go/go.sum` with 1 synthetic entry (proves US1+US2 co-existence).
- [X] T019 [P] [US2] Add integration test `us2_pants_toml_expected_version_emits_design_tier_toolchain_component` to `waybill-cli/tests/pants_go_reader.rs`. Uses fixture from T018. Assert: (a) 1 `pkg:generic/go@1.21` component; (b) that component has `waybill:sbom-tier=design` + `waybill:source-file=pants.toml`; (c) the US1 script component from the co-located fixture is ALSO emitted with `waybill:pants-target=3rdparty/go:mod`; (d) FR-010 log shows `toolchain_component_emitted=1`.
- [X] T020 [P] [US2] Add integration test `us2_no_expected_version_emits_no_toolchain_component` to `waybill-cli/tests/pants_go_reader.rs`. Uses a new sub-fixture `with_toolchain_pin_no_version/pants.toml` containing `[golang]` with only `min_dot_version = "1.21"` (NO `expected_version`). Assert: zero `pkg:generic/go@*` components emitted; `toolchain_component_emitted=0` in FR-010 log. Regression guard for spec Acceptance Scenario 3.

### Implementation for User Story 2

- [X] T021 [US2] In `waybill-cli/src/scan_fs/package_db/pants_go/mod.rs`, implement `pub fn read(scan_root: &Path, exclude_set: &ExclusionSet) -> Vec<PackageDbEntry>`. Steps: (1) read `pants.toml` at scan_root; (2) parse via `config::parse`; (3) if `[golang].expected_version` is non-empty string, emit one `PackageDbEntry` per contracts/go-build-dsl-schema.md §"Output contract A" (verbatim version, sbom_tier=design, lifecycle_scope=Development, empty hashes, `waybill:source-file=pants.toml` annotation via json! macro); (4) return the resulting Vec (0 or 1 element). Reuse `component_emit::tool_to_package_db_entry` from m225's pants_shell IF it's already `pub(crate)`; otherwise inline a private helper with the same shape. Note the `exclude_set` parameter is unused here (kept for API symmetry with `enrich`).
- [X] T022 [US2] Wire the new reader into `waybill-cli/src/scan_fs/package_db/mod.rs::read_all()`. Add `out.extend(pants_go::read(rootfs, exclude_set));` call after the existing `pants_shell::read(rootfs, exclude_set)` call (m225). Follows the same "extend the aggregated result vector" pattern.

**Checkpoint**: T019 + T020 green. US2 done with ~30 LOC production code beyond US1's implementation.

---

## Phase 5: User Story 3 — Distinguish first-party from third-party packages (Priority: P3)

**Goal**: `go_binary` / `go_package` targets attribute the main-module component; `go_mod` / `go_third_party_package` targets attribute third-party components. Operators can filter with a simple jq query on the annotation prefix.

**Independent Test**: Run scan against `waybill-cli/tests/fixtures/pants_go/go_binary_first_party/`. Assert the main-module component carries `cmd/frontend:*` addresses while third-party components carry `3rdparty/go:*` addresses.

### Tests for User Story 3

- [X] T023 [P] [US3] Create synthetic fixture `waybill-cli/tests/fixtures/pants_go/go_binary_first_party/`: (a) root-level `go.mod` naming module `github.com/waybill-fixture/frontend`; (b) root-level `go.sum` with 2 synthetic third-party entries; (c) `3rdparty/go/BUILD` declaring `go_mod(name="mod")` (owns the go.sum entries); (d) `cmd/frontend/BUILD` declaring `go_binary(name="frontend", main=".")` + `go_package(name="pkg")`; (e) `cmd/frontend/main.go` (empty scaffold — waybill doesn't parse Go source, just needs the file to exist for the main-module component's source_path to be plausible). Ensure the root go.mod's module path (`github.com/waybill-fixture/frontend`) means waybill's existing Go reader emits a main-module `pkg:golang/*` component.
- [X] T024 [P] [US3] Add integration test `us3_first_party_and_third_party_annotations_differ` to `waybill-cli/tests/pants_go_reader.rs`. Uses fixture from T023. Assert: (a) at least 1 main-module component with `waybill:component-role=main-module` present in CDX; (b) that main-module component's `waybill:pants-target` value contains `cmd/frontend:frontend` AND/OR `cmd/frontend:pkg` (per FR-006, both should apply); (c) 2 third-party `pkg:golang/*` components carry `waybill:pants-target=3rdparty/go:mod`; (d) no third-party component carries a `cmd/frontend:*` address.

### Implementation for User Story 3

- [X] T025 [US3] No new production code — US3 is entirely covered by T007 (`collect_addresses_for_component` per R3+R4) + T016 (orchestrator invocation). Verify T024 passes. If it fails: check that (a) `enrichment::collect_addresses_for_component` correctly identifies `waybill:component-role=main-module` and applies the R4 algorithm; (b) `ownership_index::build_index` correctly normalizes `main="."` → BUILD file's own dir; (c) `starts_with` matches on `source_path.parent()` for both main_targets and package_targets.

**Checkpoint**: T024 green.

---

## Phase 6: Edge cases + zero-fabrication contract (Cross-cutting)

**Purpose**: Cover FR-009 per-file/per-target fail-open + FR-012 zero-fabrication invariant + missing-import-path INFO diagnostic + FR-011 byte-identity guarantee.

- [X] T026 [P] Create synthetic fixture `waybill-cli/tests/fixtures/pants_go/missing_import_path/`: `3rdparty/go/BUILD` declares `go_mod(name="mod")` + `go_third_party_package(name="missing", import_path="github.com/waybill-fixture/does-not-exist")`; `3rdparty/go/go.sum` contains 1 valid entry for `github.com/waybill-fixture/foo` but NOT for the `does-not-exist` module.
- [X] T027 [P] Add integration test `edge_missing_import_path_no_fabrication_info_log` to `waybill-cli/tests/pants_go_reader.rs`. Uses T026 fixture. Assert: (a) exit 0; (b) exactly 1 `pkg:golang/*` component (for the valid `foo` entry — no synthetic component fabricated for `does-not-exist`); (c) the `foo` component carries `waybill:pants-target` value that includes `3rdparty/go:mod` (implicit ownership); (d) stderr contains INFO log naming `github.com/waybill-fixture/does-not-exist` as an orphan import path. Regression guard for FR-012 / SC-006.
- [X] T028 [P] Create synthetic fixture `waybill-cli/tests/fixtures/pants_go/malformed_build_partial/`: `3rdparty/go/BUILD` containing `go_mod(name="mod")` + `go_third_party_package(name="one", import_path="github.com/waybill-fixture/one")` + `go_third_party_package(name="two", import_path="github.com/waybill-fixture/two")` + one syntactically-broken target (unclosed paren, e.g. `go_third_party_package(name="broken", import_path=` spanning EOF). Plus `3rdparty/go/go.sum` with matching entries for `one` and `two` (but not `broken`).
- [X] T029 [P] Add integration test `edge_malformed_build_partial_enriches_valid_targets` to `waybill-cli/tests/pants_go_reader.rs`. Uses T028 fixture. Assert: (a) exit 0 (SC-005 fail-open); (b) 2 `pkg:golang/*` components (`one` + `two`) both carry `waybill:pants-target` containing at minimum `3rdparty/go:mod`, AND the `one` component's annotation includes `3rdparty/go:one`, the `two` includes `3rdparty/go:two`; (c) WARN log names the broken target's line range; (d) `build_files_parsed_ok=1` in FR-010 log (per-file counts as parsed_ok because valid targets extracted).
- [X] T030 [P] Add integration test `edge_no_pants_no_build_files_produces_no_enrichment` to `waybill-cli/tests/pants_go_reader.rs`. Uses ANY existing non-Pants fixture with go.sum (reuse `waybill-cli/tests/fixtures/go/simple-module` if it exists in-tree; otherwise a fresh minimal Go-only fixture). Assert: (a) exit 0; (b) zero `pkg:golang/*` components carry `waybill:pants-target` (nothing to enrich); (c) INFO log `pants-go enrichment complete` MUST NOT appear (early-return per FR-011 / SC-003). Byte-identity regression guard.
- [X] T031 [P] Add integration test `edge_zero_fabrication_byte_identity` to `waybill-cli/tests/pants_go_reader.rs`. Uses T028 fixture (which has 1 `go_mod` + 2 valid `go_third_party_package` + 1 broken + 2 matching go.sum entries). Assert: the `pkg:golang/*` component count from the CDX is EXACTLY 2 (matching the 2 go.sum entries; no synthetic component for the broken target and no synthetic component if any `go_third_party_package` names an import path with no go.sum entry). Reinforces FR-012 with a hard count assertion. (Fixture-count phrasing tightened per finding I2 from `/speckit-analyze`.)
- [X] T031a [P] Create synthetic fixture `waybill-cli/tests/fixtures/pants_go/multi_go_mod_layout/` per finding C1 from `/speckit-analyze` (multi-`go_mod` regression guard): (a) `3rdparty/go/BUILD` with `go_mod(name="root")`, `3rdparty/go/go.mod` + `3rdparty/go/go.sum` with 1 synthetic entry `github.com/waybill-fixture/root-dep v1.0.0`; (b) `services/api/3rdparty/go/BUILD` with `go_mod(name="api")`, `services/api/3rdparty/go/go.mod` + `services/api/3rdparty/go/go.sum` with 1 synthetic entry `github.com/waybill-fixture/api-dep v2.0.0`. Two independent `go_mod` roots at different depths; existing Go reader emits 2 `pkg:golang/*` components (one per go.sum).
- [X] T031b [P] Add integration test `edge_multi_go_mod_deepest_prefix_wins` to `waybill-cli/tests/pants_go_reader.rs`. Uses T031a fixture. Assert: (a) exit 0; (b) 2 `pkg:golang/*` components; (c) `pkg:golang/github.com/waybill-fixture/root-dep@v1.0.0` carries `waybill:pants-target=3rdparty/go:root` (NOT `services/api/3rdparty/go:api` — its source_path is under `3rdparty/go/`); (d) `pkg:golang/github.com/waybill-fixture/api-dep@v2.0.0` carries `waybill:pants-target=services/api/3rdparty/go:api` (deepest-prefix wins over the shallower `3rdparty/go:root` per R3). Regression guard for the multi-module Go workspace case called out in spec Edge Cases.

---

## Phase 7: C145 doc broadening + docs + memory

**Purpose**: Doc-only update to `docs/reference/sbom-format-mapping.md` C145 row per contracts/c145-broadening.md. No code changes. Plus ecosystems.md + README.md + memory entry updates.

- [X] T032 [P] Update `docs/reference/sbom-format-mapping.md` row C145 per `specs/226-pants-go-reader/contracts/c145-broadening.md` §"Description-update wording". Append the specified paragraph after the existing C145 description. Do NOT touch the row_id, annotation key, value regex, extractor triple, or KEEP-NO-NATIVE disposition. Verify `parity::extractors::tests::every_catalog_row_has_an_extractor` still passes locally (the row_id count is unchanged).
- [X] T033 [P] Update `docs/ecosystems.md` — add `## pants (Go)` section covering: BUILD-file walker discovery, 4 recognized target types, R3 ownership-by-longest-prefix, R4 main-module attribution via `waybill:component-role`, `pants.toml` `[golang] expected_version` toolchain-pin, zero-fabrication contract (FR-012), FR-010 log shape (6 fields), follow-ups. Cross-link to `specs/226-pants-go-reader/quickstart.md`. Also add a row to the coverage-matrix table at the top of the file: `[pants (Go)](#pants-go)` between the `[pants (shell)]` and `[kotlin]` rows.
- [X] T034 [P] Update `README.md` supported-ecosystems table — add a row `**pants (Go)** *(226)*` between the `**pants (shell)**` and `**vcpkg**` rows. Bump the "Fourteen production ecosystem readers" count to "Fifteen".
- [X] T035 [P] Add memory entry `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/reference_pants_go_reader.md` documenting: module location (`scan_fs/package_db/pants_go/`), the 4 recognized built-in target types, the regex-scoped DSL extraction rationale (reuse of m225 pattern), tool-pin schema (`[golang] expected_version` in `pants.toml`), C145 broadening (doc-only, zero code churn), enrichment-vs-emit split (enrich() runs post-`read_all` at `scan_fs/mod.rs:1001`), zero-fabrication invariant (FR-012 / Principle IX — never fabricates pkg:golang/* components), R3 longest-prefix ownership algorithm, R4 main-module attribution, follow-up milestones (go_source/go_test, min_dot_version, plugin-registered target types). Add corresponding line to `MEMORY.md` index.

---

## Phase 8: Pre-PR gate

- [X] T036 Run `./scripts/pre-pr.sh`. Confirm: (a) `cargo +stable clippy --workspace --all-targets` exit 0, zero warnings; (b) `cargo +stable test --workspace --no-fail-fast` — every suite reports `ok. N passed; 0 failed`. Report per-target counts per memory `feedback_prepr_gate_full_output`. Special attention: `pants_go_reader` (12 integration tests expected: US1×4 + US2×2 + US3×1 + edge×5 — US1 T011/T012/T013/T014; US2 T019/T020; US3 T024; edge T027/T029/T030/T031/T031b) + `parity::extractors::tests::every_catalog_row_has_an_extractor` + `holistic_parity` MUST pass (C145 broadening did not touch the extractor triple, so both should stay green).
- [X] T037 Verify no unintended goldens changed: `git status waybill-cli/tests/fixtures/` MUST show only additions under `pants_go/` — no modifications to any existing golden. `git status waybill-cli/tests/` similarly — only additions (plus `pants_go_reader.rs`). Any modification indicates a leaked side-effect on other readers.
- [X] T038 Run `cargo tree -p waybill --target x86_64-unknown-linux-gnu -e normal | grep -Ei 'openssl-sys|libz-sys|aws-lc-rs|aws-lc-sys|native-tls|mbedtls-sys|tough'` — expect zero output (Constitution Principle I regression guard).
- [X] T039 Reproduce CI's walker-audit gate locally (per memory `feedback_walker_audit_local_check`): run the exact grep+diff script from `.github/workflows/ci.yml` §"Walker-audit allow-list check" — expect "OK" not "STILL DIFFERS". This gate is easy to trip if we accidentally introduce a `fn walk_*` helper in `pants_go/*.rs`; using `safe_walk` from the start avoids it (same as m225 pants_shell post-fix).
- [X] T040 Locally walk `specs/226-pants-go-reader/quickstart.md` §1 and §2 end-to-end against the T018 fixture (`with_toolchain_pin` — 1 script component + 1 toolchain component). Confirm the FR-010 INFO log line appears with all 6 structured fields including `toolchain_component_emitted=1`.

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (Phase 1)**: T001 → T002. Two sequential tasks; no parallelism possible.
- **Foundational (Phase 2)**: T003 must land first (types are consumed by every other task). T004 + T005 + T006 + T007 all `[P]` w.r.t. each other (4 different files) once T003 lands. All 4 must complete before US1 starts.
- **US1 (Phase 3)**: T008 + T009 + T010 `[P]` (fixtures + test scaffolding — different files). T015 → T016 → T017 sequential (each consumes the previous — T016 uses T015's `discover_build_files`; T017 wires T016 into `scan_fs/mod.rs`). Tests T011/T012/T013/T014 depend on T017.
- **US2 (Phase 4)**: T018 + T019 + T020 all `[P]` (different files). T021 → T022 sequential (T022 wires T021 into `read_all`). T019/T020 depend on T022.
- **US3 (Phase 5)**: T023 + T024 `[P]`. T025 verification only.
- **Phase 6**: T026/T028/T031a fixtures `[P]`; T027/T029/T030/T031/T031b tests `[P]` (each depends on its fixture — T030 uses a pre-existing fixture, T031 reuses T028, T031b uses T031a).
- **Phase 7**: T032 + T033 + T034 + T035 all `[P]` (4 different files).
- **Phase 8**: T036 → T037 → T038 → T039 → T040 sequential.

### Story dependencies (visualized)

```text
Phase 1 (Setup) ──> Phase 2 (Foundational) ──> Phase 3 (US1 MVP) ──> Phase 6 (edge cases)
                                                    │                     │
                                                    ├──> Phase 4 (US2)    │
                                                    └──> Phase 5 (US3)    │
                                                                          │
                                                     Phase 7 (docs+C145 broad) <────┤
                                                                          │
                                                     Phase 8 (pre-PR) <───┘
```

### Parallel opportunities

- **Phase 2**: T004 + T005 + T006 + T007 in parallel (4 different files, all consume T003's types).
- **Phase 3 setup half**: T008 + T009 + T010 in parallel (fixtures + skeleton).
- **Phase 6**: 3 fixture tasks in parallel + 5 test tasks in parallel (each test depends on its fixture; T030 uses a pre-existing fixture and pairs to nothing).
- **Phase 7**: all 4 tasks in parallel.

---

## Implementation Strategy

### MVP first (US1 only)

1. Complete Phase 1 (Setup — module skeleton + registration).
2. Complete Phase 2 (types + BUILD-DSL extractor + ownership index + config + enrichment helper).
3. Complete Phase 3 (US1 — orchestrator + enrichment wire + 4 integration tests).
4. **STOP + VALIDATE**: `cargo +stable test -p waybill --test pants_go_reader us1_` should be all-green.
5. Deploy / demo MVP: `waybill sbom scan` now attaches Pants target attribution to `pkg:golang/*` components on Pants Go monorepos.

### Incremental delivery after MVP

- Phase 4 (US2 toolchain-pin inventory) — 1 fixture + 2 tests + orchestrator wire.
- Phase 5 (US3 first-vs-third-party discrimination) — 1 fixture + 1 test + verification.
- Phase 6 (edge-case coverage) — hardens FR-009 fail-open + FR-011 byte-identity + FR-012 zero-fabrication.
- Phase 7 (C145 broadening + docs + memory).
- Phase 8 (pre-PR gate).

Estimated total effort: **~1 focused work-day** (~30% smaller than m225 because zero new parity work + enrichment-pass model reuses existing infrastructure).

---

## Notes

- **Zero new parity-catalog rows.** C145 `waybill:pants-target` is broadened via a doc-only description update per contracts/c145-broadening.md — no changes to extractors or `EXTRACTORS` array. This is the single biggest simplification vs m225 (which shipped a NEW C145 row).
- **Zero fabrication of `pkg:golang/*` components** (FR-012 / Principle IX). The enrichment pass NEVER pushes new components; it only mutates `extra_annotations` on existing ones. T014 + T031 are hard regression tests for this invariant.
- **Walker-audit gate**: use `safe_walk` from the start in `discover_build_files` (T015) per memory `feedback_walker_audit_local_check`. m225's post-fix pattern is the template — do NOT hand-roll a directory walker.
- All fixture module names use `github.com/waybill-fixture/*` per memory `feedback_fixture_synthetic_package_names`. Never real coordinates like `github.com/spf13/cobra` that trip Kusari Inspector's advisory scan.
- `[P]` tasks touch different files with no ordering dependency.
- Every US phase is independently shippable AFTER US1 (US2/US3 are additive refinements).
- Estimated production LOC: ~400 total (T003 ≈ 70; T004 ≈ 130; T005 ≈ 80; T006 ≈ 50; T007 ≈ 60; T015+T016 ≈ 100; T017 ≈ 5; T021+T022 ≈ 40). Test LOC: ~350. Fixture LOC: ~150 (small, mostly go.sum lines).
