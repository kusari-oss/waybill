# Tasks: Read a CMake build tree to settle what a build actually used (#1535)

**Input**: `specs/1652-cmake-build-tree/` (spec.md, plan.md, research.md, data-model.md, contracts/{cli,settlement,annotations}.md, quickstart.md, measurements/)

**Tests**: requested by the spec (SC-002, SC-003, SC-006, SC-007). Each user story's tests are written first and must fail before that story's wiring lands. The control (T006) passes throughout.

**Paths**:
- `BT` = `waybill-cli/src/scan_fs/package_db/cmake/build_tree/` (new)
- `CM` = `waybill-cli/src/scan_fs/package_db/cmake.rs`
- `CD` = `…/cmake/conditions.rs`; `RE` = `…/cmake/reach.rs`; `FE` = `…/cmake/fetches.rs`; `SE` = `…/cmake/searches.rs`
- `PDB` = `waybill-cli/src/scan_fs/package_db/mod.rs`
- `SC` = `waybill-cli/src/cli/scan_cmd.rs`
- `IT` = `waybill-cli/tests/cmake_build_tree.rs` (new); `CF` = `waybill-cli/tests/common/cmake_fetch.rs`
- `FX` = `waybill-cli/tests/fixtures/cmake_build_tree/` (new)
- `MS` = `specs/1652-cmake-build-tree/measurements/`
- Research decisions are R1–R10. Rules are `contracts/settlement.md`'s. Rows are C244 and C245 (`contracts/annotations.md`).

## Phase 1: Setup (measurement, fixtures, control)

- [X] T001 **SC-001(b), the TheRock build-tree census (R10).** Gate: no reader code merges before this task reports.
  - **Workflow.** Add `.github/workflows/census-buildtree.yml` to `kusari-sandbox/waybill-fixture-therock-sdk`:
    - check out `kusari-sandbox/TheRock` at `b1cae390cc745283dce25581415353f9bc906162`, with submodules;
    - configure with every subproject off except a minimal subset. Choose the subset from TheRock's `README.md` and `cmake/therock_features.cmake`: one `therock_subproject_fetch` sysdep (e.g. zlib) and one small library whose own CMake uses FetchContent;
    - build that subset;
    - upload, in the probe's shape, the full listing plus the metadata files `inner.sh` keeps (`find … -printf '%y\t%s\t%p\n'` per build directory).

    Run it in CI only, never locally.
  - **Analysis.** Run `MS/probe/analyze.py`, or a TheRock variant of it, over the artifact. Write `MS/therock.md` with:
    - how many build directories have a `CMakeCache.txt`;
    - whether R1's four FetchContent signals agree;
    - where `therock_subproject_fetch`'s `<name>-download` stamps sit (R3, R7);
    - whether `CMAKE_HOME_DIRECTORY` points into the source checkout;
    - compile-database presence and sizes.
  - **Feasibility.** If the build does not fit on a hosted runner, record why and stop. SC-001 allows proceeding on the probe alone.
  - **If R1–R3 or R7 is contradicted,** amend `spec.md` and `contracts/settlement.md` first.
- [X] T002 [P] Build `main`'s binary before any source change and copy it to the scratchpad as `waybill-main-<sha>`. T006 uses it for the control golden.
- [X] T003 [P] **Fixture generator.** Write `MS/probe/reduce.py`. It takes one probe run directory (from `run.sh`) and writes a reduced fixture under `FX/<config>/`:
  - `source/`: a copy of `MS/probe/project/app/` with `DEPS` rewritten to `@DEPS@`.
  - `build/`, containing only:
    - `CMakeCache.txt`, with `/work/b/<run>` replaced by `@BUILD@`, the app source directory by `@SRC@`, and `/work/prefix-<v>` by `@PREFIX@`;
    - empty marker files that stand for `_deps/<lc>-src/`;
    - every `*-stamp/` file name, as an empty file;
    - `compile_commands.json` with the same rewrites;
    - for the `system` configuration, `.cmake/api/v1/reply/*.json` with the same rewrites.

  Run it on `pip3.31-ninja-{fetch,system,confonly,failed}` from the probe output, and commit `FX/`. Keep each fixture under 100 KB.
