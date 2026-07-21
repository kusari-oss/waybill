//! Milestone 139 US1 — iOS app baseline tests.
//!
//! Covers SC-001 (iOS app baseline), SC-007 (SHA-1 hash emission per
//! FR-008), SC-008 (main-module emission with dep edges), SC-009
//! (subspec distinct components with `#subpath` PURL form).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn run_scan(project_root: &Path) -> Value {
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
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn cocoapods_purls(doc: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut visit = |c: &Value| {
        if let Some(p) = c.get("purl").and_then(|v| v.as_str()) {
            if p.starts_with("pkg:cocoapods/")
                || (p.starts_with("pkg:generic/")
                    && c.get("properties")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter().any(|p| {
                                p.get("name").and_then(|v| v.as_str())
                                    == Some("waybill:source-type")
                                    && p.get("value")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.starts_with("cocoapods-"))
                                        .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false))
            {
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

fn component_with_purl<'a>(doc: &'a Value, purl: &str) -> Option<&'a Value> {
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        if c.get("purl").and_then(|v| v.as_str()) == Some(purl) {
            return Some(c);
        }
    }
    doc.get("components")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter().find(|c| {
                c.get("purl").and_then(|v| v.as_str()) == Some(purl)
            })
        })
}

fn property_value<'a>(component: &'a Value, name: &str) -> Option<&'a str> {
    component
        .get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
}

fn write_ios_app_fixture(root: &Path) {
    std::fs::write(
        root.join("Podfile"),
        "platform :ios, '13.0'\n\
         target 'MyApp' do\n\
         \x20\x20pod 'AFNetworking', '~> 4.0'\n\
         \x20\x20pod 'SDWebImage', '~> 5.18'\n\
         \x20\x20pod 'Firebase/Core'\n\
         end\n",
    )
    .unwrap();
    let sha1 = "a".repeat(40);
    let firebase_sha = "b".repeat(40);
    std::fs::write(
        root.join("Podfile.lock"),
        format!(
            "PODS:\n\
             - AFNetworking (4.0.1)\n\
             - SDWebImage (5.18.10)\n\
             - Firebase/Core (10.20.0):\n  - FirebaseCore (~> 10.20)\n\
             - FirebaseCore (10.20.0):\n  - GoogleUtilities/Environment (~> 7.13)\n\
             - GoogleUtilities/Environment (7.13.0)\n\
\n\
             DEPENDENCIES:\n\
             - AFNetworking (~> 4.0)\n\
             - SDWebImage (~> 5.18)\n\
             - Firebase/Core\n\
\n\
             SPEC CHECKSUMS:\n\
             \x20\x20AFNetworking: {sha1}\n\
             \x20\x20SDWebImage: {sha1}\n\
             \x20\x20Firebase: {firebase_sha}\n\
             \x20\x20FirebaseCore: {firebase_sha}\n\
             \x20\x20GoogleUtilities: {sha1}\n\
\n\
             PODFILE CHECKSUM: deadbeef\n\
             COCOAPODS: 1.15.2\n",
        ),
    )
    .unwrap();
}

#[test]
fn ios_app_baseline_emits_pods_count_plus_main_module() {
    // SC-001: 5 PODS entries + 1 main-module = 6 cocoapods-derived
    // components.
    let tmp = tempfile::tempdir().unwrap();
    write_ios_app_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let purls = cocoapods_purls(&doc);
    assert_eq!(
        purls.len(),
        6,
        "expected 6 cocoapods components (5 PODS + 1 main-module); got {purls:#?}"
    );
    for expected in &[
        "pkg:cocoapods/AFNetworking@4.0.1",
        "pkg:cocoapods/SDWebImage@5.18.10",
        "pkg:cocoapods/Firebase@10.20.0#Core",
        "pkg:cocoapods/FirebaseCore@10.20.0",
        "pkg:cocoapods/GoogleUtilities@7.13.0#Environment",
        "pkg:cocoapods/MyApp",
    ] {
        assert!(
            purls.contains(&expected.to_string()),
            "expected PURL {expected} not found; got {purls:#?}"
        );
    }
}

