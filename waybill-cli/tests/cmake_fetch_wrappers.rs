//! Spec 1539 (#1518): CMake fetches made through user-defined wrappers.
//!
//! Each case is a contract row (`specs/1539-cmake-fetch-wrappers/contracts/
//! wire.md`, W-1 to W-24), built from synthetic `waybill-fixture-*` trees
//! and checked in CycloneDX, with C231's accounting (spec 1538 I-2), the
//! C122/C232 pairing (I-3), C234's shape (I-5) and the `wrappers` bounds
//! (I-6) checked on every scan. The binding rules are CMake 4.4.5's, as
//! measured in `measurements/cmake-semantics/results.txt`. Nothing is
//! fetched.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;

use common::cmake_fetch::*;
use common::git_fixture::GitFixture;
use serde_json::Value;

fn h() -> String {
    "5".repeat(64)
}

const ARGN: &str = "function(wf_argn name)\nExternalProject_Add(${name} ${ARGN})\nendfunction()\n";

/// TheRock's `therock_subproject_fetch` shape (`cmake/therock_subproject.cmake:147`).
const PARSE: &str = "function(wf_parse target_name)\n\
  cmake_parse_arguments(PARSE_ARGV 1 ARG \"CMAKE_PROJECT\" \"SOURCE_DIR;PREFIX;EXCLUDE_FROM_ALL\" \"TOUCH\")\n\
  if(NOT DEFINED ARG_PREFIX)\n    set(ARG_PREFIX \"${CMAKE_CURRENT_BINARY_DIR}/${target_name}_fetch\")\n  endif()\n\
  ExternalProject_Add(\"${target_name}\" EXCLUDE_FROM_ALL TRUE PREFIX \"${ARG_PREFIX}\" SOURCE_DIR \"${ARG_SOURCE_DIR}\"\n\
    CONFIGURE_COMMAND \"\" BUILD_COMMAND \"\" INSTALL_COMMAND \"\" ${_extra} ${ARG_UNPARSED_ARGUMENTS})\n\
endfunction()\n";

const NAMED: &str = "macro(wf_named)\n\
  cmake_parse_arguments(W \"\" \"NAME;URL;URL_HASH\" \"\" ${ARGN})\n\
  FetchContent_Declare(${W_NAME} URL ${W_URL} URL_HASH ${W_URL_HASH})\n\
endmacro()\n";

const OUTER: &str = "function(wf_outer n)\nwf_argn(${n} ${ARGN})\nendfunction()\n";

fn root(extra: &str) -> String {
    format!("{ROOT}include(ExternalProject)\ninclude(cmake/wrap.cmake)\n{extra}")
}

fn url(name: &str, version: &str) -> String {
    format!("https://example.invalid/{name}-{version}.tar.gz")
}

fn sha256(c: &Value) -> Option<&str> {
    c["hashes"].as_array()?.iter().find(|x| x["alg"] == "SHA-256").and_then(|x| x["content"].as_str())
}

fn source_files(c: &Value) -> Vec<String> {
    serde_json::from_str(prop(c, "waybill:source-files").unwrap()).unwrap()
}

fn chain(names: &[&str]) -> Option<Vec<Vec<String>>> {
    Some(vec![names.iter().map(|s| s.to_string()).collect()])
}

fn cov(docs: &Docs) -> Value {
    coverage(docs).unwrap()
}

fn wrappers(docs: &Docs) -> (u64, u64, u64) {
    let c = cov(docs);
    let w = &c["wrappers"];
    (n(w, &["definitions"]), n(w, &["calls"]), n(w, &["ambiguous-calls"]))
}

