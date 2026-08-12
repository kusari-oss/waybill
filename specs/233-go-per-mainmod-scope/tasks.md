---

description: "Task list for feature 233-go-per-mainmod-scope: fix Go graph resolver so each main-module's dependsOn edges reflect only its own go.mod + go.sum"
---

# Tasks: Go graph resolver — per-main-module `dependsOn` scoping

**Input**: Design documents from `/specs/233-go-per-mainmod-scope/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/go-per-mainmod-edges.md`, `quickstart.md`

**Tests**: Included. Colocated unit tests (`graph_resolver.rs::tests`, `legacy.rs::tests`) + one integration test file with per-mode assertions + three synthetic fixtures covering the reporter's minimal repro shape, shared-version-member union, and mixed-Go-version stdlib.

**Organization**: US1 (P1) MVP-ships the fix; US2 (P2) verifies the workspace-member union invariant; FR-008 stands alongside as an independent code path (stdlib per Go version).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different files, no cross-task dependencies
- **[Story]**: `[US1]` / `[US2]` — maps back to spec.md user stories. FR-008 tasks are labeled `[US1]` since they ship in the same PR as US1's MVP.
- File paths absolute or repo-relative; every task cites exact file

---

## Phase 1: Setup

- [ ] T001 Verify feature branch is `233-go-per-mainmod-scope` (per `git branch --show-current`) and that `cargo +stable check -p waybill --lib` exits 0 against the untouched tree — locks in the pre-change baseline.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Extend `ModuleGraphEntry` with per-project provenance tracking and expose the `_for(project_root)` filtered accessor. Same-file edits; sequential.

- [ ] T002 In `waybill-cli/src/scan_fs/package_db/golang/graph_resolver.rs`, add a `discovering_project_roots: std::collections::HashSet<PathBuf>` field to `ModuleGraphEntry` (locate via `grep -n "pub struct ModuleGraphEntry"`). Default to empty set; existing constructors initialize with a single-element set containing the project_root at their insertion site (T003 threads the value through). Doc comment cites `contracts/go-per-mainmod-edges.md § Invariant 1` and the reporter's ticket.
- [ ] T003 In the same file, thread `project_root: &Path` through every ModuleGraphMap insertion path. Grep target: every `.insert(` call site touching the inner HashMap. Sites include: (a) `parse_go_mod` output ingestion; (b) `parse_go_sum` output ingestion; (c) `go mod graph` subprocess output; (d) cache-probe hits; (e) proxy fetch results; (f) `gosum_fallback` step. At each site, populate `discovering_project_roots` with the project_root that triggered the insertion. When the same `ModuleId` is inserted from multiple project_roots, the field's HashSet accumulates via `.insert()` (natural union).
- [ ] T004 In the same file, add a new public method `pub fn gosum_fallback_paths_for(&self, project_root: &Path) -> Vec<String>`. Returns only the entries whose `source == GoSumFallback` AND whose `discovering_project_roots.contains(project_root)`. Existing `gosum_fallback_paths()` remains for now (deprecation is a follow-up; T010 changes the caller). Doc comment cites FR-001 + Clarifications §1.

**Checkpoint**: `ModuleGraphMap` compiles; existing tests continue passing (T002-T004 are additive — no existing behavior changes yet). Verify with `cargo +stable test -p waybill --bin waybill -- scan_fs::package_db::golang::graph_resolver`.

---

## Phase 3: User Story 1 — Per-main-module truthful edges (Priority: P1) 🎯 MVP

**Goal**: Each Go main-module's `dependsOn` list reflects only what that module's own `go.mod` + `go.sum` declare. No cross-module bleed. Sibling main-modules only appear via `replace` directives.

**Independent Test**: Scan the 4-module fixture with `--project-discovery=all`; assert each main-module's `x/text` dep matches its own declared version (root→v0.40, hack→v0.37, tools→v0.29, deepthing→v0.25) and no main-module points at any other main-module.

### Fixture

- [ ] T005 [P] [US1] Create synthetic fixture at `waybill-cli/tests/fixtures/golden_inputs/golang/per_mainmod_scope_4modules/`. Files: root `go.mod` + `go.sum` at `.` requiring `example.com/mikebomfixture/text v0.40.0`; nested modules at `hack/`, `tools/`, `deep/src/thing/` each with their own `go.mod` + `go.sum` at v0.37.0/v0.29.0/v0.25.0 respectively. Add `main.go` at root importing `example.com/mikebomfixture/text/language`. Synthetic names per memory `feedback_fixture_synthetic_package_names`.

### Unit tests for US1

