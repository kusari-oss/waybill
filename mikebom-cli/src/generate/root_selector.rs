//! Milestone 127 — smarter BOM-subject root selection.
//!
//! See `specs/127-smarter-root-pick/` for the full spec, plan, and
//! data model. Closes #366 (polyglot Go-vs-Maven priority) and
//! #367 (multi-module Go workspace root selection).
//!
//! Today the per-format `metadata.component` / `documentDescribes` /
//! `rootElement` selection in `generate/cyclonedx/metadata.rs`,
//! `generate/spdx/document.rs`, and `generate/spdx/v3_document.rs`
//! each carries its own inline priority ladder. When multiple
//! main-module-tagged components exist (`mikebom:component-role:
//! "main-module"`), the existing ladder falls through to either the
//! Maven `scan_target_coord` branch or a `pkg:generic/<target>@0.0.0`
//! placeholder. This is wrong for two reproducible bug classes:
//!
//! - **#366** — polyglot repos where Go + Maven (+ npm) coexist:
//!   today the Maven `scan_target_coord` wins over the Go main-module.
//! - **#367** — multi-module Go workspaces where 50+ nested `go.mod`
//!   files exist: today the count-1 fast path can't fire and the
//!   ladder picks an alphabetic-leaf submodule via the synthetic
//!   placeholder branch.
//!
//! This module is the single source of truth for the new ladder.
//! All three format emitters call into [`select_root`].

use std::path::PathBuf;

use mikebom_common::resolution::ResolvedComponent;
use mikebom_common::types::purl::Purl;

use super::RootComponentOverride;
use crate::scan_fs::package_db::maven::ScanTargetCoord;

/// Annotation key carrying the main-module role (set by every
/// per-ecosystem main-module emitter).
const COMPONENT_ROLE_KEY: &str = "mikebom:component-role";
const MAIN_MODULE_ROLE: &str = "main-module";

/// Annotation key set by [`crate::scan_fs::scan_path`] after readers
/// complete. Carries a `serde_json::Value::Bool`.
pub const IS_WORKSPACE_ROOT_KEY: &str = "mikebom:is-workspace-root";

/// FR-003 ecosystem-priority order. Fixed at compile time per
/// research R2. Operators wanting a different order use
/// `--root-name`/`--root-purl-type` per FR-008.
const ECOSYSTEM_PRIORITY: &[&str] = &[
    "golang", "cargo", "maven", "npm", "pip", "gem", "generic",
];

/// One of five enum variants produced by the new selection ladder.
///
/// The two implicit cases (count==1 fast path AND operator override)
/// NEVER produce a [`RootSelectionHeuristic`] — they're handled
/// out-of-band so the existing single-main-module fast path stays
/// byte-identical (SC-003) and the milestone-077 override audit
/// channel remains the right surface for operator-supplied identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSelectionHeuristic {
    /// FR-002 — exactly one main-module has `is_workspace_root == true`.
    RepoRoot,
    /// FR-003 — multiple main-modules at the repo root; the fixed
    /// ecosystem-priority order picks one.
    EcosystemPriority,
    /// FR-004 — no main-module at the repo root; the longest common
    /// path prefix of main-module manifest paths matches exactly one.
    LongestCommonPrefix,
    /// Fallback to the existing Maven JAR-walker `scan_target_coord`.
    MavenScanTargetCoord,
    /// Fallback to `pkg:generic/<target>@0.0.0`.
    SyntheticPlaceholder,
}

impl RootSelectionHeuristic {
    /// Stable string emitted in the annotation `heuristic` field.
    pub fn name(&self) -> &'static str {
        match self {
            Self::RepoRoot => "repo-root-main-module",
            Self::EcosystemPriority => "ecosystem-priority",
            Self::LongestCommonPrefix => "longest-common-prefix",
            Self::MavenScanTargetCoord => "maven-scan-target-coord",
            Self::SyntheticPlaceholder => "synthetic-placeholder",
        }
    }

    /// Fixed confidence per heuristic. Modeled on mikebom's existing
    /// CDX `evidence.identity.confidence` channel. Always in `[0.0, 1.0]`.
    pub fn confidence(&self) -> f64 {
        match self {
            Self::RepoRoot => 0.95,
            Self::LongestCommonPrefix => 0.80,
            Self::EcosystemPriority => 0.70,
            Self::MavenScanTargetCoord => 0.60,
            Self::SyntheticPlaceholder => 0.30,
        }
    }
}

