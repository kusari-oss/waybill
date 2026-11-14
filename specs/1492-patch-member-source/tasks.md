# Tasks: Locally built patches are sources, not fragments

**Input**: Design documents from `specs/1492-patch-member-source/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/wire.md, quickstart.md

**Tests**: Requested. The spec's SC-005 asks for a control and a mutation, and FR-005 asks for a committed measurement.

**Paths**:
- `CLOSURE` = `/private/tmp/claude-501/-Users-mlieberman-Projects-mikebom/e93b0155-273d-4de5-8b5f-de35ec790c78/scratchpad/moat-closure/moat-closure.json`. Re-download with `gh run download 43535531153 -n moat-closure-x86_64-linux`, then check the sha256 is `898fcdd38aa235840cc2a76b9ee4048ef826a853ce8abe19efdfb3076c8744ab`.
- `MEAS` = `specs/1492-patch-member-source/measurements/`.
- `CL` = `waybill-cli/src/scan_fs/package_db/nix/closure/`.

**Rules for every task**:
- Revert a mutation with `cp`, never `git checkout`.
- Never pipe a check through `head`.
- Never evaluate a corpus flake locally.

## Phase 1: Setup (baseline, before any code changes)

- [X] T001 Record the edge accounting on the unchanged code: `WAYBILL_TEST_CLOSURE_JSON=$CLOSURE cargo test --bin waybill s1367_edge_accounting -- --nocapture`. Save the block from `#1367 edge accounting` to the last bucket line in `MEAS/edge-accounting-before.txt`. Verify that `projected, identity level` reads 6875. *As built:* `projected, identity level` 6875, matching C184. The closure path is masked in the committed file.
- [X] T002 [P] Record the reader's view on the unchanged code: `waybill debug closure-dump --closure-json $CLOSURE --summary-only` into `MEAS/closure-dump-before.json`, and `--filter .patch` reduced to `[member_role, name, drv, edge_endpoint]` TSV, sorted with `LC_ALL=C`, into `MEAS/patch-members-before.tsv`. Verify there are 5 `fragment` rows and 56 `source` rows. *As built:* 5 `fragment` and 56 `source` rows, as R1 measured.

---

## Phase 2: Foundational (one source of truth for the endpoint answer, research R2)

**Behaviour must not change in this phase**, because the classifier is untouched. A golden or test change here is a defect.

- [X] T003 In `CL/member.rs`, change `is_edge_endpoint(drv)` to `is_edge_endpoint(drv: &RawDerivation, patches: &PatchOutputs<'_>) -> bool`, returning `classify(drv, patches).is_edge_endpoint()`. Rewrite its doc comment: the patch set now decides between `fragment` and `source` (spec 1492 FR-001), so callers must pass the closure's set. Remove the sentence claiming the answer cannot differ. *As built:* done.
- [X] T004 In `CL/graph.rs`, `project()` builds `super::member::PatchOutputs::of(closure)` once. Pass it through `nearest_stopping(..)` to `stops(drv, patches)`, whose `None` arm calls `super::member::is_edge_endpoint(drv, patches)`. Keep `project`'s signature, so none of its 9 callers changes. Also update `account()` in the same file's tests (the two `stops(..)` calls) to build and pass the same set. *As built:* `project` builds the set once. Its signature is unchanged, so none of its 9 callers changed.
- [X] T005 In `CL/emit.rs`, `emitted_drv_keys()` builds `member::PatchOutputs::of(raw)` once and passes it to `member::is_edge_endpoint(drv, &patches)`. Keep the signature. *As built:* the anchor matched two functions; `emitted_drv_keys` was edited by its signature. The other match, the test-only superset `component_drv_keys`, has no endpoint check.
- [X] T006 Verify no behaviour change: `cargo test --bin waybill nix::closure` passes, and `cargo test --test closure_members --test closure_edges --test closure_role_scope` passes. Then re-run T001's command and compare its output with `edge-accounting-before.txt`; they must be byte-identical. *As built:* closure units 224/225; the one failure is T007's CONTROL, written ahead of T008. Integration suites 17/23/15 pass. The accounting is byte-identical to T001 (`cmp`).

