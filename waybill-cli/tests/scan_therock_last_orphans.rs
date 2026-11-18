//! Spec 1543 (#1601, #1602): TheRock's last two unexplained components.
//!
//! Contract `specs/1543-therock-last-orphans/contracts/wire.md`.
//! - **W-1..W-8:** a synthesized workspace root (uv, bun, Kotlin DSL) is
//!   reachable wherever it is not the subject, and is the subject at the
//!   scan root.
//! - **L-1..L-12:** a vendored installed-library record linked by link
//!   directory is a dependency of the project that links it, carrying the
//!   conditions it is linked under.
//!
//! Every case is checked in the three formats and repeated [`SCANS`] times.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::collections::BTreeMap;
use std::path::Path;

use common::installed_lib::{component, depends_on, prop, row_all_formats, scan_with, write, Docs};
use corpus_harness_195::invariants::checks::cdx_component_ref;
use corpus_harness_195::invariants::checks::orphan_explained::{
    cdx_components_with_containers, is_explained, reachable_with_containment,
};
use corpus_harness_195::layer2_golden::{mask_nondeterministic, masked_difference};
use serde_json::Value;

const SCANS: usize = 20;
const OP: &[&str] = &["--root-name", "waybill-fixture-op", "--root-version", "1"];

// ---------------------------------------------------------------------
// Helpers (T001)
// ---------------------------------------------------------------------

fn scan(root: &Path, extra: &[&str]) -> Docs {
    scan_with("--path", root, extra)
}

fn subject(d: &Docs) -> String {
    d.cdx["metadata"]["component"]["bom-ref"].as_str().unwrap_or_default().to_string()
}

/// I7 in one scan: every component the subject cannot reach (through
/// dependencies or containment) is explained, and none that it reaches
/// carries an orphan reason. The formats agree on C45, C104 and C105.
fn assert_explained(d: &Docs) {
    let reachable = reachable_with_containment(&d.cdx);
    let subject = subject(d);
    for (c, _) in cdx_components_with_containers(&d.cdx) {
        let Some(r) = cdx_component_ref(c) else { continue };
        if r == subject {
            continue;
        }
        if reachable.contains(r) {
            assert_eq!(prop(c, "waybill:orphan-reason"), None, "reachable {r} carries C45");
        } else {
            assert!(is_explained(c), "unreachable {r} is unexplained");
        }
    }
    for row in ["C45", "C104", "C105"] {
        row_all_formats(d, row);
    }
}

/// The refs `bom_ref` depends on. `depends_on` finds only components, and
/// the subject is in `metadata`.
fn targets_of(d: &Docs, bom_ref: &str) -> Vec<String> {
    d.cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| e["ref"] == bom_ref)
        .and_then(|e| e["dependsOn"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|t| t.as_str().map(str::to_string))
        .collect()
}

fn reachable(d: &Docs, purl: &str) -> bool {
    reachable_with_containment(&d.cdx).contains(purl)
}

/// [`SCANS`] scans of `root` give one masked document per format.
fn assert_repeatable(root: &Path, extra: &[&str]) {
    let docs: Vec<Docs> = (0..SCANS).map(|_| scan(root, extra)).collect();
    for (f, name) in ["CycloneDX", "SPDX 2.3", "SPDX 3"].iter().enumerate() {
        let mut variants: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, d) in docs.iter().enumerate() {
            let v = [&d.cdx, &d.spdx23, &d.spdx3][f];
            variants.entry(serde_json::to_string_pretty(&mask_nondeterministic(v)).unwrap()).or_default().push(i);
        }
        if variants.len() > 1 {
            let parsed: Vec<Value> = variants.keys().map(|s| serde_json::from_str(s).unwrap()).collect();
            panic!(
                "{name}: {} variants across {SCANS} scans: {:?}; {}",
                variants.len(),
                variants.values().collect::<Vec<_>>(),
                masked_difference(&parsed[0], &parsed[1]).unwrap_or_default()
            );
        }
    }
}

// ---------------------------------------------------------------------
// Workspace fixtures (T002)
// ---------------------------------------------------------------------

