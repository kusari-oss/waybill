# Tasks: Declared requirements attach to the project that declares them

**Input**: `specs/1537-requirements-root-attach/`: spec, plan, research R1–R13, data-model, contracts/wire.md, quickstart.
**Tests**: required. SC-007 asks for a mutation per FR, and every contract case (C-1 to C-13) and invariant (I-1 to I-4) becomes a test.
**Fixtures**: synthetic `waybill-fixture-*` names only, built as temporary git repositories. TheRock is scanned only in CI, through the fixture repository (R10).
**Order**: measure, then `root_edges` alone (US2), then attachment (US1). Each change regenerates the corpus separately, so each golden diff has one variable (plan § Phasing).

## Phase 1: Setup (measurement first)

- [X] T001 Commit the R1 repro arms, and the R12 arm, as a measurement record in `specs/1537-requirements-root-attach/measurements/repro.md`. The arms:
  - C-1: `requirements.txt` only;
  - C-2: with a submodule;
  - C-3: `pyproject.toml` + `requirements.txt`;
  - C-4: nested, with no project;
  - **R12 arm:** `pyproject.toml` declaring one dependency + `vcpkg.json` declaring one port;
  - **C-11 arm (U3):** `requirements.txt` plus `vendor/x/requirements.txt`. Is the vendored file read?
  - **C-12 arm (U1):** `requirements.txt` plus `requirements-constraints.txt`. Is it read as a declaration?

  For each arm, record the root's direct targets in all three formats, the completeness verdict, and any orphan reasons. Use a release build of `main` `1d25955c`, with the commands from `quickstart.md`.

  If the R12 arm strands the vcpkg component, file a follow-up issue with the measurement ("declared C/C++ manifests reach the root only through the fallback"), labelled as a follow-up to #1566. Also file the R8 follow-up: `-r` includes are not followed. Record both issue numbers in `repro.md`.

  From the C-12 result, write research R14 before T013. Either a constraints-named file is excluded from attachment, with an orphan reason, or it is attached like any other file read today. Update contract C-12 and C-11 to the observed and chosen behaviour.
- [X] T002 Create `waybill-cli/tests/declared_requirements.rs` with the helper:

  ```
  fn repo(files: &[(&str, &str)], submodules: &[(&str, &[(&str, &str)])]) -> TempDir
  ```

  It runs `git init`, writes the files, adds each submodule with `-c protocol.file.allow=always` from a sibling repository holding its own files, and commits. Also add a `scan_all_formats(dir) -> (cdx, spdx23, spdx3)` helper, reusing `tests/common` where it exists, and a `root_targets(format_doc) -> BTreeSet<String>` extractor for each format, which normalises to PURLs without qualifiers.

  Add C-1 to C-13 as tests asserting the contract. C-10 uses `--supplement-cdx` with a synthetic supplement; C-13 adds a root `CMakeLists.txt` whose `FetchContent_Declare` names the submodule. Run them at this commit:
  - C-2, C-3, C-7, C-8 and C-10 must **fail**;
  - C-1, C-4, C-9 and C-13 must **pass**. These are the controls; a passing test may exercise nothing.
  - Record which way C-5, C-6, C-11 and C-12 go.

  **Done.** At `main`, C-2, C-3, C-5, C-6, C-7, C-8, C-10 and C-13 fail; C-1, C-4, C-9, C-11 and C-12 pass. C-13 was mislisted as a control: it is C-2 plus a merge, so it fails at `main` too. C-5 to C-7 gained a scan-root project after the first run showed the root selector promoting a lone nested main module to subject (contract note). No gating: `git_submodule_reader.rs` uses the same fixture helper on every lane.

  Then update `contracts/wire.md` C-6 to the observed fallback shape, as the contract requires. Decide on file-scope `#![cfg(unix)]` by checking the Windows lane, and record the decision in the test file's header.
