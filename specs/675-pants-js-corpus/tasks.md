---

description: "Task list for feature 675: Pants JavaScript/npm corpus regression gate"
---

# Tasks: Pants JavaScript/npm corpus regression gate

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/675-pants-js-corpus/`
**Prerequisites**: `plan.md` (required), `spec.md` (required), `research.md`, `data-model.md`, `contracts/layer1-assertion.md`, `contracts/js-golden-filter.md`, `quickstart.md`

**Tests**: The filter module unit tests in Phase 2 are part of the deliverable (per `contracts/js-golden-filter.md` "Testing the filter functions"). Layer 1 corpus assertions run via the standard m195 harness — no new test-runner infrastructure required.

**Organization**: Tasks are grouped by user story. US1 delivers the full MVP; US2 is validated by the US1 goldens; US3 adds a documentation baseline for the future option-A follow-up.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- Every task includes exact file paths from the repo root at `/Users/mlieberman/Projects/mikebom/`.

## Path Conventions

Single-project layout — all Rust changes under `waybill-cli/tests/`. Fixture goldens under `waybill-cli/tests/fixtures/public_corpus/pants-example-javascript/`. External artifact: fork at `github.com/kusari-sandbox/example-javascript`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create the external fork the corpus target references, verify the pinned SHA is present.

- [X] T001 Fork `pantsbuild/example-javascript` into `kusari-sandbox` via `gh repo fork pantsbuild/example-javascript --org kusari-sandbox --clone=false` — externally visible action, requires explicit user confirmation before firing. Matches the pattern PR #757 established for the other pants example forks.
- [X] T002 Verify the pinned SHA is reachable in the fork by running `git ls-remote --heads https://github.com/kusari-sandbox/example-javascript main` and asserting the output line begins with `da76d5dbb407d82c136cfe8f18dc06f3c8a440e5`.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Implement the JS-only golden filter module. This MUST be done before any user story work — the corpus target's goldens generated without the filter would be ~570 KB and violate SC-004 (500 KB budget).

**⚠️ CRITICAL**: Phase 3 (US1) tasks cannot start until Phase 2 is complete.

- [X] T003 Create new file `waybill-cli/tests/corpus_harness_195/js_filter.rs` with a doc comment header referencing `specs/675-pants-js-corpus/contracts/js-golden-filter.md`, plus three empty `pub fn` signatures matching the contract: `filter_cdx_to_js(v: &mut serde_json::Value)`, `filter_spdx23_to_js(v: &mut serde_json::Value)`, `filter_spdx3_to_js(v: &mut serde_json::Value)`. Bodies can be `todo!()` at this task — subsequent tasks fill them in.
- [X] T004 Implement `filter_cdx_to_js` in `waybill-cli/tests/corpus_harness_195/js_filter.rs` per `contracts/js-golden-filter.md` "filter_cdx_to_js" retention rules. Filter `.components[]` by `.purl` prefix `pkg:npm/`, filter `.dependencies[]` by `.ref` prefix and `.dependsOn[]` entry prefix, preserve `.metadata` + envelope fields. Handle missing `.components` and missing `.dependencies` fields as silent no-ops.
- [X] T005 Implement `filter_spdx23_to_js` in `waybill-cli/tests/corpus_harness_195/js_filter.rs` per contract. Collect `SPDXID` set of packages retained by npm-external-ref rule + the root document SPDXID (`SPDXRef-DOCUMENT` or any SPDXID in `.documentDescribes[]`). Filter `.packages[]` by that set. Filter `.relationships[]` where both endpoint SPDXIDs are in the set. Preserve `.creationInfo`, `.documentDescribes`, envelope.
- [X] T006 Implement `filter_spdx3_to_js` in `waybill-cli/tests/corpus_harness_195/js_filter.rs` per contract. Iterate `@graph[]`. Keep doc-scope typed nodes (`SpdxDocument`, `CreationInfo`, `Person`, `Organization`, `Tool`). Keep `software_Package` / `software_File` nodes whose `externalIdentifier[].identifier` has `externalIdentifierType == "purl"` AND starts with `pkg:npm/`; collect spdxIds. Keep `Relationship` nodes where both `from` and (filtered) `to` reference kept spdxIds. Preserve `@context`.
- [X] T007 Register the new module in `waybill-cli/tests/corpus_harness_195/mod.rs` by adding `#[allow(unused)] pub mod js_filter;` in the same style as the existing `pub mod cache;` etc. entries.
- [X] T008 Add unit tests for filter functions in `waybill-cli/tests/corpus_harness_195/js_filter.rs` under `#[cfg(test)] mod tests { ... }` with `#[cfg_attr(test, allow(clippy::unwrap_used))]`. Cover: (a) happy-path CDX with mixed npm + pypi components, (b) missing `.dependencies` no-op, (c) idempotency (apply twice, byte-identical), (d) SPDX 2.3 root document retention when it lacks a `pkg:npm/*` external ref, (e) SPDX 3 relationship with mixed `.to` array (some kept, some dropped).

