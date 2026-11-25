# Tasks: graph completeness counts a partial closure read

**Input**: `specs/1656-closure-read-completeness/` (spec, plan, research R1–R5, data-model, contracts/wire.md, quickstart)
**Tests**: requested by the spec (controls, a mutation, three-format parity), so each story's tests are written first.

**Scope rule**: one gate and one regeneration cycle. Side findings are filed, not fixed. The PR opens at the first green gate (T012).

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup

- [ ] T001 Measure the R4 controls through the project's own harness, using a throwaway copy of `waybill-cli/tests/closure_binary_contents.rs`, deleted after the run. Record the results in `specs/1656-closure-read-completeness/measurements/controls-before.md`:
  - **Neither-gap candidate:** for each existing `scan(...)` call over `populated_store()`, record C213's absent, partial and failed counts, whether `waybill:nix-closure-read` is present, and C104 and C105.
  - **Outcome:** either name one scan with neither gap, or state that none exists. In that case T006 adds the missing outputs.
  - **Other two controls:** record the same four values for `closure_structured_attrs.rs`'s `CLOSURE_JSON` scan (declaration gap) and the stub fixture (contents gap only).
  - **Is the declaration control a declaration gap only?** Record whether the `CLOSURE_JSON` scan also has unread contents. If it does, the "declarations alone" case of FR-003 is covered at unit level only (T004), and `after.md` says so.
  - **Orphan reasons:** on the `CLOSURE_JSON` scan, dump each component's `waybill:orphan-reason` (PURL → value, sorted) into `controls-before.md`. This is T010's before-state for FR-005's unchanged orphan pass.

---

## Phase 2: Foundational (blocks both stories)

- [ ] T002 Carry the counts, not a flag (research R1).
  - **`waybill-cli/src/cli/scan_cmd.rs`:** `ClosureContentsSummary.unread: bool` becomes `unread: Option<ContentsUnread>`, built where `record.has_unread()` was (about line 7179) from `absent + partial + failed` and `members`. Update the test at about line 12151 to `is_some()`.
  - **`ContentsUnread { unread: usize, members: usize }`:** define it in `waybill-cli/src/generate/graph_completeness/mod.rs`, `pub(crate)`, deriving `Debug, Clone, Copy, PartialEq, Eq`.
  - **Change the field type:** in `ScanArtifacts` (`waybill-cli/src/generate/mod.rs:225`), and in the CycloneDX builder's field and `with_nix_closure_contents` (`waybill-cli/src/generate/cyclonedx/builder.rs:231, 445`) and its caller (`cyclonedx/mod.rs:204`).
  - **Literal sites:** the five `nix_closure_contents_unread: false` literals become `None`: `openvex/mod.rs:693`, `spdx/packages.rs:945`, `spdx/mod.rs:455`, `spdx/relationships.rs:718` and `spdx/document.rs:1228`, plus the builder default at `builder.rs:331`.
  - **Verify:** `cargo build -p waybill --all-targets` compiles. Behaviour is unchanged.
- [ ] T003 One predicate (R1), in `waybill-cli/src/generate/graph_completeness/mod.rs`:
  - **Add the type:** `ClosureReadGaps { declarations: Option<DeclarationGap>, contents: Option<ContentsUnread> }`, where `DeclarationGap { declarations, derivations, carrier_disagreements }`, with `any()`.
  - **Add the constructor:** `closure_read_gaps(read_outcome: Option<&ReadOutcome>, contents: Option<ContentsUnread>) -> ClosureReadGaps`. `declarations` is `Some` exactly when `read_outcome.is_some_and(ReadOutcome::has_gap)`.
  - **Route both copies through it:** `ScanArtifacts::closure_read_partial()` (`generate/mod.rs:586`) and the builder's inline expression (`builder.rs:~1118`) both become `closure_read_gaps(...).any()`.
  - **Verify:** run `cargo test -p waybill --bin waybill compositions` and `--test closure_structured_attrs --test closure_edges`. They must pass unchanged, since the root-claim tests already pin the predicate's value.

