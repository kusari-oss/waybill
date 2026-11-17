# Tasks: CMake fetches in nested projects, with standalone-build fallbacks marked

**Input**: `specs/1538-cmake-nested-fetches/` (`spec.md`, `plan.md`, `research.md`, `data-model.md`, `contracts/wire.md`, `quickstart.md`, `measurements/`)

**Tests are required.** The spec defines contract cases C-1 to C-15 and invariants I-1 to I-4, and plan.md names unit, contract, parity and mutation tests. Each story's tests are written first, and must fail at `main` before the code that passes them.

**Conventions:**
- Synthetic `waybill-fixture-*` names only.
- No new Cargo dependency (FR-011).
- TheRock in CI only.
- Revert mutations with `cp`, never `git checkout`.
- Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`.
- Commits carry `Assisted-by: Claude:claude-opus-5-5`.

## Phase 1: Setup

- [X] T001 Census TheRock's fetch declarations in CI, never locally: `census/fetches.py` and `.github/workflows/census-fetches.yml` in `kusari-sandbox/waybill-fixture-therock-sdk` (`c9001e9`, run 44005855500). Record the counts in `specs/1538-cmake-nested-fetches/measurements/census.md` (SC-007, SC-008).
- [X] T002 Probe CMake's handling of each fallback shape with the installed CMake 4.4.5: `specs/1538-cmake-nested-fetches/measurements/cmake-semantics/probe.sh`, with `results.txt` and `cmake-version.txt` (research R3).
- [X] T003 Extend `.github/workflows/census-fetches.yml` in the fixture repository with a `base_ref` input:
  - build `base` and `branch` side by side, and scan the sparse tree with each, 3 times;
  - write both CycloneDX documents and `times.txt` to the artifact.

  SC-006 then compares the two in one job. Validate by dispatching with `base_ref = waybill_ref = main`: both medians must agree within noise, which is the determinism control.

## Phase 2: Foundational (blocks every story)

- [X] T004 [P] *Done as reuse (research R15):* the existing `installed_lib::cmake_version` tokeniser, plus `commands_checked`, which reports a command still open at end of file; no new file. Original text: create a CMake command tokeniser, `waybill-cli/src/scan_fs/package_db/cmake/commands.rs`, declared from `cmake.rs`.
  - **Output:** `commands(text) -> impl Iterator<Item = Command { name (lowercased), args, line }>`.
  - **Behaviour to port from `census/fetches.py`'s `commands()`:** skip `#` line comments and `#[=*[ … ]=*]` bracket comments; inside arguments, respect quoted strings with `\` escapes, bracket arguments and nested parentheses; report 1-based start lines.
  - **Unterminated input:** a command or bracket argument still open at the end of a file is reported as unterminated, for C231 `unparseable-files`.
  - **Unit tests:**
    - a commented-out declaration is not a command;
    - `"a)b"` inside arguments does not close the call;
    - a bracket argument containing `)` does not close the call;
    - line numbers after a multi-line call are right;
    - mixed-case command names are lowercased.
- [X] T005 [P] Create the block stack, `waybill-cli/src/scan_fs/package_db/cmake/blocks.rs`.
  - **Blocks:** `enum Block { If { condition }, Function { name }, Loop }`.
  - **Transitions:** `if` pushes; `elseif` replaces the top condition; `else` prefixes it with `else: `; `endif` pops; `function`/`macro` push `Function`; `foreach`/`while` push `Loop`.
  - **Robustness:** an unbalanced `end*` is ignored, and an unclosed block at the end of a file is counted, not fatal.
  - **Output:** `conditions()` returns the enclosing conditions outermost first, with `function: <name>` for function bodies.
  - **Unit tests:** nesting, `elseif`/`else` text, a function inside an `if`, and unbalanced input.
