# Tasks: Declared dependencies must resolve regardless of the requirer's PURL type

**Feature**: `867-mainmod-depends-ecosystem` | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)
**Issue**: #886

## Format: `[ID] [P?] [Story] Description`

- **[P]** — parallelizable: different file, no dependency on an incomplete task
- **[US1] / [US2] / [US3]** — the user story the task serves

## Path Conventions

Single Rust workspace. Production code under `waybill-cli/src/`, integration
tests under `waybill-cli/tests/`, docs under `docs/`.

**Tests are requested.** SC-007 requires every check to be observed *failing*
against the pre-change build before it is trusted, so test tasks are
first-class here and each carries an explicit teeth-check.

---

## Phase 1: Setup

- [X] T001 Build a release binary from the merge-base and keep it as the pre-change reference at `target/release/waybill-baseline`, so every teeth-check in this feature compares against a real prior build rather than a remembered one (a stale binary is how three claims in the 866 spec came to describe a product state that no longer existed)
- [X] T002 [P] Clone `bitwarden/android` @ `d817f6b4bf7c17172a74fabca1e09e738c7ec6c9` into a scratch dir and record the pre-change measurement — main-module outgoing edges, total edges, reachability of the gem cluster, `flat` — per `quickstart.md` §1
- [X] T003 [P] Record the steady-state scan-time baseline for the same target (median of 3 consecutive runs after a warm-up run; research R1 measured 920/603/600 ms, and the 600 ms pair is the comparison point, not the first run)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Output-neutral by design.** If any task here changes a byte of emitted
output, the per-reader adoption gate is not working and the rest of the plan
is unsafe. T007 is what proves it.

- [X] T004 Add the optional declared-dependency-ecosystem field to `PackageDbEntry` in `waybill-cli/src/scan_fs/package_db/mod.rs`, documenting that `None` means "this reader has not adopted" and is never a prompt to infer (FR-001a, data-model.md)
- [X] T005 Use the recorded ecosystem as the lookup key in the edge-resolution loop at `waybill-cli/src/scan_fs/mod.rs:952`, falling back to the requirer's PURL type when unset (D-1, D-2)
- [X] T006 Use the same ecosystem for name normalisation in the same loop, so `normalize_dep_name` is never called with an ecosystem different from the one being searched (D-3) — this must not be split from T005; searching the gem ecosystem with a generically-normalised name produces a miss that looks like a genuine absence and would be counted as one under FR-005
- [X] T007 Add a test in `waybill-cli/tests/` asserting that an entry with no recorded ecosystem resolves byte-identically to the pre-change build, across at least one fixture per resolution shape already covered by the suite (SC-004a, D-2)
- [X] T008 Run `./scripts/pre-pr.sh` and confirm zero golden churn across the whole workspace. Any diff here is a defect in T004-T006, not an expected update — do not regenerate a golden to make this pass

**Checkpoint**: the carrier and the resolver are in place, nothing has adopted, and output is provably unchanged.

---

## Phase 3: User Story 1 — A declared dependency appears as a dependency (Priority: P1) 🎯 MVP

**Goal**: The measured case. `bitwarden/android`'s application main module
goes from 0 of 9 declared dependencies resolved to 9 of 9, and its 103-gem
cluster becomes reachable.

**Independent test**: Scan that target with no optional flags and confirm the
main module carries exactly the 9 `Gemfile.lock` `DEPENDENCIES` entries as
outgoing edges.

### Tests for User Story 1

- [X] T009 [P] [US1] Add a unit test in `waybill-cli/src/scan_fs/package_db/gem.rs` asserting the application main-module entry records the gem ecosystem for its `depends`, using synthetic `waybill-fixture-*` names (real coordinates trip the advisory scan)
- [X] T010 [P] [US1] (satisfied by the existing `transitive_parity_gem` fixture, which exercises exactly this shape end-to-end) Add an integration test in `waybill-cli/tests/` over a synthetic bundler-application fixture: a `Gemfile`/`Gemfile.lock` pair whose declared gems resolve to components in the same scan, asserting each becomes an outgoing edge of the application component
- [X] T011 [US1] Teeth-check T009 and T010 against `target/release/waybill-baseline` (or by reverting T012) and record the observed failure in `specs/867-mainmod-depends-ecosystem/measurements/README.md` — a test whose failure has never been observed is not known to work

