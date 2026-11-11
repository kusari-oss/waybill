//! Milestone 1274 (#1283 part 1) — what to keep out of nix's stderr, and what
//! it means.
//!
//! One rule, reachable by both failure paths. Before this module the rule
//! lived privately in [`super::invoke`] and the closure path used an
//! unrelated `stderr.trim().chars().take(200)` a hundred lines away — so the
//! same failure read differently depending on which tier hit it. That is the
//! defect this module exists to make impossible: FR-008 requires one rule,
//! and a copy is how a single rule becomes two that disagree.

use super::preflight::IfdPolicy;
use super::reason::DegradationReason;

/// How much of the selected text to keep.
///
/// The detail goes into a document annotation, so it is bounded on purpose. A
/// kilobyte of Nix stack trace per degraded scan is not a thing to put in an
/// SBOM.
const DETAIL_CHARS: usize = 300;

/// The clause nix prints when it refuses import-from-derivation.
///
/// **FR-005a — two conditions, each insufficient alone.** Matched against an
/// *error* line, and matched on the consequence clause rather than on the
/// setting's name.
///
/// Measured (research R2): a flake whose `nixConfig` requests the setting and
/// then fails for an unrelated cause emits
///
/// ```text
/// warning: ignoring untrusted flake configuration setting 'allow-import-from-derivation'.
/// error: attribute 'waybillFixtureNoSuchAttribute' missing
/// ```
///
/// The setting's bare name is therefore present in the stderr of a failure
/// that has nothing to do with the refusal, in a line that is not an error.
/// Matching the name anywhere would report a missing attribute as a policy
/// refusal — converting one uninformative answer into one *misleading*
/// answer, which spec FR-005 calls worse than the status quo. Real flakes set
/// this option (`slack-web` does, per [`super::invoke`]'s own comment), so
/// the false positive sits on the path of ordinary use.
const IFD_REFUSED_CLAUSE: &str = "because the option 'allow-import-from-derivation' is disabled";

/// Does this line read as one of nix's errors?
///
/// Kept as a named predicate because three things depend on agreeing about
/// it: where the detail starts, which lines the classifier may consider, and
/// the FR-009 fallback.
fn is_error_line(line: &str) -> bool {
    line.trim_start().starts_with("error") || line.contains(": error:")
}

/// The part of nix's stderr an operator can act on.
///
/// A blind prefix is the wrong thing to keep. Nix prints warnings before
/// errors, so `stderr.take(200)` on a flake that sets its own `nixConfig`
/// returns "ignoring untrusted flake configuration setting ..." and truncates
/// before the line that says what actually went wrong. Measured on a Haskell
/// flake: the operator was shown a warning about untrusted config while the
/// real cause — `cannot build 'cabal2nix-....drv' during evaluation because
/// the option 'allow-import-from-derivation' is disabled` — never appeared.
/// That reads as "waybill is broken on my repo" rather than "waybill declined
/// by policy".
///
/// So: start at an error line, and fall back to the prefix when there is
/// none.
///
/// # Why the *last* error line and not the first
///
/// Research R1 measured that **nix prints the stack trace first and the cause
/// last.** On a real `callCabal2nix` refusal the stderr is 26 lines; the
/// error predicate matches at line 1 — a bare `error:` opener — and again at
/// line 25, which carries the clause. Under `position()` the window covers
/// bytes 126..426 and the clause, at byte 1281 of 1342, falls 855 characters
/// outside it. Under `rposition()` the window covers 1152..1342 and the
/// clause is inside.
///
/// `rposition` is strictly better rather than a trade: where exactly one line
/// matches, the two are identical; where a trace precedes the cause, only
/// `rposition` reaches the cause. Confirmed against the R2 false positive
/// too, whose single error line is chosen either way.
///
/// `unwrap_or(0)` is retained deliberately — it is **FR-009**. Where no line
/// can be identified as the explanation, the stderr's own opening is recorded
/// rather than nothing. An unparseable failure is not an excuse for silence,
/// and a detail an operator has to interpret beats an absent one.
/// # Not literally one word
///
/// `rposition` requires `ExactSizeIterator`, which [`str::Lines`] does not
/// implement, so the lines are collected before the search rather than
/// iterated twice. The planning document called this a one-word change; it is
/// one *decision* and four lines. Recorded because the next person reading
/// `position` -> `rposition` in the plan will reach for the same edit.
pub(crate) fn salient_stderr(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().collect();
    let start = lines.iter().rposition(|l| is_error_line(l)).unwrap_or(0);
    lines[start..].join("\n").chars().take(DETAIL_CHARS).collect()
}

