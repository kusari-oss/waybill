# Tasks: A CMake condition that holds by default does not make a fetch optional (#1639)

**Input**: `specs/1641-cmake-conditional-scope/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested by the spec (SC-002, SC-003, SC-006, SC-007) and the plan's Testing section. Every case `contracts/wire.md` marks "Changed" is written at the log-only commit and must fail there before the wiring that fixes it. The unchanged cases are guards that pass throughout.

**Paths**:
- `CD` = `waybill-cli/src/scan_fs/package_db/cmake/conditions.rs` (new)
- `BL` = `…/cmake/blocks.rs`; `RE` = `…/cmake/reach.rs`; `FB` = `…/cmake/fallback.rs`; `FE` = `…/cmake/fetches.rs`; `SE` = `…/cmake/searches.rs`
- `IT` = `waybill-cli/tests/cmake_conditional_scope.rs` (new)
- `FX` = `waybill-cli/tests/fixtures/cmake_conditional_scope/` (new)
- `MS` = `specs/1641-cmake-conditional-scope/measurements/`
- Research decisions are R1–R8. Cases are `contracts/wire.md`'s.

## Phase 1: Setup

- [X] T001 Build `main`'s binary before any source change and copy it to the scratchpad (`waybill-main-<sha>`), for C-ctl's golden. Create `IT` with these helpers:
  - `scan(fx, path)`: three formats in one invocation, as `waybill-cli/tests/declared_url_credentials.rs::scan3` does, honouring `WAYBILL_CONDITIONAL_SCOPE_BIN` for the golden;
  - `scope_of(doc, purl_prefix)`: CycloneDX `scope`, defaulting to `required`;
  - `prop`, `parity` over the rows A1, C122, C232, C236 (and the native scope through A1's component set), `assert_repeatable` (10 scans), CycloneDX validation and unique `bom-ref`s. Copy these from `declared_url_credentials.rs`.

  Fixtures are inline templates under `ROOT = "cmake_minimum_required(VERSION 3.25)\nproject(waybill_fixture_app VERSION 1.0.0)\ninclude(ExternalProject)\n"`, with the issue's `ExternalProject_Add(waybill-fixture-zlib URL … URL_HASH …)`.
- [X] T002 C-ctl (FR-009):
  - **Tree.** One CMake tree with an unconditional fetch, one under `if(NOT TARGET x)`, one under `if(BUILD_TESTING)`, and an unconditional `find_package`. None is a catch-all conditional.
  - **Golden.** Generate `FX/ctl-golden/{cdx,spdx-2.3,spdx-3}.json` with the saved `main` binary (`WAYBILL_UPDATE_CONDITIONAL_SCOPE_GOLDENS`).
  - **Check.** It passes at `main`.

## Phase 2: Foundational (log-only: evaluator, history, defaults, census)

Scope does not change in this phase.

- [X] T003 Create `CD` (research R2, R4) and declare it in `waybill-cli/src/scan_fs/package_db/cmake.rs`'s module list.
  - `pub(crate) enum Eval { Holds, DoesNotHold, Unknown, Platform }`.
  - A tokenizer and parser for one `if()` condition string, with CMake's precedence: parentheses, unary, binary, `NOT`, `AND`, `OR`.
  - `evaluate(condition, defaults) -> Eval`, with the atom rules of R2: constants, platform variables and comparisons, the guard (both operand orders, and `PROJECT_SOURCE_DIR`), and variables through `defaults`. Everything else is `Unknown`.
  - `chain(branches, defaults) -> Eval`: the conjunction of every enclosing branch, with the `elseif`/`else` reading of R3. A `Platform` result holds.
  - `Defaults::collect(per-file commands)`: `option` (missing value means OFF), `cmake_dependent_option`, `set(… CACHE BOOL …)` and literal `set()`. Conflicting values give `Unknown`.

  Unit tests (with `#[cfg_attr(test, allow(clippy::unwrap_used))]`): every atom kind, precedence, the `Platform` algebra, the guard in both orders, each default source, a conflict, `else`, `elseif`, and nesting.
- [X] T004 Branch history (R3).
  - **In `BL`:** `Block::If` keeps its earlier branch conditions and the current branch's kind beside today's string. `if_conditions()` and `conditions()` are unchanged. Add `branches()`.
  - **In `RE`:** seeds carry the same history, and a new `ReachMap::scope_files(path)` returns F, its includers transitively, and the `CMakeLists.txt` files of the directories that `add_subdirectory()` F's directory, with the files they include (R4).
  - **Unit tests:** the history through `if`/`elseif`/`else`; seeds from inside an `else()`; `scope_files` for an include chain and for a subdirectory.
  - Every existing `BL` and `RE` test passes unchanged.
