// milestone 770 - see specs/770-sbom-quality-corpus/plan.md
//
// T009: measurement / violation / report types per data-model.md §§4-5
//       and contract quality-report.md.
// T017: atomic JSON write.
// T018: human summary table.
//
// Measurement fields are Option and serialize as ABSENT (never zero) when
// a target is unmeasurable — a zero is indistinguishable from a genuine
// collapse to zero, which is the misreading Principle X exists to prevent.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::quality::config::{Range, RangeF, TargetName};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnmeasurableReason {
    FetchFailed { detail: String },
    ScanFailed { detail: String },
    ScanTimedOut { budget_secs: u64 },
    ScoringFailed { detail: String },
    NoDocumentEmitted,
}

impl fmt::Display for UnmeasurableReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FetchFailed { detail } => write!(f, "fetch failed: {detail}"),
            Self::ScanFailed { detail } => write!(f, "scan failed: {detail}"),
            Self::ScanTimedOut { budget_secs } => {
                write!(f, "scan exceeded {budget_secs}s budget")
            }
            Self::ScoringFailed { detail } => write!(f, "scoring failed: {detail}"),
            Self::NoDocumentEmitted => write!(f, "no SBOM document emitted"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementStatus {
    Measured,
    Unmeasurable(UnmeasurableReason),
}

/// One target's observations. Every measurement is `Option` so an
/// unmeasurable target omits them entirely (contract C-2.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetMeasurement {
    pub name: TargetName,
    pub status: MeasurementStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_ms: Option<u64>,
    /// Keyed by format name. Only `"cyclonedx"` is populated this
    /// milestone; the map shape is what makes adding SPDX additive
    /// (FR-030) — do not flatten it to a bare number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sbomqs: Option<BTreeMap<String, f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pkgs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edges: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes_with_out_edges: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flat: Option<bool>,
    /// waybill's own self-report, recorded verbatim and NEVER compared
    /// against an expectation (research R3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_completeness: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sbom_bytes: Option<u64>,
}

impl TargetMeasurement {
    pub fn unmeasurable(name: TargetName, reason: UnmeasurableReason) -> Self {
        Self {
            name,
            status: MeasurementStatus::Unmeasurable(reason),
            wall_ms: None,
            sbomqs: None,
            pkgs: None,
            files: None,
            edges: None,
            nodes_with_out_edges: None,
            max_depth: None,
            flat: None,
            graph_completeness: None,
            sbom_bytes: None,
        }
    }

    pub fn cyclonedx_score(&self) -> Option<f64> {
        self.sbomqs.as_ref()?.get("cyclonedx").copied()
    }

    pub fn is_unmeasurable(&self) -> bool {
        matches!(self.status, MeasurementStatus::Unmeasurable(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    WallMs,
    Sbomqs,
    Pkgs,
    Files,
    Edges,
    MaxDepth,
    Flat,
}

impl fmt::Display for MetricKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::WallMs => "wall_ms",
            Self::Sbomqs => "sbomqs",
            Self::Pkgs => "pkgs",
            Self::Files => "files",
            Self::Edges => "edges",
            Self::MaxDepth => "max_depth",
            Self::Flat => "flat",
        };
        f.pad(s)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExpectedBound {
    Int(Range),
    Float(RangeF),
    Flat(bool),
}

impl fmt::Display for ExpectedBound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(r) => write!(f, "{}..{}", r.min, r.max),
            Self::Float(r) => write!(f, "{:.2}..{:.2}", r.min, r.max),
            Self::Flat(b) => write!(f, "{b}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ObservedValue {
    Int(u64),
    Float(f64),
    Bool(bool),
}

impl fmt::Display for ObservedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v:.2}"),
            Self::Bool(v) => write!(f, "{v}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Violation {
    pub target: TargetName,
    pub metric: MetricKind,
    pub expected: ExpectedBound,
    pub observed: ObservedValue,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}  {}  expected {}, observed {}",
            self.target, self.metric, self.expected, self.observed
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityReport {
    pub schema_version: u32,
    pub waybill_sha: String,
    pub corpus_sha: String,
    pub sbomqs_version: String,
    pub started_at: String,
    pub finished_at: String,
    pub runner: String,
    pub measurements: Vec<TargetMeasurement>,
    pub violations: Vec<Violation>,
    pub config_errors: Vec<String>,
}

impl QualityReport {
    /// FR-026: deterministic ordering so two runs over identical inputs
    /// differ only in genuinely varying measurements.
    pub fn sort(&mut self) {
        self.measurements.sort_by(|a, b| a.name.cmp(&b.name));
        self.violations
            .sort_by(|a, b| (&a.target, a.metric).cmp(&(&b.target, b.metric)));
    }

