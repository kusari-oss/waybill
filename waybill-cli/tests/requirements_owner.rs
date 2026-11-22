//! Spec 1642 (#1640): a requirements file belongs to the project in its
//! directory, not to a lockfile entry beside it; and a package the root
//! requires directly is never reported as not shipping.
//!
//! Contract `specs/1642-requirements-owner-editable/contracts/wire.md`.
//! Every case builds a synthetic `waybill-fixture-*` tree as a temporary
//! git repository and scans it in all three formats. Each case checks:
//! - the stated edges, scope, C18 (`waybill:source-files`) and C120
//!   (`waybill:workspace-member`);
//! - parity of A1, C18, C42, C62 and C120 across the formats;
//! - CycloneDX validity and unique `bom-ref`s;
//! - 10 byte-identical scans.
//!
//! `WAYBILL_REQUIREMENTS_OWNER_BIN` runs a case against another binary,
//! such as `main`'s, for the controls (`measurements/control.md`) and for
//! B-1's golden (`WAYBILL_UPDATE_REQUIREMENTS_OWNER_GOLDENS=1`).

#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use common::git_fixture::GitFixture;
use corpus_harness_195::layer2_golden::{mask_nondeterministic, masked_difference};
use serde_json::Value;

const REPEATS: usize = 10;
const ROWS: &[&str] = &["A1", "C18", "C42", "C62", "C120"];

const SHARED: &str = "pkg:pypi/waybill-fixture-shared";
const TOOL: &str = "pkg:pypi/waybill-fixture-tool@1.0.0";

// ---------------------------------------------------------------------
// Harness (T001)
// ---------------------------------------------------------------------

struct Scan {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
}

fn binary() -> String {
    std::env::var("WAYBILL_REQUIREMENTS_OWNER_BIN").unwrap_or_else(|_| common::bin().to_string())
}

/// A git repository `tree` holding `files`, committed.
fn tree(files: &[(&str, &str)]) -> GitFixture {
    let fx = GitFixture::new();
    fx.repo("tree");
    for (rel, text) in files {
        fx.write(&format!("tree/{rel}"), text);
    }
    commit_all(&fx, "tree");
    fx
}

fn commit_all(fx: &GitFixture, rel: &str) {
    let dir = fx.path(rel);
    fx.git(&dir, &["add", "-A"]);
    fx.git(&dir, &["commit", "-q", "--allow-empty", "-m", "files"]);
}

fn scan3(fx: &GitFixture, path: &Path) -> Scan {
    let out_dir = tempfile::tempdir().unwrap();
    let files = [
        out_dir.path().join("o.cdx.json"),
        out_dir.path().join("o.spdx.json"),
        out_dir.path().join("o.spdx3.json"),
    ];
    let out = Command::new(binary())
        .env("HOME", fx.home())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", fx.home().join(".gitconfig"))
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .env("RUST_LOG", "warn")
        .env("NO_COLOR", "1")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(path)
        .args(["--no-deep-hash", "--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", files[0].display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", files[1].display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", files[2].display()))
        .output()
        .unwrap();
    assert!(out.status.success(), "scan failed: {}", String::from_utf8_lossy(&out.stderr));
    let read = |p: &Path| serde_json::from_slice::<Value>(&std::fs::read(p).unwrap()).unwrap();
    Scan { cdx: read(&files[0]), spdx23: read(&files[1]), spdx3: read(&files[2]) }
}

/// Every component in `components`, recursively.
fn cdx_components(cdx: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    let mut stack: Vec<&Value> = cdx["components"].as_array().into_iter().flatten().collect();
    while let Some(c) = stack.pop() {
        out.push(c);
        stack.extend(c["components"].as_array().into_iter().flatten());
    }
    out
}

