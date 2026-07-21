//! Milestone 136 US1 — end-to-end integration test that a synthetic
//! Apple Silicon Homebrew install produces a CDX SBOM containing one
//! component per formula with the canonical `pkg:brew/<formula>@<version>`
//! PURL identity and accurate dep edges from `runtime_dependencies`.
//!
//! Covers spec acceptance scenarios US1.1, US1.2, and US1.3 plus
//! SC-001 (Apple Silicon baseline), SC-007 (tap qualifier presence/absence),
//! and FR-006 (no-op on missing prefix).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn run_scan(project_root: &Path) -> (Value, String) {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
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
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    assert!(
        result.status.success(),
        "scan failed: stderr={stderr}",
    );
    let bytes = std::fs::read(&out_path).unwrap();
    (serde_json::from_slice(&bytes).unwrap(), stderr)
}

fn write_formula(
    rootfs: &Path,
    prefix: &str,
    formula: &str,
    version: &str,
    receipt_body: &str,
) {
    let dir = rootfs
        .join(prefix)
        .join("Cellar")
        .join(formula)
        .join(version);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("INSTALL_RECEIPT.json"), receipt_body).unwrap();
}

fn brew_purls(doc: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut visit = |c: &Value| {
        if let Some(p) = c.get("purl").and_then(|v| v.as_str()) {
            if p.starts_with("pkg:brew/") {
                out.push(p.to_string());
            }
        }
    };
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            visit(c);
        }
    }
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        visit(c);
    }
    out
}

fn find_bom_ref(doc: &Value, purl: &str) -> Option<String> {
    let components = doc.get("components")?.as_array()?;
    for c in components {
        if c.get("purl").and_then(|v| v.as_str()) == Some(purl) {
            return c
                .get("bom-ref")
                .and_then(|v| v.as_str())
                .map(String::from);
        }
    }
    None
}

#[test]
fn sc_001_apple_silicon_three_formulae_with_dep_edges() {
    let tmp = tempfile::tempdir().unwrap();
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "curl",
        "8.5.0",
        r#"{
            "source": {"tap": "homebrew/core"},
            "runtime_dependencies": [
                {"full_name": "openssl@3", "version": "3.4.0"},
                {"full_name": "brotli", "version": "1.1.0"}
            ]
        }"#,
    );
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "openssl@3",
        "3.4.0",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "brotli",
        "1.1.0",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );

    let (doc, _) = run_scan(tmp.path());
    let purls = brew_purls(&doc);

    // (a) Exactly 3 pkg:brew/* components.
    assert_eq!(purls.len(), 3, "expected 3 brew components, got {purls:?}");

    // (b) Each has the expected PURL — no tap qualifier (all from
    //     homebrew/core, the default tap).
    assert!(purls.contains(&"pkg:brew/curl@8.5.0".to_string()));
    assert!(purls.contains(&"pkg:brew/openssl@3@3.4.0".to_string()));
    assert!(purls.contains(&"pkg:brew/brotli@1.1.0".to_string()));

    // (c) curl's dependsOn edges target openssl@3 + brotli bom-refs.
    let curl_ref = find_bom_ref(&doc, "pkg:brew/curl@8.5.0").expect("curl bom-ref");
    let openssl_ref =
        find_bom_ref(&doc, "pkg:brew/openssl@3@3.4.0").expect("openssl bom-ref");
    let brotli_ref =
        find_bom_ref(&doc, "pkg:brew/brotli@1.1.0").expect("brotli bom-ref");

    let deps = doc.get("dependencies").and_then(|v| v.as_array()).unwrap();
    let curl_deps = deps
        .iter()
        .find(|d| d.get("ref").and_then(|v| v.as_str()) == Some(&curl_ref))
        .and_then(|d| d.get("dependsOn").and_then(|v| v.as_array()))
        .expect("curl must have a dependencies entry");
    let curl_dep_refs: Vec<&str> =
        curl_deps.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        curl_dep_refs.contains(&openssl_ref.as_str()),
        "curl dependsOn must target openssl; got {curl_dep_refs:?}",
    );
    assert!(
        curl_dep_refs.contains(&brotli_ref.as_str()),
        "curl dependsOn must target brotli; got {curl_dep_refs:?}",
    );
}

#[test]
fn sc_007_third_party_tap_qualifier_present_default_tap_qualifier_absent() {
    let tmp = tempfile::tempdir().unwrap();
    // Default-tap formula — no tap= qualifier.
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "curl",
        "8.5.0",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );
    // Third-party-tap formula — tap= qualifier present.
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "terraform",
        "1.10.0",
        r#"{"source": {"tap": "hashicorp/tap"}}"#,
    );

    let (doc, _) = run_scan(tmp.path());
    let purls = brew_purls(&doc);

    assert!(purls.contains(&"pkg:brew/curl@8.5.0".to_string()));
    assert!(purls.contains(&"pkg:brew/terraform@1.10.0?tap=hashicorp/tap".to_string()));
    // And critically: NO `?tap=` qualifier on the core-tap entry.
    assert!(
        !purls.iter().any(|p| p.starts_with("pkg:brew/curl") && p.contains("tap=")),
        "core-tap curl must not carry a tap= qualifier; got {purls:?}",
    );
}

#[test]
fn fr_006_no_homebrew_emits_zero_components_no_warn() {
    // FR-006 — rootfs with none of the three Homebrew prefix Cellar/
    // dirs produces zero brew components and no homebrew warnings.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("etc")).unwrap();
    std::fs::write(tmp.path().join("etc/hostname"), "test").unwrap();

    let (doc, stderr) = run_scan(tmp.path());
    let purls = brew_purls(&doc);
    assert!(purls.is_empty(), "no brew components expected; got {purls:?}");

    // FR-006 — no WARN-level pacman/brew/homebrew noise on a clean scan.
    assert!(
        !stderr.contains("WARN")
            || (!stderr.to_lowercase().contains("brew")
                && !stderr.to_lowercase().contains("homebrew")),
        "no WARN about brew expected on non-Homebrew scan; stderr:\n{stderr}",
    );
}

#[test]
fn sc_006_standard_purl_filter_enumerates_brew_components() {
    // SC-006 — an external consumer using only the standard PURL
    // filter (no brew-specific code) enumerates every brew component.
    let tmp = tempfile::tempdir().unwrap();
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "curl",
        "8.5.0",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "jq",
        "1.7.1",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );

    let (doc, _) = run_scan(tmp.path());
    // Standard filter: walk components[], select purl.startswith("pkg:brew/")
    let brew_purls_list = brew_purls(&doc);
    assert_eq!(brew_purls_list.len(), 2);
}
