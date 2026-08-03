//! Milestone 225: integration tests for the Pants shell reader.
//! Each test invokes waybill as a subprocess against a synthetic
//! fixture and asserts the emitted SBOM contains the expected
//! components + annotations + log lines.
//!
//! Fixtures live at `waybill-cli/tests/fixtures/pants_shell/` per
//! T015-T037a. Every script uses synthetic `waybill-fixture-*.sh`
//! naming per memory `feedback_fixture_synthetic_package_names`.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::bin;

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pants_shell")
        .join(rel)
}

fn run_scan(
    fixture_path: &Path,
    output: &Path,
    extra_args: &[&str],
) -> std::process::Output {
    let mut cmd = Command::new(bin());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture_path)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(output)
        .arg("--no-deep-hash")
        .env("RUST_LOG", "info");
    for a in extra_args {
        cmd.arg(a);
    }
    cmd.output().expect("waybill invocation")
}

fn read_cdx(path: &Path) -> serde_json::Value {
    let raw = std::fs::read(path).expect("read cdx");
    serde_json::from_slice(&raw).expect("parse cdx")
}

fn get_property<'a>(component: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    component
        .get("properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value"))
        .and_then(|v| v.as_str())
}

/// Filter to just the pants-shell-emitted components (those whose PURL
/// is `pkg:generic/<basename>@<sha[:12]>` AND carries a
/// `waybill:pants-target` property).
fn pants_shell_components(cdx: &serde_json::Value) -> Vec<&serde_json::Value> {
    cdx.get("components")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter(|c| get_property(c, "waybill:pants-target").is_some())
        .collect()
}

fn strip_ansi(s: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").expect("valid regex");
    re.replace_all(s, "").to_string()
}

// ---------------------------------------------------------------------
// US1 T018 — minimal shell_source + shell_sources emits 2 components
// with sha256 hashes and target annotation (BOTH CDX and SPDX 2.3 in
// one scan invocation per SC-001).
// ---------------------------------------------------------------------

#[test]
fn us1_minimal_scripts_emits_2_components_with_sha256_and_target_annotation() {
    let fixture_dir = fixture("minimal_scripts");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let spdx_path = tmp.path().join("out.spdx.json");

    let out = Command::new(bin())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(&fixture_dir)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx_path.display()))
        .arg("--no-deep-hash")
        .env("RUST_LOG", "info")
        .output()
        .expect("waybill invocation");
    assert!(
        out.status.success(),
        "waybill exited nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    // ---- CDX assertions ----
    let cdx = read_cdx(&cdx_path);
    let shell = pants_shell_components(&cdx);
    assert_eq!(
        shell.len(),
        2,
        "expected 2 pants-shell components, got {} — purls: {:?}",
        shell.len(),
        shell.iter().filter_map(|c| c.get("purl").and_then(|v| v.as_str())).collect::<Vec<_>>(),
    );
    for c in &shell {
        // sha256 hash present
        let hashes = c.get("hashes").and_then(|v| v.as_array()).expect("hashes[]");
        assert!(
            hashes.iter().any(|h| {
                h.get("alg").and_then(|v| v.as_str()) == Some("SHA-256")
            }),
            "component missing SHA-256: {:?}",
            c.get("purl"),
        );
        // waybill:pants-target annotation present
        assert!(
            get_property(c, "waybill:pants-target").is_some(),
            "component missing waybill:pants-target: {:?}",
            c.get("purl"),
        );
    }

    // Multi-owner assertion (dupe-owner case handled correctly):
    // deploy.sh is owned by BOTH scripts:deploy (explicit source=) AND
    // scripts:utils (glob) — dedup pass should emit ONE component with
    // both addresses, lex-sorted comma-sep.
    let by_name: std::collections::HashMap<&str, &serde_json::Value> = shell
        .iter()
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()).map(|n| (n, *c)))
        .collect();
    let deploy = by_name
        .get("waybill-fixture-deploy.sh")
        .expect("deploy component present");
    let rollback = by_name
        .get("waybill-fixture-rollback.sh")
        .expect("rollback component present");
    assert_eq!(
        get_property(deploy, "waybill:pants-target"),
        Some("scripts:deploy,scripts:utils"),
        "deploy.sh should carry both owning target addresses",
    );
    assert_eq!(
        get_property(rollback, "waybill:pants-target"),
        Some("scripts:utils"),
        "rollback.sh should carry only the utils owner",
    );

    // ---- Compute expected sha256 to verify hash correctness ----
    use sha2::Digest;
    let deploy_bytes =
        std::fs::read(fixture_dir.join("scripts/waybill-fixture-deploy.sh")).unwrap();
    let mut h = sha2::Sha256::new();
    h.update(&deploy_bytes);
    let expected_sha = format!("{:x}", h.finalize());
    let deploy_hashes = deploy.get("hashes").and_then(|v| v.as_array()).unwrap();
    let deploy_sha = deploy_hashes
        .iter()
        .find_map(|h| {
            (h.get("alg").and_then(|v| v.as_str()) == Some("SHA-256"))
                .then(|| h.get("content").and_then(|v| v.as_str()))
        })
        .flatten()
        .expect("deploy SHA-256 present");
    assert_eq!(deploy_sha, expected_sha, "deploy sha256 mismatch");

    // ---- SPDX 2.3 assertions ----
    let spdx = read_cdx(&spdx_path);
    let packages = spdx.get("packages").and_then(|v| v.as_array()).expect("packages[]");
    let shell_pkgs: Vec<&serde_json::Value> = packages
        .iter()
        .filter(|p| {
            p.get("externalRefs")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .any(|r| {
                    r.get("referenceLocator")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| {
                            s.starts_with("pkg:generic/waybill-fixture-")
                        })
                })
        })
        .collect();
    assert_eq!(shell_pkgs.len(), 2, "expected 2 SPDX packages");
    for p in &shell_pkgs {
        let checksums = p.get("checksums").and_then(|v| v.as_array()).expect("checksums[]");
        assert!(
            checksums.iter().any(|c| {
                c.get("algorithm").and_then(|v| v.as_str()) == Some("SHA256")
            }),
            "SPDX package missing SHA256",
        );
    }
}

