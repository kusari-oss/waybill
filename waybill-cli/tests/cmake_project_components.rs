//! Spec 1618 (#1616): a component for each CMake project the scanned tree
//! builds.
//!
//! Contract `specs/1618-cmake-project-components/contracts/wire.md`, cases
//! P-1..P-19, P-ctl and P-rep. Every case is checked in the three formats
//! through the parity extractors.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use common::installed_lib::{components, prop, row_all_formats, scan_with, source_files, write, Docs};
use corpus_harness_195::layer2_golden::{mask_nondeterministic, masked_difference};
use serde_json::Value;

const SCANS: usize = 20;
const C42: &str = "waybill:lifecycle-scope";
const C62: &str = "waybill:lifecycle-scope-derivation";
const C122: &str = "waybill:optional-derivation";
const C197: &str = "waybill:version-absence";
const C236: &str = "waybill:cmake-find-conditions";
const C238: &str = "waybill:declared-version-expression";
const C239: &str = "waybill:cmake-build-reach";
const ROWS: &[&str] = &["C42", "C55", "C62", "C197", "C236", "C237", "C238", "C239", "C240", "C241"];

// ---------------------------------------------------------------------
// Helpers (T003)
// ---------------------------------------------------------------------

fn scan_args(root: &Path, extra: &[&str]) -> Docs {
    let d = scan_with("--path", root, extra);
    for row in ROWS {
        row_all_formats(&d, row);
    }
    assert_unique_refs(&d);
    d
}

/// FR-011: no two CycloneDX elements share a `bom-ref`, the root included.
fn assert_unique_refs(d: &Docs) {
    let mut refs: Vec<&str> = components(&d.cdx).into_iter().filter_map(|c| c["bom-ref"].as_str()).collect();
    refs.extend(d.cdx["metadata"]["component"]["bom-ref"].as_str());
    let mut seen = std::collections::BTreeSet::new();
    let dups: Vec<&str> = refs.iter().copied().filter(|r| !seen.insert(*r)).collect();
    assert!(dups.is_empty(), "duplicate bom-refs: {dups:?}");
}

fn scan(root: &Path) -> Docs {
    scan_args(root, &[])
}

#[allow(dead_code)]
fn by_purl<'a>(d: &'a Docs, purl: &str) -> &'a Value {
    // A lone project is promoted to the root, so the root is searched too.
    let found: Vec<&Value> = components(&d.cdx)
        .into_iter()
        .chain(std::iter::once(&d.cdx["metadata"]["component"]))
        .filter(|c| c["purl"] == purl)
        .collect();
    assert_eq!(found.len(), 1, "{purl}: expected exactly one; all: {:?}", purls(d));
    found[0]
}

fn purls(d: &Docs) -> Vec<&str> {
    components(&d.cdx).into_iter().filter_map(|c| c["purl"].as_str()).collect()
}

/// The CycloneDX `dependsOn` of `bom_ref`, sorted.
#[allow(dead_code)]
fn edges_from(d: &Docs, bom_ref: &str) -> Vec<String> {
    let mut out: Vec<String> = d.cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["ref"] == bom_ref)
        .flat_map(|e| e["dependsOn"].as_array().cloned().unwrap_or_default())
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    out.sort();
    out
}

#[allow(dead_code)]
fn root_of(d: &Docs) -> &Value {
    &d.cdx["metadata"]["component"]
}

#[allow(dead_code)]
fn c55(c: &Value) -> Option<&str> {
    prop(c, "waybill:source-mechanism")
}

/// How a test writes a project's `VERSION`.
#[allow(dead_code)]
enum Version<'a> {
    Literal(&'a str),
    None,
    Raw(&'a str),
}

/// `<dir>/CMakeLists.txt` declaring `project(<name> …)`, plus `extra` lines.
#[allow(dead_code)]
fn write_project(root: &Path, dir: &str, name: &str, version: Version<'_>, extra: &str) {
    let v = match version {
        Version::Literal(v) => format!(" VERSION {v}"),
        Version::None => String::new(),
        Version::Raw(r) => format!(" {r}"),
    };
    let rel = if dir.is_empty() { "CMakeLists.txt".to_string() } else { format!("{dir}/CMakeLists.txt") };
    write(root, &rel, &format!("project({name}{v})\n{extra}"));
}

fn tree() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[allow(dead_code)]
fn assert_repeatable(root: &Path) {
    let docs: Vec<Docs> = (0..SCANS).map(|_| scan_with("--path", root, &[])).collect();
    for (f, name) in ["CycloneDX", "SPDX 2.3", "SPDX 3"].iter().enumerate() {
        let mut variants: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, d) in docs.iter().enumerate() {
            let v = [&d.cdx, &d.spdx23, &d.spdx3][f];
            variants.entry(serde_json::to_string_pretty(&mask_nondeterministic(v)).unwrap()).or_default().push(i);
        }
        if variants.len() > 1 {
            let parsed: Vec<Value> = variants.keys().map(|s| serde_json::from_str(s).unwrap()).collect();
            panic!(
                "{name}: {} variants across {SCANS} scans; {}",
                variants.len(),
                masked_difference(&parsed[0], &parsed[1]).unwrap_or_default()
            );
        }
    }
}

