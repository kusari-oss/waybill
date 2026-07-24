# Data Model: `--project-discovery=<mode>`

**Feature**: 220-project-discovery-scope | **Date**: 2026-07-24

## E1 — `ProjectDiscoveryMode` enum

New public type in `waybill-cli/src/generate/project_discovery/mod.rs`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ProjectDiscoveryMode {
    /// Current behavior — every reader discovers main-modules
    /// wherever they find qualifying manifests. SC-005 byte-identity
    /// contract preserved.
    #[default]
    All,
    /// Discover only root-level main-modules + their ecosystem-native
    /// workspace-declared members (via the existing
    /// `waybill:workspace-member` annotation set by readers with
    /// workspace support). Independent nested projects dropped from
    /// the SBOM entirely (BFS-projection filter).
    RootOnly,
    /// Discover only root-level main-modules; ignore even ecosystem-
    /// native workspace-member declarations. Literal shallow — the
    /// SBOM contains root manifest(s) + their directly-declared deps
    /// only.
    Strict,
}
```

**Validation rules**:
- `Copy` valid because enum is 1 byte (3 variants).
- clap-parsed values are `"all"` / `"root-only"` / `"strict"` (kebab-case per `rename_all`).
- Invalid parse (`--project-discovery=nonexistent`) → clap emits stderr error listing accepted values.

## E2 — `ProjectDiscoveryMode::is_root_in_scope`

```rust
impl ProjectDiscoveryMode {
    /// Return whether `root` (a discovered main-module) is in-scope
    /// under this mode.
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
}