// ---------------------------------------------------------------------
// US1 T019 — glob sources expands to 3 components, all carrying the
// same target annotation
// ---------------------------------------------------------------------

#[test]
fn us1_glob_sources_expands_to_3_components() {
    let fixture_dir = fixture("glob_sources");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let shell = pants_shell_components(&cdx);
    assert_eq!(shell.len(), 3, "expected 3 pants-shell components");

    let mut shas: Vec<String> = Vec::new();
    for c in &shell {
        assert_eq!(
            get_property(c, "waybill:pants-target"),
            Some("helpers:utils"),
            "all 3 should carry the single owning target",
        );
        let hashes = c.get("hashes").and_then(|v| v.as_array()).unwrap();
        for h in hashes {
            if let Some(sha) = h.get("content").and_then(|v| v.as_str()) {
                shas.push(sha.to_string());
            }
        }
    }
    shas.sort();
    shas.dedup();
    assert_eq!(shas.len(), 3, "expected 3 distinct sha256 values (proves glob resolved to 3 files)");
}

// ---------------------------------------------------------------------
// US1 T020 — FR-010 INFO log includes all 6 structured fields
// ---------------------------------------------------------------------

// ---------------------------------------------------------------------
// US2 T025 — pants.toml [shellcheck]/[shfmt]/[shunit2] pins emit design-tier
// tool components alongside the script components from BUILD-file walk
// ---------------------------------------------------------------------

