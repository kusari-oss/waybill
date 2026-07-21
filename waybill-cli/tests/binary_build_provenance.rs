//! Milestone 098 integration test — compiler/linker version
//! extraction end-to-end (scan → BinaryScan → annotation → SBOM).
//!
//! Self-scan: copies the waybill binary itself into a temp dir and
//! verifies the new build-tier properties appear on the emitted
//! file-level binary component. Per-host expectations differ by
//! binary format; graceful-skip when the test environment doesn't
//! produce a file-level binary component (e.g., containerized scans
//! where the host's `waybill` was claimed by a package-db reader).
//!
//! Closes the SC-001 + SC-002 end-to-end coverage gap that the
//! analyze-phase C1 finding flagged.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn scan(dir: &Path) -> Value {
    let out_file = dir.join("out.cdx.json");
    let output = Command::new(binary_path())
        .args(["sbom", "scan", "--path"])
        .arg(dir)
        .arg("--output")
        .arg(&out_file)
        .arg("--no-deep-hash")
        .output()
        .expect("failed to invoke waybill");
    assert!(
        output.status.success(),
        "waybill sbom scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let json_bytes = std::fs::read(&out_file).expect("SBOM not written");
    serde_json::from_slice(&json_bytes).expect("invalid SBOM JSON")
}

fn property_value(component: &Value, name: &str) -> Option<Value> {
    component["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))
        .map(|p| p["value"].clone())
}

fn find_file_level(sbom: &Value) -> Option<&Value> {
    sbom["components"]
        .as_array()?
        .iter()
        .find(|c| property_value(c, "waybill:binary-class").is_some())
}

/// Per-host milestone-098 end-to-end smoke. Asserts the relevant
/// build-tier property is emitted for the host binary's format.
/// Graceful-skip when no file-level binary component is produced
/// (the host's waybill was claimed by a package-db reader, or the
/// scan walked into a directory without an unclaimed binary).
#[test]
fn mikebom_self_scan_emits_build_provenance_properties() {
    let dir = TempDir::new().unwrap();
    let dest = dir.path().join("waybill-under-test");
    std::fs::copy(binary_path(), &dest).unwrap();

    let sbom = scan(dir.path());
    let Some(file_level) = find_file_level(&sbom) else {
        eprintln!(
            "skipping milestone-098 smoke: no file-level binary component \
             emitted (host's waybill may have been claimed by a package-db \
             reader, or the temp-dir walk found no unclaimed ELF/Mach-O/PE)"
        );
        return;
    };

    let binary_class = property_value(file_level, "waybill:binary-class")
        .and_then(|v| v.as_str().map(str::to_string))
        .expect("binary-class is present on every file-level binary component");

    match binary_class.as_str() {
        "elf" => {
            // FR-001 / SC-001 end-to-end: ELF `.comment` stamps appear
            // on the file-level component. Property value is a
            // JSON-encoded string per the waybill annotation convention.
            // Don't lock the prefix — rustc + statically-linked C
            // dependencies may both contribute stamps, and the exact
            // stamp format varies across toolchain versions.
            let raw = property_value(file_level, "waybill:elf-compiler-stamps")
                .expect("ELF binary should emit waybill:elf-compiler-stamps");
            let json_str = raw.as_str().expect("property value is JSON-encoded string");
            let parsed: Value = serde_json::from_str(json_str)
                .expect("compiler-stamps value must be a JSON array string");
            let arr = parsed.as_array()
                .expect("compiler-stamps value must decode to a JSON array");
            assert!(
                !arr.is_empty(),
                "FR-001: ELF binary should emit ≥1 compiler stamp, got: {arr:?}"
            );
        }
        "macho" => {
            // FR-002 / SC-002 end-to-end: LC_BUILD_VERSION fields
            // appear on the file-level component. Modern Rust
            // toolchains target macOS with LC_BUILD_VERSION, so this
            // should fire on any recent macOS-built waybill.
            let raw = property_value(file_level, "waybill:macho-build-version")
                .expect("Mach-O binary should emit waybill:macho-build-version");
            let json_str = raw.as_str().expect("property value is JSON-encoded string");
            let bv: Value = serde_json::from_str(json_str)
                .expect("build-version value must be a JSON object string");
            assert_eq!(
                bv["platform"].as_str(),
                Some("macos"),
                "SC-002: Mach-O scan of waybill-self → platform=macos, got {bv:?}"
            );
            let raw_tools = property_value(file_level, "waybill:macho-build-tools")
                .expect("Mach-O binary should emit waybill:macho-build-tools");
            let tools_str = raw_tools.as_str().expect("build-tools is a JSON-encoded string");
            let tools: Value = serde_json::from_str(tools_str)
                .expect("build-tools value must be a JSON array string");
            let tools_arr = tools.as_array()
                .expect("build-tools value must decode to a JSON array");
            assert!(
                !tools_arr.is_empty(),
                "SC-002: Mach-O scan should emit ≥1 tool entry, got: {tools_arr:?}"
            );
        }
        "pe" => {
            // FR-003 / SC-003 always-emit: every parseable PE emits
            // a `waybill:pe-linker-version` matching `^\d+\.\d+$`.
            let lv = property_value(file_level, "waybill:pe-linker-version")
                .expect("PE binary should always emit waybill:pe-linker-version");
            let s = lv.as_str().expect("linker-version value must be a string");
            let parts: Vec<&str> = s.split('.').collect();
            assert_eq!(
                parts.len(),
                2,
                "SC-003: pe-linker-version must match <major>.<minor>, got {s:?}"
            );
            assert!(
                parts[0].chars().all(|c| c.is_ascii_digit())
                    && parts[1].chars().all(|c| c.is_ascii_digit()),
                "SC-003: pe-linker-version segments must be decimal, got {s:?}"
            );
        }
        other => {
            eprintln!("skipping milestone-098 smoke: unrecognized binary-class {other:?}");
        }
    }
}
