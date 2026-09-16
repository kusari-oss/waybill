# Tasks: Lockfile resolve graphs must be anchored to an owning component

**Feature**: `868-resolve-ownership` | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)
**Issue**: #887

## Format: `[ID] [P?] [Story?] Description`

- **[P]** — parallelizable: different file, no dependency on an incomplete task
- **[US1] / [US2] / [US3]** — the user story the task serves

## Path Conventions

Single Rust workspace. Production code under `waybill-cli/src/`, integration
tests under `waybill-cli/tests/`, docs under `docs/`.

**Tests are requested.** SC-007 requires every check to be observed *failing*
against the pre-change build before it is trusted, so each story carries an
explicit teeth-check task.

**The plan's gate is cleared**: FR-003a was amended from a prohibition to a
precedence rule before these tasks were written. Implementing against a
requirement known to be unimplementable was the thing to avoid.

---

## Phase 1: Setup

- [ ] T001 Build a release binary from the merge-base and keep it at `target/release/waybill-baseline` as the pre-change reference for every teeth-check and every A/B measurement in this feature
- [ ] T002 [P] Clone `lablup/backend.ai` @ `809fcd394dd8e39456986dd742e7d51c6aedd647` into a scratch dir and record the pre-change measurement per `quickstart.md` §1 — reachable-from-root, max depth, total edges, orphan count, and the per-resolve lifecycle classification table
- [ ] T003 [P] Record the steady-state scan-time baseline for that target (median of 3 after a warm-up). Research R6 measured 781/778/794 ms; the confirmation in T028 must be **interleaved** A/B on identical machine state, because a separately-taken comparison produced a false ~3% regression in milestone 867

---

## Phase 2: Foundational (Blocking Prerequisites)

**Scoping guard.** Seventeen of eighteen corpus targets declare no resolve.
If anything in this phase or the next moves one of them, the feature is not
scoped where it claims to be and Phase 4 (anchoring) is unsafe to land.

- [ ] T004 Parse tool sections' `install_from_resolve` back-references in `waybill-cli/src/scan_fs/package_db/pants/config.rs`, producing a resolve-name → declared-by-tool map. Currently unparsed — it appears only as an unhandled key in `pants_shell/config.rs`
- [ ] T005 Add a test in `waybill-cli/src/scan_fs/package_db/pants/config.rs` asserting the map is built from a `pants.toml` carrying both `[python.resolves]` entries and tool sections, using synthetic names (real coordinates trip the advisory scan)
- [ ] T006 Run `./scripts/pre-pr.sh` and confirm zero golden churn — parsing a key nobody reads yet must change no output (SC-004)

**Checkpoint**: the declaration is readable and nothing has changed.

---

## Phase 3: User Story 1 — A project's resolved dependencies are reachable from it (Priority: P1) 🎯 MVP

**Goal**: The resolve's contents become reachable from the document root.
`lablup/backend.ai` goes from 1 of 331 components reachable to covering the
resolves' contents.

**Independent test**: Scan that target and walk dependency edges from the
root; confirm the resolves' top-level requirements are reached.

### Tests for User Story 1

- [ ] T007 [P] [US1] Add a unit test in `waybill-cli/src/scan_fs/package_db/pants/mod.rs` asserting one resolve component is emitted per `[python.resolves]` entry, identified by the declared resolve name (FR-002, contract A-2)
- [ ] T008 [P] [US1] Add a unit test asserting a resolve component is emitted for a declared resolve and **not** emitted for a project declaring none (FR-007, contract A-7)
- [ ] T009 [P] [US1] Add an integration test in `waybill-cli/tests/` over a synthetic Pants fixture with two resolves, asserting each resolve's top-level requirements are reachable from the document root by following dependency edges (FR-001) — asserted against the emitted graph, never against the completeness annotation (contract A-8)
- [ ] T010 [US1] Teeth-check T007-T009 against `target/release/waybill-baseline` (SC-007) and record the observed failures in `specs/868-resolve-ownership/measurements/README.md`

### Implementation for User Story 1

