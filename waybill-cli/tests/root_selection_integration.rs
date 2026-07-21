//! Integration tests for milestone 127 — smarter BOM-subject root
//! selection. Covers US1 (multi-module Go workspace picks the
//! repo-root module), US2 (polyglot Go-vs-Maven-vs-npm prefers Go),
//! the SC-006 operator-override regression, and verifies the
//! `waybill:root-selection-heuristic` annotation shape across all
//! three formats.
//!
//! Fixtures are synthesized inline into `tempfile::tempdir()`
//! workspaces so the tests are hermetic and don't depend on the
//! milestone-090 fixture repo.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::fs;
use std::path::Path;
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

/// Run `waybill sbom scan --path <dir>` in offline mode against the
/// given format. Returns the parsed JSON output.
fn run_scan_returning_json(
    fake_home: &Path,
    scan_target: &Path,
    extra_args: &[&str],
    out_format: &str,
    out_filename: &str,
) -> (serde_json::Value, String) {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join(out_filename);
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target)
        .arg("--format")
        .arg(out_format)
        .arg("--output")
        .arg(format!("{out_format}={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("scan runs");
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body = fs::read_to_string(&out_path).expect("read output file");
    let json: serde_json::Value =
        serde_json::from_str(&body).expect("output is valid JSON");
    (json, String::from_utf8_lossy(&out.stderr).into_owned())
}

// -----------------------------------------------------------------
// Fixture builders
// -----------------------------------------------------------------

/// US1 — multi-module Go workspace fixture: a `go.mod` at root plus
/// two nested go.mod files. Exercises FR-002 repo-root tiebreaker.
fn build_multi_module_go_workspace(dir: &Path) {
    fs::write(
        dir.join("go.mod"),
        "module example.com/otelshape\n\ngo 1.22\n",
    )
    .unwrap();
    fs::create_dir_all(dir.join("cmd/builder")).unwrap();
    fs::write(
        dir.join("cmd/builder/go.mod"),
        "module example.com/otelshape/cmd/builder\n\ngo 1.22\n",
    )
    .unwrap();
    fs::create_dir_all(dir.join("pkg/configprovider")).unwrap();
    fs::write(
        dir.join("pkg/configprovider/go.mod"),
        "module example.com/otelshape/pkg/configprovider\n\ngo 1.22\n",
    )
    .unwrap();
}

/// US2 — polyglot Go + Maven + npm fixture: `go.mod` at root,
/// `pom.xml` in `java-client/`, `package.json` in `ui/`. Exercises
/// FR-002 repo-root tiebreaker preferring Go over Maven/npm.
fn build_polyglot_go_maven_npm(dir: &Path) {
    fs::write(
        dir.join("go.mod"),
        "module example.com/polyglot\n\ngo 1.22\n",
    )
    .unwrap();
    fs::create_dir_all(dir.join("java-client")).unwrap();
    fs::write(
        dir.join("java-client/pom.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>example.com</groupId>
  <artifactId>polyglot-java-tests</artifactId>
  <version>0.0.0</version>
</project>
"#,
    )
    .unwrap();
    fs::create_dir_all(dir.join("ui")).unwrap();
    fs::write(
        dir.join("ui/package.json"),
        r#"{"name": "polyglot-ui", "version": "1.0.0"}"#,
    )
    .unwrap();
}

// -----------------------------------------------------------------
// Helpers to extract the root component PURL across formats
// -----------------------------------------------------------------

fn cdx_root_purl(json: &serde_json::Value) -> Option<String> {
    json.pointer("/metadata/component/purl")?
        .as_str()
        .map(|s| s.to_string())
}

fn cdx_metadata_properties(json: &serde_json::Value) -> Vec<(String, String)> {
    json.pointer("/metadata/properties")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    let name = p.get("name")?.as_str()?.to_string();
                    let value = p.get("value")?.as_str()?.to_string();
                    Some((name, value))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn spdx23_root_purl(json: &serde_json::Value) -> Option<String> {
    let root_id = json.pointer("/documentDescribes/0")?.as_str()?;
    let pkgs = json.get("packages")?.as_array()?;
    for p in pkgs {
        if p.get("SPDXID")?.as_str()? == root_id {
            let refs = p.get("externalRefs")?.as_array()?;
            for r in refs {
                if r.get("referenceType")?.as_str()? == "purl" {
                    return r.get("referenceLocator")?.as_str().map(|s| s.to_string());
                }
            }
        }
    }
    None
}

fn spdx3_root_purl(json: &serde_json::Value) -> Option<String> {
    let graph = json.get("@graph")?.as_array()?;
    let root_iri = graph
        .iter()
        .find(|e| {
            e.get("type").and_then(|v| v.as_str()) == Some("SpdxDocument")
        })?
        .pointer("/rootElement/0")?
        .as_str()?
        .to_string();
    graph
        .iter()
        .find(|e| e.get("spdxId").and_then(|v| v.as_str()) == Some(&root_iri))?
        .get("software_packageUrl")?
        .as_str()
        .map(|s| s.to_string())
}

// -----------------------------------------------------------------
// Tests
// -----------------------------------------------------------------

/// US1 — multi-module Go workspace: repo-root `go.mod` wins over
/// nested submodule `go.mod` files. SC-001 in spirit (otel-collector
/// shape, but synthesized).
#[test]
fn us1_multi_module_go_workspace_picks_repo_root() {
    let fake_home = tempfile::Builder::new()
        .prefix("mb127-home-")
        .tempdir()
        .unwrap();
    let workspace = tempfile::Builder::new()
        .prefix("mb127-ws-")
        .tempdir()
        .unwrap();
    build_multi_module_go_workspace(workspace.path());

    let (cdx, _stderr) = run_scan_returning_json(
        fake_home.path(),
        workspace.path(),
        &[],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let root_purl =
        cdx_root_purl(&cdx).expect("CDX metadata.component.purl present");
    assert_eq!(
        root_purl, "pkg:golang/example.com/otelshape@v0.0.0-unknown",
        "CDX root should be repo-root go.mod's module, got {root_purl}"
    );

    let (spdx2, _) = run_scan_returning_json(
        fake_home.path(),
        workspace.path(),
        &[],
        "spdx-2.3-json",
        "out.spdx.json",
    );
    let spdx2_root = spdx23_root_purl(&spdx2).expect("SPDX 2.3 root PURL");
    assert_eq!(spdx2_root, root_purl, "SPDX 2.3 root must match CDX");

    let (spdx3, _) = run_scan_returning_json(
        fake_home.path(),
        workspace.path(),
        &[],
        "spdx-3-json",
        "out.spdx3.json",
    );
    let spdx3_root = spdx3_root_purl(&spdx3).expect("SPDX 3 root PURL");
    assert_eq!(spdx3_root, root_purl, "SPDX 3 root must match CDX");
}

/// US1 + US3 — multi-module Go workspace MUST also emit the
/// `waybill:root-selection-heuristic` annotation (since FR-002
/// repo-root tiebreaker fired with losers > 0).
#[test]
fn us1_multi_module_emits_heuristic_annotation() {
    let fake_home = tempfile::Builder::new()
        .prefix("mb127-home-")
        .tempdir()
        .unwrap();
    let workspace = tempfile::Builder::new()
        .prefix("mb127-ws-")
        .tempdir()
        .unwrap();
    build_multi_module_go_workspace(workspace.path());

    let (cdx, stderr) = run_scan_returning_json(
        fake_home.path(),
        workspace.path(),
        &[],
        "cyclonedx-json",
        "out.cdx.json",
    );

    let props = cdx_metadata_properties(&cdx);
    let heuristic_prop = props
        .iter()
        .find(|(name, _)| name == "waybill:root-selection-heuristic");
    let (_, value_json) =
        heuristic_prop.expect("heuristic annotation present in CDX properties");

    let envelope: serde_json::Value =
        serde_json::from_str(value_json).expect("envelope is JSON");
    assert_eq!(
        envelope.pointer("/value/heuristic").and_then(|v| v.as_str()),
        Some("repo-root-main-module"),
        "envelope = {envelope}"
    );
    assert_eq!(
        envelope.pointer("/value/confidence").and_then(|v| v.as_f64()),
        Some(0.95),
        "envelope = {envelope}"
    );

    // FR-007 warning fires in stderr.
    assert!(
        stderr.contains("repo-root-main-module"),
        "stderr should carry the FR-007 warning naming the heuristic; stderr = {stderr}"
    );
}

/// US2 — polyglot Go + Maven + npm: the Go main-module at the repo
/// root wins, NOT the Maven `scan_target_coord` or the npm
/// `package.json`. SC-002 in spirit (argo-workflows shape).
#[test]
fn us2_polyglot_picks_go_main_module() {
    let fake_home = tempfile::Builder::new()
        .prefix("mb127-home-")
        .tempdir()
        .unwrap();
    let workspace = tempfile::Builder::new()
        .prefix("mb127-ws-")
        .tempdir()
        .unwrap();
    build_polyglot_go_maven_npm(workspace.path());

    let (cdx, _) = run_scan_returning_json(
        fake_home.path(),
        workspace.path(),
        &[],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let root_purl =
        cdx_root_purl(&cdx).expect("CDX metadata.component.purl present");
    assert_eq!(
        root_purl, "pkg:golang/example.com/polyglot@v0.0.0-unknown",
        "CDX root should be the Go module; got {root_purl}"
    );
}

/// SC-006 — operator override (`--root-name` etc.) wins over every
/// new heuristic. Verified on the polyglot fixture (which would
/// otherwise have count > 1 main-modules and the heuristic would
/// fire) with `--root-name`/`--root-version`. The override produces
/// the synthesized PURL, NO heuristic annotation is emitted, and the
/// stderr carries no FR-007 warning.
#[test]
fn sc006_override_wins_over_heuristic() {
    let fake_home = tempfile::Builder::new()
        .prefix("mb127-home-")
        .tempdir()
        .unwrap();
    let workspace = tempfile::Builder::new()
        .prefix("mb127-ws-")
        .tempdir()
        .unwrap();
    build_polyglot_go_maven_npm(workspace.path());

    let (cdx, stderr) = run_scan_returning_json(
        fake_home.path(),
        workspace.path(),
        &["--root-name", "polyglot-overridden", "--root-version", "9.9.9"],
        "cyclonedx-json",
        "out.cdx.json",
    );

    let root_purl =
        cdx_root_purl(&cdx).expect("CDX metadata.component.purl present");
    assert_eq!(
        root_purl, "pkg:generic/polyglot-overridden@9.9.9",
        "override PURL should win; got {root_purl}"
    );

    let props = cdx_metadata_properties(&cdx);
    assert!(
        !props
            .iter()
            .any(|(name, _)| name == "waybill:root-selection-heuristic"),
        "operator override suppresses the heuristic annotation"
    );

    assert!(
        !stderr.contains("root-selection-heuristic")
            && !stderr.contains("operator override recommended"),
        "operator override suppresses the FR-007 warning; stderr = {stderr}"
    );
}

