//! Milestone 054 integration test (US1 AS#1 + AS#2): mikebom must
//! complete a scan in bounded wall-clock time on a synthesized
//! filesystem topology that mirrors the knative/func v1.22.0
//! reproducer (intentional symlink loops in test-fixture directories).
//!
//! Pre-054 this test would hang at 100% CPU forever in
//! `rpm_file::walk_dir` (and `binary::discover::walk_dir`); post-054
//! the canonicalize-keyed visited-set breaks the cycles. The test
//! invokes the actual `mikebom` binary subprocess (matching the
//! existing `tests/scan_*.rs` integration-test pattern) so the
//! coverage is end-to-end.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Build the knative/func-style fixture under `root`. Mirrors
/// `pkg/oci/testdata/test-links/` from the upstream project (where
/// the user's hang originated). Symlinks created:
///
/// - `pkg/oci/testdata/test-links/linkToRoot -> .` (self-loop)
/// - `pkg/oci/testdata/test-links/b/linkToRoot -> ..` (parent loop)
/// - `pkg/oci/testdata/test-links/b/linkToRootsParent -> ../..` (grandparent loop)
/// - `pkg/oci/testdata/test-links/b/c/linkToParent -> ..` (parent loop, deeper)
fn build_knative_style_fixture(root: &Path) {
    let test_links = root.join("pkg/oci/testdata/test-links");
    std::fs::create_dir_all(test_links.join("b/c")).expect("mkdir test-links/b/c");
    std::os::unix::fs::symlink(".", test_links.join("linkToRoot"))
        .expect("symlink linkToRoot");
    std::os::unix::fs::symlink("..", test_links.join("b/linkToRoot"))
        .expect("symlink b/linkToRoot");
    std::os::unix::fs::symlink("../..", test_links.join("b/linkToRootsParent"))
        .expect("symlink b/linkToRootsParent");
    std::os::unix::fs::symlink("..", test_links.join("b/c/linkToParent"))
        .expect("symlink b/c/linkToParent");
    // Add a sentinel go.mod so the Go reader runs (gives the scan
    // some real work to do beyond just the empty-fixture exit path).
    std::fs::write(
        root.join("go.mod"),
        "module example.com/test\n\ngo 1.22\n",
    )
    .expect("write go.mod");
}

#[test]
fn scan_handles_knative_func_style_symlink_loops_without_hanging() {
    // US1 AS#1 + AS#2 + SC-001 + SC-006: pre-054 this fixture
    // shape would hang `mikebom sbom scan` indefinitely. Post-054
    // the canonicalize-keyed visited-set breaks every cycle and
    // the scan completes promptly.
    let tmp = tempfile::tempdir().expect("tempdir");
    build_knative_style_fixture(tmp.path());

    let bin = env!("CARGO_BIN_EXE_mikebom");
    let out = tempfile::NamedTempFile::new().expect("tempfile");
    let fake_home = tempfile::tempdir().expect("fake-home");
    let empty_cache = tempfile::tempdir().expect("empty-cache");

    let start = std::time::Instant::now();
    let output = Command::new(bin)
        .env("HOME", fake_home.path())
        .env("GOMODCACHE", empty_cache.path().join("empty"))
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(tmp.path())
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--output")
        .arg(out.path())
        .arg("--no-deep-hash")
        .output()
        .expect("mikebom should run");
    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "scan failed (exit {:?}): stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
    // Test framework's per-test timeout would catch a true hang; the
    // explicit bound here is a regression-narrative check. 30s is
    // generous for a microscopic synthesized fixture; CI macos-latest
    // contention is the realistic worst case.
    assert!(
        elapsed < Duration::from_secs(30),
        "scan should complete promptly on a symlink-loop fixture; took {:?}. \
         Pre-054 this would have hung indefinitely. Reproduces the \
         knative/func v1.22.0 hang shape that closed issue #102.",
        elapsed,
    );
}
