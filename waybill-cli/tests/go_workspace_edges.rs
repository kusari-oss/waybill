//! Milestone 161 (T050): SC-010 integration test — Go workspace-mode
//! C112 annotation end-to-end via the release binary.
//!
//! Synthesizes a 3-module Go workspace in a tempdir:
//!
//!   base/         — leaf library (`example.com/base v0.1.0`)
//!   middle/       — depends on base (`example.com/middle v0.1.0`)
//!   leaf/         — depends on middle (`example.com/leaf v0.1.0`)
//!
//! Wraps them in a `go.work` file with `use ( ./base ./middle ./leaf )`.
//!
//! Then invokes the release binary in `--offline` mode and asserts:
//!   (a) doc-scope `waybill:go-workspace-mode = "detected: 3 use-modules"`;
//!   (b) parity-catalog registration is exercised via at least one
//!       emitted C112 property in `metadata.properties[]`.
//!
//! Rather than exercising the Q1 hybrid edge-attribution logic (which
//! is FR-007 follow-on work per spec.md Assumptions §7), this test
//! focuses on the doc-scope emission surface — the parts that ARE
//! implemented in the milestone-161 structural PR.

use std::path::PathBuf;
use std::process::Command;

fn write_workspace_fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    // Workspace-level go.work.
    std::fs::write(
        root.join("go.work"),
        "go 1.24\n\nuse (\n    ./base\n    ./middle\n    ./leaf\n)\n",
    )
    .expect("write go.work");

    // Base library.
    std::fs::create_dir_all(root.join("base")).unwrap();
    std::fs::write(
        root.join("base/go.mod"),
        "module example.com/base\n\ngo 1.24\n",
    )
    .unwrap();
    std::fs::write(root.join("base/go.sum"), "").unwrap();
    std::fs::write(
        root.join("base/lib.go"),
        "package base\n\nfunc Hello() string { return \"hi\" }\n",
    )
    .unwrap();

    // Middle library depending on base.
    std::fs::create_dir_all(root.join("middle")).unwrap();
    std::fs::write(
        root.join("middle/go.mod"),
        "module example.com/middle\n\ngo 1.24\n\nrequire example.com/base v0.0.0\n",
    )
    .unwrap();
    std::fs::write(root.join("middle/go.sum"), "").unwrap();
    std::fs::write(
        root.join("middle/lib.go"),
        "package middle\n\nfunc Middle() string { return \"middle\" }\n",
    )
    .unwrap();

    // Leaf app depending on middle.
    std::fs::create_dir_all(root.join("leaf")).unwrap();
    std::fs::write(
        root.join("leaf/go.mod"),
        "module example.com/leaf\n\ngo 1.24\n\nrequire example.com/middle v0.0.0\n",
    )
    .unwrap();
    std::fs::write(root.join("leaf/go.sum"), "").unwrap();
    std::fs::write(
        root.join("leaf/main.go"),
        "package main\n\nfunc main() {}\n",
    )
    .unwrap();

    tmp
}

fn scan_offline(path: &std::path::Path) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let out_path = tmp.path().to_path_buf();
    let mut cmd = Command::new(bin);
    cmd.env("WAYBILL_NO_GO_MOD_WHY", "1");
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");
    let output = cmd.output().expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn doc_property<'a>(sbom: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    sbom["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)?
        ["value"]
        .as_str()
}

/// SC-004 + SC-005: doc-scope C112 present with `detected: 3 use-modules`
/// value on the synthesized fixture.
#[test]
fn t050_workspace_scan_emits_c112_annotation() {
    let fixture = write_workspace_fixture();
    let sbom = scan_offline(fixture.path());

    let c112 = doc_property(&sbom, "waybill:go-workspace-mode");
    assert_eq!(
        c112,
        Some("detected: 3 use-modules"),
        "SC-005: C112 value must reflect the 3 use directives from go.work; got {c112:?}"
    );
}

/// SC-004: doc-scope C112 present when a `go.work` file exists.
#[test]
fn t050_c112_present_iff_go_work_exists() {
    // With go.work → present.
    let fixture = write_workspace_fixture();
    let sbom_with_workspace = scan_offline(fixture.path());
    assert!(
        doc_property(&sbom_with_workspace, "waybill:go-workspace-mode").is_some(),
        "SC-004: C112 MUST be present when go.work exists"
    );

    // Delete go.work and re-scan — C112 must vanish (byte-identity guard).
    std::fs::remove_file(fixture.path().join("go.work")).unwrap();
    let sbom_without_workspace = scan_offline(fixture.path());
    assert!(
        doc_property(&sbom_without_workspace, "waybill:go-workspace-mode").is_none(),
        "SC-003: C112 MUST be absent when go.work does NOT exist"
    );
}

/// Regression guard: workspace-mode SBOM still carries the milestone-160
/// Go-transitive annotations (interop between milestones 160 + 161).
#[test]
fn t050_workspace_scan_preserves_milestone_160_annotations() {
    let fixture = write_workspace_fixture();
    let sbom = scan_offline(fixture.path());
    // Milestone-160 doc-scope C110 (go-transitive-coverage) should be
    // present on any Go-containing scan.
    let c110 = doc_property(&sbom, "waybill:go-transitive-coverage");
    assert!(
        c110.is_some(),
        "Milestone 160 interop: C110 must remain emitted alongside C112"
    );
    // The exact value depends on resolution — in offline mode it will
    // be "unknown" with an offline-mode reason. Sanity-check the value
    // is one of the three legal values.
    let c110_val = c110.unwrap();
    assert!(
        matches!(c110_val, "complete" | "partial" | "unknown"),
        "C110 value must follow the milestone-160 vocab; got {c110_val:?}"
    );
    let _ = PathBuf::from(fixture.path()); // hold tempdir until end
}
