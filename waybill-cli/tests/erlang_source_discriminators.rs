//! Milestone 141 US2 — source-discriminator + main-module integration tests.
//!
//! Covers SC-002 (hex/git/OTP-runtime PURL distinction) +
//! SC-008 (main-module emission from *.app.src).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn run_scan(project_root: &Path) -> Value {
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

fn component_with_purl<'a>(doc: &'a Value, purl: &str) -> Option<&'a Value> {
    all_components(doc)
        .into_iter()
        .find(|c| c.get("purl").and_then(|v| v.as_str()) == Some(purl))
}

#[test]
fn sc002_hex_git_otp_runtime_discrimination() {
    let dir = tempfile::tempdir().unwrap();
    let sha = "a".repeat(64);
    std::fs::write(
        dir.path().join("rebar.config"),
        r#"{deps, [
    {cowboy, "2.10.0"},
    {my_fork, {git, "https://github.com/foo/my-fork.git", {ref, "eb39649a76b87e8451baf75d10ce82ca3a3d5601"}}}
]}."#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("rebar.lock"),
        format!(
            r#"{{"1.2.0",
[{{<<"cowboy">>,{{pkg,<<"cowboy">>,<<"2.10.0">>,<<"{sha}">>}},0}},
 {{<<"my_fork">>,{{git,"https://github.com/foo/my-fork.git",{{ref,"eb39649a76b87e8451baf75d10ce82ca3a3d5601"}}}},0}}]}}.
"#
        ),
    )
    .unwrap();
    // *.app.src declares OTP runtime libs.
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("my_app.app.src"),
        r#"{application, my_app, [
    {vsn, "1.0.0"},
    {applications, [kernel, stdlib, cowboy]},
    {description, "src/app for SC-002"}
]}."#,
    )
    .unwrap();

    let doc = run_scan(dir.path());

    // Hex default-org PURL:
    let cowboy = component_with_purl(&doc, "pkg:hex/cowboy@2.10.0").expect("cowboy hex component");
    assert_eq!(
        property_value(cowboy, "waybill:source-type"),
        Some("erlang-hex"),
    );

    // Git PURL pattern (pkg:generic/ + vcs_url=git+ + resolved SHA):
    let git_purl = "pkg:generic/my_fork@eb39649a76b87e8451baf75d10ce82ca3a3d5601?vcs_url=git+https://github.com/foo/my-fork.git";
    let my_fork = component_with_purl(&doc, git_purl).expect("git component");
    assert_eq!(
        property_value(my_fork, "waybill:source-type"),
        Some("erlang-git"),
    );
    assert_eq!(
        property_value(my_fork, "waybill:vcs-declared-ref"),
        Some("ref"),
    );

    // OTP-runtime placeholders for kernel + stdlib (NOT in lockfile).
    let kernel = component_with_purl(&doc, "pkg:generic/kernel@unspecified")
        .expect("kernel OTP runtime placeholder");
    assert_eq!(
        property_value(kernel, "waybill:source-type"),
        Some("erlang-otp-runtime"),
    );
    assert_eq!(
        property_value(kernel, "waybill:otp-stdlib"),
        Some("true"),
        "kernel is in OTP_STDLIB_ALLOWLIST per Q1",
    );

    let stdlib = component_with_purl(&doc, "pkg:generic/stdlib@unspecified")
        .expect("stdlib OTP runtime placeholder");
    assert_eq!(
        property_value(stdlib, "waybill:otp-stdlib"),
        Some("true"),
    );
}