// ---------------------------------------------------------------------
// P-ctl: byte-identity control (T004, FR-012)
// ---------------------------------------------------------------------

const GOLDEN_DIR: &str = "tests/fixtures/cmake_project_components/ctl-golden";
const REGEN: &str = "WAYBILL_UPDATE_CMAKE_PROJECT_GOLDENS";

fn masked(v: &Value) -> Value {
    let s = serde_json::to_string(&mask_nondeterministic(v)).unwrap();
    let re = regex::Regex::new(r"/[^\x22]*?/waybill-fixture-tree").unwrap();
    serde_json::from_str(&re.replace_all(&s, "/TMP/waybill-fixture-tree")).unwrap()
}

/// CMake fetches and searches, and no `project()`: `main`'s output,
/// including C237 with no `paired.project` key. The golden was generated
/// with `main`'s binary before any source change (T004).
#[test]
fn p_ctl_is_byte_identical_to_main() {
    let parent = tree();
    let root: PathBuf = parent.path().join("waybill-fixture-tree");
    write(
        &root,
        "CMakeLists.txt",
        "cmake_minimum_required(VERSION 3.20)\n\
         include(FetchContent)\n\
         FetchContent_Declare(waybill-fixture-fetch GIT_REPOSITORY https://github.com/example/waybill-fixture-fetch GIT_TAG v1.0.0)\n\
         find_package(waybill-fixture-found REQUIRED)\n\
         if(WAYBILL_FIXTURE_OPT)\n  find_package(waybill-fixture-optional)\nendif()\n",
    );
    write(&root, "sub/waybill-fixture-subConfig.cmake.in", "# config template\n");
    write(&root, "sub/CMakeLists.txt", "find_package(waybill-fixture-sub)\n");
    let d = scan(&root);
    assert!(purls(&d).iter().any(|p| p.contains("waybill-fixture-fetch")), "{:?}", purls(&d));
    let golden_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_DIR);
    for (name, doc) in [("cdx", &d.cdx), ("spdx23", &d.spdx23), ("spdx3", &d.spdx3)] {
        let got = masked(doc);
        let path = golden_dir.join(format!("{name}.json"));
        if std::env::var(REGEN).is_ok() {
            std::fs::create_dir_all(&golden_dir).unwrap();
            std::fs::write(&path, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
            continue;
        }
        let want: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        if got != want {
            panic!("{name}: differs from main's output; {}", masked_difference(&want, &got).unwrap_or_default());
        }
    }
}

/// Components with C55 `cmake-project`, by PURL.
fn projects(d: &Docs) -> Vec<String> {
    let mut out: Vec<String> = components(&d.cdx)
        .into_iter()
        .chain(std::iter::once(&d.cdx["metadata"]["component"]))
        .filter(|c| c55(c) == Some("cmake-project"))
        .filter_map(|c| c["purl"].as_str().map(str::to_string))
        .collect();
    out.sort();
    out
}

/// CycloneDX components the root does not reach through `dependencies`,
/// file-tier components aside.
fn unreachable(d: &Docs) -> Vec<String> {
    let root = root_of(d)["bom-ref"].as_str().unwrap().to_string();
    let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in d.cdx["dependencies"].as_array().into_iter().flatten() {
        let to = e["dependsOn"].as_array().cloned().unwrap_or_default();
        edges.entry(e["ref"].as_str().unwrap().to_string()).or_default().extend(to.iter().filter_map(|v| v.as_str().map(str::to_string)));
    }
    let mut seen = std::collections::BTreeSet::from([root.clone()]);
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        for t in edges.get(&n).cloned().unwrap_or_default() {
            if seen.insert(t.clone()) {
                stack.push(t);
            }
        }
    }
    components(&d.cdx)
        .into_iter()
        .filter(|c| prop(c, "waybill:component-tier") != Some("file"))
        .filter_map(|c| c["bom-ref"].as_str().map(str::to_string))
        .filter(|r| !seen.contains(r))
        .collect()
}

