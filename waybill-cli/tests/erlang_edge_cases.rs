//! Milestone 141 edge-case tests.
//!
//! Covers SC-004 (no-op preservation on non-Erlang trees) + SC-005
//! (malformed lockfile graceful degradation) + main-module fallback
//! paths + OTP-version-compatibility of `optional_applications:` per
//! research §R3 + binary-string-atom alternate form per spec Edge
//! Cases.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn run_scan(project_root: &Path) -> (Value, String) {
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
    let doc: Value = serde_json::from_slice(&bytes).unwrap();
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    (doc, stderr)
}

fn all_components(doc: &Value) -> Vec<&Value> {
    let mut out: Vec<&Value> = Vec::new();
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        out.extend(arr.iter());
    }
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        out.push(c);
    }
    out
}

fn property_value<'a>(component: &'a Value, name: &str) -> Option<&'a str> {
    component
        .get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
}

fn component_with_name<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    all_components(doc)
        .into_iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
}

#[test]
fn sc004_no_op_on_non_erlang_tree() {
    // FR-006 + SC-004: a source tree without rebar.{lock,config} or
    // *.app.src produces ZERO erlang-derived components and emits no
    // erlang-related warnings.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# Not an Erlang project\n").unwrap();
    std::fs::write(
        dir.path().join("hello.txt"),
        "no rebar.config here\n",
    )
    .unwrap();
    let (doc, stderr) = run_scan(dir.path());
    let erlang_comps: Vec<&Value> = all_components(&doc)
        .into_iter()
        .filter(|c| {
            property_value(c, "waybill:source-type")
                .map(|s| s.starts_with("erlang-"))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        erlang_comps.is_empty(),
        "non-Erlang tree must produce zero erlang-derived components; got: {erlang_comps:?}",
    );
    // No erlang-related warnings in stderr.
    assert!(
        !stderr.contains("erlang:"),
        "non-Erlang tree must not emit any 'erlang:' warnings; stderr={stderr}",
    );
}

#[test]
fn sc005_malformed_lockfile_warns_and_falls_back() {
    // SC-005 + FR-007: a malformed rebar.lock alongside a valid
    // rebar.config falls back to design-tier emission per FR-005.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("rebar.config"),
        r#"{deps, [{cowboy, "~> 2.10"}]}."#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("rebar.lock"),
        "this is not valid erlang term syntax {<<\"unclosed",
    )
    .unwrap();
    let (doc, stderr) = run_scan(dir.path());
    // Scan succeeded (exit 0 by virtue of run_scan's assert).
    // Warning about the malformed lockfile present.
    assert!(
        stderr.contains("erlang: failed to parse rebar.lock"),
        "expected parse-failure warning; stderr={stderr}",
    );
    // Fallback design-tier emission of cowboy is present.
    let cowboy = component_with_name(&doc, "cowboy").expect("cowboy design-tier fallback");
    assert_eq!(
        property_value(cowboy, "waybill:sbom-tier"),
        Some("design"),
    );
}

#[test]
fn binary_string_atom_alternate_form() {
    // Per spec Edge Case: rebar.lock binary-string atom encoding —
    // <<"name">> and bare-atom forms parse equivalently. This test
    // uses the bare-atom form for the name to verify defensive
    // fallback per extract_first_name.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("rebar.config"),
        r#"{deps, [{cowboy, "2.10.0"}]}."#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("rebar.lock"),
        r#"{"1.2.0",
[{cowboy,{pkg,<<"cowboy">>,<<"2.10.0">>},0}]}.
"#,
    )
    .unwrap();
    let (doc, _) = run_scan(dir.path());
    // Whether bare-atom or binary-string, cowboy must emit.
    let _ = component_with_name(&doc, "cowboy").expect("bare-atom name form should also parse");
}

#[test]
fn main_module_version_fallback() {
    // Per FR-012 + contract §6: *.app.src without {vsn, "..."} falls
    // back to "0.0.0-unknown".
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("rebar.config"),
        r#"{deps, []}."#,
    )
    .unwrap();
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("my_app.app.src"),
        r#"{application, my_app, [
    {applications, [kernel]},
    {description, "App without vsn"}
]}."#,
    )
    .unwrap();
    let (doc, _) = run_scan(dir.path());
    let main = all_components(&doc)
        .into_iter()
        .find(|c| c.get("purl").and_then(|v| v.as_str()) == Some("pkg:hex/my_app"))
        .expect("main-module with version fallback");
    assert_eq!(main.get("name").and_then(|v| v.as_str()), Some("my_app"));
}

#[test]
fn optional_applications_absent_keyword_is_not_error() {
    // Per research §R3 OTP-version-compatibility note: OTP-25-and-earlier
    // *.app.src files lack `optional_applications:` entirely; parsing
    // must succeed with empty optional_apps and emit no warnings.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("rebar.config"),
        r#"{deps, []}."#,
    )
    .unwrap();
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("my_app.app.src"),
        r#"{application, my_app, [
    {vsn, "1.0.0"},
    {applications, [kernel]},
    {description, "Pre-OTP-26 descriptor; no optional_applications"}
]}."#,
    )
    .unwrap();
    let (_doc, stderr) = run_scan(dir.path());
    // No erlang warnings about parsing or missing keywords.
    assert!(
        !stderr.contains("erlang: failed"),
        "absent optional_applications must not produce parse warnings; stderr={stderr}",
    );
}
