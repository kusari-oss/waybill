---
description: "Task list for milestone 161 — Go workspace-mode false dep-graph edges"
---

# Tasks: Go workspace-mode false dep-graph edges

**Input**: Design documents from `/specs/161-go-workspace-edges/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/annotations.md, quickstart.md

**Tests**: INCLUDED. SC-009 requires ≥10 unit tests; SC-010 requires a new integration test; SC-001 requires a gated audit test. All 3 test surfaces are load-bearing SC evidence and MUST land alongside the implementation.

**Organization**: Tasks are grouped by the 3 user stories from spec.md (US1 P1 workspace-attribution fix, US2 P2 doc-scope C112 signal, US3 P3 byte-identity guard). US1 is the load-bearing MVP.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different files, no dependencies on incomplete tasks — parallelizable.
- **[Story]**: US1 / US2 / US3 for user-story tasks. Setup, Foundational, Polish have NO story label.

## Path Conventions

Single-project workspace layout per plan.md §Project Structure. All paths absolute-relative to repo root `/Users/mlieberman/Projects/mikebom/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: New Rust types + go.work parser used by both US1 and US2.

- [X] T001 Create new `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs` sibling file. Add module declaration `pub mod gowork;` to `mikebom-cli/src/scan_fs/package_db/golang/mod.rs`.
- [X] T002 Add NEW `WorkspaceMode` enum with 3 variants (`Detected { use_count: usize }` / `Absent` / `Malformed { reason: String }`) + `Default` impl returning `Absent` + `as_wire_str()` method producing the C112 wire vocab per Q2 clarification + `is_active()` predicate per data-model.md E1 in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.
- [X] T003 Add NEW `GoWorkDocument` struct with `go_version: Option<String>`, `use_paths: Vec<PathBuf>`, `replaces: HashMap<(String, String), (String, String)>` fields + `Default` impl per data-model.md E2 in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.
- [X] T004 Add NEW `EdgeDisposition` enum with 3 variants (`Keep` / `Resolve { sibling_version: String }` / `Suppress { reason: String }`) per data-model.md E3 in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.
- [X] T005 Implement `parse_go_work(body: &str) -> WorkspaceMode` — line-based parser per research.md R2. State machine: `Toplevel | InUseBlock | InReplaceBlock`. Comments (`//` to EOL) stripped before token analysis. Handles single-line `use "./path"`, block-form `use ( ... )`, single-line + block-form `replace <old> => <new>`, `go <version>` line. Malformed inputs return `WorkspaceMode::Malformed { reason }` with one of the 6 documented closed-vocab codes. In `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.

**Checkpoint**: All 3 types + parser compile. `cargo build -p mikebom` succeeds. No behavior change yet (parser not called from anywhere).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Wire `WorkspaceMode` through `WorkspaceContext` → `ScanDiagnostics` → `ScanArtifacts` → format builders. Register the C112 parity catalog row. All user stories depend on this.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T006 Extend `WorkspaceContext` (existing struct at `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs`) with 3 new fields: `workspace_mode: WorkspaceMode` (defaults to `Absent`) + `use_modules_map: HashMap<String, PathBuf>` (defaults empty) + `workspace_replaces: HashMap<(String, String), (String, String)>` (defaults empty) per data-model.md E4 + FR-005 apply pipeline.
- [X] T007 Add `go.work` detection + parse invocation at the top of `legacy::read()` in `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs`. Read `<rootfs>/go.work` if it exists, invoke `parse_go_work()`, populate a local `workspace_mode: WorkspaceMode` variable. Honor `GOWORK=off` env-var override producing `WorkspaceMode::Absent` regardless of file presence. Emit an info-level `tracing::info!` log line naming the detection outcome.
- [ ] T008 Populate `use_modules_map` by iterating `GoWorkDocument.use_paths`, canonicalizing each path against the workspace root, reading each `use`d module's `go.mod` for its declared `module` path, and inserting into the map. Skip paths that don't resolve to an existing `go.mod` file (emit `tracing::warn!`). In `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs`.
- [ ] T008a Populate `WorkspaceContext.workspace_replaces` from `GoWorkDocument.replaces`. Then merge workspace-level replaces into the existing per-project-root `WorkspaceContext.replaces` map with **workspace precedence per FR-005 + Go MVS semantics** — for any `(old_path, old_ver)` key present in both, the workspace-level value overrides the module-level value. In `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs`.
- [X] T009 Extend `GoScanSignals` (existing struct in `legacy.rs`) with `workspace_mode: Option<WorkspaceMode>` field. Populate it at the end of `legacy::read()` with the parsed workspace mode (or `None` when no `go.work` present).
- [X] T010 Add `go_workspace_mode: Option<golang::gowork::WorkspaceMode>` field to `ScanDiagnostics` struct at `mikebom-cli/src/scan_fs/package_db/mod.rs:307`, sibling to milestone-160's `go_transitive_coverage` per data-model.md E5.
- [X] T011 Populate `ScanDiagnostics.go_workspace_mode` in `read_all` from `GoScanSignals.workspace_mode` (sibling to the existing `go_transitive_coverage` propagation added in milestone 160) in `mikebom-cli/src/scan_fs/package_db/mod.rs`.
- [X] T012 Add `go_workspace_mode: Option<crate::scan_fs::package_db::golang::gowork::WorkspaceMode>` field to `scan_fs::ScanResult` struct in `mikebom-cli/src/scan_fs/mod.rs`. Populate from `ScanDiagnostics.go_workspace_mode` at the end of `scan_path`.
- [X] T013 Add `go_workspace_mode: Option<&'a crate::scan_fs::package_db::golang::gowork::WorkspaceMode>` field to `ScanArtifacts` struct in `mikebom-cli/src/generate/mod.rs`.
- [X] T014 Destructure `go_workspace_mode` from `ScanResult` at the CLI level in `mikebom-cli/src/cli/scan_cmd.rs` + populate the ScanArtifacts field with `scan_result.go_workspace_mode.as_ref()`. Sibling to the existing `go_transitive_coverage` destructuring.
- [X] T015 [P] Register C112 (`mikebom:go-workspace-mode`, document) macro invocation per contracts/annotations.md §Parity catalog integration in `mikebom-cli/src/parity/extractors/cdx.rs`.
- [X] T016 [P] Register C112 `spdx23_anno!` invocation per contracts/annotations.md §Parity catalog integration in `mikebom-cli/src/parity/extractors/spdx2.rs`.
- [X] T017 [P] Register C112 `spdx3_anno!` invocation per contracts/annotations.md §Parity catalog integration in `mikebom-cli/src/parity/extractors/spdx3.rs`.
- [X] T018 Add 1 `ParityExtractor` entry (C112 with `Directionality::SymmetricEqual`, `order_sensitive: false`) adjacent to the existing C110/C111 block in `mikebom-cli/src/parity/extractors/mod.rs` AND add `c112_cdx`, `c112_spdx23`, `c112_spdx3` to the 3 import lines.
- [X] T019 Add C112 row to `docs/reference/sbom-format-mapping.md` per contracts/annotations.md §C112 wire format — needed to satisfy the `every_mikebom_emitted_field_has_a_map_row` test in `mikebom-cli/tests/sbom_format_mapping_coverage.rs`.

