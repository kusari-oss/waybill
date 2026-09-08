# Contract — Walker parallelism surface

**Feature**: 772-parallel-scan-walker
**Status**: Complete
**Date**: 2026-09-04

This milestone extends `SharedWalker::run` (m664) with a parallel execution path. This contract pins the public surface that (a) reader authors, (b) `scan_fs::scan_path` callers, and (c) walker-audit tooling can rely on before, during, and after the milestone lands.

---

## Contract 1 — `SharedWalker::run` public API is unchanged

**Pre-milestone signature**:
```rust
impl<'reg, 'ex> SharedWalker<'reg, 'ex> {
    pub fn new(rootfs: &Path, registry: &'reg ReaderRegistry, exclude_set: &'ex ExclusionSet) -> Self;
    pub fn with_max_depth(mut self, depth: usize) -> Self;
    pub fn run(&mut self); // was &mut self
}
```

**Post-milestone signature**: **identical**. Callers see no change (FR-002). Internally, `run(&mut self)` may spawn worker threads, but the caller-facing interface — construct → configure → run → read outputs — is preserved.

**Callers** (unchanged): `scan_fs::package_db::mod::run_shared_walker_pilot`. Also every future migrated reader per the walker-audit contract.

---

## Contract 2 — Reader dispatch contract preserved

**Pre-milestone**: readers register via `ReaderRegistration` with `on_file` / `on_dir` callbacks. Callbacks are invoked serially on the walker thread.

**Post-milestone**: reader callbacks MAY be invoked from any worker thread. Same shape, same arguments. Two callbacks for the same reader MAY overlap in wall time if they target different files.

**Reader author obligations** (implicit contract, spot-checked at code review):
- If the reader mutates shared state, that state MUST be `Sync` (already the case for output collectors via `Mutex<Vec<PackageDbEntry>>`).
- Reader-internal caches or scratch state SHOULD be per-call; a reader that holds `&mut self` scratch across callbacks MUST already have been unsound under m664's `&mut self` walker signature.

**Verification**: `cargo test -p waybill --workspace` byte-identity across every existing fixture — any reader that silently depended on serial ordering will surface as a diff.

---

## Contract 3 — Emit-order determinism (FR-007 + SC-004)

**Pre-milestone**: reader outputs land in the order the walker traverses (DFS-preorder from rootfs).

**Post-milestone**: reader outputs are sorted per-reader before `SharedWalker::run` returns. Sort key: per-reader natural key (typically `entry.purl.as_str()`; falls back to `entry.source_path` or `entry.name`).

**Downstream contract**: `scan_fs::scan_path` and every consumer of `SharedWalker` outputs MUST tolerate a deterministic-but-different-from-pre-milestone emit order.

**Impact assessment**: every existing regression fixture that goes through the CDX / SPDX 2.3 / SPDX 3 emitters is verified byte-identical (SC-002) because the emitters already impose their own final sort. Byte-identity confirms this contract holds end-to-end.

---

## Contract 4 — Fail-fast on worker panic (FR-008)

**Pre-milestone**: walker runs on the caller's thread; panics unwind the caller's stack directly.

**Post-milestone**: worker threads panic in isolation. `SharedWalker::run` MUST detect the panic via `JoinHandle::join()` and propagate it via `tracing::error!` + panic-any-payload OR `anyhow::bail!` at the caller-site boundary.

**Downstream contract**: `scan_fs::scan_path` observes walker panics as `anyhow::Error` returns, same as any other walker failure. NO silent partial-output drop.

**Verification**: integration test in `walker_parallelism_772.rs` registers a synthetic reader that panics on a specific filename; asserts the scan exits with a non-zero error containing "panic" + the offending worker's identifier.

---

## Contract 5 — Symlink-loop safety across worker boundaries (FR-003 + SC-005)

**Pre-milestone**: `visited: HashSet<PathBuf>` on the walker owns loop detection. A canonicalized directory visited once is skipped forever within the same scan.

**Post-milestone**: `Arc<Mutex<HashSet<PathBuf>>>` shared across workers. The check-and-insert is one atomic operation under the mutex; two workers racing on the same canonical path result in exactly one descending (the first) and the other seeing "already present" and skipping.

**Verification**:
- Existing `walks_symlink_loop_without_hanging` regression test at `waybill-cli/src/scan_fs/package_db/golang/go_binary.rs::tests` continues to pass (single-subtree loop).
- New cross-subtree symlink-loop fixture at `waybill-cli/tests/fixtures/walk_registry/symlink_loop_cross_subtree/` (two sibling dirs mutually referring to each other). Integration test asserts the walker terminates within an order of magnitude of the m054 fixture's wall time.

---

## Contract 6 — m113 `ExclusionSet` semantics preserved (FR-004)

**Pre-milestone**: `ExclusionSet::matches` gate at `walk_inner` line ~134 skips excluded directories.

**Post-milestone**: SAME gate, invoked inside each worker's `walk_one_directory` step. An excluded directory MUST NOT be pushed onto the shared queue.

**Verification**: `waybill-cli/tests/exclude_path_*.rs` integration tests pass unchanged (byte-identity).

---

## Contract 7 — m664 `descend_into` scope-restriction preserved (FR-005)

**Pre-milestone**: `compute_descend_into_scope` at `walk_inner` line ~186 determines whether a normally-skipped dir is opted back in by any reader's `descend_into`. If so, dispatch under that subtree is restricted to the requesting reader set.

**Post-milestone**: SAME gate; the resulting `Option<HashSet<ReaderId>>` scope is propagated into the `SubtreeJob` pushed onto the queue. Workers popping that job invoke `walk_one_directory` with the restricted scope, which propagates to reader dispatch calls.

**Verification**: `waybill-cli/tests/walk_registry_integration.rs::descend_into_*` tests pass unchanged.

---

## Contract 8 — Walker-audit allowlist (FR-011)

**Pre-milestone**: `waybill-cli/src/scan_fs/walk.audit-allowlist.txt` lists every `fn walk_*` function that is exempted from the m664 shared-walker migration mandate. Grep-audit runs in CI (per m115/m117).

**Post-milestone**: if this milestone introduces any new `fn walk_*` function (e.g., `fn walk_one_directory`, `fn walk_workers_drain`), the same PR MUST add each to the allowlist file. Failure surfaces as a CI grep-mismatch, blocking merge.

**Verification**: CI grep pass (already runs on every PR touching `scan_fs/`).

---

## Contract 9 — Zero new operator flags (FR-009)

**Pre-milestone**: `--no-binary-scan`, `--no-go-mod-why`, `--exclude-path`, `--no-deep-hash` etc. are the operator's perf-tuning surface.

**Post-milestone**: **identical**. Parallelism is default-on and tuned by `available_parallelism()`. NO new CLI flags are added. The `WAYBILL_*` env-var namespace also gains no new entries.

**Verification**: `cargo run -p waybill -- sbom scan --help | diff` against pre-milestone output — expected zero non-doc-comment differences.

---

## Contract 10 — Backwards compatibility with m664 walker-registry tests

Every existing test in `waybill-cli/tests/walk_registry_*.rs` continues to pass unchanged:
- `walk_registry_integration.rs` — end-to-end reader dispatch coverage
- `walk_registry_descend_into.rs` — Contract C10 exercise
- Plus every `scan_go*.rs`, `scan_cargo*.rs`, `scan_python*.rs`, `scan_npm*.rs` that exercises the walker indirectly

Byte-identity is the guardrail.
