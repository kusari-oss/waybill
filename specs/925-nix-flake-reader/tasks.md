# Tasks: Read Nix flake.lock inputs as pinned components

**Feature**: `925-nix-flake-reader` | **Spec**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md)

## Format: `[ID] [P?] [Story] Description`

- `[P]` — parallelizable: touches different files, no dependency on an incomplete task
- `[US1]`/`[US2]`/`[US3]` — user story this task serves (user-story phases only)

## Path Conventions

Single Rust workspace. Reader at `waybill-cli/src/scan_fs/package_db/nix/`,
tests at `waybill-cli/tests/`, fixtures at `waybill-cli/tests/fixtures/nix/`.

---

## Phase 1: Setup (Shared Infrastructure)

- [X] T001 Create the reader module skeleton at `waybill-cli/src/scan_fs/package_db/nix/mod.rs` with `lockfile` and `identity` submodules declared
- [X] T002 Declare the `nix` module in `waybill-cli/src/scan_fs/package_db/mod.rs` alongside the existing readers
- [X] T003 Register a `ReaderRegistration` matching `flake.lock` in `waybill-cli/src/scan_fs/package_db/nix/mod.rs`, with no `on_dir` and no `descend_into` override (FR-001, research R6)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Blocks every user story.** Nothing below can be built on a parser that cannot
represent a `follows` alias.

- [X] T004 [P] Define `LockedRef` and `OriginalRef` in `waybill-cli/src/scan_fs/package_db/nix/lockfile.rs` per data-model.md Entities 3 and 4
- [X] T005 [P] Define `FlakeLockDocument` and `FlakeNode` in `waybill-cli/src/scan_fs/package_db/nix/lockfile.rs` per data-model.md Entities 1 and 2
- [X] T006 Define `InputEdge` in `waybill-cli/src/scan_fs/package_db/nix/lockfile.rs` as a discriminated type — `NodeRef(String)` from a JSON string, `Follows(Vec<String>)` from a JSON array (research R3, Principle IV)
- [X] T007 Implement `parse_flake_lock` in `waybill-cli/src/scan_fs/package_db/nix/lockfile.rs`, rejecting `version != 7` as `UnrecognisedVersion` and invalid JSON as `Malformed`, distinguishing the two in the diagnostic only (FR-010, data-model.md Entity 1 state)
- [ ] T008 [P] Add verbatim real-format fixtures under `waybill-cli/tests/fixtures/nix/`: a single-`github`-input lockfile, a `tarball` input carrying `rev` but no owner/repo, a `follows` alias with an array-valued `inputs` entry, and a non-root node declaring its own inputs — copied from the lockfiles measured in research.md, not hand-written
- [ ] T009 [P] Add constructed failure fixtures: `waybill-cli/tests/fixtures/nix/malformed/flake.lock` (invalid JSON), `.../unknown-version/flake.lock` (`version: 99`), `.../no-inputs/flake.lock`, and `.../two-flakes/{a,b}/flake.lock` for the per-directory scoping case
- [ ] T010 Unit-test `parse_flake_lock` in `waybill-cli/src/scan_fs/package_db/nix/lockfile.rs` against every fixture from T008 and T009, asserting `follows` entries parse as `Follows` and never as `NodeRef`

**Checkpoint**: the lockfile parses into types that cannot silently mistake an alias for a pin.

---

## Phase 3: User Story 1 - The build's pinned inputs appear in the SBOM (Priority: P1) 🎯 MVP

**Goal**: every input the lockfile pins appears as a component identified by its locked revision.

**Independent test**: scan a fixture containing only `flake.lock`; assert one component per locked input, each carrying the pinned revision.

### Tests for User Story 1

- [ ] T011 [P] [US1] Write `waybill-cli/tests/nix_flake_lock_reader.rs` asserting contract C-1 and FR-002: one component per identifiable locked input, and none for root, `path`, `indirect` or `follows` aliases
- [ ] T012 [P] [US1] Extend `waybill-cli/tests/nix_flake_lock_reader.rs` with contracts C-2 and C-3: a `github` input emits `pkg:github/<owner>/<repo>@<rev>` byte-for-byte, a `tarball` input emits `pkg:generic/<name>@<rev>`, and each component's `version` field equals the locked `rev` verbatim — untruncated, not normalised, not replaced by `lastModified`. The identifier and the version field are different slots; asserting the PURL contains the rev says nothing about which value populated `version`
- [X] T012a [P] [US1] Extend `waybill-cli/tests/nix_flake_lock_reader.rs` asserting no emitted PURL begins with `pkg:nix` (FR-013c, contract C-2). Kept separate from T012 so a mutation breaking one is not masked by the other passing: T012 asserts what IS emitted, this asserts what must never be
- [ ] T013 [P] [US1] Extend `waybill-cli/tests/nix_flake_lock_reader.rs` with contract C-4: no native checksum field is populated for these components, and the NAR hash appears in its annotation with the `sha256-` prefix intact

### Implementation for User Story 1

