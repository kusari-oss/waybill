//! Milestone 1034 (#971 part A) — the opt-in `nix eval` resolution tier.
//!
//! waybill learns a Nix-built project's package versions by fetching and
//! parsing nixpkgs files. That is a reconstruction of what Nix would compute,
//! and #1033 proved it can be wrong: two components shipped with wrong
//! versions because `configuration-common.nix`, which supersedes the generated
//! set, was not being read. The next such bug will be a different file, or a
//! construct no file-parser reproduces.
//!
//! This tier asks Nix instead. It is opt-in, and deliberately so: it requires
//! `nix` on PATH, makes the answer host-dependent, and evaluates nixpkgs —
//! code waybill neither authors nor audits. File-parsing stays the default and
//! the fallback.
//!
//! What is evaluated is narrower than an early draft of this feature assumed:
//! nixpkgs at the revision `flake.lock` pins, never the project's own flake.
//! Repository-authored expressions do not run. The one repository-controlled
//! value that reaches a Nix expression is the revision string, which
//! `invoke::is_valid_revision` checks. The import-from-derivation refusal is
//! kept regardless — it guards the nixpkgs evaluation today, and becomes
//! load-bearing if project flakes are ever evaluated (research R8).
//!
//! Every failure degrades — the scan emits file-parsing results unchanged and
//! exits successfully, carrying a reason code (spec FR-011).

pub(crate) mod invoke;
pub(crate) mod preflight;
pub(crate) mod reason;
pub(crate) mod stderr;
pub(crate) mod result;
pub(crate) mod licenses;

use std::collections::BTreeMap;
use std::time::Duration;

use reason::DegradationReason;
use result::{EvaluationOutcome, NixSystem};

/// Default evaluation budget.
///
/// **Provisional.** Not a measurement, and labelled so deliberately (spec
/// FR-012).
///
/// The budget covers acquiring the pinned revision as well as evaluating it —
/// `getFlake` fetches during evaluation, inside this one subprocess — so it
/// cannot be derived from evaluation timings alone. Measured points it must
/// comfortably clear: 0.49–0.64s for a single attribute on a warm store, 0.5s
/// for 423 names, 7.7s for 423 names with a cold eval cache. What is *not*
/// measured is a cold Nix **store**, which adds a 300–335 MB fetch at whatever
/// bandwidth the host has — that is task T-R5, and it needs a clean runner.
///
/// Generous on purpose: degrading a first scan because the operator's network
/// is slow would be a worse failure than running long on a stuck evaluation,
/// which the operator can cut short.
pub(crate) const PROVISIONAL_BUDGET_SECS: u64 = 120;

/// What the operator asked for.
#[derive(Debug, Clone)]
pub(crate) struct TierConfig {
    /// Platform to evaluate for; `None` means detect the host's.
    pub(crate) system: Option<NixSystem>,
    /// Wall-clock budget for the resolving evaluation.
    pub(crate) budget: Duration,
    /// Milestone 1283 — the import-from-derivation policy requested
    /// (`--nix-allow-ifd`). Verified by gate 1 before anything is evaluated.
    pub(crate) ifd_policy: preflight::IfdPolicy,
}

impl TierConfig {
    pub(crate) fn from_flags(
        system: Option<NixSystem>,
        timeout_secs: Option<u64>,
        ifd_policy: preflight::IfdPolicy,
    ) -> Self {
        Self {
            system,
            budget: Duration::from_secs(timeout_secs.unwrap_or(PROVISIONAL_BUDGET_SECS)),
            ifd_policy,
        }
    }
}

/// What one run of the tier did: the C217 record of the policy it evaluated
/// under, whatever the outcome, and the outcome.
pub(crate) type TierRun = (
    crate::scan_fs::package_db::nix::closure::summary::NixEvaluationPolicy,
    Result<EvaluationOutcome, DegradationReason>,
);

