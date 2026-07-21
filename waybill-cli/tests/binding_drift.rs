//! Milestone 072 / T023 — strict-mode VEX propagation refusal on
//! binding drift.
//!
//! Synthesizes a (target SBOM, source OpenVEX) pair where the target
//! component is bound to source with `strength=weak` (NOT verified),
//! then runs `waybill sbom enrich --vex-overrides <vex.json>
//! --vex-propagation-mode strict` and asserts:
//!
//! 1. The command exits non-zero per VR-006.
//! 2. No `vulnerabilities[].affects[]` entry was written for the
//!    refused (vuln, instance) pair.
//! 3. A structured refusal-rationale annotation was added under the
//!    SBOM's top-level `properties[]` array (`waybill:vex-propagation-
//!    refusals`).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::process::Command;

mod common;
use common::bin;

/// Build a CDX target SBOM with one component carrying a weak
/// `waybill:source-document-binding`.
fn weak_bound_target_sbom() -> serde_json::Value {
    // Weak: lockfile + manifest match, no VCS commit recorded.
    let binding_payload = serde_json::json!({
        "source_doc_id": {
            "sha256": "e".repeat(64),
        },
        "hash": "d".repeat(64),
        "strength": "weak",
        "reason": "no-vcs-commit-recorded",
        "algo": "v1",
    });
    let binding_str = serde_json::to_string(&binding_payload).unwrap();
    serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "version": 1,
        "components": [{
            "type": "library",
            "name": "x",
            "version": "1.0.0",
            "purl": "pkg:golang/x@v1",
            "bom-ref": "x-bom-1",
            "properties": [{
                "name": "waybill:source-document-binding",
                "value": binding_str,
            }],
        }],
    })
}

/// Build a source-tier OpenVEX 0.2.0 document with one
/// `not_affected` statement on the same PURL.
fn source_openvex() -> serde_json::Value {
    serde_json::json!({
        "@context": "https://openvex.dev/ns/v0.2.0",
        "@id": "https://example.org/openvex/source",
        "author": "test-author",
        "timestamp": "2026-05-05T00:00:00Z",
        "version": 1,
        "statements": [{
            "vulnerability": { "name": "CVE-2026-9999" },
            "products": [{
                "@id": "pkg:golang/x@v1",
                "identifiers": {
                    "purl": "pkg:golang/x@v1",
                },
            }],
            "status": "not_affected",
            "justification": "vulnerable_code_not_present",
        }]
    })
}

#[test]
fn strict_mode_refuses_propagation_on_weak_binding() {
    let tmp = tempfile::tempdir().unwrap();
    let target_path = tmp.path().join("target.cdx.json");
    let vex_path = tmp.path().join("source.openvex.json");
    std::fs::write(
        &target_path,
        serde_json::to_string_pretty(&weak_bound_target_sbom()).unwrap(),
    )
    .unwrap();
    std::fs::write(
        &vex_path,
        serde_json::to_string_pretty(&source_openvex()).unwrap(),
    )
    .unwrap();

    let out = Command::new(bin())
        .args([
            "sbom",
            "enrich",
            target_path.to_str().unwrap(),
            "--vex-overrides",
            vex_path.to_str().unwrap(),
            "--vex-propagation-mode",
            "strict",
            "--author",
            "test-team@example.com",
        ])
        .output()
        .expect("sbom enrich should run");

    // Strict mode + weak binding → exit non-zero per VR-006.
    assert!(
        !out.status.success(),
        "strict mode propagation onto weak binding must exit non-zero. \
         stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // Output SBOM still written despite non-zero exit (so the
    // operator can audit refusal rationale).
    let result_text = std::fs::read_to_string(&target_path).unwrap();
    let result: serde_json::Value = serde_json::from_str(&result_text).unwrap();

    // No vulnerabilities[] entry for refused statement.
    let no_vulns = result
        .get("vulnerabilities")
        .map(|v| v.as_array().map(|a| a.is_empty()).unwrap_or(true))
        .unwrap_or(true);
    assert!(
        no_vulns,
        "refused VEX must not produce vulnerabilities[].affects[] entry: {}",
        serde_json::to_string_pretty(&result).unwrap_or_default()
    );

    // Refusal-rationale annotation present.
    let props = result["properties"].as_array().unwrap();
    let refusal = props
        .iter()
        .find(|p| p["name"] == "waybill:vex-propagation-refusals")
        .expect("refusal rationale property present");
    let value_str = refusal["value"].as_str().unwrap();
    assert!(value_str.contains("CVE-2026-9999"));
    assert!(value_str.contains("vex-propagation-refusal"));
    assert!(value_str.contains("\"binding_strength\":\"weak\""));
}

#[test]
fn strict_mode_with_verified_binding_succeeds() {
    // Inverse case: verified binding → strict mode propagates clean.
    let binding_payload = serde_json::json!({
        "source_doc_id": {
            "sha256": "e".repeat(64),
        },
        "hash": "d".repeat(64),
        "strength": "verified",
        "algo": "v1",
    });
    let binding_str = serde_json::to_string(&binding_payload).unwrap();
    let target_sbom = serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "version": 1,
        "components": [{
            "type": "library",
            "name": "x",
            "version": "1.0.0",
            "purl": "pkg:golang/x@v1",
            "bom-ref": "x-bom-1",
            "properties": [{
                "name": "waybill:source-document-binding",
                "value": binding_str,
            }],
        }],
    });

    let tmp = tempfile::tempdir().unwrap();
    let target_path = tmp.path().join("target.cdx.json");
    let vex_path = tmp.path().join("source.openvex.json");
    std::fs::write(&target_path, serde_json::to_string_pretty(&target_sbom).unwrap())
        .unwrap();
    std::fs::write(
        &vex_path,
        serde_json::to_string_pretty(&source_openvex()).unwrap(),
    )
    .unwrap();

    let out = Command::new(bin())
        .args([
            "sbom",
            "enrich",
            target_path.to_str().unwrap(),
            "--vex-overrides",
            vex_path.to_str().unwrap(),
            "--vex-propagation-mode",
            "strict",
            "--author",
            "test-team@example.com",
        ])
        .output()
        .expect("sbom enrich should run");

    assert!(
        out.status.success(),
        "strict mode with verified binding must exit zero. stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let result: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&target_path).unwrap()).unwrap();
    let vulns = result["vulnerabilities"].as_array().unwrap();
    assert_eq!(vulns.len(), 1);
    assert_eq!(vulns[0]["id"], "CVE-2026-9999");
    let affects = vulns[0]["affects"].as_array().unwrap();
    assert_eq!(affects.len(), 1);
    assert_eq!(affects[0]["ref"], "x-bom-1");
    // Verified → no caveat sibling.
    assert!(affects[0].get("waybill:vex-binding-status").is_none());
}
