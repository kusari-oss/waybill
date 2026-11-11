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

use std::collections::{BTreeMap, HashMap, HashSet};

use waybill_common::resolution::{
    ArtifactPresence, Relationship, ResolvedComponent, ScopeEvidence,
};

use crate::scan_fs::package_db::nix::closure::member_key::{key_of_component, MemberKey};
pub(crate) use crate::scan_fs::package_db::nix::closure::runtime::RuntimeEvidence;

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
    /// Milestone 1423 — members and modules the runtime evidence (the runtime
    /// closure, or the walk) decided runtime, kept runtime by static linking,
    /// and marked not-in-artifact. All zero under the role rule, except
    /// `statically_linked` when a set-aside walk kept a wrapper's consumed
    /// static build runtime (T042); C216 says which evidence it was.
    pub(crate) decided_runtime: usize,
    pub(crate) statically_linked: usize,
    pub(crate) decided_not_in_artifact: usize,
}

impl RoleScopeSummary {
    /// Total members this pass classified from a role.
    pub(crate) fn assigned(&self) -> usize {
        self.assigned_build_tooling + self.assigned_unreferenced
    }
}

/// The provenance of the milestone-1378 host → module edges.
const EMBEDDED_BUILD_INFO: &str = "embedded-build-info";

/// Decide every closure member's presence in the artifact: the one place
/// that does (milestone 1423, data-model.md "Decision function").
///
/// For each member the first rule that applies wins:
///
/// 1. the evaluated package is exempt;
/// 2. a compiler-supplied boot library is exempt;
/// 3. under [`RuntimeEvidence::Closure`]: in the runtime closure → runtime;
///    else statically linked → runtime; else not-in-artifact — each recorded
///    with its [`ScopeEvidence`]. Statically linked means linked into a
///    runtime member, or a Haskell build the evaluated wrapper root consumed
///    (and what is linked into it), which x86_64-linux's runtime closure does
///    not hold (q10);
/// 4. under [`RuntimeEvidence::Walk`]: reached by the walk (or statically
///    linked into a reached member) → runtime; else not-in-artifact — both
///    recorded as `ReachabilityWalk`;
/// 5. under [`RuntimeEvidence::None`]: the milestone-1277 role rule. Under
///    [`RuntimeEvidence::RoleWithLinked`] (the walk set aside as
///    `reached-only-root`), the same, except that a member in `linked` is
///    runtime and `StaticallyLinked` (T042).
///
/// Runtime means `artifact_presence = None`, so a `NotInArtifact` any earlier
/// step set is cleared: FR-001 holds in both directions. Embedded Go and Rust
/// modules are decided after the members, from their hosts.
///
/// Returns a zeroed summary and changes nothing when no component carries a
/// closure role, which is every scan without `--nix-closure` and every scan
/// whose closure tier degraded (FR-007). **An absent role is never treated
/// as `build-tooling`.**
pub(crate) fn assign_role_scope(
    components: &mut [ResolvedComponent],
    boot_libraries: &HashSet<String>,
    evidence: &RuntimeEvidence,
    relationships: &[Relationship],
) -> RoleScopeSummary {
    let mut summary = match evidence {
        RuntimeEvidence::None => assign_from_roles(components, boot_libraries, &HashSet::new()),
        // Rule 4's set-aside walk (`reached-only-root`): rule 5, except for
        // the consumed static build and what is linked into it (T042).
        RuntimeEvidence::RoleWithLinked { linked } => {
            assign_from_roles(components, boot_libraries, linked)
        }
        RuntimeEvidence::Closure { runtime, static_linked } => assign_from_runtime(
            components,
            boot_libraries,
            runtime,
            static_linked,
            ScopeEvidence::RuntimeClosure,
        ),
        // Rule 4. The same decision as rule 3, with the walk's reached set in
        // place of the runtime closure and every member decided by it
        // carrying the walk's evidence, marked or not.
        RuntimeEvidence::Walk { reached } => assign_from_runtime(
            components,
            boot_libraries,
            reached,
            &HashSet::new(),
            ScopeEvidence::ReachabilityWalk,
        ),
    };
    assign_modules(components, evidence, relationships, &mut summary);
    summary
}

