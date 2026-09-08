# Contract — graph_resolver parallelism surface

**Feature**: 773-graph-resolver-parallel
**Status**: Complete
**Date**: 2026-09-05
**Supersedes**: nothing — this is a purely internal refactor of the per-workspace loop at `legacy.rs:1780`.

The only external interface this milestone touches is the per-workspace loop inside `pub fn read` at `waybill-cli/src/scan_fs/package_db/golang/legacy.rs:1615` (loop body starts around line 1780). This contract pins the properties the parallelization must preserve, so reviewers can verify at code-review time.

---

## Contract 1 — `GraphResolver::resolve()` public API unchanged (FR-009)

**Signature (pre + post)**:
```rust
impl GraphResolver {
    pub fn resolve(
        &self,
        ctx: &WorkspaceContext,
        cache: &GoModCache,
    ) -> Result<ModuleGraphMap, GraphResolverError>;
}
```

**Post-milestone**: identical. Callers see no change. Internally, `resolve()` still runs its 4-step ladder synchronously per workspace. This milestone parallelizes ACROSS calls, not WITHIN a call.

**Verification**: `grep -n "pub fn resolve" graph_resolver.rs` returns exactly one line matching the signature above. Also verified by every existing test that invokes `resolver.resolve()` continuing to pass unchanged.

---

## Contract 2 — `GoModCache` public API unchanged (FR-010)

**Signatures (pre + post)**:
```rust
impl GoModCache {
    pub fn discover(rootfs: &Path) -> Self;
    pub(crate) fn is_empty(&self) -> bool;
    pub(crate) fn read_mod_file(&self, module: &str, version: &str) -> Option<String>;
}
```

**Post-milestone**: identical. All methods `&self`; the type stays `Send + Sync`.

**Verification**: `assert_send_sync::<GoModCache>()` helper in the new integration test at `tests/graph_resolver_parallel_773.rs` — compile-time guarantee.

---

## Contract 3 — Determinism via workspace_index-ordered reduce (FR-004 + SC-004)

**Post-milestone**: Phase 2 reduce iterates `results[0..parsed_roots.len()]` in ascending order. The pre-milestone loop iterated `parsed_roots` in slice-order; this milestone preserves that exact ordering by using slice-index as the reduce key. Any operation that appends to `out`, inserts into `seen_purls`, or mutates `signals` sees identical input sequence pre and post milestone.

**Verification**: SC-004 double-run byte-identity check — run waybill twice against the same fixture, mask `serialNumber` + `created`, `diff` outputs = zero.

---

## Contract 4 — Post-loop state mutation confined to main thread (FR-005)

**Pre-milestone**: `signals.go_transitive_coverage`, `signals.gosum_fallback_count`, `entries` build, `out.push`, `seen_purls.insert`, `build_main_module_entry` augmentation, `backfilled_paths.insert` — all mutate inline in the serial loop body at `legacy.rs:1806-2000+`.

**Post-milestone**: identical mutation logic runs on the main thread inside Phase 2 reduce. Workers do NOT touch this state. No synchronization primitives (no `Arc<Mutex>`, no atomics) are introduced for these fields.

**Verification**: `grep -rn "Arc<Mutex" waybill-cli/src/scan_fs/package_db/golang/legacy.rs` should show zero new occurrences in the modified code region (the existing m055 usages elsewhere unchanged).

---

## Contract 5 — Per-workspace FR-013 summary log wire-shape preserved (FR-007 + SC-005)

**Pre-milestone log line** (emitted at `graph_resolver.rs:716`):
```
INFO waybill::scan_fs::package_db::golang::graph_resolver: go transitive edges resolution summary
  total_modules=N graph_count=N cache_count=N proxy_count=N gosum_count=N unresolved_count=N coverage=STR
```

**Post-milestone**: identical wire-shape. Continues to fire exactly once per workspace analyzed. Log-line ORDERING under concurrent workers MAY interleave (different workers complete in different orders) — that's expected and does not violate the wire-shape contract.

**Verification**: `grep -c "go transitive edges resolution summary" <log>` = number of workspaces analyzed. Field names + count in each line unchanged.

---

## Contract 6 — Fail-fast on worker panic (FR-006)

**Pre-milestone**: `resolver.resolve()` runs on the main thread; panics unwind the caller's stack.

**Post-milestone**: workers run in `std::thread::scope`; panics captured by `ScopedJoinHandle::join()` returning `Err(payload)`. The main thread inspects `.join()` results, logs the failing workspace's absolute path via `tracing::error!`, and `resume_unwind(payload)`s to propagate the panic identically to pre-milestone behavior.

**Verification**: integration test `tests/graph_resolver_parallel_773.rs::m773_worker_panic_fails_fast` — construct a synthetic `WorkspaceContext` that will trigger a panic in `resolver.resolve()` (or panic in the WorkspaceContext constructor); assert the scan exits with a non-zero status.

---

## Contract 7 — `--no-go-mod-why` orthogonality (FR-008 + SC-006)

**Post-milestone**: the resolver parallelization is a distinct code path from the m112 classifier. `--no-go-mod-why` continues to short-circuit at `main.rs:330` before `apply_go_mod_why_pass` is invoked; the resolver runs regardless of the classifier's skip status.

**Verification**: SC-006 byte-identity — scan any fixture with `--no-go-mod-why`; compare CDX / SPDX 2.3 / SPDX 3 outputs against pre-milestone binary. Zero diff modulo version-string cascades.

---

## Contract 8 — Backwards compatibility with existing tests

Every existing test in `waybill-cli/tests/` that exercises the Go path must continue to pass unchanged:
- `scan_go*.rs` — end-to-end reader coverage
- `golang_transitive_*.rs` — transitive-edge tests
- `cdx_regression`, `spdx_regression`, `spdx3_regression` — golden byte-identity

Every existing `graph_resolver.rs::tests::*` unit test must continue to pass unchanged — the resolver's per-workspace behavior is untouched by this milestone.

**Verification**: `./scripts/pre-pr.sh` clean + m669 benchmark harness update.

---

## Contract 9 — Zero new operator surface (FR-012)

**Pre-milestone**: `--no-go-mod-why`, `--no-binary-scan`, `--exclude-path`, `--no-deep-hash` etc. are the operator's Go-related tuning flags.

**Post-milestone**: identical. Parallelism is default-on, tuned by `available_parallelism()`. No new CLI flags. No new `WAYBILL_*` env vars.

**Verification**: `cargo run -p waybill -- sbom scan --help | diff <(pre)` — zero non-doc-comment differences.
