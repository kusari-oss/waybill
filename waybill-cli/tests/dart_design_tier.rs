//! Milestone 137 US3 — design-tier emission tests (no lockfile).
//!
//! Covers SC-003 + US3 acceptance scenarios 1/2/3: a Dart library
//! project with `pubspec.yaml` only (no `pubspec.lock`) emits
//! components for declared `dependencies:` + `dev_dependencies:` with
//! `waybill:sbom-tier = "design"` annotation + constraint preserved
//! as `waybill:requirement-range` evidence.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn run_scan(project_root: &Path) -> Value {
    run_scan_with_flags(project_root, &[])
}

fn run_scan_with_flags(project_root: &Path, extra: &[&str]) -> Value {
    // Top-level `--exclude-scope` MUST be passed BEFORE the `sbom`
    // subcommand per main.rs:114; we splice it in before the
    // subcommand args.
    let mut top_level_extra: Vec<&str> = Vec::new();
    let mut subcommand_extra: Vec<&str> = Vec::new();
    let mut iter = extra.iter().peekable();
    while let Some(a) = iter.next() {
        if *a == "--exclude-scope" {
            top_level_extra.push(a);
            if let Some(v) = iter.next() {
                top_level_extra.push(v);
            }
        } else {
            subcommand_extra.push(a);
        }
    }
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(binary_path());
    cmd.arg("--offline");
    for a in &top_level_extra {
        cmd.arg(a);
    }
    cmd.arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()));
    for a in &subcommand_extra {
        cmd.arg(a);
    }
    let result = cmd.output().unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn dart_components(doc: &Value) -> Vec<&Value> {
    doc.get("components")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    let purl = c.get("purl").and_then(|v| v.as_str()).unwrap_or("");
                    (purl.starts_with("pkg:pub/") || purl.starts_with("pkg:generic/"))
                        && property_value(c, "waybill:component-role") != Some("main-module")
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

#[test]
fn design_tier_no_lockfile_emits_constraints() {
    // SC-003: pubspec.yaml-only project emits components with
    // sbom-tier=design + the constraint preserved as requirement-range.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("pubspec.yaml"),
        "name: my_lib\nversion: 0.1.0\n\
         environment:\n  sdk: '>=3.0.0 <4.0.0'\n\
         dependencies:\n  http: ^1.0.0\n  provider: ^6.1.0\n",
    )
    .unwrap();

    let doc = run_scan(tmp.path());
    let dart = dart_components(&doc);
    assert_eq!(
        dart.len(),
        2,
        "design-tier scan must emit 2 deps; got {dart:#?}",
    );
    for &c in &dart {
        assert_eq!(
            property_value(c, "waybill:sbom-tier"),
            Some("design"),
            "design-tier component {} missing sbom-tier=design",
            c.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
        );
    }
    let http = find_by_name(&dart, "http").expect("http component must exist");
    // Milestone 199: always-array shape — JSON-array-in-string value.
    assert_eq!(
        property_value(http, "waybill:requirement-ranges"),
        Some(r#"["^1.0.0"]"#),
        "http design-tier component must preserve constraint string verbatim (m199 plural)",
    );
    let provider = find_by_name(&dart, "provider").expect("provider component must exist");
    assert_eq!(
        property_value(provider, "waybill:requirement-ranges"),
        Some(r#"["^6.1.0"]"#),
    );
}

#[test]
fn design_tier_no_transitive_deps() {
    // US3 acceptance scenario 2: design-tier emits ONLY declared
    // direct deps; no transitives (no lockfile to resolve them).
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("pubspec.yaml"),
        "name: my_lib\nversion: 0.1.0\n\
         environment:\n  sdk: '>=3.0.0 <4.0.0'\n\
         dependencies:\n  http: ^1.0.0\n",
    )
    .unwrap();

    let doc = run_scan(tmp.path());
    let dart = dart_components(&doc);
    let names: Vec<&str> = dart
        .iter()
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(
        names.contains(&"http"),
        "declared direct dep http must emit; got {names:?}",
    );
    // No `http_parser` / `meta` / etc. — those would require lockfile resolution.
    assert!(
        !names.contains(&"http_parser"),
        "design-tier must NOT emit transitive http_parser; got {names:?}",
    );
    assert!(
        !names.contains(&"meta"),
        "design-tier must NOT emit transitive meta; got {names:?}",
    );
}

#[test]
fn design_tier_dev_deps_carry_lifecycle_scope() {
    // US3 acceptance scenario 3: dev_dependencies entries emit with
    // waybill:lifecycle-scope=development; --exclude-scope suppresses.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("pubspec.yaml"),
        "name: my_lib\nversion: 0.1.0\n\
         environment:\n  sdk: '>=3.0.0 <4.0.0'\n\
         dependencies:\n  http: ^1.0.0\n\
         dev_dependencies:\n  test: ^1.24.0\n",
    )
    .unwrap();

    let doc_with_dev = run_scan(tmp.path());
    let dart = dart_components(&doc_with_dev);
    let test_c = find_by_name(&dart, "test").expect("dev dep `test` must emit");
    let lifecycle = property_value(test_c, "waybill:lifecycle-scope");
    let cdx_scope = test_c.get("scope").and_then(|v| v.as_str());
    assert!(
        lifecycle == Some("development")
            || matches!(cdx_scope, Some("excluded") | Some("optional")),
        "dev dep `test` must carry development indicator; \
         property={lifecycle:?} scope={cdx_scope:?}",
    );

    let doc_without_dev = run_scan_with_flags(tmp.path(), &["--exclude-scope", "dev"]);
    let dart_prod = dart_components(&doc_without_dev);
    let names_prod: Vec<&str> = dart_prod
        .iter()
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(
        !names_prod.contains(&"test"),
        "--exclude-scope must suppress dev dep `test`; got {names_prod:?}",
    );
    assert!(
        names_prod.contains(&"http"),
        "--exclude-scope must NOT suppress runtime `http`; got {names_prod:?}",
    );
}
