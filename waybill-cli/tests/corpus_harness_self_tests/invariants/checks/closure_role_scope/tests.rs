#[allow(unused_imports)]
use std::collections::HashSet;
#[allow(unused_imports)]
use crate::corpus_harness_195::invariants::{CellOutcome, Violation};
#[allow(unused_imports)]
use crate::corpus_harness_195::invariants::checks::{cdx_component_ref, cdx_components, cdx_prop};
use crate::corpus_harness_195::invariants::checks::closure_role_scope::*;
use crate::corpus_harness_195::harness::EmittedSboms;

/// A document with one member in a non-shipping role.
///
/// `marker` controls whether it carries the non-runtime scope this row
/// demands; `exempt` whether it carries the C203 value saying the rule
/// deliberately left it alone. The two are independent, which is the
/// point: a passing document is one where **one** of them is present.
fn doc(role: &str, marker: bool, exempt: bool) -> serde_json::Value {
    let mut props = vec![serde_json::json!({
        "name": "waybill:closure-role", "value": role
    })];
    if marker {
        // `cdx_is_non_runtime` reads the C42 property, not the native
        // `scope` field, so a fixture asserting the marker must carry it
        // — the same trap the m1276 row's fixture documents.
        props.push(serde_json::json!({
            "name": "waybill:lifecycle-scope", "value": "build"
        }));
    }
    if exempt {
        props.push(serde_json::json!({
            "name": "waybill:scope-classification-source",
            "value": "exempt-compiler-supplied"
        }));
    }
    serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "metadata": { "component": { "bom-ref": "pkg:generic/app@1.0.0" } },
        "components": [
            { "bom-ref": "pkg:generic/tool@1.0.0", "name": "tool",
              "purl": "pkg:generic/tool@1.0.0", "type": "library",
              // Omitted entirely when unmarked, which is the pre-feature
        // shape: the schema reads an absent `scope` as `required`.
        "scope": if marker { Some("excluded") } else { None },
              "properties": props },
        ],
        "dependencies": [],
    })
}

fn sboms(cdx: serde_json::Value) -> EmittedSboms {
    use std::path::PathBuf;
    EmittedSboms {
        cdx,
        spdx_2_3: serde_json::json!({}),
        spdx_3: serde_json::json!({}),
        // No SPDX 2.3 packages: the #1465 arm judges nothing unless a
        // test supplies one (`with_spdx23`).
        paths: crate::corpus_harness_195::harness::EmittedPaths {
            cdx: PathBuf::new(),
            spdx_2_3: PathBuf::new(),
            spdx_3: PathBuf::new(),
        },
    }
}

/// The native-field spelling, which is the one this feature actually
/// emits: `scope: "excluded"` with **no** C42 property. A row reading
/// C42 alone fails here, which is what the first draft did.
#[test]
fn i10_the_native_scope_alone_satisfies_the_row() {
    let mut d = doc("build-tooling", false, false);
    d["components"][0]["scope"] = serde_json::json!("excluded");
    assert!(
        matches!(check(&sboms(d)), CellOutcome::Pass),
        "the native CycloneDX scope is milestone 1277's marker; C42 is not set"
    );
}

#[test]
fn i10_a_marked_build_tool_passes() {
    assert!(matches!(
        check(&sboms(doc("build-tooling", true, false))),
        CellOutcome::Pass
    ));
}

#[test]
fn i10_a_marked_unreferenced_member_passes() {
    assert!(matches!(
        check(&sboms(doc("unreferenced", true, false))),
        CellOutcome::Pass
    ));
}

/// T029 / SC-006. The mutation this row exists to catch: the marker
/// removed from a non-shipping member.
///
/// Without this the three tests above would pass for a checker that
/// never fails — which is indistinguishable from a checker that never
/// runs.
#[test]
fn i10_an_unmarked_build_tool_fails() {
    match check(&sboms(doc("build-tooling", false, false))) {
        CellOutcome::Fail(v) => {
            assert_eq!(v.len(), 1);
            assert_eq!(v[0].kind, KIND_TOOLING_WITHOUT_MARKER);
            assert_eq!(v[0].count, 1);
            assert_eq!(v[0].example.as_deref(), Some("pkg:generic/tool@1.0.0"));
        }
        other => panic!("expected a failure for an unmarked build tool, got {other:?}"),
    }
}

#[test]
fn i10_an_unmarked_unreferenced_member_fails() {
    match check(&sboms(doc("unreferenced", false, false))) {
        CellOutcome::Fail(_) => {}
        other => panic!("expected a failure, got {other:?}"),
    }
}

/// The exemption arm, and the reason this row is checkable at all: an
/// unmarked member that **says it was exempted** passes.
#[test]
fn i10_an_exempt_member_passes_without_a_marker() {
    assert!(
        matches!(
            check(&sboms(doc("build-tooling", false, true))),
            CellOutcome::Pass
        ),
        "a member recording `exempt-compiler-supplied` must satisfy this row"
    );
}

