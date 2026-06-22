//! Milestone 135 edge-case tests — covers spec Edge Cases section +
//! SC-005 (malformed-desc graceful degradation).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn write_pkg(rootfs: &Path, dir_name: &str, desc_body: &str) {
    let pkg_dir = rootfs.join("var/lib/pacman/local").join(dir_name);
    std::fs::create_dir_all(&pkg_dir).unwrap();
    std::fs::write(pkg_dir.join("desc"), desc_body).unwrap();
}

fn run_scan(rootfs: &Path) -> (Value, String, bool) {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(rootfs)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .output()
        .unwrap();
    let success = result.status.success();
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    let bytes = std::fs::read(&out_path).unwrap_or_default();
    let doc: Value = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (doc, stderr, success)
}

fn alpm_purls(doc: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut visit = |c: &Value| {
        if let Some(p) = c.get("purl").and_then(|v| v.as_str()) {
            if p.starts_with("pkg:alpm/") {
                out.push(p.to_string());
            }
        }
    };
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            visit(c);
        }
    }
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        visit(c);
    }
    out
}

#[test]
fn sc_005_malformed_desc_alongside_valid_packages() {
    // SC-005 — three valid packages + one corrupted; scan succeeds
    // (exit 0); 3 components emit; warn names the broken package.
    let tmp = tempfile::tempdir().unwrap();
    write_pkg(
        tmp.path(),
        "good-a-1.0-1",
        "%NAME%\ngood-a\n\n%VERSION%\n1.0-1\n\n%ARCH%\nx86_64\n",
    );
    write_pkg(
        tmp.path(),
        "good-b-2.0-1",
        "%NAME%\ngood-b\n\n%VERSION%\n2.0-1\n\n%ARCH%\nx86_64\n",
    );
    write_pkg(
        tmp.path(),
        "good-c-3.0-1",
        "%NAME%\ngood-c\n\n%VERSION%\n3.0-1\n\n%ARCH%\nx86_64\n",
    );
    // Missing %NAME% — must warn-and-skip without aborting the scan.
    write_pkg(
        tmp.path(),
        "broken-1.0-1",
        "%VERSION%\n1.0-1\n\n%ARCH%\nx86_64\n",
    );

    let (doc, stderr, success) = run_scan(tmp.path());
    assert!(success, "scan must succeed even with one malformed desc");

    let purls = alpm_purls(&doc);
    assert_eq!(purls.len(), 3, "expected 3 valid components, got {purls:?}");

    // Warn names the broken package's path.
    assert!(
        stderr.contains("broken-1.0-1") || stderr.to_lowercase().contains("pacman"),
        "stderr should warn about the broken package; got:\n{stderr}",
    );
}

#[test]
fn noarch_any_emits_arch_any_qualifier() {
    let tmp = tempfile::tempdir().unwrap();
    write_pkg(
        tmp.path(),
        "terminfo-6.4-3",
        "%NAME%\nterminfo\n\n%VERSION%\n6.4-3\n\n%ARCH%\nany\n",
    );
    let (doc, _, _) = run_scan(tmp.path());
    let purls = alpm_purls(&doc);
    assert_eq!(purls.len(), 1);
    assert_eq!(purls[0], "pkg:alpm/arch/terminfo@6.4-3?arch=any");
}

#[test]
fn multi_version_coexistence_emits_separate_components() {
    // Two `bash` packages with DIFFERENT versions in the same `local/`
    // (pathological but parseable). Each MUST emit as a distinct
    // component since the PURLs differ.
    let tmp = tempfile::tempdir().unwrap();
    write_pkg(
        tmp.path(),
        "bash-5.2.026-1",
        "%NAME%\nbash\n\n%VERSION%\n5.2.026-1\n\n%ARCH%\nx86_64\n",
    );
    write_pkg(
        tmp.path(),
        "bash-5.1.16-1",
        "%NAME%\nbash\n\n%VERSION%\n5.1.16-1\n\n%ARCH%\nx86_64\n",
    );
    let (doc, _, _) = run_scan(tmp.path());
    let purls = alpm_purls(&doc);
    assert_eq!(purls.len(), 2);
    assert!(purls.contains(&"pkg:alpm/arch/bash@5.2.026-1?arch=x86_64".to_string()));
    assert!(purls.contains(&"pkg:alpm/arch/bash@5.1.16-1?arch=x86_64".to_string()));
}

#[test]
fn lib32_multilib_package_correct_purl() {
    let tmp = tempfile::tempdir().unwrap();
    write_pkg(
        tmp.path(),
        "lib32-glibc-2.40-1",
        "%NAME%\nlib32-glibc\n\n%VERSION%\n2.40-1\n\n%ARCH%\nx86_64\n",
    );
    let (doc, _, _) = run_scan(tmp.path());
    let purls = alpm_purls(&doc);
    assert_eq!(purls.len(), 1);
    assert_eq!(purls[0], "pkg:alpm/arch/lib32-glibc@2.40-1?arch=x86_64");
}

#[test]
fn empty_pacman_dir_emits_no_components_no_warnings() {
    let tmp = tempfile::tempdir().unwrap();
    // Create the directory but no package subdirs.
    std::fs::create_dir_all(tmp.path().join("var/lib/pacman/local")).unwrap();
    let (doc, stderr, success) = run_scan(tmp.path());
    assert!(success);
    let purls = alpm_purls(&doc);
    assert!(purls.is_empty());
    // No WARN-level pacman noise on an empty DB.
    assert!(
        !stderr.contains("WARN") || !stderr.to_lowercase().contains("pacman"),
        "empty pacman DB must not warn; got:\n{stderr}",
    );
}
