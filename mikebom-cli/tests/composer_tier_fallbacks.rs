//! Milestone 138 US3 — design-tier + deployed-tier + lockfile-orphan
//! detection integration tests.
//!
//! Covers SC-003 (design-tier emission), SC-009 (deployed-tier
//! installed.json emission), and SC-010 (lockfile-orphan drift
//! detection) plus the C1 remediation (deployed-tier-only scans
//! must NOT carry the orphan annotation).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn run_scan(project_root: &Path) -> Value {
    run_scan_with_flags(project_root, &[])
}

fn run_scan_with_flags(project_root: &Path, extra: &[&str]) -> Value {
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

fn composer_components(doc: &Value) -> Vec<&Value> {
    doc.get("components")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    let purl = c.get("purl").and_then(|v| v.as_str()).unwrap_or("");
                    (purl.starts_with("pkg:composer/") || purl.starts_with("pkg:generic/"))
                        && property_value(c, "mikebom:component-role") != Some("main-module")
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
fn design_tier_no_lockfile_emits_constraints() {
    // SC-003: composer.json-only project emits components with
    // sbom-tier=design + the constraint preserved as requirement-range.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{
  "name": "acme/my-lib",
  "version": "0.1.0",
  "require": {
    "symfony/console": "^7.0",
    "monolog/monolog": "^3.5"
  }
}
"#,
    )
    .unwrap();

    let doc = run_scan(tmp.path());
    let dart = composer_components(&doc);
    assert_eq!(
        dart.len(),
        2,
        "design-tier scan must emit 2 deps; got {dart:#?}",
    );
    for &c in &dart {
        assert_eq!(
            property_value(c, "mikebom:sbom-tier"),
            Some("design"),
        );
    }
    let console = find_by_name(&dart, "symfony/console").unwrap();
    // Milestone 199: always-array shape — JSON-array-in-string value.
    assert_eq!(
        property_value(console, "mikebom:requirement-ranges"),
        Some(r#"["^7.0"]"#),
    );
    let monolog = find_by_name(&dart, "monolog/monolog").unwrap();
    assert_eq!(
        property_value(monolog, "mikebom:requirement-ranges"),
        Some(r#"["^3.5"]"#),
    );
}

#[test]
fn design_tier_no_transitive_deps() {
    // US3 acceptance scenario 2: design-tier emits ONLY declared
    // direct deps; no transitives.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"name":"acme/lib","version":"0.1.0","require":{"symfony/console":"^7.0"}}"#,
    )
    .unwrap();

    let doc = run_scan(tmp.path());
    let dart = composer_components(&doc);
    let names: Vec<&str> = dart
        .iter()
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(names.contains(&"symfony/console"), "declared direct dep must emit");
    assert!(
        !names.contains(&"psr/log"),
        "design-tier must NOT emit transitive psr/log; got {names:?}",
    );
}

#[test]
fn design_tier_dev_deps_carry_lifecycle_scope() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{
  "name": "acme/my-lib",
  "version": "0.1.0",
  "require": {"symfony/console":"^7.0"},
  "require-dev": {"phpunit/phpunit":"^11.0"}
}
"#,
    )
    .unwrap();

    let doc_with_dev = run_scan(tmp.path());
    let dart = composer_components(&doc_with_dev);
    let phpunit = find_by_name(&dart, "phpunit/phpunit").expect("dev dep phpunit must emit");
    let lifecycle = property_value(phpunit, "mikebom:lifecycle-scope");
    let cdx_scope = phpunit.get("scope").and_then(|v| v.as_str());
    assert!(
        lifecycle == Some("development")
            || matches!(cdx_scope, Some("excluded") | Some("optional")),
        "phpunit must carry development indicator; property={lifecycle:?} scope={cdx_scope:?}",
    );

    let doc_without_dev = run_scan_with_flags(tmp.path(), &["--exclude-scope", "dev"]);
    let dart_prod = composer_components(&doc_without_dev);
    let names_prod: Vec<&str> = dart_prod
        .iter()
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(
        !names_prod.contains(&"phpunit/phpunit"),
        "--exclude-scope dev must suppress phpunit; got {names_prod:?}",
    );
    assert!(
        names_prod.contains(&"symfony/console"),
        "--exclude-scope dev must NOT suppress runtime symfony/console; got {names_prod:?}",
    );
}

fn write_installed_json(vendor_composer_dir: &Path, content: &str) {
    std::fs::create_dir_all(vendor_composer_dir).unwrap();
    std::fs::write(vendor_composer_dir.join("installed.json"), content).unwrap();
}

