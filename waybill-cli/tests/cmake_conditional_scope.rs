//! Spec 1641 (#1639): a CMake condition that holds in the default
//! configuration does not make a fetch or package search optional.
//!
//! Contract `specs/1641-cmake-conditional-scope/contracts/wire.md`. Every
//! case scans in three formats, checks parity on the scope (A1's component
//! set with CycloneDX `scope`), C122, C232 and C236, validates CycloneDX,
//! checks unique `bom-ref`s, and repeats 10 scans.

#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::cmake_fetch::{cdx_components, prop};
use common::git_fixture::GitFixture;
use corpus_harness_195::layer2_golden::{mask_nondeterministic, masked_difference};
use serde_json::Value;

const REPEATS: usize = 10;
const ROWS: &[&str] = &["A1", "C122", "C232", "C236"];
const C122: &str = "waybill:optional-derivation";
const C232: &str = "waybill:cmake-fetch-conditions";
const C236: &str = "waybill:cmake-find-conditions";
const ROOT: &str = "cmake_minimum_required(VERSION 3.25)\nproject(waybill_fixture_app VERSION 1.0.0)\ninclude(ExternalProject)\ninclude(FetchContent)\n";

// ---------------------------------------------------------------------
// Harness (T001)
// ---------------------------------------------------------------------

struct Scan {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
    log: String,
}

/// `files` written under `<fx>/tree`, made a git repository.
fn tree(files: &[(&str, &str)]) -> (GitFixture, PathBuf) {
    let fx = GitFixture::new();
    fx.repo("tree");
    for (rel, text) in files {
        fx.write(&format!("tree/{rel}"), text);
    }
    let path = fx.path("tree");
    (fx, path)
}

/// The binary under test, or `WAYBILL_CONDITIONAL_SCOPE_BIN` when
/// generating C-ctl's golden from `main`'s binary.
fn binary() -> String {
    std::env::var("WAYBILL_CONDITIONAL_SCOPE_BIN").unwrap_or_else(|_| common::bin().to_string())
}

fn scan_with(fx: &GitFixture, path: &Path, rust_log: &str) -> Scan {
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
        .env("RUST_LOG", rust_log)
        .env("NO_COLOR", "1")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(path)
        .args(["--no-deep-hash", "--root-name", "waybill-fixture-root", "--root-version", "1"])
        .args(["--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", files[0].display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", files[1].display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", files[2].display()))
        .output()
        .unwrap();
    let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "scan failed: {log}");
    let read = |p: &Path| serde_json::from_slice::<Value>(&std::fs::read(p).unwrap()).unwrap();
    Scan { cdx: read(&files[0]), spdx23: read(&files[1]), spdx3: read(&files[2]), log }
}

fn scan(fx: &GitFixture, path: &Path) -> Scan {
    scan_with(fx, path, "warn")
}

/// FR-008: the formats agree, CycloneDX validates, no `bom-ref` repeats.
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
    let mut refs: Vec<&str> = cdx_components(&s.cdx).into_iter().filter_map(|c| c["bom-ref"].as_str()).collect();
    refs.extend(s.cdx["metadata"]["component"]["bom-ref"].as_str());
    let mut seen = std::collections::BTreeSet::new();
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

/// FR-009: repeat scans give the same masked output.
fn assert_repeatable(fx: &GitFixture, path: &Path, first: &Scan) {
    for _ in 1..REPEATS {
        let again = scan(fx, path);
        for (name, a, b) in [("cdx", &first.cdx, &again.cdx), ("spdx23", &first.spdx23, &again.spdx23), ("spdx3", &first.spdx3, &again.spdx3)] {
            let (a, b) = (masked(a), masked(b));
            assert!(a == b, "{name}: two scans differ; {}", masked_difference(&a, &b).unwrap_or_default());
        }
    }
}

/// One checked scan: parity, schema, refs, repeatability.
fn check(files: &[(&str, &str)]) -> Scan {
    let (fx, path) = tree(files);
    let s = scan(&fx, &path);
    parity(&s);
    assert_repeatable(&fx, &path, &s);
    s
}

/// The component whose PURL starts with `prefix`; exactly one.
fn one<'a>(s: &'a Scan, prefix: &str) -> &'a Value {
    let all: Vec<&Value> = cdx_components(&s.cdx).into_iter().filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with(prefix))).collect();
    assert_eq!(all.len(), 1, "expected one component {prefix}: {:?}", all.iter().map(|c| &c["purl"]).collect::<Vec<_>>());
    all[0]
}

/// CycloneDX scope, `required` when absent.
fn scope(c: &Value) -> &str {
    c["scope"].as_str().unwrap_or("required")
}

