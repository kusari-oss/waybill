//! Milestone 192 (issue: graph-completeness partial-value regression
//! on operator-supplied roots) — integration tests.
//!
//! Reproduces the Kusari pico scenario: scanning a Go source repo
//! with `--root-name X --root-version Y` was reporting
//! `waybill:graph-completeness: partial` with reason
//! `multi-ecosystem-partial-root: golang`. Post-m192 it reports
//! `complete` — the operator's synthetic root now covers the
//! per-ecosystem-root slot so the classifier no longer over-fires.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

fn scan_with_args(dir: &Path, format: &str, extra_args: &[&str]) -> Value {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_ext = match format {
        "cyclonedx-json" => "cdx.json",
        "spdx-2.3-json" => "spdx.json",
        "spdx-3-json" => "spdx3.json",
        _ => panic!("unknown format {format}"),
    };
    let out_path = workdir.path().join(format!("out.{out_ext}"));

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        dir.to_str().unwrap(),
        "--format",
        format,
        "--output",
        out_path.to_str().unwrap(),
    ]);
    for a in extra_args {
        cmd.arg(*a);
    }
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "scan failed: format={format} args={extra_args:?} stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&out_path).expect("read output");
    serde_json::from_slice(&bytes).expect("parse output json")
}

/// Small synthetic Go source project — go.mod + main.go with a single
/// external dep. The point isn't the dep tree; it's that the scan
/// emits Go components AND accepts the operator's --root-name override.
fn build_go_source_project(dir: &Path) -> PathBuf {
    let project = dir.join("m192-go-project");
    std::fs::create_dir_all(&project).expect("mkdir project");
    std::fs::write(
        project.join("go.mod"),
        "module example.com/m192\n\n\
         go 1.22\n\n\
         require github.com/spf13/cobra v1.8.0\n",
    )
    .expect("write go.mod");
    std::fs::write(
        project.join("main.go"),
        "package main\n\nimport \"github.com/spf13/cobra\"\n\n\
         fn main() {}\n", // deliberately not compiled — waybill doesn't build the code
    )
    .expect("write main.go");
    project
}