/// The elected BOM subject. The selector returns a typed enum rather
/// than a flat `Purl` so the per-format emitters can introspect the
/// kind of subject (e.g., to populate `bom-ref` vs. SPDXID).
#[derive(Debug, Clone)]
pub enum ResolvedRootSubject {
    /// Index into the slice passed to [`select_root`].
    MainModule(usize),
    /// Maven coord synthesized by the JAR walker. The caller pulls
    /// the actual coord from `scan_target_coord` (it's identical to
    /// the input).
    MavenCoord,
    /// `pkg:generic/<target>@0.0.0` placeholder.
    SyntheticPlaceholder { name: String, version: String },
    /// Operator override (milestone 077 + #358). The selector
    /// short-circuits at the top of the ladder when override is
    /// active; emitters then read `RootComponentOverride` for the
    /// full per-format expansion.
    OperatorOverride,
}

/// Output of [`select_root`].
#[derive(Debug, Clone)]
pub struct RootSelectionResult {
    pub subject: ResolvedRootSubject,
    /// `None` when the count==1 fast path OR operator override fired.
    /// `Some(h)` when one of the five new ladder branches fired —
    /// drives the document-scope annotation per FR-006.
    pub heuristic: Option<RootSelectionHeuristic>,
    /// PURLs of every main-module-tagged component that did NOT win
    /// when the ladder fell through past at least one detected
    /// main-module. Drives the FR-007 warning. Empty when:
    /// (a) count==1 fast path fired (no loser),
    /// (b) override fired (no loser),
    /// (c) zero main-modules detected (nothing to lose).
    pub losers: Vec<Purl>,
}

