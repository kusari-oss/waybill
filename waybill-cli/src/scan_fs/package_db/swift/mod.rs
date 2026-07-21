//! Swift Package Manager source-tree reader (milestone 122 US1, closes #...).
//!
//! Swift projects on disk carry a `Package.resolved` JSON lockfile (the
//! resolution authoritative for SwiftPM 5.0+). v1 / v2 / v3 schema variants
//! are dispatched on the top-level `version` integer. `Package.swift`
//! presence is detected (signals "this is a SwiftPM project root") but its
//! content is NEVER parsed in v0.1 — `Package.swift` is executable Swift code
//! and the dominant operator workflow has a resolved `Package.resolved`
//! sibling. Local-path / workspace-member emission from `Package.swift`
//! content is deferred to a future phase.
//!
//! PURLs emit as `pkg:swift/<host>/<namespace>/<name>@<version>` per the
//! [purl-spec swift type](https://github.com/package-url/purl-spec/blob/main/PURL-TYPES.rst#swift).
//! Commit-pinned mode (no `state.version`) uses the FULL 40-char revision
//! SHA as the version segment (matches the Go reader's
//! `pkg:golang/...@<sha>` convention).
//!
//! Per spec FR-001 + FR-002 + FR-003 + FR-014. Cross-platform (no
//! `#[cfg(unix)]`); zero new Cargo deps. Parse failures emit
//! `tracing::warn!` and yield zero components for that file (FR-009).

pub(super) mod lockfile;
pub(super) mod manifest;

use std::path::Path;

use super::PackageDbEntry;

/// Walk `rootfs` for `Package.resolved` files; parse each via the
/// schema-version dispatcher; project each pin into one
/// `PackageDbEntry`. Also detect sibling `Package.swift` without a
/// `Package.resolved` and emit a warn-and-skip diagnostic.
///
/// Honors `--exclude-path` via the existing `safe_walk` integration
/// (FR-011). Skips `.build/` subtrees (the SwiftPM build cache) per
/// the milestone-114 / milestone-090 cache-exclude convention.
pub fn read(
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: 6,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            // Skip SwiftPM build cache + the default project skip set.
            if name == ".build" {
                return true;
            }
            super::project_roots::should_skip_default_descent(name)
        },
        exclude_set,
    };
    let mut out = Vec::new();
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |project_dir| {
        if !project_dir.is_dir() {
            return;
        }
        let lockfile_path = project_dir.join("Package.resolved");
        let manifest_path = project_dir.join("Package.swift");

        let has_lockfile = lockfile_path.is_file();
        let has_manifest = manifest::detect(&manifest_path);

        if has_lockfile {
            match lockfile::read_package_resolved(&lockfile_path) {
                Ok(entries) => out.extend(entries),
                Err(e) => {
                    tracing::warn!(
                        path = %lockfile_path.display(),
                        error = %e,
                        "swift: Package.resolved parse failed; skipping this file"
                    );
                }
            }
        } else if has_manifest {
            tracing::warn!(
                path = %manifest_path.display(),
                "swift: Package.swift found without sibling Package.resolved; \
                 run `swift package resolve` to lock dependencies. \
                 No Swift components emitted from this directory."
            );
        }
    });
    out
}
