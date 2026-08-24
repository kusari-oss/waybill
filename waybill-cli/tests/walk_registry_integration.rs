//! T035 — coexistence-property integration test (US1 acceptance scenario 4).
//!
//! Verifies that a migrated reader (`ipk_file`, using the T033-consolidated
//! shared walker) and a non-migrated reader (`pip`, still using its legacy
//! `safe_walk` code path) BOTH function correctly in the same scan.
//! This is the operator-visible manifestation of the FR-004 coexistence
//! contract.
//!
//! Uses shell-out to the release-mode binary rather than importing
//! `waybill::scan_fs::*` — `scan_fs` is intentionally not exposed via
//! `waybill-cli/src/lib.rs` per Constitution Principle VI (see
//! `lib.rs:34-38`).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::fs;
use std::process::Command;

#[test]
fn us1_coexistence_property() {
    let tmpdir = tempfile::tempdir().unwrap();
    let root = tmpdir.path();

    // Migrated reader: ipk_file. An empty `.ipk` file with a conforming
    // `<name>_<version>_<arch>.ipk` filename triggers the US2 filename
    // fallback path — parse_ipk_file fails on the empty body, but the
    // fallback emits a component from the filename alone.
    fs::write(root.join("waybill-fixture-ipk_1.0_all.ipk"), b"").unwrap();

    // Non-migrated reader: cargo. A minimal `[package]` Cargo.toml
    // reliably emits a cargo main-module (m064) via cargo's own
    // safe_walk-based reader — NOT through the shared walker.
    fs::write(
        root.join("Cargo.toml"),
        b"[package]\nname = \"waybill-fixture-cargo\"\nversion = \"2.0.0\"\nedition = \"2021\"\n",
    )
    .unwrap();

    let bin = env!("CARGO_BIN_EXE_waybill");
    let out_json = tmpdir.path().join("out.cdx.json");

    let status = Command::new(bin)
        .args([
            "sbom",
            "scan",
            "--offline",
            "--file-inventory=off",
            "--path",
        ])
        .arg(root)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(&out_json)
        .status()
        .unwrap();
    assert!(status.success(), "waybill scan failed on coexistence fixture");

    let sbom_text = fs::read_to_string(&out_json).unwrap();
    let sbom: serde_json::Value = serde_json::from_str(&sbom_text).unwrap();

    // Collect every PURL from both `components[]` AND
    // `metadata.component` — the m127 root-selector may elect one
    // reader's main-module as the SBOM root, in which case it appears
    // in `metadata.component` rather than `components[]`.
    let mut purls: Vec<String> = sbom
        .get("components")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    if let Some(root_purl) = sbom
        .get("metadata")
        .and_then(|m| m.get("component"))
        .and_then(|c| c.get("purl"))
        .and_then(|v| v.as_str())
    {
        purls.push(root_purl.to_string());
    }

    // Migrated reader (ipk_file via shared walker) emitted an opkg PURL.
    assert!(
        purls
            .iter()
            .any(|p| p.starts_with("pkg:opkg/waybill-fixture-ipk")),
        "migrated ipk_file reader should have emitted a pkg:opkg/... component; got PURLs: {:?}",
        purls,
    );

    // Non-migrated reader (cargo via legacy safe_walk) emitted a cargo
    // main-module PURL. May appear as SBOM root (metadata.component)
    // when it's the only main-module; the check above accepts either
    // location.
    assert!(
        purls
            .iter()
            .any(|p| p.starts_with("pkg:cargo/waybill-fixture-cargo")),
        "non-migrated cargo reader should have emitted a pkg:cargo/... component; got PURLs: {:?}",
        purls,
    );
}