/// W-1: #1518's repro. A wrapper forwarding `${ARGN}`, called once, beside a
/// direct `ExternalProject_Add`.
#[test]
fn w1_an_argn_wrapper_call_is_a_component() {
    let fx = tree(&[
        ("cmake/wrap.cmake", ARGN),
        (
            "CMakeLists.txt",
            &root(&format!(
                "wf_argn(waybill-fixture-a URL {} URL_HASH SHA256={})\nExternalProject_Add(waybill-fixture-d URL {})\n",
                url("waybill-fixture-a", "2.1"),
                h(),
                url("waybill-fixture-dd", "3.0")
            )),
        ),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let a = the(&docs, "pkg:generic/waybill-fixture-a@2.1");
    assert_eq!(sha256(a), Some(h().as_str()), "{a}");
    assert_eq!(prop(a, "waybill:source-mechanism"), Some("cmake-externalproject"));
    assert!(required(a), "{a}");
    assert_eq!(source_files(a), vec!["CMakeLists.txt".to_string()]);
    assert_eq!(wrapper_chains(a), chain(&["wf_argn"]));
    // FR-011: a direct call keeps its declared name even where its archive
    // names another package.
    let d = the(&docs, "pkg:generic/waybill-fixture-d@3.0");
    assert_eq!(wrapper_chains(d), None, "a direct call carries no C234");
    assert_eq!(n(&cov(&docs), &["read"]), 2);
}

/// W-2: TheRock's shape. The URL arrives through
/// `${ARG_UNPARSED_ARGUMENTS}`, and the component is named from the archive.
#[test]
fn w2_therocks_shape_is_bound_and_named_from_the_archive() {
    let fx = tree(&[
        ("cmake/wrap.cmake", PARSE),
        ("CMakeLists.txt", &root("add_subdirectory(third-party/zstd)\n")),
        (
            "third-party/zstd/CMakeLists.txt",
            &format!(
                "set(_source_dir \"${{CMAKE_CURRENT_BINARY_DIR}}/source\")\n\
                 wf_parse(waybill-fixture-zstd-sources\n  SOURCE_DIR \"${{_source_dir}}\"\n\
                 # Originally mirrored from: \"https://example.invalid/upstream/waybill-fixture-zstd-1.5.7.tar.gz\"\n\
                 URL \"https://mirror.invalid/waybill-fixture-zstd-1.5.7.tar.gz\"\n  URL_HASH \"SHA256={}\"\n  TOUCH \"${{_stamp}}\")\n",
                h()
            ),
        ),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let z = the(&docs, "pkg:generic/waybill-fixture-zstd@1.5.7");
    assert_eq!(z["name"], "waybill-fixture-zstd", "FR-005: {z}");
    assert_eq!(sha256(z), Some(h().as_str()));
    assert_eq!(prop(z, "waybill:source-mechanism"), Some("cmake-externalproject"));
    assert_eq!(prop(z, "waybill:download-url"), Some("https://mirror.invalid/waybill-fixture-zstd-1.5.7.tar.gz"));
    assert!(required(z), "{z}");
    assert_eq!(source_files(z), vec!["third-party/zstd/CMakeLists.txt".to_string()], "C18 is the call site");
    assert_eq!(wrapper_chains(z), chain(&["wf_parse"]));
    assert!(!fetch_purls(&docs).iter().any(|p| p.contains("sources")), "{:?}", fetch_purls(&docs));
    assert_eq!(wrappers(&docs), (1, 1, 0));
}

/// W-3: a macro re-passing parsed keywords by name, into FetchContent.
#[test]
fn w3_parsed_variables_reach_fetchcontent() {
    let fx = tree(&[
        ("cmake/wrap.cmake", NAMED),
        (
            "CMakeLists.txt",
            &root(&format!(
                "wf_named(NAME waybill-fixture-n URL https://example.invalid/waybill-fixture-n-3.4.zip URL_HASH SHA256={})\n",
                h()
            )),
        ),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:generic/waybill-fixture-n@3.4");
    assert_eq!(prop(c, "waybill:source-mechanism"), Some("cmake-fetchcontent-url"));
    assert_eq!(sha256(c), Some(h().as_str()));
    assert_eq!(wrapper_chains(c), chain(&["wf_named"]));
}

/// W-4 and W-5: five calls are five components; the template is not a
/// declaration.
#[test]
fn w4_five_calls_five_components() {
    let calls: String = (1..=5).map(|i| format!("wf_argn(waybill-fixture-v{i} URL {})\n", url(&format!("waybill-fixture-v{i}"), "1.0"))).collect();
    let fx = tree(&[
        ("cmake/wrap.cmake", ARGN),
        ("CMakeLists.txt", &root(&format!("{calls}ExternalProject_Add(waybill-fixture-d URL {})\n", url("waybill-fixture-d", "3.0")))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    for i in 1..=5 {
        the(&docs, &format!("pkg:generic/waybill-fixture-v{i}@1.0"));
    }
    let c = cov(&docs);
    assert_eq!((n(&c, &["read"]), n(&c, &["emitted"]), n(&c, &["unresolved", "identity"])), (6, 6, 0), "{c}");
    assert_eq!(wrappers(&docs), (1, 5, 0));
}

/// T016: TheRock's fftw shape. Two calls of one URL through one wrapper,
/// under two conditions, are one component with one chain, standing for
/// two declarations.
#[test]
fn w4b_two_calls_of_one_url_are_one_component_with_one_chain() {
    let call = format!("wf_argn(waybill-fixture-fftw-sources URL {})\n", url("waybill-fixture-fftw", "3.3.10"));
    let fx = tree(&[
        ("cmake/wrap.cmake", ARGN),
        ("CMakeLists.txt", &root(&format!("if(A)\n{call}endif()\nif(B)\n{call}endif()\n"))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:generic/waybill-fixture-fftw@3.3.10");
    assert_eq!(wrapper_chains(c), chain(&["wf_argn"]));
    assert_eq!(conditions(c), Some(vec!["A".to_string(), "B".to_string()]));
    let cv = cov(&docs);
    assert_eq!((n(&cv, &["emitted"]), n(&cv, &["components"])), (2, 1), "{cv}");
}

/// FR-009: a component that also has a direct declaration keeps the chain
/// of its wrapped one. The direct, unconditional declaration is the one the
/// component is built from, so the chain arrives only through the union.
#[test]
fn w4c_a_direct_and_a_wrapped_declaration_of_one_purl_keep_the_chain() {
    let u = url("waybill-fixture-mix", "1.0");
    let fx = tree(&[
        ("cmake/wrap.cmake", ARGN),
        ("CMakeLists.txt", &root(&format!("ExternalProject_Add(waybill-fixture-mix URL {u})\nif(A)\nwf_argn(waybill-fixture-mix-sources URL {u})\nendif()\n"))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:generic/waybill-fixture-mix@1.0");
    assert!(required(c), "{c}");
    assert_eq!(wrapper_chains(c), chain(&["wf_argn"]));
}

#[test]
fn w5_a_wrapper_never_called_emits_and_counts_nothing() {
    let fx = tree(&[("cmake/wrap.cmake", ARGN), ("CMakeLists.txt", &root(""))]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert!(fetches(&docs).is_empty());
    assert!(coverage(&docs).is_none(), "nothing read: no C231 (I-4)");
}

/// W-6: an in-tree build through a wrapper names no URL.
#[test]
fn w6_an_in_tree_wrapper_call_is_unresolved() {
    let fx = tree(&[
        ("cmake/wrap.cmake", PARSE),
        ("CMakeLists.txt", &root("wf_parse(waybill-fixture-runtimes SOURCE_DIR \"${CMAKE_SOURCE_DIR}/runtimes\")\n")),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert!(fetches(&docs).is_empty(), "{:?}", fetch_purls(&docs));
    let c = cov(&docs);
    assert_eq!((n(&c, &["read"]), n(&c, &["unresolved", "identity"])), (1, 1), "{c}");
    assert_eq!(wrappers(&docs), (1, 1, 0), "the call is read, the template is not");
}

/// W-7: a call inside a loop is read once and never guessed.
#[test]
fn w7_a_loop_variable_is_not_guessed() {
    let fx = tree(&[
        ("cmake/wrap.cmake", ARGN),
        ("CMakeLists.txt", &root("foreach(d a b)\n  wf_argn(${d} URL https://example.invalid/${d}-1.0.tar.gz)\nendforeach()\n")),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert!(fetches(&docs).is_empty(), "{:?}", fetch_purls(&docs));
    let c = cov(&docs);
    assert_eq!((n(&c, &["read"]), n(&c, &["unresolved", "identity"])), (1, 1), "{c}");
    assert_eq!(wrappers(&docs), (1, 1, 0));
}

/// W-8: a wrapper of a wrapper.
#[test]
fn w8_a_wrapper_chain_is_followed() {
    let fx = tree(&[
        ("cmake/wrap.cmake", &format!("{OUTER}{ARGN}")),
        ("CMakeLists.txt", &root("wf_outer(waybill-fixture-o URL https://example.invalid/waybill-fixture-o-0.9.tgz)\n")),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let o = the(&docs, "pkg:generic/waybill-fixture-o@0.9");
    assert_eq!(wrapper_chains(o), chain(&["wf_outer", "wf_argn"]));
    assert_eq!(wrappers(&docs), (2, 1, 0));
}

/// W-9: a cycle can reach a built-in only through a second definition. It
/// terminates, and the call is unresolved.
#[test]
fn w9_a_cycle_terminates_unresolved() {
    let fx = tree(&[
        (
            "cmake/wrap.cmake",
            "function(wf_c n)\nExternalProject_Add(${n} ${ARGN})\nendfunction()\nfunction(wf_r n)\nwf_c(${n} ${ARGN})\nendfunction()\n",
        ),
        ("cmake/other.cmake", "function(wf_c n)\nwf_r(${n} ${ARGN})\nendfunction()\n"),
        ("CMakeLists.txt", &root(&format!("wf_r(waybill-fixture-cy URL {})\n", url("waybill-fixture-cy", "1.0")))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert!(fetches(&docs).is_empty(), "{:?}", fetch_purls(&docs));
    let c = cov(&docs);
    assert_eq!(n(&c, &["unresolved", "identity"]), 1, "{c}");
    assert_eq!(wrappers(&docs), (3, 1, 1), "one definition fetches, the other recurses: they disagree");
}

/// W-10, W-10b, W-10c: a wrapper defined twice (FR-002, clarified).
#[test]
fn w10_definitions_must_agree() {
    let call = format!("wf_dup(waybill-fixture-q URL {})\n", url("waybill-fixture-q", "1.0"));
    let dup = "function(wf_dup n)\nExternalProject_Add(${n} ${ARGN})\nendfunction()\n";
    let same = tree(&[("a/wrap.cmake", dup), ("b/wrap.cmake", dup), ("CMakeLists.txt", &root(&call))]);
    let docs = scan(&same, &same.path("t"), &[]);
    the(&docs, "pkg:generic/waybill-fixture-q@1.0");
    assert_eq!(wrappers(&docs), (2, 1, 0), "W-10");

    let other = "function(wf_dup n)\nExternalProject_Add(${n} URL https://example.invalid/waybill-fixture-other-2.0.tar.gz)\nendfunction()\n";
    let differ = tree(&[("a/wrap.cmake", dup), ("b/wrap.cmake", other), ("CMakeLists.txt", &root(&call))]);
    let docs = scan(&differ, &differ.path("t"), &[]);
    assert!(fetches(&docs).is_empty(), "W-10b: {:?}", fetch_purls(&docs));
    assert_eq!(n(&cov(&docs), &["unresolved", "identity"]), 1);
    assert_eq!(wrappers(&docs).2, 1, "W-10b");

    let plain = "function(wf_dup n)\nmessage(STATUS \"no fetch\")\nendfunction()\n";
    let mixed = tree(&[("a/wrap.cmake", dup), ("b/wrap.cmake", plain), ("CMakeLists.txt", &root(&call))]);
    let docs = scan(&mixed, &mixed.path("t"), &[]);
    assert!(fetches(&docs).is_empty(), "W-10c: {:?}", fetch_purls(&docs));
    assert_eq!(wrappers(&docs), (1, 1, 1), "W-10c");
}

/// W-11: probe W1. A multi-value keyword before `URL` swallows it, and
/// CMake fetches nothing; a reader searching for `URL` would emit a
/// component CMake never downloads.
#[test]
fn w11_a_multi_value_keyword_swallows_the_url() {
    let fx = tree(&[
        ("cmake/wrap.cmake", PARSE),
        ("CMakeLists.txt", &root(&format!("wf_parse(waybill-fixture-sw TOUCH t URL {})\n", url("waybill-fixture-sw", "1.0")))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert!(fetches(&docs).is_empty(), "{:?}", fetch_purls(&docs));
    assert_eq!(n(&cov(&docs), &["unresolved", "identity"]), 1);
    assert_eq!(wrappers(&docs), (1, 1, 0));
}

/// W-12: probes W2 and W3. The body's own `URL` comes first, and CMake
/// fetches the first.
#[test]
fn w12_the_bodys_url_comes_first() {
    let fx = tree(&[
        (
            "cmake/wrap.cmake",
            &format!("function(wf_body name)\nExternalProject_Add(${{name}} URL {} ${{ARGN}})\nendfunction()\n", url("waybill-fixture-body", "1.0")),
        ),
        ("CMakeLists.txt", &root(&format!("wf_body(waybill-fixture-w12 URL {})\n", url("waybill-fixture-call", "2.0")))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    the(&docs, "pkg:generic/waybill-fixture-body@1.0");
    assert!(!fetch_purls(&docs).iter().any(|p| p.contains("waybill-fixture-call")), "{:?}", fetch_purls(&docs));
}

/// W-13: probe W8. A default the body sets applies only when the call did
/// not set it.
#[test]
fn w13_a_body_default_applies_when_the_call_omits_it() {
    let fx = tree(&[
        (
            "cmake/wrap.cmake",
            &format!(
                "function(wf_def name)\ncmake_parse_arguments(PARSE_ARGV 1 ARG \"\" \"URL\" \"\")\nif(NOT DEFINED ARG_URL)\nset(ARG_URL {})\nendif()\nExternalProject_Add(${{name}} URL ${{ARG_URL}})\nendfunction()\n",
                url("waybill-fixture-def", "1.0")
            ),
        ),
        ("CMakeLists.txt", &root(&format!("wf_def(waybill-fixture-e8a)\nwf_def(waybill-fixture-e8b URL {})\n", url("waybill-fixture-e8b", "2.0")))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    the(&docs, "pkg:generic/waybill-fixture-def@1.0");
    the(&docs, "pkg:generic/waybill-fixture-e8b@2.0");
}

/// W-14: conditions from the call site, then from inside the wrapper body.
#[test]
fn w14_call_site_and_body_conditions() {
    let fx = tree(&[
        ("cmake/wrap.cmake", "function(wf_win name)\nif(WIN32)\nExternalProject_Add(${name} ${ARGN})\nendif()\nendfunction()\n"),
        (
            "CMakeLists.txt",
            &root(&format!("if(NOT WaybillFixtureX_FOUND)\nwf_win(waybill-fixture-x URL {})\nendif()\n", url("waybill-fixture-x", "1.0"))),
        ),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let x = the(&docs, "pkg:generic/waybill-fixture-x@1.0");
    assert_optional(x, "cmake-find-package-fallback");
    assert_eq!(conditions(x), Some(vec!["NOT WaybillFixtureX_FOUND".to_string(), "WIN32".to_string()]));
}

/// W-15: test scope comes from the call site's directory.
#[test]
fn w15_a_call_in_a_test_directory_is_test_scoped() {
    let fx = tree(&[
        ("cmake/wrap.cmake", ARGN),
        ("CMakeLists.txt", &root("")),
        ("tests/CMakeLists.txt", &format!("wf_argn(waybill-fixture-t URL {})\n", url("waybill-fixture-t", "1.0"))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let t = the(&docs, "pkg:generic/waybill-fixture-t@1.0");
    assert_eq!(prop(t, "waybill:lifecycle-scope"), Some("test"), "{t}");
    assert_eq!(prop(t, "waybill:lifecycle-scope-derivation"), Some("source-test-directory"));
}

/// W-16: first declaration wins by the bound name.
#[test]
fn w16_first_declaration_wins_over_a_wrapped_call() {
    let fx = tree(&[
        ("cmake/wrap.cmake", "function(wf_fc name)\nFetchContent_Declare(${name} ${ARGN})\nendfunction()\n"),
        ("CMakeLists.txt", &root(&format!("{}add_subdirectory(sub)\n", url_fetch("waybill-fixture-p", "1.0", "")))),
        ("sub/CMakeLists.txt", &format!("wf_fc(waybill-fixture-p URL {})\n", url("waybill-fixture-p", "2.0"))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let p = the(&docs, "pkg:generic/waybill-fixture-p@1.0");
    let sup: Value = serde_json::from_str(prop(p, "waybill:superseded-declarations").unwrap()).unwrap();
    assert_eq!(sup, serde_json::json!([{"file": "sub/CMakeLists.txt", "version": "2.0"}]));
    assert_eq!(wrapper_chains(p), None, "the survivor's own declaration is direct (R10)");
    assert_eq!(n(&cov(&docs), &["merged", "first-declaration-wins"]), 1);
}

/// W-17: a call inside a function that is not a wrapper is a function-body
/// fetch, from the call site's function.
#[test]
fn w17_a_call_inside_a_plain_function() {
    let fx = tree(&[
        ("cmake/wrap.cmake", ARGN),
        (
            "CMakeLists.txt",
            &root(&format!("function(add_deps)\nwf_argn(waybill-fixture-f URL {})\nendfunction()\n", url("waybill-fixture-f", "1.0"))),
        ),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let f = the(&docs, "pkg:generic/waybill-fixture-f@1.0");
    assert_optional(f, "cmake-function-body");
    assert_eq!(conditions(f), Some(vec!["function: add_deps".to_string()]));
    assert_eq!(wrapper_chains(f), chain(&["wf_argn"]));
}

/// W-18 and W-18b: FR-005's fallback, and repositories named as direct
/// calls name them.
#[test]
fn w18_names_that_do_not_parse_keep_the_bound_name() {
    let fx = tree(&[
        ("cmake/wrap.cmake", ARGN),
        (
            "CMakeLists.txt",
            &root(
                "wf_argn(waybill-fixture-v URL https://example.invalid/a/v2.1.tar.gz)\n\
                 wf_argn(waybill-fixture-dl URL https://example.invalid/download?id=7)\n\
                 wf_argn(waybill-fixture-nv URL https://example.invalid/waybill-fixture-q.tar.gz)\n\
                 wf_argn(waybill-fixture-gg GIT_REPOSITORY https://github.com/waybill-org/waybill-fixture-g.git GIT_TAG v1)\n",
            ),
        ),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let got: BTreeSet<String> = fetch_purls(&docs);
    let want: BTreeSet<String> = [
        "pkg:generic/waybill-fixture-v@2.1",
        "pkg:generic/waybill-fixture-dl@unknown",
        "pkg:generic/waybill-fixture-nv@unknown",
        "pkg:github/waybill-org/waybill-fixture-g@v1",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(got, want);
}

/// W-19: spec 1538's C-13b body has an identity of its own, so it is not a
/// wrapper, and stays as spec 1538 emits it.
#[test]
fn w19_a_body_with_its_own_identity_is_not_a_wrapper() {
    let fx = tree(&[(
        "CMakeLists.txt",
        &root("function(wf_deps dep_target)\nExternalProject_Add(${dep_target} GIT_REPOSITORY https://github.com/waybill-fixture/waybill-fixture-gh.git GIT_TAG v1.2)\nendfunction()\nwf_deps(x)\n"),
    ), ("cmake/wrap.cmake", "")]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:github/waybill-fixture/waybill-fixture-gh@v1.2");
    assert_optional(c, "cmake-function-body");
    assert_eq!(wrapper_chains(c), None);
    let cv = cov(&docs);
    assert!(cv.get("wrappers").is_none(), "no wrapper defined: {cv}");
    assert_eq!(n(&cv, &["read"]), 1, "the call is not read: {cv}");
}

/// W-20: a wrapper defined in an excluded directory is not seen, and its
/// call is counted nowhere.
#[test]
fn w20_a_definition_outside_the_scan_is_not_seen() {
    let fx = tree(&[
        ("vendor-x/wrap.cmake", ARGN),
        ("CMakeLists.txt", &format!("{ROOT}wf_argn(waybill-fixture-ex URL {})\n", url("waybill-fixture-ex", "1.0"))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &["--exclude-path", "vendor-x"]);
    assert!(fetches(&docs).is_empty(), "{:?}", fetch_purls(&docs));
    assert!(coverage(&docs).is_none());
}

/// W-21: `${ARGV0}` and `${ARGV1}` bind by position.
#[test]
fn w21_positional_arguments() {
    let fx = tree(&[
        ("cmake/wrap.cmake", "macro(wf_pos)\nFetchContent_Declare(${ARGV0} URL ${ARGV1})\nendmacro()\n"),
        ("CMakeLists.txt", &root("wf_pos(waybill-fixture-pos https://example.invalid/waybill-fixture-pos-1.2.zip)\n")),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:generic/waybill-fixture-pos@1.2");
    assert_eq!(wrapper_chains(c), chain(&["wf_pos"]));
}

/// W-22: command names are case-insensitive, in the call and in the
/// definition.
#[test]
fn w22_a_call_in_another_case_is_the_same_call() {
    let fx = tree(&[
        ("cmake/wrap.cmake", &PARSE.replace("function(wf_parse", "function(WF_Parse")),
        ("CMakeLists.txt", &root(&format!("WF_PARSE(waybill-fixture-cs-sources URL {})\n", url("waybill-fixture-cs", "1.0")))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:generic/waybill-fixture-cs@1.0");
    assert_eq!(wrapper_chains(c), chain(&["WF_Parse"]), "named as defined");
}

/// W-23: spec 1538's C-12f through a wrapper. A guarded wrapped fetch of a
/// submodule's repository at its commit merges into the submodule.
#[test]
fn w23_a_wrapped_fallback_merges_into_the_submodule() {
    let sha = "a".repeat(40);
    let fx = GitFixture::new();
    fx.repo("app");
    fx.write(
        "app/.gitmodules",
        "[submodule \"g\"]\n\tpath = elsewhere/waybill-fixture-g\n\turl = https://github.com/waybill-org/waybill-fixture-g.git\n",
    );
    fx.write("app/cmake/wrap.cmake", ARGN);
    fx.write("app/CMakeLists.txt", &root(""));
    fx.write(
        "app/a/b/CMakeLists.txt",
        &format!("if(NOT TARGET g)\nwf_argn(waybill-fixture-g GIT_REPOSITORY https://github.com/waybill-org/waybill-fixture-g.git GIT_TAG {sha})\nendif()\n"),
    );
    let app = fx.path("app");
    fx.git(&app, &["add", ".gitmodules"]);
    fx.add_gitlink("app", "elsewhere/waybill-fixture-g", &sha);
    let docs = scan(&fx, &app, &[]);
    let purl = format!("pkg:github/waybill-org/waybill-fixture-g@{sha}");
    let all: Vec<&Value> = cdx_components(&docs.cdx).into_iter().filter(|c| c["purl"] == purl.as_str()).collect();
    assert_eq!(all.len(), 1, "one component");
    let c = all[0];
    assert_eq!(prop(c, "waybill:source-mechanism"), Some("git-submodule"), "{c}");
    assert_eq!(wrapper_chains(c), None, "the survivor is the submodule (R10)");
    let sup: Value = serde_json::from_str(prop(c, "waybill:superseded-declarations").unwrap()).unwrap();
    assert_eq!(sup, serde_json::json!([{"file": "a/b/CMakeLists.txt", "version": sha}]));
    assert_eq!(n(&cov(&docs), &["merged", "existing-component"]), 1);
}

/// W-24: spec 1538's C-14 through a wrapper. A wrapped fetch called inside a
/// submodule's tree is the submodule's dependency.
#[test]
fn w24_a_wrapped_call_in_a_submodule_is_its_dependency() {
    let fx = GitFixture::new();
    fx.repo("x");
    fx.write("x/CMakeLists.txt", &format!("wf_argn(waybill-fixture-inx URL {})\n", url("waybill-fixture-inx", "1.0")));
    fx.git(&fx.path("x"), &["add", "-A"]);
    fx.git(&fx.path("x"), &["commit", "-q", "-m", "files"]);
    fx.repo("app");
    fx.add_submodule("app", "x", "third_party/x");
    fx.write("app/cmake/wrap.cmake", ARGN);
    fx.write("app/CMakeLists.txt", &root(""));
    let docs = scan(&fx, &fx.path("app"), &[]);
    let inx = the(&docs, "pkg:generic/waybill-fixture-inx@1.0");
    assert_eq!(wrapper_chains(inx), chain(&["wf_argn"]));
    let inx = inx["bom-ref"].as_str().unwrap().to_string();
    let root_ref = docs.cdx["metadata"]["component"]["bom-ref"].as_str().unwrap();
    let deps = docs.cdx["dependencies"].as_array().unwrap();
    let owner = deps
        .iter()
        .find(|d| d["dependsOn"].as_array().into_iter().flatten().any(|t| t == inx.as_str()))
        .and_then(|d| d["ref"].as_str())
        .unwrap();
    assert_ne!(owner, root_ref, "not the root's");
    assert!(owner.contains("x"), "owned by the submodule: {owner}");
}

/// W-25: TheRock's openmpi, simde, xz and sqlite calls build the URL from
/// a same-file literal `set()`, which is expanded at the call site.
#[test]
fn w25_a_call_site_variable_with_a_literal_set_is_expanded() {
    let fx = tree(&[
        ("cmake/wrap.cmake", PARSE),
        (
            "CMakeLists.txt",
            &root(
                "set(WF_V \"5.0.9\")\nset(_u \"https://mirror.invalid/waybill-fixture-xz-5.8.1.tar.bz2\")\n\
                 wf_parse(waybill-fixture-mpi-sources URL \"https://mirror.invalid/waybill-fixture-mpi-${WF_V}.tar.bz2\")\n\
                 wf_parse(waybill-fixture-lzma-sources URL \"${_u}\")\n",
            ),
        ),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    the(&docs, "pkg:generic/waybill-fixture-mpi@5.0.9");
    the(&docs, "pkg:generic/waybill-fixture-xz@5.8.1");
}

/// W-26: a call-site variable expands before the call, as in CMake. A
/// keyword held in a variable is a keyword: here the multi-value `TOUCH`
/// swallows the `URL` (probe W1), and nothing is fetched.
#[test]
fn w26_a_keyword_in_a_call_site_variable_is_a_keyword() {
    let fx = tree(&[
        ("cmake/wrap.cmake", PARSE),
        ("CMakeLists.txt", &root(&format!("set(_kw TOUCH)\nwf_parse(waybill-fixture-kw ${{_kw}} t URL {})\n", url("waybill-fixture-kw", "1.0")))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert!(fetches(&docs).is_empty(), "{:?}", fetch_purls(&docs));
    assert_eq!(n(&cov(&docs), &["unresolved", "identity"]), 1);
}

/// D-18 (spec 1540 FR-002): a spec 1539 wrapper whose
/// `cmake_parse_arguments` keyword lists are held in variables binds with
/// the resolved keywords. Read literally, nothing was parsed.
#[test]
fn d18_a_wrapper_with_keyword_lists_in_variables_binds() {
    let fx = tree(&[
        (
            "cmake/wrap.cmake",
            "function(wf_vars)\nset(one NAME URL)\ncmake_parse_arguments(W \"\" \"${one}\" \"\" ${ARGN})\nFetchContent_Declare(${W_NAME} URL ${W_URL})\nendfunction()\n",
        ),
        ("CMakeLists.txt", &root(&format!("wf_vars(NAME waybill-fixture-kv URL {})\n", url("waybill-fixture-kv", "1.0")))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:generic/waybill-fixture-kv@1.0");
    assert_eq!(wrapper_chains(c), chain(&["wf_vars"]));
}

// ---------------------------------------------------------------------
// Spec 1540 (#1594): DownloadProject's download_project(), recognised by
// name and shape (`specs/1540-cmake-download-project/contracts/wire.md`,
// D-1 to D-19). The module below is written for these tests; it has
// DownloadProject's shape and is not a copy of the upstream file.
// ---------------------------------------------------------------------

/// A DownloadProject-shaped module. `extra` adds a one-value keyword;
/// `guard` puts `set(oneValueArgs …)` under `if(<guard>)`.
fn dp_module(extra: &str, guard: Option<&str>) -> String {
    let one = format!("set(oneValueArgs PROJ PREFIX DOWNLOAD_DIR SOURCE_DIR BINARY_DIR BUILD_PROJECT{extra})\n");
    let one = match guard {
        Some(g) => format!("if({g})\n{one}endif()\n"),
        None => one,
    };
    format!(
        "set(_DownloadProjectDir \"${{CMAKE_CURRENT_LIST_DIR}}\")\n\
         function(download_project)\n\
         set(options QUIET)\n{one}set(multiValueArgs \"\")\n\
         cmake_parse_arguments(DL_ARGS \"${{options}}\" \"${{oneValueArgs}}\" \"${{multiValueArgs}}\" ${{ARGN}})\n\
         if(NOT DL_ARGS_PREFIX)\nset(DL_ARGS_PREFIX \"${{CMAKE_BINARY_DIR}}\")\nendif()\n\
         if(NOT DL_ARGS_DOWNLOAD_DIR)\nset(DL_ARGS_DOWNLOAD_DIR \"${{DL_ARGS_PREFIX}}/${{DL_ARGS_PROJ}}-download\")\nendif()\n\
         configure_file(\"${{_DownloadProjectDir}}/DownloadProject.CMakeLists.cmake.in\" \"${{DL_ARGS_DOWNLOAD_DIR}}/CMakeLists.txt\")\n\
         execute_process(COMMAND ${{CMAKE_COMMAND}} . WORKING_DIRECTORY \"${{DL_ARGS_DOWNLOAD_DIR}}\")\n\
         execute_process(COMMAND ${{CMAKE_COMMAND}} --build . WORKING_DIRECTORY \"${{DL_ARGS_DOWNLOAD_DIR}}\")\n\
         endfunction()\n"
    )
}

fn dp_root(extra: &str) -> String {
    format!("{ROOT}include(cmake/DownloadProject.cmake)\n{extra}")
}

fn dp_tree(calls: &str) -> GitFixture {
    tree(&[("cmake/DownloadProject.cmake", &dp_module("", None)), ("CMakeLists.txt", &dp_root(calls))])
}

fn dp_chain() -> Option<Vec<Vec<String>>> {
    chain(&["download_project"])
}

/// D-1: a GitHub repository, with TheRock's INSTALL_DIR from a same-file set().
#[test]
fn d1_a_github_download_project_call_is_a_component() {
    let fx = dp_tree(
        "set(ROOT \"${CMAKE_BINARY_DIR}/r\")\ndownload_project(PROJ waybill-fixture-r GIT_REPOSITORY https://github.com/waybill-org/waybill-fixture-r.git GIT_TAG v1.2 BUILD_PROJECT TRUE INSTALL_DIR ${ROOT})\n",
    );
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:github/waybill-org/waybill-fixture-r@v1.2");
    assert_eq!(prop(c, "waybill:source-mechanism"), Some("cmake-externalproject"));
    assert!(required(c), "{c}");
    assert_eq!(source_files(c), vec!["CMakeLists.txt".to_string()]);
    assert_eq!(wrapper_chains(c), dp_chain());
    assert_eq!(wrappers(&docs), (1, 1, 0));
}

/// D-2: a URL and a hash.
#[test]
fn d2_a_url_download_project_call_is_a_component() {
    let fx = dp_tree(&format!("download_project(PROJ waybill-fixture-u URL {} URL_HASH SHA256={})\n", url("waybill-fixture-u", "2.0"), h()));
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:generic/waybill-fixture-u@2.0");
    assert_eq!(sha256(c), Some(h().as_str()));
    assert_eq!(wrapper_chains(c), dp_chain());
}

/// D-3 (FR-004): named `<PROJ>`, not `<PROJ>-download`.
#[test]
fn d3_a_url_that_does_not_parse_is_named_proj() {
    let fx = dp_tree("download_project(PROJ waybill-fixture-np URL https://example.invalid/download?id=7)\n");
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert_eq!(wrapper_chains(the(&docs, "pkg:generic/waybill-fixture-np@unknown")), dp_chain());
    assert!(!fetch_purls(&docs).iter().any(|p| p.contains("-download")), "{:?}", fetch_purls(&docs));
}

/// D-4 (probes D2, D4): DownloadProject's keywords are consumed. Placed
/// before `URL`, `SOURCE_DIR` takes `URL` as its value; forwarded, the
/// fetch would read `URL URL …` and name another version.
#[test]
fn d4_downloadprojects_keywords_are_consumed() {
    let fx = dp_tree(&format!(
        "download_project(PROJ waybill-fixture-k QUIET PREFIX p DOWNLOAD_DIR d BINARY_DIR b BUILD_PROJECT FALSE SOURCE_DIR URL URL {})\n",
        url("waybill-fixture-k", "1.0")
    ));
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:generic/waybill-fixture-k@1.0");
    assert_eq!(prop(c, "waybill:download-url"), Some(url("waybill-fixture-k", "1.0").as_str()));
    assert_eq!(wrapper_chains(c), dp_chain());
}

/// D-5 (probe D3): the last PROJ names it.
#[test]
fn d5_the_last_proj_wins() {
    let fx = dp_tree("download_project(PROJ waybill-fixture-first URL https://example.invalid/download?id=5 PROJ waybill-fixture-second)\n");
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert_eq!(wrapper_chains(the(&docs, "pkg:generic/waybill-fixture-second@unknown")), dp_chain());
}

/// D-6 and D-7 (FR-007): no identity, or no PROJ, is unresolved.
#[test]
fn d6_d7_no_identity_or_no_proj_is_unresolved() {
    let fx = dp_tree("download_project(PROJ waybill-fixture-none SOURCE_DIR x)\ndownload_project(URL https://example.invalid/download?id=6)\n");
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert!(fetches(&docs).is_empty(), "{:?}", fetch_purls(&docs));
    let c = cov(&docs);
    assert_eq!((n(&c, &["read"]), n(&c, &["unresolved", "identity"])), (2, 2), "{c}");
    assert_eq!(wrappers(&docs), (1, 2, 0));
}

/// D-8 (probe D5): keywords come from each copy. A copy listing EXTRA_KW
/// consumes it; one that does not forwards it, and the fetch reads
/// `URL URL …` instead.
#[test]
fn d8_keywords_come_from_each_copy() {
    let call = format!("download_project(PROJ waybill-fixture-x EXTRA_KW URL URL {})\n", url("waybill-fixture-x", "1.0"));
    let fx = tree(&[("cmake/DownloadProject.cmake", &dp_module(" EXTRA_KW", None)), ("CMakeLists.txt", &dp_root(&call))]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert_eq!(wrapper_chains(the(&docs, "pkg:generic/waybill-fixture-x@1.0")), dp_chain());
    let plain = dp_tree(&call);
    let docs = scan(&plain, &plain.path("t"), &[]);
    assert!(!fetch_purls(&docs).contains("pkg:generic/waybill-fixture-x@1.0"), "{:?}", fetch_purls(&docs));
}

/// D-9: a keyword list set under if() is unresolved, not guessed.
#[test]
fn d9_an_unresolved_keyword_list_is_not_guessed() {
    let fx = tree(&[
        ("cmake/DownloadProject.cmake", &dp_module("", Some("X"))),
        ("CMakeLists.txt", &dp_root(&format!("download_project(PROJ waybill-fixture-g URL {})\n", url("waybill-fixture-g", "1.0")))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert!(fetches(&docs).is_empty(), "{:?}", fetch_purls(&docs));
    assert_eq!(n(&cov(&docs), &["unresolved", "identity"]), 1);
}

/// D-10: a function named download_project without DownloadProject's shape
/// is not recognised.
#[test]
fn d10_a_same_named_function_without_the_shape_is_not_read() {
    let fx = tree(&[
        ("cmake/DownloadProject.cmake", "function(download_project)\nmessage(STATUS \"not DownloadProject\")\nendfunction()\n"),
        ("CMakeLists.txt", &dp_root(&format!("download_project(PROJ waybill-fixture-o URL {})\n", url("waybill-fixture-o", "1.0")))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert!(fetches(&docs).is_empty());
    assert!(coverage(&docs).is_none());
}

/// D-11: one shaped definition and one without the shape disagree.
#[test]
fn d11_a_shaped_and_an_unshaped_definition_are_ambiguous() {
    let fx = tree(&[
        ("a/DownloadProject.cmake", &dp_module("", None)),
        ("b/other.cmake", "function(download_project)\nmessage(STATUS no)\nendfunction()\n"),
        ("CMakeLists.txt", &format!("{ROOT}download_project(PROJ waybill-fixture-a URL {})\n", url("waybill-fixture-a", "1.0"))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert!(fetches(&docs).is_empty(), "{:?}", fetch_purls(&docs));
    assert_eq!(wrappers(&docs), (1, 1, 1));
}

/// D-12: the module excluded from the scan is not seen.
#[test]
fn d12_an_excluded_module_is_not_seen() {
    let fx = tree(&[
        ("vendor-dp/DownloadProject.cmake", &dp_module("", None)),
        ("CMakeLists.txt", &format!("{ROOT}download_project(PROJ waybill-fixture-e URL {})\n", url("waybill-fixture-e", "1.0"))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &["--exclude-path", "vendor-dp"]);
    assert!(fetches(&docs).is_empty());
    assert!(coverage(&docs).is_none());
}

/// D-13: the call site's condition; the template's `if` is not modelled.
#[test]
fn d13_the_call_sites_condition_alone() {
    let fx = dp_tree(&format!(
        "if(NOT WaybillFixtureR_FOUND)\ndownload_project(PROJ waybill-fixture-c URL {})\nendif()\n",
        url("waybill-fixture-c", "1.0")
    ));
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:generic/waybill-fixture-c@1.0");
    assert_optional(c, "cmake-find-package-fallback");
    assert_eq!(conditions(c), Some(vec!["NOT WaybillFixtureR_FOUND".to_string()]));
    assert_eq!(wrapper_chains(c), dp_chain());
}

/// D-14: test scope from the call site's directory.
#[test]
fn d14_a_call_in_a_test_directory() {
    let fx = tree(&[
        ("cmake/DownloadProject.cmake", &dp_module("", None)),
        ("CMakeLists.txt", &dp_root("")),
        ("tests/CMakeLists.txt", &format!("download_project(PROJ waybill-fixture-t URL {})\n", url("waybill-fixture-t", "1.0"))),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:generic/waybill-fixture-t@1.0");
    assert_eq!(prop(c, "waybill:lifecycle-scope"), Some("test"), "{c}");
    assert_eq!(wrapper_chains(c), dp_chain());
}

/// D-16: one repository and tag in two sibling directories is one component.
#[test]
fn d16_two_calls_of_one_identity_are_one_component() {
    let call = "download_project(PROJ waybill-fixture-s GIT_REPOSITORY https://github.com/waybill-org/waybill-fixture-s.git GIT_TAG v2)\n";
    let fx = tree(&[
        ("cmake/DownloadProject.cmake", &dp_module("", None)),
        ("CMakeLists.txt", &dp_root("")),
        ("a/CMakeLists.txt", call),
        ("b/CMakeLists.txt", call),
    ]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:github/waybill-org/waybill-fixture-s@v2");
    assert_eq!(source_files(c), vec!["a/CMakeLists.txt".to_string(), "b/CMakeLists.txt".to_string()]);
    assert_eq!(wrapper_chains(c), dp_chain(), "one chain for both calls");
}

/// D-17 (probe D6): a URL given twice fetches the first.
#[test]
fn d17_the_first_url_is_fetched() {
    let fx = dp_tree(&format!(
        "download_project(PROJ waybill-fixture-m URL {} URL {})\n",
        url("waybill-fixture-m1", "1.0"),
        url("waybill-fixture-m2", "2.0")
    ));
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert_eq!(wrapper_chains(the(&docs, "pkg:generic/waybill-fixture-m1@1.0")), dp_chain());
    assert!(!fetch_purls(&docs).iter().any(|p| p.contains("m2")));
}

/// D-15: a project function forwarding to download_project is a spec 1539
/// wrapper of it; the chain is outermost first.
#[test]
fn d15_a_wrapper_around_download_project() {
    let fx = dp_tree(
        "function(add_dep name)\ndownload_project(PROJ ${name} ${ARGN})\nendfunction()\n\
         add_dep(waybill-fixture-w GIT_REPOSITORY https://github.com/waybill-org/waybill-fixture-w.git GIT_TAG v3)\n",
    );
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = the(&docs, "pkg:github/waybill-org/waybill-fixture-w@v3");
    assert_eq!(wrapper_chains(c), chain(&["add_dep", "download_project"]));
    assert_eq!(wrappers(&docs), (2, 1, 0));
}

/// I-8: a direct fetch beside download_project calls carries no C234.
#[test]
fn i8_a_direct_fetch_beside_download_project_carries_no_chain() {
    let fx = dp_tree(&format!(
        "download_project(PROJ waybill-fixture-dpx URL {})\nExternalProject_Add(waybill-fixture-direct URL {})\n",
        url("waybill-fixture-dpx", "1.0"),
        url("waybill-fixture-direct", "1.0")
    ));
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert_eq!(wrapper_chains(the(&docs, "pkg:generic/waybill-fixture-dpx@1.0")), dp_chain());
    assert_eq!(wrapper_chains(the(&docs, "pkg:generic/waybill-fixture-direct@1.0")), None);
}

/// D-19: DownloadProject defined, never called.
#[test]
fn d19_a_definition_never_called_emits_and_counts_nothing() {
    let fx = dp_tree("");
    let docs = scan(&fx, &fx.path("t"), &[]);
    assert!(fetches(&docs).is_empty());
    assert!(coverage(&docs).is_none());
}

/// I-7: a tree with no wrapper definitions keeps spec 1538's C231, with no
/// `wrappers` key.
#[test]
fn i7_no_wrapper_no_wrappers_key() {
    let fx = tree(&[("CMakeLists.txt", &format!("{ROOT}{}", url_fetch("waybill-fixture-plain", "1.0", "")))]);
    let docs = scan(&fx, &fx.path("t"), &[]);
    let c = cov(&docs);
    assert!(c.get("wrappers").is_none(), "{c}");
    assert_eq!(n(&c, &["read"]), 1);
}