/// Rules 1–4 under runtime evidence: `runtime` is the runtime closure (rule
/// 3) or the walk's reached set (rule 4), and `decided_by` is the evidence
/// recorded on every member either way.
fn assign_from_runtime(
    components: &mut [ResolvedComponent],
    boot_libraries: &HashSet<String>,
    runtime: &HashSet<MemberKey>,
    static_linked: &HashSet<MemberKey>,
    decided_by: ScopeEvidence,
) -> RoleScopeSummary {
    let mut summary = RoleScopeSummary::default();
    for component in components.iter_mut() {
        if component.extra_annotations.get(CLOSURE_ROLE).and_then(|v| v.as_str()).is_none() {
            continue;
        }
        summary.roles_seen += 1;
        if is_evaluated_package(component) {
            component.artifact_presence = Some(ArtifactPresence::ExemptEvaluatedPackage);
            component.scope_evidence = None;
            continue;
        }
        // FR-006, whatever the role. The runtime closure can omit a boot
        // library the role never would have marked -- GHC's boot packages
        // are linked into a static executable from the compiler's own store
        // path, which the executable does not reference -- and no format may
        // report a package the compiler supplies as absent.
        if compiler_supplied(component, boot_libraries) {
            summary.exempt_compiler_supplied += 1;
            component.artifact_presence = Some(ArtifactPresence::ExemptCompilerSupplied);
            component.scope_evidence = None;
            continue;
        }
        let key = key_of_component(component);
        let (presence, evidence) = if key.as_ref().is_some_and(|k| runtime.contains(k)) {
            summary.decided_runtime += 1;
            (None, decided_by)
        } else if key.as_ref().is_some_and(|k| static_linked.contains(k)) {
            summary.statically_linked += 1;
            (None, ScopeEvidence::StaticallyLinked)
        } else {
            summary.decided_not_in_artifact += 1;
            (Some(ArtifactPresence::NotInArtifact), decided_by)
        };
        // Assigned, not merged: a `NotInArtifact` set earlier is cleared when
        // the runtime closure says the member ships (FR-001's second
        // direction).
        component.artifact_presence = presence;
        component.scope_evidence = Some(evidence);
    }
    summary
}

