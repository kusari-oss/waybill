//! Spec 1653 (#1535): read a CMake build tree to settle what a build used.
//!
//! Contracts: `specs/1653-cmake-build-tree/contracts/{cli,settlement,annotations}.md`.
//! Fixtures under `tests/fixtures/cmake_build_tree/<config>/` are reduced
//! copies of real build trees captured by the spec's probe
//! (`specs/1653-cmake-build-tree/measurements/probe/`, CMake 3.31.6, Ninja):
//! `fetch`, `system`, `confonly`, `failed`. `materialise` copies one into a
//! temp dir and substitutes the `@SRC@`/`@BUILD@`/`@PREFIX@`/`@DEPS@`
//! placeholders `reduce.py` left in place of the probe's absolute paths.

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

const C122: &str = "waybill:optional-derivation";
const C245: &str = "waybill:cmake-build-settlement";
const C246: &str = "waybill:cmake-build-tree-coverage";
const ROWS: &[&str] = &["A1", "C122", "C232", "C245", "C246"];

// ---------------------------------------------------------------------
// Harness (T005)
// ---------------------------------------------------------------------

struct Fixture {
    fx: GitFixture,
    src: PathBuf,
    build: PathBuf,
}

fn fixture_dir(config: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cmake_build_tree").join(config)
}

fn copy_tree(from: &Path, to: &Path, subst: &dyn Fn(&str) -> String) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if e.file_type().unwrap().is_dir() {
            copy_tree(&src, &dst, subst);
        } else if e.file_name() == ".fixture-keep" {
            // Directory placeholder for git; the directory itself is the evidence.
        } else {
            let bytes = std::fs::read(&src).unwrap();
            match String::from_utf8(bytes) {
                Ok(text) => std::fs::write(&dst, subst(&text)).unwrap(),
                Err(e) => std::fs::write(&dst, e.into_bytes()).unwrap(),
            }
        }
    }
}

/// `config`'s source as a git repository at `<fx>/tree`, its build tree at
/// `<fx>/build` (outside the source, so a source-only scan never sees it),
/// and an install prefix at `<fx>/prefix` holding the header-only `hdronly`.
fn materialise(config: &str) -> Fixture {
    let fx = GitFixture::new();
    fx.repo("tree");
    let (src, build, prefix, deps) = (fx.path("tree"), fx.path("build"), fx.path("prefix"), fx.path("deps"));
    let s = |p: &Path| p.to_string_lossy().replace('\\', "/");
    let (s_src, s_build, s_prefix, s_deps) = (s(&src), s(&build), s(&prefix), s(&deps));
    let subst = move |t: &str| {
        t.replace("@SRC@", &s_src).replace("@BUILD@", &s_build).replace("@PREFIX@", &s_prefix).replace("@DEPS@", &s_deps)
    };
    let dir = fixture_dir(config);
    copy_tree(&dir.join("source"), &src, &subst);
    copy_tree(&dir.join("build"), &build, &subst);
    std::fs::create_dir_all(prefix.join("include/hdronly")).unwrap();
    std::fs::write(prefix.join("include/hdronly/hdronly.hpp"), "inline int hdronly_answer() { return 42; }\n").unwrap();
    std::fs::create_dir_all(prefix.join("lib/cmake/hdronly")).unwrap();
    std::fs::write(prefix.join("lib/cmake/hdronly/hdronlyConfig.cmake"), "# fixture\n").unwrap();
    std::fs::create_dir_all(&deps).unwrap();
    Fixture { fx, src, build }
}

struct Scan {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
    log: String,
}

/// The binary under test, or `WAYBILL_BUILD_TREE_BIN` when generating the
/// control golden from `main`'s binary (T006).
fn binary() -> String {
    std::env::var("WAYBILL_BUILD_TREE_BIN").unwrap_or_else(|_| common::bin().to_string())
}

fn run(fx: &GitFixture, path: &Path, extra: &[&std::ffi::OsStr]) -> (std::process::Output, [PathBuf; 3], tempfile::TempDir) {
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
        .env("RUST_LOG", "warn,waybill::cmake_build_tree=info")
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
        .args(extra)
        .output()
        .unwrap();
    (out, files, out_dir)
}

/// Scan `f.src`, naming each of `trees` with `--cmake-build-tree`.
fn scan_bt(f: &Fixture, trees: &[&Path]) -> Scan {
    let mut extra: Vec<&std::ffi::OsStr> = Vec::new();
    for t in trees {
        extra.push("--cmake-build-tree".as_ref());
        extra.push(t.as_os_str());
    }
    let (out, files, _dir) = run(&f.fx, &f.src, &extra);
    let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "scan failed: {log}");
    let read = |p: &Path| serde_json::from_slice::<Value>(&std::fs::read(p).unwrap()).unwrap();
    let s = Scan { cdx: read(&files[0]), spdx23: read(&files[1]), spdx3: read(&files[2]), log };
    assert_bt_invariants(&s);
    s
}