/// Run the tier for one pinned revision and a set of component names.
///
/// The gates are ordered so the safety one comes first: nothing is evaluated
/// until the requested import-from-derivation policy is confirmed in effect.
/// Every error return is a degradation the caller emits and continues from.
///
/// Milestone 1283: the C217 record is returned beside the outcome rather than
/// inside it, because it is emitted on degradation too.
pub(crate) fn run(
    revision: &str,
    names: &[String],
    cfg: &TierConfig,
    evaluator: &dyn Evaluator,
) -> TierRun {
    use crate::scan_fs::package_db::nix::closure::summary::NixEvaluationPolicy;
    // Gate 1 -- safety, before anything repository-controlled is touched.
    match evaluator.verify_ifd_policy(cfg.ifd_policy, cfg.budget) {
        Ok((ifd, env)) => (
            NixEvaluationPolicy::verified(&ifd, &env),
            run_verified(revision, names, cfg, evaluator, ifd),
        ),
        Err(reason) => (NixEvaluationPolicy::unverified(cfg.ifd_policy), Err(reason)),
    }
}

/// Gates 2 and 3, reachable only with the gate-1 token in hand.
fn run_verified(
    revision: &str,
    names: &[String],
    cfg: &TierConfig,
    evaluator: &dyn Evaluator,
    ifd: result::IfdPolicyVerified,
) -> Result<EvaluationOutcome, DegradationReason> {
    // Gate 2 -- the platform, which must be explicit for the resolving call
    // to stay pure.
    let system = match &cfg.system {
        Some(s) => s.clone(),
        None => evaluator.detect_host_system(cfg.budget)?,
    };

    if names.is_empty() {
        return Ok(EvaluationOutcome::new(
            revision.to_string(),
            system,
            BTreeMap::new(),
            ifd,
        ));
    }

    // Gate 3 -- the evaluation itself, pure and bounded.
    let versions = evaluator.evaluate_versions(revision, &system, names, &ifd, cfg.budget)?;

    Ok(EvaluationOutcome::new(
        revision.to_string(),
        system,
        versions,
        ifd,
    ))
}

/// The `nix` calls the tier makes, behind a trait so the orchestration above
/// is testable without a `nix` on the bench -- and so a test can assert the
/// *order* of the gates, which is the property that matters most.
pub(crate) trait Evaluator {
    fn verify_ifd_policy(
        &self,
        policy: preflight::IfdPolicy,
        budget: Duration,
    ) -> Result<(result::IfdPolicyVerified, preflight::NixEnvironment), DegradationReason>;

    fn detect_host_system(&self, budget: Duration) -> Result<NixSystem, DegradationReason>;

    fn evaluate_versions(
        &self,
        revision: &str,
        system: &NixSystem,
        names: &[String],
        ifd: &result::IfdPolicyVerified,
        budget: Duration,
    ) -> Result<BTreeMap<String, Option<String>>, DegradationReason>;
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// Records the order the gates were reached, and can fail any of them.
    struct SpyEvaluator {
        calls: RefCell<Vec<&'static str>>,
        fail_preflight: bool,
        /// m1283 — the verified policy each evaluation received.
        evaluated_under: RefCell<Vec<preflight::IfdPolicy>>,
    }

