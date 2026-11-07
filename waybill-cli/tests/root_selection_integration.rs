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

    // #1190: the CDX value is the bare `(heuristic, confidence)`
    // object. It was read here as `/value/heuristic` because m127's
    // contract described a `waybill-annotation/v1` envelope, which is
    // the SPDX-side shape — `build_annotation` adds it there because an
    // SPDX comment has no name slot. A CDX `properties[]` entry has
    // `name`, and no other CDX property is enveloped.
    let value: serde_json::Value =
        serde_json::from_str(value_json).expect("value is JSON");
    assert_eq!(
        value.pointer("/heuristic").and_then(|v| v.as_str()),
        Some("repo-root-main-module"),
        "value = {value}"
    );
    assert!(
        value.get("schema").is_none() && value.get("field").is_none(),
        "the CDX value must not be enveloped (#1190); value = {value}"
    );
    assert_eq!(
        value.pointer("/confidence").and_then(|v| v.as_f64()),
        Some(0.95),
        "value = {value}"
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


/// A cargo crate that is its own workspace (`[package]` + `[workspace]`)
/// marks itself a workspace root wherever it sits. Nested under a project
/// whose manifest IS at the scan root, it must not outrank that project:
/// "workspace root" means the manifest is at the scan root (m127 FR-001).
#[test]
fn nested_self_workspace_crate_does_not_outrank_the_scan_root_project() {
    let fake_home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"name": "waybill-fixture-rootapp", "version": "1.0.0"}"#,
    )
    .unwrap();
    let nested = dir.path().join("tools/gen");
    fs::create_dir_all(nested.join("src")).unwrap();
    fs::write(
        nested.join("Cargo.toml"),
        "[package]\nname = \"waybill-fixture-gen\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    fs::write(nested.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(
        nested.join("Cargo.lock"),
        "version = 3\n\n[[package]]\nname = \"waybill-fixture-gen\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let (json, _) = run_scan_returning_json(
        fake_home.path(),
        dir.path(),
        &[],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let root = cdx_root_purl(&json).unwrap_or_default();
    assert!(
        root.contains("waybill-fixture-rootapp"),
        "the project at the scan root must be the root, not a nested crate; got {root}"
    );
}

// -----------------------------------------------------------------
// Milestone 1376 (#1385) — C209's sum, against what each format emits
// -----------------------------------------------------------------

/// Recover the component total from a CycloneDX document the way a
/// consumer must (milestone 1376, #1385).
///
/// `.components | length` alone is **wrong**, and wrong in two directions
/// depending on the project's shape. A scan that finds exactly one main
/// module promotes it to `metadata.component`, where it is no longer in
/// `components[]` — so the naive count is short by one. But a synthetic
/// placeholder root is *not* a resolved component and must not be added,
/// and a root elected from several main modules stays in `components[]`
/// and must not be added twice.
///
/// Hence three clauses. A real root carries `waybill:*` properties; a
/// synthetic placeholder carries none.
fn cdx_component_total(doc: &serde_json::Value) -> usize {
    let comps = doc["components"].as_array().map_or(0, |a| a.len());
    let root = &doc["metadata"]["component"];
    let is_real = root["properties"]
        .as_array()
        .is_some_and(|a| !a.is_empty());
    let root_ref = root["bom-ref"].as_str();
    let already = doc["components"]
        .as_array()
        .is_some_and(|a| a.iter().any(|c| c["bom-ref"].as_str() == root_ref));
    comps + usize::from(is_real && !already)
}

fn c209_total(doc: &serde_json::Value) -> usize {
    let raw = doc["metadata"]["properties"]
        .as_array()
        .expect("metadata.properties")
        .iter()
        .find(|p| p["name"] == "waybill:cpe-coverage")
        .expect("C209 is emitted on every scan")["value"]
        .as_str()
        .expect("C209 value is a string")
        .to_string();
    let v: serde_json::Value = serde_json::from_str(&raw).expect("C209 is JSON");
    v.as_object()
        .expect("C209 is an object")
        .iter()
        .filter(|(k, _)| k.as_str() != "schema")
        .map(|(_, n)| n.as_u64().expect("counts are numbers") as usize)
        .sum()
}

/// One main module at the scan root → CDX **promotes** it out of
/// `components[]`, and the naive check is short by one.
#[test]
fn m1376_a_promoted_root_is_counted_by_the_documented_expression() {
    let home = tempfile::tempdir().unwrap();
    let proj = tempfile::tempdir().unwrap();
    fs::write(proj.path().join("go.mod"), "module example.com/solo\n\ngo 1.21\n").unwrap();
    fs::write(proj.path().join("main.go"), "package main\nfunc main(){}\n").unwrap();

    let (doc, _) = run_scan_returning_json(
        home.path(),
        proj.path(),
        &["--no-deps-dev", "--no-clearly-defined"],
        "cyclonedx-json",
        "out.cdx.json",
    );

    // CONTROL: this really is the promoted shape, or the assertions below
    // are about a case that never broke.
    let root = &doc["metadata"]["component"];
    assert!(
        root["properties"].as_array().is_some_and(|a| !a.is_empty()),
        "expected a REAL root; got {root}"
    );
    let naive = doc["components"].as_array().unwrap().len();
    let row = c209_total(&doc);
    assert_eq!(
        row,
        naive + 1,
        "expected the promoted shape, where the naive count is short by \
         exactly one: row={row} .components={naive}"
    );

    assert_eq!(cdx_component_total(&doc), row, "the documented expression must agree");
}