**Checkpoint**: Phase 2 complete → foundation ready. Run `cargo test --test public_corpus corpus_harness_195::js_filter` to confirm all filter unit tests pass. User story work can now begin.

---

## Phase 3: User Story 1 — waybill maintainer refactoring the npm reader (Priority: P1) 🎯 MVP

**Goal**: Add the `pants-example-javascript` corpus target with 4 layer 1 assertion invariants + a JS-only golden trio, so that regressions in the npm reader stack against a Pants-JS monorepo fail nightly CI with a diagnostic naming the suspected regression module.

**Independent Test**: Introduce a synthetic regression in the npm reader (e.g., early-return in `waybill-cli/src/scan_fs/package_db/npm/package_lock.rs`); rebuild; run `WAYBILL_RUN_PUBLIC_CORPUS=1 WAYBILL_CORPUS_SKIP_OCI=1 cargo test --test public_corpus corpus_pants_example_javascript`; confirm the layer 1 assertion fails with a diagnostic naming the npm reader.

### Implementation for User Story 1

- [X] T009 [P] [US1] Add the `pants-example-javascript` `CorpusTarget` entry in `waybill-cli/tests/corpus_harness_195/manifest.rs`, immediately after the existing `pants-example-golang` entry. Fields per `data-model.md` §Entity 1: `name: "pants-example-javascript"`, `source: SourceKind::Git { clone_url: "https://github.com/kusari-sandbox/example-javascript" }`, `pinned: PinnedRef::Sha { hex: "da76d5dbb407d82c136cfe8f18dc06f3c8a440e5" }`, `ecosystem: Ecosystem::Npm`, `exercises: "..."`, `layer1: super::layer1_assertions::pants_example_javascript_layer1`.
- [X] T010 [P] [US1] Add the `pants_example_javascript_layer1` function to `waybill-cli/tests/corpus_harness_195/layer1_assertions.rs` per `contracts/layer1-assertion.md`. Implement all four invariants in order: (1) `npm-transitives-present-at-scale` — count `pkg:npm/*` components ≥ 250; (2) `top-level-devdep-esbuild-present` — any `pkg:npm/esbuild@*`; (3) `top-level-devdep-jest-present` — any `pkg:npm/jest@*`; (4) `no-accidental-pants-annotations-on-npm` — no `pkg:npm/*` component carries `waybill:pants-resolve` OR `waybill:pants-target` in `.properties[]`. Reuse existing helpers `cdx_has_component_purl` and `cdx_has_component_property` from the same file.
- [X] T011 [P] [US1] Add `#[test] fn corpus_pants_example_javascript()` in `waybill-cli/tests/public_corpus.rs`, immediately after the existing `corpus_pants_example_golang` test. Body: single line calling `run_target("pants-example-javascript")`.
- [X] T012 [US1] Add JS-filter dispatch in `waybill-cli/tests/corpus_harness_195/layer2_golden.rs::compare_golden`, immediately after the existing `let masked = mask_nondeterministic(actual_value);` line. Dispatch shape: `if target == "pants-example-javascript" { match format { FailureFormat::Cdx => js_filter::filter_cdx_to_js(&mut masked), FailureFormat::Spdx23 => js_filter::filter_spdx23_to_js(&mut masked), FailureFormat::Spdx3 => js_filter::filter_spdx3_to_js(&mut masked), FailureFormat::All => unreachable!() } }`. Add `use super::js_filter;` at the top of the file. Preserves existing byte-identity guarantee for all 6 other corpus targets per data-model §"Layer 2 compare-golden flow".
- [X] T013 [US1] Compile-check the test binary: `cargo test --test public_corpus --no-run`. Must complete without errors or unused-import warnings.
- [X] T014 [US1] Run the manifest audit tests (no network needed) to confirm the new target passes `public_only_audit`, `public_hostname_allowlist`, `cross_ecosystem_coverage_check`, and `no_credentials_required`: `cargo test --test public_corpus`. All 8 audit tests + all 7 per-target skip tests (6 existing + 1 new) must pass.
- [X] T015 [US1] Generate goldens: `WAYBILL_RUN_PUBLIC_CORPUS=1 WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1 WAYBILL_CORPUS_SKIP_OCI=1 cargo test --test public_corpus corpus_pants_example_javascript`. Confirm the 3 golden files appear at `waybill-cli/tests/fixtures/public_corpus/pants-example-javascript/{cdx.json,spdx-2.3.json,spdx-3.json}`.
- [X] T016 [US1] Verify SC-004: `du -sk waybill-cli/tests/fixtures/public_corpus/pants-example-javascript/` — combined size ≤ 500 KB. If it exceeds, revisit `contracts/js-golden-filter.md` retention rules (candidate over-retention: `.metadata` — should still fit; if not, drop non-JS-relevant metadata annotations).
- [X] T017 [US1] Verify byte-identity across two consecutive runs. Run the same command from T015 twice WITHOUT `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS` on the second run: `WAYBILL_RUN_PUBLIC_CORPUS=1 WAYBILL_CORPUS_SKIP_OCI=1 cargo test --test public_corpus corpus_pants_example_javascript`. Second run must pass (compares emitted output against just-written goldens).
- [X] T018 [US1] Verify SC-002: all 6 pre-existing corpus targets stay byte-identical (no accidental cross-target contamination from the new JS-filter dispatch). Run `WAYBILL_RUN_PUBLIC_CORPUS=1 WAYBILL_CORPUS_SKIP_OCI=1 cargo test --test public_corpus`. All 7 corpus tests must pass. If any pre-existing target regenerates a golden, the dispatch condition in T012 leaked scope — investigate.

