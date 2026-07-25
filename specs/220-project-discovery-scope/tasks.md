---

description: "Task list for milestone 220 — --project-discovery=<mode> flag capping main-module discovery scope"
---

# Tasks: `--project-discovery=<mode>` — cap main-module discovery scope

**Input**: Design documents from `/specs/220-project-discovery-scope/`
**Prerequisites**: spec.md, plan.md, research.md, data-model.md, contracts/, quickstart.md — ALL committed on branch `220-project-discovery-scope` (commit `7edd4cc` and earlier).

**Tests**: Yes — TDD-style unit tests for `ProjectDiscoveryMode` + `apply_scope_filter` per contracts/scope-filter-algorithm.md. Integration tests gated by SC-001/SC-002/SC-003/SC-004/SC-006/SC-007/SC-008/SC-009/SC-011/SC-012. SC-005 byte-identity is load-bearing.

**Organization**: Tasks grouped by user story. **Both US1 and US2 are P1 (co-required)** — US1 delivers shallow-scan; US2 delivers workspace-member preservation. Neither ships without the other or the flag is broken by design. US3 (Strict mode) is P3 and independently completable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- File paths in descriptions are absolute repository-relative.

## Path Conventions

Single-crate (`waybill-cli`) touch per plan.md's Project Structure section.
- Production code: `waybill-cli/src/**`
- Tests: `waybill-cli/tests/**` (integration) + `#[cfg(test)] mod tests` (unit)
- Docs: `docs/reference/**`
- Spec artifacts: `specs/220-project-discovery-scope/**`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify branch state + capture pre-implementation baselines that the SC-005 byte-identity gate leans on.

