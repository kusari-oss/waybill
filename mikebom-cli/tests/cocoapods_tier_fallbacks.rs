//! Milestone 139 US3 — design + deployed tier + Q1 dir-basename fallback.

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

fn cocoapods_components(doc: &Value) -> Vec<&Value> {
    doc.get("components")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    let purl = c.get("purl").and_then(|v| v.as_str()).unwrap_or("");
                    if purl.starts_with("pkg:cocoapods/") {
                        return property_value(c, "mikebom:component-role")
                            != Some("main-module");
                    }
                    if purl.starts_with("pkg:generic/") {
                        return property_value(c, "mikebom:source-type")
                            .map(|s| s.starts_with("cocoapods-"))
                            .unwrap_or(false);
                    }
                    false
                })
                .collect()
        })
        .unwrap_or_default()
}

fn property_value<'a>(component: &'a Value, name: &str) -> Option<&'a str> {
    component
        .get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
}

fn find_by_name<'a>(components: &'a [&'a Value], name: &str) -> Option<&'a Value> {
    components
        .iter()
        .copied()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
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

#[test]
fn design_tier_podfile_only_emits_constraints() {
    // SC-003: Podfile-only project emits design-tier with constraint
    // preserved as requirement-range.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Podfile"),
        "target 'MyLib' do\n\
         \x20\x20pod 'AFNetworking', '~> 4.0'\n\
         \x20\x20pod 'SDWebImage', '~> 5.18'\n\
         end\n",
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    let comps = cocoapods_components(&doc);
    assert_eq!(comps.len(), 2);
    for &c in &comps {
        assert_eq!(
            property_value(c, "mikebom:sbom-tier"),
            Some("design"),
        );
    }
    let af = find_by_name(&comps, "AFNetworking").unwrap();
    assert_eq!(
        property_value(af, "mikebom:requirement-range"),
        Some("~> 4.0"),
    );
}

#[test]
fn design_tier_no_constraint_uses_unspecified_placeholder() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Podfile"),
        "target 'MyLib' do\n  pod 'AFNetworking'\nend\n",
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    assert!(
        component_with_purl(&doc, "pkg:cocoapods/AFNetworking@unspecified").is_some(),
        "no-constraint pod must emit with @unspecified placeholder",
    );
}

#[test]
fn design_tier_no_transitive_deps() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Podfile"),
        "target 'MyLib' do\n  pod 'Firebase/Core'\nend\n",
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    let comps = cocoapods_components(&doc);
    let names: Vec<&str> = comps
        .iter()
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(names.contains(&"Firebase/Core"));
    assert!(
        !names.contains(&"FirebaseCore"),
        "design-tier must NOT emit transitive FirebaseCore",
    );
}

#[test]
fn deployed_tier_manifest_lock_only_emits_with_sbom_tier_deployed() {
    // Q3 + R4: Manifest.lock-only scan (no sibling Podfile.lock) emits
    // components with sbom-tier=deployed + evidence-kind=manifest-lock.
    let tmp = tempfile::tempdir().unwrap();
    let pods_dir = tmp.path().join("Pods");
    std::fs::create_dir_all(&pods_dir).unwrap();
    let sha1 = "a".repeat(40);
    std::fs::write(
        pods_dir.join("Manifest.lock"),
        format!(
            "PODS:\n- AFNetworking (4.0.1)\n- SDWebImage (5.18.10)\n\
             DEPENDENCIES:\n- AFNetworking\n- SDWebImage\n\
             SPEC CHECKSUMS:\n  AFNetworking: {sha1}\n  SDWebImage: {sha1}\n\
             COCOAPODS: 1.15.2\n"
        ),
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    let comps = cocoapods_components(&doc);
    assert_eq!(comps.len(), 2);
    for &c in &comps {
        assert_eq!(
            property_value(c, "mikebom:sbom-tier"),
            Some("deployed"),
        );
        assert_eq!(
            property_value(c, "mikebom:evidence-kind"),
            Some("cocoapods-manifest-lock"),
        );
    }
}

#[test]
fn manifest_lock_skipped_when_podfile_lock_present() {
    // FR-011 dedup: with both Podfile.lock AND Pods/Manifest.lock
    // present, each PURL appears EXACTLY ONCE (Manifest.lock skipped).
    let tmp = tempfile::tempdir().unwrap();
    let sha1 = "a".repeat(40);
    let body = format!(
        "PODS:\n- AFNetworking (4.0.1)\n\
         DEPENDENCIES:\n- AFNetworking\n\
         SPEC CHECKSUMS:\n  AFNetworking: {sha1}\n\
         COCOAPODS: 1.15.2\n"
    );
    std::fs::write(tmp.path().join("Podfile.lock"), &body).unwrap();
    std::fs::write(
        tmp.path().join("Podfile"),
        "target 'X' do\n  pod 'AFNetworking'\nend\n",
    )
    .unwrap();
    let pods_dir = tmp.path().join("Pods");
    std::fs::create_dir_all(&pods_dir).unwrap();
    std::fs::write(pods_dir.join("Manifest.lock"), &body).unwrap();

    let doc = run_scan(tmp.path());
    let comps = cocoapods_components(&doc);
    let af_count = comps
        .iter()
        .filter(|c| c.get("name").and_then(|v| v.as_str()) == Some("AFNetworking"))
        .count();
    assert_eq!(
        af_count, 1,
        "AFNetworking must emit EXACTLY ONCE (Manifest.lock skipped per FR-011)",
    );
    // The one that emits is source-tier (Podfile.lock wins).
    let af = comps
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some("AFNetworking"))
        .unwrap();
    assert_eq!(
        property_value(af, "mikebom:sbom-tier"),
        Some("source"),
    );
}

#[test]
fn lockfile_only_main_module_from_dir_basename() {
    // Q1: lockfile-only commit (no Podfile) → main-module from
    // parent-dir basename.
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("MyContainerApp");
    std::fs::create_dir_all(&project).unwrap();
    let sha1 = "a".repeat(40);
    std::fs::write(
        project.join("Podfile.lock"),
        format!(
            "PODS:\n- AFNetworking (4.0.1)\n\
             DEPENDENCIES:\n- AFNetworking\n\
             SPEC CHECKSUMS:\n  AFNetworking: {sha1}\n\
             COCOAPODS: 1.15.2\n"
        ),
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    assert!(
        component_with_purl(&doc, "pkg:cocoapods/MyContainerApp@0.0.0-unknown").is_some(),
        "main-module must use dir-basename fallback per Q1",
    );
}
