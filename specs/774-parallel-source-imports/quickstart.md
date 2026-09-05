# Quickstart — m774 parallel source-import collection

**Audience**: reviewer + implementer verifying m774 acceptance locally after `/speckit.implement` completes.

**Duration**: ~15 min (5 min cargo build + 10 min end-to-end verification).

---

## Prerequisites

- Rust stable toolchain (`rustup toolchain install stable`; version pinned to workspace `rust-toolchain.toml`).
- `git` on PATH (standard dev prereq).
- A Go monorepo fixture. Recommended: `git clone --depth 1 https://github.com/kusari-sandbox/test-kubernetes.git /tmp/test-kubernetes` (39 go.mod files, matches the m774 reference-class fixture).
- Optional: `hyperfine` for wall-time comparison (`brew install hyperfine` / `cargo install hyperfine`).

---

## Step 1 — Build the release binary

```sh
cargo build -p waybill --release
```

Expected: `Finished release ... target(s)` in ~20s on warm cache. Binary at `target/release/waybill`.

---

## Step 2 — Verify SC-001 (wall-time targets)

### 2a. Walker-isolated (`--no-go-mod-why`) scan on `test-kubernetes`

```sh
time ./target/release/waybill --offline --no-go-mod-why sbom scan \
  --path /tmp/test-kubernetes \
  --no-deep-hash \
  --format cyclonedx-json \
  --output /tmp/m774-scan.cdx.json
```

Expected wall time on macOS aarch64 8-core warm cache: **≤ 10s** (pre-milestone baseline ~22.5s).

### 2b. Default scan on `test-kubernetes`

```sh
time ./target/release/waybill --offline sbom scan \
  --path /tmp/test-kubernetes \
  --no-deep-hash \
  --format cyclonedx-json \
  --output /tmp/m774-scan-default.cdx.json
```

Expected wall time on same host: **≤ 18s** (pre-milestone baseline ~34s post-m771).

**If either target is missed**: capture the FR-014 summary log line (`RUST_LOG=info` on stderr) and check `elapsed_ms` for the parallel phase. If elapsed_ms is high (>4000), the parallelization isn't taking effect; if low but total wall is high, another phase has become dominant → re-run m774 profiling per `docs/development/perf-methodology.md`.

---

## Step 3 — Verify SC-002 (byte-identity across existing fixtures)

Run the full test suite:

```sh
./scripts/pre-pr.sh
```

Expected: `cargo +stable clippy --workspace --all-targets` zero errors, then `cargo +stable test --workspace` every suite `N passed; 0 failed`. Byte-identity is enforced by:

- `cdx_regression*.rs` — CDX golden byte comparison.
- `spdx_regression*.rs` — SPDX 2.3 golden.
- `spdx3_regression*.rs` — SPDX 3 golden.
- `golang_transitive_*` — Go transitive-edge regression.
- `scan_go_*` — end-to-end Go reader coverage.
- The m669 corpus harness at `waybill-cli/tests/corpus_harness_195/`.

**If any golden fails**: STOP. Do NOT regenerate the golden without investigation. Byte-identity is the primary correctness guarantee for this milestone (SC-002). A golden diff means the set-union merge is producing content-different output, which per research R5 would be a genuine bug (not an ordering artifact).

Diagnostic recipe (from memory `feedback_verify_golden_churn_normalized`): mask content-addressed IDs (`rel-`, `anno-`) and `LC_ALL=C sort` before diffing to distinguish semantic diffs from ordering churn.

---

## Step 4 — Verify SC-004 (determinism across independent runs)

```sh
./target/release/waybill --offline sbom scan --path /tmp/test-kubernetes \
  --no-deep-hash --format cyclonedx-json --output /tmp/run-a.cdx.json
./target/release/waybill --offline sbom scan --path /tmp/test-kubernetes \
  --no-deep-hash --format cyclonedx-json --output /tmp/run-b.cdx.json

# Mask serialNumber + created before diffing (m669 protocol).
jq 'del(.serialNumber) | del(.metadata.timestamp)' /tmp/run-a.cdx.json > /tmp/run-a.masked.json
jq 'del(.serialNumber) | del(.metadata.timestamp)' /tmp/run-b.cdx.json > /tmp/run-b.masked.json
diff /tmp/run-a.masked.json /tmp/run-b.masked.json
```

Expected: zero output from `diff`. If diff surfaces: `signals.production_imports` or `signals.test_only_imports` iteration order is leaking into the SBOM — either the reduce is order-sensitive (per R5, downstream consumers should not be), or a new consumer has been introduced. Investigate downstream code before shipping.

---

## Step 5 — Verify SC-005 (single-workspace zero degenerate overhead)

**Pre-check** — confirm the fixture yields `parsed_roots.len() == 1` at reader level. The `go-module-medium` fixture has a `vendor/` tree with ~35 nested `go.mod` files, but per Go module semantics the vendor directory is per-workspace and should NOT enumerate as separate `parsed_roots`. Run once with `RUST_LOG=info` and grep the log:

```sh
RUST_LOG=info ./target/release/waybill --offline sbom scan \
  --path ~/Projects/waybill-test-fixtures/benchmark/source-tier/go-module-medium \
  --no-deep-hash --format cyclonedx-json --output /dev/null 2>/tmp/gomm.log
grep "m774 parallel source-import collection complete" /tmp/gomm.log
```

