//! #1046 — one component for a Haskell dependency the manifest declares and
//! the Nix closure built.
//!
//! When an overlay pins a version the nixpkgs package set does not carry, the
//! manifest reader emits a versionless design-tier `pkg:hackage/<name>`
//! (the set's version violates the declared range, so none is emitted) and
//! the closure tier emits the built member as a second component. Measured on
//! the corpus: slacklinker's `slack-web` (`>= 2.3.0.0`; set 2.2.0.0; built
//! 2.3.0.0) and slack-web's `hspec-discover` (`>=2.6.0 && <2.11`; built
//! 2.11.14). Both describe one dependency twice.
//!
//! # Where this runs, and why there
//!
//! After `closure_identity::reclassify`, so the build-tier identity is final
//! (including a Hackage-published one, #1046 US2), and before
//! `emit::qualify_backport_identities` and `graph::relationships`, so the
//! closure's edges are built against the survivor. Not inside
//! `merge_closure_components`: that runs before the identity pass, and
//! milestone 1035 measured that reordering it downgrades PURLs.
//!
//! # What makes a candidate a Haskell package
//!
//! Its PURL is already `pkg:hackage`, or one of its derivations was built by a
//! GHC — the m1275 signal that rejected 26 of 26 measured name collisions. A
//! name found in the pinned set is **not** evidence: GNU `bash 5.3p9` shares a
//! name with a Hackage package (spec FR-003).

use std::collections::{BTreeMap, HashSet};

use waybill_common::resolution::{Relationship, ResolvedComponent};

use super::closure_identity::built_by_ghc;
use super::version_range::{admits, contradicts, Admits};
use crate::scan_fs::package_db::nix::closure::derivation::{RawClosure, RawDerivation};
use crate::scan_fs::package_db::nix::closure::emit::ANN_CLOSURE_ROLE;
use crate::scan_fs::package_db::nix::closure::member::ANN_MEMBER_ROLE;

/// C221: the merged component's build is outside every declared range.
pub(crate) const ANN_REQUIREMENT_UNSATISFIED: &str = "waybill:requirement-unsatisfied";

/// Design-tier annotations that become false once the dependency is resolved,
/// so they do not move onto the survivor.
const DROPPED_ON_MERGE: &[&str] = &[
    super::ANN_UNRESOLVED_REASON,
    "waybill:unresolved-reason",
];

/// Why a design-tier twin was not merged. Counted, and logged by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Declined {
    /// No closure member of that name is a Haskell package.
    NoHaskellCandidate,
    /// Several build variants, and the declared ranges admit none of them.
    NoVariantInRange,
    /// Several build variants, and the declared ranges admit more than one.
    SeveralVariantsInRange,
}

