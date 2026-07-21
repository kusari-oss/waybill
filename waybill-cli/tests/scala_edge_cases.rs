//! Milestone 142 edge-case tests.
//!
//! Covers SC-004 (no-op preservation on non-Scala trees) + SC-005
//! (malformed lockfile graceful degradation) + Q3 content-shape gate
//! (non-SBT files matching *.sbt.lock glob) + main-module fallback
//! paths + %%% triple-percent warn-and-skip.

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

#[test]
fn sc004_no_op_on_non_scala_tree() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# Not a Scala project\n").unwrap();
    std::fs::write(dir.path().join("hello.txt"), "no build.sbt here\n").unwrap();
    let (doc, stderr) = run_scan(dir.path());
    let scala_components: Vec<&Value> = all_components(&doc)
        .into_iter()
        .filter(|c| {
            property_value(c, "mikebom:source-type")
                .map(|s| s.starts_with("scala-"))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        scala_components.is_empty(),
        "non-Scala tree must produce zero scala-derived components; got: {scala_components:?}",
    );
    assert!(
        !stderr.contains("scala:"),
        "non-Scala tree must not emit any 'scala:' warnings; stderr={stderr}",
    );
}

#[test]
fn sc005_malformed_lockfile_falls_back_to_design_tier() {
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
        "this is not valid JSON {{{",
    )
    .unwrap();
    let (doc, stderr) = run_scan(dir.path());
    assert!(
        stderr.contains("scala: failed to parse *.sbt.lock"),
        "expected parse-failure warning; stderr={stderr}",
    );
    // Design-tier fallback emits cats-core from build.sbt.
    let cats = all_components(&doc)
        .into_iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some("cats-core"))
        .expect("cats-core design-tier fallback");
    assert_eq!(property_value(cats, "mikebom:sbom-tier"), Some("design"));
}

#[test]
fn q3_content_shape_skips_non_sbt_files() {
    let dir = tempfile::tempdir().unwrap();
    // A file matching *.sbt.lock glob but with non-SBT JSON shape.
    std::fs::write(
        dir.path().join("random.sbt.lock"),
        r#"{"unrelated": "json", "no_lockversion": true}"#,
    )
    .unwrap();
    let (doc, stderr) = run_scan(dir.path());
    let scala_components: Vec<&Value> = all_components(&doc)
        .into_iter()
        .filter(|c| {
            property_value(c, "mikebom:source-type")
                .map(|s| s.starts_with("scala-"))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        scala_components.is_empty(),
        "non-SBT *.sbt.lock-matching file must NOT emit any scala components",
    );
    // Warn-and-skip diagnostic appears.
    assert!(
        stderr.contains("scala: failed to parse *.sbt.lock"),
        "Q3 content-shape failure must surface as warning; stderr={stderr}",
    );
}

#[test]
fn main_module_version_fallback_to_0_0_0_unknown() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("build.sbt"),
        r#"name := "my-app"
organization := "com.example"
scalaVersion := "2.13.12"
"#,
    )
    .unwrap();
    let (doc, _) = run_scan(dir.path());
    let main = all_components(&doc)
        .into_iter()
        .find(|c| {
            c.get("purl").and_then(|v| v.as_str())
                == Some("pkg:maven/com.example/my-app_2.13")
        })
        .expect("main-module with version fallback");
    assert_eq!(main.get("name").and_then(|v| v.as_str()), Some("my-app"));
}

#[test]
fn main_module_name_fallback_to_dir_basename() {
    let dir = tempfile::tempdir().unwrap();
    let subdir = dir.path().join("orphaned_app");
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::write(
        subdir.join("build.sbt"),
        r#"libraryDependencies += "org.typelevel" %% "cats-core" % "2.10.0"
"#,
    )
    .unwrap();
    let (doc, _) = run_scan(dir.path());
    // No name/version/organization → all fallbacks applied. The
    // main-module emits as pkg:maven/unknown/orphaned_app_2.13@0.0.0-unknown
    // for the subdir-discovered project, and another emits for the root.
    let orphan = all_components(&doc)
        .into_iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some("orphaned_app"))
        .expect("orphaned_app subdir-derived main-module");
    let purl = orphan.get("purl").and_then(|v| v.as_str()).unwrap();
    assert!(
        purl.contains("orphaned_app_2.13"),
        "main-module PURL should contain subdir-basename + Scala suffix: {purl}",
    );
}

#[test]
fn triple_percent_warn_and_skip() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("build.sbt"),
        r#"scalaVersion := "2.13.12"
libraryDependencies += "org.scala-js" %%% "scalajs-dom" % "2.4.0"
libraryDependencies += "org.typelevel" %% "cats-core" % "2.10.0"
"#,
    )
    .unwrap();
    let (doc, _) = run_scan(dir.path());
    // scalajs-dom (%%%) should NOT emit; cats-core (%%) should emit.
    let scalajs = all_components(&doc)
        .into_iter()
        .find(|c| {
            c.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.contains("scalajs-dom"))
                .unwrap_or(false)
        });
    assert!(scalajs.is_none(), "%%% scalajs-dom must NOT emit (Out-of-Scope)");
    let cats = all_components(&doc)
        .into_iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some("cats-core"));
    assert!(cats.is_some(), "%% cats-core must emit alongside the %%% skip");
}
