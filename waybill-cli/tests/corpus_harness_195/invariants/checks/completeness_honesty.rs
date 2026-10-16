//! I5 — a `complete` graph-completeness verdict implies nothing was
//! excluded from the judgement.
//!
//! `complete` is the strongest claim waybill makes about a graph. If the
//! same document also carries components unreachable from the subject,
//! the verdict is overclaiming and a consumer who trusts it will not go
//! looking for the gap.

use super::super::{CellOutcome, Violation};
use super::{cdx_component_ref, cdx_components, cdx_doc_prop, cdx_reachable, cdx_subject_ref};

pub const KIND_COMPLETE_WITH_UNREACHABLE: &str = "complete-with-unreachable";
pub const KINDS: &[&str] = &[KIND_COMPLETE_WITH_UNREACHABLE];

pub fn check(sboms: &super::EmittedSboms) -> CellOutcome {
    let cdx = &sboms.cdx;

    let verdict = cdx_doc_prop(cdx, "waybill:graph-completeness");
    match verdict {
        Some("complete") => {}
        Some(other) => {
            return CellOutcome::NotApplicable {
                reason: format!("graph-completeness verdict is `{other}`, not `complete`"),
            };
        }
        None => {
            return CellOutcome::NotApplicable {
                reason: "no graph-completeness verdict on this document".to_string(),
            };
        }
    }
    if cdx_subject_ref(cdx).is_none() {
        return CellOutcome::NotApplicable { reason: "no resolvable subject".to_string() };
    }

    let reachable = cdx_reachable(cdx);
    let mut unreachable: Vec<String> = cdx_components(cdx)
        .iter()
        .filter_map(cdx_component_ref)
        .filter(|r| !reachable.contains(r))
        .map(|r| r.to_string())
        .collect();
    if unreachable.is_empty() {
        return CellOutcome::Pass;
    }
    unreachable.sort();
    let count = unreachable.len();
    CellOutcome::Fail(vec![Violation {
        kind: KIND_COMPLETE_WITH_UNREACHABLE,
        count,
        example: unreachable.into_iter().next(),
    }])
}
