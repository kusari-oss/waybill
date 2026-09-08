# Quickstart — m773 parallel graph_resolver

**Feature**: 773-graph-resolver-parallel
**Status**: Complete
**Date**: 2026-09-05

Copy-paste recipes to validate the milestone after implementation lands.

---

## Prerequisite

```sh
# Reference class: macOS aarch64, ≥ 8 logical CPUs, warm cache
sw_vers
sysctl hw.logicalcpu                # ≥ 8

# Fixture: Kubernetes at the pinned public sandbox
mkdir -p /tmp/perf-sweep
cd /tmp/perf-sweep
git clone --depth 1 https://github.com/kusari-sandbox/test-kubernetes k8s
du -sh k8s                          # ~380 MB
find k8s -name go.mod -not -path "*/vendor/*" -not -path "*/node_modules/*" | wc -l  # 39
ls k8s/go.work                      # exists
```

Warm the filesystem cache once:
```sh
find /tmp/perf-sweep/k8s -type f > /dev/null 2>&1
```

---

## Validate SC-001 (Kubernetes wall-time)

**Targets**:
- Default scan wall time ≤ 20 s (from 34s post-m771)
- Walker-isolated (`--no-go-mod-why`) ≤ 8 s (from 19s)

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
- Measurement A `real` ≤ 8s
- Measurement B `real` ≤ 20s
- Measurement A `user` time > `real` time (concurrency working; on 8-core: `user` ~3-5× `real`)

---

## Validate SC-005 (per-workspace summary log preserved)

```sh
grep -c "go transitive edges resolution summary" /tmp/walker.log
```

**Expected**: exactly 38 (matches Kubernetes workspace count; one line per workspace analyzed).

```sh
# Field-shape check — every line has the same fields as pre-milestone.
grep "go transitive edges resolution summary" /tmp/walker.log | head -1
```

**Expected**: line contains `total_modules=N graph_count=N cache_count=N proxy_count=N gosum_count=N unresolved_count=N coverage=STR` — exact fields per FR-007 wire-shape.

---

## Validate SC-002 (byte-identity across every existing fixture)

```sh
cd /Users/mlieberman/Projects/mikebom

# Regression suite
cargo test -p waybill --no-fail-fast \
    --test scan_go --test scan_cargo --test scan_python --test scan_npm \
    --test cdx_regression --test spdx_regression --test spdx3_regression \
    --test walk_registry_integration --test golang_transitive_edges_kubernetes_smoke 2>&1 | \
    grep -E "^test result|FAILED"
```

**Expected**: every test binary reports `test result: ok. N passed; 0 failed`.

If any test fails, the parallel path is emitting entries in a different order than the reduce preserves — check FR-004 workspace_index-ordered reduce implementation.

---

## Validate SC-003 (zero new Cargo deps)

```sh
git diff --stat Cargo.lock waybill-cli/Cargo.toml waybill-common/Cargo.toml xtask/Cargo.toml
```

**Expected**: zero lines added to any `[dependencies]` block; `Cargo.lock` diff empty.

---

## Validate SC-004 (deterministic emit order across runs)

```sh
WAYBILL=$(command -v waybill)
# Run the default scan twice; compare byte-for-byte with runtime-random
# fields (serialNumber, created) masked.
$WAYBILL --offline sbom scan --path /tmp/perf-sweep/k8s --no-deep-hash \
    --format cyclonedx-json --output /tmp/run1.cdx.json >/dev/null 2>&1
$WAYBILL --offline sbom scan --path /tmp/perf-sweep/k8s --no-deep-hash \
    --format cyclonedx-json --output /tmp/run2.cdx.json >/dev/null 2>&1

# Mask the runtime-random fields.
mask() {
  sed -E \
    -e 's|"serialNumber": "urn:uuid:[0-9a-f-]+"|"serialNumber": "MASKED"|g' \
    -e 's|"created": "[^"]+"|"created": "MASKED"|g'
}
diff <(mask < /tmp/run1.cdx.json) <(mask < /tmp/run2.cdx.json)
```

**Expected**: zero diff. Parallel resolution MUST NOT produce different component orderings across runs.

---

## Validate SC-006 (`--no-go-mod-why` regression pin)

```sh
WAYBILL=$(command -v waybill)
# Run against every existing Go fixture with --no-go-mod-why set.
for f in /Users/mlieberman/Projects/mikebom/waybill-cli/tests/fixtures/golang/*/; do
  name=$(basename "$f")
  # Skip the m771 test fixture, which is synthetic-only.
  $WAYBILL --no-go-mod-why --offline sbom scan --path "$f" --no-deep-hash \
      --format cyclonedx-json --output "/tmp/nogmw-$name.cdx.json" >/dev/null 2>&1 || true
done
```

Compare each `/tmp/nogmw-*.cdx.json` against the pre-milestone `--no-go-mod-why` output. Expected: byte-identical (this flag runs the resolver but skips the classifier; the resolver code path is what m773 changes).

---

## m669 benchmark harness integration (post-merge)

```sh
cd /Users/mlieberman/Projects/mikebom
cargo run -p xtask -- bench --update-baseline
git diff docs/perf/baseline.json
```

Commit the baseline update in a **separate polish PR** so future regression bisects can attribute wall-time shifts cleanly.

---

## Common failure signatures + remediation

| Observed | Likely cause | Fix |
|---|---|---|
| Wall time unchanged after implementation | Parallel path not triggered — check `worker_count >= 2 && workspace_count >= 2` gate | Verify `mod_why::worker_count()` return value and workspace enumeration count |
| Byte-identity regression on scan_go / cdx_regression | Reduce not in workspace_index order | Check `results[workspace_index]` slot assignment + reduce iterates `0..N` in ascending order |
| Random-run SC-004 diff | Worker completion order leaking into signals aggregation | Confirm Phase 2 reduce iterates in index order, not `rx.recv()` order |
| `go transitive edges resolution summary` line count wrong | Worker died mid-analysis and panic wasn't propagated | Grep `walker.log` for `tracing::error!` about worker panics; check `ScopedJoinHandle::join()` result inspection |
| Compile error "GoModCache doesn't implement Sync" | Someone added a `!Sync` field to GoModCache | Grep for recent GoModCache changes; either revert the offending field OR wrap in `Arc<Mutex<>>` |
| `--no-go-mod-why` output differs from pre-milestone | Classifier code path accidentally affected by resolver refactor | Confirm `main.rs:330` early-return still short-circuits before `apply_go_mod_why_pass` |
| Deadlock during reduce | Main thread holding a lock while workers try to send | Confirm `drop(tx)` happens BEFORE `for result in rx { ... }` loop (main-side sender dropped so rx eventually returns Err) |