#[test]
fn us2_pants_toml_pins_emit_design_tier_tool_components() {
    let fixture_dir = fixture("with_shell_setup");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let all: Vec<&serde_json::Value> = cdx
        .get("components")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .collect();

    // 3 tool components at expected PURLs.
    let expected_tool_purls: std::collections::HashSet<&str> = [
        "pkg:generic/shellcheck@v0.9.0",
        "pkg:generic/shfmt@v3.7.0",
        "pkg:generic/shunit2@2.1.8",
    ]
    .iter()
    .copied()
    .collect();
    let actual_tool_purls: std::collections::HashSet<&str> = all
        .iter()
        .filter_map(|c| c.get("purl").and_then(|v| v.as_str()))
        .filter(|p| expected_tool_purls.contains(p))
        .collect();
    assert_eq!(
        actual_tool_purls, expected_tool_purls,
        "tool component PURL set mismatch",
    );

    // Each tool: design tier + waybill:source-file=pants.toml.
    for tool_purl in &expected_tool_purls {
        let comp = all
            .iter()
            .find(|c| c.get("purl").and_then(|v| v.as_str()) == Some(*tool_purl))
            .unwrap_or_else(|| panic!("component {tool_purl} present"));
        assert_eq!(
            get_property(comp, "waybill:sbom-tier"),
            Some("design"),
            "tool {tool_purl} should be design tier",
        );
        assert_eq!(
            get_property(comp, "waybill:source-file"),
            Some("pants.toml"),
            "tool {tool_purl} should reference pants.toml",
        );
    }

    // Script component from co-located BUILD file also emitted.
    let script_count = all
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.starts_with("pkg:generic/waybill-fixture-entrypoint.sh@"))
        })
        .count();
    assert_eq!(script_count, 1, "co-located script also emitted");

    // FR-010 log counts.
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    assert!(
        stripped.contains("tool_components_emitted=3"),
        "expected tool_components_emitted=3 in FR-010 log. stderr:\n{stripped}",
    );
    assert!(
        stripped.contains("script_components_emitted=1"),
        "expected script_components_emitted=1 in FR-010 log. stderr:\n{stripped}",
    );
}

// ---------------------------------------------------------------------
// US2 T026 — [shellcheck] present but no `version` key → no tool component
// (regression guard for spec Acceptance Scenario 3)
// ---------------------------------------------------------------------

#[test]
fn us2_no_version_key_emits_no_tool_component() {
    let fixture_dir = fixture("with_shell_setup_no_versions");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let tool_comps: Vec<&serde_json::Value> = cdx
        .get("components")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.starts_with("pkg:generic/shellcheck@"))
        })
        .collect();
    assert!(
        tool_comps.is_empty(),
        "expected zero shellcheck components when no version pinned; got {}",
        tool_comps.len(),
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    assert!(
        stripped.contains("tool_components_emitted=0"),
        "expected tool_components_emitted=0 in FR-010 log. stderr:\n{stripped}",
    );
}

// ---------------------------------------------------------------------
// US3 T029 — shunit2_* targets tag Development; shell_source targets do NOT
// ---------------------------------------------------------------------

#[test]
fn us3_shunit2_targets_tag_development_shell_source_targets_tag_runtime() {
    let fixture_dir = fixture("shunit2_dev_scope");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let shell = pants_shell_components(&cdx);
    assert_eq!(shell.len(), 3, "expected 3 script components");

    let by_name: std::collections::HashMap<&str, &serde_json::Value> = shell
        .iter()
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()).map(|n| (n, *c)))
        .collect();

    // shunit2_test-owned → development
    let deploy_test = by_name
        .get("waybill-fixture-deploy-test.sh")
        .expect("deploy-test component present");
    assert_eq!(
        get_property(deploy_test, "waybill:lifecycle-scope"),
        Some("development"),
        "shunit2_test-owned component should tag as development",
    );

    // shunit2_tests-owned (glob match) → development
    let x_test = by_name
        .get("waybill-fixture-x_test.sh")
        .expect("x_test component present");
    assert_eq!(
        get_property(x_test, "waybill:lifecycle-scope"),
        Some("development"),
        "shunit2_tests-owned component should tag as development",
    );

    // shell_source-owned → NOT development (either absent or runtime)
    let setup = by_name
        .get("waybill-fixture-setup.sh")
        .expect("setup component present");
    let scope = get_property(setup, "waybill:lifecycle-scope");
    assert!(
        scope.is_none() || scope == Some("runtime"),
        "shell_source-owned component should NOT tag as development. got: {scope:?}",
    );
}

// ---------------------------------------------------------------------
// Edge T032 — missing source file emits WARN + continues
// ---------------------------------------------------------------------

