//! Milestone 1277 (#1342) — the non-runtime classification a nix closure
//! role establishes.
//!
//! `--nix-closure` records how nix itself referenced each member: C182
//! `waybill:closure-role` carries `artifact-input`, `build-tooling`, `both`
//! or `unreferenced`, read from nix's own dependency fields rather than
//! inferred. A member in a tooling role builds the artifact and is not in
//! it, yet carried no native marker saying so — and CycloneDX 1.6 states
//! that "if scope is not specified, `required` scope SHOULD be assumed by
//! the consumer of the BOM". So a consumer filtering for what is in the
//! artifact was handed every nix build tool as though it shipped.
//!
//! Measured on `nix-closure-moat` before this pass: 80 `build-tooling` and
//! 17 `unreferenced` members, 97 of 362, none carrying a scope.
//!
//! # What this pass must never do
//!
//! **It may only add a non-runtime classification, never assert presence.**
//! `ARTIFACT_FIELDS` is `buildInputs`, `propagatedBuildInputs`,
//! `depsHostHost`, and nixpkgs' Haskell builder passes test dependencies
//! through `buildInputs` — so `artifact-input` means "nix referenced it as
//! an input to the thing being built", not "it is in the artifact at
//! runtime". `hspec` is the measured counterexample: `artifact-input`, and
//! not in the shipped binary. Nine such components already carry
//! `excluded` from the milestone-1269 propagation, and this pass must leave
//! them exactly as they are (spec FR-002).
//!
//! # Why the exemption reads the boot set
//!
//! Milestone 1276 made a package the pinned compiler supplies immune to
//! *inheriting* a non-runtime scope; this pass *assigns* one, so a boot
//! library in a tooling role is in both populations and invariant I9
//! asserts it carries no such marker. Two signals for "compiler-supplied"
//! exist: the boot set, which the propagation reads, and C170, which I9
//! reads off the emitted document. This pass reads the **boot set**, so the
//! two remain independent — if they ever disagree, I9 fails rather than
//! both passing. I9's own header warns of exactly that: "Both must read one
//! source, or this row and the propagation disagree about one document and
//! each is locally satisfiable while the pair is not."
//!
//! An empty boot set exempts nothing (FR-005). Unearned exclusion is as
//! dangerous as m1276's unearned immunity.

use std::collections::HashSet;

use waybill_common::resolution::{ArtifactPresence, ResolvedComponent};

/// The C182 annotation key this pass reads.
///
/// A literal rather than a shared constant because the producer
/// (`scan_fs::package_db::nix::closure::classify::DerivationRole::wire`)
/// and the other consumer (`generate::orphan_reason`) both spell it
/// literally today. Changing that is a wider refactor than this feature.
pub(crate) const CLOSURE_ROLE: &str = "waybill:closure-role";

/// What one run of the pass did, for the INFO line and for the tests.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RoleScopeSummary {
    /// Members classified from a `build-tooling` role.
    pub(crate) assigned_build_tooling: usize,
    /// Members classified from an `unreferenced` role.
    pub(crate) assigned_unreferenced: usize,
    /// Members in a tooling role that the compiler supplies, so exempt.
    pub(crate) exempt_compiler_supplied: usize,
    /// Members carrying a closure role at all, so the denominator.
    pub(crate) roles_seen: usize,
}

impl RoleScopeSummary {
    /// Total members this pass classified.
    pub(crate) fn assigned(&self) -> usize {
        self.assigned_build_tooling + self.assigned_unreferenced
    }
}

