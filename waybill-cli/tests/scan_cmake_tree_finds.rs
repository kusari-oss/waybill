//! Spec 1614 (#1591, #1605): package searches (`find_package`,
//! `pkg_check_modules`, `pkg_search_module`) read from every CMake file, with
//! the conditions that reach them.
//!
//! Contract `specs/1614-cmake-tree-finds/contracts/wire.md`, cases W-1..W-17,
//! W-ctl and W-rep. Every case is checked in the three formats through the
//! parity extractors.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::path::{Path, PathBuf};

use common::git_fixture::GitFixture;
use common::installed_lib::{components, depends_on, doc_field_all_formats, prop, row_all_formats, scan_with, source_files, write, Docs};
use corpus_harness_195::layer2_golden::{mask_nondeterministic, masked_difference};
use serde_json::Value;

/// C237, new in this feature: present whenever package searches are read,
/// so the byte-identity control compares documents without it (FR-009).
const C237: &str = "waybill:cmake-find-coverage";

// ---------------------------------------------------------------------
// Helpers (T002)
// ---------------------------------------------------------------------

/// All three formats of a scan of `root`, with every catalog row this
/// feature touches asserted to agree across them.
fn scan(root: &Path) -> Docs {
    let d = scan_with("--path", root, &[]);
    for row in ["C42", "C62", "C122", "C232", "C236", "C237"] {
        row_all_formats(&d, row);
    }
    d
}

/// The one component with `purl`; fails on none or several.
fn by_purl<'a>(d: &'a Docs, purl: &str) -> &'a Value {
    let found: Vec<&Value> = components(&d.cdx).into_iter().filter(|c| c["purl"] == purl).collect();
    assert_eq!(found.len(), 1, "{purl}: {} components; all: {:?}", found.len(), purls(d));
    found[0]
}

fn absent_purl(d: &Docs, purl: &str) {
    assert!(!purls(d).iter().any(|p| p == purl), "{purl} must not appear");
}

fn purls(d: &Docs) -> Vec<String> {
    components(&d.cdx).into_iter().filter_map(|c| c["purl"].as_str().map(str::to_string)).collect()
}

/// CycloneDX `scope`, which is absent for a required component.
fn scope(c: &Value) -> &str {
    c["scope"].as_str().unwrap_or("required")
}

/// C237, parsed, agreeing across the three formats.
fn coverage(d: &Docs) -> Value {
    doc_field_all_formats(d, C237).unwrap_or_else(|| panic!("C237 missing"))
}

fn tree() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// The fixture tree copied under a fresh temporary parent, as a directory
/// with a fixed name: SPDX 3 IRIs and the document name depend on it.
fn copy_fixture(rel: &str) -> (tempfile::TempDir, PathBuf) {
    let parent = tempfile::tempdir().unwrap();
    let dst = parent.path().join("waybill-fixture-tree");
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(rel);
    copy_dir(&src, &dst);
    (parent, dst)
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let to = dst.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &to);
        } else {
            std::fs::copy(e.path(), &to).unwrap();
        }
    }
}

/// `v` masked for comparison across scans and hosts: the harness's masks,
/// the temporary parent's name, and C237 removed.
fn comparable(v: &Value) -> Value {
    let s = serde_json::to_string(&mask_nondeterministic(v)).unwrap();
    let re = regex::Regex::new(r"/[^\x22]*?/waybill-fixture-tree").unwrap();
    without_mentions(&serde_json::from_str(&re.replace_all(&s, "/TMP/waybill-fixture-tree")).unwrap(), C237)
}

/// `v` without any array element that mentions `needle`, at any depth.
fn without_mentions(v: &Value, needle: &str) -> Value {
    match v {
        Value::Array(a) => Value::Array(
            a.iter().filter(|e| !e.to_string().contains(needle)).map(|e| without_mentions(e, needle)).collect(),
        ),
        Value::Object(o) => Value::Object(o.iter().map(|(k, e)| (k.clone(), without_mentions(e, needle))).collect()),
        other => other.clone(),
    }
}

// ---------------------------------------------------------------------
// W-ctl: byte-identity control (T003, FR-009)
// ---------------------------------------------------------------------

const GOLDEN_DIR: &str = "tests/fixtures/cmake_tree_finds/ctl-golden";
const REGEN: &str = "WAYBILL_UPDATE_CMAKE_TREE_FINDS_GOLDENS";

