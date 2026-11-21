//! Spec 1639 (#1558): a component that several readers found records the
//! other readers' mechanisms in C56 (`waybill:also-detected-via`).
//!
//! Contract `specs/1639-also-detected-via/contracts/wire.md`, cases A-1..A-10,
//! A-ctl and A-rep, on synthetic `waybill-fixture-*` trees. Every scan is
//! checked through the parity extractors for C55, C56 and C242 in the three
//! formats, validated against the vendored CycloneDX 1.6 schema, has no
//! repeated `bom-ref`, and is scanned ten times to the same masked output.

#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::path::Path;

use common::cmake_fetch::{cdx_components, prop, scan, Docs, ROOT};
use common::git_fixture::GitFixture;
use corpus_harness_195::layer2_golden::{mask_nondeterministic, masked_difference};
use serde_json::Value;

/// C242 joins in T021, when its row exists.
const ROWS: &[&str] = &["C55", "C56"];
const C56: &str = "waybill:also-detected-via";
const C55: &str = "waybill:source-mechanism";
const GH: &str = "https://github.com/waybill-org/waybill-fixture-dep.git";
const GH_PURL: &str = "pkg:github/waybill-org/waybill-fixture-dep";
const REPEATS: usize = 10;

// ---------------------------------------------------------------------
// Helpers (T001)
// ---------------------------------------------------------------------

fn masked(v: &Value) -> Value {
    let s = serde_json::to_string(&mask_nondeterministic(v)).unwrap();
    let re = regex::Regex::new(r#"[^"]*?[/\\]waybill-fixture-tree"#).unwrap();
    serde_json::from_str(&re.replace_all(&s, "/TMP/waybill-fixture-tree")).unwrap()
}

/// FR-011: the formats agree on C55, C56 and C242; the CycloneDX document
/// validates; no `bom-ref` repeats; no method object carries a key outside
/// the schema's three (US2).
fn parity(d: &Docs) {
    for row in ROWS {
        let e = waybill::parity::extractors::EXTRACTORS
            .iter()
            .find(|e| e.row_id == *row)
            .unwrap_or_else(|| panic!("no extractor for {row}"));
        let (a, b, c) = ((e.cdx)(&d.cdx), (e.spdx23)(&d.spdx23), (e.spdx3)(&d.spdx3));
        assert_eq!(a, b, "{row}: CycloneDX and SPDX 2.3 disagree");
        assert_eq!(a, c, "{row}: CycloneDX and SPDX 3 disagree");
    }
    let errors = common::cdx_schema::cdx_validation_errors(&d.cdx);
    assert!(errors.is_empty(), "CycloneDX schema errors: {errors:?}");
    let mut refs: Vec<&str> = cdx_components(&d.cdx).into_iter().filter_map(|c| c["bom-ref"].as_str()).collect();
    refs.extend(d.cdx["metadata"]["component"]["bom-ref"].as_str());
    let mut seen = std::collections::BTreeSet::new();
    let dups: Vec<&str> = refs.iter().copied().filter(|r| !seen.insert(*r)).collect();
    assert!(dups.is_empty(), "duplicate bom-refs: {dups:?}");
    for c in cdx_components(&d.cdx) {
        for id in c["evidence"]["identity"].as_array().into_iter().flatten() {
            for m in id["methods"].as_array().into_iter().flatten() {
                let extra: Vec<&String> = m
                    .as_object()
                    .map(|o| o.keys().filter(|k| !matches!(k.as_str(), "technique" | "confidence" | "value")).collect())
                    .unwrap_or_default();
                assert!(extra.is_empty(), "method object with keys outside the schema: {m}");
            }
        }
    }
}

/// FR-013 (A-rep): repeat scans give the same masked output.
fn assert_repeatable(scan_once: &dyn Fn() -> Docs, first: &Docs) {
    for _ in 1..REPEATS {
        let again = scan_once();
        for (name, a, b) in [("cdx", &first.cdx, &again.cdx), ("spdx23", &first.spdx23, &again.spdx23), ("spdx3", &first.spdx3, &again.spdx3)] {
            let (a, b) = (masked(a), masked(b));
            assert!(a == b, "A-rep {name}: two scans differ; {}", masked_difference(&a, &b).unwrap_or_default());
        }
    }
}

/// One checked scan of a fixture tree: parity, schema, refs, repeatability.
fn check(fx: &GitFixture, path: &Path) -> Docs {
    check_with(fx, path, &[])
}

fn check_with(fx: &GitFixture, path: &Path, extra: &[&str]) -> Docs {
    let once = || scan(fx, path, extra);
    let d = once();
    parity(&d);
    assert_repeatable(&once, &d);
    d
}

/// The component whose PURL starts with `prefix`; exactly one.
fn one<'a>(d: &'a Docs, prefix: &str) -> &'a Value {
    let all: Vec<&Value> = cdx_components(&d.cdx)
        .into_iter()
        .filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with(prefix)))
        .collect();
    assert_eq!(all.len(), 1, "expected one component for {prefix}, got {}: {all:?}", all.len());
    all[0]
}

