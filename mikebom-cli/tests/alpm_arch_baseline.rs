//! Milestone 135 US1 — end-to-end integration test that a synthetic
//! Arch rootfs produces a CDX SBOM containing one component per
//! pacman-installed package with the canonical `pkg:alpm/arch/...`
//! PURL identity and accurate dep edges.
//!
//! Covers spec acceptance scenarios US1.1, US1.2, and US1.3 plus
//! SC-001 (Arch baseline) and SC-006 (standard PURL filter).
//! FR-008 (no-op on missing pacman DB) covered by the
//! `no_pacman_db_emits_zero_components` test.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
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

fn write_pkg(rootfs: &Path, dir_name: &str, desc_body: &str) {
    let pkg_dir = rootfs.join("var/lib/pacman/local").join(dir_name);
    std::fs::create_dir_all(&pkg_dir).unwrap();
    std::fs::write(pkg_dir.join("desc"), desc_body).unwrap();
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
fn three_pacman_packages_emit_as_alpm_components() {
    let tmp = tempfile::tempdir().unwrap();
    // Construct three packages with realistic pacman version strings.
    write_pkg(
        tmp.path(),
        "bash-5.2.026-1",
        "%NAME%\nbash\n\n%VERSION%\n5.2.026-1\n\n%ARCH%\nx86_64\n\n%DEPENDS%\nglibc\n",
    );
    write_pkg(
        tmp.path(),
        "glibc-2.40-1",
        "%NAME%\nglibc\n\n%VERSION%\n2.40-1\n\n%ARCH%\nx86_64\n",
    );
    write_pkg(
        tmp.path(),
        "curl-8.5.0-1",
        "%NAME%\ncurl\n\n%VERSION%\n8.5.0-1\n\n%ARCH%\nx86_64\n\n%DEPENDS%\nglibc\nbrotli\n",
    );

    let doc = run_scan(tmp.path());
    let purls = alpm_purls(&doc);

    // (a) Exactly 3 pkg:alpm/arch/* components.
    assert_eq!(purls.len(), 3, "expected 3 alpm components, got {purls:?}");

    // (b) Each has the expected PURL — no VERSION_ID so no distro= qualifier.
    assert!(purls.contains(&"pkg:alpm/arch/bash@5.2.026-1?arch=x86_64".to_string()));
    assert!(purls.contains(&"pkg:alpm/arch/glibc@2.40-1?arch=x86_64".to_string()));
    assert!(purls.contains(&"pkg:alpm/arch/curl@8.5.0-1?arch=x86_64".to_string()));

    // (c) curl's depends-on edge targets glibc's bom-ref.
    let components = doc.get("components").and_then(|v| v.as_array()).unwrap();
    let curl_ref: Option<&str> = components
        .iter()
        .find(|c| c.get("purl").and_then(|v| v.as_str())
            == Some("pkg:alpm/arch/curl@8.5.0-1?arch=x86_64"))
        .and_then(|c| c.get("bom-ref").and_then(|v| v.as_str()));
    let glibc_ref: Option<&str> = components
        .iter()
        .find(|c| c.get("purl").and_then(|v| v.as_str())
            == Some("pkg:alpm/arch/glibc@2.40-1?arch=x86_64"))
        .and_then(|c| c.get("bom-ref").and_then(|v| v.as_str()));
    let curl_ref = curl_ref.expect("curl component must have bom-ref");
    let glibc_ref = glibc_ref.expect("glibc component must have bom-ref");
    let deps = doc.get("dependencies").and_then(|v| v.as_array()).unwrap();
    let curl_deps = deps
        .iter()
        .find(|d| d.get("ref").and_then(|v| v.as_str()) == Some(curl_ref))
        .and_then(|d| d.get("dependsOn").and_then(|v| v.as_array()))
        .expect("curl must have a dependencies entry");
    let curl_dep_refs: Vec<&str> = curl_deps
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        curl_dep_refs.contains(&glibc_ref),
        "curl dependsOn must target glibc; got {curl_dep_refs:?}",
    );
}

#[test]
fn no_pacman_db_emits_zero_alpm_components_no_warn() {
    // FR-008 — rootfs with no /var/lib/pacman/ produces zero alpm
    // components and no pacman/alpm warning lines.
    let tmp = tempfile::tempdir().unwrap();
    // Create only an unrelated file to give the walker something to do.
    std::fs::create_dir_all(tmp.path().join("etc")).unwrap();
    std::fs::write(tmp.path().join("etc/hostname"), "test").unwrap();

    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(tmp.path())
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .output()
        .unwrap();
    assert!(result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);

    let bytes = std::fs::read(&out_path).unwrap();
    let doc: Value = serde_json::from_slice(&bytes).unwrap();
    let alpm_count = alpm_purls(&doc).len();
    assert_eq!(alpm_count, 0, "no alpm components expected; got {alpm_count}");

    // FR-008 — no warning fires (we only check for explicit WARN-level
    // mentions of pacman/alpm; debug-level "expected if no pacman" is
    // fine because the user normally never sees it).
    assert!(
        !stderr.contains("WARN") || !stderr.to_lowercase().contains("pacman"),
        "no WARN about pacman expected on non-Arch scan; stderr:\n{stderr}",
    );
}

#[test]
fn sc_006_standard_purl_filter_enumerates_alpm_components() {
    // SC-006 — an external consumer using only the standard PURL
    // filter (no alpm-specific code) enumerates every alpm component.
    let tmp = tempfile::tempdir().unwrap();
    write_pkg(
        tmp.path(),
        "bash-5.2.026-1",
        "%NAME%\nbash\n\n%VERSION%\n5.2.026-1\n\n%ARCH%\nx86_64\n",
    );
    write_pkg(
        tmp.path(),
        "glibc-2.40-1",
        "%NAME%\nglibc\n\n%VERSION%\n2.40-1\n\n%ARCH%\nx86_64\n",
    );

    let doc = run_scan(tmp.path());
    // Standard filter: walk components[], select purl.startswith("pkg:alpm/")
    let alpm_purls = alpm_purls(&doc);
    assert_eq!(alpm_purls.len(), 2);
    // The filter is meaningful — no other PURLs in the synthetic
    // fixture should be alpm-namespaced. (The fixture has no .deb / .apk
    // / .rpm / lockfile content to seed unrelated alpm-prefixed purls.)
}
