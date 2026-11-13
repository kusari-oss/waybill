# Tasks: A closure member is a `package` only when it is one

**Input**: Design documents from `specs/1380-closure-package-role/`.
**Prerequisites**: plan.md, spec.md, research.md (R1–R7), data-model.md, contracts/member-role.md, quickstart.md.

**Tests**: Requested. Every rung gets a unit test and a fixture test, and every rule a mutation check (constitution VII and IX; the project's practice).

**Paths**: Code is under `waybill-cli/src/scan_fs/package_db/nix/closure/` unless stated. Tests are in `waybill-cli/tests/`.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

- [X] T001 Re-run `specs/1380-closure-package-role/measurements/package-role-census.sh` at the branch base and confirm it matches `package-role-census-2026-11-13.txt`. It is the "before" every golden comparison uses.

---

## Phase 2: Foundational (blocks every story)

- [X] T002 Add `MemberRole::Check` in `member.rs`:
  - wire `check`;
  - `is_edge_endpoint()` true;
  - appended to `ALL` (length 5);
  - doc comment per `contracts/member-role.md`.
- [X] T003 Add `PatchOutputs` in `member.rs`: a set of store basenames, built by `PatchOutputs::of(&RawClosure)` from every derivation's `paths_in("patches")` (both carriers, research R2).
- [X] T004 Change the signature to `member::classify(drv, &PatchOutputs)`, behaviour still unchanged.
  - Build `PatchOutputs` once per closure and pass it at every call site:
    - `emit.rs::emitted_drv_keys` and the component loop in `emit.rs::components`;
    - `graph.rs::project`, which builds it and passes it through `nearest_stopping` to `stops`;
    - `waybill-cli/src/cli/debug_cmd.rs`'s closure dump.
  - Every one of these already holds the `RawClosure`, so no public signature outside the closure module changes.
  - `cargo build --workspace --all-targets` must pass.
  - **As built:** `graph::stops` and `emit::emitted_drv_keys` ask `member::is_edge_endpoint(drv)` instead of threading the set. That answer cannot depend on the patch set, because every outcome of the split rung is an endpoint. `s1380_the_patch_set_never_changes_the_endpoint_answer` holds that. The full `classify(drv, &patches)` runs in `emit::components` and the debug dump.
- [X] T005 Add a unit test in `member.rs` asserting the invariant (data-model): for every member, the new classification differs from the old one only where the old role was `package`, and every new role is an endpoint. Keep the old ladder as a test-only function, so the test compares the two.

**Checkpoint**: compiles, all existing tests pass, and no role has changed yet.

---

## Phase 3: User Story 2 - A source tree is recorded as source (Priority: P1)

**Goal**: Inside the version-token population, patches, `srcOnly` trees and `-source`/`-src-<v>`/`-vendor` trees are `source`.

**Independent Test**: The fixture's E1 patch, E2 tree, `-src-<v>` member and `-vendor` member carry `source` in all three formats. Its edges are identical to a run with the old ladder.

### Tests

- [X] T006 [P] [US2] Unit tests in `member.rs`:
  - **E1:** a member whose output basename is in `PatchOutputs` → `source`.
  - **E1 negative:** a fragment patch with no version token stays `fragment` (research R5).
  - **E2:** `phases` = `unpackPhase patchPhase installPhase` → `source`, in both carriers.
  - **Source-name rule:** `-source`, `-src-21.1.8`, `-vendor` → `source`.
  - **Name-rule controls:** `sqlite-src-3510200.zip`, a fetch, stays `source` by rung 1; `waybill-fixture-lib-1.0` stays `package`.
  - **Identity:** a rung-3 `source` keeps a `Recipe` identity (FR-003).

### Implementation

- [X] T007 [US2] Implement sub-rungs 3a–3c in `member::classify`, inside the version-token branch, in R6's order: E1, E2, then the source-name rule.

**Checkpoint**: US2's unit tests pass.

---

## Phase 4: User Story 3 - Checks and test programs are not packages (Priority: P2)

**Goal**: `*-get-version-<v>` and `*-simple-program-<v>` carry `check`.

**Independent Test**: The fixture's check members carry `check` in all three formats, and the edges are unchanged.

### Tests

- [X] T008 [P] [US3] Unit tests in `member.rs`:
  - `bash-get-version-5.3`, `tinycc-boot-mes-get-version-unstable-2024-07-07` and `gcc-cxx-simple-program-4.6.4` → `check`.
  - A `-get-version` segment with no version after it is not `check`.
  - A member that matches both a check name and E1 → `source`, because evidence comes first (R6).

### Implementation

- [X] T009 [US3] Implement sub-rung 3d (the check-name rule) in `member::classify`, after 3a–3c (R1, R6).

---

## Phase 5: User Story 1 - A consumer filtering `package` gets packages (Priority: P1)

**Goal**: The `package` that remains is the genuine population, in every format and in C184.

**Independent Test**: Run the fixture end to end, then filter its C206 `package` values. The result is exactly the genuine package, the `-env` and the mes-style bootstrap member.

### Tests

- [X] T010 [US1] Create a fixture closure at `waybill-cli/tests/fixtures/nix_closure_member_role/closure.json` (format 4, synthetic `waybill-fixture-*` names). One nameless member per case:
  - E1 patch (with a version token), whose output another member lists in `patches`;
  - E2 `srcOnly` tree;
  - `-src-<v>`, `-vendor`, `-get-version-<v>` and `-simple-program-<v>` members;
  - a genuine `kaem`-style bootstrap package;
  - an `-env`;
  - a fragment patch with no version token;
  - a fixed-output fetch.
  - Plus one named package that consumes them, so edges exist.
  - **As built:** the fixture is `waybill-cli/tests/fixtures/nix_closure_members/member_roles.json`, next to m1282's, so it reuses that suite's stub-`nix` harness.
- [X] T011 [US1] Add an integration test `waybill-cli/tests/closure_member_role.rs`, following `closure_members.rs`'s harness. It asserts:
  - every member's C206 value in CDX, SPDX 2.3 and SPDX 3;
  - C184 `member-roles` counts, including `check`;
  - the named `dependencies[]` edges, against expectations computed from the old ladder (FR-006);
  - the C206 parity extractors (`c206_cdx`, `c206_spdx23`, `c206_spdx3`) return equal sets on the fixture's three documents (FR-007).
  - **As built:** 4 tests (`s1380_*`) in `waybill-cli/tests/closure_members.rs`. The suite is 23/23.
- [X] T012 [US1] Update existing tests that assert `package` for a member that now moves, in `closure_members.rs` and the `member.rs` m1282 tests. Each change is listed in the commit message with its reason.
  - **As built:** no existing assertion needed changing. The m1282 fixture's one `package` member, `waybill-fixture-mini-0.27.1`, stays `package`.

### Implementation

- [X] T013 [US1] Confirm `summary.rs`'s `member-roles` and `edge-ineligible` need no code change (seeded from `ALL`, research R7). The test in T011 is the evidence.

**Checkpoint**: every story is independently green, and `cargo test -p waybill --test closure_member_role` passes.

---

## Phase 6: Polish & cross-cutting

- [X] T014 [P] Update the C206 row in `docs/reference/sbom-format-mapping.md`:
  - five values;
  - the endpoint set;
  - which rules are evidence and which are name shape (FR-008);
  - the R5 scope note, with #1502.
  - Also update C184's note on `member-roles`.
- [X] T015 [P] Update the member-role section of `docs/reference/nix-evaluation.md` to match the contract.
- [X] T016 [P] Add a CHANGELOG `Changed` entry: `check` is a new C206 value, and which members moved. Place it apart from other open PRs' entries, because CHANGELOG adjacency breaks stacking.
- [X] T017 Mutation-check each rung: disable E1, E2, the source-name rule and the check-name rule, one at a time. Each must fail a test. Revert with `cp`, never `git checkout`. Record the results in the commit message.
  - **Observed:** failed tests per disabled rung (unit / integration): E1 3/3; E2 2/3; source-name 2/3; check-name 2/3. The graph test passes under every mutation, as it must. Every mutation was reverted with `cp`.
- [X] T018 Run `cargo clippy --workspace --all-targets` and the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Record `GATE_EXIT` and the pass/fail totals.
- [X] T019 Dispatch `public-corpus.yml` with `regen_goldens=true`, download `corpus-goldens-regen`, and verify against research's prediction table:
  - re-run the census on the new goldens: roles per group, with `package` 23/23/34/3/3 and `check` 46/46/47/0/0;
  - the named edges are identical (sorted, normalized diff);
  - non-closure targets are byte-identical.
  - **Observed (run 43466336116 at `50d0934f`):**

    | target | `package` | `check` | `source` | edges | members moved |
    |---|---|---|---|---|---|
    | moat | 23 | 46 | 502 → 506 | identical | 50 |
    | shellcheck | 23 | 46 | 760 → 764 | identical | 50 |
    | slacklinker | 34 | 47 | 902 → 914 | identical | 59 |
    | openai | 3 | 0 | 702 → 703 | identical | 1 |
    | slack-web | 3 | 0 | 613 → 614 | identical | 1 |

    Every value matches research's prediction table. On every moved member only C206 changed. Of the metadata, only C184 changed, and its counts equal the per-component ones. Non-closure targets are byte-identical. T018's gate: `GATE_EXIT=0`, 8,037 passed, 0 failed.
- [ ] T020 Commit the goldens, run a non-regenerating corpus check at that head, and confirm it passes. Then open the PR: no tool comparisons, no company names, and the AI-drafting footer.

---

## Dependencies & order

- **T001** comes first.
- **T002–T005** block every story.
- **US2 (T006–T007), then US3 (T008–T009):** both edit `classify`, so they run in sequence. Their test tasks are [P].
- **US1 (T010–T013)** needs US2 and US3, because the remaining `package` set is what is left after both.
- **Polish:** T014–T016 are [P], then T017, T018, T019, T020, in order.

## Parallel examples

- T006 and T008: unit tests in separate test functions of one file. Written together, run together.
- T014, T015, T016: different files.

## Implementation strategy

There's one PR. The MVP is not independently shippable: the golden regeneration must show every move at once, so the corpus churns once (T019). Stories are ordered so each checkpoint is green.