fn c237(d: &Docs) -> Value {
    serde_json::from_str(prop_doc(d, "waybill:cmake-find-coverage").unwrap()).unwrap()
}

fn prop_doc<'a>(d: &'a Docs, name: &str) -> Option<&'a str> {
    d.cdx["metadata"]["properties"].as_array()?.iter().find(|p| p["name"] == name)?["value"].as_str()
}

const A: &str = "pkg:generic/waybill-fixture-a@1.0.0";
const B: &str = "pkg:generic/waybill-fixture-b@2.0.0";

fn two_projects(search: &str) -> tempfile::TempDir {
    let t = tree();
    write_project(t.path(), "a", "waybill-fixture-a", Version::Literal("1.0.0"), search);
    write_project(t.path(), "b", "waybill-fixture-b", Version::Literal("2.0.0"), "");
    t
}

// ---------------------------------------------------------------------
// User story 1: a monorepo's projects and the edges between them (T008)
// ---------------------------------------------------------------------

/// P-1: one project finds the other.
#[test]
fn p01_a_search_between_projects_is_an_edge() {
    let t = two_projects("find_package(waybill-fixture-b REQUIRED)\n");
    let d = scan(t.path());
    assert_eq!(projects(&d), vec![A.to_string(), B.to_string()]);
    let a = by_purl(&d, A);
    assert_eq!(prop(a, "waybill:component-role"), Some("main-module"));
    assert_eq!(prop(a, "waybill:sbom-tier"), Some("source"));
    assert!(edges_from(&d, A).contains(&B.to_string()), "{:?}", edges_from(&d, A));
    let cov = c237(&d);
    assert_eq!(cov["paired"]["project"], 1, "{cov}");
    assert!(!cov["in-tree"].as_array().into_iter().flatten().any(|n| n == "waybill-fixture-b"), "{cov}");
    assert_eq!(unreachable(&d), Vec::<String>::new());
}

