//! Milestone 139 US2 — source-discriminator distinction tests.
//!
//! Covers SC-002 + the Phase 0 subspec subpath correction + Q2
//! CHECKOUT OPTIONS resolved-SHA + case-preservation regression.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
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

fn write_mixed_fixture(root: &Path) {
    std::fs::write(
        root.join("Podfile"),
        "platform :ios, '13.0'\n\
         target 'MyApp' do\n\
         \x20\x20pod 'AFNetworking'\n\
         \x20\x20pod 'Firebase/Core'\n\
         \x20\x20pod 'MyFork', :git => 'https://github.com/foo/my-fork.git', :branch => 'main'\n\
         \x20\x20pod 'LocalLib', :path => '../packages/local-lib'\n\
         end\n",
    )
    .unwrap();
    let sha1 = "a".repeat(40);
    let firebase_sha = "b".repeat(40);
    let resolved = "eb39649a76b87e8451baf75d10ce82ca3a3d5601";
    std::fs::write(
        root.join("Podfile.lock"),
        format!(
            "PODS:\n\
             - AFNetworking (4.0.1)\n\
             - Firebase/Core (10.20.0)\n\
             - Firebase/Auth (10.20.0)\n\
             - MyFork (1.5.0)\n\
             - LocalLib (0.1.0)\n\
\n\
             DEPENDENCIES:\n\
             - AFNetworking\n\
             - Firebase/Core\n\
             - MyFork (from `https://github.com/foo/my-fork.git`, branch `main`)\n\
             - LocalLib (from `../packages/local-lib`)\n\
\n\
             EXTERNAL SOURCES:\n\
             \x20\x20MyFork:\n\
             \x20\x20\x20\x20:git: \"https://github.com/foo/my-fork.git\"\n\
             \x20\x20\x20\x20:branch: \"main\"\n\
             \x20\x20LocalLib:\n\
             \x20\x20\x20\x20:path: \"../packages/local-lib\"\n\
\n\
             CHECKOUT OPTIONS:\n\
             \x20\x20MyFork:\n\
             \x20\x20\x20\x20:commit: \"{resolved}\"\n\
             \x20\x20\x20\x20:git: \"https://github.com/foo/my-fork.git\"\n\
\n\
             SPEC CHECKSUMS:\n\
             \x20\x20AFNetworking: {sha1}\n\
             \x20\x20Firebase: {firebase_sha}\n\
\n\
             PODFILE CHECKSUM: deadbeef\n\
             COCOAPODS: 1.15.2\n",
        ),
    )
    .unwrap();
}

#[test]
fn trunk_default_emits_bare_purl() {
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:cocoapods/AFNetworking@4.0.1")
        .expect("trunk pod must emit as bare PURL");
    assert_eq!(
        property_value(c, "waybill:source-type"),
        Some("cocoapods-trunk"),
    );
}

#[test]
fn subspec_subpath_form() {
    // Phase 0 correction regression: subspec PURL uses #subpath, NOT
    // ?subspec= qualifier.
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:cocoapods/Firebase@10.20.0#Core")
        .expect("subspec must emit with #subpath form per Phase 0 correction");
    assert_eq!(
        property_value(c, "waybill:source-type"),
        Some("cocoapods-trunk"),
    );
    assert_eq!(
        property_value(c, "waybill:subspec"),
        Some("Core"),
        "subspec annotation duplicates the subpath for easier filtering",
    );
}

#[test]
fn multi_level_subspec_preserves_slashes() {
    // Phase 0 correction: multi-level subspecs preserve `/` between
    // subpath segments per purl-spec base rules.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Podfile"),
        "target 'X' do\n  pod 'Firebase/Database/Realtime'\nend\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("Podfile.lock"),
        "PODS:\n- Firebase/Database/Realtime (10.20.0)\n\
         DEPENDENCIES:\n- Firebase/Database/Realtime\n\
         SPEC CHECKSUMS:\n  Firebase: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n\
         COCOAPODS: 1.15.2\n",
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    assert!(
        component_with_purl(&doc, "pkg:cocoapods/Firebase@10.20.0#Database/Realtime").is_some(),
        "multi-level subspec must preserve `/` between subpath segments",
    );
}

#[test]
fn git_source_emits_vcs_url_and_vcs_ref_from_checkout_options() {
    // Q2: PURL includes ?vcs_url= qualifier from EXTERNAL SOURCES;
    // waybill:vcs-ref carries the resolved 40-char SHA from CHECKOUT
    // OPTIONS; waybill:vcs-declared-ref carries the operator-declared
    // branch/tag/commit.
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let expected = "pkg:cocoapods/MyFork@1.5.0?vcs_url=git+https://github.com/foo/my-fork.git";
    let c = component_with_purl(&doc, expected)
        .unwrap_or_else(|| panic!("git-source PURL {expected} not found"));
    assert_eq!(
        property_value(c, "waybill:source-type"),
        Some("cocoapods-git"),
    );
    assert_eq!(
        property_value(c, "waybill:vcs-ref"),
        Some("eb39649a76b87e8451baf75d10ce82ca3a3d5601"),
        "Q2: resolved SHA from CHECKOUT OPTIONS must surface as vcs-ref",
    );
    assert_eq!(
        property_value(c, "waybill:vcs-declared-ref"),
        Some("main"),
        "operator-declared branch from EXTERNAL SOURCES must surface as vcs-declared-ref",
    );
}

#[test]
fn path_source_emits_generic_placeholder() {
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:generic/LocalLib@0.1.0")
        .expect("path-source PURL must use pkg:generic/ placeholder");
    assert_eq!(
        property_value(c, "waybill:source-type"),
        Some("cocoapods-path"),
    );
    assert_eq!(
        property_value(c, "waybill:path"),
        Some("../packages/local-lib"),
    );
}

#[test]
fn pod_name_case_preserved_in_purl() {
    // purl-spec: CocoaPods is case-sensitive (unlike Composer's
    // lowercase requirement). Mixed-case names round-trip exactly.
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    // `AFNetworking` mixed-case in fixture → must appear exactly in PURL.
    let c = component_with_purl(&doc, "pkg:cocoapods/AFNetworking@4.0.1").unwrap();
    assert_eq!(c.get("name").and_then(|v| v.as_str()), Some("AFNetworking"));
}

#[test]
fn subspec_shares_root_pod_sha1_hash() {
    // FR-008 root-keyed lookup: SPEC CHECKSUMS is keyed by root pod
    // (`Firebase`); both subspecs (Core + Auth) share the same SHA-1.
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let core =
        component_with_purl(&doc, "pkg:cocoapods/Firebase@10.20.0#Core").unwrap();
    let auth =
        component_with_purl(&doc, "pkg:cocoapods/Firebase@10.20.0#Auth").unwrap();
    let extract_hash = |c: &Value| -> String {
        c.get("hashes")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .find(|h| h.get("alg").and_then(|v| v.as_str()) == Some("SHA-1"))
            .and_then(|h| h.get("content").and_then(|v| v.as_str()))
            .unwrap()
            .to_string()
    };
    let core_hash = extract_hash(core);
    let auth_hash = extract_hash(auth);
    assert_eq!(
        core_hash, auth_hash,
        "subspecs of same root must share SHA-1 per root-keyed FR-008 lookup",
    );
    assert_eq!(core_hash, "b".repeat(40));
}
