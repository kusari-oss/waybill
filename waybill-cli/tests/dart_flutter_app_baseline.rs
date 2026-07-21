//! Milestone 137 US1 — end-to-end integration tests that a synthetic
//! Flutter app project produces a CDX SBOM containing one main-module
//! component plus one component per lockfile entry, with correct PURL
//! identities and dep edges.
//!
//! Covers spec acceptance scenarios US1.1, US1.2, US1.4 plus SC-001
//! (Flutter app baseline), SC-007 (dev-scope filterability), and SC-008
//! (main-module emission).

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
    // subcommand per main.rs:114; splice it in before the subcommand.
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

fn pub_purls(doc: &Value) -> Vec<String> {
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

fn write_flutter_app_fixture(root: &Path) {
    std::fs::write(
        root.join("pubspec.yaml"),
        "name: my_flutter_app\n\
         version: 1.2.3\n\
         description: Test fixture for milestone 137.\n\
         environment:\n  sdk: '>=3.0.0 <4.0.0'\n\
         dependencies:\n  http: ^1.1.0\n  provider: ^6.1.1\n  shared_preferences: ^2.2.2\n",
    )
    .unwrap();
    // pubspec.lock with the 3 direct deps + 2 transitives = 5 packages.
    let sha = "a".repeat(64);
    std::fs::write(
        root.join("pubspec.lock"),
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
             \x20\x20http_parser:\n\
             \x20\x20\x20\x20dependency: transitive\n\
             \x20\x20\x20\x20description:\n\
             \x20\x20\x20\x20\x20\x20name: http_parser\n\
             \x20\x20\x20\x20\x20\x20sha256: \"{sha}\"\n\
             \x20\x20\x20\x20\x20\x20url: \"https://pub.dev\"\n\
             \x20\x20\x20\x20source: hosted\n\
             \x20\x20\x20\x20version: \"4.0.2\"\n\
             \x20\x20provider:\n\
             \x20\x20\x20\x20dependency: \"direct main\"\n\
             \x20\x20\x20\x20description:\n\
             \x20\x20\x20\x20\x20\x20name: provider\n\
             \x20\x20\x20\x20\x20\x20sha256: \"{sha}\"\n\
             \x20\x20\x20\x20\x20\x20url: \"https://pub.dev\"\n\
             \x20\x20\x20\x20source: hosted\n\
             \x20\x20\x20\x20version: \"6.1.1\"\n\
             \x20\x20shared_preferences:\n\
             \x20\x20\x20\x20dependency: \"direct main\"\n\
             \x20\x20\x20\x20description:\n\
             \x20\x20\x20\x20\x20\x20name: shared_preferences\n\
             \x20\x20\x20\x20\x20\x20sha256: \"{sha}\"\n\
             \x20\x20\x20\x20\x20\x20url: \"https://pub.dev\"\n\
             \x20\x20\x20\x20source: hosted\n\
             \x20\x20\x20\x20version: \"2.2.2\"\n\
             \x20\x20meta:\n\
             \x20\x20\x20\x20dependency: transitive\n\
             \x20\x20\x20\x20description:\n\
             \x20\x20\x20\x20\x20\x20name: meta\n\
             \x20\x20\x20\x20\x20\x20sha256: \"{sha}\"\n\
             \x20\x20\x20\x20\x20\x20url: \"https://pub.dev\"\n\
             \x20\x20\x20\x20source: hosted\n\
             \x20\x20\x20\x20version: \"1.10.0\"\n",
        ),
    )
    .unwrap();
}

#[test]
fn flutter_app_baseline_emits_lockfile_count_plus_main_module() {
    // SC-001: 3 direct + 2 transitive + 1 main-module = 6 dart-derived
    // components in the emitted CDX.
    let tmp = tempfile::tempdir().unwrap();
    write_flutter_app_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let purls = pub_purls(&doc);

    // 5 lockfile entries + 1 main-module = 6.
    let dart_purls: Vec<&String> = purls
        .iter()
        .filter(|p| p.starts_with("pkg:pub/"))
        .collect();
    assert_eq!(
        dart_purls.len(),
        6,
        "expected 6 pkg:pub/* components (5 lockfile entries + 1 main-module), got {dart_purls:#?}"
    );

    // Each pinned dep emits with the expected PURL.
    for expected in &[
        "pkg:pub/http@1.1.0",
        "pkg:pub/http_parser@4.0.2",
        "pkg:pub/provider@6.1.1",
        "pkg:pub/shared_preferences@2.2.2",
        "pkg:pub/meta@1.10.0",
        "pkg:pub/my_flutter_app@1.2.3",
    ] {
        assert!(
            purls.contains(&expected.to_string()),
            "expected PURL {expected} not found; got {purls:#?}"
        );
    }
}

