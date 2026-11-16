//! Spec 1537 (#1566, #1538): a package a pip requirements file declares is
//! a direct dependency of the project that declares it, whatever other edges
//! the root has. All three formats agree on the root's direct targets, and
//! the completeness verdict describes the graph the document carries.
//!
//! Each case is a contract row (`specs/1537-requirements-root-attach/
//! contracts/wire.md`, C-1 to C-13), built as a temporary git repository with
//! synthetic `waybill-fixture-*` names. Nothing is fetched. Not gated to
//! unix: the git-submodule reader's tests use the same fixture helper and
//! run on every lane.
//!
//! Fixtures keep at least two root-level packages. With one, SPDX 2.3 makes
//! that package the document's subject while CycloneDX and SPDX 3 use a
//! placeholder root (#1507), and I-1 would measure root selection, not root
//! edges (measurements/repro.md, arm c11).

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use common::git_fixture::GitFixture;
use serde_json::Value;

struct Docs {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
}

fn scan(fx: &GitFixture, path: &Path, extra: &[&str]) -> Docs {
    let out_dir = tempfile::tempdir().unwrap();
    let (c, s2, s3) = (
        out_dir.path().join("o.cdx.json"),
        out_dir.path().join("o.spdx.json"),
        out_dir.path().join("o.spdx3.json"),
    );
    let out = Command::new(common::bin())
        .env("HOME", fx.home())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", fx.home().join(".gitconfig"))
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(path)
        .args(["--no-deep-hash", "--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", c.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", s2.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", s3.display()))
        .args(extra)
        .output()
        .unwrap();
    assert!(out.status.success(), "scan failed: {}", String::from_utf8_lossy(&out.stderr));
    let read = |p: &Path| serde_json::from_slice::<Value>(&std::fs::read(p).unwrap()).unwrap();
    Docs { cdx: read(&c), spdx23: read(&s2), spdx3: read(&s3) }
}

fn strip(purl: &str) -> String {
    purl.split('?').next().unwrap_or(purl).to_string()
}

fn cdx_components(cdx: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    let mut stack: Vec<&Value> = cdx["components"].as_array().into_iter().flatten().collect();
    while let Some(c) = stack.pop() {
        out.push(c);
        stack.extend(c["components"].as_array().into_iter().flatten());
    }
    out
}

fn prop<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c["properties"].as_array()?.iter().find(|p| p["name"] == name)?["value"].as_str()
}

fn cdx_purl_of(cdx: &Value, bom_ref: &str) -> String {
    cdx_components(cdx)
        .into_iter()
        .find(|c| c["bom-ref"] == bom_ref)
        .and_then(|c| c["purl"].as_str())
        .map(strip)
        .unwrap_or_else(|| bom_ref.to_string())
}

