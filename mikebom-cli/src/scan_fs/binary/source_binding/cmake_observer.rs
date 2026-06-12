//! cmake `FetchContent_Declare` build-directory observer.
//!
//! Per `contracts/walker-protocol.md`: walks `scan_root` for cmake
//! project build directories (CMakeCache.txt + `_deps/` co-presence),
//! then joins each cmake `FetchContent_Declare` source declaration
//! against the `_deps/<name>-build/` subdirectory that cmake's
//! FetchContent module creates after a successful build.
//!
//! Only `cmake-fetchcontent-git` and `cmake-fetchcontent-url` source
//! declarations participate (per the Phase-2 clarification —
//! `ExternalProject_Add` is deferred to a follow-on milestone because
//! its default build-dir layout is `<name>-prefix/` with per-project
//! variance).

use std::path::{Path, PathBuf};

use super::super::super::package_db::PackageDbEntry;
use super::{BuildDirObserver, CmakeBuildDirObservation};

/// Maximum recursion depth from `scan_root` when searching for cmake
/// project build directories. Set per research.md §R1: covers
/// typical (build/ at depth 1) + monorepo (subprojects/<name>/build/
/// at depth 3-4) layouts with a small headroom; protects against
/// pathological deep trees.
const MAX_WALK_DEPTH: usize = 6;

pub(crate) struct CmakeFetchContentObserver;

impl BuildDirObserver for CmakeFetchContentObserver {
    fn observe(
        &self,
        scan_root: &Path,
        source_declarations: &[PackageDbEntry],
    ) -> Vec<CmakeBuildDirObservation> {
        // Filter source declarations to the cmake-fetchcontent
        // subset. Other declarations (cmake-externalproject, vcpkg,
        // conan, bazel) deliberately don't participate this milestone.
        let cmake_decls: Vec<&PackageDbEntry> = source_declarations
            .iter()
            .filter(|d| is_cmake_fetchcontent(d))
            .collect();
        if cmake_decls.is_empty() {
            return Vec::new();
        }

        // Walk scan_root for cmake project build directories.
        let mut cmake_build_roots: Vec<PathBuf> = Vec::new();
        walk_for_cmake_build_dirs(scan_root, 0, &mut cmake_build_roots);
        if cmake_build_roots.is_empty() {
            return Vec::new();
        }
        // Lexical sort for determinism per the walker-protocol contract.
        cmake_build_roots.sort();

        // For each (cmake-project build root, cmake declaration) pair,
        // check `_deps/<name>-build/` existence.
        let mut observations: Vec<CmakeBuildDirObservation> = Vec::new();
        for build_root in &cmake_build_roots {
            for decl in &cmake_decls {
                let build_artifact_dir =
                    build_root.join("_deps").join(format!("{}-build", decl.name));
                if !build_artifact_dir.is_dir() {
                    // Declared but not built; falls through to the
                    // milestone-108 generic-PURL path naturally.
                    continue;
                }
                let source_mechanism = extract_source_mechanism(decl)
                    .unwrap_or_else(|| "cmake-fetchcontent-url".to_string());
                observations.push(CmakeBuildDirObservation {
                    library_name: decl.name.clone(),
                    source_tier_purl: decl.purl.as_str().to_string(),
                    source_mechanism,
                    build_artifact_dir,
                    cmake_project_build_root: build_root.clone(),
                });
            }
        }
        // Determinism — sort by (project root, library name).
        observations.sort_by(|a, b| {
            a.cmake_project_build_root
                .cmp(&b.cmake_project_build_root)
                .then_with(|| a.library_name.cmp(&b.library_name))
        });
        observations
    }
}

/// True when this PackageDbEntry was emitted by the cmake reader for
/// a `FetchContent_Declare` rule (git or url form). Reads the
/// `mikebom:source-mechanism` annotation.
fn is_cmake_fetchcontent(entry: &PackageDbEntry) -> bool {
    extract_source_mechanism(entry)
        .map(|m| m == "cmake-fetchcontent-git" || m == "cmake-fetchcontent-url")
        .unwrap_or(false)
}

