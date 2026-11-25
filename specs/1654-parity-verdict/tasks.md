# Tasks: A parity verdict per row, a test helper that can fail, and the closure parity gaps

**Input**: `specs/1654-parity-verdict/` — plan.md, spec.md, research.md (R1–R10), data-model.md, contracts/parity-check-json.md, quickstart.md, measurements/.
**Tests**: requested (SC-001 to SC-005, mutations). Tests are written first and must fail before their implementation.

**Rules that apply to every task:**
- Fixtures use `waybill-fixture-*` names only.
- No new Cargo dependencies.
- Never run `cargo fmt`.
- Mutations are reverted with `cp` from a backup, then rebuilt.
- Python runs as `python3 -I -B`.
- Never pipe a check through `head`.
- Test modules carry `#[cfg_attr(test, allow(clippy::unwrap_used))]`.
- Goldens are regenerated only by `gh workflow run public-corpus.yml -R kusari-oss/waybill --ref main -f branch=1654-parity-verdict -f regen_goldens=true`.

**Phase order:** US1 (verdict) → US3 (fixes) → US2 (shared helper). All three are P1. US2's helper turns eight test files red until US3 has fixed the gaps they exercise, so fixing first keeps every commit green.

## Phase 1: Setup

- [X] T001 Commit the FR-001 measurements already taken:
  - `specs/1654-parity-verdict/measurements/{golden-gaps-before.md, golden-failing-rows-before.md, test-failing-rows-before.md, golden_parity.sh}`.

  Add a note to `golden-failing-rows-before.md`: the instrument was temporary and is not in the tree (`git diff main -- waybill-cli/src/cli/parity_cmd.rs` is empty).

## Phase 2: User Story 1 — A consumer can read each row's parity verdict (P1)

**Goal:** `verdict` on every row of `parity-check --format json` (FR-002, R1).

**Independent test:** on a scan directory where one row is removed from SPDX 3, that row reads `fail`, and the `fail` count equals `summary.parity_gaps`.

- [X] T002 [US1] Write the failing unit tests in `waybill-cli/src/cli/parity_cmd.rs`'s test module:
  - a `SymmetricEqual` row with equal sets is `pass`, and with differing sets is `fail`;
  - a universal row no format carries is `not-checked`, and still counts in `universal_parity_rows_passing`;
  - a non-universal row is `not-checked`;
  - `CdxSubsetOfSpdx` and `PresenceOnly` follow their rules;
  - the number of `fail` rows equals `summary.parity_gaps`;
  - the serialised row keeps every existing key with unchanged values, `verdict` included (contract).
- [X] T003 [US1] Add `verdict: String` (`pass`, `fail`, `not-checked`) to `CoverageRowReport` in `waybill-cli/src/cli/parity_cmd.rs`, set in the branch that increments `universal_pass` or `gaps`. The table renderer is untouched. Make T002 green.
  - **I1, the rule exists twice:** `tests/holistic_parity.rs` (lines 146–190) keeps its own copy of the per-directionality rule. Add a unit test that runs both on the same synthetic sets, one per directionality, both passing and failing, and asserts they agree. Or make `holistic_parity` read `parity-check`'s verdict. Record which in research R1.
- [X] T004 [US1] Update `specs/1654-parity-verdict/measurements/golden_parity.sh` to list rows by `verdict == "fail"` instead of inferring them from counts. Report G1 in its own column as "masking (R3)", so it is never mistaken for a defect. Run it on all goldens, and confirm it reproduces `golden-failing-rows-before.md` exactly: the same 80 rows (SC-001). Save the output as `measurements/golden-failing-rows-verdict.md`.

## Phase 3: User Story 3 — Closure scans and tested fixtures agree across formats (P1)

**Goal:** every gap the closure goldens and the eleven files' fixtures show is fixed at its cause, or recorded as format-restricted with a reason (FR-005).

**Independent test:** `parity-check` reports 0 `fail` rows on the stub closure fixture's scan and on each of the five closure goldens after regeneration.

### Measure first (R7, R8, closure A7)

- [X] T005 [P] [US3] Trace R8 on one binary test (for example `version_string_static`): keep its scan directory, and find why SPDX 2.3 lacks the synthetic root → bundled `.so` edge that CycloneDX and SPDX 3 carry. Check whether it is emitted under another relationship type, or skipped for file-tier targets, in `waybill-cli/src/generate/spdx/relationships.rs`. Record the cause and the fix site in `research.md` R8.
- [X] T006 [P] [US3] Trace R7: find where CycloneDX `metadata.component`'s properties are built (`waybill-cli/src/generate/cyclonedx/metadata.rs` or `builder.rs`), and why only `component-role`, `sbom-tier`, `source-files` and `workspace-member` survive when the root is not also in `components[]`. List which goldens' roots are only in `metadata.component`. Record it in `research.md` R7.
- [X] T007 [P] [US3] Trace closure A7 on the five closure goldens:
  - classify every differing declared-licence value by shape (CycloneDX individual IDs against SPDX expression, `LicenseRef` text, SPDX 3 missing a value, other) with counts per target;
  - for each shape, name the emitter or extractor that is wrong.

  Record it in `research.md` (new R11), with a decision per shape: fix, or format-restricted with a reason. Shapes found only in non-closure goldens go to the issue list (T024).