/// The root's direct targets in CycloneDX, as PURLs without qualifiers.
fn cdx_root_targets(cdx: &Value) -> BTreeSet<String> {
    let root = cdx["metadata"]["component"]["bom-ref"].as_str().unwrap();
    cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["ref"] == root)
        .flat_map(|d| d["dependsOn"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .map(|r| cdx_purl_of(cdx, r))
        .collect()
}

/// The root's direct targets in SPDX 2.3: `root DEPENDS_ON x`, or a
/// scope-typed `x *_DEPENDENCY_OF root`.
fn spdx23_root_targets(doc: &Value) -> BTreeSet<String> {
    let rels = doc["relationships"].as_array().unwrap();
    let root = rels
        .iter()
        .find(|r| r["relationshipType"] == "DESCRIBES" && r["spdxElementId"] == "SPDXRef-DOCUMENT")
        .and_then(|r| r["relatedSpdxElement"].as_str())
        .unwrap();
    let purl_of = |id: &str| -> String {
        doc["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["SPDXID"] == id)
            .and_then(|p| {
                p["externalRefs"]
                    .as_array()?
                    .iter()
                    .find(|e| e["referenceType"] == "purl")?["referenceLocator"]
                    .as_str()
            })
            .map(strip)
            .unwrap_or_else(|| id.to_string())
    };
    let mut out = BTreeSet::new();
    for r in rels {
        let t = r["relationshipType"].as_str().unwrap_or("");
        if r["spdxElementId"] == root && t == "DEPENDS_ON" {
            out.insert(purl_of(r["relatedSpdxElement"].as_str().unwrap()));
        }
        if r["relatedSpdxElement"] == root && t.ends_with("_DEPENDENCY_OF") {
            out.insert(purl_of(r["spdxElementId"].as_str().unwrap()));
        }
    }
    out
}

/// The root's direct targets in SPDX 3: `dependsOn` from `rootElement`.
fn spdx3_root_targets(doc: &Value) -> BTreeSet<String> {
    let g = doc["@graph"].as_array().unwrap();
    let root = g.iter().find(|e| e["type"] == "SpdxDocument").unwrap()["rootElement"][0].as_str().unwrap();
    let purl_of = |id: &str| -> String {
        g.iter()
            .find(|e| e["spdxId"] == id)
            .and_then(|e| e["software_packageUrl"].as_str())
            .map(strip)
            .unwrap_or_else(|| id.to_string())
    };
    g.iter()
        .filter(|e| e["type"] == "Relationship" && e["relationshipType"] == "dependsOn" && e["from"] == root)
        .flat_map(|e| e["to"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .map(purl_of)
        .collect()
}

/// I-1 (FR-006): the root's direct targets are the same set in all three
/// formats. Returns that set.
fn root_targets(docs: &Docs) -> BTreeSet<String> {
    let c = cdx_root_targets(&docs.cdx);
    let s2 = spdx23_root_targets(&docs.spdx23);
    let s3 = spdx3_root_targets(&docs.spdx3);
    assert_eq!(c, s2, "I-1: CycloneDX and SPDX 2.3 root targets differ");
    assert_eq!(c, s3, "I-1: CycloneDX and SPDX 3 root targets differ");
    c
}

fn verdict_cdx(cdx: &Value) -> Option<String> {
    prop(&cdx["metadata"], "waybill:graph-completeness").map(str::to_string)
}

fn verdict_spdx(doc: &Value) -> Option<String> {
    let anns: Vec<&Value> = doc["annotations"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(doc["@graph"].as_array().into_iter().flatten().filter(|e| e["type"] == "Annotation"))
        .collect();
    anns.iter().find_map(|a| {
        let env: Value = serde_json::from_str(a["comment"].as_str().or_else(|| a["statement"].as_str())?).ok()?;
        (env["field"] == "waybill:graph-completeness").then(|| env["value"].as_str().map(str::to_string))?
    })
}

/// I-2 (FR-007): the verdict is the contract's expected value, in every
/// format. The classifier's seed set is never recomputed here (contract I-2).
fn assert_verdict(docs: &Docs, want: &str) {
    for (fmt, v) in [
        ("CycloneDX", verdict_cdx(&docs.cdx)),
        ("SPDX 2.3", verdict_spdx(&docs.spdx23)),
        ("SPDX 3", verdict_spdx(&docs.spdx3)),
    ] {
        assert_eq!(v.as_deref(), Some(want), "I-2: {fmt} verdict");
    }
}

/// The CycloneDX `dependsOn` of the component with PURL prefix `from`, as
/// PURLs without qualifiers.
fn depends_of(cdx: &Value, from: &str) -> BTreeSet<String> {
    let refs: Vec<&str> = cdx_components(cdx)
        .into_iter()
        .filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with(from)))
        .filter_map(|c| c["bom-ref"].as_str())
        .collect();
    assert_eq!(refs.len(), 1, "exactly one component {from}");
    cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["ref"] == refs[0])
        .flat_map(|d| d["dependsOn"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .map(|r| cdx_purl_of(cdx, r))
        .collect()
}

const A: &str = "pkg:pypi/waybill-fixture-a@1.0.0";
const B: &str = "pkg:pypi/waybill-fixture-b@2.0.0";
const REQ_AB: &str = "waybill-fixture-a==1.0.0\nwaybill-fixture-b==2.0.0\n";

/// `root_targets` without file-tier entries (a manifest such as a bare
/// `package.json` is itself a file-tier graph-top the fallback attaches).
/// I-1 still compares the full sets inside `root_targets`.
fn package_targets(docs: &Docs) -> BTreeSet<String> {
    root_targets(docs).into_iter().filter(|t| !t.starts_with("pkg:generic/file-tier")).collect()
}

/// A scan-root Python project with no dependencies. With a single main
/// module anywhere in the scan, the root selector promotes it to subject
/// however deep it is (C-5: `tools/package.json`; C-6: a submodule's own
/// `pyproject.toml`). Root selection is not this feature, so C-5 to C-7
/// give the repository its own project at the scan root.
const ROOT_PYPROJECT: &str = "[project]\nname = \"waybill-fixture-app\"\nversion = \"1.0.0\"\n";
const APP: &str = "pkg:pypi/waybill-fixture-app@1.0.0";

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// Commit everything in `rel` (a repository under the fixture).
fn commit_all(fx: &GitFixture, rel: &str) {
    let dir = fx.path(rel);
    fx.git(&dir, &["add", "-A"]);
    fx.git(&dir, &["commit", "-q", "-m", "files"]);
}

/// The single component whose PURL starts with `prefix`.
fn the_purl(docs: &Docs, prefix: &str) -> String {
    let found: Vec<String> = cdx_components(&docs.cdx)
        .into_iter()
        .filter_map(|c| c["purl"].as_str())
        .filter(|p| p.starts_with(prefix))
        .map(strip)
        .collect();
    assert_eq!(found.len(), 1, "exactly one {prefix}: {found:?}");
    found[0].clone()
}

// ---------------------------------------------------------------------
// C-1, C-4, C-9: controls (attach at `main` today, through the fallback)
// ---------------------------------------------------------------------

#[test]
fn c1_requirements_only() {
    let fx = GitFixture::new();
    fx.repo("app");
    fx.write("app/requirements.txt", REQ_AB);
    commit_all(&fx, "app");
    let docs = scan(&fx, &fx.path("app"), &[]);
    assert_eq!(root_targets(&docs), set(&[A, B]));
    assert_verdict(&docs, "complete");
}

#[test]
fn c4_nested_requirements_without_a_project() {
    let fx = GitFixture::new();
    fx.repo("app");
    fx.write("app/requirements.txt", REQ_AB);
    fx.write("app/tools/requirements.txt", "waybill-fixture-t==9.0.0\n");
    commit_all(&fx, "app");
    let docs = scan(&fx, &fx.path("app"), &[]);
    assert_eq!(root_targets(&docs), set(&[A, B, "pkg:pypi/waybill-fixture-t@9.0.0"]));
    assert_verdict(&docs, "complete");
}

#[test]
fn c9_os_packages_with_a_placeholder_root() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    common::installed_lib::write(
        root,
        "var/lib/dpkg/status",
        "Package: waybill-fixture-one\nStatus: install ok installed\nVersion: 1.0\nArchitecture: amd64\n\n\
         Package: waybill-fixture-two\nStatus: install ok installed\nVersion: 2.0\nArchitecture: amd64\n",
    );
    let fx = GitFixture::new();
    let docs = scan(&fx, root, &[]);
    let targets = root_targets(&docs);
    assert_eq!(targets.len(), 2, "both OS packages attach to the placeholder root: {targets:?}");
    assert!(targets.iter().all(|t| t.starts_with("pkg:deb/")), "{targets:?}");
    assert_verdict(&docs, "complete");
}

// ---------------------------------------------------------------------
// US1 / US2: declared requirements attach, whatever else the root has
// ---------------------------------------------------------------------

/// C-2, #1566's reproduction.
#[test]
fn c2_requirements_and_a_submodule() {
    let fx = GitFixture::new();
    fx.repo("dep");
    fx.repo("app");
    fx.add_submodule("app", "dep", "third_party/waybill-fixture-dep");
    fx.write("app/requirements.txt", REQ_AB);
    commit_all(&fx, "app");
    let docs = scan(&fx, &fx.path("app"), &[]);
    let dep = the_purl(&docs, "pkg:generic/dep@");
    assert_eq!(root_targets(&docs), set(&[A, B, dep.as_str()]));
    assert_verdict(&docs, "complete");
}

/// C-3: the same defect with no submodule. The root is the main module,
/// and its own declared dependency switched the fallback off at `main`.
#[test]
fn c3_requirements_next_to_a_pyproject() {
    let fx = GitFixture::new();
    fx.repo("app");
    fx.write(
        "app/pyproject.toml",
        "[project]\nname = \"waybill-fixture-app\"\nversion = \"1.0.0\"\ndependencies = [\"waybill-fixture-c==3.0.0\"]\n",
    );
    fx.write("app/requirements.txt", REQ_AB);
    commit_all(&fx, "app");
    let docs = scan(&fx, &fx.path("app"), &[]);
    assert_eq!(docs.cdx["metadata"]["component"]["purl"], "pkg:pypi/waybill-fixture-app@1.0.0");
    assert_eq!(root_targets(&docs), set(&[A, B, "pkg:pypi/waybill-fixture-c@unresolved"]));
    assert_verdict(&docs, "complete");
}

const TOOLS_PKG: &str = "pkg:npm/waybill-fixture-tools@1.0.0";
const TOOLS_JSON: &str = r#"{"name":"waybill-fixture-tools","version":"1.0.0"}"#;

/// C-5: a nested requirements file belongs to the project in its directory,
/// here an npm main module.
#[test]
fn c5_nested_requirements_owned_by_another_ecosystems_project() {
    let fx = GitFixture::new();
    fx.repo("app");
    fx.write("app/requirements.txt", REQ_AB);
    fx.write("app/tools/requirements.txt", "waybill-fixture-t==9.0.0\n");
    fx.write("app/tools/package.json", TOOLS_JSON);
    fx.write("app/pyproject.toml", ROOT_PYPROJECT);
    commit_all(&fx, "app");
    let docs = scan(&fx, &fx.path("app"), &[]);
    assert_eq!(docs.cdx["metadata"]["component"]["purl"], APP);
    assert_eq!(package_targets(&docs), set(&[A, B, TOOLS_PKG]));
    assert_eq!(depends_of(&docs.cdx, TOOLS_PKG), set(&["pkg:pypi/waybill-fixture-t@9.0.0"]));
    assert_verdict(&docs, "complete");
}

/// C-6: a submodule with its own Python project. Its requirements file
/// belongs to that project, not to the submodule component or the root.
#[test]
fn c6_requirements_inside_a_submodules_python_project() {
    let fx = GitFixture::new();
    fx.repo("x");
    fx.write(
        "x/pyproject.toml",
        "[project]\nname = \"waybill-fixture-xpy\"\nversion = \"1.0.0\"\n",
    );
    fx.write("x/requirements.txt", "waybill-fixture-r==6.0.0\n");
    commit_all(&fx, "x");
    fx.repo("app");
    fx.add_submodule("app", "x", "third_party/waybill-fixture-x");
    fx.write("app/requirements.txt", REQ_AB);
    fx.write("app/pyproject.toml", ROOT_PYPROJECT);
    commit_all(&fx, "app");
    let docs = scan(&fx, &fx.path("app"), &[]);
    assert_eq!(docs.cdx["metadata"]["component"]["purl"], APP);
    let x = the_purl(&docs, "pkg:generic/x@");
    let targets = package_targets(&docs);
    assert!(targets.contains(&x) && targets.contains(A) && targets.contains(B), "{targets:?}");
    assert!(!targets.contains("pkg:pypi/waybill-fixture-r@6.0.0"), "r is the submodule project's, not the root's: {targets:?}");
    assert_eq!(depends_of(&docs.cdx, "pkg:pypi/waybill-fixture-xpy@"), set(&["pkg:pypi/waybill-fixture-r@6.0.0"]));
    assert_verdict(&docs, "complete");
}

/// C-7: one package declared by two projects gets an edge from each, and
/// keeps both declaring files.
#[test]
fn c7_one_package_declared_by_two_projects() {
    let fx = GitFixture::new();
    fx.repo("app");
    fx.write("app/requirements.txt", REQ_AB);
    fx.write("app/tools/requirements.txt", "waybill-fixture-a==1.0.0\n");
    fx.write("app/tools/package.json", TOOLS_JSON);
    fx.write("app/pyproject.toml", ROOT_PYPROJECT);
    commit_all(&fx, "app");
    let docs = scan(&fx, &fx.path("app"), &[]);
    assert_eq!(docs.cdx["metadata"]["component"]["purl"], APP);
    assert_eq!(package_targets(&docs), set(&[A, B, TOOLS_PKG]));
    assert_eq!(depends_of(&docs.cdx, TOOLS_PKG), set(&[A]));
    let a = cdx_components(&docs.cdx).into_iter().find(|c| c["purl"] == A).unwrap();
    let files: Vec<String> = serde_json::from_str(prop(a, "waybill:source-files").unwrap()).unwrap();
    assert_eq!(files, vec!["requirements.txt".to_string(), "tools/requirements.txt".to_string()]);
    assert_verdict(&docs, "complete");
}

/// C-8: a `requirements-dev.txt` keeps the scope the reader already gives
/// it on the new edge. The pip reader classifies `-dev`, `-test`, `-docs`
/// and `-ci` files as optional (`classify_requirements_scope`), so the edge
/// is optional, not development (FR-010: keep, do not reinterpret).
#[test]
fn c8_dev_requirements_keep_their_optional_scope() {
    let fx = GitFixture::new();
    fx.repo("dep");
    fx.repo("app");
    fx.add_submodule("app", "dep", "third_party/waybill-fixture-dep");
    fx.write("app/requirements.txt", REQ_AB);
    fx.write("app/requirements-dev.txt", "waybill-fixture-d==7.0.0\n");
    commit_all(&fx, "app");
    let docs = scan(&fx, &fx.path("app"), &[]);
    let d = "pkg:pypi/waybill-fixture-d@7.0.0";
    let dep = the_purl(&docs, "pkg:generic/dep@");
    assert_eq!(root_targets(&docs), set(&[A, B, d, dep.as_str()]));
    let rels = docs.spdx23["relationships"].as_array().unwrap();
    let d_id = docs.spdx23["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "waybill-fixture-d")
        .and_then(|p| p["SPDXID"].as_str())
        .unwrap();
    assert!(
        rels.iter().any(|r| r["spdxElementId"] == d_id && r["relationshipType"] == "OPTIONAL_DEPENDENCY_OF"),
        "SPDX 2.3 spells the edge as OPTIONAL_DEPENDENCY_OF"
    );
    let d_cdx = cdx_components(&docs.cdx).into_iter().find(|c| c["purl"] == d).unwrap();
    assert_eq!(d_cdx["scope"], "excluded", "CycloneDX keeps the optional scope as `excluded`");
    assert_verdict(&docs, "complete");
}

/// C-10: a supplement anchor does not switch the fallback off either.
#[test]
fn c10_requirements_and_a_supplement_anchor() {
    let fx = GitFixture::new();
    fx.repo("app");
    fx.write("app/requirements.txt", REQ_AB);
    commit_all(&fx, "app");
    let supp = fx.path("supp.cdx.json");
    std::fs::write(
        &supp,
        r#"{"bomFormat":"CycloneDX","specVersion":"1.6",
            "metadata":{"component":{"type":"application","name":"waybill-fixture-app",
              "purl":"pkg:github/waybill-fixture/app@v1","bom-ref":"pkg:github/waybill-fixture/app@v1"}},
            "components":[{"type":"library","name":"waybill-fixture-s","version":"1.0.0",
              "purl":"pkg:generic/waybill-fixture-s@1.0.0","bom-ref":"pkg:generic/waybill-fixture-s@1.0.0"}],
            "dependencies":[{"ref":"pkg:github/waybill-fixture/app@v1","dependsOn":["pkg:generic/waybill-fixture-s@1.0.0"]}]}"#,
    )
    .unwrap();
    let docs = scan(&fx, &fx.path("app"), &["--supplement-cdx", supp.to_str().unwrap()]);
    assert_eq!(root_targets(&docs), set(&[A, B, "pkg:generic/waybill-fixture-s@1.0.0"]));
    assert_verdict(&docs, "complete");
}

/// C-11: a requirements file under `vendor/` is not read, as today.
#[test]
fn c11_vendored_requirements_are_not_read() {
    let fx = GitFixture::new();
    fx.repo("app");
    fx.write("app/requirements.txt", REQ_AB);
    fx.write("app/vendor/x/requirements.txt", "waybill-fixture-v==4.0.0\n");
    commit_all(&fx, "app");
    let docs = scan(&fx, &fx.path("app"), &[]);
    assert_eq!(root_targets(&docs), set(&[A, B]));
    assert!(cdx_components(&docs.cdx).iter().all(|c| c["purl"] != "pkg:pypi/waybill-fixture-v@4.0.0"));
    assert_verdict(&docs, "complete");
}

/// C-12: a file named like a constraints file is read as a declaration and
/// attached like any other (research R14).
#[test]
fn c12_constraints_named_file_is_a_declaration() {
    let fx = GitFixture::new();
    fx.repo("app");
    fx.write("app/requirements.txt", REQ_AB);
    fx.write("app/requirements-constraints.txt", "waybill-fixture-k==5.0.0\n");
    commit_all(&fx, "app");
    let docs = scan(&fx, &fx.path("app"), &[]);
    assert_eq!(root_targets(&docs), set(&[A, B, "pkg:pypi/waybill-fixture-k@5.0.0"]));
    assert_verdict(&docs, "complete");
}

/// C-13: a submodule that CMake also declares merges into one component
/// (spec 1530, FR-009) with exactly one root edge.
#[test]
fn c13_submodule_also_declared_by_fetchcontent() {
    let fx = GitFixture::new();
    fx.repo("app");
    let c = "5".repeat(40);
    fx.write(
        "app/.gitmodules",
        "[submodule \"m\"]\n\tpath = third_party/waybill-fixture-Mixed\n\turl = https://github.com/ROCm/waybill-fixture-Mixed.git\n",
    );
    fx.write(
        "app/CMakeLists.txt",
        &format!(
            "cmake_minimum_required(VERSION 3.25)\nproject(p)\ninclude(FetchContent)\n\
             FetchContent_Declare(waybill-fixture-mixed GIT_REPOSITORY https://github.com/ROCm/waybill-fixture-Mixed.git GIT_TAG {c})\n"
        ),
    );
    fx.write("app/requirements.txt", REQ_AB);
    let app = fx.path("app");
    fx.git(&app, &["add", ".gitmodules", "CMakeLists.txt", "requirements.txt"]);
    fx.add_gitlink("app", "third_party/waybill-fixture-Mixed", &c);
    let docs = scan(&fx, &app, &[]);
    let mixed = format!("pkg:github/rocm/waybill-fixture-mixed@{c}");
    assert_eq!(root_targets(&docs), set(&[A, B, mixed.as_str()]));
    let root = docs.cdx["metadata"]["component"]["bom-ref"].as_str().unwrap();
    let edges: Vec<&Value> = docs.cdx["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["ref"] == root)
        .flat_map(|d| d["dependsOn"].as_array().into_iter().flatten())
        .filter(|t| t.as_str().is_some_and(|t| cdx_purl_of(&docs.cdx, t) == mixed))
        .collect();
    assert_eq!(edges.len(), 1, "one root edge to the merged component");
    assert_verdict(&docs, "complete");
}