const UV_WS: &str = "pkg:generic/waybill-fixture-tensile";
const UV_MEMBERS: [&str; 2] = ["pkg:pypi/waybill-fixture-rocisa@0.1.0", "pkg:pypi/waybill-fixture-tensile@5.0.0"];
const BUN_WS: &str = "pkg:generic/waybill-fixture-mono";
const KTS_WS: &str = "pkg:generic/waybill-fixture-kts@0.0.0";

fn uv_workspace(root: &Path, at: &str) {
    write(root, &format!("{at}pyproject.toml"), "[project]\nname = \"waybill-fixture-tensile\"\nversion = \"5.0.0\"\n\n[tool.uv.workspace]\nmembers = [\"rocisa\"]\n");
    write(root, &format!("{at}rocisa/pyproject.toml"), "[project]\nname = \"waybill-fixture-rocisa\"\nversion = \"0.1.0\"\n");
    write(
        root,
        &format!("{at}uv.lock"),
        "version = 1\n\n[manifest]\nmembers = [\"waybill-fixture-rocisa\", \"waybill-fixture-tensile\"]\n\n\
         [[package]]\nname = \"waybill-fixture-rocisa\"\nversion = \"0.1.0\"\nsource = { editable = \"rocisa\" }\n\n\
         [[package]]\nname = \"waybill-fixture-tensile\"\nversion = \"5.0.0\"\nsource = { editable = \".\" }\n\
         dependencies = [{ name = \"waybill-fixture-rocisa\" }]\n",
    );
}

fn bun_workspace(root: &Path, at: &str) {
    write(root, &format!("{at}package.json"), r#"{"name":"waybill-fixture-mono","version":"1.0.0","workspaces":["packages/*"]}"#);
    write(root, &format!("{at}packages/a/package.json"), r#"{"name":"waybill-fixture-a","version":"1.0.0"}"#);
    write(
        root,
        &format!("{at}bun.lock"),
        "{\n  \"lockfileVersion\": 1,\n  \"workspaces\": {\n    \"\": { \"name\": \"waybill-fixture-mono\" },\n    \
         \"packages/a\": { \"name\": \"waybill-fixture-a\", \"version\": \"1.0.0\" }\n  },\n  \"packages\": {}\n}\n",
    );
}

fn kts_workspace(root: &Path, at: &str) {
    write(root, &format!("{at}settings.gradle.kts"), "rootProject.name = \"waybill-fixture-kts\"\ninclude(\":app\")\n");
    write(
        root,
        &format!("{at}app/build.gradle.kts"),
        "plugins { kotlin(\"jvm\") }\ndependencies {\n    implementation(\"org.waybill-fixture:lib:1.0.0\")\n}\n",
    );
}

/// The workspace in `proj/`, under a Cargo main module at the scan root.
fn in_subproject(build: fn(&Path, &str)) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path(), "proj/");
    write(dir.path(), "Cargo.toml", "[package]\nname = \"waybill-fixture-top\"\nversion = \"1.0.0\"\n");
    dir
}

fn at_root(build: fn(&Path, &str)) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path(), "");
    dir
}

/// The workspace root is reachable, carries no orphan reason, and reaches
/// every one of `members`; nothing is unexplained.
fn assert_attached(d: &Docs, ws: &str, members: &[&str]) {
    assert!(component(&d.cdx, ws).is_some(), "control: {ws} is emitted");
    assert!(reachable(d, ws), "{ws} is unreachable from {}", subject(d));
    for m in members {
        assert!(depends_on(&d.cdx, ws).iter().any(|t| t == m), "{ws} -> {m} is missing");
    }
    assert_explained(d);
}

// ---------------------------------------------------------------------
// W-1..W-3: a subproject's workspace under --root-name (T005)
// ---------------------------------------------------------------------

#[test]
fn w1_uv_subproject_workspace_root_is_attached() {
    let dir = in_subproject(uv_workspace);
    let d = scan(dir.path(), OP);
    assert_attached(&d, UV_WS, &UV_MEMBERS);
    let root_targets = targets_of(&d, &subject(&d));
    assert!(root_targets.iter().any(|t| t == UV_WS), "the root depends on the workspace root");
    for m in UV_MEMBERS {
        assert!(root_targets.iter().any(|t| t == m), "FR-003: the root's edge to member {m} is kept");
    }
    assert_repeatable(dir.path(), OP);
}

