// milestone 780 — private comparative benchmark harness.
// Spec:     specs/780-comparative-bench-harness/spec.md
// Plan:     specs/780-comparative-bench-harness/plan.md
// Contract: specs/780-comparative-bench-harness/contracts/xtask-compare-cli.md
//
// The harness scores itself before it scores anyone (FR-012, FR-013).
//
// Every wrong conclusion in the comparison that motivated this milestone was
// a harness defect, not a tool defect. An instrument that cannot demonstrate
// its own accuracy has no standing to rank anything, so this runs first and
// a failure aborts before any tool is invoked.

use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};

use super::config::TruthMethod;
use super::identity::PackageIdentity;
use super::truth;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelfCheckResult {
    Passed { expected: usize },
    Failed { detail: String },
}

impl SelfCheckResult {
    pub fn passed(&self) -> bool {
        matches!(self, Self::Passed { .. })
    }
}

pub fn fixture_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join("xtask/compare/fixtures/known-answer-go")
}

/// Derive the fixture's set by the method under test and compare it against
/// the hand-authored expectation.
pub fn run(workspace_root: &Path) -> Result<SelfCheckResult, Box<dyn Error>> {
    run_with(workspace_root, |root| {
        truth::derive(TruthMethod::GoSumUnion, root).map(|t| t.identities)
    })
}

/// Injectable form. Production passes the real derivation; the teeth test
/// passes a deliberately broken one.
pub fn run_with<F>(workspace_root: &Path, derive_fn: F) -> Result<SelfCheckResult, Box<dyn Error>>
where
    F: FnOnce(&Path) -> Result<BTreeSet<PackageIdentity>, Box<dyn Error>>,
{
    let root = fixture_path(workspace_root);
    if !root.is_dir() {
        return Ok(SelfCheckResult::Failed {
            detail: format!("fixture missing at {}", root.display()),
        });
    }

    let expected = truth::derive(TruthMethod::DeclaredExact, &root)?.identities;
    let actual = derive_fn(&root)?;

    if actual == expected {
        return Ok(SelfCheckResult::Passed {
            expected: expected.len(),
        });
    }

    // Report the symmetric difference, not just "mismatch". Whoever reads
    // this needs to know which direction the instrument is wrong in.
    let missing: Vec<String> = expected.difference(&actual).map(|i| i.to_string()).collect();
    let unexpected: Vec<String> = actual.difference(&expected).map(|i| i.to_string()).collect();
    Ok(SelfCheckResult::Failed {
        detail: format!(
            "expected {} identities, recovered {}.\n  \
             not recovered ({}): {}\n  \
             unexpectedly present ({}): {}",
            expected.len(),
            actual.len(),
            missing.len(),
            if missing.is_empty() { "-".into() } else { missing.join(", ") },
            unexpected.len(),
            if unexpected.is_empty() { "-".into() } else { unexpected.join(", ") },
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::identity::parse_purl;

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask has a parent")
            .to_path_buf()
    }

    #[test]
    fn self_check_passes_against_the_real_derivation() {
        let r = run(&workspace_root()).expect("run");
        assert_eq!(r, SelfCheckResult::Passed { expected: 5 }, "{r:?}");
    }

    /// T019 — prove the gate has teeth.
    ///
    /// A gate never observed failing is not known to be a gate. This session
    /// found five keyless tests `#[ignore]`d for a year and a notification
    /// path built but never fired; both were mechanisms nobody had made
    /// fail on purpose.
    #[test]
    fn deliberately_broken_reduction_is_caught() {
        // The specific regression the fixture is shaped to catch: strip the
        // version, so the two versions of waybill-fixture-multi merge.
        let broken = |root: &Path| -> Result<BTreeSet<PackageIdentity>, Box<dyn Error>> {
            let full = truth::derive(TruthMethod::GoSumUnion, root)?.identities;
            Ok(full
                .into_iter()
                .map(|mut i| {
                    i.version = String::new();
                    i
                })
                .collect())
        };
        let r = run_with(&workspace_root(), broken).expect("run");
        match r {
            SelfCheckResult::Failed { detail } => {
                assert!(detail.contains("expected 5"), "{detail}");
                assert!(
                    detail.contains("waybill-fixture-multi"),
                    "the discrepancy must be named, not just counted: {detail}"
                );
            }
            SelfCheckResult::Passed { .. } => {
                panic!("version-stripping must fail the self-check; if this \
                        passes, the fixture no longer discriminates the rule")
            }
        }
    }

    #[test]
    fn dropped_identity_is_reported_as_not_recovered() {
        let lossy = |root: &Path| -> Result<BTreeSet<PackageIdentity>, Box<dyn Error>> {
            let mut full = truth::derive(TruthMethod::GoSumUnion, root)?.identities;
            let victim = full.iter().next().cloned().expect("non-empty");
            full.remove(&victim);
            Ok(full)
        };
        let r = run_with(&workspace_root(), lossy).expect("run");
        match r {
            SelfCheckResult::Failed { detail } => {
                assert!(detail.contains("not recovered (1)"), "{detail}");
            }
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn extra_identity_is_reported_as_unexpectedly_present() {
        let inflating = |root: &Path| -> Result<BTreeSet<PackageIdentity>, Box<dyn Error>> {
            let mut full = truth::derive(TruthMethod::GoSumUnion, root)?.identities;
            full.insert(parse_purl("pkg:golang/example.test/ghost@v9.9.9").expect("purl"));
            Ok(full)
        };
        let r = run_with(&workspace_root(), inflating).expect("run");
        match r {
            SelfCheckResult::Failed { detail } => {
                assert!(detail.contains("unexpectedly present (1)"), "{detail}");
                assert!(detail.contains("ghost"), "{detail}");
            }
            other => panic!("expected failure, got {other:?}"),
        }
    }
}
