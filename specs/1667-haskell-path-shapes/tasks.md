# Tasks: Haskell path values have the right shape

**Input**: specs/1667-haskell-path-shapes/ (spec, plan, research R1–R6, data-model, contracts/annotations.md)
**Tests**: required (spec FR-007, SC-005).

## Phase 1: Setup

- [X] T001 Write `specs/1667-haskell-path-shapes/measurements/measure.sh <golden-root> <label>`. For every `public_corpus/<target>/{cdx,spdx-2.3,spdx-3}.json` carrying C200, write `<label>/c200.tsv` with these columns: target, format, key, and each path. Also record:
  - the count of absolute paths;
  - `grep -c /home/runner` per file;
  - a grep for C244 (`waybill:project-discovery-dropped`), expected to find none.

  Run it on the committed goldens as `main`. Expect 27 files and 0 paths outside their root (research R1, R3).
- [X] T002 Build the US2 fixture in a scratch tree:
  - a root `package.yaml` named `waybill-fixture-root`;
  - nested hpack packages `local-packages/{a,c}/package.yaml`;
  - `local-packages/d/` holding a `package.yaml` and a hand-written `waybill-fixture-d2.cabal`.

  Scan it with a `main` build using `--offline --split=workspace` and `--split=directory`. Record each `split-manifest.json` in `measurements/split-main/`, and note its field layout in the quickstart. Expected: every Haskell `source_dir` is a file, and `d`'s two packages are two sub-SBOMs under `--split=directory` (SC-003, which this task measures).

## Phase 2: Foundational

- [X] T003 Confirm T001 and T002 reproduce spec Background: 27 files, `source_dir` a file, and the `d` case ungrouped. If they don't, amend the spec and research before any code changes.

## Phase 3: US1 — no host path in C200 (P1)

**Independent test**: `waybill-cli/tests/haskell_path_shapes.rs` US1 tests; unit tests in `haskell/hpack.rs`.

- [X] T004 [US1] Add unit tests to `waybill-cli/src/scan_fs/package_db/haskell/hpack.rs`:
  - `to_document_value(root)` turns `<root>/server/package.yaml` into `server/package.yaml`, and `<root>/package.yaml` into `package.yaml`;
  - `<root>/../shared/x.yaml` becomes `../shared/x.yaml`;
  - every key in contracts/annotations.md is relativised, including the `manifest` and `file` of an `includes-resolved` entry and of an `includes-unresolved` entry;
  - a three-component path below the root (`a/b/c.yaml`) is written with `/` separators, so the Windows lane checks the separator;
  - the key set and its order are unchanged.