#[test]
fn sc002_private_org_map_form_repository_url_qualifier() {
    let dir = tempfile::tempdir().unwrap();
    let sha = "b".repeat(64);
    std::fs::write(
        dir.path().join("rebar.config"),
        r#"{deps, [{internal_lib, "2.0.0"}]}."#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("rebar.lock"),
        format!(
            r#"{{"1.2.0",
[{{<<"internal_lib">>,{{pkg,<<"internal_lib">>,<<"2.0.0">>,<<"{sha}">>,#{{repo => <<"hexpm:acme">>}}}},0}}]}}.
"#
        ),
    )
    .unwrap();
    let doc = run_scan(dir.path());
    let purl = "pkg:hex/acme/internal_lib@2.0.0?repository_url=https://repo.hex.pm";
    let comp = component_with_purl(&doc, purl).expect(
        "private-org component should emit with namespace + repository_url qualifier per milestone-140 R1",
    );
    assert_eq!(comp.get("name").and_then(|v| v.as_str()), Some("internal_lib"));
    assert_eq!(comp.get("version").and_then(|v| v.as_str()), Some("2.0.0"));
}

#[test]
fn sc002_legacy_hex_shape_no_hash() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("rebar.config"),
        r#"{deps, [{lager, "3.9.2"}]}."#,
    )
    .unwrap();
    // Pre-rebar3-3.7 flat shape.
    std::fs::write(
        dir.path().join("rebar.lock"),
        r#"{"1.1.0",
[{<<"lager">>,<<"3.9.2">>,0}]}.
"#,
    )
    .unwrap();
    let doc = run_scan(dir.path());
    let lager = component_with_purl(&doc, "pkg:hex/lager@3.9.2").expect("lager legacy component");
    let hashes = lager.get("hashes").and_then(|v| v.as_array());
    // Per FR-011 best-effort posture, no hash entries when lockfile
    // doesn't carry one.
    assert!(
        hashes.map(|a| a.is_empty()).unwrap_or(true),
        "legacy-shape lockfile entries carry no inner SHA-256",
    );
}

#[test]
fn sc002_git_tag_and_branch_forms() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("rebar.config"),
        r#"{deps, [
    {my_tag_dep, {git, "https://example.com/x.git", {tag, "v1.2.3"}}},
    {my_branch_dep, {git, "https://example.com/y.git", {branch, "main"}}}
]}."#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("rebar.lock"),
        r#"{"1.2.0",
[{<<"my_tag_dep">>,{git,"https://example.com/x.git",{tag,"v1.2.3"}},0},
 {<<"my_branch_dep">>,{git,"https://example.com/y.git",{branch,"main"}},0}]}.
"#,
    )
    .unwrap();
    let doc = run_scan(dir.path());

    let tag_purl =
        "pkg:generic/my_tag_dep@v1.2.3?vcs_url=git+https://example.com/x.git";
    let tag_comp = component_with_purl(&doc, tag_purl).expect("tag-form git component");
    assert_eq!(
        property_value(tag_comp, "waybill:vcs-declared-ref"),
        Some("tag"),
    );

    let branch_purl =
        "pkg:generic/my_branch_dep@main?vcs_url=git+https://example.com/y.git";
    let branch_comp = component_with_purl(&doc, branch_purl).expect("branch-form git component");
    assert_eq!(
        property_value(branch_comp, "waybill:vcs-declared-ref"),
        Some("branch"),
    );
}

#[test]
fn sc008_main_module_emission_from_app_src() {
    let dir = tempfile::tempdir().unwrap();
    let sha = "c".repeat(64);
    std::fs::write(
        dir.path().join("rebar.config"),
        r#"{deps, [{cowboy, "2.10.0"}]}."#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("rebar.lock"),
        format!(
            r#"{{"1.2.0",
[{{<<"cowboy">>,{{pkg,<<"cowboy">>,<<"2.10.0">>,<<"{sha}">>}},0}}]}}.
"#
        ),
    )
    .unwrap();
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("my_app.app.src"),
        r#"{application, my_app, [
    {vsn, "1.2.3"},
    {applications, [kernel, stdlib, cowboy]},
    {description, "Main-module fixture"}
]}."#,
    )
    .unwrap();
    let doc = run_scan(dir.path());

    let main_module = component_with_purl(&doc, "pkg:hex/my_app@1.2.3")
        .expect("main-module component should emit");
    assert_eq!(
        property_value(main_module, "waybill:component-role"),
        Some("main-module"),
    );
    // Note: waybill:source-type may be absent on the promoted
    // metadata.component subject — the source_type field is emitted
    // only by builder.rs into components[]. metadata.rs's curated
    // propagation list does not include waybill:source-type today; this
    // matches the milestone-140 elixir main_module test convention which
    // also asserts only waybill:component-role on the promoted subject.
    // Future cross-reader work could promote source-type propagation
    // generically; not in scope for milestone 141.
}
