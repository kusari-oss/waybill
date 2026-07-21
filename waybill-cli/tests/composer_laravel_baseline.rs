//! Milestone 138 US1 — Laravel/Symfony app baseline integration tests.
//!
//! Covers spec acceptance scenarios US1.1, US1.2, US1.4 plus SC-001
//! (Laravel app baseline), SC-007 (dev-scope filterability), SC-008
//! (main-module emission), and FR-013 (SHA-1 hash emission).

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
    // Top-level `--exclude-scope` MUST be passed BEFORE the `sbom`
    // subcommand per main.rs; splice it in before the subcommand args.
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

fn composer_purls(doc: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut visit = |c: &Value| {
        if let Some(p) = c.get("purl").and_then(|v| v.as_str()) {
            if p.starts_with("pkg:composer/") || p.starts_with("pkg:generic/") {
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

fn write_laravel_fixture(root: &Path) {
    std::fs::write(
        root.join("composer.json"),
        r#"{
  "name": "acme/my-app",
  "version": "1.2.3",
  "description": "Test fixture for milestone 138.",
  "type": "project",
  "require": {
    "php": ">=8.2",
    "symfony/console": "^7.0",
    "monolog/monolog": "^3.5",
    "guzzlehttp/guzzle": "^7.8"
  }
}
"#,
    )
    .unwrap();
    let sha1 = "a".repeat(40);
    std::fs::write(
        root.join("composer.lock"),
        format!(
            r#"{{
  "_readme": ["test fixture"],
  "content-hash": "deadbeef",
  "plugin-api-version": "2.6.0",
  "packages": [
    {{
      "name": "symfony/console",
      "version": "v7.0.4",
      "type": "library",
      "source": {{"type":"git","url":"https://github.com/symfony/console.git","reference":"abc123"}},
      "dist": {{"type":"zip","url":"https://api.github.com/repos/symfony/console/zipball/abc","shasum":"{sha1}"}},
      "require": {{"psr/log":"^3.0"}}
    }},
    {{
      "name": "monolog/monolog",
      "version": "3.5.0",
      "type": "library",
      "dist": {{"type":"zip","url":"https://api.github.com/repos/Seldaek/monolog/zipball/def","shasum":"{sha1}"}}
    }},
    {{
      "name": "guzzlehttp/guzzle",
      "version": "7.8.1",
      "type": "library",
      "dist": {{"type":"zip","url":"https://api.github.com/repos/guzzle/guzzle/zipball/ghi","shasum":"{sha1}"}}
    }},
    {{
      "name": "psr/log",
      "version": "3.0.0",
      "type": "library",
      "dist": {{"type":"zip","url":"https://api.github.com/repos/php-fig/log/zipball/jkl","shasum":"{sha1}"}}
    }},
    {{
      "name": "symfony/polyfill-mbstring",
      "version": "v1.28.0",
      "type": "library",
      "dist": {{"type":"zip","url":"https://api.github.com/repos/symfony/polyfill-mbstring/zipball/mno","shasum":"{sha1}"}}
    }}
  ],
  "packages-dev": []
}}
"#
        ),
    )
    .unwrap();
}

#[test]
fn laravel_app_baseline_emits_lockfile_count_plus_main_module() {
    // SC-001: 5 lockfile entries + 1 main-module = 6 composer-derived components.
    let tmp = tempfile::tempdir().unwrap();
    write_laravel_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let purls = composer_purls(&doc);
    let dart_purls: Vec<&String> = purls
        .iter()
        .filter(|p| p.starts_with("pkg:composer/"))
        .collect();
    assert_eq!(
        dart_purls.len(),
        6,
        "expected 6 pkg:composer/* components (5 lockfile + 1 main-module), got {dart_purls:#?}"
    );

    for expected in &[
        "pkg:composer/symfony/console@v7.0.4",
        "pkg:composer/monolog/monolog@3.5.0",
        "pkg:composer/guzzlehttp/guzzle@7.8.1",
        "pkg:composer/psr/log@3.0.0",
        "pkg:composer/symfony/polyfill-mbstring@v1.28.0",
        "pkg:composer/acme/my-app@1.2.3",
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
    write_laravel_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let main = component_with_purl(&doc, "pkg:composer/acme/my-app@1.2.3")
        .expect("main-module component must exist");
    assert_eq!(
        property_value(main, "mikebom:component-role"),
        Some("main-module"),
        "main-module must carry the component-role annotation",
    );
}

#[test]
fn main_module_depends_lists_direct_deps() {
    // US1 acceptance scenario 4 + SC-008: dependencies[] entry for
    // main-module bom-ref targets each direct dep's bom-ref.
    let tmp = tempfile::tempdir().unwrap();
    write_laravel_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let main = component_with_purl(&doc, "pkg:composer/acme/my-app@1.2.3").unwrap();
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
    let main_dep_refs: Vec<&str> = main_deps.iter().filter_map(|v| v.as_str()).collect();

    for direct_purl in &[
        "pkg:composer/symfony/console@v7.0.4",
        "pkg:composer/monolog/monolog@3.5.0",
        "pkg:composer/guzzlehttp/guzzle@7.8.1",
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
fn sha1_hash_emitted_for_packagist_entries() {
    // FR-013: lockfile entries with dist.shasum produce CDX hashes[]
    // entries with alg=SHA-1.
    let tmp = tempfile::tempdir().unwrap();
    write_laravel_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let console = component_with_purl(&doc, "pkg:composer/symfony/console@v7.0.4").unwrap();
    let hashes = console
        .get("hashes")
        .and_then(|v| v.as_array())
        .expect("symfony/console must carry a hashes[] array (FR-013)");
    let sha1_entries: Vec<&Value> = hashes
        .iter()
        .filter(|h| h.get("alg").and_then(|v| v.as_str()) == Some("SHA-1"))
        .collect();
    assert_eq!(
        sha1_entries.len(),
        1,
        "expected exactly one SHA-1 hash on symfony/console; got {hashes:#?}"
    );
    let content = sha1_entries[0].get("content").and_then(|v| v.as_str()).unwrap();
    assert_eq!(content.len(), 40, "SHA-1 hex content must be 40 chars");
}

#[test]
fn dev_scope_filterability() {
    // SC-007: packages-dev[] entries produce components with
    // mikebom:lifecycle-scope = development; --exclude-scope dev
    // suppresses them.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{
  "name": "acme/my-app",
  "version": "1.0.0",
  "require": {"symfony/console":"^7.0"},
  "require-dev": {"phpunit/phpunit":"^11.0"}
}
"#,
    )
    .unwrap();
    let sha1 = "a".repeat(40);
    std::fs::write(
        tmp.path().join("composer.lock"),
        format!(
            r#"{{
  "content-hash": "x",
  "packages": [{{
    "name":"symfony/console","version":"v7.0.4","type":"library",
    "dist":{{"type":"zip","url":"https://api.github.com/repos/x/x/zipball/x","shasum":"{sha1}"}}
  }}],
  "packages-dev": [{{
    "name":"phpunit/phpunit","version":"11.0.0","type":"library",
    "dist":{{"type":"zip","url":"https://api.github.com/repos/x/x/zipball/y","shasum":"{sha1}"}}
  }}]
}}
"#
        ),
    )
    .unwrap();

    let doc_with_dev = run_scan(tmp.path());
    let phpunit = component_with_purl(&doc_with_dev, "pkg:composer/phpunit/phpunit@11.0.0");
    assert!(phpunit.is_some(), "default scan must include dev-scope phpunit");
    if let Some(c) = phpunit {
        let lifecycle = property_value(c, "mikebom:lifecycle-scope");
        let cdx_scope = c.get("scope").and_then(|v| v.as_str());
        assert!(
            lifecycle == Some("development")
                || matches!(cdx_scope, Some("excluded") | Some("optional")),
            "phpunit must carry development indicator; property={lifecycle:?} scope={cdx_scope:?}",
        );
    }

    let doc_without_dev = run_scan_with_flags(tmp.path(), &["--exclude-scope", "dev"]);
    assert!(
        component_with_purl(&doc_without_dev, "pkg:composer/phpunit/phpunit@11.0.0").is_none(),
        "--exclude-scope dev must suppress phpunit/phpunit",
    );
    assert!(
        component_with_purl(&doc_without_dev, "pkg:composer/symfony/console@v7.0.4").is_some(),
        "--exclude-scope dev must NOT suppress runtime symfony/console",
    );
}
