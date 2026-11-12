//! Milestone 966 (#966) — which GHC built what, as the derivation closure
//! records it.
//!
//! Every Haskell derivation nixpkgs builds names its compiler in
//! `nativeBuildInputs` as a store path, `<hash>-ghc-9.10.3`, or for bootstrap
//! tools `<hash>-ghc-binary-9.8.4`. That is the signal milestone 1275 already
//! reads to decide whether a member is GHC-built (`closure_identity::
//! built_by_ghc`); this module reads the release from it, which is the key of
//! the boot-library table. Research R2.

use std::collections::{BTreeMap, BTreeSet};

use crate::scan_fs::package_db::nix::closure::derivation::{store_basename, RawDerivation};
use crate::scan_fs::package_db::nix::closure::ClassifiedClosure;

/// C219 — the document-scope record of which compilers the closure named and
/// whether the table had each.
pub(crate) const ANN_GHC_BOOT_LIBRARIES: &str = "waybill:ghc-boot-libraries";

/// Where a GHC name starts in a store basename, and what follows it.
///
/// The one place that decides what a GHC store path is called, shared by the
/// milestone-1275 build signal (`closure_identity::is_ghc_store_path`) and by
/// [`GhcRelease::from_store_path`], so the two cannot disagree about the
/// name. They differ only in how much of the version they need: the signal
/// needs a version to be present, the release needs it to be exactly
/// `X.Y.Z`.
///
/// `-ghc-binary-` is tried after `-ghc-`, whose remainder for a bootstrap
/// path is `binary-…` and so carries no version.
pub(crate) fn ghc_name(path: &str) -> Option<(bool, &str)> {
    let base = store_basename(path);
    for (marker, bootstrap) in [("-ghc-", false), ("-ghc-binary-", true)] {
        if let Some((_, rest)) = base.split_once(marker) {
            if rest.starts_with(|c: char| c.is_ascii_digit()) {
                return Some((bootstrap, rest));
            }
        }
    }
    None
}

/// A GHC release, as a closure names it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct GhcRelease {
    /// `9.10.3` — the table key.
    pub(crate) version: String,
    /// `true` for `ghc-binary-*`.
    pub(crate) bootstrap: bool,
    /// `ghc-9.10.3` or `ghc-binary-9.8.4`: C218's value and C219's key.
    pub(crate) label: String,
}

impl GhcRelease {
    /// The release a store path names, or `None` when its version is not
    /// exactly three dotted numbers (`ghc-9.10.3-doc`, `ghc-9.10`,
    /// `ghc-settings-edit`).
    pub(crate) fn from_store_path(path: &str) -> Option<Self> {
        let (bootstrap, version) = ghc_name(path)?;
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 || !parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) {
            return None;
        }
        let label = if bootstrap {
            format!("ghc-binary-{version}")
        } else {
            format!("ghc-{version}")
        };
        Some(Self { version: version.to_string(), bootstrap, label })
    }
}

/// The single GHC a derivation names in `nativeBuildInputs`, or `None` when
/// it names none, or more than one (FR-009 never chooses).
fn compiler_of(drv: &RawDerivation) -> Option<GhcRelease> {
    let found: BTreeSet<GhcRelease> =
        drv.paths_in("nativeBuildInputs").filter_map(GhcRelease::from_store_path).collect();
    if found.len() == 1 {
        found.into_iter().next()
    } else {
        None
    }
}

/// Which GHC built what in one closure. Data model `ClosureCompilers`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ClosureCompilers {
    /// The closure's evaluation system, which decides platform-specific
    /// table rows (research R4).
    pub(crate) system: String,
    /// The compiler for the project's own declarations (FR-009, last bullet).
    pub(crate) project: Option<GhcRelease>,
    /// `(pname, version)` -> every GHC that built one of its build variants.
    ///
    /// Plural because nixpkgs can build a package twice under one identity,
    /// once with the bootstrap `ghc-binary` and once with the project's GHC.
    /// Such a member links each compiler's boot libraries, so it depends on
    /// both versions. Keeping only members whose variants agree left their
    /// edges on a versionless component beside the versioned ones on
    /// `nix-closure-shellcheck` (13 members, `alex`, `happy`, `hashable`,
    /// `splitmix`, ... -> versionless `base`), which FR-010 forbids. A single
    /// variant naming two GHCs says nothing about which one compiled it, so
    /// its member is absent.
    pub(crate) by_member: BTreeMap<(String, String), BTreeSet<GhcRelease>>,
    /// Every distinct compiler assigned above: C219's keys.
    pub(crate) seen: BTreeSet<GhcRelease>,
}

