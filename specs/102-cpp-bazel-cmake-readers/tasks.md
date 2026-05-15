---
description: "Task list for milestone 102 — C/C++ source-tree readers (Bazel + CMake + vcpkg + Conan)"
---

# Tasks: C/C++ source-tree readers (Bazel + CMake)

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/102-cpp-bazel-cmake-readers/`
**Prerequisites**: plan.md, spec.md (with 3 Clarifications), research.md, data-model.md, contracts/reader-contracts.md, quickstart.md

**Tests**: Yes — TDD by integration test. Each reader gets a dedicated `tests/scan_<reader>.rs` integration test PLUS its 3 byte-identity goldens regenerated under the existing `cdx_regression` / `spdx_regression` / `spdx3_regression` test suites.

**Organization**: 3 user stories converge on 4 new readers + shared dispatcher wiring + CLI flag + docs. US1 (Bazel) + US2 (CMake) are both P1 — the MVP. US3 (vcpkg + Conan) is P2 — natural extension. All 4 readers are file-level independent so US1/US2/US3 can run in parallel after the Phase-2 foundational shared types land.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files OR independent of incomplete tasks)
- **[Story]**: User story this task belongs to (US1 / US2 / US3)
- File paths are workspace-relative.

## Path Conventions

Reader code under `mikebom-cli/src/scan_fs/package_db/` (4 new files). Integration tests under `mikebom-cli/tests/`. Fixtures under `mikebom-cli/tests/fixtures/{bazel,cmake,vcpkg,conan}/`. Goldens under `mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/`. CLI flag in `mikebom-cli/src/cli/scan_cmd.rs`. Docs in `README.md` + `docs/user-guide/cli-reference.md`. Zero changes outside these paths per FR-009 + FR-010.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify environment + baseline pre-PR gate is clean before touching anything.

- [X] T001 Confirm working branch is `102-cpp-bazel-cmake-readers`. Run `git status` + `git log -1 --oneline`; verify branch was created by `/speckit-specify` and main is at post-PR-#214 (v0.1.0-alpha.33 release-bump merge) or later. Confirm `git diff --name-only main` shows only the spec dir as untracked.
- [X] T002 Confirm baseline pre-PR gate passes on macOS/Linux dev host. Run `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh` once on the unchanged tree; expect `>>> all pre-PR checks passed.` Isolates any post-edit failure as introduced by milestone 102.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Establish the shared types + dispatch wiring that all 4 readers depend on. Per data-model.md, all 4 readers return `(Vec<PackageDbEntry>, Vec<ParseErrorAnnotation>)`; the dispatcher in `scan_fs/mod.rs::scan_path()` collects errors into a scan-summary annotation. Plus the `--include-vendored` CLI flag plumbing — needed by US2's vendored test but lands once.

- [ ] T003 Add the shared `ParseErrorAnnotation` struct to `mikebom-cli/src/scan_fs/package_db/mod.rs` (or a new `common.rs` file under that directory). Fields: `pub path: PathBuf`, `pub error: String`. Re-exported via `pub use common::ParseErrorAnnotation;`. Per FR-015 + research §1.
- [ ] T004 Add the shared `ReaderOptions { pub include_vendored: bool }` struct alongside `ParseErrorAnnotation`. Used by cmake.rs per FR-016; other 3 readers ignore the field. Per research §10.
- [X] T005 Plumb the `--include-vendored` CLI flag through `mikebom-cli/src/cli/scan_cmd.rs`:
    - Add `#[arg(long, env = "MIKEBOM_INCLUDE_VENDORED")] pub include_vendored: bool` to `ScanArgs`.
    - Add `include_vendored: bool` parameter to `execute(...)`.
    - Pass through to `scan_fs::scan_path()` (which gets a corresponding new parameter).
    - Verify with `cargo +stable build -p mikebom && ./target/debug/mikebom sbom scan --help | grep vendored` → flag appears in help.
    Per FR-016 + research §10.
