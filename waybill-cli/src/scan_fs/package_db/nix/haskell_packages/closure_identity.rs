//! Milestone 1275 (#1282) — giving a closure-appended member the identity the
//! pinned package set already holds for it.
//!
//! `--nix-closure` appends every emitting derivation in a Nix build closure as
//! a component. For Haskell packages it named them `pkg:generic/<pname>@<v>`
//! and gave them nothing else: measured on one flake, 276 such components with
//! no license, no hash and no edges, beside 65 `pkg:hackage` components that
//! the same scan, in the same process, from the same cached file, had already
//! resolved with 47 licenses and 245 out-edges.
//!
//! So this is not a missing capability. It is a pass that already works,
//! applied to one set of components and not the other.
//!
//! # Why a new pass rather than reordering the existing one
//!
//! The root cause is ordering — the Haskell enrichment runs at
//! `scan_cmd.rs:4540` and the closure merge 189 lines later — so the obvious
//! fix is to move the merge earlier. **That was tried in milestone 1035 and
//! measured to regress**: 41 components a flag-off scan emitted as
//! `pkg:hackage/QuickCheck@2.15.0.1` came out of a flag-on scan as
//! `pkg:generic/...`, so enabling the closure made PURLs *worse*. The comment
//! recording it sits at the merge site. And `is_enrichable` accepts only
//! versionless components, while closure members always carry the version nix
//! recorded, so the existing pass would skip them whatever the order.
//!
//! # Why identity needs two signals
//!
//! 26 of those 276 components have a name that exists on Hackage and nothing
//! to do with Haskell: GNU `bash 5.3p9` against Hackage's `bash 0.1.8`,
//! `curl 8.20.0` against `1.3.8`, the compiler `ghc 9.4.8` against a Hackage
//! package called `ghc` at `9.14.1`. A name match alone would mint
//! `pkg:hackage/bash@5.3p9` — a coordinate that belongs to someone else, which
//! a vulnerability feed would then answer for the wrong software. The emission
//! site's own comment warned about this before the feature existed: "a
//! wrong-but-well-formed PURL would be worse than a generic one: it would
//! collide with whatever really owns that coordinate."
//!
//! So an identity requires the exact `(name, version)` pair **and** the
//! derivation having been built by a GHC. Measured, the two agree on 60 of 60
//! exact pairs, and the GHC signal rejects all 26 collisions on its own.

use std::collections::BTreeSet;

use waybill_common::resolution::ResolvedComponent;

use super::package_set::{PackageSet, PackageSetEntry};
use crate::scan_fs::package_db::nix::closure::derivation::RawDerivation;
use crate::scan_fs::package_db::nix::closure::emit::ANN_CLOSURE_ROLE;
use crate::scan_fs::package_db::nix::closure::member::ANN_MEMBER_ROLE;
use crate::scan_fs::package_db::nix::closure::ClassifiedClosure;

/// The C201 annotation: what this pass concluded about one closure member.
pub(crate) const ANN_CLOSURE_IDENTITY: &str = "waybill:closure-identity";

/// Why a closure member was or was not given a Hackage identity.
///
/// An exhaustive enum rather than `Option<Purl>` plus a comment: FR-007
/// requires every unreclassified member to say which of these it was, and a
/// `_ => None` arm is how a later variant silently loses its reason (#1218,
/// #892). Every variant maps to exactly one C201 wire value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HackageIdentity {
    /// FR-001 — the default attribute's version equals the closure's, and the
    /// derivation was built by a GHC. Both signals, per FR-005.
    Confirmed {
        /// The attribute that matched. Carried because the caller needs the
        /// entry's license, hash and declared relations, and re-deriving the
        /// key from the component would be a second lookup that could differ
        /// from the one the decision was made on.
        attribute: String,
    },
    /// FR-004 — the name is in the set under a different version. The
    /// collision population: `bash 5.3p9` against Hackage's `bash 0.1.8`.
    /// Measured 26 of 276.
    VersionDisagrees,
    /// The exact pair matched but the derivation was not built by a GHC, so
    /// the two signals disagree. Measured 0 of 60 — kept because a variant
    /// that cannot occur today is cheaper than discovering it occurred.
    SignalsDisagree,
    /// The pinned set has no attribute of that name at all. 190 of 276.
    NotInPackageSet,
    /// nix recorded no version, so no pair could be formed. 14 of 276.
    /// Distinct from a failed lookup: the lookup was *impossible*, which calls
    /// for different action than a lookup that found nothing.
    NoVersionRecorded,
    /// FR-009 — the set itself was unavailable. Every member lands here, and
    /// the document says the pass did not run.
    PackageSetUnavailable,
}

impl HackageIdentity {
    /// The C201 wire value. A closed set; see `docs/reference/sbom-format-mapping.md`.
    pub(crate) fn wire(&self) -> &'static str {
        match self {
            Self::Confirmed { .. } => "confirmed",
            Self::VersionDisagrees => "version-disagrees",
            Self::SignalsDisagree => "signals-disagree",
            Self::NotInPackageSet => "not-in-package-set",
            Self::NoVersionRecorded => "no-version-recorded",
            Self::PackageSetUnavailable => "package-set-unavailable",
        }
    }

    /// The matched attribute, when there is one. Only `Confirmed` has one.
    pub(crate) fn attribute(&self) -> Option<&str> {
        match self {
            Self::Confirmed { attribute } => Some(attribute.as_str()),
            _ => None,
        }
    }
}

