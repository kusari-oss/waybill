---
description: "Task list for m773 — parallelize the golang::graph_resolver per-workspace loop"
---

# Tasks: Parallelize the golang::graph_resolver per-workspace loop

**Input**: Design documents from `/specs/773-graph-resolver-parallel/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md — all complete
**Tests**: Included — the milestone's acceptance criteria (SC-002 byte-identity, SC-004 determinism, SC-005 log wire-shape preservation, SC-006 --no-go-mod-why regression pin) are inherently test-verifiable.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel with other [P] tasks in the same phase.
- **[Story]**: `[US1]` on user-story-phase tasks. Absent on Setup / Foundational / Polish.
- Absolute paths from repo root: `/Users/mlieberman/Projects/mikebom/…`

## Path Conventions

Single Rust workspace. Modified/new files per plan.md §Project Structure:

- `waybill-cli/src/scan_fs/package_db/golang/legacy.rs` — MODIFIED (parallelize the loop at line ~1780)
- `waybill-cli/tests/graph_resolver_parallel_773.rs` — NEW (integration tests)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify branch state, capture pre-milestone Kubernetes baseline for regression comparison.

- [ ] T001 Confirm branch `773-graph-resolver-parallel` is current and clean (`git status --short` empty besides `specs/773-…/` untracked); confirm `cargo +stable build -p waybill --all-targets` succeeds on the pre-milestone tree.
- [ ] T002 [P] Capture pre-milestone Kubernetes wall-time baselines per quickstart.md Prerequisite block. Record BOTH `--offline sbom scan` (default) AND `--offline --no-go-mod-why sbom scan` (walker-isolated) wall-times, plus CPU utilization % + count of `"go transitive edges resolution summary"` log lines (should be 38 on kubernetes). Baseline expected: default ≈ 34s, walker-isolated ≈ 19s, summary-lines = 38. Used later by T017 empirical validation.

**Checkpoint**: Repo clean, baseline recorded.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Types + compile-time Send+Sync assertions the US1 implementation depends on. All independent of US1 execution; can bundle with US1 in one PR.

- [ ] T003 [P] In `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/golang/legacy.rs`, define the `WorkspaceJob<'a>` struct per data-model.md §WorkspaceJob at module scope near the parallelized loop. Fields: `workspace_index: usize`, `project_root: &'a PathBuf`, `doc: &'a <existing doc type>`, `sums: &'a <existing sums type>` — inspect the pre-milestone loop signature at line ~1780 to confirm the exact borrowed types. Visibility `pub(super)` (local to golang module tree). Doc comment cites spec.md FR-001 + research R3 (workspace_index for determinism).
- [ ] T004 [P] In same file, define the `ResolveResult` struct per data-model.md §ResolveResult. Fields: `workspace_index: usize`, `ctx: WorkspaceContext`, `resolve_outcome: Result<ModuleGraphMap, GraphResolverError>`. Owned data (no borrows) since it moves through the mpsc channel. Visibility `pub(super)`. Doc comment cites FR-002 + FR-004.

**Checkpoint**: Types compile in isolation (no callers yet); `cargo build -p waybill --all-targets` still passes.

---

## Phase 3: User Story 1 — Parallel per-workspace graph resolution (Priority: P1) 🎯 MVP

**Goal**: Replace the serial `for (project_root, doc, sums) in &parsed_roots` loop at `legacy.rs:1780` with a bounded thread pool over the workspace queue + mpsc reducer + workspace_index-ordered Phase 2 reduce. Wall-time on k8s drops from 34s → ≤ 20s default / 19s → ≤ 8s walker-isolated.

**Independent Test**: quickstart.md "Validate SC-001" block. Walker-isolated (`--no-go-mod-why`) wall-time ≤ 8s; default scan wall-time ≤ 20s; CPU utilization on walker-isolated > 300% (concurrency active). SC-005: `grep -c "go transitive edges resolution summary" log` = 38 (matches workspace count).

### Tests for User Story 1

- [ ] T005 [P] [US1] Compile-time assertion in `waybill-cli/tests/graph_resolver_parallel_773.rs` (new file). Add: `fn assert_send_sync<T: Send + Sync>() {}` helper + a `#[test]` that calls `assert_send_sync::<GraphResolver>()` and `assert_send_sync::<GoModCache>()`. Test fails at compile if a future patch adds a `!Send` or `!Sync` field to either type (research R7 + FR-009 + FR-010 belt-and-braces).
- [ ] T006 [P] [US1] Integration test in `waybill-cli/tests/graph_resolver_parallel_773.rs::m773_multi_workspace_scan_succeeds`. Reuses the existing `waybill-cli/tests/fixtures/golang/mod_why_scaling/` fixture (4 workspaces per m771 US2 tests). Runs `waybill --offline sbom scan` against it with `WAYBILL_GO_MOD_WHY_BUDGET_MS=1` (short-circuits classifier; focuses on resolver path); asserts scan exits 0 and stderr contains `"go transitive edges resolution summary"` line (at least one workspace was analyzed). Smoke test that the parallel path doesn't hang/panic.
- [ ] T007 [P] [US1] Integration test in `waybill-cli/tests/graph_resolver_parallel_773.rs::m773_deterministic_emit_order_across_runs`. Run waybill twice back-to-back against the mod_why_scaling fixture; mask serialNumber + created runtime-random fields; diff outputs; assert byte-identical. SC-004 direct verification.
- [ ] T008 [P] [US1] Integration test in `waybill-cli/tests/graph_resolver_parallel_773.rs::m773_per_workspace_summary_log_fires_once_per_workspace`. Run waybill against the mod_why_scaling fixture with `RUST_LOG=info`; grep stderr for `"go transitive edges resolution summary"`; assert exactly N lines match where N = detected workspace count. SC-005 direct verification.

