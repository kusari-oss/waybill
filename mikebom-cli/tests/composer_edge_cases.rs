//! Milestone 138 polish — edge-case coverage per spec Edge Cases +
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

#[test]
fn malformed_lockfile_falls_back_to_design_tier() {
    // SC-005: valid project + project with malformed lockfile;
    // scan succeeds; valid project emits normally; bad project falls
    // back to design-tier from composer.json + warns.
    let tmp = tempfile::tempdir().unwrap();
    let valid = tmp.path().join("valid");
    let bad = tmp.path().join("bad");
    std::fs::create_dir_all(&valid).unwrap();
    std::fs::create_dir_all(&bad).unwrap();

    let sha1 = "a".repeat(40);
    std::fs::write(
        valid.join("composer.json"),
        r#"{"name":"acme/valid","version":"1.0.0","require":{"symfony/console":"^7.0"}}"#,
    )
    .unwrap();
    std::fs::write(
        valid.join("composer.lock"),
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

    std::fs::write(
        bad.join("composer.json"),
        r#"{"name":"acme/bad","version":"0.2.0","require":{"monolog/monolog":"^3.5"}}"#,
    )
    .unwrap();
    std::fs::write(bad.join("composer.lock"), "{this is not: valid JSON [broken").unwrap();

    let (doc, stderr) = run_scan(tmp.path());

    assert!(
        component_with_purl(&doc, "pkg:composer/symfony/console@v7.0.4").is_some(),
        "valid project's lockfile-derived symfony/console must emit",
    );
    assert!(
        component_with_purl(&doc, "pkg:composer/acme/valid@1.0.0").is_some(),
        "valid project's main-module must emit",
    );

    assert!(
        component_with_purl(&doc, "pkg:composer/acme/bad@0.2.0").is_some(),
        "bad project's main-module must still emit",
    );
    let monolog = component_with_purl(&doc, "pkg:composer/monolog/monolog@^3.5")
        .expect("bad project's design-tier monolog must emit from composer.json fallback");
    assert_eq!(
        property_value(monolog, "mikebom:sbom-tier"),
        Some("design"),
    );

    assert!(
        stderr.contains("failed to parse composer.lock"),
        "expected warning for malformed lockfile in stderr; got: {stderr}",
    );
}

#[test]
fn monorepo_emits_one_main_module_per_composer_json() {
    let tmp = tempfile::tempdir().unwrap();
    for member in &["app", "lib_a", "lib_b"] {
        let dir = tmp.path().join("packages").join(member);
        std::fs::create_dir_all(&dir).unwrap();
        let version = match *member {
            "app" => "1.0.0",
            "lib_a" => "0.5.0",
            _ => "0.3.0",
        };
        std::fs::write(
            dir.join("composer.json"),
            format!(r#"{{"name":"acme/{member}","version":"{version}"}}"#),
        )
        .unwrap();
    }

    let (doc, _) = run_scan(tmp.path());
    let purls = composer_purls(&doc);
    for expected in &[
        "pkg:composer/acme/app@1.0.0",
        "pkg:composer/acme/lib_a@0.5.0",
        "pkg:composer/acme/lib_b@0.3.0",
    ] {
        assert!(
            purls.contains(&expected.to_string()),
            "expected workspace member {expected} not found; got {purls:?}",
        );
    }
    assert!(
        !purls.iter().any(|p| p.contains("monorepo-root")),
        "no synthetic monorepo-root component must emit; got {purls:?}",
    );
}

#[test]
fn missing_name_skips_only_main_module() {
    // Q3 clarification: composer.json without `name:` skips ONLY the
    // main-module — lockfile deps still emit per FR-002.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"version":"0.1.0","require":{"symfony/console":"^7.0"}}"#,
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

    let (doc, _) = run_scan(tmp.path());
    assert!(
        component_with_purl(&doc, "pkg:composer/symfony/console@v7.0.4").is_some(),
        "lockfile deps must still emit when composer.json lacks name:",
    );
    // No main-module under "acme/" or similar — only the dep.
    let purls = composer_purls(&doc);
    let main_modules: Vec<&String> = purls
        .iter()
        .filter(|p| {
            component_with_purl(&doc, p)
                .and_then(|c| property_value(c, "mikebom:component-role"))
                == Some("main-module")
        })
        .collect();
    assert!(
        main_modules.is_empty(),
        "no main-module must emit when composer.json lacks name:; got {main_modules:?}",
    );
}

#[test]
fn missing_version_falls_back_to_unknown_placeholder() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"name":"acme/library-in-development"}"#,
    )
    .unwrap();

    let (doc, _) = run_scan(tmp.path());
    assert!(
        component_with_purl(&doc, "pkg:composer/acme/library-in-development").is_some(),
        "missing-version main-module must emit with 0.0.0-unknown placeholder",
    );
}

#[test]
fn composer_1_installed_json_warns_and_skips() {
    // R3: top-level JSON array = Composer 1 installed.json — warn-and-skip.
    let tmp = tempfile::tempdir().unwrap();
    let vendor_composer = tmp.path().join("vendor").join("composer");
    std::fs::create_dir_all(&vendor_composer).unwrap();
    // Bare-array form (Composer 1)
    std::fs::write(
        vendor_composer.join("installed.json"),
        r#"[{"name":"foo/bar","version":"1.0.0","type":"library"}]"#,
    )
    .unwrap();

    let (doc, stderr) = run_scan(tmp.path());
    // Filter to only composer-derived PURLs (pkg:composer/...) — the
    // pkg:generic/ shape from a path-source composer entry only emits
    // if the composer reader actually parsed something; here we're
    // testing the rejection path so nothing composer-related should
    // emit. Other readers may emit pkg:generic/ for unrelated reasons
    // (e.g., scanning the tempdir as a non-PHP project), which we ignore.
    let composer_purls: Vec<String> = doc
        .get("components")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("purl").and_then(|v| v.as_str()))
                .filter(|p| {
                    p.starts_with("pkg:composer/")
                        || (p.starts_with("pkg:generic/")
                            && c_source_type(arr, p).is_some_and(|s| s.starts_with("composer-")))
                })
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    assert!(
        composer_purls.is_empty(),
        "Composer 1 installed.json must produce zero composer components; got {composer_purls:?}",
    );
    assert!(
        stderr.contains("Composer 1") || stderr.contains("failed to parse installed.json"),
        "expected Composer 1 warn-and-skip in stderr; got: {stderr}",
    );
}