/// `--root-name` forces a synthetic placeholder, which is **not** a
/// resolved component. An unconditional `+1` would break this — and with
/// it all 17 corpus targets, every one of which passes `--root-name`.
#[test]
fn m1376_a_synthetic_root_is_not_counted() {
    let home = tempfile::tempdir().unwrap();
    let proj = tempfile::tempdir().unwrap();
    fs::write(proj.path().join("go.mod"), "module example.com/solo\n\ngo 1.21\n").unwrap();
    fs::write(proj.path().join("main.go"), "package main\nfunc main(){}\n").unwrap();

    let (doc, _) = run_scan_returning_json(
        home.path(),
        proj.path(),
        &["--no-deps-dev", "--no-clearly-defined", "--root-name", "waybill-fixture-harness"],
        "cyclonedx-json",
        "out.cdx.json",
    );

    let root = &doc["metadata"]["component"];
    assert!(
        root["properties"].as_array().is_none_or(|a| a.is_empty()),
        "expected a SYNTHETIC root carrying no waybill:* properties; got {root}"
    );
    assert_eq!(
        cdx_component_total(&doc),
        c209_total(&doc),
        "a synthetic root must contribute +0"
    );
}

/// Several main modules with one at the scan root → the milestone-127
/// ladder elects it, and it **stays** in `components[]`. Adding one here
/// would double-count.
///
/// The recipe is not obvious: a `go.work` using `./a` and `./b` yields a
/// *synthetic* root instead. One module must be at the scan root.
#[test]
fn m1376_a_duplicated_root_is_not_counted_twice() {
    let home = tempfile::tempdir().unwrap();
    let proj = tempfile::tempdir().unwrap();
    fs::create_dir_all(proj.path().join("sub")).unwrap();
    fs::write(proj.path().join("go.work"), "go 1.21\n\nuse (\n    .\n    ./sub\n)\n").unwrap();
    fs::write(proj.path().join("go.mod"), "module example.com/root\n\ngo 1.21\n").unwrap();
    fs::write(proj.path().join("main.go"), "package main\nfunc main(){}\n").unwrap();
    fs::write(proj.path().join("sub/go.mod"), "module example.com/root/sub\n\ngo 1.21\n").unwrap();
    fs::write(proj.path().join("sub/main.go"), "package main\nfunc main(){}\n").unwrap();

    let (doc, _) = run_scan_returning_json(
        home.path(),
        proj.path(),
        &["--no-deps-dev", "--no-clearly-defined"],
        "cyclonedx-json",
        "out.cdx.json",
    );

    // CONTROL: the root must be real AND already present, or this is not
    // the duplicated shape and the +0 below proves nothing.
    let root = &doc["metadata"]["component"];
    let root_ref = root["bom-ref"].as_str();
    assert!(
        root["properties"].as_array().is_some_and(|a| !a.is_empty()),
        "expected a REAL root; got {root}"
    );
    assert!(
        doc["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["bom-ref"].as_str() == root_ref),
        "expected the elected root to ALSO be in components[]; got {root}"
    );

    let naive = doc["components"].as_array().unwrap().len();
    assert_eq!(c209_total(&doc), naive, "duplicated root contributes +0");
    assert_eq!(cdx_component_total(&doc), c209_total(&doc));
}

/// SPDX 2.3 counts packages; SPDX 3 counts Packages **plus Files**,
/// because file-tier entries are `File` nodes there. That is the second,
/// independent reason the naive CDX expression does not generalise.
#[test]
fn m1376_the_spdx_expressions_agree_with_the_same_row() {
    let home = tempfile::tempdir().unwrap();
    let proj = tempfile::tempdir().unwrap();
    fs::write(proj.path().join("go.mod"), "module example.com/solo\n\ngo 1.21\n").unwrap();
    fs::write(proj.path().join("main.go"), "package main\nfunc main(){}\n").unwrap();

    let (cdx, _) = run_scan_returning_json(
        home.path(), proj.path(),
        &["--no-deps-dev", "--no-clearly-defined"],
        "cyclonedx-json", "out.cdx.json",
    );
    let row = c209_total(&cdx);

    let (s23, _) = run_scan_returning_json(
        home.path(), proj.path(),
        &["--no-deps-dev", "--no-clearly-defined"],
        "spdx-2.3-json", "out.spdx.json",
    );
    assert_eq!(
        s23["packages"].as_array().map_or(0, |a| a.len()),
        row,
        "SPDX 2.3: len(packages[]) must equal the row"
    );

    let (s3, _) = run_scan_returning_json(
        home.path(), proj.path(),
        &["--no-deps-dev", "--no-clearly-defined"],
        "spdx-3-json", "out.spdx3.json",
    );
    let graph = s3["@graph"].as_array().expect("SPDX 3 @graph");
    let pkgs = graph.iter().filter(|n| n["type"].as_str().is_some_and(|t| t.contains("Package"))).count();
    let files = graph.iter().filter(|n| n["type"].as_str().is_some_and(|t| t.contains("File"))).count();
    assert_eq!(pkgs + files, row, "SPDX 3: Package + File nodes must equal the row");
}
