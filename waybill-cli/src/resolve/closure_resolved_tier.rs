//! Spec 1650 (#1681) — under `--nix-closure`, a component the closure
//! resolved is not labelled `design`.
//!
//! # Why this pass exists
//!
//! The tier of a Haskell component in a closure scan was decided by which
//! path emitted it first, not by what resolved it
//! (`specs/1650-closure-resolved-tier/research.md` R1, `measurements/census.md`):
//!
//! - **The project's own package** is the reader's `design` main module. The
//!   closure scope pass marks it as the evaluated package (C203
//!   `exempt-evaluated-package`) and leaves its tier alone.
//! - **A boot library of the closure's compiler** keeps `design` when the
//!   project's reader declared it, or when milestone 985's transitive walk
//!   reached it first. Milestone 966 versions it from the compiler without
//!   touching the tier, and the closure then finds its PURL already held, so
//!   it does not synthesise the `build` copy it emits for every other boot
//!   library.
//!
//! These few components are the graph's hubs. With `--root-name` the
//! evaluated package is the root's only direct dependency, so a consumer that
//! drops `design` lost every component in four of the five corpus closure
//! documents.
//!
//! # The rule
//!
//! A `design` component becomes `build` when the closure is evidence that it
//! is in the build graph:
//!
//! - **C203 `exempt-evaluated-package`** (FR-002): the closure evaluated it.
//!   C214 (`closure-contents`) does not enter into it. C214 records only
//!   whether a member's outputs were examined, and every closure member is
//!   `build` whatever its value.
//! - **C218 `boot-library-compiler`** (FR-003): its version came from a
//!   compiler the closure holds, inside which it ships. This is the tier the
//!   same library already has when the closure synthesises it.
//!
//! Both annotations are written only by the closure tier, so a scan without
//! `--nix-closure` is unchanged by construction (FR-007). The evidence is
//! also the recorded reason (FR-005): no new row restates it.
//!
//! This supersedes milestone 966's clarification of 2026-11-12, which kept a
//! compiler-versioned boot library `design` because "the tier says where the
//! dependency was declared". Under `--nix-closure` that rule gave one library
//! two tiers in one document. Where a dependency was declared is still
//! recorded, by its evidence.

use waybill_common::resolution::{ArtifactPresence, ResolvedComponent};

use crate::scan_fs::package_db::nix::haskell_packages::ANN_BOOT_LIBRARY_COMPILER;

/// What [`retier`] changed, for the INFO line.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RetierSummary {
    /// FR-002: evaluated packages moved from `design` to `build`.
    pub(crate) evaluated_package: usize,
    /// FR-003: compiler-supplied boot libraries moved from `design` to `build`.
    pub(crate) compiler_supplied: usize,
}

/// Re-tier every `design` component the closure resolved (FR-002, FR-003).
///
/// Runs after `closure_role_scope::assign_role_scope`, which records the
/// evaluated package, and above the second `--tier` filter, so that filter
/// sees the final tiers (#1360).
pub(crate) fn retier(components: &mut [ResolvedComponent]) -> RetierSummary {
    let mut summary = RetierSummary::default();
    for c in components.iter_mut() {
        if c.sbom_tier.as_deref() != Some("design") {
            continue;
        }
        if c.artifact_presence == Some(ArtifactPresence::ExemptEvaluatedPackage) {
            c.sbom_tier = Some("build".to_string());
            summary.evaluated_package += 1;
        } else if c.extra_annotations.contains_key(ANN_BOOT_LIBRARY_COMPILER) {
            c.sbom_tier = Some("build".to_string());
            summary.compiler_supplied += 1;
        }
    }
    summary
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::generate::graph_completeness::test_support::mk_component;

    fn comp(purl: &str, tier: &str) -> ResolvedComponent {
        let mut c = mk_component(purl);
        c.sbom_tier = Some(tier.to_string());
        c
    }

    fn evaluated(purl: &str, tier: &str, contents: &str) -> ResolvedComponent {
        let mut c = comp(purl, tier);
        c.artifact_presence = Some(ArtifactPresence::ExemptEvaluatedPackage);
        c.extra_annotations
            .insert("waybill:closure-contents".to_string(), serde_json::json!(contents));
        c
    }

    fn boot(purl: &str, tier: &str, compiler: &str) -> ResolvedComponent {
        let mut c = comp(purl, tier);
        c.extra_annotations
            .insert(ANN_BOOT_LIBRARY_COMPILER.to_string(), serde_json::json!(compiler));
        c
    }

    fn tiers(c: &[ResolvedComponent]) -> Vec<&str> {
        c.iter().map(|c| c.sbom_tier.as_deref().unwrap_or("none")).collect()
    }

    /// FR-002, whatever C214 says: moat's evaluated package is `absent`.
    #[test]
    fn s1650_evaluated_package_becomes_build_whatever_c214_says() {
        let mut c = vec![
            evaluated("pkg:hackage/waybill-fixture-a@1.0", "design", "read"),
            evaluated("pkg:hackage/waybill-fixture-b@1.0", "design", "absent"),
        ];
        let s = retier(&mut c);
        assert_eq!(tiers(&c), vec!["build", "build"]);
        assert_eq!(s, RetierSummary { evaluated_package: 2, compiler_supplied: 0 });
    }

    /// FR-003, for every compiler the closure holds.
    #[test]
    fn s1650_compiler_supplied_boot_library_becomes_build() {
        let mut c = vec![
            boot("pkg:hackage/base@4.20.2.0", "design", "ghc-9.10.3"),
            boot("pkg:hackage/base@4.19.2.0", "design", "ghc-binary-9.8.4"),
        ];
        let s = retier(&mut c);
        assert_eq!(tiers(&c), vec!["build", "build"]);
        assert_eq!(s, RetierSummary { evaluated_package: 0, compiler_supplied: 2 });
    }

    /// FR-004: no evidence, no change.
    #[test]
    fn s1650_design_without_closure_evidence_is_untouched() {
        let mut c = vec![
            comp("pkg:hackage/Win32", "design"),
            comp("pkg:hackage/waybill-fixture-extra@0.2.0", "design"),
        ];
        assert_eq!(retier(&mut c), RetierSummary::default());
        assert_eq!(tiers(&c), vec!["design", "design"]);
    }

    /// Only `design` moves: a `source` resolution keeps its tier, even with
    /// the same evidence.
    #[test]
    fn s1650_other_tiers_are_untouched() {
        let mut c = vec![
            evaluated("pkg:hackage/waybill-fixture-a@1.0", "source", "read"),
            boot("pkg:hackage/stm@2.5.3.1", "build", "ghc-9.10.3"),
            boot("pkg:hackage/text@2.1.3", "source", "ghc-9.10.3"),
        ];
        assert_eq!(retier(&mut c), RetierSummary::default());
        assert_eq!(tiers(&c), vec!["source", "build", "source"]);
    }
}
