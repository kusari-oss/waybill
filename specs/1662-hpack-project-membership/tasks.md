# Tasks: Project membership for `package.yaml`

**Input**: `specs/1662-hpack-project-membership/` (spec.md, plan.md, research.md, data-model.md, quickstart.md)
**Tests**: required (FR-007: a positive control, plus a mutation-tested test for each entry form).

## Phase 1: Setup (the baseline, before any code changes)

- [X] T001 Build `main`'s binary (`cargo build --release -p waybill` at the branch point, before any source edit) and copy it to the scratchpad as `waybill-main`. Record its `--version` and SHA in `specs/1662-hpack-project-membership/measurements/membership.md`.
- [X] T002 Fetch the measured targets at their corpus pins in `waybill-cli/tests/corpus_harness_195/manifest.rs`:
  - hpack-ogmios (`435189ae…`), hpack-hs-opentelemetry, hpack-lootbox, hpack-hs-temporal-sdk, haskell-language-server;
  - for every target that has a no-generated-cabal variant, a copy with the generated `*.cabal` files (those with `hpack` in their first 4 lines) moved aside, with the corpus rule from #1677: a `.cabal` is moved aside if `hpack` appears in its first 4 lines.

  Note any target that can't be fetched in `measurements/membership.md`; don't drop it.
- [X] T003 Scan each target and variant from T002 with `waybill-main sbom scan --offline`, default flags. Write each one's sorted unique `pkg:hackage/…` set to `specs/1662-hpack-project-membership/measurements/main/<target>.txt`, write its C200 (`waybill:hpack-read-outcome`) value to `measurements/main/<target>.c200.json`, and record the counts in `measurements/membership.tsv`. Check that `hpack-ogmios` contains `json-via-show` (the positive control for the baseline).

## Phase 2: Foundational

- [X] T004 In `waybill-cli/src/scan_fs/package_db/haskell.rs`:
  - read each `package.yaml` once, with `hpack::read_manifest`, before #1032's block;
  - keep the `(includes, read)` results keyed by path;
  - change the emit loop (currently `for path in &package_yaml_paths`, which calls `read_manifest`) to use them.

  No behaviour change. Check by running `cargo nextest run -p waybill haskell hpack` before and after, with the same pass count.
- [X] T005 Add `fn hpack_stand_in(path: &Path, read: &ManifestRead) -> PathBuf` in `waybill-cli/src/scan_fs/package_db/haskell.rs`. It returns `D/<name>.cabal`, where `<name>` is the manifest's `name:` if read, otherwise `D`'s own directory name (research R1). Expose the name from `haskell/hpack.rs` only if it isn't already reachable from the read result.

## Phase 3: User Story 1, an undeclared hpack package isn't reported (P1) 🎯 MVP

**Independent Test**: the fixture project's undeclared `package.yaml` isn't emitted, and its declared one is.

- [X] T006 [US1] Write the failing positive control first. It's a test in `waybill-cli/tests/hpack_project_membership.rs` that builds a temporary project with:
  - `cabal.project` containing `packages: ./pkgs/a`;
  - `pkgs/a/package.yaml` (`name: waybill-fixture-hpack-a`);
  - `pkgs/b/package.yaml` (`name: waybill-fixture-hpack-b`);
  - no `.cabal` files.

  It scans the project and asserts `pkg:hackage/waybill-fixture-hpack-a` is present and `…-b` is absent. Confirm it fails on the current code.
- [X] T007 [US1] In #1032's block in `waybill-cli/src/scan_fs/package_db/haskell.rs`:
  - pass the stand-ins (T005) together with `cabal_paths` to `cabal_files_declared_by`;
  - compute "nothing declared" over both kinds;
  - retain a `package.yaml` only if its stand-in isn't under `project_dir` or is declared;
  - log dropped `.cabal` and `package.yaml` counts separately in the existing info line;
  - record `hpack_outcome` entries (seen, includes, partial) and FR-001b warnings for kept manifests only.

  T006 now passes.
- [X] T008 [US1] Add unit tests in `haskell.rs`'s #1032 test module (near `same_line_and_optional_packages_are_both_membership`), one per R2 row, each comparing a lone-`package.yaml` directory with a lone-`.cabal` directory:
  - a directory, `./D` and `D/`;
  - a directory glob;
  - `.`, `./` and an empty entry (root only, not nested);
  - an explicit `D/<name>.cabal` (matches) and `D/other.cabal` (doesn't);
  - a `.cabal` glob;
  - an unnamed manifest that falls back to the directory name;
  - an unreadable manifest;
  - a `package.yaml` nested under a declared package's `test/data`, which isn't claimed.
- [X] T009 [US1] Mutation-test T006 and T008. Mutations:
  - (M1) stand-ins not passed to the matcher;
  - (M2) the stand-in uses the directory name always;
  - (M3) the glob built without `literal_separator(true)`;
  - (M4) "nothing declared" counts `.cabal` only.

  For each, record which tests fail in `measurements/mutations.md`. Every row in T008 must be killed by at least one mutation. Revert each with `cp`, check with `cmp`, and rebuild.

## Phase 4: User Story 2, nothing else changes (P1)

**Independent Test**: projects with no governing `cabal.project`, a stack-only project, and the corpus targets other than ogmios are unchanged.

- [X] T010 [P] [US2] Unit tests in `haskell.rs`:
  - a `package.yaml` with no `cabal.project`;
  - a `cabal.project` without `packages:`;
  - a stack-only project;
  - FR-005 both ways: entries matching only hpack packages drop the undeclared one, and entries matching nothing leave the tree alone;
  - a `package.yaml` beside its generated `.cabal`, declared, gives exactly 1 component;
  - the same pair undeclared gives 0;
  - a project with both `stack.yaml` and a `cabal.project` that omits a `package.yaml` directory drops it, the same as #1032 does for a `.cabal`;
  - a dropped manifest that has an unresolved `!include` appears in neither C200 nor the warnings.
- [X] T011 [US2] Build the branch binary, scan every target and variant from T002 with the same flags as T003, and write `measurements/branch/<target>.txt` and `.c200.json`. Diff both against `main/`. In `measurements/membership.md`, list every added or removed component with its reason. The expected result is that `json-via-show` is gone from ogmios and its variant, ogmios's C200 counts drop by that one manifest, and nothing else changes. Any other difference has to be explained or fixed.

## Phase 5: Polish

- [X] T012 Run the gate:

  ```sh
  ./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' --expect '^ +Summary \[ *[0-9.]+s\] [1-9][0-9]* tests? run: [1-9][0-9]* passed( \([0-9]+ slow\))?(, [0-9]+ skipped)?$' prepr -- ./scripts/pre-pr.sh
  ```

  Check for `GATE_RESULT=pass` and the summary line.
- [ ] T013 Open the PR (Refs #1685). Enumerate every CI job's conclusion.
- [ ] T014 Regenerate the corpus goldens through the `public-corpus.yml` dispatch on the PR branch. Read the diff, normalized and sorted, against T011's table: only `hpack-ogmios*` should change, by `json-via-show`. Commit the goldens.
- [X] T015 Update `docs/` or the module comment at #1032's block to say that the rule covers `package.yaml` via the stand-in. Add a "Recent Changes" line to CLAUDE.md.

## Dependencies

- T001–T003 come before any source edit.
- T004 → T005 → T006 → T007 → T008 → T009.
- T010 can run alongside T008, because it's a different test group, but after T007.
- T011 needs T007. T012 comes after all code. T014 needs the PR.

## Implementation Strategy

US1 (T006–T009) is the fix. US2 (T010–T011) is evidence that nothing else moved. Both ship in one PR.
