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
/// Milestone 1423 — a member C203 says is statically linked into a runtime
/// member, which nonetheless carries a not-in-artifact mark.
pub const KIND_LINKED_BUT_MARKED: &str = "closure-statically-linked-but-marked";
pub const KINDS: &[&str] = &[KIND_TOOLING_WITHOUT_MARKER, KIND_LINKED_BUT_MARKED];

/// How a member's C203 value says it was decided (milestone 1423,
/// `specs/1423-nix-runtime-scope/contracts/c203-values.md`).
enum Decided {
    /// An exemption: nothing to check.
    Exempt,
    /// The runtime closure or the walk decided it, in whichever direction.
    /// The native mark says which, and nothing else in the document says
    /// otherwise, so there is nothing further to compare it against.
    ByEvidence,
    /// Statically linked into a runtime member: must carry no mark.
    StaticallyLinked,
    /// The milestone-1277 role rule, which is also every pre-1423 document.
    ByRole,
}

/// Matched exhaustively over the closed C203 enum, for #892's reason. An
/// unrecognised value falls to the role rule, which is the stricter reading.
fn decided(component: &serde_json::Value) -> Decided {
    match cdx_prop(component, "waybill:scope-classification-source") {
        Some("exempt-compiler-supplied" | "exempt-evaluated-package") => Decided::Exempt,
        Some("runtime-closure" | "reachability-walk") => Decided::ByEvidence,
        Some("statically-linked") => Decided::StaticallyLinked,
        Some("closure-role" | "closure-role+lifecycle-propagation") | None => Decided::ByRole,
        Some(_) => Decided::ByRole,
    }
}

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

    // Milestone 1423: every closure member, not only the non-shipping roles.
    // Under the runtime closure a `build-tooling` member the output references
    // (openssh's `dns-root-data`) is legitimately unmarked, and an
    // `artifact-input` member it does not reference is legitimately marked,
    // so the role alone no longer says what the mark must be. C203 does.
    let population: Vec<&serde_json::Value> = components
        .iter()
        .filter(|c| cdx_prop(c, "waybill:closure-role").is_some())
        .collect();

    if population.is_empty() {
        return CellOutcome::NotApplicable {
            reason: "no closure members in this document".to_string(),
        };
    }

    let mut unmarked_tooling: HashSet<&str> = HashSet::new();
    let mut linked_but_marked: HashSet<&str> = HashSet::new();
    for c in &population {
        let marked = carries_non_runtime_marker(c);
        match decided(c) {
            // Being exempt satisfies the row, and says so. This is the arm
            // that makes the row checkable at all.
            Decided::Exempt | Decided::ByEvidence => {}
            // The converse check: linked into a runtime member, so a mark
            // would contradict the document's own evidence.
            Decided::StaticallyLinked => {
                if marked {
                    linked_but_marked.extend(cdx_component_ref(c));
                }
            }
            // Today's rule, which pre-1423 documents still meet.
            Decided::ByRole => {
                let role = cdx_prop(c, "waybill:closure-role").unwrap_or_default();
                if role_does_not_ship(role) && !marked {
                    unmarked_tooling.extend(cdx_component_ref(c));
                }
            }
        }
    }

    let mut violations = Vec::new();
    for (kind, set) in [
        (KIND_TOOLING_WITHOUT_MARKER, unmarked_tooling),
        (KIND_LINKED_BUT_MARKED, linked_but_marked),
    ] {
        if set.is_empty() {
            continue;
        }
        let mut refs: Vec<String> = set.into_iter().map(str::to_string).collect();
        refs.sort();
        violations.push(Violation {
            kind,
            count: refs.len(),
            // Sorted, so the example is stable across runs rather than
            // whichever the hash set yielded first.
            example: refs.first().cloned(),
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

    /// curl's libxml2: an `artifact-input` the output does not reference.
    #[test]
    fn i10_a_marked_artifact_member_the_runtime_closure_decided_passes() {
        for source in ["runtime-closure", "reachability-walk"] {
            assert!(
                matches!(check(&sboms(with_source("artifact-input", true, source))), CellOutcome::Pass),
                "{source}"
            );
        }
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
}
