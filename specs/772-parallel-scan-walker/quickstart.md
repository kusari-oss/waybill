# Quickstart — m772 parallel scan_fs walker

**Feature**: 772-parallel-scan-walker
**Status**: Complete
**Date**: 2026-09-04

Copy-paste recipes to validate the milestone after implementation lands.

---

## Prerequisite

```sh
# Reference class: macOS aarch64, ≥ 8 logical CPUs, warm cache
sw_vers
sysctl hw.logicalcpu                 # ≥ 8

# Fixture: Kubernetes at the pinned public sandbox
mkdir -p /tmp/perf-sweep
cd /tmp/perf-sweep
git clone --depth 1 https://github.com/kusari-sandbox/test-kubernetes k8s
du -sh k8s                           # ~380 MB
find k8s -name go.mod -not -path "*/vendor/*" -not -path "*/node_modules/*" | wc -l  # 39
```

Warm the filesystem cache once:
```sh
find /tmp/perf-sweep/k8s -type f > /dev/null 2>&1
```

---

## Validate SC-001 (Kubernetes wall-time)

**Target**:
- Default scan wall time ≤ 22 s (from 33.4s post-m771 baseline)
- Walker-isolated (`--no-go-mod-why`) ≤ 5 s (from 18.7s)

```sh
WAYBILL=$(command -v waybill)                              # or /path/to/target/release/waybill

# Warmup scan (discard output).
$WAYBILL --offline sbom scan --path /tmp/perf-sweep/k8s --no-deep-hash \
    --format cyclonedx-json --output /tmp/warm.cdx.json >/dev/null 2>&1

# Measurement A — walker-isolated
time $WAYBILL --offline --no-go-mod-why sbom scan --path /tmp/perf-sweep/k8s \
    --no-deep-hash --format cyclonedx-json --output /tmp/walker.cdx.json 2>/tmp/walker.log

# Measurement B — full default
time $WAYBILL --offline sbom scan --path /tmp/perf-sweep/k8s \
    --no-deep-hash --format cyclonedx-json --output /tmp/default.cdx.json 2>/tmp/default.log
```

**Expected**:
- Measurement A `real` ≤ 5s
- Measurement B `real` ≤ 22s
- Measurement A CPU utilization > 300% (parallel workers active)

---

## Validate SC-002 (byte-identity across every existing fixture)

```sh
cd /Users/mlieberman/Projects/mikebom

# Regression suite
cargo test -p waybill --no-fail-fast \
    --test scan_go --test scan_cargo --test scan_python --test scan_npm \
    --test cdx_regression --test spdx_regression --test spdx3_regression \
    --test walk_registry_integration --test exclude_path_walker_pilot
```

**Expected**: every test binary reports `test result: ok. N passed; 0 failed`.

If any test fails, the parallel walker is emitting entries in a different order than the emitters can tolerate — check FR-007 sort-at-end implementation.

---

## Validate SC-003 (zero new Cargo deps)

```sh
git diff --stat Cargo.lock waybill-cli/Cargo.toml waybill-common/Cargo.toml xtask/Cargo.toml
```

**Expected**: zero lines added to any `[dependencies]` block; `Cargo.lock` diff empty.

---

## Validate SC-004 (deterministic emit order across runs)

```sh
# Run the default scan twice; compare byte-for-byte with runtime-random
# fields (serialNumber, created) masked.
WAYBILL=$(command -v waybill)
$WAYBILL --offline sbom scan --path /tmp/perf-sweep/k8s --no-deep-hash \
    --format cyclonedx-json --output /tmp/run1.cdx.json >/dev/null 2>&1
$WAYBILL --offline sbom scan --path /tmp/perf-sweep/k8s --no-deep-hash \
    --format cyclonedx-json --output /tmp/run2.cdx.json >/dev/null 2>&1

# Mask the runtime-random fields (already random pre-milestone).
mask() {
  sed -E \
    -e 's|"serialNumber": "urn:uuid:[0-9a-f-]+"|"serialNumber": "MASKED"|g' \
    -e 's|"created": "[^"]+"|"created": "MASKED"|g'
}
diff <(mask < /tmp/run1.cdx.json) <(mask < /tmp/run2.cdx.json)
```

**Expected**: zero diff. Parallel discovery MUST NOT produce different component orderings across runs.

---

## Validate SC-005 (symlink-loop safety)

```sh
# Existing m054 fixture — should terminate promptly.
cargo test -p waybill --bin waybill \
    scan_fs::package_db::golang::go_binary::tests::walks_symlink_loop_without_hanging
```

**Expected**: test passes in < 1 second (unchanged from pre-milestone).

New cross-subtree symlink-loop test (added by this milestone):
```sh
cargo test -p waybill --test walker_parallelism_772 \
    m772_cross_subtree_symlink_loop_terminates
```

**Expected**: test passes in < 1 second.

---

## Validate SC-006 (serial fallback on tiny trees)

```sh
# Small fixture: m771's mod_why_scaling has only 4 dirs + a handful of files.
WAYBILL=$(command -v waybill)
time RUST_LOG=info $WAYBILL --offline sbom scan \
    --path /Users/mlieberman/Projects/mikebom/waybill-cli/tests/fixtures/golang/mod_why_scaling \
    --no-deep-hash --format cyclonedx-json --output /tmp/small.cdx.json 2>/tmp/small.log
```

**Expected**: CPU utilization ≤ 100% (single-threaded serial path); INFO log contains a "walker: serial fallback" or equivalent line naming the trigger (worker count == 1 OR rootfs has < 2 subdirs).

---

## m669 baseline refresh (post-merge)

```sh
cd /Users/mlieberman/Projects/mikebom
cargo run -p xtask -- bench --update-baseline
git diff docs/perf/baseline.json
```

Commit the baseline update in a separate polish PR so future regressions get caught by the m669 preflight-check.

---

## Common failure signatures + remediation

| Observed | Likely cause | Fix |
|---|---|---|
| Wall time unchanged after implementation | Parallel path not triggered — check `worker_count > 1` gate | Verify `available_parallelism()` and initial subdir count in `should_parallelize()` |
| Byte-identity regression in cdx_regression | Emit order changed | Confirm FR-007 sort-at-end runs before returning from `SharedWalker::run` |
| Symlink-loop test hangs | Visited-set not shared across workers | Confirm `Arc<Mutex<HashSet>>` shape + check-and-insert atomicity |
| Worker panic silently absorbed | `JoinHandle::join()` result ignored | Grep for `.join()` result handling; wire to `tracing::error!` + return-error |
| Random-run SC-004 diff | Sort not applied per-reader OR sort key unstable | Verify sort key is deterministic (PURL / source_path / name tiebreak chain) |
| Deadlock during drain | `active_workers` counter not paired with pop/finish | Grep for `active_workers.fetch_add` / `fetch_sub` — every pop MUST pair with a decrement even on early-return paths |