#[test]
fn edge_missing_source_file_warns_and_continues() {
    let fixture_dir = fixture("missing_source_file");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let shell = pants_shell_components(&cdx);
    assert_eq!(shell.len(), 1, "expected 1 valid component + missing skipped");
    assert_eq!(
        shell[0].get("name").and_then(|v| v.as_str()),
        Some("waybill-fixture-real.sh"),
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    assert!(
        stripped.contains("waybill-fixture-nonexistent.sh"),
        "WARN should name the missing file. stderr:\n{stripped}",
    );
}

// ---------------------------------------------------------------------
// Edge T034 — malformed BUILD file: valid targets still emit (SC-005)
// ---------------------------------------------------------------------

#[test]
fn edge_malformed_build_partial_emits_valid_targets() {
    let fixture_dir = fixture("malformed_build_partial");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "SC-005: waybill must not abort on malformed BUILD. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let shell = pants_shell_components(&cdx);
    // The 2 valid `shell_source` targets before the broken one emit;
    // the broken target's paren-swallow may eat subsequent content
    // through EOF, so at minimum we get the first 2.
    assert!(
        shell.len() >= 2,
        "expected at least 2 valid components; got {}",
        shell.len(),
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    // WARN naming the parse-error target/file.
    assert!(
        stripped.contains("pants-shell reader")
            && stripped.contains("target parse error"),
        "expected WARN naming the broken target. stderr:\n{stripped}",
    );
}

// ---------------------------------------------------------------------
// Edge T036 — SC-006 dupe target owners merge into single component
// ---------------------------------------------------------------------

#[test]
fn edge_dupe_target_owners_emit_one_component_with_merged_annotation() {
    let fixture_dir = fixture("dupe_target_owners");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let shell = pants_shell_components(&cdx);
    assert_eq!(
        shell.len(),
        1,
        "SC-006: expected exactly 1 component for the shared file; got {}",
        shell.len(),
    );
    assert_eq!(
        get_property(shell[0], "waybill:pants-target"),
        Some("scripts:glob,scripts:single"),
        "SC-006: expected lex-sorted comma-sep merged annotation",
    );
}

// ---------------------------------------------------------------------
// Edge T037 — no BUILD files AND no pants.toml → no reader activity
// ---------------------------------------------------------------------

#[test]
fn edge_no_pants_no_build_files_produces_no_reader_activity() {
    // Reuse pants_pex/minimal_python: has 3rdparty/python but no BUILD
    // files anywhere and no pants.toml at scan root.
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pants_pex/minimal_python");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let shell = pants_shell_components(&cdx);
    assert!(
        shell.is_empty(),
        "expected zero pants-shell components on non-Pants-shell fixture; got {}",
        shell.len(),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    assert!(
        !stripped.contains("pants-shell reader complete"),
        "FR-011 / SC-003: reader must emit no log when no BUILD files + no pants.toml. stderr:\n{stripped}",
    );
}

// ---------------------------------------------------------------------
// Edge T037b (C1) — FR-012 shell_command targets NOT ingested
// ---------------------------------------------------------------------

#[test]
fn edge_shell_command_targets_not_ingested() {
    let fixture_dir = fixture("shell_command_ignored");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let shell = pants_shell_components(&cdx);
    assert_eq!(
        shell.len(),
        1,
        "FR-012: expected exactly 1 component (wrapper) — shell_command must be ignored",
    );
    assert_eq!(
        shell[0].get("name").and_then(|v| v.as_str()),
        Some("waybill-fixture-wrapper.sh"),
    );
    // No component's target annotation should reference the shell_command target.
    for c in &shell {
        let target = get_property(c, "waybill:pants-target").unwrap_or("");
        assert!(
            !target.contains("scripts:build"),
            "FR-012: no component should be attributed to shell_command target scripts:build; got: {target}",
        );
    }
}

#[test]
fn us1_fr010_info_log_emits_all_six_structured_fields() {
    let fixture_dir = fixture("minimal_scripts");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    for field in &[
        "build_files_discovered=",
        "build_files_parsed_ok=",
        "build_files_skipped_corrupt=",
        "shell_targets_found=",
        "script_components_emitted=",
        "tool_components_emitted=",
    ] {
        assert!(
            stripped.contains(field),
            "FR-010: stderr missing structured field {field}. stderr:\n{stripped}",
        );
    }
}