/// Every package search in the narrow file set, unconditional, and a fetch
/// with no chain conditions: output is `main`'s, apart from C237. The golden
/// was generated with `main`'s binary before any source change (T003).
#[test]
fn w_ctl_is_byte_identical_to_main() {
    let (_parent, root) = copy_fixture("cmake_tree_finds/ctl");
    let d = scan(&root);
    let golden_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_DIR);
    for (name, doc) in [("cdx", &d.cdx), ("spdx23", &d.spdx23), ("spdx3", &d.spdx3)] {
        let got = comparable(doc);
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

// ---------------------------------------------------------------------
// User story 1: every package a tree searches for is in its SBOM (T007)
// ---------------------------------------------------------------------

/// W-1: a nested `find_package` becomes a component, with its range and its
/// file. Absent on `main` (measurements/trace.md (b)).
#[test]
fn w01_nested_find_package_is_a_component() {
    let t = tree();
    write(t.path(), "projects/a/CMakeLists.txt", "find_package(waybill_fixture_ext 2.1)\n");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill_fixture_ext");
    assert_eq!(scope(c), "required");
    assert_eq!(prop(c, "waybill:requirement-ranges"), Some("[\">=2.1\"]"));
    assert_eq!(source_files(c), vec!["projects/a/CMakeLists.txt".to_string()]);
}

/// W-2: two files searching for one package give one component with both
/// files and both ranges.
#[test]
fn w02_two_files_one_component_both_ranges() {
    let t = tree();
    write(t.path(), "projects/a/CMakeLists.txt", "find_package(waybill_fixture_ext 2.1)\n");
    write(t.path(), "projects/b/cmake/deps.cmake", "find_package(waybill_fixture_ext 3.0 EXACT)\n");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill_fixture_ext");
    assert_eq!(source_files(c), vec!["projects/a/CMakeLists.txt".to_string(), "projects/b/cmake/deps.cmake".to_string()]);
    let ranges: Vec<String> = serde_json::from_str(prop(c, "waybill:requirement-ranges").unwrap()).unwrap();
    assert_eq!(ranges, vec!["==3.0".to_string(), ">=2.1".to_string()]);
}

/// W-3: nested pkg-config searches become components as milestone 155 emits
/// them from the root.
#[test]
fn w03_nested_pkg_config_searches() {
    let t = tree();
    write(
        t.path(),
        "projects/a/CMakeLists.txt",
        "pkg_check_modules(X REQUIRED waybill-fixture-pc>=1.0)\npkg_search_module(Y waybill-fixture-ps)\n",
    );
    let d = scan(t.path());
    for purl in ["pkg:generic/waybill-fixture-pc", "pkg:generic/waybill-fixture-ps"] {
        let c = by_purl(&d, purl);
        assert_eq!(prop(c, "waybill:source-mechanism"), Some("cmake-pkg-check-modules"));
    }
}

/// W-4: a search whose name is a variable is counted, not emitted.
#[test]
fn w04_variable_name_is_counted_as_unresolved() {
    let t = tree();
    write(t.path(), "projects/a/CMakeLists.txt", "find_package(${WAYBILL_PKG})\nfind_package(waybill_fixture_known)\n");
    let d = scan(t.path());
    by_purl(&d, "pkg:generic/waybill_fixture_known");
    let cov = coverage(&d);
    assert_eq!(cov["unresolved"], 1, "{cov}");
    assert_eq!(cov["read"], 2, "{cov}");
    assert_eq!(cov["components"], 1, "{cov}");
}

/// W-16: a search inside a git submodule hangs off the submodule, not the
/// document root (research R8).
#[test]
fn w16_a_search_in_a_submodule_attaches_to_the_submodule() {
    let fx = GitFixture::new();
    fx.repo("dep");
    fx.write("dep/CMakeLists.txt", "find_package(waybill_fixture_w16 1.0)\n");
    fx.git(&fx.path("dep"), &["add", "CMakeLists.txt"]);
    fx.git(&fx.path("dep"), &["commit", "-q", "-m", "cmake"]);
    fx.repo("app");
    fx.add_submodule("app", "dep", "extern/waybill-fixture-dep");
    let d = scan(&fx.path("app"));
    let target = "pkg:generic/waybill_fixture_w16";
    by_purl(&d, target);
    let sub = components(&d.cdx)
        .into_iter()
        .find(|c| prop(c, "waybill:source-mechanism") == Some("git-submodule"))
        .and_then(|c| c["purl"].as_str())
        .expect("a submodule component")
        .to_string();
    assert!(depends_on(&d.cdx, &sub).contains(&target.to_string()), "{sub} must depend on {target}");
    let root = d.cdx["metadata"]["component"]["bom-ref"].as_str().unwrap_or("").to_string();
    let root_deps: Vec<String> = d.cdx["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["ref"] == root)
        .and_then(|e| e["dependsOn"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str().map(str::to_string))
        .collect();
    assert!(!root_deps.contains(&target.to_string()), "no root anchor: {root_deps:?}");
}
