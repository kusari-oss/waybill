# Tasks: Name a CMake git fetch with an unresolved reference after its repository

**Input**: `specs/1638-unresolved-github-fetch/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested by the spec (SC-001 to SC-006) and the plan's Testing section. Each story's tests are written first and must fail at `main` before its implementation, except the guard cases named as such.

**Paths**:
- `CM` = `waybill-cli/src/scan_fs/package_db/cmake.rs`
- `FE` = `waybill-cli/src/scan_fs/package_db/cmake/fetches.rs`
- `RC` = `waybill-cli/src/resolve/reconciler.rs`
- `IT` = `waybill-cli/tests/cmake_unresolved_github_fetch.rs` (new)
- `FX` = `waybill-cli/tests/fixtures/cmake_unresolved_github_fetch/` (new)
- `MAP` = `docs/reference/sbom-format-mapping.md`
- `MS` = `specs/1638-unresolved-github-fetch/measurements/`

## Phase 1: Setup

- [X] T001 **Census, before any source change** (SC-001, research R7).
  - **Script.** Add `census/unresolved_fetches.py` to `kusari-sandbox/waybill-fixture-therock-sdk`, beside `census/fetches.py`, and run it from `.github/workflows/census-fetches.yml`.
  - **Trees.** TheRock's sparse trees, as already checked out there, plus a step that clones the three quality-corpus targets at their pins from `xtask/corpus/quality-corpus.toml`, as `census-projects.yml` does: `cmake-nlohmann-json` `65ee6845`, `python-pytorch` `2b3ec348`, `cpp-mongo` `41a57524`.
  - **What it counts.** Every `FetchContent_Declare` / `ExternalProject_Add` whose `URL`, `GIT_REPOSITORY` or `GIT_TAG` still holds `${` after same-file single-literal `set()` expansion. Split them into:
    - a literal GitHub repository;
    - a literal other repository;
    - an unresolved repository;
    - a URL.

    Count separately the subset with an unexpanded declared name, and list every literal-repository case with file and line.
  - **Wrapper templates.** Report wrapper template bodies (a `function()`/`macro()` body call whose declared name is a parameter) on a separate line, outside the four-way split, because spec 1539 does not count them as declarations.
  - **Reconcile, declaration by declaration** (research R7). Dispatch with `-f waybill_ref=<full main sha>`, so the run also scans the same trees with `main`. Classify each declaration the census counts:
    - **found**: its file is in the C18 files of a `main` component whose C151 is `URL or GIT_TAG uses a CMake variable with no literal set() in the same file`;
    - **accounted**: it has an unexpanded name, or its file is in some component's C233, consistent with C231's `unresolved.identity` and `merged.*`;
    - **unexplained**.

    List and explain every unexplained declaration before using the counts. Do not compare totals: `main`'s output is post-merge.
  - **Record** the run ID, the fixture commit, the scanned SHAs read from the logs, and the per-tree table in `MS/census.md`.
- [X] T002 Create `IT` from the existing helpers:
  - `common::cmake_fetch` (scan in three formats, `prop`, C231 accounting check);
  - `common::git_fixture::GitFixture` (`add_submodule`, `git`).

  Add a `parity(dir)` helper that runs `waybill sbom parity-check` over C55, C122, C151, C232, C233, C234 and C236 and asserts no repeated `bom-ref`, plus `assert_repeatable(dir)`. Copy the DownloadProject module fixture (the `cmake_parse_arguments(DL_ARGS …)` + `configure_file(DownloadProject.CMakeLists.cmake.in …)` shape) from the spec 1540 cases in `waybill-cli/tests/cmake_fetch_wrappers.rs` into a helper.

## Phase 2: Foundational

- [X] T003 U-ctl in `IT` (FR-010).
  - **Tree.** `FX/ctl/` holds one of each unchanged shape:
    - a resolved GitHub fetch;
    - a resolved generic git fetch;
    - a resolved URL fetch;
    - a URL fetch with an unresolved version (U-10's shape);
    - an unresolved `GIT_REPOSITORY https://github.com/${OWNER}/…` (U-9's shape);
    - the `wf_tag(a v1.2)` wrapper call alone (U-6's literal-tag half).
  - **Golden.** Generate `FX/ctl-golden/{cdx,spdx-2.3,spdx-3}.json` with **`main`'s** binary before any source change, masking serial, timestamps and absolute paths as `waybill-cli/tests/cmake_project_components.rs`'s P-ctl does. U-ctl compares the masked output byte for byte.

**Checkpoint**: U-ctl passes at `main`.

## Phase 3: User Story 1: a fetched GitHub repository is named after the repository (P1) 🎯 MVP

**Goal**: FR-001, FR-002, FR-003, FR-008, FR-009 (for these components), FR-011.

**Independent Test**: U-1 to U-6 pass in three formats. U-ctl stays byte-identical.

- [X] T004 [P] [US1] Integration cases U-1, U-2, U-3, U-4, U-5 and U-6 in `IT`, as `contracts/wire.md` specifies them. U-1 uses a mixed-case owner/repo to check the lowercasing. U-3 is the aotriton shape: `wf_dep(noimage project)` with `V` set twice in the file. U-6's `wf_tag(a v1.2)` half is a guard and passes at `main`; every other assertion must fail at `main`, and the failure is recorded in the commit message.
- [X] T005 [P] [US1] Unit tests in `CM`'s test module for `fetch_entry`, one per row of `data-model.md`'s shape table:
  - **GitHub, unresolved reference:** versionless `pkg:github`, lowercased, `waybill:download-url` set, C151, design.
  - **`https://github.com/${OWNER}/r.git` with `GIT_TAG ${X}`:** `pkg:github` must not appear. This is the M2 target.
  - **Both mechanisms:** C55 `cmake-fetchcontent-git` and `cmake-externalproject`.

  Leave the existing `waybill-fixture-unset` and `branchy` unit tests as they are; if one changes, trace why.
- [X] T006 [US1] Implement R1 in `CM::fetch_entry`. Inside the unresolved branch, before building `pkg:generic`, apply this check: `git_repo` is `Some` with no `${`, `git_tag` is `Some` with a `${`, and `url` has no `${`. When `github_re` matches the repository, build `pkg:github/<owner>/<repo>`:
  - owner and repository folded by `super::forge::lowercases_owner_and_name("github")`, as on the resolved branch;
  - no version;
  - `download_url = Some(git_repo)`;
  - the same `source_mechanism`, design tier and C151 insertion as today.

  Factor the owner/repo fold into one closure or helper that both branches call, so the rule has one definition. Every other unresolved shape keeps today's code path byte for byte.
- [X] T007 [US1] Implement R2 in `FE::read_fetches`: the `Definitions::collect` predicate becomes `declared(kind, args, "", vars).is_some_and(|d| !(d.name.is_empty() && d.entry.extra_annotations.contains_key("waybill:unresolved-reason")))`. Add a comment citing spec 1539 R2 and this spec's research R2. Add a unit test in `FE`: a definition whose body is U-3's template is still a wrapper (`Definitions::is_wrapper`), and a body with a literal name and an unresolved tag is still not one, as on `main`.
- [X] T008 [US1] Run T004, T005, T007's unit test, U-ctl and the existing CMake suites until they pass:
  - `cmake_nested_fetches`, `cmake_fetch_wrappers`, `cmake_project_components`, `scan_cmake`, `scan_cmake_tree_finds`, `binary_source_binding_cmake`;
  - `cargo test -p waybill --bins cmake`.

  Trace every existing assertion that changes to an FR, in the commit message.

**Checkpoint**: US1 complete; U-ctl byte-identical.

## Phase 4: User Story 2: a fallback fetch merges into the component that already supplies its repository (P1)

**Goal**: FR-004, FR-007, FR-013, FR-014.

**Independent Test**: U-7, U-8, U-11 to U-16 pass in three formats; U-ctl byte-identical.

- [X] T009 [P] [US2] Integration cases U-7, U-8, U-11, U-12, U-13a, U-14, U-15, U-16 and U-17 in `IT`.
  - U-11's submodule uses `GitFixture::add_submodule` with the literal GitHub URL `https://github.com/waybill-org/waybill-fixture-dep.git`, the same shape as `cmake_nested_fetches.rs`'s line-411 case.
  - U-8, U-11 and U-14 assert C233 exactly, including `"version": ""` (FR-013), and C231's `merged.existing-component`.
  - U-17 covers FR-013's other path, R5 by name: C233 `"version": ""` and C231's `merged.first-declaration-wins`. Before T006 the unresolved declaration's PURL is `pkg:generic/waybill-fixture-dep`, so U-17 may already pass at `main`. If it does, record it as a guard rather than a failing-first case.
  - U-13a and U-16 are guards and pass at `main`; every other case must fail at `main`.
- [X] T010 [US2] Implement FR-004 in `CM::fetch_entry`. In the same literal-repository check as T006, when `github_re` does not match, keep `pkg:generic/<declared name>` and pass `download_url = Some(git_repo)`. A URL fetch and an unresolved repository still pass `None` (FR-005, FR-006).
- [X] T011 [US2] **R5 probe: decide before building.** Add U-13b to `IT`: two required `FetchContent_Declare(waybill-fixture-dep …)` of one GitHub URL in the root `CMakeLists.txt`, one with `GIT_TAG v1` and one with `GIT_TAG ${UNSET}`. Run it with T006 and T010 in place and no reconciler change, and record the result and the component list in `MS/reconciler-probe.md`. Apply R5's rule, which is fixed:
  - **one component** → do T012;
  - **two components** → mark T012 not needed in `MS/reconciler-probe.md`, keep U-13b as the regression test, and drop M5 from T016.
- [X] T012 [US2] (only if T011 collapses) In `RC::reconcile_design_source_tiers`, skip, as a design-tier candidate, any component whose `waybill:source-mechanism` is `cmake-fetchcontent-git`, `cmake-fetchcontent-url` or `cmake-externalproject` and whose PURL ecosystem is `github`. Add a comment citing FR-014 and research R5. Add a unit test in `RC`'s test module that a design-tier `pkg:github` CMake fetch and a source-tier `pkg:github@v1` of the same name and directory both survive, and that a design-tier `pkg:generic` CMake fetch reconciles as before.
- [X] T013 [US2] Run T009, U-13b, U-ctl and T008's suites until they pass.

**Checkpoint**: US1 and US2 complete.

## Phase 5: User Story 3: nothing that is not literal changes (P2)

**Goal**: FR-005, FR-006, FR-010, FR-011.

**Independent Test**: U-9, U-10, U-ctl and U-rep pass.

- [X] T014 [P] [US3] Integration cases in `IT`:
  - **U-9 and U-10:** assert each tree's own component (PURL, no `waybill:download-url`, C151). Their byte-identity is U-ctl's job.
  - **U-rep:** every tree from U-1 to U-17 scanned twice, identical output.
  - **U-xhost** (SC-005): generate `FX/changed-golden/{cdx,spdx-2.3,spdx-3}.json` from the branch binary after T013. Mask together, in one pass: the serial number, timestamps, absolute paths, `HOME`, and content-addressed IDs. Then confirm in T022's CI run that the test passes on the Linux, macOS and Windows lanes, enumerating each lane's conclusion.
- [X] T015 [US3] Run T014 and the whole of `IT`.

## Phase 6: Polish and cross-cutting

- [X] T016 Mutations, recorded in `MS/mutations.md` with the killing test for each.
  - **Method.** Save each file with `cp` first, apply one mutation, run `IT` and the `CM`/`FE`/`RC` unit tests, then restore with `cp`. Reject a "kill" whose log matches `^error\[E`, because a compile failure kills nothing. Rebuild after the last restore.
  - **M1:** the unresolved GitHub branch emits `pkg:generic` again.
  - **M2:** drop the `git_repo` has-no-`${` check.
  - **M3:** `download_url = None` for the non-GitHub shape.
  - **M4:** remove T007's mask.
  - **M5:** remove T012's guard, only if T012 was done.
  - **M6:** pass `download_url` for an unresolved URL fetch.

  Every mutation must be killed by at least one test.
- [X] T017 [P] Update `MAP`, the C231, C233 and C234 rows, with the wording in `contracts/wire.md` "Rows touched". No new row; confirm each row's extractor is unchanged and the parity gate in the lib target passes.
- [X] T018 [P] Update `docs/ecosystems.md` § CMake fetch declarations. The "Identity is unchanged" bullet gains: a GitHub `GIT_REPOSITORY` with an unresolved `GIT_TAG` is versionless `pkg:github/<owner>/<repo>` with C151. Add the `unresolved.identity` exception from FR-008 to the C231 paragraph.
- [X] T019 [P] `CHANGELOG.md` `## [Unreleased]`: one entry naming #1595, the before and after PURL, FR-004's merge and FR-014.
- [X] T020 File the follow-up issues, each with the attribution footer:
  - a non-GitHub `ExternalProject_Add` git fetch has no repository identity even when resolved (research R3);
  - CMake `waybill:download-url` is recorded verbatim, credentials included (spec Edge Cases).

  Link both in `MS/corpus.md`'s notes.
- [X] T021 Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`. Read `target/gate/prepr.log` for `GATE_RESULT=` and every `test result:` line, and record the counts in `MS/gate-log.md`.
- [X] T022 TheRock `measure-source` in `kusari-sandbox/waybill-fixture-therock-sdk`, `--ref main -f waybill_ref=<branch full sha> -f base_ref=<main full sha>`. Read the scanned SHAs from the logs.
  - Trace every changed component to an FR. Unexplained orphans must not increase (SC-003).
  - Record the scan-time median of 3 for each ref; it must be ≤ 1.05× (SC-006).
  - Write `MS/therock.md`, enumerating every job's conclusion.
- [X] T023 Corpora (SC-002, SC-004).
  - **Public corpus.** Dispatch `public-corpus.yml` with `regen_goldens` on the branch, and diff every golden with a normalised, sorted diff (masked IDs, `LC_ALL=C sort`). Targets with no literal-repository unresolved fetch in T001's census must be byte-identical; commit regenerated goldens only for the rest.
  - **Quality corpus.** Run against a `main` control, and trace python-pytorch component by component: `aotriton_external` becomes `pkg:github/rocm/aotriton` with C151 and no version, and `aotriton_runtime` is unchanged.
  - Record both in `MS/corpus.md`.
- [ ] T024 Verify each task's claim against the code and measurements before ticking it, then open the PR with "Refs #1517, #1518, #1594" and "Closes #1595", ending with the attribution footer. Merging is the maintainer's.

## Dependencies

- T001 runs first and gates nothing in code, but must finish before T023, which reads its census. T003's golden is made before T006.
- **US1** (T004–T008) depends on T002 and T003.
- **US2**: T010 and T011 depend on T006. T012 depends on T011's outcome.
- **US3** (T014) can be written any time after T002. It passes only once US1 and US2 are in, because U-rep covers their trees.
- **Polish**:
  - T016 after T013 and T015;
  - T017–T019 in parallel after T013;
  - T021 after T016–T019;
  - T022 and T023 after T021;
  - T024 last.

## Parallel examples

- **US1:** T004 (integration) and T005 (unit) are different files and can be written together.
- **US2:** T009 can be written alongside T010, but must run after T010 lands.
- **Polish:** T017, T018 and T019 touch three different files.

## Implementation strategy

- **MVP:** US1 (T001–T008). It fixes the reported misnaming, aotriton included, on every route.
- **Next:** US2, which adds the merges and FR-014's guard, if the probe needs one.
- **Then:** US3's guards and polish. One PR for the whole spec.
