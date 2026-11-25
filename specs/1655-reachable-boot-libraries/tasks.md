# Tasks: Closure documents contain every boot library a boot library depends on

**Input**: `specs/1655-reachable-boot-libraries/` (spec, plan, research R1–R6, data-model, contracts/wire.md, quickstart)
**Tests**: requested by the spec (mutations, the stub fixture in three formats, SC-001 to SC-006), so each story's tests come first and must fail before the code they cover.

**Scope rule** (user feedback, 2026-11-25): one gate-and-regenerate cycle. A side finding is filed as an issue (T016), never fixed in this branch. The PR opens at the first green gate (T013), not after the corpus run.

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup

- [ ] T001 Re-run `specs/1655-reachable-boot-libraries/measurements/shared_purls.py` on `main`'s goldens and `cmp` the output against `measurements/shared-purls-before.txt`. Identical means the before-state still holds; any difference is recorded in `measurements/after.md` before code changes.

---

## Phase 2: Foundational (blocks both stories)

- [ ] T002 Make `compiler_supplied_component` `pub(crate)` in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/closure_identity.rs`. No other change; its doc comment gains one line naming spec 1655 as a second caller.
- [ ] T003 In `waybill-cli/src/resolve/boot_library_edges.rs`, change the signatures. Behaviour must stay exactly today's when `compilers` is `None`.
  - `apply(components, relationships, compilers: Option<&ClosureCompilers>)`.
  - `apply_with(table, versions: &dyn Fn(&str, &str) -> Option<String>, compilers, components, relationships)`. `versions` is (release, name) → milestone 966's version; `apply` passes `|r, n| boot_table::table().version_of(r, n, &cc.system).map(str::to_string)`.
  - Update the call in `waybill-cli/src/cli/scan_cmd.rs` to pass `closure_compilers.as_ref()`, and the existing unit tests to pass `None`.
  - Run `cargo test -p waybill --lib boot_library_edges`. All existing tests must pass unchanged.

---

## Phase 3: User Story 1 — every boot-library edge has a target (P1) 🎯 MVP

**Goal**: FR-001, FR-005, FR-006, FR-007. Dependencies resolve by PURL, and the reachable set is closed.
**Independent test**: on the stub, every C247 is `{"source":…}`, C105 has no `boot-library-edges-incomplete`, and the added counts are 18 (with ghc-prim) and 19 (plain) (`measurements/stub-before.md`).

### Tests for User Story 1 (write first; each must fail before T007/T008)

- [ ] T004 [US1] Unit tests in `waybill-cli/src/resolve/boot_library_edges.rs`, all built on `apply_with` with a `ClosureCompilers` whose project is `ghc-9.10.3`:
  - **(a) `a_dependency_held_under_another_label_resolves_by_purl`.** `ghc-binary-9.8.4` `base@4.19.2.0` and a `ghc-9.10.3` `array@0.5.8.0`. The edge base→array is drawn, and base's C247 does not list `array`.
  - **(b) `the_reachable_set_is_closed`.** A 9.10.3 `base` alone gains `ghc-internal`, `ghc-prim`, `ghc-bignum` and `rts`, each with C218 `ghc-9.10.3`, and every C247 is `{"source":…}`.
  - **(c) `a_shared_version_is_one_component`.** 9.10.3 `base` and 9.8.4 `base` both reach `rts@1.0.2`, which is added once, labelled `ghc-9.10.3`.
  - **(d) `an_unreached_library_is_not_added`.** `Cabal`, which nothing present depends on, is absent after the pass.
  - **(e) `a_held_purl_is_never_duplicated`.** A component already holding `pkg:hackage/rts@1.0.2` without C218 is reused, not duplicated.
  - **(f) `a_version_the_table_lacks_stays_absent`.** The `versions` closure returns `None` for `rts`. Nothing is synthesized for it, C247 lists `rts` as absent, and `summary.incomplete > 0`. This is the reason's positive control (research R6).
  - **(g) `none_compilers_is_todays_behaviour`.** With `None`, case (a) still lists `array` as absent and nothing is added.

  Run and confirm (a) to (f) fail, and (g) passes.
- [ ] T005 [US1] Rewrite the three tests research R6 names in `waybill-cli/tests/boot_library_edges.rs`, keeping their names' intent:
  - **`every_boot_library_carries_c245`:** every C218 component's C247 has `source` and no `absent`; `ghc-internal` is a component, and 9.10.3 `base` → `ghc-internal` exists.
  - **`edges_stay_within_a_compiler` → `an_edge_reaches_the_version_its_release_ships`:** for every boot edge a→b, b's version equals the stub's 9.10.3 or 9.8.4 version of b, matching a's release. The bootstrap `base@4.19.2.0` reaches `ghc-prim@0.11.0` and never `ghc-prim@0.12.0`. The precondition assertion that both compilers are present is kept.
  - **`incomplete_boot_library_edges_are_reported` → `boot_library_edges_are_complete`:** C105 lacks `boot-library-edges-incomplete` and still contains `transitive-edges-unresolvable`, so the document stays `partial`. The `compositions[]` check is kept, since `hackage` is still unresolvable.
- [ ] T006 [US1] New integration tests in `waybill-cli/tests/boot_library_edges.rs`, both through `scan_with`, so `assert_parity` runs:
  - **`added_counts_match_the_measurement`:** boot components minus those present before (`base`, `ghc-prim@0.12.0` if declared, `process`, `stm`, 9.8.4 `base`) number 18 with ghc-prim and 19 plain.
  - **`no_added_boot_library_is_an_orphan`:** no C218 component carries `waybill:orphan-reason`.

  Confirm all of T005 and T006 fail on the unchanged code.

### Implementation for User Story 1

- [ ] T007 [US1] In `apply_with`, `waybill-cli/src/resolve/boot_library_edges.rs`: build the set of every component PURL. When resolving a dependency, try (label, name) first, then `pkg:hackage/<dep>@<versions(release, dep)>` against the set (FR-005). Run T004 (a) and (g).
- [ ] T008 [US1] In `apply_with`, with `Some(cc)`: before the edge loop, close each walkable release's reachable set breadth-first over sorted collections, following plan.md's Design step 2. Synthesize each unheld PURL once with `compiler_supplied_component(name, Some((&release, version)))`, push it, and update the PURL set and the (label, name) index. Add `added` to `BootEdgeSummary`, and print it in the INFO line in `waybill-cli/src/cli/scan_cmd.rs`. Then run T004 to T006 until all pass.
- [ ] T009 [US1] Mutations. Revert each with `cp` from a backup taken before mutating, then rebuild (`feedback_mutation_revert_with_cp_not_git_checkout`). Record each in `measurements/after.md` with the failing test names:
  - **M1:** disable the PURL lookup; T004 (a) and the T005 version test must fail.
  - **M2:** skip the synthesis; T004 (b) and (c), T005 `every_boot_library_carries_c245` and T006 counts must fail.
  - **M3:** synthesize each release's whole table instead of the reached set; T004 (d) and T006 counts must fail.
  - **M4:** label every synthesized library with the reaching compiler instead of the project rule; T004 (c) must fail.

**Checkpoint**: US1 is complete on the stub.

---

## Phase 4: User Story 2 — added libraries carry a truthful identity (P2)

**Goal**: FR-002, FR-003, FR-004.
**Independent test**: each added component's key set, PURL, version and CPE match the contract (`contracts/wire.md`).

- [ ] T010 [P] [US2] Unit test `the_version_is_milestone_966s` in `waybill-cli/src/resolve/boot_library_edges.rs`. With the real `boot_table`, release `9.2.4` (no `rts` version in 1653's table), the added `rts` is `pkg:hackage/rts@1.0.2`. The existing `libraries_and_versions_equal_milestone_966s` in `boot_dependencies.rs` already covers FR-003's agreement; leave it as is.
- [ ] T011 [US2] Integration test `added_libraries_have_their_peers_shape` in `waybill-cli/tests/boot_library_edges.rs`. Every added component (T006's set) has:
  - a PURL starting `pkg:hackage/`;
  - no `cpe`;
  - no CycloneDX `scope`;
  - `waybill:sbom-tier` = `build`;
  - `waybill:*` keys exactly {C218, C247, `waybill:sbom-tier`}, which is the shape of the stub's 9.8.4 `base` today (`measurements/stub-before.md`);
  - a version equal to the row for (its C218 release, name) in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/ghc_boot_libraries.tsv`, read from the committed file.

  If the key-set assertion fails, the synthesis is in the wrong place (research R2). Stop and reopen R2 rather than adding properties.

