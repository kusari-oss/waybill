# Tasks: multi-main-module root override

**Feature**: `860-multi-main-module-override` | **Spec**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md)
**Issue**: #863

## Format: `[ID] [P?] [Story] Description`

- **[P]** — parallelizable (different file, no dependency on incomplete work)
- **[US1/US2/US3]** — the user story the task serves

## Path Conventions

Single Rust workspace. All paths from repo root.

---

## Phase 1: Setup

- [X] T001 Record the pre-change baseline for the three affected targets into `specs/860-multi-main-module-override/baseline.md`: component counts with and without `--root-name`, main-module counts, and dangling-reference counts per format. Without this the deltas in SC-001..SC-004 cannot be shown to have been met.
- [X] T002 [P] Confirm the eight zero-main-module targets' current component counts in the same `baseline.md`, so SC-005 byte-identity has a recorded starting point.

---

## Phase 2: Foundational (blocks every user story)

**These change the policy itself. Nothing else can be verified until they land.**

- [X] T003 In `waybill-cli/src/generate/root_selector.rs`, change `apply_main_module_drop_or_demote` so an active override retains and demotes every main-module component at every N — removing the N>1 fall-through at line 525 and the preserve-flag branch. Per contract C-1.1/C-2.1.
- [X] T004 In `waybill-cli/src/generate/root_selector.rs`, rename `DropOrDemoteResult.redirected_main_module_purls` to `retained_main_module_purls` and update its doc comment: the set no longer drives edge removal, it identifies components the root must depend on (data-model.md).
- [X] T005 In `waybill-cli/src/generate/root_selector.rs`, stop stripping outbound edges from retained entries, per C-3.1. This reverses the milestone-149 US1 Option A decision recorded 2026-06-29 — leave a comment saying so and why, or the next reader will restore it.
- [X] T006 In `waybill-cli/src/generate/root_selector.rs`, replace the milestone-149 FR-013 no-op INFO diagnostic (around lines 548/573/609) with a retention diagnostic naming the count of modules retained, per Constitution Principle X and research R5.
- [X] T007 Unit-test the helper in `waybill-cli/src/generate/root_selector.rs`: N=0, N=1 and N>1 all retain and demote identically (C-1.1). Assert the demoted shape — role annotation removed, `waybill:demoted-from-main-module` added, outbound edges retained. Additionally assert C-2.2: the demoted component's PURL, name, version, licenses and hashes are byte-equal to their pre-demote values — the demote transformation edits annotations and type on the same struct, so identity drift would be silent.
- [X] T008 Add a test in `waybill-cli/src/generate/root_selector.rs` asserting invariant I1 / FR-003 / C-8.2: for a multi-module fixture under an active override, exactly one component is the document subject and no retained module is emitted as a second root. Assert on emitted shape, not on the helper's return value — the helper cannot see what the emitter does with it.
- [X] T009 Teeth-check T008 by making the demote path leave the main-module role annotation in place; the test must fail. This is the specific regression the milestone-077 clean-replacement design existed to prevent, and the one this feature is most likely to reintroduce.
- [X] T010 Teeth-check T007: revert T003 locally and confirm the N>1 case fails. A test that passes without the change is the milestone-856 trap; record the result in the PR.

**Checkpoint**: components are retained, and exactly one subject is declared. Dangling references should already be gone; reachability is not yet guaranteed.

---

## Phase 3: User Story 1 — a consumer walks the dependency graph (P1) 🎯 MVP

**Goal**: every dependency reference resolves, and every retained module is reachable from the subject.

**Independent test**: scan any affected target with `--root-name`, subtract the component set from the set of dependency-reference targets, and get an empty set — in all three formats.

- [X] T011 [US1] In `waybill-cli/src/generate/cyclonedx/builder.rs` (call site line 591), emit a `dependencies[]` entry anchoring the override root to every retained module from `retained_main_module_purls`, per C-3.2.
- [X] T012 [US1] In `waybill-cli/src/generate/spdx/document.rs` (call site line 425), emit the equivalent SPDX 2.3 relationships from the override root to every retained module.
- [X] T013 [US1] In `waybill-cli/src/generate/spdx/v3_document.rs` (call site line 65), emit the equivalent SPDX 3 relationships.
- [X] T014 [US1] Add a test in `waybill-cli/tests/` asserting invariant I2 — every `dependsOn` target and every relationship endpoint resolves to a component in the same document — run against a synthetic multi-module fixture in all three formats.
- [X] T015 [US1] Add a test asserting invariant I3 — every retained module is reachable from the subject (SC-007a).
- [X] T016 [US1] Teeth-check T014 and T015 by reverting T011–T013; both must fail.