**Checkpoint**: US1 delivered. The corpus target now regression-locks Pants-JS monorepo scanning; MVP is shippable at this checkpoint.

---

## Phase 4: User Story 2 — SBOM consumer scanning a Pants-JS monorepo (Priority: P2)

**Goal**: Guarantee that operators scanning a Pants-JS monorepo continue to receive the same output the corpus locks in.

**Independent Test**: Compare the SBOM produced by scanning the pinned fixture against the committed golden. Byte-identity confirms operators get the same output.

**Note**: US2's implementation IS US1's implementation. Task T017 in Phase 3 satisfies US2's Independent Test verbatim (byte-identity across two consecutive runs against the committed goldens). No new tasks are required for US2.

- [X] T019 [US2] Add a source-level comment block above the `pants_example_javascript_layer1` function in `waybill-cli/tests/corpus_harness_195/layer1_assertions.rs` explicitly documenting the "current expected behavior for operators" invariants that this assertion function encodes on their behalf (dual-anchor devDeps + expected absence of Pants annotations per FR-006). The comment cross-references `specs/675-pants-js-corpus/spec.md` §User Story 2 and issue #760 option A as the tracked change trigger.

**Checkpoint**: US2 delivered (jointly by T015-T018 + T019).

---

## Phase 5: User Story 3 — future feature developer building the Pants-JS enricher (Priority: P3)

**Goal**: Provide a concrete before/after baseline that a future contributor implementing issue #760 option A (`pants_js` enricher) can diff against.

**Independent Test**: When option A ships, its author regenerates this corpus target's goldens and observes only additive `waybill:pants-target` annotations on existing `pkg:npm/*` components — no dropped components, no changed PURLs, no reordered edges.

- [X] T020 [US3] Create `waybill-cli/tests/fixtures/public_corpus/pants-example-javascript/README.md` documenting: (a) this fixture directory captures the pre-option-A baseline for Pants-JS SBOM emission, (b) the goldens are JS-filtered per FR-008 clarification 2026-09-02, (c) issue #760 option A is the tracked follow-up that would legitimately regenerate these goldens with additive `waybill:pants-target` annotations, (d) any other regeneration must be justified in the accompanying PR body. Keep the README under 30 lines — this is signal for future contributors, not narrative documentation.

**Checkpoint**: All three user stories delivered. Feature is complete.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final verification + PR mechanics.