---

## Phase 5: Polish & cross-cutting

- [ ] T012 [P] Update the documentation:
  - the C247 row in `docs/reference/sbom-format-mapping.md`, per `contracts/wire.md`: replace "(`rts` and `ghc-internal` never are)" and state that `absent` names only dependencies the version table lacks;
  - a `CHANGELOG.md` `[Unreleased]` entry: closure documents gain every boot library a boot library depends on, edges resolve by PURL, and shellcheck's golden reads `complete`. Refs #1696.
- [ ] T013 Run the gate:
  - `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`.
  - Check `GATE_RESULT=pass`, the expected line, and the per-target totals.
  - **On the first green gate, commit, push, and open the PR** with a body: Refs #1696, #1688, plus the footer.
  - Then dispatch: `gh workflow run public-corpus.yml -R kusari-oss/waybill --ref main -f branch=1655-reachable-boot-libraries -f regen_goldens=true`.
  - Enumerate every CI job's conclusion.
- [ ] T014 After the dispatch commits the goldens, pull them and write `measurements/after.md`:
  - **SC-001, SC-002:** re-run `shared_purls.py`; every count is 0. Record the distinct added PURLs per target against 11/8/10/12/9.
  - **SC-003:** shellcheck's C104 is `complete`; the other four C105 values lose only the `boot-library-edges-incomplete` clause.
  - **SC-004:** C105 orphan counts are unchanged per target.
  - **SC-005:** run `specs/1654-parity-verdict/measurements/golden_parity.sh` on all goldens; the failing rows are a subset of spec 1654's after-table.
  - **R4:** no shared component's edge set shrinks against the before-golden.
  - **Golden diffs:** trace every changed file (a normalized, sorted diff with content-addressed IDs masked) to FR-001 through FR-006.
  - Commit and push.