### Implementation for User Story 1

- [ ] T009 [US1] In `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/golang/legacy.rs` (inside `pub fn read` around line 1780 — see analyze finding I2 note in T010), locate the serial loop `for (project_root, doc, sums) in &parsed_roots`. Extract the resolver-call portion into a new private function `fn resolve_workspaces_parallel(parsed_roots, resolver, cache) -> Vec<Option<ResolveResult>>`. The function: (a) builds a `Vec<WorkspaceJob>` from parsed_roots with workspace_index = slice-index; (b) wraps `resolver` in `Arc<GraphResolver>` and `cache` in `Arc<GoModCache>`; (c) uses `mod_why::worker_count(parsed_roots.len())` for the worker count (per research R2, reuse the m771 helper); (d) uses `std::thread::scope` to spawn workers that pop from `Arc<Mutex<Vec<WorkspaceJob>>>`, **construct `WorkspaceContext` inside the worker body via `let ctx = WorkspaceContext::from_parts(job.project_root.clone(), job.doc, job.sums, /* offline = */ false);` (analyze finding A1)**, call `resolver.resolve(&ctx, &cache)`, send `ResolveResult { workspace_index: job.workspace_index, ctx, resolve_outcome: <result> }` back via mpsc; (e) main-side `drop(tx)` before `rx.recv()` loop; (f) collects results into `Vec<Option<ResolveResult>>` indexed by workspace_index; (g) `join()` inspection propagates any worker panic via `resume_unwind`. Serial-fallback branch when `worker_count <= 1 || parsed_roots.len() <= 1` — builds the same shape by calling `resolver.resolve()` inline for each workspace and populating the `Vec<Option<ResolveResult>>` directly (byte-identical to pre-milestone).
- [ ] T010 [US1] In same file, refactor the loop body at line ~1780 (inside `pub fn read` — the enclosing function name per analyze finding I2) to first call `let mut results = resolve_workspaces_parallel(&parsed_roots, &resolver, &cache)` and receive `Vec<Option<ResolveResult>>`. Then iterate **`parsed_roots` and `results` in lockstep** so the reduce body has access to BOTH the pre-milestone bindings (`project_root`, `doc`, `sums`) AND the resolver output (`ctx`, `resolve_outcome`) — see analyze finding I1. Explicit reduce shape:
    ```rust
    for (i, (project_root, doc, sums)) in parsed_roots.iter().enumerate() {
        let ResolveResult { ctx, resolve_outcome, .. } = results[i]
            .take()
            .expect("no gaps — every workspace produces a ResolveResult");
        let graph_map = match resolve_outcome {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    project_root = %project_root.display(),
                    error = %e,
                    "Go transitive-edge resolver failed; falling back to empty edge set"
                );
                Default::default()
            }
        };
        // All existing post-resolver.resolve() lines from the old loop body:
        //   - signals.go_transitive_coverage merge via merge_coverage
        //   - signals.gosum_fallback_count accumulator
        //   - entries build via build_entries_from_go_module_with_lookup(doc, sums, ...)
        //   - +incompatible filter
        //   - out.push(entry), seen_purls.insert(purl)
        //   - build_main_module_entry(doc, project_root, ...) + gosum augment
        //   - backfilled_paths.insert(...)
        // ... all reference project_root / doc / sums / ctx / graph_map from the scope above.
    }
    ```
    Preserve every existing post-`resolver.resolve()` line VERBATIM in semantics — move them from inside the old loop body to inside the reduce loop body, unchanged. The `parsed_roots.iter().enumerate()` + `results[i].take()` lockstep ensures byte-identity with pre-milestone behavior since the iteration order + input bindings are preserved.
