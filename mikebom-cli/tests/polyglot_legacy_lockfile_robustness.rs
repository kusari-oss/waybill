//! Polyglot legacy-lockfile robustness — milestone 105 phase 2G
//! (T027, SC-008).
//!
//! Regression test for the gRPC-discovered abort: a stray legacy
//! npm v1 `package-lock.json` sitting deep inside a polyglot project
//! (e.g., `third_party/<deep>/package-lock.json`) used to cause
//! `mikebom sbom scan` to exit non-zero with the message
//! "package-lock.json v1 not supported; regenerate with npm ≥7",
//! preventing the C/C++ readers (and every other ecosystem reader)
//! from contributing their components to the output.
//!
//! After milestone 105's dispatcher-level fix (T026), the npm reader
//! warn-and-skips on v1 lockfiles instead of aborting the whole
//! scan. This test creates a tempdir containing both a valid C/C++
//! manifest AND a deliberately-bad v1 package-lock.json, scans it,
//! and asserts the C/C++ component emerges in the output.

use std::path::Path;
use std::process::Command;

mod common;
use common::normalize::apply_fake_home_env;
use common::bin;

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir parent");
    }
    std::fs::write(path, contents).expect("write file");
}

/// Polyglot fixture: a C/C++ project using CMake FetchContent for
/// boost AND a stray legacy npm v1 lockfile in a vendored test
/// fixtures path (the exact shape that triggered the gRPC abort).
fn build_polyglot_fixture(root: &Path) {
    // Top-level CMakeLists.txt declaring a real FetchContent dep.
    // The cmake reader emits `pkg:generic/boost@1.84.0` for this.
    write(
        &root.join("CMakeLists.txt"),
        r#"cmake_minimum_required(VERSION 3.16)
project(polyglot LANGUAGES CXX)
include(FetchContent)
FetchContent_Declare(boost URL https://example.com/boost_1_84_0.tar.gz)
"#,
    );

    // Deliberately-bad legacy v1 npm lockfile, deep in the tree.
    // Mirrors the gRPC shape: a Node.js example/tooling sub-tree
    // shipped with an old lockfile that has nothing to do with the
    // C/C++ build the operator is actually scanning.
    write(
        &root.join("examples/node/package-lock.json"),
        r#"{
  "name": "polyglot-node-example",
  "version": "1.0.0",
  "lockfileVersion": 1,
  "requires": true,
  "dependencies": {
    "left-pad": { "version": "1.3.0" }
  }
}
"#,
    );
    // package.json adjacent to the lockfile so the npm dispatcher
    // considers this a candidate project root.
    write(
        &root.join("examples/node/package.json"),
        r#"{
  "name": "polyglot-node-example",
  "version": "1.0.0",
  "dependencies": {
    "left-pad": "1.3.0"
  }
}
"#,
    );
}

#[test]
fn v1_lockfile_does_not_abort_scan_or_block_other_readers() {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fixture_root = workdir.path().join("fixture");
    std::fs::create_dir_all(&fixture_root).expect("fixture mkdir");
    build_polyglot_fixture(&fixture_root);

    let out_path = workdir.path().join("sbom.cdx.json");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("MIKEBOM_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        fixture_root.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
    ]);
    let output = cmd.output().expect("spawn mikebom");

    // 1. Scan MUST succeed (zero exit code). Pre-T026, this asserted
    //    non-zero with the v1-unsupported message.
    assert!(
        output.status.success(),
        "scan unexpectedly failed: status={:?}\nstdout={}\nstderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // 2. The C/C++ component MUST appear in the output (the whole
    //    point — other readers can't be blocked by a single npm
    //    reader failure).
    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");
    let components = json["components"].as_array().expect("components[] present");
    let boost = components
        .iter()
        .find(|c| c["name"].as_str().unwrap_or("").to_lowercase().contains("boost"));
    assert!(
        boost.is_some(),
        "C/C++ component (boost) missing from output — npm v1 abort regressed.\nGot components: {}",
        serde_json::to_string_pretty(&components).unwrap_or_default()
    );

    // 3. The stderr MUST contain a warning naming the offending
    //    lockfile path so operators can locate it and decide whether
    //    to regenerate it.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("package-lock.json v1 unsupported"),
        "expected warn-log naming the v1 lockfile; got stderr:\n{stderr}"
    );
}
