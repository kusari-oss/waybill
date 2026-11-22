# Tasks: A requirements file belongs to the project in its directory, not to a lockfile entry beside it (#1640)

**Input**: `specs/1642-requirements-owner-editable/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested by the spec (SC-002, SC-003, SC-006, SC-007) and the plan's Testing section. Every case in `contracts/wire.md` with a control different from its expected result is written at the log-only commit and must fail there before the change that fixes it. The others are guards that pass throughout.

**Paths**:
- `PM` = `waybill-cli/src/scan_fs/package_db/pip/mod.rs`; `UV` = `…/pip/uv_lock.rs`
- `DA` = `waybill-cli/src/scan_fs/declared_attach.rs`
- `SP` = `waybill-cli/src/resolve/scope_propagation.rs`
- `SC` = `waybill-cli/src/cli/scan_cmd.rs`
- `IT` = `waybill-cli/tests/requirements_owner.rs` (new)
- `MS` = `specs/1642-requirements-owner-editable/measurements/`
- `FR` = the TheRock fixture repository, `kusari-sandbox/waybill-fixture-therock-sdk`
- Research decisions are R1–R8. Cases are `contracts/wire.md`'s.

## Phase 1: Setup

- [X] T001 Build `main`'s binary (`79b3350b` or the current `origin/main`, full SHA recorded) before any source change, and copy it to the scratchpad as `waybill-main-<sha>`. Create `IT` with helpers copied from `waybill-cli/tests/declared_url_credentials.rs` and `waybill-cli/tests/declared_requirements.rs`:
  - `scan3(dir)`: all three formats in one invocation, honouring `WAYBILL_REQUIREMENTS_OWNER_BIN` so a case can be run against the saved `main` binary;
  - `edges_into(doc, purl_prefix)`: sorted `ref`s of CycloneDX `dependencies` entries whose `dependsOn` holds the component;
  - `scope_of(doc, purl_prefix)` (default `required`) and `prop(doc, purl, name)`;
  - `parity` over C18, C62, C120 and the native scope; CycloneDX validation; unique `bom-ref`s; `assert_repeatable` (10 scans).

  The tree builders write #1640's tree (`tree_1640(dir, dev_name)`) and probe R4's tree (`tree_r4(dir)`) exactly as `MS/probe.sh` does.
- [X] T002 B-1 (FR-007): a one-project-per-directory tree with no root anchors (a project `app/` with `pyproject.toml`, `uv.lock` and `requirements.txt`). Generate goldens `waybill-cli/tests/fixtures/requirements_owner/b1-golden/{cdx,spdx-2.3,spdx-3}.json` with the saved `main` binary (`WAYBILL_UPDATE_REQUIREMENTS_OWNER_GOLDENS`), and assert byte-identity with per-run fields masked as `declared_url_credentials.rs` masks them. It passes at `main`.

## Phase 2: Foundational (log-only census)

Output does not change in this phase.

- [X] T003 Census events on target `waybill::owner_census` (research R6, data-model "Census events"), each one `tracing::debug!` with a JSON string field:
  - **In `PM`,** the augment-existing branch: `merge_source` for every merge, with the existing entry's `sbom_tier` and `source_path`, the synthesized main module's `source_path`, and the editable path. Carry the editable path from `UV` on the parse result for this. It is not emitted, and is used by R2 later. Also `agrees`.
  - **In `DA`,** `OwnerIndex::build`: `owner_candidates` for every directory with more than one candidate at the same rank, with each candidate's PURL, rank, `non_runtime` and first source path, the step that decides it under R3's key, and today's owner.
  - **In `DA` or `SC`:** `dev_only_main_module` for every main module whose only inbound edges are `DevDependsOn` or `OptionalDependsOn` from a dependency group (C122 `pep-735-dependency-groups:*`). Record its directory, and whether it is an owner candidate, a root candidate (`root_selector` input) or an `added_seeds` member.
  - **In `SP`/`SC`:** keep an owned copy of the reader root anchor list built before `install_reader_root_anchors` (research R5). Before the propagation, run `runtime_reachable` with `seeds ∪ added_seeds ∪ runtime anchors` on the same inputs, and emit `anchor_immunity` for each component that set holds but today's immune set does not, and that the pass then assigns a non-runtime scope. Assign nothing.

  Every existing suite passes unchanged (`cargo test -p waybill --test declared_requirements --test scope_propagation_ecosystems --test git_submodule_reader` and the pip suites), and so does B-1.
- [X] T004 Census positive control: an `IT` test that scans `tree_1640(.., "waybill-fixture-devtool")` and `tree_r4` at `RUST_LOG=waybill::owner_census=debug`. It asserts at least one `merge_source` with `agrees`, one `owner_candidates` for `tool` decided by the PURL, one `dev_only_main_module` for the dev package, and one `anchor_immunity` for `waybill-fixture-shared`.
- [X] T005 **Census at the log-only commit** (SC-001, R6).
  - **In `FR`:** add `census/owners.py` (stdlib, `python3 -I -B`, ANSI-stripped; it reads only the events and does not recompute ownership) and `.github/workflows/census-owners.yml`, modelled on `census-conditions.yml`.
    - **Matrix:** TheRock with top-level submodules, plus all 18 targets of `xtask/corpus/quality-corpus.toml` at their pins.
    - **Tally per target:**
      - same-rank directories, and the deciding step for each;
      - candidates whose first path is a lockfile name (`uv.lock`, `poetry.lock`, `Pipfile.lock`): R3 expects 0 after R2;
      - merges by tier, and how many change C18/C120 under R2;
      - editable disagreements;
      - dev-only main modules and what keys on them;
      - `anchor_immunity` by ecosystem and anchor kind.
  - **Dispatch** with `--ref main -f waybill_ref=<full sha>`. Enumerate every job, and read the scanned SHAs from `refs.txt`.
  - **Public corpus** at the same commit, `regen_goldens=true`: 0 differing files expected, which confirms the commit is log-only. Enumerate every job.
  - **Record** in `MS/census.md`. No prediction is written before the runs.

**Checkpoint**: census recorded. If it contradicts FR-002's default (a dev-only main module still owns a directory or is selected as root after R2), FR-003's order, or R2's installed exemption (an installed merge decides a requirements owner), stop and raise it before Phase 3.

## Phase 3: User Story 1: a requirements file attaches to the project that declares it (P1) 🎯 MVP

**Goal**: FR-001, FR-002, FR-003. **Independent test**: O-1 to O-8, O-10 to O-12, and T008's unit test.

- [X] T006 [US1] Write O-1 to O-8 and O-10 to O-12 in `IT`. O-4 uses the `zdevtool` order (`contracts/wire.md`).
  - **O-7:** a poetry project, a `poetry.lock` with a same-PURL entry, and a requirements file.
  - **O-8:** an editable install, `.venv/lib/python3.12/site-packages/waybill_fixture_app-1.0.0.dist-info/` plus the project's `pyproject.toml`. Follow the existing dist-info fixtures in `waybill-cli/src/scan_fs/package_db/pip/dist_info.rs` tests.
  - **O-10:** O-1 with the tool's lock entry as `virtual = "."`. **O-11, O-12:** guards (an editable path outside the scan root; an editable entry with no manifest).

  **Run them at the log-only commit:** O-1, O-3, O-4, O-6, O-7 and O-10 must fail on their stated assertion, and O-2, O-5, O-8, O-11 and O-12 must pass. Record per case in `MS/control.md`. A case expected to fail that passes stops the phase: its control is wrong, and the case is rebuilt before T007.
- [X] T007 [US1] R2 in `PM`'s augment-existing branch. When the existing entry's `sbom_tier` is not `deployed`:
  - set `source_path` to the synthesized main module's;
  - insert the old `source_path` at the front of `extra_source_paths`, unless it is already there.

  When the entry carries a uv.lock editable path, compare it, resolved against the lock's directory, with the manifest directory, and `debug!(target: "waybill::owner_index", …)` on disagreement. Unit tests in `PM`: a lockfile merge reorders, an installed merge does not, and the editable cross-check logs on a mismatch (use `tracing-test` only if already a dev-dependency; otherwise assert through a returned flag).
- [X] T008 [US1] (Census amendment: the `non_runtime` key was implemented, then dropped after T005; the key stays `(rank, purl)` with the PURL-tie log, `measurements/census.md`.) R3 in `DA`: the candidate key becomes `(rank, non_runtime, purl)`, where `non_runtime` is `lifecycle_scope.is_some_and(|s| s.is_non_runtime())`, and `all_by_dir` sorts by the same key. Emit `debug!(target: "waybill::owner_index", …)` when the PURL decides. Unit tests: rank still dominates, a runtime candidate beats a non-runtime one of the same rank, and the PURL decides last. This unit test is FR-003's runtime-step test and kills M4 (no integration case produces the tie; `contracts/wire.md`).
- [X] T009 [US1] Run O-1 to O-8 and O-10 to O-12, the unit tests, B-1, `declared_requirements`, `git_submodule_reader` and the CMake suites that use `OwnerIndex` (spec 1538 fetch attach, spec 1614/1618 search pairing) until they pass. An existing assertion whose expectation changes is recorded with its reason in the commit message. None may be loosened otherwise.

## Phase 4: User Story 2: a package the root requires directly is never reported as not shipping (P1)

**Goal**: FR-004, FR-005. **Independent test**: A-1 to A-6.

- [X] T010 [US2] Write A-1 to A-6 in `IT`.
  - **A-4:** a top-level git submodule built as `waybill-cli/tests/git_submodule_reader.rs` builds one (`.gitmodules` plus a gitlink), holding a non-pip project, for example an npm `package.json` and lockfile. A dev-scoped component outside it depends on the submodule project's dependency.
  - **A-6:** a runtime npm main module whose direct dependency a dev-scoped component also reaches.

  **A-4 guard:** before asserting scope, A-4 asserts that the CycloneDX `dependencies` contain a path of runtime edges from the submodule component to the dependency, and that the dev-scoped component has an edge to it. If no such path exists, the case tests nothing: rebuild it (for example, a submodule whose own requirements file declares the dependency, which spec 1537 attaches to the submodule) before going on.

  **Run them at the log-only commit:** A-1 and A-4 must fail, and A-2, A-3, A-5 and A-6 must pass. (A-3 and A-4 were measured on main's binary before writing; `contracts/wire.md`.) Add the results to `MS/control.md`.
- [X] T011 [US2] R5:
  - **In `DA`:** `DeclaredAttachment` gains `runtime_root_anchors`: an anchor goes there when a requirements file declaring it is classified runtime by `requirements_txt::classify_requirements_scope_under` (research R5).
  - **In `SC`:** keep an owned list of `runtime_root_anchors` plus the top-level submodule anchors (T003), filter it with `scope_propagation::is_runtime_seed_candidate`, and add it to the `added_seeds` passed to `propagate_non_runtime_scope_seeded`. Graph completeness and the FR-012 gate keep `seeds`.
  - **In `SP`:** `PropagationSummary` gains `immune_by_root_anchors`, computed like `immune_by_added_seeds` and logged with it. This needs a separate `anchor_seeds` parameter, or a split of `added_seeds` into its two sources; choose the one that keeps the existing call sites' meaning.
  - **Unit tests in `SP`:** a runtime anchor makes its target and runtime descendants immune; a non-runtime anchor does not; and graph completeness is unchanged.
- [X] T012 [US2] Run A-1 to A-6, `scope_propagation_ecosystems`, the `SP` unit tests and B-1 until they pass. The invariant test (FR-005) is A-1 plus A-4 plus A-6. Confirm that A-1 and A-4 failed at the log-only commit (T010).

## Phase 5: User Story 3: nothing else changes (P2)

**Goal**: FR-007. **Independent test**: B-1, B-2.

- [X] T013 [US3] B-2: run `declared_requirements` and every spec 1618 test unchanged after T007–T011. Spec 1537's uv-workspace expectations are re-read against O-6, which changes by design: the members now own their own directories.

## Phase 6: Polish and cross-cutting

- [X] T014 [P] FR-008 docs:
  - the C18, C62 and C120 rows of `docs/reference/sbom-format-mapping.md`: the manifest directory comes first for a lockfile-merged main module, and an anchor protects what it anchors;
  - the requirements-ownership paragraph of `docs/reference/reading-a-mikebom-sbom.md`;
  - `CHANGELOG.md` `## [Unreleased]`, Fixed (#1640).
