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

// ---------------------------------------------------------------------
// User story 2: a conditional search is not a runtime requirement (T011)
// ---------------------------------------------------------------------

const C236: &str = "waybill:cmake-find-conditions";
const C232: &str = "waybill:cmake-fetch-conditions";

fn conds(c: &Value, row: &str) -> Vec<String> {
    prop(c, row).map(|v| serde_json::from_str(v).unwrap()).unwrap_or_default()
}

/// Optional with C122 `cmake-conditional`.
fn assert_optional(c: &Value) {
    assert_eq!(scope(c), "excluded", "{c}");
    assert_eq!(prop(c, "waybill:lifecycle-scope"), Some("optional"), "{c}");
    assert_eq!(prop(c, "waybill:optional-derivation"), Some("cmake-conditional"), "{c}");
}

/// W-5: a search inside `if()` in a nested file.
#[test]
fn w05_in_file_condition() {
    let t = tree();
    write(t.path(), "projects/a/CMakeLists.txt", "if(WAYBILL_OPT)\n  find_package(waybill_fixture_opt)\nendif()\n");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill_fixture_opt");
    assert_optional(c);
    assert_eq!(conds(c, C236), vec!["WAYBILL_OPT"]);
}

/// W-6: a fetch in a `*.cmake` file included only under `if()`. Required on
/// `main`, which reads each file on its own.
#[test]
fn w06_fetch_in_conditionally_included_file() {
    let t = tree();
    write(t.path(), "CMakeLists.txt", "project(waybill_fixture_root)\nif(WIN_SDK)\n  include(cmake/win.cmake)\nendif()\n");
    write(
        t.path(),
        "cmake/win.cmake",
        "include(FetchContent)\nFetchContent_Declare(waybill_fixture_win GIT_REPOSITORY https://github.com/waybill-fixture/win.git GIT_TAG v1.0.0)\n",
    );
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:github/waybill-fixture/win@v1.0.0");
    assert_eq!(scope(c), "excluded", "{c}");
    assert!(conds(c, C232).contains(&"WIN_SDK".to_string()), "{c}");
}

/// W-7: `BUILD_TESTING` reaching a directory through `add_subdirectory()`
/// makes its searches test-scoped. The directory is not named like a test
/// directory, so only the chain can say so.
#[test]
fn w07_build_testing_through_add_subdirectory() {
    let t = tree();
    write(t.path(), "CMakeLists.txt", "project(waybill_fixture_root)\nif(BUILD_TESTING)\n  add_subdirectory(checks)\nendif()\n");
    write(t.path(), "checks/CMakeLists.txt", "find_package(waybill_fixture_t)\n");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill_fixture_t");
    assert_eq!(scope(c), "excluded", "{c}");
    assert_eq!(prop(c, "waybill:lifecycle-scope-derivation"), Some("cmake-build-testing"), "{c}");
    assert!(conds(c, C236).contains(&"BUILD_TESTING".to_string()), "{c}");
}

/// W-8: conditions accumulate along an `add_subdirectory()` then
/// `include()` chain, and the file's own block comes last.
#[test]
fn w08_conditions_union_along_a_chain() {
    let t = tree();
    write(t.path(), "CMakeLists.txt", "project(waybill_fixture_root)\nif(A)\n  add_subdirectory(x)\nendif()\n");
    write(t.path(), "x/CMakeLists.txt", "include(y.cmake)\n");
    write(t.path(), "x/y.cmake", "if(B)\n  find_package(waybill_fixture_ab)\nendif()\n");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill_fixture_ab");
    assert_optional(c);
    assert_eq!(conds(c, C236), vec!["A", "B"]);
}

/// W-9: one unconditional search makes the package a runtime requirement.
#[test]
fn w09_one_unconditional_declaration_wins() {
    let t = tree();
    write(t.path(), "projects/a/CMakeLists.txt", "if(A)\n  find_package(waybill_fixture_both)\nendif()\n");
    write(t.path(), "projects/b/CMakeLists.txt", "find_package(waybill_fixture_both)\n");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill_fixture_both");
    assert_eq!(scope(c), "required", "{c}");
    assert!(prop(c, C236).is_none(), "{c}");
}

