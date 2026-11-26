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
//!
//! **#1465: runtime evidence wins.** A `runtime-closure` member the closure
//! holds carries no mark, whatever scope a reader gave it. CycloneDX alone
//! cannot tell such a member from one the closure marked, so this arm reads
//! the closure's verdict from SPDX 2.3 (see [`spdx23_closure_verdict`]).

use std::collections::HashSet;

use super::super::{CellOutcome, Violation};
use super::{cdx_component_ref, cdx_components, cdx_prop};

pub const KIND_TOOLING_WITHOUT_MARKER: &str = "closure-tooling-without-non-runtime-marker";
/// Milestone 1423 — a member C203 says is statically linked into a runtime
/// member, which nonetheless carries a not-in-artifact mark.
pub const KIND_LINKED_BUT_MARKED: &str = "closure-statically-linked-but-marked";
/// #1465 — a member the runtime closure holds, which nonetheless carries a
/// non-runtime mark (a reader's lifecycle scope, which runtime evidence
/// overrides).
pub const KIND_RUNTIME_BUT_MARKED: &str = "closure-runtime-member-but-marked";
pub const KINDS: &[&str] =
    &[KIND_TOOLING_WITHOUT_MARKER, KIND_LINKED_BUT_MARKED, KIND_RUNTIME_BUT_MARKED];

/// How a member's C203 value says it was decided (milestone 1423,
/// `specs/1423-nix-runtime-scope/contracts/c203-values.md`).
pub(crate) enum Decided {
    /// An exemption: nothing to check.
    Exempt,
    /// The runtime closure decided it, in whichever direction. Inside the
    /// closure it must carry no mark (#1465); outside, a mark is correct.
    RuntimeClosure,
    /// The walk decided it, in whichever direction. The walk approximates the
    /// runtime closure and is not evidence that a member ships, so a reader's
    /// scope on a member it reached is not contradicted by anything.
    ByWalk,
    /// Statically linked into a runtime member: must carry no mark.
    StaticallyLinked,
    /// The milestone-1277 role rule, which is also every pre-1423 document.
    ByRole,
}

/// Matched exhaustively over the closed C203 enum, for #892's reason. An
/// unrecognised value falls to the role rule, which is the stricter reading.
pub(crate) fn decided(component: &serde_json::Value) -> Decided {
    match cdx_prop(component, "waybill:scope-classification-source") {
        Some("exempt-compiler-supplied" | "exempt-evaluated-package") => Decided::Exempt,
        Some("runtime-closure") => Decided::RuntimeClosure,
        Some("reachability-walk") => Decided::ByWalk,
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
pub(crate) fn role_does_not_ship(role: &str) -> bool {
    match role {
        "build-tooling" | "unreferenced" => true,
        "artifact-input" | "both" => false,
        // An unrecognised role is not evidence either way, so it is not in
        // the population. The emitter's own classifier fails closed the same
        // way.
        _ => false,
    }
}

/// Whether the component carries a non-runtime marker, in either spelling
/// ([`super::cdx_carries_non_runtime_marker`]).
///
/// A row reading C42 alone would report 95 of milestone 1277's 97 classified
/// components as offenders *after* a correct refresh. Measured: that is
/// exactly what the first draft of this row did, and the prediction against
/// the committed goldens is what caught it before the refresh rather than
/// after. #1283 moved the helper to `checks` so that I3's marker test reads
/// the same two spellings; I3's *population* still reads C42 alone.
pub(crate) fn carries_non_runtime_marker(component: &serde_json::Value) -> bool {
    super::cdx_carries_non_runtime_marker(component)
}

/// The closure's own not-in-artifact verdict, read from SPDX 2.3: the PURLs
/// of the packages on the member side of a `BUILD_TOOL_OF` edge, and the
/// PURLs of every package the document holds.
///
/// **Why SPDX 2.3, and not CycloneDX.** CycloneDX has one field for two
/// marks: `scope: excluded` is set by the closure's verdict
/// (`artifact_presence`) and, separately, by a reader's lifecycle scope. A
/// member inside the runtime closure carrying a reader's `development`
/// (#1465's QuickCheck) and a test dependency outside it carrying the same
/// (`hspec`) are therefore the same CycloneDX component: C203
/// `runtime-closure`, C42 `development`, `scope: excluded`. Measured on the
/// committed slack-web golden before #1465. SPDX 2.3 keeps the two apart:
/// `BUILD_TOOL_OF` is emitted from the closure's verdict alone
/// (`generate/spdx/relationships.rs`), and the lifecycle scope rides the
/// dependency-edge types instead. hspec has a `BUILD_TOOL_OF` edge there;
/// QuickCheck and th-compat had none.
///
/// This depends on SPDX 2.3's full relationship vocabulary, which is the
/// default and what the corpus harness emits; `--spdx2-relationship-compat
/// basic` drops `BUILD_TOOL_OF` entirely.
pub(crate) fn spdx23_closure_verdict(spdx: &serde_json::Value) -> (HashSet<&str>, HashSet<&str>) {
    let id_to_purl = super::spdx_id_to_purl(spdx);
    let marked = spdx
        .get("relationships")
        .and_then(|r| r.as_array())
        .map(|rels| {
            rels.iter()
                .filter(|r| r.get("relationshipType").and_then(|t| t.as_str()) == Some("BUILD_TOOL_OF"))
                .filter_map(|r| r.get("spdxElementId")?.as_str())
                .filter_map(|id| id_to_purl.get(id).copied())
                .collect()
        })
        .unwrap_or_default();
    (marked, id_to_purl.values().copied().collect())
}

pub fn check(sboms: &super::EmittedSboms) -> CellOutcome {
    let components = cdx_components(&sboms.cdx);
    let (closure_marked, spdx23_purls) = spdx23_closure_verdict(&sboms.spdx_2_3);

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
    let mut runtime_but_marked: HashSet<&str> = HashSet::new();
    for c in &population {
        let marked = carries_non_runtime_marker(c);
        match decided(c) {
            // Being exempt satisfies the row, and says so. This is the arm
            // that makes the row checkable at all.
            Decided::Exempt | Decided::ByWalk => {}
            // #1465: the closure holds it — SPDX 2.3 carries no
            // `BUILD_TOOL_OF` for it — so a mark can only be a reader's scope,
            // which runtime evidence overrides. A member SPDX 2.3 does not
            // hold is not judged: the document cannot say which way the
            // closure went.
            Decided::RuntimeClosure => {
                let purl = c.get("purl").and_then(|p| p.as_str());
                let closure_holds = purl
                    .is_some_and(|p| spdx23_purls.contains(p) && !closure_marked.contains(p));
                if marked && closure_holds {
                    runtime_but_marked.extend(cdx_component_ref(c));
                }
            }
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
        (KIND_RUNTIME_BUT_MARKED, runtime_but_marked),
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