- [X] T006 [P] Create `FallbackShape`, `waybill-cli/src/scan_fs/package_db/cmake/fallback.rs`.
  - **The enum:** a closed enum with `as_str()` giving the C122 values (`cmake-find-package-args`, `cmake-not-target`, `cmake-find-package-fallback`, `cmake-try-find-package-always`, `cmake-dependency-provider`, `cmake-function-body`, `cmake-conditional`). Its order is the precedence (data-model).
  - **The classifier:** `classify(kind, args, conditions, mode_in_force, provider_fetchcontent) -> Option<FallbackShape>`.
    - `FIND_PACKAGE_ARGS` is cancelled when the mode in force is `NEVER` (R3 P4).
    - `TryFindPackageAlways` and `DependencyProvider` apply to FetchContent only (R3 P5).
    - `NOT <X>_FOUND` is matched by `\bNOT\s+\S+_FOUND\b`.
  - **Mode scope:** a helper resolves the `FETCHCONTENT_TRY_FIND_PACKAGE_MODE` in force for a directory, as the last `set()` in the nearest directory, else an ancestor.
  - **Provider:** a helper decides whether a `cmake_language(SET_DEPENDENCY_PROVIDER … SUPPORTED_METHODS …)` lists `FETCHCONTENT_MAKEAVAILABLE_SERIAL`.
  - **Unit tests:** one per shape and per cancellation, plus ExternalProject never matching the FetchContent-only shapes.
- [X] T007 Make spec 1537's `OwnerIndex` reusable for any relative path, in `waybill-cli/src/scan_fs/declared_attach.rs`: `pub(crate)` on the type, `build` and `owner_of`, with no behaviour change. The existing `declared_attach` unit tests and `tests/declared_requirements.rs` must stay green.
- [X] T008 Thread a document-scope `cmake_fetch_coverage: Option<String>` (C231), mirroring C229 `declared_downloads` site for site.
  - **Where it comes from:** the field on `ScanResult` (`waybill-cli/src/scan_fs/mod.rs`) and its destructuring in `waybill-cli/src/cli/scan_cmd.rs`.
  - **The emit path:**
    - `generate::ScanArtifacts` in `waybill-cli/src/generate/mod.rs`;
    - CycloneDX in `generate/cyclonedx/{builder.rs,metadata.rs,mod.rs}`;
    - SPDX 2.3 in `generate/spdx/annotations.rs`;
    - SPDX 3 in `generate/spdx/v3_annotations.rs`.
  - **Every struct literal** that sets `declared_downloads: None` also sets `cmake_fetch_coverage: None`: `openvex/mod.rs`, `spdx/{packages,relationships,document,mod}.rs`, and any others a compile error names.
- [X] T009 Add rows C231 `waybill:cmake-fetch-coverage`, C232 `waybill:cmake-fetch-conditions` and C233 `waybill:superseded-declarations` to `docs/reference/sbom-format-mapping.md`, each with a Principle V audit (research R4, R5, R8).
  - **Same file:** add the new C122 values and the C62 values `source-test-directory` and `cmake-build-testing`.
  - **Extractors, in the same commit:** the catalog gate is in the lib target. Add `c231_*`, `c232_*` and `c233_*` to `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`, registered in `parity/extractors/mod.rs` as `SymmetricEqual`, `order_sensitive: false`.

**Checkpoint:** the gate's lib target passes, and nothing emits the new rows yet.

## Phase 3: User Story 1 — fetches in nested projects appear (P1) 🎯 MVP, lands with US2

**Goal:** every fetch declaration in any CMake file is read and emitted, owned by its nearest enclosing project, and counted.
**Independent test:** contract cases C-1, C-2 and C-14, plus invariants I-2 and I-4.

- [X] T010 [US1] Create `waybill-cli/tests/cmake_nested_fetches.rs`.
  - **Helpers:** `scan(fixture, extra_args) -> Docs` in all three formats, mirroring `tests/declared_requirements.rs`; per-format component and root-target extractors; `coverage(docs)`, which reads C231 and asserts I-2's V1 identity.
  - **Cases C-1, C-2, C-14 and C-15** (`contracts/wire.md`). C-15 checks that the skip set and `--exclude-path` still apply to the widened walk.
  - **I-4:** a CMake tree with no fetch carries no C231, C232 or C233.
  - **Prove they bite:** run against `main`'s reader. C-1, C-2 and C-14 must fail; C-15 is a control and passes. Record which, in this task's text.
  - **Done:** before the implementation, all 15 contract cases failed and I-4 passed. C-15 failed only on the missing C231, not on what was read.
