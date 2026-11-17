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