### Fixes

- [X] T008 [P] [US3] Unit tests, then the B1/B2 target rule (R4a) in `waybill-cli/src/parity/extractors/cdx.rs`: `cdx_dependency_edges` classifies an edge by its target's `scope` only, with a comment citing research R4's measurement.
  - **Tests:** an edge from an excluded source to a runtime target is runtime; an edge into an excluded target is dev.
  - **C1:** then run every test file that asserts on B1/B2 or calls these extractors. Grep `cdx_dependency_edges`, `runtime_deps`, `dev_deps`, `"B1"` and `"B2"` under `waybill-cli/tests/`, which includes `holistic_parity`, `spdx_cdx_parity`, `spdx3_cdx_parity` and every `transitive_parity_*`. Each stays green, or the failure is traced to R4 before going on.
- [X] T009 [P] [US3] Unit tests, then pass 3 (R4b) in `apply_lifecycle_scope_to_edges` (`waybill-cli/src/scan_fs/mod.rs`): a `DependsOn` into a component with `artifact_presence.is_absent_from_artifact()` becomes `BuildDependsOn`. Passes 1 and 2 keep precedence. One INFO line with the rewrite count.
  - **Call site:** the function's last call in `waybill-cli/src/cli/scan_cmd.rs` (around line 5559) runs **before** `closure_role_scope::assign_role_scope` (around 5602) sets `artifact_presence`. Call `apply_lifecycle_scope_to_edges` once more after `withdraw_scope_runtime_evidence_contradicts` (around 5615), or pass 3 types nothing.
  - **Unit tests:** precedence over an already-typed edge; no effect without `artifact_presence`.
  - **Integration assertion,** in `tests/boot_library_edges.rs` or a new closure test: on the stub closure, SPDX 2.3 carries at least one `BUILD_DEPENDENCY_OF` whose target's CycloneDX `scope` is `excluded`. Unit tests alone build `artifact_presence` by hand and cannot catch a wrong call site.
  - **U2:** confirm that `is_absent_from_artifact()` covers milestone 1277's `unreferenced` members on purpose, and say in the comment and in research R4 that `BUILD_DEPENDENCY_OF` is the nearest SPDX type for a member nix keeps but the artifact does not reference.