/// Milestone 1377 — the evaluated package is exempt the same way, and
/// `unreferenced` is the role it carries (nothing consumes it).
#[test]
fn i10_the_evaluated_package_passes_without_a_marker() {
    let mut d = doc("unreferenced", false, false);
    d["components"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "name": "waybill:scope-classification-source",
            "value": "exempt-evaluated-package"
        }));
    assert!(matches!(check(&sboms(d)), CellOutcome::Pass));
    // CONTROL: the same member without the value fails.
    assert!(!matches!(
        check(&sboms(doc("unreferenced", false, false))),
        CellOutcome::Pass
    ));
}

/// The control on the exemption arm. If the arm matched any C203 value
/// — or any value at all — a classified member that lost its marker
/// would pass, and the row would be worthless on exactly the population
/// it covers.
#[test]
fn i10_a_classified_member_that_lost_its_marker_still_fails() {
    let mut d = doc("build-tooling", false, false);
    d["components"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "name": "waybill:scope-classification-source",
            "value": "closure-role"
        }));
    match check(&sboms(d)) {
        CellOutcome::Fail(_) => {}
        other => panic!(
            "a member claiming the role classified it, with no marker, must fail: {other:?}"
        ),
    }
}

/// FR-002's half of the role rule: these two roles need no marker, so an
/// absent one is correct. Milestone 1423 widened the population to every
/// closure member, so the document now passes rather than reporting n/a.
#[test]
fn i10_artifact_roles_need_no_marker_under_the_role_rule() {
    for role in ["artifact-input", "both"] {
        assert!(
            matches!(check(&sboms(doc(role, false, false))), CellOutcome::Pass),
            "{role} needs no marker"
        );
    }
}

// ---- milestone 1423: the evidence values (contracts/c203-values.md) ----

fn with_source(role: &str, marker: bool, source: &str) -> serde_json::Value {
    let mut d = doc(role, marker, false);
    d["components"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "name": "waybill:scope-classification-source", "value": source
        }));
    d
}

/// openssh's `dns-root-data`: a tooling role the output references. The
/// runtime closure decided it, so no mark is correct.
#[test]
fn i10_an_unmarked_tooling_member_the_runtime_closure_decided_passes() {
    for source in ["runtime-closure", "reachability-walk"] {
        assert!(
            matches!(check(&sboms(with_source("build-tooling", false, source))), CellOutcome::Pass),
            "{source}"
        );
    }
}

/// The SPDX 2.3 twin of [`doc`]'s member, `pkg:generic/tool@1.0.0`.
/// `build_tool_of` is the closure's own not-in-artifact verdict.
fn spdx23(build_tool_of: bool) -> serde_json::Value {
    let mut rels = vec![serde_json::json!({
        "spdxElementId": "SPDXRef-DOCUMENT", "relationshipType": "DESCRIBES",
        "relatedSpdxElement": "SPDXRef-app" })];
    if build_tool_of {
        rels.push(serde_json::json!({
            "spdxElementId": "SPDXRef-tool", "relationshipType": "BUILD_TOOL_OF",
            "relatedSpdxElement": "SPDXRef-app" }));
    }
    let pkg = |id: &str, purl: &str| {
        serde_json::json!({ "SPDXID": id, "externalRefs": [
            { "referenceCategory": "PACKAGE-MANAGER", "referenceType": "purl",
              "referenceLocator": purl } ] })
    };
    serde_json::json!({
        "packages": [pkg("SPDXRef-app", "pkg:generic/app@1.0.0"), pkg("SPDXRef-tool", "pkg:generic/tool@1.0.0")],
        "relationships": rels,
    })
}

fn with_spdx23(cdx: serde_json::Value, spdx: serde_json::Value) -> EmittedSboms {
    let mut s = sboms(cdx);
    s.spdx_2_3 = spdx;
    s
}

/// curl's libxml2: an `artifact-input` the output does not reference. The
/// closure marked it, so SPDX 2.3 carries its `BUILD_TOOL_OF`.
#[test]
fn i10_a_marked_artifact_member_the_runtime_closure_decided_passes() {
    for source in ["runtime-closure", "reachability-walk"] {
        assert!(
            matches!(
                check(&with_spdx23(with_source("artifact-input", true, source), spdx23(true))),
                CellOutcome::Pass
            ),
            "{source}"
        );
    }
}

