# Contract — collect_*_imports parallelism surface

**Feature**: 774-parallel-source-imports
**Status**: Complete
**Date**: 2026-09-04
**Supersedes**: nothing — this is a purely internal refactor of the per-workspace loop at `legacy.rs:1787–2233`.

The only external interface this milestone touches is the per-workspace loop inside `pub fn read` at `waybill-cli/src/scan_fs/package_db/golang/legacy.rs:1615`. This contract pins the properties the parallelization must preserve, so reviewers can verify at code-review time.

---

## Contract 1 — `collect_production_imports` + `collect_test_imports` signatures unchanged (FR-005)

**Signature (pre + post)**:

```rust
fn collect_production_imports(
    project_root: &Path,
    depth: usize,
    known_modules: &[String],
    into: &mut HashSet<String>,
);

fn collect_test_imports(
    project_root: &Path,
    depth: usize,
    known_modules: &[String],
    into: &mut HashSet<String>,
);
```

**Post-milestone**: identical. Callers see no change to the function shapes. This milestone parallelizes ACROSS calls, not WITHIN a call.

**Verification**: `grep -n "fn collect_production_imports\|fn collect_test_imports" waybill-cli/src/scan_fs/package_db/golang/legacy.rs` returns exactly the same two-line output pre and post milestone.

---

## Contract 2 — `Signals::production_imports` + `Signals::test_only_imports` field shapes unchanged

**Signatures (pre + post)**:

```rust
pub struct Signals {
    // ... other fields unchanged ...
    pub production_imports: HashSet<String>,
    pub test_only_imports: HashSet<String>,
    // ... other fields unchanged ...
}
```

**Post-milestone**: identical field types + names + visibility. Every downstream consumer (`apply_go_production_set_filter` at `mod.rs:2172`, `compute_go_test_only_closure` at `mod.rs:788`) sees unchanged input shape.

**Verification**: `grep -n "pub production_imports\|pub test_only_imports" waybill-cli/src/scan_fs/package_db/golang/legacy.rs` returns unchanged output.

---

## Contract 3 — Element-identity of `production_imports` + `test_only_imports` (FR-004 + FR-011 + SC-002)

**Pre-milestone**: `signals.production_imports` accumulates via N inline mutations inside the loop body. Content after loop exit = ∪ᵢ (production imports discovered in workspace i).

**Post-milestone**: `signals.production_imports` is initialized empty, then filled via N `extend()` calls in the Phase 2 reduce. Content = ∪ᵢ (production imports discovered in workspace i) — identical by set-union commutativity.

Same identity applies to `test_only_imports` (computed downstream as difference; input to the difference is content-identical, so output is content-identical).

**Verification**: SC-002 byte-identity check on every fixture. Every existing regression test (`cdx_regression*`, `spdx_regression*`, `spdx3_regression*`, `golang_transitive_*`, `scan_go_*`) MUST pass unchanged.

---

## Contract 4 — Determinism across independent runs (SC-004)

**Post-milestone**: two independent `waybill sbom scan` invocations against the same tree on the same host produce byte-identical outputs (modulo `serialNumber` + `created` masks). Determinism holds by construction because:
- The reduce writes to `signals.production_imports` via `extend()` — content is set-union, order-independent.
- Every downstream consumer of `production_imports` and `test_only_imports` is verified order-independent (per research R5).

**Verification**: `waybill-cli/tests/collect_imports_parallel_774.rs::m774_determinism_across_runs` — synthetic 3-workspace fixture, run scan twice, assert masked byte-identity of CDX + SPDX 2.3 + SPDX 3 outputs.

---

## Contract 5 — Panic fail-fast propagation with workspace-path logging (FR-007 + Principle III)

**Pre-milestone**: `collect_*_imports` panic unwinds the caller's stack, terminating the scan with a non-zero exit.

**Post-milestone** (per research R2 revised): worker-thread wraps each per-job body in `std::panic::catch_unwind(AssertUnwindSafe(...))`. On `Err(payload)`, the worker:
1. Logs the failing workspace's absolute path (via `job.project_root.display()`) and `workspace_index` via `tracing::error!` on THIS worker thread — the per-job context is still in scope, unlike the pool-worker-level `.join()` approach.
2. Calls `std::panic::resume_unwind(payload)` to re-raise the original panic.

`std::thread::scope`'s automatic join at scope-close propagates the re-raised panic to the enclosing `pub fn read` call, matching pre-milestone unwind path. Scan exit code + top-level panic string are byte-identical to pre-milestone semantics; the workspace-scoped `tracing::error!` line is strictly-additional diagnostic.