    pub fn unmeasurable_count(&self) -> usize {
        self.measurements.iter().filter(|m| m.is_unmeasurable()).count()
    }
}

/// Atomic write (temp file + rename), mirroring `bench::write_run_atomically`.
pub fn write_report(report: &QualityReport, path: &Path) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let json = serde_json::to_vec_pretty(report)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(&json)?;
    tmp.flush()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Human summary per contract quality-report.md § C-4.
pub fn render_summary(report: &QualityReport) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "{:<26}{:>9}{:>8}{:>7}{:>7}{:>7}{:>7}{:>6}  {}\n",
        "target", "wall", "sbomqs", "pkgs", "files", "edges", "depth", "flat", "waybill-says"
    ));
    s.push_str(&"-".repeat(100));
    s.push('\n');

    for m in &report.measurements {
        if let MeasurementStatus::Unmeasurable(reason) = &m.status {
            s.push_str(&format!("{:<26}  UNMEASURABLE: {}\n", m.name, reason));
            continue;
        }
        let f = |o: Option<u64>| o.map(|v| v.to_string()).unwrap_or_else(|| "-".into());
        s.push_str(&format!(
            "{:<26}{:>7}ms{:>8}{:>7}{:>7}{:>7}{:>7}{:>6}  {}\n",
            m.name,
            f(m.wall_ms),
            m.cyclonedx_score()
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "-".into()),
            f(m.pkgs),
            f(m.files),
            f(m.edges),
            f(m.max_depth),
            m.flat
                .map(|b| if b { "YES" } else { "no" })
                .unwrap_or("-"),
            m.graph_completeness.as_deref().unwrap_or("-"),
        ));
    }

    if !report.config_errors.is_empty() {
        s.push_str(&format!(
            "\nCONFIGURATION ERRORS ({})\n",
            report.config_errors.len()
        ));
        for e in &report.config_errors {
            s.push_str(&format!("  {e}\n"));
        }
    }

    let unmeasurable = report.unmeasurable_count();
    if report.violations.is_empty() && unmeasurable == 0 {
        // C-4.1: say so explicitly rather than printing nothing.
        s.push_str("\nNo violations. Every measured value is inside its authored range (or unranged).\n");
    } else {
        if !report.violations.is_empty() {
            s.push_str(&format!("\nVIOLATIONS ({})\n", report.violations.len()));
            for v in &report.violations {
                s.push_str(&format!("  {v}\n"));
            }
        }
        if unmeasurable > 0 {
            s.push_str(&format!(
                "\n{unmeasurable} target(s) could not be measured — see the table above.\n"
            ));
        }
    }
    s
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn name(s: &str) -> TargetName {
        serde_json::from_str(&format!("\"{s}\"")).unwrap()
    }

    fn measured(n: &str, pkgs: u64) -> TargetMeasurement {
        let mut m = TargetMeasurement::unmeasurable(name(n), UnmeasurableReason::NoDocumentEmitted);
        m.status = MeasurementStatus::Measured;
        m.wall_ms = Some(100);
        m.pkgs = Some(pkgs);
        m.files = Some(1);
        m.edges = Some(5);
        m.max_depth = Some(2);
        m.flat = Some(false);
        m.graph_completeness = Some("partial".into());
        let mut sc = BTreeMap::new();
        sc.insert("cyclonedx".to_string(), 7.5);
        m.sbomqs = Some(sc);
        m
    }

    fn report(ms: Vec<TargetMeasurement>, vs: Vec<Violation>) -> QualityReport {
        QualityReport {
            schema_version: SCHEMA_VERSION,
            waybill_sha: "abc123".into(),
            corpus_sha: "def456".into(),
            sbomqs_version: "v2.0.6".into(),
            started_at: "2026-09-03T22:00:00Z".into(),
            finished_at: "2026-09-03T22:05:00Z".into(),
            runner: "test".into(),
            measurements: ms,
            violations: vs,
            config_errors: vec![],
        }
    }

    /// Contract C-2.3: unmeasurable targets omit measurement fields
    /// entirely. A zero would be indistinguishable from a real collapse.
    #[test]
    fn unmeasurable_omits_measurement_fields_rather_than_zeroing() {
        let m = TargetMeasurement::unmeasurable(
            name("x"),
            UnmeasurableReason::ScanTimedOut { budget_secs: 600 },
        );
        let j = serde_json::to_string(&m).unwrap();
        assert!(!j.contains("pkgs"), "{j}");
        assert!(!j.contains("wall_ms"), "{j}");
        assert!(j.contains("scan_timed_out"), "{j}");
    }

    #[test]
    fn sbomqs_serializes_as_a_map_keyed_by_format() {
        let m = measured("x", 10);
        let j = serde_json::to_string(&m).unwrap();
        assert!(j.contains(r#""sbomqs":{"cyclonedx":7.5}"#), "{j}");
    }

    #[test]
    fn sort_orders_measurements_and_violations_deterministically() {
        let mut r = report(
            vec![measured("zeta", 1), measured("alpha", 2)],
            vec![
                Violation {
                    target: name("zeta"),
                    metric: MetricKind::Pkgs,
                    expected: ExpectedBound::Int(Range::new(1, 2).unwrap()),
                    observed: ObservedValue::Int(9),
                },
                Violation {
                    target: name("alpha"),
                    metric: MetricKind::Files,
                    expected: ExpectedBound::Int(Range::new(1, 2).unwrap()),
                    observed: ObservedValue::Int(9),
                },
            ],
        );
        r.sort();
        assert_eq!(r.measurements[0].name.as_str(), "alpha");
        assert_eq!(r.violations[0].target.as_str(), "alpha");
    }

    #[test]
    fn clean_summary_says_so_explicitly() {
        let r = report(vec![measured("a", 1)], vec![]);
        let s = render_summary(&r);
        assert!(s.contains("No violations"), "{s}");
    }

    #[test]
    fn summary_names_target_metric_expected_and_observed() {
        let r = report(
            vec![measured("vue", 412)],
            vec![Violation {
                target: name("vue"),
                metric: MetricKind::Pkgs,
                expected: ExpectedBound::Int(Range::new(600, 700).unwrap()),
                observed: ObservedValue::Int(412),
            }],
        );
        let s = render_summary(&r);
        assert!(s.contains("VIOLATIONS (1)"), "{s}");
        assert!(s.contains("vue"), "{s}");
        assert!(s.contains("600..700"), "{s}");
        assert!(s.contains("412"), "{s}");
    }

    #[test]
    fn summary_reports_unmeasurable_targets_distinctly_from_violations() {
        let r = report(
            vec![TargetMeasurement::unmeasurable(
                name("gone"),
                UnmeasurableReason::FetchFailed { detail: "404".into() },
            )],
            vec![],
        );
        let s = render_summary(&r);
        assert!(s.contains("UNMEASURABLE"), "{s}");
        assert!(!s.contains("No violations"), "a failed fetch must not read as clean: {s}");
    }

    #[test]
    fn report_round_trips_through_json() {
        let r = report(vec![measured("a", 1)], vec![]);
        let j = serde_json::to_string(&r).unwrap();
        let back: QualityReport = serde_json::from_str(&j).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn write_report_creates_parents_and_persists() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("nested/deep/run.json");
        write_report(&report(vec![measured("a", 1)], vec![]), &out).unwrap();
        assert!(out.exists());
    }
}