#[test]
fn deployed_tier_installed_json_only_emits_with_sbom_tier_deployed() {
    // SC-009: installed.json-only scan produces deployed-tier
    // components; dev-package-names[] entries classify as development.
    let tmp = tempfile::tempdir().unwrap();
    let vendor_composer = tmp.path().join("vendor").join("composer");
    let sha1 = "a".repeat(40);
    write_installed_json(
        &vendor_composer,
        &format!(
            r#"{{
  "packages": [
    {{"name":"symfony/console","version":"v7.0.4","type":"library","dist":{{"type":"zip","url":"https://api.github.com/repos/x/x/zipball/x","shasum":"{sha1}"}}}},
    {{"name":"monolog/monolog","version":"3.5.0","type":"library","dist":{{"type":"zip","url":"https://api.github.com/repos/x/x/zipball/y","shasum":"{sha1}"}}}},
    {{"name":"phpunit/phpunit","version":"11.0.0","type":"library","dist":{{"type":"zip","url":"https://api.github.com/repos/x/x/zipball/z","shasum":"{sha1}"}}}}
  ],
  "dev": true,
  "dev-package-names": ["phpunit/phpunit"]
}}
"#
        ),
    );

    let doc = run_scan(tmp.path());
    let components = composer_components(&doc);
    assert_eq!(
        components.len(),
        3,
        "expected 3 installed.json-derived components; got {components:#?}",
    );
    for &c in &components {
        assert_eq!(
            property_value(c, "mikebom:sbom-tier"),
            Some("deployed"),
            "every installed.json-derived component must carry sbom-tier=deployed",
        );
    }
    let phpunit = find_by_name(&components, "phpunit/phpunit").unwrap();
    let lifecycle = property_value(phpunit, "mikebom:lifecycle-scope");
    let cdx_scope = phpunit.get("scope").and_then(|v| v.as_str());
    assert!(
        lifecycle == Some("development")
            || matches!(cdx_scope, Some("excluded") | Some("optional")),
        "phpunit in dev-package-names must carry development indicator",
    );
}

#[test]
fn lockfile_orphan_drift_detection() {
    // SC-010: composer.lock (1 package) + installed.json (2 packages,
    // 1 matching lockfile + 1 orphan); assert lockfile entry emits
    // WITHOUT orphan annotation AND orphan entry emits WITH
    // mikebom:lockfile-orphan = "true" (string value per I3).
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"name":"acme/app","version":"1.0.0","require":{"symfony/console":"^7.0"}}"#,
    )
    .unwrap();
    let sha1 = "a".repeat(40);
    std::fs::write(
        tmp.path().join("composer.lock"),
        format!(
            r#"{{
  "content-hash":"x",
  "packages":[{{
    "name":"symfony/console","version":"v7.0.4","type":"library",
    "dist":{{"type":"zip","url":"https://api.github.com/repos/x/x/zipball/x","shasum":"{sha1}"}}
  }}],
  "packages-dev":[]
}}
"#
        ),
    )
    .unwrap();
    let vendor_composer = tmp.path().join("vendor").join("composer");
    write_installed_json(
        &vendor_composer,
        &format!(
            r#"{{
  "packages": [
    {{"name":"symfony/console","version":"v7.0.4","type":"library","dist":{{"type":"zip","url":"https://api.github.com/repos/x/x/zipball/x","shasum":"{sha1}"}}}},
    {{"name":"foo/orphan","version":"1.5.2","type":"library","dist":{{"type":"zip","url":"https://api.github.com/repos/foo/orphan/zipball/abc","shasum":"{sha1}"}}}}
  ],
  "dev": false,
  "dev-package-names": []
}}
"#
        ),
    );

    let doc = run_scan(tmp.path());

    // Lockfile-pinned entry emits without orphan annotation.
    let console = component_with_purl(&doc, "pkg:composer/symfony/console@v7.0.4").unwrap();
    assert_eq!(
        property_value(console, "mikebom:lockfile-orphan"),
        None,
        "lockfile-pinned entry must NOT carry orphan annotation",
    );

    // Orphan entry MUST carry orphan annotation with value "true" (string).
    let orphan = component_with_purl(&doc, "pkg:composer/foo/orphan@1.5.2")
        .expect("orphan installed.json entry must emit");
    assert_eq!(
        property_value(orphan, "mikebom:lockfile-orphan"),
        Some("true"),
        "orphan entry must carry mikebom:lockfile-orphan = \"true\" (string value per I3 remediation)",
    );
    assert_eq!(
        property_value(orphan, "mikebom:sbom-tier"),
        Some("deployed"),
    );
}

#[test]
fn deployed_tier_only_no_orphan_annotation() {
    // C1 remediation: when NO sibling composer.lock exists, deployed-
    // tier entries from installed.json MUST NOT carry the orphan
    // annotation (the lockfile-vs-disk comparison is undefined).
    let tmp = tempfile::tempdir().unwrap();
    let vendor_composer = tmp.path().join("vendor").join("composer");
    let sha1 = "a".repeat(40);
    write_installed_json(
        &vendor_composer,
        &format!(
            r#"{{
  "packages": [
    {{"name":"foo/bar","version":"1.0.0","type":"library","dist":{{"type":"zip","url":"https://api.github.com/repos/x/x/zipball/x","shasum":"{sha1}"}}}}
  ],
  "dev": false,
  "dev-package-names": []
}}
"#
        ),
    );

    let doc = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:composer/foo/bar@1.0.0")
        .expect("deployed-tier-only entry must emit");
    assert_eq!(
        property_value(c, "mikebom:sbom-tier"),
        Some("deployed"),
    );
    assert_eq!(
        property_value(c, "mikebom:lockfile-orphan"),
        None,
        "deployed-tier-only scan (no sibling lockfile) must NOT carry orphan annotation \
         per C1 remediation — the lockfile-vs-disk comparison is undefined when no lockfile exists",
    );
}
