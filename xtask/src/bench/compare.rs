// milestone 669 - see specs/669-bench-harness/plan.md
// Regression-diff logic — subject BenchRun vs baseline BenchRun.
//
// T029: compare() — per-fixture-mode, per-dimension percentage-delta
//       classifier (regression / improvement / matrix asymmetry).
// T030: unit tests over compare() semantics.
//
// Dimension semantics:
// - WallClockMs: positive delta ≥ threshold = regression (slower).
// - MaxRssKb: positive delta ≥ threshold = regression (memory bloat).
// - OutputBytes: positive delta ≥ threshold = regression (bigger SBOM).
// - ComponentCount: positive delta ≥ threshold = regression (SBOM
//   SHAPE drift — could be new coverage OR a bug; flagged either way
//   for investigation per data-model.md §5 Dimension doc-string).
// Negative deltas ≥ threshold in absolute magnitude on any dimension
// are recorded as improvements (informational, never a failure).
//
// WallClockMs and MaxRssKb additionally carry an absolute noise floor
// (#833) — below it, a percentage delta measures the host, not the code.

use std::collections::HashMap;

use crate::bench::schema::{
    BenchResult, BenchRun, Dimension, MatrixAsymmetryEntry, MatrixSide, Mode,
    RegressionDiff, RegressionEntry,
};

/// Compare a subject BenchRun against a baseline BenchRun at the
/// given threshold. Returns a fully-populated RegressionDiff.
///
/// Fixture-mode combinations present in both are compared across all
/// 4 dimensions; combinations in only one are recorded as
/// `matrix_asymmetry` (informational, not a failure per data-model V8).
///
/// Threshold semantics: `threshold = 0.25` means "flag any dimension
/// whose subject differs from baseline by ≥ 25%". Zero-baseline
/// values are treated specially — if baseline is 0 AND subject is >0,
/// that's a subject-only surface (baseline never measured that
/// dimension); recorded as an improvement iff subject value is
/// meaningfully large (>0 for count, >0 for bytes). We treat
/// zero-baseline as "no signal" and skip the percentage_delta
/// computation to avoid divide-by-zero + false regressions.
///
/// Threshold alone is not sufficient. `WallClockMs` and `MaxRssKb` also
/// carry an absolute noise floor (see `noise_floor`): a dimension whose
/// baseline AND subject both sit below it is skipped, because a ratio
/// computed over values that small reports host jitter as a code change.
/// `OutputBytes` and `ComponentCount` have no floor — they are
/// deterministic for a fixed fixture and binary.
pub fn compare(subject: &BenchRun, baseline: &BenchRun, threshold: f64) -> RegressionDiff {
    let subject_ix = index_results(&subject.results);
    let baseline_ix = index_results(&baseline.results);

    let mut regressions: Vec<RegressionEntry> = Vec::new();
    let mut improvements: Vec<RegressionEntry> = Vec::new();
    let mut matrix_asymmetry: Vec<MatrixAsymmetryEntry> = Vec::new();

    // Fixture-modes in subject: either overlap (compare) or subject-only (asymmetry).
    for (key, sr) in &subject_ix {
        match baseline_ix.get(key) {
            Some(br) => classify_dimensions(sr, br, threshold, &mut regressions, &mut improvements),
            None => matrix_asymmetry.push(MatrixAsymmetryEntry {
                fixture_name: key.0.clone(),
                mode: key.1,
                side: MatrixSide::SubjectOnly,
            }),
        }
    }
    // Fixture-modes in baseline but not subject: dropped-coverage asymmetry.
    for key in baseline_ix.keys() {
        if !subject_ix.contains_key(key) {
            matrix_asymmetry.push(MatrixAsymmetryEntry {
                fixture_name: key.0.clone(),
                mode: key.1,
                side: MatrixSide::BaselineOnly,
            });
        }
    }

    // Deterministic ordering — sort each vector so the emitted JSON
    // is diff-friendly (matters for the m669 baseline commit workflow).
    regressions.sort_by_key(entry_sort_key);
    improvements.sort_by_key(entry_sort_key);
    matrix_asymmetry.sort_by_key(|a| (a.fixture_name.clone(), format!("{:?}", a.mode), a.side));

    RegressionDiff {
        subject_sha: subject.metadata.waybill_commit_sha.clone(),
        baseline_sha: baseline.metadata.waybill_commit_sha.clone(),
        threshold,
        regressions,
        improvements,
        matrix_asymmetry,
    }
}

