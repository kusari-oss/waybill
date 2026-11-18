# Tasks: The same build gives the same answer, scan after scan

**Input**: `specs/1542-deterministic-binary-merge/` (`spec.md`, `plan.md`, `research.md`, `data-model.md`, `contracts/wire.md`, `quickstart.md`, `measurements/trace.md`)

**Tests are required** (SC-005, SC-008). Every story's contract cases are written first, and must fail at `main` before the code that passes them.
- **Repeatability tests scan each fixture at least 20 times.** The trace saw the old code vary in 4 of 12 scans, so fewer scans could pass by luck.
- **A repeatability test is shown to fail at `main`** by running it there and recording the count. A single green run is not that evidence.

**Conventions:**
- Synthetic `waybill-fixture-*` fixtures; no new Cargo dependency.
- TheRock in CI only. Revert mutations with `cp`.
- Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, run alone.
- Commits carry `Assisted-by: Claude:claude-opus-5-5`.

## Phase 1: Setup

- [X] T001 Trace (FR-001, SC-001): reproduce and attribute the variation. Recorded in `specs/1542-deterministic-binary-merge/measurements/trace.md`, with the probe at `measurements/probe_repro.rs`.
  - **Two causes**, each confirmed alone: the dedup key omits `file-sha256`; `add_containment_edges` reads the map it writes.
  - **The spec's premise is corrected:** the two `libgotcha.so` share a name, not a hash.

## Phase 2: Foundational (blocks every story)

- [X] T002 Create `waybill-cli/tests/scan_determinism.rs` with three helpers:
  - `scan_n(root, n, extra) -> Vec<Docs>`, which scans `n` times through `common::installed_lib::scan_with`;
  - `masked(&Docs) -> (Value, Value, Value)`, with the masking in data-model.md;
  - `assert_repeatable(&[Docs])`, which asserts one masked document per format and one `waybill:graph-completeness-reason`, and on failure prints the distinct variants with their scan indices. The comparison is byte-for-byte on the masked documents, array order included. A failure reports whether the scans differ only in order (the same multiset of array elements) or in content.

  Reuse the corpus harness's masking (`corpus_harness_195::layer2_golden::mask_nondeterministic`), made `pub(crate)` or reached through a `#[path]` include, as `orphan_explanations.rs` does. Never re-derive it.
- [X] T003 Port the trace's fixture builders into `waybill-cli/tests/scan_determinism.rs`:
  - `chain(root, dir, base, versions, bytes)`, which builds a symlink chain `base` → `base.<v1>` → … → final file;
  - `lib(paths, tag) -> Vec<u8>`, an ELF whose `.rodata` holds the embedded bundled-tree paths.

  Gate the file `#![cfg(unix)]`: symlinks, and the Windows clippy rule for platform shims.

**Done.** The control (`control_the_comparison_detects_a_difference`) found real variation in its first runs, at `main`. Two scans of **one** symlinked library differ in SPDX 2.3's `evidence.occurrences` annotation: the deduplicator merges occurrences in member order, and member order varies between scans. It failed in 2 of 3 runs. This is in the binary merge path, so it is fixed here (clarification), by T010's total order on group members.

**Checkpoint:** an empty fixture scanned 20 times is repeatable (a control that the masking does not hide a real difference: add a deliberate one and see it reported).

## Phase 3: User Story 2 — no identification loses its file (P1) 🎯 MVP with US1

**Goal:** different files with one name are different components (R1).
**Independent test:** D-1, D-2, D-3, D-7.

- [X] T004 [US2] Add contract cases D-1 (symlink chains to different bytes), D-2 (one name, same bytes), D-3 (one name, different bytes, no symlinks) and D-7 (same bytes, different names) to `waybill-cli/tests/scan_determinism.rs`. Each asserts:
  - the file components per name and hash;
  - every bundled tree is nested under its own file;
  - no identification is top-level (I-D);
  - repeatability, over 20 scans.

  Run at `main` and record which fail, and for D-1 how many of the 20 scans differ.