/// C246 invariants (contracts/annotations.md), on every scan.
fn assert_bt_invariants(s: &Scan) {
    let Some(cov) = coverage_bt(s) else { return };
    let settled: Vec<Value> = cdx_components(&s.cdx).into_iter().filter_map(|c| prop(c, C245)).map(|v| serde_json::from_str(v).unwrap()).collect();
    let decl = ["taken", "not-taken", "incomplete", "no-evidence"].iter().map(|k| cov["declarations"][k].as_u64().unwrap()).sum::<u64>();
    let not_build_only = settled.iter().filter(|v| v["build-only"] != true).count() as u64;
    let build_only = settled.iter().filter(|v| v["build-only"] == true).count() as u64;
    assert!(decl >= not_build_only, "declarations counted before the per-PURL merge cover every settled component: {cov}");
    assert_eq!(cov["build-only"].as_u64().unwrap(), build_only, "C246 build-only = C245 build-only components: {cov}");
}

/// C245 of the one component whose CycloneDX `name` is `name`.
fn settlement(s: &Scan, name: &str) -> Value {
    serde_json::from_str(prop(named(s, name), C245).unwrap_or_else(|| panic!("{name} has no C245"))).unwrap()
}

fn named<'a>(s: &'a Scan, name: &str) -> &'a Value {
    let hits: Vec<&Value> = cdx_components(&s.cdx).into_iter().filter(|c| c["name"] == name).collect();
    assert_eq!(hits.len(), 1, "exactly one component named {name}: {:?}", cdx_components(&s.cdx).iter().map(|c| (&c["name"], &c["purl"])).collect::<Vec<_>>());
    hits[0]
}

fn coverage_bt(s: &Scan) -> Option<Value> {
    prop(&s.cdx["metadata"], C246).map(|v| serde_json::from_str(v).unwrap())
}

/// CycloneDX scope, `required` when absent.
fn scope(c: &Value) -> &str {
    c["scope"].as_str().unwrap_or("required")
}

/// FR-012: the formats agree, CycloneDX validates, no `bom-ref` repeats.
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

/// Mask the temp-dir prefix so two fixtures' documents compare.
fn masked(v: &Value, f: &Fixture) -> Value {
    let s = serde_json::to_string(&mask_nondeterministic(v)).unwrap();
    let root = f.fx.path("").to_string_lossy().trim_end_matches('/').to_string();
    serde_json::from_str(&s.replace(&root, "/TMP")).unwrap()
}

// ---------------------------------------------------------------------
// T006 (FR-001, SC-004): no --cmake-build-tree, byte-identical to main.
// ---------------------------------------------------------------------

const GOLDEN_ENV: &str = "WAYBILL_UPDATE_BUILD_TREE_GOLDENS";

#[test]
fn source_only_is_byte_identical_to_main() {
    let f = materialise("fetch");
    let (out, files, _dir) = run(&f.fx, &f.src, &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let read = |p: &Path| serde_json::from_slice::<Value>(&std::fs::read(p).unwrap()).unwrap();
    let dir = fixture_dir("ctl-golden");
    let docs = [("cdx.json", read(&files[0])), ("spdx-2.3.json", read(&files[1])), ("spdx-3.json", read(&files[2]))];
    assert!(!docs[0].1.to_string().contains(C246), "no C246 without the flag");
    if std::env::var_os(GOLDEN_ENV).is_some() {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, doc) in &docs {
            std::fs::write(dir.join(name), serde_json::to_string_pretty(&masked(doc, &f)).unwrap() + "\n").unwrap();
        }
        return;
    }
    for (name, doc) in &docs {
        let want: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(name)).unwrap()).unwrap();
        let got = masked(doc, &f);
        assert!(got == want, "control {name} differs from main's golden; {}", masked_difference(&want, &got).unwrap_or_default());
    }
}

// ---------------------------------------------------------------------
// US1 (T014, T015): a fallback's outcome is stated, not guessed.
// ---------------------------------------------------------------------