/// P-2: a conditional search records its conditions on the project and does
/// not make it Optional.
#[test]
fn p02_a_conditional_search_records_conditions_only() {
    let t = two_projects("if(WAYBILL_FIXTURE_OPT)\n  find_package(waybill-fixture-b)\nendif()\n");
    let d = scan(t.path());
    assert!(edges_from(&d, A).contains(&B.to_string()));
    let b = by_purl(&d, B);
    assert_eq!(prop(b, C236), Some(r#"["WAYBILL_FIXTURE_OPT"]"#), "{b}");
    assert_eq!(prop(b, C42), None, "{b}");
    assert_eq!(prop(b, C122), None, "{b}");
}

/// P-3: a project found only from a test directory is Test.
#[test]
fn p03_found_only_from_tests_is_test() {
    let t = tree();
    write_project(t.path(), "b", "waybill-fixture-b", Version::Literal("2.0.0"), "");
    write(t.path(), "tests/CMakeLists.txt", "find_package(waybill-fixture-b REQUIRED)\n");
    let d = scan(t.path());
    let b = by_purl(&d, B);
    assert_eq!(prop(b, C42), Some("test"), "{b}");
    assert_eq!(prop(b, C62), Some("source-test-directory"), "{b}");
}

/// P-4: no `VERSION`.
#[test]
fn p04_no_version_is_not_declared() {
    let t = tree();
    write_project(t.path(), "n", "waybill-fixture-nover", Version::None, "");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill-fixture-nover");
    assert_eq!(prop(c, C197), Some("not-declared"));
    assert_eq!(prop(c, C238), None);
}

/// P-5: `VERSION ${V}` with a same-file `set(V …)`.
#[test]
fn p05_a_same_file_set_resolves_the_version() {
    let t = tree();
    write(t.path(), "v/CMakeLists.txt", "set(V 3.1.4)\nproject(waybill-fixture-var VERSION ${V})\n");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill-fixture-var@3.1.4");
    assert_eq!(prop(c, C197), None);
    assert_eq!(prop(c, C238), None);
}

/// P-6: an unresolved expression is kept verbatim.
#[test]
fn p06_an_unresolved_version_is_kept_as_an_expression() {
    let t = tree();
    write_project(t.path(), "e", "waybill-fixture-expr", Version::Raw("VERSION ${FROM_ELSEWHERE}"), "");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill-fixture-expr");
    assert_eq!(prop(c, C238), Some("${FROM_ELSEWHERE}"));
    assert_eq!(prop(c, C197), None);
}

/// P-7: the same name and version in two directories is one component.
#[test]
fn p07_same_name_and_version_merge() {
    let t = tree();
    write_project(t.path(), "x", "waybill-fixture-dup", Version::Literal("1.0.0"), "");
    write_project(t.path(), "y", "waybill-fixture-dup", Version::Literal("1.0.0"), "");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill-fixture-dup@1.0.0");
    let files = source_files(c);
    assert!(files.iter().any(|f| f.ends_with("x/CMakeLists.txt")) && files.iter().any(|f| f.ends_with("y/CMakeLists.txt")), "{files:?}");
}

/// P-8: different versions are two components.
#[test]
fn p08_different_versions_split() {
    let t = tree();
    write_project(t.path(), "x", "waybill-fixture-dup", Version::Literal("1.0.0"), "");
    write_project(t.path(), "y", "waybill-fixture-dup", Version::Literal("2.0.0"), "");
    let d = scan(t.path());
    assert_eq!(
        projects(&d),
        vec!["pkg:generic/waybill-fixture-dup@1.0.0".to_string(), "pkg:generic/waybill-fixture-dup@2.0.0".to_string()]
    );
}

/// P-8b: a versionless declaration merges into the versioned one.
#[test]
fn p08b_versionless_merges_into_versioned() {
    let t = tree();
    write_project(t.path(), "x", "waybill-fixture-dup", Version::Literal("1.0.0"), "");
    write_project(t.path(), "y", "waybill-fixture-dup", Version::None, "");
    let d = scan(t.path());
    assert_eq!(projects(&d), vec!["pkg:generic/waybill-fixture-dup@1.0.0".to_string()]);
    let c = by_purl(&d, "pkg:generic/waybill-fixture-dup@1.0.0");
    assert_eq!(source_files(c).len(), 2, "{:?}", source_files(c));
    assert_eq!(prop(c, C197), None);
}

/// P-10: a project under a test directory is Test.
#[test]
fn p10_a_test_directory_project_is_test() {
    let t = tree();
    write_project(t.path(), "tests/t", "waybill-fixture-t", Version::Literal("1.0.0"), "");
    // A runtime sibling, so the test project is not the document's only
    // top-level component (SPDX 2.3 would make that one the root).
    write_project(t.path(), "lib", "waybill-fixture-lib", Version::Literal("0.1.0"), "");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill-fixture-t@1.0.0");
    assert_eq!(prop(c, C42), Some("test"));
    assert_eq!(prop(c, C62), Some("source-test-directory"));
}

/// P-11: a project under an example (or documentation) directory is
/// Development.
#[test]
fn p11_an_example_directory_project_is_development() {
    let t = tree();
    write_project(t.path(), "lib", "waybill-fixture-lib", Version::Literal("0.1.0"), "");
    write_project(t.path(), "examples/e", "waybill-fixture-e", Version::Literal("1.0.0"), "");
    write_project(t.path(), "docs/integration/d", "waybill-fixture-d", Version::Literal("1.0.0"), "");
    let d = scan(t.path());
    for p in ["pkg:generic/waybill-fixture-e@1.0.0", "pkg:generic/waybill-fixture-d@1.0.0"] {
        let c = by_purl(&d, p);
        assert_eq!(prop(c, C42), Some("development"), "{c}");
        assert_eq!(prop(c, C62), Some("source-example-directory"), "{c}");
    }
}

/// P-12: a project the root build does not reach is marked, and still in
/// the graph.
#[test]
fn p12_not_reached_from_the_root_build() {
    let t = tree();
    write(t.path(), "CMakeLists.txt", "cmake_minimum_required(VERSION 3.20)\nadd_subdirectory(a)\n");
    write_project(t.path(), "a", "waybill-fixture-a", Version::Literal("1.0.0"), "");
    write_project(t.path(), "other", "waybill-fixture-other", Version::Literal("1.0.0"), "");
    let d = scan(t.path());
    assert_eq!(prop(by_purl(&d, A), C239), None);
    assert_eq!(prop(by_purl(&d, "pkg:generic/waybill-fixture-other@1.0.0"), C239), Some("not-reached"));
    assert_eq!(unreachable(&d), Vec::<String>::new());
}

/// P-16: a project named like a fetch is the fetch.
#[test]
fn p16_a_fetch_claims_the_name() {
    let t = tree();
    write(
        t.path(),
        "CMakeLists.txt",
        "include(FetchContent)\nFetchContent_Declare(waybill-fixture-dep GIT_REPOSITORY https://github.com/example/waybill-fixture-dep GIT_TAG v1.0.0)\n",
    );
    write_project(t.path(), "third_party/dep", "waybill-fixture-dep", Version::Literal("1.0.0"), "");
    let d = scan(t.path());
    assert_eq!(projects(&d), Vec::<String>::new());
    let f = by_purl(&d, "pkg:github/example/waybill-fixture-dep@v1.0.0");
    assert!(source_files(f).iter().any(|s| s.ends_with("third_party/dep/CMakeLists.txt")), "{:?}", source_files(f));
}

/// P-16b: a project named like an installed-library record is the record.
#[test]
fn p16b_an_installed_record_claims_the_name() {
    let t = tree();
    write(t.path(), "prefix/lib/cmake/waybill-fixture-inst/waybill-fixture-instConfigVersion.cmake", "set(PACKAGE_VERSION \"1.0.0\")\n");
    write_project(t.path(), "src/inst", "waybill-fixture-inst", Version::Literal("1.0.0"), "");
    let d = scan(t.path());
    assert_eq!(projects(&d), Vec::<String>::new());
    let r = by_purl(&d, "pkg:generic/waybill-fixture-inst@1.0.0");
    assert_eq!(c55(r), Some("cmake-package-version"));
    assert!(source_files(r).iter().any(|s| s.ends_with("src/inst/CMakeLists.txt")), "{:?}", source_files(r));
}

/// P-16c: a submodule directory named like its project is the submodule; a
/// differently named project inside a submodule is its own component,
/// enclosed by the submodule.
#[test]
fn p16c_a_submodule_claims_its_own_name() {
    use common::git_fixture::GitFixture;
    let fx = GitFixture::new();
    fx.repo("same");
    fx.repo("diff");
    fx.repo("app");
    fx.add_submodule("app", "same", "waybill-fixture-same");
    fx.add_submodule("app", "diff", "vendor-dir");
    fx.write("app/waybill-fixture-same/CMakeLists.txt", "project(waybill-fixture-same VERSION 1.0.0)\n");
    fx.write("app/vendor-dir/CMakeLists.txt", "project(waybill-fixture-other VERSION 1.0.0)\n");
    let f = common::cmake_fetch::scan(&fx, &fx.path("app"), &[]);
    let d = Docs { cdx: f.cdx, spdx23: f.spdx23, spdx3: f.spdx3, stderr: String::new() };
    assert_eq!(projects(&d), vec!["pkg:generic/waybill-fixture-other@1.0.0".to_string()]);
    let sub = components(&d.cdx)
        .into_iter()
        .find(|c| c55(c) == Some("git-submodule") && c["name"].as_str().is_some_and(|n| n.contains("diff") || n.contains("vendor-dir")))
        .expect("the vendor-dir submodule");
    let sub_ref = sub["bom-ref"].as_str().unwrap();
    assert!(edges_from(&d, sub_ref).contains(&"pkg:generic/waybill-fixture-other@1.0.0".to_string()), "{:?}", edges_from(&d, sub_ref));
}

/// P-17: an excluded directory contributes no project.
#[test]
fn p17_an_excluded_directory_contributes_nothing() {
    let t = tree();
    write_project(t.path(), "skip/p", "waybill-fixture-skip", Version::Literal("1.0.0"), "");
    let d = scan_args(t.path(), &["--exclude-path", "skip"]);
    assert!(!purls(&d).iter().any(|p| p.contains("waybill-fixture-skip")), "{:?}", purls(&d));
}

// ---------------------------------------------------------------------
// User story 3: a CMake-only repository describes itself (T018)
// ---------------------------------------------------------------------

const TOP: &str = "pkg:generic/waybill-fixture-top@1.2.3";

fn top_tree() -> tempfile::TempDir {
    let t = tree();
    write_project(t.path(), "", "waybill-fixture-top", Version::Literal("1.2.3"), "add_subdirectory(lib)\n");
    write_project(t.path(), "lib", "waybill-fixture-lib", Version::Literal("0.1.0"), "");
    t
}

/// The root's PURL in each format.
fn root_purls(d: &Docs) -> [Option<String>; 3] {
    let cdx = root_of(d)["purl"].as_str().map(str::to_string);
    let rid = &d.spdx23["documentDescribes"][0];
    let spdx23 = d.spdx23["packages"].as_array().unwrap().iter().find(|p| &p["SPDXID"] == rid).and_then(|p| {
        p["externalRefs"].as_array()?.iter().find(|r| r["referenceType"] == "purl")?["referenceLocator"].as_str().map(str::to_string)
    });
    let g = d.spdx3["@graph"].as_array().unwrap();
    let doc = g.iter().find(|e| e["type"] == "SpdxDocument").unwrap();
    let spdx3 = g.iter().find(|e| e["spdxId"] == doc["rootElement"][0]).and_then(|e| e["software_packageUrl"].as_str().map(str::to_string));
    [cdx, spdx23, spdx3]
}

/// P-13: a CMake-only tree's top-level project is the root.
#[test]
fn p13_a_cmake_only_tree_has_its_top_project_as_root() {
    let t = top_tree();
    let d = scan(t.path());
    assert_eq!(root_purls(&d), [Some(TOP.to_string()), Some(TOP.to_string()), Some(TOP.to_string())]);
    assert_eq!(unreachable(&d), Vec::<String>::new());
}

/// Root PURL and C69 of a scan, for comparing against the same tree
/// without its `CMakeLists.txt` files.
fn root_and_heuristic(root: &Path) -> (Option<String>, Option<String>) {
    let d = scan(root);
    (root_of(&d)["purl"].as_str().map(str::to_string), prop_doc(&d, "waybill:root-selection-heuristic").map(str::to_string))
}

fn without_cmake(root: &Path) {
    for e in walk(root) {
        if e.file_name().is_some_and(|n| n == "CMakeLists.txt") {
            std::fs::remove_file(e).unwrap();
        }
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

const PYPROJECT: &str = "[project]\nname = \"waybill-fixture-py\"\nversion = \"0.1.0\"\n";

/// P-14: another ecosystem's main module at the scan root keeps the root
/// and C69, and encloses the top project.
#[test]
fn p14_a_scan_root_python_project_keeps_the_root() {
    let t = top_tree();
    write(t.path(), "pyproject.toml", PYPROJECT);
    let with = root_and_heuristic(t.path());
    let d = scan(t.path());
    let py = root_of(&d)["bom-ref"].as_str().unwrap().to_string();
    assert!(edges_from(&d, &py).contains(&TOP.to_string()), "{:?}", edges_from(&d, &py));
    without_cmake(t.path());
    assert_eq!(with, root_and_heuristic(t.path()));
}

/// P-15: one Python main module below the root and CMake projects
/// elsewhere: root and C69 as without CMake (research R2).
#[test]
fn p15_a_nested_python_project_keeps_the_root() {
    let t = tree();
    write(t.path(), "py/pyproject.toml", PYPROJECT);
    write_project(t.path(), "a", "waybill-fixture-a", Version::Literal("1.0.0"), "");
    write_project(t.path(), "b", "waybill-fixture-b", Version::Literal("2.0.0"), "");
    let with = root_and_heuristic(t.path());
    without_cmake(t.path());
    assert_eq!(with, root_and_heuristic(t.path()));
}

/// P-18: under an override the project is demoted and anchored (m860);
/// `--root-purl` naming its coordinate folds it into the root (#1626).
#[test]
fn p18_override_demotes_or_folds() {
    let t = top_tree();
    let d = scan_args(t.path(), &["--root-name", "waybill-fixture-override", "--root-version", "9.9.9"]);
    assert_eq!(prop(by_purl(&d, TOP), "waybill:demoted-from-main-module"), Some("true"));
    assert!(edges_from(&d, root_of(&d)["bom-ref"].as_str().unwrap()).contains(&TOP.to_string()));
    assert_eq!(unreachable(&d), Vec::<String>::new());
    let d = scan_args(t.path(), &["--root-purl", TOP]);
    assert!(!purls(&d).contains(&TOP), "folded into the root: {:?}", purls(&d));
    assert_eq!(unreachable(&d), Vec::<String>::new());
}

/// P-19: `--split` gives one document per project boundary.
#[test]
fn p19_split_has_a_document_per_project() {
    let t = two_projects("find_package(waybill-fixture-b REQUIRED)\n");
    let out = tree();
    let o = std::process::Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(t.path())
        .args(["--no-deep-hash", "--split", "--format", "cyclonedx-json", "--output-dir"])
        .arg(out.path())
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let manifest: Value = serde_json::from_str(&std::fs::read_to_string(out.path().join("split-manifest.json")).unwrap()).unwrap();
    let mut roots: Vec<String> = manifest["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["root_purl"].as_str().map(str::to_string))
        .collect();
    roots.sort();
    assert_eq!(roots, vec![A.to_string(), B.to_string()], "{manifest}");
}

// ---------------------------------------------------------------------
// User story 2: vendored code is visible and marked (T016)
// ---------------------------------------------------------------------

const C240: &str = "waybill:vendored";
const C241: &str = "waybill:vendored-signals";

/// A git superproject `app` with project `name` at `dir`, a submodule when
/// `submodule`, and a licence file when `licence`.
fn vendored_tree(dir: &str, name: &str, submodule: bool, licence: bool) -> (common::git_fixture::GitFixture, Docs) {
    use common::git_fixture::GitFixture;
    let fx = GitFixture::new();
    fx.repo("app");
    // The root build reaches both directories, so `not-reached` is not among
    // the signals under test.
    fx.write("app/CMakeLists.txt", &format!("project(waybill-fixture-app VERSION 1.0.0)\nadd_subdirectory(core)\nadd_subdirectory({dir})\n"));
    // A runtime sibling, so the vendored project is a component, not the root.
    fx.write("app/core/CMakeLists.txt", "project(waybill-fixture-core VERSION 1.0.0)\n");
    if submodule {
        fx.repo("dep");
        fx.add_submodule("app", "dep", dir);
    }
    fx.write(&format!("app/{dir}/CMakeLists.txt"), &format!("project({name} VERSION 2.0.0)\n"));
    if licence {
        fx.write(&format!("app/{dir}/LICENSE"), "Copyright (c) Somebody Else\n");
    }
    let f = common::cmake_fetch::scan(&fx, &fx.path("app"), &[]);
    let d = Docs { cdx: f.cdx, spdx23: f.spdx23, spdx3: f.spdx3, stderr: String::new() };
    (fx, d)
}

fn signals(c: &Value) -> Vec<String> {
    prop(c, C241).map(|v| serde_json::from_str(v).unwrap()).unwrap_or_default()
}

/// P-9: path segment and submodule boundary: vendored (T002's rule).
#[test]
fn p09_a_vendored_project_is_marked() {
    let (_fx, d) = vendored_tree("third_party/vend", "waybill-fixture-vendored", true, true);
    let c = by_purl(&d, "pkg:generic/waybill-fixture-vendored@2.0.0");
    assert_eq!(prop(c, C240), Some("true"), "{c}");
    assert_eq!(signals(c), ["licence-file", "path-segment", "submodule-boundary"]);
}

/// Each signal alone is recorded, and alone is not a verdict.
#[test]
fn p09_single_signals_are_recorded_without_a_verdict() {
    for (dir, sub, lic, want) in [
        ("third_party/vend", false, false, vec!["path-segment"]),
        ("libs/vend", true, false, vec!["submodule-boundary"]),
        ("libs/vend", false, true, vec!["licence-file"]),
    ] {
        let (_fx, d) = vendored_tree(dir, "waybill-fixture-vendored", sub, lic);
        let c = by_purl(&d, "pkg:generic/waybill-fixture-vendored@2.0.0");
        assert_eq!(signals(c), want, "{dir} sub={sub} lic={lic}");
        assert_eq!(prop(c, C240), None, "{dir} sub={sub} lic={lic}");
    }
}

/// A first-party project carries neither row.
#[test]
fn p09_a_first_party_project_carries_neither() {
    let (_fx, d) = vendored_tree("libs/own", "waybill-fixture-own", false, false);
    let c = by_purl(&d, "pkg:generic/waybill-fixture-own@2.0.0");
    assert_eq!(prop(c, C240), None);
    assert_eq!(prop(c, C241), None);
}

// ---------------------------------------------------------------------
// P-rep (T020, FR-013)
// ---------------------------------------------------------------------

/// The cases in one tree, scanned 20 times: identical each time.
#[test]
fn p_rep_repeatable() {
    let t = tree();
    let r = t.path();
    write(r, "CMakeLists.txt", "cmake_minimum_required(VERSION 3.20)\nadd_subdirectory(a)\nadd_subdirectory(b)\n");
    write_project(r, "a", "waybill-fixture-a", Version::Literal("1.0.0"), "find_package(waybill-fixture-b REQUIRED)\nif(OPT)\n find_package(waybill-fixture-c)\nendif()\n");
    write_project(r, "b", "waybill-fixture-b", Version::Literal("2.0.0"), "");
    write_project(r, "c", "waybill-fixture-c", Version::None, "");
    write(r, "v/CMakeLists.txt", "set(V 3.1.4)\nproject(waybill-fixture-var VERSION ${V})\n");
    write_project(r, "e", "waybill-fixture-expr", Version::Raw("VERSION ${FROM_ELSEWHERE}"), "");
    write_project(r, "x", "waybill-fixture-dup", Version::Literal("1.0.0"), "");
    write_project(r, "y", "waybill-fixture-dup", Version::None, "");
    write_project(r, "z", "waybill-fixture-dup", Version::Literal("2.0.0"), "");
    write_project(r, "tests/t", "waybill-fixture-t", Version::Literal("1.0.0"), "find_package(waybill-fixture-only-tests)\n");
    write_project(r, "lib/only", "waybill-fixture-only-tests", Version::Literal("1.0.0"), "");
    write_project(r, "examples/e", "waybill-fixture-e", Version::Literal("1.0.0"), "");
    write_project(r, "third_party/v", "waybill-fixture-vendored", Version::Literal("2.0.0"), "");
    write(r, "third_party/v/LICENSE", "x\n");
    let d = scan(r);
    // 13 declarations, 11 components: the versionless `dup` joins `dup@1.0.0`.
    assert_eq!(
        projects(&d),
        [
            "pkg:generic/waybill-fixture-a@1.0.0",
            "pkg:generic/waybill-fixture-b@2.0.0",
            "pkg:generic/waybill-fixture-c",
            "pkg:generic/waybill-fixture-dup@1.0.0",
            "pkg:generic/waybill-fixture-dup@2.0.0",
            "pkg:generic/waybill-fixture-e@1.0.0",
            "pkg:generic/waybill-fixture-expr",
            "pkg:generic/waybill-fixture-only-tests@1.0.0",
            "pkg:generic/waybill-fixture-t@1.0.0",
            "pkg:generic/waybill-fixture-var@3.1.4",
            "pkg:generic/waybill-fixture-vendored@2.0.0",
        ]
    );
    assert_eq!(unreachable(&d), Vec::<String>::new());
    assert_repeatable(r);
}

/// P-20 (research R5): with `--include-vendored`, a `third_party/<n>`
/// directory that declares a project is that project, not a second
/// `cmake-vendored` component.
#[test]
fn p20_include_vendored_folds_into_the_project() {
    let t = tree();
    write(t.path(), "CMakeLists.txt", "add_subdirectory(third_party/vlib)\n");
    write_project(t.path(), "third_party/vlib", "waybill-fixture-vlib", Version::Literal("1.0.0"), "");
    write_project(t.path(), "core", "waybill-fixture-core", Version::Literal("1.0.0"), "");
    let d = scan_args(t.path(), &["--include-vendored"]);
    assert!(
        !components(&d.cdx).iter().any(|c| c55(c) == Some("cmake-vendored")),
        "{:?}",
        purls(&d)
    );
    let c = by_purl(&d, "pkg:generic/waybill-fixture-vlib@1.0.0");
    let files = source_files(c);
    assert!(files.iter().any(|f| f == "CMakeLists.txt"), "the adding file joins the evidence: {files:?}");
}

/// P-21: two projects declared in one directory do not enclose each other;
/// what is above encloses both, and neither has an edge to the other.
#[test]
fn p21_two_projects_in_one_directory() {
    let t = tree();
    write_project(t.path(), "", "waybill-fixture-top", Version::Literal("1.0.0"), "add_subdirectory(a)\n");
    write(t.path(), "a/CMakeLists.txt", "project(waybill-fixture-one VERSION 1.0.0)\nproject(waybill-fixture-two VERSION 1.0.0)\n");
    let d = scan(t.path());
    let one = "pkg:generic/waybill-fixture-one@1.0.0";
    let two = "pkg:generic/waybill-fixture-two@1.0.0";
    assert!(!edges_from(&d, one).contains(&two.to_string()), "{:?}", edges_from(&d, one));
    assert!(!edges_from(&d, two).contains(&one.to_string()), "{:?}", edges_from(&d, two));
    let top = edges_from(&d, "pkg:generic/waybill-fixture-top@1.0.0");
    assert!(top.contains(&one.to_string()) && top.contains(&two.to_string()), "{top:?}");
}