The summary log MUST show `workspaces_scanned=1` (SC-005's single-workspace precondition). If it shows > 1, this fixture exercises the multi-workspace parallel path and CANNOT be used for SC-005 verification — fall back to the synthetic 1-workspace helper `make_one_workspace_fixture()` from `waybill-cli/tests/collect_imports_parallel_774.rs` and re-run the hyperfine benchmark against it via a `cargo bench`-style scratch harness, OR use the m669 bench harness's `waybill-cli/tests/perf_*` infrastructure directly.

**Benchmark** (only after pre-check confirms `workspaces_scanned=1`):

```sh
hyperfine --warmup 3 --runs 10 \
  "./target/release/waybill --offline sbom scan \
    --path ~/Projects/waybill-test-fixtures/benchmark/source-tier/go-module-medium \
    --no-deep-hash --format cyclonedx-json --output /dev/null"
```

Expected p50 delta vs pre-milestone binary (`/tmp/mikebom-main/target/release/waybill` from T002's worktree): **≤ ±3%**. If overhead exceeds ±3%, the degenerate short-circuit at `parsed_roots.len() <= 1` is not firing correctly (see research R9).

---

## Step 6 — Verify FR-014 summary log

```sh
RUST_LOG=info ./target/release/waybill --offline sbom scan \
  --path /tmp/test-kubernetes \
  --no-deep-hash \
  --format cyclonedx-json \
  --output /tmp/m774-log-check.cdx.json 2>/tmp/m774.log

grep "m774 parallel source-import collection complete" /tmp/m774.log
```

Expected: exactly one matching line with fields `workspaces_scanned`, `parallel_workers_used`, `production_imports_count`, `test_imports_count`, `elapsed_ms`. Field values MUST be non-zero (except `test_imports_count` which MAY be zero on fixtures without `_test.go` files).

Example expected output:

```
INFO waybill::scan_fs::package_db::golang::legacy:
  m774 parallel source-import collection complete
  workspaces_scanned=38 parallel_workers_used=8
  production_imports_count=847 test_imports_count=312 elapsed_ms=2145
```

---

## Step 7 — Run the new integration test suite

```sh
cargo +stable test --workspace --test collect_imports_parallel_774
```

Expected: all tests pass, output line `test result: ok. N passed; 0 failed`. Test coverage:
- `m774_multi_workspace_merge_correctness` — 3-workspace fixture, assert union content matches sum of per-workspace outputs.
- `m774_determinism_across_runs` — 3-workspace fixture, run twice, assert byte-identity.
- `m774_worker_panic_fails_fast` — inject panic, assert scan exits non-zero + `tracing::error!` line captured.
- `m774_single_workspace_no_thread_spawn` — 1-workspace fixture, assert wall-time delta ≤ ±3% (compared to a pre-milestone baseline captured under `#[ignore]` for local reference).
- `m774_summary_log_fires_once_per_read` — capture tracing, assert exactly one summary line per `pub fn read`.

---

## Troubleshooting

**Symptom**: `pre-pr.sh` fails at `cargo clippy` with `unused import` warnings on `Arc`, `Mutex`, or `mpsc`.
**Fix**: The single-workspace short-circuit path (research R9) doesn't use these types; make sure imports are gated to the parallel-path arm or use `#[allow(unused_imports)]` sparingly if the arm-shape forces it. Prefer keeping imports scoped to the parallel arm via `use` inside the block.

**Symptom**: `hyperfine` shows single-workspace overhead > 3%.
**Fix**: Trace through `pub fn read` — the degenerate arm at `parsed_roots.len() <= 1` MUST inline the two calls without spawning a `std::thread::scope` block. If the scope block is unconditionally entered, that's ~50-100μs of dead overhead per scan.

**Symptom**: Byte-identity fails on a specific golden (`cdx_regression::path_to_specific_test`).
**Fix**: Mask + sort per memory `feedback_verify_golden_churn_normalized`. If sorted diff still shows semantic differences, the reduce is producing content-different `production_imports` or `test_only_imports` sets — inspect worker output for missing/extra entries; likely a bug in the merge loop (e.g., `.extend()` on the wrong set).

**Symptom**: Perf target missed on Linux CI even though local macOS meets it.
**Fix**: GitHub Actions `ubuntu-latest` runners have variable core counts (typically 4). Rerun with `--warmup 3 --runs 5` to get a stable p50; if 4-core wall-time is > SC-001 target, adjust SC-001 to reference the 4-core equivalent (~40% higher upper bound). Do NOT hide the difference; document in the PR description.

---

## Rollback triggers

Per `docs/development/perf-methodology.md`, if `hyperfine` on `test-kubernetes` shows the parallel phase completes in ≤ 3s BUT total scan wall does NOT drop meaningfully (< 30% reduction from baseline), the log-line-as-cost-proxy pitfall may be recurring. Escalation:

1. Re-instrument with the `scratch-m774-profile` markers (or equivalent per-phase `tracing::info!` calls).
2. Re-run the profiling on the post-implementation binary.
3. If the parallel phase's cost has genuinely moved to another site (e.g., orphan backfill got slower under contention), roll back per the m772/m773 process:
   - `git revert` the m774 PR.
   - Update `docs/development/perf-methodology.md` with the new incident's lesson.
   - Update memory `feedback_perf_spec_needs_per_phase_decomposition` with the incident.

If the phase completes in ≤ 3s AND total wall drops as expected, ship it.
