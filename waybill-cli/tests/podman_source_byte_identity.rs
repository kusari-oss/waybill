//! Milestone 206 (#440) — FR-005 byte-identity guardrail.
//!
//! Runs on every platform + every CI lane. Asserts that non-image
//! scans (--path <dir>) don't emit the new C124 waybill:image-source
//! annotation. This is the FR-005 / SC-005 regression guard: adding
//! m206's C124 to the emitters must NOT change output for pre-m206
//! scan modes.

use std::process::Command;

fn mikebom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn scan_path_to_cdx(path: &str) -> serde_json::Value {
    let tempdir = tempfile::tempdir().unwrap();
    let out = tempdir.path().join("out.cdx.json");
    let cmd = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--offline",
            "--path",
            path,
            "--format",
            "cyclonedx-json",
            "--output",
            out.to_str().unwrap(),
            "--no-deep-hash",
        ])
        .output()
        .expect("spawn waybill");
    assert!(
        cmd.status.success(),
        "scan should succeed. stderr:\n{}",
        String::from_utf8_lossy(&cmd.stderr),
    );
    serde_json::from_slice(&std::fs::read(&out).unwrap()).expect("output is valid JSON")
}

fn has_image_source_property(cdx: &serde_json::Value) -> bool {
    cdx.pointer("/metadata/properties")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter().any(|p| {
                p.get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s == "waybill:image-source")
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

#[test]
fn fr005_non_image_scan_omits_image_source_annotation() {
    // Scan an existing non-image public_corpus fixture. m206's
    // conditional emission (podman-only) MUST leave this annotation
    // absent for --path scans.
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/public_corpus/npm-express"
    );
    let cdx = scan_path_to_cdx(fixture);
    assert!(
        !has_image_source_property(&cdx),
        "FR-005 / SC-005: non-image scan MUST NOT emit waybill:image-source. cdx: {cdx:#}"
    );
}