- [ ] T011 [US1] Emit one resolve component per declared resolve in `waybill-cli/src/scan_fs/package_db/pants/mod.rs`, using `pkg:generic/` with the declared name (research R3). No edges yet
- [ ] T012 [US1] Verify at this point that component count rises by exactly the number of declared resolves and **no package component is created** (FR-008, SC-006, contract A-6) — the cheapest moment to catch an over-broad change, before any edge exists
- [ ] T013 [US1] Compute each resolve's top-level requirements — packages nothing else *within that resolve* depends on — in `waybill-cli/src/scan_fs/mod.rs`. Per-resolve, not global: a package can be top-level in one resolve and transitive in another
- [ ] T014 [US1] Emit the anchor edges root → resolve component → top-level requirements in `waybill-cli/src/scan_fs/mod.rs`, as ordinary dependency edges so consumers reach them without special handling
- [ ] T015 [US1] Re-run the T002 measurement and confirm reachable-from-root rises from 1 of 331, max depth rises above 1, and the document no longer reports itself flat (SC-001, SC-002)
- [ ] T016 [US1] Confirm the orphan count falls from 271 and **agrees with the emitted graph** rather than with the classifier's own report (FR-009, SC-003, contract A-8)
- [ ] T017 [US1] Confirm a package belonging to more than one resolve is reachable via each (FR-002b, FR-006, SC-006a)
- [ ] T018 [US1] Run `./scripts/pre-pr.sh` and enumerate the per-target `N passed; 0 failed` lines

**Checkpoint**: the defect is fixed on the target that holds the evidence, and the MVP is independently demonstrable.

---

## Phase 4: User Story 2 — The anchor says what it is (Priority: P2)

**Goal**: A consumer can tell a resolve component from a package, and an
anchor edge from a declared dependency.

**Independent test**: Inspect an emitted document and determine the nature of
the anchoring without reference to waybill's source.

### Tests for User Story 2

- [ ] T019 [P] [US2] Add an integration test in `waybill-cli/tests/` asserting a resolve component is identifiable as a resolve rather than a package (FR-002a, contract A-3) — a consumer must not try to fetch or vulnerability-scan it. Additionally assert the root → resolve edge is distinguishable from a declared dependency **via its target's marker**, which is how FR-004 is satisfied without a per-edge annotation
- [ ] T020 [US2] Teeth-check T019 and record the observed failure

### Implementation for User Story 2

- [ ] T021 [US2] Emit the resolve-nature marker on resolve components across all three formats under `waybill-cli/src/generate/`
- [ ] T022 [US2] Add the catalogue row to `docs/reference/sbom-format-mapping.md` **and** its matching entry in `waybill-cli/src/parity/extractors/mod.rs::EXTRACTORS` in the same commit — a row without an extractor fails `every_catalog_row_has_an_extractor` and `holistic_parity`. Record the Principle V justification naming the native field that was missing (research R4)
- [ ] T023 [US2] Run `./scripts/pre-pr.sh`

**Checkpoint**: the anchoring is self-describing in the document.

---

## Phase 5: User Story 3 — Multiple resolves stay distinguishable (Priority: P3)

**Goal**: Each package's originating resolve stays determinable, and
classification stops relying on a heuristic that is already misfiring.

**Independent test**: Scan a repository with several resolves and determine,
from the document alone, which resolve each package came from and how its
lifecycle was decided.

### Tests for User Story 3

- [ ] T024 [P] [US3] Add a test in `waybill-cli/src/scan_fs/package_db/pants/resolve_classifier.rs` asserting a declaration beats the name allowlist, including the case where the two disagree (FR-003a, contract A-4)
- [ ] T025 [P] [US3] Add a test asserting a resolve nothing declares falls to runtime **and** is counted as classified by weaker-than-declaration evidence (FR-003b, FR-003c, contract A-5)
- [ ] T026 [P] [US3] Add a test asserting the weak-evidence count is emitted **even when zero**, so "nothing needed guessing" and "the field is missing" stay distinguishable (FR-003c)
- [ ] T027 [US3] Teeth-check T024-T026, including feeding a document with the count absent to confirm the check fails rather than only catching a wrong value. Record all observed failures

### Implementation for User Story 3

- [ ] T028 [US3] Make the `install_from_resolve` declaration take precedence over `DEV_RESOLVE_NAMES` in `waybill-cli/src/scan_fs/package_db/pants/resolve_classifier.rs`, keeping the allowlist as a fallback for undeclared resolves (FR-003a, FR-003a-i). Do **not** widen the allowlist — research R1 deferred that deliberately, since patching `coverage-py` and `setuptools` into it would paper over why a name allowlist is the wrong instrument
- [ ] T029 [US3] Confirm on the measured target that `coverage-py` and `setuptools` move from runtime to build-time, and that **no resolve moves the other way** (SC-005a). Two live misclassifications corrected
- [ ] T030 [US3] Count resolves classified by anything weaker than a declaration and emit it document-scope across all three formats under `waybill-cli/src/generate/`, always present including zero (FR-003c)
- [ ] T031 [US3] Add the catalogue row and its extractor entry in the same commit, as T022
- [ ] T032 [US3] Verify per-package resolve attribution still holds — `waybill:pants-resolve` (C143) already ships on 248 of 272 pypi components, so FR-005/SC-005 are **verified, not built** (research R2)
- [ ] T033 [US3] Run `./scripts/pre-pr.sh`

