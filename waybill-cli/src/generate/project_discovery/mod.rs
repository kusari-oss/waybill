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

use std::path::Path;

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

/// A main-module is "root-level" iff its source-dir is either empty
/// (subproject IS scan root) OR canonicalizes to `scan_root`.
/// `canonicalize` failure on either side degrades to `false`
/// (conservative — fewer false-positives on broken paths). Matches
/// the permissive posture of milestone-114 `safe_walk`.
fn is_root_level(root: &SubprojectRoot, scan_root: &Path) -> bool {
    let source_dir_str = root.source_dir.to_string_lossy();
    if source_dir_str.is_empty() {
        return true;
    }
    // The stored source_dir is scan-root-relative; join back with the
    // scan_root before canonicalizing so we can compare filesystem
    // identity (m215 `source_dir_for` at split.rs:214 relativizes).
    let joined = scan_root.join(&root.source_dir);
    let source_canon = std::fs::canonicalize(&joined).ok();
    let scan_canon = std::fs::canonicalize(scan_root).ok();
    matches!((source_canon, scan_canon), (Some(a), Some(b)) if a == b)
}

/// Scan-scoped aggregate populated by [`filter::apply_scope_filter`].
/// Feeds the FR-011 doc-scope C140 annotation + the FR-012 INFO log.
#[derive(Debug, Clone, Copy)]
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
    pub nested_projects_ignored: usize,
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
        }
    }
}