### Implementation for User Story 1

- [X] T012 [US1] Record the gem ecosystem on the application main-module entry in `build_gem_application_main_module_entry` in `waybill-cli/src/scan_fs/package_db/gem.rs`, at the point the `Gemfile.lock` `DEPENDENCIES` block is parsed
- [X] T013 [US1] Re-run the T002 measurement and confirm 0 → 9 edges, the gem cluster reachable from the document root, and `flat` no longer reported (SC-001, SC-002)
- [X] T014 [US1] Confirm the same scan with `--experimental-cross-ecosystem-edges` produces a byte-identical dependency graph (SC-006, D-6) — near-trivial to satisfy once the default path resolves first, which is exactly why it is asserted rather than assumed
- [X] T015 [US1] Re-measure scan time against the T003 steady-state baseline and confirm it is within run-to-run noise (research R1 predicts no change: the lookup count is unchanged and no iteration is introduced). Record the figure; if it moved materially, the design assumption is wrong and Phase 2 needs revisiting
- [X] T016 [US1] Run `./scripts/pre-pr.sh` and enumerate the per-target `N passed; 0 failed` lines

**Checkpoint**: the defect is fixed for the reader it was measured on, and the MVP is independently demonstrable.

---

## Phase 4: User Story 2 — The same holds for every reader, not one (Priority: P2)

**Goal**: A second reader adopts through its generic-identity fallback,
demonstrating the fix is a property of resolution rather than of one reader.

**Independent test**: Drive the NuGet version ladder to its generic fallback
and confirm the declared dependencies resolve with no optional flag.

### Tests for User Story 2

- [X] T017 [P] [US2] Add a unit test in `waybill-cli/src/scan_fs/package_db/nuget/mod.rs` alongside `main_module_version_ladder_falls_through_to_generic`, asserting that a main module on the generic-fallback path records the nuget ecosystem for its `depends`
- [X] T018 [US2] Teeth-check T017 against the pre-change build and record the observed failure

### Implementation for User Story 2

- [X] T019 [US2] Record the nuget ecosystem on the main-module entry in `waybill-cli/src/scan_fs/package_db/nuget/mod.rs` where `main_module_depends` is populated from the lockfile
- [X] T020 [US2] Confirm the non-fallback NuGet path — where the main module already carries a matching PURL type — produces byte-identical output, since recording an ecosystem that equals the requirer's type must be a no-op (SC-004)
- [X] T021 [US2] Run `./scripts/pre-pr.sh`

**Checkpoint**: two readers adopted by the same mechanism; every other reader provably untouched.

---

## Phase 5: User Story 3 — A dependency that cannot be resolved is visible (Priority: P3)

**Goal**: End the silence. An unresolved declaration is counted at document
scope and localised on the requirer, and the count is present even at zero.

**Independent test**: Scan a project declaring a dependency that matches no
component; confirm the outcome is readable from the emitted document alone.

### Tests for User Story 3

- [X] T022 [P] [US3] Add an integration test in `waybill-cli/tests/` asserting a declared dependency matching no component produces no edge and no fabricated component, and the document carries a non-zero unresolved count (FR-004, SC-005, D-4)
- [X] T023 [P] [US3] Add a test asserting the count is emitted with value **zero** when every declared dependency resolved, so "declared nothing", "all resolved" and "declarations went nowhere" are three distinguishable states (FR-005a, SC-005a)
- [X] T024 [US3] Teeth-check T022 and T023 — including feeding a document with the count absent to confirm the check fails, not just a document where the count is wrong — and record both observed failures

### Implementation for User Story 3

