//! Milestone 220 — `--project-discovery=<mode>` scope cap.
//!
//! Post-discovery filter over `Vec<ResolvedComponent>` +
//! `Vec<Relationship>`. Readers walk + discover as they do today
//! (SC-005 byte-identity contract preserved for `All`); this module
//! runs after `enumerate_workspace_roots` populates the main-module
//! set and drops out-of-scope main-modules + their unreachable
//! transitive components under `RootOnly` / `Strict`.
//!
//! Under `RootOnly` an additional annotation-based follow-up pass
//! retains workspace-declared members (identified via the existing
//! m127-era `waybill:workspace-member` annotation set by cargo /
//! npm / go / maven readers). A fixpoint recursion pulls in the
//! members of any inner workspace whose root is itself a member of
//! the outer workspace (FR-005). Under `Strict` this pass is
//! skipped — workspace members are dropped even if annotated.
//!
//! See `specs/220-project-discovery-scope/` for spec / plan /
//! contracts.

use std::path::{Path, PathBuf};

use super::split::SubprojectRoot;

pub mod filter;

/// Milestone 220 — cap main-module discovery scope. Extensibility
/// contract mirrors m219 `SplitMode`: adding a variant touches this
/// enum, the `is_root_in_scope` + `follows_workspace_members` match
/// arms, the docs page mode table, and (optionally) a new test
/// scenario. Zero touches to CLI flag parsing, filter pipeline,
/// doc-scope annotation, or FR-012 INFO log — the enum's method
/// surface abstracts the mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ProjectDiscoveryMode {
    /// Current behavior — every reader discovers main-modules wherever
    /// they find qualifying manifests. SC-005 byte-identity contract:
    /// `--project-discovery=all` (or the flag omitted entirely)
    /// produces byte-identical output to alpha.68 on every existing
    /// test fixture.
    #[default]
    All,
    /// Discover only root-level main-modules + their ecosystem-native
    /// workspace-declared members (via the existing
    /// `waybill:workspace-member` annotation set by cargo / npm / go /
    /// maven readers today). Independent nested projects are dropped
    /// from the SBOM entirely.
    RootOnly,
    /// Discover only root-level main-modules; ignore even ecosystem-
    /// native workspace-member declarations. Literal shallow — the
    /// SBOM contains root manifest(s) + directly-declared deps only.
    Strict,
}

impl ProjectDiscoveryMode {
    /// Return whether `root` (a discovered main-module) is in-scope
    /// under this mode. Pure function; deterministic per input.
    pub fn is_root_in_scope(
        &self,
        root: &SubprojectRoot,
        scan_root: &Path,
    ) -> bool {
        match self {
            Self::All => true,
            Self::RootOnly | Self::Strict => is_root_level(root, scan_root),
        }
    }

    /// Return whether workspace-declared members of in-scope roots
    /// should be walked. `All` + `RootOnly` → yes (US2 requirement).
    /// `Strict` → no.
    pub fn follows_workspace_members(&self) -> bool {
        !matches!(self, Self::Strict)
    }
}

/// Display renders the lowercase `ValueEnum` wire form (`all` /
/// `root-only` / `strict`). Load-bearing for the FR-012 INFO log —
/// the log emits via `%mode` (Display) so the operator-visible
/// substring is `mode=root-only` (matching consumer-facing CLI
/// spelling + jq queries). `?mode` (Debug) would render `RootOnly`
/// (capitalized), breaking the SC-009 test. Same B1-remediation
/// lesson from m219.
impl std::fmt::Display for ProjectDiscoveryMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use clap::ValueEnum;
        f.write_str(
            self.to_possible_value()
                .expect("ProjectDiscoveryMode variants all have possible values via ValueEnum derive")
                .get_name(),
        )
    }
}

/// Spec 1652 (research R1): the main module's directory, relative to the
/// scan root.
///
/// Spec 1652 added a file test here because `split::source_dir_for` stripped
/// the file name only for names `is_manifest_basename` lists, which has no
/// `package.yaml` or `*.cabal`, so every Haskell repository fell back to a
/// full scan under `root-only` and `strict` (8 of 8 corpus targets). Spec 1667
/// (#1691) moved the rule into [`crate::generate::split::subproject_dir`],
/// which `source_dir_for` now uses too, so `source_dir` is already a directory.
/// This stays a wrapper so a `SubprojectRoot` built elsewhere still resolves.
pub(crate) fn root_dir(root: &SubprojectRoot, scan_root: &Path) -> PathBuf {
    crate::generate::split::subproject_dir(&root.source_dir, scan_root)
}

/// A main-module is "root-level" iff its directory ([`root_dir`]) is either
/// empty (subproject IS scan root) OR canonicalizes to `scan_root`.
/// `canonicalize` failure on either side degrades to `false`
/// (conservative — fewer false-positives on broken paths). Matches
/// the permissive posture of milestone-114 `safe_walk`.
fn is_root_level(root: &SubprojectRoot, scan_root: &Path) -> bool {
    let dir = root_dir(root, scan_root);
    if dir.as_os_str().is_empty() {
        return true;
    }
    // The stored source_dir is scan-root-relative; join back with the
    // scan_root before canonicalizing so we can compare filesystem
    // identity (m215 `source_dir_for` at split.rs:214 relativizes).
    let joined = scan_root.join(&dir);
    let source_canon = std::fs::canonicalize(&joined).ok();
    let scan_canon = std::fs::canonicalize(scan_root).ok();
    matches!((source_canon, scan_canon), (Some(a), Some(b)) if a == b)
}