/// C56 from the CycloneDX property, as a list.
fn c56(c: &Value) -> Option<Vec<String>> {
    prop(c, C56).map(|v| serde_json::from_str(v).unwrap())
}

fn c55(c: &Value) -> Option<&str> {
    prop(c, C55)
}

/// FR-001: C56 is the other mechanisms of the merge, sorted.
fn assert_c56(c: &Value, expected: &[&str]) {
    let want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    assert_eq!(c56(c), Some(want), "C56 of {}: {c}", c["purl"]);
    assert!(!c56(c).unwrap().iter().any(|m| Some(m.as_str()) == c55(c)), "C56 holds the component's own C55: {c}");
}

fn fetch(name: &str, repo: &str, tag: &str) -> String {
    format!("FetchContent_Declare({name} GIT_REPOSITORY {repo} GIT_TAG {tag})\n")
}

fn guarded(body: &str) -> String {
    format!("if(NOT TARGET waybill_fixture_dep)\n{body}endif()\n")
}

/// A super-project `app` with a submodule of `GH` at `sha` at `extern/<dir>`,
/// plus the given root `CMakeLists.txt` body and extra files.
fn submodule_app(dir: &str, sha: &str, cmake: &str, extra: &[(&str, &str)]) -> GitFixture {
    let fx = GitFixture::new();
    fx.repo("app");
    fx.write("app/.gitmodules", &format!("[submodule \"d\"]\n\tpath = extern/{dir}\n\turl = {GH}\n"));
    fx.write("app/CMakeLists.txt", &format!("{ROOT}{cmake}"));
    for (rel, text) in extra {
        fx.write(&format!("app/{rel}"), text);
    }
    let app = fx.path("app");
    fx.git(&app, &["add", ".gitmodules"]);
    fx.add_gitlink("app", &format!("extern/{dir}"), sha);
    fx
}

// ---------------------------------------------------------------------
// A-ctl (T002): nothing to record, output byte-identical to `main`
// ---------------------------------------------------------------------

const GOLDEN_ENV: &str = "WAYBILL_UPDATE_ALSO_DETECTED_GOLDENS";

fn golden_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/also_detected_via/ctl-golden")
}

/// One merge of two members with the same mechanism (two required fetches of
/// one repository at one tag, merged by R4), a submodule of another
/// repository, and a `.pc` file: no merge joins two mechanisms.
fn ctl_fixture() -> GitFixture {
    let fx = GitFixture::new();
    fx.repo("waybill-fixture-tree");
    fx.write(
        "waybill-fixture-tree/.gitmodules",
        "[submodule \"o\"]\n\tpath = extern/waybill-fixture-other\n\turl = https://github.com/waybill-org/waybill-fixture-other.git\n",
    );
    fx.write("waybill-fixture-tree/CMakeLists.txt", &format!("{ROOT}{}", fetch("waybill-fixture-dep", GH, "v1.0.0")));
    fx.write("waybill-fixture-tree/sub/CMakeLists.txt", &fetch("waybill-fixture-dep", GH, "v1.0.0"));
    fx.write("waybill-fixture-tree/opt/sdk/lib/pkgconfig/waybill-fixture-pc.pc", "Version: 3.1\n");
    let t = fx.path("waybill-fixture-tree");
    fx.git(&t, &["add", ".gitmodules"]);
    fx.add_gitlink("waybill-fixture-tree", "extern/waybill-fixture-other", &"b".repeat(40));
    fx
}

