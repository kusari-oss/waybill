//! I11 — a component the closure resolved is not labelled `design`
//! (spec 1650, #1681).
//!
//! **Catches**: in a `--nix-closure` document,
//!
//! - (a) a component that is `design` while the closure is evidence it is in
//!   the build graph: C203 `exempt-evaluated-package` (the closure evaluated
//!   it) or C218 `waybill:boot-library-compiler` (a closure compiler supplies
//!   it);
//! - (b) a root whose direct dependencies are all `design`, which is the
//!   shape that left a consumer dropping `design` with nothing at all (moat,
//!   openai, slack-web and slacklinker at `main` `18691d8`).
//!
//! **Boundary with I9 and I10**: those assert *scope* (`optional`,
//! `excluded`) and read C203 and C218 to decide which scope a member may
//! carry. This row asserts *tier* and reads the same two annotations as
//! evidence of resolution. The rows share inputs, not claims.
//!
//! **Not applicable** to a document with no closure member (no component
//! carrying C214 `waybill:closure-contents`): there `design` is correct by
//! definition, because nothing resolved it.

use super::super::{CellOutcome, Violation};
use super::{cdx_component_ref, cdx_prop};

pub const KIND_RESOLVED_BUT_DESIGN: &str = "closure-resolved-but-design";
pub const KIND_ROOT_HANGS_OFF_DESIGN: &str = "root-direct-dependencies-all-design";
pub const KINDS: &[&str] = &[KIND_RESOLVED_BUT_DESIGN, KIND_ROOT_HANGS_OFF_DESIGN];

const TIER: &str = "waybill:sbom-tier";

/// Every CycloneDX component, nested ones included: cargo-auditable and the
/// closure's recovered modules can nest, and a top-level-only walk would miss
/// them.
fn all_components(cdx: &serde_json::Value) -> Vec<&serde_json::Value> {
    let mut out = Vec::new();
    let mut stack: Vec<&serde_json::Value> =
        cdx.get("components").and_then(|c| c.as_array()).into_iter().flatten().collect();
    while let Some(c) = stack.pop() {
        out.push(c);
        stack.extend(c.get("components").and_then(|c| c.as_array()).into_iter().flatten());
    }
    out
}

fn is_design(c: &serde_json::Value) -> bool {
    cdx_prop(c, TIER) == Some("design")
}

/// The closure's evidence that this component is in the build graph.
fn closure_resolved(c: &serde_json::Value) -> bool {
    cdx_prop(c, "waybill:scope-classification-source") == Some("exempt-evaluated-package")
        || cdx_prop(c, "waybill:boot-library-compiler").is_some()
}