fn ep(name: &str) -> String {
    format!(
        "ExternalProject_Add(waybill-fixture-{name}\n  URL \"https://example.invalid/waybill-fixture-{name}-1.3.2.tar.gz\"\n  URL_HASH \"SHA256=0000000000000000000000000000000000000000000000000000000000000000\")\n"
    )
}

fn under(cond: &str, body: &str) -> String {
    format!("if({cond})\n{body}endif()\n")
}

fn purl(name: &str) -> String {
    format!("pkg:generic/waybill-fixture-{name}@1.3.2")
}

/// In scope (FR-001, FR-004): required, no C122, C232 present.
fn assert_in_scope(s: &Scan, name: &str) {
    let c = one(s, &purl(name));
    assert_eq!(scope(c), "required", "{name} should be in scope: {c}");
    assert!(prop(c, C122).is_none(), "{name} in scope carries no C122: {c}");
    assert!(prop(c, C232).is_some(), "{name} keeps C232 (FR-005): {c}");
}

/// Optional with `cmake-conditional`, C232 present.
fn assert_conditional(s: &Scan, name: &str) {
    let c = one(s, &purl(name));
    assert_eq!(scope(c), "excluded", "{name} should be optional: {c}");
    assert_eq!(prop(c, C122), Some("cmake-conditional"), "{name}: {c}");
    assert!(prop(c, C232).is_some(), "{name}: {c}");
}

// ---------------------------------------------------------------------
// C-ctl (T002, FR-009): no catch-all conditional; byte-identical to main.
// ---------------------------------------------------------------------

const GOLDEN_ENV: &str = "WAYBILL_UPDATE_CONDITIONAL_SCOPE_GOLDENS";

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cmake_conditional_scope/ctl-golden")
}

#[test]
fn c_ctl_no_catch_all_conditional_is_byte_identical_to_main() {
    let body = format!(
        "{ROOT}{}{}{}find_package(waybill_fixture_found)\n",
        ep("plain"),
        under("NOT TARGET waybill_fixture_t", &ep("guarded")),
        under("BUILD_TESTING", &ep("testonly")),
    );
    let s = check(&[("CMakeLists.txt", &body)]);
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
        assert!(got == want, "C-ctl {name} differs from main's golden; {}", masked_difference(&want, &got).unwrap_or_default());
    }
}

// ---------------------------------------------------------------------
// T006: census positive control (research R6). One fetch per category,
// one event each, with the expected evaluation.
// ---------------------------------------------------------------------

#[test]
fn census_one_event_per_category() {
    let body = format!(
        "{ROOT}option(WF_ON \"\" ON)\noption(WF_OFF \"\" OFF)\nset(WF_LIT 1)\n{}{}{}{}{}{}{}",
        under("UNIX", &ep("c-platform")),
        under("NOT CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR", &ep("c-superbuild")),
        under("WF_ON", &ep("c-on")),
        under("WF_OFF", &ep("c-off")),
        under("WF_LIT", &ep("c-literal")),
        under("EXISTS ${CMAKE_SOURCE_DIR}/nothing", &ep("c-unknown")),
        under("UNIX AND WF_OFF", &ep("c-compound")),
    );
    let (fx, path) = tree(&[("CMakeLists.txt", &body)]);
    let s = scan_with(&fx, &path, "warn,waybill::cmake_conditions=debug");
    let events: Vec<&str> = s.log.lines().filter(|l| l.contains("a catch-all conditional (spec 1641)")).collect();
    for (name, category, eval) in [
        ("c-platform", "platform", "holds"),
        ("c-superbuild", "superbuild", "holds"),
        ("c-on", "option-on", "holds"),
        ("c-off", "option-off", "does-not-hold"),
        ("c-literal", "literal", "holds"),
        ("c-unknown", "unknown", "unknown"),
        ("c-compound", "compound", "does-not-hold"),
    ] {
        let hits: Vec<&&str> = events.iter().filter(|l| l.contains(&format!("waybill-fixture-{name}@"))).collect();
        assert_eq!(hits.len(), 1, "{name}: one event; got {hits:?}");
        assert!(hits[0].contains(&format!("category=\"{category}\"")), "{name}: {}", hits[0]);
        assert!(hits[0].contains(&format!("eval=\"{eval}\"")), "{name}: {}", hits[0]);
    }
}

// ---------------------------------------------------------------------
// US1 (T008): conditions that hold keep a fetch or search in scope.
// ---------------------------------------------------------------------

#[test]
fn t01_platform_check() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}{}", under("CMAKE_SYSTEM_NAME STREQUAL \"Linux\"", &ep("t1"))))]);
    assert_in_scope(&s, "t1");
}

#[test]
fn t02_superbuild_guard() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}{}", under("NOT CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR", &ep("t2"))))]);
    assert_in_scope(&s, "t2");
}

