# Tasks: Every unreachable component on TheRock explains itself

**Input**: `specs/1541-explain-therock-orphans/` (`spec.md`, `plan.md`, `research.md`, `data-model.md`, `contracts/wire.md`, `quickstart.md`, `measurements/source-trace.md`)

**Tests are required** (SC-008). Each story's contract cases are written first, and must fail at `main` before the code that passes them.

**Conventions:**
- Synthetic `waybill-fixture-*` fixtures; no new Cargo dependency.
- TheRock in CI only. Its documents are probed locally with `probe_i7_on_document`.
- Revert mutations with `cp`.
- Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, run alone.
- Commits carry `Assisted-by: Claude:claude-opus-5-5`.

## Phase 1: Setup

- [X] T001 Trace TheRock's source tree (FR-001, SC-001): `probe_i7_on_document` in `waybill-cli/tests/public_corpus.rs` on `measure-source` 44149685219's `branch.sub.cdx.json`. Recorded in `specs/1541-explain-therock-orphans/measurements/source-trace.md`.
- [X] T002 Trace TheRock's SDK at `main`: `probe_i7_on_document` on `measure-full` run 44173368036's `branch.cdx.json`. Recorded in `specs/1541-explain-therock-orphans/measurements/sdk-trace.md`:
  - 645 unreachable by the completeness reason;
  - 644 nested components, which the I7 check never sees;
  - 1 top-level unexplained unowned binary.

  `measure-full` emits CycloneDX only, so its scan step gains SPDX 2.3 and SPDX 3 in T013, for FR-009.

## Phase 2: Foundational (blocks every story)

- [ ] T003 Write unit tests in `waybill-cli/src/generate/graph_completeness/mod.rs`, which must fail before T004:
  - for a fixture with an operator root override, a main module the override replaces, a resolve anchor and a workspace peer, `for_document`'s `reachable_set` equals what the CycloneDX builder's current preparation produces;
  - a child with `parent_purl` set to a reachable component is reachable;
  - a child of an unreachable container is not.
- [ ] T004 Implement `graph_completeness::for_document(components, relationships, &DocumentSubject, unread)` in `waybill-cli/src/generate/graph_completeness/mod.rs` (data-model):
  - move the preparation out of `waybill-cli/src/generate/cyclonedx/builder.rs`: root selection, `effective_components`, `anchor_retained_mainmod_edges`, `anchor_resolve_components` and workspace-peer edges;
  - add containment edges from `parent_purl` to each child.

  Make the CycloneDX builder, `waybill-cli/src/generate/spdx/document.rs` and `waybill-cli/src/generate/spdx/v3_document.rs` call it. Diff each emitter's former preparation against the extracted one first; a step one emitter had and another lacked is a finding, recorded here before it is unified.
- [ ] T005 Make `classify_orphans_pre_emit` in `waybill-cli/src/generate/orphan_reason.rs` call `for_document`. In `waybill-cli/src/cli/scan_cmd.rs`, pass it the scan's real root override and target version (the values `ScanArtifacts` is built from), instead of `RootComponentOverride::default()` and `"0.0.0"`.

**Checkpoint:** T003 passes. `cdx_regression`, `spdx_regression`, `spdx3_regression`, `cmake_fetch_wrappers`, `cmake_nested_fetches` and `declared_requirements` pass. A changed golden is traced before it is regenerated.

## Phase 3: User Story 2 — one reachability answer, in every format (P1)

**Goal:** the classifier, the document and I7 see one unreachable set, including for nested components, in three formats.
**Independent test:** O-1, O-2, O-8, O-9, with I-A and I-B on every case.

- [ ] T006 [US2] Create `waybill-cli/tests/orphan_explanations.rs`, using the `tests/common/cmake_fetch.rs` helpers where they fit:
  - **Cases:** O-1, O-2, O-8 and O-9 (`contracts/wire.md`).
  - **I-A:** assert it with the corpus harness's own `cdx_reachable` and `orphan_explained::is_explained`, reached through the harness path module, never a re-derived walk: no reachable component carries C45; every unreachable one carries an I7 marker; the completeness count equals the I7 population.
  - **I-B:** the formats agree.

  Run against `main`'s code (before T004/T005) and record which fail.
- [ ] T007 [US2] Extend the I7 check in `waybill-cli/tests/corpus_harness_195/invariants/checks/orphan_explained.rs`: walk nested `components[]`, and treat a component as reachable when its container is. Add unit tests beside the existing ones: a nested child of a reachable container is not an offender; a nested child of an unreachable container is, unless explained.
- [ ] T008 [US2] Fix whatever T006 shows still failing after T004–T007. Record each fix and its cause in this task's text.

## Phase 4: User Story 1 — every unreachable component on TheRock says why (P1) 🎯 MVP with US2

**Goal:** the binary codes, the uv reader's root and edges, and the remaining populations.
**Independent test:** O-3 to O-7 and O-10 to O-13; then TheRock (T013).

- [ ] T009 [US1] Add cases O-3, O-4, O-5 (including an unowned binary in a non-source scan), O-6, O-7, O-10, O-11, O-12 and O-13 to `waybill-cli/tests/orphan_explanations.rs`, and confirm they fail before T010 and T011.
- [ ] T010 [US1] Register `test-data-binary` and `unowned-binary` in C45's row in `docs/reference/sbom-format-mapping.md`. Give each its population and its exclusions, and place them below every existing code. Then implement them in `waybill-cli/src/generate/orphan_reason.rs`:
  - two `OrphanReasonCode` variants;
  - the closed test-data directory list (research R3), matched case-sensitively per path segment;
  - the "derived from binaries" predicate (data-model), fixed from TheRock's population;
  - placement after every existing arm, so FR-006 holds by position;
  - counters in `OrphanReasonCounts`, and in the info log.
