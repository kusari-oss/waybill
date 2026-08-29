# Tasks: Persisted reproducible benchmark suite

**Feature Branch**: `669-bench-harness`
**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md) | **Data model**: [data-model.md](./data-model.md) | **Contracts**: [json-schema.md](./contracts/json-schema.md) + [xtask-bench-cli.md](./contracts/xtask-bench-cli.md) + [ci-workflow.md](./contracts/ci-workflow.md) | **Quickstart**: [quickstart.md](./quickstart.md)
**Closes**: [#328](https://github.com/kusari-oss/waybill/issues/328)

## PR delivery arc (per plan.md summary)

This spec ships across **5 self-contained PRs** — each a coherent shippable unit. Task phases below organize by user story per spec-kit convention, but the "Implementation strategy" at the bottom maps user-story phases to PRs.

**Task-count note**: 65 total tasks (T001-T065) after the M2/M3 analyze remediations. See the bottom of this file for the PR delivery arc mapping.

## Phase 1: Setup

- [X] T001 Verify baseline pre-PR gate is green on the fresh branch: `./scripts/pre-pr.sh` exits 0. Record test count (should equal or exceed the m668 post-merge baseline of 5208 workspace tests) in this task's completion note. **Done 2026-08-29**: **Total passed: 5208 / 0 failed** — byte-identical to the post-m668 merge baseline. Confirms zero regressions from #727 (docs-only) shipping. SC-006-equivalent anchor established: m669's xtask-side test additions land ON TOP of 5208. **Note**: first run failed on the documented m203 helm-timing flake (`m203_us2_5_env_var_override_shortens_timeout` observed 77.65s vs 1s timeout — see memory `reference_m203_helm_test_flake`). Re-ran per memory guidance; second run exit 0 clean. Flake is a known false-positive; not a regression against #727 or main.
- [X] T002 Verify empirical claims from research.md § "Empirical claims to re-verify at implementation time": (a) `cargo add sysinfo --dry-run --package xtask` succeeds against current published `sysinfo` version — record the actual pinned version if v0.32 has drifted; (b) confirm `waybill-cli/tests/dual_format_perf.rs:237-247` still uses 5-sample median with warmup pattern (grep it); (c) `.github/workflows/perf.yml` still exists and doesn't conflict with the m669 new `bench.yml` trigger scope (release-tag vs. perf-lane triggered by different events); (d) `xtask/src/main.rs` is still the 40-line one-subcommand shape; (e) run `cargo run -p xtask -- ebpf --help` on a Linux host or verify inspection matches the plan.md's `Cli::Ebpf` variant assumption. Any drift documented back into research.md before proceeding. **Done 2026-08-29**: one drift caught (same pattern as m668 T002). (a) `cargo search sysinfo` → **v0.39.6** (research had v0.32 — knowledge-cutoff artifact). Updated research.md R1 + plan.md Primary Dependencies pin to v0.39.6. (b) `dual_format_perf.rs:237-247` confirmed intact — median_of_5 fn signature + 5-sample array literal + `samples.sort()` + `samples[2]` return unchanged from planning-time inspection. (c) `.github/workflows/perf.yml` triggers on `pull_request:` + `schedule:` (cron). m669's proposed `bench.yml` will trigger on `push:` (release tags) + `schedule:` + `workflow_dispatch:` — disjoint event scopes, different job content, no conflict. (d) `xtask/src/main.rs` is exactly 40 lines with the `Cli::Ebpf` sole variant as expected. (e) `Cli::Ebpf` visible at line 8 of the file; matches plan.md's extension assumption verbatim. All 5 sub-checks green; only sysinfo version-pin drift needed a research edit.
- [X] T003 Read the sibling `waybill-test-fixtures` repo's current structure (`WAYBILL_FIXTURES_DIR` env var set by build.rs). Enumerate existing top-level directories to confirm `benchmark/` isn't already claimed by an unrelated milestone; if it is, coordinate with the operator before proceeding. **Done 2026-08-29**: sibling repo cache resolved to `~/.cache/waybill/fixtures/fffc00b50395e731650de09317a88972a49faac6/` (pin from `waybill-cli/build.rs:27` → `kusari-sandbox/waybill-test-fixtures.git`). 23 top-level entries: `apk`, `bazel`, `cargo`, `cargo-workspace`, `cmake`, `cmake_findpackage_only`, `conan`, `conan_vcpkg_cross`, `deb`, `gem`, `gem-source-project`, `go`, `maven`, `maven-multi-module-reactor`, `npm`, `npm-scoped-package`, `npm-workspace`, `pip-pyproject-pep621`, `pip-pyproject-poetry-only`, `polyglot-monorepo`, `python`, `rpm`, `transitive_parity`, `vcpkg`, plus `README.md`. `grep -i "bench\|perf"` → 0 matches. **`benchmark/` is unclaimed**; safe to create in PR 1 per T013-T015 plan.

## Phase 2: Foundational

Blocking prerequisites for all user stories. Land these before Phase 3 begins.

- [X] T004 Add `sysinfo = "<T002-recorded-version>"` to `xtask/Cargo.toml` under `[dependencies]`. Zero waybill-runtime deps added; verify via `git diff Cargo.toml waybill-cli/Cargo.toml waybill-common/Cargo.toml waybill-ebpf/Cargo.toml` → 0 lines (SC-009 anchor). **Done 2026-08-29**: `sysinfo = "0.39.6"` added at `xtask/Cargo.toml:9` with inline comment citing FR-015 + SC-009. Zero waybill-runtime dep additions confirmed.
- [X] T006 Create `xtask/src/bench/mod.rs` + child modules as empty stubs (`matrix.rs`, `run.rs`, `measure.rs`, `schema.rs`, `compare.rs`, `docs.rs`). Each file has a top-level `// milestone 669 - see specs/669-bench-harness/plan.md` comment and empty `pub` items so `cargo check -p xtask` passes. Ensures the module tree is discoverable before any subsystem lands. **Done 2026-08-29** (executed before T005 due to compile-order — T005's match arms reference `bench::run` and `bench::docs::run` which need the module to exist). 7 files created: `mod.rs` declaring 6 child modules + `BenchArgs` empty struct + `pub fn run` stub; `docs.rs` with `BenchDocsArgs` empty struct + `pub fn run` stub; `matrix.rs`/`measure.rs`/`run.rs`/`schema.rs`/`compare.rs` as comment-only placeholders. All stubs use `Result<(), Box<dyn std::error::Error>>` return type (stdlib only; T024+ migrate to anyhow when richer error handling is needed). Each stub `todo!()`s with a pointer to the specific task ID that will implement it.
- [X] T005 Extend `xtask/src/main.rs` `Cli` enum with two new variants: `Bench(BenchArgs)` and `BenchDocs(BenchDocsArgs)`. Add corresponding match arms that dispatch to `bench::run` and `bench::docs::run`. `BenchArgs` and `BenchDocsArgs` remain empty structs at this task (stub); flags land in T014/T015/T020/T026/T031. **Done 2026-08-29** (executed after T006 due to compile-order — tasks.md dependency arrow T005→T006 was cosmetic; the correct build order is T004→T006→T005→T007). `xtask/src/main.rs` grew from 40 → 54 lines with the two new variants + match arms. `main()` refactored to return `Result` from each variant + centralize error printing at the top level; existing `Ebpf` variant behavior byte-preserved (still calls `build_ebpf()` unchanged).
- [X] T007 Add `target/bench/` to `.gitignore` at repo root. Prevents per-run capture files from polluting `git status`. **Done 2026-08-29** as no-op: `.gitignore:2` already ignores `target/` broadly which covers `target/bench/` transitively. Adding an explicit `target/bench/` entry would be redundant. Task's intent (bench captures don't pollute `git status`) is satisfied by the existing broader rule.

**Checkpoint after Phase 2**: `cargo check -p xtask` passes with the new module tree; workspace-level `cargo check --workspace` unchanged from T001 baseline.

## Phase 3: US1 — Anyone with the repo can measure waybill's performance reproducibly (Priority: P1) 🎯 MVP

**Goal**: `cargo run -p xtask -- bench` produces reproducible measurements. Corresponds to PRs 1 + 2 of the delivery arc.

**Independent Test**: After the fixture-curation PR + driver PR land, on any workstation with the repo checked out, running `cargo run -p xtask -- bench --filter cargo-workspace-medium` produces `target/bench/run-<git-sha>.json` with one BenchResult record whose `waybill_commit_sha` and `fixture_sha` are non-empty 40-char hex strings, whose `raw_samples_ms.len() == 5`, and whose `median_wall_clock_ms == raw_samples_ms.sorted()[2]`. Re-running the same command produces a second file whose median differs from the first by ≤25% (SC-002).

### Data model + JSON schema (sequential — all touch `xtask/src/bench/schema.rs`)

- [X] T008 [US1] Implement `Fixture`, `FixtureKind`, `Mode`, `ScanClass` structs in `xtask/src/bench/schema.rs` per data-model.md §1. Each has `#[derive(Debug, Clone, Serialize, Deserialize)]`; enums use `#[serde(rename_all = "kebab-case")]`. Add `impl Fixture { pub fn all_from_manifest(path: &Path) -> Result<Vec<Self>> }` reading the fixture manifest JSON. **Done 2026-08-29**: 4 types + manifest wrapper + `all_from_manifest` implemented. Also added `serde = "1"` (features=["derive"]) + `serde_json = "1"` to `xtask/Cargo.toml` under `[dependencies]` (plan.md listed as "existing workspace crates" but xtask/Cargo.toml only had clap; needed to be explicit) plus `tempfile = "3"` as dev-dep for the schema-manifest round-trip test. 6 unit tests all pass: kebab-case wire shape for each of the 3 enums (FixtureKind/Mode/ScanClass) + Fixture round-trip through JSON + `all_from_manifest` reads a valid manifest + rejects a missing file. Return type is `Result<Vec<Self>, Box<dyn Error>>` (stdlib only; anyhow migration can happen later when T024+ needs richer error surface). `cargo test -p xtask` → 6 passed / 0 failed.
- [X] T009 [US1] Implement `BenchResult` + `ExitStatus` structs in `xtask/src/bench/schema.rs` per data-model.md §2. Add `impl BenchResult { pub fn validate(&self) -> Result<()> }` asserting V2 (5 samples) + V3 (median = sorted[2]) + V4 (both SHAs non-empty 40-char hex). **Done 2026-08-29**: BenchResult (10 fields per data-model.md §2) + ExitStatus (5-variant kebab-case enum) + `impl BenchResult::validate` + `fn is_valid_sha` helper. V2 (5 samples) enforced by the `[u64; 5]` type declaration (compile-time constraint, not runtime). V3 asserted by re-sorting `raw_samples_ms` and comparing to `median_wall_clock_ms`. V4 asserted by `is_valid_sha` (len == 40 + all bytes in `[0-9a-f]`). 11 new tests: exit_status wire shape (all 5 variants), BenchResult JSON round-trip, validate passes on well-formed + unsorted-samples, validate rejects wrong median (V3), empty/short/uppercase/non-hex SHAs (V4 × 4), empty fixture_sha (V4), is_valid_sha edge cases (6 case rundown). `cargo test -p xtask` → **17 passed / 0 failed** (11 new + 6 from T008).
- [X] T010 [US1] Implement `BenchRun` + `RunMetadata` + `NoiseClass` structs in `xtask/src/bench/schema.rs` per data-model.md §3. Add `impl BenchRun { pub fn schema_version() -> u32 { 1 }, pub fn validate(&self) -> Result<()> }` asserting V1 + V5 (every Result's fixture-mode exists in manifest) + V6 (no duplicate (fixture, mode) pairs). **Done 2026-08-29**: BenchRun (3 fields) + RunMetadata (7 fields) + NoiseClass (3-variant kebab-case enum). Design decision: V5 requires the manifest as input, so split into `validate(&self)` (V1+V6 self-consistency) + `validate_against_manifest(&self, &[Fixture])` (V5 manifest cross-check). This mirrors data-model.md V5's "asserted at Run emission time" wording — T023 runner will call both back-to-back with the manifest it just loaded. `schema_version` is a `u32` field on the struct plus an associated fn `BenchRun::schema_version()` returning the binary's expected version (`1`); V1 asserts they match (fail-close on future schema_version=2 files per contract json-schema.md C-1). 11 new tests: schema_version constant, NoiseClass wire shape, BenchRun JSON round-trip, contract C-1 schema_version-at-root wire shape, validate happy path, V1 rejection (wrong schema_version), V6 rejection (duplicate fixture-mode) + V6 acceptance (same fixture different mode), validate_against_manifest happy + V5 unknown-fixture + V5 mode-not-supported. `cargo test -p xtask` → **28 passed / 0 failed** (11 new + 17 from T008/T009).
- [X] T011 [US1] [P] Contract test at `xtask/tests/schema_roundtrip.rs` — construct a full `BenchRun` in-memory, serialize to JSON, deserialize back, assert equality. Locks every field name to wire representation (contract json-schema.md T1). **Done 2026-08-29**: 4 test fns covering (1) full BenchRun round-trip equality, (2) every-field-name-appears-verbatim (BenchRun/RunMetadata/BenchResult — 18 field names locked externally), (3) nested enum wire-shape spot-check (NoiseClass/Mode/ExitStatus), (4) Fixture manifest round-trip (locked separately since Fixture is loaded from a different file). Refactored xtask from binary-only to lib+bin (added `xtask/src/lib.rs` declaring `pub mod bench;`, updated `src/main.rs` to `use xtask::bench;`) so external integration tests under `xtask/tests/` can access `xtask::bench::schema::*`. `cargo test -p xtask` → 4 passed.
- [X] T012 [US1] [P] Contract test at `xtask/tests/schema_version_gate.rs` — serialize a synthetic `{"schema_version": 2, ...}`, attempt to deserialize into a v1 `BenchRun` reader with `serde(deny_unknown_fields)` disabled and a validate step, assert clear rejection (json-schema.md T3). **Done 2026-08-29**: 3 test fns covering (1) v2 file deserializes cleanly at serde level but fails at validate() with V1 violation naming both observed + expected versions in the diagnostic, (2) v0 also rejected (fail-close on any unrecognized version, not just future ones — over-broad-gate protection), (3) v1 sanity: valid v1 payload still passes (isolates gate from over-broad rejection). `cargo test -p xtask --test schema_version_gate` → 3 passed. **Final aggregate**: 28 unit tests + 4 T011 + 3 T012 = **35/35 pass** across all xtask crate tests.

### Fixture curation (test-fixtures sibling repo — PR 1)

- [ ] T013 [US1] Author `benchmark/manifest.json` in the sibling `waybill-test-fixtures` repo per research.md R4. Include ~14 fixture entries: one per source-tier ecosystem (cargo, npm, pip, go, maven, gradle, gem, nuget, cmake, bazel, conan, vcpkg = 12), one container-image (debian-slim), one binary-set (linux-binaries-50). Each entry: `{name, path, kind, ecosystem, supported_modes, expected_scan_class}`.
- [ ] T014 [US1] Populate the fixtures themselves in `benchmark/source-tier/<ecosystem>/`, `benchmark/container-images/debian-slim.tar` (via `docker pull debian:12-slim && docker save debian:12-slim > debian-slim.tar`), `benchmark/binaries/linux-binaries-50/` (50 real binaries from a canonical source — coreutils / busybox / etc.; NOT random garbage bytes). Fixtures MUST NOT reference real package coordinates that would trigger Kusari Inspector advisory scans on downstream consumers (per memory `feedback_fixture_synthetic_package_names`); use `waybill-fixture-<ecosystem>-*` synthetic package names in every lockfile.
- [ ] T015 [US1] Open + merge PR against the sibling `waybill-test-fixtures` repo. Get the merge commit SHA; this is the fixture-SHA baseline for m669.
- [ ] T016 [US1] Bump the fixture-SHA pin in waybill main-repo `build.rs` to the T015-recorded SHA. This unblocks the `WAYBILL_FIXTURES_DIR` env var pointing at the m669 benchmark subdirectory.

### Matrix enumeration (sequential — touches `xtask/src/bench/matrix.rs`)

- [ ] T017 [US1] Implement `pub fn enumerate(manifest_path: &Path, filter: Option<&Vec<String>>) -> Result<Vec<(Fixture, Mode)>>` in `xtask/src/bench/matrix.rs`. Reads manifest, iterates fixtures × their `supported_modes`, filters by glob pattern per contract xtask-bench-cli.md C-2.
- [ ] T018 [US1] Unit test at `xtask/src/bench/matrix.rs::tests` — hand-craft a 3-fixture manifest with different `supported_modes`, assert the enumeration produces the expected cartesian product and the filter cases behave (single-glob, multi-glob-union, no-matches → empty).

### Measurement (sequential — all touch `xtask/src/bench/measure.rs`)

- [ ] T019 [US1] Implement `pub fn measure_one(cmd: &Command, timeout: Duration) -> Result<Sample>` in `xtask/src/bench/measure.rs`. `Sample` is a struct `{wall_clock_ms, max_rss_kb, output_bytes, exit_status}`. Spawn cmd via `std::process::Command`, poll `sysinfo::Process::memory()` at ~10 Hz on a background thread while the child runs, `wait_with_output()` on the main thread, compute wall-clock from `Instant::now()` diff. Timeout enforcement: kill child + return `ExitStatus::Timeout` if elapsed exceeds `timeout` (Q3 5-min default).
- [ ] T020 [US1] Unit test at `xtask/src/bench/measure.rs::tests` — measure a `sleep 0.5` (or its cross-platform equivalent) invocation, assert wall-clock ≥ 500ms + ≤ 600ms, exit status is Success. Also test timeout by measuring a `sleep 10` with `timeout = 1s`, assert wall-clock ~1s + status is Timeout.
- [ ] T021 [US1] Implement `pub fn parse_output_metadata(cdx_path: &Path) -> Result<OutputMeta>` in `measure.rs`. `OutputMeta` = `{output_bytes, component_count}`. Reads the CDX JSON output, extracts `.components.length` for component count; sums output file bytes across all `--output` paths for triple-format modes.

### Runner (median-of-5) (sequential — touches `xtask/src/bench/run.rs`)

- [ ] T022 [US1] Implement `pub fn run_one_fixture(fixture: &Fixture, mode: Mode, cfg: &RunConfig) -> Result<BenchResult>` in `xtask/src/bench/run.rs`. Constructs the waybill CLI invocation from `fixture.kind` + `mode`, calls `measure_one` in a warmup pass (result discarded), then 5 timed passes. Median = sorted[2]. Constructs `BenchResult`. Invokes `.validate()` before returning.
- [ ] T023 [US1] Implement `pub fn run_matrix(matrix: Vec<(Fixture, Mode)>, cfg: &RunConfig) -> Result<BenchRun>` in `run.rs`. Iterates fixtures sequentially (no parallelism v1 per contract xtask-bench-cli.md non-contract), calls `run_one_fixture` per, accumulates into `BenchRun.results`. Fills `RunMetadata` with waybill commit SHA (`git rev-parse HEAD`), fixture SHA (from build.rs env), runner uname (`uname -srmn` shell-out), noise-class classifier (Linux x86_64 GHA = Reference, macos-latest = Noisy, else = Other), started/finished timestamps.

### CLI wiring (sequential — touches `xtask/src/bench/mod.rs` + `xtask/src/main.rs`)

- [ ] T024 [US1] Define `BenchArgs` clap struct in `xtask/src/bench/mod.rs` with the 8 flags per contract xtask-bench-cli.md (`--filter`, `--baseline`, `--threshold`, `--output`, `--fixtures-dir`, `--per-fixture-timeout-sec`, `--update-baseline`, `--preflight-check`). Only wire up `--filter`, `--output`, `--fixtures-dir`, `--per-fixture-timeout-sec` in this task; the other 4 remain declared-but-unhandled (their logic lands in US2/US4 phases).
- [ ] T025 [US1] Implement `pub fn run(args: BenchArgs) -> Result<()>` in `xtask/src/bench/mod.rs`. Wires: enumerate matrix → run_matrix → write JSON to `--output` path (default `target/bench/run-<git-sha>.json`) → print Markdown table to stdout. Atomic write via `tempfile::NamedTempFile::persist` per contract json-schema.md C-6.
- [ ] T026 [US1] Contract test at `xtask/tests/cli_flag_parsing.rs` — assert every T024-wired flag parses via clap, including validation on `--per-fixture-timeout-sec` (accept 60-3600; reject 0 and 3601).

### US1 acceptance test

- [ ] T027 [US1] Manual acceptance: run `cargo run -p xtask -- bench --filter cargo-workspace-medium` twice in a row on the current workstation. Verify: (a) both runs produce non-empty `target/bench/run-<sha>.json`, (b) each has 1 BenchResult with `raw_samples_ms.len() == 5` and median = sorted[2], (c) both waybill-commit-SHA + fixture-SHA are non-empty 40-char hex, (d) the two medians differ by ≤25% (SC-002). Record the two medians + delta in this task's completion note.

**Checkpoint after Phase 3 (US1)**: PR 2 shippable. Local benchmarks work reproducibly. US1 acceptance test green.

## Phase 4: US2 — Every release cycle catches perf regressions before they ship (Priority: P2)

**Goal**: `--baseline` flag enables regression detection; release CI blocks on ≥25% regressions. Corresponds to PR 3 of the delivery arc.

**Independent Test**: On a branch that intentionally introduces a 40% slowdown (`std::thread::sleep(500ms)` in a hot reader path), running `cargo run -p xtask -- bench --baseline docs/perf/baseline.json` returns exit 1 with a diff artifact naming the regressed fixture-mode-dimension tuple.

### Regression comparison (sequential — touches `xtask/src/bench/compare.rs`)

- [ ] T028 [US2] Implement `RegressionDiff`, `RegressionEntry`, `MatrixAsymmetryEntry`, `Dimension` structs in `xtask/src/bench/schema.rs` per data-model.md §5.
- [ ] T029 [US2] Implement `pub fn compare(subject: &BenchRun, baseline: &BenchRun, threshold: f64) -> RegressionDiff` in `xtask/src/bench/compare.rs`. For each fixture-mode combination present in both, compute per-dimension `percentage_delta = (subject_val - baseline_val) / baseline_val`. Positive-and-≥-threshold on wall-clock/RSS/output-bytes = regression; positive-and-≥-threshold on component-count = also regression (SBOM shape drift). Negative-and-≥-threshold in absolute magnitude = improvement. Fixture-mode in only one Run = matrix asymmetry.
- [ ] T030 [US2] Unit test at `xtask/src/bench/compare.rs::tests` — hand-craft baseline + subject BenchRuns with (a) one 40% wall-clock regression, (b) one 40% RSS improvement, (c) one fixture present only in subject. Assert compare() returns 1 regression + 1 improvement + 1 asymmetry entry. Also test the threshold-not-breached case (10% delta → no regression).

### Baseline discipline

- [ ] T031 [US2] Extend `BenchArgs` in `xtask/src/bench/mod.rs` to wire `--baseline`, `--threshold`, `--update-baseline`. `--baseline <path>` triggers post-run comparison; `--update-baseline` overrides `--output` to `docs/perf/baseline.json` and validates via V1-V6 before writing.
- [ ] T032 [US2] Extend `pub fn run` in `bench/mod.rs` to: (a) call compare() when `--baseline` present, (b) write RegressionDiff to `target/bench/regression-diff-<sha>.json`, (c) print Markdown table of the diff, (d) exit 1 if `diff.regressions.len() > 0` (contract xtask-bench-cli.md C-3), (e) exit 0 otherwise.
- [ ] T033 [US2] Create the initial `docs/perf/baseline.json` by running `cargo run -p xtask -- bench --update-baseline` on the current merge commit. This baseline captures the T027 acceptance-test-time numbers. Committed as the seed baseline for future regression comparisons.

### US2 acceptance test

- [ ] T034 [US2] Manual acceptance — TWO scratch-branch scenarios covering SC-003 regression detection AND SC-004 improvement-not-flagged discipline. Both use throwaway scratch branches (do NOT commit to the m669 branch).

  **Scenario A (SC-003 regression)**: create a scratch branch with a `std::thread::sleep(500)` inserted in a hot reader path (e.g., `waybill-cli/src/scan_fs/package_db/cargo.rs` at the start of the reader function). Run `cargo run -p xtask -- bench --filter cargo-workspace-medium --baseline docs/perf/baseline.json`. Verify: (a) exit code is 1, (b) `target/bench/regression-diff-<sha>.json` names the cargo fixture with wall-clock dimension regressed by ~35-45%. Revert the scratch branch after verification.

  **Scenario B (SC-004 improvement not flagged as regression)**: create a scratch branch with a fake perf improvement (e.g., add an early return at the top of the same hot reader path that skips actual work — resulting in a much faster scan). Run the same `xtask bench --filter cargo-workspace-medium --baseline docs/perf/baseline.json`. Verify: (a) exit code is 0 (improvements MUST NOT fail the run per SC-004), (b) `regression-diff.json` records the change in the `improvements` field (informational, not `regressions`), (c) the CI-side workflow would NOT block a release on this shape. Revert the scratch branch after verification.

**Checkpoint after Phase 4 (US2)**: PR 3 shippable. Regression detection works; committed baseline in place.

## Phase 5: US3 — Public perf claims cite reproducible measurements (Priority: P3)

**Goal**: `xtask bench-docs` generates `docs/perf/numbers.md` from the committed baseline; every quoted number cites fixture-SHA + waybill-commit-SHA. Corresponds to PR 4 of the delivery arc.

**Independent Test**: After PR 4 lands, running `cargo run -p xtask -- bench-docs` produces `docs/perf/numbers.md` where `grep -c "fixture-sha:" docs/perf/numbers.md` equals the number of BenchResult rows in the baseline (every row cites a fixture-SHA).

### Docs generation (sequential — touches `xtask/src/bench/docs.rs`)

- [ ] T035 [US3] Implement `pub fn generate_markdown(baseline: &BenchRun) -> String` in `xtask/src/bench/docs.rs`. Output structure: (a) title + generation-date footer, (b) reference-architecture note pinning citations to Linux x86_64 GHA class, (c) per-fixture section grouping Results by fixture-name, (d) per-mode table with columns `mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | fixture-SHA | waybill-SHA`. Every row cites both SHAs from the baseline's `metadata` block (they're identical for every Result within one Run per V4 + V5).
- [ ] T036 [US3] Define `BenchDocsArgs` clap struct in `xtask/src/bench/docs.rs` with 3 flags (`--baseline`, `--output`, `--dry-run`) per contract xtask-bench-cli.md § bench-docs flags.
- [ ] T037 [US3] Implement `pub fn run(args: BenchDocsArgs) -> Result<()>` in `xtask/src/bench/docs.rs`. Reads baseline via V1 schema-version gate, calls `generate_markdown`, writes to `--output` (default `docs/perf/numbers.md`) or prints on `--dry-run`.
- [ ] T038 [US3] [P] Contract test at `xtask/tests/docs_generation_deterministic.rs` per contract xtask-bench-cli.md T4 — runs `bench-docs` twice against the same baseline, asserts byte-identical output. Enforces C-7 (pure-function derivation).
- [ ] T039 [US3] Wire `Cli::BenchDocs(args)` match arm in `xtask/src/main.rs` to `bench::docs::run(args)`.

### US3 acceptance test

- [ ] T040 [US3] Manual acceptance: run `cargo run -p xtask -- bench-docs`. Verify: (a) `docs/perf/numbers.md` is created + non-empty, (b) `grep -c "fixture-sha:" docs/perf/numbers.md` equals BenchResult count in baseline, (c) `grep -c "waybill-sha:" docs/perf/numbers.md` matches too, (d) SC-006 grep (100% of quoted numbers include both SHAs) passes. Record grep counts in completion note.

**Checkpoint after Phase 5 (US3)**: PR 4 shippable. Docs numbers page live + reproducible.

## Phase 6: US4 — Docs numbers stay current across releases (Priority: P3)

**Goal**: `--preflight-check` prevents release-prep from shipping with stale baseline. Corresponds to the release-prep-integration slice of PR 5.

**Independent Test**: After PR 5 lands, running `xtask bench --preflight-check` on a branch with waybill-runtime code changes since the baseline SHA exits non-zero with the R7-mandated recovery-command diagnostic. On a branch with only docs/CI changes, exits zero silently.

### Pre-flight check (sequential — touches `xtask/src/bench/mod.rs`)

- [ ] T041 [US4] Wire `--preflight-check` flag in `BenchArgs` per contract xtask-bench-cli.md C-5. When set, this flag skips the entire bench-run path and instead executes the R7 staleness algorithm: read baseline → extract `waybill_commit_sha` → run `git diff --stat <baseline-sha>..HEAD -- 'waybill-cli/**' 'waybill-common/**' 'waybill-ebpf/**' Cargo.lock` → exit 1 with C-5 diagnostic if non-empty, exit 0 if empty. `--preflight-check` MUST be mutually exclusive with `--update-baseline` and `--baseline` (clap `conflicts_with`).
- [ ] T042 [US4] [P] Contract test at `xtask/tests/preflight_check_stale.rs` per contract xtask-bench-cli.md T2 — plant a `baseline.json` with `metadata.waybill_commit_sha` set to `HEAD~5`, modify a file under `waybill-cli/src/` in the working tree, run `--preflight-check`, assert exit 1 + diagnostic text includes "Perf baseline is stale" and "cargo run -p xtask -- bench --update-baseline".
- [ ] T043 [US4] [P] Contract test at `xtask/tests/preflight_check_current.rs` per contract xtask-bench-cli.md T3 — plant a `baseline.json` with `metadata.waybill_commit_sha` set to the current `git rev-parse HEAD`, run `--preflight-check`, assert exit 0 + no output.

### US4 acceptance test

- [ ] T044 [US4] Manual acceptance: on the m669 branch (which has code changes since main), run `cargo run -p xtask -- bench --preflight-check`. Verify exit 1 + diagnostic includes the recovery command. Then temporarily edit `docs/perf/baseline.json` to set `metadata.waybill_commit_sha` to current `git rev-parse HEAD`, re-run, verify exit 0. Revert the edit.

**Checkpoint after Phase 6 (US4)**: PR 5 partial. Release-prep-integration done; CI workflow still pending in Phase 7.

## Phase 7: CI Workflow Integration (crosscut US2 acceptance)

**Goal**: `.github/workflows/bench.yml` runs the suite on release tags + weekly cron; posts edit-in-place PR comment on regressions. Corresponds to the CI slice of PR 5.

**Independent Test**: A release-tag push (or workflow_dispatch simulation on a fork) fires `bench.yml`, completes within SC-008's 90-min budget, and posts (or updates) a PR comment on the release PR containing the RegressionDiff table.

### Workflow authoring (sequential — touches `.github/workflows/bench.yml`)

- [ ] T045 Author `.github/workflows/bench.yml` per contract ci-workflow.md § job-graph diff summary. 9 steps: checkout → rust-toolchain → rust-cache → `cargo build -p xtask` → `cargo build --release -p waybill` → `cargo run -p xtask -- bench` → `cargo run -p xtask -- bench --baseline docs/perf/baseline.json --output <run-json-path>` → upload artifacts → post/edit PR comment via `actions/github-script`. Triggers per C-triggers: `push:` tag pattern + `schedule:` weekly cron + `workflow_dispatch:`.
- [ ] T046 SHA-pin every `uses:` reference in `bench.yml` per contract ci-workflow.md C-2. Look up current SHAs for `actions/checkout`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `actions/upload-artifact`, `actions/github-script` via `gh api /repos/<owner>/<repo>/tags`. Record all 5 in this task's completion note.
- [ ] T047 Implement the `actions/github-script` step body per contract ci-workflow.md C-7: read `regression-diff.json`, format Markdown table, prepend magic marker `<!-- bench-regression-comment-v1 -->`, query existing PR comments for marker, PATCH-or-POST accordingly.
- [ ] T048 Grep-gate verify contract ci-workflow.md T2 (SHA-pin): `grep -nE "uses:.*@[a-f0-9]{40}" .github/workflows/bench.yml` matches every `uses:` line; `grep -nE "uses:.*@v[0-9]" .github/workflows/bench.yml` returns empty.
- [ ] T049 Grep-gate verify contract ci-workflow.md T3 (fail-closed): `grep -B2 continue-on-error .github/workflows/bench.yml | grep xtask` returns empty.

### CI acceptance test

- [ ] T050 Manual acceptance: push a scratch tag `v0.4.0-dev.m669` to a fork; observe `bench.yml` fires, completes within 90 min, produces run.json + regression-diff.json artifacts. If the fork isn't practical, skip to post-merge acceptance: first stable release tag after m669 lands MUST exercise this path.

## Phase 8: Polish & cross-cutting

### Contract-verify grep gates

- [ ] T051 [P] Verify FR-015 + SC-009 zero-runtime-Cargo-diff: `git diff main -- Cargo.toml Cargo.lock waybill-cli/Cargo.toml waybill-common/Cargo.toml waybill-ebpf/Cargo.toml` → 0 lines. `xtask/Cargo.toml` diff is expected (sysinfo add); allowed.
- [ ] T052 [P] Verify FR-016 no-new-top-level-crate: `git diff main --name-only | grep -E "^[a-z_-]+/Cargo\.toml$" | grep -v "^xtask/"` → 0 lines. Only xtask/Cargo.toml touched at the crate-manifest layer.
- [ ] T053 [P] Verify FR-019 fixtures-in-sibling-repo: `git diff main --stat | grep "^ tests/fixtures/benchmark/"` → 0 lines. No benchmark fixtures in the waybill main repo.
- [ ] T054 [P] Verify contract json-schema.md C-1 schema-version present in shipped baseline: `jq '.schema_version' docs/perf/baseline.json` returns `1`.
- [ ] T055 [P] Verify contract json-schema.md C-4 SHAs on every Result: `jq '.results[] | select(.waybill_commit_sha == "" or .fixture_sha == "")' docs/perf/baseline.json` returns empty.
- [ ] T056 [P] Verify SC-006 docs SHA citations: `grep -c "fixture-sha:" docs/perf/numbers.md` equals `jq '.results | length' docs/perf/baseline.json` (every Result row cites a fixture-SHA in the docs page).

### Fixture-cache fetch measurement (SC-007)

- [ ] T057 [P] Verify SC-007 (fixture-cache fetch ≤ 60s on cache-miss). Steps: `rm -rf ~/.cache/waybill/fixtures/<pinned-sha>/` (clean slate) then `time cargo build -p xtask` (triggers m090 build.rs fetch as side effect). Record elapsed seconds in this task's completion note. Assertion: elapsed ≤ 60s on the reference-architecture runner class (or the current workstation with reasonable bandwidth). If exceeded, root-cause before advancing — likely candidates: sibling test-fixtures repo grew unexpectedly large, GitHub API rate-limiting, or network flake. Explicitly guards against silent regression in m090 infrastructure that this feature inherits.

### Pre-PR gate + walker audit

- [ ] T058 Run full pre-PR gate: `./scripts/pre-pr.sh` MUST exit 0 with the T001 baseline test count (5208 + any xtask-side test additions from T011/T012/T018/T020/T026/T030/T038/T042/T043). Record the new count.
- [ ] T059 Verify walker-audit gate per memory `feedback_walker_audit_local_check`. m669 introduces zero waybill-runtime code changes so this is a trivial pass; run it anyway.

### Memory + commit + PR chain

- [ ] T060 Update auto-memory: append `reference_bench_harness.md` at `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/` documenting: (a) `xtask bench` + `xtask bench-docs` invocation shape, (b) median-of-5 + warmup posture, (c) reference architecture pin (Linux x86_64 GHA class), (d) reproducibility target (25% noise budget), (e) release-prep pre-flight staleness algorithm (R7), (f) 5-PR delivery arc so future contributors know how to slice. Cross-link from MEMORY.md immediately after the `reference_slsa_provenance` entry.
- [ ] T061 Commit + open **PR 1** (fixture curation in sibling `waybill-test-fixtures` repo). This PR was already opened and merged as T015 — record its URL here as a back-reference.
- [ ] T062 Commit + open **PR 2** (driver implementation: T004-T027). Ping the user before firing `gh pr create` per memory `feedback_upstream_prs_need_explicit_approval`. PR title: `feat(m669 US1): xtask bench driver — reproducible measurement harness (partial #328)`.
- [ ] T063 Commit + open **PR 3** (regression detection: T028-T034). PR title: `feat(m669 US2): --baseline comparison + committed baseline.json (partial #328)`.
- [ ] T064 Commit + open **PR 4** (docs generation: T035-T040). PR title: `feat(m669 US3): xtask bench-docs + docs/perf/numbers.md (partial #328)`.
- [ ] T065 Commit + open **PR 5** (CI workflow + release-prep pre-flight: T041-T050). PR title: `feat(m669 US4+CI): bench.yml + release-prep --preflight-check (closes #328)`. This PR closes the issue. Include a post-merge acceptance checklist in the PR body citing: T012/T018-style SC-002 reproducibility, SC-003 regression detection on first release-tag CI run, SC-008 90-min budget measurement, **SC-001 (a non-author engineer's 15-min onboarding time-box)** — the SC-001 check needs a fresh set of eyes and is deferred to a post-merge follow-up per the M1 analysis finding.

## Dependencies

**Sequential within US1**:
```
T004 (sysinfo dep) → T005 (Cli enum) → T006 (module tree)
        ↓
T008-T010 (schema.rs — sequential; same file)
        ↓
T011-T012 (contract tests — [P] independent files)
        ↓
T013-T016 (fixture curation — sequential on the sibling repo + build.rs bump)
        ↓
T017-T018 (matrix.rs)
        ↓
T019-T021 (measure.rs — sequential; same file)
        ↓
T022-T023 (run.rs — sequential; same file)
        ↓
T024-T026 (mod.rs CLI wiring)
        ↓
T027 (US1 acceptance test)
```

**US2** depends on US1 completion (needs schema + runner):
```
T028 (RegressionDiff struct) → T029-T030 (compare.rs)
        ↓
T031-T032 (mod.rs baseline flags)
        ↓
T033 (initial baseline commit)
        ↓
T034 (US2 acceptance)
```

**US3** depends on US2 completion (needs shipped baseline):
```
T035-T037 (docs.rs)
        ↓
T038 (contract test — [P])
        ↓
T039 (main.rs wiring)
        ↓
T040 (US3 acceptance)
```

**US4** depends on US2 completion (needs baseline file to check against):
```
T041 (--preflight-check flag)
        ↓
T042-T043 (contract tests — [P])
        ↓
T044 (US4 acceptance)
```

**Phase 7 CI** depends on US2 + US4 completion (needs both `--baseline` and `--preflight-check` shipped for release-prep integration):
```
T045-T047 (bench.yml — sequential; same file)
        ↓
T048-T049 (grep-gate verify — [P])
        ↓
T050 (CI acceptance)
```

**Phase 8 Polish tasks** can run in parallel:
- T051-T057 (6 contract-verify grep gates + 1 fixture-cache-fetch measurement) — 7 independent `[P]` tasks
- T058 (pre-PR), T059 (walker-audit), T060 (memory) — sequential-after-verifications

## Parallel execution opportunities

- **Phase 8 verifications** (T051-T057): 6 grep-based gates + 1 fixture-cache-fetch measurement (SC-007), all independent. Batch as a single message.
- **Contract tests within each user story**: T011+T012 (US1), T038 (US3), T042+T043 (US4) all `[P]`-marked — can be authored in parallel with their sibling implementation tasks.
- **Fixture files (T014 substep)**: each ecosystem's fixture directory is independent — a maintainer could parallelize authoring across ecosystems if hand-rolling; typically not worth batching for LLM implementation.

## MVP scope

**Phase 3 (US1) alone** delivers the substrate — reproducible local measurement — that every other user story depends on. If US2/US3/US4 slip, US1 alone answers "how do I measure waybill perf reproducibly?" which is the SC-001 anchor. The recommended MVP shipping order is Phase 1 → Phase 2 → Phase 3, with Phase 4-7 as follow-up PRs if needed.

## Implementation strategy

**5-PR delivery arc** (matches spec Assumptions + plan.md summary):

| PR | Phases covered | Tasks | Recommended commit range |
|---|---|---|---|
| PR 1 | Phase 3 US1 fixture-curation subset | T013-T015 (sibling repo) | Open against `kusari-oss/waybill-test-fixtures`. Merges before PR 2. |
| PR 2 | Phase 2 + Phase 3 US1 driver | T004-T012, T016-T027 | Waybill main-repo. Closes #328 US1 slice. |
| PR 3 | Phase 4 US2 | T028-T034 | Waybill main-repo. Depends on PR 2. |
| PR 4 | Phase 5 US3 | T035-T040 | Waybill main-repo. Depends on PR 3 (needs baseline). |
| PR 5 | Phase 6 US4 + Phase 7 CI + Phase 8 polish | T041-T065 | Waybill main-repo. Closes #328 fully. |

**Estimated task-time**:
- Phase 1 (Setup + empirical re-check): ~30 min
- Phase 2 (Foundational): ~15 min (mechanical stubs)
- Phase 3 (US1 — driver + fixtures): ~4-6 hours + fixture-curation time (sibling repo) — the largest chunk
- Phase 4 (US2): ~1.5 hours
- Phase 5 (US3): ~1 hour
- Phase 6 (US4): ~30 min
- Phase 7 (CI): ~1 hour
- Phase 8 (Polish): ~30 min
- **Total**: ~10-12 hours of focused engineering across ~4-6 shipping sessions

**Post-merge acceptance follow-ups** (mirror the T012/T018 pattern from m668):
- First release-tag after PR 5 merges MUST exercise `bench.yml` end-to-end. Failure blocks the release.
- SC-008 90-min budget MUST hold on the first CI run. If exceeded, root-cause before the next release.

## Advisory notes

- **Sizeable feature**: this tasks.md enumerates 65 tasks across 5 PRs. Consider tackling one PR at a time — spec-kit's `/speckit.implement` supports task-range batches, so `/speckit.implement T004-T027` completes PR 2 as one arc.
- **PR 1 is against a sibling repo**: T013-T015 fire `gh pr create` against `kusari-oss/waybill-test-fixtures`, not waybill main. Follow the standard user-approval dance before opening.
- **T033 initial-baseline commit**: this baseline captures Whatever The Current Numbers Are on the merge commit of PR 3. If those numbers look wrong (unexpectedly slow, unexpectedly fast), root-cause BEFORE committing — the baseline is the anchor everything else compares against.