/// W-10: a file reached both under a condition and without one is
/// unconditional.
#[test]
fn w10_a_file_reached_both_ways_is_unconditional() {
    let t = tree();
    write(
        t.path(),
        "CMakeLists.txt",
        "project(waybill_fixture_root)\nif(A)\n  include(cmake/shared.cmake)\nendif()\ninclude(cmake/shared.cmake)\n",
    );
    write(t.path(), "cmake/shared.cmake", "find_package(waybill_fixture_shared)\n");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill_fixture_shared");
    assert_eq!(scope(c), "required", "{c}");
}

/// W-12: an include cycle terminates, and its files stay unconditional.
#[test]
fn w12_include_cycle_terminates() {
    let t = tree();
    write(t.path(), "CMakeLists.txt", "project(waybill_fixture_root)\ninclude(cmake/a.cmake)\n");
    write(t.path(), "cmake/a.cmake", "include(${CMAKE_CURRENT_LIST_DIR}/b.cmake)\nfind_package(waybill_fixture_cyc_a)\n");
    write(t.path(), "cmake/b.cmake", "include(${CMAKE_CURRENT_LIST_DIR}/a.cmake)\nfind_package(waybill_fixture_cyc_b)\n");
    let d = scan(t.path());
    for p in ["pkg:generic/waybill_fixture_cyc_a", "pkg:generic/waybill_fixture_cyc_b"] {
        assert_eq!(scope(by_purl(&d, p)), "required");
    }
}

/// W-17: a search in a `function()` body is treated as spec 1538 treats a
/// fetch in one: conditional, the body named among its conditions.
#[test]
fn w17_search_in_a_function_body() {
    let t = tree();
    write(t.path(), "projects/a/CMakeLists.txt", "function(waybill_deps)\n  find_package(waybill_fixture_body)\nendfunction()\n");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill_fixture_body");
    assert_optional(c);
    assert_eq!(conds(c, C236), vec!["function: waybill_deps"]);
}

// ---------------------------------------------------------------------
// User story 3: a search for something the tree provides is not a second
// component (T016)
// ---------------------------------------------------------------------

/// W-14a: a search paired with a fetch of the same name gives one
/// component, the fetched one, with the search's file in its evidence. Both
/// exist on `main` (measurements/trace.md (a)).
#[test]
fn w14a_search_paired_with_a_fetch() {
    let t = tree();
    write(
        t.path(),
        "CMakeLists.txt",
        "project(waybill_fixture_root)\ninclude(FetchContent)\nif(NOT waybill_fixture_fmt_FOUND)\n  FetchContent_Declare(waybill_fixture_fmt GIT_REPOSITORY https://github.com/waybill-fixture/fmt.git GIT_TAG v1.0.0)\nendif()\n",
    );
    write(t.path(), "projects/app/CMakeLists.txt", "find_package(waybill_fixture_fmt QUIET)\n");
    let d = scan(t.path());
    absent_purl(&d, "pkg:generic/waybill_fixture_fmt");
    let c = by_purl(&d, "pkg:github/waybill-fixture/fmt@v1.0.0");
    assert!(source_files(c).contains(&"projects/app/CMakeLists.txt".to_string()), "{c}");
    assert_eq!(coverage(&d)["paired"]["fetch"], 1);
}

fn installed_record(root: &Path) {
    write(
        root,
        "sdk/lib/cmake/waybill_fixture_inst/waybill_fixture_instConfigVersion.cmake",
        "set(PACKAGE_VERSION \"4.5.6\")\n",
    );
}

/// W-14b: a search paired with an installed-library record gives one
/// component, the record, with the search's file in its evidence. Both exist
/// on `main`.
#[test]
fn w14b_search_paired_with_an_installed_record() {
    let t = tree();
    installed_record(t.path());
    write(t.path(), "projects/app/CMakeLists.txt", "find_package(waybill_fixture_inst)\n");
    let d = scan(t.path());
    absent_purl(&d, "pkg:generic/waybill_fixture_inst");
    let c = by_purl(&d, "pkg:generic/waybill_fixture_inst@4.5.6");
    assert!(source_files(c).contains(&"projects/app/CMakeLists.txt".to_string()), "{c}");
    assert_eq!(scope(c), "required", "an unconditional search keeps the record required");
    assert_eq!(coverage(&d)["paired"]["installed"], 1);
}