**Checkpoint**: endpoint, projection and emitter now share the closure's patch set, and nothing moved.

---

## Phase 3: User Story 1 — A locally built patch is reported as a source (P1) 🎯 MVP

**Goal**: A member another closure derivation lists in `patches` carries C206 `source` whatever its name (FR-001, FR-010).

**Independent Test**: A constructed closure with a listed bare-named patch and an unlisted twin gives `source` and `fragment` respectively, in all three formats.

- [X] T007 [US1] Add unit tests in `CL/member.rs` (`mod tests`, spec 1492 section) that fail before T008:
  - `s1492_a_listed_patch_is_source_whatever_its_name`: a consumer lists `h-waybill-fixture-local.patch`; the patch member has no version token and `dontUnpack`. Expect `source`.
  - `s1492_an_unlisted_twin_stays_fragment`: the same shape, listed by nothing. Expect `fragment`.
  - `s1492_a_listed_generated_patch_is_source_not_scaffolding`: `env.text` plus listed. Expect `source`, per FR-010 and the clarification.
  - `s1492_a_fetched_patch_is_source_by_its_hash`: has a content hash and **no output path**, as in the real closure (research R1). Expect `source`.
  - `s1492_the_endpoint_helper_agrees_with_classify_given_the_patch_set`: over `CASES` and the new members. Its control: for the listed bare patch, `is_edge_endpoint(d, &PatchOutputs::of(&raw)) != is_edge_endpoint(d, &PatchOutputs::default())`. *As built:* six tests. Five are as listed. The agreement test replaced `s1380_the_patch_set_never_changes_the_endpoint_answer`, at the same position with its history in the doc comment (A1). `s1492_the_classifier_stays_total` was added (U1). Three failed before T008, as expected.
- [X] T008 [US1] In `CL/member.rs` `classify()`, insert `if patches.lists(drv) { return MemberRole::Source; }` directly after the content-hash rung and before `env.text`. Remove `patches.lists(drv) ||` from the version-name rung's `Source` clause. Update the function's doc comment to the FR-010 order. T007 now passes. *As built:* the doc comment states the FR-010 order and records that this change moves edges.
- [X] T009 [US1] Rewrite the three spec 1380 tests in `CL/member.rs` per research R4, keeping each one's control:
  - `s1380_only_package_members_move_and_only_between_endpoints`: allow a `Fragment → Source` move for a listed member, and assert that it is the only move that changes endpoint status.
  - `s1380_the_patch_set_never_changes_the_endpoint_answer`: replace it with an agreement test that uses the patch set, or delete it in favour of T007's agreement test, and record which in the as-built note.
  - `s1380_source_trees_and_patches_are_source`: `q.drv` is now `Source`. Add an unlisted twin that stays `Fragment`. *As built:* the unit tests were rewritten. The three spec 1380 **integration** tests in `closure_members.rs` that pin R5 (`member_roles.json` lists `waybill-fixture-local.patch`) were updated as well. `s1380_the_graph_is_unchanged` became `s1492_the_graph_gains_only_the_listed_bare_patch`, since its claim is no longer true.
- [X] T010 [US1] Add the integration fixture `waybill-cli/tests/fixtures/nix_closure_members/local_patch.json`, using synthetic `waybill-fixture-*` names. It needs:
  - a consumer package that lists a local patch in `patches` and consumes it through `inputs.drvs`;
  - the patch derivation (`dontUnpack`, no hash, no version token), which consumes a builder derivation;
  - an unlisted twin. *As built:* the twin is consumed but not listed. As a `fragment` it is never an edge endpoint, so it is an orphan by construction (C1). C206 parity is checked with the registered extractor.

  Add `s1492_a_listed_local_patch_is_source_in_every_format` to `waybill-cli/tests/closure_members.rs`, using the file's `scan_closure` stub-nix harness. Assert C206 `source` on the patch and `fragment` on the twin in CycloneDX, SPDX 2.3 and SPDX 3.
