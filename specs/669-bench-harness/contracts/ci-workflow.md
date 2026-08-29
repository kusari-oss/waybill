# Contract: CI workflow (`.github/workflows/bench.yml`)

**Feature**: `669-bench-harness` | **Applies to**: `.github/workflows/bench.yml`

The bench workflow runs the benchmark suite on every release tag and on a weekly cron. Contracts govern the workflow's shape, triggers, regression-detection semantics, and PR-comment mechanism.

## Interface

### Triggers

- `push:` on tags matching `v[0-9]+.[0-9]+.[0-9]+` (stable release tags — nightly tags `-nightly.YYYYMMDD` are excluded from bench cadence per SC-008 budget)
- `schedule:` weekly cron at `0 6 * * 0` (Sunday 06:00 UTC — off-peak)
- `workflow_dispatch:` for manual invocation with optional `--filter` input

### Outputs

- `target/bench/run-<sha>.json` uploaded as workflow artifact
- `target/bench/regression-diff-<sha>.json` uploaded as workflow artifact (if `--baseline` was supplied)
- PR comment posted/edited on the release PR (release-tag runs only)
- Non-zero exit code from the `xtask bench --baseline` step propagates to workflow failure (release-tag runs only)

## Behavioral contracts

### C-1: Baseline path is fixed at `docs/perf/baseline.json`

The workflow reads the baseline from this exact path. No workflow parameter or env-var override. Fixed path matches contract `xtask-bench-cli.md` C-6.

### C-2: SHA-pinned actions only (per memory `feedback_sha_pin_before_dependabot`)

Every `uses:` entry in `bench.yml` MUST reference a 40-char commit SHA, not a tag. Dependabot's `github-actions` ecosystem entry catches SHA drift and files bump PRs weekly.

### C-3: Fail-closed bench step

The `xtask bench --baseline docs/perf/baseline.json` step MUST NOT set `continue-on-error: true`. Regression detection fails the workflow; failure blocks the release-tag processing (upstream release.yml step ordering ensures bench.yml runs before the artifact-publish step).

### C-4: Runner class is `ubuntu-latest`

Per spec Assumption's reference-architecture anchor (Linux x86_64 GitHub-hosted-runner class). The workflow MUST NOT run on `macos-latest` (known-noisy per m094) or `windows-latest` (not part of docs-numbers-page reference class). Local benchmarks by maintainers may use other classes; the CI workflow is pinned to `ubuntu-latest` so cross-release numbers stay comparable.

### C-5: Fixture cache warmed via existing m090 build.rs mechanism

The workflow's `Setup fixtures` step runs `cargo build -p xtask` (which triggers m090's `build.rs` fixture-cache fetch as a side effect). Cache lives at `~/.cache/waybill/fixtures/<sha>/`. Cache-hit runs skip fetch; cache-miss runs pay ~60s per SC-007.

### C-6: Regression comparison happens post-run

Sequence:
1. `xtask bench` — runs the suite, writes `run-<sha>.json`
2. `xtask bench --baseline docs/perf/baseline.json --output <existing-run-json>` — computes RegressionDiff, exits 1 on regression
3. Upload artifacts regardless of exit status
4. Post/edit PR comment (release-tag only) — see C-7

### C-7: PR comment via magic-marker edit-in-place (FR-018)

The final workflow step invokes `actions/github-script@<SHA>` with a JavaScript block that:
1. Reads `target/bench/regression-diff-<sha>.json`
2. Formats a Markdown table listing regressions + improvements + matrix asymmetry (per data-model.md `RegressionDiff` shape)
3. Prepends the magic marker `<!-- bench-regression-comment-v1 -->` to the comment body
4. Queries the PR's comment list via GitHub API (`GET /repos/{owner}/{repo}/issues/{issue_number}/comments`)
5. Finds the first comment whose body starts with the magic marker
6. If found: `PATCH` the existing comment body
7. If not found: `POST` a new comment
Result: exactly one bench-regression comment per PR, updated in-place on subsequent runs.

## Non-contracts

- **Runs on non-release commits**: NOT covered by v1. Regular PR commits don't trigger bench.yml. Adding per-PR bench runs is a v2 concern (would 10x the CI budget).
- **Multi-arch benchmarks**: v1 pins reference architecture to Linux x86_64. macOS and Windows local runs are supported but excluded from CI cadence.
- **eBPF trace benchmarks**: excluded per Q1 clarification.

## Job-graph diff summary

New file: `.github/workflows/bench.yml`.

**Steps** (in order):
1. `actions/checkout@<SHA>` — checkout waybill main repo at the release tag
2. `dtolnay/rust-toolchain@<SHA>` — install stable Rust
3. `Swatinem/rust-cache@<SHA>` — restore cargo build cache
4. `cargo build -p xtask` — build the driver (triggers m090's fixture-cache fetch)
5. `cargo build --release -p waybill` — build the tool-under-test
6. `cargo run -p xtask -- bench` — run the suite
7. `cargo run -p xtask -- bench --baseline docs/perf/baseline.json --output target/bench/run-*.json` — compute regression diff
8. `actions/upload-artifact@<SHA>` — upload run.json + regression-diff.json
9. `actions/github-script@<SHA>` — post/edit PR comment (release-tag runs only; skipped for scheduled runs)

No other workflows are modified.

## Test-authoring rules

- **T1**: YAML syntax validation via `python3 -c "import yaml; yaml.safe_load(...)"`
- **T2**: SHA-pin grep gate: `grep -nE "uses:.*@[a-f0-9]{40}" .github/workflows/bench.yml` matches every `uses:` line; zero `uses:.*@v[0-9]` matches
- **T3**: fail-closed grep: `grep -B2 continue-on-error .github/workflows/bench.yml | grep xtask` returns empty (no continue-on-error on any xtask step)
- **T4**: workflow file dry-run via GitHub's own workflow-syntax API (`gh api /repos/{owner}/{repo}/actions/workflows/bench.yml`) — smoke check for parse errors post-PR
