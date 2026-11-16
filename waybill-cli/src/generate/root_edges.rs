//! Spec 1537 (#1566, #1538), research R2: the document root's direct
//! dependency targets, computed once for every consumer.
//!
//! Before this module the rule "attach every graph-top to a root that has
//! no edges" lived in four copies: the CycloneDX dependency builder, the
//! SPDX 2.3 and SPDX 3 document builders, and the graph-completeness
//! classifier that mirrors them. Each was patched separately, and they
//! drifted. They added root anchors on different sides of the gate, so a
//! git submodule switched the fallback off in two formats and not the
//! third, and the classifier never saw anchors at all, so every document
//! said `complete`. They also disagreed about which roots get the fallback,
//! which edges close it, and whether nested components count.
//!
//! The rule, one definition for all four:
//!
//! - **Gate.** The fallback is open when no relationship leaves the root
//!   other than to a lockfile-resolve component (#892, #894). It applies to
//!   any root: synthetic, promoted main module or operator override.
//!   Every `RelationshipType` is a dependency type, and emitter-built
//!   tool-use edges (#1438) are not scan relationships, so neither can
//!   close it.
//! - **Anchors** are root edges supplied out of band: a supplement's
//!   subject's dependencies (#1009), top-level git submodules (spec 1530)
//!   and requirements declared at the scan root (spec 1537). They are added
//!   after the gate is decided and never change it.
//! - **Graph-tops** are components with no container (`parent_purl` is
//!   `None`) that no relationship targets. A nested component is reached
//!   through its container, not as a dependency of the root.

use std::collections::{BTreeSet, HashSet};

use waybill_common::resolution::{Relationship, ResolvedComponent};

fn is_resolve_component(c: &ResolvedComponent) -> bool {
    c.extra_annotations
        .get("waybill:component-kind")
        .and_then(|v| v.as_str())
        == Some("lockfile-resolve")
}

/// Whether the fallback applies: no relationship leaves `root_ref` except to
/// a lockfile-resolve component.
pub(crate) fn fallback_open(
    components: &[ResolvedComponent],
    relationships: &[Relationship],
    root_ref: &str,
) -> bool {
    let resolve: HashSet<&str> = components
        .iter()
        .filter(|c| is_resolve_component(c))
        .map(|c| c.purl.as_str())
        .collect();
    !relationships
        .iter()
        .any(|r| r.from == root_ref && !resolve.contains(r.to.as_str()))
}

/// The root's direct targets that do not come from a relationship sourced
/// at the root: present anchors, plus the graph-tops when the fallback is
/// open. Emitters add an edge for each. Sorted, and never contains
/// `root_ref` or a relationship target the root already has.
pub(crate) fn synthesized_root_targets(
    components: &[ResolvedComponent],
    relationships: &[Relationship],
    root_ref: &str,
    anchors: &[String],
) -> BTreeSet<String> {
    let present: HashSet<&str> = components.iter().map(|c| c.purl.as_str()).collect();
    let existing: HashSet<&str> = relationships
        .iter()
        .filter(|r| r.from == root_ref)
        .map(|r| r.to.as_str())
        .collect();
    let mut out: BTreeSet<String> = anchors
        .iter()
        .filter(|a| present.contains(a.as_str()))
        .cloned()
        .collect();
    if !root_ref.is_empty() && fallback_open(components, relationships, root_ref) {
        let depended_on: HashSet<&str> = relationships.iter().map(|r| r.to.as_str()).collect();
        out.extend(
            components
                .iter()
                .filter(|c| c.parent_purl.is_none())
                .map(|c| c.purl.as_str())
                .filter(|p| !depended_on.contains(p))
                .map(str::to_string),
        );
    }
    out.retain(|t| t != root_ref && !existing.contains(t.as_str()));
    out
}