#[test]
fn a_ctl_nothing_to_record_is_byte_identical_to_main() {
    let fx = ctl_fixture();
    let d = check(&fx, &fx.path("waybill-fixture-tree"));
    assert!(cdx_components(&d.cdx).iter().all(|c| prop(c, C56).is_none()), "A-ctl has no merge of two mechanisms");
    let dir = golden_dir();
    let docs = [("cdx.json", &d.cdx), ("spdx-2.3.json", &d.spdx23), ("spdx-3.json", &d.spdx3)];
    if std::env::var_os(GOLDEN_ENV).is_some() {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, doc) in docs {
            let text = serde_json::to_string_pretty(&masked(doc)).unwrap() + "\n";
            std::fs::write(dir.join(name), text).unwrap();
        }
        return;
    }
    for (name, doc) in docs {
        let want: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(name)).unwrap()).unwrap();
        let got = masked(doc);
        assert!(got == want, "A-ctl {name} differs from main's golden; {}", masked_difference(&want, &got).unwrap_or_default());
    }
}

// ---------------------------------------------------------------------
// US1 (T008): a merged component says which other readers found it
// ---------------------------------------------------------------------

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const UNSET: &str = "${WAYBILL_FIXTURE_UNSET}";

fn ep(name: &str, repo: &str, tag: &str) -> String {
    format!("ExternalProject_Add({name} GIT_REPOSITORY {repo} GIT_TAG {tag})\n")
}

/// FR-001: C56 is every mechanism of the merge but the component's own C55,
/// sorted. The survivor is whichever member today's rule keeps (FR-003), so
/// the expectation is computed from the emitted C55.
fn assert_c56_is_the_rest(c: &Value, mechanisms: &[&str]) {
    let own = c55(c).unwrap_or_else(|| panic!("the survivor has a C55: {c}"));
    assert!(mechanisms.contains(&own), "C55 {own} is one of the merge's mechanisms {mechanisms:?}");
    let mut rest: Vec<&str> = mechanisms.iter().copied().filter(|m| *m != own).collect();
    rest.sort_unstable();
    assert_c56(c, &rest);
}

/// A-1 (S1): a submodule and a required fetch of one repository at one
/// commit merge into one component that names both readers.
#[test]
fn a01_submodule_and_fetch_at_one_commit() {
    let fx = submodule_app("waybill-fixture-dep", SHA, &fetch("waybill-fixture-dep", GH, SHA), &[]);
    let d = check(&fx, &fx.path("app"));
    let c = one(&d, GH_PURL);
    assert_eq!(c["purl"], format!("{GH_PURL}@{SHA}"));
    assert_c56_is_the_rest(c, &["cmake-fetchcontent-git", "git-submodule"]);
}

/// A-2 (S1): an opkg-installed database and a Yocto image manifest naming one
/// package. `opkg-installed` wins today (`cross_reader_dedup_ecosystem_107`).
#[test]
fn a02_opkg_database_and_image_manifest() {
    let fx = common::cmake_fetch::tree(&[
        (
            "var/lib/opkg/status",
            "Package: waybill-fixture-shared\nVersion: 9.9.9\nArchitecture: waybill-fixture-arch\n\
             Maintainer: Waybill Fixture <fixture@example.invalid>\nStatus: install user installed\n",
        ),
        (
            "build/tmp/deploy/images/waybill-fixture-machine/waybill-fixture-image.manifest",
            "waybill-fixture-shared waybill-fixture-arch 9.9.9\n",
        ),
    ]);
    let d = check(&fx, &fx.path("t"));
    let c = one(&d, "pkg:opkg/waybill-fixture-shared@9.9.9");
    assert_eq!(c55(c), Some("opkg-installed"));
    assert_c56(c, &["yocto-image-manifest"]);
}

