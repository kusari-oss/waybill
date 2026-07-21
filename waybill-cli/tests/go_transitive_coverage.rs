//! Milestone 160 (T039): SC-009 integration test — Go transitive-edge
//! coverage annotations end-to-end via the release binary.
//!
//! Rather than synthesize a mock proxy (which would need a valid go.sum
//! plus hash-signed modules to survive the go.sum-hash check), this
//! test uses the existing milestone-055 `simple-module` fixture in
//! `--offline` mode. Offline mode is the deterministic path that:
//!
//!   - Emits every Go component with
//!     `waybill:go-transitive-source = "go-sum-fallback"` (step 5
//!     claims all go.sum modules).
//!   - Emits doc-scope `waybill:go-transitive-coverage = "unknown"`
//!     with reason `offline-mode: transitive edges from proxy fetches
//!     unavailable` (Q1 caution-first).
//!   - Does NOT emit `waybill:go-transitive-unresolved-reason` because
//!     no module reaches `ResolutionStep::None` (step 5 always claims).
//!
//! This exercises the full pipeline: resolver → per-component emitter
//! (T022/T023) → doc-scope emitter (T034/T035).

use std::path::PathBuf;
use std::process::Command;

fn fixture(sub: &str) -> PathBuf {
    PathBuf::from(env!("WAYBILL_FIXTURES_DIR")).join("go").join(sub)
}

fn scan_offline(path: &std::path::Path) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let out_path = tmp.path().to_path_buf();
    let mut cmd = Command::new(bin);
    cmd.env("WAYBILL_NO_GO_MOD_WHY", "1");
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");
    let output = cmd.output().expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn doc_property<'a>(sbom: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    sbom["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)?
        ["value"]
        .as_str()
}

fn go_components(sbom: &serde_json::Value) -> Vec<&serde_json::Value> {
    sbom["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .map(|p| p.starts_with("pkg:golang/"))
                .unwrap_or(false)
        })
        .collect()
}

fn component_property<'a>(component: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    component["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)?
        ["value"]
        .as_str()
}

/// SC-005 + SC-006: doc-scope C110 == "unknown" + C111 reason "offline-mode:...".
#[test]
fn t039_offline_scan_emits_unknown_coverage_with_reason() {
    let sbom = scan_offline(&fixture("simple-module"));

    let c110 = doc_property(&sbom, "waybill:go-transitive-coverage");
    assert_eq!(
        c110,
        Some("unknown"),
        "SC-006: offline scan MUST emit C110 = unknown; got {c110:?}"
    );

    let c111 = doc_property(&sbom, "waybill:go-transitive-coverage-reason");
    let c111_str = c111.expect("SC-006: C111 must accompany unknown-value C110");
    assert!(
        c111_str.starts_with("offline-mode:"),
        "SC-006: C111 reason must start with 'offline-mode:', got: {c111_str:?}"
    );
}

/// SC-004: every Go module component carries `waybill:go-transitive-source`
/// (universal per Q2). The synthetic `pkg:golang/stdlib@vX.Y.Z` component is
/// exempt — it's not a go.sum-declared module and doesn't flow through the
/// resolver ladder.
#[test]
fn t039_every_go_component_has_transitive_source_annotation() {
    let sbom = scan_offline(&fixture("simple-module"));
    let go_comps: Vec<&serde_json::Value> = go_components(&sbom)
        .into_iter()
        .filter(|c| {
            !c["purl"]
                .as_str()
                .map(|p| p.starts_with("pkg:golang/stdlib@"))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        !go_comps.is_empty(),
        "fixture must produce at least one non-stdlib Go component"
    );
    for c in &go_comps {
        let source = component_property(c, "waybill:go-transitive-source");
        assert!(
            source.is_some(),
            "SC-004: Go component missing waybill:go-transitive-source: {}",
            c["purl"].as_str().unwrap_or("?")
        );
        // Value must be one of the 5-code C108 vocab.
        let val = source.unwrap();
        assert!(
            matches!(
                val,
                "go-mod-graph" | "module-cache" | "proxy-fetch" | "go-sum-fallback" | "unresolved"
            ),
            "SC-004: C108 value out of vocab: {val:?} on {}",
            c["purl"].as_str().unwrap_or("?")
        );
    }
}

/// FR-003: C109 (`waybill:go-transitive-unresolved-reason`) MUST accompany C108
/// exactly when C108 == "unresolved". In offline mode step 5 claims every
/// module, so no C109 should be emitted anywhere.
#[test]
fn t039_c109_absent_when_no_component_is_unresolved_in_offline() {
    let sbom = scan_offline(&fixture("simple-module"));
    for c in go_components(&sbom) {
        let c108 = component_property(c, "waybill:go-transitive-source");
        let c109 = component_property(c, "waybill:go-transitive-unresolved-reason");
        if c108 == Some("unresolved") {
            assert!(
                c109.is_some(),
                "FR-003: C109 must be present when C108 == unresolved on {}",
                c["purl"].as_str().unwrap_or("?")
            );
        } else {
            assert!(
                c109.is_none(),
                "FR-003: C109 must be absent when C108 != unresolved on {}",
                c["purl"].as_str().unwrap_or("?")
            );
        }
    }
}
