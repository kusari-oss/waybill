//! Milestone 156 SC-003 integration — symlink cycle safety.
//!
//! Creates a `cmake/loop -> ../cmake/` symlink at test setup time
//! (not checked into git — symlinks in git fixtures are fragile).
//! Asserts the scan completes in <5s and each file is read at most
//! once, exercising milestone-054's safe_walk canonicalize-keyed
//! visited-set for the cmake walker's recursive descent.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture_source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cmake-walker-depth/symlink-cycle")
}

/// Copy the fixture into a fresh tempdir + create the symlink loop.
/// Symlinks are OS-specific; test skips (returns None) on unsupported
/// platforms.
fn setup_fixture_with_symlink() -> Option<tempfile::TempDir> {
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path();

    // Copy files from the fixture template.
    std::fs::write(
        dest.join("CMakeLists.txt"),
        std::fs::read_to_string(fixture_source().join("CMakeLists.txt")).unwrap(),
    )
    .unwrap();
    std::fs::create_dir_all(dest.join("cmake")).unwrap();
    std::fs::write(
        dest.join("cmake").join("defs.cmake"),
        std::fs::read_to_string(fixture_source().join("cmake").join("defs.cmake")).unwrap(),
    )
    .unwrap();

    // Create the symlink loop: cmake/loop -> ../cmake (relative).
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("../cmake", dest.join("cmake").join("loop")).unwrap();
    }
    #[cfg(windows)]
    {
        // Windows symlinks require elevated privs; skip on Windows if
        // the operation fails (common in CI).
        if std::os::windows::fs::symlink_dir("../cmake", dest.join("cmake").join("loop")).is_err() {
            return None;
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        return None;
    }

    // Keep the tempdir alive by returning it.
    Some(tmp)
}

fn run_scan(project_root: &std::path::Path) -> Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn cmake_walker_symlink_cycle_bounded() {
    let Some(fixture) = setup_fixture_with_symlink() else {
        eprintln!("skip: symlink creation not supported on this platform");
        return;
    };

    let start = Instant::now();
    let doc = run_scan(fixture.path());
    let elapsed = start.elapsed();

    // SC-003: scan MUST complete in <5s despite the symlink loop.
    assert!(
        elapsed < Duration::from_secs(5),
        "scan took {elapsed:?}, expected <5s (SC-003 symlink cycle safety)"
    );

    // Assert Foo emission — proves the scan actually ran, not just aborted.
    let comps = doc
        .get("components")
        .and_then(|v| v.as_array())
        .unwrap_or(&Vec::new())
        .iter()
        .filter(|c| c.get("purl").and_then(|v| v.as_str()) == Some("pkg:generic/foo@1.0"))
        .count();
    assert_eq!(
        comps, 1,
        "expected exactly one pkg:generic/foo@1.0 component (via safe_walk's dedup on the symlink loop)"
    );
}