/// A-3 (S1): a pkg-config file and a CMake config package of one library at
/// one version (spec 1531 SC-003): one component, the `.pc` mechanism.
#[test]
fn a03_pkg_config_and_cmake_config_package() {
    let fx = common::cmake_fetch::tree(&[
        ("opt/sdk/lib/pkgconfig/waybill-fixture-m.pc", "Version: 7.0\n"),
        ("opt/sdk/lib/cmake/waybill-fixture-m/waybill-fixture-m-config-version.cmake", "set(PACKAGE_VERSION \"7.0\")\n"),
    ]);
    let d = check(&fx, &fx.path("t"));
    let c = one(&d, "pkg:generic/waybill-fixture-m@7.0");
    assert_eq!(c55(c), Some("pkg-config"));
    assert_c56(c, &["cmake-package-version"]);
}

/// A-4 (S5 then S1): a submodule, a fetch and an `ExternalProject_Add` of one
/// repository at one commit: three mechanisms, two in C56.
#[test]
fn a04_three_mechanisms() {
    let body = format!("{}{}", fetch("waybill-fixture-dep", GH, SHA), ep("waybill-fixture-dep", GH, SHA));
    let fx = submodule_app("waybill-fixture-dep", SHA, &body, &[]);
    let d = check(&fx, &fx.path("app"));
    let c = one(&d, GH_PURL);
    assert_c56_is_the_rest(c, &["cmake-externalproject", "cmake-fetchcontent-git", "git-submodule"]);
}

/// A-6 (S6, spec 1538 R9): a conditional fetch under a submodule of the same
/// repository is recorded on the submodule: C233 as before, and C56.
#[test]
fn a06_conditional_fetch_under_its_submodule() {
    let fx = submodule_app("waybill-fixture-dep", SHA, &guarded(&fetch("waybill-fixture-dep", GH, UNSET)), &[]);
    let d = check(&fx, &fx.path("app"));
    let c = one(&d, GH_PURL);
    assert_eq!(c55(c), Some("git-submodule"));
    assert!(prop(c, "waybill:superseded-declarations").is_some(), "C233 still records the fetch: {c}");
    assert_c56(c, &["cmake-fetchcontent-git"]);
}

/// A-7 (S6 then S1): the R9 survivor merges again with a required
/// `ExternalProject_Add` at the same commit: one C56, the union.
#[test]
fn a07_two_sites_give_one_union() {
    let body = format!("{}{}", guarded(&fetch("waybill-fixture-dep", GH, UNSET)), ep("waybill-fixture-dep", GH, SHA));
    let fx = submodule_app("waybill-fixture-dep", SHA, &body, &[]);
    let d = check(&fx, &fx.path("app"));
    let c = one(&d, GH_PURL);
    assert_c56_is_the_rest(c, &["cmake-externalproject", "cmake-fetchcontent-git", "git-submodule"]);
}

/// A-9 (S7): with `--include-vendored`, a `third_party/<n>` directory that
/// declares a project folds into that project.
#[test]
fn a09_vendored_directory_folds_into_its_project() {
    let fx = common::cmake_fetch::tree(&[
        ("CMakeLists.txt", "cmake_minimum_required(VERSION 3.25)\nproject(waybill_fixture_superbuild)\nadd_subdirectory(third_party/vlib)\n"),
        ("third_party/vlib/CMakeLists.txt", "cmake_minimum_required(VERSION 3.25)\nproject(waybill-fixture-vlib VERSION 1.0.0)\n"),
    ]);
    let d = check_with(&fx, &fx.path("t"), &["--include-vendored"]);
    let c = one(&d, "pkg:generic/waybill-fixture-vlib@1.0.0");
    assert_eq!(c55(c), Some("cmake-project"));
    assert_c56(c, &["cmake-vendored"]);
}

/// A-10 (S9): a package search named like a submodule is recorded on it.
#[test]
fn a10_package_search_paired_with_a_submodule() {
    let fx = submodule_app("waybill-fixture-dep", SHA, "find_package(waybill-fixture-dep 1.0)\n", &[]);
    let d = check(&fx, &fx.path("app"));
    let c = one(&d, GH_PURL);
    assert_eq!(c55(c), Some("git-submodule"));
    assert_c56(c, &["cmake-find-package"]);
}
