# Phase 1 Data Model: benchmark schema + regression-diff

**Feature**: `669-bench-harness` | **Date**: 2026-08-29

Typed Rust structs (in `xtask/src/bench/schema.rs`) with serde-derived JSON serialization. Every entity has a documented JSON on-wire shape; every field has a documented invariant. Schema version 1 (R2) — additive-only until version bumps to 2.

## Entities

### 1. Fixture (from `manifest.json` in test-fixtures repo)

Read-only descriptor of one benchmark input.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fixture {
    /// Stable identifier — e.g., "cargo-workspace-medium". Used as
    /// the primary lookup key in bench Results.
    pub name: String,
    /// Path relative to the fixtures-repo root — e.g.,
    /// "benchmark/source-tier/cargo-workspace-medium".
    pub path: String,
    /// Fixture kind driving the waybill invocation shape.
    pub kind: FixtureKind,
    /// Ecosystem this fixture represents (informational; used in the
    /// docs numbers-page grouping). None for non-ecosystem fixtures
    /// (container-images, binaries).
    pub ecosystem: Option<String>,
    /// Which mode axes are meaningful for this fixture. Non-listed
    /// modes are skipped in the matrix for this fixture.
    pub supported_modes: Vec<Mode>,
    /// Expected scan-time class (informational; helps operators
    /// predict run duration).
    pub expected_scan_class: ScanClass,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FixtureKind {
    SourceTree,     // waybill sbom scan --path <fixture>
    ContainerImage, // waybill sbom scan --image <fixture>/image.tar
    BinarySet,      // waybill sbom scan --path <fixture> (dir of binaries)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    Default,                       // no flags
    NoDeepHash,                    // --no-deep-hash
    TripleFormat,                  // --format cdx --format spdx-2.3 --format spdx-3
    NoDeepHashPlusTripleFormat,    // both
    FingerprintsCorpus,            // --fingerprints-corpus <cache>
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScanClass {
    Fast,   // <500 ms per run
    Medium, // 500 ms – 5 s per run
    Slow,   // >5 s per run
}
```

### 2. Result (one measurement point)

Emitted per fixture × mode combination.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    pub fixture_name: String,
    pub mode: Mode,
    /// Median wall-clock across the 5 timed runs, in milliseconds.
    pub median_wall_clock_ms: u64,
    /// Peak RSS across the 5 timed runs, in kilobytes. Captured
    /// via sysinfo polling at 10 Hz per R1.
    pub max_rss_kb: u64,
    /// Byte count of the emitted SBOM output. Sum across all
    /// --output paths for triple-format modes.
    pub output_bytes: u64,
    /// Component count in the emitted SBOM. Parsed post-run from
    /// the CycloneDX output's .components.length.
    pub component_count: u64,
    /// Terminal state of the measurement — success, per-fixture
    /// timeout, corpus-unreachable, or an explicit error class.
    pub exit_status: ExitStatus,
    /// waybill commit SHA at measurement time (FR-013).
    pub waybill_commit_sha: String,
    /// Fixture-repo commit SHA at measurement time (FR-013).
    pub fixture_sha: String,
    /// Individual sample wall-clocks (5 entries; median is
    /// samples.sorted()[2]). Retained for post-hoc analysis of
    /// distribution shape — not used by regression comparison.
    pub raw_samples_ms: [u64; 5],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExitStatus {
    Success,
    Timeout,             // per-fixture 5-min cap hit (Q3)
    CorpusUnreachable,   // fingerprint-corpus mode with cache miss
    NonZeroExitCode,     // waybill CLI returned non-zero
    SchemaParseError,    // output CDX didn't parse
}
```

### 3. Run (top-level container)

One complete benchmark pass across the full matrix. Serialized as one JSON file.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchRun {
    /// Schema-version pin per R2. v1 is additive-only for at least
    /// 12 months per FR-005.
    pub schema_version: u32,
    /// Metadata about this run (host, timestamps, waybill SHA).
    pub metadata: RunMetadata,
    /// One entry per fixture-mode combination attempted.
    pub results: Vec<BenchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    pub waybill_commit_sha: String,
    pub fixture_sha: String,
    /// Result of `uname -srmn` at run-start.
    pub runner_uname: String,
    /// Classifier: is this a known-noisy runner class per m094?
    pub noise_class: NoiseClass,
    /// RFC 3339 timestamp of run start (wall-clock, not monotonic).
    pub started_at: String,
    /// RFC 3339 timestamp of run end.
    pub finished_at: String,
    /// Total wall-clock duration of the whole matrix run, in
    /// seconds. Should be well under SC-008's 90-min ceiling.
    pub total_duration_sec: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoiseClass {
    /// Linux x86_64 GitHub-hosted-runner class. Reference architecture.
    Reference,
    /// macOS-latest CI runner — known-noisy per m094.
    Noisy,
    /// Any other host — treated as unknown-noise, weight accordingly.
    Other,
}
```

### 4. Baseline (committed persistent Run)

Same shape as `BenchRun`. Lives at `docs/perf/baseline.json` in the waybill main repo. FR-009 mandates every release commit has a Baseline pointing at that commit's own measurement snapshot.

**Storage**: JSON pretty-printed with sorted keys for stable diffing. Regenerated via `xtask bench --update-baseline` (which writes to `docs/perf/baseline.json` atomically via tempfile rename).

### 5. Regression Diff

Computed from a new `BenchRun` (subject) vs a `BenchRun` (baseline). Not persisted; emitted as JSON + Markdown at CI time.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionDiff {
    /// Which subject-vs-baseline pair this diff represents.
    pub subject_sha: String,
    pub baseline_sha: String,
    /// Comparison threshold used (default 0.25 per SC-003).
    pub threshold: f64,
    /// One entry per fixture-mode combination where any measured
    /// dimension crossed the threshold in the "worse" direction.
    pub regressions: Vec<RegressionEntry>,
    /// One entry per fixture-mode combination where a dimension
    /// improved by ≥ threshold (informational; not a failure).
    pub improvements: Vec<RegressionEntry>,
    /// Fixture-mode combinations present in the baseline but
    /// absent in the subject (or vice versa). Not a failure by
    /// itself; surfaces the diff for human review.
    pub matrix_asymmetry: Vec<MatrixAsymmetryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionEntry {
    pub fixture_name: String,
    pub mode: Mode,
    pub dimension: Dimension,
    pub baseline_value: f64,
    pub subject_value: f64,
    pub percentage_delta: f64,  // Positive = worse for regressions, negative = better for improvements.
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dimension {
    WallClockMs,
    MaxRssKb,
    OutputBytes,
    ComponentCount,  // Threshold breach here means the SBOM shape changed — investigation warranted, not always a regression.
}
```

## Relationships

```text
                ┌─────────────────────┐
                │  Fixture (manifest) │
                └──────────┬──────────┘
                           │ N Modes each
                           ▼
                ┌─────────────────────┐
                │  BenchResult        │
                │  (one per fixture × │
                │  mode combination)  │
                └──────────┬──────────┘
                           │ N per Run
                           ▼
                ┌─────────────────────┐
                │  BenchRun           │
                │  (schema_version +  │
                │  metadata + results)│
                └──────────┬──────────┘
                           │
                ┌──────────┴──────────┐
                │                     │
                ▼                     ▼
       ┌───────────────┐    ┌───────────────┐
       │  Baseline     │    │  RegressionDiff│
       │  (committed at │    │  (computed;    │
       │  docs/perf/)  │    │  not persisted)│
       └───────────────┘    └───────────────┘
```

**Cardinality**: 1 Run → ~14 Fixtures × ~5 Modes = ~70 Results. 1 Baseline per release. 1 RegressionDiff per CI comparison run.

## State transitions

**BenchResult lifecycle**:

| State | Trigger | Next State |
|---|---|---|
| Pending | Fixture × Mode enters matrix | Warmup |
| Warmup | 1 un-timed run completes | Timed samples |
| Timed samples | 5 timed runs complete | Result computed |
| Result computed | Median + max sample values captured | Recorded in Run |
| Timeout | Per-fixture 5-min cap hit during any single sample | ExitStatus::Timeout recorded |
| CLI error | waybill exits non-zero on any sample | ExitStatus::NonZeroExitCode recorded |

**Baseline lifecycle**:

| State | Trigger | Next State |
|---|---|---|
| Fresh | `xtask bench --update-baseline` writes docs/perf/baseline.json | Committed |
| Committed | Merged to main | Reference for next N release cycles |
| Stale | Waybill-runtime code changes since baseline SHA (R7 pre-flight check detects) | Refresh required before next release |
| Superseded | `--update-baseline` overwrites | Historical (in git log) |

## Validation rules

- **V1**: `schema_version == 1` for all v1-shipped baselines. Consumers reading a `schema_version != 1` baseline MUST refuse to process it (fail-loud forward compat).
- **V2**: Every `BenchResult` MUST have `raw_samples_ms.len() == 5` (matches FR-003's 5-sample requirement).
- **V3**: Median wall-clock MUST equal `raw_samples_ms.sort()[2]` — asserted at write time; sanity check protects against a bug in the sampler.
- **V4**: `waybill_commit_sha` and `fixture_sha` MUST both be non-empty 40-char hex strings (FR-013).
- **V5**: Every `BenchResult` in a `BenchRun.results` must correspond to a fixture-mode combination that appears in the fixtures-repo `manifest.json`'s `supported_modes` list (asserted at Run emission time).
- **V6**: No two `BenchResults` in the same `BenchRun` may have the same `(fixture_name, mode)` pair (uniqueness).
- **V7**: `RegressionDiff.threshold` MUST equal 0.25 for release-CI runs (SC-003 anchor). Local runs may override via `--threshold` flag.
- **V8**: `RegressionDiff.regressions` is non-empty ⟹ the release CI workflow exits non-zero. Enforcement lives in the CI workflow's post-comparison step (contract `ci-workflow.md` C-6).