**Checkpoint**: US1 is independently deliverable. The graph is internally consistent even before the inventory claims in US2 are verified.

---

## Phase 4: User Story 2 — an operator names the subject of a workspace scan (P1)

**Goal**: naming the subject does not reduce the inventory.

**Independent test**: component count with `--root-name` equals the count without it, on all three affected targets.

- [X] T017 [US2] Add a test asserting C-2.3 — for a multi-module fixture, the component set emitted with an override is a superset of the set emitted without one, minus at most one component absorbed under C-4.
- [X] T018 [US2] Implement the FR-011 identity collision rule in `waybill-cli/src/generate/root_selector.rs`: when a retained module's PURL equals the override root's PURL, do not emit it separately, attach its outbound edges to the root, and emit no root→module edge for it (C-4.1).
- [X] T019 [US2] Add a synthetic test for T018 in `waybill-cli/src/generate/root_selector.rs`. No corpus target exhibits this case (research R6), so it cannot be covered by the corpus.
- [X] T020 [US2] Add an N=1 convergence test that runs the **real emitter path**, not a hand-assembled component vector — research R6 and the milestone-856 lesson. No corpus target has N=1.
- [X] T021 [US2] Teeth-check T017, T019 and T020; each must fail with its corresponding change reverted.

**Checkpoint**: US1 + US2 together satisfy SC-001 through SC-005 modulo golden regeneration.

---

## Phase 5: User Story 3 — a maintainer reads why a module is not the root (P3)

**Goal**: a component that held the main-module role stays distinguishable from one that never did.

**Independent test**: a retained module carries `waybill:demoted-from-main-module = "true"`; a natural library dependency does not.

- [X] T022 [US3] Verify C102 emission still fires for retained modules in all three formats, and that a never-main-module component does not carry it (FR-004).
- [X] T023 [US3] Update the C102 row in `docs/reference/sbom-format-mapping.md:147`. Its current text states the demoted entry has no outbound `dependsOn` edges and is re-anchored on the override root; FR-007 makes that false. Research R2 — the `every_catalog_row_has_an_extractor` gate checks for a missing extractor, not a stale description, so nothing will catch this if it is skipped.

---

## Phase 6: Polish & Cross-Cutting