- [X] T015 Spec record: add the census figures to `research.md` R6, and resolve FR-002 against them. No figure is written before T005.
- [X] T016 Mutations (SC-006): `MS/mutate.py`, modelled on `specs/1641-cmake-conditional-scope/measurements/mutate.py`.
  - **M1:** drop R2's `source_path` swap (killed by O-1/O-3).
  - **M2:** apply R2 to installed entries too (killed by O-8).
  - **M3:** drop the old path's move into `extra_source_paths` (killed by O-3).
  - **M4:** remove the `non_runtime` key (killed by T008's unit test).
  - **M5:** drop the anchor seeds (killed by A-1, A-4).
  - **M6:** drop the declaring-file filter (`runtime_root_anchors` → `root_anchors`) (killed by A-3).
  - **M7:** drop the `is_runtime_seed_candidate` filter on anchors (killed by a unit test in `SP`: an anchor whose target is reader-scoped Optional, as a root `requirements-dev.txt` alone gives, protects nothing).

  Restore with `cp`, rebuild after the last one, and use a `^error\[E` guard. Record in `MS/mutations.md`.
- [X] T017 Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`. Record `GATE_RESULT` and every `test result:` count in `MS/gate-log.md`.
- [X] T018 Public corpus at the final commit (SC-004): `regen_goldens=true`. Enumerate the jobs and confirm the scanned SHA.
  - `cmp` every regenerated golden against the committed one.
  - Trace every changed component and edge to FR-001, FR-003 or FR-005.
  - Commit the changed goldens from the artifact.
  - Record in `MS/corpus.md`. This is also the public corpus's census (planning amendment).
- [X] T019 TheRock and quality corpus (SC-004, SC-005).
  - **TheRock:** `measure-source` with `--ref main -f waybill_ref=<branch full sha> -f base_ref=<main full sha>`.
    - Read the SHAs from `refs.txt`.
    - Compare all six documents with per-run fields masked.
    - Trace every changed component and edge (owner, scope, C18, C62, C120 before and after) to an FR, and check counts against T005's census. In particular, check whether #1640's five root-declared packages appear and become runtime.
    - Component counts must be equal, and unexplained orphans must not increase.
    - Record the median of 3 and the ratio (at most 1.05).
  - **Quality corpus:** branch and `main` runs, with every changed metric traced component by component, through `census-conditions`-style document runs where the quality report keeps no documents.
  - Record in `MS/therock.md` and `MS/corpus.md`.
- [ ] T020 Verify each task's claim against the code and measurements before ticking it. Confirm `git diff --quiet origin/main -- Cargo.lock '*/Cargo.toml' Cargo.toml` (FR-009). Open the PR with "Closes #1640" and "Refs #1566, #1587, #1618", ending with the attribution footer. Merging is the maintainer's.

## Dependencies

- **Setup:** T001 → T002, with `main`'s binary before any source change.
- **Foundational:** T003 → T004 → T005. The census gates Phases 3 and 4.
- **US1:** T006 at the log-only commit, then T007 → T008 → T009.
- **US2:** T010 at the log-only commit (best written with T006), then T011 → T012. US2 is independent of US1's code (different files), but T011 lands after T009 so each phase's suite runs on a settled base.
- **US3:** T013 after T012.
- **Polish:** T014 after T012; T015 after T005; T016 after T013; T017 after T016; T018 and T019 after T017; T020 last.

## Parallel examples

- T006 and T010 (test writing) together at the log-only commit, as separate test functions in `IT`.
- T014 alongside T016.
- T018 and T019 dispatch together.

## Implementation strategy

- **MVP:** Phases 1–3. The census is recorded, and a requirements file attaches to the project in its directory whatever the package names.
- **Then:**
  - Phase 4 stops root-declared runtime requirements being reported as not shipping;
  - Phase 5 guards what must not move;
  - Phase 6 carries the corpus and TheRock evidence.