/// Public entry point implementing FR-002..FR-004 + FR-008 + FR-009.
///
/// Ladder order (topmost wins):
///
/// 1. **Operator override** ([`RootComponentOverride::is_active`]) →
///    `OperatorOverride` subject, no heuristic annotation. FR-008.
/// 2. **Single main-module fast path** (exactly one main-module
///    component) → `MainModule(idx)`, no heuristic annotation
///    (preserves byte-identity per FR-009). This is the
///    pre-milestone-127 behavior, unchanged.
/// 3. **FR-002 repo-root tiebreaker** — exactly one main-module has
///    `mikebom:is-workspace-root == true` → `MainModule(idx)`,
///    heuristic = `RepoRoot` (confidence 0.95).
/// 4. **FR-003 ecosystem-priority** — multiple main-modules have
///    `is_workspace_root == true`; pick by [`ECOSYSTEM_PRIORITY`] →
///    heuristic = `EcosystemPriority` (confidence 0.70).
/// 5. **FR-004 longest common path prefix** — zero main-modules have
///    `is_workspace_root == true`; LCP of all main-module manifest
///    paths matches exactly one → heuristic = `LongestCommonPrefix`
///    (confidence 0.80).
/// 6. **Maven `scan_target_coord` branch** — fall through with
///    `scan_target_coord.is_some()` → heuristic =
///    `MavenScanTargetCoord` (confidence 0.60).
/// 7. **Synthetic placeholder** — last resort
///    `pkg:generic/<target>@0.0.0` → heuristic =
///    `SyntheticPlaceholder` (confidence 0.30).
///
/// `losers` is populated for branches 4–7 with the PURLs of
/// main-modules NOT picked (FR-007 warning surface). Empty for
/// branches 1, 2, and 3.
pub fn select_root(
    components: &[ResolvedComponent],
    root_override: &RootComponentOverride,
    scan_target_coord: Option<&ScanTargetCoord>,
    target_name: &str,
    target_version: &str,
) -> RootSelectionResult {
    // Ladder branch 1 — operator override.
    if root_override.is_active() {
        return RootSelectionResult {
            subject: ResolvedRootSubject::OperatorOverride,
            heuristic: None,
            losers: Vec::new(),
        };
    }

    // Collect main-module-tagged components (indexed for stable refs).
    let main_modules: Vec<usize> = components
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            c.extra_annotations
                .get(COMPONENT_ROLE_KEY)
                .and_then(|v| v.as_str())
                == Some(MAIN_MODULE_ROLE)
        })
        .map(|(i, _)| i)
        .collect();

    // Ladder branch 2 — count==1 fast path (byte-identity preserved).
    if main_modules.len() == 1 {
        return RootSelectionResult {
            subject: ResolvedRootSubject::MainModule(main_modules[0]),
            heuristic: None,
            losers: Vec::new(),
        };
    }

    // No main-modules detected — go straight to Maven coord or synthetic
    // placeholder, no losers (nothing to lose).
    if main_modules.is_empty() {
        return if scan_target_coord.is_some() {
            RootSelectionResult {
                subject: ResolvedRootSubject::MavenCoord,
                heuristic: Some(RootSelectionHeuristic::MavenScanTargetCoord),
                losers: Vec::new(),
            }
        } else {
            RootSelectionResult {
                subject: ResolvedRootSubject::SyntheticPlaceholder {
                    name: target_name.to_string(),
                    version: target_version.to_string(),
                },
                heuristic: Some(RootSelectionHeuristic::SyntheticPlaceholder),
                losers: Vec::new(),
            }
        };
    }

    // 2+ main-modules. Partition by is_workspace_root and try
    // each tiebreaker in order.
    let workspace_root_modules: Vec<usize> = main_modules
        .iter()
        .copied()
        .filter(|&i| is_workspace_root(&components[i]))
        .collect();

    // Helper: compute losers as all main-modules except the picked one.
    let losers = |picked: usize| -> Vec<Purl> {
        main_modules
            .iter()
            .copied()
            .filter(|&i| i != picked)
            .map(|i| components[i].purl.clone())
            .collect()
    };

    // Ladder branch 3 — FR-002 repo-root tiebreaker.
    if workspace_root_modules.len() == 1 {
        let picked = workspace_root_modules[0];
        return RootSelectionResult {
            subject: ResolvedRootSubject::MainModule(picked),
            heuristic: Some(RootSelectionHeuristic::RepoRoot),
            losers: losers(picked),
        };
    }

    // Ladder branch 4 — FR-003 ecosystem-priority among workspace-root
    // main-modules.
    if workspace_root_modules.len() > 1 {
        if let Some(picked) = pick_by_ecosystem(&workspace_root_modules, components) {
            return RootSelectionResult {
                subject: ResolvedRootSubject::MainModule(picked),
                heuristic: Some(RootSelectionHeuristic::EcosystemPriority),
                losers: losers(picked),
            };
        }
    }

    // Ladder branch 5 — FR-004 longest common path prefix among ALL
    // main-modules (only reached when workspace_root_modules is empty).
    if workspace_root_modules.is_empty() {
        if let Some(picked) = pick_by_lcp(&main_modules, components) {
            return RootSelectionResult {
                subject: ResolvedRootSubject::MainModule(picked),
                heuristic: Some(RootSelectionHeuristic::LongestCommonPrefix),
                losers: losers(picked),
            };
        }
    }

    // Ladder branches 6 + 7 — fall through. `losers` here includes
    // every main-module (none was picked), so FR-007 fires.
    let all_losers: Vec<Purl> = main_modules
        .iter()
        .copied()
        .map(|i| components[i].purl.clone())
        .collect();

    if scan_target_coord.is_some() {
        RootSelectionResult {
            subject: ResolvedRootSubject::MavenCoord,
            heuristic: Some(RootSelectionHeuristic::MavenScanTargetCoord),
            losers: all_losers,
        }
    } else {
        RootSelectionResult {
            subject: ResolvedRootSubject::SyntheticPlaceholder {
                name: target_name.to_string(),
                version: target_version.to_string(),
            },
            heuristic: Some(RootSelectionHeuristic::SyntheticPlaceholder),
            losers: all_losers,
        }
    }
}

