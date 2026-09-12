# Tasks: Refresh the public-corpus goldens with verified drift

**Feature**: `840-refresh-corpus-goldens` | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Issue**: #763

## Format: `[ID] [P?] [Story] Description`

- **[P]** = parallelizable (different files, no dependency on incomplete work)
- **[US1]/[US2]/[US3]** = owning user story (user-story phases only)

## Path Conventions

- Goldens: `waybill-cli/tests/fixtures/public_corpus/<target>/{cdx,spdx-2.3,spdx-3}.json`
- Review tool: `xtask/src/corpus_diff/`
- Procedure doc: `docs/development/refreshing-corpus-goldens.md`
- Harness (do not modify): `waybill-cli/tests/corpus_harness_195/`
- Workflow (do not modify): `.github/workflows/public-corpus.yml`

---

## Phase 1: Setup (Shared Infrastructure)

- [ ] T001 Dispatch `.github/workflows/public-corpus.yml` read-only (`regen_goldens: false`) against branch `840-refresh-corpus-goldens` and record the definitive list of failing target/format pairs in the PR draft. FR-001 scopes to "currently failing", so this list is observed, not inherited from the 2026-09-11 run.
- [ ] T002 For each failing target, record its pin (commit SHA or image digest) from `xtask/corpus/` config as it stands at branch point, so any later input change is detectable rather than silent.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Blocks both P1 stories.** Diffs must not be read before this exists — the raw diff is the unreviewable artifact this feature exists to avoid, and reading it first anchors the reviewer on noise.

- [ ] T003 Create `xtask/src/corpus_diff/mod.rs` implementing the normaliser per `contracts/xtask-corpus-diff-cli.md` C-1/C-2: accept `--old`/`--new` or `--target`/`--old-ref`, sort unordered collections by a stable total key, apply normalisation symmetrically to both sides, write to stdout, exit 0 regardless of whether differences exist.
- [ ] T004 Wire `CorpusDiff(corpus_diff::CorpusDiffArgs)` into the `Cli` enum and match arm in `xtask/src/main.rs`, and add `pub mod corpus_diff;` to `xtask/src/lib.rs`.
- [ ] T005 [P] Add `xtask/src/corpus_diff/tests.rs` covering contract C-5: an ordering-only difference normalises to empty output (C-5.1); a single changed licence still appears (C-5.2); a golden compared with itself is empty (C-5.3).
- [ ] T006 Add to `xtask/src/corpus_diff/tests.rs` (after T005; same file, so not parallel) a test asserting committed goldens are byte-identical before and after a `corpus-diff` run, enforcing C-2.3/C-5.4. This is the check that catches an accidental write-back, which would make a reordered-but-equal golden compare equal and silently weaken the gate.
- [ ] T007 Verify the normaliser does NOT re-apply the harness's masking (C-2.4). Goldens are already masked at write time in `waybill-cli/tests/corpus_harness_195/layer2_golden.rs:51`; masking twice risks diverging from what the lane compares.

**Checkpoint**: `cargo test -p xtask --lib corpus_diff` green, and `git status` clean after running the tool against a real golden.

---

## Phase 3: User Story 1 - The lane reports real regressions again (Priority: P1) 🎯 MVP

**Goal**: The corpus lane passes on every target it gates, so a future failure means something.

**Independent test**: Run the lane against an unchanged tree; every gated target passes.