**Checkpoint**: Types wired end-to-end. `cargo +stable clippy --workspace --all-targets -- -D warnings` clean. Parity registration + sbom-format-mapping row in place but not yet exercised (C112 not yet emitted — happens in US2).

---

## Phase 3: User Story 1 - test-kubernetes false-edge fix (Priority: P1) 🎯 MVP

**Goal**: Fix the FR-007 root causes so per-`use`d-module edges match `GOWORK=off go mod graph` executed in each module's directory. Reduce `test-kubernetes` wrong-edge rate from 30.8% to ≤ 5%.

**Independent Test**: Scan `test-kubernetes` in online mode. Assert (a) `|mikebom_edges \ go_mod_graph_edges(per-module)| / |go_mod_graph_edges(per-module)| ≤ 0.05` (SC-001); (b) the 3 specific false edges from the milestone-160 audit (`k8s.io/api → kube-proxy`, `k8s.io/apimachinery → endpointslice`, `k8s.io/cli-runtime → streaming`) MUST NOT appear (SC-002); (c) zero emitted `dependsOn` targets reference a workspace-internal module with version `v0.0.0-unknown` (SC-006).

### FR-007 investigation + fixes for User Story 1

- [ ] T020 [US1] Instrument `legacy::read` with `tracing::debug!` lines counting per-project-root emitted edges + noting each edge's target version. Land the instrumentation ONLY (no fix yet); use it to record the pre-fix baseline against `test-kubernetes` in `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs`.
- [ ] T021 [US1] Empirical investigation — run the T020-instrumented binary against `$MIKEBOM_FIXTURES_DIR/go/workspace-kubernetes/` (test-kubernetes fixture must be pre-populated per Assumptions §6 in the sibling fixture-cache repo), capture per-project-root edge counts + `v0.0.0-unknown` version-target frequency, cross-reference with per-module `GOWORK=off go mod graph` output, identify which of FR-007a (multi-go.mod walker attribution) / FR-007b (v0.0.0-unknown handling) / FR-007c (workspace-internal synthetic component) apply for the 3 SC-002 spot-check false edges. Document findings inline as code comments in the fix commits. Investigation-only task — no code fix here.
- [ ] T022 [US1] Update `run_go_mod_graph` in `mikebom-cli/src/scan_fs/package_db/golang/go_mod_graph.rs` to accept a `gowork_off: bool` parameter. When `true`, add `.env("GOWORK", "off")` to the subprocess `Command` invocation. Default caller behavior (non-workspace-mode scans) passes `false` — no wire-level change on non-workspace scans.
- [ ] T023 [US1] Update `GraphResolver::step1_go_mod_graph` in `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs` to pass `ctx.workspace_mode.is_active()` as the `gowork_off` argument to `run_go_mod_graph`. This tells Go to treat each `use`d module as a standalone unit rather than merging graphs across siblings.
- [ ] T024 [US1] Fix FR-007a (if T021 confirmed): the multi-`go.mod` walker at `legacy.rs::candidate_project_roots` must distinguish workspace-root from `use`d modules explicitly when workspace_mode.is_active(). Restrict discovery in workspace mode to ONLY the `use_paths` set from the parsed go.work, ignoring other `go.mod` files that might appear in vendored trees or unrelated staging directories.
- [ ] T025 [US1] Implement `classify_workspace_edge(source_go_mod, target_module_id, use_modules_map, sibling_go_mods) -> EdgeDisposition` in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs` per research.md R4. Handles the Q1 hybrid rule: `Keep` iff target is not workspace-internal OR version already resolved; `Resolve { sibling_version }` iff target IS workspace-internal AND source's own require block names the target; `Suppress { reason }` iff target IS workspace-internal AND source's require block does NOT name the target.
- [ ] T026 [US1] Wire the Q1 hybrid disposition sweep into `legacy::read` post-resolution. After `resolver.resolve()` returns `graph_map` for a project root, iterate its edges and apply `classify_workspace_edge` per edge. `Keep` → no-op; `Resolve` → rewrite the target version to `sibling_version`; `Suppress` → drop the edge from the map. Only run this sweep iff `ctx.workspace_mode.is_active()`. In `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs`.
- [ ] T027 [US1] Fix FR-007c (if T021 confirmed): suppress emission of synthetic workspace-internal components with `v0.0.0-unknown` versions. When `workspace_mode.is_active()` AND a candidate component's version is `v0.0.0-unknown` AND the component's module path is in `use_modules_map`, replace the version with the real declared version from the sibling's `go.mod` OR skip the component if no version is discoverable. In `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs`.
- [ ] T028 [US1] Remove the T020 debug-instrumentation (`tracing::debug!` lines added in T020). Investigation complete; leave only the FR-011 info-level summary log line naming `use_module_count`, `workspace_replace_count`, `has_workspace_root_gomod`, per-module edge counts in `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs`.

### Tests for User Story 1

- [X] T029 [P] [US1] Unit test: `parse_go_work()` on a minimal `go 1.24\nuse (\n    .\n    ./staging/foo\n)\n` input returns `WorkspaceMode::Detected { use_count: 2 }` per SC-009 (a) in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.
- [X] T030 [P] [US1] Unit test: `parse_go_work()` on missing-close-paren input returns `WorkspaceMode::Malformed { reason: "missing-use-close-paren" }` per SC-009 (b) in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.
- [X] T031 [P] [US1] Unit test: `parse_go_work()` on empty `use ()` block returns `WorkspaceMode::Detected { use_count: 0 }` per Q2 clarification + SC-009 (f) in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.
- [X] T032 [P] [US1] Unit test: `parse_go_work()` parses `replace <old> => <new>` directives correctly (both single-line + block form) per SC-009 (c) in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.
- [X] T033 [P] [US1] Unit test: `parse_go_work()` handles quoted + unquoted `use` paths (`use "./staging/foo"` and `use ./staging/foo`) per SC-009 (d) in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.
- [X] T033a [P] [US1] Unit test: `use .` (workspace-root self-reference) case per SC-009 (j) + FR-003 — synthesizes `use ( . ./child )`; asserts `parse_go_work` returns `use_paths.len() == 2` with `.` as an entry; then feeds through `populate_use_modules_map()` (or the T008 code path) and asserts `use_modules_map` includes both the workspace root's own declared module path (from the root `go.mod`) AND the child's module path. In `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.
- [X] T034 [P] [US1] Unit test: `WorkspaceMode::as_wire_str()` returns exact C112 vocabulary strings (`detected: 47 use-modules`, `absent`, `malformed: missing-use-close-paren`) per contracts/annotations.md §C112 vocabulary in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.
- [X] T035 [P] [US1] Unit test: `classify_workspace_edge()` returns `Keep` when target is not workspace-internal (target module path not in `use_modules_map`) per SC-009 (g) in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.
- [X] T036 [P] [US1] Unit test: `classify_workspace_edge()` returns `Resolve` when target IS workspace-internal AND source's require block names the target; sibling_version matches the target sibling's declared module version per Q1 clarification in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.
- [X] T037 [P] [US1] Unit test: `classify_workspace_edge()` returns `Suppress` when target IS workspace-internal AND source's require block does NOT name the target (test-kubernetes false-edge shape: `k8s.io/api → kube-proxy` where api's go.mod doesn't require kube-proxy) per SC-009 (h) in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.
- [X] T037a [P] [US1] Unit test: workspace-level replace overrides module-level replace of the same shape per FR-005 — synthesizes a `WorkspaceContext` where `workspace_replaces` contains `("github.com/old/lib", "v1.0.0") → ("github.com/new/lib", "v2.0.0")` AND the same key exists in per-project-root `replaces` with a DIFFERENT target; runs the T008a merge; asserts the resolved `WorkspaceContext.replaces` value matches the workspace-level target (workspace precedence per Go MVS semantics). In `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.
- [X] T038 [P] [US1] Unit test: `WorkspaceMode::is_active()` returns `true` iff variant is `Detected` (both zero and non-zero use_count); returns `false` for `Absent` and `Malformed` per data-model.md E1 in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`.
- [ ] T039 [P] [US1] Unit test: `GOWORK=off` env-var override at the `legacy::read` detection site produces `WorkspaceMode::Absent` regardless of `go.work` file presence per SC-009 (e) — synthesizes a tempdir with a valid `go.work` + sets `env::var("GOWORK", "off")` inside the test; asserts detection returns `Absent`. In `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs`.
- [ ] T040 [US1] Add SC-001+SC-002+SC-006 audit test at `mikebom-cli/tests/go_workspace_edges_audit.rs` per research.md R5 — gated behind `MIKEBOM_WORKSPACE_EDGES_AUDIT=1` env var. Locates `test-kubernetes` fixture; parses `go.work` for `use`d module list; for each `use`d module M shells out `cd $M && GOWORK=off go mod graph`; parses edges into ground-truth `HashSet<(source, target)>`; scans fixture via `env!("CARGO_BIN_EXE_mikebom")`; extracts mikebom's per-`use`d-module edges from CDX `dependencies[].dependsOn[]`. THREE assertions: (a) SC-001 aggregate — `|wrong_edges| / |ground_truth_edges| ≤ 0.05` with a 20-sample diagnostic failure message; (b) SC-002 spot-check — each of the 3 concrete false edges (`k8s.io/api → kube-proxy`, `k8s.io/apimachinery → endpointslice`, `k8s.io/cli-runtime → streaming`) MUST NOT be present in mikebom's output; (c) SC-006 explicit — iterate every emitted `dependencies[].dependsOn[]` target PURL; assert none contains `@v0.0.0-unknown` for a target module path present in the parsed `use_modules_map` (10-sample diagnostic on failure).