#[test]
fn t02r_superbuild_guard_reversed() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}{}", under("NOT CMAKE_CURRENT_SOURCE_DIR STREQUAL CMAKE_SOURCE_DIR", &ep("t2r"))))]);
    assert_in_scope(&s, "t2r");
}

#[test]
fn t03_option_on() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}option(WF_BUNDLE \"\" ON)\n{}", under("WF_BUNDLE", &ep("t3"))))]);
    assert_in_scope(&s, "t3");
}

#[test]
fn t04_option_off_stays_optional() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}option(WF_EXTRAS \"\" OFF)\n{}", under("WF_EXTRAS", &ep("t4"))))]);
    assert_conditional(&s, "t4");
}

#[test]
fn t10_no_default_visible() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}{}", under("WF_NOWHERE", &ep("t10"))))]);
    assert_in_scope(&s, "t10");
}

#[test]
fn t11_unreadable_expression() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}{}", under("EXISTS ${CMAKE_SOURCE_DIR}/x", &ep("t11"))))]);
    assert_in_scope(&s, "t11");
}

#[test]
fn t12_dependent_option() {
    let s = check(&[(
        "CMakeLists.txt",
        &format!("{ROOT}include(CMakeDependentOption)\ncmake_dependent_option(WF_DEP \"\" ON \"UNIX\" OFF)\n{}", under("WF_DEP", &ep("t12"))),
    )]);
    assert_in_scope(&s, "t12");
}

#[test]
fn t13_conflicting_defaults() {
    let s = check(&[
        ("CMakeLists.txt", &format!("{ROOT}option(WF_TWICE \"\" ON)\ninclude(cmake/more.cmake)\n{}", under("WF_TWICE", &ep("t13")))),
        ("cmake/more.cmake", "option(WF_TWICE \"\" OFF)\n"),
    ]);
    assert_in_scope(&s, "t13");
}

/// The strongest-scope rule across merged declarations (spec edge case).
#[test]
fn m1_merged_declarations_keep_the_strongest_scope() {
    let s = check(&[(
        "CMakeLists.txt",
        &format!("{ROOT}{}{}", under("NOT TARGET waybill_fixture_m1", &ep("m1")), under("UNIX", &ep("m1"))),
    )]);
    let c = one(&s, &purl("m1"));
    assert_eq!(scope(c), "required", "{c}");
    assert!(prop(c, C122).is_none(), "{c}");
    let conds: Vec<String> = serde_json::from_str(prop(c, C232).unwrap()).unwrap();
    assert!(conds.iter().any(|x| x.contains("NOT TARGET")) && conds.iter().any(|x| x == "UNIX"), "C232 is the union: {conds:?}");
}

/// Spec 1538 R9 still merges a conditional fetch into the submodule an
/// enclosing project supplies (FR-006).
#[test]
fn r1_a_kept_conditional_fetch_still_merges_into_its_submodule() {
    let fx = GitFixture::new();
    fx.repo("tree");
    fx.write("tree/.gitmodules", "[submodule \"d\"]\n\tpath = extern/waybill-fixture-dep\n\turl = https://github.com/waybill-org/waybill-fixture-dep.git\n");
    fx.write(
        "tree/CMakeLists.txt",
        &format!(
            "{ROOT}{}",
            under("UNIX", "FetchContent_Declare(waybill-fixture-dep GIT_REPOSITORY https://github.com/waybill-org/waybill-fixture-dep.git GIT_TAG ${FX_UNSET})\n")
        ),
    );
    let path = fx.path("tree");
    fx.git(&path, &["add", ".gitmodules"]);
    fx.add_gitlink("tree", "extern/waybill-fixture-dep", &"a".repeat(40));
    let s = scan(&fx, &path);
    parity(&s);
    let c = one(&s, "pkg:github/waybill-org/waybill-fixture-dep@");
    assert_eq!(prop(c, "waybill:source-mechanism"), Some("git-submodule"), "{c}");
    assert!(prop(c, "waybill:superseded-declarations").is_some(), "the fetch merged into the submodule: {c}");
}

fn search_record<'a>(s: &'a Scan, name: &str) -> &'a Value {
    one(s, &format!("pkg:generic/{name}"))
}

#[test]
fn s1_a_package_search_under_a_platform_check_is_in_scope() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}{}", under("UNIX", "find_package(waybill_fixture_s1)\n")))]);
    let c = search_record(&s, "waybill_fixture_s1");
    assert_eq!(scope(c), "required", "{c}");
    assert!(prop(c, C236).is_some(), "C236 still records the condition: {c}");
}

#[test]
fn s2_a_package_search_under_an_off_option_stays_optional() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}option(WF_S2 \"\" OFF)\n{}", under("WF_S2", "find_package(waybill_fixture_s2)\n")))]);
    let c = search_record(&s, "waybill_fixture_s2");
    assert_eq!(scope(c), "excluded", "{c}");
}

