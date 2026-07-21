//! Milestone 142 US2 — source-discriminator + main-module integration tests.
//!
//! Covers SC-002 (Scala 2 vs 3 vs Java distinction) + SC-008 (main-module
//! emission from build.sbt) + SC-010 (cross-built libs emit as distinct
//! components).

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

fn all_components(doc: &Value) -> Vec<&Value> {
    let mut out: Vec<&Value> = Vec::new();
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        out.extend(arr.iter());
    }
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        out.push(c);
    }
    out
}

fn component_with_purl<'a>(doc: &'a Value, purl: &str) -> Option<&'a Value> {
    all_components(doc)
        .into_iter()
        .find(|c| c.get("purl").and_then(|v| v.as_str()) == Some(purl))
}

fn property_value<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c.get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
}

#[test]
fn sc002_scala_2_13_lockfile_purl() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("build.sbt"),
        r#"scalaVersion := "2.13.12"
libraryDependencies += "org.typelevel" %% "cats-core" % "2.10.0"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("build.sbt.lock"),
        r#"{"lockVersion":1,"modules":[
            {"org":"org.typelevel","name":"cats-core_2.13","version":"2.10.0","configurations":["compile"]}
        ]}"#,
    )
    .unwrap();
    let doc = run_scan(dir.path());
    let _ = component_with_purl(&doc, "pkg:maven/org.typelevel/cats-core_2.13@2.10.0")
        .expect("Scala 2.13 PURL must emit");
}

#[test]
fn sc002_scala_3_lockfile_purl_bare_3() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("build.sbt"), "scalaVersion := \"3.3.1\"\n").unwrap();
    std::fs::write(
        dir.path().join("build.sbt.lock"),
        r#"{"lockVersion":1,"modules":[
            {"org":"org.typelevel","name":"cats-core_3","version":"2.10.0","configurations":["compile"]}
        ]}"#,
    )
    .unwrap();
    let doc = run_scan(dir.path());
    // Bare _3 (NOT _3.3) per spec FR-003 + research §R3.
    let _ = component_with_purl(&doc, "pkg:maven/org.typelevel/cats-core_3@2.10.0")
        .expect("Scala 3 PURL must use bare _3 suffix");
}

#[test]
fn sc002_pure_java_lockfile_purl_no_suffix() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("build.sbt"),
        "libraryDependencies += \"org.postgresql\" % \"postgresql\" % \"42.7.0\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("build.sbt.lock"),
        r#"{"lockVersion":1,"modules":[
            {"org":"org.postgresql","name":"postgresql","version":"42.7.0","configurations":["compile"]}
        ]}"#,
    )
    .unwrap();
    let doc = run_scan(dir.path());
    let _ = component_with_purl(&doc, "pkg:maven/org.postgresql/postgresql@42.7.0")
        .expect("pure-Java PURL must NOT carry Scala suffix");
}

#[test]
fn sc010_cross_built_distinct_components() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("build.sbt"), "scalaVersion := \"2.13.12\"\n").unwrap();
    // Lockfile contains BOTH _2.13 AND _3 cross-built variants.
    std::fs::write(
        dir.path().join("build.sbt.lock"),
        r#"{"lockVersion":1,"modules":[
            {"org":"org.typelevel","name":"cats-core_2.13","version":"2.10.0","configurations":["compile"]},
            {"org":"org.typelevel","name":"cats-core_3","version":"2.10.0","configurations":["compile"]}
        ]}"#,
    )
    .unwrap();
    let doc = run_scan(dir.path());
    // Two distinct components emit; they do NOT collapse via dedup.
    let _ = component_with_purl(&doc, "pkg:maven/org.typelevel/cats-core_2.13@2.10.0")
        .expect("_2.13 variant must emit");
    let _ = component_with_purl(&doc, "pkg:maven/org.typelevel/cats-core_3@2.10.0")
        .expect("_3 variant must emit");
}

#[test]
fn sc008_main_module_emission_scala_2_13() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("build.sbt"),
        r#"name := "my_app"
version := "1.2.3"
organization := "com.example"
scalaVersion := "2.13.12"
"#,
    )
    .unwrap();
    // Add a lockfile so sbom_tier is "source" not "design"
    std::fs::write(
        dir.path().join("build.sbt.lock"),
        r#"{"lockVersion":1,"modules":[]}"#,
    )
    .unwrap();
    let doc = run_scan(dir.path());
    let main = component_with_purl(&doc, "pkg:maven/com.example/my_app_2.13@1.2.3")
        .expect("main-module PURL must emit with _2.13 suffix");
    assert_eq!(
        property_value(main, "waybill:component-role"),
        Some("main-module"),
    );
    // Per F6 remediation: main-module carries scala-version-source.
    assert_eq!(
        property_value(main, "waybill:scala-version-source"),
        Some("build-sbt-explicit"),
    );
}

#[test]
fn sc008_main_module_emission_scala_3_bare_suffix() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("build.sbt"),
        r#"name := "my_app"
version := "1.0.0"
organization := "com.example"
scalaVersion := "3.3.1"
"#,
    )
    .unwrap();
    let doc = run_scan(dir.path());
    let _ = component_with_purl(&doc, "pkg:maven/com.example/my_app_3@1.0.0")
        .expect("main-module must use bare _3 (not _3.3)");
}
