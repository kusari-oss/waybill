// milestone 780 — private comparative benchmark harness.
// Spec:     specs/780-comparative-bench-harness/spec.md
// Plan:     specs/780-comparative-bench-harness/plan.md
// Contract: specs/780-comparative-bench-harness/contracts/xtask-compare-cli.md
//
// Accuracy scoring against a declared truth set (FR-008, FR-008a, FR-008b,
// FR-009, SC-008).

use std::collections::BTreeSet;

use super::config::TruthMethod;
use super::identity::PackageIdentity;
use super::truth::TruthSet;

/// How a tool scored against truth. Absent when the target declared none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accuracy {
    pub method: TruthMethod,
    pub truth_size: usize,
    /// In truth AND reported.
    pub found: usize,
    /// In truth, not reported.
    pub missed: usize,
    /// Reported, not in truth.
    pub extra: usize,
    /// FR-008b. When true, `found` rewards over-reporting and `extra` is not
    /// necessarily wrong — the truth set contains things the build may never
    /// use. Every presentation of this score must say so.
    pub truth_is_superset: bool,
}

/// Why a target produced no accuracy score. FR-009 requires this be stated
/// rather than rendered as a blank or a zero, both of which read as "the
/// tool found nothing".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotScored {
    /// The target declared no truth method.
    NoTruthDeclared { ecosystem: String },
    /// The tool did not produce usable output, so there is nothing to score
    /// (SC-008). Scoring a crash as zero coverage would turn a failure into
    /// a result.
    ToolDidNotSucceed { outcome: String },
}

impl NotScored {
    pub fn reason(&self) -> String {
        match self {
            Self::NoTruthDeclared { ecosystem } => format!(
                "accuracy not scored: no truth-derivation method is declared \
                 for this target (ecosystem: {ecosystem})"
            ),
            Self::ToolDidNotSucceed { outcome } => format!(
                "accuracy not scored: the tool did not succeed ({outcome}); \
                 a tool that failed found nothing BECAUSE it failed"
            ),
        }
    }
}

/// The scoring outcome for one tool on one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scored {
    Yes(Box<Accuracy>),
    No(NotScored),
}

/// Score reported identities against a truth set.
///
/// `truth` is `None` when the target declared no method (FR-009); `succeeded`
/// is false when the tool did not produce usable output (SC-008). Either
/// yields a stated reason, never a zero.
pub fn score(
    reported: &BTreeSet<PackageIdentity>,
    truth: Option<&TruthSet>,
    ecosystem: &str,
    succeeded: bool,
    outcome_label: &str,
) -> Scored {
    if !succeeded {
        return Scored::No(NotScored::ToolDidNotSucceed {
            outcome: outcome_label.to_string(),
        });
    }
    let Some(truth) = truth else {
        return Scored::No(NotScored::NoTruthDeclared {
            ecosystem: ecosystem.to_string(),
        });
    };
    let found = reported.intersection(&truth.identities).count();
    Scored::Yes(Box::new(Accuracy {
        method: truth.method,
        truth_size: truth.len(),
        found,
        missed: truth.len() - found,
        extra: reported.len() - found,
        truth_is_superset: truth.is_superset(),
    }))
}

/// FR-008a. Scores derived by different methods measure against different
/// universes and are not comparable; refuse rather than silently rank.
pub fn methods_comparable(a: &Accuracy, b: &Accuracy) -> bool {
    a.method == b.method
}

