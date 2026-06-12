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

pub(super) mod lockfile;

use std::path::Path;

use super::PackageDbEntry;

/// Walk `rootfs` for `gradle.lockfile` and `buildscript-gradle.lockfile`
/// files; parse each one; return all emitted entries. Empty when neither
/// file appears anywhere in the scan tree.
pub fn read(
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    let cfg = super::project_roots::WalkConfig {
        max_depth: 6,
        is_project_root: &|dir: &Path| {
            dir.join("gradle.lockfile").is_file()
                || dir.join("buildscript-gradle.lockfile").is_file()
        },
        should_skip: &|name: &str| super::project_roots::should_skip_default_descent(name),
        exclude_set,
    };
    let mut out = Vec::new();
    for project_dir in super::project_roots::walk_for_project_roots(rootfs, &cfg) {
        for filename in ["gradle.lockfile", "buildscript-gradle.lockfile"] {
            let path = project_dir.join(filename);
            if !path.is_file() {
                continue;
            }
            out.extend(lockfile::read_gradle_lockfile(&path));
        }
    }
    out
}