- [X] T005 [US1] Implement it in `waybill-cli/src/scan_fs/package_db/haskell/hpack.rs`:
  - add `fn root_relative(root: &Path, p: &Path) -> String` (research R3: drop the common components, add `..` for each remaining root component). Build the string from `Path::components()` joined with `/`, never from `display()`, because Windows `canonicalize` yields `\\?\` paths with `\` separators;
  - give `to_document_value` a `root: &Path` parameter and pass every path through `root_relative`.

  In `waybill-cli/src/scan_fs/package_db/haskell.rs` `finalize`, call it with `std::fs::canonicalize(rootfs).unwrap_or_else(|_| rootfs.to_path_buf())`. Add a unit test asserting that, before relativising, every path `finalize` collects starts with that canonical root (research R2, which was otherwise inferred from a doc comment). Run T004 green.
- [X] T006 [US1] In `waybill-cli/src/cli/scan_cmd.rs` (around line 4653), pass `report.dropped_evidence_paths` to `amend_scope_dropped` without the `canonical_root.join`, and replace the comment with the spec 1667 rationale (research R4). Update the unit test at `haskell.rs` (around line 4326) to relative strings. Run `us3_symlinked_root_c200_matches` and confirm it still finds 2 (FR-004).
- [X] T007 [US1] Create `waybill-cli/tests/haskell_path_shapes.rs`, with:
  - **`c200_has_no_host_path`**: an hpack `waybill-fixture-*` project in a tempdir, with a nested package, an `!include` of a file inside the root, and an `!include` of a file above the scan root. Scan it in all three formats. Assert:
    - no C200 path in any format contains the tempdir prefix or starts with `/`;
    - the above-root include is under `includes-unresolved` with reason `outside-root`, with a relative `manifest` and `file` (research R3: no scan can put a `..` path into C200, so the `..` form is unit-tested in T004);
    - `common::parity::assert_parity` passes.

## Phase 4: US2 — `source_dir` is a directory (P1)

**Independent test**: `haskell_path_shapes.rs` US2 tests; unit tests in `split.rs`.

- [X] T008 [US2] Add unit tests to `waybill-cli/src/generate/split.rs`. `subproject_dir`:
  - returns the parent for `package.yaml`, for `a/a.cabal` (existing on disk), and for `crates/api/Cargo.toml` (listed, absent from disk);
  - returns `crates/api` unchanged for a directory;
  - returns empty for a root `package.yaml`.

  The existing `root_dir_*` tests in `project_discovery/mod.rs` must pass unchanged.
- [X] T009 [US2] Add `pub(crate) fn subproject_dir(rel: &Path, scan_root: &Path) -> PathBuf` in `waybill-cli/src/generate/split.rs`. It returns the parent when `is_manifest_basename(file_name)` matches or `scan_root.join(rel).is_file()`; otherwise it returns `rel`. Call it at the end of `source_dir_for` in place of the list-only branch. Make `project_discovery::root_dir` call `split::subproject_dir(&root.source_dir, scan_root)`, and update its doc comment (research R5). Run T008 green.
- [X] T010 [US2] Add to `waybill-cli/tests/haskell_path_shapes.rs`:
  - `split_source_dir_is_a_directory`: the T002 fixture under `--split=workspace`; every Haskell `source_dir` is a directory, empty for the root;
  - `two_main_modules_in_one_dir_group`: under `--split=directory`, `local-packages/d`'s two packages are one sub-SBOM.

## Phase 5: Polish

- [X] T011 [P] Update `docs/reference/sbom-format-mapping.md` C200: paths are relative to the canonical scan root with `/` separators, `..` above the root, never absolute. Then confirm the existing `split_*` tests and the `project_discovery_*` tests pass unchanged (FR-006).
- [X] T012 Mutation-test with `measurements/mutate.py`, which reverts with `copyfile` plus `utime` and checks with `cmp`, plus an unmutated control run. The mutations are:
  - M1: `root_relative` returns the path unchanged;
  - M2: no `..` (a path above the root loses its climb);
  - M3: the includes keys aren't relativised;
  - M4: `subproject_dir` drops the `is_file` test;
  - M5: the candidates are joined to `canonical_root` again.

  Each must fail at least one test. Record the results in `measurements/mutations.md`.
- [X] T013 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' --expect '^ +Summary \[ *[0-9.]+s\] [1-9][0-9]* tests? run: [1-9][0-9]* passed( \([0-9]+ slow\))?(, [0-9]+ skipped)?$' prepr -- ./scripts/pre-pr.sh`. Check `GATE_RESULT=pass` and the Summary line.
- [X] T014 Open the PR, then dispatch `gh workflow run public-corpus.yml -f branch=1667-haskell-path-shapes -f regen_goldens=true` and download `corpus-goldens-regen`.
  - Run `measure.sh` as `branch`.
  - Diff it against `main`, using a normalized, sorted diff. Explain every line in `measurements/branch.md`. The expected result: only the C200 line changes, in the 27 files, and 0 files contain `/home/runner`.
  - Re-run the T002 fixture with the branch build into `measurements/split-branch/`, and explain every `source_dir` and grouping difference from `split-main/` in `branch.md` (FR-001).
  - Commit only the goldens that are explained, and exclude the #1738 stray element if it recurs.
- [X] T015 Add the `CLAUDE.md` Recent Changes line by hand, and tick every task only after checking its claim.

## Dependencies

- T001–T003 come before any code change.
- US1 (T004–T007) and US2 (T008–T010) are independent.
- T012 needs both stories. T014 needs the PR open.

## Parallel

- T004 and T008 are in different files. T011 can run alongside the stories.

## MVP

US1 alone closes #1463 (SC-001).