- [X] T006 Create 4 stub reader files + register them in the package_db module:
    - `mikebom-cli/src/scan_fs/package_db/bazel.rs` (NEW, stub):
      ```rust
      //! Bazel source-tree reader. Stub — real implementation lands in T009.
      use std::path::Path;
      use mikebom_common::resolution::PackageDbEntry;
      use crate::scan_fs::package_db::ParseErrorAnnotation;

      pub fn read(_scan_root: &Path) -> (Vec<PackageDbEntry>, Vec<ParseErrorAnnotation>) {
          (vec![], vec![])
      }
      ```
    - `mikebom-cli/src/scan_fs/package_db/cmake.rs` (NEW, stub) — same shape; signature also takes `ReaderOptions` per FR-016: `pub fn read(_scan_root: &Path, _opts: ReaderOptions) -> ...`.
    - `mikebom-cli/src/scan_fs/package_db/vcpkg.rs` (NEW, stub).
    - `mikebom-cli/src/scan_fs/package_db/conan.rs` (NEW, stub).
    - Add `pub mod bazel; pub mod cmake; pub mod conan; pub mod vcpkg;` to `mikebom-cli/src/scan_fs/package_db/mod.rs` (alphabetical insert).
    Real bodies land in per-story phases (T009, T013, T019, T020). This stub-first approach keeps the workspace compiling at every checkpoint and lets T007's dispatch wiring be written + tested before any reader has real logic.
- [X] T007 Wire dispatch in `mikebom-cli/src/scan_fs/mod.rs::scan_path()`: add 4 new reader-invocation lines alongside the existing 11. Collect `Vec<ParseErrorAnnotation>` from each; aggregate into the scan-summary `mikebom:parse-error` annotation surfaced via `metadata.properties[]`. Per FR-015 + data-model.md `scan_fs/mod.rs`.

**Checkpoint**: After Phase 2, the workspace compiles + clippy-clean; the 4 readers exist as stubs returning empty results; the dispatcher wiring is in place; the `--include-vendored` flag appears in `mikebom --help`. Run `cargo +stable clippy -p mikebom --all-targets -- -D warnings` to lock the gate.

---

## Phase 3: User Story 1 — Bazel reader (Priority: P1) 🎯 MVP

**Goal**: A `mikebom sbom scan --path <bazel-project>` invocation emits SBOM components for every `bazel_dep` in `MODULE.bazel` and every `http_archive`/`http_file`/`git_repository` in `WORKSPACE.bazel`. PURLs are `pkg:bazel/<name>@<version>`. `mikebom:download-url` + `mikebom:bazel-archive-name` annotations recorded. `dev_dependency = True` maps to `LifecycleScope::Development`.

**Independent Test**: `cargo +stable test --test scan_bazel` against the bazel fixture; assert the 4 expected components emit (2 from MODULE.bazel, 1 http_archive, 1 git_repository).

### Implementation for User Story 1

- [ ] T008 [US1] Create the Bazel fixture at `mikebom-cli/tests/fixtures/bazel/`:
    - `MODULE.bazel` — 2 `bazel_dep` calls (one with `dev_dependency = True`).
    - `WORKSPACE.bazel` — 1 `http_archive` (with `urls = [...]` + `sha256`) + 1 `git_repository` (with `remote` + `commit`).
    Per data-model.md §`tests/fixtures/bazel/`.
