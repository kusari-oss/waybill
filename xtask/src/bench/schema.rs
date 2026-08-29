// milestone 669 - see specs/669-bench-harness/plan.md
// Serde structs for Fixture / BenchResult / BenchRun / Baseline / RegressionDiff
// per specs/669-bench-harness/data-model.md.
//
// This file is filled in incrementally across T008 (Fixture + supporting enums),
// T009 (BenchResult + ExitStatus), and T010 (BenchRun + RunMetadata + NoiseClass).

use std::error::Error;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ────────────────────────────────────────────────────────────────
// T008 — Fixture + supporting enums (data-model.md §1)
// ────────────────────────────────────────────────────────────────

/// Read-only descriptor of one benchmark input, sourced from the
/// `benchmark/manifest.json` in the sibling `waybill-test-fixtures`
/// repo per research.md R4.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Fixture {
    /// Stable identifier — e.g., `"cargo-workspace-medium"`. Primary
    /// lookup key used in `BenchResult.fixture_name`.
    pub name: String,
    /// Path relative to the fixtures-repo root — e.g.,
    /// `"benchmark/source-tier/cargo-workspace-medium"`.
    pub path: String,
    /// Fixture kind — drives the shape of the `waybill` invocation
    /// (source-tree scan vs container-image scan vs binary-set scan).
    pub kind: FixtureKind,
    /// Ecosystem this fixture represents. Used for docs-page
    /// grouping (US3). None for non-ecosystem fixtures
    /// (container-images, binary-sets).
    pub ecosystem: Option<String>,
    /// Which mode axes are meaningful for this fixture. Non-listed
    /// modes are skipped by the matrix enumerator for this fixture.
    pub supported_modes: Vec<Mode>,
    /// Expected scan-time class. Informational; helps operators
    /// predict per-fixture wall-clock cost.
    pub expected_scan_class: ScanClass,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum FixtureKind {
    /// `waybill sbom scan --path <fixture>`.
    SourceTree,
    /// `waybill sbom scan --image <fixture>/image.tar`.
    ContainerImage,
    /// `waybill sbom scan --path <fixture>` where fixture is a
    /// directory containing a fixed set of binaries.
    BinarySet,
}

/// Measurement modes. Each fixture declares which of these are
/// meaningful in its `supported_modes` list.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// No mode-specific flags — baseline `waybill sbom scan` invocation.
    Default,
    /// `--no-deep-hash` — skips per-file SHA-256 computation.
    NoDeepHash,
    /// `--format cyclonedx-json --format spdx-2.3-json --format spdx-3-json`
    /// — measures the multi-format emission overhead.
    TripleFormat,
    /// `--no-deep-hash --format cyclonedx-json --format spdx-2.3-json
    /// --format spdx-3-json` — the "release-representative" combination.
    NoDeepHashPlusTripleFormat,
    /// `--fingerprints-corpus <cache>` — measures the fingerprint
    /// matching cost against the m108 corpus.
    FingerprintsCorpus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ScanClass {
    /// < 500 ms per timed run.
    Fast,
    /// 500 ms – 5 s per timed run.
    Medium,
    /// > 5 s per timed run.
    Slow,
}

/// Wrapper for the top-level `benchmark/manifest.json` shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FixtureManifest {
    fixtures: Vec<Fixture>,
}

impl Fixture {
    /// Read the `benchmark/manifest.json` at the given path and
    /// return every declared fixture. See research.md R4 for the
    /// manifest shape.
    pub fn all_from_manifest(path: &Path) -> Result<Vec<Self>, Box<dyn Error>> {
        let contents = std::fs::read_to_string(path)?;
        let manifest: FixtureManifest = serde_json::from_str(&contents)?;
        Ok(manifest.fixtures)
    }
}

// ────────────────────────────────────────────────────────────────
// T009 — BenchResult + ExitStatus (data-model.md §2)
// ────────────────────────────────────────────────────────────────