fn basis(v: &Value) -> Vec<&str> {
    v["basis"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect()
}

/// Checked scan: parity, schema, refs, and no host path (SC-006).
fn checked(f: &Fixture, trees: &[&Path]) -> Scan {
    let s = scan_bt(f, trees);
    parity(&s);
    assert_no_host_paths(f, &s);
    s
}

/// SC-006: no string in any format names the temp dir, its canonical form,
/// or the scan's `$HOME`.
fn assert_no_host_paths(f: &Fixture, s: &Scan) {
    let roots = [f.fx.path(""), std::fs::canonicalize(f.fx.path("")).unwrap(), f.fx.home()];
    for (name, doc) in [("cdx", &s.cdx), ("spdx23", &s.spdx23), ("spdx3", &s.spdx3)] {
        let text = doc.to_string();
        for r in &roots {
            let r = r.to_string_lossy().trim_end_matches('/').to_string();
            let r = serde_json::to_string(&r).unwrap();
            let r = &r[1..r.len() - 1];
            assert!(!text.contains(r), "{name} contains the host path {r}");
        }
    }
}

#[test]
fn fetchcontent_fallback_taken_is_in_scope() {
    let f = materialise("fetch");
    let s = checked(&f, &[&f.build]);
    let c = named(&s, "hdronly");
    assert_eq!(scope(c), "required", "taken: in scope: {c}");
    assert!(prop(c, C122).is_none(), "taken: no C122: {c}");
    let v = settlement(&s, "hdronly");
    assert_eq!(v["state"], "taken");
    // `compile-include` is US3's: the project's unit includes its population.
    assert_eq!(basis(&v), ["cache-source-dir", "compile-include", "deps-src-dir", "fetchcontent-source-dir-entry"]);
    assert_eq!(v["builds"], serde_json::json!(["build:."]));
    assert_eq!(v["build-only"], false);
    let cov = coverage_bt(&s).unwrap();
    assert_eq!(cov["build-dirs"], 1, "sub-builds are not build directories: {cov}");
    assert_eq!(cov["signal-disagreements"], 0, "{cov}");
    assert_eq!(cov["source-mismatch"], 0, "{cov}");
    // T021: the INFO summary, with no host path in it (FR-006).
    let line = s.log.lines().find(|l| l.contains("fetch declarations settled (spec 1653)")).unwrap_or_else(|| panic!("no settlement summary in: {}", s.log));
    assert!(line.contains("taken=") && line.contains("build_dirs=1"), "{line}");
    assert!(!line.contains(&*f.fx.path("").to_string_lossy()), "{line}");
}

#[test]
fn fetchcontent_found_on_the_system_is_not_taken() {
    let f = materialise("system");
    let s = checked(&f, &[&f.build]);
    let c = named(&s, "hdronly");
    assert_eq!(scope(c), "excluded", "not taken: source scope kept: {c}");
    assert_eq!(prop(c, C122), Some("cmake-find-package-args"), "{c}");
    let v = settlement(&s, "hdronly");
    assert_eq!(v["state"], "not-taken");
    assert_eq!(basis(&v), ["cache-package-dir", "compile-include"]);
}

#[test]
fn declared_never_made_available_has_no_evidence() {
    let f = materialise("fetch");
    let s = scan_bt(&f, &[&f.build]);
    let v = settlement(&s, "neverused");
    assert_eq!(v, serde_json::json!({"basis": [], "build-only": false, "state": "no-evidence"}));
    assert_eq!(scope(named(&s, "neverused")), "required", "no evidence never narrows scope");
}

#[test]
fn externalproject_taken_and_mixed_case_names() {
    let f = materialise("fetch");
    let s = scan_bt(&f, &[&f.build]);
    for n in ["extproj", "extgit", "ExtMixed"] {
        let v = settlement(&s, n);
        assert_eq!(v["state"], "taken", "{n}: {v}");
        assert_eq!(basis(&v), ["download-stamp"], "{n}");
    }
    assert_eq!(settlement(&s, "MixedCase")["state"], "taken", "FetchContent keys fold case (R1, T004)");
}

#[test]
fn externalproject_without_download_is_incomplete() {
    for config in ["confonly", "failed"] {
        let f = materialise(config);
        let s = scan_bt(&f, &[&f.build]);
        let v = settlement(&s, "extproj");
        assert_eq!(v["state"], "incomplete", "{config}: {v}");
        assert_eq!(basis(&v), ["stamp-dir-only"], "{config}");
        assert_eq!(scope(named(&s, "extproj")), "required", "{config}: scope unchanged");
        // FetchContent populates at configure time (R1).
        assert_eq!(settlement(&s, "hdronly")["state"], "taken", "{config}");
    }
}

#[test]
fn a_source_dir_override_is_taken_and_its_path_is_not_emitted() {
    let f = materialise("fetch");
    let cache = f.build.join("CMakeCache.txt");
    let local = f.fx.path("local-urldep");
    std::fs::create_dir_all(&local).unwrap();
    let text = std::fs::read_to_string(&cache)
        .unwrap()
        .replace("FETCHCONTENT_SOURCE_DIR_URLDEP:PATH=\n", &format!("FETCHCONTENT_SOURCE_DIR_URLDEP:PATH={}\n", local.display()));
    std::fs::write(&cache, text).unwrap();
    let s = checked(&f, &[&f.build]);
    let v = settlement(&s, "urldep");
    assert_eq!(v["state"], "taken");
    assert!(basis(&v).contains(&"source-dir-override"), "{v}");
}

#[test]
fn a_population_no_declaration_accounts_for_is_build_only() {
    let f = materialise("fetch");
    std::fs::create_dir_all(f.build.join("_deps/orphan-src")).unwrap();
    let s = checked(&f, &[&f.build]);
    let c = named(&s, "orphan");
    assert_eq!(scope(c), "required");
    assert_eq!(prop(c, "waybill:source-mechanism"), Some("cmake-fetchcontent"));
    let v = settlement(&s, "orphan");
    assert_eq!(v["build-only"], true);
    assert_eq!(v["state"], "taken");
    assert_eq!(coverage_bt(&s).unwrap()["build-only"], 1);
}

#[test]
fn two_build_trees_taken_wins_and_the_conflict_is_counted() {
    let f = materialise("fetch");
    // A second tree configured from the same source, where hdronly was found.
    let g = materialise("system");
    let second = f.fx.path("build-system");
    copy_tree(&g.build, &second, &|t: &str| {
        t.replace(&g.src.to_string_lossy().to_string(), &f.src.to_string_lossy())
            .replace(&g.build.to_string_lossy().to_string(), &second.to_string_lossy())
    });
    let s = checked(&f, &[&f.build, &second]);
    let v = settlement(&s, "hdronly");
    assert_eq!(v["state"], "taken");
    assert_eq!(v["builds"], serde_json::json!(["build1:."]), "only the tree that took it gave evidence");
    let cov = coverage_bt(&s).unwrap();
    assert_eq!(cov["build-trees"], 2);
    assert_eq!(cov["build-dirs"], 2);
    assert!(cov["conflicts"].as_u64().unwrap() >= 1, "{cov}");
}

#[test]
fn a_taken_fetch_under_build_testing_keeps_test_scope() {
    let f = materialise("fetch");
    let lists = f.src.join("CMakeLists.txt");
    let text = std::fs::read_to_string(&lists).unwrap();
    // Wrap urldep's declaration in BUILD_TESTING.
    let text = text.replacen("FetchContent_Declare(urldep", "if(BUILD_TESTING)\nFetchContent_Declare(urldep", 1).replacen(
        "URL_HASH SHA256=${URLDEP_SHA256})",
        "URL_HASH SHA256=${URLDEP_SHA256})\nendif()",
        1,
    );
    std::fs::write(&lists, text).unwrap();
    let s = scan_bt(&f, &[&f.build]);
    let c = named(&s, "urldep");
    assert_eq!(settlement(&s, "urldep")["state"], "taken");
    assert_eq!(prop(c, "waybill:lifecycle-scope-derivation"), Some("cmake-build-testing"), "Test outranks taken: {c}");
}

#[test]
fn repeatable() {
    let f = materialise("fetch");
    let first = scan_bt(&f, &[&f.build]);
    for _ in 1..5 {
        let again = scan_bt(&f, &[&f.build]);
        for (name, a, b) in [("cdx", &first.cdx, &again.cdx), ("spdx23", &first.spdx23, &again.spdx23), ("spdx3", &first.spdx3, &again.spdx3)] {
            let (a, b) = (masked(a, &f), masked(b, &f));
            assert!(a == b, "{name}: two scans differ; {}", masked_difference(&a, &b).unwrap_or_default());
        }
    }
}

#[test]
fn a_missing_build_tree_is_an_operator_error() {
    let f = materialise("fetch");
    let missing = f.fx.path("nope");
    let (out, _, _d) = run(&f.fx, &f.src, &["--cmake-build-tree".as_ref(), missing.as_os_str()]);
    assert_eq!(out.status.code(), Some(2), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--cmake-build-tree"));
}

// ---------------------------------------------------------------------
// US2 (T022): conditions are evaluated against the configuration built.
// ---------------------------------------------------------------------

/// Replace one cache line in `f`'s build tree.
fn set_cache(f: &Fixture, from: &str, to: &str) {
    let cache = f.build.join("CMakeCache.txt");
    let text = std::fs::read_to_string(&cache).unwrap();
    assert!(text.contains(from), "fixture cache has {from}");
    std::fs::write(&cache, text.replace(from, to)).unwrap();
}

#[test]
fn a_cache_string_comparison_is_decided_by_the_configured_value() {
    // #1656: source-only, `BACKEND STREQUAL B` is unknown and bdep is in scope.
    let f = materialise("fetch");
    let s = checked(&f, &[&f.build]);
    let c = named(&s, "bdep");
    assert_eq!(scope(c), "excluded", "BACKEND=A configured: bdep is optional: {c}");
    assert_eq!(prop(c, C122), Some("cmake-conditional"), "{c}");
    assert_eq!(settlement(&s, "bdep")["state"], "no-evidence");
    let cov = coverage_bt(&s).unwrap();
    assert!(cov["configured-values-applied"].as_u64().unwrap() >= 1, "{cov}");

    let g = materialise("system");
    let s = checked(&g, &[&g.build]);
    assert_eq!(scope(named(&s, "bdep")), "required", "BACKEND=B configured: in scope");
}

#[test]
fn a_default_off_option_configured_on_is_in_scope_without_a_population() {
    let f = materialise("fetch");
    set_cache(&f, "WITH_EXTRA:BOOL=OFF", "WITH_EXTRA:BOOL=ON");
    let s = checked(&f, &[&f.build]);
    let c = named(&s, "extra");
    assert_eq!(scope(c), "required", "configured ON: in scope: {c}");
    assert!(prop(c, C122).is_none(), "{c}");
    assert_eq!(settlement(&s, "extra")["state"], "no-evidence", "decided by the condition, not by a population");
}

#[test]
fn builds_that_configured_different_values_leave_the_condition_unknown() {
    // Two builds of one source, neither populating bdep: BACKEND is A in one
    // and B in the other, so the comparison is unknown and bdep stays in
    // scope (spec 1641 FR-004). One build alone (A) makes it optional.
    let f = materialise("fetch");
    let second = f.fx.path("build-b");
    let (b1, b2) = (f.build.to_string_lossy().to_string(), second.to_string_lossy().to_string());
    copy_tree(&f.build, &second, &|t: &str| t.replace(&b1, &b2).replace("BACKEND:STRING=A", "BACKEND:STRING=B"));
    let s = checked(&f, &[&f.build, &second]);
    let c = named(&s, "bdep");
    assert_eq!(scope(c), "required", "{c}");
    assert_eq!(settlement(&s, "bdep")["state"], "no-evidence", "decided by the conflict, not by a population");
}

#[test]
fn a_package_search_follows_the_configured_value() {
    let f = materialise("fetch");
    let lists = f.src.join("CMakeLists.txt");
    let text = std::fs::read_to_string(&lists).unwrap() + "if(WITH_EXTRA)\n  find_package(WaybillFixtureSearched)\nendif()\n";
    std::fs::write(&lists, text).unwrap();
    let s = scan_bt(&f, &[&f.build]);
    let c = named(&s, "waybillfixturesearched");
    assert_eq!(scope(c), "excluded", "WITH_EXTRA=OFF configured: {c}");
    set_cache(&f, "WITH_EXTRA:BOOL=OFF", "WITH_EXTRA:BOOL=ON");
    let s = scan_bt(&f, &[&f.build]);
    assert_eq!(scope(named(&s, "waybillfixturesearched")), "required");
}


// ---------------------------------------------------------------------
// US3: compile edges (FR-009)
// ---------------------------------------------------------------------

fn purl_of_named(s: &Scan, name: &str) -> String {
    named(s, name)["purl"].as_str().unwrap().to_string()
}

fn cdx_edges(cdx: &Value) -> std::collections::BTreeSet<(String, String)> {
    cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|d| {
            let from = d["ref"].as_str().unwrap().to_string();
            d["dependsOn"].as_array().into_iter().flatten().filter_map(move |t| t.as_str().map(|t| (from.clone(), t.to_string())))
        })
        .collect()
}

fn spdx23_edges(doc: &Value) -> std::collections::BTreeSet<(String, String)> {
    let purl_of: std::collections::BTreeMap<&str, &str> = doc["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let purl = p["externalRefs"].as_array()?.iter().find(|r| r["referenceType"] == "purl")?["referenceLocator"].as_str()?;
            Some((p["SPDXID"].as_str()?, purl))
        })
        .collect();
    // An edge to an optional or test component is written reversed, as
    // `<dep> OPTIONAL_DEPENDENCY_OF <project>` (milestone 179).
    doc["relationships"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let a = purl_of.get(r["spdxElementId"].as_str()?)?;
            let b = purl_of.get(r["relatedSpdxElement"].as_str()?)?;
            match r["relationshipType"].as_str()? {
                "DEPENDS_ON" => Some((a.to_string(), b.to_string())),
                t if t.ends_with("_DEPENDENCY_OF") => Some((b.to_string(), a.to_string())),
                _ => None,
            }
        })
        .collect()
}

