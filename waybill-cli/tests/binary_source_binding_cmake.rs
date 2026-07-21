//! Milestone 109 — US1 + US2 end-to-end integration tests.
//!
//! Verifies the cross-tier PURL attribution mechanism: when waybill
//! scans a cmake project root (source CMakeLists.txt + a build dir
//! with `_deps/<name>-build/` + a binary that exports the matching
//! library's symbols), the emitted SBOM contains ONE component per
//! library (under the source-tier PURL) — not two.
//!
//! Test fixtures are synthetic: the `CMakeLists.txt` is written by
//! hand; the `_deps/<name>-build/` directory tree is created with
//! `std::fs::create_dir_all`; the binary is a real Mach-O / ELF
//! executable that exports the zlib API. The test SKIPS gracefully
//! when no such fixture binary is available on the test host (e.g.,
//! the waybill-cmake-demo's `build/crc-demo` hasn't been built
//! locally) — useful for CI lanes that don't pre-build the demo.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

/// Locate a binary that exports zlib's full API surface (10 of 10
/// zlib fingerprint symbols). Looks for the cmake-demo's
/// `build/crc-demo` at a well-known sibling-repo path. Returns
/// `None` on hosts without the demo pre-built.
fn find_zlib_exporting_binary() -> Option<PathBuf> {
    let candidates = [
        // waybill-cmake-demo sibling repo (when developer has built it locally)
        "../waybill-cmake-demo/build/crc-demo",
        "../../waybill-cmake-demo/build/crc-demo",
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.is_file() {
            return p.canonicalize().ok();
        }
    }
    None
}

/// Construct a synthetic cmake project root with a `CMakeLists.txt`
/// declaring `FetchContent_Declare(<library> GIT_REPOSITORY <git_url>
/// GIT_TAG <git_tag>)`, a `build/` directory with `CMakeCache.txt` +
/// `_deps/<library>-build/`, and a copy of `binary` placed under
/// `build/`. Returns the project root tempdir.
fn make_cmake_project_fixture(
    library: &str,
    git_url: &str,
    git_tag: &str,
    binary: &Path,
) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    // CMakeLists.txt at project root
    let cmake_lists = format!(
        r#"cmake_minimum_required(VERSION 3.16)
project(test_proj LANGUAGES C)
include(FetchContent)
FetchContent_Declare(
    {library}
    GIT_REPOSITORY {git_url}
    GIT_TAG {git_tag}
)
FetchContent_MakeAvailable({library})
"#
    );
    std::fs::write(root.path().join("CMakeLists.txt"), cmake_lists).unwrap();

    // build/ with CMakeCache.txt + _deps/<library>-build/ + the binary
    let build = root.path().join("build");
    std::fs::create_dir_all(&build).unwrap();
    std::fs::write(build.join("CMakeCache.txt"), "# synthetic for milestone-109 test\n").unwrap();
    let deps_build = build.join("_deps").join(format!("{library}-build"));
    std::fs::create_dir_all(&deps_build).unwrap();
    let deps_src = build.join("_deps").join(format!("{library}-src"));
    std::fs::create_dir_all(&deps_src).unwrap();

    // Copy the binary into build/ so the binary walker finds it AND
    // the path-ancestor scoping rule sees it as under the cmake
    // project's build dir.
    let bin_name = binary.file_name().unwrap();
    let bin_dest = build.join(bin_name);
    std::fs::copy(binary, &bin_dest).unwrap();
    // Preserve exec bit on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&bin_dest).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&bin_dest, perms).unwrap();
    }

    root
}