/// Why `nix` exited non-zero, as specifically as its stderr supports.
///
/// Returns [`DegradationReason::ImportFromDerivationRequired`] when the
/// refusal is what stopped the evaluation, and
/// [`DegradationReason::EvaluationFailed`] otherwise. **FR-006**: no clause,
/// no classification — the generic reason standing is correct behaviour, not
/// a failure, and a reworded nix message lands there by construction rather
/// than by guesswork.
///
/// # FR-005c — the refusal wins
///
/// Where both the refusal and some other error appear, the refusal is the
/// reason. That is *not* a by-product of the selection: `rposition` picks the
/// last error line, which may well be the other error. So the clause is
/// searched for across every error line, independently of where the detail
/// starts. The detail still comes from [`salient_stderr`], so the other
/// error's line is retained — the operator sees both the classification and
/// the text.
///
/// # Milestone 1283 (FR-010) — an allowed build that failed
///
/// Under `--nix-allow-ifd` nix builds during evaluation, and that build can
/// fail. The calls classified here (`nix eval`, `nix derivation show`) build
/// nothing except through import-from-derivation, so a failed `.drv` build in
/// their stderr is the IFD build. Measured (`build-failed.sh`, nix 2.34.6):
///
/// ```text
/// error: Cannot build '/nix/store/…-waybill-fixture-ifd-broken.drv'.
///        Reason: builder failed with exit code 1.
/// ```
///
/// Keyed on the **policy** as well as the text: without the opt-in no IFD
/// build is attempted, so the same text there is not this reason. The refusal
/// clause is checked first, and its line also reads `cannot build '….drv…'`,
/// so the order is load-bearing.
pub(crate) fn classify_failure(stderr: &str, policy: IfdPolicy) -> DegradationReason {
    let detail = salient_stderr(stderr);
    if stderr
        .lines()
        .any(|l| is_error_line(l) && l.contains(IFD_REFUSED_CLAUSE))
    {
        return DegradationReason::ImportFromDerivationRequired(detail);
    }
    if policy == IfdPolicy::Allow && stderr.lines().any(is_failed_build_line) {
        return DegradationReason::ImportFromDerivationBuildFailed(detail);
    }
    DegradationReason::EvaluationFailed(detail)
}

/// An error line saying a derivation failed to build: nix 2.34's
/// `Cannot build '….drv'.`, or the `builder for '….drv' failed` wording of
/// earlier releases (the latter is inference, not a capture).
fn is_failed_build_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    is_error_line(line)
        && lower.contains(".drv")
        && (lower.contains("cannot build '") || lower.contains("builder for '"))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// The measured 26-line refusal, abbreviated in the middle but keeping
    /// the shape R1 found: a bare `error:` opener, a stack trace, the cause
    /// last.
    fn measured_refusal() -> String {
        let mut s = String::from(
            "warning: Git tree '/private/var/folders/dz/tmp.lHHzzfbSms/cabal2nix' has uncommitted changes\nerror:\n",
        );
        for i in 0..22 {
            s.push_str(&format!(
                "       … while evaluating frame {i} of the call stack, at /nix/store/aaa-nixpkgs/pkgs/x.nix:{i}:1\n"
            ));
        }
        s.push_str(
            "error: cannot build '/nix/store/aaa-cabal2nix-waybill-fixture-ifd.drv^out' during evaluation because the option 'allow-import-from-derivation' is disabled\n",
        );
        s
    }

    /// R2's measured false positive: the setting's name appears, in a
    /// warning, and the failure is unrelated.
    const FALSE_POSITIVE: &str = "\
warning: Git tree '/tmp/nixconfig_unrelated' has uncommitted changes
warning: ignoring untrusted flake configuration setting 'allow-import-from-derivation'.
Pass '--accept-flake-config' to trust it
error: attribute 'waybillFixtureNoSuchAttribute' missing
       at /nix/store/aaa-source/flake.nix:6:40