fn c_source_type<'a>(arr: &'a [Value], purl: &str) -> Option<&'a str> {
    arr.iter()
        .find(|c| c.get("purl").and_then(|v| v.as_str()) == Some(purl))
        .and_then(|c| property_value(c, "mikebom:source-type"))
}

#[test]
fn multi_layer_installed_json_dedupes_via_seen_purls() {
    // Q2: two installed.json files at different paths (simulating
    // container layers) containing the same package → only ONE
    // component emits per PURL via orchestrator dedup.
    let tmp = tempfile::tempdir().unwrap();
    let sha1 = "a".repeat(40);
    let body = format!(
        r#"{{
  "packages":[{{"name":"foo/bar","version":"1.0.0","type":"library","dist":{{"type":"zip","url":"https://api.github.com/repos/x/x/zipball/x","shasum":"{sha1}"}}}}],
  "dev":false,
  "dev-package-names":[]
}}
"#
    );
    for layer in &["layer1", "layer2"] {
        let vc = tmp.path().join(layer).join("vendor").join("composer");
        std::fs::create_dir_all(&vc).unwrap();
        std::fs::write(vc.join("installed.json"), &body).unwrap();
    }

    let (doc, _) = run_scan(tmp.path());
    let foo_bar_count = composer_purls(&doc)
        .iter()
        .filter(|p| p.as_str() == "pkg:composer/foo/bar@1.0.0")
        .count();
    assert_eq!(
        foo_bar_count, 1,
        "multi-layer installed.json with same PURL must dedupe to ONE component",
    );
}

#[test]
fn git_source_missing_reference_warns_and_skips_entry() {
    // Edge Cases: git source lacking `source.reference` warns-and-skips
    // that single entry; other lockfile entries still emit.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"name":"acme/app","version":"1.0.0"}"#,
    )
    .unwrap();
    let sha1 = "a".repeat(40);
    std::fs::write(
        tmp.path().join("composer.lock"),
        format!(
            r#"{{
  "content-hash":"x",
  "packages":[
    {{
      "name":"symfony/console","version":"v7.0.4","type":"library",
      "dist":{{"type":"zip","url":"https://api.github.com/repos/x/x/zipball/x","shasum":"{sha1}"}}
    }},
    {{
      "name":"acme/broken-git","version":"dev-main","type":"library",
      "source":{{"type":"git","url":"https://example.com/r.git"}}
    }}
  ],
  "packages-dev":[]
}}
"#
        ),
    )
    .unwrap();

    let (doc, stderr) = run_scan(tmp.path());
    assert!(
        component_with_purl(&doc, "pkg:composer/symfony/console@v7.0.4").is_some(),
        "valid hosted entry must still emit when sibling git entry is malformed",
    );
    let purls = composer_purls(&doc);
    assert!(
        !purls.iter().any(|p| p.contains("broken-git")),
        "malformed git entry must NOT emit; got {purls:?}",
    );
    assert!(
        stderr.contains("skipping malformed lockfile entry")
            || stderr.contains("source.reference"),
        "expected git skip warning in stderr; got: {stderr}",
    );
}

#[test]
fn empty_packages_block_emits_only_main_module() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"name":"acme/empty-app","version":"0.1.0"}"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("composer.lock"),
        r#"{"content-hash":"x","packages":[],"packages-dev":[]}"#,
    )
    .unwrap();

    let (doc, stderr) = run_scan(tmp.path());
    let purls = composer_purls(&doc);
    assert_eq!(
        purls.len(),
        1,
        "only main-module must emit on empty packages block; got {purls:?}",
    );
    assert_eq!(purls[0], "pkg:composer/acme/empty-app@0.1.0");
    assert!(
        !stderr.contains("composer: failed"),
        "no composer warnings expected on empty packages block; got: {stderr}",
    );
}

#[test]
fn platform_requirements_dont_emit_as_components() {
    // composer.json::require typically includes platform requirements
    // (`php`, `php-64bit`, `ext-mbstring`, `lib-*`). These aren't
    // Packagist packages and MUST NOT emit as components in design-tier
    // mode (no `/` separator filters them out).
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{
  "name": "acme/app",
  "version": "1.0.0",
  "require": {
    "php": ">=8.2",
    "ext-mbstring": "*",
    "ext-json": "*",
    "symfony/console": "^7.0"
  }
}
"#,
    )
    .unwrap();

    let (doc, _) = run_scan(tmp.path());
    let purls = composer_purls(&doc);
    for forbidden in &["pkg:composer/php", "pkg:composer/ext-mbstring", "pkg:composer/ext-json"] {
        assert!(
            !purls.iter().any(|p| p.starts_with(forbidden)),
            "platform requirement {forbidden}* must NOT emit as a component; got {purls:?}",
        );
    }
    assert!(
        purls.iter().any(|p| p.starts_with("pkg:composer/symfony/console@")),
        "real Packagist dep must still emit",
    );
}