- [ ] T021 Run the full pre-PR gate: `./scripts/pre-pr.sh`. Confirms workspace clippy + all workspace tests pass. Per memory `feedback_prepr_gate_full_output`, verify the final "all pre-PR checks passed" line — do not grep-and-declare-victory on partial output.
- [ ] T022 Commit changes with a message summarizing the corpus-target-addition (extending PR #757 to cover the 4th pants ecosystem), following the commit style of PR #757 and PR #761. Include the `Co-Authored-By: Claude Opus 4.7 (1M context)` line.
- [ ] T023 Push the branch and open a PR against `main` on `kusari-oss/waybill`. PR body mirrors PR #761's shape: Summary, Fixture shape (link to research.md R1 findings), Test plan checkbox list. Include an explicit call-out of the `WAYBILL_RUN_PUBLIC_CORPUS=1` gate so reviewers know the target won't run in the default per-PR lane.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 needs explicit user go before firing (external action). T002 depends on T001.
- **Phase 2 (Foundational)**: Depends on Phase 1 completion. BLOCKS all user stories — the JS-filter module MUST exist before US1 can generate goldens without violating SC-004.
- **Phase 3 (US1)**: Depends on Phase 2 completion. T009, T010, T011 can run in parallel (different files). T012 depends on T007 (filter module registered). T013-T018 are strict sequential verification steps.
- **Phase 4 (US2)**: Depends on Phase 3 completion. T019 is documentation-only.
- **Phase 5 (US3)**: Depends on Phase 3 completion (needs the golden files to exist). T020 is documentation-only, can run in parallel with T019.
- **Phase 6 (Polish)**: T021 depends on all preceding phases. T022 depends on T021. T023 depends on T022.

### Within Phase 2 (Foundational)

- T003 must complete first (creates the file skeleton).
- T004, T005, T006 can technically run in parallel because they modify different logical sections of the same file, but for coordination safety, run them sequentially. All three depend on T003.
- T007 depends on T003.
- T008 depends on T004, T005, T006 (needs the implementations to test).

### Within Phase 3 (US1)

- **Parallel opportunity**: T009, T010, T011 modify three different files (`manifest.rs`, `layer1_assertions.rs`, `public_corpus.rs`). Run them in parallel.
- T012 modifies `layer2_golden.rs` — depends on Phase 2 completion, no dependency on T009-T011.
- T013 depends on T009-T012 all done (compile check).
- T014 depends on T013.
- T015 depends on T014.
- T016, T017, T018 depend on T015 (need goldens to exist).

### Parallel Opportunities Summary

- Phase 3: `{T009, T010, T011}` in parallel. `T012` can also run parallel with those three.
- Phase 4 + Phase 5: `T019` and `T020` are both documentation-only, can run in parallel with each other.

---

## Parallel Example: User Story 1

```bash
# Parallel: three independent file additions
Task: "Add pants-example-javascript CorpusTarget in waybill-cli/tests/corpus_harness_195/manifest.rs"
Task: "Add pants_example_javascript_layer1 fn in waybill-cli/tests/corpus_harness_195/layer1_assertions.rs"
Task: "Add corpus_pants_example_javascript test in waybill-cli/tests/public_corpus.rs"

# Sequential: verification steps
cargo test --test public_corpus --no-run                                        # T013
cargo test --test public_corpus                                                 # T014
WAYBILL_RUN_PUBLIC_CORPUS=1 WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1 \
  WAYBILL_CORPUS_SKIP_OCI=1 cargo test --test public_corpus corpus_pants_example_javascript  # T015
du -sk waybill-cli/tests/fixtures/public_corpus/pants-example-javascript/       # T016
WAYBILL_RUN_PUBLIC_CORPUS=1 WAYBILL_CORPUS_SKIP_OCI=1 \
  cargo test --test public_corpus corpus_pants_example_javascript               # T017
WAYBILL_RUN_PUBLIC_CORPUS=1 WAYBILL_CORPUS_SKIP_OCI=1 cargo test --test public_corpus  # T018
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 (Setup) — fork the upstream repo.
2. Complete Phase 2 (Foundational) — implement + unit-test the JS-filter module.
3. Complete Phase 3 (US1) — add corpus target + assertions + goldens.
4. **STOP and VALIDATE**: T017 (byte-identity across two runs) + T018 (existing targets unaffected) is the MVP acceptance gate.
5. At this point the feature is shippable; US2 + US3 add documentation-only polish.

### Incremental Delivery

- **Ship after Phase 3**: the MVP delivers the P1 goal. Documentation tasks in Phase 4 and Phase 5 can land in a follow-up PR if scope-cutting is required — but the T019 comment + T020 README are cheap and fold naturally into the same PR.
- **Recommended**: land the whole feature in one PR — it's small enough that splitting adds review overhead without shipping-velocity gain.

### Parallel Team Strategy

With one contributor (the expected staffing model):

- Phase 1 → Phase 2 → Phase 3 (using the parallel opportunities noted) → Phase 4/5 (parallel) → Phase 6.
- The `[P]` tasks in Phase 3 (T009, T010, T011) and the documentation tasks in Phases 4/5 (T019, T020) benefit less from multi-agent parallelism than from single-agent batching in one commit.

---

## Notes

- `[P]` tasks = different files, no dependencies.
- `[Story]` label maps task to specific user story for traceability.
- Every task references an exact file path per repo-root-relative convention.
- T001 is an externally-visible action; agent MUST NOT fire `gh repo fork` without an explicit user go.
- SC-002 (zero production waybill code changes) is a hard constraint. If any task begins editing `waybill-cli/src/**/*.rs`, halt and re-evaluate scope.
- Every filter helper in Phase 2 MUST be idempotent per `contracts/js-golden-filter.md`. Idempotency is validated by T008(c).
- Do NOT manually edit generated goldens. Regen via `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1`.