- [X] T004 [P] **Mixed-case fixture (R1's gap).**
  - Extend `MS/probe/project/app/CMakeLists.txt` with `FetchContent_Declare(MixedCase URL ${DEPS}/urldep.tar.gz …)` made available, and `ExternalProject_Add(ExtMixed …)`.
  - Re-run `MS/probe/run.sh` (Docker), regenerate `MS/probe-results.md`, and record in `research.md` R1 the case of the observed directory, cache-entry and stamp names.
  - Regenerate `FX/` with T003.
- [X] T005 **Test scaffold.** Create `IT`:
  - `materialise(config) -> (TempDir, src, build)`: copies `FX/<config>/` into a tempdir and substitutes `@SRC@`, `@BUILD@`, `@PREFIX@` and `@DEPS@` with real paths inside it. `@PREFIX@` gets an `include/hdronly/hdronly.hpp` and `lib/cmake/hdronly/hdronlyConfig.cmake`.
  - `scan_bt(src, &[build…])`: calls `CF::scan(fx, path, &["--cmake-build-tree", …])` for all three formats.
  - `settlement(doc, name) -> serde_json::Value`: reads C244.
  - `coverage_bt(doc)`: reads C245.
  - `parity()` over C122, C232, C244, C245, plus the native scope, using the `cmake_conditional_scope.rs:95-112` pattern; `assert_repeatable` (10 scans) and the no-host-path assertion are added later.
- [X] T006 **Control (FR-001, SC-004).**
  - Generate `FX/ctl-golden/{cdx,spdx-2.3,spdx-3}.json` by scanning `FX/fetch/source` **without** `--cmake-build-tree`, using T002's binary. Use an env var `WAYBILL_UPDATE_BUILD_TREE_GOLDENS` and binary override `WAYBILL_BUILD_TREE_BIN`, as `cmake_conditional_scope.rs:50-53` does.
  - The test `source_only_is_byte_identical_to_main` passes now, and must keep passing.

## Phase 2: Foundational (evidence loading; no effect on output yet)

- [X] T007 [P] **Cache parser.** In `BT/cache.rs`: `CacheView::parse(&str) -> Result<CacheView, CacheError>`, with `CacheKind` and `CacheEntry` per data-model.md. Rules:
  - `NAME:TYPE=VALUE` splits at the first `:` and then the first `=`;
  - `#` and `//` lines are skipped;
  - `NAME=VALUE` with no type is `Untyped`.

  `#[cfg(test)]` unit tests: the probe's `CMakeCache.txt` lines from `FX/fetch/build`, a value containing `=` and `:`, CRLF line endings, and an empty file.
- [X] T008 [P] **Path redaction.** In `BT/redact.rs`: `Redactor::new(trees, scan_root, prefixes)` and `redact(&Path) -> RedactedPath`, giving `build:`/`build<N>:`, `<rel>`, `<prefix>/<rel>` or `<external>` (R8). The newtype has no `From<String>`. Unit tests cover:
  - each form;
  - a symlinked root;
  - a canonical vs non-canonical root (reuse the logic of `installed_lib/location.rs:17` `relative_to_root`);
  - Windows separators normalised to `/`.
- [X] T009 [P] **Discovery.** In `BT/discover.rs`: `discover(tree: &Path, exclude_set) -> Vec<PathBuf>`. It finds every directory holding a regular `CMakeCache.txt`, via `scan_fs::walk::safe_walk` with `should_skip` that does **not** apply `should_skip_default_descent`, so it descends into `build/`, `out/` and dot-directories. It drops caches inside another build directory's `_deps/` (contract, matching rule 1). Unit tests: nested `_deps/*-subbuild` caches are excluded; two sibling subproject builds are both found; the depth bound holds.
- [X] T010 **Evidence loading.** In `BT/mod.rs`, add `BuildEvidence::load(trees: &[PathBuf], scan_root: &Path, exclude_set) -> BuildEvidence`. For each discovered directory it reads:
  - the cache (`home` from `CMAKE_HOME_DIRECTORY`);
  - `populations`: directory names `_deps/*-src`;
  - `stamps`: files named `*-download` / `*-mkdir` and directories `*-stamp`, found by name (R3), excluding nested build directories;
  - `compile_includes` (raw paths only; resolution is US3). *Deferred to US3 (T028): nothing reads it before then.*

  Fill `BuildTreeCoverage { build_trees, build_dirs, unreadable_files, source_mismatch }`. Read errors are counted, never fatal (FR-008). Declare the module in `…/cmake.rs` (`mod build_tree;`). Stamp files are tested for existence only (R9).
- [X] T011 **The flag** (`contracts/cli.md`). In `SC`:
  - add `#[arg(long = "cmake-build-tree", value_name = "PATH", action = clap::ArgAction::Append)] pub cmake_build_tree: Vec<PathBuf>` to `ScanArgs` (`SC:418`), next to `cmake_third_party_recursive` (`SC:1320`);
  - in `execute`, next to `SC:3346-3350`: reject the flag with `--image` (exit 2); reject a path that does not exist or is not a directory (exit 2, message names the flag); export `WAYBILL_CMAKE_BUILD_TREES` via `std::env::join_paths`;
  - CLI tests in `IT` for both rejections.
- [X] T012 **Load the evidence and thread C245.**
  - In `CM::finalize` (`CM:198`), read `WAYBILL_CMAKE_BUILD_TREES` and call `BuildEvidence::load` once, before `read_with_downloads`. Pass `Option<&BuildEvidence>` to `read_with_downloads` and on to `fetches::read_declarations`; it is unused until US1. Return the coverage wire string alongside `FetchCoverage`.
  - Add `ScanDiagnostics.cmake_build_tree_coverage: Option<String>` next to `cmake_fetch_coverage` (`PDB:407`), set where C231 is set (`PDB:2503`).
  - Thread it the way C231 is threaded:
    - `scan_fs/mod.rs:162`, `:579`, `:1787` (`ScanResult`);
    - `SC:3938` and `:6143`;
    - `generate/mod.rs:60/175` (`ScanArtifacts`);
    - CycloneDX `generate/cyclonedx/metadata.rs:216/319/569`, `builder.rs:213/317/474/1000`, `cyclonedx/mod.rs:191`;
    - SPDX 2.3 `generate/spdx/annotations.rs:717`;
    - SPDX 3 `generate/spdx/v3_annotations.rs:729`;
    - `None` in the other `ScanArtifacts` constructors listed in plan.md.
  - It is emitted only when the flag is given.
- [X] T013 [P] **Catalog rows.** Add C244 `waybill:cmake-build-settlement` (component) and C245 `waybill:cmake-build-tree-coverage` (document):
  - `cdx_anno!` in `parity/extractors/cdx.rs`, `spdx23_anno!` in `spdx2.rs`, `spdx3_anno!` in `spdx3.rs`;
  - `ParityExtractor` entries (`SymmetricEqual`, `order_sensitive: false`) in `parity/extractors/mod.rs` after C243;
  - rows in `docs/reference/sbom-format-mapping.md` after C243, with the Principle V audit from `contracts/annotations.md`.

  `every_catalog_row_has_an_extractor` (`parity/extractors/mod.rs:1172`) and `tests/sbom_format_mapping_coverage.rs` must pass.

**Checkpoint**: with the flag, C245 is emitted with `build-dirs` > 0 on `FX/fetch`. Without it, T006 is byte-identical.

## Phase 3: User Story 1 - A fallback's outcome is stated, not guessed (P1) 🎯 MVP

**Goal**: every CMake fetch declaration is settled. A *taken* declaration is in scope. Build-only populations are emitted.
**Independent test**: `FX/fetch` vs `FX/system`. `hdronly` is *taken* in the first and *not taken* in the second, in all three formats.

### Tests (write first; must fail before T018)

- [X] T014 [US1] Add to `IT`, one test per row of `contracts/settlement.md`:
  - **FetchContent taken** (`FX/fetch`, `hdronly` with `FIND_PACKAGE_ARGS`): in scope; no C122; C244 `state=taken`, basis ⊇ {`cache-source-dir`, `deps-src-dir`}.
  - **FetchContent not taken** (`FX/system`, `hdronly`): scope and C122 unchanged from the source-only scan; C244 `state=not-taken`, basis {`cache-package-dir`}.
  - **No evidence** (`neverused` in every configuration): unchanged; C244 `{"state":"no-evidence"}`.
  - **ExternalProject taken** (`extproj` in `FX/fetch`): basis {`download-stamp`}.
  - **Incomplete** (`extproj` in `FX/confonly` and `FX/failed`): C244 `state=incomplete`, scope unchanged.
  - **Override**: `FX/fetch` with `FETCHCONTENT_SOURCE_DIR_URLDEP` set to a path in the materialised cache. *Taken*, basis includes `source-dir-override`, and the path is absent from all outputs.
  - **Build-only** (FR-007): an extra empty `_deps/orphan-src/` in a copy of `FX/fetch/build`. One component, in scope, C244 `build-only: true`.
  - **Two build trees** (FR-002): `FX/system/build` and `FX/fetch/build`, both named, both with `home` = the same source. `hdronly` is *taken*; C245 `conflicts` = 1.
  - **Test outranks taken**: a fetch under `if(BUILD_TESTING)` that is populated keeps Test scope.
- [X] T015 [US1] Add to `IT`:
  - `no_host_paths` (SC-006): for every scan above, no string in any format contains the tempdir path, its canonical form, or `$HOME`. Pattern: `tests/binary_source_paths.rs:94-104`.
  - `repeatable`: 10 scans byte-identical after masking.

### Implementation

- [X] T016 [US1] **FetchContent rules.** In `BT/settle.rs`, `settle_fetchcontent(name, &BuildDir) -> (SettlementState, BTreeSet<EvidenceKind>)`, following `contracts/settlement.md`:
  - lower/upper-case keys per T004's finding;
  - a `<lc>_DIR` value ending in `CMakeFiles/pkgRedirects` is not *not taken*;
  - a non-empty `FETCHCONTENT_SOURCE_DIR_<UC>` gives `SourceDirOverride`;
  - a disagreement among R1's signals bumps `signal_disagreements`.

  Unit tests use synthetic `BuildDir`s.
- [X] T017 [US1] **ExternalProject rules and combining.** In `BT/settle.rs`:
  - `settle_externalproject(name, &BuildDir)`: *taken* on `<name>-download`; *incomplete* on `<name>-mkdir` or a `<name>-stamp` directory alone.
  - `owners(decl_file, &BuildEvidence) -> Vec<&BuildDir>`: the deepest canonical `home` ancestor.
  - `settle(kind, name, decl_file, &BuildEvidence) -> Settlement`: precedence Taken > NotTaken > Incomplete > NoEvidence; differing proposals bump `conflicts`.

  Unit tests: TheRock-style moved `PREFIX` (stamp under `<target>_fetch/src/<target>-stamp/`), and two owners.
- [X] T018 [US1] **Wire settlement into `FE`.** In `read_declarations` (`FE:164`), before `mark()` (`FE:491`), for each declaration call `settle(kind, &name, path, ev)` when `ev` is `Some`. On *taken*:
  - the shape is discarded;
  - `lifecycle_scope` becomes `None` unless the declaration is Test (`FE:354-360`);
  - C122 is not written.

  For every state, write C244 (sorted-key JSON; `builds` as `RedactedPath`) into `extra_annotations`, and add to `BuildTreeCoverage.declarations`. Download wrappers and `download_project()` declarations (`wrappers.rs:388`) go through the same path.
- [X] T019 [US1] **Merging.**
  - In `one_per_purl` (`FE:565`): a merged component is *taken* if any merged declaration is; C244 states merge by precedence; `basis` and `builds` are unioned.
  - In `merge_into_existing` (`FE:680`): the "conditional" predicate (`FE:690-695`) is false for a *taken* declaration, so a taken fetch is not dropped into another component as a conditional one.

  Add a test in `IT` with two declarations of the same PURL, one *taken*.
- [X] T020 [US1] **Build-only components.** In `CM::finalize`, after the fetch entries are appended (`CM:385`), for each `BuildDir` population or `<name>-download` stamp that no declaration owned by that build directory matched:
  - append an entry built by the unresolved-fetch path (`CM:1192`, `fetch_entry` at `CM:1135`), with source mechanism `cmake-fetchcontent` or `cmake-externalproject`;
  - give it C244 `build-only: true` and in-scope status;
  - count it in `build_only`.

  No info files are read (R4).
- [X] T021 [US1] **Invariants and logging.** *Done in `IT::assert_bt_invariants`, which every `scan_bt` call runs; `CF` is shared with suites that never pass the flag.*
  - Extend `CF::assert_invariants` (`CF:106`): when C245 is present, `sum(declarations)` = the number of C244 values with `build-only: false`, and `build-only` = the number with `true`.
  - In `BT/mod.rs`, add the INFO summary on target `waybill::cmake_build_tree`, using redacted paths only.

**Checkpoint**: T014 and T015 pass; T006 still passes; the three formats agree.

## Phase 4: User Story 2 - Conditions are evaluated against the configuration that was built (P2)

**Goal**: configured cache values replace source defaults in spec 1641's evaluator, for fetches and package searches. String comparisons are evaluated (#1656).
**Independent test**: `FX/system` puts `extra` (`WITH_EXTRA=ON`) and `bdep` (`BACKEND STREQUAL "B"`) in scope; `FX/fetch` leaves both optional with `cmake-conditional`.

### Tests (write first; must fail before T024)

- [ ] T022 [US2] Add to `IT`:
  - **Default OFF, configured ON**: `extra` in `FX/system` is in scope; C245 `configured-values-applied` ≥ 1.
  - **Cache string**: `bdep` in `FX/system` is in scope; in `FX/fetch`, optional with C122 `cmake-conditional`.
  - **Still unknown** (`if(DEFINED SOMETHING)`): in scope, per spec 1641 FR-004.
  - **Conflicting builds**: two named trees that configure `BACKEND` differently. `bdep` is *unknown*, so in scope.
  - **Package search**: a `find_package(Foo)` under `if(WITH_EXTRA)` follows the configured value (C236).
  - **Without the flag**: `cmake_conditional_scope.rs` is unchanged (run it).

### Implementation

- [ ] T023 [US2] **Overlay type.** In `CD`, add `ConfiguredOverlay { values: HashMap<String, Configured> }` (`Known(String) | Conflicting`) and `Defaults::with_overlay(self, &ConfiguredOverlay) -> Defaults`. In `Defaults::lookup` (`CD:247`), an overlay value is checked before the source defaults:
  - a boolean reading uses CMake truthiness (data-model.md);
  - `Conflicting` reads as `Unknown`.

  Only `Bool`, `String`, `Uninitialized` and `Untyped` cache entries feed it. Unit tests cover each truthiness case and the precedence.
- [ ] T024 [US2] **Comparisons.** In `Parser::compare` (`CD:470-485`), when both operands are known literals or overlay values (or one is a quoted literal), evaluate:
  - `STREQUAL`, `EQUAL`;
  - `MATCHES`: translate the CMake regex to `regex::Regex` only when it uses the shared syntax subset; otherwise `Unknown`;
  - `VERSION_LESS`, `VERSION_GREATER`, `VERSION_EQUAL`, `VERSION_LESS_EQUAL`, `VERSION_GREATER_EQUAL`, with CMake's component-wise integer comparison.

  Platform and `Standalone` handling are unchanged. Unit tests cover each operator, plus an unknown operand.
- [ ] T025 [US2] **Applying the overlay.** In `RE::defaults_for` (`RE:127`), apply the overlay of the deepest-owning build directory (`BT/settle.rs::owners`). Pass `Option<&BuildEvidence>` into `ReachMap` from `CM::finalize`. Build each overlay once per `BuildDir`. Several owners with different values give `Conflicting`.
- [ ] T026 [US2] **Searches.** In `SE::read_file` (`SE:389-396`), replace `Defaults::default()` with the overlay for the file's owning build directory when evidence is present. Count `configured_values_applied` in `CD`/`FE` whenever a chain's result with the overlay differs from its result without it. Do not recount through the census tracing at `FE:470`.

**Checkpoint**: T022 passes; T014 and T006 still pass.

## Phase 5: User Story 3 - Header-only and build-only dependencies appear (P3)

**Goal**: include directories from `compile_commands.json` give edges from the compiling project to known components.
**Independent test**: in `FX/fetch`, the app's project component has an edge to `hdronly`, which is header-only. In `FX/system`, the include under `@PREFIX@/include` resolves through `hdronly_DIR`.

### Tests (write first; must fail before T029)

- [ ] T027 [US3] Add to `IT`:
  - **`FX/fetch`**: edge `app-project → hdronly` in CycloneDX `dependencies`, SPDX 2.3 `DEPENDS_ON` and SPDX 3; C244 basis includes `compile-include`; C245 `compile-edges` ≥ 1.
  - **`FX/system`**: same edge via the prefix.
  - **No compile database** (delete it from a copy): no compile edges; C245 `compile-edges` = 0; no failure.
  - **Unresolved include directory** (`/usr/include`-style): counted in `include-dirs-unresolved`, not emitted.
  - **Scope is unchanged by compile evidence** (FR-009): an optional, *not taken* component compiled against keeps its scope.

### Implementation

- [ ] T028 [P] [US3] **Parsing.** In `BT/compile_db.rs`, `include_dirs(json: &str) -> Result<BTreeSet<PathBuf>, _>`. It reads `-I`, `-I <dir>`, `-isystem`, `-iquote`, `/I` (MSVC), and both the `command` and `arguments` forms. Relative directories are resolved against each entry's `directory`. Unit tests use the probe's `compile_commands.json`.
- [ ] T029 [US3] **Resolution.** In `BT/compile_db.rs`, `resolve(dir, &BuildDir, components) -> Option<Purl>`, trying in order:
  1. under `_deps/<lc>-src/`: that declaration's component;
  2. under an ExternalProject source directory (sibling of its stamp directory): that component;
  3. under a `<name>_DIR` install prefix (`<prefix>/include`): the component whose name matches `<name>`;
  4. under a git submodule path or an installed-library prefix the scan already has a component for;
  5. otherwise unresolved.
- [ ] T030 [US3] **Edges.** In `CM::finalize`, for each `BuildDir`, add a `Relationship` from spec 1618's CMake project component for `home` (C55 `cmake-project`) to each resolved component. If there is no project component, attach to the main module as spec 1618 does. Add `compile-include` to the target's C244 basis, without changing its state or scope. Count `compile_edges` and `include_dirs_unresolved`.

**Checkpoint**: T027 passes; T014 and T022 still pass.

## Phase 6: User Story 4 - The file API is used when present (P4)

**Goal**: a file-API reply, when present, is preferred over `CMakeCache.txt` and the compile database.
**Independent test**: `FX/system` (it has a reply) gives the same settlement and edges with the reply as without it, and C245 `file-api-replies` = 1.

- [ ] T031 [US4] Add to `IT`:
  - `FX/system` with the reply: C245 `file-api-replies` = 1.
  - The reply's `cache-v2` value differs from `CMakeCache.txt` (edit the reply copy, setting `BACKEND` to `A`): the reply wins.
  - Link edges match `codemodel-v2` target `dependencies`/`link` data.
- [ ] T032 [P] [US4] **Reading the reply.** In `BT/file_api.rs`: `FileApiReply::load(build_dir)` reads the newest `index-*.json`, then the referenced `cache-v2` and `codemodel-v2` JSON files. Malformed files are counted in `unreadable-files`. Expose `cache_entries()` and `target_includes()` (per target, from `compileGroups[].includes[].path`).
- [ ] T033 [US4] **Preference.** In `BT/mod.rs`, when a reply is present, `CacheView` is built from `cache_entries()` and `compile_includes` from `target_includes()`; the files are the fallback. waybill writes no query (FR-010).

## Phase 7: Polish & cross-cutting

- [ ] T034 [P] **Docs (FR-014).**
  - `docs/user-guide/cli-reference.md`: `--cmake-build-tree`, from `contracts/cli.md`, including the super-build and multi-stage advice in quickstart.md.
  - `docs/reference/reading-a-mikebom-sbom.md`: C244 and C245 entries next to C232 (`:1298`), with quickstart.md's `jq` recipes.
  - `docs/ecosystems.md:153`: the CMake section gains one paragraph on build-tree settlement.
  - Note in the docs that a file-API query placed before configure gives the most precise evidence.
- [ ] T035 [P] **Mutation check (SC-007).** Write `MS/mutate.py`, modelled on `specs/1641-cmake-conditional-scope/measurements/mutate.py`. It removes each rule in turn:
  - FetchContent taken;
  - FetchContent not taken;
  - ExternalProject taken;
  - incomplete;
  - overlay precedence;
  - string comparison;
  - build-only;
  - compile resolution.

  For each, it runs `cargo test -p waybill --test cmake_build_tree` and asserts that at least one test fails. Record the results in `MS/mutations.md`.
- [ ] T036 **Performance (SC-008).** Median of 3 runs, source+build vs source-only, on `FX/fetch` and on T001's TheRock tree if it was captured. Record in `MS/perf.md`. The bound is ≤ 1.10×.
- [ ] T037 **TheRock before/after census (SC-005).** If T001 captured a tree, extend `census-buildtree.yml` to scan it at `main` and at this branch. Report:
  - fallback and conditional components settled, by state;
  - how many of them are versioned;
  - build-only count and C245.

  Record in `MS/therock.md`. Make no prediction beforehand.
- [ ] T038 **Corpus byte-identity (SC-004).** Dispatch `public-corpus.yml` on the branch. Goldens must be unchanged, because no target passes `--cmake-build-tree`. Run `xtask quality` against a `main` control; component counts and C231 must be equal.
- [ ] T039 **Walker audit.** Confirm `BT/discover.rs` uses `safe_walk` and needs no entry in `scan_fs/walk.audit-allowlist.txt`. If the audit CI step flags it, add the rationale to `scan_fs/walk.audit-allowlist.rationale.md`.
- [ ] T040 **Pre-PR gate** *(passed for the MVP, Phases 1–3, on 2026-11-24: clippy `GATE_RESULT=pass`; tests 435 targets ok, 11,416 passed, 0 failed. The walker-audit step was run under Linux bash 5 in Docker because macOS `/bin/bash` 3.2 cannot parse `scripts/check-walker-audit.sh:87`; that is pre-existing and unrelated. Re-run after US2–US4.)*, default features, no `+toolchain`: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Check the gate's own exit status and its positive line, not a pipeline's.

## Dependencies & Execution Order

- **T001 gates merging**, not writing. Phase 2 can start in parallel, but no PR with reader code merges until `MS/therock.md` exists, and the spec is amended if needed.
- **Setup:** T002, T003 and T004 run in parallel. T004 re-runs T003. T005 depends on T003; T006 depends on T002 and T005.
- **Foundational:** T007, T008 and T009 run in parallel. T010 depends on all three. T011 and T013 run in parallel with T010. T012 depends on T010 and T011.
- **US1:** depends on Phase 2. Tests T014–T015 come first. Then T016 ∥ T017 → T018 → T019 → T020 → T021.
- **US2:** depends on US1's `owners()` (T017) and T012. T022 first, then T023 → T024 → T025 → T026. It is independent of US3 and US4.
- **US3:** depends on T010 and T017. T027 first, then T028 → T029 → T030.
- **US4:** depends on US1 and US3, because it substitutes their inputs. T031 first, then T032 → T033.
- **Polish:** after the stories it covers. T035 needs every story; T037 needs T001.

## Parallel example: User Story 1

```text
After T014 and T015 are written and failing:
  T016 settle.rs FetchContent rules      ∥   T017 settle.rs ExternalProject rules + owners()
then sequentially: T018 (fetches.rs) → T019 (merge) → T020 (build-only) → T021 (invariants, log)
```

Phase 2: T007 (`cache.rs`) ∥ T008 (`redact.rs`) ∥ T009 (`discover.rs`) ∥ T013 (catalog rows).

## Implementation strategy

- **MVP = Phases 1–3 (US1).** It answers #1519's open question: which fallbacks fired. It needs only a configured tree for FetchContent (R1), so it is useful even without a full build. Ship it as one PR after T001 reports.
- **Then US2** (closes #1656 for scanned builds), **then US3**, **then US4**, each as its own PR with its own tests, re-running T006 and T038.
- Every PR runs T040, and a `public-corpus.yml` dispatch (the goldens are `#[ignore]`d locally).