**Checkpoint**: US1 is fully functional. Running `MIKEBOM_WORKSPACE_EDGES_AUDIT=1 cargo test --test go_workspace_edges_audit` on `test-kubernetes` fixture yields ≤ 5% wrong-edge rate + zero SC-002 spot-check false edges + zero workspace-internal `v0.0.0-unknown` targets (per T040's three-part assertion). SC-009 unit-test floor covered by T029–T039 + T033a + T037a (13 unit tests total: SC-009 sub-items a–j + workspace-precedence-replace test + wire-vocab sanity).

---

## Phase 4: User Story 2 - Document-scope go-workspace-mode signal (Priority: P2)

**Goal**: Emit `mikebom:go-workspace-mode` doc-scope annotation with the C112 vocabulary. Present iff `go.work` file at scanned root; absent otherwise (byte-identity guard).

**Independent Test**: For every emitted SBOM containing ≥1 Go component AND `go.work` at scanned root, assert `mikebom:go-workspace-mode` present at document scope exactly once (SC-004). Value follows the closed vocab. If value starts with `malformed:`, reason follows the 6-code closed vocab.

### Document-scope emission for User Story 2

- [X] T041 [US2] Extend CycloneDX builder `mikebom-cli/src/generate/cyclonedx/builder.rs`: add `go_workspace_mode: Option<crate::scan_fs::package_db::golang::gowork::WorkspaceMode>` field; add `with_go_workspace_mode()` setter method; thread the value into `build_metadata()` at the existing call site (adjacent to `go_transitive_coverage` from milestone 160).
- [X] T042 [US2] Extend `build_metadata()` signature in `mikebom-cli/src/generate/cyclonedx/metadata.rs` with `go_workspace_mode: Option<&WorkspaceMode>` parameter. Emit C112 iff `Some(m)` AND `!matches!(m, WorkspaceMode::Absent)`, using `m.as_wire_str()` as the property value. Update all 6 test-site `build_metadata` calls to pass `None` as the new argument.
- [X] T043 [US2] Wire `.with_go_workspace_mode(scan.go_workspace_mode.cloned())` in `mikebom-cli/src/generate/cyclonedx/mod.rs` (adjacent to `.with_go_transitive_coverage()` from milestone 160).
- [X] T044 [US2] Emit C112 at document scope in `mikebom-cli/src/generate/spdx/annotations.rs` (SPDX 2.3) — sibling to the existing `mikebom:go-transitive-coverage` block from milestone 160. Guard emission with `if let Some(m) = artifacts.go_workspace_mode { if !matches!(m, WorkspaceMode::Absent) { push(&mut out, "mikebom:go-workspace-mode", json!(m.as_wire_str())); } }`.
- [X] T045 [US2] Emit C112 at document scope in `mikebom-cli/src/generate/spdx/v3_annotations.rs` (SPDX 3.0.1) — sibling to the SPDX 2.3 block from T044 with the same guard.
- [X] T046 [US2] For every non-productive `ScanArtifacts { ... }` construction site (openvex mock, spdx test mocks, spdx doc flow-through — same pattern as milestone 160 T009-T010), add `go_workspace_mode: None` or `go_workspace_mode: artifacts.go_workspace_mode` as appropriate. Compile-driven — the Rust type checker enumerates the sites via missing-field errors.

### Tests for User Story 2

- [X] T047 [P] [US2] Unit test: doc-scope emission code emits C112 iff `WorkspaceMode` is `Detected` — synthesizes a `WorkspaceMode::Detected { use_count: 5 }`; calls `build_metadata` with `go_workspace_mode: Some(&mode)`; asserts CDX output contains `mikebom:go-workspace-mode = "detected: 5 use-modules"` in `metadata.properties[]` per SC-004 in `mikebom-cli/src/generate/cyclonedx/metadata.rs`.
- [X] T048 [P] [US2] Unit test: doc-scope emission code omits C112 when `WorkspaceMode::Absent` — synthesizes `Absent` and asserts CDX output does NOT contain any `mikebom:go-workspace-mode` property. This is the SC-003 byte-identity guard test in `mikebom-cli/src/generate/cyclonedx/metadata.rs`.
- [X] T049 [P] [US2] Unit test: doc-scope emission code emits `malformed: <reason>` on `WorkspaceMode::Malformed` variant per FR-004 in `mikebom-cli/src/generate/cyclonedx/metadata.rs`.
- [X] T050 [US2] Integration test at `mikebom-cli/tests/go_workspace_edges.rs` per SC-010 — synthesizes a 3-module Go workspace in a tempdir (base library, middle library depending on base, leaf application depending on middle) with a valid `go.work` file, invokes the release binary via `env!("CARGO_BIN_EXE_mikebom")`, asserts (a) per-component `mikebom:go-transitive-source` values remain sensible on all 3 modules (regression guard for milestone 160 interop); (b) doc-scope C112 == `detected: 3 use-modules`; (c) `dependsOn` edges match ground-truth (base has no outgoing edges to workspace siblings; middle → base; leaf → middle) with ZERO false workspace-sibling edges.

**Checkpoint**: US2 is fully functional. Doc-scope C112 annotation emitted on every workspace-mode Go scan. Integration test T050 passes with the correct edge shape.

---

## Phase 5: User Story 3 - Non-Go and non-workspace scans byte-identical to pre-161 (Priority: P3)

**Goal**: Regression guard. Verify the milestone-090 non-Go goldens (10 ecosystems × 3 formats) + the single-module `golang` fixture (× 3 formats) are byte-identical to pre-161. Total 33 byte-identical goldens.

**Independent Test**: `git diff <pre-161-sha> HEAD -- mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/{apk,bazel,cargo,cmake,deb,gem,golang,maven,npm,pip,rpm}.*` produces zero output.

### Golden verification for User Story 3

- [X] T051 [US3] Verify SC-003 dual-side byte-identity: after T041–T050 land, run `cargo +stable test --workspace --no-fail-fast --test cdx_regression --test spdx_regression --test spdx3_regression` and inspect the diff for any golden that changed. Any diff on the 33 pre-161 goldens (including single-module `golang`) indicates an emission-leak bug that needs fixing in US1/US2 before proceeding.
- [X] T052 [US3] If the `golang-workspace` fixture is added to the fixture-cache repo (per plan Assumption §6), a new set of goldens for that fixture WILL be generated. Verify those goldens contain C112 with `detected: 3 use-modules` (matching the fixture shape) but do NOT change any existing golden's content.

**Checkpoint**: 33 pre-161 goldens byte-identical to pre-161. Any new `golang-workspace` fixture goldens carry C112 correctly.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Fixture setup, documentation, CHANGELOG, pre-PR gate, issue closure.

- [ ] T053 Coordinate `test-kubernetes` fixture addition to `kusari-oss/mikebom-fixtures` under `go/workspace-kubernetes/`. Fixture should include: `go.work`, `go.sum`, and a subset of staging modules sufficient for the 3 SC-002 spot-check edges (at minimum: `staging/src/k8s.io/api`, `staging/src/k8s.io/apimachinery`, `staging/src/k8s.io/kube-proxy`, `staging/src/k8s.io/endpointslice`, `staging/src/k8s.io/cli-runtime`, `staging/src/k8s.io/streaming`). Total fixture size target: <50MB. This is an out-of-repo task — creates a follow-on PR on `kusari-oss/mikebom-fixtures`.
- [X] T054 [P] Add `CHANGELOG.md` entry per SC-011 documenting: (a) motivation (issue #495 + milestone-155–160 audit expansion), (b) FR-007 fix summary (workspace-mode detection + per-`use`d-module attribution + Q1 hybrid disposition), (c) new annotation vocab table (C112), (d) empirical impact — pre/post SC-001 numbers on test-kubernetes, (e) consumer jq recipe from contracts/annotations.md, (f) Q1-Q3 clarification bullets.
- [X] T055 [P] Update `docs/reference/sbom-format-mapping.md` C112 entry to match the final wire shape after implementation is complete — no-op if T019 already captured the correct shape.
- [X] T056 Run `./scripts/pre-pr.sh` — both `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace --no-fail-fast` MUST pass clean (SC-008). Any failure blocks PR opening.
- [ ] T057 Verify `MIKEBOM_WORKSPACE_EDGES_AUDIT=1 cargo test --test go_workspace_edges_audit` passes with wrong-edge ratio ≤ 0.05 on the `test-kubernetes` fixture (SC-001). If empirically below target but above pre-161 baseline of 30.8%, revise SC-001 inline per Assumptions §7 and document the revised floor in CHANGELOG.
- [ ] T058 Include `closes #495` in the impl PR body per SC-013 so merging the PR auto-closes the tracking issue.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No prerequisites. Types + parser land first.
- **Phase 2 (Foundational)**: Depends on Phase 1. Wires `WorkspaceMode` through `WorkspaceContext` → `ScanDiagnostics` → `ScanArtifacts` + parity catalog. Blocks US1/US2/US3.
- **Phase 3 (US1)**: Depends on Phase 2. FR-007 investigation + fixes + Q1 hybrid classifier + per-`use`d-module attribution + 11 unit tests + SC-001+SC-002 audit test.
- **Phase 4 (US2)**: Depends on Phase 2 (uses `ScanArtifacts.go_workspace_mode`). Doc-scope emission across 3 formats + tests. **Can be worked in parallel with US1's Phase 3 tests once Phase 2 lands.**
- **Phase 5 (US3)**: Depends on Phase 3+4 completion (need final emission behavior for the byte-identity check).
- **Phase 6 (Polish)**: Depends on Phases 1–5 completion. T053 (fixture setup) is a soft prereq for T057 (audit test) — the audit test won't run until the fixture exists.

### Within Each User Story

- **US1**: T020 (instrumentation) → T021 (investigation) → T022-T027 (fixes) → T028 (remove instrumentation) → T029-T039 + T033a + T037a (13 unit tests, mostly parallel) → T040 (audit test).
- **US2**: T041-T046 (emission wiring) → T047-T049 (unit tests, parallel) → T050 (integration test).
- **US3**: T051 (verify existing goldens) → T052 (verify new fixture goldens if fixture landed).

### Parallel Opportunities

Genuine `[P]` (different files, no dependencies):

- **Phase 2 T015/T016/T017** — parity registration across 3 different files (cdx.rs, spdx2.rs, spdx3.rs).
- **Phase 3 T029-T038** — 10 unit tests all in the same file (`gowork.rs`) BUT non-conflicting test-fn additions; can be authored together.
- **Phase 4 T047-T049** — 3 unit tests in the same file (`metadata.rs`) with the same non-conflicting property.
- **Phase 6 T054/T055** — CHANGELOG + docs updates in different files.

**File-conflict note**: T029-T038 all touch `gowork.rs`'s `#[cfg(test)]` module — technically NOT `[P]` under strict interpretation (same file), but as append-only additions they don't block each other for a single contributor. Marking them `[P]` for team-parallelism visibility.

---

## Parallel Example: Phase 2 parity registration

```bash
# T015 + T016 + T017 all edit DIFFERENT files:
Task: "Register C112 cdx_anno! invocation in mikebom-cli/src/parity/extractors/cdx.rs"
Task: "Register C112 spdx23_anno! invocation in mikebom-cli/src/parity/extractors/spdx2.rs"
Task: "Register C112 spdx3_anno! invocation in mikebom-cli/src/parity/extractors/spdx3.rs"

# T018 depends on T015-T017 completing (mod.rs registration references the extractor fns defined by T015-T017).
```

## Parallel Example: Phase 6 docs

```bash
Task: "Add CHANGELOG.md entry per SC-011"
Task: "Update docs/reference/sbom-format-mapping.md C112 entry"
```

---

## Implementation Strategy

### MVP scope

**US1 alone is the MVP.** Delivers the observable-bug fix (30.8% → ≤5% wrong edges on `test-kubernetes`). US2's doc-scope signal is a P2 enhancement; US3 is a regression guard.

Ship order:

1. Phase 1 (Setup) — 1 sitting. New types + parser.
2. Phase 2 (Foundational) — 1 sitting. Diagnostics plumbing + parity registration.
3. Phase 3 (US1) — **the heavy lift**. T021 empirical investigation is the load-bearing task; may take 3-5 investigation loops. T024-T027 fixes depend on T021 findings.
4. **STOP + VALIDATE**: Run T040 audit test. Iterate on T021-T027 if SC-001 > 5%.
5. Phase 4 (US2) — 1 sitting once US1 stabilizes.
6. Phase 5 (US3) — 1 sitting. Should be trivially green — SC-003 predicts zero changes to existing goldens.
7. Phase 6 (Polish) — 1 sitting. Includes T053 fixture-repo PR coordination.

### Empirical revision escape hatch

Per spec.md Assumptions §7, if T021 investigation reveals FR-007 root causes are more complex than anticipated (e.g. Go's `GOWORK=off go mod graph` in a `use`d module still returns wrong edges when the surrounding filesystem has a `go.work`), SC-001 target ≤ 5% may be revised inline to a demonstrated floor below 30.8%. CHANGELOG entry (T054) MUST document the revised floor + rationale. In the extreme case, an alternative fix path (parse `go.mod` require blocks directly instead of shelling out) may be needed — that's a bigger design change and would be its own milestone per plan.md Notes.

### Parallel team strategy

With 2-3 contributors:

- Contributor A: Phases 1 → 2 → 3 (US1) — the load-bearing sequential path.
- Contributor B: Phase 4 (US2) — starts after Phase 2 lands; parallelizable with US1's tests-only section (T029-T039).
- Contributor C: Phase 6 T053 (fixture-repo coordination) can start early — the fixture takes time to land in the sibling repo.

---

## Notes

- All test tasks are load-bearing SC evidence (SC-009 requires ≥10 unit tests; SC-010 requires the integration test; SC-001 requires the audit test). Skipping tests fails the milestone acceptance.
- The FR-007 investigation (T021) is deliberately investigation-first — the exact root causes are not fully knowable at spec time. Documenting findings inline in T024-T027 commit messages is the deliverable, not a separate spec update.
- Preserve milestone-055 fetch-concurrency (16-way, per `graph_resolver.rs:344`) unchanged per FR-008. No perf-tuning tasks in this milestone.
- SC-003 dual-side byte-identity is a REGRESSION GUARD, not new emission. Failing SC-003 during Phase 5 verification indicates an emission-leak bug that needs fixing in US1/US2 before proceeding.
- Constitution Principle IV (`no .unwrap()` in production): all new code follows the milestone-055/091/160 pattern with `anyhow::Result` + `?` propagation. Test modules using `.unwrap()` MUST be guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per the crate-root convention.
- No new Cargo dependencies (spec Assumption §4). The parser + subprocess env-override use stdlib only.
- Per closed-but-extensible reason vocab: if T021 uncovers a new go.work parse-failure class not covered by the current 6 codes, discuss vocab extension at PR review; don't unilaterally add codes.
- **Task-ID reference reconciliation**: spec.md and plan.md reference "T014–T016 empirical investigation" as shorthand for the FR-007 discovery loop. During tasks decomposition this materialized as T020 (instrumentation) → T021 (investigation) → T022–T027 (fixes). Both ID ranges refer to the same body of work; the spec's `T014–T016` label predates the final tasks.md numbering.
- **T053 fixture-repo dependency**: the `test-kubernetes` fixture is out-of-repo. T040 (SC-001 audit test) is skippable during local test runs (env-var-gated) but MUST pass in CI once T053 lands the fixture. Sequencing: T053 first (starts landing in fixture repo), then T040 authored, then T057 verifies.