impl ClosureCompilers {
    /// Read the compilers from a classified closure.
    ///
    /// `fallback_system` is the system the closure tier detected and
    /// evaluated for, used when the evaluated derivation records none.
    pub(crate) fn from_closure(closure: &ClassifiedClosure, fallback_system: &str) -> Self {
        let evaluated = closure
            .graph
            .evaluated
            .as_ref()
            .and_then(|k| closure.raw.derivations.get(k));
        Self::from_derivations(closure.raw.derivations.values(), evaluated, fallback_system)
    }

    fn from_derivations<'a>(
        drvs: impl Iterator<Item = &'a RawDerivation>,
        evaluated: Option<&RawDerivation>,
        fallback_system: &str,
    ) -> Self {
        let mut by_member: BTreeMap<(String, String), Option<BTreeSet<GhcRelease>>> = BTreeMap::new();
        let mut non_bootstrap: BTreeSet<GhcRelease> = BTreeSet::new();
        for drv in drvs {
            let all: BTreeSet<GhcRelease> =
                drv.paths_in("nativeBuildInputs").filter_map(GhcRelease::from_store_path).collect();
            non_bootstrap.extend(all.iter().filter(|g| !g.bootstrap).cloned());
            let Some(pname) = drv.pname() else { continue };
            let key = (pname.to_string(), drv.version().unwrap_or_default().to_string());
            let mine = compiler_of(drv);
            // Each build variant contributes the GHC that compiled it. A
            // variant naming no GHC says nothing (a wrapper or a data output
            // beside the library build); one naming two poisons the member,
            // because which of them compiled it is unknown.
            match mine {
                None if all.len() > 1 => {
                    by_member.insert(key, None);
                }
                None => {}
                Some(g) => {
                    if let Some(held) = by_member.entry(key).or_insert_with(|| Some(BTreeSet::new())) {
                        held.insert(g);
                    }
                }
            }
        }
        let by_member: BTreeMap<(String, String), BTreeSet<GhcRelease>> = by_member
            .into_iter()
            .filter_map(|(k, v)| v.filter(|s| !s.is_empty()).map(|s| (k, s)))
            .collect();

        // Research R2: the evaluated derivation's compiler; failing that, the
        // closure's single non-bootstrap GHC, and otherwise none.
        let project = evaluated.and_then(compiler_of).or_else(|| {
            if non_bootstrap.len() == 1 {
                non_bootstrap.iter().next().cloned()
            } else {
                None
            }
        });
        let system = evaluated
            .and_then(|d| d.attr_str("system"))
            .filter(|s| !s.is_empty())
            .unwrap_or(fallback_system)
            .to_string();
        let mut seen: BTreeSet<GhcRelease> = by_member.values().flatten().cloned().collect();
        seen.extend(project.iter().cloned());
        Self { system, project, by_member, seen }
    }

    /// The project compiler, when the boot-library table has it.
    pub(crate) fn project_in_table(&self) -> Option<&GhcRelease> {
        self.project
            .as_ref()
            .filter(|g| super::boot_table::table().has_release(&g.version))
    }

    /// The compilers that built `(pname, version)`, when the table has
    /// **every** one of them. Empty otherwise, so a member some of whose
    /// variants were built by an unknown compiler keeps today's path whole,
    /// rather than gaining versioned edges for some variants and a versionless
    /// one for the rest.
    pub(crate) fn members_in_table(&self, pname: &str, version: &str) -> Vec<&GhcRelease> {
        let table = super::boot_table::table();
        match self.by_member.get(&(pname.to_string(), version.to_string())) {
            Some(set) if set.iter().all(|g| table.has_release(&g.version)) => set.iter().collect(),
            _ => Vec::new(),
        }
    }

    /// The C219 value: which compilers, whether the table had each, which
    /// one the project's own declarations took, the closure's system and the
    /// table's date. Keys sort through `serde_json::Map`.
    pub(crate) fn document_value(&self, table_date: &str) -> serde_json::Value {
        let table = super::boot_table::table();
        let compilers: serde_json::Map<String, serde_json::Value> = self
            .seen
            .iter()
            .map(|g| {
                let state = if table.has_release(&g.version) { "in-table" } else { "absent-from-table" };
                (g.label.clone(), serde_json::Value::from(state))
            })
            .collect();
        serde_json::json!({
            "compilers": compilers,
            "project": self.project.as_ref().map(|g| g.label.clone()),
            "schema": 1,
            "system": self.system,
            "table": table_date,
        })
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn drv(pname: &str, version: &str, native: &[&str], system: Option<&str>) -> RawDerivation {
        let mut env = BTreeMap::new();
        env.insert("pname".to_string(), pname.to_string());
        env.insert("version".to_string(), version.to_string());
        env.insert("nativeBuildInputs".to_string(), native.join(" "));
        if let Some(s) = system {
            env.insert("system".to_string(), s.to_string());
        }
        RawDerivation { env, ..Default::default() }
    }

    const G9103: &str = "/nix/store/aaa-ghc-9.10.3";
    const G984B: &str = "/nix/store/bbb-ghc-binary-9.8.4";

    // ---- T005: the release parser ----

    #[test]
    fn a_ghc_path_names_its_release() {
        let g = GhcRelease::from_store_path(G9103).unwrap();
        assert_eq!((g.version.as_str(), g.bootstrap, g.label.as_str()), ("9.10.3", false, "ghc-9.10.3"));
    }

    #[test]
    fn a_bootstrap_path_names_its_release() {
        let g = GhcRelease::from_store_path(G984B).unwrap();
        assert_eq!((g.version.as_str(), g.bootstrap, g.label.as_str()), ("9.8.4", true, "ghc-binary-9.8.4"));
    }

    #[test]
    fn a_path_without_an_exact_release_names_none() {
        for p in [
            "/nix/store/aaa-ghc-9.10.3-doc",
            "/nix/store/aaa-ghc-settings-edit",
            "/nix/store/aaa-ghc-9.10",
            "/nix/store/aaa-gcc-13.2.0",
        ] {
            assert_eq!(GhcRelease::from_store_path(p), None, "{p}");
        }
    }

    // ---- T007: ClosureCompilers ----

    #[test]
    fn a_members_compiler_comes_from_its_native_build_inputs() {
        let lib = drv("waybill-fixture-lib", "1.0", &["/nix/store/x-pkg-config-0.29", G9103], None);
        let tool = drv("waybill-fixture-tool", "2.0", &[G984B], None);
        let cc = ClosureCompilers::from_derivations([&lib, &tool].into_iter(), None, "x86_64-linux");
        let labels = |k: (&str, &str)| -> Vec<String> {
            cc.by_member[&(k.0.to_string(), k.1.to_string())].iter().map(|g| g.label.clone()).collect()
        };
        assert_eq!(labels(("waybill-fixture-lib", "1.0")), ["ghc-9.10.3"]);
        assert_eq!(labels(("waybill-fixture-tool", "2.0")), ["ghc-binary-9.8.4"]);
    }

    #[test]
    fn a_member_naming_two_ghcs_gets_none() {
        let both = drv("waybill-fixture-lib", "1.0", &[G9103, G984B], None);
        let cc = ClosureCompilers::from_derivations([&both].into_iter(), None, "x86_64-linux");
        assert!(cc.by_member.is_empty(), "{:?}", cc.by_member);
    }

    #[test]
    fn build_variants_built_by_different_ghcs_keep_both() {
        // nixpkgs builds some packages twice under one identity, once with the
        // bootstrap compiler; the member links each one's boot libraries.
        let a = drv("waybill-fixture-lib", "1.0", &[G9103], None);
        let b = drv("waybill-fixture-lib", "1.0", &[G984B], None);
        let cc = ClosureCompilers::from_derivations([&a, &b].into_iter(), None, "x86_64-linux");
        let got: Vec<&str> = cc.by_member[&("waybill-fixture-lib".into(), "1.0".into())]
            .iter()
            .map(|g| g.label.as_str())
            .collect();
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(got.contains(&"ghc-9.10.3") && got.contains(&"ghc-binary-9.8.4"), "{got:?}");
        assert_eq!(cc.members_in_table("waybill-fixture-lib", "1.0").len(), 2);
    }

    #[test]
    fn a_variant_naming_two_ghcs_poisons_its_member_in_either_order() {
        let one = drv("waybill-fixture-lib", "1.0", &[G9103], None);
        let both = drv("waybill-fixture-lib", "1.0", &[G9103, G984B], None);
        for order in [[&one, &both], [&both, &one]] {
            let cc = ClosureCompilers::from_derivations(order.into_iter(), None, "x86_64-linux");
            assert!(cc.by_member.is_empty(), "{:?}", cc.by_member);
        }
    }

    #[test]
    fn a_member_with_any_compiler_absent_from_the_table_keeps_todays_path() {
        let a = drv("waybill-fixture-lib", "1.0", &[G9103], None);
        let b = drv("waybill-fixture-lib", "1.0", &["/nix/store/ddd-ghc-99.0.1"], None);
        let cc = ClosureCompilers::from_derivations([&a, &b].into_iter(), None, "x86_64-linux");
        assert!(cc.members_in_table("waybill-fixture-lib", "1.0").is_empty());
    }

    #[test]
    fn the_project_compiler_is_the_evaluated_derivations() {
        let app = drv("waybill-fixture-app", "0.1", &[G9103], Some("x86_64-linux"));
        let tool = drv("waybill-fixture-tool", "2.0", &[G984B], None);
        let cc = ClosureCompilers::from_derivations([&app, &tool].into_iter(), Some(&app), "aarch64-darwin");
        assert_eq!(cc.project.unwrap().label, "ghc-9.10.3");
        // The evaluated derivation's own system wins over the fallback.
        assert_eq!(cc.system, "x86_64-linux");
    }

    #[test]
    fn the_system_falls_back_to_the_detected_one() {
        let app = drv("waybill-fixture-app", "0.1", &[G9103], None);
        let cc = ClosureCompilers::from_derivations([&app].into_iter(), Some(&app), "aarch64-darwin");
        assert_eq!(cc.system, "aarch64-darwin");
        let cc = ClosureCompilers::from_derivations([&app].into_iter(), None, "aarch64-darwin");
        assert_eq!(cc.system, "aarch64-darwin");
    }

    #[test]
    fn without_an_evaluated_compiler_the_single_non_bootstrap_ghc_is_the_project_compiler() {
        let wrapper = drv("waybill-fixture-env", "0.1", &[], None);
        let lib = drv("waybill-fixture-lib", "1.0", &[G9103], None);
        let tool = drv("waybill-fixture-tool", "2.0", &[G984B], None);
        let cc = ClosureCompilers::from_derivations([&wrapper, &lib, &tool].into_iter(), Some(&wrapper), "x86_64-linux");
        assert_eq!(cc.project.unwrap().label, "ghc-9.10.3");
    }

    #[test]
    fn two_non_bootstrap_ghcs_and_no_evaluated_compiler_name_no_project_compiler() {
        let a = drv("waybill-fixture-a", "1.0", &[G9103], None);
        let b = drv("waybill-fixture-b", "1.0", &["/nix/store/ccc-ghc-9.6.6"], None);
        let cc = ClosureCompilers::from_derivations([&a, &b].into_iter(), None, "x86_64-linux");
        assert_eq!(cc.project, None);
    }

    #[test]
    fn seen_collects_every_distinct_compiler() {
        let a = drv("waybill-fixture-a", "1.0", &[G9103], None);
        let b = drv("waybill-fixture-b", "1.0", &[G9103], None);
        let t = drv("waybill-fixture-tool", "2.0", &[G984B], None);
        let cc = ClosureCompilers::from_derivations([&a, &b, &t].into_iter(), Some(&a), "x86_64-linux");
        let labels: Vec<&str> = cc.seen.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, vec!["ghc-9.10.3", "ghc-binary-9.8.4"]);
    }

    #[test]
    fn the_document_value_says_which_compilers_the_table_has() {
        let a = drv("waybill-fixture-a", "1.0", &[G9103], None);
        let z = drv("waybill-fixture-z", "1.0", &["/nix/store/zzz-ghc-99.0.1"], None);
        let cc = ClosureCompilers::from_derivations([&a, &z].into_iter(), Some(&a), "x86_64-linux");
        assert_eq!(
            cc.document_value("2026-11-12").to_string(),
            r#"{"compilers":{"ghc-9.10.3":"in-table","ghc-99.0.1":"absent-from-table"},"project":"ghc-9.10.3","schema":1,"system":"x86_64-linux","table":"2026-11-12"}"#
        );
    }
}
