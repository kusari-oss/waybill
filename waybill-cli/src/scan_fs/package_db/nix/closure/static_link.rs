//! Milestone 1423 (#1423), FR-002 — Haskell libraries statically linked into
//! a runtime member.
//!
//! A runtime closure lists the store paths an output **references**, and
//! static linking leaves none: shellcheck's runtime closure is 5 paths and
//! holds none of the 50 Haskell libraries compiled into its binary
//! (`specs/1423-nix-runtime-scope/measurements/haskell-static/`). Marking scope
//! from the runtime closure alone would therefore declare linked code absent.
//!
//! Two sources of library-dependency edges, scored in
//! `measurements/nix-behaviour/q6-*`:
//!
//! 1. **Declared edges** (preferred): milestone 1275's `nix-closure-declared`
//!    relations, `libraryHaskellDepends + executableHaskellDepends` from the
//!    pinned package set, with no test or benchmark dependencies. Exact on
//!    ormolu (29 of 29).
//! 2. **Derivation fields** (fallback): at a runtime Haskell member,
//!    `buildInputs + propagatedBuildInputs`; then `propagatedBuildInputs`
//!    transitively. Exact on shellcheck (50 of 50), but at a root built with
//!    checks the generic builder puts test dependencies in the same
//!    `buildInputs`, so on ormolu it finds 75 against 29. It errs toward
//!    runtime, never toward dropping linked code.
//!
//! Go modules and Rust crates are not here: they inherit their host binary's
//! status through the milestone-1378 `embedded-build-info` edges.
//!
//! # A root's test suite is not linked code (#1283)
//!
//! The fallback reads a root's `buildInputs`, and with `doCheck` the generic
//! builder puts the test suite's dependencies there. Two facts narrow it, each
//! measured at the pinned revision
//! (`specs/1283-nix-allow-ifd/measurements/invariants/`):
//!
//! - **A library's `buildInputs` holds nothing it links.** The generic builder
//!   propagates a library's library and executable dependencies, so
//!   `aeson-2.2.4.1` lists all 24 of them in `propagatedBuildInputs` and its
//!   test suite (`tasty`, `tasty-hunit`, `tasty-golden`, …) in `buildInputs`;
//!   warp, servant-server, hspec-core and tasty-hunit measure the same. Only
//!   an executable-only build (`isLibrary = false`, which
//!   `justStaticExecutables` sets) lists its link dependencies in
//!   `buildInputs`, as the static `ShellCheck` does. So at a root that is a
//!   library, `buildInputs` is not followed. A root is a library when it
//!   propagates a Haskell build, or, since one whose dependencies are all
//!   boot packages propagates nothing (`hashable`), when its install phase
//!   registers a package ([`super::derivation::RawDerivation::registers_haskell_package`]).
//! - **The project's own manifest names its test suite.** At an
//!   executable-only root, library and test dependencies share `buildInputs`;
//!   when the scanned project's manifest declares a dependency for its test
//!   suite only, that root's `buildInputs` entry of the same name is not
//!   followed ([`ManifestTestDeps`]).
//!
//! Neither makes a library *not* linked: anything also reached through a
//! library edge from a runtime member is still in the set.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use waybill_common::resolution::{Relationship, RelationshipType, ResolvedComponent};

use super::derivation::{store_basename, RawClosure};
use super::member_key::{drvs_by_key, key_of_component};
use crate::scan_fs::package_db::nix::haskell_packages::closure_identity::ANN_CLOSURE_IDENTITY;

/// The milestone-1275 relationship data type for package-set relations.
pub(crate) const DECLARED_DATA_TYPE: &str = "nix-closure-declared";

/// Which edges found the statically linked libraries (C216
/// `static-link-source`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StaticLinkSource {
    /// Milestone 1275's declared relations, which exclude test dependencies.
    DeclaredEdges,
    /// The derivation-field fallback, which may include test-only
    /// dependencies of a root built with checks.
    DerivationFields,
    /// Nothing seeded static linking: the runtime closure holds no
    /// Haskell-builder member, and the evaluated root consumed no Haskell
    /// build ([`super::walk::consumed_haskell_builds`]).
    None,
}

impl StaticLinkSource {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::DeclaredEdges => "declared-edges",
            Self::DerivationFields => "derivation-fields",
            Self::None => "none",
        }
    }
}

/// Milestone 1275's declared library relations, mapped back to `.drv` keys.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DeclaredEdges {
    /// Derivations whose component the pinned package set confirmed. Only
    /// these have declared relations; an absent out-edge on one of them means
    /// "declares nothing", never "not looked up".
    pub(crate) confirmed: BTreeSet<String>,
    pub(crate) edges: BTreeMap<String, BTreeSet<String>>,
}