/// Embedded Go and Rust modules inherit their hosts' status (FR-002).
///
/// A module is runtime when any host is: the module is compiled into it.
/// Under the role rule this reproduces milestone 1378's behaviour, which
/// marked a module only when every member reaching it was itself marked; it
/// only ever touches modules that are not closure members themselves, as
/// that milestone did.
fn assign_modules(
    components: &mut [ResolvedComponent],
    evidence: &RuntimeEvidence,
    relationships: &[Relationship],
    summary: &mut RoleScopeSummary,
) {
    let index: HashMap<&str, usize> = components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.purl.as_str(), i))
        .collect();
    // module index → whether any host ships
    let mut ships: BTreeMap<usize, bool> = BTreeMap::new();
    for r in relationships.iter().filter(|r| r.provenance.data_type == EMBEDDED_BUILD_INFO) {
        let (Some(&host), Some(&module)) = (index.get(r.from.as_str()), index.get(r.to.as_str()))
        else {
            continue;
        };
        let host = &components[host];
        if !host.extra_annotations.contains_key(CLOSURE_ROLE) {
            continue;
        }
        let host_ships = !host
            .artifact_presence
            .is_some_and(|p| p.is_absent_from_artifact());
        *ships.entry(module).or_default() |= host_ships;
    }
    // (evidence on a module that ships, evidence on one that does not), or
    // `None` under the role rule. Under the runtime closure a shipping module
    // is runtime by link, not by reference; under the walk both directions
    // are the walk's approximation (data-model rule 4).
    let decided_by = match evidence {
        RuntimeEvidence::Closure { .. } => {
            Some((ScopeEvidence::StaticallyLinked, ScopeEvidence::RuntimeClosure))
        }
        RuntimeEvidence::Walk { .. } => {
            Some((ScopeEvidence::ReachabilityWalk, ScopeEvidence::ReachabilityWalk))
        }
        RuntimeEvidence::None | RuntimeEvidence::RoleWithLinked { .. } => None,
    };
    let store = crate::scan_fs::package_db::nix::closure::contents::store_dir();
    for (module, host_ships) in ships {
        let c = &mut components[module];
        let is_member = c.extra_annotations.contains_key(CLOSURE_ROLE);
        // Only a module the closure itself recovered from a binary is the
        // closure's to mark. One the project declares (a `go.mod`, a
        // `Cargo.lock`) has evidence outside the store, and a build tool
        // embedding the same module says nothing about whether the project
        // ships it. Milestone 1378 never marked those either.
        let markable = !is_member
            && c.evidence
                .source_file_paths
                .iter()
                .all(|p| std::path::Path::new(p).starts_with(&store));
        if let Some((shipping, absent)) = decided_by {
            if host_ships {
                // A member the evidence marked, compiled into a runtime
                // binary, ships after all.
                if !is_member || c.artifact_presence.is_some_and(|p| p.is_absent_from_artifact()) {
                    if shipping == ScopeEvidence::StaticallyLinked {
                        summary.statically_linked += 1;
                    } else {
                        summary.decided_runtime += 1;
                    }
                    c.artifact_presence = None;
                    c.scope_evidence = Some(shipping);
                }
            } else if markable {
                summary.decided_not_in_artifact += 1;
                c.artifact_presence = Some(ArtifactPresence::NotInArtifact);
                c.scope_evidence = Some(absent);
            }
        } else if markable && !host_ships {
            c.artifact_presence = Some(ArtifactPresence::NotInArtifact);
            c.scope_evidence = Some(ScopeEvidence::ClosureRole);
        }
    }
}