- [X] T024 Verify whether the SPDX 3 PURL alias at `waybill-cli/src/generate/spdx/v3_document.rs:318-324` is still required now that re-anchoring is gone (contract C-6.2). It exists to serve re-anchoring (issue #229) but predates milestone 149 and may serve untouched paths. **Verify, do not assume.**
- [X] T025 If T024 shows the alias is unnecessary, remove it and confirm the SPDX 3 C102 annotation subject now matches CDX and SPDX 2.3 — closing the divergence milestone 149 deferred. If it is still required, re-document the divergence in the C102 row rather than carrying it silently (C-6.3).
- [X] T026 [P] Update `--preserve-manifest-main-module` help text in `waybill-cli/src/cli/scan_cmd.rs` to say it is retained for compatibility and no longer changes output (C-7.2, FR-006).
- [X] T027 [P] Add a test asserting the flag is still accepted and produces output identical to omitting it (SC-006).
- [X] T028 Confirm the existing `holistic_parity` and `every_catalog_row_has_an_extractor` gates actually cover C102 under the new shape (FR-009, C-5.1, C-5.2) by reading what they assert, rather than inferring coverage from a green run. If they do not compare the annotation value across formats, add an assertion that does. This repo has shipped a schema gate that passed because its `$ref`s resolved to stubs.
- [X] T029 Run `./scripts/pre-pr.sh` — must exit 0. Enumerate every `test result:` line rather than trusting the exit code; the script lacks `--no-fail-fast`.
- [X] T030 Regenerate the public-corpus goldens through CI dispatch for the three affected targets, following `docs/development/refreshing-corpus-goldens.md`. Never locally.
- [X] T031 Read and attribute every diff from T030 before accepting it. Expect: maven-guice +16, rust-ripgrep +10, python-flask +4 components, and 14 dangling references resolved. Anything else needs explaining before the goldens land. Also assert SC-007b at corpus level: the inter-module dependency edges present on maven-guice without `--root-name` are present with it — component counts alone do not prove edges survived.
- [X] T032 Prove SC-005: the eight zero-main-module targets are byte-identical to their committed goldens.
- [X] T033 Dispatch the corpus lane read-only against the branch and confirm green (SC-008).
- [X] T034 Put the attribution in the PR body, not a committed document — the same rule milestone 840 T023 applies. Reference the PR from the commit message.
- [X] T035 Close #863 with a reference to the merged PR, noting that the dangling references it reported were a symptom of the broader component-loss defect.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)** — no dependencies. T001 must precede T031, which compares against it.
- **Phase 2 (Foundational)** — blocks everything. T003 is the single behaviour change; T004–T006 follow it in the same file.
- **Phase 3 (US1)** — needs Phase 2. T011–T013 are one per format.
- **Phase 4 (US2)** — needs Phase 2. Independent of Phase 3: retention alone satisfies the inventory claim, edges satisfy reachability.
- **Phase 5 (US3)** — needs Phase 2 only.
- **Phase 6 (Polish)** — T030 needs everything merged into the branch; T024 needs Phase 3.

### User Story Dependencies

US1 and US2 both depend on Phase 2 and on nothing else. They can be developed and verified independently. US3 is documentation plus a verification of behaviour Phase 2 already produces.

### Parallel Opportunities

- T001 and T002 (baseline capture, different sections).
- T011, T012, T013 — three formats, three files, same contract.
- T026 and T027 (flag help text, flag test).
- Phase 4 and Phase 5 can proceed alongside Phase 3 once Phase 2 lands.

### Not parallel

- T003–T006 all edit `root_selector.rs`.
- Every teeth-check (T009, T010, T016, T021) requires reverting its subject, so it serialises against it.
- T030 and T031 — reading the diffs is the point, and regenerating again before reading defeats it.

---

## Implementation Strategy

**MVP** is Phase 2 + Phase 3 (US1). At that point the emitted graph is internally consistent and the fourteen dangling references are gone — the only symptom third-party consumers observe.

Phase 4 adds the inventory guarantee, which is the larger user-visible win (30 components across three targets) but is not a correctness failure in the way a dangling reference is.

Deliver Phase 6's golden regeneration last and once. Any emission-affecting change after T030 invalidates the artifact and the attribution built from it — the freeze-the-fix-set rule from milestone 840 step 5b.

---

## Completion record

All 35 tasks complete. Delivered in PR #866 (merged), closing #863.

**Outcome**: 34 components restored across seven targets; 14 dangling
references resolved; corpus lane green (run 34773489214, 22/0).

**Scope grew during implementation, twice, both times because a
measurement was wrong rather than because the work expanded:**

- Research R6 claimed no corpus target exercises N=1. Four do. The
  method counted `main-module` role annotations in `components[]`, and
  at N=1 the module is promoted out of that array into
  `metadata.component` — so it could only ever observe N>1. Three
  affected targets became seven.
- Re-anchoring lived at three sites, not one. Missing the third
  (`purl_aliases` in `spdx/document.rs`) had CDX and SPDX 2.3 emitting
  different graphs. My parity check compared component counts, which
  agreed, instead of topology, which did not.

**Two claims retracted in flight**: the "26 → 0 SPDX 3 dangling
endpoints" improvement (a masking artifact — see #865), and R6 above.

**Five superseded test pins inverted rather than deleted.** One of the
five failures was NOT superseded: it was a live parity guard catching a
real defect. That ratio is the durable lesson — in a change that
legitimately invalidates many tests, the real failure hides among the
outdated ones, and the instinct to invert them all is exactly wrong.

**Filed, not fixed**: #865 (SPDX 3 golden masking asymmetry).

**Process note**: goldens were regenerated twice, because the SPDX 2.3
fix landed after the first regeneration. That is the freeze-the-fix-set
rule from milestone 840 step 5b — written by me, broken by me, one
regeneration cycle later.