/// Classify every closure member whose role says it is not in the artifact.
///
/// Returns a zeroed summary and changes nothing when no component carries a
/// closure role, which is every scan without `--nix-closure` and every scan
/// whose closure tier degraded (FR-007). **An absent role is never treated
/// as `build-tooling`.**
pub(crate) fn assign_role_scope(
    components: &mut [ResolvedComponent],
    boot_libraries: &HashSet<String>,
) -> RoleScopeSummary {
    let mut summary = RoleScopeSummary::default();

    for component in components.iter_mut() {
        // The role is read from the unioned C182 value, never from an
        // individual derivation variant (FR-003). The union happens in the
        // classifier: measured, 13 of 97 duplicated identities in a real
        // closure disagree about their role, and `flex 2.6.4` is `Both` as
        // one variant and `Unreferenced` as another. By the time the
        // annotation exists, that is already resolved.
        let Some(role) = component
            .extra_annotations
            .get(CLOSURE_ROLE)
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        summary.roles_seen += 1;

        // Milestone 1377 (#1398) — the derivation the evaluated attribute
        // built IS the artifact, so it cannot be absent from it. Nothing in
        // the closure consumes it, which is why it classifies `unreferenced`;
        // that is a fact about nix's references, not about presence. Before
        // this, openssh 10.3p1, the package a scan of its flake was about,
        // was emitted `scope: excluded`. No counter: the subject is named by
        // the document itself, so the exemption is visible where it applies.
        if component
            .extra_annotations
            .get(crate::generate::root_selector::NIX_EVALUATED_PACKAGE_KEY)
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            continue;
        }

        // Matched exhaustively over the wire values rather than tested for
        // membership of a list. A role added upstream lands in the final
        // arm and classifies nothing, which is the fail-closed answer
        // (FR-007): it must reach this pass deliberately, not by
        // defaulting into exclusion.
        let classifies = match role {
            "build-tooling" => Some(Role::BuildTooling),
            "unreferenced" => Some(Role::Unreferenced),
            // FR-002. `ARTIFACT_FIELDS` includes `buildInputs`, through
            // which nixpkgs' Haskell builder passes test dependencies, so
            // neither of these is evidence of runtime presence and this
            // pass asserts nothing about them. Any scope they carry came
            // from the milestone-1269 propagation and must survive
            // untouched.
            "artifact-input" | "both" => None,
            _ => None,
        };
        let Some(role) = classifies else { continue };

        // FR-005 / FR-014. A package the pinned compiler supplies is
        // installed in every build, so no format may report it absent.
        // Checked after the role so the counter distinguishes "exempted"
        // from "not in a tooling role at all" — the distinction milestone
        // 1276 learned to keep, when "the rule applied and exempted
        // nothing" and "the rule did not apply" turned out to be different
        // facts.
        if compiler_supplied(component, boot_libraries) {
            summary.exempt_compiler_supplied += 1;
            // Recorded rather than skipped. An exemption that leaves no
            // trace is indistinguishable from a rule that never ran, which
            // is the shape invariant I10 cannot check and a reader cannot
            // audit. See `ArtifactPresence::ExemptCompilerSupplied`.
            component.artifact_presence = Some(ArtifactPresence::ExemptCompilerSupplied);
            continue;
        }

        component.artifact_presence = Some(ArtifactPresence::NotInArtifact);
        match role {
            Role::BuildTooling => summary.assigned_build_tooling += 1,
            Role::Unreferenced => summary.assigned_unreferenced += 1,
        }
    }

    summary
}

/// The two roles this pass classifies from, so the counter cannot be
/// incremented for a role that was not matched.
#[derive(Clone, Copy)]
enum Role {
    BuildTooling,
    Unreferenced,
}

/// The C203 annotation key.
pub(crate) const CLASSIFICATION_SOURCE: &str = "waybill:scope-classification-source";

/// The C203 value for a component, or `None` when this feature did not
/// classify it.
///
/// One function for all three emitters rather than three copies, so the
/// formats agree by construction. C19 is the lesson: when one format's value
/// drifted in shape from the others, its parity extractor was quietly
/// hand-written to compensate, and the difference only surfaced on Go CPEs.
///
/// Absence means "the closure role did not classify this". It deliberately
/// does **not** report the propagation acting alone: C62
/// `waybill:lifecycle-scope-derivation` already carries that, and repeating
/// it would put a row on every dev-scoped component in every ecosystem —
/// measured at 35 golden byte-identity failures when milestone 1276 did the
/// equivalent thing (spec FR-009's scope correction).
pub(crate) fn classification_source(
    component: &ResolvedComponent,
) -> Option<&'static str> {
    let presence = component.artifact_presence?;
    if !presence.is_absent_from_artifact() {
        // The exempt state. Reported as itself, because "the rule exempted
        // this" and "the rule did not reach this" are different facts and
        // absence can only mean one of them.
        return Some(presence.as_str());
    }
    // The propagation reached the same conclusion independently. Worth
    // distinguishing: a reader deciding whether to trust one analysis wants
    // to know the other agreed.
    let also_propagated = component
        .lifecycle_scope
        .is_some_and(|s| s.is_non_runtime());
    Some(if also_propagated {
        "closure-role+lifecycle-propagation"
    } else {
        presence.as_str()
    })
}

