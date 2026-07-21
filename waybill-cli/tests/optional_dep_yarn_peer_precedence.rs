//! Milestone 181 US3 — end-to-end regression guard for the m178/m181
//! peer-optional precedence rule (FR-005) on yarn.
//!
//! **Contract**: when a dep name appears in BOTH `dependenciesMeta.
//! <name>.optional = true` (m181 Optional-classification input) AND
//! in `peerDependencies + peerDependenciesMeta.<name>.optional = true`
//! (peer-optional), the peer classification wins. mikebom's yarn reader
//! MUST NOT classify the dep as `LifecycleScope::Optional` — its
//! lifecycle stays None/Runtime and the m180-shared `mikebom:optional-
//! derivation` annotation MUST NOT be emitted.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/optional_dep/yarn-peer-optional")
}

fn run_scan(project_root: &Path) -> Value {
    let out_dir = tempfile::tempdir().unwrap();
    let cdx_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    serde_json::from_slice(&std::fs::read(&cdx_path).unwrap()).unwrap()
}

fn find_component_by_name<'a>(cdx: &'a Value, name: &str) -> Option<&'a Value> {
    cdx.get("components")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
}

fn find_property<'a>(component: &'a Value, name: &str) -> Option<&'a Value> {
    component
        .get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value"))
}

#[test]
fn t022_yarn_peer_optional_stays_peer_not_optional() {
    // FR-005 flagship — peer-framework is BOTH `dependenciesMeta`
    // optional AND peer-optional. The reader-time guard MUST short-
    // circuit the Optional classification: no `mikebom:optional-
    // derivation`, no `scope: "excluded"` on the peer-framework
    // component.
    let cdx = run_scan(&fixture_path());

    let peer_framework = find_component_by_name(&cdx, "peer-framework")
        .expect("peer-framework component in CDX");

    // (a) peer-framework MUST NOT carry the m181 derivation annotation.
    assert!(
        find_property(peer_framework, "mikebom:optional-derivation").is_none(),
        "FR-005: peer-optional peer-framework MUST NOT carry mikebom:optional-derivation"
    );

    // (b) peer-framework MUST NOT be marked `scope: "excluded"` — its
    // lifecycle stays None/Runtime because the guard short-circuited
    // the Optional classification.
    assert!(
        peer_framework.get("scope").and_then(|v| v.as_str()) != Some("excluded"),
        "FR-005: peer-optional peer-framework MUST NOT be marked scope: \"excluded\" (lifecycle stays Runtime/None)"
    );
}
