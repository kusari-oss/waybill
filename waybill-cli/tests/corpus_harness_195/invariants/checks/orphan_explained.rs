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

    /// Milestone 1275 — a compiler-supplied boot library is explained by
    /// C170, and nothing weaker counts.
    ///
    /// Unlike the C197 arm below, this one **moves cells**:
    /// `nix-closure-moat` gains three components of exactly this shape
    /// (`directory`, `ghc-heap`, `pretty`), emitted for declared
    /// dependencies the closure does not contain because GHC ships them
    /// inside its own derivation. They are unreachable from the root by
    /// construction — nothing depends on a boot library except closure
    /// members that are themselves unreachable — and they carry no
    /// `waybill:closure-role`, because they are not closure members.
    #[test]
    fn a_compiler_supplied_boot_library_is_explained() {
        let explained = versionless_edgeless(
            "waybill-fixture-bootlib",
            serde_json::json!([
                { "name": "waybill:haskell-version-unresolved-reason",
                  "value": "compiler-supplied" }
            ]),
        );
        assert!(is_explained(&explained));

        // The negative control that matters: `sbom-tier` alone, which is
        // what these components carried before the C170 arm existed, is
        // NOT an explanation. Without this the test would pass for a
        // component carrying nothing of substance.
        let tier_only = versionless_edgeless(
            "waybill-fixture-bootlib-tier-only",
            serde_json::json!([
                { "name": "waybill:sbom-tier", "value": "build" }
            ]),
        );
        assert!(!is_explained(&tier_only));

        // And no properties at all is not an explanation either.
        let bare = versionless_edgeless("waybill-fixture-bare", serde_json::json!([]));
        assert!(!is_explained(&bare));
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
        // Every arm of `is_explained` EXCEPT the C197 one under test. The
        // name says "pre_1156" because C197 is what milestone 1156 added, not
        // because this stops at that milestone: an arm added later and omitted
        // here makes the two predicates differ for a reason the test does not
        // name, and the assertion then blames C197 for it.
        //
        // That is exactly what happened when milestone 1275 added the C170 arm
        // to `is_explained` and not to this copy. Nothing failed until
        // `nix-closure-moat`'s goldens were refreshed and gained three
        // versionless compiler-supplied components — `exceptions`, `ghc-heap`
        // and `pretty` — which `is_explained` accounts for and this did not.
        // The gap was invisible for as long as no golden held a component of
        // that shape.
        fn explained_pre_1156(component: &serde_json::Value) -> bool {
            cdx_prop(component, "waybill:orphan-reason").is_some()
                || cdx_prop(component, "waybill:component-tier") == Some("file")
                || cdx_prop(component, "waybill:unresolved-reason").is_some()
                || cdx_prop(component, "waybill:closure-role").is_some()
                || cdx_prop(component, "waybill:haskell-version-unresolved-reason").is_some()
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

    // -----------------------------------------------------------------
    // Spec 1541 (#1593), FR-003 — nested components
    // -----------------------------------------------------------------

    fn sboms(cdx: serde_json::Value) -> super::super::EmittedSboms {
        use std::path::PathBuf;
        super::super::EmittedSboms {
            cdx,
            spdx_2_3: serde_json::json!({}),
            spdx_3: serde_json::json!({}),
            paths: crate::corpus_harness_195::harness::EmittedPaths {
                cdx: PathBuf::new(),
                spdx_2_3: PathBuf::new(),
                spdx_3: PathBuf::new(),
            },
        }
    }

    /// A subject depending on `bin`, which nests `lib`. `loose` is
    /// top-level, unreachable and explained, and nests `stray`.
    fn nested_doc(stray_props: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "metadata": { "component": { "bom-ref": "root@1" } },
            "components": [
                { "bom-ref": "pkg:generic/bin@1", "purl": "pkg:generic/bin@1",
                  "components": [
                      { "bom-ref": "pkg:generic/lib@2", "purl": "pkg:generic/lib@2" }
                  ] },
                { "bom-ref": "pkg:generic/loose@3", "purl": "pkg:generic/loose@3",
                  "properties": [ { "name": "waybill:orphan-reason", "value": "unowned-binary" } ],
                  "components": [
                      { "bom-ref": "pkg:generic/stray@4", "purl": "pkg:generic/stray@4",
                        "properties": stray_props }
                  ] }
            ],
            "dependencies": [
                { "ref": "root@1", "dependsOn": ["pkg:generic/bin@1"] }
            ]
        })
    }

    /// A component nested in a reachable container is reachable.
    #[test]
    fn s1541_a_nested_child_of_a_reachable_container_is_not_an_offender() {
        let doc = nested_doc(serde_json::json!([
            { "name": "waybill:orphan-reason", "value": "unowned-binary" }
        ]));
        assert!(reachable_with_containment(&doc).contains("pkg:generic/lib@2"));
        assert_eq!(check(&sboms(doc)), CellOutcome::Pass);
    }

    /// A component nested in an unreachable container is in the population,
    /// and an unexplained one is an offender.
    #[test]
    fn s1541_a_nested_child_of_an_unreachable_container_is_checked() {
        let doc = nested_doc(serde_json::json!([]));
        assert!(!reachable_with_containment(&doc).contains("pkg:generic/stray@4"));
        match check(&sboms(doc)) {
            CellOutcome::Fail(v) => {
                assert_eq!(v[0].count, 1);
                assert_eq!(v[0].example.as_deref(), Some("pkg:generic/stray@4"));
            }
            other => panic!("expected the nested stray to fail: {other:?}"),
        }
    }

    /// Dependencies of a component reached only through containment are
    /// reachable too.
    #[test]
    fn s1541_a_nested_childs_dependencies_are_reachable() {
        let mut doc = nested_doc(serde_json::json!([
            { "name": "waybill:orphan-reason", "value": "unowned-binary" }
        ]));
        doc["components"].as_array_mut().unwrap().push(serde_json::json!(
            { "bom-ref": "pkg:generic/dep@5", "purl": "pkg:generic/dep@5" }
        ));
        doc["dependencies"].as_array_mut().unwrap().push(serde_json::json!(
            { "ref": "pkg:generic/lib@2", "dependsOn": ["pkg:generic/dep@5"] }
        ));
        assert!(reachable_with_containment(&doc).contains("pkg:generic/dep@5"));
        assert_eq!(check(&sboms(doc)), CellOutcome::Pass);
    }
}
