# Contract: `xtask bench` + `xtask bench-docs` CLI surface

**Feature**: `669-bench-harness` | **Applies to**: `xtask/src/bench/mod.rs` argument parsing + subcommand behavior.

Contracts governing the CLI surface — invocable from repo root via `cargo run -p xtask -- <subcommand> <args>`.

## Interface

### `xtask bench` — run the benchmark suite

**Basic usage**:
```bash
cargo run -p xtask -- bench
```
Runs the full matrix, writes `target/bench/run-<git-sha>.json`, prints a Markdown table to stdout, exits 0 on success.

**Flags** (all optional):
- `--filter <pattern>` — glob-match fixture names (FR-006). Multiple `--filter` flags = union of matches.
- `--baseline <path>` — compute a RegressionDiff against the named baseline file. Exits non-zero if any dimension breach ≥ threshold. Default: no comparison (FR-020 capture-only mode).
- `--threshold <fraction>` — override the default 0.25 regression threshold. Only accepted with `--baseline`.
- `--output <path>` — override the default `target/bench/run-<git-sha>.json` location.
- `--fixtures-dir <path>` — override the fixture-cache location. Defaults to `$WAYBILL_FIXTURES_DIR` (m090 env), then `~/.cache/waybill/fixtures/<pinned-sha>/`.
- `--per-fixture-timeout-sec <N>` — override the default 5-minute per-fixture timeout (Q3). Accepted range: 60-3600.
- `--update-baseline` — write the run output to `docs/perf/baseline.json` instead of the default `target/bench/run-<sha>.json` location. Atomic write per contract `json-schema.md` C-6.
- `--preflight-check` — do NOT run the benchmark. Instead, read `docs/perf/baseline.json`, extract its `waybill_commit_sha`, run the R7 `git diff` staleness check, exit non-zero with a recovery-command diagnostic if stale.

### `xtask bench-docs` — regenerate the docs numbers page

**Basic usage**:
```bash
cargo run -p xtask -- bench-docs
```
Reads `docs/perf/baseline.json`, writes `docs/perf/numbers.md`, exits 0.

**Flags**:
- `--baseline <path>` — read from a non-default baseline location.
- `--output <path>` — write to a non-default output location.
- `--dry-run` — print what would be written, don't touch disk.

## Behavioral contracts

### C-1: Default matrix runs all fixtures × all modes

`xtask bench` with no flags iterates every fixture in the fixtures-repo `manifest.json`, and for each fixture, iterates every mode in that fixture's `supported_modes`. Emits ~70 Results per run (14 fixtures × 5 modes average).

### C-2: `--filter` short-circuits the matrix

`--filter cargo-*` selects only fixtures whose `name` matches the glob. Exit-code is 0 even if the filter matches zero fixtures (empty-set is valid; not an error).

### C-3: `--baseline` produces a RegressionDiff output

When `--baseline <path>` is present, the run's ordinary Result-emission is followed by a RegressionDiff computation (per data-model.md `RegressionDiff` shape). The diff is:
1. Written to `target/bench/regression-diff-<sha>.json`
2. Printed to stdout as a Markdown table
3. If non-empty regressions found: process exits with code 1 (fail-closed per FR-010).

### C-4: Every Result has BOTH SHAs at emission (contract json-schema.md C-4)

Assert at Result-construction time — refuses to emit a Result whose `waybill_commit_sha` or `fixture_sha` is empty. Panic with a clear error message; do NOT silently write malformed data.

### C-5: `--preflight-check` embodies the R7 staleness algorithm (Q2)

The pre-flight check MUST:
1. Refuse to run if `docs/perf/baseline.json` is missing (initial-baseline bootstrap case is handled by `--update-baseline`, not pre-flight).
2. Extract `metadata.waybill_commit_sha` from the baseline JSON.
3. Run `git diff --stat <baseline-sha>..HEAD -- 'waybill-cli/**' 'waybill-common/**' 'waybill-ebpf/**' Cargo.lock` — SC-006-scoped exactly.
4. If the diff is non-empty: exit 1, print the diagnostic:
   ```text
   Perf baseline is stale.
   Baseline was captured at <baseline-sha>; HEAD is <head-sha>.
   The following waybill-runtime files changed since:
     <first-10-lines-of-git-diff>
   Refresh the baseline before releasing:
     $ cargo run -p xtask -- bench --update-baseline
     $ git add docs/perf/baseline.json
     $ git commit -m "release: refresh perf baseline"
   ```
5. If the diff is empty: exit 0 silently.

The release-prep flow (m229) invokes `xtask bench --preflight-check` before opening the release PR. Failure blocks release-prep with the recovery command already printed.

### C-6: `--update-baseline` writes to the fixed docs/perf/baseline.json path

No override — the baseline path is a fixed location per contract `ci-workflow.md` C-1. `--update-baseline` is meaningfully "commit the current run as the new baseline"; letting operators point it elsewhere invites drift.

### C-7: `bench-docs` produces a fully-derived Markdown file

The `numbers.md` output is a pure function of `baseline.json`. Given the same baseline input, `bench-docs` MUST produce byte-identical output. Enforced by contract-verifiable diff: `xtask bench-docs && git diff docs/perf/numbers.md` on a clean checkout MUST return zero lines (SC-005 anchor).

### C-8: Per-run scratch dirs (test isolation, Principle VII)

Each fixture-mode's warmup + 5 timed samples share a single `tempfile::tempdir()` for waybill's output paths. Cross-fixture invocations use fresh tempdirs. No shared bench-run state between fixtures or between runs.

## Non-contracts

- **CI-vs-local mode distinction**: there is no `--ci-mode` flag. The tool's behavior is identical in both contexts; the CI workflow imposes fail-closed semantics via `exit 1` propagation, not via a mode switch.
- **Parallel fixture execution**: v1 runs fixtures sequentially. Parallelizing risks cross-fixture RSS contention and inflated wall-clocks; deferred to v2 if needed.
- **JSONL vs JSON output**: v1 emits one JSON file per run (not JSONL). Streamable multi-run capture is a v2 concern.

## Test-authoring rules

- **T1**: `xtask/tests/cli_flag_parsing.rs` — asserts every flag documented above parses via clap without panic + rejects invalid values (e.g., `--per-fixture-timeout-sec 0` fails validation).
- **T2**: `xtask/tests/preflight_check_stale.rs` — plants a `baseline.json` with an old SHA, runs `--preflight-check` in a subprocess, asserts non-zero exit + the diagnostic text C-5 requires.
- **T3**: `xtask/tests/preflight_check_current.rs` — writes a baseline with `metadata.waybill_commit_sha` = current HEAD, runs `--preflight-check`, asserts exit 0.
- **T4**: `xtask/tests/docs_generation_deterministic.rs` — runs `bench-docs` twice against the same baseline, asserts byte-identical output.