/// [`cdx_components`] plus `metadata.component`, one per `bom-ref`. A main
/// module chosen as root is listed in both places (#1633, on `main` too).
fn all_components(cdx: &Value) -> Vec<&Value> {
    let mut seen = BTreeSet::new();
    std::iter::once(&cdx["metadata"]["component"])
        .chain(cdx_components(cdx))
        .filter(|c| c["bom-ref"].as_str().is_some_and(|r| seen.insert(r.to_string())))
        .collect()
}

/// `purl` names the package `prefix` (a versioned prefix ending in `@`, or
/// a bare `pkg:type/name` matching any version or none).
fn names(purl: &str, prefix: &str) -> bool {
    if prefix.ends_with('@') {
        return purl.starts_with(prefix);
    }
    let bare = strip(purl);
    bare == prefix || bare.starts_with(&format!("{prefix}@"))
}

fn strip(purl: &str) -> String {
    purl.split('?').next().unwrap_or(purl).to_string()
}

fn prop<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c["properties"].as_array()?.iter().find(|p| p["name"] == name)?["value"].as_str()
}

/// The one component (metadata component included) whose PURL starts with
/// `prefix`.
fn one<'a>(s: &'a Scan, prefix: &str) -> &'a Value {
    let all: Vec<&Value> = all_components(&s.cdx)
        .into_iter()
        .filter(|c| c["purl"].as_str().is_some_and(|p| names(p, prefix)))
        .collect();
    assert_eq!(all.len(), 1, "expected one component {prefix}: {:?}", all.iter().map(|c| &c["purl"]).collect::<Vec<_>>());
    all[0]
}

/// The PURL (or bom-ref, for a component without one) of every component
/// with a CycloneDX dependency edge to the component `prefix` names.
fn edge_sources(s: &Scan, prefix: &str) -> BTreeSet<String> {
    let by_ref: BTreeMap<&str, String> = all_components(&s.cdx)
        .into_iter()
        .filter_map(|c| {
            let r = c["bom-ref"].as_str()?;
            Some((r, c["purl"].as_str().map(strip).unwrap_or_else(|| r.to_string())))
        })
        .collect();
    let target = one(s, prefix)["bom-ref"].as_str().unwrap().to_string();
    s.cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["dependsOn"].as_array().is_some_and(|a| a.iter().any(|t| t == target.as_str())))
        .filter_map(|d| d["ref"].as_str())
        .map(|r| by_ref.get(r).cloned().unwrap_or_else(|| r.to_string()))
        .collect()
}

/// CycloneDX `scope`, `required` when absent.
fn scope_of(s: &Scan, prefix: &str) -> String {
    one(s, prefix)["scope"].as_str().unwrap_or("required").to_string()
}

fn json_list(c: &Value, name: &str) -> Option<Vec<String>> {
    prop(c, name).map(|v| serde_json::from_str(v).unwrap())
}

/// C18 as a set: spec 1542 sorts a merged component's paths, so their
/// order carries no meaning.
fn source_files(s: &Scan, prefix: &str) -> BTreeSet<String> {
    json_list(one(s, prefix), "waybill:source-files").unwrap_or_default().into_iter().collect()
}