/// One measurement point representing a single fixture × mode
/// combination. Emitted per-fixture-per-mode by the runner (T022).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchResult {
    pub fixture_name: String,
    pub mode: Mode,
    /// Median wall-clock across the 5 timed runs, in milliseconds.
    /// MUST equal `raw_samples_ms.sorted()[2]` (V3).
    pub median_wall_clock_ms: u64,
    /// Peak RSS across the 5 timed runs, in kilobytes. Captured via
    /// sysinfo polling at ~10 Hz per research.md R1.
    pub max_rss_kb: u64,
    /// Byte count of the emitted SBOM output. Sum across all
    /// `--output` paths for triple-format modes.
    pub output_bytes: u64,
    /// Component count in the emitted SBOM. Parsed post-run from
    /// the CycloneDX output's `.components.length`.
    pub component_count: u64,
    /// Terminal state of the measurement.
    pub exit_status: ExitStatus,
    /// waybill commit SHA at measurement time. MUST be non-empty
    /// 40-char lowercase hex (V4).
    pub waybill_commit_sha: String,
    /// Fixture-repo commit SHA at measurement time. MUST be
    /// non-empty 40-char lowercase hex (V4).
    pub fixture_sha: String,
    /// Individual sample wall-clocks (5 entries; median is
    /// `samples.sorted()[2]`). Retained for post-hoc analysis of
    /// distribution shape; not used by regression comparison.
    pub raw_samples_ms: [u64; 5],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExitStatus {
    /// Fixture-mode completed all 5 samples cleanly.
    Success,
    /// Per-fixture 5-minute timeout hit on any sample (Q3).
    Timeout,
    /// Fingerprint-corpus mode with cache miss / network flake.
    CorpusUnreachable,
    /// `waybill` CLI returned non-zero.
    NonZeroExitCode,
    /// Output CycloneDX / SPDX JSON failed to parse.
    SchemaParseError,
}

impl BenchResult {
    /// Assert V2 (implicit — array type enforces), V3
    /// (`median == sorted[2]`), and V4 (both SHAs non-empty
    /// 40-char lowercase hex). Called at emission time by the runner.
    pub fn validate(&self) -> Result<(), Box<dyn Error>> {
        // V3: median MUST equal the middle of the sorted samples.
        let mut sorted = self.raw_samples_ms;
        sorted.sort_unstable();
        if self.median_wall_clock_ms != sorted[2] {
            return Err(format!(
                "V3 violation: BenchResult.median_wall_clock_ms={} does not \
                 equal sorted(raw_samples_ms)[2]={} for fixture={} mode={:?}",
                self.median_wall_clock_ms, sorted[2], self.fixture_name, self.mode,
            )
            .into());
        }
        // V4: waybill_commit_sha MUST be non-empty 40-char lowercase hex.
        if !is_valid_sha(&self.waybill_commit_sha) {
            return Err(format!(
                "V4 violation: BenchResult.waybill_commit_sha={:?} is not a \
                 40-char lowercase hex string for fixture={}",
                self.waybill_commit_sha, self.fixture_name,
            )
            .into());
        }
        // V4: fixture_sha MUST be non-empty 40-char lowercase hex.
        if !is_valid_sha(&self.fixture_sha) {
            return Err(format!(
                "V4 violation: BenchResult.fixture_sha={:?} is not a \
                 40-char lowercase hex string for fixture={}",
                self.fixture_sha, self.fixture_name,
            )
            .into());
        }
        Ok(())
    }
}