- [ ] T001 Verify branch `220-project-discovery-scope` is checked out and up-to-date with `main` post-alpha.68 release. Confirm HEAD is the plan-phase commit: `git log -1 --oneline` should show `7edd4cc plan(220): project-discovery scope — plan + research + data-model + contracts + quickstart`.
- [ ] T002 Capture the alpha.68 SC-005 baseline: build release binary at HEAD (`cargo +stable build -p waybill --release`), then run against an existing m215 split fixture: `./target/release/waybill sbom scan --path waybill-cli/tests/fixtures/split_heterogeneous --format cyclonedx-json --output /tmp/m220_baseline.cdx.json`. Snapshot component count via `jq '.components | length' /tmp/m220_baseline.cdx.json`. This is the reference set that default-mode m220 output MUST reproduce byte-identically.
- [ ] T003 [P] Read `waybill-cli/src/generate/split.rs:96-160` (`enumerate_workspace_roots` + `SubprojectRoot` struct + `is_main_module` helper). This is the discovered-main-module surface m220 filters.
- [ ] T004 [P] Read `waybill-cli/src/generate/split.rs:220-320` (`project_for_root` BFS). This is the BFS routine m220's filter reuses verbatim per contracts/scope-filter-algorithm.md Step 2.
- [ ] T005 [P] Read `waybill-cli/src/cli/scan_cmd.rs:3234-3300` (the existing `waybill:workspace-member` reader-aggregate that already inspects the annotation). Confirm the annotation values are populated by cargo/npm/go/maven readers per contracts/workspace-member-preservation.md ecosystem-detection matrix.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Type surface + CLI rewrite + parity extractors. Nothing user-facing yet — this is the substrate US1/US2/US3 all depend on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [ ] T006 Create `waybill-cli/src/generate/project_discovery/` module dir with three files: `mod.rs` (`ProjectDiscoveryMode` enum per data-model E1 + `is_root_in_scope` per E2 + `follows_workspace_members` per E3 + `Display` per E4 + `ProjectDiscoveryReport` struct per E5), `filter.rs` (empty stub — populated by T009), `report.rs` (helper for the report's INFO-log formatting, optional). Register the new module in `waybill-cli/src/generate/mod.rs` via `pub mod project_discovery;`.
- [ ] T007 [P] Add `pub project_discovery_mode: Option<crate::generate::project_discovery::ProjectDiscoveryMode>` field to `ScanArtifacts` in `waybill-cli/src/generate/mod.rs` per data-model E8. Default at every construction site: `None`. Grep for `ScanArtifacts {` construction sites (7+ per m218/m219 precedent) and add the new field to each. Update `ScanArtifacts::narrow` to copy the field through.
- [ ] T008 In `waybill-cli/src/cli/scan_cmd.rs`, add the `--project-discovery` clap arg per data-model E7 + contracts/project-discovery-flag.md. Type: `pub project_discovery: crate::generate::project_discovery::ProjectDiscoveryMode`. Clap attributes: `long = "project-discovery"`, `value_enum`, `default_value = "all"`, `require_equals = true`. Update `Default for ScanArgs` impl to set `project_discovery: ProjectDiscoveryMode::All`. Also add the env-var bridge at the same site scan_cmd bridges other flags (mirror the m218 `WAYBILL_EXPERIMENTAL_CROSS_ECOSYSTEM_EDGES` pattern): when `args.project_discovery != ProjectDiscoveryMode::All`, set `std::env::set_var("WAYBILL_PROJECT_DISCOVERY", args.project_discovery.to_string())`.
- [ ] T009 In `waybill-cli/src/generate/project_discovery/filter.rs`, implement `pub fn apply_scope_filter(...)` per contracts/scope-filter-algorithm.md (5 steps: fast-path All, enumerate + filter main-modules, BFS-project each root via m215 `project_for_root`, workspace-member annotation follow-up under RootOnly, filter component + relationship slices, build report). Add 5 unit tests: (1) All mode is zero-op; (2) RootOnly on a synthetic 2-root fixture drops nested; (3) RootOnly preserves workspace-member components; (4) Strict drops workspace members; (5) report counters correct.
- [ ] T010 [P] Add C140 parity extractors per contracts/project-discovery-annotation.md. Three files touched: `waybill-cli/src/parity/extractors/cdx.rs` (add `cdx_anno!(c140_cdx, "waybill:project-discovery-mode", document);` at end); `spdx2.rs` (add `spdx23_anno!(c140_spdx23, ..., document);`); `spdx3.rs` (add `spdx3_anno!(c140_spdx3, ..., document);`). Then `parity/extractors/mod.rs`: add `c140_cdx`, `c140_spdx23`, `c140_spdx3` to the three use-list blocks + add a new `ParityExtractor` row for C140 after C139 (m218's last row).
- [ ] T011 [P] Add a docs C140 row to `docs/reference/sbom-format-mapping.md` following the m217 C136 + m218 C137/C138/C139 doc-scope KEEP-NO-NATIVE template. Content per contracts/project-discovery-annotation.md standards-native audit (4 rejected alternatives). **⚠️ COUPLED WITH T010**: T011 (docs row) MUST commit together with (or before) T010 (extractor registration) to keep the `every_catalog_row_has_an_extractor` bidirectional test green at every commit. Same trip that caught m216/m217/m218/m219 pre-PR gates.
- [ ] T012 In `waybill-cli/src/scan_fs/mod.rs`, wire up the filter integration. Read the mode from env var `WAYBILL_PROJECT_DISCOVERY` (falls back to `ProjectDiscoveryMode::All` on parse failure). After `enumerate_workspace_roots` populates the resolved-component + relationship set, call `apply_scope_filter(components, relationships, mode, &scan_root)` per contracts/scope-filter-algorithm.md. Thread the returned `ProjectDiscoveryReport.mode` (Some(mode) when non-default) into `ScanArtifacts.project_discovery_mode`. Emit FR-012 INFO log per contracts (`tracing::info!(mode = %report.mode, root_main_modules = report.root_main_modules, workspace_members_followed = report.workspace_members_followed, nested_projects_ignored = report.nested_projects_ignored, "scan: project-discovery mode complete")`) when mode is non-default.

**Checkpoint**: Foundation ready. `ProjectDiscoveryMode` + `ProjectDiscoveryReport` + `apply_scope_filter` + CLI flag + C140 extractors all exist. The filter runs but has no US1/US2 test coverage yet.

---

## Phase 3: User Story 1 - Shallow scan of polyglot repos (Priority: P1)

**Goal**: When `--project-discovery=root-only` is passed on a polyglot fixture with root `Cargo.toml` + nested `services/api/package.json` + `services/worker/go.mod`, the emitted SBOM contains ONLY the root cargo main-module + its cargo transitive deps. No `pkg:npm/*` or `pkg:golang/*` components. Delivers SC-001 + SC-002.

**Independent Test**: Scan T013 fixture WITH `--project-discovery=root-only`. Assert `jq '.components[] | .purl' | grep -cE "^pkg:(npm|golang)"` returns `0`. Assert `jq '[.components[] | .purl | startswith("pkg:cargo")] | length'` returns the expected cargo-component count.

### Fixture + integration test for User Story 1

- [ ] T013 [US1] Author the polyglot-nested-independent fixture at `waybill-cli/tests/fixtures/project_discovery/polyglot_nested_independent/`. Structure per research R10:
  - Root: `Cargo.toml` (`[package] name = "p220-root" version = "0.1.0"` + `serde` dep), `Cargo.lock` (must be internally consistent with the Cargo.toml — copy-shape from m219 T017 fixture).
  - Nested: `services/api/{package.json, package-lock.json}` (npm project with `lodash` dep — copy-shape from m219 T017).
  - Nested: `services/worker/{go.mod, go.sum}` (Go module with `github.com/google/uuid` dep — copy-shape from m219 T017).
  - **Fixture-state verification** (per m219 C1-remediation lesson): before authoring tests, smoke-test that the fixture produces the intended state: `./target/release/waybill sbom scan --path <fixture> --format cyclonedx-json | jq '[.components[] | select(.properties[]?.name == "waybill:component-role" and .properties[]?.value == "main-module")] | length'` MUST return `3`.
- [ ] T014 [US1] Create `waybill-cli/tests/project_discovery_scope.rs` with a `run_scan(fixture_path, mode: Option<&str>) -> (tempfile::TempDir, String)` helper following the m219 `split_modes.rs` pattern (fake-HOME isolation via env, `NO_COLOR=1`, `RUST_LOG=info`, capture stdout+stderr). Then add `#[test] fn us1_root_only_drops_nested_independent_projects()` — invoke `--project-discovery=root-only` on the T013 fixture; parse the emitted CDX; assert (a) `.components[]` contains ONLY `pkg:cargo/*` and possibly `pkg:generic/*` synthetic-root, NO `pkg:npm/*` OR `pkg:golang/*`; (b) count of `pkg:cargo/*` components ≥ 1 (root main-module + its deps). Delivers SC-001.
- [ ] T015 [US1] Add `#[test] fn us1_all_mode_default_includes_all_ecosystems()` — invoke default mode (no `--project-discovery` flag) on the same T013 fixture; assert emitted CDX has all 3 main-modules (`pkg:cargo/*`, `pkg:npm/*`, `pkg:golang/*`) + their transitive deps. This is the SC-002 assertion + a SC-005 sanity check (default mode = m215/m219 behavior).
- [ ] T016 [US1] Add `#[test] fn us1_root_only_emits_c140_doc_scope_annotation()` — invoke `--project-discovery=root-only` on T013 fixture; parse emitted CDX; assert `.metadata.properties[] | select(.name == "waybill:project-discovery-mode") | .value == "root-only"`. Then invoke default mode on the same fixture; assert NO C140 annotation present. Delivers SC-008.
- [ ] T017 [US1] Add `#[test] fn us1_info_log_carries_mode_and_counts()` — invoke `--project-discovery=root-only` on T013 fixture with `RUST_LOG=info + NO_COLOR=1`; capture combined stdout+stderr; assert substring `project-discovery mode complete mode=root-only` present. Assert `nested_projects_ignored=2` substring (T013 has 3 main-modules; 1 root + 2 nested). Delivers SC-009.

**Checkpoint**: US1 complete. Shallow-scan works; C140 annotation + FR-012 INFO log emitted.

---

## Phase 4: User Story 2 - Workspace-member preservation (Priority: P1)

**Goal**: Under `--project-discovery=root-only`, ecosystem-native workspace-declared members (Cargo `[workspace] members`, npm `workspaces`, go.work `use`, maven `<modules>`) are still walked. Only INDEPENDENT nested projects (not tagged as workspace members) are dropped. Delivers SC-003 + SC-004 + SC-012.

**Independent Test**: Scan T018 fixture (Cargo workspace with independent `bench/Gemfile` neighbor) WITH `--project-discovery=root-only`. Assert workspace-member components present + `pkg:gem/*` absent.

### Fixture + integration tests for User Story 2

- [ ] T018 [US2] Author the cargo-workspace-with-independent-neighbor fixture at `waybill-cli/tests/fixtures/project_discovery/cargo_workspace_with_independent_neighbor/`. Structure per research R10:
  - Root: `Cargo.toml` with `[workspace] members = ["crates/api", "crates/worker"]`, `Cargo.lock`.
  - Members: `crates/api/Cargo.toml` (`[package] name = "p220-api"` + `serde` dep), `crates/worker/Cargo.toml` (`[package] name = "p220-worker"` + `serde_json` dep). Both need `src/lib.rs` files with minimal `pub fn` for cargo to be happy.
  - Independent neighbor: `bench/Gemfile` (`gem "rack"`) + `bench/Gemfile.lock`.
  - **Fixture-state verification**: smoke-test that the workspace-member relationship is correctly stamped: `./target/release/waybill sbom scan --path <fixture> --format cyclonedx-json | jq '[.components[] | select(.properties[]?.name == "waybill:workspace-member")] | length'` MUST return `2` (both crate members annotated with the workspace-member marker).
- [ ] T019 [US2] Add `#[test] fn us2_root_only_preserves_workspace_members()` to `project_discovery_scope.rs`. Invoke `--project-discovery=root-only` on T018 fixture. Parse emitted CDX. Assert: (a) workspace-root main-module present (`p220-workspace-root` or synthetic), (b) both `pkg:cargo/p220-api` + `pkg:cargo/p220-worker` present with `waybill:workspace-member` annotations, (c) cargo transitive deps (`serde`, `serde_json`) present, (d) NO `pkg:gem/*` components. Delivers SC-003 + SC-004.
- [ ] T020 [US2] Add `#[test] fn us2_all_mode_includes_independent_gemfile()` — invoke default mode on T018 fixture; assert emitted CDX contains BOTH the cargo workspace stuff AND `pkg:gem/rack@*`. This is the SC-002 assertion for US2's fixture.
- [ ] T021 [US2] Add unit test `apply_scope_filter_workspace_member_annotation_pass_covers_orphan_members` to `waybill-cli/src/generate/project_discovery/filter.rs::tests`. Construct a synthetic `Vec<ResolvedComponent>` with: (a) a workspace-root main-module at scan-root; (b) a workspace-member component tagged `waybill:workspace-member = <root-purl>` that is NOT a `depends_on` target from the root (simulates the Cargo case where `[workspace] members` declaration doesn't create a dep edge). Assert: under `RootOnly`, the workspace-member component is IN filtered_components (belt-and-suspenders annotation pass caught it per contracts/scope-filter-algorithm.md Step 3). Under `Strict`, the same component is DROPPED (Step 3 skipped).
- [ ] T022 [US2] Add unit test `apply_scope_filter_nested_workspace_fixpoint_recursion` per contracts/workspace-member-preservation.md FR-005 recursion contract. Construct a synthetic scan with: (a) outer workspace root at scan-root with `waybill:component-role = main-module`; (b) an inner workspace root component tagged `waybill:workspace-member = <outer-root-purl>` AND ALSO tagged `waybill:component-role = main-module` (nested workspace); (c) an inner-workspace member component tagged `waybill:workspace-member = <inner-root-purl>`. Assert: under `RootOnly`, all three components present in filtered_components (fixpoint recursion added inner-root's PURL to `root_purls` in a second pass, which pulled in inner's members). Delivers SC-012.

**Checkpoint**: US2 complete. Workspace-member preservation works; nested-workspace recursion works.

---

## Phase 5: User Story 3 - Strict-atomic mode (Priority: P3)

**Goal**: `--project-discovery=strict` treats the workspace root as ONE atomic file — even declared workspace members are dropped. Delivers SC-006.

**Independent Test**: Scan T018 (Cargo workspace) fixture WITH `--project-discovery=strict`. Assert workspace-member components ABSENT from emitted SBOM (delta vs root-only which preserves them).

### Integration test for User Story 3

- [ ] T023 [US3] Add `#[test] fn us3_strict_drops_workspace_members()` to `project_discovery_scope.rs`. Invoke `--project-discovery=strict` on T018 fixture (same fixture as US2 for direct comparison). Parse emitted CDX. Assert: workspace-root main-module PRESENT + directly-declared root deps PRESENT + BOTH workspace-member crates ABSENT + `pkg:gem/*` ABSENT + `pkg:cargo/serde_json` (member's transitive dep) ABSENT. Delivers SC-006.
- [ ] T024 [US3] Add `#[test] fn us3_strict_c140_annotation_value_is_strict()` — invoke `--project-discovery=strict` on any fixture; parse CDX; assert C140 annotation value == `"strict"` (not `"root-only"`). Confirms the Display impl renders variant-specific lowercase kebab-case correctly under Strict.
- [ ] T025 [US3] Add `#[test] fn us3_gemfile_only_ruby_app_root_only_strict_identical()` — since m216 Gemfile-only apps don't declare workspace members (Ruby has no workspace concept), `--project-discovery=root-only` and `--project-discovery=strict` MUST produce byte-identical output on a Gemfile-only fixture. Use `waybill-cli/tests/fixtures/gemfile_application/` (existing m216 fixture). Compare the two mode outputs; assert byte-identical modulo timestamps + serials.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: SC-007 invalid-mode error test, SC-011 --split composition test, docs, README link, m215 test-suite SC-005 verify, pre-PR gate, PR.

- [ ] T026 Add `#[test] fn invalid_mode_value_fails_cli_parse()` to `project_discovery_scope.rs`. Invoke waybill with `--project-discovery=nonexistent-mode --output <tempfile>`. Assert exit status non-zero. Assert stderr contains the string `nonexistent-mode` AND ALL THREE accepted values (`all`, `root-only`, `strict`). Assert NO output file created. Delivers SC-007.
- [ ] T027 Add `#[test] fn compose_with_split_directory_yields_single_sbom()` to `project_discovery_scope.rs`. On T013 fixture invoke `--project-discovery=root-only --split=directory --output-dir <tempdir>`. Assert `ls <tempdir>/*.cdx.json | wc -l == 1` (scope filter drops nested projects, split-directory sees only 1 root main-module → 1 group → 1 sub-SBOM). Delivers SC-011.
- [ ] T028 [P] Author `docs/reference/project-discovery.md` per FR-013 + research R11 (6 sections, ~150-200 lines): mode table with when-to-choose; interaction matrix vs `--split[=<mode>]` (R9 table); per-ecosystem workspace-member detection rules (R2 matrix); worked examples per mode; C140 doc-scope annotation contract; extensibility contract for future modes matching m219 pattern. Cross-link from `docs/reference/sbom-scopes.md` "SBOM interpretation" subsection (PR-#639 established this as the canonical docs location for feature refs) + from `docs/index.md` top-level reference list.
- [ ] T029 [P] Update `docs/reference/sbom-scopes.md` "SBOM interpretation" subsection to add a bullet linking `docs/reference/project-discovery.md`. Pattern-match the m218 `cross-ecosystem-edges.md` + m219 `split-modes.md` bullets already in that section (post PR-#639 merge).
- [ ] T030 SC-005 verification against the full m215 test suite + goldens: run `cargo +stable test -p waybill --test split_manifest_schema --test split_modes` — every m215 + m219 split test MUST pass unchanged. Run `cargo +stable test -p waybill --test cdx_regression --test spdx_regression --test spdx3_regression` — every m215-era regression MUST pass unchanged with zero golden regeneration. If any test fails or a golden diff appears, investigate — the default `--project-discovery=all` fast-path must be truly zero-op.
- [ ] T031 Pre-PR gate per Constitution: `./scripts/pre-pr.sh` — clippy `-D warnings` + `cargo test --workspace` (every suite `ok. N passed; 0 failed`). Watch for the pre-existing podman env-var race per `reference_podman_test_flake.md` memory. Read `feedback_prepr_gate_bails_on_first_failure.md` before treating any failure as a flake.
- [ ] T032 m214 grep gate: `BADHITS=$(grep -rE '\bmikebom\b' waybill-cli/src waybill-common/src waybill-ebpf/src xtask/src Cargo.toml waybill-cli/Cargo.toml waybill-common/Cargo.toml waybill-ebpf/Cargo.toml xtask/Cargo.toml Dockerfile.ebpf-test scripts 2>/dev/null | grep -v '^Binary file' | grep -vE 'mikebom-test-fixtures' || true)`; expects zero output.
- [ ] T033 Push branch: `git push origin 220-project-discovery-scope`.
- [ ] T034 Open PR against `main` titled `impl(220): --project-discovery=<mode> flag capping main-module discovery scope`. PR body includes: (a) summary + link to spec/plan/design-frame; (b) Test Plan enumerating every US1/US2/US3 integration test + unit tests + composition test + pre-PR gate + SC-005 m215-suite verify + m214 grep gate; (c) Migration/backward-compat note (default is `all`; SC-005 byte-identity preserved; existing consumers see zero change); (d) Docs link to `docs/reference/project-discovery.md`; (e) C140 doc-scope annotation added — parity-catalog row registered.
- [ ] T035 CI-side verification: all 20 CI checks (linux-x86_64 default + ebpf-tracing, macOS, Windows, Kusari Inspector, 15 rootfs/language scanners) MUST pass. Merge blocked until all green. Watch for the pre-existing podman env-var race documented in `reference_podman_test_flake.md`; rerun failed CI job once before treating as a real regression.

---

## Dependency Graph

- **Phase 1** (T001-T005) — T001-T002 sequential; T003-T005 parallel (all read-only surveys).
- **Phase 2** (T006-T012) — T006 first (creates the module); T007 || T008 (different files); T009 depends on T006; T010 || T011 (parity extractors + docs; **⚠️ MUST commit together per bidirectional catalog test invariant**); T012 depends on T009 (needs apply_scope_filter available).
- **Phase 3 US1** (T013-T017) — depends on Phase 2 complete. T013 (fixture) first; T014-T017 tests depend on T013. T014 || T015 || T016 || T017 (independent tests, same file — sequential commits but concurrent authoring).
- **Phase 4 US2** (T018-T022) — depends on Phase 2 complete. T018 (fixture) first; T019 || T020 depend on T018; T021 || T022 are unit tests in split.rs and depend on T009.
- **Phase 5 US3** (T023-T025) — T023 depends on T018 (reuses that fixture); T024 depends on Phase 3 (any fixture works); T025 uses pre-existing m216 gemfile_application fixture.
- **Phase 6 Polish** (T026-T035) — T026 || T027 independent tests; T028 || T029 docs; T030 depends on all impl phases; T031 depends on T030; T032-T035 sequential.

## Parallel Execution Examples

- **After T002**: T003 || T004 || T005 (three read-only file surveys).
- **After T006**: T007 || T008 (different files).
- **T010+T011 must commit together** per bidirectional catalog test invariant (analyze-phase lesson from m216/m217/m218/m219). Both parallel-authorable, then bundled into one commit.
- **After T018**: T019 || T020 (different tests, same fixture).
- **T021+T022**: parallel-authorable in split.rs unit-tests block; sequential commits.
- **T028 || T029**: docs page + section link.

## Implementation Strategy

**MVP scope (US1 + US2 both P1)**: Ship Phases 1+2+3+4+part-of-6 (T001-T022 + T026-T035, skip US3/T023-T025). Delivers the shallow-scan correctness fix + workspace-member preservation. **32 tasks.**

**Recommended scope (US1 + US2 + US3)**: Ship all 35 tasks. Adds Strict-mode coverage — the third variant costs 3 tests and gives operators the literal-shallow semantic + a natural third enum variant. **35 tasks total.**

## Format Validation

All 35 tasks follow the checklist format (`- [ ] TID [P?] [Story?] Description with file path`). Story labels present on all Phase 3-5 tasks (US1/US2/US3); absent on Phase 1/2/6 tasks per convention. File paths absolute-repository-relative throughout. Parallel markers `[P]` applied where independence is genuine.