impl Declined {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::NoHaskellCandidate => "no-haskell-candidate",
            Self::NoVariantInRange => "no-variant-in-range",
            Self::SeveralVariantsInRange => "several-variants-in-range",
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct TwinMergeSummary {
    pub(crate) merged: usize,
    /// Of those, the ones whose build every declared range rejects (C221).
    pub(crate) out_of_range: usize,
    pub(crate) declined: BTreeMap<&'static str, usize>,
}

/// A versionless design-tier Hackage component: the twin's manifest half.
fn is_design_twin(c: &ResolvedComponent) -> bool {
    c.version.is_empty()
        && c.sbom_tier.as_deref() == Some("design")
        && c.purl.as_str().starts_with("pkg:hackage/")
}

/// A closure member that declares a `pname`: the twin's build half.
fn is_build_member(c: &ResolvedComponent) -> bool {
    c.extra_annotations.contains_key(ANN_CLOSURE_ROLE)
        && !c.extra_annotations.contains_key(ANN_MEMBER_ROLE)
}

/// Pick the survivor among the Haskell candidates (spec FR-004).
///
/// One candidate is the survivor whatever its version (US4). Several are
/// build variants: the one the ranges admit, if exactly one is.
fn select(candidates: &[usize], components: &[ResolvedComponent], ranges: &[String]) -> Result<usize, Declined> {
    match candidates {
        [] => Err(Declined::NoHaskellCandidate),
        [only] => Ok(*only),
        several => {
            let admitted: Vec<usize> = several
                .iter()
                .copied()
                .filter(|&i| {
                    ranges.is_empty()
                        || ranges.iter().any(|r| admits(r, &components[i].version) == Admits::Yes)
                })
                .collect();
            match admitted.as_slice() {
                [one] => Ok(*one),
                [] => Err(Declined::NoVariantInRange),
                _ => Err(Declined::SeveralVariantsInRange),
            }
        }
    }
}

/// Fold `design` into `survivor` (research R6).
fn fold(survivor: &mut ResolvedComponent, design: &ResolvedComponent) {
    for r in &design.requirement_ranges {
        if !survivor.requirement_ranges.contains(r) {
            survivor.requirement_ranges.push(r.clone());
        }
    }
    for p in &design.evidence.source_file_paths {
        if !survivor.evidence.source_file_paths.contains(p) {
            survivor.evidence.source_file_paths.push(p.clone());
        }
    }
    if survivor.occurrences.is_empty() {
        survivor.occurrences = design.occurrences.clone();
    }
    if survivor.licenses.is_empty() {
        survivor.licenses = design.licenses.clone();
    }
    for (k, v) in &design.extra_annotations {
        if DROPPED_ON_MERGE.contains(&k.as_str()) {
            continue;
        }
        survivor.extra_annotations.entry(k.clone()).or_insert_with(|| v.clone());
    }
    // C221 — only when every declared range parses and rejects the version.
    // `contradicts` returns false for an empty set, and for any range it
    // cannot parse, so an unparseable range claims nothing.
    if contradicts(&survivor.requirement_ranges, &survivor.version) {
        let mut m = serde_json::Map::new();
        m.insert(
            "requirement-ranges".to_string(),
            serde_json::Value::from(survivor.requirement_ranges.clone()),
        );
        m.insert("version".to_string(), serde_json::Value::from(survivor.version.clone()));
        survivor.extra_annotations.insert(
            ANN_REQUIREMENT_UNSATISFIED.to_string(),
            serde_json::Value::String(serde_json::Value::Object(m).to_string()),
        );
    }
}

/// Merge every design-tier twin into its build-tier member, and move the
/// edges that named it.
pub(crate) fn merge(
    components: &mut Vec<ResolvedComponent>,
    relationships: &mut Vec<Relationship>,
    raw: &RawClosure,
) -> TwinMergeSummary {
    let mut drvs_of: BTreeMap<(&str, &str), Vec<&RawDerivation>> = BTreeMap::new();
    for drv in raw.derivations.values() {
        if let Some(pname) = drv.pname() {
            drvs_of
                .entry((pname, drv.version().unwrap_or_default()))
                .or_default()
                .push(drv);
        }
    }
    let is_haskell = |c: &ResolvedComponent| {
        c.purl.as_str().starts_with("pkg:hackage/")
            || drvs_of
                .get(&(c.name.as_str(), c.version.as_str()))
                .is_some_and(|ds| ds.iter().copied().any(built_by_ghc))
    };

    let mut summary = TwinMergeSummary::default();
    let mut renames: Vec<(String, String)> = Vec::new();
    let mut remove: HashSet<usize> = HashSet::new();
    let designs: Vec<usize> = (0..components.len()).filter(|&i| is_design_twin(&components[i])).collect();
    for d in designs {
        let name = components[d].name.clone();
        let candidates: Vec<usize> = (0..components.len())
            .filter(|&i| i != d && !remove.contains(&i))
            .filter(|&i| components[i].name == name && is_build_member(&components[i]))
            .filter(|&i| is_haskell(&components[i]))
            .collect();
        let survivor = match select(&candidates, components, &components[d].requirement_ranges) {
            Ok(s) => s,
            Err(why) => {
                // Only a design twin with *some* namesake in the closure is a
                // decline worth counting; one with none is simply undeclared
                // in the build, which is not this pass's concern.
                let has_namesake = components
                    .iter()
                    .enumerate()
                    .any(|(i, c)| i != d && c.name == name && is_build_member(c));
                if has_namesake || why != Declined::NoHaskellCandidate {
                    *summary.declined.entry(why.wire()).or_default() += 1;
                }
                continue;
            }
        };
        let design = components[d].clone();
        fold(&mut components[survivor], &design);
        if components[survivor].extra_annotations.contains_key(ANN_REQUIREMENT_UNSATISFIED) {
            summary.out_of_range += 1;
        }
        renames.push((design.purl.as_str().to_string(), components[survivor].purl.as_str().to_string()));
        remove.insert(d);
        summary.merged += 1;
    }

    if remove.is_empty() {
        return summary;
    }
    let mut i = 0usize;
    components.retain(|_| {
        let keep = !remove.contains(&i);
        i += 1;
        keep
    });
    super::apply_renames(&renames, relationships);
    // A design edge and a closure edge can now name the same pair.
    let mut seen: HashSet<(String, String, String)> = HashSet::new();
    relationships.retain(|r| {
        seen.insert((r.from.clone(), r.to.clone(), format!("{:?}", r.relationship_type)))
    });
    summary
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::resolution::{EnrichmentProvenance, RelationshipType};

    const GHC: &str = "/nix/store/zzz-ghc-99.0.1";

    fn design(name: &str, ranges: &[&str]) -> ResolvedComponent {
        let mut c = super::super::enrich_tests::comp(name);
        c.sbom_tier = Some("design".to_string());
        c.requirement_ranges = ranges.iter().map(|r| r.to_string()).collect();
        c.evidence.source_file_paths = vec!["waybill-fixture-proj.cabal".to_string()];
        c.extra_annotations.insert(
            super::super::ANN_UNRESOLVED_REASON.to_string(),
            serde_json::json!("nixpkgs-version-violates-requirement"),
        );
        c.extra_annotations
            .insert("waybill:unresolved-reason".to_string(), serde_json::json!("declared"));
        c.extra_annotations.insert(
            "waybill:nixpkgs-version-disagreement".to_string(),
            serde_json::json!("{\"nixpkgs\":\"1.0.0.0\"}"),
        );
        c
    }

    fn build(name: &str, version: &str, purl_type: &str) -> ResolvedComponent {
        let mut c = super::super::enrich_tests::comp(name);
        c.version = version.to_string();
        c.purl = waybill_common::types::purl::Purl::new(&format!("pkg:{purl_type}/{name}@{version}")).unwrap();
        c.sbom_tier = Some("build".to_string());
        c.extra_annotations
            .insert(ANN_CLOSURE_ROLE.to_string(), serde_json::json!("artifact-input"));
        c
    }

    /// A closure holding one derivation per `(name, version, ghc_built)`.
    fn closure(members: &[(&str, &str, bool)]) -> RawClosure {
        let drvs: serde_json::Map<String, serde_json::Value> = members
            .iter()
            .map(|(n, v, ghc)| {
                let mut env = serde_json::json!({"pname": n, "version": v});
                if *ghc {
                    env["nativeBuildInputs"] = serde_json::json!(GHC);
                }
                (format!("d-{n}-{v}.drv"), serde_json::json!({"env": env, "outputs": {"out": {"path": format!("h-{n}-{v}")}}}))
            })
            .collect();
        RawClosure::parse(&serde_json::json!({"derivations": drvs}).to_string()).unwrap()
    }

    fn edge(from: &str, to: &str) -> Relationship {
        Relationship {
            from: from.to_string(),
            to: to.to_string(),
            relationship_type: RelationshipType::DependsOn,
            provenance: EnrichmentProvenance {
                source: "test".to_string(),
                data_type: "dependency".to_string(),
            },
        }
    }

    fn named<'a>(cs: &'a [ResolvedComponent], name: &str) -> Vec<&'a ResolvedComponent> {
        cs.iter().filter(|c| c.name == name).collect()
    }

    // --- T007: selection --------------------------------------------------

    #[test]
    fn a_single_haskell_candidate_merges() {
        let mut cs = vec![design("waybill-fixture-a", &[">= 2.0"]), build("waybill-fixture-a", "2.1", "generic")];
        let s = merge(&mut cs, &mut Vec::new(), &closure(&[("waybill-fixture-a", "2.1", true)]));
        assert_eq!(s.merged, 1);
        assert_eq!(named(&cs, "waybill-fixture-a").len(), 1);
    }

    /// FR-003: a namesake no GHC built, and not `pkg:hackage`, never merges.
    #[test]
    fn a_namesake_that_is_not_a_haskell_package_does_not_merge() {
        let mut cs = vec![design("waybill-fixture-n", &[]), build("waybill-fixture-n", "1.0", "generic")];
        let s = merge(&mut cs, &mut Vec::new(), &closure(&[("waybill-fixture-n", "1.0", false)]));
        assert_eq!(s.merged, 0);
        assert_eq!(named(&cs, "waybill-fixture-n").len(), 2, "both stay");
        assert_eq!(s.declined.get("no-haskell-candidate"), Some(&1));
    }

    /// A `pkg:hackage` build member is a Haskell package without consulting
    /// the closure (the `confirmed` shape).
    #[test]
    fn a_hackage_build_member_merges_without_the_ghc_signal() {
        let mut cs = vec![design("waybill-fixture-h", &[]), build("waybill-fixture-h", "1.6", "hackage")];
        let s = merge(&mut cs, &mut Vec::new(), &closure(&[]));
        assert_eq!(s.merged, 1);
    }

    #[test]
    fn two_variants_with_exactly_one_in_range_merge_onto_it() {
        let mut cs = vec![
            design("waybill-fixture-v", &[">= 3.0"]),
            build("waybill-fixture-v", "2.9", "generic"),
            build("waybill-fixture-v", "3.1", "generic"),
        ];
        let raw = closure(&[("waybill-fixture-v", "2.9", true), ("waybill-fixture-v", "3.1", true)]);
        let s = merge(&mut cs, &mut Vec::new(), &raw);
        assert_eq!(s.merged, 1);
        let v = named(&cs, "waybill-fixture-v");
        assert_eq!(v.len(), 2);
        let survivor = v.iter().find(|c| c.version == "3.1").unwrap();
        assert_eq!(survivor.requirement_ranges, vec![">= 3.0"]);
    }

    #[test]
    fn two_variants_both_or_neither_in_range_do_not_merge() {
        for (ranges, why) in [(vec![">= 3.0"], "several-variants-in-range"), (vec![">= 9.0"], "no-variant-in-range")] {
            let mut cs = vec![
                design("waybill-fixture-v", &ranges),
                build("waybill-fixture-v", "3.1", "generic"),
                build("waybill-fixture-v", "3.2", "generic"),
            ];
            let raw = closure(&[("waybill-fixture-v", "3.1", true), ("waybill-fixture-v", "3.2", true)]);
            let s = merge(&mut cs, &mut Vec::new(), &raw);
            assert_eq!(s.merged, 0, "{ranges:?}");
            assert_eq!(named(&cs, "waybill-fixture-v").len(), 3);
            assert_eq!(s.declined.get(why), Some(&1), "{ranges:?}");
        }
    }

    /// A design-tier component that already has a version is not a twin.
    #[test]
    fn a_versioned_design_component_is_not_a_twin() {
        let mut d = design("waybill-fixture-r", &[]);
        d.version = "1.0".to_string();
        d.purl = waybill_common::types::purl::Purl::new("pkg:hackage/waybill-fixture-r@1.0").unwrap();
        let mut cs = vec![d, build("waybill-fixture-r", "1.0", "generic")];
        let s = merge(&mut cs, &mut Vec::new(), &closure(&[("waybill-fixture-r", "1.0", true)]));
        assert_eq!(s, TwinMergeSummary::default());
        assert_eq!(cs.len(), 2);
    }

    // --- T008: the merge's effect -----------------------------------------

    #[test]
    fn the_survivor_keeps_its_identity_and_gains_the_declaration() {
        let mut cs = vec![design("waybill-fixture-a", &[">= 2.0"]), build("waybill-fixture-a", "2.1", "generic")];
        merge(&mut cs, &mut Vec::new(), &closure(&[("waybill-fixture-a", "2.1", true)]));
        let c = named(&cs, "waybill-fixture-a")[0];
        assert_eq!(c.purl.as_str(), "pkg:generic/waybill-fixture-a@2.1");
        assert_eq!(c.version, "2.1");
        assert_eq!(c.sbom_tier.as_deref(), Some("build"));
        assert_eq!(c.requirement_ranges, vec![">= 2.0"]);
        assert_eq!(c.evidence.source_file_paths, vec!["waybill-fixture-proj.cabal"]);
        assert!(!c.extra_annotations.contains_key(super::super::ANN_UNRESOLVED_REASON));
        assert!(!c.extra_annotations.contains_key("waybill:unresolved-reason"));
        assert!(c.extra_annotations.contains_key("waybill:nixpkgs-version-disagreement"), "C172 stays true");
        assert!(c.extra_annotations.contains_key(ANN_CLOSURE_ROLE));
    }

    #[test]
    fn edges_follow_the_survivor_and_duplicates_collapse() {
        let mut cs = vec![
            build("waybill-fixture-root", "0.1", "generic"),
            design("waybill-fixture-a", &[]),
            build("waybill-fixture-a", "2.1", "generic"),
        ];
        let design_purl = "pkg:hackage/waybill-fixture-a";
        let build_purl = "pkg:generic/waybill-fixture-a@2.1";
        let root = "pkg:generic/waybill-fixture-root@0.1";
        let mut rels = vec![edge(root, design_purl), edge(root, build_purl)];
        merge(&mut cs, &mut rels, &closure(&[("waybill-fixture-a", "2.1", true)]));
        assert_eq!(rels.len(), 1, "{rels:?}");
        assert_eq!((rels[0].from.as_str(), rels[0].to.as_str()), (root, build_purl));
        assert!(!cs.iter().any(|c| c.purl.as_str() == design_purl), "the design twin is gone");
    }

    // --- T023: C221 ---------------------------------------------------------

    fn c221(c: &ResolvedComponent) -> Option<String> {
        c.extra_annotations
            .get(ANN_REQUIREMENT_UNSATISFIED)
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    #[test]
    fn an_out_of_range_build_merges_and_records_c221() {
        let mut cs = vec![design("waybill-fixture-x", &[">=1.0 && <1.5"]), build("waybill-fixture-x", "1.6", "hackage")];
        let s = merge(&mut cs, &mut Vec::new(), &closure(&[]));
        assert_eq!((s.merged, s.out_of_range), (1, 1));
        assert_eq!(
            c221(named(&cs, "waybill-fixture-x")[0]).as_deref(),
            Some(r#"{"requirement-ranges":[">=1.0 && <1.5"],"version":"1.6"}"#)
        );
    }

    #[test]
    fn c221_is_not_claimed_without_evidence() {
        for (ranges, why) in [
            (vec![">=1.0 && <2"], "in range"),
            (vec!["this is not a range"], "unparseable"),
            (vec![">= 9", ">= 1.0"], "one of several admits"),
        ] {
            let mut cs = vec![design("waybill-fixture-x", &ranges), build("waybill-fixture-x", "1.6", "hackage")];
            let s = merge(&mut cs, &mut Vec::new(), &closure(&[]));
            assert_eq!(s.merged, 1, "{why}");
            assert_eq!(c221(named(&cs, "waybill-fixture-x")[0]), None, "{why}");
        }
    }
}