fn spdx3_edges(doc: &Value) -> std::collections::BTreeSet<(String, String)> {
    let graph = doc["@graph"].as_array().unwrap();
    let purl_of: std::collections::BTreeMap<&str, &str> =
        graph.iter().filter_map(|e| Some((e["spdxId"].as_str()?, e["software_packageUrl"].as_str()?))).collect();
    graph
        .iter()
        .filter(|e| e["type"] == "Relationship" && e["relationshipType"] == "dependsOn")
        .flat_map(|e| {
            let from = e["from"].as_str().and_then(|f| purl_of.get(f)).map(|s| s.to_string());
            e["to"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| purl_of.get(t.as_str()?).map(|s| s.to_string()))
                .filter_map(move |to| from.clone().map(|f| (f, to)))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The edge `from → to` in all three formats.
fn assert_edge(s: &Scan, from: &str, to: &str) {
    let e = (from.to_string(), to.to_string());
    assert!(cdx_edges(&s.cdx).contains(&e), "CycloneDX: {from} -> {to}");
    assert!(spdx23_edges(&s.spdx23).contains(&e), "SPDX 2.3: {from} -> {to}");
    assert!(spdx3_edges(&s.spdx3).contains(&e), "SPDX 3: {from} -> {to}");
}

fn edit_compile_db(f: &Fixture, from: &str, to: &str) {
    let p = f.build.join("compile_commands.json");
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(text.contains(from), "fixture drift: {from}");
    std::fs::write(&p, text.replacen(from, to, 1)).unwrap();
}

/// The project's own translation unit includes `hdronly` and `urldep` from
/// their populations: two compile edges, and `compile-include` in both
/// bases. `MixedCase` is only compiled against itself, by its own unit.
#[test]
fn includes_into_populations_are_compile_edges() {
    let f = materialise("fetch");
    let s = checked(&f, &[&f.build]);
    let probe = purl_of_named(&s, "probe");
    for dep in ["hdronly", "urldep"] {
        assert_edge(&s, &probe, &purl_of_named(&s, dep));
        let v = settlement(&s, dep);
        assert!(basis(&v).contains(&"compile-include"), "{dep}: {v}");
        assert_eq!(v["state"], "taken", "{dep}: {v}");
    }
    let mixed = settlement(&s, "MixedCase");
    assert!(!basis(&mixed).contains(&"compile-include"), "a dependency compiling itself is not the project compiling against it: {mixed}");
    let cov = coverage_bt(&s).unwrap();
    assert_eq!(cov["compile-edges"], 2, "{cov}");
    assert_eq!(cov["include-dirs-unresolved"], 0, "{cov}");
    assert_eq!(cov["unreadable-files"], 0, "{cov}");
}

/// FX/system: `hdronly` came from the install prefix its `hdronly_DIR`
/// names. The edge is the same; the fetch stays *not taken* and out of
/// scope, because compile evidence alone changes no scope (FR-009).
#[test]
fn an_include_under_a_package_prefix_is_a_compile_edge_and_scope_is_unchanged() {
    let f = materialise("system");
    let s = checked(&f, &[&f.build]);
    let hdronly = named(&s, "hdronly");
    assert_eq!(scope(hdronly), "excluded", "compile evidence changes no scope: {hdronly}");
    assert_edge(&s, &purl_of_named(&s, "probe"), &purl_of_named(&s, "hdronly"));
    let v = settlement(&s, "hdronly");
    assert_eq!(v["state"], "not-taken", "{v}");
    assert_eq!(basis(&v), ["cache-package-dir", "compile-include"]);
    assert_eq!(coverage_bt(&s).unwrap()["compile-edges"], 2);
}

/// No compilation database, or TheRock's 0-byte one (measurements/therock.md):
/// no compile edge is claimed, and nothing is unreadable.
#[test]
fn without_a_compile_database_nothing_is_claimed() {
    for contents in [None, Some("")] {
        let f = materialise("fetch");
        let db = f.build.join("compile_commands.json");
        match contents {
            None => std::fs::remove_file(&db).unwrap(),
            Some(c) => std::fs::write(&db, c).unwrap(),
        }
        let s = checked(&f, &[&f.build]);
        let cov = coverage_bt(&s).unwrap();
        assert_eq!(cov["compile-edges"], 0, "{contents:?}: {cov}");
        assert_eq!(cov["unreadable-files"], 0, "{contents:?}: {cov}");
        assert!(!basis(&settlement(&s, "hdronly")).contains(&"compile-include"));
    }
}

#[test]
fn a_malformed_compile_database_is_counted_not_fatal() {
    let f = materialise("fetch");
    std::fs::write(f.build.join("compile_commands.json"), "[{\"directory\": ").unwrap();
    let s = checked(&f, &[&f.build]);
    let cov = coverage_bt(&s).unwrap();
    assert_eq!(cov["unreadable-files"], 1, "{cov}");
    assert_eq!(cov["compile-edges"], 0, "{cov}");
}

/// An include directory no component accounts for is counted, never
/// emitted. The project's own directories are not counted.
#[test]
fn an_unresolved_include_directory_is_counted_and_not_emitted() {
    let f = materialise("fetch");
    let outside = f.fx.path("elsewhere/include");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::create_dir_all(f.src.join("include")).unwrap();
    let extra = format!("-isystem {} -I{}/include -Igenerated -I", outside.display(), f.src.display());
    edit_compile_db(&f, "-I", &extra);
    let s = checked(&f, &[&f.build]);
    let cov = coverage_bt(&s).unwrap();
    assert_eq!(cov["include-dirs-unresolved"], 1, "{cov}");
    assert_eq!(cov["compile-edges"], 2, "{cov}");
}

/// Rule 4: an include directory inside a git submodule is an edge to the
/// submodule's component. Declarations alone give no such edge.
#[test]
fn an_include_inside_a_submodule_is_an_edge_to_it() {
    let f = materialise("fetch");
    f.fx.repo("subdep");
    f.fx.write("subdep/include/sub.h", "\n");
    f.fx.git(&f.fx.path("subdep"), &["add", "."]);
    f.fx.git(&f.fx.path("subdep"), &["commit", "-q", "-m", "headers"]);
    f.fx.add_submodule("tree", "subdep", "vendor/sub");
    let before = scan_bt(&f, &[&f.build]);
    let sub = cdx_components(&before.cdx)
        .into_iter()
        .find(|c| c["purl"].as_str().is_some_and(|p| p.contains("subdep")))
        .unwrap_or_else(|| panic!("no submodule component: {:#?}", cdx_components(&before.cdx)))["purl"]
        .as_str()
        .unwrap()
        .to_string();
    let probe = purl_of_named(&before, "probe");
    assert!(!cdx_edges(&before.cdx).contains(&(probe.clone(), sub.clone())), "control: no edge before");
    edit_compile_db(&f, "-I", &format!("-I{}/vendor/sub/include -I", f.src.display()));
    // Not `checked`: a submodule added by local path carries that path in
    // its own identity (spec 1530), which is not this feature's output.
    let s = scan_bt(&f, &[&f.build]);
    parity(&s);
    assert_edge(&s, &probe, &sub);
    assert_eq!(coverage_bt(&s).unwrap()["compile-edges"], 3);
}

/// #1717 review, finding 2: a Find-module cache entry is not a package
/// directory. `ZLIB_INCLUDE_DIR=<prefix>/include` shares `hdronly_DIR`'s
/// prefix; counted as a package, it tied with `hdronly` in rule 3, which
/// then resolved the include to nothing and dropped `compile-include`.
#[test]
fn a_find_module_include_dir_is_not_a_package_prefix() {
    let f = materialise("system");
    let cache = f.build.join("CMakeCache.txt");
    let text = std::fs::read_to_string(&cache).unwrap();
    let dir = text.lines().find_map(|l| l.strip_prefix("hdronly_DIR:PATH=")).expect("fixture drift: hdronly_DIR").to_string();
    let prefix = dir.strip_suffix("/lib/cmake/hdronly").expect("fixture drift: hdronly_DIR layout");
    std::fs::write(&cache, format!("{text}\n//Path to a file.\nZLIB_INCLUDE_DIR:PATH={prefix}/include\n")).unwrap();
    // The fixture has a file-API reply, which is preferred (US4): the same
    // entry goes into its cache, with the `HELPSTRING` a Find module writes.
    let reply = reply_file(&f, "cache-v2-");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&reply).unwrap()).unwrap();
    v["entries"].as_array_mut().expect("fixture drift: cache-v2 entries").push(serde_json::json!({
        "name": "ZLIB_INCLUDE_DIR", "type": "PATH", "value": format!("{prefix}/include"),
        "properties": [{ "name": "HELPSTRING", "value": "Path to a file." }],
    }));
    std::fs::write(&reply, serde_json::to_string(&v).unwrap()).unwrap();
    for source in ["the reply", "CMakeCache.txt"] {
        let s = checked(&f, &[&f.build]);
        assert_eq!(coverage_bt(&s).unwrap()["file-api-replies"], u64::from(source == "the reply"), "{source}");
        assert_eq!(basis(&settlement(&s, "hdronly")), ["cache-package-dir", "compile-include"], "{source}");
        let cov = coverage_bt(&s).unwrap();
        assert_eq!(cov["include-dirs-unresolved"], 0, "{source}: {cov}");
        std::fs::remove_dir_all(f.build.join(".cmake")).ok();
    }
}

// ---------------------------------------------------------------------
// US4: the file API (FR-010)
// ---------------------------------------------------------------------

fn reply_dir(f: &Fixture) -> PathBuf {
    f.build.join(".cmake/api/v1/reply")
}

fn reply_file(f: &Fixture, prefix: &str) -> PathBuf {
    std::fs::read_dir(reply_dir(f))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.file_name().unwrap().to_string_lossy().starts_with(prefix))
        .unwrap_or_else(|| panic!("fixture drift: no {prefix}* reply"))
}