/// FR-008b — the caveat that must accompany any superset-derived score.
pub fn superset_caveat(method: TruthMethod) -> Option<&'static str> {
    method.is_superset().then_some(
        "truth is a SUPERSET of the built set: `found` rewards over-reporting \
         and a tool correctly omitting unused packages scores lower. Do not \
         read this as a ranking.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::identity::parse_purl;

    fn ids(items: &[&str]) -> BTreeSet<PackageIdentity> {
        items
            .iter()
            .map(|s| parse_purl(s).unwrap_or_else(|| panic!("bad purl {s}")))
            .collect()
    }

    fn truth_of(items: &[&str], method: TruthMethod) -> TruthSet {
        TruthSet {
            method,
            identities: ids(items),
        }
    }

    #[test]
    fn perfect_match_scores_all_found() {
        let t = truth_of(&["pkg:golang/a/b@v1", "pkg:golang/a/c@v1"], TruthMethod::DeclaredExact);
        let got = score(&ids(&["pkg:golang/a/b@v1", "pkg:golang/a/c@v1"]), Some(&t), "golang", true, "ok");
        match got {
            Scored::Yes(a) => {
                assert_eq!((a.found, a.missed, a.extra), (2, 0, 0));
                assert!(!a.truth_is_superset);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn hand_computed_mixed_case() {
        let t = truth_of(
            &["pkg:golang/a/b@v1", "pkg:golang/a/c@v1", "pkg:golang/a/d@v1"],
            TruthMethod::DeclaredExact,
        );
        // reports b (hit), c (hit), and z (not in truth); misses d.
        let got = score(&ids(&["pkg:golang/a/b@v1", "pkg:golang/a/c@v1", "pkg:golang/a/z@v1"]), Some(&t), "golang", true, "ok");
        match got {
            Scored::Yes(a) => assert_eq!((a.found, a.missed, a.extra, a.truth_size), (2, 1, 1, 3)),
            other => panic!("{other:?}"),
        }
    }

    /// The behaviour the superset label warns about: a tool that correctly
    /// omits an unused module scores WORSE than one that reports everything.
    #[test]
    fn correct_omission_scores_worse_against_a_superset() {
        let t = truth_of(
            &["pkg:golang/a/used@v1", "pkg:golang/a/never-linked@v1"],
            TruthMethod::GoSumUnion,
        );
        let careful = score(&ids(&["pkg:golang/a/used@v1"]), Some(&t), "golang", true, "ok");
        let indiscriminate = score(
            &ids(&["pkg:golang/a/used@v1", "pkg:golang/a/never-linked@v1"]),
            Some(&t), "golang", true, "ok",
        );
        let (Scored::Yes(c), Scored::Yes(i)) = (careful, indiscriminate) else {
            panic!("both should score");
        };
        assert!(c.found < i.found, "the careful tool scores lower");
        assert!(c.truth_is_superset && i.truth_is_superset);
        assert!(superset_caveat(TruthMethod::GoSumUnion).is_some());
    }

    /// FR-009 — no truth declared. Must state the omission, not show zeros.
    #[test]
    fn no_truth_declared_is_stated_not_zeroed() {
        let got = score(&ids(&["pkg:cargo/serde@1.0"]), None, "cargo", true, "ok");
        match got {
            Scored::No(n @ NotScored::NoTruthDeclared { .. }) => {
                let r = n.reason();
                assert!(r.contains("not scored"), "{r}");
                assert!(r.contains("cargo"), "must name the ecosystem: {r}");
            }
            other => panic!("expected NoTruthDeclared, got {other:?}"),
        }
    }

    /// SC-008 — a failed tool is absent from scoring, not scored as zero.
    #[test]
    fn failed_tool_is_not_scored_as_finding_nothing() {
        let t = truth_of(&["pkg:golang/a/b@v1"], TruthMethod::DeclaredExact);
        let got = score(&BTreeSet::new(), Some(&t), "golang", false, "TimedOut");
        match got {
            Scored::No(n @ NotScored::ToolDidNotSucceed { .. }) => {
                assert!(n.reason().contains("TimedOut"), "{}", n.reason());
            }
            Scored::Yes(a) => panic!(
                "a crashed tool must not be scored; got found={} missed={}",
                a.found, a.missed
            ),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn cross_method_scores_are_not_comparable() {
        let a = Accuracy { method: TruthMethod::GoSumUnion, truth_size: 10, found: 8, missed: 2, extra: 0, truth_is_superset: true };
        let b = Accuracy { method: TruthMethod::GoModRequires, truth_size: 7, found: 7, missed: 0, extra: 1, truth_is_superset: true };
        assert!(!methods_comparable(&a, &b));
        assert!(methods_comparable(&a, &a.clone()));
    }

    #[test]
    fn exact_method_carries_no_caveat() {
        assert!(superset_caveat(TruthMethod::DeclaredExact).is_none());
    }
}
