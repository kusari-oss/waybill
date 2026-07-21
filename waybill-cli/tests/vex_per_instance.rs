//! Milestone 072 / T024 — canonical worked-example test from US2 AS#4
//! / SC-003.
//!
//! Synthesizes a target SBOM with TWO instances of the same PURL
//! `pkg:golang/golang.org/x/net@v0.28.0`:
//!
//! - **Instance A** (`bom-ref=foo-net-instance`) bound to source with
//!   `strength=verified` — the project's first-party binary.
//! - **Instance B** (`bom-ref=baselayer-net-instance`) unbound
//!   (`strength=unknown`) — a base-layer binary waybill couldn't
//!   trace to source.
//!
//! Source-tier OpenVEX has a `not_affected` statement on the PURL
//! (no per-instance identifier — typical pre-072 input).
//!
//! Run `waybill sbom enrich --vex-propagation-mode caveated`. Per
//! `contracts/openvex-instance-identifiers.md` C-3 + the spec's FR-008
//! aggregation rule, assertions:
//!
//! 1. Instance A receives `not_affected` cleanly (no caveat).
//! 2. Instance B receives the statement WITH a `waybill:vex-binding-
//!    status: unverified` caveat.
//! 3. The per-PURL aggregate (`affected ⊕ unbound-and-not-explicitly-
//!    vexed = affected`) reports `affected` because instance B is
//!    unverified — surfacing the user's worked-example concern that
//!    a verified `not_affected` on instance A doesn't mask
//!    instance B's potential affectedness.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::process::Command;

mod common;
use common::bin;

const PURL: &str = "pkg:golang/golang.org/x/net@v0.28.0";

fn binding_payload(strength: &str, reason: Option<&str>) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "source_doc_id": {
            "sha256": "e".repeat(64),
        },
        "strength": strength,
        "algo": "v1",
    });
    if strength != "unknown" {
        obj.as_object_mut()
            .unwrap()
            .insert("hash".to_string(), serde_json::Value::String("d".repeat(64)));
    }
    if let Some(r) = reason {
        obj.as_object_mut()
            .unwrap()
            .insert("reason".to_string(), serde_json::Value::String(r.to_string()));
    }
    obj
}

fn target_two_instance_sbom() -> serde_json::Value {
    let verified = serde_json::to_string(&binding_payload("verified", None)).unwrap();
    let unknown = serde_json::to_string(&binding_payload(
        "unknown",
        Some("base-layer-system-package"),
    ))
    .unwrap();

    serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "version": 1,
        "components": [
            {
                "type": "library",
                "name": "x/net",
                "version": "v0.28.0",
                "purl": PURL,
                "bom-ref": "foo-net-instance",
                "properties": [{
                    "name": "waybill:source-document-binding",
                    "value": verified,
                }],
            },
            {
                "type": "library",
                "name": "x/net",
                "version": "v0.28.0",
                "purl": PURL,
                "bom-ref": "baselayer-net-instance",
                "properties": [{
                    "name": "waybill:source-document-binding",
                    "value": unknown,
                }],
            },
        ],
    })
}

fn source_openvex_no_instance_id() -> serde_json::Value {
    serde_json::json!({
        "@context": "https://openvex.dev/ns/v0.2.0",
        "@id": "https://example.org/openvex/source",
        "author": "test-author",
        "timestamp": "2026-05-05T00:00:00Z",
        "version": 1,
        "statements": [{
            "vulnerability": { "name": "CVE-2024-12345" },
            "products": [{
                "@id": PURL,
                "identifiers": {
                    "purl": PURL,
                },
            }],
            "status": "not_affected",
            "justification": "vulnerable_code_not_present",
        }]
    })
}

#[test]
fn caveated_propagation_handles_per_instance_binding_correctly() {
    let tmp = tempfile::tempdir().unwrap();
    let target_path = tmp.path().join("target.cdx.json");
    let vex_path = tmp.path().join("source.openvex.json");
    std::fs::write(
        &target_path,
        serde_json::to_string_pretty(&target_two_instance_sbom()).unwrap(),
    )
    .unwrap();
    std::fs::write(
        &vex_path,
        serde_json::to_string_pretty(&source_openvex_no_instance_id()).unwrap(),
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
            "caveated",
            "--author",
            "test-team@example.com",
        ])
        .output()
        .expect("sbom enrich should run");

    assert!(
        out.status.success(),
        "caveated mode propagation must exit zero (no refusals). \
         stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let result: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&target_path).unwrap()).unwrap();

    // One vulnerabilities[] entry covering both matched instances.
    let vulns = result["vulnerabilities"].as_array().unwrap();
    assert_eq!(vulns.len(), 1, "one statement → one vulnerability entry");
    let vuln = &vulns[0];
    assert_eq!(vuln["id"], "CVE-2024-12345");
    assert_eq!(vuln["analysis"]["state"], "not_affected");
    assert_eq!(vuln["analysis"]["justification"], "vulnerable_code_not_present");

    // Two affects[] entries — one per instance.
    let affects = vuln["affects"].as_array().unwrap();
    assert_eq!(
        affects.len(),
        2,
        "broadcast to both instances of {} (caveated mode preserves \
         pre-072 broadcast semantic)",
        PURL,
    );

    // Locate each instance's `affects` entry by `ref`.
    let foo_entry = affects
        .iter()
        .find(|a| a["ref"] == "foo-net-instance")
        .expect("instance A (verified) present in affects[]");
    let baselayer_entry = affects
        .iter()
        .find(|a| a["ref"] == "baselayer-net-instance")
        .expect("instance B (unverified) present in affects[]");

    // (a) Instance A — verified-bound → no caveat.
    assert!(
        foo_entry.get("waybill:vex-binding-status").is_none(),
        "verified-bound instance must NOT carry waybill:vex-binding-status \
         caveat: {}",
        serde_json::to_string_pretty(foo_entry).unwrap_or_default(),
    );

    // (b) Instance B — unverified-bound → caveat present.
    let caveat = baselayer_entry["waybill:vex-binding-status"]
        .as_object()
        .expect("unverified instance must carry caveat sibling");
    assert_eq!(caveat["status"], "unverified");
    let reason = caveat["reason"].as_str().unwrap();
    assert!(
        reason.contains("binding-strength-unknown"),
        "caveat reason must name the binding-strength category: {reason}"
    );
    assert!(
        reason.contains("base-layer-system-package"),
        "caveat reason must surface the source-tier reason: {reason}"
    );

    // (c) Per-PURL aggregate per C-3 aggregation rule:
    // `affected ⊕ unbound-and-not-explicitly-vexed = affected`.
    // The post-072 consumer sees: instance A = not_affected (verified),
    // instance B = not_affected-but-caveated-unverified. Aggregating,
    // the unverified caveat means instance B's status is effectively
    // "could be affected"; aggregate = affected.
    //
    // This test verifies the underlying data shape that supports the
    // aggregation: both instances visible separately, instance B's
    // caveat machine-readable. The aggregation itself is a consumer-
    // side computation per C-3.
    let unverified_count = affects
        .iter()
        .filter(|a| a.get("waybill:vex-binding-status").is_some())
        .count();
    assert_eq!(
        unverified_count, 1,
        "exactly one instance carries the unverified caveat (the \
         unbound base-layer instance)",
    );
    let verified_count = affects
        .iter()
        .filter(|a| a.get("waybill:vex-binding-status").is_none())
        .count();
    assert_eq!(
        verified_count, 1,
        "exactly one instance is clean (the verified-bound first-party \
         instance)",
    );
}
