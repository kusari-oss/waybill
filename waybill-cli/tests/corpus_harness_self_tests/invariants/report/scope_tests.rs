#[allow(unused_imports)]
use std::collections::{BTreeMap, BTreeSet};
#[allow(unused_imports)]
use std::path::PathBuf;
#[allow(unused_imports)]
use crate::corpus_harness_195::manifest::{EcosystemId, TARGETS};
#[allow(unused_imports)]
use crate::corpus_harness_195::invariants::{applicability, Cell, CellOutcome, Verdict};
use crate::corpus_harness_195::invariants::report::*;
use crate::corpus_harness_195::invariants::declarations::DeclarationsFile;

fn na_cells(target: &str) -> Vec<Cell> {
    crate::corpus_harness_195::invariants::REGISTRY
        .iter()
        .map(|i| Cell {
            invariant: i.id,
            target: target.to_string(),
            outcome: CellOutcome::NotApplicable { reason: "precondition unmet".into() },
        })
        .collect()
}

/// A single-target report must not report FR-006a unfalsifiability.
/// Not-applicable on one target is the ordinary case; the floor is a
/// corpus-wide rule. Reporting it per-target made the lane print
/// "N lane failure(s)" for targets that had none, on every target.
#[test]
fn a_single_target_report_does_not_cry_wolf_about_applicability() {
    let decls = DeclarationsFile::default();
    let rep = build_for_target(na_cells("go-cobra"), &decls);
    assert_eq!(rep.scope, Scope::SingleTarget);
    assert!(rep.unfalsifiable.is_empty(), "{:?}", rep.unfalsifiable);
    assert!(
        rep.lane_failures().is_empty(),
        "a target on which every invariant is n/a fails nothing: {:?}",
        rep.lane_failures()
    );
}

/// And the corpus-wide report still enforces it, so the fix did not
/// simply delete the rule.
#[test]
fn a_corpus_report_still_enforces_the_applicability_floor() {
    let decls = DeclarationsFile::default();
    let rep = build(na_cells("go-cobra"), &decls);
    assert_eq!(rep.scope, Scope::Corpus);
    assert_eq!(rep.unfalsifiable.len(), crate::corpus_harness_195::invariants::REGISTRY.len());
    assert!(!rep.lane_failures().is_empty());
}