- [X] T011 [US1] Mutation (SC-005): delete the T008 rung (cp-backup `CL/member.rs` first). Expect T007's listed cases, T009's `q.drv` and T010 to fail. Restore with `cp`, re-run, and record the failing test names in `MEAS/mutations.md`. *As built:* 13 tests fail (7 unit, 6 integration), listed in `mutations.md`. They include two spec 1380 tests, because the rung now carries spec 1380's own `patches` evidence. The file was restored with `cp` and compared with `cmp`.

**Checkpoint**: US1 is complete and independently testable.

---

## Phase 4: User Story 2 — The graph names the patch a package was built with (P1)

**Goal**: A reclassified patch is a stopping point. Its consumer has an edge to it, and edges past it start at it (FR-004).

**Independent Test**: On `local_patch.json`, the consumer has a dependency edge to the patch, and the patch has an edge to its builder.

- [X] T012 [US2] Add the unit test `s1492_the_walk_stops_at_a_listed_patch` in `CL/graph.rs`, using an inline closure: consumer → listed local patch → builder package. Expect projected edges `(consumer, patch)` and `(patch, builder)`, and no `(consumer, builder)`. Add a control: the same closure with the patch unlisted gives `(consumer, builder)`. *As built:* done, with the control.
- [X] T013 [US2] In `waybill-cli/tests/closure_members.rs`, add `s1492_a_listed_local_patch_is_an_edge_target_in_every_format` on `local_patch.json`:
  - the consumer → patch dependency edge exists in all three formats;
  - the patch carries no `waybill:orphan-reason = closure-member-not-an-edge-endpoint`;
  - the unlisted twin still does, if it is an orphan. *As built:* edges are compared by element name in all three formats. The orphan-reason assertion is unconditional (C1).
- [X] T014 [US2] Mutation (research R2): make `is_edge_endpoint` ignore its `patches` argument and use `PatchOutputs::default()` (cp-backup first). Expect T012 and T013 to fail while T007's classification tests still pass, which proves the role and the graph are wired separately. Restore with `cp` and record the result in `MEAS/mutations.md`. *As built:* only the 4 graph tests fail and every classification test passes (R2 proven). Restored with `cp` and compared with `cmp`.
- [X] T015 [US2] Measure the edge delta (FR-005):
  - Re-run T001's command into `MEAS/edge-accounting-after.txt`, and T002's commands into `MEAS/closure-dump-after.json` and `MEAS/patch-members-after.tsv`. Run each 3 times and confirm the outputs are byte-identical.
  - Write `MEAS/edge-delta.md` with the before and after tables, and the change in each bucket and in `projected, identity level`, which is the new C184 `edges`.
  - Attribute the change to the 5 members (SC-003). The edge-by-edge attribution against the golden is completed in T021. *As built:* `edge-delta.md`. 6,875 → 6,891 (+16); `direct` +16 = 5 walked-through + 11 from-unemitted; 3 identical runs. The per-edge check is in T021.

**Checkpoint**: US1 and US2 together are the feature. The edge change is measured.

---

## Phase 5: User Story 3 — Fragment keeps meaning a live-bootstrap function (P2)

**Goal**: No `fragment` member is listed in any `patches` attribute (SC-002).

- [X] T016 [US3] From `MEAS/patch-members-after.tsv` and `closure-dump-after.json`, record in `MEAS/edge-delta.md`:
  - `fragment` is 240 (from 245) and no-`pname` `source` is 558 (from 553);
  - 0 `.patch`-named rows are `fragment`;
  - the 5 reclassified members by name and `.drv` are the same 5 as in research R1 (SC-001). *As built:* in `edge-delta.md`, with units labelled (I1): derivations from closure-dump, components from C184.

  Any other change in role counts is a defect: stop and investigate.

---

## Phase 6: Polish, docs, goldens

- [X] T017 [P] In `docs/reference/sbom-format-mapping.md`, edit the C206 row per `contracts/wire.md`:
  - add the `patches` rung to the evidence-order sentence, before `env.text`;
  - move the `patches` clause out of the version-name rung's description;
  - delete "A member outside the rung keeps its role (#1502 tracks local patches that are `fragment`)". *As built:* done.