**Verification**: `waybill-cli/tests/collect_imports_parallel_774.rs::m774_worker_panic_fails_fast` — inject panic via `#[cfg(test)]` gate or find a natural panic path; assert:
- Scan exits non-zero.
- `tracing::error!` line with `project_root=<absolute path>` AND `workspace_index=<usize>` appears in captured stderr BEFORE the top-level panic message.

---

## Contract 6 — Single-workspace degenerate path zero overhead (NFR-002 + SC-005)

**Pre-milestone**: single-workspace scan calls `collect_*_imports` inline in the serial loop body — no thread spawn, no mutex, no mpsc.

**Post-milestone**: `parsed_roots.len() <= 1` short-circuits before spawning any workers; the parallel phase inlines the two `collect_*_imports` calls on the main thread, matching pre-milestone latency to within ±3% (SC-005 target).

**Verification**: `waybill-cli/tests/collect_imports_parallel_774.rs::m774_single_workspace_no_thread_spawn` — using `assert!` on wall-time delta vs a pre-milestone baseline (m669 `go-module-medium` fixture; SC-005 tolerance ±3%).

---

## Contract 7 — Log-line wire-shape preservation (FR-009)

**Pre-milestone log lines** (unchanged): `Issue #255: filtered +incompatible legacy-version residue ...`, `Issue #251: flat-attaching residual-orphan Go components to main-module ...`, `Issue #250: linked Go 1.24+ tool-directive entries to main-module ...`.

**Post-milestone**: all three log lines continue to fire from the SERIAL MAIN LOOP (unchanged code path). Content, level, fields, and firing conditions are byte-identical. Ordering is also byte-identical (serial main loop preserves iteration order).

**Verification**: `grep -c "Issue #255\|Issue #250\|Issue #251"` on captured logs from a Go scan is unchanged pre vs post milestone.

---

## Contract 8 — New FR-014 summary log line

**Post-milestone log line** (new, one per `pub fn read` invocation, always fires):

```
INFO waybill::scan_fs::package_db::golang::legacy:
  m774 parallel source-import collection complete
  workspaces_scanned=N parallel_workers_used=W
  production_imports_count=P test_imports_count=T
  elapsed_ms=E
```

Fields:
- `workspaces_scanned`: `parsed_roots.len()`
- `parallel_workers_used`: return value of `mod_why::worker_count(parsed_roots.len())`
- `production_imports_count`: `signals.production_imports.len()` at end of reduce
- `test_imports_count`: aggregated union size (before difference computation)
- `elapsed_ms`: `Instant::now() - phase_start`, as `u64`

**Verification**: `waybill-cli/tests/collect_imports_parallel_774.rs::m774_summary_log_fires_once_per_read` — capture tracing output, assert exactly one line matching the pattern.

---

## Contract 9 — `--no-go-mod-why` orthogonality (SC-007)

**Post-milestone**: the parallelization is a distinct code path from the m112 `go mod why` classifier. `--no-go-mod-why` continues to short-circuit at `main.rs:330` before `apply_go_mod_why_pass` is invoked; the parallel imports phase runs regardless of the classifier's skip status.

**Verification**: SC-007 byte-identity — scan any fixture with `--no-go-mod-why`; compare CDX / SPDX 2.3 / SPDX 3 outputs against pre-milestone binary. Zero diff modulo version-string cascades + `serialNumber` + `created` masks.

---

## Contract 10 — Zero new operator surface (FR-006)

**Pre-milestone**: `--no-go-mod-why`, `--no-binary-scan`, `--exclude-path`, `--no-deep-hash` etc. are the operator's Go-related tuning flags.

**Post-milestone**: identical. Parallelism is default-on, tuned by `available_parallelism()`. No new CLI flags. No new `WAYBILL_*` env vars.

**Verification**: `cargo run -p waybill -- sbom scan --help | diff <(pre)` — zero non-doc-comment differences.

---

## Contract 11 — Zero new Cargo dependencies (FR-010 + SC-003)

**Pre + post milestone**: `waybill-cli/Cargo.toml` dependency section unchanged. `Cargo.lock` unchanged. Every new type in this milestone (`WorkspaceImportJob`, `ImportCollectionResult`, `SharedImportState`) uses only stdlib types (`HashSet`, `Arc<Mutex<_>>`, `mpsc::Sender/Receiver`, `PathBuf`).

**Verification**: `git diff Cargo.toml Cargo.lock` on the merged PR returns zero output.
