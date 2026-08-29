# Feature Specification: Persisted reproducible benchmark suite

**Feature Branch**: `669-bench-harness`
**Created**: 2026-08-29
**Status**: Draft
**Input**: User description: "closes #328 — solidify waybill's performance benchmarks into a persisted, reproducible test suite"

## Clarifications

### Session 2026-08-29

- Q: Does m669 v1 include benchmarking of `waybill trace` mode, or defer to a follow-up feature? → A: Defer to follow-up (Option A). v1 covers `sbom scan` paths only; trace-mode benchmarking is out of scope. A future follow-up feature will add trace-mode fixtures + mode axis when the measurement discipline for eBPF-observed traces is defined.
- Q: When a maintainer runs release-prep without a refreshed baseline, does the flow auto-run benchmarks (~90 min) or fail loudly with a recovery instruction? → A: Fail loudly (Option B). Release-prep detects stale baseline via a fast pre-flight check and errors out with `xtask bench --update-baseline` as the recovery instruction. Two-step flow but each step is fast and inspectable; keeps release-prep decoupled from the ~90-minute benchmark cycle.
- Q: What's the per-fixture timeout default (FR-012)? → A: 5 minutes per fixture. Covers the slowest realistic scan on the reference-architecture runner (large Java multi-module or fat OS image) while keeping runaway-fixture failures under ~5 min instead of consuming hours before hitting the 6-hour job limit. Matches milestone-094's perf-test-posture proportionality (~2 min tight; 5 min broad).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Anyone with the repo can measure waybill's performance reproducibly (Priority: P1) 🎯 MVP

A waybill maintainer wants to answer "how long does waybill take to scan a Cargo workspace?" in a way that produces the same number tomorrow, on a different machine, or after a release. They run one command from the repo root, and get a structured output (both machine-readable JSON and a human-readable table) with median wall-clock, max resident memory, output size, and component count — for every fixture in the standard matrix.

**Why this priority**: Without reproducible measurement, every subsequent perf improvement is unfalsifiable. Today's flow is "run an ad-hoc bash script, capture wall-clock with `python3 -c import time`, report in chat, lose the numbers at session end." The reproducibility gap is the root cause preventing waybill from making defensible perf claims in docs, catching regressions across releases, or answering user capacity-planning questions with anything better than "run it and see." This is the MVP because every other user story depends on it — regression detection (US2) needs stable numbers to compare against, and doc-cited perf claims (US3) need a fixture-SHA-plus-git-SHA pair to point at.

**Independent Test**: after merge, on any workstation with the waybill repo checked out, running the benchmark command produces (a) a JSON file at a documented path containing per-fixture results with the recorded schema, (b) a Markdown table on stdout with the same numbers formatted for reading, (c) an exit code of 0 when all fixtures completed within their per-fixture timeout. Re-running the same command against the same commit + same fixture-SHA on the same host produces results whose median wall-clocks are within 25% of each other (the same noise budget the existing perf tests use per milestone 094).

**Acceptance Scenarios**:

1. **Given** a fresh waybill checkout on a workstation with the test-fixtures cache pre-warmed, **When** the maintainer runs the benchmark command with no filters, **Then** every fixture in the standard matrix executes at least the warm-up run plus the timed-runs cohort, and the output JSON file contains one result record per fixture-plus-mode combination with all required fields populated (wall-clock median in milliseconds, max RSS in kilobytes, component count, output-bytes count, exit status).
2. **Given** a fresh waybill checkout, **When** the maintainer runs the benchmark command filtering to a single fixture, **Then** only that fixture executes, and the run completes in under a small multiple of that fixture's per-run duration (no whole-matrix overhead when the operator scoped down).
3. **Given** the maintainer has run the suite twice in a row against the same waybill commit + same fixture-SHA on the same workstation, **When** they diff the two output JSON files, **Then** every fixture-mode combination's median wall-clock differs by no more than 25% between the two runs (matching the pre-existing perf-test noise budget for local hosts).
4. **Given** the maintainer runs the benchmark command on a workstation where the test-fixtures cache is missing, **When** the run starts, **Then** the tool fetches the pinned fixture set (matching the existing milestone-090 fixture-cache pattern) before any timed measurement begins, and no fixture's warm-up latency contaminates its timed measurement.