/// Every C245 value by component name, and the compile edges: what the
/// reply and the files must agree on.
fn outcome(s: &Scan) -> (Vec<(String, Value)>, Value) {
    let mut settled: Vec<(String, Value)> = cdx_components(&s.cdx)
        .into_iter()
        .filter_map(|c| Some((c["name"].as_str()?.to_string(), serde_json::from_str(prop(c, C245)?).ok()?)))
        .collect();
    settled.sort_by(|a, b| a.0.cmp(&b.0));
    (settled, coverage_bt(s).unwrap()["compile-edges"].clone())
}

/// FX/system has a reply: it is read, and it gives the settlement and the
/// edges the cache and the compilation database give.
#[test]
fn a_file_api_reply_is_read_and_agrees_with_the_files() {
    let f = materialise("system");
    let with = checked(&f, &[&f.build]);
    let cov = coverage_bt(&with).unwrap();
    assert_eq!(cov["file-api-replies"], 1, "{cov}");
    assert_eq!(cov["unreadable-files"], 0, "{cov}");
    assert_edge(&with, &purl_of_named(&with, "probe"), &purl_of_named(&with, "hdronly"));
    std::fs::remove_dir_all(f.build.join(".cmake")).unwrap();
    let without = checked(&f, &[&f.build]);
    assert_eq!(coverage_bt(&without).unwrap()["file-api-replies"], 0);
    assert_eq!(outcome(&with), outcome(&without));
    assert_eq!(cdx_edges(&with.cdx), cdx_edges(&without.cdx));
}

