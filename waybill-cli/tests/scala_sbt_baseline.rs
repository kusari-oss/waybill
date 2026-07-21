//! Milestone 142 US1 — *.sbt.lock baseline integration tests.
//!
//! Covers SC-001 (3 direct + 4 transitive = 7 pkg:maven/* components
//! emit) + FR-011 (schema-v2 inner SHA-256 emission).

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

fn scala_components(doc: &Value) -> Vec<&Value> {
    let mut out: Vec<&Value> = Vec::new();
    let predicate = |c: &Value| -> bool {
        c.get("properties")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter().any(|p| {
                    p.get("name").and_then(|v| v.as_str()) == Some("waybill:source-type")
                        && p.get("value")
                            .and_then(|v| v.as_str())
                            .map(|s| s.starts_with("scala-"))
                            .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    };
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            if predicate(c) {
                out.push(c);
            }
        }
    }
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        if predicate(c) {
            out.push(c);
        }
    }
    out
}

fn component_with_name<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        if c.get("name").and_then(|v| v.as_str()) == Some(name) {
            return Some(c);
        }
    }
    doc.get("components")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
        })
}

fn write_sbt_fixture(root: &Path) {
    // 64-hex-char SHA-256 strings (deterministic constants for test
    // reproducibility per the milestone-141 F5 finding remediation).
    let sha_cats = "a".repeat(64);
    let sha_akka = "b".repeat(64);
    let sha_scala_lib = "c".repeat(64);
    let sha_collection = "d".repeat(64);
    let sha_config = "e".repeat(64);
    let sha_ssl_config = "f".repeat(64);
    let sha_reactive = "0".repeat(64);

    std::fs::write(
        root.join("build.sbt"),
        r#"name := "my-app"
version := "1.2.3"
organization := "com.example"
scalaVersion := "2.13.12"

libraryDependencies ++= Seq(
  "org.typelevel" %% "cats-core" % "2.10.0",
  "com.typesafe.akka" %% "akka-actor" % "2.6.20",
  "com.typesafe" % "config" % "1.4.3"
)
"#,
    )
    .unwrap();

    std::fs::write(
        root.join("build.sbt.lock"),
        format!(
            r#"{{
  "lockVersion": 2,
  "timestamp": "2024-01-15T12:34:56Z",
  "configurations": ["compile", "test"],
  "modules": [
    {{"org": "org.typelevel", "name": "cats-core_2.13", "version": "2.10.0", "configurations": ["compile"],
     "checksums": [{{"name": "cats-core_2.13.jar", "type": "SHA-256", "checksum": "{sha_cats}"}}]}},
    {{"org": "com.typesafe.akka", "name": "akka-actor_2.13", "version": "2.6.20", "configurations": ["compile"],
     "checksums": [{{"name": "akka-actor_2.13.jar", "type": "SHA-256", "checksum": "{sha_akka}"}}]}},
    {{"org": "com.typesafe", "name": "config", "version": "1.4.3", "configurations": ["compile"],
     "checksums": [{{"name": "config.jar", "type": "SHA-256", "checksum": "{sha_config}"}}]}},
    {{"org": "org.scala-lang", "name": "scala-library", "version": "2.13.12", "configurations": ["compile"],
     "checksums": [{{"name": "scala-library.jar", "type": "SHA-256", "checksum": "{sha_scala_lib}"}}]}},
    {{"org": "org.scala-lang.modules", "name": "scala-collection-compat_2.13", "version": "2.11.0", "configurations": ["compile"],
     "checksums": [{{"name": "scala-collection-compat_2.13.jar", "type": "SHA-256", "checksum": "{sha_collection}"}}]}},
    {{"org": "com.typesafe", "name": "ssl-config-core_2.13", "version": "0.6.1", "configurations": ["compile"],
     "checksums": [{{"name": "ssl-config-core_2.13.jar", "type": "SHA-256", "checksum": "{sha_ssl_config}"}}]}},
    {{"org": "org.reactivestreams", "name": "reactive-streams", "version": "1.0.4", "configurations": ["compile"],
     "checksums": [{{"name": "reactive-streams.jar", "type": "SHA-256", "checksum": "{sha_reactive}"}}]}}
  ]
}}
"#,
        ),
    )
    .unwrap();
}

#[test]
fn sc001_baseline_seven_components() {
    let dir = tempfile::tempdir().unwrap();
    write_sbt_fixture(dir.path());
    let doc = run_scan(dir.path());

    // 7 lockfile-derived components carry waybill:source-type = scala-sbt-lock.
    let lock_components: Vec<&Value> = scala_components(&doc)
        .into_iter()
        .filter(|c| {
            c.get("properties")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter().any(|p| {
                        p.get("name").and_then(|v| v.as_str()) == Some("waybill:source-type")
                            && p.get("value").and_then(|v| v.as_str()) == Some("scala-sbt-lock")
                    })
                })
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(
        lock_components.len(),
        7,
        "expected exactly 7 pkg:maven/ components with scala-sbt-lock source-type",
    );
}

#[test]
fn sc001_inner_sha256_hash_emitted() {
    let dir = tempfile::tempdir().unwrap();
    write_sbt_fixture(dir.path());
    let doc = run_scan(dir.path());
    let cats = component_with_name(&doc, "cats-core_2.13").expect("cats-core_2.13 component");
    let hashes = cats
        .get("hashes")
        .and_then(|v| v.as_array())
        .expect("cats-core_2.13 must carry hashes array");
    assert_eq!(hashes.len(), 1, "exactly one SHA-256 entry per FR-011");
    let alg = hashes[0].get("alg").and_then(|v| v.as_str()).unwrap();
    let content = hashes[0].get("content").and_then(|v| v.as_str()).unwrap();
    assert_eq!(alg, "SHA-256");
    assert_eq!(content, "a".repeat(64));
}

#[test]
fn sc001_source_type_annotation() {
    let dir = tempfile::tempdir().unwrap();
    write_sbt_fixture(dir.path());
    let doc = run_scan(dir.path());
    let cats = component_with_name(&doc, "cats-core_2.13").expect("cats-core_2.13 component");
    let props = cats.get("properties").and_then(|v| v.as_array()).unwrap();
    let st = props
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some("waybill:source-type"))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
        .expect("waybill:source-type property");
    assert_eq!(st, "scala-sbt-lock");
}
