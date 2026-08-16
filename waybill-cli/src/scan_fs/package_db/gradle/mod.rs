//! Gradle source-tree reader (milestone 106 US3, closes #277).
//!
//! Gradle projects on disk emit dependency lockfiles in one of two shapes:
//!
//! - `gradle.lockfile` — application/library runtime classpath
//! - `buildscript-gradle.lockfile` — build-script (plugin) classpath
//!
//! Both files share a single line-oriented format. The filename alone
//! determines the lifecycle scope of the entries — runtime (no scope) vs
//! build — which the existing milestone-052 emission path then translates
//! into native CDX / SPDX 2.3 / SPDX 3 fields.
//!
//! Per spec FR-005 + FR-006 + Contract `gradle-lockfile.md`. PURLs are
//! emitted as `pkg:maven/<group>/<name>@<version>` so existing deps.dev
//! and Maven-side enrichment downstream applies without changes.
//!
//! Cross-platform (no `#[cfg(unix)]`); zero new Cargo deps. Parse failures
//! emit `tracing::warn!` and yield zero components for that file (FR-015).

pub(super) mod cache_reader;
// `ladder` + `tier` are `pub` (not `pub(super)`) because
// `ScanDiagnostics.gradle_scan_summary` (m235 US4) references
// `GradleScanSummary` from outside the module tree — the format
// emitters at `generate/*` read it to emit
// `waybill:gradle-resolution-tier`.
pub mod ladder;
pub(super) mod lockfile;
pub(super) mod static_parser;
pub(super) mod subprocess;
pub mod tier;
pub(super) mod version_catalog;

use std::path::Path;

use super::PackageDbEntry;

