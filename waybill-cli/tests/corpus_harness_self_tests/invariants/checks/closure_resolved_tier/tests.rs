#[allow(unused_imports)]
use crate::corpus_harness_195::invariants::{CellOutcome, Violation};
#[allow(unused_imports)]
use crate::corpus_harness_195::invariants::checks::{cdx_component_ref, cdx_prop};
use crate::corpus_harness_195::invariants::checks::closure_resolved_tier::*;
use crate::corpus_harness_195::harness::EmittedSboms;

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
