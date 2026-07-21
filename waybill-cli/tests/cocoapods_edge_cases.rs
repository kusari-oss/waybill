//! Milestone 139 polish — edge case coverage per spec Edge Cases + I1/I2
//! remediations + Q2 partial cases.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
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
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    let doc: Value = serde_json::from_slice(&bytes).unwrap();
    (doc, String::from_utf8_lossy(&result.stderr).into_owned())
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
                                    == Some("mikebom:source-type")
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

#[test]
fn malformed_podfile_lock_falls_back_to_design_tier() {
    // SC-005: malformed Podfile.lock + sibling Podfile → design-tier
    // fallback + warning fires.
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("BadProj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(
        proj.join("Podfile"),
        "target 'BadApp' do\n  pod 'AFNetworking', '~> 4.0'\nend\n",
    )
    .unwrap();
    std::fs::write(proj.join("Podfile.lock"), "this is not: : valid yaml\n[broken").unwrap();
    let (doc, stderr) = run_scan(tmp.path());
    // Design-tier emission falls through.
    let af = component_with_purl(&doc, "pkg:cocoapods/AFNetworking@~> 4.0");
    assert!(
        af.is_some() || cocoapods_purls(&doc).iter().any(|p| p.contains("AFNetworking")),
        "design-tier AFNetworking must emit when lockfile is malformed; got {:#?}",
        cocoapods_purls(&doc),
    );
    assert!(
        stderr.contains("failed to parse Podfile.lock"),
        "expected warning in stderr; got: {stderr}",
    );
}

#[test]
fn multi_target_podfile_emits_first_target_as_main_module() {
    // FR-010: multi-target Podfiles emit each pod once; first target
    // wins for main-module derivation.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Podfile"),
        "target 'AppMain' do\n  pod 'AFNetworking'\nend\n\
         target 'AppTests' do\n  pod 'SDWebImage'\nend\n",
    )
    .unwrap();
    let sha1 = "a".repeat(40);
    std::fs::write(
        tmp.path().join("Podfile.lock"),
        format!(
            "PODS:\n- AFNetworking (4.0.1)\n- SDWebImage (5.18.10)\n\
             DEPENDENCIES:\n- AFNetworking\n- SDWebImage\n\
             SPEC CHECKSUMS:\n  AFNetworking: {sha1}\n  SDWebImage: {sha1}\n\
             COCOAPODS: 1.15.2\n"
        ),
    )
    .unwrap();
    let (doc, _) = run_scan(tmp.path());
    assert!(
        component_with_purl(&doc, "pkg:cocoapods/AppMain").is_some(),
        "first-target wins for main-module name",
    );
    assert!(
        component_with_purl(&doc, "pkg:cocoapods/AppTests").is_none(),
        "second target should NOT emit as separate main-module",
    );
}

#[test]
fn git_source_missing_checkout_options_emits_without_vcs_ref() {
    // Q2 partial case: EXTERNAL SOURCES present, CHECKOUT OPTIONS
    // absent. PURL still emits with ?vcs_url= qualifier; mikebom:vcs-ref
    // absent; mikebom:vcs-declared-ref still present.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Podfile"),
        "target 'X' do\n  pod 'MyFork', :git => 'https://example.com/r.git', :tag => 'v1.0'\nend\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("Podfile.lock"),
        "PODS:\n- MyFork (1.0.0)\n\
         DEPENDENCIES:\n- MyFork (from `https://example.com/r.git`, tag `v1.0`)\n\
         EXTERNAL SOURCES:\n  MyFork:\n    :git: \"https://example.com/r.git\"\n    :tag: \"v1.0\"\n\
         SPEC CHECKSUMS: {}\n\
         COCOAPODS: 1.15.2\n",
    )
    .unwrap();
    let (doc, _) = run_scan(tmp.path());
    let expected = "pkg:cocoapods/MyFork@1.0.0?vcs_url=git+https://example.com/r.git";
    let c = component_with_purl(&doc, expected).unwrap_or_else(|| {
        panic!("VCS PURL must emit even without CHECKOUT OPTIONS; got {:?}", cocoapods_purls(&doc))
    });
    assert!(
        property_value(c, "mikebom:vcs-ref").is_none(),
        "mikebom:vcs-ref must be absent when CHECKOUT OPTIONS missing",
    );
    assert_eq!(
        property_value(c, "mikebom:vcs-declared-ref"),
        Some("v1.0"),
        "mikebom:vcs-declared-ref must still surface from EXTERNAL SOURCES",
    );
}