#[test]
fn main_module_emission() {
    // SC-008: main-module component carries the expected PURL +
    // component-role annotation.
    let tmp = tempfile::tempdir().unwrap();
    write_flutter_app_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let main = component_with_purl(&doc, "pkg:pub/my_flutter_app@1.2.3")
        .expect("main-module component must exist");
    assert_eq!(
        property_value(main, "waybill:component-role"),
        Some("main-module"),
        "main-module must carry the component-role annotation; got {main:#?}",
    );
}

#[test]
fn main_module_depends_lists_direct_deps() {
    // US1 acceptance scenario 4 + SC-008: dependencies[] entry for
    // main-module bom-ref targets each direct dep's bom-ref.
    let tmp = tempfile::tempdir().unwrap();
    write_flutter_app_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let main = component_with_purl(&doc, "pkg:pub/my_flutter_app@1.2.3").unwrap();
    let main_ref = main
        .get("bom-ref")
        .and_then(|v| v.as_str())
        .expect("main-module must have bom-ref");

    let deps = doc
        .get("dependencies")
        .and_then(|v| v.as_array())
        .expect("dependencies[] block must exist");
    let main_deps = deps
        .iter()
        .find(|d| d.get("ref").and_then(|v| v.as_str()) == Some(main_ref))
        .and_then(|d| d.get("dependsOn").and_then(|v| v.as_array()))
        .expect("main-module must have a dependencies entry");
    let main_dep_refs: Vec<&str> = main_deps
        .iter()
        .filter_map(|v| v.as_str())
        .collect();

    // Each of the 3 direct deps' bom-refs MUST appear under main-module's dependsOn.
    for direct_purl in &[
        "pkg:pub/http@1.1.0",
        "pkg:pub/provider@6.1.1",
        "pkg:pub/shared_preferences@2.2.2",
    ] {
        let direct = component_with_purl(&doc, direct_purl)
            .unwrap_or_else(|| panic!("direct dep {direct_purl} component missing"));
        let direct_ref = direct.get("bom-ref").and_then(|v| v.as_str()).unwrap();
        assert!(
            main_dep_refs.contains(&direct_ref),
            "main-module dependsOn must include {direct_purl}'s bom-ref {direct_ref}; got {main_dep_refs:?}",
        );
    }
}

#[test]
fn dev_scope_filterability() {
    // SC-007: a fixture with a `direct dev` entry produces a component
    // with `waybill:lifecycle-scope = development`; running with
    // `--exclude-scope dev` suppresses it.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("pubspec.yaml"),
        "name: my_app\nversion: 0.1.0\n\
         environment:\n  sdk: '>=3.0.0 <4.0.0'\n\
         dependencies:\n  http: ^1.1.0\n\
         dev_dependencies:\n  test: ^1.24.0\n",
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
             \x20\x20test:\n\
             \x20\x20\x20\x20dependency: \"direct dev\"\n\
             \x20\x20\x20\x20description:\n\
             \x20\x20\x20\x20\x20\x20name: test\n\
             \x20\x20\x20\x20\x20\x20sha256: \"{sha}\"\n\
             \x20\x20\x20\x20\x20\x20url: \"https://pub.dev\"\n\
             \x20\x20\x20\x20source: hosted\n\
             \x20\x20\x20\x20version: \"1.24.0\"\n",
        ),
    )
    .unwrap();

    // With dev (default — flag varies; we rely on the default behavior
    // of `waybill sbom scan` which includes dev deps unless opted out).
    let doc_with_dev = run_scan(tmp.path());
    let test_component = component_with_purl(&doc_with_dev, "pkg:pub/test@1.24.0");
    assert!(
        test_component.is_some(),
        "default scan must include dev-scope `test` component"
    );
    if let Some(c) = test_component {
        // Property may be set via `lifecycle-scope` or `scope` (CDX native).
        let lifecycle_property = property_value(c, "waybill:lifecycle-scope");
        let cdx_scope = c.get("scope").and_then(|v| v.as_str());
        assert!(
            lifecycle_property == Some("development")
                || matches!(cdx_scope, Some("excluded") | Some("optional")),
            "dev-scope `test` component should carry development indicator; \
             property={lifecycle_property:?} scope={cdx_scope:?}",
        );
    }

    // With --exclude-scope dev: the dev-scope `test` component must not appear.
    let doc_without_dev = run_scan_with_flags(tmp.path(), &["--exclude-scope", "dev"]);
    assert!(
        component_with_purl(&doc_without_dev, "pkg:pub/test@1.24.0").is_none(),
        "--exclude-scope dev must suppress dev-scope `test` component",
    );
    // Runtime dep MUST still appear.
    assert!(
        component_with_purl(&doc_without_dev, "pkg:pub/http@1.1.0").is_some(),
        "--exclude-scope dev must NOT suppress runtime `http` component",
    );
}