/// #1465 — slack-web's QuickCheck: the runtime closure holds it (no
/// `BUILD_TOOL_OF` in SPDX 2.3), yet it carries a mark, from a reader's
/// `development`. Runtime evidence wins, so the mark is a defect.
#[test]
fn i10_a_runtime_closure_member_the_closure_holds_that_is_marked_fails() {
    match check(&with_spdx23(with_source("artifact-input", true, "runtime-closure"), spdx23(false))) {
        CellOutcome::Fail(v) => {
            assert_eq!(v.len(), 1);
            assert_eq!(v[0].kind, KIND_RUNTIME_BUT_MARKED);
            assert_eq!(v[0].count, 1);
            assert_eq!(v[0].example.as_deref(), Some("pkg:generic/tool@1.0.0"));
        }
        other => panic!("expected a failure, got {other:?}"),
    }
    // CONTROL: unmarked, the same member passes.
    assert!(matches!(
        check(&with_spdx23(with_source("artifact-input", false, "runtime-closure"), spdx23(false))),
        CellOutcome::Pass
    ));
}

/// The walk is not evidence of shipping (#1465 scopes the rule to
/// `runtime-closure` and `statically-linked`), so a reader's mark on a
/// member it reached contradicts nothing.
#[test]
fn i10_a_marked_member_the_walk_reached_passes() {
    assert!(matches!(
        check(&with_spdx23(with_source("artifact-input", true, "reachability-walk"), spdx23(false))),
        CellOutcome::Pass
    ));
}

/// A member SPDX 2.3 does not hold is not judged: the document cannot say
/// which way the closure went.
#[test]
fn i10_a_runtime_closure_member_missing_from_spdx23_is_not_judged() {
    assert!(matches!(
        check(&sboms(with_source("artifact-input", true, "runtime-closure"))),
        CellOutcome::Pass
    ));
}

#[test]
fn i10_a_statically_linked_member_passes_unmarked() {
    assert!(matches!(
        check(&sboms(with_source("artifact-input", false, "statically-linked"))),
        CellOutcome::Pass
    ));
}

/// The converse check. A member C203 says is linked into a runtime
/// member, yet marked not-in-artifact, contradicts the document's own
/// evidence.
#[test]
fn i10_a_statically_linked_member_that_is_marked_fails() {
    match check(&sboms(with_source("artifact-input", true, "statically-linked"))) {
        CellOutcome::Fail(v) => {
            assert_eq!(v.len(), 1);
            assert_eq!(v[0].kind, KIND_LINKED_BUT_MARKED);
            assert_eq!(v[0].example.as_deref(), Some("pkg:generic/tool@1.0.0"));
        }
        other => panic!("expected a failure, got {other:?}"),
    }
}

/// A pre-1423 document: C203 `closure-role` on marked tooling members,
/// nothing on the rest. It still passes, and losing a mark still fails.
#[test]
fn i10_a_pre_1423_document_still_passes_under_the_role_rule() {
    let d = serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "metadata": { "component": { "bom-ref": "pkg:generic/app@1.0.0" } },
        "components": [
            { "bom-ref": "pkg:generic/tool@1.0.0", "name": "tool", "type": "library",
              "scope": "excluded",
              "properties": [
                { "name": "waybill:closure-role", "value": "build-tooling" },
                { "name": "waybill:scope-classification-source", "value": "closure-role" } ] },
            { "bom-ref": "pkg:generic/lib@1.0.0", "name": "lib", "type": "library",
              "properties": [ { "name": "waybill:closure-role", "value": "artifact-input" } ] },
        ],
        "dependencies": [],
    });
    assert!(matches!(check(&sboms(d.clone())), CellOutcome::Pass));
    // CONTROL: the same document with the tooling member's mark removed.
    let mut lost = d;
    lost["components"][0].as_object_mut().unwrap().remove("scope");
    match check(&sboms(lost)) {
        CellOutcome::Fail(v) => assert_eq!(v[0].kind, KIND_TOOLING_WITHOUT_MARKER),
        other => panic!("expected a failure, got {other:?}"),
    }
}

/// 16 of 17 corpus targets emit no closure role at all. The row must say
/// n/a there rather than Pass — a row that passes where it cannot apply
/// is indistinguishable from one that passes where it can.
#[test]
fn i10_reports_not_applicable_without_any_closure_role() {
    let d = serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "metadata": { "component": { "bom-ref": "pkg:cargo/app@1.0.0" } },
        "components": [
            { "bom-ref": "pkg:cargo/x@1.0.0", "name": "x",
              "purl": "pkg:cargo/x@1.0.0", "type": "library" },
        ],
        "dependencies": [],
    });
    assert!(matches!(
        check(&sboms(d)),
        CellOutcome::NotApplicable { .. }
    ));
}

/// An unrecognised role is not evidence either way, so the role rule
/// demands no marker of it — matching the emitter's own fail-closed
/// classifier. #892 is why this is pinned: a membership test over an
/// enum silently absorbed a variant added later and survived about two
/// years. (Milestone 1423 widened the population to every closure member,
/// so this passes rather than reporting n/a.)
#[test]
fn i10_an_unknown_role_demands_no_marker() {
    assert!(matches!(
        check(&sboms(doc("some-new-role", false, false))),
        CellOutcome::Pass
    ));
}
