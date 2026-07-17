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
    // Milestone 201 (FR-007, closes #587): m201's new positive-
    // identifier annotation `mikebom:is-cargo-workspace-toplevel`
    // (stamped by cargo m064 emission, consumed by scan_fs/mod.rs's
    // is_workspace_root stamping) is internal-only — same treatment
    // as `mikebom:is-workspace-root`. It never appears in emitted
    // CDX/SPDX SBOMs.
    matches!(
        key,
        IS_WORKSPACE_ROOT_KEY | "mikebom:is-cargo-workspace-toplevel"
    )
}

/// Milestone 145 US3 (FR-009 + research §C/§C.1): annotation keys that
/// are emitted from a field-derived source (typically
/// `c.evidence.*`) and MUST NOT be re-emitted from the
/// `extra_annotations` bag — doing so produces double-emission +
/// per-emitter value-drift like the 2026-06-26 audit flagged for
/// `mikebom:source-files` on Maven nested-JAR components.
///
/// Defense-in-depth: callers that intend to carry per-reader source
/// provenance MUST use a DISTINCT annotation key (e.g.,
/// `mikebom:<reader>-source-url`) to avoid colliding with the
/// field-derived emission.
pub fn is_field_owned_annotation_key(key: &str) -> bool {
    matches!(key, "mikebom:source-files")
}

// ============================================================
// Milestone 149: apply_main_module_drop_or_demote
// ============================================================

/// Annotation key signalling that a `library`-typed component in
/// `components[]` was preserved from the manifest-derived main-module
/// after a milestone-077 root-override (`--root-name` / `--root-version`
/// / `--root-purl`) AND the milestone-149 `--preserve-manifest-main-module`
/// opt-in flag was set. Constitution Principle V parity-bridging:
/// CDX 1.6 `component.type = "library"` describes the role but not the
/// demote provenance; SPDX 2.3 + SPDX 3 likewise lack a native carrier.
/// See `docs/reference/sbom-format-mapping.md` C102 for the audit.
pub(crate) const DEMOTED_FROM_MAIN_MODULE_KEY: &str = "mikebom:demoted-from-main-module";

/// Return shape for [`apply_main_module_drop_or_demote`].
///
/// `effective_components` is the filtered/transformed Vec the emitter
/// iterates to build wire-side `components[]` / `packages[]` /
/// `software_Package` elements.
///
/// `redirected_main_module_purls` collects the PURLs of every main-module
/// entry whose outbound dependency edges need re-anchoring onto the
/// operator-override root (milestone-084 logic at
/// `cyclonedx/builder.rs:442-447` and parallel SPDX sites). Per
/// milestone-149 US1 clarification (recorded 2026-06-29), the demoted
/// entry has NO outbound `dependsOn` edges in the wire output even when
/// it's KEPT in `components[]` — so this Vec is populated regardless of
/// whether the entry was dropped (Path 2) or demoted (Path 3).
pub(crate) struct DropOrDemoteResult {
    pub effective_components: Vec<ResolvedComponent>,
    pub redirected_main_module_purls: Vec<String>,
}