#[test]
fn w2_bun_subproject_workspace_root_is_attached() {
    let dir = in_subproject(bun_workspace);
    let d = scan(dir.path(), OP);
    assert_attached(&d, BUN_WS, &["pkg:npm/waybill-fixture-a@1.0.0"]);
    assert_repeatable(dir.path(), OP);
}

#[test]
fn w3_kotlin_subproject_workspace_root_is_attached() {
    let dir = in_subproject(kts_workspace);
    let d = scan(dir.path(), OP);
    assert_attached(&d, KTS_WS, &[]);
    assert_repeatable(dir.path(), OP);
}

// ---------------------------------------------------------------------
// W-4..W-6: a workspace at the scan root is the subject (T006)
// ---------------------------------------------------------------------

/// C69 is emitted only when the subject displaced a main module
/// (milestone 127 FR-006), so `displaced` says whether to expect it.
fn assert_subject_is_workspace_root(d: &Docs, ws: &str, displaced: bool) {
    assert_eq!(subject(d), ws);
    let heuristic = row_all_formats(d, "C69");
    if displaced {
        assert!(
            heuristic.iter().any(|h| h.contains("workspace-root")),
            "C69 names the workspace-root rung: {heuristic:?}"
        );
    } else {
        assert!(heuristic.is_empty(), "no main module lost, so no C69: {heuristic:?}");
    }
    assert_explained(d);
}

#[test]
fn w4_uv_workspace_at_the_root_is_the_subject() {
    let dir = at_root(uv_workspace);
    let d = scan(dir.path(), &[]);
    assert_subject_is_workspace_root(&d, UV_WS, true);
    for m in UV_MEMBERS {
        assert!(reachable(&d, m), "member {m} is reached through the workspace root");
    }
    assert_repeatable(dir.path(), &[]);
}

#[test]
fn w5_bun_workspace_at_the_root_is_the_subject() {
    let dir = at_root(bun_workspace);
    let d = scan(dir.path(), &[]);
    assert_subject_is_workspace_root(&d, BUN_WS, true);
    assert!(reachable(&d, "pkg:npm/waybill-fixture-a@1.0.0"));
    assert_repeatable(dir.path(), &[]);
}

#[test]
fn w6_kotlin_workspace_at_the_root_is_the_subject() {
    let dir = at_root(kts_workspace);
    let d = scan(dir.path(), &[]);
    assert_subject_is_workspace_root(&d, KTS_WS, false);
    assert_repeatable(dir.path(), &[]);
}

// ---------------------------------------------------------------------
// W-7, W-8 (T007)
// ---------------------------------------------------------------------

/// W-7: two workspace roots at the scan root. Neither is promoted; both are
/// reachable from whatever the ladder picks.
#[test]
fn w7_two_workspace_roots_at_the_root_are_both_anchored() {
    let dir = tempfile::tempdir().unwrap();
    uv_workspace(dir.path(), "");
    bun_workspace(dir.path(), "");
    let d = scan(dir.path(), &[]);
    let s = subject(&d);
    assert!(s != UV_WS && s != BUN_WS, "neither workspace root is promoted: {s}");
    assert!(reachable(&d, UV_WS) && reachable(&d, BUN_WS), "both are reachable from {s}");
    assert_explained(&d);
    assert_repeatable(dir.path(), &[]);
}

/// W-8: a workspace root something already depends on gets no root edge.
/// Kotlin's workspace root at a subdirectory is reached from the Cargo main
/// module through the fallback, which is open without --root-name.
#[test]
fn w8_a_targeted_workspace_root_gets_no_extra_edge() {
    let dir = in_subproject(kts_workspace);
    let d = scan(dir.path(), &[]);
    let s = subject(&d);
    let incoming: Vec<&str> = d.cdx["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["dependsOn"].as_array().into_iter().flatten().any(|t| t == KTS_WS))
        .map(|e| e["ref"].as_str().unwrap())
        .collect();
    assert_eq!(incoming, [s.as_str()], "exactly one edge into the workspace root");
    assert_explained(&d);
    assert_repeatable(dir.path(), &[]);
}
