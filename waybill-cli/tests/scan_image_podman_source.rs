//! Milestone 206 (#440) — podman source integration tests.
//!
//! All tests here are Linux-only (podman storage layout is Linux-specific
//! per spec Assumption 1) AND require the `podman` binary on `$PATH`
//! at least one image pre-cached. Gated behind
//! `WAYBILL_PODMAN_INTEGRATION=1` per m188/m203/m205 precedent — CI's
//! Linux lane opts in; local dev workstations without podman skip
//! cleanly.

#![cfg(target_os = "linux")]

use std::process::Command;

fn mikebom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

/// Skip cleanly if `WAYBILL_PODMAN_INTEGRATION=1` is not set OR podman
/// is not on PATH. Prints a skip message so test runners record it.
fn require_podman_integration() -> bool {
    if std::env::var("WAYBILL_PODMAN_INTEGRATION").as_deref() != Ok("1") {
        eprintln!("skipping: WAYBILL_PODMAN_INTEGRATION != 1");
        return false;
    }
    match Command::new("podman").arg("--version").output() {
        Ok(o) if o.status.success() => true,
        _ => {
            eprintln!("skipping: podman binary not found on $PATH");
            false
        }
    }
}

fn ensure_alpine_cached() {
    let out = Command::new("podman")
        .args(["pull", "alpine:3.19"])
        .output()
        .expect("spawn podman pull");
    assert!(
        out.status.success(),
        "podman pull alpine:3.19 failed: stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
}

fn scan_image_via_podman(image_ref: &str, extra_args: &[&str]) -> (serde_json::Value, String, bool) {
    let tempdir = tempfile::tempdir().unwrap();
    let out = tempdir.path().join("out.cdx.json");
    let mut cmd = Command::new(mikebom_bin());
    cmd.args([
        "sbom",
        "scan",
        "--offline",
        "--image",
        image_ref,
        "--format",
        "cyclonedx-json",
        "--output",
        out.to_str().unwrap(),
        "--no-deep-hash",
    ]);
    for a in extra_args {
        cmd.arg(a);
    }
    let cmd_out = cmd.output().expect("spawn waybill");
    let stderr = String::from_utf8_lossy(&cmd_out.stderr).to_string();
    let success = cmd_out.status.success();
    let json = if success {
        serde_json::from_slice(&std::fs::read(&out).unwrap())
            .expect("output is valid JSON")
    } else {
        serde_json::Value::Null
    };
    (json, stderr, success)
}

fn find_image_source_property(cdx: &serde_json::Value) -> Option<String> {
    cdx.pointer("/metadata/properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("waybill:image-source"))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()).map(String::from))
}

fn count_apk_components(cdx: &serde_json::Value) -> usize {
    cdx.get("components")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    c.get("purl")
                        .and_then(|p| p.as_str())
                        .map(|s| s.starts_with("pkg:apk/"))
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}

// ── T019 US1 — rootless podman scan (P1 MVP) ──

#[test]
fn us1_podman_source_scans_rootless_alpine() {
    if !require_podman_integration() {
        return;
    }
    ensure_alpine_cached();

    let (cdx, stderr, ok) = scan_image_via_podman("alpine:3.19", &["--image-src", "podman"]);
    assert!(ok, "podman-source scan should succeed. stderr:\n{stderr}");

    // (b) apk components detected.
    let apk_count = count_apk_components(&cdx);
    assert!(
        apk_count >= 10,
        "expected ≥10 pkg:apk/ components; got {apk_count}. cdx: {cdx:#}"
    );

    // (c) waybill:image-source = "podman" annotation present.
    assert_eq!(
        find_image_source_property(&cdx).as_deref(),
        Some("podman"),
        "CDX MUST carry waybill:image-source = podman. cdx: {cdx:#}"
    );

    // (d) F2 — SC-004: metadata.component.name reflects the operator's ref.
    let component_name = cdx
        .pointer("/metadata/component/name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        component_name.contains("alpine"),
        "SC-004: metadata.component.name should contain 'alpine'; got {component_name:?}"
    );
}

// ── T020 US2 — rootful podman scan ──

#[test]
fn us2_podman_source_scans_rootful_image() {
    if !require_podman_integration() {
        return;
    }
    if std::env::var("WAYBILL_PODMAN_ROOTFUL_INTEGRATION").as_deref() != Ok("1") {
        eprintln!("skipping: WAYBILL_PODMAN_ROOTFUL_INTEGRATION != 1 (test needs root)");
        return;
    }
    // Skip if not root.
    let euid_output = Command::new("id").arg("-u").output().unwrap();
    let euid = String::from_utf8_lossy(&euid_output.stdout).trim().to_string();
    if euid != "0" {
        eprintln!("skipping: rootful test needs euid == 0");
        return;
    }
    ensure_alpine_cached();

    let (cdx, stderr, ok) = scan_image_via_podman("alpine:3.19", &["--image-src", "podman"]);
    assert!(ok, "rootful podman scan should succeed. stderr:\n{stderr}");
    assert_eq!(
        find_image_source_property(&cdx).as_deref(),
        Some("podman"),
        "CDX MUST carry waybill:image-source = podman"
    );
    let apk_count = count_apk_components(&cdx);
    assert!(apk_count >= 10, "expected ≥10 apk components; got {apk_count}");
}

// ── T022 US3 — auto-detection default order ──

#[test]
fn us3_default_order_falls_back_from_docker_to_podman() {
    if !require_podman_integration() {
        return;
    }
    // Docker may or may not be present; if present, ensure alpine is NOT
    // in the docker cache so we exercise the fallback.
    let docker_available = Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if docker_available {
        let _ = Command::new("docker")
            .args(["rmi", "alpine:3.19"])
            .output();
    }
    ensure_alpine_cached();

    let (cdx, stderr, ok) = scan_image_via_podman("alpine:3.19", &[]);
    assert!(ok, "default-order scan should succeed. stderr:\n{stderr}");
    assert_eq!(
        find_image_source_property(&cdx).as_deref(),
        Some("podman"),
        "default order MUST fall back to podman when docker has no image. cdx: {cdx:#}"
    );
}

// ── T022a US3 — F3 remediation: explicit --image-src podman first wins ──

#[test]
fn us3b_explicit_image_src_podman_first_wins_over_docker() {
    if !require_podman_integration() {
        return;
    }
    let docker_available = Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !docker_available {
        eprintln!("skipping: docker binary needed to demonstrate podman-first-wins");
        return;
    }
    // Ensure alpine is in BOTH docker + podman caches.
    ensure_alpine_cached();
    let _ = Command::new("docker")
        .args(["pull", "alpine:3.19"])
        .output();

    let (cdx, stderr, ok) =
        scan_image_via_podman("alpine:3.19", &["--image-src", "podman,docker"]);
    assert!(ok, "explicit-order scan should succeed. stderr:\n{stderr}");
    // FR-005: operator ordering respected verbatim → podman wins.
    assert_eq!(
        find_image_source_property(&cdx).as_deref(),
        Some("podman"),
        "FR-005: --image-src podman,docker MUST use podman (operator preference). cdx: {cdx:#}"
    );
}

