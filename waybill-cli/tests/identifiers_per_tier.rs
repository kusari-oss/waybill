//! Milestone 073 T018 — per-tier identifier emission test.
//!
//! Confirms the same identifier mechanism applies on:
//!
//! - `waybill sbom scan --image <tarball>` — image-tier auto-detects
//!   an `image:` identifier per the canonical Q3 shape, plus accepts
//!   manual `--repo` / `--id` flags.
//! - Cross-tier consistency — manual identifier flags ride the
//!   same per-format carriers regardless of tier (path / image).
//!
//! The `waybill trace` (build-tier) path is exercised via the unit-test
//! coverage in `cli/run.rs` and `cli/generate.rs` — running an actual
//! trace requires Linux + eBPF kernel privileges and is gated behind
//! the `ebpf-tracing` feature flag, so a hermetic integration test
//! against a real eBPF capture isn't feasible here. The flag-parsing +
//! propagation path is covered by the parse-time clap behavior.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

/// Build a minimal docker-save tarball with a single layer containing
/// one trivial file (so the scan succeeds with a non-empty rootfs).
/// Returns the on-disk path to the tarball.
fn build_synthetic_image_tarball() -> (PathBuf, tempfile::TempDir) {
    // Inner layer tar containing /etc/os-release (so distro detection
    // doesn't error out) plus a trivial dpkg stanza so the scan
    // produces at least one component.
    let mut layer_bytes = Vec::new();
    {
        let mut layer_tar = tar::Builder::new(&mut layer_bytes);
        let os_release =
            b"NAME=\"Debian\"\nID=debian\nVERSION_ID=\"12\"\nVERSION_CODENAME=bookworm\n";
        let mut h = tar::Header::new_ustar();
        h.set_path("etc/os-release").unwrap();
        h.set_size(os_release.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        layer_tar.append(&h, os_release.as_slice()).unwrap();

        let dpkg_status =
            b"Package: foo\nStatus: install ok installed\nVersion: 1.0\nArchitecture: amd64\nMaintainer: Debian <debian@example.org>\n\n";
        let mut h = tar::Header::new_ustar();
        h.set_path("var/lib/dpkg/status").unwrap();
        h.set_size(dpkg_status.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        layer_tar.append(&h, dpkg_status.as_slice()).unwrap();

        layer_tar.finish().unwrap();
    }

    let manifest = r#"[{"Config":"config.json","RepoTags":["docker.io/test/foo:v1"],"Layers":["layer0/layer.tar"]}]"#;
    let td = tempfile::tempdir().unwrap();
    let tarball_path = td.path().join("img.tar");
    let file = std::fs::File::create(&tarball_path).unwrap();
    {
        let mut outer = tar::Builder::new(file);

        let mut mh = tar::Header::new_ustar();
        mh.set_path("manifest.json").unwrap();
        mh.set_size(manifest.len() as u64);
        mh.set_mode(0o644);
        mh.set_cksum();
        outer.append(&mh, manifest.as_bytes()).unwrap();

        let mut lh = tar::Header::new_ustar();
        lh.set_path("layer0/layer.tar").unwrap();
        lh.set_size(layer_bytes.len() as u64);
        lh.set_mode(0o644);
        lh.set_cksum();
        outer.append(&lh, layer_bytes.as_slice()).unwrap();

        outer.into_inner().unwrap().flush().unwrap();
    }
    (tarball_path, td)
}

fn scan_image_cdx(
    tarball: &Path,
    fake_home: &Path,
    extra_args: &[&str],
) -> serde_json::Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--image")
        .arg(tarball)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("scan runs");
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bytes = std::fs::read(&out_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    // Keep tarball+tempdir alive for the duration of parsing
    drop(out_dir);
    parsed
}

#[test]
fn image_scan_auto_detects_image_identifier_in_canonical_shape() {
    let (tarball, _td) = build_synthetic_image_tarball();
    let fake_home = tempfile::tempdir().unwrap();

    let cdx = scan_image_cdx(&tarball, fake_home.path(), &[]);
    let refs = cdx["metadata"]["component"]["externalReferences"]
        .as_array()
        .expect("externalReferences array");
    let dist_entry = refs
        .iter()
        .find(|r| r.get("type").and_then(|v| v.as_str()) == Some("distribution"))
        .expect("auto-detected image identifier rides type:distribution");
    let url = dist_entry["url"].as_str().expect("url present");
    // Per Q3 canonical shape, the URL contains the registry/name:tag
    // portion AND the @sha256:<digest> suffix.
    assert!(
        url.starts_with("docker.io/test/foo:v1"),
        "expected canonical image: shape with registry/name:tag prefix; got {url}"
    );
    assert!(
        url.contains("@sha256:"),
        "expected @sha256:<digest> portion in canonical shape; got {url}"
    );
    assert_eq!(
        dist_entry["comment"].as_str(),
        Some("auto-detected from resolved image reference")
    );
}

#[test]
fn image_scan_accepts_manual_identifier_flags_alongside_auto_detection() {
    let (tarball, _td) = build_synthetic_image_tarball();
    let fake_home = tempfile::tempdir().unwrap();

    let cdx = scan_image_cdx(
        &tarball,
        fake_home.path(),
        &[
            "--repo",
            "git@github.com:test/foo.git",
            "--id",
            "acme_corp_id=svc-alpha",
        ],
    );
    let refs = cdx["metadata"]["component"]["externalReferences"]
        .as_array()
        .expect("externalReferences array");
    let has_image = refs
        .iter()
        .any(|r| r.get("type").and_then(|v| v.as_str()) == Some("distribution"));
    let has_repo = refs
        .iter()
        .any(|r| r.get("type").and_then(|v| v.as_str()) == Some("vcs"));
    assert!(
        has_image,
        "image-tier scan must auto-detect an image: identifier (type:distribution)"
    );
    assert!(
        has_repo,
        "image-tier scan must accept manual --repo (type:vcs)"
    );

    // The user-defined identifier rides the metadata.properties[]
    // entry (NOT the externalReferences[] array).
    let props = cdx["metadata"]
        .get("properties")
        .and_then(|v| v.as_array())
        .expect("metadata.properties present");
    let entry = props
        .iter()
        .find(|p| {
            p.get("name").and_then(|v| v.as_str()) == Some("waybill:identifiers")
        })
        .expect("user-defined annotation present");
    let raw = entry["value"].as_str().unwrap();
    let payload: Vec<serde_json::Value> = serde_json::from_str(raw).unwrap();
    let acme = payload
        .iter()
        .find(|e| e["scheme"].as_str() == Some("acme_corp_id"))
        .expect("acme entry present");
    assert_eq!(acme["value"].as_str(), Some("svc-alpha"));
}

#[test]
fn image_and_path_tiers_use_same_per_format_carriers() {
    let (tarball, _td_image) = build_synthetic_image_tarball();
    let fake_home = tempfile::tempdir().unwrap();

    // Image-tier with manual repo: identifier.
    let img_cdx = scan_image_cdx(
        &tarball,
        fake_home.path(),
        &["--repo", "git@github.com:test/x.git"],
    );
    let img_vcs_count = img_cdx["metadata"]["component"]["externalReferences"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|r| r.get("type").and_then(|v| v.as_str()) == Some("vcs"))
                .count()
        })
        .unwrap_or(0);
    assert_eq!(
        img_vcs_count, 1,
        "image-tier should emit exactly one vcs externalRef from --repo"
    );

    // Path-tier with the SAME flag — confirm both tiers ride the same
    // standards-native carrier (no tier-specific divergence per FR-008).
    let td = tempfile::tempdir().unwrap();
    std::fs::write(
        td.path().join("Cargo.toml"),
        b"[package]\nname = \"x\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(
        td.path().join("Cargo.lock"),
        b"version = 3\n\n[[package]]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let out_path = td.path().join("path.cdx.json");
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    let out = cmd
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(td.path())
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash")
        .arg("--repo")
        .arg("git@github.com:test/x.git")
        .output()
        .expect("scan runs");
    assert!(
        out.status.success(),
        "path scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bytes = std::fs::read(&out_path).unwrap();
    let path_cdx: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let path_vcs_count = path_cdx["metadata"]["component"]["externalReferences"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|r| r.get("type").and_then(|v| v.as_str()) == Some("vcs"))
                .count()
        })
        .unwrap_or(0);
    assert_eq!(
        path_vcs_count, 1,
        "path-tier should emit exactly one vcs externalRef from --repo"
    );
}