/// Every direct target of the root: relationships sourced at the root, plus
/// [`synthesized_root_targets`].
pub(crate) fn root_targets(
    components: &[ResolvedComponent],
    relationships: &[Relationship],
    root_ref: &str,
    anchors: &[String],
) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = relationships
        .iter()
        .filter(|r| r.from == root_ref)
        .map(|r| r.to.clone())
        .collect();
    out.extend(synthesized_root_targets(components, relationships, root_ref, anchors));
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::generate::graph_completeness::test_support::{mk_component as make_component, mk_rel as rel};

    const ROOT: &str = "pkg:generic/waybill-fixture-root@0.0.0";
    const A: &str = "pkg:pypi/waybill-fixture-a@1.0.0";
    const B: &str = "pkg:pypi/waybill-fixture-b@2.0.0";
    const DEP: &str = "pkg:generic/waybill-fixture-dep@1";
    const C: &str = "pkg:pypi/waybill-fixture-c@3.0.0";

    fn s(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn an_anchor_does_not_close_the_gate() {
        let comps = vec![make_component(A), make_component(B), make_component(DEP)];
        let got = root_targets(&comps, &[], ROOT, &[DEP.to_string()]);
        assert_eq!(got, s(&[A, B, DEP]));
    }

    #[test]
    fn a_dependency_edge_closes_the_gate_and_anchors_still_attach() {
        let comps = vec![make_component(A), make_component(C), make_component(DEP)];
        let got = root_targets(&comps, &[rel(ROOT, C)], ROOT, &[DEP.to_string()]);
        assert_eq!(got, s(&[C, DEP]), "A is not a declared target and the gate is closed");
    }

    #[test]
    fn a_resolve_anchor_does_not_close_the_gate() {
        let mut resolve = make_component("pkg:generic/waybill-fixture-resolve");
        resolve
            .extra_annotations
            .insert("waybill:component-kind".to_string(), serde_json::json!("lockfile-resolve"));
        let comps = vec![resolve, make_component(A)];
        let got = root_targets(&comps, &[rel(ROOT, "pkg:generic/waybill-fixture-resolve")], ROOT, &[]);
        assert_eq!(got, s(&["pkg:generic/waybill-fixture-resolve", A]));
    }

    #[test]
    fn a_nested_component_is_not_a_graph_top() {
        let mut nested = make_component("pkg:generic/waybill-fixture-nested@1");
        nested.parent_purl = Some(A.to_string());
        let comps = vec![make_component(A), nested];
        assert_eq!(root_targets(&comps, &[], ROOT, &[]), s(&[A]));
    }

    #[test]
    fn a_depended_on_component_is_not_a_graph_top() {
        let comps = vec![make_component(A), make_component(B)];
        let rels = vec![rel(A, B)];
        assert_eq!(root_targets(&comps, &rels, ROOT, &[]), s(&[A]));
    }

    #[test]
    fn the_root_is_never_its_own_target_and_a_missing_anchor_is_dropped() {
        let comps = vec![make_component(ROOT), make_component(A)];
        let got = root_targets(&comps, &[], ROOT, &[ROOT.to_string(), "pkg:generic/waybill-fixture-gone@1".to_string()]);
        assert_eq!(got, s(&[A]));
    }

    #[test]
    fn synthesized_targets_exclude_existing_root_edges() {
        let comps = vec![make_component(C), make_component(DEP)];
        let got = synthesized_root_targets(&comps, &[rel(ROOT, C)], ROOT, &[C.to_string(), DEP.to_string()]);
        assert_eq!(got, s(&[DEP]));
    }

    /// Contract I-4: every target is a relationship target, an anchor, or,
    /// only while no non-resolve dependency edge leaves the root, a
    /// graph-top without a container.
    #[test]
    fn every_target_is_declared_anchored_or_an_open_gate_graph_top() {
        let mut nested = make_component("pkg:generic/waybill-fixture-nested@1");
        nested.parent_purl = Some(A.to_string());
        let cases: Vec<(Vec<ResolvedComponent>, Vec<Relationship>, Vec<String>)> = vec![
            (vec![make_component(A), make_component(B), nested.clone()], vec![], vec![]),
            (vec![make_component(A), make_component(B), make_component(C)], vec![rel(ROOT, C)], vec![A.to_string()]),
            (vec![make_component(A), make_component(B)], vec![rel(A, B)], vec![DEP.to_string()]),
        ];
        for (comps, rels, anchors) in cases {
            let open = fallback_open(&comps, &rels, ROOT);
            for t in root_targets(&comps, &rels, ROOT, &anchors) {
                let declared = rels.iter().any(|r| r.from == ROOT && r.to == t);
                let anchored = anchors.contains(&t);
                let top = comps.iter().any(|c| c.purl.as_str() == t && c.parent_purl.is_none())
                    && !rels.iter().any(|r| r.to == t);
                assert!(declared || anchored || (open && top), "{t} is not licensed");
            }
        }
    }
}