- [ ] T015 Close out:
  - verify each task's claim against the code and the measurements, not the checkbox (`feedback_a_task_can_be_unimplemented_not_unticked`);
  - tick the tasks;
  - confirm zero new Cargo dependencies (`git diff main -- '**/Cargo.toml' Cargo.lock` is empty).
- [ ] T016 File the out-of-scope items as issues, unless one already exists: (i) whole-set boot-library emission; (ii) the CycloneDX `scope` of boot libraries, which today carry none, including the bootstrap compiler's. #1707 is already filed. Each issue gets "Refs #1696" and the footer.

---

## Dependencies & Execution Order

- **T001** runs first.
- **T002 and T003** block every story.
- **US1:** T004–T006 (tests that fail first) → T007 → T008 → T009.
- **US2:** T010 can be written alongside T004, since it is the same file but an independent test. T011 depends on T008, which is what produces added components.
- **Polish:** T012 can run alongside T009–T011. T013 needs T012. T014 needs T013's dispatch. T015 and T016 come last.

## Parallel Examples

- T010 and T012 (different concerns) while T009's mutation rebuilds run.
- T016's issue drafting while T013's corpus dispatch runs.

## Implementation Strategy

- **MVP:** US1 (T001–T009). The stub shows every edge with a target.
- **US2** adds the identity checks. Its behaviour comes from US1's constructor, so US2 is verification rather than new code. If T011 fails, that is a design signal (R2), not a property to add.
- **One cycle:** a single gate, a single PR, a single regeneration dispatch.