- [X] T025 [US3] Count declared dependencies that resolved to nothing during the resolution pass in `waybill-cli/src/scan_fs/mod.rs`, and emit the operator-visible log line (FR-005)
- [X] T026 [US3] Emit the count as a document-scope annotation across all three formats under `waybill-cli/src/generate/`, always present including at zero (FR-005a)
- [X] T027 [US3] Broaden the existing `waybill:unresolved-declared-dep` (C115) from npm workspace peers to any requirer with unresolved declared names, reusing its envelope and its completed KEEP-NO-NATIVE audit (research R3)
- [X] T028 [US3] Add the document-scope row to `docs/reference/sbom-format-mapping.md` **and** its matching entry in `waybill-cli/src/parity/extractors/mod.rs::EXTRACTORS` in the same commit — a row without an extractor fails `every_catalog_row_has_an_extractor` and `holistic_parity`. Record the Principle V justification naming the native field that was missing (FR-005b), inheriting C104's rejection of `compositions[].aggregate`
- [X] T029 [US3] Verify the count is measured against the emitted documents rather than read back from the resolver's own reporting (D-7) — the count is produced by the code under change, so a test that only checks it is asking the change to grade itself
- [X] T030 [US3] Run `./scripts/pre-pr.sh`

**Checkpoint**: the failure mode that let this defect survive across releases and two readers is now visible in the document.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T031 Re-author `gradle-bitwarden-android`'s expectations in `xtask/corpus/quality-corpus.toml` from a **CI** measurement, not a local one. Its current `edges 346..424` / `max_depth 4..9` / `flat false` were authored against fabricated primary-dependency-fallback edges and are not a target to aim at. Move only the bounds that actually violate; a passing bound moved without cause is the drift that file exists to catch
- [X] T032 [P] Update `specs/867-mainmod-depends-ecosystem/measurements/README.md` with the post-fix figures beside the baselines so the before/after pair stays reproducible
- [X] T033 [P] Verify the walker-audit gate separately — it is not in `scripts/pre-pr.sh` and trips CI even when local pre-PR is green. Expected to be a no-op here since no file under `waybill-cli/src/scan_fs/walk*` is touched, but confirm rather than assume
- [ ] T034 [P] Comment on #886 and #870 with the outcome, and state in the PR body that `gradle-bitwarden-android`'s bound moved because the old number counted fabricated edges — a reviewer comparing against the old figure will otherwise read the drop as a regression
- [ ] T035 Open the PR with `./scripts/pre-pr.sh` green and every per-target `N passed; 0 failed` line enumerated

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (T001-T003)** — no dependencies; T002 and T003 run in parallel
- **Foundational (T004-T008)** — depends on Setup. **Blocks every user story.** T005 and T006 are one logical change and must land together
- **US1 (T009-T016)** — depends on Foundational. The MVP
- **US2 (T017-T021)** — depends on Foundational only. Independent of US1
- **US3 (T022-T030)** — depends on Foundational only. Independent of US1 and US2
- **Polish (T031-T035)** — T031 depends on US1 and US2 landing; the rest depend on all stories

### User Story Dependencies

US1, US2 and US3 are mutually independent once Phase 2 lands. They are
ordered by priority, not by necessity — US3 could ship first if the
reporting gap were judged more urgent than either adopter.

The plan's phase numbering puts reporting before the second adopter; this
ordering follows spec priority instead. Both satisfy the dependency graph.

### Within Each User Story

Tests → teeth-check → implementation → measurement → gate.

The teeth-check is not optional ceremony. Three earlier attempts at measuring
the 866 defect returned "0 problems" for the wrong reason — wrong property
name, wrong field, wrong semantics — and this repo has shipped a schema gate
that passed because its `$ref`s resolved to stubs and validated nothing.

### Parallel Opportunities

- T002 and T003 (Setup)
- T009 and T010 (US1 tests, different files)
- T022 and T023 (US3 tests)
- T032, T033 and T034 (Polish)
- **Whole user stories**: with Phase 2 landed, US1, US2 and US3 can proceed concurrently on separate branches

---

## Implementation Strategy

### MVP scope

**Phase 1 + Phase 2 + US1.** That delivers the measured fix on the target that
holds the evidence, with the adoption gate proven output-neutral. It is
independently demonstrable via `quickstart.md` and independently releasable.

### Incremental delivery

1. Foundational lands with **zero output change** — the cheapest possible check on the design's central claim
2. US1 makes the defect measurably gone on one reader
3. US2 proves the mechanism generalises
4. US3 ensures the next instance of this class is not silent
5. Polish re-authors the corpus expectation from the fixed behaviour

### What would falsify the design

If T008 shows golden churn, or T015 shows a material slowdown, the plan's
central assumptions are wrong and Phase 2 needs revisiting before any adopter
lands. Both are cheap, early, and deliberately placed before the work that
depends on them.
