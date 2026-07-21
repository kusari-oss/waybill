//! Milestone 136 US3 — macOS Casks emit as `pkg:brew/<token>@<version>?type=cask`
//! components. Modern Homebrew 4.0+ JSON-backed casks parse cleanly;
//! pre-4.0 `.rb`-only casks warn-and-skip per Constitution Principle I.
//!
//! Covers SC-003 + US3 acceptance scenarios + the Ruby-DSL warn-and-skip
//! design from research §R3 + R5.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn run_scan(rootfs: &Path) -> (Value, String) {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(rootfs)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    assert!(result.status.success(), "scan failed: stderr={stderr}");
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

fn write_cask_json(
    rootfs: &Path,
    prefix: &str,
    token: &str,
    version: &str,
    timestamp: &str,
    json_body: &str,
) {
    let dir = rootfs
        .join(prefix)
        .join("Caskroom")
        .join(token)
        .join(".metadata")
        .join(version)
        .join(timestamp)
        .join("Casks");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{token}.json")), json_body).unwrap();
    // Also create the payload version dir so the cask walker iterates it.
    std::fs::create_dir_all(rootfs.join(prefix).join("Caskroom").join(token).join(version))
        .unwrap();
}

fn write_cask_rb_only(
    rootfs: &Path,
    prefix: &str,
    token: &str,
    version: &str,
    timestamp: &str,
    rb_body: &str,
) {
    let dir = rootfs
        .join(prefix)
        .join("Caskroom")
        .join(token)
        .join(".metadata")
        .join(version)
        .join(timestamp)
        .join("Casks");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{token}.rb")), rb_body).unwrap();
    std::fs::create_dir_all(rootfs.join(prefix).join("Caskroom").join(token).join(version))
        .unwrap();
}

fn brew_purls(doc: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            if let Some(p) = c.get("purl").and_then(|v| v.as_str()) {
                if p.starts_with("pkg:brew/") {
                    out.push(p.to_string());
                }
            }
        }
    }
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        if let Some(p) = c.get("purl").and_then(|v| v.as_str()) {
            if p.starts_with("pkg:brew/") {
                out.push(p.to_string());
            }
        }
    }
    out
}

#[test]
fn sc_003_cask_emits_with_type_cask_qualifier_no_dep_edges() {
    let tmp = tempfile::tempdir().unwrap();
    write_cask_json(
        tmp.path(),
        "opt/homebrew",
        "visual-studio-code",
        "1.95.3",
        "20251001120000.000",
        r#"{"token":"visual-studio-code","version":"1.95.3"}"#,
    );

    let (doc, _) = run_scan(tmp.path());
    let purls = brew_purls(&doc);
    assert_eq!(purls.len(), 1);
    assert_eq!(purls[0], "pkg:brew/visual-studio-code@1.95.3?type=cask");

    // Find the cask component and verify it has no dep edges.
    let components = doc.get("components").and_then(|v| v.as_array()).unwrap();
    let cask = components
        .iter()
        .find(|c| {
            c.get("purl").and_then(|v| v.as_str())
                == Some("pkg:brew/visual-studio-code@1.95.3?type=cask")
        })
        .expect("cask component must exist");
    let cask_ref = cask
        .get("bom-ref")
        .and_then(|v| v.as_str())
        .expect("cask bom-ref");

    // The cask's `dependencies` entry (if present) must have an empty
    // `dependsOn`. waybill only emits a dependencies entry when there
    // are no edges OR when the component is referenced as a dep
    // target — so absence is equally valid.
    let deps = doc.get("dependencies").and_then(|v| v.as_array()).unwrap();
    let cask_deps_entry = deps
        .iter()
        .find(|d| d.get("ref").and_then(|v| v.as_str()) == Some(cask_ref));
    if let Some(entry) = cask_deps_entry {
        let depends_on =
            entry.get("dependsOn").and_then(|v| v.as_array()).unwrap();
        assert!(
            depends_on.is_empty(),
            "casks must emit no dep edges; got {depends_on:?}",
        );
    }
}

#[test]
fn ruby_dsl_only_cask_warns_and_skips() {
    // R5 — pre-4.0 casks with only .rb metadata trigger warn-and-skip.
    let tmp = tempfile::tempdir().unwrap();
    write_cask_rb_only(
        tmp.path(),
        "opt/homebrew",
        "transmission",
        "3.00",
        "20240101000000.000",
        "cask 'transmission' do\n  version '3.00'\nend",
    );

    let (doc, stderr) = run_scan(tmp.path());
    let purls = brew_purls(&doc);
    assert!(
        purls.is_empty(),
        "Ruby-DSL-only cask must NOT emit; got {purls:?}",
    );
    // Warn line MUST name the cask AND cite Ruby-DSL.
    assert!(
        stderr.contains("transmission") && stderr.contains("Ruby-DSL"),
        "stderr must warn about transmission's Ruby-DSL skip; got:\n{stderr}",
    );
}

#[test]
fn formula_and_cask_coexistence_with_distinct_purls() {
    // T021 + U4 (analysis remediation) — formula and cask coexist in
    // the same scan; PURLs are distinguishable by the type=cask
    // qualifier. Also covers the same-name collision potential.
    let tmp = tempfile::tempdir().unwrap();
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "curl",
        "8.5.0",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );
    write_cask_json(
        tmp.path(),
        "opt/homebrew",
        "firefox",
        "121.0",
        "20251001120000.000",
        r#"{"token":"firefox","version":"121.0"}"#,
    );

    let (doc, _) = run_scan(tmp.path());
    let purls = brew_purls(&doc);
    assert_eq!(purls.len(), 2);
    assert!(purls.contains(&"pkg:brew/curl@8.5.0".to_string()));
    assert!(purls.contains(&"pkg:brew/firefox@121.0?type=cask".to_string()));
}

#[test]
fn same_name_formula_and_cask_collapse_via_deduplicator() {
    // U4 (analysis remediation) — when a formula and a cask share
    // (ecosystem, name, version), the post-emission deduplicator at
    // `waybill-cli/src/resolve/deduplicator.rs::deduplicate` groups
    // by `(ecosystem, name, version, parent_purl)` — the `?type=cask`
    // qualifier does NOT participate in the dedup key. The two
    // entries collapse to one survivor (highest-confidence wins).
    //
    // This is by-design behavior, not a brew-reader bug. Documenting
    // it here so future contributors understand the cross-reader
    // dedup semantics. If consumer demand surfaces for keeping
    // formula+cask distinct on collision, that would be a
    // deduplicator change with cross-reader implications — out of
    // scope for milestone 136.
    let tmp = tempfile::tempdir().unwrap();
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "firefox",
        "121.0",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );
    write_cask_json(
        tmp.path(),
        "opt/homebrew",
        "firefox",
        "121.0",
        "20251001120000.000",
        r#"{"token":"firefox","version":"121.0"}"#,
    );

    let (doc, _) = run_scan(tmp.path());
    let purls = brew_purls(&doc);
    assert_eq!(
        purls.len(),
        1,
        "deduplicator collapses same-identity formula+cask; got {purls:?}",
    );
    // The survivor's PURL is whichever entry sorts to the highest
    // confidence (both come from package-DB readers — same confidence;
    // result depends on HashMap iteration order). Just confirm SOMETHING
    // named firefox@121.0 survives.
    let survivor = &purls[0];
    assert!(
        survivor.starts_with("pkg:brew/firefox@121.0"),
        "survivor must be firefox@121.0; got {survivor}",
    );
}