---

## Phase 3: User Story 1 — C104 never claims more than the native fields (P1) 🎯 MVP

**Goal**: FR-001, FR-002, FR-005, FR-006.
**Independent test**: on the declaration-gap fixture, C104 is `partial` and C105 names `closure-declarations-unconsulted` with its counts, in all three formats.

### Tests (write first; confirm they fail before T007)

- [ ] T004 [P] [US1] Unit tests in `waybill-cli/src/generate/graph_completeness/reason_codes.rs` and `mod.rs`:
  - **Wire text:** `ClosureDeclarationsUnconsulted { 211, 16, 0 }` renders `closure-declarations-unconsulted: 211 declaration(s) in 16 derivation(s)`; with `carrier_disagreements: 2` it appends `, 2 carrier disagreement(s)`; `{ 0, 0, 2 }`, a gap made only of carrier disagreements (spec edge case), renders `closure-declarations-unconsulted: 0 declaration(s) in 0 derivation(s), 2 carrier disagreement(s)`. `ClosureContentsUnread { 1284, 2045 }` renders `closure-contents-unread: 1284 of 2045 member(s)`.
  - **`apply_closure_read`:**
    - with neither gap, the result is unchanged;
    - with each gap alone, exactly that code is appended and the value becomes `partial`;
    - with both, declarations comes before contents and both follow every existing code, including a `project-discovery-scope` already present;
    - an empty set already `unknown` stays `unknown`.
  - **FR-003 equivalence:** for the four combinations, `closure_read_gaps(..).any()` is true exactly when a code is appended.
- [ ] T005 [P] [US1] Integration tests that also run `assert_parity` on each scan:
  - **`closure_structured_attrs.rs`** (`CLOSURE_JSON`): `m1720_a_declaration_gap_is_a_completeness_reason`. C104 is `partial`, and C105 contains `closure-declarations-unconsulted:` with N and M equal to the same document's `waybill:nix-closure-read` `declarations-unconsulted` and `derivations-affected`. Check this in CycloneDX, SPDX 2.3 and SPDX 3.
  - **`boot_library_edges.rs`** (stub): `m1720_unread_contents_are_a_completeness_reason`. C105 contains `closure-contents-unread: 4 of 4 member(s)` and no `closure-declarations-unconsulted`. Update `boot_library_edges_are_complete` so it also expects that clause.

### Implementation

- [ ] T006 [US1] The neither-gap control, from T001's finding, in `waybill-cli/tests/closure_binary_contents.rs`: `m1720_a_fully_read_closure_names_neither_gap`. C105 contains neither new code, and C104 and C105 equal the values T001 recorded for that scan (SC-004: decided by its other reasons alone, `complete` when it has none). Where T001 found no such scan, add the missing outputs to `populated_store()`'s use for this test only.
- [ ] T007 [US1] Implement the two variants and their wire text in `reason_codes.rs`, and `apply_closure_read(result, Option<&ClosureReadGaps>)` in `graph_completeness/mod.rs`, which mirrors `apply_scope_drop`.
  - **`graph_completeness/document.rs`:** `for_document` gains `closure_read: Option<&ClosureReadGaps>` and applies it after `apply_scope_drop`.
  - **Callers:** the CycloneDX builder (`builder.rs:917`), SPDX 2.3 (`spdx/document.rs:707`) and SPDX 3 (`spdx/v3_document.rs:684`) pass `Some(&gaps)`, built from `closure_read_gaps`. `orphan_reason.rs:796` passes `None` with a comment that matches its scope-drop comment.
  - **Verify:** run T004 to T006 until they pass.