/// Whether a PURL names a Hackage package.
///
/// Duplicated deliberately from `scope_propagation`: see the module header
/// on why the two passes read `compiler-supplied` independently.
fn is_haskell_purl(purl: &str) -> bool {
    purl.starts_with("pkg:hackage/")
}

/// Whether the pinned revision's compiler supplies this component.
fn compiler_supplied(component: &ResolvedComponent, boot: &HashSet<String>) -> bool {
    !boot.is_empty()
        && is_haskell_purl(component.purl.as_str())
        && boot.contains(component.name.as_str())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::generate::graph_completeness::test_support::mk_component;

    /// A component carrying `role`, or no role at all when `role` is `None`.
    ///
    /// Built through the workspace's own factory rather than a literal:
    /// milestone 1277's own T005 had to touch 57 `ResolvedComponent`
    /// literals across 47 files to add one field, and a hand-rolled literal
    /// here would be the 58th.
    fn component(purl: &str, role: Option<&str>) -> ResolvedComponent {
        let mut c = mk_component(purl);
        if let Some(r) = role {
            c.extra_annotations.insert(
                CLOSURE_ROLE.to_string(),
                serde_json::Value::String(r.to_string()),
            );
        }
        c
    }

    fn presence(components: &[ResolvedComponent], purl: &str) -> Option<ArtifactPresence> {
        components
            .iter()
            .find(|c| c.purl.as_str() == purl)
            .expect("component present")
            .artifact_presence
    }

    // ---- T007: one test per row of contracts/role-scope.md ----

    #[test]
    fn m1277_build_tooling_is_classified_as_not_in_the_artifact() {
        let mut c = vec![component("pkg:generic/tool@1.0.0", Some("build-tooling"))];
        let s = assign_role_scope(&mut c, &HashSet::new());
        assert_eq!(presence(&c, "pkg:generic/tool@1.0.0"), Some(ArtifactPresence::NotInArtifact));
        assert_eq!(s.assigned_build_tooling, 1);
    }

    #[test]
    fn m1277_unreferenced_is_classified_as_not_in_the_artifact() {
        let mut c = vec![component("pkg:generic/patch@1.0.0", Some("unreferenced"))];
        let s = assign_role_scope(&mut c, &HashSet::new());
        assert_eq!(presence(&c, "pkg:generic/patch@1.0.0"), Some(ArtifactPresence::NotInArtifact));
        assert_eq!(s.assigned_unreferenced, 1);
    }

    /// Milestone 1377 — the evaluated package is classified `unreferenced`
    /// because nothing consumes it, and must not be declared absent from
    /// the artifact it is.
    #[test]
    fn m1377_the_evaluated_package_is_never_classified_not_in_the_artifact() {
        let mut root = component("pkg:generic/waybill-fixture-app@2.1", Some("unreferenced"));
        root.extra_annotations.insert(
            crate::generate::root_selector::NIX_EVALUATED_PACKAGE_KEY.to_string(),
            serde_json::Value::Bool(true),
        );
        // CONTROL: the same role without the tag is still classified.
        let mut c = vec![root, component("pkg:generic/patch@1.0.0", Some("unreferenced"))];
        let s = assign_role_scope(&mut c, &HashSet::new());
        assert_eq!(presence(&c, "pkg:generic/waybill-fixture-app@2.1"), None);
        assert_eq!(presence(&c, "pkg:generic/patch@1.0.0"), Some(ArtifactPresence::NotInArtifact));
        assert_eq!(s.assigned_unreferenced, 1);
        assert_eq!(s.roles_seen, 2);
    }

    /// FR-002. `artifact-input` includes test dependencies on real data —
    /// `hspec` is one — so the role is not evidence of runtime presence and
    /// this pass must assert nothing about it.
    #[test]
    fn m1277_artifact_input_is_left_alone() {
        let mut c = vec![component("pkg:hackage/lib@1.0.0", Some("artifact-input"))];
        let s = assign_role_scope(&mut c, &HashSet::new());
        assert_eq!(presence(&c, "pkg:hackage/lib@1.0.0"), None);
        assert_eq!(s.assigned(), 0);
    }

    #[test]
    fn m1277_both_is_left_alone() {
        let mut c = vec![component("pkg:generic/dual@1.0.0", Some("both"))];
        assign_role_scope(&mut c, &HashSet::new());
        assert_eq!(presence(&c, "pkg:generic/dual@1.0.0"), None);
    }

    // ---- T009: fail closed (FR-007) ----

    /// An absent role is **not** `build-tooling`. This is the m1276 posture
    /// restated with the sign flipped: unearned exclusion is as dangerous
    /// as unearned immunity.
    #[test]
    fn m1277_an_absent_role_classifies_nothing() {
        let mut c = vec![component("pkg:cargo/plain@1.0.0", None)];
        let s = assign_role_scope(&mut c, &HashSet::new());
        assert_eq!(presence(&c, "pkg:cargo/plain@1.0.0"), None);
        assert_eq!(s.roles_seen, 0);
        assert_eq!(s.assigned(), 0);
    }

    /// A degraded closure tier emits no roles at all, so the whole document
    /// is untouched — the byte-identity guarantee FR-008 depends on.
    #[test]
    fn m1277_a_document_with_no_roles_is_untouched() {
        let mut c = vec![
            component("pkg:cargo/a@1.0.0", None),
            component("pkg:npm/b@1.0.0", None),
        ];
        let before = c.clone();
        let s = assign_role_scope(&mut c, &HashSet::new());
        assert_eq!(c, before, "a document with no closure roles must not move");
        assert_eq!(s, RoleScopeSummary::default());
    }

    // ---- T008: the union is established upstream, not here ----

    /// FR-003 asks that the classification derive from the **unioned** role
    /// and never from one derivation variant. This pass cannot get that
    /// wrong, because it never sees variants: by the time the C182
    /// annotation exists the classifier has already unioned them
    /// (`closure::classify::DerivationRole::union`), and the annotation is
    /// a single string.
    ///
    /// So the requirement is verified upstream, by
    /// `closure::emit::tests::build_variants_merge_into_one_component_and_union_their_roles`
    /// and `::unreferenced_is_absorbed_rather_than_winning_a_union`, both
    /// passing. A test here could only assert that the pass reads the one
    /// value available to it, which cannot fail — and a test that cannot
    /// fail is the thing this milestone pair keeps finding. What *is* worth
    /// pinning here is that a unioned `both` — the value a
    /// tooling-plus-artifact member gets — classifies nothing, since that
    /// is the union's whole point reaching this pass's decision.
    #[test]
    fn m1277_a_unioned_both_classifies_nothing_which_is_the_unions_point() {
        let mut c = vec![component("pkg:generic/flex@2.6.4", Some("both"))];
        let s = assign_role_scope(&mut c, &HashSet::new());
        assert_eq!(presence(&c, "pkg:generic/flex@2.6.4"), None);
        assert_eq!(s.roles_seen, 1, "the role was seen");
        assert_eq!(s.assigned(), 0, "but it classified nothing");
    }

    // ---- T017/T018/T019: the exemption (FR-005, FR-014) ----

    /// FR-005. A package the pinned compiler supplies is installed in every
    /// build, so no format may report it absent — even in a tooling role.
    #[test]
    fn m1277_a_compiler_supplied_member_in_a_tooling_role_is_exempt() {
        let mut c = vec![component("pkg:hackage/containers@0.6.7", Some("build-tooling"))];
        let boot: HashSet<String> = ["containers".to_string()].into_iter().collect();
        let s = assign_role_scope(&mut c, &boot);
        let p = presence(&c, "pkg:hackage/containers@0.6.7");
        assert_eq!(
            p,
            Some(ArtifactPresence::ExemptCompilerSupplied),
            "the exemption is recorded, not collapsed into absence -- a member the \
             rule left alone and one it never reached must be distinguishable"
        );
        assert!(
            !p.unwrap().is_absent_from_artifact(),
            "and the recorded state must NOT claim the member is absent from the \
             artifact, which is the whole point of the exemption"
        );
        assert_eq!(s.exempt_compiler_supplied, 1);
        assert_eq!(s.assigned(), 0);
    }

    /// FR-005a. The exemption withholds the classification, not the role.
    ///
    /// **This test cannot fail against the current implementation**, and the
    /// control proves it: with the exemption stubbed out, the two tests
    /// either side of it failed and this one passed, because no code path
    /// removes the annotation in either case. It is kept deliberately, as a
    /// guard against a future implementation that suppresses the role along
    /// with the scope — which would destroy the evidence a reader needs to
    /// see *why* a tooling-role member carries no marker. Recorded rather
    /// than quietly counted as exemption coverage: a test that passes for a
    /// reason unrelated to its name is the defect milestone 1275 found in
    /// its own most-important test (F7 there).
    #[test]
    fn m1277_an_exempt_member_keeps_its_closure_role_annotation() {
        let mut c = vec![component("pkg:hackage/containers@0.6.7", Some("build-tooling"))];
        let boot: HashSet<String> = ["containers".to_string()].into_iter().collect();
        assign_role_scope(&mut c, &boot);
        assert_eq!(
            c[0].extra_annotations.get(CLOSURE_ROLE).and_then(|v| v.as_str()),
            Some("build-tooling"),
            "the exemption must not suppress the classification's own evidence"
        );
    }

    /// T018's negative control: the exemption is narrow, not blanket. Without
    /// this, an exemption that matched everything would satisfy the test
    /// above and silently disable the whole feature.
    #[test]
    fn m1277_a_non_compiler_supplied_member_in_the_same_role_is_still_classified() {
        let mut c = vec![
            component("pkg:hackage/containers@0.6.7", Some("build-tooling")),
            component("pkg:hackage/happy@2.1.7", Some("build-tooling")),
        ];
        let boot: HashSet<String> = ["containers".to_string()].into_iter().collect();
        let s = assign_role_scope(&mut c, &boot);
        assert_eq!(
            presence(&c, "pkg:hackage/containers@0.6.7"),
            Some(ArtifactPresence::ExemptCompilerSupplied),
            "exempt, and recorded as such"
        );
        assert_eq!(
            presence(&c, "pkg:hackage/happy@2.1.7"),
            Some(ArtifactPresence::NotInArtifact),
            "not in the boot set, so not exempt -- `happy` is the real corpus case: \
             it appears in the pinned configurations only as an alias, never as `happy = null`"
        );
        assert_eq!(s.exempt_compiler_supplied, 1);
        assert_eq!(s.assigned_build_tooling, 1);
    }

    /// FR-005, the m1276 posture with the sign flipped: an empty boot set
    /// exempts **nothing**. Unearned exclusion is as dangerous as unearned
    /// immunity, and the dangerous default here is to exempt by accident
    /// and silently disable the feature.
    #[test]
    fn m1277_an_empty_boot_set_exempts_nothing() {
        let mut c = vec![component("pkg:hackage/containers@0.6.7", Some("build-tooling"))];
        let s = assign_role_scope(&mut c, &HashSet::new());
        assert_eq!(
            presence(&c, "pkg:hackage/containers@0.6.7"),
            Some(ArtifactPresence::NotInArtifact)
        );
        assert_eq!(s.exempt_compiler_supplied, 0);
    }

    /// The boot set holds bare names, which are not unique across
    /// ecosystems — `binary`, `array`, `unix`, `directory` and `pretty` are
    /// all in it and all plausible elsewhere. A name match alone must not
    /// exempt a non-Haskell component, or the feature silently stops
    /// classifying nix build tools that happen to share a name.
    #[test]
    fn m1277_a_name_match_outside_hackage_is_not_exempt() {
        let mut c = vec![component("pkg:generic/unix@1.0.0", Some("build-tooling"))];
        let boot: HashSet<String> = ["unix".to_string()].into_iter().collect();
        let s = assign_role_scope(&mut c, &boot);
        assert_eq!(
            presence(&c, "pkg:generic/unix@1.0.0"),
            Some(ArtifactPresence::NotInArtifact),
            "`unix` in the boot set is the Hackage package, not every component so named"
        );
        assert_eq!(s.exempt_compiler_supplied, 0);
    }

    /// An unrecognised role value is not a licence to guess. It is neither
    /// a tooling role nor an artifact role, and the fail-closed answer is
    /// to classify nothing — a new role added upstream must reach this
    /// pass deliberately, not by defaulting into exclusion.
    #[test]
    fn m1277_an_unknown_role_classifies_nothing() {
        let mut c = vec![component("pkg:generic/odd@1.0.0", Some("some-new-role"))];
        let s = assign_role_scope(&mut c, &HashSet::new());
        assert_eq!(presence(&c, "pkg:generic/odd@1.0.0"), None);
        assert_eq!(s.assigned(), 0);
    }
}
