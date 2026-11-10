//! I10 — a closure member that does not ship carries a marker saying so.
//!
//! Milestone 1277 (#1342). CycloneDX 1.6 states that "if scope is not
//! specified, `required` scope SHOULD be assumed by the consumer of the
//! BOM", so a build tool with no marker reads as part of the artifact. This
//! row asserts the marker is there.
//!
//! **The boundary with I9 and I3, which the three partition between them.**
//! I3 asserts a component reachable only through non-runtime edges carries a
//! non-runtime marker *unless the compiler supplies it*. I9 asserts a
//! component the compiler supplies carries no such marker at all. This row
//! asserts a component whose **closure role** says it does not ship carries
//! one — again unless the compiler supplies it, and that exception is the
//! seam where all three meet. I3's own header records getting a boundary
//! wrong once, when a loose one put two instances of a single defect class
//! under the wrong number; this is why the exception is stated here rather
//! than left implicit.
//!
//! **Why the exemption is read from C203 and not from C170.** I9 identifies
//! compiler-supplied components by C170
//! `waybill:haskell-version-unresolved-reason == "compiler-supplied"`, which
//! milestone 1275 sets on the *synthetic* components it emits for declared
//! edges to boot libraries the closure does not contain. A closure member
//! that happens to be a boot library carries no such property — measured on
//! milestone 1277's fixture, the exempt member carries only
//! `waybill:closure-identity` and `waybill:closure-role`. So an invariant
//! reusing I9's signal would not recognise the exemption and would report
//! correct output as a defect.
//!
//! That is why milestone 1277 emits the exemption as a C203 value of its
//! own: `exempt-compiler-supplied`. The requirement came out of writing this
//! row, not out of the spec — with the exemption collapsed into absence, a
//! deliberate omission and a missed component are the same document.

use std::collections::HashSet;

use super::super::{CellOutcome, Violation};
use super::{cdx_component_ref, cdx_components, cdx_is_non_runtime, cdx_prop};

pub const KIND_TOOLING_WITHOUT_MARKER: &str = "closure-tooling-without-non-runtime-marker";
pub const KINDS: &[&str] = &[KIND_TOOLING_WITHOUT_MARKER];

/// Roles whose members do not go into the artifact.
///
/// Matched exhaustively against the C182 wire values rather than tested for
/// membership of a list, so a role added upstream reaches this row as an
/// unknown rather than silently joining or leaving the population. #892 is
/// the precedent for why: a `matches!` allowlist over an enum missed a
/// variant added later and survived about two years.
fn role_does_not_ship(role: &str) -> bool {
    match role {
        "build-tooling" | "unreferenced" => true,
        "artifact-input" | "both" => false,
        // An unrecognised role is not evidence either way, so it is not in
        // the population. The emitter's own classifier fails closed the same
        // way.
        _ => false,
    }
}

/// Whether the component carries a non-runtime marker, in either spelling.
///
/// **Both, and the distinction is load-bearing.** The shared
/// `cdx_is_non_runtime` helper reads only C42 `waybill:lifecycle-scope`,
/// which is correct for the rows that use it — but milestone 1277's marker
/// is the **native** CycloneDX `scope` field, set deliberately without C42
/// so that the emission stays outside the `--include-dev` gate and
/// `--exclude-scope build` cannot delete the components (spec research
/// R1–R2). A row reading C42 alone would report 95 of this feature's 97
/// classified components as offenders *after* a correct refresh.
///
/// Measured: that is exactly what the first draft of this row did, and the
/// prediction against the committed goldens is what caught it before the
/// refresh rather than after.
///
/// The shared helper is deliberately **not** widened. I3 uses it to define
/// its own population, and adding the native field there would change which
/// components I3 covers — a different row's behaviour, altered as a side
/// effect of this one. Milestone 1277 adds no C42, so I3's population is
/// unaffected by this feature either way.
fn carries_non_runtime_marker(component: &serde_json::Value) -> bool {
    if cdx_is_non_runtime(component) {
        return true;
    }
    // The native field. `required` is the schema's default and says the
    // opposite, so only the two non-runtime values count.
    matches!(
        component.get("scope").and_then(|s| s.as_str()),
        Some("excluded") | Some("optional")
    )
}

pub fn check(sboms: &super::EmittedSboms) -> CellOutcome {
    let components = cdx_components(&sboms.cdx);

    let population: Vec<&serde_json::Value> = components
        .iter()
        .filter(|c| {
            cdx_prop(c, "waybill:closure-role").is_some_and(role_does_not_ship)
        })
        .collect();

    if population.is_empty() {
        return CellOutcome::NotApplicable {
            reason: "no closure members in a non-shipping role in this document".to_string(),
        };
    }

    let offenders: HashSet<&str> = population
        .iter()
        // Carrying the marker satisfies the row, in **either** spelling.
        .filter(|c| !carries_non_runtime_marker(c))
        // Being exempt satisfies it too, and says so. This is the arm that
        // makes the row checkable at all.
        .filter(|c| {
            !matches!(
                cdx_prop(c, "waybill:scope-classification-source"),
                // Milestone 1377: the evaluated package is the artifact.
                Some("exempt-compiler-supplied" | "exempt-evaluated-package")
            )
        })
        .filter_map(|c| cdx_component_ref(c))
        .collect();

    if offenders.is_empty() {
        return CellOutcome::Pass;
    }

    let mut refs: Vec<String> = offenders.into_iter().map(str::to_string).collect();
    refs.sort();
    CellOutcome::Fail(vec![Violation {
        kind: KIND_TOOLING_WITHOUT_MARKER,
        count: refs.len(),
        // Sorted, so the example is stable across runs rather than whichever
        // the hash set yielded first.
        example: refs.first().cloned(),
    }])
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::corpus_harness_195::invariants::EmittedSboms;

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
            // This row reads only the CycloneDX document.
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

    /// FR-002's half of the population rule: these two roles are not in it,
    /// so an absent marker is correct and the row reports n/a.
    #[test]
    fn i10_artifact_roles_are_not_in_the_population() {
        for role in ["artifact-input", "both"] {
            assert!(
                matches!(
                    check(&sboms(doc(role, false, false))),
                    CellOutcome::NotApplicable { .. }
                ),
                "{role} must not be in the population"
            );
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

    /// An unrecognised role is not evidence either way, so it is out of the
    /// population — matching the emitter's own fail-closed classifier. #892
    /// is why this is pinned: a membership test over an enum silently
    /// absorbed a variant added later and survived about two years.
    #[test]
    fn i10_an_unknown_role_is_not_in_the_population() {
        assert!(matches!(
            check(&sboms(doc("some-new-role", false, false))),
            CellOutcome::NotApplicable { .. }
        ));
    }
}
