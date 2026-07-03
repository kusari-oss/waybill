---

description: "Task list for milestone 156 — CMake walker depth extension"
---

# Tasks: CMake walker depth extension (milestone 156)

**Input**: Design documents from `/specs/156-cmake-walker-depth/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: Included. SC-008 requires ≥6 new tests; the spec + research inventory 6 unit tests + 5 integration tests = 11 total.

**Organization**: Tasks are grouped by user story. US1 (P1 compliance auditor gets full Kamailio roster) is the MVP. US2 (P2 byte-identity guard for existing depth-1-only fixtures) is verification of the SC-002 backward-compat guarantee via pre-existing golden tests + the milestone-155 Kamailio-shape integration test.

**Depends on**: milestone 155 (PR #489) must be merged. The extended walker feeds the milestone-155 emission pipeline unchanged; without milestone 155 landed, this milestone's tests fail at the first `find_package` extraction.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1 / US2)
- Include exact file paths in descriptions

## Path Conventions

- Primary deliverable: `mikebom-cli/src/scan_fs/package_db/cmake.rs`
- CLI arg struct: `mikebom-cli/src/cli/scan_cmd.rs`
- Direct callers of `cmake::read`: `mikebom-cli/src/scan_fs/package_db/mod.rs`, `mikebom-cli/src/scan_fs/binary/mod.rs`
- Integration tests: `mikebom-cli/tests/`
- Fixture files: `mikebom-cli/tests/fixtures/cmake-walker-depth/`
- CHANGELOG: `CHANGELOG.md` at repo root
- No changes to: `mikebom-cli/src/generate/`, `mikebom-cli/src/parity/extractors/`, other readers, `mikebom-common/`, `mikebom-ebpf/`, `docs/reference/sbom-format-mapping.md`, `mikebom-cli/tests/fixtures/golden/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Baseline verification. No project scaffolding needed — this is a single-file additive change to an existing crate.

