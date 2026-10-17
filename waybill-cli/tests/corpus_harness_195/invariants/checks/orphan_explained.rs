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
///
/// `waybill:version-absence` = `by-construction` (C197, milestone 1156)
/// is accepted **forward-lookingly** and moves no cell today: the seven
/// Pants resolve anchors it is emitted on are all reachable from their
/// subject, so none of them is in I7's population. The arm exists so an
/// edge-wiring change that later strands an anchor cannot turn these
/// seven components into a second defect class — a resolve anchor is a
/// grouping rather than a dependency, and that is accounted for whether
/// or not anything happens to depend on it. The tests below are the only
/// thing exercising this arm, by construction and deliberately.
fn is_explained(component: &serde_json::Value) -> bool {
    cdx_prop(component, "waybill:orphan-reason").is_some()
        || cdx_prop(component, "waybill:version-absence") == Some("by-construction")
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

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use super::super::super::goldens;

    fn versionless_edgeless(reference: &str, props: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "bom-ref": format!("pkg:generic/{reference}"),
            "name": reference,
            "version": "",
            "type": "library",
            "purl": format!("pkg:generic/{reference}"),
            "properties": props
        })
    }

    /// FR-007 — a versionless, edgeless component carrying C197 is
    /// explained. No corpus target exhibits this shape (the seven anchors
    /// are all reachable), so this is the arm's only coverage; the two
    /// negative controls are what keep it from being a test that
    /// `is_explained` returns true for everything.
    #[test]
    fn an_edgeless_component_with_the_version_absence_marker_is_explained() {
        let explained = versionless_edgeless(
            "mutation-anchor",
            serde_json::json!([
                { "name": "waybill:version-absence", "value": "by-construction" }
            ]),
        );
        assert!(is_explained(&explained));

        // The key alone is not an explanation.
        let unrecognised = versionless_edgeless(
            "mutation-anchor-bad-value",
            serde_json::json!([
                { "name": "waybill:version-absence", "value": "somebody-guessed" }
            ]),
        );
        assert!(!is_explained(&unrecognised));

        // And the co-present `component-kind` is not one either (FR-009).
        let kind_only = versionless_edgeless(
            "mutation-anchor-kind-only",
            serde_json::json!([
                { "name": "waybill:component-kind", "value": "lockfile-resolve" }
            ]),
        );
        assert!(!is_explained(&kind_only));
    }

    /// SC-007 — the arm adds acceptance without moving a cell. Measured
    /// rather than asserted: for every committed golden, I7's outcome
    /// must equal its outcome with the arm's contribution removed.
    ///
    /// This is the shape that catches the failure the arm could cause and
    /// that an "I7 is unchanged" comment could not — a component that was
    /// reported before and is not now.
    #[test]
    fn the_version_absence_arm_moves_no_cell_on_any_corpus_target() {
        for (target, sboms) in goldens::load_all() {
            assert_eq!(
                check(&sboms),
                check_without_version_absence(&sboms),
                "{target}: accepting waybill:version-absence changed I7's outcome, so the \
                 arm is not the forward-looking no-op it claims to be"
            );
        }
    }

    /// `check`, with the C197 arm removed. Duplicated deliberately: the
    /// point is to compare against the pre-feature behaviour, and
    /// parameterising `check` would make the comparison vacuous by
    /// construction.
    fn check_without_version_absence(sboms: &super::super::EmittedSboms) -> CellOutcome {
        fn explained_pre_1156(component: &serde_json::Value) -> bool {
            cdx_prop(component, "waybill:orphan-reason").is_some()
                || cdx_prop(component, "waybill:component-tier") == Some("file")
                || cdx_prop(component, "waybill:unresolved-reason").is_some()
                || cdx_prop(component, "waybill:closure-role").is_some()
        }
        let cdx = &sboms.cdx;
        let components = cdx_components(cdx);
        let Some(root) = cdx_subject_ref(cdx) else {
            return CellOutcome::NotApplicable { reason: "no resolvable subject".to_string() };
        };
        if !components.iter().any(|c| cdx_component_ref(c) != Some(root)) {
            return CellOutcome::NotApplicable {
                reason: "document has no component other than the subject".to_string(),
            };
        }
        let reachable = cdx_reachable(cdx);
        let mut offenders: Vec<String> = components
            .iter()
            .filter(|c| {
                let Some(r) = cdx_component_ref(c) else { return false };
                r != root && !reachable.contains(r) && !explained_pre_1156(c)
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
}