    impl SpyEvaluator {
        fn new(fail_preflight: bool) -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                fail_preflight,
                evaluated_under: RefCell::new(Vec::new()),
            }
        }
        fn calls(&self) -> Vec<&'static str> {
            self.calls.borrow().clone()
        }
    }

    impl Evaluator for SpyEvaluator {
        fn verify_ifd_policy(
            &self,
            policy: preflight::IfdPolicy,
            _b: Duration,
        ) -> Result<(result::IfdPolicyVerified, preflight::NixEnvironment), DegradationReason>
        {
            self.calls.borrow_mut().push("preflight");
            if self.fail_preflight {
                Err(DegradationReason::IfdRefusalUnverified("spy".into()))
            } else {
                Ok((
                    result::IfdPolicyVerified::assumed_for_test(policy),
                    preflight::NixEnvironment { sandbox: Some("true".into()) },
                ))
            }
        }

        fn detect_host_system(&self, _b: Duration) -> Result<NixSystem, DegradationReason> {
            self.calls.borrow_mut().push("detect_system");
            Ok("x86_64-linux".parse().unwrap())
        }

        fn evaluate_versions(
            &self,
            _r: &str,
            _s: &NixSystem,
            names: &[String],
            ifd: &result::IfdPolicyVerified,
            _b: Duration,
        ) -> Result<BTreeMap<String, Option<String>>, DegradationReason> {
            self.calls.borrow_mut().push("evaluate");
            self.evaluated_under.borrow_mut().push(ifd.policy());
            Ok(names
                .iter()
                .map(|n| (n.clone(), Some("1.0.0".to_string())))
                .collect())
        }
    }

    fn cfg() -> TierConfig {
        TierConfig::from_flags(None, Some(30), preflight::IfdPolicy::Refuse)
    }

    #[test]
    fn the_safety_gate_runs_before_anything_is_evaluated() {
        // The ordering IS the safety property. Evaluating first and checking
        // afterwards would be indistinguishable in every output the tier
        // produces, and wrong.
        let spy = SpyEvaluator::new(false);
        let (record, out) = run("a".repeat(40).as_str(), &["aeson".to_string()], &cfg(), &spy);
        let out = out.unwrap();
        assert_eq!(spy.calls(), vec!["preflight", "detect_system", "evaluate"]);
        assert_eq!(out.versions.len(), 1);
        assert!(record.verified);
    }

    #[test]
    fn a_failed_preflight_stops_before_evaluating() {
        let spy = SpyEvaluator::new(true);
        let (record, out) = run("a".repeat(40).as_str(), &["aeson".to_string()], &cfg(), &spy);
        let err = out.unwrap_err();
        assert_eq!(err.wire(), "ifd-refusal-unverified");
        assert_eq!(
            spy.calls(),
            vec!["preflight"],
            "nothing may run after the safety gate fails"
        );
        // m1283 — C217 is recorded on degradation too, unverified.
        assert!(!record.verified);
        assert_eq!(record.sandbox, None);
    }

    /// m1283 (T008) — the policy from the config is the one verified and the
    /// one every evaluation receives.
    #[test]
    fn the_requested_policy_is_verified_and_passed_on() {
        let spy = SpyEvaluator::new(false);
        let cfg = TierConfig::from_flags(None, Some(30), preflight::IfdPolicy::Allow);
        let (record, out) = run("d".repeat(40).as_str(), &["aeson".to_string()], &cfg, &spy);
        out.unwrap();
        assert_eq!(*spy.evaluated_under.borrow(), vec![preflight::IfdPolicy::Allow]);
        assert_eq!(record.import_from_derivation, preflight::IfdPolicy::Allow);
        assert!(record.verified);
        assert_eq!(record.sandbox.as_deref(), Some("true"));
    }

    #[test]
    fn a_named_platform_skips_host_detection() {
        // Detection is the one impure call; naming the platform must avoid it
        // entirely rather than detect-then-override.
        let spy = SpyEvaluator::new(false);
        let cfg = TierConfig::from_flags(
            Some("aarch64-darwin".parse().unwrap()),
            Some(30),
            preflight::IfdPolicy::Refuse,
        );
        let out = run("b".repeat(40).as_str(), &["text".to_string()], &cfg, &spy).1.unwrap();
        assert_eq!(spy.calls(), vec!["preflight", "evaluate"]);
        assert_eq!(out.system.as_str(), "aarch64-darwin");
    }

    #[test]
    fn an_empty_name_set_still_passes_the_safety_gate_but_does_not_evaluate() {
        let spy = SpyEvaluator::new(false);
        let out = run("c".repeat(40).as_str(), &[], &cfg(), &spy).1.unwrap();
        assert_eq!(spy.calls(), vec!["preflight", "detect_system"]);
        assert!(out.versions.is_empty());
    }
}