/// Where the reply and `CMakeCache.txt` disagree, the reply is used. A
/// package search under `if(BACKEND STREQUAL "B")`: the cache says B.
#[test]
fn the_reply_cache_is_preferred_over_cmakecache() {
    let f = materialise("system");
    let lists = f.src.join("CMakeLists.txt");
    let text = std::fs::read_to_string(&lists).unwrap() + "if(BACKEND STREQUAL \"B\")\n  find_package(WaybillFixtureSearched)\nendif()\n";
    std::fs::write(&lists, text).unwrap();
    let s = checked(&f, &[&f.build]);
    assert_eq!(scope(named(&s, "waybillfixturesearched")), "required", "both say B");
    let cache = reply_file(&f, "cache-v2-");
    let text = std::fs::read_to_string(&cache).unwrap();
    let b = "\"type\" : \"STRING\",\n\t\t\t\"value\" : \"B\"";
    assert!(text.contains(b), "fixture drift: BACKEND=B in the reply");
    std::fs::write(&cache, text.replacen(b, "\"type\" : \"STRING\",\n\t\t\t\"value\" : \"A\"", 1)).unwrap();
    let s = checked(&f, &[&f.build]);
    assert_eq!(scope(named(&s, "waybillfixturesearched")), "excluded", "the reply says A");
}

