//! Milestone 135 US2 — Arch derivative distros (SteamOS, Manjaro,
//! EndeavourOS, CachyOS, unknown derivatives) get the correct
//! `pkg:alpm/<distro-id>/...` PURL namespace + `distro=<id>-<verid>`
//! qualifier convention. Rolling Arch (no VERSION_ID) correctly
//! omits the qualifier; missing `/etc/os-release` defaults to `arch`.
//!
//! Covers spec acceptance scenarios US2.1–US2.4 + SC-002 + FR-004 +
//! FR-005 + FR-010.

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

fn write_os_release(rootfs: &Path, body: &str) {
    let etc = rootfs.join("etc");
    std::fs::create_dir_all(&etc).unwrap();
    std::fs::write(etc.join("os-release"), body).unwrap();
}

fn run_scan(rootfs: &Path) -> Value {
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
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn first_alpm_purl(doc: &Value) -> Option<String> {
    let mut purls: Vec<String> = Vec::new();
    let mut visit = |c: &Value| {
        if let Some(p) = c.get("purl").and_then(|v| v.as_str()) {
            if p.starts_with("pkg:alpm/") {
                purls.push(p.to_string());
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
    purls.into_iter().next()
}

fn build_fixture(rootfs: &Path, id_value: &str, version_id: Option<&str>) {
    let mut os_release = format!("ID={id_value}\n");
    if let Some(v) = version_id {
        os_release.push_str(&format!("VERSION_ID={v}\n"));
    }
    write_os_release(rootfs, &os_release);
    write_pkg(
        rootfs,
        "bash-5.2.026-1",
        "%NAME%\nbash\n\n%VERSION%\n5.2.026-1\n\n%ARCH%\nx86_64\n",
    );
}

#[test]
fn steamos_namespace_includes_distro_qualifier() {
    let tmp = tempfile::tempdir().unwrap();
    build_fixture(tmp.path(), "steamos", Some("3.5.7"));
    let doc = run_scan(tmp.path());
    let purl = first_alpm_purl(&doc).expect("alpm component must emit");
    assert_eq!(
        purl,
        "pkg:alpm/steamos/bash@5.2.026-1?arch=x86_64&distro=steamos-3.5.7"
    );
}

#[test]
fn manjaro_namespace_includes_distro_qualifier() {
    let tmp = tempfile::tempdir().unwrap();
    build_fixture(tmp.path(), "manjaro", Some("24.0.0"));
    let doc = run_scan(tmp.path());
    let purl = first_alpm_purl(&doc).expect("alpm component must emit");
    assert_eq!(
        purl,
        "pkg:alpm/manjaro/bash@5.2.026-1?arch=x86_64&distro=manjaro-24.0.0"
    );
}

#[test]
fn endeavouros_rolling_no_qualifier() {
    // EndeavourOS is rolling-release; their os-release typically omits
    // VERSION_ID.
    let tmp = tempfile::tempdir().unwrap();
    build_fixture(tmp.path(), "endeavouros", None);
    let doc = run_scan(tmp.path());
    let purl = first_alpm_purl(&doc).expect("alpm component must emit");
    assert_eq!(purl, "pkg:alpm/endeavouros/bash@5.2.026-1?arch=x86_64");
}

#[test]
fn cachyos_with_qualifier() {
    let tmp = tempfile::tempdir().unwrap();
    build_fixture(tmp.path(), "cachyos", Some("2024.10.10"));
    let doc = run_scan(tmp.path());
    let purl = first_alpm_purl(&doc).expect("alpm component must emit");
    assert_eq!(
        purl,
        "pkg:alpm/cachyos/bash@5.2.026-1?arch=x86_64&distro=cachyos-2024.10.10"
    );
}

#[test]
fn unknown_derivative_passes_through_verbatim() {
    // FR-010 — verbatim pass-through for unknown derivative IDs.
    let tmp = tempfile::tempdir().unwrap();
    build_fixture(tmp.path(), "mydistro", None);
    let doc = run_scan(tmp.path());
    let purl = first_alpm_purl(&doc).expect("alpm component must emit");
    assert_eq!(purl, "pkg:alpm/mydistro/bash@5.2.026-1?arch=x86_64");
}

#[test]
fn rolling_arch_omits_distro_qualifier() {
    let tmp = tempfile::tempdir().unwrap();
    build_fixture(tmp.path(), "arch", None);
    let doc = run_scan(tmp.path());
    let purl = first_alpm_purl(&doc).expect("alpm component must emit");
    assert_eq!(purl, "pkg:alpm/arch/bash@5.2.026-1?arch=x86_64");
    // Critically: the `distro=` qualifier MUST NOT appear at all.
    assert!(
        !purl.contains("distro="),
        "rolling Arch must omit distro qualifier; got {purl}",
    );
}

#[test]
fn no_os_release_defaults_to_arch_namespace() {
    // FR-004 — when /etc/os-release is absent, namespace defaults to `arch`.
    let tmp = tempfile::tempdir().unwrap();
    // No /etc/os-release at all.
    write_pkg(
        tmp.path(),
        "bash-5.2.026-1",
        "%NAME%\nbash\n\n%VERSION%\n5.2.026-1\n\n%ARCH%\nx86_64\n",
    );
    let doc = run_scan(tmp.path());
    let purl = first_alpm_purl(&doc).expect("alpm component must emit");
    assert_eq!(purl, "pkg:alpm/arch/bash@5.2.026-1?arch=x86_64");
}