- [ ] T011 [US1] In `waybill-cli/src/scan_fs/package_db/pip/uv_lock.rs` (research R4):
  - emit an editable root with no `version` as a versionless component, with `waybill:unresolved-reason` = `dynamic version in pyproject.toml; not resolved from the lock`;
  - wire its `dependencies` (runtime), `optional-dependencies` (optional, keeping `waybill:optional-derivation`) and `dev-dependencies` (development) edges, from every root, including one with a version;
  - add an edge to every listed version of a dependency named more than once under markers.

  Run spec 674's uv tests and the pip suites. Record every changed expectation, and why.
- [ ] T012 [US1] Re-probe the source-tree document at the branch (after T013's CI run). For each population in `source-trace.md` still unexplained (expected: none of the cargo, npm, conan or vcpkg ones after T005), trace it, and record the cause in `measurements/source-trace.md` under "At the branch". Covers `pkg:generic/tensile` and `pkg:generic/wkmi@1.0.0` (FR-007). A waybill defect is declared in `waybill-cli/tests/corpus_harness_195/invariant-declarations.toml` with a new issue, not given a code.

## Phase 5: User Story 3 — measured on TheRock, the corpora traced (P2)

**Goal:** SC-002 to SC-007.
**Independent test:** CI runs at the branch against `main`.

- [ ] T013 [US3] Push the branch. Extend `measure-full`'s scan step in the fixture repository's `.github/workflows/measure-full.yml` to emit SPDX 2.3 and SPDX 3 as well (FR-009). Dispatch `measure-source` (`--ref main -f waybill_ref=1541-explain-therock-orphans -f base_ref=main`) and `measure-full` (the same inputs) in the fixture repository. Probe both documents at each ref with `probe_i7_on_document`, and record in `specs/1541-explain-therock-orphans/measurements/therock.md`:
  - unexplained counts before and after (SC-002, SC-003);
  - the codes by count;
  - the completeness reason at both refs;
  - equality of the classifier, document and I7 counts (SC-004);
  - every component carrying a C45 code at `main` keeping it (SC-005).
- [ ] T014 [US3] Record `measure-source`'s interleaved medians and their ratio in `measurements/therock.md` (SC-007, ≤ 1.5×).
- [ ] T015 [US3] Corpora:
  - dispatch `public-corpus.yml` at the branch and at `main` together, and the same for `quality-corpus.yml`;
  - trace every golden change to a rule (R1 to R4), and only then regenerate with `-f regen_goldens=true`;
  - confirm the invariant gate (`invariant-must-pass.toml`) is green, and any I7 cell that moves is explained;
  - trace every quality-corpus movement PURL by PURL.

  Record in `measurements/corpus.md` (SC-006).

## Phase 6: Polish

- [ ] T016 Mutations: `specs/1541-explain-therock-orphans/measurements/mutate.py` and `mutate-run.sh`, recorded in `measurements/mutations.md`. At least these, each caught by a named test:
  - the classifier using its own preparation again;
  - containment edges removed;
  - containment edges reversed;
  - the I7 check ignoring nested components;
  - each new code's arm removed;
  - a directory dropped from the test-data list;
  - "every file" weakened to "any file";
  - a new arm moved above an existing code;
  - the uv editable root skipped;
  - dev edges dropped;
  - only one forked version wired.
- [ ] T017 [P] Docs:
  - **`docs/reference/sbom-invariants.md`, I7:** containment counts as reachability; the two new codes.
  - **`docs/reference/sbom-format-mapping.md`:** C45's row.
  - **`CHANGELOG.md`:** Fixed #1593, with TheRock's before and after.
- [ ] T018 Run the gate alone and record it in `specs/1541-explain-therock-orphans/measurements/gate-log.md`: the ok-line count, 0 failed, the expected line, `orphan_explanations` N/N, and `git diff --quiet main -- Cargo.toml Cargo.lock '*/Cargo.toml'` (FR-010).
- [ ] T019 Open the PR: `fix: every unreachable component on TheRock explains itself (#1593)`. The body carries the trace, the before and after, the corpora, the mutations and the reader change (`Refs #1214`), ending with the attribution line. After it lands, tick #1593 on #1542.

## Dependencies

- **Setup** is done.
- **Foundational:** T003 → T004 → T005, which blocks everything after.
- **US2:** T006 (tests first, which may start before T004) → T007 → T008.
- **US1:** T009 → T010 and T011 (separate files, so they can run in parallel) → T012, which needs T013's run.
- **US3:** T013 and T015 need US1 and US2 pushed. T014 comes from T013's run.
- **Polish:** T016 needs all code. T017 runs any time after T013. T018, then T019.

## Parallel examples

- T010 (`orphan_reason.rs`) and T011 (`uv_lock.rs`).
- T007 (the harness) beside T004 to T005.
- The CI dispatches in T013 and T015.

## Implementation strategy

**MVP = Foundational + US2 + US1 in one PR:** one reachability answer, the binary codes and the uv reader fix together take TheRock's source tree from 841 unexplained components towards 0. US3 measures it before the PR is opened.