/// Milestone 149: consolidate the duplicated main-module-drop logic
/// from three emitter sites (`cyclonedx/builder.rs:325-347`,
/// `spdx/document.rs:262-282`, `spdx/v3_document.rs:57-75`) into a
/// single shared helper, AND add the milestone-149 preserve-as-library-
/// demote branch gated on `preserve_main_module`.
///
/// Three behavior paths:
///
/// 1. **Override INACTIVE** — passthrough; returns a clone of `components`
///    with empty `redirected_main_module_purls`. Byte-identical to
///    pre-149 for scans without override flags (SC-003 regression guard).
///
/// 2. **Override ACTIVE + preserve OFF** — milestone-077 clean-replacement.
///    Filters out main-module entries from the returned Vec; populates
///    `redirected_main_module_purls` with their PURLs for downstream
///    relationship re-anchoring (milestone-084 logic). Byte-identical to
///    pre-149 default behavior (SC-002 regression guard).
///
/// 3. **Override ACTIVE + preserve ON** — milestone 149 NEW. For each
///    main-module entry: KEEP in the returned Vec after applying the
///    demote transformation (remove the `mikebom:component-role:
///    main-module` annotation so downstream type-derivation produces
///    `type: "library"`; add `mikebom:demoted-from-main-module: "true"`
///    annotation per Constitution Principle V parity-bridging); ADD
///    PURL to `redirected_main_module_purls` so relationship re-anchoring
///    still fires per US1 clarification Option A.
///
/// Multi-main-module scans (per milestone 127 — Cargo workspace,
/// polyglot) where N>1 components carry the main-module role tag fall
/// through to the drop path even when `preserve_main_module` is true,
/// because there's no SINGLE manifest-derived main-module to demote.
/// An INFO-level diagnostic surfaces the no-op per spec FR-013 +
/// Edge Case 4.
///
/// Edge Case 1: `preserve_main_module` set WITHOUT an active override
/// is a silent no-op with an INFO-level diagnostic. Path 1 fires.
///
/// Pure function over its inputs. No side effects beyond the
/// `tracing::info!` diagnostics.
pub(crate) fn apply_main_module_drop_or_demote(
    components: &[ResolvedComponent],
    root_override: &RootComponentOverride,
    preserve_main_module: bool,
) -> DropOrDemoteResult {
    let override_active = root_override.is_active();

    // Edge Case 1: preserve flag set without an active override is a
    // silent no-op with an INFO diagnostic so the operator notices the
    // flag had no effect.
    if !override_active && preserve_main_module {
        tracing::info!(
            "--preserve-manifest-main-module has no effect without --root-name override",
        );
    }

    // Path 1: override INACTIVE → passthrough.
    if !override_active {
        return DropOrDemoteResult {
            effective_components: components.to_vec(),
            redirected_main_module_purls: Vec::new(),
        };
    }

    // Multi-main-module guard (Edge Case 4 + FR-013): when N>1 main-modules
    // are tagged, NONE were promoted to metadata.component pre-149
    // (milestone 127's placeholder-path behavior); the preserve flag is
    // a no-op because there's no single main-module to demote. Emit INFO
    // log and fall through to the drop-all path so the override clean-
    // replacement semantic stays unchanged.
    let main_module_count = components
        .iter()
        .filter(|c| is_main_module(c))
        .count();
    let effective_preserve = preserve_main_module && main_module_count == 1;
    if preserve_main_module && main_module_count > 1 {
        tracing::info!(
            count = main_module_count,
            "--preserve-manifest-main-module skipped: multi-main-module scan ({main_module_count} modules detected)",
        );
    }

    // Single-pass walk: collect redirected PURLs + build effective Vec.
    let mut effective = Vec::with_capacity(components.len());
    let mut redirected = Vec::new();
    for c in components {
        if is_main_module(c) {
            // Per US1 clarification Option A (recorded 2026-06-29):
            // the demoted entry has NO outbound dependsOn edges in the
            // wire output even when kept. Push the PURL to `redirected`
            // regardless of whether we drop (Path 2) or demote (Path 3)
            // so milestone-084 re-anchoring fires identically in both
            // cases.
            redirected.push(c.purl.as_str().to_string());
            if effective_preserve {
                // Path 3: demote in place — keep entry with transformed
                // annotations. Removing the role tag flips downstream
                // type-derivation to `library` automatically (per
                // research §B + the existing `binary_role_to_cdx_type`
                // default path).
                let mut demoted = c.clone();
                demoted.extra_annotations.remove(COMPONENT_ROLE_KEY);
                demoted.extra_annotations.insert(
                    DEMOTED_FROM_MAIN_MODULE_KEY.to_string(),
                    serde_json::Value::String("true".to_string()),
                );
                effective.push(demoted);
            } else {
                // Path 2: drop. The existing tracing::info from the
                // pre-149 emitter-side filter migrated here so the
                // operator-facing diagnostic stays uniform across all
                // three formats.
                tracing::info!(
                    purl = %c.purl,
                    "override is set; dropping manifest-derived main-module component '{}' from emitted SBOM (per milestone 077 clean-replacement; see GitHub issue #151 + milestone 149 for the preserve-as-library opt-in)",
                    c.purl,
                );
            }
        } else {
            effective.push(c.clone());
        }
    }

    DropOrDemoteResult {
        effective_components: effective,
        redirected_main_module_purls: redirected,
    }
}

