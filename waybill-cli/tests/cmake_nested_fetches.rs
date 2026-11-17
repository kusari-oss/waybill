//! Spec 1538 (#1516, #1519): CMake fetch declarations in nested projects
//! are read, and the standalone-build fallbacks among them are marked
//! instead of presented as built.
//!
//! Each case is a contract row (`specs/1538-cmake-nested-fetches/contracts/
//! wire.md`, C-1 to C-15), built from synthetic `waybill-fixture-*` trees and
//! checked in CycloneDX, with the format-native spellings checked in SPDX
//! where the row names one. Every case also checks C231's accounting
//! identity (I-2) and the C122/C232 pairing (I-3). Nothing is fetched.

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
        .args(["--no-deep-hash", "--root-name", "waybill-fixture-root", "--root-version", "1"])
        .args(["--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
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
    let docs = Docs { cdx: read(&c), spdx23: read(&s2), spdx3: read(&s3) };
    assert_invariants(&docs);
    docs
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
    c["properties"].as_array()?.iter().find(|p| p["name"] == name).and_then(|p| p["value"].as_str())
}

fn is_fetch(c: &Value) -> bool {
    prop(c, "waybill:source-mechanism")
        .is_some_and(|m| m.starts_with("cmake-fetchcontent") || m == "cmake-externalproject")
}

fn fetches(docs: &Docs) -> Vec<&Value> {
    cdx_components(&docs.cdx).into_iter().filter(|c| is_fetch(c)).collect()
}

fn fetch_purls(docs: &Docs) -> BTreeSet<String> {
    fetches(docs).iter().filter_map(|c| c["purl"].as_str()).map(str::to_string).collect()
}

/// The one fetch component whose PURL starts with `prefix`.
fn the<'a>(docs: &'a Docs, prefix: &str) -> &'a Value {
    let hits: Vec<&Value> = fetches(docs)
        .into_iter()
        .filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with(prefix)))
        .collect();
    assert_eq!(hits.len(), 1, "exactly one fetch component {prefix}: {:?}", fetch_purls(docs));
    hits[0]
}

fn coverage(docs: &Docs) -> Option<Value> {
    prop(&docs.cdx["metadata"], "waybill:cmake-fetch-coverage").map(|v| serde_json::from_str(v).unwrap())
}

fn n(v: &Value, path: &[&str]) -> u64 {
    let mut x = v;
    for k in path {
        x = &x[*k];
    }
    x.as_u64().unwrap_or_else(|| panic!("missing {path:?} in {v}"))
}

fn conditions(c: &Value) -> Option<Vec<String>> {
    prop(c, "waybill:cmake-fetch-conditions").map(|v| serde_json::from_str(v).unwrap())
}

/// I-2 (C231 V1/V2) and I-3 (C122 and C232 go together), on every scan.
fn assert_invariants(docs: &Docs) {
    if let Some(c) = coverage(docs) {
        let read = n(&c, &["read"]);
        let accounted = n(&c, &["emitted"])
            + n(&c, &["merged", "first-declaration-wins"])
            + n(&c, &["merged", "existing-component"])
            + n(&c, &["unresolved", "identity"]);
        assert_eq!(read, accounted, "I-2 V1: every declaration read is accounted for: {c}");
        assert!(n(&c, &["conditional"]) + n(&c, &["test"]) <= n(&c, &["components"]), "I-2 V2: {c}");
        for doc in [&docs.spdx23, &docs.spdx3] {
            assert!(doc.to_string().contains("waybill:cmake-fetch-coverage"), "C231 in every format");
        }
    }
    for f in fetches(docs) {
        let derivation = prop(f, "waybill:optional-derivation").filter(|d| d.starts_with("cmake-"));
        let optional = f["scope"] == "excluded" && derivation.is_some();
        // A shape that comes from an enclosing block has condition text;
        // FIND_PACKAGE_ARGS, ALWAYS and a provider do not, and C122 names them.
        let from_block = ["cmake-not-target", "cmake-find-package-fallback", "cmake-conditional", "cmake-function-body"];
        if derivation.is_some_and(|d| from_block.contains(&d)) {
            assert!(conditions(f).is_some(), "I-3: a block-derived conditional fetch states its conditions: {f}");
        }
        if f["scope"] == "excluded" && prop(f, "waybill:lifecycle-scope") == Some("optional") {
            assert!(derivation.is_some(), "I-3: a conditional fetch names its shape (C122): {f}");
        }
        if conditions(f).is_some() {
            let test = prop(f, "waybill:lifecycle-scope") == Some("test");
            assert!(optional || test, "I-3: conditions only on a conditional or test fetch: {f}");
        }
    }
}