- [ ] T001 Verify baseline state: `git log -1 --oneline`, confirm branch `156-cmake-walker-depth`, capture pre-milestone `cmake.rs` LOC (`wc -l mikebom-cli/src/scan_fs/package_db/cmake.rs`), pre-milestone `discover_cmake_files_` test count (`grep -cE "^\s+fn discover_cmake_files_" mikebom-cli/src/scan_fs/package_db/cmake.rs` — should be 0 pre-156), and confirm milestone 155 (PR #489) is merged to main (`git log main --oneline | head -3` — should show impl(155) or later).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Extract shared helpers + extend `cmake::read` signature. Both edit `cmake.rs` + their downstream callers, so they are sequential.

**⚠️ CRITICAL**: T002 + T003 must complete before US1 work begins. T002 breaks the `cmake::read` API, so all 3 callers (T002 itself + package_db/mod.rs + binary/mod.rs) update together in one commit-block to keep the codebase compilable.

- [ ] T002 Extend `pub fn read` in `mikebom-cli/src/scan_fs/package_db/cmake.rs:35` to accept `exclude_set: &super::exclude_path::ExclusionSet` as a third parameter (immediately after `include_vendored`). New signature: `pub fn read(scan_root: &Path, include_vendored: bool, exclude_set: &super::exclude_path::ExclusionSet) -> Vec<PackageDbEntry>`. In the same commit-block, update the two call sites: `mikebom-cli/src/scan_fs/package_db/mod.rs:1533` (change `cmake::read(rootfs, include_vendored)` → `cmake::read(rootfs, include_vendored, exclude_set)`; `exclude_set` is already in scope as a parameter of `read_all`); `mikebom-cli/src/scan_fs/binary/mod.rs:198` (change `cmake::read(rootfs, false)` → `cmake::read(rootfs, false, exclude_set)`; `exclude_set` is already in scope inside the binary scan loop per its `discover_binaries(rootfs, exclude_set)` invocation two lines later). Verify workspace compiles via `cargo check -p mikebom` before proceeding.

- [ ] T003 Extract three module-private helpers at the top of the `discover_cmake_files` region of `mikebom-cli/src/scan_fs/package_db/cmake.rs`:
  - `fn is_cmake_file(p: &Path) -> bool` — extension check (`eq_ignore_ascii_case("cmake")`) OR filename check (`eq_ignore_ascii_case("CMakeLists.txt")`), preserving the exact predicates from the existing `cmake.rs:206-215`.
  - `fn collect_cmake_files_depth1(dir: &Path, out: &mut Vec<PathBuf>)` — extract the existing `read_dir → is_cmake_file → push` loop from `cmake.rs:200-221`. This helper preserves the milestone-102 behavior for `third_party/` when the opt-in flag is not set.
  - `fn collect_cmake_files_recursive(dir: &Path, exclude_set: &super::exclude_path::ExclusionSet, out: &mut Vec<PathBuf>)` — LEFT AS `todo!()` for now; T005 fills the body. Signature declared here so the T006 refactor of `discover_cmake_files` can call it. Alternatively land the empty body here and fill in T005.

  Do NOT change `discover_cmake_files`'s behavior yet — it still calls the pre-extraction inlined `read_dir` loop. This task ONLY refactors helpers into shared functions used by both the depth-1 and (future) recursive paths.

**Checkpoint**: Foundation ready — US1 implementation can now begin.

---

## Phase 3: User Story 1 — Compliance auditor scanning a Kamailio-shaped tree gets the full declared-dep roster (Priority: P1) 🎯 MVP

**Goal**: Recursive descent under `cmake/` + `Modules/`; opt-in recursion for `third_party/` via `--cmake-third-party-recursive` (env alias `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1`). Milestone-155 emission pipeline runs unchanged against the longer discovered-files list.

**Independent Test**: Run the 5 SC-integration tests (T009-T013); each exercises one aspect (symlink cycle, depth-3 emission, cross-depth version consolidation, exclude-path, third-party opt-in). Plus the 6 in-module unit tests (T008).

### Implementation for User Story 1

- [ ] T004 [US1] At the top of `pub fn read` in `mikebom-cli/src/scan_fs/package_db/cmake.rs:35`, add the env-var read for the new opt-in flag (mirrors milestone-102 `MIKEBOM_INCLUDE_VENDORED` read at `read_all:1193`):
  ```rust
  let include_third_party_recursive = std::env::var("MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE")
      .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
      .unwrap_or(false);
  ```
  Pass `include_third_party_recursive` to `discover_cmake_files` (T006 will consume it).

- [ ] T005 [US1] Fill the `collect_cmake_files_recursive` body in `mikebom-cli/src/scan_fs/package_db/cmake.rs` per research §R4:
  ```rust
  fn collect_cmake_files_recursive(
      dir: &Path,
      exclude_set: &super::exclude_path::ExclusionSet,
      out: &mut Vec<PathBuf>,
  ) {
      use crate::scan_fs::walk::{safe_walk, WalkConfig};
      let cfg = WalkConfig {
          max_depth: 16,
          should_skip: &|_candidate: &Path, _rootfs: &Path| false,
          exclude_set,
      };
      safe_walk(dir, &cfg, |path: &Path| {
          if path.is_file() && is_cmake_file(path) {
              out.push(path.to_path_buf());
          }
      });
  }
  ```
  Reuses milestone-054's `safe_walk` at `mikebom-cli/src/scan_fs/walk.rs:174` — get symlink-cycle safety + rootfs sandbox + exclude-path integration + debug logging for free per research §R1.

- [ ] T006 [US1] Refactor `fn discover_cmake_files` in `mikebom-cli/src/scan_fs/package_db/cmake.rs:195` to the new signature:
  ```rust
  fn discover_cmake_files(
      scan_root: &Path,
      include_third_party_recursive: bool,
      exclude_set: &super::exclude_path::ExclusionSet,
  ) -> Vec<PathBuf>
  ```
  Body per research §R4:
  ```rust
  let mut out = Vec::new();
  let top = scan_root.join("CMakeLists.txt");
  if top.is_file() {
      out.push(top);
  }
  for subdir in &["cmake", "Modules"] {
      let dir = scan_root.join(subdir);
      if dir.is_dir() {
          collect_cmake_files_recursive(&dir, exclude_set, &mut out);
      }
  }
  let third_party = scan_root.join("third_party");
  if third_party.is_dir() {
      if include_third_party_recursive {
          collect_cmake_files_recursive(&third_party, exclude_set, &mut out);
      } else {
          collect_cmake_files_depth1(&third_party, &mut out);
      }
  }
  out
  ```
  Update the callers of `discover_cmake_files` inside `cmake.rs` (there are 2 — `read()` at line 36 and `collect_find_package_targets()` at line 97) to pass `false` for `include_third_party_recursive` in the collector case (that helper's semantics are name-collection for milestone-105 US6 submodule classification, orthogonal to walker-depth). For `read()`, pass the value captured in T004.

- [ ] T007 [US1] Add the new CLI arg field in `mikebom-cli/src/cli/scan_cmd.rs` immediately after `pub include_vendored: bool` at line 365 (per data-model.md §2):
  ```rust
  /// Extend the CMake reader's recursive descent to third_party/.
  /// By default (unset) third_party/ is walked at depth-1 only
  /// (matching milestone-102 behavior); recursive descent applies
  /// only to cmake/ and Modules/. Setting this flag treats
  /// third_party/ the same way. Useful when the parent project has
  /// vendored a large dep tree (LLVM, Chromium, WebRTC, etc.) whose
  /// transitive find_package declarations should surface in the SBOM.
  ///
  /// Also accepts MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1 env var.
  #[arg(long)]
  pub cmake_third_party_recursive: bool,
  ```
  Then add the env-var propagation block near `args.include_vendored` at scan_cmd.rs:1703 (immediately after the existing `MIKEBOM_INCLUDE_VENDORED` set):
  ```rust
  if args.cmake_third_party_recursive {
      // SAFETY: single-threaded at this point in the scan-cmd lifecycle
      // (same as MIKEBOM_INCLUDE_VENDORED above).
      unsafe {
          std::env::set_var("MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE", "1");
      }
  }
  ```
  Also update the test helper `include_vendored: false,` default at scan_cmd.rs:3396 to include `cmake_third_party_recursive: false,` — matches the pattern from milestone-155 fix's `feedback_build_check_all_targets.md` memory (per-crate `cargo build --all-targets` catches derive-forgotten struct-literal test helpers).

- [ ] T008 [US1] Add 6 unit test bodies inside the existing `#[cfg(test)] mod tests` block in `mikebom-cli/src/scan_fs/package_db/cmake.rs`, following research §R6 inventory:
  1. `discover_cmake_files_walks_cmake_recursively` — fixture: `cmake/modules/FindFoo.cmake` (depth-2). Assert discovered.
  2. `discover_cmake_files_walks_modules_recursively` — fixture: `Modules/utils/Extra.cmake` (depth-2). Assert discovered.
  3. `discover_cmake_files_depth1_third_party_by_default` — fixture: `third_party/depth1.cmake` (depth-1) + `third_party/subdir/depth2.cmake` (depth-2). No env var set. Assert depth-1 discovered, depth-2 NOT discovered.
  4. `discover_cmake_files_recursive_third_party_when_opt_in` — same fixture as #3. Set `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1` via `std::env::set_var`. Assert BOTH depth-1 and depth-2 discovered. Reset env var at test end via `std::env::remove_var` — env-var mutation is a shared-state hazard, so this test uses a serial guard (e.g., a `Mutex` in the tests module or `serial_test` crate — check if workspace uses `serial_test`; if not, use a module-scoped `Mutex`).
  5. `discover_cmake_files_respects_exclude_set` — fixture: `cmake/modules/FindFoo.cmake`. Construct an `ExclusionSet` containing `cmake/modules`. Assert file NOT discovered.
  6. `find_package_at_depth2_emits_via_read` — fixture: `cmake/modules/FindLibev.cmake` containing `find_package(Libev 1.4.0)`. Call `read(scan_root, false, &empty_exclusion_set)`. Assert emitted `pkg:generic/libev@1.4.0` with mechanism `cmake-find-package`.

  All 6 tests use `tempfile::tempdir()` + `std::fs::write` following the existing cmake.rs test pattern. Use `#[cfg_attr(test, allow(clippy::unwrap_used))]` on the existing `mod tests` block (already present).

- [ ] T009 [P] [US1] Create SC-003 symlink cycle testbed:
  - Fixture: `mikebom-cli/tests/fixtures/cmake-walker-depth/symlink-cycle/` containing:
    - `CMakeLists.txt` (trivial, empty content ok).
    - `cmake/defs.cmake` containing `find_package(Foo 1.0)`.
    - `cmake/loop` → symlink pointing back to `../cmake` (created at test-setup time, NOT checked in — symlinks in git are fragile; the test creates it via `std::os::unix::fs::symlink` on Unix and `std::os::windows::fs::symlink_dir` on Windows, or skips-with-warn on platforms not supporting either).
  - Integration test: `mikebom-cli/tests/cmake_walker_depth_symlink_cycle.rs`. Invokes the release binary via `Command::new(env!("CARGO_BIN_EXE_mikebom"))`; times out at 5 seconds; asserts scan completes + exactly one `pkg:generic/foo@1.0` component in the emitted CDX. Uses the same `run_scan()` pattern as milestone-155's `cmake_find_package_kamailio_shape_integration.rs`.

- [ ] T010 [P] [US1] Create SC-004 depth-3 emission testbed:
  - Fixture: `mikebom-cli/tests/fixtures/cmake-walker-depth/depth3-emission/` containing:
    - `CMakeLists.txt` (trivial).
    - `cmake/modules/vendor/Extra.cmake` containing `find_package(Foo 2.5)`.
  - Integration test: `mikebom-cli/tests/cmake_walker_depth_deep_emission.rs`. Asserts exactly one `pkg:generic/foo@2.5` component with `mikebom:source-mechanism = "cmake-find-package"` and `mikebom:source-files` containing `cmake/modules/vendor/Extra.cmake`.

- [ ] T011 [P] [US1] Create SC-005 cross-depth version consolidation testbed:
  - Fixture: `mikebom-cli/tests/fixtures/cmake-walker-depth/cross-depth-version/` containing:
    - `CMakeLists.txt` containing `find_package(OpenSSL 1.1.0)`.
    - `cmake/modules/FindOpenSSL.cmake` containing `find_package(OpenSSL 3.0)`.
  - Integration test: `mikebom-cli/tests/cmake_walker_depth_cross_depth_version.rs`. Asserts exactly ONE `pkg:generic/openssl@3.0` component (Q1 milestone-155 highest-version-wins fires across depths). Asserts `mikebom:source-files` contains BOTH `CMakeLists.txt` AND `cmake/modules/FindOpenSSL.cmake` (milestone-148 union preserved).

- [ ] T012 [P] [US1] Create SC-006 exclude-path integration testbed:
  - Fixture: `mikebom-cli/tests/fixtures/cmake-walker-depth/exclude-path-integration/` containing:
    - `CMakeLists.txt` (trivial).
    - `cmake/defs.cmake` containing `find_package(Bar 1.0)`.
    - `cmake/modules/FindFoo.cmake` containing `find_package(Foo)`.
  - Integration test: `mikebom-cli/tests/cmake_walker_depth_exclude_path.rs`. Invokes the release binary with `--exclude-path cmake/modules/`. Asserts exactly ONE `pkg:generic/bar@1.0` component (Bar from depth-1 defs.cmake) AND ZERO `pkg:generic/foo` components (Foo from excluded cmake/modules/).

- [ ] T013 [P] [US1] Create SC-011 third-party opt-in testbed:
  - Fixture: `mikebom-cli/tests/fixtures/cmake-walker-depth/third-party-opt-in/` containing:
    - `CMakeLists.txt` (trivial).
    - `third_party/somedep/cmake/deps.cmake` containing `find_package(VendoredDepDep)`.
  - Integration test: `mikebom-cli/tests/cmake_walker_depth_third_party_opt_in.rs` with TWO test functions:
    1. `third_party_opt_in_off_by_default` — invokes release binary without any flag or env var. Asserts ZERO `pkg:generic/vendoreddepdep` components (third_party/ at depth-3 not walked).
    2. `third_party_opt_in_flag_enables_recursion` — invokes release binary with `--cmake-third-party-recursive`. Asserts exactly ONE `pkg:generic/vendoreddepdep` component. (Alternative equivalent test: set `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1` env var instead of flag — both paths must be exercised.)

**Checkpoint**: At this point, US1 is fully functional. `mikebom sbom scan` against Kamailio (SC-001 manual test in T017 below) should produce ≥10 identified components.

---

## Phase 4: User Story 2 — Existing depth-1 emissions unchanged (Priority: P2)

**Goal**: Verify SC-002 byte-identity guard — no golden regeneration required; the milestone-155 Kamailio-shape integration test still passes with the same 5+1 component counts.

**Independent Test**: Run all 3 format regression suites + the milestone-155 Kamailio-shape integration test. Zero diffs; same emission counts.

### Verification for User Story 2

- [ ] T014 [US2] Run `cargo +stable test --workspace --no-fail-fast --test cdx_regression --test spdx_regression --test spdx3_regression --test cmake_find_package_kamailio_shape_integration --test cmake_find_package_dedup_integration` and verify: (a) ALL 3 golden-regression test binaries pass (11 tests × 3 formats = 33 tests, plus the 2 milestone-155 integration tests = 35 total); (b) NO goldens require regeneration (a clean regeneration attempt via `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo test --test cdx_regression 2>&1 | grep 'wrote'` should output nothing — if any golden was rewritten, SC-002 has failed and the emission-shape needs debugging). If ANY test fails, halt US2 and open a regression investigation — most likely the milestone-155 Kamailio-shape fixture's depth-2 `FindLibev.cmake` file's `find_package_handle_standard_args(Libev ...)` regex-boundary check has drifted, OR the milestone-090 cmake fixture has picked up an unexpected depth-2 file. Do NOT regenerate goldens as a fix — investigate root cause.

**Checkpoint**: US2 verified. Backward compatibility guarantee satisfied.

---

## Phase 5: Polish & Cross-Cutting Concerns

- [ ] T015 Add CHANGELOG.md entry under `## [Unreleased]` per research §R7 + SC-009. Entry names: (a) the walker-depth extension for `cmake/` + `Modules/`; (b) the new `--cmake-third-party-recursive` opt-in flag + env alias; (c) the Kamailio testbed impact (1 → ≥10 identified components); (d) reference back to milestone-155's F1 remediation as the closed debt; (e) recommendation for build-tree contamination: `--exclude-path build,cmake-build-*,out` (per FR-018). Include the consumer jq recipe from research §R7 for filtering by source-file depth. Place the entry above whatever entry is currently at the top of `[Unreleased]`.

- [ ] T016 Run SC-007 pre-PR gate. **CRITICAL** — per the milestone-155 fix memory `feedback_prepr_gate_bails_on_first_failure.md`, both commands MUST be run explicitly:
  1. `./scripts/pre-pr.sh` — the mandatory gate.
  2. `cargo +stable test --workspace --no-fail-fast 2>&1 | grep -E '^---- .+ stdout ----'` — enumerate every failing test binary. Expected output: ONLY `sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` (documented env-only flake). Any other failure name → real regression; do NOT proceed to T017. Reproduce the failure individually via `cargo test -p mikebom --test <name>` and fix.

- [ ] T017 SC-010 wire-format guard verification. Run each guard command from quickstart.md Scenario 10 and confirm the expected empty output:
  ```bash
  git diff main --name-only -- mikebom-cli/src/generate/
  git diff main --name-only -- mikebom-cli/src/parity/
  git diff main --name-only -- docs/reference/sbom-format-mapping.md
  git diff main --name-only -- mikebom-common/ mikebom-ebpf/
  git diff main --name-only -- mikebom-cli/tests/fixtures/golden/
  git diff main --name-only -- mikebom-cli/src/scan_fs/package_db/ | grep -v 'cmake.rs\|^mod.rs$'
  ```
  Each MUST return empty. Also run `git diff main --name-only` and verify the shipped file-list matches the plan.md expected shape (cmake.rs + scan_cmd.rs + 2 caller updates + 5 integration tests + 5 fixture dirs + CHANGELOG.md + CLAUDE.md + spec artifacts).

- [ ] T018 SC-001 manual operator-cadence Kamailio testbed verification per quickstart.md Scenario 1. Clone or point at `/Users/mlieberman/Projects/kamailio`, build release binary `cargo +stable build --release -p mikebom`, run `./target/release/mikebom --offline sbom scan --path /Users/mlieberman/Projects/kamailio --format cyclonedx-json --output cyclonedx-json=/tmp/mikebom-m156/kamailio.cdx.json --no-deep-hash`. Run the SC-001 jq recipes to count cmake-derived components + verify expected names appear. Expected: ≥10 named components (OpenSSL, Libev, NETSNMP, MariaDBClient, LibfreeradiusClient, Radius, Ldap, Unistring, Erlang, Oracle — subset match acceptable per spec Assumption 2). Report PASS/FAIL in the PR comments. If the count is <10, investigate whether Kamailio HEAD has restructured its `cmake/modules/Find*.cmake` files.

- [ ] T019 Update the requirements checklist at `specs/156-cmake-walker-depth/checklists/requirements.md` to mark implementation-completion notes (mirror the milestone-155 pattern) — add sub-bullets for T018 SC-001 result (PASS/FAIL + measured count) and T016 pre-PR gate result (green + only `sbomqs_parity` failing).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1, T001)**: No dependencies. Runs first.
- **Foundational (Phase 2, T002-T003)**: Depends on T001. Sequential (both edit cmake.rs + downstream callers). T002 MUST include the caller updates in the SAME commit-block or the codebase temporarily doesn't compile.
- **User Story 1 (Phase 3, T004-T013)**: All depend on Phase 2 completion. T004-T008 edit `cmake.rs` sequentially (same-file conflict). T007 edits `scan_cmd.rs` (different file — could parallelize with T005-T006 but the plan.md tree keeps it sequential for clarity). T009-T013 create separate fixture directories + separate integration test files — parallel-safe.
- **User Story 2 (Phase 4, T014)**: Depends on Phase 3 completion (needs the recursive walker to run against goldens). Just verification — no new code.
- **Polish (Phase 5, T015-T019)**: T015 can run at any time after T007. T016 MUST run after all US1 + US2 tasks. T017 + T018 + T019 sequential after T016.

### User Story Dependencies

- **US1 (P1)**: Depends on Phase 2. No dependency on US2.
- **US2 (P2)**: Depends on US1 completion (the extended walker must be running against goldens for T014 to observe byte-identity).

### Parallel Opportunities

- **T009-T013 (5 integration test files + 5 fixture dirs)**: fully parallel — different files with no cross-dependencies. If working with a team, these can be authored in parallel.
- **T007 (scan_cmd.rs) + T005 (cmake.rs collect_recursive)**: different files; can parallelize if the developer wants to context-switch. But cleaner as a single serial commit-block.
- **T015 (CHANGELOG) + T009-T013 (fixtures)**: independent files.

---

## Parallel Example: US1 fixture + integration test authoring

Once T008 (unit tests + cmake.rs production code) is done:

```bash
# 5 parallel-safe tasks — each creates its own fixture dir + integration test file:
Task T009: Create SC-003 symlink cycle testbed
Task T010: Create SC-004 depth-3 emission testbed
Task T011: Create SC-005 cross-depth version testbed
Task T012: Create SC-006 exclude-path testbed
Task T013: Create SC-011 third-party opt-in testbed
```

## Parallel Example: Post-implementation polish

Once T014 passes:

```bash
Task T015: CHANGELOG entry (CHANGELOG.md)
```

T016 (pre-PR gate) must run last locally before PR — sequential.

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001, ~5 min baseline capture)
2. Complete Phase 2: Foundational (T002-T003, ~30 LOC signature + helper extraction)
3. Complete Phase 3: US1 (T004-T013, primary deliverable + tests)
4. **STOP + VALIDATE**: Run `cargo test -p mikebom scan_fs::package_db::cmake` and all 5 integration test binaries. Confirm they pass.
5. Optional MVP ship: this alone closes the walker-depth debt. US2 verification is a defensive backward-compat check.