#[inline]
fn is_main_module(c: &ResolvedComponent) -> bool {
    c.extra_annotations
        .get(COMPONENT_ROLE_KEY)
        .and_then(|v| v.as_str())
        == Some(MAIN_MODULE_ROLE)
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

    /// Milestone 145 US3 (FR-009): the field-owned-key helper protects
    /// `mikebom:source-files` from double-emission. New field-owned
    /// keys can be added by extending the `matches!` arm; the test
    /// guards the contract.
    #[test]
    fn is_field_owned_annotation_key_md145() {
        assert!(is_field_owned_annotation_key("mikebom:source-files"));
        // Unrelated keys MUST NOT be filtered (they're emitted from
        // extra_annotations as the only source).
        assert!(!is_field_owned_annotation_key("mikebom:source-files-nested-url"));
        assert!(!is_field_owned_annotation_key("mikebom:lifecycle-scope"));
        assert!(!is_field_owned_annotation_key("mikebom:cpe-candidates"));
        assert!(!is_field_owned_annotation_key("mikebom:file-paths"));
    }

    /// Milestone 201 (FR-007, closes #587): the m201 positive-identifier
    /// annotation `mikebom:is-cargo-workspace-toplevel` MUST be internal-
    /// emission-only (filtered from CDX/SPDX output), matching the
    /// existing treatment of `mikebom:is-workspace-root`.
    #[test]
    fn is_internal_emission_key_filters_workspace_toplevel_annotation_m201() {
        assert!(is_internal_emission_key("mikebom:is-cargo-workspace-toplevel"));
        assert!(is_internal_emission_key(IS_WORKSPACE_ROOT_KEY));
        // Guardrail: filter must not over-broaden.
        assert!(!is_internal_emission_key("mikebom:some-other-annotation"));
        assert!(!is_internal_emission_key("mikebom:lifecycle-scope"));
    }

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
            requirement_ranges: Vec::new(),
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

    // ============================================================
    // Milestone 149: apply_main_module_drop_or_demote tests
    // ============================================================

    fn active_override() -> RootComponentOverride {
        RootComponentOverride {
            name: Some("widget-svc".to_string()),
            version: Some("1.2.3".to_string()),
            ..Default::default()
        }
    }

    /// Helper: build a non-main-module library component (no role tag).
    fn make_library(purl_str: &str) -> ResolvedComponent {
        let purl = Purl::new(purl_str).expect("valid purl");
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
            requirement_ranges: Vec::new(),
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
            extra_annotations: BTreeMap::new(),
            binary_role: None,
        }
    }

    /// T009 — FR-007 + SC-003 regression guard: Path 1 (passthrough).
    /// Override INACTIVE → helper returns input verbatim, no redirected
    /// PURLs, no demote transformation. Main-module entries KEEP their
    /// `mikebom:component-role` annotation.
    #[test]
    fn apply_drop_or_demote_no_override_is_passthrough_md149() {
        let components = vec![
            make_main_module("pkg:cargo/foo-internal@0.5.1", "/p/Cargo.toml", false),
            make_library("pkg:cargo/dep-a@1.0.0"),
        ];
        let result = apply_main_module_drop_or_demote(
            &components,
            &no_override(),
            /* preserve_main_module = */ true, // ← deliberately set; should be a no-op without override
        );
        assert_eq!(result.effective_components.len(), 2,
            "passthrough MUST preserve every input component");
        assert!(result.redirected_main_module_purls.is_empty(),
            "passthrough MUST NOT populate redirected PURLs");
        let preserved_main = result.effective_components.iter()
            .find(|c| c.purl.as_str() == "pkg:cargo/foo-internal@0.5.1")
            .expect("main-module entry preserved");
        assert_eq!(
            preserved_main.extra_annotations.get(COMPONENT_ROLE_KEY).and_then(|v| v.as_str()),
            Some(MAIN_MODULE_ROLE),
            "passthrough MUST preserve the main-module role tag (no demote when override inactive)",
        );
        assert!(
            !preserved_main.extra_annotations.contains_key(DEMOTED_FROM_MAIN_MODULE_KEY),
            "passthrough MUST NOT add the demote annotation",
        );
    }

    /// T010 — FR-007 + SC-002 regression guard: Path 2 (drop).
    /// Override ACTIVE + preserve OFF → main-modules dropped from
    /// effective_components AND their PURLs land in redirected_main_module_purls
    /// for milestone-084 re-anchoring.
    #[test]
    fn apply_drop_or_demote_override_no_preserve_drops_main_module_md149() {
        let components = vec![
            make_main_module("pkg:cargo/foo-internal@0.5.1", "/p/Cargo.toml", false),
            make_library("pkg:cargo/dep-a@1.0.0"),
        ];
        let result = apply_main_module_drop_or_demote(
            &components,
            &active_override(),
            /* preserve_main_module = */ false,
        );
        // Main-module dropped; dep library survives.
        assert_eq!(result.effective_components.len(), 1);
        assert_eq!(result.effective_components[0].purl.as_str(), "pkg:cargo/dep-a@1.0.0");
        // Main-module's PURL in redirected for re-anchoring.
        assert_eq!(
            result.redirected_main_module_purls,
            vec!["pkg:cargo/foo-internal@0.5.1".to_string()],
            "FR-007 + milestone-084: dropped main-module's PURL MUST be redirected for re-anchoring",
        );
    }

    /// T011 — FR-001 + FR-004 + US1 clarification Option A: Path 3 (demote).
    /// Override ACTIVE + preserve ON → main-module KEPT in
    /// effective_components with role-tag removed + demote annotation added;
    /// PURL STILL in redirected_main_module_purls so re-anchoring fires
    /// per Option A (demoted entry has no outbound dependsOn in wire output).
    #[test]
    fn apply_drop_or_demote_override_with_preserve_demotes_main_module_md149() {
        let components = vec![
            make_main_module("pkg:cargo/foo-internal@0.5.1", "/p/Cargo.toml", false),
            make_library("pkg:cargo/dep-a@1.0.0"),
        ];
        let result = apply_main_module_drop_or_demote(
            &components,
            &active_override(),
            /* preserve_main_module = */ true,
        );
        // Main-module KEPT + dep library: 2 entries.
        assert_eq!(result.effective_components.len(), 2);
        let demoted = result.effective_components.iter()
            .find(|c| c.purl.as_str() == "pkg:cargo/foo-internal@0.5.1")
            .expect("FR-001: main-module entry kept in effective_components");
        // Role tag REMOVED → downstream type-derivation flips to library.
        assert!(
            !demoted.extra_annotations.contains_key(COMPONENT_ROLE_KEY),
            "FR-003: main-module role tag MUST be removed so wire type derives to library",
        );
        // Demote annotation ADDED.
        assert_eq!(
            demoted.extra_annotations.get(DEMOTED_FROM_MAIN_MODULE_KEY),
            Some(&serde_json::Value::String("true".to_string())),
            "FR-004: demote annotation MUST be added",
        );
        // US1 clarification Option A: redirected PURL populated even when entry kept.
        assert_eq!(
            result.redirected_main_module_purls,
            vec!["pkg:cargo/foo-internal@0.5.1".to_string()],
            "US1 Option A: demoted entry's PURL MUST be redirected so relationship re-anchoring fires (demoted entry has no outbound edges in wire output)",
        );
    }

    /// T012 — FR-005: demote preserves every field other than annotations.
    #[test]
    fn apply_drop_or_demote_demote_preserves_other_fields_md149() {
        let mut main_module = make_main_module(
            "pkg:cargo/foo-internal@0.5.1",
            "/p/Cargo.toml",
            false,
        );
        // Populate rich non-default values across the field surface.
        main_module.lifecycle_scope =
            Some(mikebom_common::resolution::LifecycleScope::Runtime);
        main_module.sbom_tier = Some("source".to_string());
        main_module.evidence.confidence = 0.9;
        main_module.evidence.source_connection_ids = vec!["conn-42".to_string()];
        // Keep a snapshot of the non-annotation fields for assertion.
        let snapshot_name = main_module.name.clone();
        let snapshot_version = main_module.version.clone();
        let snapshot_parent_purl = main_module.parent_purl.clone();
        let snapshot_lifecycle = main_module.lifecycle_scope;
        let snapshot_sbom_tier = main_module.sbom_tier.clone();
        let snapshot_hashes = main_module.hashes.clone();
        let snapshot_confidence = main_module.evidence.confidence;
        let snapshot_technique = main_module.evidence.technique.clone();
        let snapshot_conn_ids = main_module.evidence.source_connection_ids.clone();
        let snapshot_purl_str = main_module.purl.as_str().to_string();

        let components = vec![main_module];
        let result = apply_main_module_drop_or_demote(
            &components,
            &active_override(),
            /* preserve_main_module = */ true,
        );
        let demoted = result.effective_components.iter()
            .find(|c| c.purl.as_str() == snapshot_purl_str)
            .expect("demoted entry kept");
        // FR-005: every named field unchanged.
        assert_eq!(demoted.name, snapshot_name);
        assert_eq!(demoted.version, snapshot_version);
        assert_eq!(demoted.parent_purl, snapshot_parent_purl);
        assert_eq!(demoted.lifecycle_scope, snapshot_lifecycle);
        assert_eq!(demoted.sbom_tier, snapshot_sbom_tier);
        assert_eq!(demoted.hashes, snapshot_hashes);
        assert_eq!(demoted.evidence.confidence, snapshot_confidence);
        assert_eq!(demoted.evidence.technique, snapshot_technique);
        assert_eq!(demoted.evidence.source_connection_ids, snapshot_conn_ids);
    }

    /// T013 — Edge Case 4 + FR-013: multi-main-module + preserve = no-op.
    /// Falls through to the drop path so override clean-replacement stays
    /// unchanged for workspace/polyglot scans. Both main-modules dropped,
    /// both PURLs in redirected.
    #[test]
    fn apply_drop_or_demote_multi_main_module_with_preserve_is_noop_md149() {
        let components = vec![
            make_main_module("pkg:cargo/crate-a@0.1.0", "/p/a/Cargo.toml", false),
            make_main_module("pkg:cargo/crate-b@0.2.0", "/p/b/Cargo.toml", false),
            make_library("pkg:cargo/dep@1.0.0"),
        ];
        let result = apply_main_module_drop_or_demote(
            &components,
            &active_override(),
            /* preserve_main_module = */ true,
        );
        // Only the library survives; both main-modules dropped.
        assert_eq!(result.effective_components.len(), 1);
        assert_eq!(result.effective_components[0].purl.as_str(), "pkg:cargo/dep@1.0.0");
        // Both main-module PURLs in redirected.
        assert_eq!(result.redirected_main_module_purls.len(), 2);
        // No demote annotation anywhere (the multi-MM case falls through).
        for c in &result.effective_components {
            assert!(
                !c.extra_annotations.contains_key(DEMOTED_FROM_MAIN_MODULE_KEY),
                "FR-013: multi-main-module + preserve MUST NOT emit demote annotation",
            );
        }
    }

    /// T014 — US1 Option A regression guard: demoted entry's PURL is in
    /// redirected_main_module_purls (matters because milestone-084 logic
    /// re-anchors deps off it onto the operator-override root; demoted
    /// entry has zero outbound edges in wire output).
    #[test]
    fn apply_drop_or_demote_demoted_entry_purl_is_redirected_for_re_anchoring_md149() {
        let components = vec![make_main_module(
            "pkg:npm/foo-internal@0.5.1",
            "/p/package.json",
            false,
        )];
        let result = apply_main_module_drop_or_demote(
            &components,
            &active_override(),
            /* preserve_main_module = */ true,
        );
        assert_eq!(result.effective_components.len(), 1, "demoted entry kept");
        assert!(
            result.redirected_main_module_purls
                .contains(&"pkg:npm/foo-internal@0.5.1".to_string()),
            "US1 Option A: demoted PURL MUST be in redirected (drives milestone-084 \
             re-anchoring; demoted entry has empty dependsOn in wire output)",
        );
    }

    /// T014b — FR-009 + Edge Case 5 (M1 analyze-finding fix): when a
    /// transitive library dep happens to share the manifest-main-module
    /// PURL (rare; theoretical), the deduplicator's existing
    /// (ecosystem, name, version, parent_purl) group-key merges them
    /// pre-helper. The helper sees the single merged entry and demotes
    /// it correctly. This test asserts the helper's behavior is invariant
    /// over whether the input was a merged-from-collision case or a
    /// natural single-entry case — both produce the same output shape.
    #[test]
    fn apply_drop_or_demote_handles_pre_merged_collision_entry_md149() {
        // Simulate a merged-from-collision entry: it has the main-module
        // role tag (because the main-module reader's emission won the
        // confidence tiebreak in dedup) AND extra source_file_paths
        // (because the colliding library dep contributed its paths
        // during the within-group merge at deduplicator.rs:74-78).
        let mut merged = make_main_module(
            "pkg:cargo/foo-internal@0.5.1",
            "/p/Cargo.toml",
            false,
        );
        // Two source paths instead of zero — the collision contribution.
        merged.evidence.source_file_paths = vec![
            "Cargo.toml".to_string(),
            "vendor/foo-internal/Cargo.toml".to_string(),
        ];

        let components = vec![merged];
        let result = apply_main_module_drop_or_demote(
            &components,
            &active_override(),
            /* preserve_main_module = */ true,
        );
        // The merged entry demotes cleanly: role tag removed, annotation added,
        // source_file_paths PRESERVED verbatim (FR-005).
        assert_eq!(result.effective_components.len(), 1);
        let demoted = &result.effective_components[0];
        assert!(!demoted.extra_annotations.contains_key(COMPONENT_ROLE_KEY));
        assert_eq!(
            demoted.extra_annotations.get(DEMOTED_FROM_MAIN_MODULE_KEY),
            Some(&serde_json::Value::String("true".to_string())),
        );
        // FR-005 + FR-009: source_file_paths from the pre-helper merge
        // are preserved on the demoted entry.
        assert_eq!(
            demoted.evidence.source_file_paths,
            vec![
                "Cargo.toml".to_string(),
                "vendor/foo-internal/Cargo.toml".to_string(),
            ],
        );
    }
}