/// Index a slice of BenchResults by (fixture_name, mode). V6 already
/// guarantees no duplicates within a single BenchRun.
fn index_results(rs: &[BenchResult]) -> HashMap<(String, Mode), &BenchResult> {
    rs.iter()
        .map(|r| ((r.fixture_name.clone(), r.mode), r))
        .collect()
}

/// Wall-clock below this is dominated by process spawn and reap, not by
/// anything waybill does. Issue #833: `cargo-workspace-medium` was gated
/// as a +100% regression on a 50ms → 100ms move.
const WALL_CLOCK_FLOOR_MS: f64 = 200.0;

/// Peak RSS below this is dominated by allocator and scheduler behaviour.
/// Issue #833: one bench run reported `MaxRssKb` deltas from -73.6% to
/// +222.8% across fixtures simultaneously, and the baseline disagreed with
/// itself — 18540 KB vs 4884 KB for two modes of the same fixture. Every
/// current fixture measures between 2.5 MB and 18 MB, so in practice this
/// floor stops `MaxRssKb` gating anything today. That is deliberate: a
/// dimension whose noise exceeds its signal cannot support a 25% gate at
/// any threshold. It re-activates on its own if a fixture ever grows large
/// enough for peak RSS to carry signal.
const MAX_RSS_FLOOR_KB: f64 = 51_200.0;

/// Absolute magnitude below which a percentage delta is not evidence.
///
/// Returns `None` for dimensions that are deterministic for a fixed
/// fixture and binary — `OutputBytes` and `ComponentCount` do not vary
/// with host load, so a small absolute change there is real signal (one
/// component appearing on a seven-component fixture is a 14% shift worth
/// seeing). Only the two host-sensitive dimensions get a floor; they are
/// the same two that `assert_baseline_is_comparable` already describes as
/// non-portable across host classes. That guard catches cross-class
/// comparison; this floor catches within-class noise at small magnitudes,
/// which is the case it cannot see.
fn noise_floor(dim: Dimension) -> Option<f64> {
    match dim {
        Dimension::WallClockMs => Some(WALL_CLOCK_FLOOR_MS),
        Dimension::MaxRssKb => Some(MAX_RSS_FLOOR_KB),
        Dimension::OutputBytes | Dimension::ComponentCount => None,
    }
}

/// Populate `regressions` and `improvements` for one overlapping
/// fixture-mode pair across all 4 dimensions.
fn classify_dimensions(
    subject: &BenchResult,
    baseline: &BenchResult,
    threshold: f64,
    regressions: &mut Vec<RegressionEntry>,
    improvements: &mut Vec<RegressionEntry>,
) {
    for dim in [
        Dimension::WallClockMs,
        Dimension::MaxRssKb,
        Dimension::OutputBytes,
        Dimension::ComponentCount,
    ] {
        let sv = read_dim(subject, dim) as f64;
        let bv = read_dim(baseline, dim) as f64;
        // Skip zero-baseline entries: no meaningful percentage delta,
        // and we don't want to flag a subject-only value as regression.
        if bv == 0.0 {
            continue;
        }
        // Skip when the whole measurement sits inside the noise band.
        // Requires BOTH sides below the floor: a move from inside the
        // band to well outside it (400ms -> 10s) is still a regression
        // and still gates.
        if let Some(floor) = noise_floor(dim) {
            if sv < floor && bv < floor {
                continue;
            }
        }
        let delta = (sv - bv) / bv;
        if delta >= threshold {
            regressions.push(RegressionEntry {
                fixture_name: subject.fixture_name.clone(),
                mode: subject.mode,
                dimension: dim,
                baseline_value: bv,
                subject_value: sv,
                percentage_delta: delta,
            });
        } else if delta <= -threshold {
            improvements.push(RegressionEntry {
                fixture_name: subject.fixture_name.clone(),
                mode: subject.mode,
                dimension: dim,
                baseline_value: bv,
                subject_value: sv,
                percentage_delta: delta,
            });
        }
    }
}