fn required(c: &Value) -> bool {
    c["scope"].is_null() && prop(c, "waybill:optional-derivation").is_none() && conditions(c).is_none()
}

fn assert_optional(c: &Value, derivation: &str) {
    assert_eq!(c["scope"], "excluded", "{c}");
    assert_eq!(prop(c, "waybill:optional-derivation"), Some(derivation), "{c}");
}

fn url_fetch(name: &str, version: &str, extra: &str) -> String {
    format!(
        "FetchContent_Declare({name} URL https://example.invalid/{name}-{version}.tar.gz URL_HASH SHA256={} {extra})\n",
        "5".repeat(64)
    )
}

const ROOT: &str = "cmake_minimum_required(VERSION 3.25)\nproject(waybill_fixture_superbuild)\ninclude(FetchContent)\n";

fn tree(files: &[(&str, &str)]) -> GitFixture {
    let fx = GitFixture::new();
    for (rel, text) in files {
        fx.write(&format!("t/{rel}"), text);
    }
    fx
}

/// C-1: a fetch in `third-party/fx`, reached by `add_subdirectory()`, is the
/// same component the same declaration gives in the root file.
#[test]
fn c1_a_fetch_in_a_hyphenated_third_party_project() {
    let decl = url_fetch("waybill-fixture-hyphen-dir", "5.0.0", "");
    let nested = tree(&[
        ("CMakeLists.txt", &format!("{ROOT}add_subdirectory(third-party/fx)\n")),
        ("third-party/fx/CMakeLists.txt", &decl),
    ]);
    let docs = scan(&nested, &nested.path("t"), &[]);
    let c = the(&docs, "pkg:generic/waybill-fixture-hyphen-dir@5.0.0");
    assert!(required(c), "{c}");
    let at_root = tree(&[("CMakeLists.txt", &format!("{ROOT}{decl}"))]);
    let root_docs = scan(&at_root, &at_root.path("t"), &[]);
    let r = the(&root_docs, "pkg:generic/waybill-fixture-hyphen-dir@5.0.0");
    assert_eq!(c["purl"], r["purl"]);
    assert_eq!(c["hashes"], r["hashes"]);
    assert_eq!(prop(c, "waybill:download-url"), prop(r, "waybill:download-url"));
}

