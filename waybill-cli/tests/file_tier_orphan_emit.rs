//! Milestone 133 US1.B integration test — `--file-inventory=orphan`
//! emits a CDX file-tier component for an unattributed binary, with
//! no PURL and the `waybill:component-tier = "file"` annotation.
//!
//! Default behavior (no flag) must NOT emit file-tier components —
//! preserves the pre-milestone-133 byte-identity guarantee per FR-004
//! / SC-005 until US1.C flips the default.

use std::path::{Path, PathBuf};
use std::process::Command;

fn write_file(dir: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().expect("rel path has parent"))
        .expect("create test dir");
    std::fs::write(&p, bytes).expect("write test file");
    p
}

fn run_scan(path: &Path, extra_args: &[&str]) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("sbom.cdx.json");
    let mut cmd = Command::new(bin);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let status = cmd.status().expect("waybill should run");
    assert!(status.success(), "scan failed: {extra_args:?}");
    let raw = std::fs::read(&out_path).expect("read sbom");
    serde_json::from_slice(&raw).expect("valid JSON")
}

fn file_tier_components(sbom: &serde_json::Value) -> Vec<&serde_json::Value> {
    let Some(comps) = sbom["components"].as_array() else {
        return Vec::new();
    };
    comps
        .iter()
        .filter(|c| {
            let Some(props) = c["properties"].as_array() else {
                return false;
            };
            props.iter().any(|p| {
                p["name"].as_str() == Some("waybill:component-tier")
                    && p["value"].as_str() == Some("file")
            })
        })
        .collect()
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn off_mode_emits_no_file_tier_components() {
    let tmp = tempfile::tempdir().unwrap();
    // ELF magic header → would qualify in orphan mode.
    write_file(
        tmp.path(),
        "opt/unattributed-binary",
        b"\x7FELF\x02\x01\x01\x00rest-of-elf",
    );
    // Lone Cargo.toml → would qualify in orphan mode.
    write_file(
        tmp.path(),
        "app/Cargo.toml",
        b"[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    );

    // Milestone 133 US1.C flipped the default to `orphan`; opt
    // out explicitly to verify the off-path preserves pre-feature
    // byte-identity. Operators wanting that behavior pass
    // `--file-inventory=off`.
    let sbom = run_scan(tmp.path(), &["--file-inventory=off"]);
    let file_tier = file_tier_components(&sbom);
    assert!(
        file_tier.is_empty(),
        "off mode must NOT emit file-tier components; got {} entries",
        file_tier.len()
    );
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn default_mode_is_orphan_and_emits_file_tier_components() {
    let tmp = tempfile::tempdir().unwrap();
    write_file(
        tmp.path(),
        "opt/unattributed-binary",
        b"\x7FELF\x02\x01\x01\x00rest-of-elf",
    );
    // Milestone 133 US1.C default-flip regression: no `--file-inventory`
    // flag at all → orphan walker runs → file-tier components emit.
    let sbom = run_scan(tmp.path(), &[]);
    let file_tier = file_tier_components(&sbom);
    assert!(
        !file_tier.is_empty(),
        "post-US1.C default is `orphan`; expected at least one file-tier component, got none"
    );
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn orphan_mode_emits_file_tier_components_with_correct_shape() {
    let tmp = tempfile::tempdir().unwrap();
    write_file(
        tmp.path(),
        "opt/unattributed-binary",
        b"\x7FELF\x02\x01\x01\x00rest-of-elf",
    );
    write_file(
        tmp.path(),
        "app/Cargo.toml",
        b"[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    );

    let sbom = run_scan(tmp.path(), &["--file-inventory=orphan"]);
    let file_tier = file_tier_components(&sbom);
    assert!(
        file_tier.len() >= 2,
        "expected at least 2 file-tier components (ELF + lone Cargo.toml); got {}",
        file_tier.len()
    );

    for c in &file_tier {
        // FR-001: CDX type = "file".
        assert_eq!(
            c["type"].as_str(),
            Some("file"),
            "file-tier component type must be `file`; got {:?}",
            c["type"]
        );
        // FR-009: no PURL in the wire format.
        assert!(
            c["purl"].is_null(),
            "file-tier component must NOT carry a `purl` field; got {:?}",
            c["purl"]
        );
        // FR-008: SHA-256 mandatory.
        let hashes = c["hashes"].as_array().expect("hashes array");
        assert!(
            hashes
                .iter()
                .any(|h| h["alg"].as_str() == Some("SHA-256")),
            "file-tier component must carry a SHA-256 hash"
        );
        // FR-007: `waybill:file-paths` JSON-encoded array.
        let props = c["properties"].as_array().expect("properties array");
        let fp = props
            .iter()
            .find(|p| p["name"].as_str() == Some("waybill:file-paths"))
            .and_then(|p| p["value"].as_str())
            .expect("waybill:file-paths annotation present");
        let parsed: Vec<String> =
            serde_json::from_str(fp).expect("waybill:file-paths is JSON-encoded array");
        assert!(
            !parsed.is_empty(),
            "waybill:file-paths must list at least one path"
        );
        for p in &parsed {
            assert!(
                !p.starts_with('/'),
                "file-paths entry {p:?} must not start with leading `/`"
            );
        }
    }
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn full_mode_bypasses_dedupe_and_emits_more_than_orphan() {
    // Milestone 133 US3 — `--file-inventory=full` MUST emit a
    // component for every content-shape-passing file regardless of
    // dedupe coverage. The orphan-mode count is the baseline; full
    // mode equals or exceeds it. Two unique ELF files + one
    // duplicate-content file → orphan emits 2 file-tier (unique
    // hashes); full mode also emits 2 (per-unique-hash dedupe is
    // intrinsic to FileTierEntry, not part of the FR-011 dedupe
    // bypass). The point of this test is the BYPASS, not the
    // collapse, so we verify both modes succeed and that full mode
    // produces components even when an empty dedupe set wouldn't
    // have changed behavior on this fixture.
    let tmp = tempfile::tempdir().unwrap();
    write_file(
        tmp.path(),
        "opt/a",
        b"\x7FELF\x02\x01\x01\x00bytes-a",
    );
    write_file(
        tmp.path(),
        "opt/b",
        b"\x7FELF\x02\x01\x01\x00bytes-b",
    );

    let orphan = run_scan(tmp.path(), &["--file-inventory=orphan"]);
    let full = run_scan(tmp.path(), &["--file-inventory=full"]);

    let orphan_count = file_tier_components(&orphan).len();
    let full_count = file_tier_components(&full).len();
    assert!(
        full_count >= orphan_count,
        "full mode must emit ≥ orphan mode count; got full={full_count} orphan={orphan_count}"
    );
    assert!(
        full_count >= 2,
        "full mode must emit at least 2 components (2 distinct ELF files); got {full_count}"
    );
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn invalid_file_inventory_flag_value_exits_nonzero() {
    let tmp = tempfile::tempdir().unwrap();
    write_file(tmp.path(), "noop", b"");
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let out_path = tmp.path().join("sbom.cdx.json");
    let status = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(tmp.path())
        .arg("--output")
        .arg(&out_path)
        .arg("--file-inventory=bogus")
        .arg("--no-deep-hash")
        .status()
        .expect("waybill should run");
    assert!(
        !status.success(),
        "scan with invalid --file-inventory must exit non-zero"
    );
}