#[test]
fn main_module_emission_from_target_block() {
    // SC-008: Podfile `target 'MyApp' do` produces main-module
    // `pkg:cocoapods/MyApp` with component-role annotation.
    let tmp = tempfile::tempdir().unwrap();
    write_ios_app_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let main = component_with_purl(&doc, "pkg:cocoapods/MyApp")
        .expect("main-module component must exist");
    assert_eq!(
        property_value(main, "waybill:component-role"),
        Some("main-module"),
    );
}

#[test]
fn main_module_depends_lists_direct_deps() {
    // SC-008: dependencies[] entry for main-module bom-ref targets each
    // direct dep's bom-ref.
    let tmp = tempfile::tempdir().unwrap();
    write_ios_app_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let main = component_with_purl(&doc, "pkg:cocoapods/MyApp").unwrap();
    let main_ref = main.get("bom-ref").and_then(|v| v.as_str()).unwrap();
    let deps = doc.get("dependencies").and_then(|v| v.as_array()).unwrap();
    let main_deps = deps
        .iter()
        .find(|d| d.get("ref").and_then(|v| v.as_str()) == Some(main_ref))
        .and_then(|d| d.get("dependsOn").and_then(|v| v.as_array()))
        .expect("main-module must have a dependencies entry");
    let main_dep_refs: Vec<&str> = main_deps.iter().filter_map(|v| v.as_str()).collect();
    for direct_purl in &[
        "pkg:cocoapods/AFNetworking@4.0.1",
        "pkg:cocoapods/SDWebImage@5.18.10",
        "pkg:cocoapods/Firebase@10.20.0#Core",
    ] {
        let direct = component_with_purl(&doc, direct_purl)
            .unwrap_or_else(|| panic!("direct dep {direct_purl} not found"));
        let direct_ref = direct.get("bom-ref").and_then(|v| v.as_str()).unwrap();
        assert!(
            main_dep_refs.contains(&direct_ref),
            "main-module dependsOn must include {direct_purl}'s bom-ref; got {main_dep_refs:?}",
        );
    }
}

#[test]
fn sha1_hash_emitted_for_trunk_pods() {
    // SC-007 / FR-008: trunk pod with SPEC CHECKSUMS produces CDX
    // hashes[] entry with alg=SHA-1.
    let tmp = tempfile::tempdir().unwrap();
    write_ios_app_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let af = component_with_purl(&doc, "pkg:cocoapods/AFNetworking@4.0.1").unwrap();
    let hashes = af
        .get("hashes")
        .and_then(|v| v.as_array())
        .expect("AFNetworking must carry hashes[] array (FR-008)");
    let sha1_entries: Vec<&Value> = hashes
        .iter()
        .filter(|h| h.get("alg").and_then(|v| v.as_str()) == Some("SHA-1"))
        .collect();
    assert_eq!(
        sha1_entries.len(),
        1,
        "expected exactly one SHA-1 hash on AFNetworking; got {hashes:#?}"
    );
    let content = sha1_entries[0].get("content").and_then(|v| v.as_str()).unwrap();
    assert_eq!(content.len(), 40, "SHA-1 hex content must be 40 chars");
}

#[test]
fn subspec_emits_distinct_component_with_shared_root_sha1() {
    // SC-009: `Firebase/Core` emits as distinct component with #subpath
    // PURL form. The SHA-1 is looked up by ROOT key `Firebase` per
    // FR-008 root-keyed lookup; the parent root pod doesn't auto-emit
    // (only what's in PODS appears).
    let tmp = tempfile::tempdir().unwrap();
    write_ios_app_fixture(tmp.path());
    let doc = run_scan(tmp.path());

    let firebase_core =
        component_with_purl(&doc, "pkg:cocoapods/Firebase@10.20.0#Core").unwrap();
    // Subspec PURL MUST use #subpath form — the entire point of SC-009.
    assert!(
        firebase_core
            .get("purl")
            .and_then(|v| v.as_str())
            .unwrap()
            .contains('#'),
        "subspec PURL must contain #subpath",
    );

    let core_hash = firebase_core
        .get("hashes")
        .and_then(|v| v.as_array())
        .expect("Firebase/Core must carry SHA-1 from root Firebase SPEC CHECKSUMS")
        .iter()
        .find(|h| h.get("alg").and_then(|v| v.as_str()) == Some("SHA-1"))
        .and_then(|h| h.get("content").and_then(|v| v.as_str()))
        .unwrap();
    // The hash is the Firebase root's checksum (all b's per fixture).
    assert_eq!(core_hash, &"b".repeat(40));
}