impl DeclaredEdges {
    /// Read the declared relations off the document.
    ///
    /// The relations are PURL to PURL; each endpoint maps to every derivation
    /// sharing its component's identity, through the same join `contents`
    /// uses. A target the closure does not hold (a compiler-supplied boot
    /// library) maps to nothing and is dropped: it is not a closure member,
    /// so there is nothing to decide about it.
    pub(crate) fn from_document(
        raw: &RawClosure,
        components: &[ResolvedComponent],
        relationships: &[Relationship],
    ) -> Self {
        let by_key = drvs_by_key(raw);
        let mut drvs_of_purl: BTreeMap<&str, &BTreeSet<String>> = BTreeMap::new();
        let mut out = Self::default();
        for c in components {
            let Some(drvs) = key_of_component(c).and_then(|k| by_key.get(&k)) else {
                continue;
            };
            drvs_of_purl.entry(c.purl.as_str()).or_insert(drvs);
            if c.extra_annotations
                .get(ANN_CLOSURE_IDENTITY)
                .and_then(|v| v.as_str())
                == Some("confirmed")
            {
                out.confirmed.extend(drvs.iter().cloned());
            }
        }
        for r in relationships
            .iter()
            .filter(|r| r.provenance.data_type == DECLARED_DATA_TYPE)
        {
            let (Some(from), Some(to)) =
                (drvs_of_purl.get(r.from.as_str()), drvs_of_purl.get(r.to.as_str()))
            else {
                continue;
            };
            for f in from.iter() {
                out.edges.entry(f.clone()).or_default().extend(to.iter().cloned());
            }
        }
        out
    }
}

/// The relationship data type of an edge a manifest reader declared
/// (`scan_fs::mod`, from each entry's `depends`).
///
/// An allowlist of one, deliberately. Every other data type is an edge waybill
/// inferred or that the build graph recorded, and milestone 1269's re-typing
/// marks those `DevDependsOn` too whenever their target is scoped
/// `development`, including a library relation into a test library. Reading
/// one of those as "the manifest declares this for its tests" could drop a
/// library an executable links. An edge type added later is not read, which
/// errs toward runtime.
const MANIFEST_DATA_TYPE: &str = "package-database-depends";

/// #1283 — what a closure member's own manifest declares for its test suite or
/// for development only, by package name, keyed by the member's `.drv`.
///
/// In practice the scanned project: its main module is the evaluated package,
/// and the Haskell reader typed each declared edge from the stanza that names
/// it. Keyed by **name** rather than by component because the build may use a
/// version the declaration does not resolve to, which makes the built
/// derivation a different component from the declared one (slacklinker's
/// `tmp-postgres`: declared 1.34.1.0, built 1.35.0.0). The name is what
/// cabal2nix joins on when it turns the same declaration into `buildInputs`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ManifestTestDeps {
    pub(crate) by_drv: BTreeMap<String, BTreeSet<String>>,
}

impl ManifestTestDeps {
    /// Read off the document as it stands before the scope pass.
    pub(crate) fn from_document(
        raw: &RawClosure,
        components: &[ResolvedComponent],
        relationships: &[Relationship],
    ) -> Self {
        let name_of: HashMap<&str, &str> =
            components.iter().map(|c| (c.purl.as_str(), c.name.as_str())).collect();
        let mut names_of_purl: HashMap<&str, BTreeSet<String>> = HashMap::new();
        for r in relationships.iter().filter(|r| {
            r.provenance.data_type == MANIFEST_DATA_TYPE
                && matches!(
                    r.relationship_type,
                    RelationshipType::DevDependsOn | RelationshipType::TestDependsOn
                )
        }) {
            if let Some(name) = name_of.get(r.to.as_str()) {
                names_of_purl.entry(r.from.as_str()).or_default().insert((*name).to_string());
            }
        }
        let by_key = drvs_by_key(raw);
        let mut out = Self::default();
        for c in components {
            let Some(names) = names_of_purl.get(c.purl.as_str()) else { continue };
            let Some(drvs) = key_of_component(c).and_then(|k| by_key.get(&k)) else {
                continue;
            };
            for d in drvs {
                out.by_drv.entry(d.clone()).or_default().extend(names.iter().cloned());
            }
        }
        out
    }
}

/// The Haskell libraries linked into the runtime members, as `.drv` keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StaticLinkSet {
    pub(crate) members: BTreeSet<String>,
    pub(crate) source: StaticLinkSource,
}