/// C-2: sibling projects that nothing `add_subdirectory()`s are read.
#[test]
fn c2_unreached_sibling_projects_are_read() {
    let fx = tree(&[
        ("CMakeLists.txt", ROOT),
        ("a/CMakeLists.txt", &url_fetch("waybill-fixture-a", "1.0.0", "")),
        ("b/CMakeLists.txt", &url_fetch("waybill-fixture-b", "2.0.0", "")),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert!(required(the(&docs, "pkg:generic/waybill-fixture-a@1.0.0")));
    assert!(required(the(&docs, "pkg:generic/waybill-fixture-b@2.0.0")));
}

/// C-3 / C-3b: `FIND_PACKAGE_ARGS`, and `NEVER` cancelling it (probe P4).
#[test]
fn c3_find_package_args_and_never() {
    let inner = url_fetch("waybill-fixture-zlib", "1.3.0", "FIND_PACKAGE_ARGS NAMES ZLIB");
    let fx = tree(&[("CMakeLists.txt", ROOT), ("sub/CMakeLists.txt", &inner)]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert_optional(the(&docs, "pkg:generic/waybill-fixture-zlib@"), "cmake-find-package-args");
    let fx = tree(&[
        ("CMakeLists.txt", &format!("{ROOT}set(FETCHCONTENT_TRY_FIND_PACKAGE_MODE NEVER)\n")),
        ("sub/CMakeLists.txt", &inner),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert!(required(the(&docs, "pkg:generic/waybill-fixture-zlib@")), "NEVER cancels FIND_PACKAGE_ARGS");
}

/// C-4: `if(NOT TARGET …)`, in every format's optional spelling.
#[test]
fn c4_not_target_guard() {
    let fx = tree(&[
        ("CMakeLists.txt", ROOT),
        (
            "sub/CMakeLists.txt",
            &format!("if(NOT TARGET waybill_fixture_gtest)\n{}endif()\n", url_fetch("waybill-fixture-gtest", "1.10.0", "")),
        ),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:generic/waybill-fixture-gtest@1.10.0");
    assert_optional(c, "cmake-not-target");
    assert_eq!(conditions(c), Some(vec!["NOT TARGET waybill_fixture_gtest".to_string()]));
    let rels = docs.spdx23["relationships"].as_array().unwrap();
    assert!(rels.iter().any(|r| r["relationshipType"] == "OPTIONAL_DEPENDENCY_OF"), "SPDX 2.3 optional spelling");
}

/// C-5: `if(NOT <X>_FOUND)`, TheRock's commonest fallback.
#[test]
fn c5_find_package_fallback() {
    let fx = tree(&[
        ("CMakeLists.txt", ROOT),
        ("sub/CMakeLists.txt", &format!("if(NOT WaybillFixtureX_FOUND)\n{}endif()\n", url_fetch("waybill-fixture-x", "2.0.0", ""))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert_optional(the(&docs, "pkg:generic/waybill-fixture-x@"), "cmake-find-package-fallback");
}

/// C-6: `ALWAYS` marks FetchContent, never ExternalProject (probe P5).
#[test]
fn c6_try_find_package_always() {
    let fx = tree(&[
        ("CMakeLists.txt", &format!("{ROOT}set(FETCHCONTENT_TRY_FIND_PACKAGE_MODE ALWAYS)\n")),
        (
            "sub/CMakeLists.txt",
            &format!(
                "{}ExternalProject_Add(waybill-fixture-ep URL https://example.invalid/waybill-fixture-ep-3.0.0.tar.gz)\n",
                url_fetch("waybill-fixture-fc", "1.0.0", "")
            ),
        ),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert_optional(the(&docs, "pkg:generic/waybill-fixture-fc@"), "cmake-try-find-package-always");
    assert!(required(the(&docs, "pkg:generic/waybill-fixture-ep@")));
}

/// C-7: a provider counts only when it supports FetchContent.
#[test]
fn c7_dependency_provider_methods() {
    for (methods, marked) in [("FIND_PACKAGE FETCHCONTENT_MAKEAVAILABLE_SERIAL", true), ("FIND_PACKAGE", false)] {
        let fx = tree(&[
            ("CMakeLists.txt", ROOT),
            ("cmake/provider.cmake", &format!("cmake_language(SET_DEPENDENCY_PROVIDER wf_provider SUPPORTED_METHODS {methods})\n")),
            ("sub/CMakeLists.txt", &url_fetch("waybill-fixture-p", "1.0.0", "")),
        ]);
        let docs = scan(&fx, &fx.path("t"), &[]);
        let c = the(&docs, "pkg:generic/waybill-fixture-p@");
        if marked {
            assert_optional(c, "cmake-dependency-provider");
        } else {
            assert!(required(c), "a FIND_PACKAGE-only provider never sees FetchContent: {c}");
        }
    }
}

/// C-8: any `if()`, root file included (clarification 2).
#[test]
fn c8_any_condition_in_the_root_file() {
    let fx = tree(&[("CMakeLists.txt", &format!("{ROOT}if(WIN32)\n{}endif()\n", url_fetch("waybill-fixture-win", "1.0.0", "")))]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:generic/waybill-fixture-win@");
    assert_optional(c, "cmake-conditional");
    assert_eq!(conditions(c), Some(vec!["WIN32".to_string()]));
}

/// C-9: test directories and `BUILD_TESTING` give test scope; `OR` does not.
#[test]
fn c9_test_scope() {
    let fx = tree(&[
        ("CMakeLists.txt", &format!(
            "{ROOT}if(BUILD_TESTING AND WF_X)\n{}endif()\nif(BUILD_TESTING OR WF_X)\n{}endif()\n",
            url_fetch("waybill-fixture-bt", "1.0.0", ""),
            url_fetch("waybill-fixture-or", "1.0.0", "")
        )),
        ("tests/CMakeLists.txt", &url_fetch("waybill-fixture-td", "1.0.0", "")),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let td = the(&docs, "pkg:generic/waybill-fixture-td@");
    assert_eq!(prop(td, "waybill:lifecycle-scope"), Some("test"), "{td}");
    assert_eq!(prop(td, "waybill:lifecycle-scope-derivation"), Some("source-test-directory"));
    let bt = the(&docs, "pkg:generic/waybill-fixture-bt@");
    assert_eq!(prop(bt, "waybill:lifecycle-scope"), Some("test"), "{bt}");
    assert_eq!(prop(bt, "waybill:lifecycle-scope-derivation"), Some("cmake-build-testing"));
    assert_eq!(conditions(bt), Some(vec!["BUILD_TESTING AND WF_X".to_string()]));
    assert_optional(the(&docs, "pkg:generic/waybill-fixture-or@"), "cmake-conditional");
    assert_eq!(n(&coverage(&docs).unwrap(), &["test"]), 2);
}

/// C-10: an ancestor's declaration wins; the subproject's is recorded.
#[test]
fn c10_first_declaration_wins_over_a_descendant() {
    let fx = tree(&[
        ("CMakeLists.txt", &format!("{ROOT}{}add_subdirectory(sub)\n", url_fetch("waybill-fixture-pp", "1.0", ""))),
        ("sub/CMakeLists.txt", &url_fetch("waybill-fixture-pp", "2.0", "")),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert_eq!(fetch_purls(&docs).iter().filter(|p| p.contains("waybill-fixture-pp")).count(), 1);
    let c = the(&docs, "pkg:generic/waybill-fixture-pp@1.0");
    let files: Vec<String> = serde_json::from_str(prop(c, "waybill:source-files").unwrap()).unwrap();
    assert_eq!(files, vec!["CMakeLists.txt".to_string(), "sub/CMakeLists.txt".to_string()]);
    let sup: Value = serde_json::from_str(prop(c, "waybill:superseded-declarations").unwrap()).unwrap();
    assert_eq!(sup, serde_json::json!([{"file": "sub/CMakeLists.txt", "version": "2.0"}]));
    assert_eq!(n(&coverage(&docs).unwrap(), &["merged", "first-declaration-wins"]), 1);
}

/// C-11: siblings never merge (clarification 1).
#[test]
fn c11_siblings_stay_separate() {
    let fx = tree(&[
        ("CMakeLists.txt", ROOT),
        ("a/CMakeLists.txt", &url_fetch("waybill-fixture-pp", "1.0", "")),
        ("b/CMakeLists.txt", &url_fetch("waybill-fixture-pp", "2.0", "")),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    the(&docs, "pkg:generic/waybill-fixture-pp@1.0");
    the(&docs, "pkg:generic/waybill-fixture-pp@2.0");
}

/// C-12: a guarded fetch of a repository the scan already holds as a git
/// submodule is recorded on the submodule's component.
#[test]
fn c12_a_fallback_for_a_submodule_merges_into_it() {
    let fx = GitFixture::new();
    fx.repo("pdep");
    fx.repo("app");
    fx.add_submodule("app", "pdep", "third_party/pdep");
    let dep = fx.path("pdep");
    fx.write("app/CMakeLists.txt", ROOT);
    fx.write(
        "app/sub/CMakeLists.txt",
        &format!(
            "if(NOT TARGET pdep)\nFetchContent_Declare(waybill-fixture-pdep GIT_REPOSITORY {} GIT_TAG v9.0)\nendif()\n",
            dep.display()
        ),
    );
    let docs = scan(&fx, &fx.path("app"), &[]);
    assert!(
        !fetch_purls(&docs).iter().any(|p| p.contains("waybill-fixture-pdep")),
        "no second component: {:?}",
        fetch_purls(&docs)
    );
    let sub = cdx_components(&docs.cdx)
        .into_iter()
        .find(|c| prop(c, "waybill:superseded-declarations").is_some())
        .expect("the submodule records the fallback");
    let sup: Value = serde_json::from_str(prop(sub, "waybill:superseded-declarations").unwrap()).unwrap();
    assert_eq!(sup, serde_json::json!([{"file": "sub/CMakeLists.txt", "version": "v9.0"}]));
    assert_eq!(n(&coverage(&docs).unwrap(), &["merged", "existing-component"]), 1);
}

/// C-13: a function body; a parameterised name is counted, not guessed.
#[test]
fn c13_function_bodies() {
    let fx = tree(&[(
        "CMakeLists.txt",
        &format!(
            "{ROOT}function(wf_fetch name)\n{}FetchContent_Declare(${{name}} URL https://example.invalid/${{name}}-1.0.tar.gz)\nendfunction()\n",
            url_fetch("waybill-fixture-fn", "1.0.0", "")
        ),
    )]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:generic/waybill-fixture-fn@");
    assert_optional(c, "cmake-function-body");
    assert_eq!(conditions(c), Some(vec!["function: wf_fetch".to_string()]));
    assert_eq!(n(&coverage(&docs).unwrap(), &["unresolved", "identity"]), 1);
}

/// C-14: a fetch inside a submodule's tree is the submodule's dependency;
/// one in a plain directory is the root's.
#[test]
fn c14_owners() {
    let fx = GitFixture::new();
    fx.repo("x");
    fx.write("x/CMakeLists.txt", &url_fetch("waybill-fixture-inx", "1.0.0", ""));
    fx.git(&fx.path("x"), &["add", "-A"]);
    fx.git(&fx.path("x"), &["commit", "-q", "-m", "files"]);
    fx.repo("app");
    fx.add_submodule("app", "x", "third_party/x");
    fx.write("app/CMakeLists.txt", ROOT);
    fx.write("app/tools/CMakeLists.txt", &url_fetch("waybill-fixture-tool", "1.0.0", ""));
    let docs = scan(&fx, &fx.path("app"), &[]);
    let inx = the(&docs, "pkg:generic/waybill-fixture-inx@")["bom-ref"].as_str().unwrap().to_string();
    let tool = the(&docs, "pkg:generic/waybill-fixture-tool@")["bom-ref"].as_str().unwrap().to_string();
    let root = docs.cdx["metadata"]["component"]["bom-ref"].as_str().unwrap();
    let deps = docs.cdx["dependencies"].as_array().unwrap();
    let on = |r: &str| -> BTreeSet<String> {
        deps.iter()
            .filter(|d| d["ref"] == r)
            .flat_map(|d| d["dependsOn"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    };
    assert!(on(root).contains(&tool), "the plain directory's fetch is the root's");
    assert!(!on(root).contains(&inx), "the submodule's fetch is not the root's");
    let owner = deps
        .iter()
        .find(|d| d["dependsOn"].as_array().into_iter().flatten().any(|t| t == inx.as_str()))
        .and_then(|d| d["ref"].as_str())
        .unwrap();
    assert!(owner.contains("x"), "owned by the submodule: {owner}");
}

/// C-15: the skip set and `--exclude-path` still apply to the widened walk.
#[test]
fn c15_exclusions_apply() {
    let fx = tree(&[
        ("CMakeLists.txt", &format!("{ROOT}add_subdirectory(third-party/fx)\n")),
        ("third-party/fx/CMakeLists.txt", &url_fetch("waybill-fixture-hyphen-dir", "5.0.0", "")),
        ("vendor-x/CMakeLists.txt", &url_fetch("waybill-fixture-excluded", "1.0.0", "")),
        ("node_modules/n/CMakeLists.txt", &url_fetch("waybill-fixture-nm", "1.0.0", "")),
    ]);
    let docs = scan(&fx, &fx.path("t"), &["--exclude-path", "vendor-x"]);
    assert_eq!(fetch_purls(&docs).len(), 1, "{:?}", fetch_purls(&docs));
    assert_eq!(n(&coverage(&docs).unwrap(), &["read"]), 1);
}

/// I-4: no fetch declaration, no rows.
#[test]
fn i4_a_tree_without_fetches_has_no_rows() {
    let fx = tree(&[("CMakeLists.txt", &format!("{ROOT}find_package(Threads)\n"))]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert!(coverage(&docs).is_none());
    let text = docs.cdx.to_string();
    assert!(!text.contains("waybill:cmake-fetch-conditions") && !text.contains("waybill:superseded-declarations"));
}

/// C-12b (R9): an **unconditional** fetch of the submodule's repository at
/// another tag is a separate claim, never merged.
#[test]
fn c12b_an_unconditional_fetch_of_a_submodule_stays_separate() {
    let fx = GitFixture::new();
    fx.repo("pdep");
    fx.repo("app");
    fx.add_submodule("app", "pdep", "third_party/pdep");
    let dep = fx.path("pdep");
    fx.write("app/CMakeLists.txt", ROOT);
    fx.write(
        "app/sub/CMakeLists.txt",
        &format!("FetchContent_Declare(waybill-fixture-pdep GIT_REPOSITORY {} GIT_TAG v9.0)\n", dep.display()),
    );
    let docs = scan(&fx, &fx.path("app"), &[]);
    the(&docs, "pkg:generic/waybill-fixture-pdep@v9.0");
    assert_eq!(n(&coverage(&docs).unwrap(), &["merged", "existing-component"]), 0);
}


fn git_fetch(name: &str, tag: &str) -> String {
    format!("FetchContent_Declare({name} GIT_REPOSITORY /waybill-fixture/rr.git GIT_TAG {tag})\n")
}

fn guarded(body: &str) -> String {
    format!("if(NOT TARGET rr)\n{body}endif()\n")
}

/// C-12c (R9): a sibling's component is never a supplier. A guarded fetch of
/// a repository only a sibling project declares stays its own component.
#[test]
fn c12c_a_sibling_never_supplies_a_fallback() {
    let fx = tree(&[
        ("CMakeLists.txt", ROOT),
        ("a/CMakeLists.txt", &git_fetch("waybill-fixture-rr-a", "v1")),
        ("b/CMakeLists.txt", &guarded(&git_fetch("waybill-fixture-rr-b", "v2"))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    the(&docs, "pkg:generic/waybill-fixture-rr-a@v1");
    assert_optional(the(&docs, "pkg:generic/waybill-fixture-rr-b@v2"), "cmake-not-target");
    assert_eq!(n(&coverage(&docs).unwrap(), &["merged", "existing-component"]), 0);
}

/// C-12d (R9): the nearest enclosing supplier wins.
#[test]
fn c12d_the_nearest_enclosing_supplier_wins() {
    let fx = tree(&[
        ("CMakeLists.txt", &format!("{ROOT}{}", git_fetch("waybill-fixture-rr-a", "v1"))),
        ("mid/CMakeLists.txt", &git_fetch("waybill-fixture-rr-b", "v2")),
        ("mid/leaf/CMakeLists.txt", &guarded(&git_fetch("waybill-fixture-rr-c", "v3"))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert!(!fetch_purls(&docs).iter().any(|p| p.contains("rr-c")), "{:?}", fetch_purls(&docs));
    let mid = the(&docs, "pkg:generic/waybill-fixture-rr-b@v2");
    let sup: Value = serde_json::from_str(prop(mid, "waybill:superseded-declarations").unwrap()).unwrap();
    assert_eq!(sup, serde_json::json!([{"file": "mid/leaf/CMakeLists.txt", "version": "v3"}]));
    assert!(prop(the(&docs, "pkg:generic/waybill-fixture-rr-a@v1"), "waybill:superseded-declarations").is_none());
    assert_eq!(n(&coverage(&docs).unwrap(), &["merged", "existing-component"]), 1);
}

/// C-12e (R9): two suppliers at the same depth is no supplier.
#[test]
fn c12e_a_tie_between_suppliers_merges_nothing() {
    let fx = tree(&[
        (
            "CMakeLists.txt",
            &format!("{ROOT}{}{}", git_fetch("waybill-fixture-rr-a", "v1"), git_fetch("waybill-fixture-rr-b", "v2")),
        ),
        ("sub/CMakeLists.txt", &guarded(&git_fetch("waybill-fixture-rr-c", "v3"))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert_optional(the(&docs, "pkg:generic/waybill-fixture-rr-c@v3"), "cmake-not-target");
    assert_eq!(n(&coverage(&docs).unwrap(), &["merged", "existing-component"]), 0);
}