";

    // ── T006 / FR-007, FR-009 — the selection rule ──────────────────────

    #[test]
    fn the_measured_refusal_selects_the_clause_not_the_trace() {
        let d = salient_stderr(&measured_refusal());
        assert!(
            d.contains(IFD_REFUSED_CLAUSE),
            "the cause is what an operator needs; got {d:?}"
        );
        assert!(
            !d.contains("while evaluating frame"),
            "the stack trace must not crowd out the cause: {d:?}"
        );
        assert!(
            !d.contains("uncommitted changes"),
            "warnings must not crowd it out: {d:?}"
        );
    }

    /// The pre-change rule, inlined, so the test states what it is better
    /// than rather than asserting an absolute. Without this the suite could
    /// pass under `position()` for reasons nobody checked.
    #[test]
    fn the_first_error_line_would_have_missed_it() {
        let stderr = measured_refusal();
        let lines: Vec<&str> = stderr.lines().collect();
        let first = lines.iter().position(|l| is_error_line(l)).unwrap_or(0);
        let under_position: String =
            lines[first..].join("\n").chars().take(DETAIL_CHARS).collect();
        assert!(
            !under_position.contains(IFD_REFUSED_CLAUSE),
            "if the first error line reached the clause, this milestone's \
             premise is wrong and R1 needs re-measuring; got {under_position:?}"
        );
    }

    #[test]
    fn one_error_line_selects_the_same_either_way() {
        let stderr = "warning: something\nerror: attribute 'x' missing\n       at /f.nix:1:1\n";
        let lines: Vec<&str> = stderr.lines().collect();
        let first = lines.iter().position(|l| is_error_line(l));
        let last = lines.iter().rposition(|l| is_error_line(l));
        assert_eq!(first, last, "the premise of this test");
        let d = salient_stderr(stderr);
        assert!(d.starts_with("error: attribute 'x' missing"), "{d:?}");
    }

    /// FR-009. CONTROL: with no error line there is nothing to seek to, and
    /// the output must still be the message rather than empty.
    #[test]
    fn stderr_without_an_error_line_is_still_reported() {
        let d = salient_stderr("something unusual happened\nand then stopped\n");
        assert!(d.starts_with("something unusual"), "{d:?}");
        assert!(!d.is_empty());
    }

    #[test]
    fn the_false_positive_selects_the_attribute_error_not_the_warning() {
        let d = salient_stderr(FALSE_POSITIVE);
        assert!(d.starts_with("error: attribute"), "{d:?}");
        assert!(
            !d.contains("ignoring untrusted"),
            "the warning is not the explanation: {d:?}"
        );
    }

    #[test]
    fn the_detail_is_bounded() {
        let long = format!("error: {}\n", "x".repeat(5_000));
        assert_eq!(salient_stderr(&long).chars().count(), DETAIL_CHARS);
    }

    // ── T006 / FR-005, FR-005a, FR-005b, FR-005c, FR-006 — the classifier ─

    #[test]
    fn the_measured_refusal_classifies_as_a_refusal() {
        let r = classify_failure(&measured_refusal(), IfdPolicy::Refuse);
        assert_eq!(r.wire(), "import-from-derivation-required");
    }

    /// **The negative test FR-015 names.** The setting's name is in stderr
    /// and the failure is a missing attribute.
    #[test]
    fn the_false_positive_keeps_the_generic_reason() {
        assert!(
            FALSE_POSITIVE.contains("allow-import-from-derivation"),
            "the premise: the setting's bare name IS present"
        );
        let r = classify_failure(FALSE_POSITIVE, IfdPolicy::Refuse);
        assert_eq!(
            r.wire(),
            "evaluation-failed",
            "a classifier keyed on the bare name reports this as a refusal"
        );
    }

    /// FR-005b / FR-006 — a reworded clause degrades to the generic reason,
    /// with no panic and no failed scan.
    #[test]
    fn a_reworded_clause_degrades_to_generic() {
        let reworded = measured_refusal().replace(
            "because the option 'allow-import-from-derivation' is disabled",
            "because import-from-derivation is not permitted",
        );
        let r = classify_failure(&reworded, IfdPolicy::Refuse);
        assert_eq!(r.wire(), "evaluation-failed");
        // And the detail still reaches the line, so the operator is not left
        // with nothing when nix changes its wording.
        assert!(
            r.to_string().contains("import-from-derivation is not permitted"),
            "{r}"
        );
    }

    /// FR-005c — the refusal wins, and the other error's line is kept.
    #[test]
    fn a_refusal_alongside_another_error_is_still_a_refusal() {
        let both = format!(
            "{}error: attribute 'somethingElse' missing\n       at /f.nix:9:1\n",
            measured_refusal()
        );
        // The premise: rposition now lands on the OTHER error, so precedence
        // cannot be a by-product of the selection.
        let d = salient_stderr(&both);
        assert!(d.starts_with("error: attribute 'somethingElse' missing"), "{d:?}");

        let r = classify_failure(&both, IfdPolicy::Refuse);
        assert_eq!(r.wire(), "import-from-derivation-required", "the refusal wins");
        assert!(
            r.to_string().contains("somethingElse"),
            "the other error's line is retained in the detail: {r}"
        );
    }

    #[test]
    fn a_clause_in_a_warning_is_not_a_refusal() {
        // Belt and braces for FR-005a's error-line condition specifically:
        // the clause itself, on a non-error line.
        let s = format!("warning: nix would have said {IFD_REFUSED_CLAUSE}\nerror: unrelated\n");
        assert_eq!(classify_failure(&s, IfdPolicy::Refuse).wire(), "evaluation-failed");
    }

    // ── T017 (#1283, FR-010) — an allowed IFD whose build failed ────────

    /// nix's real stderr: IFD allowed, and the derivation built during
    /// evaluation exited 1. Captured by
    /// `specs/1283-nix-allow-ifd/measurements/ifd-eval/build-failed.sh`.
    const BUILD_FAILED: &str = include_str!("../../../../../tests/fixtures/nix_ifd/ifd-build-failed.stderr");

    #[test]
    fn t017_an_allowed_ifd_whose_build_failed_is_its_own_reason() {
        let r = classify_failure(BUILD_FAILED, IfdPolicy::Allow);
        assert_eq!(r.wire(), "import-from-derivation-build-failed");
        assert!(
            r.to_string().contains("Cannot build '/nix/store/")
                && r.to_string().contains("waybill-fixture-ifd-broken.drv"),
            "the detail names the derivation that failed: {r}"
        );
    }

    /// Without the opt-in no IFD build is attempted, so a build failure there
    /// is not one: the classification is keyed on the policy, not the text.
    #[test]
    fn t017_the_same_stderr_under_a_refusal_is_not_an_ifd_build_failure() {
        assert_eq!(
            classify_failure(BUILD_FAILED, IfdPolicy::Refuse).wire(),
            "evaluation-failed"
        );
    }

    /// The refusal clause still wins under a refusal, and an allowed
    /// evaluation never reports a refusal it did not make.
    #[test]
    fn t017_a_refusal_is_not_reported_as_a_build_failure() {
        assert_eq!(
            classify_failure(&measured_refusal(), IfdPolicy::Refuse).wire(),
            "import-from-derivation-required"
        );
        assert_ne!(
            classify_failure(&measured_refusal(), IfdPolicy::Allow).wire(),
            "import-from-derivation-build-failed"
        );
    }

    /// Unrelated failures keep the generic reason under the opt-in too.
    #[test]
    fn t017_an_unrelated_failure_under_the_opt_in_stays_generic() {
        assert_eq!(classify_failure(FALSE_POSITIVE, IfdPolicy::Allow).wire(), "evaluation-failed");
    }

    /// nix's older wording, `builder for '….drv' failed with exit code 1`,
    /// is the same event. **Inference, not a capture**: this is the shape
    /// earlier nix releases print, written from memory; only the
    /// `Cannot build` form above was measured (nix 2.34.6).
    #[test]
    fn t017_the_older_builder_failed_wording_is_recognised() {
        let s = "error: builder for '/nix/store/aaa-cabal2nix-waybill-fixture.drv' failed with exit code 1\n";
        assert_eq!(
            classify_failure(s, IfdPolicy::Allow).wire(),
            "import-from-derivation-build-failed"
        );
    }
}