- [ ] T008 [US1] Dispatch `public-corpus.yml` against this branch with `regen_goldens: true`; download the `corpus-goldens-regen` artifact. Do NOT regenerate locally — FR-003. Record the run ID in the PR draft.
- [ ] T009 [US1] Confirm the artifact's target set matches T001's failing set. A target present in one and not the other means the failing set moved between dispatches and T001 must be re-run.
- [ ] T010 [US1] For each failing target, produce a normalised diff with `cargo run -p xtask -- corpus-diff --target <name> --old-ref HEAD` and save each to the PR draft. Thirty diffs across ten targets × three formats, per the last observed run.
- [ ] T011 [US1] Read every diff. Classify each change as a repeated shape (a category) or a singleton. Per `research.md` R4 a singleton is NOT a category and gets its own explanation — folding singletons into a category is how a regression is absorbed into a large benign diff.
- [ ] T012 [US1] For any target whose diff shows a change inconsistent with accumulated drift, investigate before proceeding. FR-007 blocks the refresh for that target until the change is explained or raised as a defect.
- [ ] T013 [US1] Apply FR-012 to any target failing for a non-drift reason: repair it in this feature if the fix is small, otherwise remove it from the lane's gating set and open a tracked issue. Regenerating its golden is forbidden — that encodes the fault as expected output.
- [ ] T014 [US1] If any target was removed under T013, make the removal visible in the lane's own output per FR-012a, so shrinking coverage to reach 100% cannot be mistaken for earning it.
- [ ] T015 [US1] Copy the regenerated goldens for every verified target into `waybill-cli/tests/fixtures/public_corpus/`, all targets in one change per FR-013.
- [ ] T016 [US1] Confirm no assertion was widened, relaxed or disabled to achieve a pass (FR-008): `git diff` must show changes under `fixtures/` only, with `waybill-cli/tests/corpus_harness_195/` untouched.
- [ ] T017 [US1] Dispatch the lane read-only against the branch; confirm every gated target passes (SC-001).

**Checkpoint**: Lane green. Goldens committed. No harness or workflow file modified.

---

## Phase 4: User Story 2 - Every accepted change is understood (Priority: P1)

**Goal**: A reviewer can see, per target, what changed and why — without re-deriving it.

**Independent test**: A reviewer reads the PR description and can name the cause of each delta category for any target, having done none of the analysis.

**Coupling note**: US2 is not deliverable *after* US1 in the usual sense. FR-006/FR-007 make attribution a precondition of committing a golden, so T011–T012 (in US1) are where the understanding is produced; the tasks below are where it is recorded and proven. The stories are separable as *deliverables* — green lane vs reviewable evidence — not as a work sequence.