- [ ] T006 [P] [US1] Add unit test `gosum_fallback_paths_for_scopes_to_project_root` in `graph_resolver.rs::tests`. Fixture: build a `ModuleGraphMap` with two entries; entry A has `discovering_project_roots = {"/tmp/root/hack"}`; entry B has `{"/tmp/root/tools"}`. Assert `gosum_fallback_paths_for("/tmp/root/hack")` returns `[A]` only. FR-001 unit assertion.
- [ ] T007 [P] [US1] Add unit test `gosum_fallback_paths_for_returns_shared_entries` in same block. Fixture: single entry C with `discovering_project_roots = {"/tmp/root/a", "/tmp/root/b"}`. Assert `gosum_fallback_paths_for("/tmp/root/a")` and `..._for("/tmp/root/b")` both return `[C]`. FR-004 shared-version unit assertion.
- [ ] T008 [P] [US1] Add unit test `replace_directive_to_sibling_produces_main_module_edge` in `legacy.rs::tests`. Fixture: parsed `GoModDocument` with `require some.example.com/B v0.0.0` + `replace some.example.com/B => ../local/B`; a second parsed_root for `../local/B` module `some.example.com/B`. Call the modified `build_main_module_entry` (post-T011); assert `main_entry.depends` contains `"some.example.com/B"` (the sibling's module name). FR-002 + Clarifications §2 unit assertion.

### Implementation for US1

- [ ] T009 [US1] In `waybill-cli/src/scan_fs/package_db/golang/legacy.rs` at ~line 1893 (the existing `let fallback_paths = graph_map.gosum_fallback_paths();` in the `build_main_module_entry` augmentation), change the call to `graph_map.gosum_fallback_paths_for(project_root)`. This is the single-line fix that closes FR-001 for the shipping code path. Preserve the surrounding dedup + push logic.
- [ ] T010 [US1] In the same file, add a `replace`-directive → sibling-main-module edge pass. Placement: after the T009 augmentation, before the existing tool-directive block at ~line 1924. Logic: for each `replace old_path => new_path` in the current `doc`'s replace list where `new_path` is a filesystem path (contains `/` or `.`), canonicalize `new_path` relative to `project_root`; check if the canonical path matches any other entry's project_root in the outer `parsed_roots` loop's collection; if yes, push that sibling's `module_path` (from its own parsed `doc.module_path`) into `main_entry.depends`. Dedup against existing `main_entry.depends` via HashSet.
- [ ] T011 [US1] Create integration test `waybill-cli/tests/go_per_mainmod_scope.rs`. Reuse the `common::bin` + `apply_fake_home_env` scaffold from `waybill-cli/tests/nuget_main_module_parity.rs` verbatim. Add helper `run_scan(fixture, project_discovery_mode)` returning parsed CDX + stderr.
- [ ] T012 [US1] In the integration-test file, add `per_mainmod_dep_matches_own_gomod_all_mode` — spawn scan against the 4-module fixture with `--project-discovery=all`; jq the `dependencies[]` for each main-module (`example.com/root`, `example.com/hack`, `example.com/tools`, `example.com/deepthing`); assert each main-module's `dependsOn` list contains exactly its own declared `mikebomfixture/text` version and no other. **Additionally assert the emitted `components[]` contains exactly 4 distinct `pkg:golang/example.com/mikebomfixture/text@<V>` entries — one per declared version (FR-003 explicit assertion; closes analyze-report C3 gap).** SC-001 assertion.
- [ ] T013 [US1] Add integration test `per_mainmod_root_only_drops_nested_versions` — spawn scan with `--project-discovery=root-only`; assert `pkg:golang/example.com/mikebomfixture/text@v0.25.0` (and v0.37.0 and v0.29.0) do NOT appear in the emitted `components[]`; assert `pkg:golang/example.com/mikebomfixture/text@v0.40.0` does. SC-002 assertion.
- [ ] T014 [US1] Add integration test `no_main_module_depends_on_other_main_module` — spawn scan with `--project-discovery=all`; extract every main-module `bom-ref` (component with `waybill:component-role: "main-module"` property); assert no main-module's `dependsOn` list contains any OTHER main-module's `bom-ref`. SC-005 assertion. Use the 4-module fixture (no `replace` directives, so cross-main-module edges MUST be zero).
- [ ] T014b [US1] Add integration test `mode_invariance_root_only_vs_all` in the same file. Scan the 4-module fixture TWICE — once with `--project-discovery=all`, once with `--project-discovery=root-only`. Extract the root main-module (`pkg:golang/example.com/root@...`) from each output; extract its `dependsOn` list. Assert the two edge sets are IDENTICAL when compared as sets (order-insensitive). Uses the same `run_scan` helper from T011. **FR-005 explicit assertion** (closes analyze-report C1 gap: without this, a future refactor could introduce mode-dependent edge shape and the existing per-mode tests wouldn't catch it).
- [ ] T014c [US1] Add integration test `graph_completeness_no_leak_orphans` in the same file. Scan the 4-module fixture with `--project-discovery=all`; extract the document-scope `waybill:graph-completeness-reason` annotation from `metadata.properties`. Assert the value does NOT contain any orphan-reason token attributable to the leak (concretely: assert `orphaned-components-detected` either does not appear OR appears with count 0). Pre-233 baseline (implementer-verified on the same fixture): the classifier reports orphaned components because root's declared v0.40.0 has no incoming edge while the mis-attributed v0.25.0 does. Post-233: the mis-attribution is gone, root's v0.40.0 gains its incoming edge from root's main-module, and the classifier no longer flags this orphan class. **FR-007 automated assertion** (closes analyze-report C2 gap: SC-006 is manual-only via T023 Grafana rerun; this task adds CI coverage against the synthetic fixture).

**Checkpoint**: Run `cargo +stable test -p waybill --test go_per_mainmod_scope` and `cargo +stable test -p waybill --bin waybill -- scan_fs::package_db::golang`. All new + all pre-existing Go tests green.

---

## Phase 4: FR-008 — Per-Go-version stdlib

**Goal**: Emit one `pkg:golang/stdlib@<version>` per distinct `go <version>` directive declared across the scan. Each main-module's `dependsOn` points at the stdlib matching its own Go version.

### Fixture + tests

- [ ] T015 [P] [US1] Create synthetic fixture at `waybill-cli/tests/fixtures/golden_inputs/golang/per_mainmod_scope_mixed_go/`. Two modules: root `go.mod` declaring `go 1.24.0`, nested `legacy/go.mod` declaring `go 1.22.5`. Both minimal (no external requires beyond stdlib).
- [ ] T016 [P] [US1] Add unit test `stdlib_component_emitted_per_distinct_go_version` in `legacy.rs::tests`. Fixture: two parsed `GoModDocument`s with different `go_version` values; call the modified emission (post-T017); assert two distinct `pkg:golang/stdlib@<version>` entries appear in the returned components. FR-008 unit assertion.

### Implementation

- [ ] T017 [US1] In `legacy.rs` at ~line 2304 (`e.depends.push("stdlib".to_string())`), change to push a version-qualified name: `e.depends.push(format!("stdlib@{}", go_version))` where `go_version` is the current module's parsed `doc.go_version` (defaulting to a documented sentinel like `"unknown"` when absent). Additionally, in the component emission path (grep the file for where the stdlib `PackageDbEntry` gets created), emit one component per distinct Go version discovered across the scan, with PURL `pkg:golang/stdlib@<version>`. Chain-of-caller edits: T017 may require a helper that collects the union of `go_version` values across `parsed_roots` in the outer loop.
- [ ] T018 [US1] Add integration test `mixed_go_versions_emit_distinct_stdlib_components` in `go_per_mainmod_scope.rs`. Spawn scan against the mixed-Go fixture with `--project-discovery=all`; assert both `pkg:golang/stdlib@v1.24.0` and `pkg:golang/stdlib@v1.22.5` appear in `components[]`; assert root main-module `dependsOn` contains `stdlib@v1.24.0` and NOT `stdlib@v1.22.5`; assert legacy main-module `dependsOn` contains `stdlib@v1.22.5` and NOT `stdlib@v1.24.0`. FR-008 integration assertion.

---

## Phase 5: User Story 2 — Workspace-member union on shared components (Priority: P2)

**Goal**: When two main-modules require the same package + version, the emitted component's `waybill:workspace-member` annotation is a sorted deduplicated union of the two directories. Per data-model.md, this should be a free consequence of existing m176 code — this phase adds verification.

### Fixture + test

- [ ] T019 [P] [US2] Create synthetic fixture at `waybill-cli/tests/fixtures/golden_inputs/golang/per_mainmod_scope_shared_ver/`. Root `go.mod` with no `require`s. Two nested modules `hack/` and `tools/`, each with `go.mod` requiring `example.com/mikebomfixture/text v0.29.0` and matching `go.sum` entries.
- [ ] T020 [US2] Add integration test `shared_version_workspace_member_union` in `go_per_mainmod_scope.rs`. Spawn scan against the shared-version fixture; jq the `pkg:golang/example.com/mikebomfixture/text@v0.29.0` component; assert its `waybill:workspace-member` property value is exactly `["hack","tools"]` (JSON string with sorted union — verify shape via `.properties[] | select(.name == "waybill:workspace-member") | .value`). FR-004 integration assertion.

**Checkpoint**: If T020 fails, investigate whether m176's `tag_components_with_workspace_member` (`scan_fs/mod.rs:1290`) is correctly producing the union. Fix may be a one-liner in that pass OR a bug in how `evidence.source_file_paths` accumulates for shared components upstream.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T021 [P] Update `docs/reference/component-tiers.md` (or a new `docs/reference/go-modules.md` if none exists) with a short note documenting the per-main-module edge invariant + the per-Go-version stdlib rule for future contributors. Cross-link to `contracts/go-per-mainmod-edges.md`.
- [ ] T022 Run `./scripts/pre-pr.sh` locally. Expect green. Per memory `feedback_prepr_gate_bails_on_first_failure`, if it fails, enumerate every `^---- .+ stdout ----` line before triaging.
- [ ] T023 **Manual verification (SC-003 + SC-004)**: With a local clone of `github.com/grafana/grafana`, run the milestone-233 binary against it in offline mode. Record the exact `x/text` and `klauspost/compress` version sets in the root unit's SBOM. Include the delta (pre-233: e.g., `[v0.37.0, v0.40.0]`; post-233: `[v0.40.0]` only) in the PR body. Note: this scan takes ~15 min per unit; only the ROOT unit needs re-verification for this milestone.
- [ ] T024 Walk through `specs/233-go-per-mainmod-scope/quickstart.md` end-to-end against a fresh `cargo build --release -p waybill` binary. Confirm every SC-001..SC-005 + FR-004 + FR-008 assertion returns the expected shape.

---

## Dependencies & Execution Order

- **Phase 1**: T001 no dependencies.
- **Phase 2**: T002 → T003 → T004 same file; sequential.
- **Phase 3 (US1)**: Requires Phase 2. Fixture T005 parallel with unit tests T006–T008. Impl T009 → T010 sequential (both touch `legacy.rs` at close call sites; T010 depends on T009's `_for` call being in place). Integration tests T011 → T012 → T013 → T014 sequential in same file.
- **Phase 4 (FR-008)**: Fixture T015 + unit test T016 parallel with US1's phase 3 work; impl T017 must land after T009 (both edit `legacy.rs`). Integration test T018 requires T017.
- **Phase 5 (US2)**: Fixture T019 parallelizable with US1; test T020 requires the impl from Phase 3 to be in place (since the shared-version output shape depends on the resolver fix).
- **Phase 6**: Requires all prior phases. T021 + T022 largely independent. T023 + T024 sequential.

### Parallel Opportunities

- Fixtures T005, T015, T019 — different directories; parallelizable at authoring time.
- Unit tests T006, T007, T008, T016 — different test-function names in different `mod tests` blocks.
- Integration tests T012, T013, T014, T018, T020 — different `#[test] fn` names in the same file (`go_per_mainmod_scope.rs`); parallelizable at authoring time, sequential at merge time.

---

## Implementation Strategy

### MVP: US1 + FR-008 only

1. Setup (T001).
2. Foundational (T002 → T003 → T004).
3. US1 fixture + unit tests + impl + integration tests (T005–T014).
4. FR-008 fixture + unit + impl + integration (T015–T018).
5. Ship as MVP.

US2's verification test (T019 + T020) piles on trivially since it's just a fixture + one assertion and the underlying behavior is expected free from m176. Bundle in the same PR.

### Not a parallel-team milestone

Single-contributor session — ~500 LOC net across `graph_resolver.rs` + `legacy.rs` + one integration test file + three fixtures. Estimated 4–6 hours end-to-end including Grafana verification.

---

## Notes

- Every task cites a concrete file path. Test tasks additionally name the `#[test] fn` block.
- Fixture module names use `example.com/mikebomfixture/*` synthetic prefix per memory `feedback_fixture_synthetic_package_names`. NEVER use real coordinates like `golang.org/x/text` — real coordinates trip Kusari Inspector's advisory scanner (bit us in PR #285 and #640).
- Env-var-mutating tests: none in this milestone. `GOWORK`, `GOMODCACHE`, etc. are consulted only by subprocess-invoked `go` in m173's cache warmer, which isn't touched here.
- Grafana verification (T023) is a one-shot manual step; not automated per m090 corpus policy. The synthetic-fixture assertions (T012, T013, T014, T018, T020) are the CI regression signal.
- No new Cargo dependencies; no `Cargo.toml` edits.
- No changes to `project_discovery/filter.rs` (verified during Phase 0 research — the leak is purely upstream).
- No changes to `scan_fs/mod.rs:526-547` edge-emission loop (existing `depends → PURL` lookup works unchanged with per-main-module scoped depends lists).