- [X] T005 Log-only wiring.
  - **In `FE`:** for each declaration whose shape is `Conditional`, compute the chain's `Eval` with `Defaults` restricted to `scope_files`. Emit `tracing::debug!(target: "waybill::cmake_conditions", kind = "fetch", subject, versioned, category, eval, conditions)`.
  - **In `SE`:** the same for each conditional search (`kind = "search"`).
  - `category` is R6's.
  - **Scope, C122, C232 and C236 are unchanged.** Confirm with the existing CMake suites (`cmake_*`, `git_submodule_reader`, the installed-library suites) and C-ctl.
- [X] T006 Census positive control: an `IT` test that scans one fixture with one catch-all conditional fetch per category at `RUST_LOG=waybill::cmake_conditions=debug`, and asserts one event per category with the expected `eval`.
- [ ] T007 **Census at the log-only commit** (SC-001, R6).
  - **Fixture repository** (`kusari-sandbox/waybill-fixture-therock-sdk`): add `census/conditions.py` (stdlib, `python3 -I -B`, ANSI-stripped) and `.github/workflows/census-conditions.yml`, modelled on `census-merges.yml`.
    - **Matrix:** TheRock (submodules) and the quality corpus's CMake targets at their `xtask/corpus/quality-corpus.toml` pins.
    - **Tally:** by kind × category × eval; versioned counts; how many would change scope under FR-004/FR-007; options defined only after their use (planning amendment).
  - **Dispatch** with `--ref main -f waybill_ref=<full sha>`, and read the scanned SHAs from `refs.txt`.
  - **Public corpus** at the same commit, `regen_goldens=true`: 0 differing files expected. Enumerate every job.
  - **Public corpus census (SC-001):** count `waybill:cmake-fetch-conditions`, `waybill:cmake-find-conditions` and `cmake-conditional` occurrences in the regenerated goldens. If they are 0, the corpus has no catch-all conditional, so its census count is 0 by construction. Record that rather than assume it. Otherwise add the targets that have them to the census matrix.
  - **Record** in `MS/census.md`. No prediction is written before the runs.

**Checkpoint**: census recorded. If the counts contradict the spec's choices (FR-004 unknown stays in scope, FR-007 searches follow the rule), stop and raise it before Phase 3.

## Phase 3: User Story 1: a bundled library under a condition that holds stays in scope (P1) 🎯 MVP

**Goal**: FR-001, FR-004, FR-005, FR-006, FR-007. **Independent test**: T-1 to T-4, T-10, T-11, S-1, S-2, R-1.

- [X] T008 [US1] Write T-1, T-2, T-2r, T-3, T-4, T-10, T-11, T-12, T-13, M-1, S-1, S-2 and R-1 in `IT`. **Run them at the log-only commit:** every "Changed" case must fail on its scope assertion and every other case must pass. Record the result per case in `MS/control.md`.
- [ ] T009 [US1] In `FE`:
  - a `Conditional` declaration whose chain evaluates `DoesNotHold` keeps `Optional` with `cmake-conditional`. Otherwise it gets no scope and no C122 (FR-001, FR-004).
  - `mark` writes C232 whenever the declaration has conditions, in scope or not (FR-005).
  - The six other shapes and Test are not re-evaluated.
- [ ] T010 [US1] In `FE::merge_into_existing`, `conditional` becomes "a fetch carrying C232 that is not Test" (FR-006, R5). C231's `conditional` stays the count of Optional components. Keep C231 V2 holding.
- [ ] T011 [US1] In `SE::scopes`, a conditional search whose chain is `DoesNotHold` counts as `optional`, and any other conditional search counts as `runtime` (FR-007). Test-directory and `BUILD_TESTING` searches are unchanged. C236 is unchanged.
- [ ] T012 [US1] Run T008's cases, the unit tests, C-ctl and the existing CMake and installed-library suites until they pass. An existing assertion whose expectation changes is recorded with its reason in the commit message. None may be loosened otherwise.

## Phase 4: User Story 2: combined and branching conditions (P1)

**Goal**: FR-002, FR-003. **Independent test**: T-5 to T-9.

