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

use std::collections::{BTreeMap, BTreeSet};

use waybill_common::resolution::{Relationship, ResolvedComponent};

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
pub(crate) fn static_link_set(
    raw: &RawClosure,
    runtime_drvs: &BTreeSet<String>,
    declared: Option<&DeclaredEdges>,
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

    let mut any_root_by_fields = false;
    let mut stack: Vec<String> = Vec::new();
    for r in &roots {
        match declared_out(r) {
            Some(targets) => stack.extend(targets),
            None => {
                any_root_by_fields = true;
                stack.extend(field(r, "buildInputs"));
                stack.extend(field(r, "propagatedBuildInputs"));
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
                       "outputs": {"out": {"path":"h-zlib"}}}
      }
    }"#;

    fn set(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn derivation_fields_follow_the_root_then_propagation_and_include_test_deps() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let s = static_link_set(&raw, &set(&["d-root.drv"]), None);
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
        let s = static_link_set(&raw, &set(&["d-root.drv"]), Some(&declared));
        assert_eq!(s.source, StaticLinkSource::DeclaredEdges);
        assert_eq!(s.members, set(&["d-dep.drv", "d-lib.drv"]));
    }

    #[test]
    fn an_unconfirmed_root_falls_back_and_says_so() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let declared = DeclaredEdges { confirmed: set(&["d-lib.drv"]), edges: BTreeMap::new() };
        let s = static_link_set(&raw, &set(&["d-root.drv"]), Some(&declared));
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
        let s = static_link_set(&raw, &set(&["d-root.drv"]), Some(&declared));
        assert_eq!(s.source, StaticLinkSource::DeclaredEdges);
        assert_eq!(s.members, set(&["d-dep.drv", "d-lib.drv"]));
    }

    #[test]
    fn no_runtime_haskell_member_means_none() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let s = static_link_set(&raw, &set(&["d-zlib.drv"]), None);
        assert_eq!(s.source, StaticLinkSource::None);
        assert!(s.members.is_empty());
    }
}