- [X] T005 [US2] Add the `file-sha256` qualifier to the dedup group key in `waybill-cli/src/resolve/deduplicator.rs` (`content_hash`, research R1; the doc comment cites #1600 and the SDK's `libgotcha.so`). Unit tests beside the existing ones:
  - two components with one name and different `file-sha256` stay two;
  - with the same `file-sha256` they merge;
  - two `deb` components that differ only in `?arch=` still merge (milestone 1064).
- [X] T006 [US2] Run `python_owned_binaries`, `installed_lib_embedded_paths`, `installed_lib_*`, `binary_*` and the three regression suites. Record any changed expectation and its cause in this task.
  - **At `main` (T004):**
    - D-1 fails: one file component where two are expected.
    - D-3 fails: the same merge, without symlinks.
    - D-7 fails repeatability: the occurrence-order variation, 12 scans against 8 of 20.
    - D-2 passes.
  - **After R1 (T005):** the unit tests pass, and D-1 and D-3 get two file components. D-1 still varies in occurrence order (9 variants in 20 scans) until T010.
  - **T006:** every suite passes, with no golden change: `python_owned_binaries` 22, `installed_lib_*` 18, `installed_test_scope` 7, the `binary_*` suites 23, the three regression suites 11 each, and `orphan_explanations` 146.

## Phase 4: User Story 1 — a scan of a fixed input says the same thing every time (P1)

**Goal:** reachability and the deduplicator's output are independent of order (R2, R3).
**Independent test:** D-4, D-6, and exhaustive permutation unit tests.

- [X] T007 [US1] Add contract cases to `waybill-cli/tests/scan_determinism.rs`, and record two variation counts: at `main` (both causes present), and at the branch after T005 and before T009 (only R2 missing), so the second count isolates R2:
  - **D-4:** D-1 plus an npm app. The binaries are unreachable, and their codes and the completeness reason are the same in all 20 scans. I-A holds in every scan, with the corpus harness's I7 functions.
  - **D-6:** a dpkg status file with one package at two `?arch=` values. The survivor is the same in all 20 scans.

  Record both counts.
- [X] T008 [US1] Write a unit test in `waybill-cli/src/generate/graph_completeness/mod.rs`: for a component set holding a top-level PURL copy and two nested copies (the trace's shape), every permutation of the component order (3! × the other components, at least 24 orders) gives one `reachable_set`. It must fail before T009.
- [X] T009 [US1] Make `add_containment_edges` in `waybill-cli/src/generate/graph_completeness/mod.rs` read rule R2's edges from a snapshot taken before the loop (research R2). The comment cites #1600 and says this was spec 1541's defect.
- [X] T010 [US1] Make the deduplicator deterministic in `waybill-cli/src/resolve/deduplicator.rs` (research R3):
  - **Output order:** the final sort (deduplicator.rs:324) is completed to `(purl, parent_purl, content hash, first source path)`.
  - **Survivor:** chosen by confidence, then PURL string, then first source path.
  - **Tests:** a unit test feeds one input set in every permutation (at least 24) and asserts one output, order included.
- [X] T011 [US1] Run every suite T006 ran, plus `orphan_explanations`, `public_corpus` and the closure suites. Trace any change.
  - **T007:**
    - **D-4:** I-A held in every scan both at `main` and after R1. Its binaries are unreachable either way, so containment order cannot change the answer. Both states varied only in SPDX 2.3 occurrences (8 variants in 20 scans each). R2's own failure is isolated by T008 instead.
    - **D-6** passes.
  - **T008:** fails before T009, with reachability taking more than one value over 120 component orders, and passes after.
  - **T010, refined by the suites.** A within-group tie-break by first path overrode the installed-library reader's deliberate `.pc`-before-CMake precedence (`installed_lib_us1`, spec 1531 SC-003), so it was removed:
    - the survivor follows the stable confidence order, as before;
    - the merged `source_file_paths` and `source_connection_ids` are sorted, like occurrences and requirement ranges already are;
    - the output sort is completed to a total order.

    The variation the tests found was between same-PURL groups (SPDX 2.3 takes the first copy's annotations), which the output sort fixes. The permutation test fails with a PURL-only output sort (3 of 3 runs), and passes with the fix.
  - **T011:** 27 integration suites and 96 unit tests, all clean, with no golden change; clippy clean.

## Phase 5: User Story 3 — one identity in every format (P2)

**Goal:** the three formats report one component set (FR-004).
**Independent test:** D-5.

- [ ] T012 [US3] Add D-5 to `waybill-cli/tests/scan_determinism.rs`: one bundled tree named by two different binaries. Through `common::installed_lib::row_all_formats`, the three formats agree on C45, C104 and C105, and the (PURL, container) pairs are equal across the formats: read from CycloneDX nesting and from SPDX `CONTAINS`, with two pairs for the tree. Research R4 expects no SPDX 2.3 change. If D-5 fails after T005 and T009, the cause is traced and recorded here before any format change.

## Phase 6: The determinism audit (FR-006) and measurement

- [ ] T013 Add repeat-and-compare to the corpus harness (`waybill-cli/tests/corpus_harness_195/harness.rs`). With `WAYBILL_CORPUS_REPEAT=N`, each target is scanned `N` times, and its masked documents compared through `layer2_golden`'s masking. The comparison includes array order. A content difference fails the target with class `nondeterministic-output`, and an order-only difference with `nondeterministic-order`, each naming the first differing JSON path.
  - Add a `repeat` input to `.github/workflows/public-corpus.yml`, passed to the environment.
  - Unit-test the comparison with two hand-built documents that differ in one masked field (passes) and in one unmasked field (fails).
- [ ] T014 In the fixture repository's `.github/workflows/measure-full.yml`, keep every scan's documents: `out/{side}-{tag}.cdx.json` and the SPDX equivalents, not only the last. This needs nothing else from the workflow.
- [ ] T015 Push the branch. Dispatch:
  - `public-corpus.yml` with `repeat=2`, at the branch and at `main`;
  - `measure-full` (`--ref main -f waybill_ref=1542-deterministic-binary-merge -f base_ref=main`), at least twice, for at least 8 branch scans;
  - `measure-source` once;
  - `quality-corpus.yml` at the branch and at `main`.
- [ ] T016 Record in `specs/1542-deterministic-binary-merge/measurements/audit.md`:
  - every corpus target's repeat result, at both refs;
  - the SDK's masked documents, compared across all branch scans and against `main`'s;
  - `gotcha`'s outcome in every scan;
  - I-A per scan, with `probe_i7_on_document` per document;
  - the formats' agreement;
  - the interleaved medians of both `measure-full` and `measure-source` against `main`, each at most 1.1× (SC-007).

  Any variation outside the binary path is filed as its own issue and named here (clarification).
- [ ] T017 Trace every golden change, regenerate with `-f regen_goldens=true` only after tracing, and record in `measurements/corpus.md`. Trace every quality-corpus movement against `main`.

## Phase 7: Polish

- [ ] T018 Mutations: `specs/1542-deterministic-binary-merge/measurements/mutate.py` and `mutate-run.sh`, recorded in `measurements/mutations.md`. At least these, each caught by a named test:
  - the `file-sha256` field dropped from the key;
  - the snapshot reverted to the live map;
  - group order back to `HashMap` order;
  - the survivor tie-breaks removed;
  - the content-hash parse returning `None`.
- [ ] T019 [P] Docs:
  - **`CHANGELOG.md`:** Fixed #1600, with the trace's two causes and the SDK result.
  - **`docs/reference/sbom-invariants.md`, I7:** a sentence that reachability is order-independent and a test pins it.
- [ ] T020 Run the gate alone and record it in `specs/1542-deterministic-binary-merge/measurements/gate-log.md`, with `git diff --quiet main -- Cargo.toml Cargo.lock '*/Cargo.toml'` (FR-008).
- [ ] T021 Open the PR: `fix: the same build gives the same answer, scan after scan (#1600)`, ending with the attribution line. After it lands, tick #1600 where it is tracked.

## Dependencies

- **Setup** is done.
- **Foundational:** T002 → T003 block every story.
- **US2:** T004 (tests first) → T005 → T006.
- **US1:** T007 → T008 → T009 → T010 → T011. US1 needs T005 for D-4's expected outcome; its test fails at `main` regardless.
- **US3:** T012 after T005 and T009.
- **Audit:** T013 and T014 can run in parallel. T015 needs all code. T016 and T017 come from T015's runs.
- **Polish:** T018 after all code; T019 any time after T016; T020, then T021.

## Parallel examples

- T013 (harness) beside T004 to T010 (other files).
- T014 (fixture repository) any time.
- T019 (docs) beside T018.

## Implementation strategy

**MVP = US2 + US1 in one PR.** Neither cause alone gives repeatable *and* correct documents. R1 removes the detached identification; R2 and R3 make what remains independent of order. The audit (Phase 6) runs before the PR is opened.
