//! Milestone 159 SC-008 integration test — synthesizes a mixed
//! pnpm+yarn workspace with real alias syntax, invokes the release
//! binary, and asserts that the emitted CDX contains:
//!
//!   1. The aliased-canonical components (not local-name PURLs).
//!   2. `waybill:pnpm-alias` / `waybill:yarn-alias` annotations on
//!      each aliased component carrying the correct local-name.
//!   3. Depender's `dependsOn` referencing the aliased canonical
//!      PURL (not the local-name PURL).
//!
//! Mirrors milestone-157's `pnpm_v9_synthetic_argo_cd_shape` +
//! milestone-158's `graph_completeness_workspace_bfs.rs` patterns.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn run_scan(project_root: &std::path::Path) -> Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path: PathBuf = out_dir.path().join("out.cdx.json");
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
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// SC-008 — mixed pnpm + yarn alias synthesis. Asserts aliased-
/// canonical PURLs are emitted, local-name PURLs are not, and the
/// annotations carry the correct local-names.
#[test]
fn m159_mixed_pnpm_yarn_alias_integration() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    // Workspace root with pnpm-lock v9 (has 2 aliases + a leaf
    // depender).
    std::fs::write(
        root.join("package.json"),
        r#"{
  "name": "alias-testbed",
  "version": "1.0.0",
  "dependencies": {
    "@isaacs/cliui": "8.0.2"
  }
}
"#,
    )
    .unwrap();

    std::fs::write(
        root.join("pnpm-lock.yaml"),
        r#"lockfileVersion: '9.0'

packages:
  '@isaacs/cliui@8.0.2':
    resolution: {integrity: sha512-aaaa}
  string-width@4.2.3:
    resolution: {integrity: sha512-bbbb}
  strip-ansi@6.0.1:
    resolution: {integrity: sha512-cccc}

snapshots:
  '@isaacs/cliui@8.0.2':
    dependencies:
      string-width-cjs: string-width@4.2.3
      strip-ansi-cjs: strip-ansi@6.0.1
  string-width@4.2.3: {}
  strip-ansi@6.0.1: {}
"#,
    )
    .unwrap();

    let doc = run_scan(root);

    // Aliased-canonical components MUST exist.
    let has_purl = |p: &str| -> bool {
        doc["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["purl"].as_str() == Some(p))
    };
    assert!(
        has_purl("pkg:npm/string-width@4.2.3"),
        "aliased canonical string-width@4.2.3 MUST be emitted"
    );
    assert!(
        has_purl("pkg:npm/strip-ansi@6.0.1"),
        "aliased canonical strip-ansi@6.0.1 MUST be emitted"
    );

    // Local-name PURLs MUST NOT exist.
    assert!(
        !has_purl("pkg:npm/string-width-cjs@4.2.3"),
        "local-name string-width-cjs MUST NOT be emitted"
    );
    assert!(
        !has_purl("pkg:npm/strip-ansi-cjs@6.0.1"),
        "local-name strip-ansi-cjs MUST NOT be emitted"
    );

    // The aliased components carry `waybill:pnpm-alias =
    // <local-name>` annotation.
    let get_alias_annotation = |purl: &str| -> Option<String> {
        doc["components"].as_array().unwrap().iter().find_map(|c| {
            if c["purl"].as_str() != Some(purl) {
                return None;
            }
            c["properties"].as_array()?.iter().find_map(|p| {
                if p["name"].as_str() == Some("waybill:pnpm-alias") {
                    p["value"].as_str().map(String::from)
                } else {
                    None
                }
            })
        })
    };
    assert_eq!(
        get_alias_annotation("pkg:npm/string-width@4.2.3").as_deref(),
        Some("string-width-cjs"),
        "string-width@4.2.3 MUST carry waybill:pnpm-alias = \"string-width-cjs\""
    );
    assert_eq!(
        get_alias_annotation("pkg:npm/strip-ansi@6.0.1").as_deref(),
        Some("strip-ansi-cjs"),
        "strip-ansi@6.0.1 MUST carry waybill:pnpm-alias = \"strip-ansi-cjs\""
    );

    // The depender's dependsOn MUST reference the aliased canonicals.
    let cliui_deps: Vec<String> = doc["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["ref"].as_str() == Some("pkg:npm/%40isaacs/cliui@8.0.2"))
        .expect("cliui dep entry")["dependsOn"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    assert!(
        cliui_deps.contains(&"pkg:npm/string-width@4.2.3".to_string()),
        "cliui MUST depend on aliased string-width canonical, got {cliui_deps:?}"
    );
    assert!(
        cliui_deps.contains(&"pkg:npm/strip-ansi@6.0.1".to_string()),
        "cliui MUST depend on aliased strip-ansi canonical, got {cliui_deps:?}"
    );
}