- [ ] T009 [US1] Implement `mikebom-cli/src/scan_fs/package_db/bazel.rs` per data-model.md §`bazel.rs`:
    - `pub fn read(scan_root: &Path) -> (Vec<PackageDbEntry>, Vec<ParseErrorAnnotation>)` entry point.
    - `parse_module_bazel(path)` — `(?ms)bazel_dep\s*\(\s*name\s*=\s*"([^"]+)"\s*,\s*version\s*=\s*"([^"]+)"(?:\s*,\s*dev_dependency\s*=\s*(True|False))?\s*\)` regex; sets `LifecycleScope::Development` when `dev_dependency = True`.
    - `parse_workspace_bazel(path)` — matches `http_archive` / `http_file` / `git_repository`. Sets `mikebom:download-url` + `mikebom:bazel-archive-name` annotations. Records SHA-256 in `hashes[]` when present.
    - `build_bazel_purl(name, version) -> Purl` helper using `encode_purl_segment()` + `Purl::new()`.
    - `dedup_module_wins(entries)` per Contract 3.
    - `find_file(scan_root, names)` helper (or use shared helper if available).
    - Parse errors → `tracing::warn!` + return in the `Vec<ParseErrorAnnotation>` tuple element per FR-015.
- [ ] T010 [US1] Create the integration test at `mikebom-cli/tests/scan_bazel.rs`:
    - Test 1 (`bazel_module_emits_pkg_bazel_purls_with_native_scope`): scans the fixture, asserts:
      1. 2 components emit with PURLs `pkg:bazel/abseil-cpp@20240722.0` + `pkg:bazel/googletest@1.14.0`.
      2. The googletest component (declared `dev_dependency = True`) emits in the CDX JSON with **the standards-native `scope` field** populated correctly per Principle V — specifically `"scope": "excluded"` (which is the existing mikebom milestone-052 mapping for `LifecycleScope::Development` → CDX `scope`; verify against an existing `gem.cdx.json` golden to confirm the mapping convention). Plus `mikebom:lifecycle-scope = "development"` annotation per the existing milestone-052 dual-emission pattern.
      3. The abseil-cpp component (no dev_dependency) emits with NO `scope` field on its CDX entry (matches the existing convention for non-dev components).
      This negative-emission + native-field assertion verifies FR-014 + Principle V at the test layer, not just via the byte-identity goldens (T026).
    - Test 2 (`bazel_workspace_emits_with_url_and_sha`): asserts the `rules_python` http_archive component carries `mikebom:download-url` + SHA-256 in `hashes[]` + `mikebom:bazel-archive-name`.
    - Test 3 (`bazel_workspace_git_repository_emits_commit_as_version`): asserts the `rules_foo` git_repository component has the commit-SHA version.
    Use `env!("CARGO_BIN_EXE_mikebom")` + `Command::new(...)` to invoke mikebom against the fixture (matches the milestone-101 `scan_polyglot_monorepo.rs` pattern).

### Verification for User Story 1

- [ ] T011 [US1] Verify Contracts 1+2+3 from `contracts/reader-contracts.md`. Run:
    ```bash
    cargo +stable test --test scan_bazel 2>&1 | grep "test result:"
    # Expected: ok. 3 passed.
    cargo +stable clippy -p mikebom --all-targets -- -D warnings 2>&1 | tail -3
    # Expected: zero warnings.
    ```

**Checkpoint**: US1 complete. The Bazel reader emits components against the fixture; clippy-clean.

---

## Phase 4: User Story 2 — CMake reader (Priority: P1)

**Goal**: A `mikebom sbom scan --path <cmake-project>` invocation emits SBOM components for every `FetchContent_Declare` + `ExternalProject_Add` in `CMakeLists.txt` + included `.cmake` files. PURLs are `pkg:github/...` for GitHub-hosted git deps, `pkg:generic/...` otherwise. Optional `--include-vendored` flag enables emission of `add_subdirectory(third_party/...)` components.

**Independent Test**: `cargo +stable test --test scan_cmake` (default-off vendored) + `cargo +stable test --test scan_cmake_vendored` (with `MIKEBOM_INCLUDE_VENDORED=1`).

### Implementation for User Story 2

