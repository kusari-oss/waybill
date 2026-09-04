# Quickstart — m771 `go mod why` subprocess scaling

**Feature**: 771-gomodwhy-subprocess-scale
**Status**: Complete
**Date**: 2026-09-04

Copy-paste recipes to validate each user story after implementation lands.

---

## Prerequisite: benchmark host + fixture

```sh
# Reference class: macOS aarch64, ≥ 8 logical CPUs, warm cache.
sw_vers                              # macOS
sysctl hw.logicalcpu                 # ≥ 8
sysctl kern.argmax                   # confirms ≥ 128 KiB (macOS reports 1048576)

# Fixture: Kubernetes source tree at the pinned public sandbox.
mkdir -p /tmp/perf-sweep
cd /tmp/perf-sweep
git clone --depth 1 https://github.com/kusari-sandbox/test-kubernetes k8s
du -sh k8s                           # ~380 MB
find k8s -name go.mod -not -path "*/vendor/*" -not -path "*/node_modules/*" | wc -l  # 39
ls k8s/go.work                       # exists

# Ensure Go toolchain is available (waybill shells out to `go`).
go version                           # any go ≥ 1.18 supports go.work
```

**Warm the module cache once** — subsequent scans reuse it:

```sh
cd /tmp/perf-sweep/k8s && go mod download -x >/dev/null 2>&1 || true
```

---

## Validate US1 (CHUNK_SIZE bump)

**Goal**: Wall-time ≤ 30 seconds with default flags; every module the pre-fix scan classified still classified.

```sh
WAYBILL=$(command -v waybill)                          # or /path/to/target/release/waybill

# Warm-scan discard once
$WAYBILL --offline sbom scan --path /tmp/perf-sweep/k8s --no-deep-hash \
    --format cyclonedx-json --output /tmp/warm.cdx.json >/dev/null 2>&1

# Measurement
time $WAYBILL --offline sbom scan --path /tmp/perf-sweep/k8s --no-deep-hash \
    --format cyclonedx-json --output /tmp/us1.cdx.json 2>/tmp/us1.log
```

**Expected**:
- `real ≤ 30s`
- `analyzed=` in `/tmp/us1.log` ≥ 421 (v0.6.1 pre-fix baseline)
- `components: 817` (unchanged; verify with `jq '.components | length' /tmp/us1.cdx.json`)

**Verify per-workspace subprocess count fell**:
```sh
grep -c "go-mod-why chunk" /tmp/us1.log
```
Pre-fix count: ≥ 45 chunks total (13 chunks × 39 workspaces, capped by budget). Post-US1: ≤ 39 chunks total (1 chunk × 39 workspaces).

---

## Validate US1 + US2 (parallel workspaces)

**Goal**: Wall-time ≤ 15 seconds; concurrent per-workspace log lines carry a workspace identifier.

```sh
time $WAYBILL --offline sbom scan --path /tmp/perf-sweep/k8s --no-deep-hash \
    --format cyclonedx-json --output /tmp/us12.cdx.json 2>/tmp/us12.log
```

**Expected**:
- `real ≤ 15s`
- User time > wall time (concurrency working; on 8-core: `user` should be ~3-5× `real`)

**Verify log correlation**:
```sh
# Every warn/info line from the classifier should carry the workspace path.
grep -E "waybill::scan_fs::package_db::golang::mod_why" /tmp/us12.log | \
  grep -v "main_module="
# Expected output: empty (every line has main_module=)
```

**Verify subprocess concurrency cap**:
```sh
# Sample child process count during a fresh scan.
$WAYBILL --offline sbom scan --path /tmp/perf-sweep/k8s --no-deep-hash \
    --format cyclonedx-json --output /tmp/us12.cdx.json 2>/dev/null &
WAYBILL_PID=$!
while kill -0 $WAYBILL_PID 2>/dev/null; do
  pgrep -P $WAYBILL_PID go | wc -l
  sleep 0.2
done | sort -u
```
**Expected**: max value ≤ `sysctl -n hw.logicalcpu`. On 8-core: max ≤ 8.

---

## Validate US1 + US2 + US3 (shared preflight)

**Goal**: Wall-time ≤ 10 seconds; exactly one `go list all` per `go.work` scope.

