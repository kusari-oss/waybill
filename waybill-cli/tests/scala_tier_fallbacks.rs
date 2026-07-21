//! Milestone 142 US3 — design-tier + Q1 cascade + Q2 multi-project tests.
//!
//! Covers SC-003 (design-tier from build.sbt only) + SC-007 (test-config
//! → dev-scope filterability) + SC-009 (multi-project = 3 subprojects + 1
//! root = 4 main-modules per FR-009 + F1 remediation) + Q1 default-fallback
//! annotation.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
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

fn run_scan(project_root: &Path) -> Value {
    run_scan_with_flags(project_root, &[])
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

fn property_value<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c.get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
}

fn component_with_name<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    all_components(doc)
        .into_iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
}

#[test]
fn sc003_design_tier_from_build_sbt_only() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("build.sbt"),
        r#"scalaVersion := "2.13.12"
libraryDependencies ++= Seq(
  "org.typelevel" %% "cats-core" % "2.10.0",
  "org.postgresql" % "postgresql" % "42.7.0"
)
"#,
    )
    .unwrap();
    // NO build.sbt.lock — triggers design-tier fallback per FR-005.
    let doc = run_scan(dir.path());
    let cats = component_with_name(&doc, "cats-core").expect("cats-core design-tier component");
    assert_eq!(property_value(cats, "waybill:sbom-tier"), Some("design"));
    assert_eq!(
        property_value(cats, "waybill:source-type"),
        Some("scala-sbt-design"),
    );
    // F6: %% dep carries scala-version-source annotation.
    assert_eq!(
        property_value(cats, "waybill:scala-version-source"),
        Some("build-sbt-explicit"),
    );
    // Pure-Java dep does NOT carry scala-version-source.
    let postgres =
        component_with_name(&doc, "postgresql").expect("postgresql design-tier component");
    assert_eq!(property_value(postgres, "waybill:scala-version-source"), None);
}

#[test]
fn q1_default_fallback_when_scalaversion_absent() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("build.sbt"),
        r#"libraryDependencies += "org.typelevel" %% "cats-core" % "2.10.0"
"#,
    )
    .unwrap();
    // No scalaVersion declared, no project/build.properties → Q1 rung 3.
    let doc = run_scan(dir.path());
    let cats =
        component_with_name(&doc, "cats-core").expect("cats-core design-tier component");
    // PURL uses _2.13 default suffix.
    assert_eq!(
        cats.get("purl").and_then(|v| v.as_str()),
        Some("pkg:maven/org.typelevel/cats-core_2.13@2.10.0"),
    );
    assert_eq!(
        property_value(cats, "waybill:scala-version-source"),
        Some("default-fallback"),
    );
}

#[test]
fn sc007_test_config_dev_scope_filterability() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("build.sbt"),
        r#"scalaVersion := "2.13.12"
libraryDependencies += "org.scalatest" %% "scalatest" % "3.2.18" % Test
"#,
    )
    .unwrap();
    let doc = run_scan(dir.path());
    let scalatest =
        component_with_name(&doc, "scalatest").expect("scalatest dev-scope component");
    // CDX native scope=excluded per milestone-052 dev-scope bridge.
    let scope = scalatest.get("scope").and_then(|v| v.as_str());
    assert_eq!(scope, Some("excluded"));
    // --exclude-scope dev suppresses scalatest.
    let doc_excluded = run_scan_with_flags(dir.path(), &["--exclude-scope", "dev"]);
    assert!(
        component_with_name(&doc_excluded, "scalatest").is_none(),
        "scalatest must be suppressed under --exclude-scope dev",
    );
}

#[test]
fn sc009_multi_project_three_subprojects_plus_root() {
    let dir = tempfile::tempdir().unwrap();
    // Root build.sbt declares 3 subprojects via lazy val.
    std::fs::write(
        dir.path().join("build.sbt"),
        r#"ThisBuild / organization := "com.example"
ThisBuild / version := "1.0.0"
ThisBuild / scalaVersion := "2.13.12"

lazy val core = project.in(file("core"))
lazy val server = project.in(file("server"))
lazy val worker = project.in(file("worker"))
"#,
    )
    .unwrap();
    // Each subproject directory exists (no inner build.sbt necessary —
    // Phase A's lazy val parsing surfaces them).
    for sub in &["core", "server", "worker"] {
        std::fs::create_dir_all(dir.path().join(sub)).unwrap();
    }
    let doc = run_scan(dir.path());
    // Per F1 remediation: 3 subprojects + 1 root = 4 main-modules.
    let main_modules: Vec<&Value> = all_components(&doc)
        .into_iter()
        .filter(|c| {
            property_value(c, "waybill:component-role") == Some("main-module")
                && property_value(c, "waybill:source-type") == Some("scala-main-module")
        })
        .collect();
    assert_eq!(
        main_modules.len(),
        4,
        "expected 3 subprojects + 1 root = 4 main-modules per FR-009 + F1 remediation; got {}",
        main_modules.len(),
    );
    // Each subproject name surfaces.
    let names: Vec<&str> = main_modules
        .iter()
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(names.contains(&"core"));
    assert!(names.contains(&"server"));
    assert!(names.contains(&"worker"));
}