- [X] T018 [P] In `docs/reference/nix-evaluation.md`:
  - update the evidence-order paragraph to the FR-010 order;
  - replace "A member outside the rung keeps its role. That includes four local patches with no version token, which are `fragment` today (#1502)" with the new behaviour;
  - update the role-table `fragment` count to the value measured on the regenerated golden (T021), not a predicted one. *As built:* the whole role table was refreshed from the regenerated moat golden's C184 `member-roles`, with the golden and date named (I2): 511/23/46/267/240. The evidence-order paragraph and the #1502 sentence were replaced. The "leaves 513 orphans" figure, already stale at 512, is now the measured 507.
- [X] T019 [P] In `CHANGELOG.md`, add an `Unreleased` → `Changed` entry: locally built patches listed in a `patches` attribute are C206 `source` and dependency-edge targets (#1502). Place it so it is not adjacent to the lines of another unreleased entry that is still open in a stacked PR. *As built:* at the top of `Unreleased` → `Changed`. #1509 adds no CHANGELOG entry, so no stacked PR shares these lines.
- [X] T020 Commit, push the branch, and dispatch `public-corpus.yml` with `--ref 1492-patch-member-source -f branch=1492-patch-member-source -f regen_goldens=true`. Download the regenerated goldens artifact. *As built:* run 43547217059.
- [X] T021 For each `nix-closure-*` target, diff the committed and regenerated goldens, normalized (content-addressed IDs masked) and sorted with `LC_ALL=C`. Write `MEAS/golden-review.md`, per target:
  - the changed components, each a member listed in `patches` that moved `fragment → source`;
  - the changed edges, each incident to such a member or replacing a walk through one;
  - the C184 count changes;
  - 0 unexplained changes (SC-003, SC-004). *As built:* `golden-review.md`. 67/67 files present; 15 differ, all `nix-closure-*`, and the other 52 are byte-identical. On every target, every role change is `fragment → source` and every changed edge touches a moved member: 0 unexplained. Moat C184 `edges` is 6,891, equal to T015, with 0 edges removed. Parity is unchanged (A5 fails before and after). Four further differences are each explained in the file: C184 `wanted` +N; shellcheck's 5 subject fan-out edges; the openai/slack-web stdenv fan-out; the graph-completeness orphan count. The C2 deduction is stated in the file.

  For moat, the C184 `edges` must equal T015's `projected, identity level`. Then commit the goldens.
- [X] T022 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Confirm `GATE_RESULT=pass`, and confirm the per-target `test result: ok` lines with 0 failed. *As built:* re-run on the final tree, with goldens and docs: `GATE_RESULT=pass`, 393 test targets, 8,085 passed, 0 failed.
- [X] T023 Tick every task with an as-built note. Then open the PR, stacked on #1509 (`--base 1367-closure-edge-count`, or `main` once #1509 lands), with "Closes #1502" and the measurement summary. *As built:* the PR is stacked on #1509. A simulated squash-land of #1509 followed by a merge of this branch conflicts in `graph.rs` `account()`, because the `stops` calls #1509 added now take the patch set. The resolution is mechanical (take this side) and is recorded in the PR body.

---

## Dependencies & Execution Order

- **Phase 1 (T001, T002)** must run before any code change. It is the "before" half of FR-005.
- **Phase 2 (T003 → T004, T005 → T006)** blocks every story. T004 and T005 touch different files and can be done in either order after T003.
- **US1 (T007 → T008 → T009 → T010 → T011)** depends on Phase 2.
- **US2 (T012–T015)** depends on US1. The edges move only once the role does.
- **US3 (T016)** depends on T015's data.
- **Polish:** T017–T019 can run in parallel once T016 is done. T020 → T021 → T022 → T023 run in sequence. T018's count waits for T021.

## Parallel Opportunities

- T001 and T002, the two baseline reads.
- T004 and T005, in different files once T003 lands.
- T017, T018 (except its count) and T019, all docs in different files.

## Implementation Strategy

US1 and US2 ship together. A role change without the matching graph change is exactly the disagreement research R2 rules out, so there is no smaller MVP than Phases 1–4. US3 and the golden review complete the evidence the PR carries.
