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

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::PackageDbEntry;

// Milestone 664 US2 T052: shared-walker migration types.
use crate::scan_fs::walk_registry::{
    globset_from_patterns, ReaderId, ReaderRegistration, SharedWalkerContext,
};

/// Milestone 664 US2 T052: shared-walker discovery state. Records
/// SwiftPM project roots detected during the single-pass descent via
/// sibling-lookup on `Package.resolved` / `Package.swift`.
#[derive(Default, Debug)]
pub(crate) struct SwiftDiscoveredPaths {
    /// Directories containing `Package.resolved` (source-tier).
    pub(crate) lockfile_dirs: Vec<PathBuf>,
    /// Directories with only `Package.swift` (design-tier warn-and-skip).
    pub(crate) manifest_only_dirs: Vec<PathBuf>,
}

/// Per-directory callback. Sibling-lookup on `Package.resolved` +
/// `Package.swift` via `ctx.dir_index()` (contract C2 zero-syscalls).
fn on_swift_dir(dir: &Path, ctx: &SharedWalkerContext<'_>) {
    let idx = ctx.dir_index();
    let has_lockfile = idx.contains(dir, OsStr::new("Package.resolved"));
    let has_manifest = idx.contains(dir, OsStr::new("Package.swift"));
    if !has_lockfile && !has_manifest {
        return;
    }
    let Some(state) = ctx.state::<Mutex<SwiftDiscoveredPaths>>(ReaderId::SWIFT) else {
        return;
    };
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if has_lockfile {
        guard.lockfile_dirs.push(dir.to_path_buf());
    } else {
        // Manifest-only case: warn-and-skip in finalize.
        guard.manifest_only_dirs.push(dir.to_path_buf());
    }
}

pub(crate) fn registration() -> anyhow::Result<ReaderRegistration> {
    // Even though this reader is `on_dir`-driven, a non-empty patterns
    // set is required by the registration validator (contract). Any
    // never-matching pattern works — pick `Package.resolved` so a
    // `grep`-audit of the source finds this reader by its marker.
    let patterns = globset_from_patterns(&["**/Package.resolved"])?;
    Ok(ReaderRegistration {
        reader_id: ReaderId::SWIFT,
        state: Some(Arc::new(Mutex::new(SwiftDiscoveredPaths::default()))),
        patterns,
        on_file: None,
        on_dir: Some(on_swift_dir),
        descend_into: None,
    })
}

pub(crate) fn extract_paths(registration: &ReaderRegistration) -> SwiftDiscoveredPaths {
    let Some(state_arc) = registration.state.as_ref() else {
        return SwiftDiscoveredPaths::default();
    };
    let Some(mutex) = state_arc.downcast_ref::<Mutex<SwiftDiscoveredPaths>>() else {
        return SwiftDiscoveredPaths::default();
    };
    let mut guard = match mutex.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    std::mem::take(&mut *guard)
}

/// Legacy public entry — retained during FR-004 coexistence.
///
/// Honors `--exclude-path` via the existing `safe_walk` integration
/// (FR-011). Skips `.build/` subtrees (the SwiftPM build cache) per
/// the milestone-114 / milestone-090 cache-exclude convention.
#[allow(dead_code)]
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
    let mut paths = SwiftDiscoveredPaths::default();
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |project_dir| {
        if !project_dir.is_dir() {
            return;
        }
        let has_lockfile = project_dir.join("Package.resolved").is_file();
        let has_manifest = manifest::detect(&project_dir.join("Package.swift"));
        if has_lockfile {
            paths.lockfile_dirs.push(project_dir.to_path_buf());
        } else if has_manifest {
            paths.manifest_only_dirs.push(project_dir.to_path_buf());
        }
    });
    finalize(paths)
}

/// Post-walker entry — takes precomputed dir sets + runs Package.resolved
/// parse pipeline. Manifest-only dirs emit the warn-and-skip diagnostic.
pub(crate) fn finalize(paths: SwiftDiscoveredPaths) -> Vec<PackageDbEntry> {
    let SwiftDiscoveredPaths {
        mut lockfile_dirs,
        mut manifest_only_dirs,
    } = paths;
    // Deterministic order — safe_walk was parent-first visitation; the
    // shared walker dispatches on_dir bottom-up. Sort to normalize
    // both paths' output ordering for FR-006 identity.
    lockfile_dirs.sort();
    manifest_only_dirs.sort();

    let mut out: Vec<PackageDbEntry> = Vec::new();
    for project_dir in &lockfile_dirs {
        let lockfile_path = project_dir.join("Package.resolved");
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
    }
    for project_dir in &manifest_only_dirs {
        let manifest_path = project_dir.join("Package.swift");
        tracing::warn!(
            path = %manifest_path.display(),
            "swift: Package.swift found without sibling Package.resolved; \
             run `swift package resolve` to lock dependencies. \
             No Swift components emitted from this directory."
        );
    }
    out
}
