//! Milestone 217 — integration tests for the GOROOT-stdlib skip
//! (closes waybill#631: the Go rootfs walker previously picked up
//! $GOROOT/src/go.mod (module std) as a Go project-root candidate,
//! causing a downstream `go list all` preflight to fail with ~180
//! "use of internal package … not allowed" errors that GitHub Actions'
//! Go problem-matcher converted to ##[error] annotations).
//!
//! Four scenarios:
//!   T015 goroot_stdlib_not_emitted_as_main_module     — happy path (fixture)
//!   T016 install_path_independence_opt_go              — synthetic /opt/go layout
//!   T021 go_toolchain_detected_annotation_present     — SC-005 P2 annotation
//!   T022 annotation_absent_when_no_toolchain          — silence-on-no-observation

use std::path::PathBuf;
use std::process::Command;

use tempfile::tempdir;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn m217_goroot_stub_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/goroot_stub")
}

fn waybill_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_waybill"))
}

/// Run a scan in an isolated $HOME so per-host caches don't leak.
/// Returns (ok, stdout, stderr).
fn run_scan(path: &PathBuf, extra_args: &[&str]) -> (bool, String, String) {
    let home = tempdir().expect("home tempdir");
    let output = Command::new(waybill_bin())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .args(extra_args)
        .arg("--offline")
        .env("HOME", home.path())
        .env("XDG_CACHE_HOME", home.path())
        .env("CARGO_HOME", home.path().join(".cargo"))
        .env("GOMODCACHE", home.path().join("go-mod"))
        .env("M2_REPO", home.path().join(".m2"))
        .current_dir(workspace_root())
        .output()
        .expect("spawn waybill");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

// ============ T015 — happy path ============

#[test]
fn goroot_stdlib_not_emitted_as_main_module() {
    let out_dir = tempdir().expect("out tempdir");
    let out_path = out_dir.path().join("waybill.cdx.json");
    let (ok, _stdout, stderr) = run_scan(
        &m217_goroot_stub_fixture(),
        &[
            "--format",
            "cyclonedx-json",
            "--output",
            out_path.to_str().unwrap(),
        ],
    );
    assert!(ok, "scan failed:\n{stderr}");

    // SC-002 gate: zero "use of internal package" lines in stderr.
    // Pre-fix: ~180 such lines per Go-toolchain scan. Post-fix: zero
    // because the walker skips $GOROOT/src before `go list` runs.
    let internal_pkg_lines = stderr
        .lines()
        .filter(|l| l.contains("use of internal package"))
        .count();
    assert_eq!(
        internal_pkg_lines, 0,
        "SC-002: expected zero 'use of internal package' lines, got {internal_pkg_lines}:\n{stderr}"
    );

    // SC-001 gate: zero pkg:golang/std or pkg:golang/cmd components.
    let text = std::fs::read_to_string(&out_path).expect("read cdx");
    let v: serde_json::Value = serde_json::from_str(&text).expect("parse cdx");
    let bad = v["components"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c["purl"].as_str())
                .filter(|p| p.starts_with("pkg:golang/std@") || p.starts_with("pkg:golang/cmd@"))
                .count()
        })
        .unwrap_or(0);
    assert_eq!(
        bad, 0,
        "SC-001: expected zero pkg:golang/std@* or pkg:golang/cmd@* components, got {bad}"
    );
    // Also assert the metadata.component (BOM subject root) isn't one either.
    let root_purl = v
        .pointer("/metadata/component/purl")
        .and_then(|s| s.as_str())
        .unwrap_or("");
    assert!(
        !(root_purl.starts_with("pkg:golang/std@") || root_purl.starts_with("pkg:golang/cmd@")),
        "SC-001: metadata.component.purl MUST NOT be stdlib/cmd; got {root_purl}"
    );

    // FR-004 non-regression: user project MUST still be emitted.
    // Look for `example.com/app` either as the root or in components[].
    let has_user = root_purl.contains("example.com/app")
        || v["components"]
            .as_array()
            .map(|arr| {
                arr.iter().any(|c| {
                    c["purl"]
                        .as_str()
                        .map(|p| p.contains("example.com/app"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
    assert!(
        has_user,
        "FR-004: user project example.com/app MUST be preserved in the SBOM"
    );
}

// ============ T016 — install-path independence ============

#[test]
fn install_path_independence_opt_go() {
    // Synthetic tempdir with Go toolchain at /opt/go (NOT /usr/local/go).
    // Proves FR-005: detection is module-path-based, not install-path-based.
    let scratch = tempdir().expect("fixture tempdir");
    let src = scratch.path().join("opt/go/src");
    std::fs::create_dir_all(&src).expect("mkdir src");
    std::fs::write(src.join("go.mod"), b"module std\n\ngo 1.26\n").unwrap();

    let out_dir = tempdir().expect("out tempdir");
    let out_path = out_dir.path().join("waybill.cdx.json");
    let (ok, _stdout, stderr) = run_scan(
        &scratch.path().to_path_buf(),
        &[
            "--format",
            "cyclonedx-json",
            "--output",
            out_path.to_str().unwrap(),
        ],
    );
    assert!(ok, "scan failed:\n{stderr}");

    let text = std::fs::read_to_string(&out_path).expect("read cdx");
    let v: serde_json::Value = serde_json::from_str(&text).expect("parse cdx");
    let bad = v["components"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c["purl"].as_str())
                .filter(|p| p.starts_with("pkg:golang/std@"))
                .count()
        })
        .unwrap_or(0);
    assert_eq!(
        bad, 0,
        "FR-005: filter must fire regardless of install path (opt/go, not /usr/local/go); got {bad} stdlib components"
    );
}

// ============ T021 — SC-005 annotation present ============

#[test]
fn go_toolchain_detected_annotation_present() {
    let out_dir = tempdir().expect("out tempdir");
    let out_path = out_dir.path().join("waybill.cdx.json");
    let (ok, _stdout, stderr) = run_scan(
        &m217_goroot_stub_fixture(),
        &[
            "--format",
            "cyclonedx-json",
            "--output",
            out_path.to_str().unwrap(),
        ],
    );
    assert!(ok, "scan failed:\n{stderr}");

    let v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&out_path).expect("read cdx"),
    )
    .expect("parse cdx");

    // C136 annotation on document metadata.properties[].
    let props = v
        .pointer("/metadata/properties")
        .and_then(|s| s.as_array())
        .expect("metadata.properties array");
    let ann_value = props
        .iter()
        .find(|p| p["name"].as_str() == Some("waybill:go-toolchain-detected"))
        .and_then(|p| p["value"].as_str())
        .expect("waybill:go-toolchain-detected annotation present (SC-005)");

    // Value is a JSON array; parse and assert at least one entry ends
    // with `usr/local/go` (the fixture's GOROOT layout).
    let paths: Vec<String> =
        serde_json::from_str(ann_value).expect("annotation value parses as JSON array");
    assert!(
        !paths.is_empty(),
        "SC-005: annotation must have at least one path when emitted"
    );
    assert!(
        paths.iter().any(|p| p.ends_with("usr/local/go")),
        "SC-005: expected an entry ending with 'usr/local/go', got {paths:?}"
    );
}

// ============ T022 — annotation absent on no-toolchain scans ============

#[test]
fn annotation_absent_when_no_toolchain() {
    // Fixture: user Go project only, no stubbed toolchain.
    let scratch = tempdir().expect("fixture tempdir");
    let app = scratch.path().join("app");
    std::fs::create_dir_all(&app).expect("mkdir app");
    std::fs::write(app.join("go.mod"), b"module example.com/only\n\ngo 1.22\n").unwrap();
    std::fs::write(app.join("go.sum"), b"").unwrap();
    std::fs::write(app.join("main.go"), b"package main\n\nfunc main() {}\n").unwrap();

    let out_dir = tempdir().expect("out tempdir");
    let out_path = out_dir.path().join("waybill.cdx.json");
    let (ok, _stdout, stderr) = run_scan(
        &scratch.path().to_path_buf(),
        &[
            "--format",
            "cyclonedx-json",
            "--output",
            out_path.to_str().unwrap(),
        ],
    );
    assert!(ok, "scan failed:\n{stderr}");

    let v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&out_path).expect("read cdx"),
    )
    .expect("parse cdx");
    let props = v
        .pointer("/metadata/properties")
        .and_then(|s| s.as_array())
        .expect("metadata.properties array");
    let ann_present = props
        .iter()
        .any(|p| p["name"].as_str() == Some("waybill:go-toolchain-detected"));
    assert!(
        !ann_present,
        "silence-on-no-observation: annotation MUST be absent when no toolchain is observed"
    );
}