---

### User Story 2 - Every release cycle catches perf regressions before they ship (Priority: P2)

A waybill maintainer merges a PR that unknowingly slows down the Java multi-module scan by 40%. The release-time CI run detects that the multi-module fixture's median wall-clock crossed the 25% regression threshold against the committed baseline, posts a comment on the release PR naming which fixture-plus-mode combination regressed by how much, and blocks the release until either a fix lands or the maintainer explicitly acknowledges the regression by updating the baseline.

**Why this priority**: this is the pay-off for US1. Reproducible measurement enables regression catching; regression catching enables shipping fast releases without perf drift. Without US2, the benchmark suite is a diagnostic tool that gets run occasionally when someone remembers. With US2, it's a gate that fires automatically.

**Independent Test**: after merge, a maintainer intentionally introduces a 40% slowdown in a specific reader path (a hand-inserted `std::thread::sleep`) and opens a PR. The release-tag CI workflow (or the equivalent trigger) runs the suite against the PR, compares to the committed baseline, and posts a comment on the PR that names the specific fixture-plus-mode combination whose median wall-clock regressed by ≥25%.

**Acceptance Scenarios**:

1. **Given** a committed baseline JSON on `main` and a PR that regresses one fixture's median wall-clock by 40%, **When** the release-CI run executes against the PR head, **Then** the workflow produces a diff artifact (or PR comment) that names the regressed fixture-plus-mode combination, the baseline median, the observed median, and the percentage delta.
2. **Given** a committed baseline JSON on `main` and a PR that improves one fixture's median wall-clock by 40% (perf win), **When** the release-CI run executes against the PR head, **Then** the workflow does NOT flag the change as a regression, and the diff artifact records the improvement.
3. **Given** the maintainer intentionally accepts a documented slowdown (rare but sometimes correct — e.g., adding accuracy comes with cost), **When** they update the committed baseline JSON in the same PR, **Then** the release-CI run against the PR head finds no regression against the updated baseline.

---

### User Story 3 - Public perf claims cite reproducible measurements (Priority: P3)

A downstream consumer evaluating waybill for capacity planning visits the docs and sees a numbers table saying "waybill scans a 100k-LOC Java multi-module project in ~4.5 seconds on Linux x86_64 (fixture SHA: `abc123`, waybill commit: `def456`)". They can reproduce that measurement on their own hardware to calibrate expectations. Every quoted number in waybill's public perf documentation points at a specific fixture-SHA-plus-commit pair, not a laptop snapshot.

**Why this priority**: this closes the loop from "we have numbers" to "we have credible numbers users can act on." Not blocking for MVP (US1 alone is enough for internal use), but essential for waybill's public credibility on perf claims. Downstream consumers evaluating waybill against Syft/Trivy/Grype today can't compare defensibly because none of the tools publish reproducible-measurement pairs; being the first project to ship this is a differentiator.

**Independent Test**: after the docs-generation script has been run against a committed baseline, the resulting `docs/perf/numbers.md` file cites at least one measurement per fixture-plus-mode combination, and every cited number is accompanied by both a fixture SHA and a waybill commit SHA. A reader with the repo can copy the fixture-SHA into a local benchmark run and get a wall-clock within the noise budget of the cited number.

**Acceptance Scenarios**:

1. **Given** a committed baseline JSON exists at the documented path, **When** the doc-generation script runs, **Then** the output Markdown numbers page contains a table with at least one row per fixture-plus-mode combination, and every row includes the median wall-clock, the fixture SHA, and the waybill commit SHA.
2. **Given** the docs numbers page is served alongside waybill's other public docs, **When** a downstream reader follows the fixture-SHA to the corresponding fixture in the test-fixtures repo, **Then** the fixture is present at that exact SHA (the doc generation MUST NOT emit dead fixture-SHA references).
3. **Given** the docs numbers page cites a specific waybill commit SHA, **When** a downstream reader checks out that commit and re-runs the benchmark suite against the same fixture-SHA on Linux x86_64, **Then** the reproduced median wall-clock is within 25% of the cited number (the standard noise budget).

---

### User Story 4 - Docs numbers stay current across releases without maintenance chore (Priority: P3)

A waybill maintainer prepares a release. As part of the release PR flow, the perf docs are automatically refreshed to cite the new baseline that ships with this release. No manual doc-editing step; no risk that the release ships with a stale "numbers as of milestone 500" line while the actual baseline advanced to milestone 700.

**Why this priority**: convenience + drift prevention. Without US4, the docs and the committed baseline drift out of sync every release cycle unless someone remembers to hand-update the numbers page. With US4, the numbers page is a derived artifact — always in sync with the baseline.

**Independent Test**: a maintainer runs the release-prep flow. Without any manual docs edit, the release PR's diff includes an updated `docs/perf/numbers.md` (or equivalent) whose citations point at the release commit's baseline SHA. Merging the release PR ships the docs and the baseline together.

**Acceptance Scenarios**:

1. **Given** a scheduled release-prep run, **When** the maintainer executes the release-prep flow, **Then** the release PR diff includes an updated docs numbers page whose citations match the release baseline exactly.
2. **Given** a maintainer bumps the version between releases without running a full benchmark cycle, **When** the release-prep run tries to update the numbers page, **Then** the flow performs a fast pre-flight check comparing the baseline commit-SHA against the current HEAD's Rust-workspace + workflow diff scope, and if the baseline is stale, exits non-zero with a diagnostic naming (a) the specific stale baseline path and (b) the exact recovery command (`xtask bench --update-baseline`). Release-prep MUST NOT auto-run the ~90-minute benchmark suite, and MUST NOT silently ship a version bump with unrefreshed numbers.

---

### Edge Cases