/// Run `waybill sbom scan --fingerprints-corpus` against the given
/// project root + return the parsed CDX SBOM as a `serde_json::Value`.
fn scan_with_corpus(project_root: &Path) -> Value {
    let out = tempfile::tempdir().unwrap();
    let out_file = out.path().join("sbom.cdx.json");
    let result = Command::new(binary_path())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--output")
        .arg(&out_file)
        .arg("--no-deep-hash")
        .arg("--fingerprints-corpus")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "waybill sbom scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = std::fs::read(&out_file).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// US1 AS1 — attribution fires when a cmake declaration, a matching
/// build dir, and a binary exporting the library's symbols are all
/// present. Asserts the SBOM contains EXACTLY ONE zlib component
/// with the source-tier PURL (no `pkg:generic/zlib` shadow).
#[test]
fn attribution_fires_when_cmake_decl_and_build_dir_present() {
    let Some(binary) = find_zlib_exporting_binary() else {
        println!(
            "skipped: no zlib-exporting binary available \
             (build waybill-cmake-demo first: \
             `cd ../waybill-cmake-demo && cmake -S . -B build -G Ninja && ninja -C build`)"
        );
        return;
    };

    let root = make_cmake_project_fixture(
        "zlib",
        "https://github.com/madler/zlib.git",
        "v1.3.1",
        &binary,
    );

    let sbom = scan_with_corpus(root.path());

    // Pull out every component named `zlib` and assert ONE.
    let zlib_components: Vec<&Value> = sbom["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["name"].as_str() == Some("zlib"))
        .collect();
    assert_eq!(
        zlib_components.len(),
        1,
        "expected exactly 1 zlib component; got {}: {:#?}",
        zlib_components.len(),
        zlib_components
    );
    let zlib = zlib_components[0];

    // PURL is the source-tier (cmake-derived) form, not the generic.
    assert_eq!(
        zlib["purl"].as_str(),
        Some("pkg:github/madler/zlib@v1.3.1"),
        "expected source-tier PURL; got {:?}",
        zlib["purl"]
    );

    // No `pkg:generic/zlib` shadow exists anywhere in the SBOM.
    let generic_zlib_count = sbom["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["purl"].as_str() == Some("pkg:generic/zlib"))
        .count();
    assert_eq!(
        generic_zlib_count, 0,
        "found {generic_zlib_count} `pkg:generic/zlib` shadows; attribution \
         should have rewritten them to the source-tier PURL"
    );

    // Both source-tier and binary-tier evidence survived the dedup
    // merge (cross-tier transparency).
    let props: Vec<(&str, &str)> = zlib["properties"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["name"].as_str().unwrap(), p["value"].as_str().unwrap()))
        .collect();
    assert!(
        props.iter().any(|(k, v)| *k == "waybill:source-mechanism"
            && *v == "cmake-fetchcontent-git"),
        "expected waybill:source-mechanism=cmake-fetchcontent-git in properties; got {props:?}"
    );
    assert!(
        props
            .iter()
            .any(|(k, _)| *k == "waybill:fingerprint-corpus-sha"),
        "expected waybill:fingerprint-corpus-sha in properties (binary-tier \
         annotation should survive the dedup merge); got {props:?}"
    );
    assert!(
        props
            .iter()
            .any(|(k, v)| *k == "waybill:fingerprint-symbols-matched"
                && *v == "10/10"),
        "expected waybill:fingerprint-symbols-matched=10/10 in properties; got {props:?}"
    );
}

/// US1 AS3 — attribution falls back when the build directory is
/// absent. The cmake declaration still emits a source-tier component,
/// but no binary-tier shadow is created (nothing was actually built /
/// linked).
#[test]
fn attribution_falls_back_when_build_dir_absent() {
    let root = tempfile::tempdir().unwrap();
    let cmake_lists = r#"cmake_minimum_required(VERSION 3.16)
project(test_proj LANGUAGES C)
include(FetchContent)
FetchContent_Declare(
    zlib
    GIT_REPOSITORY https://github.com/madler/zlib.git
    GIT_TAG v1.3.1
)
FetchContent_MakeAvailable(zlib)
"#;
    std::fs::write(root.path().join("CMakeLists.txt"), cmake_lists).unwrap();
    // Intentionally NO `build/` directory.

    let sbom = scan_with_corpus(root.path());

    // The source-tier zlib component should be present (cmake reader
    // doesn't depend on the build dir to emit declarations).
    let zlib_count = sbom["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["name"].as_str() == Some("zlib"))
        .count();
    assert_eq!(
        zlib_count, 1,
        "expected exactly 1 zlib component (source-tier); got {zlib_count}"
    );
    // No `pkg:generic/zlib` either — there's no binary to scan.
    assert_eq!(
        sbom["components"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["purl"].as_str() == Some("pkg:generic/zlib"))
            .count(),
        0,
        "expected no pkg:generic/zlib shadow when no binary scanned"
    );
}

/// US2 — consumer joins source + binary SBOMs by PURL equality.
/// Emits two SBOMs (source-only + project-root) and verifies that
/// every source-tier PURL appears in the binary-tier SBOM (zero
/// phantom mismatches for declared-AND-built deps).
#[test]
fn consumer_equality_join_recovers_zero_phantom_mismatches() {
    let Some(binary) = find_zlib_exporting_binary() else {
        println!(
            "skipped: no zlib-exporting binary available \
             (build waybill-cmake-demo first)"
        );
        return;
    };

    // Source-only scan: run waybill on a project WITHOUT build/
    // (mirrors `waybill sbom scan --path src/` at PR-merge time).
    let source_root = tempfile::tempdir().unwrap();
    let cmake_lists = r#"cmake_minimum_required(VERSION 3.16)
project(t LANGUAGES C)
include(FetchContent)
FetchContent_Declare(zlib GIT_REPOSITORY https://github.com/madler/zlib.git GIT_TAG v1.3.1)
FetchContent_MakeAvailable(zlib)
"#;
    std::fs::write(source_root.path().join("CMakeLists.txt"), cmake_lists).unwrap();
    let source_sbom = scan_with_corpus(source_root.path());

    // Project-root scan: source + build + binary (mirrors
    // `waybill sbom scan --path .` at release-build time).
    let project = make_cmake_project_fixture(
        "zlib",
        "https://github.com/madler/zlib.git",
        "v1.3.1",
        &binary,
    );
    let project_sbom = scan_with_corpus(project.path());

    // Compute PURL sets.
    let source_purls: std::collections::BTreeSet<String> = source_sbom["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["purl"].as_str().map(String::from))
        .collect();
    let project_purls: std::collections::BTreeSet<String> = project_sbom["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["purl"].as_str().map(String::from))
        .collect();

    // Every source-tier PURL must appear in the project-root SBOM.
    // Otherwise, the cross-tier alignment goal isn't met.
    let missing: Vec<&String> =
        source_purls.difference(&project_purls).collect();
    assert!(
        missing.is_empty(),
        "source-tier PURLs missing from project-root SBOM: {missing:?}\n\
         source: {source_purls:?}\nproject: {project_purls:?}"
    );
}
