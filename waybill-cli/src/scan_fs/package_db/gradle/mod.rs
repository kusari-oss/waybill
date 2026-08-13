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
pub(super) mod ladder;
pub(super) mod lockfile;
pub(super) mod static_parser;
pub(super) mod subprocess;
pub(super) mod tier;
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
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |project_dir| {
        if !project_dir.is_dir() {
            return;
        }
        // m106 lockfile pass — unchanged.
        for filename in ["gradle.lockfile", "buildscript-gradle.lockfile"] {
            let path = project_dir.join(filename);
            if !path.is_file() {
                continue;
            }
            out.extend(lockfile::read_gradle_lockfile(&path));
        }

        // m235 ladder pass — runs for every project directory that
        // looks like a Gradle project (has `build.gradle` or `.kts` or
        // `settings.gradle(.kts)`). Skips directories that don't so we
        // don't waste subprocess calls on arbitrary directories.
        let is_gradle_project = ["build.gradle", "build.gradle.kts",
                                  "settings.gradle", "settings.gradle.kts"]
            .iter()
            .any(|name| project_dir.join(name).is_file());
        if is_gradle_project {
            let graph = ladder::resolve(project_dir, &ladder_config);
            out.extend(graph.components);
            // `graph.edges` + `graph.tier` + `graph.fallback_history` are
            // consumed by the m235 US4 emitters (transparency
            // annotations). MVP (Phase 3): US4 not yet wired; the edge
            // + tier information flows into `out` via the annotations
            // once US4 lands. For now, only the components appear in
            // the SBOM (which matches m106 lockfile behavior).
            //
            // TODO(m235 US4): thread edges + tier + fallback_history
            // out to the emission layer via `ScanResult` extension.
        }
    });
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