### Incremental Delivery

1. Complete Setup + Foundational → foundation ready.
2. Add US1 → test independently via T009-T013 → SC-003 through SC-006 + SC-011 verified.
3. Add US2 → verify byte-identity via T014 → SC-002 verified.
4. Polish (T015-T019) → run pre-PR gate → open PR.

### Suggested commit shape

Following the project's per-milestone convention (matches milestone 155's shipped chain):

- `spec(156): ...` — spec + clarify session (already committed via /speckit.specify + /speckit.clarify).
- `plan(156): ...` — plan.md + research.md + data-model.md + contracts/ + quickstart.md + CLAUDE.md.
- `tasks(156): ...` — this tasks.md file.
- `impl(156): ...` — T002-T014 production + test code + fixtures.
- `docs(156): ...` — T015 CHANGELOG + T019 checklist update.

Per milestone-155's `feedback_prepr_gate_bails_on_first_failure.md` memory: BEFORE claiming pre-PR gate green, MUST enumerate every `^---- <name> stdout ----` line via `cargo test --workspace --no-fail-fast`.

---

## Notes

- [P] tasks = different files, no dependencies.
- All `cmake.rs` edits are sequential (same file). Parallelism is limited to fixture + integration test authoring.
- Verify tests compile-fail before implementing (TDD): T008 adds tests that reference `discover_cmake_files`'s NEW signature, which doesn't exist until T003 + T006 land. The mod tests block will fail to compile until T006 provides the extended signature.
- Constitution "Pre-PR Verification" is mandatory (T016).
- SC-001 (Kamailio manual scan) requires a Kamailio checkout — the maintainer's `/Users/mlieberman/Projects/kamailio` is the reference testbed.
- Do NOT touch `docs/reference/sbom-format-mapping.md` (no new annotation keys per FR-015; milestone-155's C55 + C103 rows cover everything).
- Do NOT touch `mikebom-cli/src/generate/` or `mikebom-cli/src/parity/extractors/` — SC-010 wire-format guard fails if either changes.
- Do NOT regenerate `mikebom-cli/tests/fixtures/golden/**` — SC-002 byte-identity fails if any regeneration is required.