/// C120: the workspace root of every source path, as a set.
fn workspace_member(s: &Scan, prefix: &str) -> BTreeSet<String> {
    json_list(one(s, prefix), "waybill:workspace-member").unwrap_or_default().into_iter().collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn parity(s: &Scan) {
    for row in ROWS {
        let e = waybill::parity::extractors::EXTRACTORS
            .iter()
            .find(|e| e.row_id == *row)
            .unwrap_or_else(|| panic!("no extractor for {row}"));
        let (a, b, c) = ((e.cdx)(&s.cdx), (e.spdx23)(&s.spdx23), (e.spdx3)(&s.spdx3));
        assert_eq!(a, b, "{row}: CycloneDX and SPDX 2.3 disagree");
        assert_eq!(a, c, "{row}: CycloneDX and SPDX 3 disagree");
    }
    let errors = common::cdx_schema::cdx_validation_errors(&s.cdx);
    assert!(errors.is_empty(), "CycloneDX schema errors: {errors:?}");
    let refs: Vec<&str> = cdx_components(&s.cdx).into_iter().filter_map(|c| c["bom-ref"].as_str()).collect();
    let mut seen = BTreeSet::new();
    let dups: Vec<&str> = refs.iter().copied().filter(|r| !seen.insert(*r)).collect();
    assert!(dups.is_empty(), "duplicate bom-refs: {dups:?}");
}

fn masked(v: &Value) -> Value {
    let s = serde_json::to_string(&mask_nondeterministic(v)).unwrap();
    let re = regex::Regex::new(r#"[^"]*?[/\\]tree(?:[/\\"])"#).unwrap();
    serde_json::from_str(&re.replace_all(&s, |c: &regex::Captures| {
        let m = c.get(0).unwrap().as_str();
        format!("/TMP/tree{}", &m[m.len() - 1..])
    }))
    .unwrap()
}

fn assert_repeatable(fx: &GitFixture, path: &Path, first: &Scan) {
    for _ in 1..REPEATS {
        let again = scan3(fx, path);
        for (name, a, b) in [("cdx", &first.cdx, &again.cdx), ("spdx23", &first.spdx23, &again.spdx23), ("spdx3", &first.spdx3, &again.spdx3)] {
            let (a, b) = (masked(a), masked(b));
            assert!(a == b, "{name}: two scans differ; {}", masked_difference(&a, &b).unwrap_or_default());
        }
    }
}

/// Scan `tree` and run every per-case check.
fn check(fx: &GitFixture) -> Scan {
    let path = fx.path("tree");
    let s = scan3(fx, &path);
    parity(&s);
    assert_repeatable(fx, &path, &s);
    s
}

// ---------------------------------------------------------------------
// Trees, as `measurements/probe.sh` writes them
// ---------------------------------------------------------------------

fn uv_lock_1640(dev: &str, tool_source: &str) -> String {
    format!(
        "version = 1\nrequires-python = \">=3.10\"\n\n[[package]]\nname = \"{dev}\"\nversion = \"0.1.0\"\nsource = {{ editable = \"devtool\" }}\n\n[[package]]\nname = \"waybill-fixture-tool\"\nversion = \"1.0.0\"\nsource = {{ {tool_source} = \".\" }}\n\n[package.dev-dependencies]\ndev = [\n    {{ name = \"{dev}\" }},\n]\n"
    )
}

/// #1640's tree with dev package `dev`.
fn files_1640(dev: &str) -> Vec<(String, String)> {
    vec![
        ("requirements.txt".into(), "waybill-fixture-shared>=1.0\n".into()),
        ("tool/requirements.txt".into(), "waybill-fixture-shared>=1.0\n".into()),
        (
            "tool/pyproject.toml".into(),
            format!("[project]\nname = \"waybill-fixture-tool\"\nversion = \"1.0.0\"\ndependencies = []\n[dependency-groups]\ndev = [\"{dev}\"]\n[tool.uv.sources]\n{dev} = {{ path = \"devtool\", editable = true }}\n"),
        ),
        ("tool/devtool/pyproject.toml".into(), format!("[project]\nname = \"{dev}\"\nversion = \"0.1.0\"\n")),
        ("tool/uv.lock".into(), uv_lock_1640(dev, "editable")),
    ]
}

fn build(files: &[(String, String)]) -> GitFixture {
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    tree(&refs)
}

const R4_LOCK: &str = "version = 1\nrequires-python = \">=3.10\"\n\n[[package]]\nname = \"waybill-fixture-app\"\nversion = \"1.0.0\"\nsource = { editable = \".\" }\n\n[package.dev-dependencies]\ndev = [\n    { name = \"waybill-fixture-devlib\" },\n]\n\n[[package]]\nname = \"waybill-fixture-devlib\"\nversion = \"1.0\"\nsource = { registry = \"https://pypi.org/simple\" }\ndependencies = [\n    { name = \"waybill-fixture-shared\" },\n]\n\n[[package]]\nname = \"waybill-fixture-shared\"\nversion = \"2.0\"\nsource = { registry = \"https://pypi.org/simple\" }\n";
const R4_PYPROJECT: &str = "[project]\nname = \"waybill-fixture-app\"\nversion = \"1.0.0\"\ndependencies = []\n[dependency-groups]\ndev = [\"waybill-fixture-devlib\"]\n";

/// Probe R4: `app/` whose dev group reaches `waybill-fixture-shared`, plus
/// `root_req` (a requirements file name, or none) declaring it at the root.
fn files_r4(root_req: Option<&str>) -> Vec<(String, String)> {
    let mut f = vec![
        ("app/pyproject.toml".to_string(), R4_PYPROJECT.to_string()),
        ("app/uv.lock".to_string(), R4_LOCK.to_string()),
    ];
    if let Some(name) = root_req {
        f.push((name.to_string(), "waybill-fixture-shared==2.0\n".to_string()));
    }
    f
}

fn owned(files: Vec<(String, String)>, extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut f = files;
    f.extend(extra.iter().map(|(a, b)| (a.to_string(), b.to_string())));
    f
}

// ---------------------------------------------------------------------
// User Story 1: ownership (T006)
// ---------------------------------------------------------------------

/// O-1: `tool/requirements.txt` belongs to the tool, not the dev package.
#[test]
fn o1_requirements_attach_to_the_project_not_the_dev_package() {
    let s = check(&build(&files_1640("waybill-fixture-devtool")));
    let from = edge_sources(&s, SHARED);
    assert!(from.contains(TOOL), "no edge from the tool: {from:?}");
    assert!(!from.contains("pkg:pypi/waybill-fixture-devtool@0.1.0"), "edge from the dev package: {from:?}");
}

/// O-2: the same with the dev package's name sorting after the tool's.
#[test]
fn o2_the_owner_does_not_depend_on_name_order() {
    let s = check(&build(&files_1640("waybill-fixture-zdevtool")));
    let from = edge_sources(&s, SHARED);
    assert!(from.contains(TOOL), "no edge from the tool: {from:?}");
    assert!(!from.contains("pkg:pypi/waybill-fixture-zdevtool@0.1.0"), "edge from the dev package: {from:?}");
}

/// O-3: a lockfile-merged main module records its manifest's directory.
#[test]
fn o3_a_merged_main_module_keeps_its_own_directory() {
    let s = check(&build(&files_1640("waybill-fixture-devtool")));
    let dev = "pkg:pypi/waybill-fixture-devtool@0.1.0";
    assert_eq!(source_files(&s, dev), set(&["tool/devtool", "tool/uv.lock"]));
    assert_eq!(workspace_member(&s, dev), set(&["tool", "tool/devtool"]));
    assert_eq!(source_files(&s, TOOL), set(&["tool", "tool/uv.lock"]));
    assert_eq!(workspace_member(&s, TOOL), set(&["tool"]));
}

/// O-4: a requirements file in the dev package's own directory is its.
#[test]
fn o4_a_project_owns_its_own_directory() {
    let mut f = files_1640("waybill-fixture-zdevtool");
    f.push(("tool/devtool/requirements.txt".into(), "waybill-fixture-other==1.0\n".into()));
    let s = check(&build(&f));
    assert_eq!(edge_sources(&s, "pkg:pypi/waybill-fixture-other@"), BTreeSet::from(["pkg:pypi/waybill-fixture-zdevtool@0.1.0".to_string()]));
}

/// O-5: without the lockfile nothing changes (a guard).
#[test]
fn o5_without_a_lockfile_the_tool_owns_its_directory() {
    let f: Vec<(String, String)> = files_1640("waybill-fixture-devtool").into_iter().filter(|(p, _)| p != "tool/uv.lock").collect();
    let s = check(&build(&f));
    let from = edge_sources(&s, SHARED);
    assert!(from.contains(TOOL) && !from.contains("pkg:pypi/waybill-fixture-devtool@0.1.0"), "{from:?}");
    assert_eq!(workspace_member(&s, "pkg:pypi/waybill-fixture-devtool@0.1.0"), set(&["tool/devtool"]));
}

const WS_LOCK: &str = "version = 1\nrequires-python = \">=3.10\"\n\n[manifest]\nmembers = [\"waybill-fixture-a\", \"waybill-fixture-b\", \"waybill-fixture-ws\"]\n\n[[package]]\nname = \"waybill-fixture-a\"\nversion = \"1.0.0\"\nsource = { editable = \"packages/a\" }\n\n[[package]]\nname = \"waybill-fixture-b\"\nversion = \"1.0.0\"\nsource = { editable = \"packages/b\" }\n\n[[package]]\nname = \"waybill-fixture-ws\"\nversion = \"1.0.0\"\nsource = { editable = \".\" }\ndependencies = [\n    { name = \"waybill-fixture-a\" },\n]\n";

/// O-6: in a uv workspace, each member owns its own directory.
#[test]
fn o6_uv_workspace_members_own_their_directories() {
    let f: Vec<(String, String)> = [
        ("pyproject.toml", "[project]\nname = \"waybill-fixture-ws\"\nversion = \"1.0.0\"\ndependencies = [\"waybill-fixture-a\"]\n[tool.uv.workspace]\nmembers = [\"packages/a\", \"packages/b\"]\n[tool.uv.sources]\nwaybill-fixture-a = { workspace = true }\n"),
        ("packages/a/pyproject.toml", "[project]\nname = \"waybill-fixture-a\"\nversion = \"1.0.0\"\n"),
        ("packages/b/pyproject.toml", "[project]\nname = \"waybill-fixture-b\"\nversion = \"1.0.0\"\n"),
        ("packages/b/requirements.txt", "waybill-fixture-r==1.0\n"),
        ("uv.lock", WS_LOCK),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    let s = check(&build(&f));
    for (m, dir) in [("pkg:pypi/waybill-fixture-a@1.0.0", "packages/a"), ("pkg:pypi/waybill-fixture-b@1.0.0", "packages/b")] {
        assert_eq!(prop(one(&s, m), "waybill:component-role"), Some("main-module"), "{m}");
        assert_eq!(workspace_member(&s, m), set(&[".", dir]), "{m}: its own directory and the lock's");
    }
    assert_eq!(edge_sources(&s, "pkg:pypi/waybill-fixture-r@"), BTreeSet::from(["pkg:pypi/waybill-fixture-b@1.0.0".to_string()]));
}

const POETRY_LOCK: &str = "[[package]]\nname = \"waybill-fixture-devtool\"\nversion = \"0.1.0\"\ndescription = \"\"\noptional = false\npython-versions = \"*\"\nfiles = []\ndevelop = true\n\n[package.source]\ntype = \"directory\"\nurl = \"devtool\"\n\n[metadata]\nlock-version = \"2.0\"\npython-versions = \"^3.10\"\ncontent-hash = \"0\"\n";

/// O-7: the poetry form of O-1.
#[test]
fn o7_poetry_path_dependency_does_not_own_the_project_directory() {
    let f: Vec<(String, String)> = [
        ("requirements.txt", "waybill-fixture-other==1.0\n"),
        ("tool/pyproject.toml", "[tool.poetry]\nname = \"waybill-fixture-tool\"\nversion = \"1.0.0\"\n[tool.poetry.dependencies]\npython = \"^3.10\"\n[tool.poetry.group.dev.dependencies]\nwaybill-fixture-devtool = { path = \"devtool\", develop = true }\n"),
        ("tool/devtool/pyproject.toml", "[project]\nname = \"waybill-fixture-devtool\"\nversion = \"0.1.0\"\n"),
        ("tool/requirements.txt", "waybill-fixture-shared==1.0\n"),
        ("tool/poetry.lock", POETRY_LOCK),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    let s = check(&build(&f));
    assert_eq!(edge_sources(&s, SHARED), BTreeSet::from([TOOL.to_string()]));
    assert!(source_files(&s, "pkg:pypi/waybill-fixture-devtool@0.1.0").contains("tool/devtool"));
}

/// O-8: an installed entry keeps its own path first (planning amendment).
#[test]
fn o8_an_installed_entry_keeps_its_path_first() {
    let di = ".venv/lib/python3.12/site-packages/waybill_fixture_app-1.0.0.dist-info";
    let f: Vec<(String, String)> = [
        ("pyproject.toml".to_string(), "[project]\nname = \"waybill-fixture-app\"\nversion = \"1.0.0\"\n".to_string()),
        (format!("{di}/METADATA"), "Metadata-Version: 2.1\nName: waybill-fixture-app\nVersion: 1.0.0\n".to_string()),
        (format!("{di}/RECORD"), "waybill_fixture_app-1.0.0.dist-info/METADATA,,\n".to_string()),
        (".venv/pyvenv.cfg".to_string(), "home = /usr/bin\n".to_string()),
    ]
    .into_iter()
    .collect();
    let s = check(&build(&f));
    let app = "pkg:pypi/waybill-fixture-app@1.0.0";
    assert_eq!(source_files(&s, app), set(&[&format!("{di}/METADATA")]));
    assert_eq!(workspace_member(&s, app), set(&[".venv"]));
    assert_eq!(prop(one(&s, app), "waybill:sbom-tier"), Some("deployed"));
}

/// O-10: a virtual project source behaves as an editable one.
#[test]
fn o10_a_virtual_project_source() {
    let f: Vec<(String, String)> = files_1640("waybill-fixture-devtool")
        .into_iter()
        .map(|(p, t)| if p == "tool/uv.lock" { (p, uv_lock_1640("waybill-fixture-devtool", "virtual")) } else { (p, t) })
        .collect();
    let s = check(&build(&f));
    let from = edge_sources(&s, SHARED);
    assert!(from.contains(TOOL) && !from.contains("pkg:pypi/waybill-fixture-devtool@0.1.0"), "{from:?}");
}

/// O-11, O-12: editable entries with no manifest in the tree own nothing.
#[test]
fn o11_o12_editable_entries_without_a_manifest_own_nothing() {
    let lock = format!(
        "{}\n[[package]]\nname = \"waybill-fixture-outside\"\nversion = \"1.0.0\"\nsource = {{ editable = \"../../outside\" }}\n\n[[package]]\nname = \"waybill-fixture-missing\"\nversion = \"1.0.0\"\nsource = {{ editable = \"missing\" }}\n",
        uv_lock_1640("waybill-fixture-devtool", "editable")
    );
    let f: Vec<(String, String)> = files_1640("waybill-fixture-devtool")
        .into_iter()
        .map(|(p, t)| if p == "tool/uv.lock" { (p, lock.clone()) } else { (p, t) })
        .collect();
    let s = check(&build(&f));
    for m in ["pkg:pypi/waybill-fixture-outside@1.0.0", "pkg:pypi/waybill-fixture-missing@1.0.0"] {
        assert_eq!(prop(one(&s, m), "waybill:component-role"), None, "{m} is not a main module");
    }
}

// ---------------------------------------------------------------------
// User Story 2: anchor immunity (T010)
// ---------------------------------------------------------------------

fn assert_runtime(s: &Scan, prefix: &str) {
    let c = one(s, prefix);
    assert_eq!(scope_of(s, prefix), "required", "{prefix}: {:?}", c["properties"]);
    assert_eq!(prop(c, "waybill:lifecycle-scope-derivation"), None, "{prefix}");
}

fn assert_transitive_optional(s: &Scan, prefix: &str) {
    let c = one(s, prefix);
    assert_eq!(scope_of(s, prefix), "excluded", "{prefix}");
    assert_eq!(prop(c, "waybill:lifecycle-scope"), Some("optional"), "{prefix}");
    assert_eq!(prop(c, "waybill:lifecycle-scope-derivation"), Some("transitive"), "{prefix}");
}

/// A-1: a root-declared package stays runtime though a dev path reaches it.
#[test]
fn a1_a_root_declared_package_is_runtime() {
    let s = check(&build(&files_r4(Some("requirements.txt"))));
    assert_runtime(&s, SHARED);
}

/// A-2: the same file owned by the project (passes on `main`).
#[test]
fn a2_a_project_declared_package_is_runtime() {
    let s = check(&build(&files_r4(Some("app/requirements.txt"))));
    assert_runtime(&s, SHARED);
}

/// A-3: a dev requirements file's anchor protects nothing.
#[test]
fn a3_a_dev_requirements_anchor_is_not_a_seed() {
    let s = check(&build(&files_r4(Some("requirements-dev.txt"))));
    assert_transitive_optional(&s, SHARED);
}

/// A-4: a top-level submodule's anchor protects what it declares.
#[test]
fn a4_a_submodule_anchor_protects_its_dependency() {
    let fx = GitFixture::new();
    fx.repo("x");
    fx.write("x/requirements.txt", "waybill-fixture-shared==2.0\n");
    commit_all(&fx, "x");
    fx.repo("tree");
    for (p, t) in files_r4(None) {
        fx.write(&format!("tree/{p}"), &t);
    }
    fx.write("tree/requirements.txt", "waybill-fixture-rootx==1.0\n");
    commit_all(&fx, "tree");
    fx.add_submodule("tree", "x", "third_party/x");
    let s = check(&fx);
    // The guard: the case tests something only if the submodule reaches the
    // package and a dev component does too.
    let from = edge_sources(&s, SHARED);
    assert!(from.iter().any(|p| p.starts_with("pkg:generic/x@")), "no edge from the submodule: {from:?}");
    assert!(from.contains("pkg:pypi/waybill-fixture-devlib@1.0"), "no edge from the dev package: {from:?}");
    assert_runtime(&s, SHARED);
}

/// A-5: a package reached only through a dev path stays non-runtime.
#[test]
fn a5_a_dev_only_package_stays_non_runtime() {
    let s = check(&build(&files_r4(None)));
    assert_transitive_optional(&s, SHARED);
}

const NPM_LOCK: &str = r#"{
  "name": "waybill-fixture-web",
  "version": "1.0.0",
  "lockfileVersion": 3,
  "requires": true,
  "packages": {
    "": {
      "name": "waybill-fixture-web",
      "version": "1.0.0",
      "dependencies": { "waybill-fixture-y": "1.0.0" },
      "devDependencies": { "waybill-fixture-z": "1.0.0" }
    },
    "node_modules/waybill-fixture-y": { "version": "1.0.0" },
    "node_modules/waybill-fixture-z": {
      "version": "1.0.0",
      "dev": true,
      "dependencies": { "waybill-fixture-y": "1.0.0" }
    }
  }
}
"#;

/// A-6: a runtime main module of another ecosystem protects its direct
/// dependency (spec 1616; passes on `main`).
#[test]
fn a6_a_runtime_main_modules_direct_dependency_is_runtime() {
    let f: Vec<(String, String)> = [
        ("web/package.json", "{\"name\":\"waybill-fixture-web\",\"version\":\"1.0.0\",\"dependencies\":{\"waybill-fixture-y\":\"1.0.0\"},\"devDependencies\":{\"waybill-fixture-z\":\"1.0.0\"}}\n"),
        ("web/package-lock.json", NPM_LOCK),
        ("requirements.txt", "waybill-fixture-rootx==1.0\n"),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    let s = check(&build(&f));
    let from = edge_sources(&s, "pkg:npm/waybill-fixture-y@");
    assert!(from.contains("pkg:npm/waybill-fixture-z@1.0.0"), "no edge from the dev package: {from:?}");
    assert_runtime(&s, "pkg:npm/waybill-fixture-y@");
}

// ---------------------------------------------------------------------
// User Story 3 / FR-007: B-1 is byte-identical to `main` (T002)
// ---------------------------------------------------------------------

const GOLDEN_ENV: &str = "WAYBILL_UPDATE_REQUIREMENTS_OWNER_GOLDENS";

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/requirements_owner/b1-golden")
}

#[test]
fn b1_one_project_per_directory_is_byte_identical_to_main() {
    let f = owned(
        vec![],
        &[
            ("app/pyproject.toml", "[project]\nname = \"waybill-fixture-app\"\nversion = \"1.0.0\"\ndependencies = [\"waybill-fixture-lib\"]\n"),
            ("app/requirements.txt", "waybill-fixture-lib2==1.0\n"),
            ("svc/pyproject.toml", "[project]\nname = \"waybill-fixture-svc\"\nversion = \"2.0.0\"\n"),
            ("svc/requirements.txt", "waybill-fixture-lib3==1.0\n"),
        ],
    );
    let s = check(&build(&f));
    let dir = golden_dir();
    let docs = [("cdx.json", &s.cdx), ("spdx-2.3.json", &s.spdx23), ("spdx-3.json", &s.spdx3)];
    if std::env::var_os(GOLDEN_ENV).is_some() {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, doc) in docs {
            std::fs::write(dir.join(name), serde_json::to_string_pretty(&masked(doc)).unwrap() + "\n").unwrap();
        }
        return;
    }
    for (name, doc) in docs {
        let want: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(name)).unwrap()).unwrap();
        let got = masked(doc);
        assert!(got == want, "B-1 {name} differs from main's golden; {}", masked_difference(&want, &got).unwrap_or_default());
    }
}