// ---------------------------------------------------------------------
// US2 (T013): combined and branching conditions.
// ---------------------------------------------------------------------

#[test]
fn t05_platform_and_off_option() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}option(WF_X \"\" OFF)\n{}", under("WIN32 AND WF_X", &ep("t5"))))]);
    assert_conditional(&s, "t5");
}

#[test]
fn t06_else_of_a_platform_check() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}if(WIN32)\nmessage(STATUS w)\nelse()\n{}endif()\n", ep("t6")))]);
    assert_in_scope(&s, "t6");
}

#[test]
fn t07_else_of_an_on_option() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}option(WF_X \"\" ON)\nif(WF_X)\nmessage(STATUS x)\nelse()\n{}endif()\n", ep("t7")))]);
    assert_conditional(&s, "t7");
}

#[test]
fn t07e_elseif_after_a_branch_that_holds() {
    let s = check(&[(
        "CMakeLists.txt",
        &format!("{ROOT}option(WF_X \"\" ON)\noption(WF_Y \"\" ON)\nif(WF_X)\nmessage(STATUS x)\nelseif(WF_Y)\n{}endif()\n", ep("t7e")),
    )]);
    assert_conditional(&s, "t7e");
}

#[test]
fn t08_nested() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}option(WF_X \"\" ON)\n{}", under("UNIX", &under("WF_X", &ep("t8")))))]);
    assert_in_scope(&s, "t8");
}

#[test]
fn t09_option_in_the_parent_directory() {
    let s = check(&[
        ("CMakeLists.txt", &format!("{ROOT}option(WF_PARENT \"\" ON)\nadd_subdirectory(sub)\n")),
        ("sub/CMakeLists.txt", &under("WF_PARENT", &ep("t9"))),
    ]);
    assert_in_scope(&s, "t9");
}

// ---------------------------------------------------------------------
// US3 (T015): the six fallback shapes and BUILD_TESTING are unchanged.
// ---------------------------------------------------------------------

fn assert_shape(s: &Scan, name: &str, shape: &str) {
    let c = one(s, &purl(name));
    assert_eq!(scope(c), "excluded", "{name}: {c}");
    assert_eq!(prop(c, C122), Some(shape), "{name}: {c}");
}

#[test]
fn f1_not_target_under_a_platform_check() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}{}", under("UNIX", &under("NOT TARGET waybill_fixture_f1", &ep("f1")))))]);
    assert_shape(&s, "f1", "cmake-not-target");
}

#[test]
fn f2_not_found() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}{}", under("NOT WaybillFixtureF2_FOUND", &ep("f2"))))]);
    assert_shape(&s, "f2", "cmake-find-package-fallback");
}

#[test]
fn f3_find_package_args() {
    let s = check(&[(
        "CMakeLists.txt",
        &format!("{ROOT}FetchContent_Declare(waybill-fixture-f3 URL \"https://example.invalid/waybill-fixture-f3-1.3.2.tar.gz\" FIND_PACKAGE_ARGS)\n"),
    )]);
    assert_shape(&s, "f3", "cmake-find-package-args");
}

#[test]
fn f4_try_find_package_always() {
    let s = check(&[(
        "CMakeLists.txt",
        &format!("{ROOT}set(FETCHCONTENT_TRY_FIND_PACKAGE_MODE ALWAYS)\nFetchContent_Declare(waybill-fixture-f4 URL \"https://example.invalid/waybill-fixture-f4-1.3.2.tar.gz\")\n"),
    )]);
    assert_shape(&s, "f4", "cmake-try-find-package-always");
}

#[test]
fn f5_dependency_provider() {
    let s = check(&[(
        "CMakeLists.txt",
        &format!("{ROOT}cmake_language(SET_DEPENDENCY_PROVIDER wf_provide SUPPORTED_METHODS FETCHCONTENT_MAKEAVAILABLE_SERIAL)\nFetchContent_Declare(waybill-fixture-f5 URL \"https://example.invalid/waybill-fixture-f5-1.3.2.tar.gz\")\n"),
    )]);
    assert_shape(&s, "f5", "cmake-dependency-provider");
}

#[test]
fn f6_function_body() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}function(wf_fetch)\n{}endfunction()\n", ep("f6")))]);
    assert_shape(&s, "f6", "cmake-function-body");
}

#[test]
fn b1_build_testing_is_test() {
    let s = check(&[("CMakeLists.txt", &format!("{ROOT}{}", under("BUILD_TESTING", &ep("b1"))))]);
    let c = one(&s, &purl("b1"));
    assert_eq!(prop(c, "waybill:lifecycle-scope"), Some("test"), "{c}");
}