/// Scan-scoped aggregate populated by [`filter::apply_scope_filter`].
/// Feeds the FR-011 doc-scope C140 annotation + the FR-012 INFO log.
#[derive(Debug, Clone)]
pub struct ProjectDiscoveryReport {
    /// The mode the scan actually ran under.
    pub mode: ProjectDiscoveryMode,
    /// Number of root-level main-modules discovered + retained.
    pub root_main_modules: usize,
    /// Number of workspace-declared-member components followed via
    /// their `waybill:workspace-member` annotation. 0 under
    /// `Strict`; 0 under `All` (no filtering happens).
    pub workspace_members_followed: usize,
    /// Number of main-modules that WOULD have been in the SBOM under
    /// `All` mode but were dropped under the current mode. This is
    /// the operator-visible signal of "how much did the scope cap
    /// actually change." 0 under `All`.
    ///
    /// Spec 1652 (R3): always `dropped_main_modules.len()`. It used to be
    /// the count of roots out of scope, which also counted nested main
    /// modules the projection then kept through an edge (go-opentelemetry:
    /// 28 reported, 25 dropped).
    pub nested_projects_ignored: usize,
    /// Spec 1652 (FR-004): the PURLs of the main modules that were dropped,
    /// sorted. Computed after filtering, so a nested main module the root
    /// reaches is never in it. Empty under `All` and on the #1045 fall-back.
    pub dropped_main_modules: Vec<String>,
    /// Spec 1652 (FR-006): the evidence paths of those main modules — their
    /// manifests, as each reader recorded them — sorted and unique. The scan
    /// matches them against C200's `manifests-read`.
    pub dropped_evidence_paths: Vec<String>,
}

/// Spec 1652 (FR-004, FR-005): what `root-only` / `strict` dropped, carried
/// from the scan to every emitter. Built only on the filtered branch and only
/// when something was dropped, so `All`, the #1045 fall-back and a scan that
/// dropped nothing carry `None` and emit nothing new.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDiscoveryDrop {
    /// The dropped main modules' PURLs, sorted. Never empty.
    pub main_modules: Vec<String>,
    /// Their PURL types, sorted and unique: the ecosystems the completeness
    /// reason names and CycloneDX `compositions[]` marks incomplete.
    pub ecosystems: Vec<String>,
}

impl ProjectDiscoveryDrop {
    pub fn from_report(report: &ProjectDiscoveryReport) -> Option<Self> {
        if report.dropped_main_modules.is_empty() {
            return None;
        }
        let ecosystems: std::collections::BTreeSet<String> = report
            .dropped_main_modules
            .iter()
            .filter_map(|p| waybill_common::types::purl::Purl::new(p).ok())
            .map(|p| p.ecosystem().to_string())
            .collect();
        Some(Self {
            main_modules: report.dropped_main_modules.clone(),
            ecosystems: ecosystems.into_iter().collect(),
        })
    }

    /// C244 `waybill:project-discovery-dropped`: a canonical JSON string.
    /// `serde_json`'s map is ordered here, so the keys sort.
    pub fn to_document_value(&self) -> String {
        serde_json::json!({
            "count": self.main_modules.len(),
            "main-modules": self.main_modules,
            "reason": "project-discovery-scope",
        })
        .to_string()
    }
}

impl ProjectDiscoveryReport {
    /// Default report for `All` mode — no filtering happened.
    /// Consumers reading "All mode + zero counters" know the filter
    /// pass short-circuited.
    pub fn all_default() -> Self {
        Self {
            mode: ProjectDiscoveryMode::All,
            root_main_modules: 0,
            workspace_members_followed: 0,
            nested_projects_ignored: 0,
            dropped_main_modules: Vec::new(),
            dropped_evidence_paths: Vec::new(),
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::types::purl::Purl;

    fn root_at(source_dir: &str) -> SubprojectRoot {
        let purl = Purl::new("pkg:hackage/waybill-fixture-root@0.1.0").unwrap();
        SubprojectRoot {
            purl_string: purl.to_string(),
            purl,
            source_dir: PathBuf::from(source_dir),
            ecosystem: "hackage".to_string(),
        }
    }

    fn tree() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("package.yaml"), "name: waybill-fixture-root\n").unwrap();
        std::fs::create_dir_all(t.path().join("local-packages/a")).unwrap();
        std::fs::write(t.path().join("local-packages/a/a.cabal"), "name: a\n").unwrap();
        std::fs::create_dir_all(t.path().join("crates/api")).unwrap();
        t
    }

    #[test]
    fn root_dir_of_empty_is_empty() {
        let t = tree();
        assert_eq!(root_dir(&root_at(""), t.path()), PathBuf::new());
    }

    #[test]
    fn root_dir_strips_a_root_manifest_file() {
        let t = tree();
        assert_eq!(root_dir(&root_at("package.yaml"), t.path()), PathBuf::new());
    }

    #[test]
    fn root_dir_strips_a_nested_manifest_file() {
        let t = tree();
        assert_eq!(
            root_dir(&root_at("local-packages/a/a.cabal"), t.path()),
            PathBuf::from("local-packages/a")
        );
    }

    #[test]
    fn root_dir_keeps_a_directory() {
        let t = tree();
        assert_eq!(root_dir(&root_at("crates/api"), t.path()), PathBuf::from("crates/api"));
    }

    #[test]
    fn root_dir_keeps_a_path_that_does_not_exist() {
        let t = tree();
        assert_eq!(root_dir(&root_at("gone/x"), t.path()), PathBuf::from("gone/x"));
    }

    /// The #1676 defect: a root `package.yaml` was never root-level.
    #[test]
    fn a_root_package_yaml_is_root_level() {
        let t = tree();
        assert!(is_root_level(&root_at("package.yaml"), t.path()));
        assert!(!is_root_level(&root_at("local-packages/a/a.cabal"), t.path()));
    }
}