- [X] T011 [US1] Split discovery in `waybill-cli/src/scan_fs/package_db/cmake.rs`.
  - **Fetch declarations** are read from every `CMakeLists.txt` and `*.cmake` the scan reaches. Use the existing `safe_walk` with the exclusion set and the walker's skip set, as `collect_cmake_files_recursive` does.
  - **`find_package` / `pkg_check_modules` / `pkg_search_module`** stay on `discover_cmake_files`' set (FR-002). `discover_cmake_files`' callers for submodule references (C57, line ~600) keep their set.
  - **One pass per file:** use `cmake::commands` and `cmake::blocks` instead of whole-file regexes for the fetch calls. The existing `FetchContent`/`ExternalProject` argument parsing, PURL building and same-file `set()` resolution (#1517) are reused unchanged on each command's arguments.
  - **Each `PackageDbEntry`** carries its declaring file, line and conditions.
- [X] T012 [US1] Create `waybill-cli/src/scan_fs/cmake_fetch_attach.rs`, wired after `declared_attach` in `waybill-cli/src/scan_fs/mod.rs`.
  - **Ownership:** for each fetch component, `OwnerIndex::owner_of(file)` gives `owner DependsOn fetch` (provenance `data_type: "cmake-fetch-declaration"`).
  - **No owner:** the fetch becomes a declared root anchor (spec 1537 channel).
  - **No duplicates:** no self-edge, and no edge that already exists.
- [X] T013 [US1] Compute C231 in `cmake_fetch_attach.rs`.
  - **The counts:** `read`, `emitted`, `unresolved.identity` (a `${…}` name or URL with no same-file literal `set()`), `unparseable-files`, and zero-filled `merged.*`, `conditional` and `test` until US2 and US3. `unparseable-files` counts I/O or non-UTF-8 errors, and files that end inside an open command or bracket argument; the tokeniser reports the latter (T004).
  - **Sorted keys**, encoded as a string.
  - **Absent when `read == 0`** (I-4).
  - Thread it into `ScanResult.cmake_fetch_coverage` (T008).
  - **Unit test:** V1 holds on a mixed fixture.
- [X] T014 [US1] **The control for FR-010** is the existing CMake regression goldens: `cdx_regression_cmake`, plus the SPDX 2.3 and SPDX 3 `cmake` cases, in `tests/{cdx,spdx,spdx3}_regression.rs`.
  - They MUST stay byte-identical, except for C231, and except for the conditional marker on any of their fetches inside an `if()` (FR-010).
  - Diff any change field by field, and explain it in this task's text before regenerating.
  - The existing `cmake` unit tests run unchanged.
  - **Done:** CycloneDX, SPDX 2.3 and SPDX 3 each gained only the C231 document entry. In CycloneDX it is `read` 3 = `emitted` 3, with no conditionals. Components, scopes and every other field are identical. The goldens are regenerated, and 85 `cmake` unit tests plus 18 CMake-related integration files pass.

**Checkpoint:** C-1, C-2, C-14, I-2 and I-4 pass. With no US2, nested fallbacks are emitted as *required*. That over-reports, which is why US1 and US2 land together.

## Phase 4: User Story 2 — standalone-build fallbacks marked (P1)

**Goal:** a conditional fetch is Optional, with C122 naming the shape and C232 the condition text. Test-scoped fetches are Test.
**Independent test:** contract cases C-3, C-3b, C-4 to C-9 and C-13, plus invariant I-3.

- [X] T015 [US2] Add contract cases C-3, C-3b, C-4, C-5, C-6, C-7, C-8, C-9 and C-13 to `waybill-cli/tests/cmake_nested_fetches.rs`, plus an `assert_i3(docs)` helper: every conditional fetch component has C122 and C232, and no unconditional one has C232. Confirm each case fails after US1 alone.
- [X] T016 [US2] Classify each declaration in `cmake.rs`.
  - **The inputs to `fallback::classify`:** the block-stack conditions, the declaration's arguments, the mode in force for its directory (a pre-pass over all files collects `set(FETCHCONTENT_TRY_FIND_PACKAGE_MODE …)`), and whether any file defines a FetchContent-capable provider.
  - **On `Some(shape)`:** `lifecycle_scope = Optional`, `waybill:optional-derivation = shape.as_str()` (the `conan.rs:271` pattern), and `waybill:cmake-fetch-conditions` = the JSON array of conditions.
  - **`function:` entries:** a declaration in a function body gets `FunctionBody` and is emitted only if its identity is literal (R13).
- [X] T017 [US2] Test scope in `cmake.rs` (R7).
  - **Which fetches:** a declaring file under a `test`, `tests` or `unittests` directory below the scan root, or an enclosing condition containing `BUILD_TESTING` that is not inside `OR`.
  - **What it gets:** `LifecycleScope::Test`, with `waybill:lifecycle-scope-derivation` = `source-test-directory` or `cmake-build-testing`. C232 is kept when a condition applies.
  - **Test outranks Optional.**
- [X] T018 [US2] Combine same-PURL declarations in `cmake_fetch_attach.rs`, before the cross-reader dedup.
  - **Scope:** a component is Optional only if **every** declaration is conditional; otherwise its scope is the strongest (feature 1269's rule).
  - **C232:** the sorted union of conditions, dropped if the result is unconditional.
  - **C18:** the union of files.
  - **Unit test:** a conditional and an unconditional declaration of one PURL give a required component with no C232.
- [X] T019 [US2] C231's `conditional` and `test` counts (V2). Extend the T013 unit test.

**Checkpoint:** C-1 to C-9, C-13 and C-14 pass, with I-1 to I-4. This is the MVP: US1 + US2.

## Phase 5: User Story 3 — one component when the super-project supplies it (P2)

**Goal:** first-declaration-wins and existing-component merges, recorded in C233.
**Independent test:** contract cases C-10, C-11 and C-12.

- [X] T020 [US3] Add contract cases C-10, C-11 and C-12 to `waybill-cli/tests/cmake_nested_fetches.rs`, and confirm C-10 and C-12 fail after US2.
- [X] T021 [US3] First declaration wins (R5), in `cmake_fetch_attach.rs`.
  - **Which declarations:** for FetchContent only, a declaration whose lowercased name is declared by a file in an ancestor directory.
  - **What happens:** it is not emitted. The ancestor's component gains its file in C18 and `{file, version}` in C233, and C231 counts `merged.first-declaration-wins`.
  - **Siblings never merge** (C-11).
  - **Unit test:** ExternalProject never merges (R3 P6).
- [X] T022 [US3] Existing-component merge (R9), in `cmake_fetch_attach.rs`.
  - **Which fetches:** conditional ones only, when their versionless identity (`pkg:github/<owner>/<repo>`, or the URL with its version path segment removed) equals a component another reader emitted.
  - **What happens:** the fetch merges into that component. C56 gains the CMake mechanism when it differs, C233 gains `{file, version}`, and C231 counts `merged.existing-component`.
  - **Unconditional fetches never merge across versions.**

**Checkpoint:** C-1 to C-15 all pass.

## Phase 6: User Story 4 — TheRock measured in CI (P2)

**Goal:** SC-002 to SC-004 and SC-006 on the tree that motivated the feature.
**Independent test:** `census-fetches` at the branch.

- [ ] T023 [US4] Push the branch and dispatch `census-fetches` with `waybill_ref = 1538-cmake-nested-fetches` and `base_ref = main` (T003). Record in `specs/1538-cmake-nested-fetches/measurements/therock.md`:
  - **SC-002:** compare file by file. Every declaration in the census's `fetches.json` is either read, which its file appears in C18 or the C231 accounting shows, or in a directory the walk skips, listed by path. V1 holds. Zero are silently dropped.
  - **SC-003:** counts by C122 value against the census's shape counts: 148 under an `if`, 5 `FIND_PACKAGE_ARGS`, 32 `NOT TARGET`, and `NOT <X>_FOUND`.
  - **SC-004:** no two fetch-derived components share a PURL. List every versionless identity on more than one component, and confirm each comes only from sibling declarations.
  - **SC-006:** the branch's median of 3 is at most 1.5× `main`'s, in the same job.
  - **Ownership:** the components' owners, by submodule.
- [X] T024 [US4] File the SC-008 follow-up issue: nested `find_package` 2,030, `pkg_check_modules` 42, `pkg_search_module` 3 (`measurements/census.md`). Ask whether to read them and how fallbacks would apply. Link it from `measurements/census.md`.

## Phase 7: Polish and cross-cutting

- [ ] T025 Mutations: `specs/1538-cmake-nested-fetches/measurements/mutate.py` plus `mutate-run.sh`, in spec 1537's pattern, restoring with `cp` and checking `git diff --quiet` after a commit.
  - **Discovery and reading:** discovery back to today's set; the tokeniser not skipping bracket comments; `else` not recorded.
  - **Each shape, removed in turn:** `NOT TARGET`, `NOT <X>_FOUND`, `FIND_PACKAGE_ARGS`, the `NEVER` cancellation, `ALWAYS`, the provider's method check, function body.
  - **Scope:** Test not outranking Optional; the all-declarations-conditional rule inverted.
  - **Merging and ownership:** first-declaration-wins applied to siblings and to ExternalProject; existing-component merge applied to unconditional fetches; owner edges skipped.
  - **Accounting:** C231 `unresolved` not counted, which must break V1.

  Each mutation must fail a test. Record the results in `measurements/mutations.md`.
- [ ] T026 Corpora in CI.
  - **Public corpus:** compare mode at the branch. It is expected **byte-identical** (R12), and any change stops the work until it is explained.
  - **Quality corpus** at the branch. Trace any `cpp-mongo` movement edge by edge against `main`, and re-author `xtask/corpus/quality-corpus.toml` with dated causes.
  - Record both in `measurements/corpus.md`.
- [ ] T027 Update `docs/ecosystems.md`' CMake section: the discovery rule, the shapes and their C122 values, test scope, merging, C231 to C233, and the `find_package` boundary (FR-002).
  - In `waybill-cli/src/cli/scan_cmd.rs`, `--cmake-third-party-recursive`'s help text says it now affects `find_package` only (FR-012).
  - Add CHANGELOG `[Unreleased]` entries: Added (C231 to C233); Fixed (#1516, #1519, with T023's TheRock numbers).
- [ ] T028 Run the gate and record it in `measurements/gate-log.md`: the ok-line count, 0 failed, the expected line found, and `cmake_nested_fetches` N of N. Also check `git diff --quiet main -- Cargo.lock Cargo.toml '*/Cargo.toml'`.
- [ ] T029 Open the PR. The body carries:
  - the census baseline and T023's TheRock numbers;
  - the CMake probe table;
  - the R2 marker decision;
  - the corpus results, mutations and gate;
  - the follow-ups: T024's issue, #1518 and #1539.

  It ends with `Closes #1516` and `Closes #1519`, then the attribution line.

## Dependencies

- **Phase 1:** T003 runs alongside Phase 2.
- **Phase 2 blocks every story.** T004, T005 and T006 are independent [P]. T008 and T009 are independent of each other and of T004 to T006. T007 is independent.
- **US1:** T010 first, then T011, T012, T013, T014.
- **US2:** after US1, because it classifies US1's entries. T015 first, then T016, T017, T018, T019.
- **US3:** after US2, because R9 merges conditional fetches only. T020 first, then T021, T022.
- **US4:** after US3, and it needs T003.
- **Polish:** T025 to T027 after US3. T028, then T029, last.

## Parallel examples

- **Phase 2:** T004 (`commands.rs`), T005 (`blocks.rs`), T006 (`fallback.rs`), T008 (the emit path) and T009 (the catalog and extractors) all touch different files.
- **CI:** T023 (`census-fetches`) and T026 (the corpora) are separate workflows, and run together.

## Implementation strategy

- **MVP = US1 + US2,** and they ship together, as #1519 requires. US1 alone over-reports nested fallbacks as required.
- **US3** removes duplicates. **US4** proves it on TheRock.
- **Stop conditions:**
  - a public-corpus change at T026;
  - SC-006 exceeded at T023;
  - a V1 failure on TheRock.

  Each stops the work for diagnosis before anything is re-fitted.