- [ ] T008 [US1] Mutations, each reverted with `cp` from a backup and verified with `cmp`, then rebuilt. Record which tests fail in `measurements/after.md`:
  - **M1:** the builder passes `None`; T005's CycloneDX assertion and `assert_parity` must fail.
  - **M2:** SPDX 3 passes `None`; T005's SPDX 3 assertion or `assert_parity` must fail.
  - **M3:** `closure_read_gaps` ignores `contents`; T005 `m1720_unread_contents…` and T004's equivalence must fail.
  - **M4:** `apply_closure_read` appends nothing; T004 and T005 must fail.

**Checkpoint**: US1 is complete on the fixtures.

---

## Phase 4: User Story 2 — one predicate, no new degradation (P2)

**Goal**: FR-003, FR-004, and the unchanged orphan pass (FR-005).

- [ ] T009 [P] [US2] Unit test in `waybill-cli/src/generate/cyclonedx/compositions.rs`: `the_closure_read_codes_degrade_no_ecosystem`. A `GraphCompletenessResult` carrying both new codes yields an empty `degraded_ecosystems` set. That function ends in a `_ => {}` wildcard, so this test is what stops a future arm from silently degrading an ecosystem (R3).
- [ ] T010 [US2] Integration test in `waybill-cli/tests/closure_structured_attrs.rs`: `m1720_native_claims_are_unchanged`. On the declaration-gap fixture, CycloneDX `compositions[]` and every SPDX 3 relationship `completeness` value equal those pinned by the existing m1278 test (no claim of `complete`). Per-component `waybill:orphan-reason` values must equal T001's dump in `controls-before.md`.

---

## Phase 5: Polish & cross-cutting

- [ ] T011 [P] Documentation:
  - the C105 row in `docs/reference/sbom-format-mapping.md`: add both codes and their detail templates to the code list;
  - `specs/158-graph-completeness/contracts/graph-completeness-vocabulary.md`: list both codes;
  - `CHANGELOG.md` `[Unreleased]` `### Fixed`: C104 no longer reads `complete` when the closure read left declarations unconsulted or contents unread; shellcheck's golden now reads `partial`. Refs #1720.
- [ ] T012 Gate:
  - `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, checking `GATE_RESULT=pass`, the expected line and the per-target totals;
  - **on the first green gate**, commit, push and open the PR, ending with "Refs #1720, #1348, #1696" and the footer;
  - dispatch `gh workflow run public-corpus.yml -R kusari-oss/waybill --ref main -f branch=1656-closure-read-completeness -f regen_goldens=true`;
  - enumerate every CI job's conclusion.
- [ ] T013 Apply the regeneration:
  - Download the `corpus-goldens-regen` artifact and check that it has all targets × 3 formats.
  - Copy in only the differing files, then score research R5 in `measurements/after.md`:
    - exactly the five closure targets' 15 files changed;
    - `read_gaps.py` shows the expected clauses and counts, and shellcheck at `partial`;
    - a normalized diff shows only C104 and C105 changed;
    - `root-record-dependencies` are unchanged;
    - parity is identical before and after (spec 1654's `golden_parity.sh`).
  - Record any miss against R5 and what it corrected.
  - Commit, push, and update the PR body with the measured results.
- [ ] T014 Close out:
  - verify each task's claim against code and measurements before ticking it;
  - zero new Cargo dependencies (`git diff main -- '**/Cargo.toml' Cargo.lock` is empty);
  - tick the tasks.

---

## Dependencies & Execution Order

- T001 → T002 → T003 block everything.
- **US1:** T004 and T005 can be written in parallel and must fail first. Then T006, T007, T008.
- **US2:** T009 can be written alongside T004. T010 needs T007.
- **Polish:** T011 can run alongside T008–T010. Then T012, T013, T014.

## Implementation Strategy

- **MVP:** US1. Shellcheck's false `complete` is gone on the fixtures.
- **US2:** proves nothing else moved.
- **One cycle:** one gate, one PR opened at green, one dispatch.
