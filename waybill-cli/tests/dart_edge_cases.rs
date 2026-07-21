//! Milestone 137 polish — edge-case coverage per spec Edge Cases +
//! SC-005 + the v1 scope clarifications.

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
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    (doc, stderr)
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

fn dart_purls(doc: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut visit = |c: &Value| {
        if let Some(p) = c.get("purl").and_then(|v| v.as_str()) {
            if p.starts_with("pkg:pub/") || p.starts_with("pkg:generic/") {
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

fn write_minimal_pubspec(dir: &Path, name: &str, version: Option<&str>) {
    let body = match version {
        Some(v) => format!("name: {name}\nversion: {v}\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\n"),
        None => format!("name: {name}\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\n"),
    };
    std::fs::write(dir.join("pubspec.yaml"), body).unwrap();
}

#[test]
fn malformed_lockfile_falls_back_to_design_tier() {
    // SC-005: 1 valid + 1 corrupted lockfile across 2 sibling
    // project dirs. The valid one emits per FR-002 (source-tier
    // lockfile-driven). The corrupted one falls back to design-tier
    // from pubspec.yaml. Scan exits 0 (no abort); a warning fires
    // for the corrupted file.
    let tmp = tempfile::tempdir().unwrap();
    let valid = tmp.path().join("valid");
    let bad = tmp.path().join("bad");
    std::fs::create_dir_all(&valid).unwrap();
    std::fs::create_dir_all(&bad).unwrap();

    write_minimal_pubspec(&valid, "valid_app", Some("1.0.0"));
    let sha = "a".repeat(64);
    std::fs::write(
        valid.join("pubspec.lock"),
        format!(
            "packages:\n\
             \x20\x20http:\n\
             \x20\x20\x20\x20dependency: \"direct main\"\n\
             \x20\x20\x20\x20description:\n\
             \x20\x20\x20\x20\x20\x20name: http\n\
             \x20\x20\x20\x20\x20\x20sha256: \"{sha}\"\n\
             \x20\x20\x20\x20\x20\x20url: \"https://pub.dev\"\n\
             \x20\x20\x20\x20source: hosted\n\
             \x20\x20\x20\x20version: \"1.1.0\"\n",
        ),
    )
    .unwrap();

    std::fs::write(
        bad.join("pubspec.yaml"),
        "name: bad_app\nversion: 0.2.0\n\
         environment:\n  sdk: '>=3.0.0 <4.0.0'\n\
         dependencies:\n  provider: ^6.1.0\n",
    )
    .unwrap();
    std::fs::write(
        bad.join("pubspec.lock"),
        "this is not: : valid yaml\n\t\t\n[broken",
    )
    .unwrap();

    let (doc, stderr) = run_scan(tmp.path());

    // Valid project's lockfile emission MUST appear.
    assert!(
        component_with_purl(&doc, "pkg:pub/http@1.1.0").is_some(),
        "valid project's lockfile-derived http MUST emit",
    );
    assert!(
        component_with_purl(&doc, "pkg:pub/valid_app@1.0.0").is_some(),
        "valid project's main-module MUST emit",
    );

    // Bad project's main-module + design-tier provider MUST emit
    // (graceful degradation per FR-007 + R7).
    assert!(
        component_with_purl(&doc, "pkg:pub/bad_app@0.2.0").is_some(),
        "bad project's main-module MUST emit even with malformed lockfile",
    );
    let provider = component_with_purl(&doc, "pkg:pub/provider@^6.1.0")
        .expect("bad project's design-tier provider MUST emit from pubspec.yaml fallback");
    assert_eq!(
        property_value(provider, "waybill:sbom-tier"),
        Some("design"),
    );

    // Warn for the corrupted lockfile must fire to stderr.
    assert!(
        stderr.contains("failed to parse pubspec.lock"),
        "expected warning for malformed lockfile in stderr; got: {stderr}",
    );
}

#[test]
fn workspace_monorepo_emits_one_main_module_per_pubspec() {
    // FR-009 (Melos shape): a monorepo with multiple member packages,
    // each with its own pubspec.yaml + pubspec.lock. One main-module
    // per member; NO synthetic workspace-root component.
    let tmp = tempfile::tempdir().unwrap();
    for member in &["app", "lib_a", "lib_b"] {
        let dir = tmp.path().join("packages").join(member);
        std::fs::create_dir_all(&dir).unwrap();
        let version = match *member {
            "app" => "1.0.0",
            "lib_a" => "0.5.0",
            _ => "0.3.0",
        };
        write_minimal_pubspec(&dir, member, Some(version));
    }

    let (doc, _) = run_scan(tmp.path());
    let purls = dart_purls(&doc);
    for expected in &[
        "pkg:pub/app@1.0.0",
        "pkg:pub/lib_a@0.5.0",
        "pkg:pub/lib_b@0.3.0",
    ] {
        assert!(
            purls.contains(&expected.to_string()),
            "expected workspace member {expected} not found; got {purls:?}",
        );
    }
    // No synthetic workspace-root component (the tmpdir itself has no pubspec.yaml).
    assert!(
        !purls.iter().any(|p| p.contains("workspace-root")),
        "no synthetic workspace-root component must emit; got {purls:?}",
    );
}

#[test]
fn missing_version_falls_back_to_unknown_placeholder() {
    // FR-012: pubspec.yaml without `version:` field emits main-module
    // with PURL `pkg:pub/<name>@0.0.0-unknown`.
    let tmp = tempfile::tempdir().unwrap();
    write_minimal_pubspec(tmp.path(), "library_in_development", None);

    let (doc, _) = run_scan(tmp.path());
    assert!(
        component_with_purl(&doc, "pkg:pub/library_in_development").is_some(),
        "missing-version main-module MUST emit with 0.0.0-unknown placeholder",
    );
}

#[test]
fn sdk_pseudo_deps_emit_zero_zero_zero() {
    // FR-011: lockfile with `flutter` SDK pseudo-dep emits
    // `pkg:pub/flutter@0.0.0` with `waybill:source-type = pub-sdk`.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("pubspec.yaml"),
        "name: my_app\nversion: 0.1.0\n\
         environment:\n  sdk: '>=3.0.0 <4.0.0'\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("pubspec.lock"),
        "packages:\n  \
         flutter:\n    \
         dependency: \"direct main\"\n    \
         description: flutter\n    \
         source: sdk\n    \
         version: \"0.0.0\"\n",
    )
    .unwrap();

    let (doc, _) = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:pub/flutter@0.0.0")
        .expect("SDK pseudo-dep must emit at @0.0.0");
    assert_eq!(property_value(c, "waybill:source-type"), Some("pub-sdk"));
}

#[test]
fn direct_overridden_treated_as_runtime() {
    // R2 lifecycle mapping: lockfile entry with `dependency: "direct
    // overridden"` emits as lifecycle-scope: Runtime (no development
    // indicator).
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("pubspec.yaml"),
        "name: my_app\nversion: 0.1.0\n\
         environment:\n  sdk: '>=3.0.0 <4.0.0'\n",
    )
    .unwrap();
    let sha = "a".repeat(64);
    std::fs::write(
        tmp.path().join("pubspec.lock"),
        format!(
            "packages:\n\
             \x20\x20http:\n\
             \x20\x20\x20\x20dependency: \"direct overridden\"\n\
             \x20\x20\x20\x20description:\n\
             \x20\x20\x20\x20\x20\x20name: http\n\
             \x20\x20\x20\x20\x20\x20sha256: \"{sha}\"\n\
             \x20\x20\x20\x20\x20\x20url: \"https://pub.dev\"\n\
             \x20\x20\x20\x20source: hosted\n\
             \x20\x20\x20\x20version: \"1.1.0\"\n",
        ),
    )
    .unwrap();

    let (doc, _) = run_scan(tmp.path());
    let http = component_with_purl(&doc, "pkg:pub/http@1.1.0")
        .expect("direct-overridden entry must still emit as a component");
    // Must NOT carry the development lifecycle indicator.
    let lifecycle = property_value(http, "waybill:lifecycle-scope");
    let cdx_scope = http.get("scope").and_then(|v| v.as_str());
    assert!(
        lifecycle != Some("development") && cdx_scope != Some("excluded"),
        "direct-overridden must NOT be tagged as development; \
         got property={lifecycle:?} scope={cdx_scope:?}",
    );
}

#[test]
fn empty_packages_block_emits_only_main_module() {
    // Edge Cases: pubspec.lock with `packages: {}` emits just the
    // main-module; no dep components; no warnings.
    let tmp = tempfile::tempdir().unwrap();
    write_minimal_pubspec(tmp.path(), "empty_app", Some("0.1.0"));
    std::fs::write(
        tmp.path().join("pubspec.lock"),
        "packages: {}\nsdks:\n  dart: '>=3.0.0 <4.0.0'\n",
    )
    .unwrap();

    let (doc, stderr) = run_scan(tmp.path());
    let purls = dart_purls(&doc);
    assert_eq!(
        purls.len(),
        1,
        "only main-module must emit on empty packages block; got {purls:?}",
    );
    assert_eq!(purls[0], "pkg:pub/empty_app@0.1.0");
    // No dart-specific warnings.
    assert!(
        !stderr.contains("dart: failed"),
        "no dart warnings expected on empty packages block; got: {stderr}",
    );
}

#[test]
fn git_source_missing_resolved_ref_warns_and_skips() {
    // Edge Cases: lockfile entry for a git source lacking
    // `resolved-ref:` warns and skips that single entry; other valid
    // lockfile entries still emit.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("pubspec.yaml"),
        "name: my_app\nversion: 0.1.0\n\
         environment:\n  sdk: '>=3.0.0 <4.0.0'\n",
    )
    .unwrap();
    let sha = "a".repeat(64);
    std::fs::write(
        tmp.path().join("pubspec.lock"),
        format!(
            "packages:\n\
             \x20\x20http:\n\
             \x20\x20\x20\x20dependency: \"direct main\"\n\
             \x20\x20\x20\x20description:\n\
             \x20\x20\x20\x20\x20\x20name: http\n\
             \x20\x20\x20\x20\x20\x20sha256: \"{sha}\"\n\
             \x20\x20\x20\x20\x20\x20url: \"https://pub.dev\"\n\
             \x20\x20\x20\x20source: hosted\n\
             \x20\x20\x20\x20version: \"1.1.0\"\n\
             \x20\x20broken_git:\n\
             \x20\x20\x20\x20dependency: \"direct main\"\n\
             \x20\x20\x20\x20description:\n\
             \x20\x20\x20\x20\x20\x20url: \"https://example.com/r.git\"\n\
             \x20\x20\x20\x20\x20\x20ref: \"main\"\n\
             \x20\x20\x20\x20source: git\n\
             \x20\x20\x20\x20version: \"0.0.0\"\n",
        ),
    )
    .unwrap();

    let (doc, stderr) = run_scan(tmp.path());
    assert!(
        component_with_purl(&doc, "pkg:pub/http@1.1.0").is_some(),
        "valid hosted entry must still emit when sibling git entry is malformed",
    );
    let purls = dart_purls(&doc);
    assert!(
        !purls.iter().any(|p| p.contains("broken_git")),
        "malformed git entry must NOT emit; got {purls:?}",
    );
    assert!(
        stderr.contains("skipping malformed lockfile entry")
            || stderr.contains("git source missing resolved-ref"),
        "expected skip warning in stderr; got: {stderr}",
    );
}