/// Whether a component is the derivation the evaluated attribute built.
fn is_evaluated_package(component: &ResolvedComponent) -> bool {
    component
        .extra_annotations
        .get(crate::generate::root_selector::NIX_EVALUATED_PACKAGE_KEY)
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

/// Rules 1, 2 and 5: the milestone-1277 role rule, unchanged, except for
/// members in `linked`, which are runtime and `StaticallyLinked` whatever
/// their role (T042). `linked` is empty under [`RuntimeEvidence::None`].
fn assign_from_roles(
    components: &mut [ResolvedComponent],
    boot_libraries: &HashSet<String>,
    linked: &HashSet<MemberKey>,
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
        // was emitted `scope: excluded`. Recorded rather than skipped (C203
        // `exempt-evaluated-package`): with `--root-name` the evaluated
        // package is not the subject, and an unmarked `unreferenced` member
        // is what invariant I10 reports as a rule that never ran.
        if is_evaluated_package(component) {
            component.artifact_presence = Some(ArtifactPresence::ExemptEvaluatedPackage);
            continue;
        }

        // T042 — the walk was set aside because it reached only a wrapper
        // root, and this member is the static build that root consumed, or
        // a library linked into it: its code is in the wrapper's binary, so
        // it ships, whatever its role (the build itself is `build-tooling`).
        // The compiler-supplied exemption still wins, as under rule 3.
        if key_of_component(component).is_some_and(|k| linked.contains(&k)) {
            if compiler_supplied(component, boot_libraries) {
                summary.exempt_compiler_supplied += 1;
                component.artifact_presence = Some(ArtifactPresence::ExemptCompilerSupplied);
                component.scope_evidence = None;
            } else {
                summary.statically_linked += 1;
                component.artifact_presence = None;
                component.scope_evidence = Some(ScopeEvidence::StaticallyLinked);
            }
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
        component.scope_evidence = Some(ScopeEvidence::ClosureRole);
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
    if let Some(presence) = component.artifact_presence {
        if !presence.is_absent_from_artifact() {
            // The exempt state. Reported as itself, because "the rule
            // exempted this" and "the rule did not reach this" are different
            // facts and absence can only mean one of them. An exemption wins
            // over any evidence (spec FR-006).
            return Some(presence.as_str());
        }
    }
    // Milestone 1423 — the evidence the scope pass decided from. Reported
    // whether or not the member was marked: the native mark says which way
    // it was decided, this says how (contracts/c203-values.md).
    match component.scope_evidence {
        Some(e @ (ScopeEvidence::RuntimeClosure
        | ScopeEvidence::StaticallyLinked
        | ScopeEvidence::ReachabilityWalk)) => return Some(e.as_str()),
        // Today's rule. Keeps the pre-1423 vocabulary, including the
        // propagation suffix, so a `closure-role` document reads exactly as
        // it did before this milestone.
        Some(ScopeEvidence::ClosureRole) | None => {}
    }
    let presence = component.artifact_presence?;
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

    /// The pass with no runtime evidence and no edges: the milestone-1277
    /// role rule, which every pre-1423 test here pins.
    fn role_only(c: &mut [ResolvedComponent], boot: &HashSet<String>) -> RoleScopeSummary {
        assign_role_scope(c, boot, &RuntimeEvidence::None, &[])
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
        let s = role_only(&mut c, &HashSet::new());
        assert_eq!(presence(&c, "pkg:generic/tool@1.0.0"), Some(ArtifactPresence::NotInArtifact));
        assert_eq!(s.assigned_build_tooling, 1);
    }

    #[test]
    fn m1277_unreferenced_is_classified_as_not_in_the_artifact() {
        let mut c = vec![component("pkg:generic/patch@1.0.0", Some("unreferenced"))];
        let s = role_only(&mut c, &HashSet::new());
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
        let s = role_only(&mut c, &HashSet::new());
        assert_eq!(
            presence(&c, "pkg:generic/waybill-fixture-app@2.1"),
            Some(ArtifactPresence::ExemptEvaluatedPackage)
        );
        assert_eq!(
            classification_source(&c[0]),
            Some("exempt-evaluated-package"),
            "the exemption leaves a trace (C203)"
        );
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
        let s = role_only(&mut c, &HashSet::new());
        assert_eq!(presence(&c, "pkg:hackage/lib@1.0.0"), None);
        assert_eq!(s.assigned(), 0);
    }

    #[test]
    fn m1277_both_is_left_alone() {
        let mut c = vec![component("pkg:generic/dual@1.0.0", Some("both"))];
        role_only(&mut c, &HashSet::new());
        assert_eq!(presence(&c, "pkg:generic/dual@1.0.0"), None);
    }

    // ---- T009: fail closed (FR-007) ----

    /// An absent role is **not** `build-tooling`. This is the m1276 posture
    /// restated with the sign flipped: unearned exclusion is as dangerous
    /// as unearned immunity.
    #[test]
    fn m1277_an_absent_role_classifies_nothing() {
        let mut c = vec![component("pkg:cargo/plain@1.0.0", None)];
        let s = role_only(&mut c, &HashSet::new());
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
        let s = role_only(&mut c, &HashSet::new());
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
        let s = role_only(&mut c, &HashSet::new());
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
        let s = role_only(&mut c, &boot);
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
        role_only(&mut c, &boot);
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
        let s = role_only(&mut c, &boot);
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
        let s = role_only(&mut c, &HashSet::new());
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
        let s = role_only(&mut c, &boot);
        assert_eq!(
            presence(&c, "pkg:generic/unix@1.0.0"),
            Some(ArtifactPresence::NotInArtifact),
            "`unix` in the boot set is the Hackage package, not every component so named"
        );
        assert_eq!(s.exempt_compiler_supplied, 0);
    }

    // ---- m1423 T018: the decision function ----

    fn key(name: &str, version: &str) -> MemberKey {
        MemberKey::Named(name.to_string(), version.to_string())
    }

    fn closure_evidence(runtime: &[(&str, &str)], linked: &[(&str, &str)]) -> RuntimeEvidence {
        RuntimeEvidence::Closure {
            runtime: runtime.iter().map(|(n, v)| key(n, v)).collect(),
            static_linked: linked.iter().map(|(n, v)| key(n, v)).collect(),
        }
    }

    fn embedded(host: &str, module: &str) -> Relationship {
        Relationship {
            from: host.to_string(),
            to: module.to_string(),
            relationship_type: waybill_common::resolution::RelationshipType::DependsOn,
            provenance: waybill_common::resolution::EnrichmentProvenance {
                source: "nix-closure-contents".to_string(),
                data_type: EMBEDDED_BUILD_INFO.to_string(),
            },
        }
    }

    #[test]
    fn m1423_the_runtime_closure_decides_in_both_directions() {
        let mut c = vec![
            component("pkg:generic/waybill-fixture-dns@1.0", Some("build-tooling")),
            component("pkg:generic/waybill-fixture-xml@2.0", Some("artifact-input")),
            component("pkg:generic/waybill-fixture-lib@3.0", Some("artifact-input")),
        ];
        // A mark an earlier step set is cleared when the runtime closure says
        // the member ships.
        c[0].artifact_presence = Some(ArtifactPresence::NotInArtifact);
        let ev = closure_evidence(
            &[("waybill-fixture-dns", "1.0"), ("waybill-fixture-lib", "3.0")],
            &[],
        );
        let s = assign_role_scope(&mut c, &HashSet::new(), &ev, &[]);
        assert_eq!(presence(&c, "pkg:generic/waybill-fixture-dns@1.0"), None, "tooling, but referenced");
        assert_eq!(
            presence(&c, "pkg:generic/waybill-fixture-xml@2.0"),
            Some(ArtifactPresence::NotInArtifact),
            "an artifact-input the output does not reference"
        );
        assert_eq!(presence(&c, "pkg:generic/waybill-fixture-lib@3.0"), None);
        for x in &c {
            assert_eq!(x.scope_evidence, Some(ScopeEvidence::RuntimeClosure), "{}", x.name);
        }
        assert_eq!((s.decided_runtime, s.decided_not_in_artifact), (2, 1));
    }

    /// Rule 4: the walk decides in both directions, records itself on every
    /// member and module it decided, and never falls back to the role.
    #[test]
    fn m1423_the_walk_decides_in_both_directions() {
        let mut c = vec![
            component("pkg:generic/waybill-fixture-lib@3.0", Some("artifact-input")),
            component("pkg:generic/waybill-fixture-xml@2.0", Some("artifact-input")),
            component("pkg:generic/waybill-fixture-dns@1.0", Some("build-tooling")),
            component("pkg:golang/example.test/libmod@v1.0.0", None),
            component("pkg:golang/example.test/xmlmod@v1.0.0", None),
        ];
        c[2].artifact_presence = Some(ArtifactPresence::NotInArtifact);
        let rels = [
            embedded("pkg:generic/waybill-fixture-lib@3.0", "pkg:golang/example.test/libmod@v1.0.0"),
            embedded("pkg:generic/waybill-fixture-xml@2.0", "pkg:golang/example.test/xmlmod@v1.0.0"),
        ];
        let ev = RuntimeEvidence::Walk {
            reached: [key("waybill-fixture-lib", "3.0"), key("waybill-fixture-dns", "1.0")]
                .into_iter()
                .collect(),
        };
        let s = assign_role_scope(&mut c, &HashSet::new(), &ev, &rels);
        assert_eq!(c[0].artifact_presence, None);
        assert_eq!(
            c[1].artifact_presence,
            Some(ArtifactPresence::NotInArtifact),
            "an artifact-input the walk does not reach, which the role rule would leave alone"
        );
        assert_eq!(c[2].artifact_presence, None, "reached, so a prior mark is cleared");
        assert_eq!(c[3].artifact_presence, None, "compiled into a reached host");
        assert_eq!(c[4].artifact_presence, Some(ArtifactPresence::NotInArtifact));
        for x in &c {
            assert_eq!(classification_source(x), Some("reachability-walk"), "{}", x.purl.as_str());
        }
        assert_eq!((s.decided_runtime, s.decided_not_in_artifact, s.statically_linked), (3, 2, 0));
    }

    /// T042: with the walk set aside, the role rule decides everything but
    /// the linked set, which is runtime and `statically-linked` whatever its
    /// role. The exemption still wins inside the set; outside it, a tooling
    /// member stays marked (the control).
    #[test]
    fn m1423_a_set_aside_walk_keeps_the_linked_set_runtime() {
        let mut c = vec![
            component("pkg:hackage/waybill-fixture-ShellCheck@0.11.0", Some("build-tooling")),
            component("pkg:hackage/waybill-fixture-aeson@2.2", Some("artifact-input")),
            component("pkg:hackage/containers@0.6.7", Some("artifact-input")),
            component("pkg:hackage/waybill-fixture-alex@3.5", Some("build-tooling")),
        ];
        c[0].artifact_presence = Some(ArtifactPresence::NotInArtifact);
        let ev = RuntimeEvidence::RoleWithLinked {
            linked: [
                key("waybill-fixture-ShellCheck", "0.11.0"),
                key("waybill-fixture-aeson", "2.2"),
                key("containers", "0.6.7"),
            ]
            .into_iter()
            .collect(),
        };
        let boot: HashSet<String> = ["containers".to_string()].into_iter().collect();
        let s = assign_role_scope(&mut c, &boot, &ev, &[]);
        for x in &c[..2] {
            assert_eq!(x.artifact_presence, None, "{}", x.name);
            assert_eq!(classification_source(x), Some("statically-linked"), "{}", x.name);
        }
        assert_eq!(c[2].artifact_presence, Some(ArtifactPresence::ExemptCompilerSupplied));
        assert_eq!(c[3].artifact_presence, Some(ArtifactPresence::NotInArtifact));
        assert_eq!(classification_source(&c[3]), Some("closure-role"));
        assert_eq!(
            (s.statically_linked, s.exempt_compiler_supplied, s.assigned_build_tooling),
            (2, 1, 1)
        );
    }

    #[test]
    fn m1423_static_linking_keeps_a_member_runtime() {
        let mut c = vec![component("pkg:hackage/waybill-fixture-aeson@2.2", Some("artifact-input"))];
        let ev = closure_evidence(&[], &[("waybill-fixture-aeson", "2.2")]);
        assign_role_scope(&mut c, &HashSet::new(), &ev, &[]);
        assert_eq!(c[0].artifact_presence, None);
        assert_eq!(c[0].scope_evidence, Some(ScopeEvidence::StaticallyLinked));
    }

    #[test]
    fn m1423_exemptions_win_under_the_runtime_closure() {
        let mut root = component("pkg:generic/waybill-fixture-app@1.0", Some("unreferenced"));
        root.extra_annotations.insert(
            crate::generate::root_selector::NIX_EVALUATED_PACKAGE_KEY.to_string(),
            serde_json::Value::Bool(true),
        );
        let mut c = vec![
            root,
            // An artifact-input boot library outside the runtime closure: the
            // static-executable shape, where GHC's boot packages are linked
            // from the compiler's own store path.
            component("pkg:hackage/containers@0.6.7", Some("artifact-input")),
        ];
        let boot: HashSet<String> = ["containers".to_string()].into_iter().collect();
        assign_role_scope(&mut c, &boot, &closure_evidence(&[], &[]), &[]);
        assert_eq!(c[0].artifact_presence, Some(ArtifactPresence::ExemptEvaluatedPackage));
        assert_eq!(c[1].artifact_presence, Some(ArtifactPresence::ExemptCompilerSupplied));
        assert!(c.iter().all(|x| x.scope_evidence.is_none()));
    }

    #[test]
    fn m1423_module_inherits_its_hosts_status_under_the_runtime_closure() {
        let mut c = vec![
            component("pkg:generic/waybill-fixture-app@1.0", Some("artifact-input")),
            component("pkg:generic/waybill-fixture-tool@3.0", Some("build-tooling")),
            component("pkg:golang/example.test/goa@v1.0.0", None),
            component("pkg:golang/example.test/toolmod@v0.1.0", None),
            component("pkg:golang/example.test/shared@v2.0.0", None),
        ];
        let rels = [
            embedded("pkg:generic/waybill-fixture-app@1.0", "pkg:golang/example.test/goa@v1.0.0"),
            embedded("pkg:generic/waybill-fixture-tool@3.0", "pkg:golang/example.test/toolmod@v0.1.0"),
            embedded("pkg:generic/waybill-fixture-app@1.0", "pkg:golang/example.test/shared@v2.0.0"),
            embedded("pkg:generic/waybill-fixture-tool@3.0", "pkg:golang/example.test/shared@v2.0.0"),
        ];
        let ev = closure_evidence(&[("waybill-fixture-app", "1.0")], &[]);
        assign_role_scope(&mut c, &HashSet::new(), &ev, &rels);
        let get = |p: &str| c.iter().find(|x| x.purl.as_str() == p).unwrap();
        for p in ["pkg:golang/example.test/goa@v1.0.0", "pkg:golang/example.test/shared@v2.0.0"] {
            assert_eq!(get(p).artifact_presence, None, "{p}");
            assert_eq!(get(p).scope_evidence, Some(ScopeEvidence::StaticallyLinked), "{p}");
        }
        let t = get("pkg:golang/example.test/toolmod@v0.1.0");
        assert_eq!(t.artifact_presence, Some(ArtifactPresence::NotInArtifact));
        assert_eq!(t.scope_evidence, Some(ScopeEvidence::RuntimeClosure));
    }

    /// Without runtime evidence, a module is marked only when every member
    /// reaching it is marked, which is what milestone 1378's contents pass
    /// decided before 1423 moved the decision here.
    #[test]
    fn m1423_module_under_the_role_rule_matches_milestone_1378() {
        let mut c = vec![
            component("pkg:generic/waybill-fixture-app@1.0", Some("artifact-input")),
            component("pkg:generic/waybill-fixture-tool@3.0", Some("build-tooling")),
            component("pkg:golang/example.test/shared@v2.0.0", None),
            component("pkg:golang/example.test/toolonly@v0.1.0", None),
        ];
        let rels = [
            embedded("pkg:generic/waybill-fixture-app@1.0", "pkg:golang/example.test/shared@v2.0.0"),
            embedded("pkg:generic/waybill-fixture-tool@3.0", "pkg:golang/example.test/shared@v2.0.0"),
            embedded("pkg:generic/waybill-fixture-tool@3.0", "pkg:golang/example.test/toolonly@v0.1.0"),
        ];
        assign_role_scope(&mut c, &HashSet::new(), &RuntimeEvidence::None, &rels);
        assert_eq!(c[2].artifact_presence, None, "a shipped member reaches it");
        assert_eq!(c[3].artifact_presence, Some(ArtifactPresence::NotInArtifact));
        assert_eq!(classification_source(&c[3]), Some("closure-role"));
    }

    /// A module the project declares itself (here, from a `go.mod`) is not
    /// the closure's to mark, even when a build tool's binary also embeds
    /// it. Milestone 1378 only ever marked modules it recovered from a
    /// binary; a manifest-derived one got the host edge and nothing else.
    #[test]
    fn m1423_a_manifest_declared_module_is_never_marked_from_a_tool_binary() {
        for ev in [closure_evidence(&[("waybill-fixture-app", "1.0")], &[]), RuntimeEvidence::None] {
            let mut declared = component("pkg:golang/example.test/declared@v1.0.0", None);
            declared.evidence.source_file_paths = vec!["go.mod".to_string()];
            let mut recovered = component("pkg:golang/example.test/recovered@v1.0.0", None);
            recovered.evidence.source_file_paths =
                vec!["/nix/store/aaaa-waybill-fixture-tool-3.0/bin/tool".to_string()];
            let mut c = vec![
                component("pkg:generic/waybill-fixture-app@1.0", Some("artifact-input")),
                component("pkg:generic/waybill-fixture-tool@3.0", Some("build-tooling")),
                declared,
                recovered,
            ];
            let rels = [
                embedded("pkg:generic/waybill-fixture-tool@3.0", "pkg:golang/example.test/declared@v1.0.0"),
                embedded("pkg:generic/waybill-fixture-tool@3.0", "pkg:golang/example.test/recovered@v1.0.0"),
            ];
            assign_role_scope(&mut c, &HashSet::new(), &ev, &rels);
            assert_eq!(c[2].artifact_presence, None, "the project's own go.mod declares it");
            assert_eq!(
                c[3].artifact_presence,
                Some(ArtifactPresence::NotInArtifact),
                "control: a module known only from the tool's binary is still marked"
            );
        }
    }

    // ---- m1423 T004: C203 reports the scope evidence ----

    fn with(
        presence: Option<ArtifactPresence>,
        evidence: Option<ScopeEvidence>,
    ) -> ResolvedComponent {
        let mut c = component("pkg:generic/waybill-fixture-m@1.0", Some("build-tooling"));
        c.artifact_presence = presence;
        c.scope_evidence = evidence;
        c
    }

    #[test]
    fn m1423_each_new_evidence_value_is_reported_marked_or_not() {
        for (e, wire) in [
            (ScopeEvidence::RuntimeClosure, "runtime-closure"),
            (ScopeEvidence::StaticallyLinked, "statically-linked"),
            (ScopeEvidence::ReachabilityWalk, "reachability-walk"),
        ] {
            assert_eq!(classification_source(&with(None, Some(e))), Some(wire), "unmarked {e:?}");
            assert_eq!(
                classification_source(&with(Some(ArtifactPresence::NotInArtifact), Some(e))),
                Some(wire),
                "marked {e:?}"
            );
        }
    }

    #[test]
    fn m1423_closure_role_evidence_keeps_the_pre_1423_vocabulary() {
        let mut c = with(Some(ArtifactPresence::NotInArtifact), Some(ScopeEvidence::ClosureRole));
        assert_eq!(classification_source(&c), Some("closure-role"));
        c.lifecycle_scope = Some(waybill_common::resolution::LifecycleScope::Development);
        assert_eq!(classification_source(&c), Some("closure-role+lifecycle-propagation"));
        // No mark, so the role rule decided nothing and nothing is reported.
        assert_eq!(classification_source(&with(None, Some(ScopeEvidence::ClosureRole))), None);
        assert_eq!(classification_source(&with(None, None)), None);
    }

    #[test]
    fn m1423_an_exemption_wins_over_any_evidence() {
        for e in [
            ScopeEvidence::RuntimeClosure,
            ScopeEvidence::StaticallyLinked,
            ScopeEvidence::ReachabilityWalk,
            ScopeEvidence::ClosureRole,
        ] {
            assert_eq!(
                classification_source(&with(Some(ArtifactPresence::ExemptEvaluatedPackage), Some(e))),
                Some("exempt-evaluated-package")
            );
            assert_eq!(
                classification_source(&with(Some(ArtifactPresence::ExemptCompilerSupplied), Some(e))),
                Some("exempt-compiler-supplied")
            );
        }
    }

    /// An unrecognised role value is not a licence to guess. It is neither
    /// a tooling role nor an artifact role, and the fail-closed answer is
    /// to classify nothing — a new role added upstream must reach this
    /// pass deliberately, not by defaulting into exclusion.
    #[test]
    fn m1277_an_unknown_role_classifies_nothing() {
        let mut c = vec![component("pkg:generic/odd@1.0.0", Some("some-new-role"))];
        let s = role_only(&mut c, &HashSet::new());
        assert_eq!(presence(&c, "pkg:generic/odd@1.0.0"), None);
        assert_eq!(s.assigned(), 0);
    }
}