- [X] T010 [P] [US3] **Done as revised in research R6:** the cause was the emitter (`has_complete_record` lacked milestone 1278's closure-read flag), not the extractors, so the fix is there and the extractors are unchanged. Original text: Unit tests, then the E1 rule (R6) in `e1_cdx`, `e1_spdx23` and `e1_spdx3` (`waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`):
  - CycloneDX is present iff a record has `aggregate == "complete"` and non-empty `assemblies`;
  - SPDX is present iff `complete_ecosystems` is non-empty.

  Tests include the shellcheck shape: no ecosystem complete, plus the root's dependency-only claim, gives absent in all three.
  - **C2:** after the change, run T004's `golden_parity.sh` on all goldens. E1 still passes on the 23 non-closure goldens, and passes on the five closures.
- [X] T011 [US3] Implement R7 at the site T006 found: CycloneDX `metadata.component` carries the same `waybill:*` properties the root carries elsewhere. A unit test pins `source-type`, `evidence-kind`, the closure role and the scope source.
- [X] T012 [US3] Implement R8 at the site T005 found, with a unit test.
- [X] T013 [US3] **Decided at T007 with the maintainer: A7 is its own spec (spec Clarifications, research R11). No change here; the issue is filed in T024.** Original text: Implement each closure A7 decision from T007, with unit tests. Where a shape is format-restricted, record the reason in that row's `docs/reference/sbom-format-mapping.md` entry, and keep the catalog classification consistent: the lib-target gate.
- [X] T014 [US3] Verify, without the helper:
  - Run `parity-check` on a `--nix-closure` scan of `tests/fixtures/nix_closure_boot_versions`, through a temporary uncommitted test using `NixStub`, as spec 1653's T014 did.
  - Confirm 0 `fail` rows, and record it in `measurements/after.md`.
  - **The five closure goldens are not checked here.** They are Linux-only (a Darwin closure differs), so the CI regeneration in T021 and T022 is their check.

## Phase 4: User Story 2 — Test parity assertions can fail (P1)

**Goal:** one shared helper, adopted by eleven files, each proven able to fail (FR-003, FR-004).

**Independent test:** each of the eleven files fails when one format's value of a row it exercises is removed.

- [X] T015 [US2] Create `waybill-cli/tests/common/parity.rs` with `pub fn assert_parity(out_dir: &Path)`, per data-model.md, with all three assertions (no `fail`; `parity_gaps == 0`; `fail` count equals `parity_gaps`), and register it in `tests/common/mod.rs`. It panics listing each failing row's ID, label, kinds and counts. Put `#[allow(dead_code)]` on the module where test binaries that don't use it would otherwise warn, following `common`'s existing convention.
- [X] T016 [US2] Replace each file's own `parity` helper with `common::parity::assert_parity`, deleting the old helper:
  - `binary_kind_technique`, `closure_resolved_tier`, `hpack_include_merge`, `library_identity`, `merged_binary_evidence`;
  - `project_discovery_haskell`, `sbom_edit_retier`, `version_string_anchored`, `version_string_boundary`, `version_string_static`;
  - `boot_library_edges`, removing `PRE_EXISTING_GAPS`.

  Keep each call site; only the helper changes.
- [X] T017 [US2] Run all eleven files, and every other parity suite (`holistic_parity`, `spdx_cdx_parity`, `spdx3_cdx_parity`, `sbom_format_mapping_coverage`, `cargo test -p waybill --lib`). Each must be green. A remaining failure returns to US3; it is never excluded.

## Phase 5: User Story 4 and polish

- [X] T018 Mutations, each via a `cp` backup, rebuild, the targeted test failing, restore and rebuild. Recorded in `measurements/after.md`.
  - **For each of the eleven files (SC-002):** remove, in the emitter, one format's value of a row that file asserts on. For example, C247 from SPDX 3 for `boot_library_edges`; one `waybill:*` row each for the version-string, hpack and discovery files.
  - **For the fixes:**
    - the verdict set to `pass` unconditionally;
    - pass 3 removed;
    - the target rule reverted to "either endpoint";
    - the E1 rules reverted;
    - R7 reverted;
    - R8 reverted;
    - each A7 fix reverted.
- [X] T019 [P] Docs:
  - `docs/reference/conformance-harness-guide.md` (or wherever `parity-check` JSON is documented; grep `parity-check` under `docs/`): the `verdict` field, and that `universal_parity` is the catalog's expectation;
  - `docs/reference/sbom-format-mapping.md`: B1/B2's target rule, the third typing pass and E1's meaning.
- [X] T020 [P] CHANGELOG entry under Unreleased → Fixed. It covers:
  - the verdict field;
  - the helper;
  - the B1/B2 rule and SPDX `BUILD_DEPENDENCY_OF` into artifact-absent closure members;
  - E1;
  - the root's CycloneDX properties;
  - R8 and A7 as found.

  Refs #1695.
- [X] T021 Run `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` (no `+toolchain`). Confirm `GATE_RESULT=pass`, the expected line, and every per-target `test result: ok. N passed; 0 failed`. Then push, and dispatch the corpus with `regen_goldens=true`. Enumerate every job, and confirm the scanned SHA is the branch head.
- [X] T022 Download the regenerated goldens, then:
  - list every changed file;
  - trace each change with a normalised, sorted diff to R4(b), R7, R8 or T013 (SC-005);
  - run `golden_parity.sh` on them, and record the after-table in `measurements/after.md` against `golden-failing-rows-before.md`, with **0 failing rows on the five closure goldens** (SC-003, SC-004).

  Commit the goldens.
- [ ] T023 Dispatch a verification run without regeneration, and confirm every golden passes.
- [X] T024 File the out-of-scope gaps, each with its per-row measurement and the public footer (R10):
  - the non-closure B1/B2 causes (R5, corrected at T024, and `optional`);
  - A8 on image-postgres16;
  - any non-closure A7 shape from T007.
- [ ] T025 Mark the tasks and checklist complete, verifying each claim against the diff. Open the PR with `Refs #1695`, the footer, the before and after tables, and the issue numbers from T024.

## Dependencies

- T001 comes first.
- US1 (T002–T004) comes before everything else: T004's script uses the verdict.
- In US3, T005–T007 come before T011–T013, which use their findings. T008, T009 and T010 are independent of each other and of the traces. T014 comes after T008–T013.
- US2 (T015–T017) comes after US3, so the adopted helper meets fixed code.
- Polish: T018 comes after T017, T021 after T018–T020, then T022, T023, T024, T025.

## Parallel opportunities

- T005 ∥ T006 ∥ T007 (traces).
- T008 ∥ T009 ∥ T010 (separate files).
- T019 ∥ T020.

## Implementation strategy

- **MVP = US1:** the verdict field alone makes every future parity claim checkable.
- **Then US3:** the fixes.
- **Then US2:** the helper turns the eleven files into real checks.
- **Polish:** proves each can fail, and traces the goldens.