fn is_root_level(root: &SubprojectRoot, scan_root: &Path) -> bool {
    let source_dir_str = root.source_dir.to_string_lossy();
    if source_dir_str.is_empty() {
        return true; // subproject IS scan root
    }
    let source_canon = std::fs::canonicalize(&root.source_dir).ok();
    let scan_canon = std::fs::canonicalize(scan_root).ok();
    matches!((source_canon, scan_canon), (Some(a), Some(b)) if a == b)
}
```

**Validation rules**:
- Pure function; deterministic per `(root, scan_root)`.
- Never fails — falls back to `false` (conservative) if canonicalization fails.

## E3 — `ProjectDiscoveryMode::follows_workspace_members`

```rust
impl ProjectDiscoveryMode {
    /// Return whether workspace-declared members of in-scope roots
    /// should be walked. `All`/`RootOnly` → yes (US2 requirement).
    /// `Strict` → no.
    pub fn follows_workspace_members(&self) -> bool {
        !matches!(self, Self::Strict)
    }
}
```

## E4 — `ProjectDiscoveryMode::Display`

```rust
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
```

**Rationale**: Load-bearing for the FR-012 INFO log format `project-discovery=<mode>` — matches CLI wire form so operator-visible logs match jq queries. Same B1-remediation lesson from m219 (Debug form would render `RootOnly` instead of `root-only`).

## E5 — `ProjectDiscoveryReport` struct

Scan-scoped aggregate populated by the filter pass.

```rust
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
    pub fn all_default() -> Self {
        Self {
            mode: ProjectDiscoveryMode::All,
            root_main_modules: 0,
            workspace_members_followed: 0,
            nested_projects_ignored: 0,
        }
    }
}
```

**Validation rules**:
- `all_default()` returns zero counters — consumers know "All mode + zero counters" = "filter didn't run."
- Under non-default modes, all counters populated at filter time.

## E6 — `apply_scope_filter` function

Pure function; the entry point for the filter pass.

```rust
pub fn apply_scope_filter(
    components: Vec<ResolvedComponent>,
    relationships: Vec<Relationship>,
    mode: ProjectDiscoveryMode,
    scan_root: &Path,
) -> (Vec<ResolvedComponent>, Vec<Relationship>, ProjectDiscoveryReport) {
    // Fast path: no filter under All.
    if mode == ProjectDiscoveryMode::All {
        return (components, relationships, ProjectDiscoveryReport::all_default());
    }
    // Enumerate main-modules from the resolved-component set (reuses
    // m215 helper).
    let all_roots = crate::generate::split::enumerate_workspace_roots(&components, scan_root);
    let in_scope_roots: Vec<SubprojectRoot> = all_roots
        .iter()
        .filter(|r| mode.is_root_in_scope(r, scan_root))
        .cloned()
        .collect();
    let nested_projects_ignored = all_roots.len().saturating_sub(in_scope_roots.len());
    // BFS-project each in-scope root; union the reachable component-PURLs.
    let mut reachable: BTreeSet<String> = BTreeSet::new();
    for root in &in_scope_roots {
        let proj = crate::generate::split::project_for_root(root, &components, &relationships);
        for c in &proj.components {
            reachable.insert(c.purl.as_str().to_string());
        }
    }
    // Under RootOnly, additionally include workspace-member components
    // whose `waybill:workspace-member` annotation points at any
    // in-scope root's PURL (belt-and-suspenders for FR-004).
    let mut workspace_members_followed = 0usize;
    if mode.follows_workspace_members() {
        let root_purls: BTreeSet<String> = in_scope_roots.iter()
            .map(|r| r.purl_string.clone()).collect();
        for c in &components {
            if reachable.contains(c.purl.as_str()) { continue; }
            if let Some(v) = c.extra_annotations.get("waybill:workspace-member") {
                if let Some(root_ref) = v.as_str() {
                    if root_purls.contains(root_ref) {
                        reachable.insert(c.purl.as_str().to_string());
                        workspace_members_followed += 1;
                    }
                }
            }
        }
    }
    // Filter both slices.
    let filtered_components: Vec<ResolvedComponent> = components.into_iter()
        .filter(|c| reachable.contains(c.purl.as_str())).collect();
    let filtered_relationships: Vec<Relationship> = relationships.into_iter()
        .filter(|r| reachable.contains(&r.from) && reachable.contains(&r.to)).collect();
    let report = ProjectDiscoveryReport {
        mode,
        root_main_modules: in_scope_roots.len(),
        workspace_members_followed,
        nested_projects_ignored,
    };
    (filtered_components, filtered_relationships, report)
}
```

**Validation rules**:
- Deterministic; `All` mode is zero-cost.
- Component + relationship dedup preserves input ordering.
- Report counters populated only under non-default mode.

## E7 — CLI flag `pub project_discovery: ProjectDiscoveryMode` on `ScanArgs`

New field in `waybill-cli/src/cli/scan_cmd.rs::ScanArgs`.

```rust
/// Milestone 220 — cap main-module discovery scope. Accepts
/// `all` (default; current behavior), `root-only` (discover only
/// root-level main-modules + their ecosystem-native workspace-
/// declared members), `strict` (only the root manifest itself).
///
/// See `docs/reference/project-discovery.md` for the mode table +
/// interaction matrix vs `--split[=<mode>]`.
#[arg(
    long = "project-discovery",
    value_enum,
    default_value = "all",
    require_equals = true,
)]
pub project_discovery: crate::generate::project_discovery::ProjectDiscoveryMode,
```

**Validation rules**:
- `default_value = "all"` — every scan has a mode; omitted flag = All.
- `require_equals = true` — reject `--project-discovery root-only` (space); require `=`.
- No env var alias at the clap level; scan_cmd bridges to `WAYBILL_PROJECT_DISCOVERY` internally (mirrors m218 `WAYBILL_EXPERIMENTAL_CROSS_ECOSYSTEM_EDGES` pattern).

## E8 — `ScanArtifacts.project_discovery_mode` field

Threaded through `ScanArtifacts` per the m134/m173/m217/m218 propagation pattern.

```rust
/// Milestone 220 — mode the scan ran under. Consumed by the C140
/// doc-scope annotation emitter (silence-on-`All` per FR-011).
pub project_discovery_mode: Option<ProjectDiscoveryMode>,
```

**Validation rules**:
- `None` = flag was defaulted (`All`) → doc-scope annotation absent (SC-005 byte-identity).
- `Some(mode)` where mode != `All` → doc-scope annotation `waybill:project-discovery-mode = <mode>` emitted.
- `Some(All)` should never occur (scan_cmd only populates the field when mode is non-default).

## E9 — Parity catalog row C140

`waybill:project-discovery-mode` — doc-scope, string-valued.

- CDX extractor: `c140_cdx` — pattern-matches m217 `c136_cdx` verbatim.
- SPDX 2.3 extractor: `c140_spdx23` — pattern-matches m217 `c136_spdx23`.
- SPDX 3 extractor: `c140_spdx3` — pattern-matches m217 `c136_spdx3`.
- `EXTRACTORS` row registered after C139 (m218's last row).
- KEEP-NO-NATIVE audit documented per Principle V.

## State Transitions

None — all data is scan-lifetime in-process. No state machines, no persistence.

## Data Volume Assumptions

- **Root-level main-modules**: typically 1-3 per scan (polyglot roots).
- **Workspace-declared members**: typically 0-100 (large Cargo workspaces reach ~100 crates; npm workspaces similar).
- **Nested independent projects ignored**: highly variable — 0 on well-scoped scans; 10-50 on monorepo-root scans.
- **BFS-projection cost**: dominated by `project_for_root` per m215 — O(V + E) per root. Under root-only with 2 roots + 10k total components + 20k relationships → ~40k ops. Trivial (<10ms).