/// Read the `mikebom:is-workspace-root` annotation. Absent or
/// non-bool → `false` (degrades gracefully per FR-001 contract).
fn is_workspace_root(c: &ResolvedComponent) -> bool {
    c.extra_annotations
        .get(IS_WORKSPACE_ROOT_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// FR-003 — pick by `ECOSYSTEM_PRIORITY`. Returns the FIRST main-module
/// (by `main_modules` slice order) whose PURL ecosystem matches the
/// earliest entry in `ECOSYSTEM_PRIORITY`. Returns `None` only when
/// the slice is empty (caller guarantees non-empty in practice).
fn pick_by_ecosystem(
    candidates: &[usize],
    components: &[ResolvedComponent],
) -> Option<usize> {
    for &priority_eco in ECOSYSTEM_PRIORITY {
        for &i in candidates {
            if components[i].purl.ecosystem() == priority_eco {
                return Some(i);
            }
        }
    }
    // Fall-back: pick first by slice order (deterministic).
    candidates.first().copied()
}

/// FR-004 — pick by longest common path prefix of main-module manifest
/// paths. Returns `Some(idx)` when exactly one main-module's manifest
/// path equals the LCP. Returns `None` when zero or multiple
/// main-modules match — ladder falls through.
fn pick_by_lcp(
    main_modules: &[usize],
    components: &[ResolvedComponent],
) -> Option<usize> {
    if main_modules.len() < 2 {
        return main_modules.first().copied();
    }

    // Collect manifest paths (the first source_files entry per FR-001
    // contract — main-modules emit their defining manifest file there).
    // Ecosystems vary in where they record the path: Go reader uses
    // `evidence.source_file_paths`; workspace-synthesizer + Swift /
    // Kotlin / NuGet readers use the `mikebom:source-files` annotation
    // (string OR array). Read both, prefer evidence.
    let manifest_paths: Vec<(usize, PathBuf)> = main_modules
        .iter()
        .filter_map(|&i| {
            let comp = &components[i];
            let from_evidence = comp.evidence.source_file_paths.first().cloned();
            let from_annotation = comp
                .extra_annotations
                .get("mikebom:source-files")
                .and_then(|v| match v {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Array(arr) => arr
                        .first()
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string()),
                    _ => None,
                });
            let first_src = from_evidence.or(from_annotation)?;
            Some((i, PathBuf::from(first_src)))
        })
        .collect();

    if manifest_paths.len() < main_modules.len() {
        // At least one main-module lacks the source-files annotation —
        // can't compute LCP deterministically. Fall through.
        return None;
    }

    // Compute LCP component-wise over the manifest paths' PARENT
    // directories (the manifest itself is e.g. `/p/sub/go.mod`, the
    // module's directory is `/p/sub/`).
    let dirs: Vec<PathBuf> = manifest_paths
        .iter()
        .map(|(_, p)| p.parent().map(|d| d.to_path_buf()).unwrap_or_default())
        .collect();

    let lcp = longest_common_path_prefix(&dirs);

    // Pick the main-module whose directory equals the LCP. If none or
    // multiple match, return None.
    let matches: Vec<usize> = manifest_paths
        .iter()
        .filter_map(|(i, _)| {
            let dir = dirs[manifest_paths.iter().position(|(j, _)| j == i)?].clone();
            if dir == lcp {
                Some(*i)
            } else {
                None
            }
        })
        .collect();

    if matches.len() == 1 {
        Some(matches[0])
    } else {
        None
    }
}

/// Compute the longest common path prefix across `paths`.
/// Component-wise comparison; returns the longest prefix all paths share.
fn longest_common_path_prefix(paths: &[PathBuf]) -> PathBuf {
    if paths.is_empty() {
        return PathBuf::new();
    }
    let mut prefix: Vec<std::ffi::OsString> =
        paths[0].components().map(|c| c.as_os_str().to_os_string()).collect();
    for p in &paths[1..] {
        let comps: Vec<std::ffi::OsString> =
            p.components().map(|c| c.as_os_str().to_os_string()).collect();
        let common_len = prefix
            .iter()
            .zip(comps.iter())
            .take_while(|(a, b)| a == b)
            .count();
        prefix.truncate(common_len);
    }
    let mut out = PathBuf::new();
    for c in &prefix {
        out.push(c);
    }
    out
}

/// Annotation keys that drive internal selection logic and MUST NOT
/// appear in serialized SBOM output. Filtered at every per-format
/// `extra_annotations` iteration site (CDX builder, SPDX 2.3 + 3
/// annotations) so the count==1 fast path stays byte-identical to
/// pre-milestone-127 emission for every fixture in
/// `mikebom-cli/tests/fixtures/golden/`.
pub fn is_internal_emission_key(key: &str) -> bool {
    key == IS_WORKSPACE_ROOT_KEY
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use mikebom_common::resolution::{
        ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
    };
    use mikebom_common::types::purl::Purl;
    use std::collections::BTreeMap;

    fn make_main_module(
        purl_str: &str,
        manifest_path: &str,
        is_workspace_root_flag: bool,
    ) -> ResolvedComponent {
        let purl = Purl::new(purl_str).expect("valid purl");
        let mut extra_annotations: BTreeMap<String, serde_json::Value> =
            BTreeMap::new();
        extra_annotations.insert(
            COMPONENT_ROLE_KEY.to_string(),
            serde_json::Value::String(MAIN_MODULE_ROLE.to_string()),
        );
        extra_annotations.insert(
            IS_WORKSPACE_ROOT_KEY.to_string(),
            serde_json::Value::Bool(is_workspace_root_flag),
        );
        extra_annotations.insert(
            "mikebom:source-files".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String(
                manifest_path.to_string(),
            )]),
        );
        ResolvedComponent {
            build_inclusion: None,
            name: purl.name().to_string(),
            version: purl.version().unwrap_or("0.0.0").to_string(),
            purl,
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::PackageDatabase,
                confidence: 0.85,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: Vec::new(),
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            requirement_range: None,
            source_type: None,
            sbom_tier: None,
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
            external_references: Vec::new(),
            extra_annotations,
            binary_role: None,
        }
    }

    fn no_override() -> RootComponentOverride {
        RootComponentOverride::default()
    }

    #[test]
    fn single_main_module_fast_path_no_heuristic_no_losers() {
        let comps = vec![make_main_module(
            "pkg:golang/example.com/single@v1.0.0",
            "/p/go.mod",
            true,
        )];
        let result = select_root(
            &comps,
            &no_override(),
            None,
            "target",
            "0.0.0",
        );
        assert!(matches!(result.subject, ResolvedRootSubject::MainModule(0)));
        assert!(result.heuristic.is_none());
        assert!(result.losers.is_empty());
    }

    #[test]
    fn override_always_wins_no_heuristic() {
        let comps = vec![
            make_main_module("pkg:golang/example.com/a@v1.0.0", "/p/a/go.mod", false),
            make_main_module("pkg:golang/example.com/b@v1.0.0", "/p/b/go.mod", false),
        ];
        let over = RootComponentOverride {
            name: Some("overridden".to_string()),
            ..Default::default()
        };
        let result = select_root(
            &comps,
            &over,
            None,
            "target",
            "0.0.0",
        );
        assert!(matches!(result.subject, ResolvedRootSubject::OperatorOverride));
        assert!(result.heuristic.is_none());
        assert!(result.losers.is_empty());
    }

    #[test]
    fn repo_root_tiebreaker_picks_unique_workspace_root_module() {
        let comps = vec![
            make_main_module("pkg:golang/example.com/root@v1.0.0", "/p/go.mod", true),
            make_main_module(
                "pkg:golang/example.com/sub/a@v1.0.0",
                "/p/sub/a/go.mod",
                false,
            ),
            make_main_module(
                "pkg:golang/example.com/sub/b@v1.0.0",
                "/p/sub/b/go.mod",
                false,
            ),
        ];
        let result = select_root(
            &comps,
            &no_override(),
            None,
            "target",
            "0.0.0",
        );
        assert!(matches!(result.subject, ResolvedRootSubject::MainModule(0)));
        assert_eq!(result.heuristic, Some(RootSelectionHeuristic::RepoRoot));
        assert_eq!(result.losers.len(), 2);
    }

    #[test]
    fn ecosystem_priority_picks_golang_over_maven_at_repo_root() {
        let comps = vec![
            make_main_module(
                "pkg:maven/example.com/java@1.0",
                "/p/pom.xml",
                true,
            ),
            make_main_module("pkg:golang/example.com/go@v1.0", "/p/go.mod", true),
            make_main_module(
                "pkg:npm/example.com/ui@1.0",
                "/p/package.json",
                true,
            ),
        ];
        let result = select_root(
            &comps,
            &no_override(),
            None,
            "target",
            "0.0.0",
        );
        assert!(matches!(result.subject, ResolvedRootSubject::MainModule(1)));
        assert_eq!(
            result.heuristic,
            Some(RootSelectionHeuristic::EcosystemPriority)
        );
        assert_eq!(result.losers.len(), 2);
    }

    #[test]
    fn lcp_picks_unique_module_when_no_workspace_root() {
        // /p/services/api/go.mod and /p/services/api/sub/go.mod —
        // LCP is /p/services/api/; the first matches it exactly.
        let comps = vec![
            make_main_module(
                "pkg:golang/example.com/api@v1.0",
                "/p/services/api/go.mod",
                false,
            ),
            make_main_module(
                "pkg:golang/example.com/api/sub@v1.0",
                "/p/services/api/sub/go.mod",
                false,
            ),
        ];
        let result = select_root(
            &comps,
            &no_override(),
            None,
            "target",
            "0.0.0",
        );
        assert!(matches!(result.subject, ResolvedRootSubject::MainModule(0)));
        assert_eq!(
            result.heuristic,
            Some(RootSelectionHeuristic::LongestCommonPrefix)
        );
    }

    #[test]
    fn lcp_no_winner_falls_through_to_synthetic_placeholder() {
        // /p/services/api/go.mod and /p/services/worker/go.mod —
        // LCP is /p/services/, which neither manifest's parent equals.
        let comps = vec![
            make_main_module(
                "pkg:golang/example.com/api@v1.0",
                "/p/services/api/go.mod",
                false,
            ),
            make_main_module(
                "pkg:golang/example.com/worker@v1.0",
                "/p/services/worker/go.mod",
                false,
            ),
        ];
        let result = select_root(
            &comps,
            &no_override(),
            None,
            "target",
            "0.0.0",
        );
        match result.subject {
            ResolvedRootSubject::SyntheticPlaceholder { ref name, .. } => {
                assert_eq!(name, "target");
            }
            other => panic!("expected SyntheticPlaceholder, got {other:?}"),
        }
        assert_eq!(
            result.heuristic,
            Some(RootSelectionHeuristic::SyntheticPlaceholder)
        );
        assert_eq!(result.losers.len(), 2);
    }

    #[test]
    fn no_main_modules_with_maven_coord_picks_maven() {
        let comps: Vec<ResolvedComponent> = Vec::new();
        let coord = ScanTargetCoord {
            group: "com.ex".to_string(),
            artifact: "art".to_string(),
            version: "1.0".to_string(),
        };
        let result = select_root(
            &comps,
            &no_override(),
            Some(&coord),
            "target",
            "0.0.0",
        );
        assert!(matches!(result.subject, ResolvedRootSubject::MavenCoord));
        assert_eq!(
            result.heuristic,
            Some(RootSelectionHeuristic::MavenScanTargetCoord)
        );
        // No losers — no main-modules existed.
        assert!(result.losers.is_empty());
    }

    #[test]
    fn confidence_values_match_data_model() {
        assert_eq!(RootSelectionHeuristic::RepoRoot.confidence(), 0.95);
        assert_eq!(
            RootSelectionHeuristic::LongestCommonPrefix.confidence(),
            0.80
        );
        assert_eq!(
            RootSelectionHeuristic::EcosystemPriority.confidence(),
            0.70
        );
        assert_eq!(
            RootSelectionHeuristic::MavenScanTargetCoord.confidence(),
            0.60
        );
        assert_eq!(
            RootSelectionHeuristic::SyntheticPlaceholder.confidence(),
            0.30
        );
    }

    #[test]
    fn heuristic_names_match_contract() {
        assert_eq!(RootSelectionHeuristic::RepoRoot.name(), "repo-root-main-module");
        assert_eq!(
            RootSelectionHeuristic::EcosystemPriority.name(),
            "ecosystem-priority"
        );
        assert_eq!(
            RootSelectionHeuristic::LongestCommonPrefix.name(),
            "longest-common-prefix"
        );
        assert_eq!(
            RootSelectionHeuristic::MavenScanTargetCoord.name(),
            "maven-scan-target-coord"
        );
        assert_eq!(
            RootSelectionHeuristic::SyntheticPlaceholder.name(),
            "synthetic-placeholder"
        );
    }
}