- [X] T014 [US1] Implement host-typed identifier construction in `waybill-cli/src/scan_fs/package_db/nix/identity.rs` for `github`, `gitlab` and `sourcehut` inputs with a known `rev` (FR-013a), following the m128 `yocto/recipe.rs` pattern rather than calling its `SRC_URI`-shaped helper
- [X] T015 [US1] Implement the `pkg:generic/<name>@<rev>` fallback in `waybill-cli/src/scan_fs/package_db/nix/identity.rs` for `tarball` and `git` inputs, carrying the upstream URL through the existing source-url / source-type annotation channel (FR-013b)
- [X] T016 [US1] Skip `path` and `indirect` inputs in `waybill-cli/src/scan_fs/package_db/nix/mod.rs` (FR-003), and decline to emit an input with no `rev`, recording the omission rather than passing over it silently (data-model.md Entity 3 validation)
- [X] T017 [US1] Resolve `follows` aliases to their target node in `waybill-cli/src/scan_fs/package_db/nix/mod.rs` so no second component is minted for one underlying pin (FR-004)
- [X] T018 [US1] Record each input's upstream source location using the target formats' native field in `waybill-cli/src/scan_fs/package_db/nix/mod.rs` (FR-005, contract C-5's native-first half)
- [X] T019 [US1] Emit the NAR hash verbatim, SRI prefix intact, into its annotation in `waybill-cli/src/scan_fs/package_db/nix/mod.rs`, and populate no native checksum field (FR-009, FR-009a, contract A-1)
- [X] T020 [US1] Add the NAR-hash annotation row to `docs/reference/sbom-format-mapping.md` (FR-009b)
- [X] T021 [US1] Add matching NAR-hash extractors to `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs` and register the row in `waybill-cli/src/parity/extractors/mod.rs`, or `every_catalog_row_has_an_extractor` and `holistic_parity` fail by construction (FR-009b)
- [X] T022 [US1] Sort emitted inputs by a total order over the identifier before emission in `waybill-cli/src/scan_fs/package_db/nix/mod.rs` — `nodes` is a JSON object and its iteration order is not a guarantee (SC-006, the #948 failure mode)
- [X] T023 [US1] Scope each `flake.lock` to the directory that contains it in `waybill-cli/src/scan_fs/package_db/nix/mod.rs`, so one lockfile never speaks for another directory (FR-011, contract C-9, the #938 rule applied from the start)
- [ ] T024 [US1] Assert the repo-report census counts `flake.lock` as claimed in `waybill-cli/tests/repo_report_census.rs` — it should follow from the T003 registration, so this task is verification, and a failure means the registration did not take (FR-014)
- [ ] T024a [US1] Add the Nix files this feature does NOT read — `flake.nix`, `default.nix`, `shell.nix`, `*.nix` — to `waybill-cli/src/report/ecosystems.data` as recognised-but-unread markers, so they report as a known ecosystem rather than as wholly unknown. SC-004 requires zero Nix files unrecognised, and `flake.lock` alone is one of six on the reference repository
- [ ] T025 [P] [US1] Add a determinism test to `waybill-cli/tests/nix_flake_lock_reader.rs`: two scans of one fixture emit a byte-identical component sequence (SC-006)

**Checkpoint**: SC-001 and SC-004 are met by this phase alone — the MVP.

---

## Phase 4: User Story 2 - The document says what depends on those inputs (Priority: P2)

**Goal**: every emitted input is reachable from the document root.

**Independent test**: scan a fixture with a `flake.lock`; walk dependency edges from the root and assert zero unreachable input components.

### Tests for User Story 2

- [ ] T026 [P] [US2] Write `waybill-cli/tests/nix_flake_lock_graph.rs` asserting contract C-5: walking `dependencies[]` from the document root reaches every emitted input component (SC-003)
- [ ] T027 [P] [US2] Extend `waybill-cli/tests/nix_flake_lock_graph.rs` to assert a nested input is an edge from its declaring input, not re-parented to the root (FR-008)

### Implementation for User Story 2

- [ ] T028 [US2] Emit an edge from the consuming project to each root-declared input in `waybill-cli/src/scan_fs/package_db/nix/mod.rs` (FR-007)
- [X] T029 [US2] Emit edges between input components for non-root nodes that declare their own inputs in `waybill-cli/src/scan_fs/package_db/nix/mod.rs`, resolving `Follows` edges to their target (FR-008, research R4)

**Checkpoint**: SC-003 met; inputs are connected rather than merely present.

---

## Phase 5: User Story 3 - What was asked for is distinguishable from what was resolved (Priority: P3)

**Goal**: a moving reference that has been pinned is visible as such.

**Independent test**: scan a fixture whose `original` names a branch and whose `locked` names a revision; assert both are recoverable.

### Tests for User Story 3

- [ ] T030 [P] [US3] Write `waybill-cli/tests/nix_flake_lock_original_ref.rs` asserting both the branch and the locked revision are recoverable when they differ (FR-006, contracts C-6 and A-2, US3 acceptance scenario 1)
- [ ] T031 [P] [US3] Extend `waybill-cli/tests/nix_flake_lock_original_ref.rs` asserting no annotation is emitted when `original` already names the locked revision (FR-006, US3 acceptance scenario 2)

### Implementation for User Story 3

- [X] T032 [US3] Emit the pre-resolution reference annotation in `waybill-cli/src/scan_fs/package_db/nix/mod.rs`, only when `original` differs from `locked` (FR-006, contract A-2)
- [X] T033 [US3] Add the original-reference annotation row to `docs/reference/sbom-format-mapping.md` (FR-009b applies to this annotation too)
- [X] T034 [US3] Add matching original-reference extractors to `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs` and register the row in `waybill-cli/src/parity/extractors/mod.rs` (FR-009b, which binds this annotation as it does the NAR hash)

**Checkpoint**: all three user stories complete.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T035 [P] Write `waybill-cli/tests/nix_flake_lock_failure_modes.rs` asserting contract C-7: a malformed lockfile and an unrecognised-version lockfile each emit no components, warn naming the file, and leave every other ecosystem's component count identical to a scan of the same tree with the file removed (FR-010, SC-007)
- [ ] T036 [P] Add an SC-005 test to `waybill-cli/tests/nix_flake_lock_failure_modes.rs`: adding a `flake.lock` to a tree never reduces the component count for any other ecosystem — the property #937 and #938 were both violations of
- [ ] T037 [P] Add an SC-002 test to `waybill-cli/tests/nix_flake_lock_failure_modes.rs` asserting a scan of a `flake.lock` tree produces identical output with and without `--offline`, AND succeeds with `nix` absent from `PATH` (FR-013, contract C-8). SC-002 has two clauses — no network and no Nix installation — and only the first is otherwise tested; on a developer machine that has Nix, an accidental dependency on it would pass every other check
- [ ] T038 [P] Add an FR-012 test to `waybill-cli/tests/nix_flake_lock_failure_modes.rs` asserting a flake present without a lockfile records why no inputs were emitted rather than emitting nothing silently
- [ ] T039 Teeth-check every new test in `waybill-cli/tests/nix_*.rs` by mutation: revert each fix in turn and confirm the intended test fails and the others do not, recording the matrix in the PR. A test that passes under its own mutation proves nothing
- [ ] T040 Run the mandatory pre-PR gate (`./scripts/pre-pr.sh`) and enumerate per-target results, checking for `exited abnormally` and `error[E` as well as `test result: FAILED` — a target that dies prints no `test result:` line
- [ ] T041 [P] Add a `CHANGELOG.md` entry under `[Unreleased]` describing what a Nix repository's SBOM gains
- [ ] T042 Verify SC-001 and SC-004 against the reference repository by hand, recording before/after figures in the PR

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)** → no dependencies
- **Phase 2 (Foundational)** → requires Phase 1. **Blocks all user stories.**
- **Phase 3 (US1)** → requires Phase 2
- **Phase 4 (US2)** → requires Phase 3 (edges need components to connect)
- **Phase 5 (US3)** → requires Phase 3; independent of Phase 4
- **Phase 6 (Polish)** → requires the phases whose behaviour it tests

### User Story Dependencies

- **US1** is self-contained and is the MVP.
- **US2** depends on US1 — there is nothing to connect until components exist.
- **US3** depends on US1 only. US2 and US3 may proceed in parallel once US1 lands.

### Within Each User Story

Tests first where listed, then identity, then emission, then cross-cutting
(ordering, scoping). T020/T021 and T033/T034 must land together with their
annotation — a catalogue row without extractors fails the suite by construction.

### Parallel Opportunities

- T004, T005 — different types, same file section; safe to split
- T008, T009 — disjoint fixture sets
- T011, T012, T013 — separate assertions, written before implementation
- T026, T027 and T030, T031 — separate test files
- T035, T036, T037, T038 — independent polish tests
- US2 and US3 phases entirely, once US1 is complete

## Parallel Example: User Story 1

```
T011, T012, T013   (tests, written first)
        ↓
T014, T015         (identity construction — same file, sequential)
        ↓
T016, T017, T018, T019   (emission rules)
        ↓
T020 → T021        (annotation row, then its extractors)
        ↓
T022, T023, T024   (ordering, scoping, census)
        ↓
T025               (determinism)
```

## Implementation Strategy

**MVP is Phase 3.** US1 alone takes the reference repository from zero pinned
inputs visible to all of them, and stops the Nix files reading as unrecognised —
SC-001 and SC-004 both satisfied without US2 or US3.

Ship US1, verify against the reference repository by hand, then add US2's edges
and US3's provenance as separate increments.

Two cross-cutting concerns are deliberately inside US1 rather than deferred to
Polish: deterministic ordering (T022) and per-directory scoping (T023).
Retrofitting either is more expensive than building it in, and both have a
recent precedent in this repository for exactly that reason.