- **Fixture cache miss on first run**: the tool fetches the pinned fixture-SHA into `~/.cache/waybill/fixtures/<sha>/` before any timed measurement (matching the milestone-090 pattern). Fetch cost is NOT counted toward any fixture's timed measurement.
- **Noisy runner (macOS-latest CI runner known-noisy per milestone 094)**: measurements taken on known-noisy hosts get a wider noise band. The release-CI workflow runs on the dedicated non-macos runner; local runs on any host include a diagnostic in the JSON output identifying the runner's noise class.
- **Fixture-format skew across ecosystems**: a fixture that scans in 20 ms vs one that scans in 30 seconds have different noise profiles. Per-fixture median-of-5-with-warmup is uniform; per-fixture regression thresholds may need per-fixture tuning in follow-up work (v1 uses uniform 25%).
- **Missing baseline (first commit of the suite)**: first release ships with the baseline that first-ran. Regression detection only fires against a committed baseline; the initial baseline commit is exempt.
- **Fixture updates via test-fixtures repo bumps**: bumping the test-fixtures repo SHA in `build.rs` may cause fixture-content churn that changes measured numbers legitimately (e.g., a fixture gained a new dependency). Baseline updates in the same commit as the fixture-repo bump are required; the CI regression comparison MUST compare against the baseline captured at the fixture-SHA the PR is bumping to, not the pre-bump baseline.
- **Long-running fixtures on GitHub-hosted runners**: the release-CI workflow gets ~6-hour job limits; a runaway benchmark that never returns (e.g., a bug in a fixture) MUST be killed per-fixture at a documented timeout, not consume the entire job's timeout.
- **Deep-hash vs no-deep-hash mode axis**: some fixtures spend most of their time in the deep-hash path; benchmarking both `--no-deep-hash` and default gives a two-axis view. Both modes are measured for every fixture where the mode axis is meaningful (source-tier ecosystems where deep-hash makes a measurable difference; not meaningful for OS DB scans where deep-hash isn't invoked).
- **Fingerprint-corpus on/off axis**: fingerprint matching has its own runtime cost; benchmarking with and without the corpus gives a directly-attributable cost measurement per fixture.
- **Format-axis (single vs triple)**: single-format and triple-format (CDX + SPDX 2.3 + SPDX 3) emissions have known cost differences (per the existing `dual_format_perf.rs` + `triple_format_perf.rs` posture); triple-format is the release-representative mode + gets measured; single-format is the lightweight mode + gets measured; running only one axis would under-sell the full picture.
- **Regressions in non-timed dimensions (RSS, output size, component count)**: a PR that keeps wall-clock stable but doubles RSS should be flagged. Regression detection covers all four measured dimensions with per-dimension thresholds.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST provide a command invocable from the waybill workspace root that runs a documented benchmark matrix (fixture × mode) and produces both machine-readable JSON output at a documented path and a human-readable Markdown summary on standard output.
- **FR-002**: The benchmark matrix MUST cover, at minimum, one representative fixture per waybill-supported source-tier ecosystem (cargo, npm, pip, go, maven, gradle, gem, nuget, cmake, bazel, conan, vcpkg), one representative container-image fixture, and one representative binary-introspection fixture (a directory containing a fixed set of binaries). **Scope boundary (Q1 clarification)**: the matrix covers `waybill sbom scan` paths only. `waybill trace` (eBPF-observed build trace) benchmarking is deliberately excluded from v1 and tracked as a follow-up feature; the measurement discipline for long-running observation is a separate design.
- **FR-003**: For each fixture in the matrix, the system MUST perform at least one un-timed warm-up run before any timed measurement, and MUST perform at least five timed runs and record their median wall-clock as the fixture's canonical number.
- **FR-004**: Each timed run MUST capture, at minimum: wall-clock elapsed in milliseconds, peak resident-memory usage in kilobytes, output byte-count, component count in the emitted SBOM, and process exit status. All five dimensions land in the JSON output for every fixture-mode combination.
- **FR-005**: The JSON output schema MUST be stable across releases — additive-only changes to fields, no rename-in-place. Downstream consumers reading the JSON can rely on field names remaining valid across at least the next 12 months.
- **FR-006**: The system MUST support filtering the benchmark matrix by fixture-name pattern so operators can benchmark a single ecosystem without paying the whole-matrix wall-clock cost.
- **FR-007**: The system MUST support taking a baseline JSON file as input and producing a comparison report identifying every fixture-mode combination whose median wall-clock differs from the baseline by more than a documented threshold (default 25% per the milestone-094 noise budget).
- **FR-008**: The system MUST support the same baseline-comparison shape for the other three measured dimensions (RSS, output bytes, component count) — a threshold breach on any single dimension MUST flag the fixture-mode as regressed.
- **FR-009**: A committed baseline JSON MUST live at a documented path in the repo. Every release MUST include an up-to-date baseline in its release commit — no release ships with a baseline the release code doesn't correspond to.
- **FR-010**: The release CI workflow MUST run the benchmark suite against every release tag, compare against the committed baseline, and produce a diff artifact (or PR comment) identifying regressions ≥ the threshold. Regressions MUST NOT silently ship; the release either resolves them or explicitly acknowledges them via a baseline update in the same release PR.
- **FR-011**: The benchmark tool MUST fetch missing fixture sets automatically before timed measurement — the operator does not need to pre-warm the fixture cache manually. Fetch cost is NOT counted toward any fixture's timed measurement.
- **FR-012**: The benchmark tool MUST support a per-fixture timeout, defaulting to **5 minutes per fixture** (Q3 clarification), so a runaway fixture cannot consume the entire benchmark run's time budget. The default is overridable via a documented flag for maintainers running exploratory benchmarks on slower hosts. Timed-out fixtures MUST be recorded in the JSON output with an explicit timeout status, not silently omitted.
- **FR-013**: The benchmark tool MUST record, alongside each result, the waybill commit SHA it was measured against and the fixture-SHA of the fixture set used. Downstream consumers using the numbers for capacity planning need both to reproduce.
- **FR-014**: The docs numbers page (or equivalent) MUST be a derived artifact — generated from the committed baseline JSON via a documented command, not hand-authored. Every quoted number in the docs MUST cite both the fixture-SHA and the waybill commit SHA.
- **FR-015**: The system MUST NOT introduce new user-space runtime dependencies beyond what waybill already ships. The benchmark tool is a development/CI concern; end users installing waybill from the binary tarball are unaffected.
- **FR-016**: All benchmark tooling MUST live under the existing `xtask` workspace crate (or an equivalent existing dev-tool location). No new top-level workspace crate is introduced.
- **FR-017**: The benchmark JSON output MUST include host-metadata fields (operating system, architecture, whether the host is known-noisy per the milestone-094 macos-latest classification) so downstream consumers reading the JSON can weight the numbers appropriately.
- **FR-018**: The release CI workflow's regression-comment mechanism MUST post at most one diff comment per PR — subsequent runs update the same comment rather than piling on new ones.
- **FR-019**: Fixtures used by the benchmark suite MUST live in the existing test-fixtures repo (per the milestone-090 split-repo pattern), NOT in the waybill main repo. This preserves the milestone-090 stay-set discipline and keeps the waybill main repo lean.
- **FR-020**: The system MUST support running the benchmark suite in a mode where NO comparison against a baseline occurs — this is the "capture-only" mode used for producing a fresh baseline (US4 release-prep flow).

### Key Entities *(include if feature involves data)*

- **Benchmark Fixture**: a directory tree (source-tree, container-image tarball, or binary-set) that represents a specific measurement axis (per-ecosystem, per-size, per-mode). Fixtures live in the test-fixtures repo pinned by SHA. Each fixture has a stable identifier (fixture-name) and metadata declaring which measurement modes make sense for it (deep-hash yes/no, fingerprint-corpus yes/no, format axis).
- **Benchmark Result**: one measurement point representing a single fixture × single mode combination. Contains: fixture-name, mode-descriptor, median wall-clock (ms), max RSS (KB), output-bytes, component-count, exit-status, waybill-commit-SHA, fixture-SHA, runner uname, noise-classification.
- **Benchmark Run**: a complete pass through the fixture-mode matrix producing a set of Benchmark Results. Recorded with a top-level metadata block (waybill commit SHA, fixture-SHA, runner metadata, wall-clock start/end timestamps for the whole run) plus the array of Results. Serialized as one JSON file per run.
- **Baseline**: a specific persisted Benchmark Run that lives at a documented path in the waybill main repo. Every release commit MUST have a Baseline pointing at that commit's own measurement snapshot. Used by CI as the comparison anchor for regression detection.
- **Regression Diff**: a computed comparison between a new Benchmark Run and a Baseline. Lists every fixture-mode combination whose measured value in any dimension crosses the threshold in the "worse" direction. Emitted as JSON + Markdown (JSON for machine consumers, Markdown for the release-PR comment).

## Assumptions

- The reproducibility target is "same commit + same fixture-SHA + same host produces medians within 25%." NOT "same commit produces identical numbers across different hosts" — cross-host comparability is a separate, harder problem the docs numbers-page handles by pinning citations to a specific reference architecture (Linux x86_64 GitHub-hosted runner, matching the CI workflow's runner class).
- Median-of-5 (matching the existing `dual_format_perf.rs` posture) is the noise-robustness posture. Not median-of-N-configurable in v1 (adds surface without clear value).
- The 25% regression threshold matches the pre-existing `dual_format_perf` SC-007 threshold — reusing a documented posture consumers of the perf-test infra already understand. Not per-dimension-configurable in v1.
- Fixtures pinned by SHA in the test-fixtures repo are the authoritative reference. When a new fixture is added, the fixture-repo commit that adds it becomes the pin bump; the waybill main-repo baseline commit that follows captures the numbers at that new pin.
- Release-CI is the primary regression-detection surface. Local runs are the exploratory/debugging surface. The two workflows share the same tool + schema; only the trigger and threshold-enforcement policy differ.
- Doc generation runs as part of the release-prep flow (US4), NOT on every CI run. A slightly-stale numbers page between releases is acceptable; a numbers page that never matches the shipped baseline is not.
- The benchmark tool is a development / CI concern — no waybill-runtime code change is required. Adding this feature does NOT bloat the compiled `waybill` binary, does NOT touch the CLI surface, does NOT change SBOM emission behavior.
- Historical perf data from ad-hoc bash-script sessions is NOT imported into the new baseline. First committed baseline is the one measured against the merge commit of this feature.
- Fingerprint-corpus mode measurement assumes the milestone-108 corpus is available at the pinned SHA. If the corpus is unreachable at benchmark time, corpus-mode fixtures are recorded with an explicit "corpus-unreachable" status, not silently omitted.
- Feature is delivered in 5 self-contained PRs matching the issue-body scoping:
  - PR 1: fixture curation in the test-fixtures repo (mostly file shuffling)
  - PR 2: benchmark driver implementation (`xtask bench`)
  - PR 3: JSON schema + initial baseline commit + regression-comparison logic
  - PR 4: doc-generation script (`xtask bench-docs`) + `docs/perf/numbers.md`
  - PR 5: CI workflow + regression-comment integration
- This spec covers all 5 PRs' worth of scope. Plan phase decides ordering + which PR gets which user-story slice.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A maintainer who has never used the benchmark tool before can, on a fresh checkout, produce a Benchmark Run JSON file for the full matrix in under 15 minutes of wall-clock time (excluding fixture-cache warm-up). Measured by asking one non-author engineer to time-box the walk-through and record the elapsed time.
- **SC-002**: The same commit + same fixture-SHA + same host produces two consecutive Benchmark Runs whose per-fixture median wall-clocks differ by no more than 25%. Verified in the acceptance test for US1 scenario 3.
- **SC-003**: A perf regression of ≥ 25% in any measured dimension on any fixture MUST be detected by the release-CI workflow and surfaced in a PR comment. Verified in the acceptance test for US2 scenario 1 (hand-inserted `std::thread::sleep`).
- **SC-004**: A perf improvement of ≥ 25% in any measured dimension MUST NOT be flagged as a regression. Verified in US2 scenario 2.
- **SC-005**: The docs numbers page is fully derived from the committed baseline — a `git diff` between the baseline JSON and the numbers page shows perfect correspondence for every quoted number. Verified by running the doc-gen command against a known baseline and diffing against the shipped page.
- **SC-006**: 100% of numbers cited in `docs/perf/numbers.md` include both a fixture-SHA and a waybill commit SHA. Verified by grep against the page.
- **SC-007**: Fetch of a missing fixture-cache set completes in under 60 seconds on a workstation with reasonable bandwidth (measured against a fresh fixture cache miss on the reference architecture Linux x86_64 GitHub-hosted runner class).
- **SC-008**: The full benchmark run on the reference-architecture CI runner completes in under 90 minutes wall-clock time (fits comfortably within the 6-hour GitHub-hosted-runner job limit with margin for slower-than-usual runs).
- **SC-009**: Zero net-new Cargo dependencies at the workspace-runtime layer. The benchmark tool may add crate deps under `xtask/` only; `waybill-cli`/`waybill-common`/`waybill-ebpf` dependency closures MUST remain byte-identical pre-vs-post feature.
- **SC-010**: A downstream reader who copies the fixture-SHA + waybill commit SHA cited in `docs/perf/numbers.md` to their own workstation running the reference-architecture class (Linux x86_64) and re-runs the benchmark suite reproduces the cited median wall-clock within 25%. This is the credibility test — the docs numbers must be reproducible by others, not just by the person who committed them.
