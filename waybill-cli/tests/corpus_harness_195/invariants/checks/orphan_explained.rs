//! I7 — a component not reachable from the subject by any dependency
//! edge carries a marker explaining why.
//!
//! A component nothing explains leaves a consumer unable to tell whether
//! it is a real part of the artifact or a reader artefact.
//!
//! **Boundary with I3**: this invariant's population is the unreachable
//! one. I3's is reachable-but-only-through-non-runtime-edges. The two
//! preconditions are disjoint by construction (FR-019).

use super::super::{CellOutcome, Violation};
use super::{cdx_component_ref, cdx_components, cdx_prop, cdx_reachable, cdx_subject_ref};

pub const KIND_UNREACHABLE_AND_UNEXPLAINED: &str = "unreachable-and-unexplained";
pub const KINDS: &[&str] = &[KIND_UNREACHABLE_AND_UNEXPLAINED];

/// Annotations that explain why a component has no edge. `orphan-reason`
/// is milestone 167's vocabulary; `component-tier = file` explains
/// itself, since a file is contained rather than depended upon and is
/// I1's business.
fn is_explained(component: &serde_json::Value) -> bool {
    cdx_prop(component, "waybill:orphan-reason").is_some()
        || cdx_prop(component, "waybill:component-tier") == Some("file")
        || cdx_prop(component, "waybill:unresolved-reason").is_some()
        || cdx_prop(component, "waybill:closure-role").is_some()
}

pub fn check(sboms: &super::EmittedSboms) -> CellOutcome {
    let cdx = &sboms.cdx;
    let components = cdx_components(cdx);

    let Some(root) = cdx_subject_ref(cdx) else {
        return CellOutcome::NotApplicable { reason: "no resolvable subject".to_string() };
    };
    let non_subject = components
        .iter()
        .filter(|c| cdx_component_ref(c) != Some(root))
        .count();
    if non_subject == 0 {
        return CellOutcome::NotApplicable {
            reason: "document has no component other than the subject".to_string(),
        };
    }

    let reachable = cdx_reachable(cdx);
    let mut offenders: Vec<String> = components
        .iter()
        .filter(|c| {
            let Some(r) = cdx_component_ref(c) else { return false };
            r != root && !reachable.contains(r) && !is_explained(c)
        })
        .filter_map(cdx_component_ref)
        .map(|r| r.to_string())
        .collect();
    if offenders.is_empty() {
        return CellOutcome::Pass;
    }
    offenders.sort();
    let count = offenders.len();
    CellOutcome::Fail(vec![Violation {
        kind: KIND_UNREACHABLE_AND_UNEXPLAINED,
        count,
        example: offenders.into_iter().next(),
    }])
}