#[test]
fn pre_1_0_lockfile_emits_components_with_empty_hashes() {
    // I1 remediation: lockfile lacking SPEC CHECKSUMS section (pre-1.0
    // sentinel) emits components without hashes; info-level log fires;
    // scan does NOT fail.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Podfile.lock"),
        "PODS:\n- AFNetworking (4.0.1)\n\
         DEPENDENCIES:\n- AFNetworking\n\
         COCOAPODS: 0.39.0\n",
    )
    .unwrap();
    let (doc, _) = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:cocoapods/AFNetworking@4.0.1")
        .expect("pre-1.0 lockfile must still emit components per Principle VIII");
    // Hashes empty (no SPEC CHECKSUMS).
    let hashes = c.get("hashes").and_then(|v| v.as_array());
    assert!(
        hashes.is_none() || hashes.unwrap().is_empty(),
        "pre-1.0 component must NOT carry SHA-1 hash; got {:?}",
        hashes,
    );
}

#[test]
fn empty_pods_block_emits_only_main_module() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Podfile"),
        "target 'EmptyApp' do\nend\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("Podfile.lock"),
        "PODS: []\nDEPENDENCIES: []\nSPEC CHECKSUMS: {}\nCOCOAPODS: 1.15.2\n",
    )
    .unwrap();
    let (doc, stderr) = run_scan(tmp.path());
    let purls = cocoapods_purls(&doc);
    assert_eq!(purls.len(), 1, "only main-module emits; got {purls:?}");
    assert_eq!(purls[0], "pkg:cocoapods/EmptyApp");
    assert!(
        !stderr.contains("cocoapods: failed"),
        "no warnings expected; got: {stderr}",
    );
}

#[test]
fn subspec_purl_subpath_does_not_include_subspec_qualifier() {
    // Phase 0 correction paranoia check: re-introduction of the
    // initial spec-guess `?subspec=` form is a regression.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Podfile"),
        "target 'X' do\n  pod 'Firebase/Core'\nend\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("Podfile.lock"),
        "PODS:\n- Firebase/Core (10.20.0)\n\
         DEPENDENCIES:\n- Firebase/Core\n\
         SPEC CHECKSUMS:\n  Firebase: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n\
         COCOAPODS: 1.15.2\n",
    )
    .unwrap();
    let (doc, _) = run_scan(tmp.path());
    for purl in cocoapods_purls(&doc) {
        assert!(
            !purl.contains("?subspec="),
            "subspec PURL must use #subpath form, not ?subspec= qualifier; got: {purl}",
        );
    }
    assert!(
        component_with_purl(&doc, "pkg:cocoapods/Firebase@10.20.0#Core").is_some(),
        "subspec must emit with #Core subpath form",
    );
}

#[test]
fn manifest_lock_multi_layer_dedupes_via_seen_purls() {
    // R4 multi-layer container support: two Pods/Manifest.lock files
    // at different paths containing the same pod → one component per
    // PURL via orchestrator dedup.
    let tmp = tempfile::tempdir().unwrap();
    let sha1 = "a".repeat(40);
    let body = format!(
        "PODS:\n- AFNetworking (4.0.1)\n\
         DEPENDENCIES:\n- AFNetworking\n\
         SPEC CHECKSUMS:\n  AFNetworking: {sha1}\n\
         COCOAPODS: 1.15.2\n"
    );
    for layer in &["layer1", "layer2"] {
        let pods = tmp.path().join(layer).join("Pods");
        std::fs::create_dir_all(&pods).unwrap();
        std::fs::write(pods.join("Manifest.lock"), &body).unwrap();
    }
    let (doc, _) = run_scan(tmp.path());
    let af_count = cocoapods_purls(&doc)
        .iter()
        .filter(|p| *p == "pkg:cocoapods/AFNetworking@4.0.1")
        .count();
    assert_eq!(
        af_count, 1,
        "multi-layer Manifest.lock must dedupe to ONE AFNetworking component",
    );
}

#[test]
fn path_sourced_subspec_flattens_slash_to_hyphen() {
    // I2 remediation regression: path-sourced subspec PURL MUST flatten
    // `/` to `-` (matches milestone-138 composer convention) to avoid
    // the pkg:generic/<namespace>/<name> ambiguity per purl-spec.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Podfile"),
        "target 'X' do\n  pod 'Firebase/Core', :path => '../firebase-core'\nend\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("Podfile.lock"),
        "PODS:\n- Firebase/Core (10.20.0)\n\
         DEPENDENCIES:\n- Firebase/Core (from `../firebase-core`)\n\
         EXTERNAL SOURCES:\n  Firebase/Core:\n    :path: \"../firebase-core\"\n\
         SPEC CHECKSUMS: {}\n\
         COCOAPODS: 1.15.2\n",
    )
    .unwrap();
    let (doc, _) = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:generic/Firebase-Core@10.20.0")
        .unwrap_or_else(|| {
            panic!(
                "path-sourced subspec PURL must flatten `/` to `-`; got {:?}",
                cocoapods_purls(&doc)
            )
        });
    assert_eq!(
        property_value(c, "mikebom:source-type"),
        Some("cocoapods-path"),
    );
    assert_eq!(
        property_value(c, "mikebom:path"),
        Some("../firebase-core"),
    );
    assert_eq!(
        property_value(c, "mikebom:subspec"),
        Some("Core"),
        "original subspec form preserved as annotation for recovery per I2",
    );
}