/// Whether a derivation was built by a GHC — the signal FR-005 requires to be
/// independent of the package set.
///
/// Reads `env.nativeBuildInputs` for a store path naming a GHC. No new
/// parsing: `RawDerivation` retains `env` whole, and `paths_in` already splits
/// a whitespace-separated field.
///
/// # The pattern is load-bearing and the obvious one is wrong
///
/// `-ghc-<digit>` reports `happy-lib 2.1.7` as **not** GHC-built, because that
/// package bootstraps with `ghc-binary-9.0.2`. With the naive pattern the two
/// signals agreed on 59 of 60 exact pairs rather than 60 of 60, and that one
/// disagreeing row was the only evidence the pattern — not the system — was at
/// fault. Hence `-ghc(-binary)?-<digit>`, and hence FR-006 requiring the
/// agreement be measured rather than asserted.
pub(crate) fn built_by_ghc(drv: &RawDerivation) -> bool {
    drv.paths_in("nativeBuildInputs").any(is_ghc_store_path)
}

/// Whether one store path names a GHC or a bootstrap GHC.
fn is_ghc_store_path(path: &str) -> bool {
    let base = crate::scan_fs::package_db::nix::closure::derivation::store_basename(path);
    // Strip the hash-dash prefix nix gives every store path, then look for a
    // GHC name immediately followed by a version. Matching the bare string
    // `ghc` would accept `ghc-settings-edit` and every other package whose
    // name merely starts that way.
    for marker in ["-ghc-", "-ghc-binary-"] {
        if let Some(rest) = base.split_once(marker).map(|(_, r)| r) {
            if rest.starts_with(|c: char| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

/// Decide what identity a closure member may claim.
///
/// A pure function of the component's `(name, version)`, the pinned set, and
/// the derivations that produced it. `set` is `None` when the pass could not
/// obtain the package set at all (FR-009).
///
/// `drvs` is every derivation in the closure that emitted this component —
/// plural because a closure holds several derivations per `(pname, version)`
/// (build variants), and the GHC signal holds if **any** of them was built by
/// a GHC. Requiring all of them would make the signal depend on which variants
/// nix happened to include.
pub(crate) fn decide(
    name: &str,
    version: &str,
    set: Option<&PackageSet>,
    drvs: &[&RawDerivation],
) -> HackageIdentity {
    let Some(set) = set else {
        return HackageIdentity::PackageSetUnavailable;
    };
    if version.is_empty() {
        // No pair can be formed, so the lookup is impossible rather than
        // unsuccessful. Checked before consulting the set so the two cannot be
        // confused: a name present in the set with no closure version is still
        // `NoVersionRecorded`, not a version disagreement.
        return HackageIdentity::NoVersionRecorded;
    }
    // R4: the DEFAULT attribute only, via a single `get(name)`. nixpkgs
    // encodes multiple versions as distinct attributes (`hashable`,
    // `hashable_1_4_7_0`), and a reverse `pname -> attributes` index would
    // match exactly one more component on the measured fixture while
    // introducing a new ambiguity when two aliases carry the same version.
    // Measured cost of this choice: 1 of 60 (`hashable 1.4.7.0`).
    let Some(entry) = set.get(name) else {
        return HackageIdentity::NotInPackageSet;
    };
    if entry.version != version {
        // FR-004: matching on name alone is forbidden. This is the arm that
        // keeps `pkg:hackage/bash@5.3p9` out of the document.
        return HackageIdentity::VersionDisagrees;
    }
    if !drvs.iter().copied().any(built_by_ghc) {
        return HackageIdentity::SignalsDisagree;
    }
    HackageIdentity::Confirmed { attribute: name.to_string() }
}

/// The declared runtime dependencies of a confirmed member, as attribute
/// names.
///
/// `runtime_relations` is `libraryHaskellDepends + executableHaskellDepends`,
/// merged and deduplicated by milestone 985's parser — already parsed, never
/// consulted for closure members until now. **Not** store references: FR-012a
/// keeps milestone 1066's decision that a store reference drives a component's
/// *role* and nothing else.
pub(crate) fn declared_relations(entry: &PackageSetEntry) -> BTreeSet<String> {
    entry.runtime_relations.iter().cloned().collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A derivation whose `nativeBuildInputs` holds exactly these paths.
    fn drv_with_native(paths: &[&str]) -> RawDerivation {
        let mut env = BTreeMap::new();
        env.insert("nativeBuildInputs".to_string(), paths.join(" "));
        RawDerivation { env, ..Default::default() }
    }

    fn set_of(pairs: &[(&str, &str)]) -> PackageSet {
        let text = pairs
            .iter()
            .map(|(n, v)| {
                format!(
                    "  {n} = callPackage (\n    ({{ mkDerivation }}:\n     mkDerivation {{\n       pname = \"{n}\";\n       version = \"{v}\";\n       license = lib.licenses.mit;\n     }}) {{}}) {{}};\n"
                )
            })
            .collect::<String>();
        super::super::package_set::parse(&text)
    }

    // ---- the GHC signal (FR-005) ----

    #[test]
    fn a_ghc_store_path_is_the_signal() {
        let d = drv_with_native(&["/nix/store/abc123-ghc-9.4.8"]);
        assert!(built_by_ghc(&d));
    }

    #[test]
    fn a_bootstrap_ghc_also_counts() {
        // research R5: `happy-lib` bootstraps with `ghc-binary-9.0.2`, and the
        // naive `-ghc-\d` pattern reported it as not-GHC-built. That single
        // disagreeing row was the evidence the pattern was wrong.
        let d = drv_with_native(&["/nix/store/abc123-ghc-binary-9.0.2"]);
        assert!(built_by_ghc(&d));
    }

    #[test]
    fn a_package_whose_name_merely_starts_with_ghc_is_not_the_signal() {
        // `ghc-settings-edit` is a real derivation in the measured closure and
        // IS ghc-built, but the point here is the pattern must require a
        // version, or every `ghc-*`-named input would satisfy the signal.
        let d = drv_with_native(&["/nix/store/abc123-ghc-settings-edit"]);
        assert!(!built_by_ghc(&d));
    }

    #[test]
    fn a_non_haskell_toolchain_is_not_the_signal() {
        let d = drv_with_native(&[
            "/nix/store/abc123-gcc-13.2.0",
            "/nix/store/def456-cmake-3.29.0",
        ]);
        assert!(!built_by_ghc(&d));
    }

    #[test]
    fn an_absent_field_is_not_the_signal() {
        let d = RawDerivation::default();
        assert!(!built_by_ghc(&d));
    }

    #[test]
    fn a_ghc_anywhere_in_the_field_counts() {
        let d = drv_with_native(&[
            "/nix/store/aaa-pkg-config-0.29.2",
            "/nix/store/bbb-ghc-9.6.4",
        ]);
        assert!(built_by_ghc(&d));
    }

    // ---- the decision (FR-001, FR-004, FR-005, FR-007, FR-009) ----

    #[test]
    fn an_exact_pair_built_by_ghc_is_confirmed() {
        let set = set_of(&[("waybill-fixture-aeson", "2.2.4.1")]);
        let d = drv_with_native(&["/nix/store/x-ghc-9.4.8"]);
        let id = decide("waybill-fixture-aeson", "2.2.4.1", Some(&set), &[&d]);
        assert_eq!(id, HackageIdentity::Confirmed {
            attribute: "waybill-fixture-aeson".to_string()
        });
        assert_eq!(id.wire(), "confirmed");
        assert_eq!(id.attribute(), Some("waybill-fixture-aeson"));
    }

    #[test]
    fn a_name_match_at_another_version_is_never_confirmed() {
        // FR-004. The real shape: GNU bash 5.3p9 against Hackage's bash 0.1.8.
        let set = set_of(&[("waybill-fixture-shell", "0.1.8")]);
        let d = drv_with_native(&["/nix/store/x-ghc-9.4.8"]);
        let id = decide("waybill-fixture-shell", "5.3p9", Some(&set), &[&d]);
        assert_eq!(id, HackageIdentity::VersionDisagrees);
        assert_eq!(id.attribute(), None, "a rejected identity names no attribute");
    }

    #[test]
    fn an_exact_pair_not_built_by_ghc_is_not_confirmed() {
        // FR-005: two signals, and disagreement does not mint an identity.
        let set = set_of(&[("waybill-fixture-aeson", "2.2.4.1")]);
        let d = drv_with_native(&["/nix/store/x-gcc-13.2.0"]);
        let id = decide("waybill-fixture-aeson", "2.2.4.1", Some(&set), &[&d]);
        assert_eq!(id, HackageIdentity::SignalsDisagree);
    }

    #[test]
    fn one_ghc_built_variant_is_enough() {
        // A closure holds several derivations per (pname, version).
        let set = set_of(&[("waybill-fixture-aeson", "2.2.4.1")]);
        let plain = drv_with_native(&["/nix/store/x-coreutils-9.5"]);
        let ghc = drv_with_native(&["/nix/store/y-ghc-9.4.8"]);
        let id = decide("waybill-fixture-aeson", "2.2.4.1", Some(&set), &[&plain, &ghc]);
        assert!(matches!(id, HackageIdentity::Confirmed { .. }));
    }

    #[test]
    fn a_name_absent_from_the_set_says_so() {
        let set = set_of(&[("waybill-fixture-aeson", "2.2.4.1")]);
        let d = drv_with_native(&["/nix/store/x-ghc-9.4.8"]);
        assert_eq!(
            decide("waybill-fixture-autoconf", "2.72", Some(&set), &[&d]),
            HackageIdentity::NotInPackageSet
        );
    }

    #[test]
    fn no_version_is_distinct_from_a_failed_lookup() {
        // US3 scenario 2 / FR-007. The lookup was impossible, not unsuccessful,
        // and the two call for different operator action.
        let set = set_of(&[("waybill-fixture-aeson", "2.2.4.1")]);
        let d = drv_with_native(&["/nix/store/x-ghc-9.4.8"]);
        assert_eq!(
            decide("waybill-fixture-aeson", "", Some(&set), &[&d]),
            HackageIdentity::NoVersionRecorded
        );
        assert_ne!(
            decide("waybill-fixture-aeson", "", Some(&set), &[&d]),
            HackageIdentity::NotInPackageSet
        );
    }

    #[test]
    fn an_unavailable_set_is_its_own_outcome() {
        // FR-009: every member lands here, and nothing fails.
        let d = drv_with_native(&["/nix/store/x-ghc-9.4.8"]);
        let id = decide("waybill-fixture-aeson", "2.2.4.1", None, &[&d]);
        assert_eq!(id, HackageIdentity::PackageSetUnavailable);
        assert_eq!(id.wire(), "package-set-unavailable");
    }

    #[test]
    fn every_variant_has_a_distinct_wire_value() {
        // FR-007 is only satisfied if the five non-confirmed outcomes are
        // distinguishable on the wire. An enum with two variants sharing a
        // string would satisfy "carries a reason" and fail the requirement.
        let all = [
            HackageIdentity::Confirmed { attribute: "x".into() },
            HackageIdentity::VersionDisagrees,
            HackageIdentity::SignalsDisagree,
            HackageIdentity::NotInPackageSet,
            HackageIdentity::NoVersionRecorded,
            HackageIdentity::PackageSetUnavailable,
        ];
        let wires: BTreeSet<&str> = all.iter().map(|i| i.wire()).collect();
        assert_eq!(wires.len(), all.len(), "two variants share a wire value");
    }

    // ---- declared relations (FR-012) ----

    #[test]
    fn declared_relations_come_from_the_package_set() {
        let text = concat!(
            "  waybill-fixture-aeson = callPackage (\n",
            "    ({ mkDerivation }:\n",
            "     mkDerivation {\n",
            "       pname = \"waybill-fixture-aeson\";\n",
            "       version = \"2.2.4.1\";\n",
            "       libraryHaskellDepends = [ waybill-fixture-text waybill-fixture-base ];\n",
            "       license = lib.licenses.mit;\n",
            "     }) {}) {};\n",
        );
        let set = super::super::package_set::parse(text);
        let entry = set.get("waybill-fixture-aeson").unwrap();
        let rels = declared_relations(entry);
        assert!(rels.contains("waybill-fixture-text"), "got {rels:?}");
        assert!(rels.contains("waybill-fixture-base"), "got {rels:?}");
    }

    /// #1459: a declared target names a Haskell package, so when a C library
    /// shares the name, the edge binds to the `pkg:hackage` component in
    /// either document order. Measured on nixpkgs `a799d3e3`: `tls -> zlib`
    /// bound to `pkg:generic/zlib@1.3.2`, which `tls` does not depend on.
    #[test]
    fn a_declared_target_binds_to_the_haskell_namesake() {
        let text = concat!(
            "  waybill-fixture-tls = callPackage ({ mkDerivation }: mkDerivation {\n",
            "      pname = \"waybill-fixture-tls\";\n",
            "      version = \"2.1.8\";\n",
            "      libraryHaskellDepends = [ waybill-fixture-zlib ];\n",
            "      license = lib.licenses.bsd3;\n",
            "  }) { };\n",
        );
        let set = super::super::package_set::parse(text);
        let member = |purl: &str, name: &str, version: &str, confirmed: bool| {
            let mut c = super::super::enrich_tests::comp(name);
            c.purl = waybill_common::types::purl::Purl::new(purl).unwrap();
            c.version = version.to_string();
            if confirmed {
                c.extra_annotations
                    .insert(ANN_CLOSURE_IDENTITY.to_string(), serde_json::Value::from("confirmed"));
            }
            c
        };
        let tls = member("pkg:hackage/waybill-fixture-tls@2.1.8", "waybill-fixture-tls", "2.1.8", true);
        let hs = member("pkg:hackage/waybill-fixture-zlib@0.7.1.1", "waybill-fixture-zlib", "0.7.1.1", true);
        let c = member("pkg:generic/waybill-fixture-zlib@1.3.2", "waybill-fixture-zlib", "1.3.2", false);
        let builds = |pairs: &[(&str, &str)]| -> std::collections::HashSet<(String, String)> {
            pairs.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect()
        };
        let target_of = |order: Vec<ResolvedComponent>, hs_builds: &std::collections::HashSet<(String, String)>| {
            let mut components = order;
            let mut relationships = Vec::new();
            let mut summary = ClosureIdentitySummary::default();
            emit_declared_edges(
                &mut components,
                &mut relationships,
                Some(&set),
                &BTreeSet::new(),
                hs_builds,
                &mut summary,
            );
            relationships
                .iter()
                .filter(|r| r.from == "pkg:hackage/waybill-fixture-tls@2.1.8")
                .map(|r| r.to.clone())
                .collect::<Vec<_>>()
        };

        let hs_zlib = builds(&[("waybill-fixture-tls", "2.1.8"), ("waybill-fixture-zlib", "0.7.1.1")]);
        for order in [vec![tls.clone(), hs.clone(), c.clone()], vec![tls.clone(), c.clone(), hs.clone()]] {
            assert_eq!(target_of(order, &hs_zlib), vec!["pkg:hackage/waybill-fixture-zlib@0.7.1.1"]);
        }

        // No Haskell build of the name in the closure: a `pkg:hackage`
        // component (a manifest-declared one) still beats the C library.
        let none = builds(&[("waybill-fixture-tls", "2.1.8")]);
        let declared = member("pkg:hackage/waybill-fixture-zlib", "waybill-fixture-zlib", "", false);
        for order in [vec![tls.clone(), declared.clone(), c.clone()], vec![tls.clone(), c.clone(), declared.clone()]] {
            assert_eq!(target_of(order, &none), vec!["pkg:hackage/waybill-fixture-zlib"]);
        }

        // The `os-string` shape: a Haskell build the package set does not
        // hold stays `pkg:generic`, beside a versionless manifest-declared
        // `pkg:hackage` component. The build is what is linked, so it wins.
        let generic_hs = member("pkg:generic/waybill-fixture-zlib@0.7.2", "waybill-fixture-zlib", "0.7.2", false);
        let hs_generic = builds(&[("waybill-fixture-tls", "2.1.8"), ("waybill-fixture-zlib", "0.7.2")]);
        for order in [
            vec![tls.clone(), declared.clone(), generic_hs.clone()],
            vec![tls.clone(), generic_hs.clone(), declared.clone()],
        ] {
            assert_eq!(target_of(order, &hs_generic), vec!["pkg:generic/waybill-fixture-zlib@0.7.2"]);
        }
    }

    /// #1466: a member that declares no `pname` (C206) is never a declared
    /// target, whatever it is called. On nixpkgs `a799d3e3` the live-bootstrap
    /// fragment `time` took 24 edges meant for the GHC boot library. Without
    /// it, a boot target gets FR-012c's compiler-supplied component and any
    /// other target is omitted and counted.
    #[test]
    fn a_declared_target_never_binds_to_a_nameless_member() {
        let text = concat!(
            "  waybill-fixture-lib = callPackage ({ mkDerivation }: mkDerivation {\n",
            "      pname = \"waybill-fixture-lib\";\n",
            "      version = \"1.0.0\";\n",
            "      libraryHaskellDepends = [ waybill-fixture-clock ];\n",
            "      license = lib.licenses.bsd3;\n",
            "  }) { };\n",
        );
        let set = super::super::package_set::parse(text);
        let mut lib = super::super::enrich_tests::comp("waybill-fixture-lib");
        lib.purl = waybill_common::types::purl::Purl::new("pkg:hackage/waybill-fixture-lib@1.0.0").unwrap();
        lib.version = "1.0.0".to_string();
        lib.extra_annotations
            .insert(ANN_CLOSURE_IDENTITY.to_string(), serde_json::Value::from("confirmed"));
        let mut fragment = super::super::enrich_tests::comp("waybill-fixture-clock");
        fragment.purl = waybill_common::types::purl::Purl::new("pkg:generic/waybill-fixture-clock-c10ck").unwrap();
        fragment.version = String::new();
        fragment
            .extra_annotations
            .insert(ANN_MEMBER_ROLE.to_string(), serde_json::Value::from("fragment"));

        let run = |boot: &BTreeSet<String>| {
            let mut components = vec![lib.clone(), fragment.clone()];
            let mut relationships = Vec::new();
            let mut summary = ClosureIdentitySummary::default();
            emit_declared_edges(
                &mut components,
                &mut relationships,
                Some(&set),
                boot,
                &Default::default(),
                &mut summary,
            );
            let targets: Vec<String> = relationships.iter().map(|r| r.to.clone()).collect();
            (targets, summary)
        };

        let boot: BTreeSet<String> = ["waybill-fixture-clock".to_string()].into();
        let (targets, summary) = run(&boot);
        assert_eq!(targets, vec!["pkg:hackage/waybill-fixture-clock"]);
        assert_eq!(summary.boot_targets, 1);

        let (targets, summary) = run(&BTreeSet::new());
        assert!(targets.is_empty(), "bound to {targets:?}");
        assert_eq!(summary.dangling_non_boot, 1, "the omission is counted");
    }
}

/// Milestone 1275 T009 — FR-006's agreement, measured against **this** code.
///
/// FR-006 requires the two signals' agreement be measured rather than
/// assumed. `measurements/size-closure-members.py` reports it too, but from a
/// Python re-implementation of the rules: it can agree 60 of 60 while the
/// shipped Rust disagrees, which would make the recorded finding a statement
/// about the probe instead of about the pass. So this probe runs `decide` and
/// `built_by_ghc` themselves over a real closure and a real package set.
///
/// `#[ignore]` and env-gated: it reads a 6 MB closure and a 16 MB package set,
/// which no ordinary test may do (Principle VII).
///
/// ```sh
/// WAYBILL_T009_CLOSURE=/path/to/closure.json \
/// WAYBILL_T009_PACKAGE_SET=~/.cache/waybill/nixpkgs/<rev>/hackage-packages.nix \
///   cargo test -p waybill --bin waybill m1275_t009 -- --ignored --nocapture
/// ```
#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod t009_agreement_probe {
    use super::*;
    use crate::scan_fs::package_db::nix::closure::derivation::RawClosure;

    #[test]
    #[ignore = "reads a real closure and package set; env-gated probe"]
    fn m1275_t009_signal_agreement() {
        let (Ok(cpath), Ok(spath)) = (
            std::env::var("WAYBILL_T009_CLOSURE"),
            std::env::var("WAYBILL_T009_PACKAGE_SET"),
        ) else {
            eprintln!("WAYBILL_T009_{{CLOSURE,PACKAGE_SET}} unset; nothing measured");
            return;
        };
        let closure = RawClosure::parse(&std::fs::read_to_string(&cpath).unwrap()).unwrap();
        let set = super::super::package_set::parse(&std::fs::read_to_string(&spath).unwrap());

        // Measure over the components the tier ACTUALLY emits, not over every
        // derivation in the closure. The first draft of this probe grouped all
        // 1,290 derivations and reported 763 identities and 106 confirmed,
        // against the Python probe's 276 and 60 -- because `emit::components`
        // drops members by role (only `artifact-input`, `build-tooling` and
        // `both` emit, plus `unreferenced` patch-appliers) and this skipped
        // that filter while its comment claimed otherwise. Classification is
        // pure, so there is no excuse for approximating it.
        let roles = crate::scan_fs::package_db::nix::closure::classify::classify(&closure);
        let classified = crate::scan_fs::package_db::nix::closure::ClassifiedClosure {
            runtime: None,
            attribute: "probe".to_string(),
            raw: closure,
            roles,
            graph_admitted: Default::default(),
            graph_admission: Default::default(),
            read_outcome: Default::default(),
            graph: Default::default(),
            licenses: Default::default(),
            degraded_licenses: None,
            license_coverage: Default::default(),
            identifiers: Default::default(),
        };
        let emitted = crate::scan_fs::package_db::nix::closure::emit::components(&classified);

        // Re-associate each emitted component with the derivations behind it,
        // which is what the GHC signal reads.
        let mut drvs_of: std::collections::BTreeMap<(String, String), Vec<&RawDerivation>> =
            Default::default();
        for drv in classified.raw.derivations.values() {
            let Some(pname) = drv.pname() else { continue };
            let version = drv.version().unwrap_or_default().to_string();
            drvs_of.entry((pname.to_string(), version)).or_default().push(drv);
        }
        let by_identity: std::collections::BTreeMap<(String, String), Vec<&RawDerivation>> =
            emitted
                .iter()
                .map(|c| {
                    let key = (c.name.clone(), c.version.clone());
                    let d = drvs_of.get(&key).cloned().unwrap_or_default();
                    (key, d)
                })
                .collect();

        let mut tally: std::collections::BTreeMap<&str, usize> = Default::default();
        // The rows where the two signals could disagree: an exact pair whose
        // derivations are not GHC-built, or a GHC-built derivation whose pair
        // is not exact. Only the first can block an identity, and it is the
        // one FR-006 is about.
        let mut exact_but_not_ghc: Vec<String> = Vec::new();
        let mut ghc_but_not_exact: Vec<String> = Vec::new();

        for ((name, version), drvs) in &by_identity {
            let id = decide(name, version, Some(&set), drvs);
            *tally.entry(id.wire()).or_default() += 1;
            let ghc = drvs.iter().copied().any(built_by_ghc);
            let exact = !version.is_empty()
                && set.get(name).map(|e| e.version == *version).unwrap_or(false);
            if exact && !ghc {
                exact_but_not_ghc.push(format!("{name} {version}"));
            }
            if ghc && !exact {
                ghc_but_not_exact.push(format!("{name} {version}"));
            }
        }

        let exact_total = by_identity
            .iter()
            .filter(|((n, v), _)| {
                !v.is_empty() && set.get(n).map(|e| e.version == *v).unwrap_or(false)
            })
            .count();

        eprintln!("T009 components emitted by the closure tier: {}", by_identity.len());
        eprintln!("T009 outcomes (this code's `decide`):");
        for (wire, n) in &tally {
            eprintln!("    {wire:26} {n}");
        }
        eprintln!(
            "T009 the two signals agree on {} of {} exact-pair rows",
            exact_total - exact_but_not_ghc.len(),
            exact_total
        );
        eprintln!("T009 exact pair but NOT ghc-built ({}): {:?}",
            exact_but_not_ghc.len(), exact_but_not_ghc);
        eprintln!("T009 ghc-built but NOT an exact pair ({}): {:?}",
            ghc_but_not_exact.len(), ghc_but_not_exact);

        // Not an assertion about the number -- FR-006 says measure it, and a
        // disagreement is a finding to explain, not a test failure. What IS
        // asserted is that the probe examined something, because a probe that
        // silently measured nothing is the failure mode `feedback_zero_results
        // _can_mean_broken` is about.
        assert!(!by_identity.is_empty(), "the closure yielded no identities");
        assert!(exact_total > 0, "no exact pairs found; the probe measured nothing");
    }
}

/// What the closure-identity pass did, for the operator-facing log and for
/// the tests that assert on populations rather than on single components.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ClosureIdentitySummary {
    /// Outcomes by C201 wire value.
    pub(crate) outcomes: std::collections::BTreeMap<String, usize>,
    /// PURL rewrites, as `(before, after)`. The caller MUST apply these to
    /// the relationship endpoints; `reclassify` does it for the edges that
    /// exist when it runs, and returns them so a later producer can too.
    pub(crate) renames: Vec<(String, String)>,
    /// Declared edges emitted (FR-012).
    pub(crate) edges: usize,
    /// Declared edges whose target was already in the graph.
    pub(crate) edges_duplicate: usize,
    /// Boot-library components synthesised for a target the closure does not
    /// contain (FR-012c). Measured 13 of 278 on the real closure, all three
    /// distinct targets being GHC boot libraries.
    pub(crate) boot_targets: usize,
    /// Declared targets absent from the document that are **not** boot
    /// libraries, so there is no basis on which to emit a component for them.
    ///
    /// Counted rather than silently skipped: measured 0 on the real closure,
    /// and a count that quietly omitted edges is the shape #1218 was about. A
    /// nonzero value here means FR-012c has a population its research did not
    /// predict, and the log line says so.
    pub(crate) dangling_non_boot: usize,
}

impl ClosureIdentitySummary {
    pub(crate) fn confirmed(&self) -> usize {
        self.outcomes.get("confirmed").copied().unwrap_or(0)
    }

    /// The C-row document-scope value for this pass.
    pub(crate) fn to_document_value(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        for (k, v) in &self.outcomes {
            m.insert(k.clone(), serde_json::Value::from(*v));
        }
        serde_json::Value::Object(m)
    }
}

/// Whether this component is one the closure tier appended and nothing has
/// since identified.
///
/// Two conditions, and the second is the one measurement forced (findings F5).
/// `merge_closure_components` folds a closure member's annotations onto an
/// identically-named design-tier component, so `waybill:closure-role` is
/// present on **merged** components too and cannot be the only gate: 47 of the
/// 323 members the tier emits merge that way, and they already carry a
/// `pkg:hackage` PURL that FR-015 forbids touching. So the gate is "carries
/// the closure annotation **and** is still generic".
fn is_unidentified_closure_member(c: &ResolvedComponent) -> bool {
    c.extra_annotations.contains_key(ANN_CLOSURE_ROLE)
        && c.purl.as_str().starts_with("pkg:generic/")
}

/// Give every closure-appended member the identity the pinned set holds for
/// it, and say why for the ones it does not.
///
/// Runs **after** `merge_closure_components` and before emission. Not before:
/// milestone 1035 measured that moving the merge above the Haskell enrichment
/// downgraded 41 PURLs from `pkg:hackage` to `pkg:generic`.
///
/// `set` is `None` when the package set could not be obtained, in which case
/// every member lands in `PackageSetUnavailable` and the scan still succeeds
/// (FR-009, Principle XI).
pub(crate) fn reclassify(
    components: &mut Vec<ResolvedComponent>,
    relationships: &mut Vec<waybill_common::resolution::Relationship>,
    closure: &ClassifiedClosure,
    set: Option<&PackageSet>,
    boot: &BTreeSet<String>,
) -> ClosureIdentitySummary {
    // Derivations by the identity a consumer sees. A closure holds several
    // derivations per `(pname, version)` -- build variants -- and the GHC
    // signal holds if any of them was GHC-built.
    let mut drvs_of: std::collections::BTreeMap<(&str, String), Vec<&RawDerivation>> =
        Default::default();
    for drv in closure.raw.derivations.values() {
        let Some(pname) = drv.pname() else { continue };
        let version = drv.version().unwrap_or_default().to_string();
        drvs_of.entry((pname, version)).or_default().push(drv);
    }

    let mut summary = ClosureIdentitySummary::default();
    for c in components.iter_mut().filter(|c| is_unidentified_closure_member(c)) {
        let drvs = drvs_of
            .get(&(c.name.as_str(), c.version.clone()))
            .cloned()
            .unwrap_or_default();
        let id = decide(&c.name, &c.version, set, &drvs);
        *summary.outcomes.entry(id.wire().to_string()).or_default() += 1;

        // Principle X / FR-007: every examined member records the outcome,
        // including `confirmed` -- which is also what lets a consumer tell a
        // closure-derived `pkg:hackage` component from a manifest-derived one
        // (FR-013, C201).
        c.extra_annotations.insert(
            ANN_CLOSURE_IDENTITY.to_string(),
            serde_json::Value::from(id.wire()),
        );

        let Some(attribute) = id.attribute() else { continue };
        let Some(entry) = set.and_then(|s| s.get(attribute)) else { continue };

        // FR-002 / FR-003 through the SAME helpers the existing resolution
        // uses, so "by the same rules" is one implementation rather than two
        // that agree today.
        super::attach_declared_license(c, entry.license.as_ref());
        super::attach_source_hash(c, entry.source_hash.as_deref());

        // FR-001. `name` and `version` are untouched: reclassification only
        // happens when the closure's version and the set's already agree, so
        // there is nothing to rewrite -- which is what keeps this clear of
        // #1216's defect class, where a dedup preferred one identity and
        // discarded a real version.
        if let Ok(p) = waybill_common::types::purl::Purl::new(&format!(
            "pkg:hackage/{}@{}",
            c.name, c.version
        )) {
            let before = c.purl.as_str().to_string();
            let after = p.as_str().to_string();
            if before != after {
                summary.renames.push((before, after));
            }
            c.purl = p;
        }
    }

    // #980, and the reason this is not a following pass: changing a PURL
    // orphans every relationship holding the old one, and the failure is
    // SILENT in all three formats -- CycloneDX keeps an edge pointing at the
    // old PURL, SPDX drops the relationship outright, and neither errors. So
    // the endpoints move in the same step the identities do.
    let rewritten = super::apply_renames(&summary.renames, relationships);
    if rewritten > 0 {
        tracing::info!(
            renamed_components = summary.renames.len(),
            rewritten_endpoints = rewritten,
            "nix-closure: rewrote dependency-edge endpoints after reclassification"
        );
    }

    let haskell_builds: std::collections::HashSet<(String, String)> = drvs_of
        .iter()
        .filter(|(_, drvs)| drvs.iter().any(|d| d.is_haskell_builder()))
        .map(|((name, version), _)| (name.to_string(), version.clone()))
        .collect();
    emit_declared_edges(components, relationships, set, boot, &haskell_builds, &mut summary);
    summary
}

/// FR-012 — give each confirmed member the out-edges the pinned set declares.
///
/// Runs after the rename, so every endpoint is a final identity and #980's
/// failure mode cannot apply to these edges: they are built from the PURLs the
/// document will carry rather than from ones a later pass rewrites.
///
/// `haskell_builds` is every closure `(pname, version)` with a derivation
/// built by the Haskell generic builder, which is what a declared target binds
/// to first when a name is shared (#1459).
fn emit_declared_edges(
    components: &mut Vec<ResolvedComponent>,
    relationships: &mut Vec<waybill_common::resolution::Relationship>,
    set: Option<&PackageSet>,
    boot: &BTreeSet<String>,
    haskell_builds: &std::collections::HashSet<(String, String)>,
    summary: &mut ClosureIdentitySummary,
) {
    let Some(set) = set else { return };

    // Which members this pass confirmed, and their final PURLs.
    let confirmed: Vec<(String, String)> = components
        .iter()
        .filter(|c| {
            c.extra_annotations
                .get(ANN_CLOSURE_IDENTITY)
                .and_then(|v| v.as_str())
                == Some("confirmed")
        })
        .map(|c| (c.name.clone(), c.purl.as_str().to_string()))
        .collect();
    if confirmed.is_empty() {
        return;
    }

    // Targets resolve BY NAME against the document, never by the set's
    // version. This is load-bearing and measured: 165 of 278 declared targets
    // are present at a version the set does not declare, because the closure
    // carries what GHC actually shipped. Binding to the set's version would
    // point 59% of edges at components that do not exist.
    //
    // #1459: a declared target names a Haskell package, so when components
    // share the name, a Haskell build of it wins, then any other `pkg:hackage`
    // component, then the rest. Without this the last component of the name
    // won, and on nixpkgs `a799d3e3` `tls -> zlib` bound to
    // `pkg:generic/zlib@1.3.2`, the C library, leaving Haskell `zlib`
    // unreached by the static-link walk and marked on pandoc-cli, hledger and
    // cachix. A Haskell build outranks `pkg:hackage` because a closure's
    // Haskell build can stay `pkg:generic` (`os-string 2.0.10`, not in the
    // package set) beside a versionless manifest-declared `pkg:hackage`
    // component, and the build is what is linked. Within a rank the last still
    // wins, as before.
    let rank = |c: &ResolvedComponent| -> u8 {
        if haskell_builds.contains(&(c.name.clone(), c.version.clone())) {
            2
        } else if c.purl.as_str().starts_with("pkg:hackage/") {
            1
        } else {
            0
        }
    };
    let mut best: std::collections::HashMap<String, (u8, String)> = std::collections::HashMap::new();
    for c in components.iter() {
        // #1466 — a closure member that declares no `pname` (C206) is never a
        // candidate. Its name is whatever its derivation is called, and on
        // nixpkgs `a799d3e3` a live-bootstrap C fragment is called `time`:
        // 24 libraries declaring the boot library bound to it. A Haskell
        // build always declares its `pname`, so no such member can be the
        // package a declared target names. Excluded, a boot target falls
        // through to FR-012c's compiler-supplied component and any other
        // target to the counted omission, exactly as when the name is absent.
        if c.extra_annotations.contains_key(ANN_MEMBER_ROLE) {
            continue;
        }
        let r = rank(c);
        if best.get(&c.name).is_none_or(|(held, _)| r >= *held) {
            best.insert(c.name.clone(), (r, c.purl.as_str().to_string()));
        }
    }
    let mut purl_of: std::collections::HashMap<String, String> =
        best.into_iter().map(|(name, (_, purl))| (name, purl)).collect();

    let mut present: std::collections::HashSet<(String, String)> = relationships
        .iter()
        .filter(|r| {
            r.relationship_type == waybill_common::resolution::RelationshipType::DependsOn
        })
        .map(|r| (r.from.clone(), r.to.clone()))
        .collect();

    for (name, from_purl) in &confirmed {
        let Some(entry) = set.get(name) else { continue };
        for target in declared_relations(entry) {
            let to_purl = match purl_of.get(&target) {
                Some(p) => p.clone(),
                None => {
                    // FR-012c — the closure does not contain this declared
                    // dependency. Measured: 13 of 278, every distinct one a
                    // GHC boot library, because the compiler ships them
                    // inside its own derivation rather than beside it.
                    if !super::boot_libraries::is_boot_library(&target, boot) {
                        // No basis to emit a component: inventing one would
                        // assert a package exists on the strength of a name.
                        // Counted so the omission is reported, not silent.
                        summary.dangling_non_boot += 1;
                        continue;
                    }
                    let Some(c) = compiler_supplied_component(&target) else { continue };
                    let p = c.purl.as_str().to_string();
                    purl_of.insert(target.clone(), p.clone());
                    components.push(c);
                    summary.boot_targets += 1;
                    p
                }
            };
            if from_purl == &to_purl {
                continue;
            }
            if !present.insert((from_purl.clone(), to_purl.clone())) {
                summary.edges_duplicate += 1;
                continue;
            }
            relationships.push(waybill_common::resolution::Relationship {
                from: from_purl.clone(),
                to: to_purl,
                relationship_type: waybill_common::resolution::RelationshipType::DependsOn,
                // Principle X — the edge says where it came from, so a
                // consumer can tell a relation read from the pinned nixpkgs
                // apart from one the project declared itself.
                provenance: waybill_common::resolution::EnrichmentProvenance {
                    source: "nixpkgs".to_string(),
                    data_type: "nix-closure-declared".to_string(),
                },
            });
            summary.edges += 1;
        }
    }

    if summary.dangling_non_boot > 0 {
        tracing::warn!(
            dangling_non_boot = summary.dangling_non_boot,
            "nix-closure: declared targets absent from the document and not              boot libraries; their edges are omitted because emitting a              component for a bare name would assert a package exists"
        );
    }
    tracing::info!(
        edges = summary.edges,
        duplicate = summary.edges_duplicate,
        boot_targets = summary.boot_targets,
        "nix-closure: declared dependency edges emitted"
    );
}

/// A declared dependency the compiler supplies: versionless, with the reason
/// that says so.
///
/// The shape the design tier already uses for a boot library, which milestone
/// 1273's US4 test asserts. Versionless **and** explained, because a
/// versionless component with no accepted reason fails the
/// `versionless_explained` invariant — "the compiler supplies this" and
/// "waybill could not resolve it" call for completely different action.
fn compiler_supplied_component(name: &str) -> Option<ResolvedComponent> {
    let purl = waybill_common::types::purl::Purl::new(&format!("pkg:hackage/{name}")).ok()?;
    // Written out in full because `ResolvedComponent` has no `Default`, which
    // is itself deliberate: a component with every field defaulted is a
    // component nobody decided anything about.
    let mut c = ResolvedComponent {
        build_inclusion: None,
        artifact_presence: None,
        scope_evidence: None,
        name: name.to_string(),
        version: String::new(),
        purl,
        evidence: waybill_common::resolution::ResolutionEvidence {
            technique: waybill_common::resolution::ResolutionTechnique::UrlPattern,
            confidence: 0.95,
            source_connection_ids: vec![],
            source_file_paths: vec![],
            deps_dev_match: None,
        },
        licenses: vec![],
        concluded_licenses: vec![],
        hashes: vec![],
        supplier: None,
        cpes: vec![],
        advisories: vec![],
        occurrences: vec![],
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
        source_type: None,
        sbom_tier: Some("build".to_string()),
        buildinfo_status: None,
        evidence_kind: None,
        binary_class: None,
        binary_stripped: None,
        linkage_kind: None,
        detected_go: None,
        confidence: None,
        binary_packed: None,
        npm_role: None,
        raw_version: None,
        parent_purl: None,
        co_owned_by: None,
        shade_relocation: None,
        external_references: vec![],
        extra_annotations: Default::default(),
        binary_role: None,
    };
    c.extra_annotations.insert(
        super::ANN_UNRESOLVED_REASON.to_string(),
        serde_json::Value::from(super::UnresolvedReason::CompilerSupplied.as_str()),
    );
    // Deliberately NO `waybill:closure-identity` here. C201 declares a closed
    // enum of six values, every one of them an outcome the pass reached about
    // a closure MEMBER — and this component is not one: it is synthesised as
    // an edge target because the compiler ships it inside its own derivation.
    // Emitting a seventh value would make a row that says "closed" not be,
    // which is the defect `enum_allowlist_falls_out_of_date` records. What
    // this component owes a reader is why it has no version, and C170 above
    // says exactly that.
    Some(c)
}