/// Walk `rootfs` for `gradle.lockfile` and `buildscript-gradle.lockfile`
/// files; parse each one; return all emitted entries. Empty when neither
/// file appears anywhere in the scan tree.
///
/// Milestone 235: ALSO invokes the m235 resolution ladder on every
/// Gradle project directory encountered. The ladder's output SUPPLEMENTS
/// the m106 lockfile output per FR-009 non-regression — when a lockfile
/// is present, m106's flat entries are unchanged; the ladder adds
/// transitive-edge information (once US4 emitters land, m235 MVP only
/// adds the components right now).
pub fn read(
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
    diagnostics: &mut super::ScanDiagnostics,
) -> Vec<PackageDbEntry> {
    // Milestone 235: read the ladder config from env vars set by
    // `GradleCliFlags::export_env`. Absent/zero means opt-out and the
    // ladder returns an empty `LockfileOnly` graph, preserving the
    // pre-m235 behavior when the operator didn't pass `--gradle-resolve`.
    let ladder_config = ladder::GradleLadderConfig {
        gradle_resolve: env_flag("WAYBILL_GRADLE_RESOLVE"),
        gradle_resolve_buildscript: env_flag("WAYBILL_GRADLE_RESOLVE_BUILDSCRIPT"),
        gradle_daemon: env_flag("WAYBILL_GRADLE_DAEMON"),
        gradle_timeout_secs: std::env::var("WAYBILL_GRADLE_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300),
        gradle_extra_configurations: std::env::var("WAYBILL_GRADLE_EXTRA_CONFIGURATIONS")
            .ok()
            .map(|s| s.split(',').filter(|p| !p.is_empty()).map(String::from).collect())
            .unwrap_or_default(),
    };

    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: 6,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            candidate
                .file_name()
                .and_then(|s| s.to_str())
                .map(super::project_roots::should_skip_default_descent)
                .unwrap_or(true)
        },
        exclude_set,
    };
    let mut out = Vec::new();
    // Milestone 235 US4: per-scan record of every Gradle project touched
    // and which ladder tier won for it. Aggregated at the end into a
    // `GradleScanSummary` on `ScanDiagnostics` — the m235 US4 emitters
    // read the aggregate and emit `waybill:gradle-resolution-tier` at
    // document scope.
    //
    // Also drives the FR-014 per-scan INFO log summary — the pairs
    // below (project-dir → tier) are formatted into the one-line
    // summary at the end of the walker.
    let mut per_project_pairs: Vec<(std::path::PathBuf, tier::GradleResolutionTier)> = Vec::new();
    // m235 US4 (C147): accumulate every tier-attempt failure across all
    // Gradle projects encountered. Pure `OperatorOptOut` is filtered out
    // — that's the default no-flag path, not something to surface. The
    // set is deduplicated + sorted (BTreeSet gives free ordering via the
    // `Ord` derive on both enums) and joined at the end.
    let mut all_fallbacks: std::collections::BTreeSet<(
        tier::GradleResolutionTier,
        tier::GradleFallbackReason,
    )> = std::collections::BTreeSet::new();

    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |project_dir| {
        if !project_dir.is_dir() {
            return;
        }

        // m235 ladder pass — runs for every project directory that
        // looks like a Gradle project. Also detects whether m106 later
        // matches a lockfile in this project so we know whether to
        // report the tier as `LockfileOnly` vs no-tier.
        let is_gradle_project = ["build.gradle", "build.gradle.kts",
                                  "settings.gradle", "settings.gradle.kts"]
            .iter()
            .any(|name| project_dir.join(name).is_file());

        // Track the tier that ACTUALLY contributed components for this
        // project (the "effective tier"). If the ladder produced
        // output, its tier wins. Otherwise m106 lockfile output (if
        // any) is what shows up in the SBOM, so the tier is
        // `LockfileOnly`. If neither contributed, we don't record a
        // tier for this project at all — no annotation drift toward
        // `mixed` from settings-only roots or empty build.gradle
        // stubs.
        let mut effective_tier: Option<tier::GradleResolutionTier> = None;
        if is_gradle_project {
            let graph = ladder::resolve(project_dir, &ladder_config);
            if !graph.components.is_empty() {
                effective_tier = Some(graph.tier);
            }
            for (t, r) in &graph.fallback_history {
                if !matches!(r, tier::GradleFallbackReason::OperatorOptOut) {
                    all_fallbacks.insert((*t, *r));
                }
            }
            out.extend(graph.components);
        }

        // m106 lockfile pass — unchanged behaviorally.
        let mut saw_lockfile = false;
        for filename in ["gradle.lockfile", "buildscript-gradle.lockfile"] {
            let path = project_dir.join(filename);
            if !path.is_file() {
                continue;
            }
            out.extend(lockfile::read_gradle_lockfile(&path));
            saw_lockfile = true;
        }

        // Fold lockfile contribution into the tier record if the
        // ladder didn't contribute.
        if effective_tier.is_none() && saw_lockfile {
            effective_tier = Some(tier::GradleResolutionTier::LockfileOnly);
        }

        if let Some(t) = effective_tier {
            per_project_pairs.push((project_dir.to_path_buf(), t));
        }
    });

    // Compute the aggregate summary for the doc-scope annotation.
    // Fire the summary whenever we either produced components OR
    // recorded a real fallback attempt — the fallback set alone is
    // enough to warrant emission so the operator can see the failed
    // attempt even when nothing landed downstream.
    if !per_project_pairs.is_empty() || !all_fallbacks.is_empty() {
        let (first_tier, all_same) = if per_project_pairs.is_empty() {
            // No tier contributed components; the honest default is
            // `lockfile-only` (the pre-ladder baseline). C147 will
            // still carry the diagnostic explaining why the ladder
            // itself didn't land.
            (tier::GradleResolutionTier::LockfileOnly, true)
        } else {
            let ft = per_project_pairs[0].1;
            let same = per_project_pairs.iter().all(|(_, t)| *t == ft);
            (ft, same)
        };
        let fallback_summary = if all_fallbacks.is_empty() {
            None
        } else {
            Some(
                all_fallbacks
                    .iter()
                    .map(|(t, r)| {
                        format!("{}:{}", t.as_annotation_str(), r.as_annotation_str())
                    })
                    .collect::<Vec<_>>()
                    .join(","),
            )
        };
        diagnostics.gradle_scan_summary = Some(ladder::GradleScanSummary {
            subprojects: Vec::new(), // per-subproject detail is a follow-on
            aggregate_tier: first_tier,
            aggregate_mixed: !all_same,
            fallback_summary,
        });

        // FR-014: emit a single INFO-level summary line naming which
        // tier fired for each project this scan touched. Format is
        // designed to be greppable (`gradle-resolver:` prefix + a
        // simple `,`-separated list of `<project-dir>=<tier>` pairs).
        // Project dirs are made scan-root-relative for readability
        // when possible; absolute paths otherwise.
        let items: Vec<String> = per_project_pairs
            .iter()
            .map(|(dir, tier_val)| {
                let display = dir
                    .strip_prefix(rootfs)
                    .map(|p| {
                        if p.as_os_str().is_empty() {
                            std::path::PathBuf::from(".")
                        } else {
                            p.to_path_buf()
                        }
                    })
                    .unwrap_or_else(|_| dir.clone());
                format!("{}={}", display.display(), tier_val.as_annotation_str())
            })
            .collect();
        if !items.is_empty() {
            tracing::info!(
                target: "waybill::gradle",
                "gradle-resolver: {}",
                items.join(", "),
            );
        }
    }

    out
}

/// Bool env-flag parser: "1", "true", "yes" (case-insensitive) → true.
/// Anything else (missing, "0", empty, garbage) → false.
fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}
