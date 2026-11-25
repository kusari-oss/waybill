# Tasks: cut serial test time, first slice

**Input**: `specs/1657-test-time-first-slice/` (spec, plan, research R1–R4, quickstart)
**Scope rule**: one gate, one PR (opened at the first green gate), no golden changes, side findings filed, not fixed.

## Phase 1: Setup (before-state)

- [X] T001 Build the main binary and record the before-state in `specs/1657-test-time-first-slice/measurements/before.md`:
  - **Affected binaries:** wall time for each one, run alone, min/median/max over 3 runs, on an idle machine (`sbom_edit_filter`, `holistic_parity`, the 17 harness includers, and the `scan-without-offline.txt` binaries).
  - **Edit times:** the moat SPDX 2.3 and SPDX 3 edit times (debug and release, 3 runs each).
  - **Test counts:** `--list` counts per binary. Explain the 145 `corpus_harness_195::` entries against the 56 `#[test]` attributes in the harness files.
  - **Whole suite:** one full `cargo test --workspace --no-fail-fast` log, with its per-binary durations.
  - **Reference binary:** keep a copy of the before-binary as `measurements/waybill-before` for T005. It is not committed.
- [X] T002 Measure the R3 enforcement point. Run one split scan without `--offline` and with `HTTPS_PROXY=http://127.0.0.1:9` / `HTTP_PROXY` set. Record:
  - whether it hangs, with its time;
  - whether the enrichment failure is visible in the document (C191 / C192);
  - the decision: adopt or drop.

## Phase 2: User Story 1, SPDX 3 edits take linear time (P1) 🎯 MVP

- [X] T003 [US1] Write `measurements/edit_identity.sh <before-bin> <after-bin>`. It edits, with both binaries:
  - every input file `tests/sbom_edit*.rs` edits, with each test's recipe (enumerated by hand into the script);
  - the five closure goldens' SPDX 3, SPDX 2.3 and CycloneDX under `--drop scope=excluded`.

  It then `cmp`s each pair and fails on any difference.
- [X] T004 [US1] Implement the one-pass `(subject, field) -> Vec<String>` index in `waybill-cli/src/edit/spdx3.rs`:
  - it replaces the per-call scans in `components()` and `doc_annotation`;
  - it keeps graph order;
  - it is built at each read, never cached across a mutation.
- [X] T005 [US1] Check FR-002 and FR-003:
  - `edit_identity.sh` shows every output byte-identical;
  - the moat SPDX 3 median is within 2× of SPDX 2.3, debug and release, 3 runs each;
  - `cargo test -p waybill --test sbom_edit_filter --test sbom_edit*` passes.

  Record the results in `measurements/after.md`.
- [X] T006 [US1] Mutation: make the index drop values for one field (for example, skip `waybill:sbom-tier`). An `sbom_edit*` test or `edit_identity.sh` must fail. Revert with `cp`, verify with `cmp`, and record in `after.md`.

## Phase 3: User Story 2, each test runs once (P2)

- [X] T007 [US2] Create `waybill-cli/tests/corpus_harness_self_tests.rs`. It includes the harness with `#[path]` and holds the `#[cfg(test)]` test modules moved out of `tests/corpus_harness_195/{cache,expectations,harness,js_filter,layer2_golden,manifest}.rs` (every file holding a `#[test]`). Items those tests reach become `pub(crate)` within the harness only.
- [X] T008 [P] [US2] Move `build_benchmark_fixture` and `ImageFile` into `waybill-cli/tests/common/`. Drop `mod dual_format_perf;` from `waybill-cli/tests/holistic_parity.rs`, and point both files at the common helpers.
- [X] T009 [US2] Write `measurements/list_counts.sh`. It checks `--list` across all test binaries:
  - `corpus_harness_195::` tests appear in exactly one binary, with 145 of them;
  - `dual_format_perf::` tests appear in exactly one binary, with 2 of them;
  - every other binary's count equals T001's, minus the removed duplicates.

  Run it and record the result in `after.md`.

## Phase 4: User Story 3, scan tests stay off the network (P3)

- [X] T010 [US3] Review each file in `measurements/scan-without-offline.txt` by hand, and write one row per file in `measurements/offline-review.md`. Each row gives:
  - whether the file really scans;
  - which helper it uses;
  - whether it needs live enrichment;
  - the disposition: offline, `--no-deps-dev` (closure helpers: never `--offline`, which refuses the closure tier), mocked, deliberately online with the reason, or not a scan.
- [X] T011 [US3] Apply the dispositions:
  - add the flag in `run_split_scan` (`waybill-cli/tests/scan_split_basic.rs`), in `tests/common/{nix_stub,installed_lib,cmake_fetch}.rs`, and in each reviewed file that scans directly;
  - if T002 adopted enforcement, set the unreachable proxy in those helpers too.
- [X] T012 [US3] Coverage:
  - for each test whose input changed (any assertion on enrichment-derived values), add a mutation or a mock showing it still fails on its defect, or record why its assertions do not depend on enrichment;
  - if enforcement was adopted, add the positive control: a helper-driven scan without the flag fails or reports the enrichment failure, as T002 measured.

## Phase 5: Polish

- [X] T013 Record the after-state in `measurements/after.md`:
  - per-binary min/median/max against T001's;
  - one full-suite log with its summed binary time against T001's, with no target;
  - no golden changed (`git diff main --stat -- waybill-cli/tests/fixtures` is empty);
  - zero new dependencies.
- [X] T014 Gate:
  - run `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` and check `GATE_RESULT=pass` and the per-target totals;
  - commit, push, and open the PR (Refs #1719, #1711, #1712, #1715, plus the footer);
  - enumerate every CI job;
  - record the CI lanes' test-step times against run 45613291165.
- [X] T015 Close-out: verify each task's claim against the code and the measurements, then tick it.

## Dependencies

- **Setup:** T001 and T002 come first. T002 informs T011.
- **US1:** T003 → T004 → T005 → T006.
- **US2:** T007 and T008 can run in parallel, then T009.
- **US3:** T010 → T011 → T012.
- **Independence:** the three stories touch different files and can run in any order.
- **Polish:** T013, then T014, then T015.

## Implementation Strategy

US1 alone removes the single slowest test and fixes the product defect. US2 and US3 then add the rest of the saving.