- [ ] T034 [P] [US3] Add a test in `waybill-cli/tests/` asserting a glob-discovered lockfile that `[python.resolves]` does not name produces **no** resolve component and **no** anchor edge, and is counted as unanchored — a stem-derived resolve name is not a declaration of ownership (FR-003)
- [ ] T035 [US3] Emit the unanchored-lockfile count document-scope in `waybill-cli/src/generate/`, alongside the FR-003c weak-evidence count and always present including zero, reusing the same catalogue row added in T031 rather than adding a second

**Checkpoint**: multi-resolve attribution is intact and classification prefers evidence over guessing.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T036 Re-author `pants-backend-ai`'s expectations in `xtask/corpus/quality-corpus.toml` from a **CI** measurement, not a local one. Its current `edges 826..1010` / `max_depth 4..9` / `flat false` were authored against fabricated primary-dependency-fallback edges — scanning without `--root-name` still reproduces 918 today. Move only the bounds that actually violate
- [ ] T037 [P] Update `specs/868-resolve-ownership/measurements/README.md` with post-fix figures beside the baselines so the before/after pair stays reproducible
- [ ] T038 [P] Confirm the interleaved A/B scan-time comparison against the T003 baseline is within run-to-run noise, and record it
- [ ] T039 [P] Verify the walker-audit gate separately — it is not in `scripts/pre-pr.sh`. Expected to be a no-op since no file under `waybill-cli/src/scan_fs/walk*` is touched, but confirm rather than assume
- [ ] T040 [P] Comment on #887 and #870 with the outcome, and state in the PR body that `pants-backend-ai`'s bound moved because the old number counted fabricated edges — a reviewer comparing against it will otherwise read the drop as a regression
- [ ] T041 Open the PR with `./scripts/pre-pr.sh` green and every per-target `N passed; 0 failed` line enumerated

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (T001-T003)** — no dependencies; T002 and T003 run in parallel
- **Foundational (T004-T006)** — depends on Setup. Blocks US3; US1 and US2 do not need it
- **US1 (T007-T018)** — the MVP. Independent of Foundational
- **US2 (T019-T023)** — depends on US1 (there must be a resolve component to describe)
- **US3 (T024-T033)** — depends on Foundational (T004) for the declaration map; independent of US1 and US2
- **Polish (T036-T041)** — T036 depends on US1 landing; the rest on all stories

**Note on phase numbering.** plan.md describes five implementation phases;
these tasks use six, organised by user story per the template. The mapping:
plan Phase 1 (read the declaration) = tasks Phase 2 (Foundational); plan
Phase 2 (emit resolve components) = T011, inside US1; plan Phase 3 (anchor) =
T013-T014, also inside US1; plan Phase 4 = US2 plus parts of US3; plan Phase 5
= Polish. Both orderings satisfy the dependency graph.

### User Story Dependencies

US1 and US3 are mutually independent and can proceed concurrently once their
respective prerequisites land. US2 needs US1.

US3 is genuinely shippable alone: correcting `coverage-py` and `setuptools`
is a real fix even if nothing is ever anchored.

### Within Each User Story

Tests → teeth-check → implementation → measurement → gate.

The teeth-check is not ceremony. This repo has shipped a schema gate that
passed because its `$ref`s resolved to stubs, and a document reporting
`complete` over a graph it had itself filled in with fabricated edges.

### Parallel Opportunities

- T002 and T003 (Setup)
- T007, T008, T009 (US1 tests, different files)
- T024, T025, T026 (US3 tests)
- T037, T038, T039, T040 (Polish)
- **Whole user stories**: US1 and US3 concurrently, on separate branches

---

## Implementation Strategy

### MVP scope

**Phase 1 + US1.** That delivers the measured fix — a reachable dependency
graph on the target holding the evidence — and is independently demonstrable
via `quickstart.md`. US1 does not depend on the Foundational phase, so the
MVP is shorter than the phase numbering suggests.

### Incremental delivery

1. Foundational parses a key nobody reads yet — zero output change
2. US1 makes 760 already-correct edges reachable
3. US2 makes the anchoring self-describing
4. US3 corrects two live misclassifications and reports weak evidence
5. Polish re-authors the corpus expectation from the fixed behaviour

### What would falsify the design

If T006 or T012 shows any corpus target without resolves moving, the feature
is not scoped where it claims to be and anchoring must not land. Both are
cheap, early, and deliberately placed before the work that depends on them.