// ---------------------------------------------------------------------
// T004: the census events fire (positive control for the log-only census)
// ---------------------------------------------------------------------

/// The scan's log at `waybill::owner_census=debug`, one line per event.
fn census_events(fx: &GitFixture) -> Vec<String> {
    let out_dir = tempfile::tempdir().unwrap();
    let out = Command::new(binary())
        .env("HOME", fx.home())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", fx.home().join(".gitconfig"))
        .env("RUST_LOG", "waybill::owner_census=debug")
        .env("NO_COLOR", "1")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(fx.path("tree"))
        .args(["--no-deep-hash", "--format", "cyclonedx-json", "--output"])
        .arg(out_dir.path().join("o.json"))
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stderr).lines().filter(|l| l.contains("owner_census")).map(str::to_string).collect()
}

fn has_event(events: &[String], event: &str, needles: &[&str]) -> bool {
    events.iter().any(|l| l.contains(&format!("event=\"{event}\"")) && needles.iter().all(|n| l.contains(n)))
}

#[test]
fn t004_census_events_fire() {
    let e = census_events(&build(&files_1640("waybill-fixture-devtool")));
    assert!(has_event(&e, "merge_source", &["waybill-fixture-devtool", r#""agrees":true"#]), "no agreeing merge_source: {e:#?}");
    // After R2 the dev package owns `tool/devtool`, so `tool` has one
    // candidate and no event (at the log-only commit it had two).
    assert!(!has_event(&e, "owner_candidates", &[r#""dir":"tool""#]), "tool still has two candidates: {e:#?}");
    assert!(has_event(&e, "dev_only_main_module", &["waybill-fixture-devtool"]), "no dev_only_main_module: {e:#?}");
    let e = census_events(&build(&files_r4(Some("requirements.txt"))));
    assert!(has_event(&e, "anchor_seed", &["waybill-fixture-shared", r#""kind":"declared""#]), "no anchor_seed: {e:#?}");
    let e = census_events(&build(&files_r4(Some("requirements-dev.txt"))));
    assert!(!has_event(&e, "anchor_seed", &["waybill-fixture-shared"]), "a dev file's anchor must not be a seed: {e:#?}");
}
