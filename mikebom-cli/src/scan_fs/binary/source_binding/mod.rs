//! Source-tier ↔ binary-tier PURL attribution (milestone 109).
//!
//! Bridges the gap between mikebom's two SBOM-emission paths for the
//! same C/C++ library:
//! - **Source-tier**: the milestone-102/103 cmake reader emits
//!   `pkg:github/madler/zlib@v1.3.1` when it parses
//!   `FetchContent_Declare(zlib GIT_REPOSITORY ... GIT_TAG v1.3.1)`.
//! - **Binary-tier**: the milestone-099/108 symbol-fingerprint matcher
//!   emits `pkg:generic/zlib` when it sees zlib's exported-symbol
//!   set in a binary's dynamic symbol table.
//!
//! Pre-milestone-109, these two emissions don't equality-join — the
//! SBOM carries two components for the same library. This module
//! observes cmake's documented `_deps/<name>-build/` build-directory
//! layout to attribute the binary-tier fingerprint match to the
//! source-tier PURL.
//!
//! Architecture (per FR-012 forward-compat): the cmake-specific
//! path-observation logic lives in [`cmake_observer`]; the
//! attribution registry + matcher-rewrite plumbing in [`registry`]
//! are observer-agnostic. A future Bazel observer lands as a sibling
//! module (`bazel_observer.rs`) that implements the same
//! [`BuildDirObserver`] trait and feeds the same registry.
//!
//! See `specs/109-binary-source-purl-binding/`.

pub(crate) mod cmake_observer;
pub(crate) mod registry;

use std::path::{Path, PathBuf};

use super::super::package_db::PackageDbEntry;

pub(crate) use registry::BuildAttributionRegistry;

/// One per (cmake-project, cmake declaration) pair where the
/// corresponding `_deps/<name>-build/` directory exists. Computed
/// once per scan; consumed read-only by the per-binary matcher.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct CmakeBuildDirObservation {
    /// The library name VERBATIM as it appeared in the cmake
    /// declaration's first positional arg (e.g.,
    /// `FetchContent_Declare(zlib ...)` → `"zlib"`). NOT lowercased
    /// here; the lowercasing happens at registry-lookup time to keep
    /// the source-of-truth name faithful for diagnostics.
    pub library_name: String,

    /// The source-tier PURL the cmake reader emitted for this
    /// declaration. Drives the rewrite of the binary-tier match's
    /// PURL when attribution fires.
    pub source_tier_purl: String,

    /// The `mikebom:source-mechanism` enum value the cmake reader
    /// tagged this declaration with — one of `cmake-fetchcontent-git`
    /// / `cmake-fetchcontent-url`. Drives the merged component's
    /// source-mechanism annotation.
    pub source_mechanism: String,

    /// Absolute path of the `<cmake-project-build-dir>/_deps/<name>-build/`
    /// directory whose existence corroborates the binding. Confirmed
    /// to exist at observation time; MAY have disappeared by lookup
    /// time (rare race; the cached value still drives the rewrite).
    pub build_artifact_dir: PathBuf,

    /// Absolute path of the cmake-project build dir itself (the
    /// parent of `_deps/`). Used to constrain attribution to
    /// binaries under this project's build dir (per the multi-cmake-
    /// project scoping rule).
    pub cmake_project_build_root: PathBuf,
}

/// Observer-agnostic trait for future build-system extensions
/// (Bazel, Meson, etc.). The cmake observer is the only implementer
/// this milestone (per the Phase-2 clarification: ExternalProject,
/// Bazel, and Meson are all deferred). The trait is `pub(crate)`
/// because every implementer lives inside `mikebom-cli`.
#[allow(dead_code)]
pub(crate) trait BuildDirObserver {
    /// Walk `scan_root` and join the build-tree artifacts against
    /// the source-tier declarations the cmake / vcpkg / Conan / etc.
    /// readers already parsed. Each returned observation represents
    /// one verified source-tier ↔ build-artifact pairing.
    fn observe(
        &self,
        scan_root: &Path,
        source_declarations: &[PackageDbEntry],
    ) -> Vec<CmakeBuildDirObservation>;
}