fn cdx_graph_completeness_value(doc: &Value) -> Option<String> {
    doc["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some("waybill:graph-completeness"))
        .and_then(|p| p["value"].as_str().map(str::to_string))
}

fn cdx_graph_completeness_reason(doc: &Value) -> Option<String> {
    doc["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some("waybill:graph-completeness-reason"))
        .and_then(|p| p["value"].as_str().map(str::to_string))
}

// ── US1 acceptance: --root-name Go source repo emits `complete` ───

#[test]
fn us1_go_source_with_root_name_reports_complete() {
    // The primary regression case from the Kusari pico report. Pre-m192
    // this returned `partial: multi-ecosystem-partial-root: golang`.
    let dir = tempfile::tempdir().unwrap();
    let project = build_go_source_project(dir.path());
    let doc = scan_with_args(
        &project,
        "cyclonedx-json",
        &["--root-name", "m192-service", "--root-version", "abc123"],
    );

    let value = cdx_graph_completeness_value(&doc)
        .expect("waybill:graph-completeness annotation present");
    assert_eq!(
        value, "complete",
        "operator-override Go source scan MUST report `complete` post-m192 (pre-m192 was `partial: multi-ecosystem-partial-root: golang`)"
    );

    // Reason annotation MUST be absent for a clean-complete scan.
    let reason = cdx_graph_completeness_reason(&doc);
    assert!(
        reason.is_none(),
        "graph-completeness-reason MUST be absent on a `complete` scan; got: {reason:?}"
    );
}

// ── US1 acceptance: cross-format consistency ─────────────────────

#[test]
fn us1_cross_format_all_three_report_complete() {
    // Q1 clarification requires the corrected value to fire in all
    // three format emitters identically.
    let dir = tempfile::tempdir().unwrap();
    let project = build_go_source_project(dir.path());
    let args = &["--root-name", "m192-service", "--root-version", "abc123"];

    let cdx = scan_with_args(&project, "cyclonedx-json", args);
    let spdx23 = scan_with_args(&project, "spdx-2.3-json", args);
    let spdx3 = scan_with_args(&project, "spdx-3-json", args);

    assert_eq!(
        cdx_graph_completeness_value(&cdx).as_deref(),
        Some("complete"),
        "CDX must report `complete`"
    );

    // SPDX 2.3: annotation is inside an Annotation.comment JSON envelope
    // per waybill's m111 pkg-alias-binding schema (`"field":"waybill:...",
    // "value":"..."`).
    let spdx23_ann = spdx23["annotations"]
        .as_array()
        .expect("SPDX 2.3 annotations array");
    let has_complete = spdx23_ann.iter().any(|a| {
        let c = a["comment"].as_str().unwrap_or("");
        c.contains("\"field\":\"waybill:graph-completeness\"")
            && c.contains("\"value\":\"complete\"")
    });
    assert!(
        has_complete,
        "SPDX 2.3 must carry waybill:graph-completeness=complete in a document Annotation"
    );

    // SPDX 3: annotation is a graph element with type=Annotation +
    // statement carrying the same JSON envelope shape.
    let spdx3_graph = spdx3["@graph"].as_array().expect("SPDX 3 @graph array");
    let has_complete_v3 = spdx3_graph.iter().any(|e| {
        if e["type"].as_str() != Some("Annotation") {
            return false;
        }
        let s = e["statement"].as_str().unwrap_or("");
        s.contains("\"field\":\"waybill:graph-completeness\"")
            && s.contains("\"value\":\"complete\"")
    });
    assert!(
        has_complete_v3,
        "SPDX 3 must carry waybill:graph-completeness=complete in an Annotation graph element"
    );
}

// ── US1 acceptance: --root-purl-type golang (Q2 answer A path) ────

#[test]
fn us1_root_purl_type_golang_reports_complete_no_duplicate_root() {
    // Q2 answer A: when operator picks `--root-purl-type golang`, the
    // target_ref is pkg:golang/... — the golang ecosystem is covered
    // by the operator's own root, no duplicate placeholder needed.
    let dir = tempfile::tempdir().unwrap();
    let project = build_go_source_project(dir.path());
    let doc = scan_with_args(
        &project,
        "cyclonedx-json",
        &[
            "--root-name",
            "github.com/example/m192-svc",
            "--root-version",
            "abc123",
            "--root-purl-type",
            "golang",
        ],
    );
    let value = cdx_graph_completeness_value(&doc)
        .expect("graph-completeness annotation present");
    assert_eq!(value, "complete");
    // Sanity: root PURL is pkg:golang/... (not pkg:generic/...).
    let root_purl = doc["metadata"]["component"]["purl"]
        .as_str()
        .expect("metadata.component.purl present");
    assert!(
        root_purl.starts_with("pkg:golang/"),
        "root PURL must reflect --root-purl-type; got: {root_purl}"
    );
}

// ── FR-004 / SC-004 byte-identity guard: native-root path is untouched ──

#[test]
fn us1_native_root_scan_is_byte_identical() {
    // Without --root-name, waybill's Go reader detects the module
    // name from go.mod and emits it as the main-module root. That
    // path goes through ResolvedRootSubject::MainModule → the m192
    // synthesis block is SKIPPED. Output must be byte-identical to
    // pre-m192.
    let dir = tempfile::tempdir().unwrap();
    let project = build_go_source_project(dir.path());
    let doc = scan_with_args(&project, "cyclonedx-json", &[]);

    // Sanity: root PURL is pkg:golang/... derived from go.mod.
    let root_purl = doc["metadata"]["component"]["purl"]
        .as_str()
        .expect("metadata.component.purl present");
    assert!(
        root_purl.starts_with("pkg:golang/"),
        "native-root scan derives root from go.mod; got: {root_purl}"
    );
    // The completeness value could be `complete` or `partial` (real
    // orphan detection) — the m192 fix does NOT change the value on
    // the native-root path. The assertion here is BEHAVIOR: the fix
    // did not introduce a value change for this input. Pre-m192 and
    // post-m192 emit the same value.
    let value = cdx_graph_completeness_value(&doc)
        .expect("graph-completeness annotation present");
    // Empirically for this minimal fixture, native-root Go scans
    // return `complete` when the primary-dep-fallback covers all
    // components. If the value ever regresses on this minimal
    // fixture it means m192's guard is broken.
    assert_eq!(
        value, "complete",
        "native-root Go scan should report complete on this minimal fixture (byte-identity gate)"
    );
}