- [ ] T011 [US1] Verify FR-009 + FR-010 compile-time guarantees hold. `cargo build -p waybill --all-targets` must succeed; specifically the T005 assert_send_sync test compiles. If it fails, one of `GraphResolver` / `GoModCache` has a `!Send` or `!Sync` field — investigate + fix or (worst case) wrap the offending field in `Arc<Mutex<>>` before shipping. Regression pin for FR-009 / FR-010.
- [ ] T012 [US1] Grep sanity-check per Contract 4 (negative-grep). `grep -n "Arc<Mutex" waybill-cli/src/scan_fs/package_db/golang/legacy.rs` must show only the m055 pre-existing usages plus the new `Arc<Mutex<Vec<WorkspaceJob>>>` for the queue — NO `Arc<Mutex<>>` around signals, entries, out, seen_purls, or backfilled_paths. Post-loop state must be main-thread-only per FR-005. **Positive-grep verification (analyze finding C1)**: also confirm each of the 5 post-loop mutation sites remains inside the reduce loop's `{ ... }` scope (not inside any `s.spawn(...)` closure) by running the following greps and inspecting context lines around each hit:
    ```sh
    grep -n "signals.go_transitive_coverage\|signals.gosum_fallback_count" waybill-cli/src/scan_fs/package_db/golang/legacy.rs
    grep -n "out\.push\|seen_purls\.insert\|backfilled_paths\.insert" waybill-cli/src/scan_fs/package_db/golang/legacy.rs
    grep -n "build_entries_from_go_module_with_lookup\|build_main_module_entry" waybill-cli/src/scan_fs/package_db/golang/legacy.rs
    ```
    For every hit inside the modified region, confirm the surrounding indent + `for (i, ...) in parsed_roots.iter().enumerate()` context matches the reduce loop, NOT a `std::thread::scope` or `s.spawn` closure. Reviewer-visible property; belt-and-braces safety pin for FR-005.

**Checkpoint**: US1 done. All new unit tests + integration tests pass; `cargo build --all-targets` clean; grep-checks land clean. Manual quickstart.md SC-001 validation gives ≤ 8s walker-isolated on k8s.

---

## Phase 4: Polish & Cross-Cutting Concerns

**Purpose**: Full regression sweep, empirical validation, m669 baseline refresh, pre-PR gate.

