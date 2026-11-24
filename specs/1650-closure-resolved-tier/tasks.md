# Tasks: Closure-resolved Haskell components are not labelled `design` (#1681)

**Input**: `specs/1650-closure-resolved-tier/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested by the spec (SC-001, SC-003, SC-006). Fixture cases are written red before the pass exists. I11 is written before the goldens move, so `main`'s committed goldens are its positive control.

**Paths**:
- `RT` = `waybill-cli/src/resolve/closure_resolved_tier.rs` (new)
- `SC` = `waybill-cli/src/cli/scan_cmd.rs`
- `HP` = `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs`
- `IT` = `waybill-cli/tests/closure_resolved_tier.rs` (new)
- `INV` = `waybill-cli/tests/corpus_harness_195/invariants/`
- `GD` = `waybill-cli/tests/fixtures/public_corpus/`
- `MS` = `specs/1650-closure-resolved-tier/measurements/`

Research decisions are R1–R8; fixture cases are contracts/wire.md's P1–P3, B1–B3, R1 and N1, and I11 cases V1–V5.

## Phase 1: Setup

- [ ] T001 FR-001 census, in `MS/census.md`. For every `design` component in the five closure goldens in `GD` (`main` `18691d8`), record:
  - its group: C203 `exempt-evaluated-package`, C218 (and which compiler), or neither;
  - whether a component already held its PURL before the closure ran, which explains why the closure did not synthesise it as `build` (R1);
  - its evidence (`package.yaml` / `.cabal` occurrence, or none).

  Confirm the counts in research R7: 21, 22, 24, 24 and 0, and the `readme` exception. Confirm that no `design` component is versioned from a bootstrap `ghc-binary`. Trace moat's evidence-free `transformers` and `unix` to the reader output that emitted them. A count that contradicts R7 amends the spec before T004.
- [ ] T002 Build `main`'s binary at `18691d8` into the session scratchpad as `waybill-main-18691d8`, after confirming `git diff 18691d8 -- waybill-cli waybill-common Cargo.toml Cargo.lock` is empty. Record the full SHA in `MS/control.md`.

## Phase 2: Foundational

- [ ] T003 Create `IT` with the stub-`nix` harness, copied from `waybill-cli/tests/closure_role_scope.rs` (stub `nix` from `nix_config_closure.rs`; package-set cache seeded as `nix_haskell_resolution_m926.rs` does; `#![cfg(unix)]`; no host cache). Include:
  - a three-format scan helper with parity and unique `bom-ref`s;
  - `tier(doc, purl_prefix)` read from all three formats, asserting that they agree;
  - `root_direct_tiers(doc)`;
  - `control_byte_identical_to_main`, with `WAYBILL_TIER_MAIN_BIN` and version-4 UUIDs masked.

## Phase 3: User Story 1 — the project's own package says the closure built it (P1) 🎯 MVP

**Goal**: FR-002.
**Independent test**: P1–P3 and R1.

- [ ] T004 [US1] Write P1, P2, P3 and R1 in `IT`. P1, P2 and R1 are red; P3 passes (a guard).
- [ ] T005 [US1] Create `RT` with `pub(crate) fn retier(components: &mut [ResolvedComponent]) -> RetierSummary` (data-model). For a `design` component carrying C203 = `exempt-evaluated-package`, set `build`. Unit tests `s1650_*`:
  - the evaluated package becomes `build`;
  - a component already `source` or `build` is untouched;
  - a `design` component with no C203 is untouched;
  - C214 has no effect, checked with `read` and with `absent`.

  Register the module in `waybill-cli/src/resolve/mod.rs`.
- [ ] T006 [US1] In `SC`, call `retier` straight after `closure_role_scope::withdraw_scope_runtime_evidence_contradicts` (both follow `assign_role_scope`, whose C203 it reads), and log the summary at INFO when non-zero. P1, P2, P3 and R1 pass.

## Phase 4: User Story 2 — a boot library of the closure's compiler has one tier (P1)

**Goal**: FR-003.
**Independent test**: B1–B3.

- [ ] T007 [US2] Write B1, B2 and B3 in `IT`. B1 is red; B2 and B3 pass (guards).
- [ ] T008 [US2] In `RT`, add the C218 rule: a `design` component carrying `waybill:boot-library-compiler` becomes `build`. Unit tests:
  - a C218 component becomes `build`;
  - a `design` component with neither annotation is untouched.

  In `HP`, rewrite `apply_compiler_version`'s doc comment: the tier is now decided by `closure_resolved_tier::retier`, which supersedes milestone 966's 2026-11-12 clarification as #1681 asks (research R2). B1, B2 and B3 pass.

## Phase 5: User Story 3 — a regression fails CI (P2)

**Goal**: FR-006 (I11).
**Independent test**: V1–V5, and the positive control.

