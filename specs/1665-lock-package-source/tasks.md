# Tasks: Haskell lock-file packages record their lock file

**Input**: `specs/1665-lock-package-source/` (spec.md, plan.md, research.md, data-model.md, contracts/source-files.md)
**Tests**: required by spec FR-009 (positive control plus mutation testing).

## Phase 1: Setup

- [X] T001 Measure `main` with `specs/1665-lock-package-source/measurements/measure.sh` and write the results to `measurements/main.tsv` and `measurements/main/` (research R1). Done at plan time.

## Phase 2: Foundational

- [X] T002 In `waybill-cli/src/scan_fs/package_db/haskell.rs`:
  - add `source: PathBuf` to `StackLockEntry` and to `CabalFreezeEntry::ExactPin`;
  - set it in `parse_stack_lock` and `parse_cabal_freeze` from the parsed path;
  - fix every constructor, including the unit tests (`build_freeze_component_*` and others), so the crate compiles.

## Phase 3: User Story 1 — every lock-pinned package names its lock file (P1) 🎯 MVP

**Independent test**: in the fixture, every lock- and freeze-pinned component lists exactly its files.

- [X] T003 [US1] Make `build_stack_lock_component` and `build_freeze_component` in `haskell.rs` set `source_path` from `entry.source`.
- [X] T004 [US1] In Phase F of `haskell.rs`, replace first-wins for lock and freeze components with a merge keyed by PURL across both loops. A repeat PURL appends its `source_path` to the first component's `extra_source_paths`, sorted and unique. Keep inserting the PURL into `seen_purls` (research R3).
- [X] T005 [P] [US1] Add unit tests in `haskell.rs` that `parse_stack_lock` and `parse_cabal_freeze` set `source` to the path they parsed.
- [X] T006 [US1] Add the integration test `waybill-cli/tests/lock_package_source.rs` with the plan's fixture (`a/`, `b/` and `c/` `stack.yaml.lock`, `d/cabal.project.freeze`, which also pins `waybill-fixture-reached ==1.0.0`). Assert the exact source-files set for each component:
  - `reached@1.0.0` → `a`, `b` and `d` (a merge across the freeze and stack-lock loops);
  - `unreached@1.0.0` → `a`;
  - `split@2.0.0` → `b`;
  - `split@3.0.0` → `c`;
  - `frozen@4.0.0` → `d`.
  
  No value is `[""]`. Check `parse_cabal_freeze`'s accepted syntax before writing `d/cabal.project.freeze`. If it doesn't accept a form, write the form it does accept; don't widen the parser in this spec.

## Phase 4: User Story 2 — nothing else moves without a reason (P1)

**Independent test**: the unreached pin is still `edgeless-lock-pin`, and the corpus diff is fully explained.

- [X] T007 [US2] In `tests/lock_package_source.rs`, assert that `waybill-fixture-unreached@1.0.0` is unreachable with orphan reason `edgeless-lock-pin`, and that `reached@1.0.0` is reachable.
- [X] T008 [US2] In `tests/lock_package_source.rs`, scan the fixture in SPDX 2.3 and SPDX 3 as well, and assert that the source-files value of each lock component is identical to the CycloneDX one (FR-008). *As implemented:* `common::parity::assert_parity`, which checks every catalog row, C18 included, across the three formats.
- [X] T009 [US2] Build release, run `measure.sh` with label `branch`, and diff `branch.tsv` and `branch/` against `main`. Record every difference in `research.md` under "R6 — Branch measurement". The prediction is that only the source-files column changes, and that `no_source_file` becomes 0.
- [X] T010 [US2] Run the mutations with a `cp` backup, a `cmp` revert and a rebuild, and record them in `measurements/mutations.md`:
  - M1: no stack-lock source;
  - M2: no freeze source;
  - M3: merge reverts to first-wins;
  - M4: merge keyed by name;
  - M5: extra paths unsorted;
  - M6: merge only within the stack-lock loop, which the `reached` → `a`, `b`, `d` assertion kills.
  
  Commit the script as `measurements/mutate.py`, with no local paths.

## Phase 5: Polish

- [X] T011 [P] Add a one-sentence note to the C18 row in `docs/reference/sbom-format-mapping.md` saying that Haskell lock and freeze pins list their files (spec 1665). Add a CLAUDE.md "Recent Changes" line by hand.
- [X] T012 File the side finding from research R4 as an issue: an unreachable `hackage-freeze` pin has no orphan reason. Use the attribution footer.
- [X] T013 Run the gate (`./scripts/gate.sh … prepr -- ./scripts/pre-pr.sh`), and check `GATE_RESULT=pass` and the Summary line.
- [X] T014 Commit and open the PR ("Refs #1736"). Dispatch `public-corpus.yml` with `regen_goldens=true`. Commit only the hs-opentelemetry and lootbox golden changes that R6 explains, check that no invariant declaration moved, and check for the #1738 agent.

## Dependencies

T002 → T003, T004 → T005–T008 → T009, T010 → T011–T014. T005 ∥ T006.

## Strategy

US1 and US2 ship together, as one PR.