- [ ] T013 [P] Run the full byte-identity regression suite: `cargo test -p waybill --no-fail-fast --test scan_go --test scan_cargo --test scan_python --test scan_npm --test cdx_regression --test spdx_regression --test spdx3_regression --test walk_registry_integration --test golang_transitive_edges_kubernetes_smoke`. Assert 0 failures across every binary. SC-002 direct verification; any regression fails the ship.
- [ ] T014 [P] Run the resolver-adjacent unit-test suite: `cargo test -p waybill --no-fail-fast --bin waybill graph_resolver::tests golang::`. Every pre-milestone test must still pass — `GraphResolver::resolve()` semantics unchanged (FR-009).
- [ ] T015 SC-006 `--no-go-mod-why` regression: run `WAYBILL_NO_GO_MOD_WHY=1` (or the equivalent CLI flag) against every existing Go fixture; compare CDX / SPDX 2.3 / SPDX 3 outputs against the pre-milestone baseline (masked normalized diff per memory `feedback_verify_golden_churn_normalized`). Expected: byte-identical.
- [ ] T016 SC-004 double-run byte-identity per quickstart.md: run k8s scan twice back-to-back with masked-random-fields diff; expect zero-line diff.
- [ ] T017 Empirical SC-001 validation on Kubernetes fixture per quickstart.md "Validate SC-001". Warmup + measure walker-isolated (`--no-go-mod-why`) + default scan. Compare against T002 baseline. Confirm: walker-isolated ≤ 8s (from 19s); default ≤ 20s (from 34s); CPU utilization on walker-isolated > 300% (concurrency active); `grep -c "go transitive edges resolution summary" log` = 38.
- [ ] T018 [P] Confirm SC-003 (zero new Cargo deps): `git diff --stat Cargo.lock waybill-cli/Cargo.toml waybill-common/Cargo.toml xtask/Cargo.toml` shows no additions to any `[dependencies]` block. Cargo.lock diff empty.
- [ ] T019 [P] Update `/Users/mlieberman/Projects/mikebom/docs/perf/baseline.json` via `cargo run -p xtask -- bench --update-baseline` (m669 harness). Commit the baseline change SEPARATELY from the code change so future regression bisects can attribute wall-time shifts cleanly. Deferred to a follow-up polish PR is also acceptable per m771/m772 convention.
- [ ] T020 [P] Confirm FR-012 (zero new operator flags): `cargo run -p waybill --release -- sbom scan --help 2>/dev/null | grep -E "^\s+--" | sort > /tmp/help-post.txt`; compare against pre-milestone reference. Zero diff expected. Cheap regression pin.
- [ ] T021 Run the full pre-PR gate: `./scripts/pre-pr.sh` (per CLAUDE.md — clippy + `cargo test --workspace`). Both MUST land clean. Per memory `feedback_prepr_gate_bails_on_first_failure` — use `--no-fail-fast` and enumerate every `^---- .+ stdout ----` line if any test fails.

**Checkpoint**: Milestone complete. All acceptance criteria (SC-001 through SC-006) empirically satisfied. Ready to open PR.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 sequential; T002 [P] can run concurrently once branch confirmed clean.
- **Phase 2 (Foundational)**: T003 + T004 [P] can be authored in the same commit — different types in same file but no interdep.
- **Phase 3 (US1)**: Depends on Foundational types. Tests T005–T008 can be authored in parallel with implementation T009–T012 (either order per the codebase's TDD-adjacent convention).
- **Phase 4 (Polish)**: Depends on US1 complete.

### Task Dependencies (within Phase 3)

- **T009 (extract `resolve_workspaces_parallel`)** must land before T010 (caller-site refactor consumes the new function).
- **T010 (reduce refactor)** requires T009 done.
- **T011 (compile-time Send+Sync check)** independent of T009/T010 — just needs T005 test to exist.
- **T012 (Arc<Mutex> grep sanity)** requires T009 + T010 done to grep the modified code.

### Parallel Opportunities

- **T003 + T004** [P] — Foundational types, same file, no interdep.
- **T005 + T006 + T007 + T008** [P] — 4 test-authoring tasks in the same new test file, mostly independent (share helper functions but different `#[test]` bodies).
- **T013 + T014 + T018 + T019 + T020** [P] — 5 polish tasks touching different artifacts (regression sweeps vs baseline vs Cargo diff vs --help).

---

## Parallel Example: Foundational

```bash
# T003 + T004: both types in legacy.rs, no interdep — one commit.
grep -c "struct WorkspaceJob\|struct ResolveResult" waybill-cli/src/scan_fs/package_db/golang/legacy.rs
# Expected: 2 (post-commit).
```

## Parallel Example: User Story 1 tests

```bash
# All 4 tests in the same new binary; can commit in one change.
cargo test -p waybill --test graph_resolver_parallel_773
```

---

## Implementation Strategy

**Single-PR delivery** — mirrors m772's single-US shape but with a real bottleneck confirmed via the methodology at `docs/development/perf-methodology.md`. Bundle Setup + Foundational + US1 + Polish into one PR (~250 LOC delta including tests).

- SC-002 byte-identity regression suite is the safety net; passes or fails atomically.
- LOC delta is small (~100 code + ~150 tests); reviewer cognitive load is manageable.
- Reviewer focus: the workspace_index-ordered reduce (FR-004 correctness) + the panic propagation path (FR-006). Everything else is a mechanical loop refactor.

**PR sizing target**:
- Total ~250 lines net add. Smaller than m771 US3 (180 lines source + 300 lines tests). Larger than m771 US1 (30 lines).
- Byte-identity regression suite is fast (< 10s locally); empirical Kubernetes validation is ~30s per run.

**Total task count**: 21 tasks across 4 phases. Single-US shape reflects the single-shippable-slice nature of the milestone.
