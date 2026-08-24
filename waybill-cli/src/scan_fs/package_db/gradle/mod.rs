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

use std::path::{Path, PathBuf};

use super::PackageDbEntry;

// Milestone 664 US2 T041: shared-walker migration types.
use crate::scan_fs::walk_registry::{
    ReaderId, ReaderRegistration, ReaderRegistryBuilder, SharedWalker, SharedWalkerContext,
};
use std::ffi::OsStr;
use std::sync::{Arc, Mutex};

/// Per-scan state — accumulates Gradle project directories discovered
/// via `on_dir` callback + sibling-lookup. This is the first reader to
/// use the sibling-lookup pattern from FR-003 + quickstart.md's
/// "two-phase reader" recipe.
#[derive(Default, Debug)]
pub(crate) struct GradleDiscoveredPaths {
    pub(crate) project_dirs: Vec<PathBuf>,
}

/// Per-directory callback. Fires once per dir descended into. Consults
/// `ctx.dir_index()` (populated by the shared walker before dispatch)
/// to check for Gradle-marker files — no fresh `read_dir()` syscalls.
/// This is the FR-003 sibling-lookup pattern.
fn on_gradle_dir(dir: &Path, ctx: &SharedWalkerContext<'_>) {
    let Some(state) = ctx.state::<Mutex<GradleDiscoveredPaths>>(ReaderId::GRADLE) else {
        return;
    };
    // Sibling lookup — zero extra syscalls per FR-003 / contract C2.
    let is_gradle_project = [
        OsStr::new("build.gradle"),
        OsStr::new("build.gradle.kts"),
        OsStr::new("settings.gradle"),
        OsStr::new("settings.gradle.kts"),
        // m106 lockfile presence also qualifies — some projects have
        // lockfiles without build.gradle (Gradle 7+ supports
        // settings-only projects with per-subproject build files).
        OsStr::new("gradle.lockfile"),
        OsStr::new("buildscript-gradle.lockfile"),
    ]
    .iter()
    .any(|marker| ctx.dir_index().contains(dir, marker));
    if !is_gradle_project {
        return;
    }
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.project_dirs.push(dir.to_path_buf());
}

/// Build the `ReaderRegistration`. Uses `on_dir` (not `on_file`) since
/// the sibling-lookup pattern needs one dispatch per directory, not per
/// file. Empty pattern set is fine — on_dir fires unconditionally on
/// every descent per the m664 dispatch design.
pub(crate) fn registration() -> anyhow::Result<ReaderRegistration> {
    // Empty GlobSet: on_dir fires unconditionally; on_file is None so
    // no file dispatches occur.
    let patterns = crate::scan_fs::walk_registry::globset_from_patterns(&[])?;
    Ok(ReaderRegistration {
        reader_id: ReaderId::GRADLE,
        state: Some(Arc::new(Mutex::new(GradleDiscoveredPaths::default()))),
        patterns,
        on_file: None,
        on_dir: Some(on_gradle_dir),
        descend_into: None,
    })
}

pub(crate) fn extract_paths(registration: &ReaderRegistration) -> GradleDiscoveredPaths {
    let Some(state_arc) = registration.state.as_ref() else {
        return GradleDiscoveredPaths::default();
    };
    let Some(mutex) = state_arc.downcast_ref::<Mutex<GradleDiscoveredPaths>>() else {
        return GradleDiscoveredPaths::default();
    };
    let mut guard = match mutex.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    std::mem::take(&mut *guard)
}

/// Coexistence-period entry point — mini-registry per reader.
/// **Post-T033**: `read_all` uses the consolidated shared-walker pilot;
/// this fn is retained as a shortcut for tests + single-reader debug.
#[allow(dead_code)]
pub(crate) fn build_and_run(
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
    diagnostics: &mut super::ScanDiagnostics,
) -> Vec<PackageDbEntry> {
    let reg = match registration() {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!(error = %err, "gradle: registration() failed");
            return Vec::new();
        }
    };
    let registry = match ReaderRegistryBuilder::new().register(reg).build() {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!(error = %err, "gradle: build() failed");
            return Vec::new();
        }
    };
    let mut walker = SharedWalker::new(rootfs, &registry, exclude_set).with_max_depth(6);
    walker.run();
    let _ = walker.finish();
    let gradle_reg = registry
        .registrations()
        .iter()
        .find(|r| r.reader_id == ReaderId::GRADLE)
        .expect("gradle registration must be present");
    let paths = extract_paths(gradle_reg);
    finalize(paths, rootfs, exclude_set, diagnostics)
}

/// Legacy `pub fn read()` — retained during FR-004 coexistence.
/// Discovers Gradle project dirs via safe_walk + delegates to
/// `finalize()`.
#[allow(dead_code)]
pub fn read(
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
    diagnostics: &mut super::ScanDiagnostics,
) -> Vec<PackageDbEntry> {
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
    let mut project_dirs: Vec<PathBuf> = Vec::new();
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |project_dir| {
        if !project_dir.is_dir() {
            return;
        }
        let is_gradle_project = [
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
            "gradle.lockfile",
            "buildscript-gradle.lockfile",
        ]
        .iter()
        .any(|name| project_dir.join(name).is_file());
        if is_gradle_project {
            project_dirs.push(project_dir.to_path_buf());
        }
    });
    let paths = GradleDiscoveredPaths { project_dirs };
    finalize(paths, rootfs, exclude_set, diagnostics)
}

/// Post-walker entry — takes discovered project dirs and runs the
/// m235 ladder + m106 lockfile pass per dir. Mutates `diagnostics`
/// with the aggregated summary.
pub(crate) fn finalize(
    paths: GradleDiscoveredPaths,
    rootfs: &Path,
    _exclude_set: &super::exclude_path::ExclusionSet,
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

    // Defensive sort — the shared walker's on_dir dispatch fires in
    // descent order which is already lex-sorted per contract, but the
    // legacy read()'s safe_walk output would also be sorted. Ensure
    // matching output between the two entry points.
    let mut project_dirs = paths.project_dirs;
    project_dirs.sort();
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

    for project_dir in &project_dirs {
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
            // C148 per-component tier annotation: tag every ladder
            // component with the tier that produced it. Complementary
            // to C146 doc-scope (the aggregate) — this gives per-
            // component audit trail in both homogeneous and mixed
            // scans without any emit-time conditional.
            let tier_str = graph.tier.as_annotation_str();
            for mut entry in graph.components {
                entry.extra_annotations.insert(
                    "waybill:gradle-subproject-tier".to_string(),
                    serde_json::Value::String(tier_str.to_string()),
                );
                out.push(entry);
            }
        }

        // m106 lockfile pass — unchanged behaviorally, plus the same
        // C148 per-component tag using the `LockfileOnly` tier so
        // lockfile-only components carry the same audit trail.
        let mut saw_lockfile = false;
        for filename in ["gradle.lockfile", "buildscript-gradle.lockfile"] {
            let path = project_dir.join(filename);
            if !path.is_file() {
                continue;
            }
            for mut entry in lockfile::read_gradle_lockfile(&path) {
                entry.extra_annotations.insert(
                    "waybill:gradle-subproject-tier".to_string(),
                    serde_json::Value::String(
                        tier::GradleResolutionTier::LockfileOnly
                            .as_annotation_str()
                            .to_string(),
                    ),
                );
                out.push(entry);
            }
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
    }

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