/// Build the per-scan attribution registry from the cmake reader's
/// parsed declarations. Returns an empty registry when no cmake
/// projects exist in the scan root (the common no-cmake-project
/// case) — the matcher then falls through to the milestone-108
/// generic-PURL path naturally.
///
/// Called once at scan-start; the resulting registry is passed
/// by reference into the per-binary matcher loop.
#[allow(dead_code)]
pub(crate) fn build_attribution_registry(
    scan_root: &Path,
    source_declarations: &[PackageDbEntry],
) -> BuildAttributionRegistry {
    let observer = cmake_observer::CmakeFetchContentObserver;
    let observations = observer.observe(scan_root, source_declarations);
    BuildAttributionRegistry::from_observations(observations)
}

// ============================================================
// Architectural extension path (FR-012 forward-compat)
// ============================================================
//
// Future Bazel / Meson observers live as sibling modules in this
// directory (e.g., `bazel_observer.rs`, `meson_observer.rs`). Each
// implements the `BuildDirObserver` trait above and returns
// `Vec<CmakeBuildDirObservation>` (the type name is generic enough
// to reuse — it could be renamed to `BuildDirObservation` in a
// follow-on if Bazel-specific fields become necessary). The
// `BuildAttributionRegistry::from_observations` constructor accepts
// any observer's output uniformly; the registry's lookup logic is
// observer-agnostic.
//
// To add a Bazel observer:
//   1. Add `mikebom-cli/src/scan_fs/binary/source_binding/bazel_observer.rs`
//      with a `BazelExternalRepoObserver` struct implementing
//      `BuildDirObserver`. Walk `bazel-out/<config>/bin/external/`
//      for Bazel's external-repo build artifacts.
//   2. Declare it in `mod.rs` (`pub(crate) mod bazel_observer;`).
//   3. Extend `build_attribution_registry` to fold its observations
//      in alongside the cmake observer's output, OR rename the
//      function to `build_attribution_registry_for(observers: &[...])`
//      taking a slice of trait objects.
//
// No changes to `cmake_observer.rs` or `registry.rs` are required.
// The `attribution-rules.md` contract (case-insensitive name match
// + scope-ancestry path-ancestor match) applies uniformly to all
// observers.

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// US5 / FR-012 — stub Bazel-shaped observer proves the trait
    /// surface is observer-agnostic. This test would fail to compile
    /// if `BuildDirObserver` had cmake-specific assumptions baked
    /// into its method signature.
    struct StubBazelObserver {
        canned_observations: Vec<CmakeBuildDirObservation>,
    }

    impl BuildDirObserver for StubBazelObserver {
        fn observe(
            &self,
            _scan_root: &Path,
            _source_declarations: &[PackageDbEntry],
        ) -> Vec<CmakeBuildDirObservation> {
            self.canned_observations.clone()
        }
    }

    #[test]
    fn stub_bazel_observer_integrates_with_registry_without_modification() {
        // Construct a stub observation that a hypothetical Bazel
        // reader might emit — same shape as the cmake observer's
        // output. Note: `source_mechanism` would be `bazel-http-archive`
        // for a real Bazel observer; we use `cmake-fetchcontent-git`
        // here for type compatibility (the registry doesn't validate
        // the source_mechanism value's per-observer correctness).
        let stub_observations = vec![CmakeBuildDirObservation {
            library_name: "zlib".to_string(),
            source_tier_purl: "pkg:github/madler/zlib@v1.3.1".to_string(),
            source_mechanism: "bazel-http-archive".to_string(),
            build_artifact_dir: PathBuf::from(
                "/tmp/proj/bazel-out/k8-fastbuild/bin/external/zlib",
            ),
            cmake_project_build_root: PathBuf::from("/tmp/proj"),
        }];
        let observer = StubBazelObserver {
            canned_observations: stub_observations.clone(),
        };

        let observations = observer.observe(Path::new("/tmp/proj"), &[]);
        let registry = BuildAttributionRegistry::from_observations(observations);

        // The registry's lookup logic doesn't care which observer
        // produced the observation. Bazel-shaped path-ancestor
        // matching works identically to cmake-shaped matching.
        let hit = registry.lookup("zlib", Path::new("/tmp/proj/bazel-bin/main"));
        assert!(
            hit.is_some(),
            "registry must accept observations from any BuildDirObserver impl"
        );
        assert_eq!(hit.unwrap().source_mechanism, "bazel-http-archive");
    }
}