- [X] T003 [P] Add `.github/workflows/measure-source.yml` and `census/source_report.py` to `kusari-sandbox/waybill-fixture-therock-sdk` (research R10).
  - **Inputs:** `base_ref`, `waybill_ref`, `therock_ref` (default `b1cae390cc745283dce25581415353f9bc906162`).
  - **Steps:**
    1. Free disk with plain `rm -rf` of the preinstalled toolchains, and record `df -h` before and after.
    2. Clone TheRock at the pin.
    3. Scan at both refs, with `--root-name TheRock --root-version 10.3.0` and all three formats.
    4. `git submodule update --init --depth 1 --jobs 8`, then scan again at both refs.
  - **Report:** per #1538's counting rule. PyPI components whose source files are the super-project's own `requirements*.txt` (top level, `build_tools/`, `external-builds/`): how many, how many have an incoming edge, and how many have `waybill:orphan-reason`. Also the root's direct-target count in each format, and the completeness verdict.

  Run it with `base_ref = waybill_ref = main` (pass `--ref <branch> -f ...` per the dispatch convention). Record the run ID and the SC-006 baselines in `specs/1537-requirements-root-attach/measurements/source.md`; they replace the reporter's 73 and 62 of 66. Nothing is cloned locally.

## Phase 2: User Story 2 — every format tells the same story (P1) 🎯 first

**Goal**: one root-edge rule (research R2, R6). No attachment change yet.
**Independent test**: invariants I-1 and I-2 hold on every fixture. C-2 now attaches in CycloneDX and SPDX 2.3 through the fallback, as SPDX 3 already does.

- [X] T004 [US2] Create `waybill-cli/src/generate/root_edges.rs` and declare it in `waybill-cli/src/generate/mod.rs`. Add:

  ```
  pub(crate) fn root_targets(components, relationships, root_ref, anchors) -> BTreeSet<String>
  ```

  It follows data-model § root_edges and the four R2 decisions:
  - the fallback applies to any root;
  - only dependency-typed edges switch the gate off, and resolve anchors do not;
  - graph-tops require `parent_purl.is_none()`;
  - anchors are added after the gate and never change it.

  Unit-test each decision, plus V5 (anchors never close the gate), in the same file (`#[cfg_attr(test, allow(clippy::unwrap_used))]`).