/// A malformed reply is counted, and the files are used instead.
#[test]
fn a_malformed_reply_falls_back_to_the_files() {
    let f = materialise("system");
    let good = outcome(&checked(&f, &[&f.build]));
    std::fs::write(reply_file(&f, "codemodel-v2-"), "{\"configurations\": [").unwrap();
    let s = checked(&f, &[&f.build]);
    let cov = coverage_bt(&s).unwrap();
    assert_eq!(cov["file-api-replies"], 0, "{cov}");
    assert_eq!(cov["unreadable-files"], 1, "{cov}");
    assert_eq!(outcome(&s), good);
}

/// The reply's codemodel replaces the compilation database: without the
/// database, the reply alone gives the same compile edges.
#[test]
fn the_reply_codemodel_gives_compile_edges_without_a_database() {
    let f = materialise("system");
    let good = outcome(&checked(&f, &[&f.build]));
    std::fs::remove_file(f.build.join("compile_commands.json")).unwrap();
    let s = checked(&f, &[&f.build]);
    assert_eq!(coverage_bt(&s).unwrap()["compile-edges"], 2);
    assert_eq!(outcome(&s), good);
}

#[test]
fn without_a_reply_none_is_counted() {
    let f = materialise("fetch");
    let s = checked(&f, &[&f.build]);
    assert_eq!(coverage_bt(&s).unwrap()["file-api-replies"], 0);
}