/// W-14b, conditional: a record searched for only under a condition, and
/// linked and required by nothing else, is Optional with C236.
#[test]
fn w14b_a_record_searched_for_only_under_a_condition_is_optional() {
    let t = tree();
    installed_record(t.path());
    write(t.path(), "projects/app/CMakeLists.txt", "if(A)\n  find_package(waybill_fixture_inst)\nendif()\n");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill_fixture_inst@4.5.6");
    assert_optional(c);
    assert_eq!(conds(c, C236), vec!["A"]);
}

/// A git submodule at `extern/<name>` holding `body` as its CMakeLists.txt,
/// in a super-project whose `projects/app/CMakeLists.txt` is `app`.
fn submodule_tree(fx: &GitFixture, name: &str, body: &str, app: &str) -> PathBuf {
    fx.repo("dep");
    fx.write("dep/CMakeLists.txt", body);
    fx.git(&fx.path("dep"), &["add", "CMakeLists.txt"]);
    fx.git(&fx.path("dep"), &["commit", "-q", "-m", "cmake"]);
    fx.repo("app");
    fx.add_submodule("app", "dep", &format!("extern/{name}"));
    fx.write("app/projects/app/CMakeLists.txt", app);
    fx.path("app")
}

fn submodule(d: &Docs) -> &Value {
    components(&d.cdx)
        .into_iter()
        .find(|c| prop(c, "waybill:source-mechanism") == Some("git-submodule"))
        .expect("a submodule component")
}

/// W-14c: a search named like a git submodule's directory pairs with it,
/// and C57 sees the nested search (FR-008).
#[test]
fn w14c_search_paired_with_a_submodule_and_c57_widened() {
    let fx = GitFixture::new();
    let root = submodule_tree(&fx, "waybill_fixture_sub", "", "find_package(waybill_fixture_sub)\n");
    let d = scan(&root);
    absent_purl(&d, "pkg:generic/waybill_fixture_sub");
    let sub = submodule(&d);
    assert_eq!(prop(sub, "waybill:build-reference"), Some("declared-and-used"), "{sub}");
    assert_eq!(coverage(&d)["paired"]["submodule"], 1);
}

/// W-15 variant (analysis I2): a submodule that is also a CMake project
/// pairs with the submodule; it is not counted as built in the tree.
#[test]
fn w15_a_submodule_that_is_a_project_pairs_rather_than_counting_in_tree() {
    let fx = GitFixture::new();
    let root = submodule_tree(&fx, "waybill_fixture_sub2", "project(waybill_fixture_sub2)\n", "find_package(waybill_fixture_sub2)\n");
    let d = scan(&root);
    absent_purl(&d, "pkg:generic/waybill_fixture_sub2");
    let cov = coverage(&d);
    assert_eq!(cov["paired"]["submodule"], 1, "{cov}");
    assert_eq!(cov["in-tree"], serde_json::json!([]), "{cov}");
}

/// W-15: a search for a project the tree builds, by `project()` or by a
/// package-config template, yields no component and no edge.
#[test]
fn w15_a_search_for_something_built_in_the_tree() {
    let t = tree();
    write(t.path(), "projects/lib/CMakeLists.txt", "project(waybill_fixture_lib VERSION 3.0)\n");
    write(t.path(), "projects/cfg/cmake/waybill_fixture_cfgConfig.cmake.in", "@PACKAGE_INIT@\n");
    write(t.path(), "projects/app/CMakeLists.txt", "find_package(waybill_fixture_lib)\nfind_package(waybill_fixture_cfg)\nfind_package(waybill_fixture_cfg)\n");
    let d = scan(t.path());
    absent_purl(&d, "pkg:generic/waybill_fixture_lib");
    absent_purl(&d, "pkg:generic/waybill_fixture_cfg");
    let cov = coverage(&d);
    assert_eq!(cov["in-tree"], serde_json::json!(["waybill_fixture_cfg", "waybill_fixture_lib"]), "{cov}");
    assert_eq!(cov["in-tree-declarations"], 3, "{cov}");
    assert_eq!(cov["components"], 0, "{cov}");
}