pub fn check(sboms: &super::EmittedSboms) -> CellOutcome {
    let components = all_components(&sboms.cdx);
    if !components.iter().any(|c| cdx_prop(c, "waybill:closure-contents").is_some()) {
        return CellOutcome::NotApplicable {
            reason: "no closure members in this document".to_string(),
        };
    }

    let mut violations = Vec::new();

    let mut resolved_design: Vec<String> = components
        .iter()
        .filter(|c| is_design(c) && closure_resolved(c))
        .filter_map(|c| cdx_component_ref(c).map(str::to_string))
        .collect();
    resolved_design.sort();
    if !resolved_design.is_empty() {
        violations.push(Violation {
            kind: KIND_RESOLVED_BUT_DESIGN,
            count: resolved_design.len(),
            example: resolved_design.first().cloned(),
        });
    }

    let root = sboms.cdx.pointer("/metadata/component/bom-ref").and_then(|r| r.as_str());
    let direct: Vec<&str> = sboms
        .cdx
        .get("dependencies")
        .and_then(|d| d.as_array())
        .into_iter()
        .flatten()
        .find(|d| root.is_some() && d.get("ref").and_then(|r| r.as_str()) == root)
        .and_then(|d| d.get("dependsOn"))
        .and_then(|d| d.as_array())
        .into_iter()
        .flatten()
        .filter_map(|t| t.as_str())
        .collect();
    let design_of = |r: &str| {
        components
            .iter()
            .find(|c| cdx_component_ref(c) == Some(r))
            .is_some_and(|c| is_design(c))
    };
    if !direct.is_empty() && direct.iter().all(|r| design_of(r)) {
        let mut d: Vec<&str> = direct.clone();
        d.sort_unstable();
        violations.push(Violation {
            kind: KIND_ROOT_HANGS_OFF_DESIGN,
            count: d.len(),
            example: Some(d.join(", ")),
        });
    }

    if violations.is_empty() {
        CellOutcome::Pass
    } else {
        CellOutcome::Fail(violations)
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::corpus_harness_195::invariants::EmittedSboms;

    fn comp(purl: &str, tier: &str, extra: &[(&str, &str)]) -> serde_json::Value {
        let mut props = vec![serde_json::json!({"name": TIER, "value": tier})];
        props.extend(extra.iter().map(|(n, v)| serde_json::json!({"name": n, "value": v})));
        serde_json::json!({"bom-ref": purl, "purl": purl, "name": purl, "type": "library", "properties": props})
    }

    /// A closure member, which makes the row applicable.
    fn member() -> serde_json::Value {
        comp("pkg:generic/waybill-fixture-gcc@13", "build", &[("waybill:closure-contents", "absent")])
    }

    fn doc(components: Vec<serde_json::Value>, root_deps: &[&str]) -> EmittedSboms {
        use std::path::PathBuf;
        EmittedSboms {
            cdx: serde_json::json!({
                "bomFormat": "CycloneDX",
                "metadata": {"component": {"bom-ref": "pkg:generic/waybill-fixture-root@0.0.0"}},
                "components": components,
                "dependencies": [{"ref": "pkg:generic/waybill-fixture-root@0.0.0", "dependsOn": root_deps}],
            }),
            spdx_2_3: serde_json::json!({}),
            spdx_3: serde_json::json!({}),
            paths: crate::corpus_harness_195::harness::EmittedPaths {
                cdx: PathBuf::new(),
                spdx_2_3: PathBuf::new(),
                spdx_3: PathBuf::new(),
            },
        }
    }

    fn kinds(o: &CellOutcome) -> Vec<&'static str> {
        o.kinds().into_iter().collect()
    }

    const APP: &str = "pkg:hackage/waybill-fixture-app@0.1.0";
    const BUILT: &str = "pkg:hackage/waybill-fixture-dep@1.2.0";

    /// V1: the evaluated package is `design`.
    #[test]
    fn v1_evaluated_package_design_fails() {
        let d = doc(
            vec![
                member(),
                comp(APP, "design", &[("waybill:scope-classification-source", "exempt-evaluated-package")]),
                comp(BUILT, "build", &[]),
            ],
            &[APP, BUILT],
        );
        assert_eq!(kinds(&check(&d)), vec![KIND_RESOLVED_BUT_DESIGN]);
    }

    /// V2: a compiler-supplied boot library is `design`.
    #[test]
    fn v2_compiler_supplied_design_fails() {
        let base = "pkg:hackage/base@4.20.2.0";
        let d = doc(
            vec![member(), comp(base, "design", &[("waybill:boot-library-compiler", "ghc-9.10.3")]), comp(BUILT, "build", &[])],
            &[BUILT],
        );
        assert_eq!(kinds(&check(&d)), vec![KIND_RESOLVED_BUT_DESIGN]);
    }

    /// V3: the root depends only on `design` components, none of them
    /// closure-resolved.
    #[test]
    fn v3_root_hangs_off_design_fails() {
        let extra = "pkg:hackage/waybill-fixture-extra@0.2.0";
        let d = doc(vec![member(), comp(extra, "design", &[])], &[extra]);
        assert_eq!(kinds(&check(&d)), vec![KIND_ROOT_HANGS_OFF_DESIGN]);
    }

    /// V4: a `design` repository package beside a `build` root dependency is
    /// fine: nothing resolved it.
    #[test]
    fn v4_unresolved_design_beside_a_built_dependency_passes() {
        let extra = "pkg:hackage/waybill-fixture-extra@0.2.0";
        let d = doc(
            vec![
                member(),
                comp(APP, "build", &[("waybill:scope-classification-source", "exempt-evaluated-package")]),
                comp(extra, "design", &[]),
                comp("pkg:hackage/Win32", "design", &[]),
            ],
            &[APP, extra],
        );
        assert!(matches!(check(&d), CellOutcome::Pass));
    }

    /// V5: a manifest-only document full of `design` is not applicable.
    #[test]
    fn v5_manifest_only_is_not_applicable() {
        let d = doc(vec![comp(APP, "design", &[])], &[APP]);
        assert!(matches!(check(&d), CellOutcome::NotApplicable { .. }));
    }
}
