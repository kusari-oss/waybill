// milestone 770 — T023/T024/T026: range evaluation.
//
// Every measurement on every target is evaluated before returning
// (FR-018): a first-failure exit hides the blast radius, and a maintainer
// needs to know whether one target regressed or all eighteen did.

use crate::quality::config::Expectations;
use crate::quality::report::{
    ExpectedBound, MeasurementStatus, MetricKind, ObservedValue, TargetMeasurement, Violation,
};

/// Compare one target's observations against its authored expectations.
/// Measurements with no expectation are skipped, never failed (FR-020).
pub fn evaluate(m: &TargetMeasurement, expect: Option<&Expectations>) -> Vec<Violation> {
    let mut out = Vec::new();
    // An unmeasurable target has nothing to compare; it fails the run via
    // the exit-code policy, not via a fabricated violation.
    if matches!(m.status, MeasurementStatus::Unmeasurable(_)) {
        return out;
    }
    let Some(exp) = expect else {
        return out;
    };

    let int_checks: [(MetricKind, Option<crate::quality::config::Range>, Option<u64>); 5] = [
        (MetricKind::WallMs, exp.wall_ms, m.wall_ms),
        (MetricKind::Pkgs, exp.pkgs, m.pkgs),
        (MetricKind::Files, exp.files, m.files),
        (MetricKind::Edges, exp.edges, m.edges),
        (MetricKind::MaxDepth, exp.max_depth, m.max_depth),
    ];
    for (metric, range, observed) in int_checks {
        if let (Some(r), Some(v)) = (range, observed) {
            if !r.contains(v) {
                out.push(Violation {
                    target: m.name.clone(),
                    metric,
                    expected: ExpectedBound::Int(r),
                    observed: ObservedValue::Int(v),
                });
            }
        }
    }

    if let (Some(r), Some(v)) = (exp.sbomqs, m.cyclonedx_score()) {
        if !r.contains(v) {
            out.push(Violation {
                target: m.name.clone(),
                metric: MetricKind::Sbomqs,
                expected: ExpectedBound::Float(r),
                observed: ObservedValue::Float(v),
            });
        }
    }

    // FR-022. NOTE FOR FUTURE MAINTAINERS: `graph_completeness` is
    // deliberately NOT evaluated here and must never be. It is waybill's
    // own self-report; three trial targets claimed `complete` while being
    // structurally flat (research R3). Gating on it would reintroduce
    // exactly the blind spot this milestone exists to close.
    if let (Some(want), Some(got)) = (exp.flat, m.flat) {
        if want != got {
            out.push(Violation {
                target: m.name.clone(),
                metric: MetricKind::Flat,
                expected: ExpectedBound::Flat(want),
                observed: ObservedValue::Bool(got),
            });
        }
    }

    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::quality::config::{Range, RangeF, TargetName};
    use crate::quality::report::UnmeasurableReason;
    use std::collections::BTreeMap;

    fn name(s: &str) -> TargetName {
        serde_json::from_str(&format!("\"{s}\"")).unwrap()
    }

    fn m_with(pkgs: u64, score: f64, flat: bool) -> TargetMeasurement {
        let mut m =
            TargetMeasurement::unmeasurable(name("t"), UnmeasurableReason::NoDocumentEmitted);
        m.status = MeasurementStatus::Measured;
        m.pkgs = Some(pkgs);
        m.flat = Some(flat);
        let mut sc = BTreeMap::new();
        sc.insert("cyclonedx".into(), score);
        m.sbomqs = Some(sc);
        m
    }

    fn exp_pkgs(min: u64, max: u64) -> Expectations {
        Expectations { pkgs: Some(Range::new(min, max).unwrap()), ..Default::default() }
    }

    #[test]
    fn value_at_lower_bound_passes() {
        assert!(evaluate(&m_with(10, 5.0, false), Some(&exp_pkgs(10, 20))).is_empty());
    }

    #[test]
    fn value_at_upper_bound_passes() {
        assert!(evaluate(&m_with(20, 5.0, false), Some(&exp_pkgs(10, 20))).is_empty());
    }

    #[test]
    fn value_below_bound_fails() {
        let v = evaluate(&m_with(9, 5.0, false), Some(&exp_pkgs(10, 20)));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].metric, MetricKind::Pkgs);
    }

    #[test]
    fn value_above_bound_fails() {
        let v = evaluate(&m_with(21, 5.0, false), Some(&exp_pkgs(10, 20)));
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn absent_expectation_never_fails() {
        // FR-020 — this is what makes the corpus landable with no ranges.
        assert!(evaluate(&m_with(9999, 0.1, true), None).is_empty());
        assert!(evaluate(&m_with(9999, 0.1, true), Some(&Expectations::default())).is_empty());
    }

    #[test]
    fn expected_not_flat_but_observed_flat_fails() {
        let e = Expectations { flat: Some(false), ..Default::default() };
        let v = evaluate(&m_with(5, 5.0, true), Some(&e));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].metric, MetricKind::Flat);
    }

    #[test]
    fn expected_flat_and_observed_flat_passes() {
        // Legitimate for lockfile-less upstreams (express, ansible).
        let e = Expectations { flat: Some(true), ..Default::default() };
        assert!(evaluate(&m_with(5, 5.0, true), Some(&e)).is_empty());
    }

    #[test]
    fn float_bounds_are_inclusive() {
        let e = Expectations {
            sbomqs: Some(RangeF::new(5.75, 7.70).unwrap()),
            ..Default::default()
        };
        assert!(evaluate(&m_with(1, 5.75, false), Some(&e)).is_empty());
        assert!(evaluate(&m_with(1, 7.70, false), Some(&e)).is_empty());
        assert_eq!(evaluate(&m_with(1, 7.71, false), Some(&e)).len(), 1);
    }

    #[test]
    fn multiple_metrics_all_reported_not_just_the_first() {
        // FR-018.
        let e = Expectations {
            pkgs: Some(Range::new(1, 2).unwrap()),
            sbomqs: Some(RangeF::new(9.0, 10.0).unwrap()),
            flat: Some(false),
            ..Default::default()
        };
        let v = evaluate(&m_with(500, 3.0, true), Some(&e));
        assert_eq!(v.len(), 3, "expected pkgs + sbomqs + flat, got {v:?}");
    }

    #[test]
    fn unmeasurable_target_yields_no_fabricated_violations() {
        // It fails the run via the exit-code policy, not by pretending the
        // measurements collapsed to zero (Principle X).
        let m = TargetMeasurement::unmeasurable(
            name("gone"),
            UnmeasurableReason::FetchFailed { detail: "404".into() },
        );
        assert!(evaluate(&m, Some(&exp_pkgs(10, 20))).is_empty());
    }
}
