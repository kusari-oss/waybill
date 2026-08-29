# Quickstart: benchmarking waybill

**Audience**: waybill maintainer wanting to (a) measure how fast waybill scans something, (b) refresh the shipped baseline before a release, or (c) reproduce a docs-cited number on their own hardware.

## 5-step operator recipe

### Step 1 — Prerequisites

- Waybill main-repo checkout on a workstation with `cargo` + `git` installed
- Linux/macOS/Windows all supported for local runs; docs-cited numbers use the Linux x86_64 GitHub-hosted-runner class as reference

### Step 2 — First benchmark run

```bash
cd /path/to/waybill
cargo run -p xtask -- bench
```

**What happens**:
1. If the fixture cache is missing, `build.rs` fetches the pinned fixture-set into `~/.cache/waybill/fixtures/<sha>/` (~60s on first run per SC-007)
2. Warmup pass for each fixture-mode combination (untimed)
3. 5 timed samples per fixture-mode combination
4. Median-of-5 per dimension recorded
5. Output written to `target/bench/run-<git-sha>.json`
6. Markdown table printed to stdout

**Typical duration**: ~90 min on the reference-architecture runner (SC-008 ceiling); ~30-60 min on a fast local workstation with cache warm.

**Expected output shape** (stdout Markdown):

```text
| Fixture | Mode | Wall-clock (ms) | Peak RSS (KB) | Output bytes | Components | Status |
|---|---|---|---|---|---|---|
| cargo-workspace-medium | default | 1523 | 47280 | 82734 | 234 | success |
| cargo-workspace-medium | no-deep-hash | 981 | 46102 | 82734 | 234 | success |
| npm-monorepo-medium | default | 3421 | 61230 | 158420 | 892 | success |
| ...
```

### Step 3 — Reproduce a docs-cited number

Every quoted number in `docs/perf/numbers.md` is accompanied by both a fixture-SHA and a waybill commit SHA. To reproduce:

```bash
# Check out the cited waybill commit
git checkout <waybill-commit-sha>

# Set the fixture-SHA env var (build.rs uses it as cache key)
export WAYBILL_FIXTURES_SHA=<fixture-sha>

# Run just the fixture in question
cargo run -p xtask -- bench --filter <fixture-name>
```

**Expected outcome**: your reproduced median wall-clock is within 25% of the docs-cited number (SC-002 noise budget). If not on the reference-architecture runner class, expect a wider gap.

### Step 4 — Refresh the baseline before a release

The release-prep flow's pre-flight check (contract `xtask-bench-cli.md` C-5) will fail if the shipped baseline is stale relative to HEAD's waybill-runtime code paths. Refresh:

```bash
cargo run -p xtask -- bench --update-baseline

# Verify the diff makes sense (regressions or improvements should have obvious source)
git diff docs/perf/baseline.json

# Also refresh the operator-facing numbers page (US3 auto-derivation)
cargo run -p xtask -- bench-docs

# Commit both together
git add docs/perf/baseline.json docs/perf/numbers.md
git commit -m "release: refresh perf baseline for vX.Y.Z"
```

**Total cost**: one full 90-minute benchmark run + ~10s of docs generation. Do this in a pre-release-prep session, not during the release-prep step itself.

### Step 5 — Cite a benchmark result in an external context (Slack, issue comment, etc.)

Every quoted number in an external context MUST cite:
- The fixture-SHA (from `metadata.fixture_sha` in the run.json)
- The waybill commit SHA (from `metadata.waybill_commit_sha`)
- The runner class (from `metadata.noise_class` — Reference / Noisy / Other)

**Wrong** (non-reproducible):
> waybill scans a Cargo workspace in 1.5 seconds

**Right** (reproducible):
> waybill @ commit `abc1234` scans the `cargo-workspace-medium` fixture (@ fixture SHA `def5678`) in ~1.5 seconds on the Linux x86_64 reference runner class (median-of-5).

For docs-cited numbers, `xtask bench-docs` produces the right shape automatically.

## Verifying advanced scenarios

### Scenario A: Regression sniff before opening a PR

```bash
# On your feature branch:
cargo run -p xtask -- bench --baseline docs/perf/baseline.json
# Exits non-zero if any fixture-mode combination regressed ≥25% vs the shipped baseline
```

**What to do if the exit is non-zero**: read the `target/bench/regression-diff-<sha>.json` — it names the specific fixture-mode-dimension tuple(s) that regressed. Fix the regression, or if the slowdown is intentional (rare, e.g., adding accuracy comes with cost), update the baseline in the same PR.

### Scenario B: Local capture-only mode (no comparison)

```bash
cargo run -p xtask -- bench
# Just captures numbers; no comparison; always exits 0 if all fixtures succeed
```

Useful for exploratory work: "how much did my caching PR speed up the npm-monorepo fixture?"

### Scenario C: Filter to a single ecosystem

```bash
cargo run -p xtask -- bench --filter cargo-*
# Runs only cargo-shaped fixtures. ~1 min total instead of ~90 min.
```

Useful for scoped-down local iteration.

## Troubleshooting

### `Fixture cache is missing and $WAYBILL_FIXTURES_SHA is unset`

The `build.rs` fixture-cache fetch normally handles this automatically when you invoke `xtask bench` fresh. If you see this error, ensure `cargo build -p xtask` runs to completion first (its `build.rs` fires the fetch as a side effect).

### `Perf baseline is stale`

This is C-5's diagnostic. The recovery command is printed inline:

```bash
cargo run -p xtask -- bench --update-baseline
```

Run it. Commit the updated `docs/perf/baseline.json`. Re-run release-prep.

### `sysinfo::Process::memory() returned 0 KB`

Two possible causes:
- Waybill child exited before the sampler took its first sample (rare — scan was <100ms). Fix: ignore or increase the fixture's warmup pass duration.
- Running under a restricted container that doesn't expose `/proc/<pid>/status` to the observer. Fix: run outside the container, or use `--per-fixture-timeout-sec` to skip the fixture.

### `The reproduced number is way off from the docs-cited number`

Check:
1. Are you on the same runner class? Docs numbers pin Linux x86_64 GitHub-hosted; your local Apple M3 will differ significantly.
2. Are you at the cited waybill commit + fixture-SHA?
3. Is `noise_class` in your metadata `Reference` or `Noisy`? Docs numbers are Reference-class.

## Reference

- Full data model: [`data-model.md`](./data-model.md)
- JSON schema contract: [`contracts/json-schema.md`](./contracts/json-schema.md)
- CLI contract: [`contracts/xtask-bench-cli.md`](./contracts/xtask-bench-cli.md)
- CI workflow contract: [`contracts/ci-workflow.md`](./contracts/ci-workflow.md)
- Research decisions: [`research.md`](./research.md)
- Original issue: [#328](https://github.com/kusari-oss/waybill/issues/328)