fn read_dim(r: &BenchResult, d: Dimension) -> u64 {
    match d {
        Dimension::WallClockMs => r.median_wall_clock_ms,
        Dimension::MaxRssKb => r.max_rss_kb,
        Dimension::OutputBytes => r.output_bytes,
        Dimension::ComponentCount => r.component_count,
    }
}

fn entry_sort_key(e: &RegressionEntry) -> (String, String, String) {
    (
        e.fixture_name.clone(),
        format!("{:?}", e.mode),
        format!("{:?}", e.dimension),
    )
}

// ────────────────────────────────────────────────────────────────
// T030 — compare() unit tests
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::schema::{ExitStatus, NoiseClass, RunMetadata};

    fn valid_metadata(sha: &str) -> RunMetadata {
        RunMetadata {
            waybill_commit_sha: sha.into(),
            fixture_sha: "1111111111111111111111111111111111111111".into(),
            runner_uname: "Linux ci 6.5.0 x86_64".into(),
            noise_class: NoiseClass::Reference,
            started_at: "2026-08-29T00:00:00Z".into(),
            finished_at: "2026-08-29T00:15:00Z".into(),
            total_duration_sec: 900,
        }
    }

    fn result(fixture: &str, mode: Mode, wall: u64, rss: u64, bytes: u64, comp: u64) -> BenchResult {
        BenchResult {
            fixture_name: fixture.into(),
            mode,
            median_wall_clock_ms: wall,
            max_rss_kb: rss,
            output_bytes: bytes,
            component_count: comp,
            exit_status: ExitStatus::Success,
            waybill_commit_sha: "0000000000000000000000000000000000000000".into(),
            fixture_sha: "1111111111111111111111111111111111111111".into(),
            raw_samples_ms: [wall, wall, wall, wall, wall],
        }
    }

    fn run(sha: &str, rs: Vec<BenchResult>) -> BenchRun {
        BenchRun {
            schema_version: BenchRun::schema_version(),
            metadata: valid_metadata(sha),
            results: rs,
        }
    }

    #[test]
    fn compare_flags_wall_clock_regression_at_40_percent() {
        // Baseline 300ms; subject 420ms; delta +40% → regression.
        let baseline = run(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec![result("cargo-medium", Mode::Default, 300, 47000, 82000, 234)],
        );
        let subject = run(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            vec![result("cargo-medium", Mode::Default, 420, 47000, 82000, 234)],
        );
        let diff = compare(&subject, &baseline, 0.25);
        assert_eq!(diff.regressions.len(), 1);
        assert_eq!(diff.regressions[0].dimension, Dimension::WallClockMs);
        assert!(diff.regressions[0].percentage_delta > 0.39);
        assert!(diff.regressions[0].percentage_delta < 0.41);
        assert!(diff.improvements.is_empty());
        assert!(diff.matrix_asymmetry.is_empty());
    }

    #[test]
    fn compare_flags_rss_improvement_at_40_percent() {
        // Baseline 100 MB RSS; subject 60 MB RSS; delta -40% → improvement.
        let baseline = run(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec![result("cargo-medium", Mode::Default, 300, 100_000, 82000, 234)],
        );
        let subject = run(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            vec![result("cargo-medium", Mode::Default, 300, 60_000, 82000, 234)],
        );
        let diff = compare(&subject, &baseline, 0.25);
        assert_eq!(diff.improvements.len(), 1);
        assert_eq!(diff.improvements[0].dimension, Dimension::MaxRssKb);
        assert!(diff.improvements[0].percentage_delta < -0.39);
        assert!(diff.improvements[0].percentage_delta > -0.41);
        assert!(diff.regressions.is_empty());
    }

    #[test]
    fn noise_floor_suppresses_tiny_wall_clock_delta() {
        // #833: the exact row that gated the lane red — 50ms -> 100ms.
        // A doubling on paper, process-spawn jitter in reality.
        let baseline = run(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec![result("cargo-workspace-medium", Mode::Default, 50, 47000, 82000, 234)],
        );
        let subject = run(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            vec![result("cargo-workspace-medium", Mode::Default, 100, 47000, 82000, 234)],
        );
        let diff = compare(&subject, &baseline, 0.25);
        assert!(
            diff.regressions.is_empty(),
            "50ms -> 100ms is below the wall-clock floor; got {:?}",
            diff.regressions,
        );
    }

    #[test]
    fn noise_floor_suppresses_small_magnitude_rss_swing() {
        // #833: go-module-medium read +222.8% while other fixtures in the
        // same run read -73.6%. Both sides are a few MB.
        let baseline = run(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec![result("go-module-medium", Mode::Default, 300, 2512, 82000, 234)],
        );
        let subject = run(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            vec![result("go-module-medium", Mode::Default, 300, 8108, 82000, 234)],
        );
        let diff = compare(&subject, &baseline, 0.25);
        assert!(
            diff.regressions.is_empty(),
            "2.5MB -> 8MB is below the RSS floor; got {:?}",
            diff.regressions,
        );
        assert!(diff.improvements.is_empty());
    }

    #[test]
    fn noise_floor_does_not_mask_a_real_regression() {
        // The floor requires BOTH sides below it. A run that starts inside
        // the noise band and ends far outside it is still a regression --
        // otherwise the floor would hide the failures worth catching.
        let baseline = run(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec![result("cargo-medium", Mode::Default, 120, 4000, 82000, 234)],
        );
        let subject = run(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            vec![result("cargo-medium", Mode::Default, 10_000, 900_000, 82000, 234)],
        );
        let diff = compare(&subject, &baseline, 0.25);
        let dims: Vec<_> = diff.regressions.iter().map(|e| e.dimension).collect();
        assert!(
            dims.contains(&Dimension::WallClockMs),
            "120ms -> 10s must still gate; got {dims:?}",
        );
        assert!(
            dims.contains(&Dimension::MaxRssKb),
            "4MB -> 900MB must still gate; got {dims:?}",
        );
    }

    #[test]
    fn deterministic_dimensions_have_no_floor() {
        // A single component appearing on a small fixture is real signal,
        // not host noise, so ComponentCount is gated at any magnitude.
        let baseline = run(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec![result("tiny", Mode::Default, 300, 47000, 100, 4)],
        );
        let subject = run(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            vec![result("tiny", Mode::Default, 300, 47000, 100, 7)],
        );
        let diff = compare(&subject, &baseline, 0.25);
        assert_eq!(diff.regressions.len(), 1);
        assert_eq!(diff.regressions[0].dimension, Dimension::ComponentCount);
    }

    #[test]
    fn compare_flags_subject_only_as_asymmetry() {
        // Fixture in subject but not baseline → SubjectOnly asymmetry.
        let baseline = run("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", vec![]);
        let subject = run(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            vec![result("new-fixture", Mode::Default, 300, 47000, 82000, 234)],
        );
        let diff = compare(&subject, &baseline, 0.25);
        assert_eq!(diff.matrix_asymmetry.len(), 1);
        assert_eq!(diff.matrix_asymmetry[0].side, MatrixSide::SubjectOnly);
        assert_eq!(diff.matrix_asymmetry[0].fixture_name, "new-fixture");
        assert!(diff.regressions.is_empty());
        assert!(diff.improvements.is_empty());
    }

    #[test]
    fn compare_flags_baseline_only_as_asymmetry() {
        // Fixture in baseline but not subject → BaselineOnly asymmetry.
        let baseline = run(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec![result("dropped-fixture", Mode::Default, 300, 47000, 82000, 234)],
        );
        let subject = run("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", vec![]);
        let diff = compare(&subject, &baseline, 0.25);
        assert_eq!(diff.matrix_asymmetry.len(), 1);
        assert_eq!(diff.matrix_asymmetry[0].side, MatrixSide::BaselineOnly);
        assert_eq!(diff.matrix_asymmetry[0].fixture_name, "dropped-fixture");
    }

    #[test]
    fn compare_below_threshold_delta_is_no_op() {
        // 10% wall-clock delta at 25% threshold → no regression / no improvement.
        let baseline = run(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec![result("cargo-medium", Mode::Default, 300, 47000, 82000, 234)],
        );
        let subject = run(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            vec![result("cargo-medium", Mode::Default, 330, 47000, 82000, 234)],
        );
        let diff = compare(&subject, &baseline, 0.25);
        assert!(diff.regressions.is_empty());
        assert!(diff.improvements.is_empty());
        assert!(diff.matrix_asymmetry.is_empty());
    }

    #[test]
    fn compare_composite_scenario_matches_data_model_spec() {
        // T030 spec: (a) one 40% wall-clock regression, (b) one 40%
        // RSS improvement, (c) one fixture present only in subject.
        // Compose across 3 fixtures.
        let baseline = run(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec![
                result("regressing", Mode::Default, 300, 47000, 82000, 234),
                result("improving", Mode::Default, 300, 100_000, 82000, 234),
            ],
        );
        let subject = run(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            vec![
                result("regressing", Mode::Default, 420, 47000, 82000, 234),
                result("improving", Mode::Default, 300, 60_000, 82000, 234),
                result("newcomer", Mode::Default, 300, 47000, 82000, 234),
            ],
        );
        let diff = compare(&subject, &baseline, 0.25);
        assert_eq!(diff.regressions.len(), 1, "regressions: {:?}", diff.regressions);
        assert_eq!(diff.improvements.len(), 1, "improvements: {:?}", diff.improvements);
        assert_eq!(diff.matrix_asymmetry.len(), 1);
        assert_eq!(diff.regressions[0].fixture_name, "regressing");
        assert_eq!(diff.improvements[0].fixture_name, "improving");
        assert_eq!(diff.matrix_asymmetry[0].fixture_name, "newcomer");
        assert_eq!(diff.matrix_asymmetry[0].side, MatrixSide::SubjectOnly);
        assert_eq!(diff.threshold, 0.25);
    }

    #[test]
    fn compare_treats_zero_baseline_as_no_signal() {
        // Baseline had 0 components; subject has 234. Without the
        // zero-baseline guard, this would be a division-by-zero
        // regression flare-up. Should skip silently.
        let baseline = run(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec![result("empty-scan", Mode::Default, 300, 47000, 82000, 0)],
        );
        let subject = run(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            vec![result("empty-scan", Mode::Default, 300, 47000, 82000, 234)],
        );
        let diff = compare(&subject, &baseline, 0.25);
        // Component-count dimension skipped; other 3 dimensions
        // unchanged; net: no regressions, no improvements.
        assert!(diff.regressions.is_empty());
        assert!(diff.improvements.is_empty());
    }

    #[test]
    fn compare_flags_component_count_growth_as_regression() {
        // SBOM shape drift: subject emits 60% more components than
        // baseline. Per data-model §5 Dimension doc, positive delta
        // on ComponentCount is a regression (investigation warranted).
        let baseline = run(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec![result("cargo-medium", Mode::Default, 300, 47000, 82000, 100)],
        );
        let subject = run(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            vec![result("cargo-medium", Mode::Default, 300, 47000, 82000, 160)],
        );
        let diff = compare(&subject, &baseline, 0.25);
        assert_eq!(diff.regressions.len(), 1);
        assert_eq!(diff.regressions[0].dimension, Dimension::ComponentCount);
        assert!(diff.regressions[0].percentage_delta > 0.59);
        assert!(diff.regressions[0].percentage_delta < 0.61);
    }

    #[test]
    fn compare_records_subject_and_baseline_shas() {
        let baseline = run("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", vec![]);
        let subject = run("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", vec![]);
        let diff = compare(&subject, &baseline, 0.25);
        assert_eq!(diff.subject_sha, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        assert_eq!(diff.baseline_sha, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(diff.threshold, 0.25);
    }

    #[test]
    fn compare_sorts_entries_deterministically() {
        // Reordering the subject's Vec must not change the emitted
        // regression / improvement / asymmetry ordering.
        let baseline = run(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec![
                result("a-fixture", Mode::Default, 300, 47000, 82000, 234),
                result("b-fixture", Mode::Default, 300, 47000, 82000, 234),
            ],
        );
        let subject_asc = run(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            vec![
                result("a-fixture", Mode::Default, 420, 47000, 82000, 234),
                result("b-fixture", Mode::Default, 420, 47000, 82000, 234),
            ],
        );
        let subject_desc = run(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            vec![
                result("b-fixture", Mode::Default, 420, 47000, 82000, 234),
                result("a-fixture", Mode::Default, 420, 47000, 82000, 234),
            ],
        );
        let d1 = compare(&subject_asc, &baseline, 0.25);
        let d2 = compare(&subject_desc, &baseline, 0.25);
        assert_eq!(d1.regressions, d2.regressions);
    }
}