- [ ] T018 [US2] Build the attribution table: for each delta category found in T011, name the merge that caused it, working the log from `25bfbce` (2026-07-21, the last golden write) to the branch point. Expect m776 (#797) to account for a large share via source-provenance `externalReferences`.
- [ ] T019 [US2] Record which targets exhibit each category, so categories can be compared ACROSS targets per FR-013a. A target exhibiting a category no other target shows is the most likely place for a regression to hide.
- [ ] T020 [US2] Confirm every category has a named cause. "Expected churn" is not a cause; any category without one returns to T012.
- [ ] T021 [US2] Write the attribution into the pull request description per FR-015. Do NOT commit it as a document — it describes one moment and would read as current long after it is not, which is the failure mode #827 is currently cleaning up.
- [ ] T022 [US2] Reference the pull request from the commit message per FR-015a, then verify SC-007 by running `git log -1` on the merged commit and confirming the attribution is reachable from what it prints — following only the reference, without prior knowledge that evidence exists.
- [ ] T023 [US2] Prove the lane still detects change (FR-010, SC-003): introduce a deliberate emission change, dispatch the lane, confirm it fails and names the affected target and format, then revert. Record the failing run ID in the PR. A green lane is not evidence of a working lane — this repo has shipped a schema gate that passed because its `$ref`s resolved to stubs.
- [ ] T024 [US2] Cover at least one target per format in T023, so a format whose comparison silently no-ops cannot hide behind the other two.

**Checkpoint**: Attribution complete in the PR, every category caused, teeth demonstrated per format.

---

## Phase 5: User Story 3 - The method is repeatable (Priority: P2)

**Goal**: The next maintainer follows a procedure instead of rediscovering it.

**Independent test**: Someone who did not do this refresh can restate the procedure from the document alone.

- [ ] T025 [P] [US3] Publish `specs/840-refresh-corpus-goldens/quickstart.md` to `docs/development/refreshing-corpus-goldens.md` per FR-014, as a sibling of `docs/perf/refreshing-the-baseline.md` — same hazard class, same shelf.
- [ ] T026 [P] [US3] Ensure the document states the local-generation prohibition and *why*, citing #818 (macOS-recorded baseline vs Linux CI, nine days of phantom failures) and #832 (LFS-dependent fixture, three nights, two wrong diagnoses). The reason is what makes the rule survive contact with someone in a hurry.
- [ ] T027 [P] [US3] Link the procedure from `CONTRIBUTING.md` where the other maintenance procedures are listed, so it is found without knowing its filename.

- [ ] T028 [US3] Validate SC-006: have someone who did not perform this refresh — a colleague, or a fresh session with no feature context — read `docs/development/refreshing-corpus-goldens.md` alone and restate the procedure. Record which steps they could not reconstruct and fix those gaps in the document. Self-review does not satisfy this: the author cannot un-know the procedure, which is the one thing the criterion measures.

**Checkpoint**: Procedure published, discoverable, and confirmed followable by someone other than its author.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T029 Verify reproducibility (FR-011, SC-004): dispatch regen a second time against the unchanged tree and confirm the output is identical to the committed goldens. A difference means something non-deterministic is unmasked.
- [ ] T030 If T029 finds non-determinism, fix it in the harness's `mask_nondeterministic` where the gate honours it — NOT in `corpus_diff`, where only human reviewers would see the fix and the lane would keep failing.
- [ ] T031 Run the full pre-PR gate: `./scripts/pre-pr.sh` must exit 0.
- [ ] T032 Close #763 with a reference to the merged PR, noting the corrected scope (ten targets, not the five originally reported).

---

## Dependencies & Execution Order

### Phase Dependencies

```
Phase 1 (Setup)
   └─▶ Phase 2 (Foundational: normaliser)      ← blocks ALL diff reading
          ├─▶ Phase 3 (US1) ──┐
          │                    ├─▶ Phase 6 (Polish)
          └─▶ Phase 4 (US2) ──┘
                 Phase 5 (US3) — independent, any time after Phase 1
```

### User Story Dependencies

- **US1 and US2 are interleaved, not sequential.** FR-007 makes attribution a precondition of commit, so T011–T012 produce the understanding that T018–T020 record. Do not schedule US2 as a follow-up PR; the spec's whole argument is that a refresh without verification is worse than no refresh.
- **US3 is genuinely independent** and can proceed in parallel with either P1 story.

### Within Each User Story

- T010 → T011 → T012 strictly sequential: produce diffs, read them, then act on anomalies.
- T013/T014 only execute if T012 finds a non-drift failure. Both may be no-ops.
- T015 must not begin until T012 clears every target.
- T023 must run against the committed state, so it follows T015.

### Parallel Opportunities

- T005 → T006 in Phase 2 are NOT parallel: same file. T007 may run alongside either (it inspects the harness and writes nothing).
- T025, T026, T027 in Phase 5 — different files. T028 is NOT parallel: it validates the document those three produce, so it runs after them.
- Phase 5 in parallel with Phases 3–4 entirely.
- **Not parallel**: reading the thirty diffs (T010–T011). Cross-target comparison is the point (FR-013a); splitting them across people or sessions destroys the signal that one target's delta pattern is unlike its peers.

---

## Implementation Strategy

**MVP scope**: Phases 1–3 deliver a green lane, but shipping there would violate FR-006 — the goldens would be committed without recorded attribution. The true minimum shippable unit is **Phases 1–4**.

**Suggested increments**:

1. Phases 1–2 land as their own PR if the normaliser proves non-trivial. It is independently useful and independently testable, and it has no dependency on the refresh.
2. Phases 3–4 land together as the refresh PR. One change, all targets (FR-013).
3. Phase 5 can land before, with, or after — it blocks nothing.

**Estimated shape**: 32 tasks. The volume is in T010–T011 (thirty diffs to read and classify), which is human judgment and does not parallelise without losing the cross-target signal that makes it worth doing.