- [X] T013 [US2] Write T-5, T-6, T-7, T-7e, T-8 and T-9 in `IT`. The "Changed" ones (T-6, T-8, T-9) must fail at the log-only commit; add them to `MS/control.md`. If they are written after T009, record that M-mutations stand in for the failing run (T018).
- [ ] T014 [US2] Make T-5 to T-9 pass. Fix any `CD` rule they expose, adding the unit test that would have caught it.

## Phase 5: User Story 3: nothing else changes (P2)

**Goal**: FR-001's exclusions, FR-006, FR-009. **Independent test**: F-1 to F-6, B-1, R-1, C-ctl.

- [X] T015 [US3] Write F-1 to F-6 and B-1 in `IT`, each asserting the same scope, C122 and C232 as `main`. They pass at the log-only commit and after T009–T011.

## Phase 6: Polish and cross-cutting

- [ ] T016 [P] FR-010 docs:
  - the C122 and C232 rows of `docs/reference/sbom-format-mapping.md`, worded as in `contracts/wire.md`;
  - a paragraph in `docs/reference/reading-a-mikebom-sbom.md`;
  - `CHANGELOG.md` `## [Unreleased]`, Fixed (#1639).
- [ ] T017 Spec record: add the census's figures to `research.md` R6 (no figure before T007).
- [ ] T018 Mutations (SC-006): `MS/mutate.py`, modelled on `specs/1640-strip-url-credentials/measurements/mutate.py`.
  - **One per evaluation rule:** platform, its negation, the guard, its reversed order, option default, missing value, `cmake_dependent_option`, `CACHE BOOL`, literal `set`, conflict, `NOT`, `AND`, `OR`, `else`, `elseif`, nesting, `scope_files`.
  - **Plus:** R9's predicate back to scope; C232 only with a scope; searches on the old rule.
  - Restore with `cp`, rebuild after the last one, and use a `^error\[E…\]` guard. Record in `MS/mutations.md`.
- [ ] T019 Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`. Record `GATE_RESULT` and every `test result:` count in `MS/gate-log.md`.
- [ ] T020 Public corpus at the final commit (SC-004): `regen_goldens=true`. Enumerate the jobs and confirm the scanned SHA.
  - `cmp` every regenerated golden against the committed one.
  - Trace every changed component to FR-001 or FR-007 by its C232/C236 conditions.
  - Commit the changed goldens from the artifact.
  - Record in `MS/corpus.md`.
- [ ] T021 TheRock and quality corpus (SC-004, SC-005).
  - **TheRock:** `measure-source` with `--ref main -f waybill_ref=<branch full sha> -f base_ref=<main full sha>`.
    - Read the SHAs from `refs.txt`.
    - Compare all six documents with per-run fields masked.
    - Trace every changed component (scope, C122 and C232 before and after) to an FR, and check its count against T007's census.
    - Component counts must be equal, and unexplained orphans must not increase.
    - Record the median of 3 and the ratio (at most 1.05).
  - **Quality corpus:** branch and `main` runs, every changed metric traced.
  - Record in `MS/therock.md` and `MS/corpus.md`.
- [ ] T022 Verify each task's claim against the code and measurements before ticking it. Confirm `git diff --quiet origin/main -- Cargo.lock '*/Cargo.toml' Cargo.toml` (FR-011). Open the PR with "Closes #1639" and "Refs #1591, #1605, #1516, #1519, #1623, #1624", ending with the attribution footer. Merging is the maintainer's.

## Dependencies

- **Setup:** T001 → T002, with `main`'s binary before any source change.
- **Foundational:** T003 → T004 → T005 → T006 → T007. The census gates Phase 3.
- **US1:** T008 runs at the log-only commit, before T009. Then T009 → T010 → T011, all in `FE`/`SE`, sequential, then T012.
- **US2:** T013 is best written with T008, at the log-only commit. T014 comes after T009.
- **US3:** T015 can run any time after T002.
- **Polish:** T016 after T012; T017 after T007; T018 after T014 and T015; T019 after T018; T020 and T021 after T019; T022 last.

## Parallel examples

- T013 and T015 (test writing) alongside T008, all in `IT` but separate test functions, if written together before T009.
- T020 and T021 dispatch together.

## Implementation strategy

- **MVP:** Phases 1–3. The census is recorded, and platform checks, the super-build guard, default-ON options and unknown conditions stop excluding fetches and searches.
- **Then:**
  - Phase 4 hardens compound and branching conditions;
  - Phase 5 guards what must not move;
  - Phase 6 carries the corpus and TheRock evidence.