/// Returns true iff `s` is a non-empty 40-char lowercase hex string.
fn is_valid_sha(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

// ────────────────────────────────────────────────────────────────
// T010 — BenchRun + RunMetadata + NoiseClass (data-model.md §3)
// ────────────────────────────────────────────────────────────────

/// A complete benchmark pass across the full matrix. Serialized as
/// one JSON file (either `target/bench/run-<sha>.json` for capture
/// runs or `docs/perf/baseline.json` for the committed baseline).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchRun {
    /// Schema-version pin per contract json-schema.md C-1 + research
    /// R2. v1 is additive-only for at least 12 months per FR-005.
    pub schema_version: u32,
    /// Run-scope metadata (host, timestamps, waybill + fixture SHAs).
    pub metadata: RunMetadata,
    /// One entry per fixture-mode combination attempted.
    pub results: Vec<BenchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    /// seconds. Should sit well under SC-008's 90-minute ceiling.
    pub total_duration_sec: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum NoiseClass {
    /// Linux x86_64 GitHub-hosted-runner class. Reference
    /// architecture — the class waybill's docs perf numbers cite.
    Reference,
    /// macOS-latest CI runner — known-noisy per milestone 094.
    Noisy,
    /// Any other host — treated as unknown-noise; weight accordingly.
    Other,
}

impl BenchRun {
    /// The schema version THIS binary emits and expects to read.
    /// Consumers reading a file with `schema_version != this`
    /// MUST refuse to process it (V1 forward-compat fail-close).
    pub fn schema_version() -> u32 {
        1
    }

    /// Assert V1 (schema-version match) and V6 (no duplicate
    /// `(fixture, mode)` pairs). V5 (every fixture-mode exists in a
    /// manifest) requires the manifest as input; see
    /// `validate_against_manifest`.
    pub fn validate(&self) -> Result<(), Box<dyn Error>> {
        // V1: schema_version MUST match this binary's declared version.
        if self.schema_version != Self::schema_version() {
            return Err(format!(
                "V1 violation: BenchRun.schema_version={} does not match \
                 this binary's expected version={}",
                self.schema_version,
                Self::schema_version(),
            )
            .into());
        }
        // V6: no duplicate (fixture_name, mode) pairs.
        let mut seen: std::collections::HashSet<(&str, Mode)> =
            std::collections::HashSet::with_capacity(self.results.len());
        for r in &self.results {
            let key = (r.fixture_name.as_str(), r.mode);
            if !seen.insert(key) {
                return Err(format!(
                    "V6 violation: duplicate BenchResult for fixture={} mode={:?}",
                    r.fixture_name, r.mode,
                )
                .into());
            }
        }
        Ok(())
    }

    /// Assert V5 — every `(fixture_name, mode)` in `results` must
    /// correspond to a fixture-mode combination declared in the
    /// manifest's `supported_modes` list. Called by the runner at
    /// Run-emission time (T023) with the manifest it just loaded.
    pub fn validate_against_manifest(
        &self,
        manifest: &[Fixture],
    ) -> Result<(), Box<dyn Error>> {
        let mut manifest_index: std::collections::HashMap<&str, &Fixture> =
            std::collections::HashMap::with_capacity(manifest.len());
        for f in manifest {
            manifest_index.insert(f.name.as_str(), f);
        }
        for r in &self.results {
            let fixture = manifest_index.get(r.fixture_name.as_str()).ok_or_else(|| -> Box<dyn Error> {
                format!(
                    "V5 violation: BenchResult references fixture={:?} which is \
                     not in the manifest",
                    r.fixture_name,
                )
                .into()
            })?;
            if !fixture.supported_modes.contains(&r.mode) {
                return Err(format!(
                    "V5 violation: BenchResult fixture={} references mode={:?} \
                     which is NOT in the manifest's supported_modes for that \
                     fixture (supported: {:?})",
                    r.fixture_name, r.mode, fixture.supported_modes,
                )
                .into());
            }
        }
        Ok(())
    }
}

// ────────────────────────────────────────────────────────────────
// Unit tests
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_kind_kebab_case_wire_shape() {
        assert_eq!(
            serde_json::to_string(&FixtureKind::SourceTree).unwrap(),
            "\"source-tree\""
        );
        assert_eq!(
            serde_json::to_string(&FixtureKind::ContainerImage).unwrap(),
            "\"container-image\""
        );
        assert_eq!(
            serde_json::to_string(&FixtureKind::BinarySet).unwrap(),
            "\"binary-set\""
        );
    }

    #[test]
    fn mode_kebab_case_wire_shape() {
        assert_eq!(
            serde_json::to_string(&Mode::Default).unwrap(),
            "\"default\""
        );
        assert_eq!(
            serde_json::to_string(&Mode::NoDeepHash).unwrap(),
            "\"no-deep-hash\""
        );
        assert_eq!(
            serde_json::to_string(&Mode::NoDeepHashPlusTripleFormat).unwrap(),
            "\"no-deep-hash-plus-triple-format\""
        );
        assert_eq!(
            serde_json::to_string(&Mode::FingerprintsCorpus).unwrap(),
            "\"fingerprints-corpus\""
        );
    }

    #[test]
    fn scan_class_kebab_case_wire_shape() {
        assert_eq!(
            serde_json::to_string(&ScanClass::Fast).unwrap(),
            "\"fast\""
        );
        assert_eq!(
            serde_json::to_string(&ScanClass::Medium).unwrap(),
            "\"medium\""
        );
        assert_eq!(
            serde_json::to_string(&ScanClass::Slow).unwrap(),
            "\"slow\""
        );
    }

    #[test]
    fn fixture_round_trips_through_json() {
        let f = Fixture {
            name: "cargo-workspace-medium".into(),
            path: "benchmark/source-tier/cargo-workspace-medium".into(),
            kind: FixtureKind::SourceTree,
            ecosystem: Some("cargo".into()),
            supported_modes: vec![
                Mode::Default,
                Mode::NoDeepHash,
                Mode::TripleFormat,
                Mode::NoDeepHashPlusTripleFormat,
            ],
            expected_scan_class: ScanClass::Medium,
        };
        let s = serde_json::to_string(&f).unwrap();
        let back: Fixture = serde_json::from_str(&s).unwrap();
        assert_eq!(f, back);
    }

    #[test]
    fn all_from_manifest_reads_a_valid_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest_path = tmp.path().join("manifest.json");
        let manifest_json = r#"{
            "fixtures": [
                {
                    "name": "cargo-small",
                    "path": "benchmark/source-tier/cargo-small",
                    "kind": "source-tree",
                    "ecosystem": "cargo",
                    "supported_modes": ["default", "no-deep-hash"],
                    "expected_scan_class": "fast"
                },
                {
                    "name": "debian-slim",
                    "path": "benchmark/container-images/debian-slim.tar",
                    "kind": "container-image",
                    "ecosystem": null,
                    "supported_modes": ["default", "triple-format"],
                    "expected_scan_class": "medium"
                }
            ]
        }"#;
        std::fs::write(&manifest_path, manifest_json).unwrap();

        let fixtures = Fixture::all_from_manifest(&manifest_path).unwrap();
        assert_eq!(fixtures.len(), 2);
        assert_eq!(fixtures[0].name, "cargo-small");
        assert_eq!(fixtures[0].kind, FixtureKind::SourceTree);
        assert_eq!(fixtures[0].ecosystem.as_deref(), Some("cargo"));
        assert_eq!(fixtures[1].name, "debian-slim");
        assert_eq!(fixtures[1].kind, FixtureKind::ContainerImage);
        assert_eq!(fixtures[1].ecosystem, None);
    }

    #[test]
    fn all_from_manifest_rejects_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist.json");
        assert!(Fixture::all_from_manifest(&missing).is_err());
    }

    // ────────────────────────────────────────────────────────────
    // T009 — BenchResult + validate() tests
    // ────────────────────────────────────────────────────────────

    /// A helper that returns a well-formed BenchResult for tests to
    /// mutate. Median of [100, 200, 300, 400, 500] is 300 (the
    /// middle sorted value).
    fn valid_result() -> BenchResult {
        BenchResult {
            fixture_name: "cargo-workspace-medium".into(),
            mode: Mode::Default,
            median_wall_clock_ms: 300,
            max_rss_kb: 47280,
            output_bytes: 82734,
            component_count: 234,
            exit_status: ExitStatus::Success,
            waybill_commit_sha: "0000000000000000000000000000000000000000".into(),
            fixture_sha: "1111111111111111111111111111111111111111".into(),
            raw_samples_ms: [100, 200, 300, 400, 500],
        }
    }

    #[test]
    fn exit_status_kebab_case_wire_shape() {
        assert_eq!(
            serde_json::to_string(&ExitStatus::Success).unwrap(),
            "\"success\""
        );
        assert_eq!(
            serde_json::to_string(&ExitStatus::Timeout).unwrap(),
            "\"timeout\""
        );
        assert_eq!(
            serde_json::to_string(&ExitStatus::CorpusUnreachable).unwrap(),
            "\"corpus-unreachable\""
        );
        assert_eq!(
            serde_json::to_string(&ExitStatus::NonZeroExitCode).unwrap(),
            "\"non-zero-exit-code\""
        );
        assert_eq!(
            serde_json::to_string(&ExitStatus::SchemaParseError).unwrap(),
            "\"schema-parse-error\""
        );
    }

    #[test]
    fn bench_result_round_trips_through_json() {
        let r = valid_result();
        let s = serde_json::to_string(&r).unwrap();
        let back: BenchResult = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn validate_passes_on_well_formed_result() {
        assert!(valid_result().validate().is_ok());
    }

    #[test]
    fn validate_passes_even_when_samples_are_unsorted() {
        // The runner emits samples in the order they were taken;
        // sorting to find the median is validate()'s job. This
        // asserts we don't accidentally require pre-sorted input.
        let mut r = valid_result();
        r.raw_samples_ms = [500, 100, 400, 200, 300];
        r.median_wall_clock_ms = 300; // still sorted[2] = 300
        assert!(r.validate().is_ok());
    }

    #[test]
    fn validate_rejects_v3_wrong_median() {
        let mut r = valid_result();
        r.median_wall_clock_ms = 250; // not the middle-of-sorted
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("V3 violation"));
    }

    #[test]
    fn validate_rejects_v4_empty_waybill_sha() {
        let mut r = valid_result();
        r.waybill_commit_sha = String::new();
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("V4 violation"));
        assert!(err.to_string().contains("waybill_commit_sha"));
    }

    #[test]
    fn validate_rejects_v4_short_sha() {
        let mut r = valid_result();
        r.waybill_commit_sha = "abc123".into(); // too short
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("V4 violation"));
    }

    #[test]
    fn validate_rejects_v4_uppercase_sha() {
        let mut r = valid_result();
        r.waybill_commit_sha = "ABCDEF0123456789ABCDEF0123456789ABCDEF01".into();
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("V4 violation"));
    }

    #[test]
    fn validate_rejects_v4_non_hex_sha() {
        let mut r = valid_result();
        r.waybill_commit_sha = "z000000000000000000000000000000000000000".into();
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("V4 violation"));
    }

    #[test]
    fn validate_rejects_v4_empty_fixture_sha() {
        let mut r = valid_result();
        r.fixture_sha = String::new();
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("V4 violation"));
        assert!(err.to_string().contains("fixture_sha"));
    }

    #[test]
    fn is_valid_sha_edge_cases() {
        assert!(is_valid_sha("0000000000000000000000000000000000000000"));
        assert!(is_valid_sha("abcdef0123456789abcdef0123456789abcdef01"));
        assert!(!is_valid_sha("")); // empty
        assert!(!is_valid_sha("abc")); // too short
        assert!(!is_valid_sha(
            "0000000000000000000000000000000000000000a" // too long (41)
        ));
        assert!(!is_valid_sha("ABCDEF0123456789ABCDEF0123456789ABCDEF01")); // uppercase
        assert!(!is_valid_sha("g000000000000000000000000000000000000000")); // non-hex
    }

    // ────────────────────────────────────────────────────────────
    // T010 — BenchRun + RunMetadata + NoiseClass tests
    // ────────────────────────────────────────────────────────────

    fn valid_metadata() -> RunMetadata {
        RunMetadata {
            waybill_commit_sha: "0000000000000000000000000000000000000000".into(),
            fixture_sha: "1111111111111111111111111111111111111111".into(),
            runner_uname: "Linux ci-runner 6.5.0-generic x86_64".into(),
            noise_class: NoiseClass::Reference,
            started_at: "2026-08-29T00:00:00Z".into(),
            finished_at: "2026-08-29T00:15:00Z".into(),
            total_duration_sec: 900,
        }
    }

    fn valid_run(results: Vec<BenchResult>) -> BenchRun {
        BenchRun {
            schema_version: BenchRun::schema_version(),
            metadata: valid_metadata(),
            results,
        }
    }

    fn valid_manifest() -> Vec<Fixture> {
        vec![
            Fixture {
                name: "cargo-workspace-medium".into(),
                path: "benchmark/source-tier/cargo-workspace-medium".into(),
                kind: FixtureKind::SourceTree,
                ecosystem: Some("cargo".into()),
                supported_modes: vec![Mode::Default, Mode::NoDeepHash],
                expected_scan_class: ScanClass::Medium,
            },
            Fixture {
                name: "debian-slim".into(),
                path: "benchmark/container-images/debian-slim.tar".into(),
                kind: FixtureKind::ContainerImage,
                ecosystem: None,
                supported_modes: vec![Mode::Default, Mode::TripleFormat],
                expected_scan_class: ScanClass::Slow,
            },
        ]
    }

    #[test]
    fn schema_version_is_one() {
        assert_eq!(BenchRun::schema_version(), 1);
    }

    #[test]
    fn noise_class_kebab_case_wire_shape() {
        assert_eq!(
            serde_json::to_string(&NoiseClass::Reference).unwrap(),
            "\"reference\""
        );
        assert_eq!(
            serde_json::to_string(&NoiseClass::Noisy).unwrap(),
            "\"noisy\""
        );
        assert_eq!(
            serde_json::to_string(&NoiseClass::Other).unwrap(),
            "\"other\""
        );
    }

    #[test]
    fn bench_run_round_trips_through_json() {
        let run = valid_run(vec![valid_result()]);
        let s = serde_json::to_string(&run).unwrap();
        let back: BenchRun = serde_json::from_str(&s).unwrap();
        assert_eq!(run, back);
    }

    #[test]
    fn bench_run_wire_shape_has_schema_version_at_root() {
        // Contract json-schema.md C-1: schema_version MUST be at root.
        let run = valid_run(vec![]);
        let v: serde_json::Value = serde_json::to_value(&run).unwrap();
        assert_eq!(v.get("schema_version").and_then(|v| v.as_u64()), Some(1));
    }

    #[test]
    fn validate_passes_on_well_formed_run() {
        let run = valid_run(vec![valid_result()]);
        assert!(run.validate().is_ok());
    }

    #[test]
    fn validate_rejects_v1_wrong_schema_version() {
        let mut run = valid_run(vec![]);
        run.schema_version = 2; // future version we can't read
        let err = run.validate().unwrap_err();
        assert!(err.to_string().contains("V1 violation"));
    }

    #[test]
    fn validate_rejects_v6_duplicate_fixture_mode() {
        let r1 = valid_result();
        let r2 = valid_result(); // same (fixture, mode) pair
        let run = valid_run(vec![r1, r2]);
        let err = run.validate().unwrap_err();
        assert!(err.to_string().contains("V6 violation"));
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn validate_accepts_same_fixture_different_mode() {
        // Two results with same fixture but different mode is legal;
        // that's the primary reason the matrix runs.
        let mut r1 = valid_result();
        r1.mode = Mode::Default;
        let mut r2 = valid_result();
        r2.mode = Mode::NoDeepHash;
        let run = valid_run(vec![r1, r2]);
        assert!(run.validate().is_ok());
    }

    #[test]
    fn validate_against_manifest_passes_on_well_formed_run() {
        let mut r = valid_result();
        r.fixture_name = "cargo-workspace-medium".into();
        r.mode = Mode::Default;
        let run = valid_run(vec![r]);
        assert!(run.validate_against_manifest(&valid_manifest()).is_ok());
    }

    #[test]
    fn validate_against_manifest_rejects_v5_unknown_fixture() {
        let mut r = valid_result();
        r.fixture_name = "unknown-fixture".into();
        let run = valid_run(vec![r]);
        let err = run.validate_against_manifest(&valid_manifest()).unwrap_err();
        assert!(err.to_string().contains("V5 violation"));
        assert!(err.to_string().contains("not in the manifest"));
    }

    #[test]
    fn validate_against_manifest_rejects_v5_mode_not_supported_by_fixture() {
        // cargo-workspace-medium supports [Default, NoDeepHash];
        // requesting FingerprintsCorpus is a V5 violation.
        let mut r = valid_result();
        r.fixture_name = "cargo-workspace-medium".into();
        r.mode = Mode::FingerprintsCorpus;
        let run = valid_run(vec![r]);
        let err = run.validate_against_manifest(&valid_manifest()).unwrap_err();
        assert!(err.to_string().contains("V5 violation"));
        assert!(err.to_string().contains("NOT in the manifest"));
    }
}