- [ ] T009 [US3] Create `INV/checks/closure_resolved_tier.rs` with `KINDS` and `check`, following `INV/checks/closure_role_scope.rs`:
  - **precondition:** a component carrying C214, else `NotApplicable`;
  - **(a)** a `design` component carrying C203 `exempt-evaluated-package` or C218;
  - **(b)** the root has direct dependencies and all are `design`;
  - each violation names the component, or the root's direct dependencies with their tiers.

  Unit tests V1–V5. Register `pub mod closure_resolved_tier` in `INV/checks/mod.rs`, and an `Invariant { id: "I11", … }` entry in `INV/mod.rs`'s `REGISTRY`.
- [ ] T010 [US3] Positive control, recorded in `MS/control.md`. With I11 registered and `GD` still holding `main`'s goldens, `golden_matrix_is_evaluable_and_gated` must fail with I11 undeclared failures on exactly moat, openai, slack-web and slacklinker, and must pass on shellcheck. Record the output verbatim.
- [ ] T011 [P] [US3] Catalog I11 in `docs/reference/sbom-invariants.md`: what it catches, its boundary with I9 and I10 (they assert scope, I11 asserts tier), and what to do when it fires (R6).

## Phase 6: User Story 4 — nothing else changes (P2)

**Goal**: FR-007; SC-002, SC-004–SC-006.

- [ ] T012 [US4] Write N1 in `IT`, and run `control_byte_identical_to_main` for N1 against T002's binary; it must pass. As a positive control, P1 must differ.
- [ ] T013 [US4] Mutations (SC-006), with `MS/mutate.py` copied from spec 1649's runner:
  - Ma: FR-002's rule removed;
  - Mb: FR-003's rule removed;
  - Mc: the `design` guard removed (re-tiers any tier);
  - Md: I11 clause (a) removed;
  - Me: I11 clause (b) removed.

  Every mutation must be killed. Restore with `cp`, rebuild, and record the results in `MS/mutations.md`.
- [ ] T014 [US4] Corpus. Push, and dispatch `public-corpus.yml -f branch=1650-closure-resolved-tier -f regen_goldens=true`. Confirm the scanned commit and every job's conclusion.
  - Install only goldens that differ, after tracing each one.
  - Every non-closure golden must be byte-identical (SC-004).
  - Each closure golden diff must be a tier change on a T001 component, or a document-level aggregate (CycloneDX `metadata.lifecycles`, SPDX 3 `software_sbomType`).

  Record in `MS/corpus.md`.
- [ ] T015 [US4] SC-002: on the installed goldens, count `design` per closure target (expected 1, 0, 0, 0, 0) and reachability without `design` nodes (expected 1,134, 1,362, 1,235, 2,441, 2,085). I11 must pass on all five, and `golden_matrix_is_evaluable_and_gated` must pass. Record in `MS/corpus.md`. Then run a verification dispatch (no regeneration) at the final commit, which must pass.

## Phase 7: Polish

- [ ] T016 [P] Docs:
  - in `docs/reference/sbom-format-mapping.md`, the C203 and C218 rows: under `--nix-closure` they imply `waybill:sbom-tier=build` (R5);
  - in `docs/reference/sbom-types.md`, a paragraph on closure scans: the evaluated package and compiler-supplied boot libraries are `build`; manifest-only `design` is unchanged.
- [ ] T017 [P] `CHANGELOG.md`: a Fixed entry citing #1681, with T015's numbers. It must state the reversal of milestone 966's clarification.
- [ ] T018 Run the full gate on the final commit, and record in `MS/gate-log.md`:
  - the per-target totals, `GATE_RESULT` and the expected line;
  - that `git diff 18691d8 -- Cargo.lock '*Cargo.toml'` is empty.
- [ ] T019 Close-out: check each task's claim against the tree and `MS/`, note as-built differences inline, and tick each task.

## Dependencies

- **Setup:** T001 → T004 (its counts can amend the spec). T002 → T012.
- **Foundational:** T003 → T004 and T007.
- **US1:** T004 → T005 → T006.
- **US2:** T007 → T008. US2 depends on T005's module.
- **US3:** T009 → T010. **T010 must run before T014 installs new goldens,** or the positive control is lost. T011 can run any time after T009.
- **US4:** T012 after T008. T013 after T009. T014 after T010 and T013. T015 after T014.
- **Polish:** T016 and T017 after T015. T018 and T019 last.

## Parallel opportunities

- T001 with T002 and T003.
- T011 with T012 and T013.
- T016 with T017, while T014's CI run is in progress.

## Implementation strategy

- **MVP:** US1 and US2 together. They are one pass, and either alone leaves the cliff:
  - US1 alone gives the root a `build` dependency, whose subgraph still holds `design` boot libraries;
  - US2 alone leaves the root's only dependency `design`.
- **The guard before the goldens:** I11 (US3) is written and shown failing on `main`'s goldens before the regeneration replaces them.