/// Compute the statically linked set from `runtime_drvs`: the runtime
/// members, plus any Haskell build the evaluated root consumed that the
/// runtime closure does not hold (x86_64-linux's shellcheck, q10). The seeds
/// themselves are not in the result.
///
/// `declared` is `Some` when the pinned package set was available to the
/// identity pass. A runtime Haskell member whose component the set did not
/// confirm has no declared relations, so the fallback applies to it; the
/// source is then `derivation-fields`, because that root's walk may include
/// its test-only dependencies. Below a root, a library is followed through its
/// `propagatedBuildInputs` and, when confirmed, its declared relations too;
/// neither carries test dependencies at a library (q6), and the declared
/// relations alone miss a platform-conditional dependency (#1459).
///
/// A root that falls back reads `buildInputs` only when it is an
/// executable-only build, and then skips what `test_deps` says its manifest
/// declares for its test suite (#1283, module docs).
pub(crate) fn static_link_set(
    raw: &RawClosure,
    runtime_drvs: &BTreeSet<String>,
    declared: Option<&DeclaredEdges>,
    test_deps: &ManifestTestDeps,
) -> StaticLinkSet {
    let index = raw.output_index();
    let is_hs = |k: &str| raw.derivations.get(k).is_some_and(|d| d.is_haskell_builder());
    let field = |k: &str, f: &str| -> Vec<String> {
        raw.derivations
            .get(k)
            .map(|d| {
                d.paths_in(f)
                    .filter_map(|p| index.get(store_basename(p)).copied())
                    .filter(|k| is_hs(k))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    // Haskell builds only, as with the derivation fields: the set is the
    // Haskell libraries linked in, and a declared target bound to a C library
    // of the same name (#1459) is not one of them.
    let declared_out = |k: &str| -> Option<Vec<String>> {
        let d = declared?;
        d.confirmed.contains(k).then(|| {
            d.edges
                .get(k)
                .map(|t| t.iter().filter(|t| is_hs(t)).cloned().collect())
                .unwrap_or_default()
        })
    };

    let roots: Vec<&String> = runtime_drvs.iter().filter(|k| is_hs(k)).collect();
    if roots.is_empty() {
        return StaticLinkSet { members: BTreeSet::new(), source: StaticLinkSource::None };
    }

    let pname_of = |k: &str| raw.derivations.get(k).and_then(|d| d.pname());

    let mut any_root_by_fields = false;
    let mut stack: Vec<String> = Vec::new();
    for r in &roots {
        match declared_out(r) {
            Some(targets) => stack.extend(targets),
            None => {
                any_root_by_fields = true;
                let propagated = field(r, "propagatedBuildInputs");
                // #1283: a library propagates everything it links, and its
                // `buildInputs` is its test suite. One whose dependencies
                // are all boot packages propagates nothing, so its install
                // phase is the other sign.
                let library = !propagated.is_empty()
                    || raw.derivations.get(r.as_str()).is_some_and(|d| d.registers_haskell_package());
                if !library {
                    let tests = test_deps.by_drv.get(r.as_str());
                    stack.extend(field(r, "buildInputs").into_iter().filter(|k| {
                        !pname_of(k).is_some_and(|n| tests.is_some_and(|t| t.contains(n)))
                    }));
                }
                stack.extend(propagated);
            }
        }
    }
    // Below a root, a library's `propagatedBuildInputs` is followed even when
    // it is confirmed. The declared relations are cabal2nix's Linux
    // resolution, so on darwin `fsnotify` declares `hinotify` while its
    // derivation propagates `hfsevents`, which is what is linked (#1459,
    // cachix). A library propagates no test dependencies (q6), so the union
    // errs toward runtime only by what a library propagates beyond its link
    // dependencies (aeson-pretty's executable-only ones, R4).
    let mut members = BTreeSet::new();
    while let Some(n) = stack.pop() {
        if !members.insert(n.clone()) {
            continue;
        }
        stack.extend(declared_out(&n).unwrap_or_default());
        stack.extend(field(&n, "propagatedBuildInputs"));
    }
    StaticLinkSet {
        members,
        source: if declared.is_none() || any_root_by_fields {
            StaticLinkSource::DerivationFields
        } else {
            StaticLinkSource::DeclaredEdges
        },
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// The ormolu shape: a static root whose `buildInputs` carries library
    /// AND test-only dependencies (`doCheck`), libraries that propagate their
    /// own library dependencies, and a non-Haskell input that must never be
    /// followed.
    const CLOSURE: &str = r#"{
      "derivations": {
        "d-root.drv": {"env": {"pname":"root","version":"1","setupCompilerEnvironmentPhase":"x",
                               "buildInputs":"/nix/store/h-lib /nix/store/h-test /nix/store/h-zlib"},
                       "outputs": {"out": {"path":"h-root"}}},
        "d-lib.drv":  {"env": {"pname":"lib","version":"1","setupCompilerEnvironmentPhase":"x",
                               "propagatedBuildInputs":"/nix/store/h-dep"},
                       "outputs": {"out": {"path":"h-lib"}}},
        "d-dep.drv":  {"env": {"pname":"dep","version":"1","setupCompilerEnvironmentPhase":"x"},
                       "outputs": {"out": {"path":"h-dep"}}},
        "d-test.drv": {"env": {"pname":"test","version":"1","setupCompilerEnvironmentPhase":"x",
                               "propagatedBuildInputs":"/nix/store/h-testdep"},
                       "outputs": {"out": {"path":"h-test"}}},
        "d-testdep.drv": {"env": {"pname":"testdep","version":"1","setupCompilerEnvironmentPhase":"x"},
                       "outputs": {"out": {"path":"h-testdep"}}},
        "d-zlib.drv": {"env": {"pname":"zlib","version":"1"},
                       "outputs": {"out": {"path":"h-zlib"}}},
        "d-libroot.drv": {"env": {"pname":"libroot","version":"1","setupCompilerEnvironmentPhase":"x",
                               "propagatedBuildInputs":"/nix/store/h-lib",
                               "buildInputs":"/nix/store/h-test"},
                       "outputs": {"out": {"path":"h-libroot"}}},
        "d-bootonly.drv": {"env": {"pname":"bootonly","version":"1","setupCompilerEnvironmentPhase":"x",
                               "installPhase":"./Setup copy\n./Setup register --gen-pkg-config=$f",
                               "buildInputs":"/nix/store/h-test"},
                       "outputs": {"out": {"path":"h-bootonly"}}}
      }
    }"#;

    fn set(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn no_manifest() -> ManifestTestDeps {
        ManifestTestDeps::default()
    }

    fn manifest(drv: &str, names: &[&str]) -> ManifestTestDeps {
        ManifestTestDeps { by_drv: BTreeMap::from([(drv.to_string(), set(names))]) }
    }

    /// #1283, slack-web and openai: a library root propagates everything it
    /// links, so its `buildInputs` (the test suite, with `doCheck`) is not
    /// followed even with no manifest to say so.
    #[test]
    fn a_library_root_follows_what_it_propagates_and_not_its_test_suite() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let s = static_link_set(&raw, &set(&["d-libroot.drv"]), None, &no_manifest());
        assert_eq!(s.source, StaticLinkSource::DerivationFields);
        assert_eq!(s.members, set(&["d-dep.drv", "d-lib.drv"]));
    }

    /// #1283, hashable's shape: a library whose dependencies are all boot
    /// packages propagates nothing; its install phase registering a package
    /// is what says its `buildInputs` is a test suite.
    #[test]
    fn a_library_that_propagates_nothing_is_known_by_its_install_phase() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let s = static_link_set(&raw, &set(&["d-bootonly.drv"]), None, &no_manifest());
        assert!(s.members.is_empty(), "{:?}", s.members);
    }

    /// #1283, slacklinker: an executable-only root lists library and test
    /// dependencies together; the one its manifest declares for its test
    /// suite is not followed, and nor is what only it reaches.
    #[test]
    fn an_executable_root_skips_what_its_manifest_declares_for_tests() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let s = static_link_set(&raw, &set(&["d-root.drv"]), None, &manifest("d-root.drv", &["test"]));
        assert_eq!(s.source, StaticLinkSource::DerivationFields);
        assert_eq!(s.members, set(&["d-dep.drv", "d-lib.drv"]));
    }

    /// The manifest narrows one root's own edge, not the library: `dep` is
    /// declared for the tests too, and it is still linked through `lib`.
    #[test]
    fn a_manifest_test_name_does_not_hide_a_library_linked_another_way() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let s = static_link_set(
            &raw,
            &set(&["d-root.drv"]),
            None,
            &manifest("d-root.drv", &["test", "dep"]),
        );
        assert_eq!(s.members, set(&["d-dep.drv", "d-lib.drv"]));
    }

    /// The manifest is read only from edges a manifest reader declared. A
    /// build-graph edge into a test library is typed `DevDependsOn` too once
    /// the propagation scopes its target, and it says nothing about which of
    /// the root's inputs its manifest names.
    #[test]
    fn manifest_test_deps_are_read_from_manifest_edges_only() {
        use crate::generate::graph_completeness::test_support::mk_component;
        use waybill_common::resolution::EnrichmentProvenance;
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let mut root = mk_component("pkg:hackage/root@1");
        root.extra_annotations.insert(
            super::super::emit::ANN_CLOSURE_ROLE.to_string(),
            serde_json::Value::String("unreferenced".to_string()),
        );
        let components =
            vec![root, mk_component("pkg:hackage/test"), mk_component("pkg:hackage/lib@1")];
        let edge = |to: &str, ty: RelationshipType, data_type: &str| Relationship {
            from: "pkg:hackage/root@1".to_string(),
            to: to.to_string(),
            relationship_type: ty,
            provenance: EnrichmentProvenance {
                source: "root.cabal".to_string(),
                data_type: data_type.to_string(),
            },
        };
        let relationships = vec![
            edge("pkg:hackage/test", RelationshipType::DevDependsOn, MANIFEST_DATA_TYPE),
            edge("pkg:hackage/lib@1", RelationshipType::DependsOn, MANIFEST_DATA_TYPE),
            edge("pkg:hackage/lib@1", RelationshipType::DevDependsOn, "nix-build-graph"),
        ];
        let m = ManifestTestDeps::from_document(&raw, &components, &relationships);
        assert_eq!(m, manifest("d-root.drv", &["test"]));
    }

    #[test]
    fn derivation_fields_follow_the_root_then_propagation_and_include_test_deps() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let s = static_link_set(&raw, &set(&["d-root.drv"]), None, &no_manifest());
        assert_eq!(s.source, StaticLinkSource::DerivationFields);
        assert_eq!(
            s.members,
            set(&["d-dep.drv", "d-lib.drv", "d-test.drv", "d-testdep.drv"]),
            "the fallback cannot tell a test dependency from a library one at a \
             root built with checks, so it errs toward runtime; zlib is not \
             Haskell and is not followed"
        );
    }

    #[test]
    fn declared_edges_exclude_test_only_dependencies() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let declared = DeclaredEdges {
            confirmed: set(&["d-root.drv", "d-lib.drv", "d-dep.drv"]),
            edges: BTreeMap::from([
                ("d-root.drv".to_string(), set(&["d-lib.drv"])),
                ("d-lib.drv".to_string(), set(&["d-dep.drv"])),
            ]),
        };
        let s = static_link_set(&raw, &set(&["d-root.drv"]), Some(&declared), &no_manifest());
        assert_eq!(s.source, StaticLinkSource::DeclaredEdges);
        assert_eq!(s.members, set(&["d-dep.drv", "d-lib.drv"]));
    }

    #[test]
    fn an_unconfirmed_root_falls_back_and_says_so() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let declared = DeclaredEdges { confirmed: set(&["d-lib.drv"]), edges: BTreeMap::new() };
        let s = static_link_set(&raw, &set(&["d-root.drv"]), Some(&declared), &no_manifest());
        assert_eq!(s.source, StaticLinkSource::DerivationFields);
        assert!(s.members.contains("d-test.drv"));
        assert!(
            s.members.contains("d-dep.drv"),
            "below the root, a confirmed library that declares nothing is still \
             followed through what its derivation propagates (#1459: the declared \
             relations are Linux's, and darwin's `fsnotify` propagates `hfsevents`)"
        );
    }

    /// #1459: a declared target bound to a non-Haskell derivation (a C library
    /// sharing a Haskell library's name) is not a statically linked Haskell
    /// library, so it never enters the set.
    #[test]
    fn a_declared_target_that_is_not_a_haskell_build_is_not_followed() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let declared = DeclaredEdges {
            confirmed: set(&["d-root.drv"]),
            edges: BTreeMap::from([("d-root.drv".to_string(), set(&["d-lib.drv", "d-zlib.drv"]))]),
        };
        let s = static_link_set(&raw, &set(&["d-root.drv"]), Some(&declared), &no_manifest());
        assert_eq!(s.source, StaticLinkSource::DeclaredEdges);
        assert_eq!(s.members, set(&["d-dep.drv", "d-lib.drv"]));
    }

    #[test]
    fn no_runtime_haskell_member_means_none() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let s = static_link_set(&raw, &set(&["d-zlib.drv"]), None, &no_manifest());
        assert_eq!(s.source, StaticLinkSource::None);
        assert!(s.members.is_empty());
    }
}