/// Pull the `mikebom:source-mechanism` annotation value out of a
/// PackageDbEntry's extra_annotations bag.
fn extract_source_mechanism(entry: &PackageDbEntry) -> Option<String> {
    entry
        .extra_annotations
        .get("mikebom:source-mechanism")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Bounded-depth walk for cmake project build directories. A
/// directory is recorded as a cmake project build root when BOTH
/// `<dir>/CMakeCache.txt` exists as a file AND `<dir>/_deps/` exists
/// as a non-empty directory. Once recorded, do NOT descend into the
/// build dir itself (the per-dep `_deps/<name>-build/` subdirs are
/// what later observation queries against, not full traversal).
fn walk_for_cmake_build_dirs(
    dir: &Path,
    depth: usize,
    out: &mut Vec<PathBuf>,
) {
    if depth > MAX_WALK_DEPTH {
        return;
    }
    // Check at this level first.
    if is_cmake_build_dir(dir) {
        out.push(dir.to_path_buf());
        // Don't descend into _deps/ — the inner sub-builds aren't
        // independent cmake projects (cmake doesn't write
        // CMakeCache.txt inside _deps/<name>-build/).
        return;
    }
    // Otherwise descend.
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(
                dir = %dir.display(),
                error = %e,
                "cmake-observer: failed to read directory; skipping subtree",
            );
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Skip well-known noise directories that can never be cmake
        // project roots — keeps the walk fast on real-world trees.
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if matches!(
                name,
                ".git" | "node_modules" | "target" | ".cache" | "vendor"
            ) {
                continue;
            }
        }
        walk_for_cmake_build_dirs(&path, depth + 1, out);
    }
}

