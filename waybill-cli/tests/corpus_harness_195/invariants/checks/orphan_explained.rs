//! I7 — a component not reachable from the subject by any dependency
//! edge or containment carries a marker explaining why.
//!
//! A component nothing explains leaves a consumer unable to tell whether
//! it is a real part of the artifact or a reader artefact.
//!
//! **Boundary with I3**: this invariant's population is the unreachable
//! one. I3's is reachable-but-only-through-non-runtime-edges. The two
//! preconditions are disjoint by construction (FR-019).

use super::super::{CellOutcome, Violation};
use std::collections::{HashSet, VecDeque};

use super::{cdx_adjacency, cdx_component_ref, cdx_components, cdx_prop, cdx_subject_ref};
#[cfg(test)]
use super::cdx_reachable;

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
pub fn is_explained(component: &serde_json::Value) -> bool {
    cdx_prop(component, "waybill:orphan-reason").is_some()
        || cdx_prop(component, "waybill:version-absence") == Some("by-construction")
        || cdx_prop(component, "waybill:component-tier") == Some("file")
        || cdx_prop(component, "waybill:unresolved-reason").is_some()
        // Milestone 1280 (#1346) — mirrors the narrowing in
        // `generate/orphan_reason.rs`'s 2b-bis guard, and must keep mirroring
        // it: a closure role explains an orphan only when it says nothing
        // references the member. A role asserting a reference, on a component
        // nothing reaches, is a contradiction rather than an explanation, and
        // such a component is expected to carry
        // `waybill:orphan-reason = reachable-only-through-unemitted-package`.
        || (cdx_prop(component, "waybill:closure-role") == Some("unreferenced")
            // Milestone 1282 — narrowed again, mirroring the second
            // narrowing in `generate/orphan_reason.rs`.
            //
            // Every member milestone 1282 adds is `unreferenced`, so without
            // this clause the arm above would accept all 513 as explained
            // whether or not they carried a reason — and this invariant
            // would keep passing if the reason stopped being emitted. The
            // check is only sensitive to the thing it exists to catch
            // because these components are held to the `orphan-reason` arm
            // at the top instead.
            && cdx_prop(component, "waybill:closure-member-role").is_none())
        // C170, added by milestone 1275 — and it moves cells, unlike the
        // forward-looking arm above. The closure-identity pass emits a
        // versionless `compiler-supplied` component for a declared dependency
        // the closure does not contain, which is unreachable by construction
        // and carries no `closure-role` because it is not a closure member.
        // `nix-closure-moat` gains three of them (`directory`, `ghc-heap`,
        // `pretty`), and without this arm it gains three I7 offenders where it
        // has none today — a defect created by the milestone that would then
        // have to be declared rather than fixed.
        || cdx_prop(component, "waybill:haskell-version-unresolved-reason").is_some()
        // C218, milestone 966 — mirrors `generate/orphan_reason.rs`: the same
        // synthesised boot-library component, versioned from its compiler.
        || cdx_prop(component, "waybill:boot-library-compiler").is_some()
}

/// Every component, nested ones included, each with its container's ref.
///
/// Spec 1541 (#1593): a component nested in another is part of the
/// document, and before this the check read only the top level, so the 644
/// libraries nested in binaries on a ROCm SDK were outside its population.
pub fn cdx_components_with_containers(
    cdx: &serde_json::Value,
) -> Vec<(&serde_json::Value, Option<&str>)> {
    fn walk<'a>(
        container: &'a serde_json::Value,
        container_ref: Option<&'a str>,
        out: &mut Vec<(&'a serde_json::Value, Option<&'a str>)>,
    ) {
        for c in container.get("components").and_then(|c| c.as_array()).into_iter().flatten() {
            out.push((c, container_ref));
            walk(c, cdx_component_ref(c), out);
        }
    }
    let mut out = Vec::new();
    walk(cdx, None, &mut out);
    out
}

/// Refs reachable from the subject by a dependency edge or by containment
/// (spec 1541 FR-003): a nested component is reached through its container,
/// and what it depends on is reached through it.
pub fn reachable_with_containment(cdx: &serde_json::Value) -> HashSet<&str> {
    let mut seen: HashSet<&str> = HashSet::new();
    let Some(root) = cdx_subject_ref(cdx) else { return seen };
    let mut adj = cdx_adjacency(cdx);
    for (c, container) in cdx_components_with_containers(cdx) {
        if let (Some(container), Some(r)) = (container, cdx_component_ref(c)) {
            adj.entry(container).or_default().push(r);
        }
    }
    let mut q: VecDeque<&str> = VecDeque::new();
    q.push_back(root);
    seen.insert(root);
    while let Some(cur) = q.pop_front() {
        for next in adj.get(cur).into_iter().flatten() {
            if seen.insert(next) {
                q.push_back(next);
            }
        }
    }
    seen
}

pub fn check(sboms: &super::EmittedSboms) -> CellOutcome {
    let cdx = &sboms.cdx;
    let components: Vec<&serde_json::Value> =
        cdx_components_with_containers(cdx).into_iter().map(|(c, _)| c).collect();

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

    let reachable = reachable_with_containment(cdx);
    let mut offenders: Vec<String> = components
        .iter()
        .filter(|c| {
            let Some(r) = cdx_component_ref(c) else { return false };
            r != root && !reachable.contains(r) && !is_explained(c)
        })
        .filter_map(|c| cdx_component_ref(c))
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