- [ ] T012 [P] [US2] Create the CMake fixture at `mikebom-cli/tests/fixtures/cmake/`:
    - `CMakeLists.txt` — 1 `FetchContent_Declare(googletest GIT_REPOSITORY https://github.com/google/googletest.git GIT_TAG release-1.14.0)` + 1 `ExternalProject_Add(zlib URL https://zlib.net/zlib-1.3.1.tar.gz URL_HASH SHA256=9a93b2b7dfdac77ceba5a558a580e74667dd6fede4585b91eefb60f03b72df23)` + **1 `find_package(zlib REQUIRED)` line** (exercises FR-011's negative-emission contract — verified by T014 Test 5). Also `include(cmake/third_party.cmake)`.
    - `cmake/third_party.cmake` — 1 `FetchContent_Declare(boost URL ... URL_HASH SHA256=...)`.
    - `third_party/foo/CMakeLists.txt` + `third_party/foo/version.txt` (containing `1.2.3`) for the vendored test.
    Per data-model.md §`tests/fixtures/cmake/`.
- [ ] T013 [US2] Implement `mikebom-cli/src/scan_fs/package_db/cmake.rs` per data-model.md §`cmake.rs`:
    - `pub fn read(scan_root: &Path, opts: ReaderOptions) -> (Vec<PackageDbEntry>, Vec<ParseErrorAnnotation>)` entry point.
    - `parse_fetchcontent(content, path)` — git-form + url-form regexes per research §4. GitHub URL detection produces `pkg:github/<owner>/<repo>@<tag>`; otherwise `pkg:generic/<name>@<tag>` with `mikebom:download-url`.
    - `parse_externalproject(content, path)` — same shape.
    - `parse_add_subdirectory(content, path, opts)` — gated on `opts.include_vendored`; only emits when path starts with `third_party/` or `vendor/`. Reads co-located `version.txt` for version backfill.
    - `walk_for_cmake_files(scan_root)` — finds `CMakeLists.txt` at root + `*.cmake` under `cmake/`, `Modules/`, `third_party/` (per FR-005).
    - Parse errors → `tracing::warn!` + `ParseErrorAnnotation` per FR-015.
- [ ] T014 [US2] Create the integration test at `mikebom-cli/tests/scan_cmake.rs`:
    - Test 1 (`cmake_fetchcontent_github_emits_pkg_github`): asserts the googletest component has PURL `pkg:github/google/googletest@release-1.14.0`.
    - Test 2 (`cmake_externalproject_url_emits_sha256_and_url`): asserts the zlib component carries the SHA-256 hash + `mikebom:download-url`.
    - Test 3 (`cmake_includes_walked`): asserts the boost component from `cmake/third_party.cmake` is present with `mikebom:source-files` pointing to the included file.
    - Test 4 (`cmake_vendored_not_emitted_by_default`): asserts NO `pkg:generic/foo@1.2.3` component appears when `--include-vendored` is not set.
    - Test 5 (`cmake_find_package_does_not_emit_components`): the fixture's `CMakeLists.txt` MUST include a `find_package(zlib REQUIRED)` directive. The test asserts zero `pkg:vcpkg/zlib` / `pkg:conan/zlib` / `pkg:generic/zlib` / `pkg:cmake/zlib` components appear in the emitted SBOM from the CMakeLists.txt source. Per FR-011 — `find_package` declarations refer to system-installed packages (handled by OS-package readers / vcpkg / Conan separately); cmake.rs MUST NOT emit phantom entries for them. Add the `find_package(zlib REQUIRED)` line to the fixture's `CMakeLists.txt` as part of T012; the negative assertion lives here in T014.
    Use the milestone-101 `CARGO_BIN_EXE_mikebom` pattern.
- [ ] T015 [US2] Create the vendored-specific test at `mikebom-cli/tests/scan_cmake_vendored.rs`:
    - Test (`cmake_vendored_emitted_when_include_vendored_set`): invokes mikebom with `Command::env("MIKEBOM_INCLUDE_VENDORED", "1")`; asserts the `pkg:generic/foo@1.2.3` component appears with `mikebom:vendored = true` annotation.

### Verification for User Story 2

- [ ] T016 [US2] Verify Contracts 4+5+6+9 from `contracts/reader-contracts.md`. Run:
    ```bash
    cargo +stable test --test scan_cmake --test scan_cmake_vendored 2>&1 | grep "test result:"
    # Expected: ok. 5 passed + ok. 1 passed (6 total across the 2 test binaries).
    cargo +stable clippy -p mikebom --all-targets -- -D warnings 2>&1 | tail -3
    # Expected: zero warnings.
    ```

**Checkpoint**: US2 complete. The CMake reader emits components against the fixture, including the included-file walk and the opt-in vendored case.

---

## Phase 5: User Story 3 — vcpkg + Conan readers (Priority: P2)

**Goal**: A `mikebom sbom scan --path <project>` invocation emits `pkg:vcpkg/<name>@<version>` components for every `vcpkg.json::dependencies[]` entry AND `pkg:conan/<name>@<version>` components for every `conanfile.txt::[requires]` + `conanfile.py::requires=[...]` entry. `[tool_requires]` lines map to `LifecycleScope::Build`. Cross-ecosystem same-name deps emit as TWO separate components per the Q2 clarification.

**Independent Test**: `cargo +stable test --test scan_vcpkg` + `cargo +stable test --test scan_conan`.

### Implementation for User Story 3

- [X] T017 [P] [US3] Create the vcpkg fixture at `mikebom-cli/tests/fixtures/vcpkg/vcpkg.json`:
    ```json
    { "name": "test-project", "version": "0.1.0",
      "dependencies": ["zlib", {"name": "openssl", "version>=": "3.0.0"}] }
    ```
    Per data-model.md §`tests/fixtures/vcpkg/`.
- [X] T018 [P] [US3] Create the Conan fixture at `mikebom-cli/tests/fixtures/conan/`:
    - `conanfile.txt` with `[requires]\nzlib/1.2.13\nopenssl/3.0.0\n[tool_requires]\ncmake/3.27.0`.
    - `conanfile.py` with `requires = ["zlib/1.2.13", "openssl/3.0.0"]` and `tool_requires = ["cmake/3.27.0"]`.
    Per data-model.md §`tests/fixtures/conan/`.
- [X] T019 [P] [US3] Implement `mikebom-cli/src/scan_fs/package_db/vcpkg.rs` per data-model.md §`vcpkg.rs`:
    - `pub fn read(scan_root: &Path) -> (Vec<PackageDbEntry>, Vec<ParseErrorAnnotation>)` entry.
    - serde-derive `VcpkgManifest` struct with `dependencies: Vec<Dependency>` and `overrides: Vec<Override>`; `Dependency` is `#[serde(untagged)]` Simple-vs-Detailed enum.
    - Post-process `overrides` to substitute the overridden version per Edge Cases.
    - Build `pkg:vcpkg/<name>@<version>` via `Purl::new()` + `encode_purl_segment()`. Omit `@<version>` when no version declared.
    - Set `source_path = path-to-vcpkg.json` per FR-012.
    - Parse errors via `serde_json::from_str` `Err` → `tracing::warn!` + `ParseErrorAnnotation`.
- [X] T020 [P] [US3] Implement `mikebom-cli/src/scan_fs/package_db/conan.rs` per data-model.md §`conan.rs`:
    - `pub fn read(scan_root: &Path) -> (Vec<PackageDbEntry>, Vec<ParseErrorAnnotation>)` entry.
    - `parse_conanfile_txt(path)` — hand-rolled line-by-line INI parser per research §6. Detects `[requires]` / `[tool_requires]` sections; parses each `<name>/<version>` line; sets `LifecycleScope::Build` for `[tool_requires]`.
    - `parse_conanfile_py(path)` — regex on `(?ms)^\s*(requires|tool_requires)\s*=\s*\[([^\]]+)\]` per research §7; splits inner list on `,`, extracts literal `"<name>/<version>"` strings, ignores non-string entries.
    - Build `pkg:conan/<name>@<version>` via `Purl::new()` + `encode_purl_segment()`.
- [X] T021 [US3] Create the vcpkg integration test at `mikebom-cli/tests/scan_vcpkg.rs`:
    - Test 1 (`vcpkg_simple_dependency_emits_no_version`): asserts `pkg:vcpkg/zlib` component (no version segment).
    - Test 2 (`vcpkg_detailed_dependency_emits_version`): asserts `pkg:vcpkg/openssl@3.0.0` component (version from `version>=`).
- [X] T022 [US3] Create the Conan integration test at `mikebom-cli/tests/scan_conan.rs`:
    - Test 1 (`conan_txt_requires_emit_runtime_scope`): asserts 2 `pkg:conan/zlib@1.2.13` + `pkg:conan/openssl@3.0.0` components from conanfile.txt with no scope.
    - Test 2 (`conan_txt_tool_requires_emit_build_scope`): asserts `pkg:conan/cmake@3.27.0` from conanfile.txt with `lifecycle_scope = Build`.
    - Test 3 (`conan_py_requires_emit_components`): asserts the same components emit from conanfile.py.
- [X] T023 [US3] Add a cross-ecosystem dedup test (Contract 10) at `mikebom-cli/tests/scan_vcpkg_conan_cross.rs`:
    - Fixture with BOTH `vcpkg.json` (declaring `openssl`) AND `conanfile.txt` (declaring `openssl/3.0.0`).
    - Test (`cross_ecosystem_same_name_emits_two_components`): asserts the SBOM contains BOTH `pkg:vcpkg/openssl` AND `pkg:conan/openssl@3.0.0` as separate components.

### Verification for User Story 3

- [X] T024 [US3] Verify Contracts 7+8+10 from `contracts/reader-contracts.md`. Run:
    ```bash
    cargo +stable test --test scan_vcpkg --test scan_conan --test scan_vcpkg_conan_cross 2>&1 | grep "test result:"
    # Expected: ok. 2 + 3 + 1 = 6 tests passed.
    cargo +stable clippy -p mikebom --all-targets -- -D warnings 2>&1 | tail -3
    # Expected: zero warnings.
    ```

**Checkpoint**: US3 complete. vcpkg + Conan readers emit components; cross-ecosystem dedup behaves per Q2 clarification.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Goldens generation, parse-error coverage, README + cli-reference docs, diff-scope audit, pre-PR gate, PR open.

- [ ] T025 [P] Add 4 ecosystem test functions per format to extend the existing byte-identity goldens regression suite:
    - `mikebom-cli/tests/cdx_regression.rs` — add `cdx_regression_bazel`, `cdx_regression_cmake`, `cdx_regression_vcpkg`, `cdx_regression_conan` (4 new `#[test]` fns).
    - `mikebom-cli/tests/spdx_regression.rs` — add `bazel_byte_identity`, `cmake_byte_identity`, `vcpkg_byte_identity`, `conan_byte_identity` (4 new).
    - `mikebom-cli/tests/spdx3_regression.rs` — same 4 new tests.
    Wire each into the existing `CASES` array. Match the format of the existing 9 ecosystems' test stubs.
- [ ] T026 Regenerate the 12 NEW goldens. Run:
    ```bash
    MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 \
      cargo +stable test -p mikebom \
        --test cdx_regression --test spdx_regression --test spdx3_regression
    git status mikebom-cli/tests/fixtures/golden/ | head -20
    # Expected: 12 NEW files (4 ecosystems × 3 formats).
    git diff --stat mikebom-cli/tests/fixtures/golden/cyclonedx/{apk,cargo,deb,gem,golang,maven,npm,pip,rpm}.cdx.json | tail -1
    # Expected: empty (existing 9 ecosystems untouched per SC-006).
    ```
    Then re-run without env vars to confirm byte-identity locks:
    ```bash
    cargo +stable test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression \
      2>&1 | grep "test result:"
    # Expected: ok. 13 passed × 3 formats.
    ```
- [ ] T027 [P] Add a parse-error coverage integration test at `mikebom-cli/tests/scan_parse_errors.rs`:
    - Fixture: a deliberately-malformed `vcpkg.json` (truncated with unbalanced braces) in a separate fixture directory.
    - Test (`malformed_manifest_emits_scan_summary_parse_error`): scans the malformed fixture; asserts (a) NO components from that file, (b) `metadata.properties[]` contains a `mikebom:parse-error` entry naming the file path. Per Contract 11 / FR-015.
- [ ] T028 [P] Update `README.md` "Supported ecosystems" table — add 4 new rows for Bazel (manifests: `MODULE.bazel`, `WORKSPACE.bazel`), CMake (`CMakeLists.txt` + `cmake/*.cmake`), vcpkg (`vcpkg.json`), Conan (`conanfile.txt` + `conanfile.py`). Match the format of existing rows.
- [ ] T029 [P] Update `docs/user-guide/cli-reference.md` — add a `--include-vendored` section per FR-017. Must cover: default-OFF, what counts as vendored (`third_party/` or `vendor/` path prefix), false-positive risks (e.g., `add_subdirectory(src)`, `add_subdirectory(tests)`), `version.txt` version-backfill convention.
- [ ] T030 Verify Contract 12 — diff-scope audit. Run:
    ```bash
    # No new Cargo deps:
    git diff --name-only main | grep -E '^Cargo\.(lock|toml)$|/Cargo\.(lock|toml)$' | wc -l
    # Expected: 0

    # Existing 9 ecosystems' goldens unchanged:
    git diff --stat mikebom-cli/tests/fixtures/golden/cyclonedx/{apk,cargo,deb,gem,golang,maven,npm,pip,rpm}.cdx.json | tail -1
    git diff --stat mikebom-cli/tests/fixtures/golden/spdx-2.3/{apk,cargo,deb,gem,golang,maven,npm,pip,rpm}.spdx.json | tail -1
    git diff --stat mikebom-cli/tests/fixtures/golden/spdx-3/{apk,cargo,deb,gem,golang,maven,npm,pip,rpm}.spdx3.json | tail -1
    # Expected: all 3 empty.

    # File-tree allowlist:
    git diff --name-only origin/main | sort
    # Expected:
    #   CLAUDE.md                                              (auto-updated)
    #   README.md
    #   docs/user-guide/cli-reference.md
    #   mikebom-cli/src/cli/scan_cmd.rs
    #   mikebom-cli/src/scan_fs/mod.rs
    #   mikebom-cli/src/scan_fs/package_db/mod.rs
    #   mikebom-cli/tests/cdx_regression.rs
    #   mikebom-cli/tests/spdx_regression.rs
    #   mikebom-cli/tests/spdx3_regression.rs
    #   specs/102-cpp-bazel-cmake-readers/...
    git ls-files --others --exclude-standard | sort
    # Expected NEW:
    #   mikebom-cli/src/scan_fs/package_db/{bazel,cmake,conan,vcpkg}.rs    (4 readers)
    #   mikebom-cli/tests/scan_{bazel,cmake,cmake_vendored,vcpkg,conan,vcpkg_conan_cross,parse_errors}.rs   (7 tests)
    #   mikebom-cli/tests/fixtures/{bazel,cmake,vcpkg,conan}/...    (fixture trees)
    #   mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/{bazel,cmake,vcpkg,conan}.*    (12 goldens)
    ```
- [X] T031 Run the mandatory pre-PR gate per Contract 12 / Contract 9 of milestone 100. Run `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh`. Expect: `>>> all pre-PR checks passed.` Every target reports `0 failed`. The new readers' tests + the extended goldens regression all pass.
- [ ] T032 Open the PR. Title: `feat(102): C/C++ source-tree readers (Bazel + CMake + vcpkg + Conan)`. Body must mention:
    - 4 new readers + cross-ecosystem dedup behavior
    - `--include-vendored` flag with default-OFF + env-var fallback
    - Parse-error transparency annotation
    - Zero new Cargo deps; zero changes to existing 11 readers
    - Diff scope (per Contract 12 output)

---

## Dependencies

```text
T001 (branch check) → T002 (baseline pre-PR)
T002 → T003 (ParseErrorAnnotation) → T004 (ReaderOptions) → T005 (CLI flag) → T006 (mod.rs decls) → T007 (dispatch wiring)
T007 → US1: T008 [P] → T009 → T010 → T011
T007 → US2: T012 [P] → T013 → T014 → T015 → T016
T007 → US3: T017/T018 [P] → T019/T020 [P] → T021/T022 → T023 → T024
T011 + T016 + T024 → T025 [P] (test stubs) → T026 (regen goldens) + T027 [P] (parse-errors) + T028 [P] (README) + T029 [P] (cli-reference)
T030 (diff audit) → T031 (pre-PR) → T032 (open PR)
```

US1 / US2 / US3 are file-level independent after Phase 2 completes — they can be implemented in parallel by 3 different developers (or 3 LLM-agent threads). Within each story, fixture creation [P] runs in parallel with reader implementation since they're different files.

## Parallel Execution Opportunities

- **After T007**: fixture creation (T008, T012, T017, T018) for all 3 stories runs in parallel — different file trees.
- **Within US3**: T019 (vcpkg reader) + T020 (conan reader) run in parallel — different files.
- **Phase 6 polish**: T025 (test stubs), T027 (parse-error test), T028 (README), T029 (cli-reference) are all [P] — different files, no inter-dependency.

## Implementation Strategy

**MVP scope**: US1 (Bazel) + US2 (CMake). Both P1. Ship together because they're the headline value and they share zero implementation code (independent readers). US3 (vcpkg + Conan) is P2 — ship in the same PR for completeness, but US3 could be a follow-up PR if the schedule demands.

**Suggested execution order**: T001 → T002 → T003 → T004 → T005 → T006 → T007 → (parallel: T008 + T012 + T017 + T018) → (parallel: T009 + T013 + T019 + T020) → (parallel: T010 + T014 + T015 + T021 + T022 + T023) → (parallel: T011 + T016 + T024) → (parallel: T025 + T027 + T028 + T029) → T026 → T030 → T031 → T032. Total: 32 tasks.

**Risk**: T013 (CMake reader) is the largest single implementation task and the most heuristic. Budget ~1.5h for it specifically; budget ~1h each for T009 (Bazel) and ~30min each for T019/T020 (vcpkg/Conan). Total reader-impl budget: ~3.5h. Plus tests, goldens, docs = ~6h end-to-end.

**Backup plan**: if T013's CMake heuristics surface unexpected real-world corpus failures during implementation, descope to only `FetchContent_Declare` (drop `ExternalProject_Add`) and ship US2 as "partial CMake coverage" with the gap documented in #210-style follow-up issue.

## Task format validation

All 32 tasks follow the required format `- [ ] TXXX [P?] [USX?] Description with file path`:
- ✅ Checkboxes start every line
- ✅ Sequential task IDs T001 – T032
- ✅ [P] markers only where parallelization is sound
- ✅ [US1/US2/US3] labels on every user-story-phase task
- ✅ Setup (T001-T002), Foundational (T003-T007), Polish (T025-T032) — NO story label
- ✅ Every task includes a concrete file path (or meta-action descriptor for branch-check / gate-run tasks)