- [X] T005 [US2] Switch `waybill-cli/src/generate/cyclonedx/dependencies.rs` to `root_targets`. Delete the local gate and the `current_root_anchors` loop at `:116`. Keep `resolve_bom_refs`. Update the module's existing tests to the R2 rule wherever they asserted the old copy's behaviour, and name each changed assertion in the commit message.
- [X] T006 [US2] Switch `waybill-cli/src/generate/spdx/document.rs` to `root_targets`. Delete the anchor loop at `:595` and the `synthetic_root_added` fallback block at `:729–`. Render every target through `relationships::synthesized_root_edge`, so scope-typed spellings are kept (#1000). Remove the `synthetic_root_added` condition per R2. Update `synth_root_fallback_*` tests the same way as T005.
- [X] T007 [US2] Switch `waybill-cli/src/generate/spdx/v3_document.rs` to `root_targets`. Delete the fallback block at `:746–` and the anchor block at `:862`. Keep the per-target lifecycle scope on `dependsOn` (#1000).
- [X] T008 [US2] In `waybill-cli/src/generate/graph_completeness/mod.rs` (`:221–272`), build `target_ref`'s adjacency from `root_targets`, passing `crate::supplement::current_root_anchors()`. Delete the mirror. The multi-root seed set is unchanged (clarification 3). Unit tests:
  - a root with anchors and no relationship edges, where the fallback is open, is `complete`; the same with a dependency edge, where the fallback is closed, is `partial`;
  - **I-2 by construction (A1):** for a table of inputs, the adjacency the classifier walks from `target_ref` equals `root_targets` on the same inputs. This is the classifier half of I-2. The test never re-derives the seed set.
- [X] T009 [US2] Add three checks to `waybill-cli/tests/declared_requirements.rs` as helpers, and assert them in every C-case test:
  - **I-1:** the root's direct targets are equal across the three formats;
  - **I-2:** the verdict equals the contract's expected value for that case. The seed set is never recomputed (contract I-2);
  - **I-4:** in the fixtures, each case's exact expected root-target set (contract table) already rules out an undeclared root edge. The general property goes in `generate/root_edges.rs` as a unit test over a table of inputs: every output is a relationship target, an anchor, or, only when no dependency edge leaves the root, a graph-top without a container.

  Run the gate locally with `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`.
  **Done (T004–T009).**
  - **Gate run:** after the switch, 406 targets passed and two failed. One is `declared_requirements`: C-3, C-5, C-6 and C-7 await US1.
  - **The other is `transitive_parity_pip_poetry`:** 64 → 89 SPDX 2.3 edges. Measured with `measurements/root_targets.py`, this is R9 case 5. At `main`, that fixture's root targets were 25 / 0 / 0 (CycloneDX / SPDX 2.3 / SPDX 3); on the branch they are 25 / 25 / 25. The baseline was updated, with the cause.
  - **C-8 expectation corrected:** the reader classifies `requirements-dev.txt` as optional, not development, so the edge is `OPTIONAL_DEPENDENCY_OF` (FR-010).

- [X] T010 [US2] Commit and push. Regenerate the corpus goldens in CI (`public-corpus.yml`, `regen_goldens=true`, `--ref <branch> -f branch=<branch>`). Diff them normalised and `LC_ALL=C`-sorted against the committed goldens.
  - Explain every changed file by R9 cases 3–7.
  - **I-1 on the corpus (SC-002, G1):** commit `measurements/root_targets.py`, run with `python3 -I -B`. It extracts the root's direct targets from each regenerated target's three documents and asserts they are equal. Validate it first against the fixtures' Rust I-1 helper, using T002's emitted documents. Record the result for every target.
  - Record the per-case counts in `specs/1537-requirements-root-attach/measurements/golden-review.md` under "Step 1: root_edges".
  - If any case touches more targets than its explanation supports, stop and report before T011.
  - Commit the explained goldens.

## Phase 3: User Story 1 — a repository's own requirements are its dependencies (P1) 🎯 MVP

**Goal**: declared edges from the owning project (research R3–R5).
**Independent test**: C-2, C-3, C-5, C-7 and C-8 pass in all three formats, with I-1 to I-4.

- [X] T011 [P] [US1] In `waybill-cli/src/scan_fs/package_db/git_submodule/mod.rs`, return the `purl_of_dir` map with the reader's output. In `waybill-cli/src/scan_fs/package_db/mod.rs`, carry it as `diagnostics.git_submodule_dirs: BTreeMap<PathBuf, String>`, next to `git_submodule_coverage`. No wire change. Unit test: the reader's map holds the submodule's canonical directory.
- [X] T012 [P] [US1] In `waybill-cli/src/scan_fs/package_db/pip/mod.rs`, change `merge_without_override` (`:572`): on a duplicate PURL, append the addition's `source_path` to the kept entry's `extra_source_paths` if absent (research R5). Unit test: two requirements files pinning the same package give one entry carrying both paths.
- [X] T013 [US1] Create `waybill-cli/src/scan_fs/declared_attach.rs`:
  - `OwnerIndex::build(components, submodule_dirs, scan_root)`, with V1 precedence: `pkg:pypi` main module, then other main modules with the lowest PURL first, then a submodule. The scan root is excluded.
  - `attach(components, relationships, owner_index, scan_root) -> DeclaredAttachment`, following data-model V2–V4: one candidate per declaring file; an ancestor walk up to the scan root; no self-edge or duplicate edge; `pkg:pypi` only; basename `requirements*.txt`.

  Unit tests:
  - each precedence rule;
  - a file at the scan root becomes an anchor;
  - a nested file with no owner becomes an anchor;
  - a nested file under a submodule goes to the submodule;
  - a nested file under a submodule's `pyproject.toml` goes to the main module;
  - a CMake-only directory is skipped (R3);
  - a cget `requirements.txt` produces nothing.
- [X] T014 [US1] In `waybill-cli/src/scan_fs/mod.rs`, run `declared_attach::attach` after component resolution and **before** `apply_lifecycle_scope_to_edges`. Append its edges to the relationships, and set `ScanResult.declared_root_anchors`. Plumb `git_submodule_dirs` from the DB scan.
- [X] T015 [US1] In `waybill-cli/src/cli/scan_cmd.rs` (`:4462–4490`), extend the reader-anchor set with `scan.declared_root_anchors`. Install the guard whenever either set is non-empty, so the default path stays byte-identical.
- [X] T016 [US1] Run `waybill-cli/tests/declared_requirements.rs`. C-1 to C-13 must pass with I-1, I-2 and I-4. C-8 must show development scope in all three formats. Run the gate. Commit and push.

  **Done (T011–T016).** C-1 to C-13 pass in all three formats, with I-1, I-2 and I-4. The gate passed at `16959878`: `GATE_RESULT=pass`, 408 `test result: ok.` lines, 0 failed.

  **Two implementation findings, both recorded in code comments:**
  - **Main-module source paths are directories.** At the scan stage, a main module's first source path is its project *directory*, as `path+file://<abs dir>` (npm, pip), not a manifest file. `OwnerIndex` derives it with `workspace_root::derive_workspace_root`, the function behind the `waybill:workspace-member` annotation, rather than a new parser.
  - **C-3's verdict is `partial` for an existing reason.** It is `transitive-edges-unresolvable: pypi`, because `c` is declared without a lockfile. It is not an orphan; the contract was corrected.

## Phase 4: User Story 3 — what stays unreachable says why (P2)

**Goal**: FR-008's population is empty by construction (research R7), and that is enforced.
**Independent test**: I-3 holds on every fixture and on every corpus document.

- [X] T017 [US3] Add an I-3 helper to `waybill-cli/tests/declared_requirements.rs`, and assert it in every C-case: no component whose C18 `waybill:source-files` includes a `requirements*.txt` path is unreachable from the seed set. Add a negative control that switches attachment off through a test-only path. It must make I-3 fail, which proves the check can fire.
  **Done.** No test-only switch in production code is needed. I-3 checks reachability from the root alone, which is stricter than the seed set, so it needs no classifier model. The negative control (`i3_detects_a_stranded_requirements_package`) removes the root's edge from a correct document and asserts that the check reports the package.
- [X] T018 [US3] Regenerate the corpus goldens in CI again (as T010). Explain every change against the T010 goldens by R9 cases 1, 2 and 4. Run `root_targets.py` (I-1) again and check I-3 on every regenerated CycloneDX document. The I-2 corpus check follows in T021. Record the results under "Step 2: declared attachment" in `measurements/golden-review.md`, and commit the goldens.

## Phase 5: User Story 4 — a super-project with its submodules checked out (P2)

**Goal**: SC-006 on #1538's super-project, in CI only.
**Independent test**: `measure-source.yml` at the branch.

- [X] T019 [US4] Run `measure-source.yml` (T003) with `base_ref=main` and `waybill_ref=1537-requirements-root-attach`. Record in `measurements/source.md`, against T003's baselines:
  - requirements-declared components unreachable without a reason, before and after, with and without submodules (target 0);
  - the root's direct-target counts in all three formats, and whether they are equal;
  - the completeness verdicts.

  If the without-reason count is not 0, record the remaining components and their cause. Do not claim #1538 is closed (spec Assumptions).

## Phase 6: Polish and cross-cutting

- [X] T020 [P] Run the SBOM quality corpus in CI at the branch (`quality-corpus.yml`, `-f branch=<branch>`). Explain every metric outside its range by the same R9 causes. Re-author those ranges in `xtask/corpus/quality-corpus.toml`, with dated comments citing the runs, as #1581 did. Record the results in `measurements/quality.md`.
- [X] T021 [P] Commit `specs/1537-requirements-root-attach/measurements/root_only.py`, run with `python3 -I -B` (research R11).
  - **What it does:** for each regenerated CycloneDX document from T018's artifact, compute reachability from `metadata.component` alone and list the documents marked `complete` with an unreachable component.
  - **Validation:** check it first against one target where both definitions must agree.
  - **I-2 on the corpus (SC-003, G1):** for each listed document, record which main-module component in the same document reaches each root-unreachable component. A component reached by neither the root nor any main module is an SC-003 failure. It blocks T025.
  - **Record and follow-up:** record the counts in `measurements/root-only.md`, and file the SC-008 follow-up issue with them.
- [X] T022 Mutations, applied by `measurements/mutate.py`, with each file restored by `cp` and checked with `git diff --quiet`. Results go in `measurements/mutations.md`. Each must fail at least one test:
  - M1: anchors close the gate;
  - M2: the fallback only for synthetic roots;
  - M3: tool-use edges close the gate;
  - M4: nested components counted as graph-tops;
  - M5: completeness ignores anchors;
  - M6: the scan root not excluded from `OwnerIndex`;
  - M7: precedence reversed (submodule before main module);
  - M8: the walk stops at the first ancestor rather than the first owner;
  - M9: the duplicate merge drops paths;
  - M10: the pass runs after lifecycle typing (C-8's scope is lost);
  - M11: declared root anchors not installed;
  - M12: a cget file accepted;
  - M13 (FR-006): SPDX 2.3 bypasses `root_targets` and keeps its own gate;
  - M14 (FR-009): submodule anchors are not installed.
- [X] T023 [P] Update the docs:
  - `docs/ecosystems.md`, pip section: requirements files are attached to the declaring project, with the precedence rule;
  - `CHANGELOG.md` `[Unreleased]`:
    - Fixed: #1566 and #1538, with SC-006's numbers;
    - Changed: one root-edge rule for all formats, listing R2's three parity corrections and their measured corpus counts from T010.
- [ ] T024 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Record it in `measurements/gate-log.md`, counting the `test result: ok.` lines and `declared_requirements` N of N, with no `head`. Also record `git diff --quiet main -- Cargo.lock Cargo.toml '*/Cargo.toml'` exiting 0 (FR-011).
- [ ] T025 Open the PR. The body carries:
  - the repro table before and after;
  - the R2 decisions, with T010's counts;
  - SC-006's numbers;
  - the quality and root-only results;
  - the mutations, the gate and the follow-up issue numbers.

  It ends `Closes #1566`, then the attribution line. It says `Refs #1538` unless T019 shows 0 unexplained components with submodules checked out, in which case it says `Closes #1538` (memory: a partial fix closes the issue).

## Dependencies and execution order

- **Phase 1:** T001, T002 and T003 can run in parallel. T002's results gate the C-6 contract update.
- **US2 (T004–T010)** comes before US1, by design (plan § Phasing): T004 → T005, T006, T007, T008 → T009 → T010.
- **US1 (T011–T016):** T011 and T012 can run in parallel. Then T013 → T014 → T015 → T016. It needs T010 committed.
- **US3 (T017–T018)** comes after T016.
- **US4 (T019)** comes after T016, and needs T003.
- **Polish:** T020 and T021 come after T018, and can run in parallel with T019. T022 comes after T018. T023 can run any time after T010. T024, then T025, come last.

## Parallel examples

- **Phase 1:** T003 (fixture repository) runs alongside T001 and T002 (this repository).
- **US1:** T011 (submodule map) and T012 (pip merge) touch different files.
- **CI runs:** T019, T020 and T021 are separate workflows and run together.

## Implementation strategy

- **MVP = US2 + US1.** US2 alone already fixes #1566's format disagreement and false `complete`. Its C-2 attaches through the fallback once anchors stop closing the gate. US1 fixes C-3 (a `pyproject.toml` next to a `requirements.txt`) and the nested cases, which US2 cannot.
- **Each story ends at a corpus regeneration.** If T010's review stops on an unexplained or oversized change, US2 is reworked before US1 starts.