```sh
time $WAYBILL --offline sbom scan --path /tmp/perf-sweep/k8s --no-deep-hash \
    --format cyclonedx-json --output /tmp/us123.cdx.json 2>/tmp/us123.log
```

**Expected**:
- `real ≤ 10s`
- `analyzed=` at least the sum of every workspace's module set (not capped by 421 pre-fix baseline; all 39 workspaces analyzed)
- Zero `skipped=budget-exhausted` in the summary log

**Verify preflight dedup**:
```sh
grep -c "go list all" /tmp/us123.log
```
**Expected**: 1 + (count of loose main-modules outside the go.work scope). k8s has 1 `go.work` scope + 3 loose main-modules under `hack/tools/*` → **≤ 4 preflights** (down from 39 pre-milestone).

---

## Byte-identity regression (SC-003, SC-006)

**Goal**: Existing Go fixtures produce identical CDX / SPDX output modulo version-string cascades.

```sh
cd /Users/mlieberman/Projects/mikebom
cargo +stable build -p waybill --release
export WAYBILL=$(pwd)/target/release/waybill

# Baseline: run against every Go fixture directory, capture output.
for f in waybill-cli/tests/fixtures/golang/*/; do
  name=$(basename "$f")
  $WAYBILL --offline sbom scan --path "$f" --no-deep-hash \
      --format cyclonedx-json --output "/tmp/golden-post-$name.cdx.json" >/dev/null 2>&1
done

# Compare to golden files (regenerate one time via /tmp/golden-pre-*.cdx.json vs post).
# Normalization protocol: mask content-addressed IDs + version strings + sort.
# Reuse the sed pipeline from feedback_verify_golden_churn_normalized.
```

**Regression pin for `--no-go-mod-why` (SC-006)**:

```sh
for f in waybill-cli/tests/fixtures/golang/*/; do
  name=$(basename "$f")
  $WAYBILL --no-go-mod-why --offline sbom scan --path "$f" --no-deep-hash \
      --format cyclonedx-json --output "/tmp/nogmw-post-$name.cdx.json" >/dev/null 2>&1
done
```

Compare `/tmp/nogmw-post-*.cdx.json` against pre-milestone `--no-go-mod-why` output. Expected: byte-identical (this flag skips the classifier entirely; the milestone's changes are inaccessible on this code path).

---

## Zero-new-Cargo-deps validation (SC-004)

```sh
cd /Users/mlieberman/Projects/mikebom
# Confirm Cargo.lock unchanged at the dep-tree level (modulo any pre-existing bumps).
git diff --stat Cargo.lock waybill-cli/Cargo.toml waybill-common/Cargo.toml xtask/Cargo.toml
```
**Expected**: zero lines added to any `Cargo.toml` `[dependencies]` block; `Cargo.lock` diff is empty (or trivially non-empty from parallel unrelated updates).

---

## m669 benchmark harness integration

Once implementation lands, update the perf baseline:

```sh
cargo run -p xtask -- bench --update-baseline
# Emits new timings for k8s target; baseline lands at docs/perf/baseline.json
```

Subsequent CI runs will regression-test against the new baseline via the m669 preflight-check.

---

## Common failure signatures + remediation

| Observed | Likely cause | Fix |
|---|---|---|
| Wall-time still > 30s after US1 | CHUNK_SIZE not applied to running binary | Rebuild release binary; confirm `waybill --version` reports post-milestone SHA |
| `analyzed=` stalled at 421 | US1 landed but budget still exhausted | Landing US2 required — one workspace consuming the budget still blocks siblings |
| Log lines lack `main_module=` field | US2 landed without FR-005 threading | Grep for `tracing::warn!\|tracing::info!` in `mod_why.rs`; ensure every emit carries `main_module = %<path>` |
| `go list all` invoked N times where N = workspace count | US3 landed but scope enumeration wrong | Check `GoWorkScope.members` list is populated; verify `parse_go_work` recognizes the `use ( ... )` block form vs. bare `use <dir>` |
| Byte-identity regression on `waybill-cli/tests/fixtures/golang/*` | Verdict ordering shifted | `parse_go_mod_why` returns a HashMap — ordering shouldn't affect output; check if a downstream reducer became order-dependent |
| Post-fix `--no-go-mod-why` output diverges | Classifier code was called even when flag set | Check `no_go_mod_why` early-return in `main.rs:330` still short-circuits before workspace enumeration |