/// True when `dir` has both `CMakeCache.txt` AND a non-empty `_deps/`
/// subdirectory.
fn is_cmake_build_dir(dir: &Path) -> bool {
    if !dir.join("CMakeCache.txt").is_file() {
        return false;
    }
    let deps = dir.join("_deps");
    if !deps.is_dir() {
        return false;
    }
    // Non-empty check: at least one entry under _deps/.
    match std::fs::read_dir(&deps) {
        Ok(mut iter) => iter.next().is_some(),
        Err(_) => false,
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use mikebom_common::types::purl::Purl;

    /// Build a synthetic PackageDbEntry tagged with a cmake source-
    /// mechanism. Mirrors what the real cmake reader emits.
    fn cmake_decl(name: &str, purl: &str, mechanism: &str) -> PackageDbEntry {
        let mut extra_annotations = std::collections::BTreeMap::new();
        extra_annotations.insert(
            "mikebom:source-mechanism".to_string(),
            serde_json::json!(mechanism),
        );
        PackageDbEntry {
            build_inclusion: None,
            purl: Purl::new(purl).unwrap(),
            name: name.to_string(),
            version: String::new(),
            arch: None,
            source_path: String::new(),
            depends: Vec::new(),
            maintainer: None,
            licenses: vec![],
            lifecycle_scope: None,
            requirement_range: None,
            source_type: None,
            sbom_tier: None,
            shade_relocation: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            raw_version: None,
            parent_purl: None,
            npm_role: None,
            co_owned_by: None,
            hashes: Vec::new(),
            extra_annotations,
            binary_role: None,
        }
    }

    /// Create a synthetic cmake project build directory at `path`
    /// with the given dep names under `_deps/<name>-build/`.
    fn make_cmake_build_dir(path: &Path, deps: &[&str]) {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("CMakeCache.txt"), "# synthetic").unwrap();
        let deps_dir = path.join("_deps");
        std::fs::create_dir_all(&deps_dir).unwrap();
        for d in deps {
            std::fs::create_dir_all(deps_dir.join(format!("{d}-build"))).unwrap();
            std::fs::create_dir_all(deps_dir.join(format!("{d}-src"))).unwrap();
        }
    }

    #[test]
    fn observer_returns_empty_when_no_cmake_declarations() {
        let tmp = tempfile::tempdir().unwrap();
        make_cmake_build_dir(&tmp.path().join("build"), &["zlib"]);
        let observer = CmakeFetchContentObserver;
        let observations = observer.observe(tmp.path(), &[]);
        assert!(observations.is_empty());
    }

    #[test]
    fn observer_returns_empty_when_no_build_dir_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let decls = vec![cmake_decl(
            "zlib",
            "pkg:github/madler/zlib@v1.3.1",
            "cmake-fetchcontent-git",
        )];
        let observer = CmakeFetchContentObserver;
        let observations = observer.observe(tmp.path(), &decls);
        assert!(observations.is_empty());
    }

    #[test]
    fn observer_emits_one_per_decl_when_build_dir_exists() {
        let tmp = tempfile::tempdir().unwrap();
        make_cmake_build_dir(&tmp.path().join("build"), &["zlib"]);
        let decls = vec![cmake_decl(
            "zlib",
            "pkg:github/madler/zlib@v1.3.1",
            "cmake-fetchcontent-git",
        )];
        let observer = CmakeFetchContentObserver;
        let observations = observer.observe(tmp.path(), &decls);
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].library_name, "zlib");
        assert_eq!(
            observations[0].source_tier_purl,
            "pkg:github/madler/zlib@v1.3.1"
        );
        assert_eq!(observations[0].source_mechanism, "cmake-fetchcontent-git");
    }

    #[test]
    fn observer_skips_declarations_for_missing_deps_build_dir() {
        let tmp = tempfile::tempdir().unwrap();
        // Build dir exists for zlib only; libcurl declared but not built.
        make_cmake_build_dir(&tmp.path().join("build"), &["zlib"]);
        let decls = vec![
            cmake_decl("zlib", "pkg:github/madler/zlib@v1.3.1", "cmake-fetchcontent-git"),
            cmake_decl(
                "libcurl",
                "pkg:github/curl/curl@curl-8_0_0",
                "cmake-fetchcontent-git",
            ),
        ];
        let observer = CmakeFetchContentObserver;
        let observations = observer.observe(tmp.path(), &decls);
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].library_name, "zlib");
    }

    #[test]
    fn observer_skips_non_fetchcontent_source_mechanisms() {
        let tmp = tempfile::tempdir().unwrap();
        make_cmake_build_dir(&tmp.path().join("build"), &["zlib"]);
        // ExternalProject_Add declarations are out of scope this milestone.
        let decls = vec![cmake_decl(
            "zlib",
            "pkg:generic/zlib@1.3.1",
            "cmake-externalproject",
        )];
        let observer = CmakeFetchContentObserver;
        let observations = observer.observe(tmp.path(), &decls);
        assert!(observations.is_empty());
    }

    #[test]
    fn observer_detects_multiple_cmake_projects_in_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        // Two cmake projects under a monorepo-style workspace.
        make_cmake_build_dir(&tmp.path().join("subprojects/A/build"), &["zlib"]);
        make_cmake_build_dir(&tmp.path().join("subprojects/B/build"), &["zlib"]);
        let decls = vec![cmake_decl(
            "zlib",
            "pkg:github/madler/zlib@v1.3.1",
            "cmake-fetchcontent-git",
        )];
        let observer = CmakeFetchContentObserver;
        let observations = observer.observe(tmp.path(), &decls);
        assert_eq!(observations.len(), 2);
        // Sorted by (project_root, library_name).
        assert!(observations[0]
            .cmake_project_build_root
            .ends_with("subprojects/A/build"));
        assert!(observations[1]
            .cmake_project_build_root
            .ends_with("subprojects/B/build"));
    }

    #[test]
    fn observer_skips_noise_directories() {
        let tmp = tempfile::tempdir().unwrap();
        // .git/ contains a fake CMakeCache.txt — observer must skip it
        // entirely (deny-list of noise directories).
        let git_dir = tmp.path().join(".git/build");
        make_cmake_build_dir(&git_dir, &["zlib"]);
        let decls = vec![cmake_decl(
            "zlib",
            "pkg:github/madler/zlib@v1.3.1",
            "cmake-fetchcontent-git",
        )];
        let observer = CmakeFetchContentObserver;
        let observations = observer.observe(tmp.path(), &decls);
        assert!(observations.is_empty());
    }
}
