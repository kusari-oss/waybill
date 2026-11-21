//! Spec 1638 (#1595): a CMake git fetch whose `GIT_REPOSITORY` is literal but
//! whose `GIT_TAG` is unresolved is named after its repository.
//!
//! Contract `specs/1638-unresolved-github-fetch/contracts/wire.md`, cases
//! U-1..U-17, U-ctl, U-rep and U-xhost, on synthetic `waybill-fixture-*`
//! trees. Every scan is checked through the parity extractors for C55, C122,
//! C151, C232, C233, C234 and C236 in the three formats, has no repeated
//! `bom-ref`, and is scanned twice to the same masked output (U-rep).

#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::path::Path;

use common::cmake_fetch::*;
use common::git_fixture::GitFixture;
use corpus_harness_195::layer2_golden::{mask_nondeterministic, masked_difference};
use serde_json::{json, Value};

const ROWS: &[&str] = &["C55", "C122", "C151", "C232", "C233", "C234", "C236"];
const C151_REASON: &str = "URL or GIT_TAG uses a CMake variable with no literal set() in the same file";
/// A mixed-case GitHub repository: the PURL lowercases it (spec 1530 FR-015).
const GH: &str = "https://github.com/Waybill-Org/Waybill-Fixture-Dep.git";
const GH_PURL: &str = "pkg:github/waybill-org/waybill-fixture-dep";
const OTHER: &str = "https://example.invalid/waybill-fixture-other.git";
const UNSET: &str = "${WAYBILL_FIXTURE_UNSET}";

// ---------------------------------------------------------------------
// Helpers (T002)
// ---------------------------------------------------------------------

fn masked(v: &Value) -> Value {
    let s = serde_json::to_string(&mask_nondeterministic(v)).unwrap();
    let re = regex::Regex::new(r#"[^"]*?[/\\]waybill-fixture-tree"#).unwrap();
    serde_json::from_str(&re.replace_all(&s, "/TMP/waybill-fixture-tree")).unwrap()
}

/// FR-009: the formats agree on every row, and no `bom-ref` repeats.
fn assert_rows_and_refs(d: &Docs) {
    for row in ROWS {
        let e = waybill::parity::extractors::EXTRACTORS
            .iter()
            .find(|e| e.row_id == *row)
            .unwrap_or_else(|| panic!("no extractor for {row}"));
        let (a, b, c) = ((e.cdx)(&d.cdx), (e.spdx23)(&d.spdx23), (e.spdx3)(&d.spdx3));
        assert_eq!(a, b, "{row}: CycloneDX and SPDX 2.3 disagree");
        assert_eq!(a, c, "{row}: CycloneDX and SPDX 3 disagree");
    }
    let mut refs: Vec<&str> = cdx_components(&d.cdx).into_iter().filter_map(|c| c["bom-ref"].as_str()).collect();
    refs.extend(d.cdx["metadata"]["component"]["bom-ref"].as_str());
    let mut seen = std::collections::BTreeSet::new();
    let dups: Vec<&str> = refs.iter().copied().filter(|r| !seen.insert(*r)).collect();
    assert!(dups.is_empty(), "duplicate bom-refs: {dups:?}");
}

/// One checked scan: rows, refs, and a second scan to the same masked output
/// (U-rep, FR-011).
fn check(fx: &GitFixture, path: &Path) -> Docs {
    let d = scan(fx, path, &[]);
    assert_rows_and_refs(&d);
    let again = scan(fx, path, &[]);
    for (name, a, b) in [("cdx", &d.cdx, &again.cdx), ("spdx23", &d.spdx23, &again.spdx23), ("spdx3", &d.spdx3, &again.spdx3)] {
        let (a, b) = (masked(a), masked(b));
        assert!(a == b, "U-rep {name}: two scans differ; {}", masked_difference(&a, &b).unwrap_or_default());
    }
    d
}

fn check_tree(files: &[(&str, &str)]) -> Docs {
    let fx = tree(files);
    check(&fx, &fx.path("t"))
}

fn git(name: &str, repo: &str, tag: &str) -> String {
    format!("FetchContent_Declare({name} GIT_REPOSITORY {repo} GIT_TAG {tag})\n")
}

fn ep(name: &str, repo: &str, tag: &str) -> String {
    format!("ExternalProject_Add({name} GIT_REPOSITORY {repo} GIT_TAG {tag})\n")
}

fn root(extra: &str) -> String {
    format!("{ROOT}{extra}")
}

fn superseded(c: &Value) -> Value {
    serde_json::from_str(prop(c, "waybill:superseded-declarations").unwrap_or("null")).unwrap()
}

fn source_files(c: &Value) -> Vec<String> {
    serde_json::from_str(prop(c, "waybill:source-files").unwrap_or("[]")).unwrap()
}

/// FR-001: a versionless GitHub component that keeps C151 and the design tier.
fn assert_unresolved_github(c: &Value, mechanism: &str) {
    assert_eq!(c["purl"], GH_PURL, "no version, lowercased: {c}");
    assert!(c["version"].as_str().unwrap_or("").is_empty(), "no version: {c}");
    assert_eq!(prop(c, "waybill:unresolved-reason"), Some(C151_REASON), "{c}");
    assert_eq!(prop(c, "waybill:sbom-tier"), Some("design"), "{c}");
    assert_eq!(prop(c, "waybill:source-mechanism"), Some(mechanism), "{c}");
    assert_eq!(prop(c, "waybill:download-url"), Some(GH), "{c}");
}

fn chain(names: &[&str]) -> Option<Vec<Vec<String>>> {
    Some(vec![names.iter().map(|s| s.to_string()).collect()])
}

/// A DownloadProject-shaped module, as in `cmake_fetch_wrappers.rs` (spec
/// 1540); written for these tests, not a copy of the upstream file.
fn dp_module() -> String {
    "set(_DownloadProjectDir \"${CMAKE_CURRENT_LIST_DIR}\")\n\
     function(download_project)\n\
     set(options QUIET)\n\
     set(oneValueArgs PROJ PREFIX DOWNLOAD_DIR SOURCE_DIR BINARY_DIR BUILD_PROJECT)\n\
     set(multiValueArgs \"\")\n\
     cmake_parse_arguments(DL_ARGS \"${options}\" \"${oneValueArgs}\" \"${multiValueArgs}\" ${ARGN})\n\
     if(NOT DL_ARGS_PREFIX)\nset(DL_ARGS_PREFIX \"${CMAKE_BINARY_DIR}\")\nendif()\n\
     if(NOT DL_ARGS_DOWNLOAD_DIR)\nset(DL_ARGS_DOWNLOAD_DIR \"${DL_ARGS_PREFIX}/${DL_ARGS_PROJ}-download\")\nendif()\n\
     configure_file(\"${_DownloadProjectDir}/DownloadProject.CMakeLists.cmake.in\" \"${DL_ARGS_DOWNLOAD_DIR}/CMakeLists.txt\")\n\
     execute_process(COMMAND ${CMAKE_COMMAND} . WORKING_DIRECTORY \"${DL_ARGS_DOWNLOAD_DIR}\")\n\
     execute_process(COMMAND ${CMAKE_COMMAND} --build . WORKING_DIRECTORY \"${DL_ARGS_DOWNLOAD_DIR}\")\n\
     endfunction()\n"
        .to_string()
}

/// The aotriton shape (python-pytorch `cmake/External/aotriton.cmake`): a
/// wrapper whose name is a parameter, and a tag set twice, once from the
/// environment (FR-002).
fn aotriton_shape(repo: &str) -> String {
    format!(
        "set(WAYBILL_FIXTURE_COMMIT $ENV{{WAYBILL_FIXTURE_COMMIT}})\n\
         if(NOT WAYBILL_FIXTURE_COMMIT)\nset(WAYBILL_FIXTURE_COMMIT 0123456789abcdef0123456789abcdef01234567)\nendif()\n\
         function(wf_dep noimage project)\n\
         ExternalProject_Add(${{project}} GIT_REPOSITORY {repo} GIT_TAG ${{WAYBILL_FIXTURE_COMMIT}})\n\
         endfunction()\n\
         wf_dep(OFF waybill-fixture-target)\n"
    )
}

fn tag_wrapper() -> String {
    format!("function(wf_tag name tag)\nFetchContent_Declare(${{name}} GIT_REPOSITORY {GH} GIT_TAG ${{tag}})\nendfunction()\n")
}

// ---------------------------------------------------------------------
// U-ctl: byte-identity control (T003, FR-010)
// ---------------------------------------------------------------------

const GOLDEN_DIR: &str = "tests/fixtures/cmake_unresolved_github_fetch";
const REGEN: &str = "WAYBILL_UPDATE_CMAKE_UNRESOLVED_GOLDENS";

fn against_golden(d: &Docs, dir: &str) {
    let golden_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_DIR).join(dir);
    for (name, doc) in [("cdx", &d.cdx), ("spdx-2.3", &d.spdx23), ("spdx-3", &d.spdx3)] {
        let got = masked(doc);
        let path = golden_dir.join(format!("{name}.json"));
        if std::env::var(REGEN).is_ok() {
            std::fs::create_dir_all(&golden_dir).unwrap();
            std::fs::write(&path, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
            continue;
        }
        let want: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(got == want, "{dir}/{name}: differs from the golden; {}", masked_difference(&want, &got).unwrap_or_default());
    }
}

/// Every unchanged shape: resolved fetches, a URL with an unresolved version,
/// an unresolved repository, and a wrapper forwarding a literal tag. The
/// golden was generated with `main`'s binary before any source change.
#[test]
fn u_ctl_unchanged_shapes_are_byte_identical_to_main() {
    let fx = GitFixture::new();
    fx.write(
        "waybill-fixture-tree/CMakeLists.txt",
        &root(&format!(
            "{}{}{}{}{}{}wf_tag(waybill-fixture-a v1.2)\n",
            git("waybill-fixture-gh", "https://github.com/waybill-org/waybill-fixture-gh.git", "v1.0"),
            git("waybill-fixture-gen", "https://example.invalid/waybill-fixture-gen.git", "v2"),
            url_fetch("waybill-fixture-url", "3.0", ""),
            "FetchContent_Declare(waybill-fixture-u URL https://example.invalid/waybill-fixture-u-${WAYBILL_FIXTURE_V}.tar.gz)\n",
            git("waybill-fixture-ur", "https://github.com/${WAYBILL_FIXTURE_OWNER}/waybill-fixture-ur.git", UNSET),
            tag_wrapper(),
        )),
    );
    let d = check(&fx, &fx.path("waybill-fixture-tree"));
    against_golden(&d, "ctl-golden");
}

// ---------------------------------------------------------------------
// US1: a fetched GitHub repository is named after the repository (T004)
// ---------------------------------------------------------------------

/// U-1: a direct `FetchContent_Declare`.
#[test]
fn u1_direct_fetchcontent() {
    let d = check_tree(&[("CMakeLists.txt", &root(&git("waybill-fixture-target", GH, UNSET)))]);
    let c = the(&d, GH_PURL);
    assert_unresolved_github(c, "cmake-fetchcontent-git");
    assert_eq!(c["name"], "waybill-fixture-target", "the declared name stays the name");
    assert!(required(c), "{c}");
}

/// U-2: a direct `ExternalProject_Add`.
#[test]
fn u2_direct_externalproject() {
    let d = check_tree(&[("CMakeLists.txt", &root(&ep("waybill-fixture-target", GH, UNSET)))]);
    assert_unresolved_github(the(&d, GH_PURL), "cmake-externalproject");
}

/// U-3: the aotriton shape keeps its wrapper chain (FR-001, research R2).
#[test]
fn u3_aotriton_shaped_wrapper_keeps_c234() {
    let d = check_tree(&[("CMakeLists.txt", &root(&aotriton_shape(GH)))]);
    let c = the(&d, GH_PURL);
    assert_unresolved_github(c, "cmake-externalproject");
    assert_eq!(c["name"], "waybill-fixture-target");
    assert_eq!(wrapper_chains(c), chain(&["wf_dep"]));
    let cov = coverage(&d).unwrap();
    assert_eq!((n(&cov, &["wrappers", "definitions"]), n(&cov, &["wrappers", "calls"])), (1, 1), "{cov}");
    assert_eq!(fetches(&d).len(), 1, "the template is not a declaration: {:?}", fetch_purls(&d));
}

/// U-4: `download_project()` (spec 1540).
#[test]
fn u4_download_project() {
    let d = check_tree(&[
        ("cmake/DownloadProject.cmake", &dp_module()),
        (
            "CMakeLists.txt",
            &root(&format!("include(cmake/DownloadProject.cmake)\ndownload_project(PROJ waybill-fixture-target GIT_REPOSITORY {GH} GIT_TAG {UNSET})\n")),
        ),
    ]);
    let c = the(&d, GH_PURL);
    assert_unresolved_github(c, "cmake-externalproject");
    assert_eq!(wrapper_chains(c), chain(&["download_project"]));
}

/// U-5: an unexpanded declared name is named after the repository and
/// counted as emitted (FR-008).
#[test]
fn u5_unexpanded_name_is_emitted() {
    let d = check_tree(&[("CMakeLists.txt", &root(&git("${WAYBILL_FIXTURE_NAME}", GH, UNSET)))]);
    let c = the(&d, GH_PURL);
    assert_unresolved_github(c, "cmake-fetchcontent-git");
    assert_eq!(c["name"], "waybill-fixture-dep", "named after the repository");
    let cov = coverage(&d).unwrap();
    assert_eq!((n(&cov, &["emitted"]), n(&cov, &["unresolved", "identity"])), (1, 0), "{cov}");
}

/// U-6: a wrapper that forwards its tag. A literal tag is unchanged from
/// `main`; an unresolved one is versionless. Both keep C234.
#[test]
fn u6_a_wrapper_forwarding_the_tag() {
    let d = check_tree(&[(
        "CMakeLists.txt",
        &root(&format!("{}wf_tag(waybill-fixture-a v1.2)\nwf_tag(waybill-fixture-b {UNSET})\n", tag_wrapper())),
    )]);
    let a = the(&d, &format!("{GH_PURL}@v1.2"));
    assert_eq!(a["name"], "waybill-fixture-a");
    assert_eq!(wrapper_chains(a), chain(&["wf_tag"]));
    let b: Vec<&Value> = fetches(&d).into_iter().filter(|c| c["purl"] == GH_PURL).collect();
    assert_eq!(b.len(), 1, "{:?}", fetch_purls(&d));
    assert_unresolved_github(b[0], "cmake-fetchcontent-git");
    assert_eq!(b[0]["name"], "waybill-fixture-b");
    assert_eq!(wrapper_chains(b[0]), chain(&["wf_tag"]));
}

// ---------------------------------------------------------------------
// US2: a fallback fetch merges into its supplier (T009)
// ---------------------------------------------------------------------

/// U-7: a literal non-GitHub repository keeps its generic name and gains its
/// URL (FR-004).
#[test]
fn u7_other_repository_records_its_url() {
    let d = check_tree(&[("CMakeLists.txt", &root(&git("waybill-fixture-other", OTHER, UNSET)))]);
    let c = the(&d, "pkg:generic/waybill-fixture-other");
    assert_eq!(c["purl"], "pkg:generic/waybill-fixture-other", "{c}");
    assert_eq!(prop(c, "waybill:unresolved-reason"), Some(C151_REASON));
    assert_eq!(prop(c, "waybill:download-url"), Some(OTHER));
}

/// U-8: a conditional unresolved fallback of that repository merges into
/// the required resolved fetch above it (FR-004, FR-007, FR-013).
#[test]
fn u8_other_repository_fallback_merges() {
    let d = check_tree(&[
        ("CMakeLists.txt", &root(&git("waybill-fixture-other", OTHER, "v1"))),
        ("sub/CMakeLists.txt", &guarded(&git("waybill-fixture-other-fallback", OTHER, UNSET))),
    ]);
    assert_eq!(fetches(&d).len(), 1, "{:?}", fetch_purls(&d));
    let c = the(&d, "pkg:generic/waybill-fixture-other@v1");
    assert!(required(c), "{c}");
    assert_eq!(superseded(c), json!([{"file": "sub/CMakeLists.txt", "version": ""}]));
    assert_eq!(n(&coverage(&d).unwrap(), &["merged", "existing-component"]), 1);
}

fn submodule_tree(fetch_at: &str) -> Docs {
    let sha = "a".repeat(40);
    let fx = GitFixture::new();
    fx.repo("app");
    fx.write(
        "app/.gitmodules",
        "[submodule \"d\"]\n\tpath = elsewhere/waybill-fixture-dep\n\turl = https://github.com/waybill-org/waybill-fixture-dep.git\n",
    );
    let fetch = guarded(&git("waybill-fixture-dep", "https://github.com/waybill-org/waybill-fixture-dep.git", UNSET));
    if fetch_at.is_empty() {
        fx.write("app/CMakeLists.txt", &root(&fetch));
    } else {
        fx.write("app/CMakeLists.txt", ROOT);
        fx.write(&format!("app/{fetch_at}/CMakeLists.txt"), &fetch);
    }
    let app = fx.path("app");
    fx.git(&app, &["add", ".gitmodules"]);
    fx.add_gitlink("app", "elsewhere/waybill-fixture-dep", &sha);
    check(&fx, &app)
}

/// U-11: a git submodule of a GitHub repository, and a conditional
/// unresolved fetch of it in the same directory or a descendant: one
/// component, the submodule's, with the fetch in C233 (FR-007, FR-013).
#[test]
fn u11_fallback_merges_into_a_submodule() {
    for (at, file) in [("", "CMakeLists.txt"), ("a/b", "a/b/CMakeLists.txt")] {
        let d = submodule_tree(at);
        let all: Vec<&Value> = cdx_components(&d.cdx)
            .into_iter()
            .filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with(GH_PURL)))
            .collect();
        assert_eq!(all.len(), 1, "{at:?}: one component for the repository: {:?}", fetch_purls(&d));
        let c = all[0];
        assert_eq!(prop(c, "waybill:source-mechanism"), Some("git-submodule"), "{c}");
        assert!(required(c), "{c}");
        assert_eq!(superseded(c), json!([{"file": file, "version": ""}]), "{at:?}");
        assert_eq!(n(&coverage(&d).unwrap(), &["merged", "existing-component"]), 1);
    }
}

/// U-12: two unresolved fetches of one repository become one component.
#[test]
fn u12_two_unresolved_fetches_of_one_repository() {
    let d = check_tree(&[
        ("CMakeLists.txt", ROOT),
        ("x/CMakeLists.txt", &git("waybill-fixture-a", GH, UNSET)),
        ("y/CMakeLists.txt", &git("waybill-fixture-b", GH, UNSET)),
    ]);
    let c = the(&d, GH_PURL);
    assert_eq!(c["purl"], GH_PURL);
    let files = source_files(c);
    assert!(files.contains(&"x/CMakeLists.txt".to_string()) && files.contains(&"y/CMakeLists.txt".to_string()), "{files:?}");
    let cov = coverage(&d).unwrap();
    assert_eq!((n(&cov, &["emitted"]), n(&cov, &["components"])), (2, 1), "{cov}");
}

/// U-13a: a required resolved fetch and a required unresolved fetch of one
/// repository stay two components (FR-014). Guard: passes at `main`.
#[test]
fn u13a_required_versioned_and_versionless_stay_apart() {
    let d = check_tree(&[
        ("CMakeLists.txt", ROOT),
        ("x/CMakeLists.txt", &git("waybill-fixture-a", GH, "v1")),
        ("y/CMakeLists.txt", &git("waybill-fixture-b", GH, UNSET)),
    ]);
    assert_eq!(fetches(&d).len(), 2, "{:?}", fetch_purls(&d));
    the(&d, &format!("{GH_PURL}@v1"));
}

/// U-13b: the same two declarations under one name in one file stay two
/// components (FR-014, research R5).
#[test]
fn u13b_same_name_same_directory_stay_apart() {
    let d = check_tree(&[(
        "CMakeLists.txt",
        &root(&format!("{}{}", git("waybill-fixture-dep", GH, "v1"), git("waybill-fixture-dep", GH, UNSET))),
    )]);
    let ours: Vec<&Value> = cdx_components(&d.cdx)
        .into_iter()
        .filter(|c| c["name"] == "waybill-fixture-dep")
        .collect();
    assert_eq!(ours.len(), 2, "two components: {:?}", fetch_purls(&d));
}

/// U-14: a conditional resolved fetch merges into a required unresolved one
/// above it, recorded with its version.
#[test]
fn u14_resolved_fallback_merges_into_versionless() {
    let d = check_tree(&[
        ("CMakeLists.txt", &root(&git("waybill-fixture-dep", GH, UNSET))),
        ("sub/CMakeLists.txt", &guarded(&git("waybill-fixture-dep-fallback", GH, "v2"))),
    ]);
    assert_eq!(fetches(&d).len(), 1, "{:?}", fetch_purls(&d));
    let c = the(&d, GH_PURL);
    assert_eq!(c["purl"], GH_PURL);
    assert_eq!(superseded(c), json!([{"file": "sub/CMakeLists.txt", "version": "v2"}]));
}

/// U-15: a conditional unresolved fetch with no supplier stands alone.
#[test]
fn u15_no_supplier() {
    let d = check_tree(&[("CMakeLists.txt", &root(&guarded(&git("waybill-fixture-target", GH, UNSET))))]);
    let c = the(&d, GH_PURL);
    assert_unresolved_github(c, "cmake-fetchcontent-git");
    assert_optional(c, "cmake-not-target");
    assert!(conditions(c).is_some(), "{c}");
}

/// U-16: a non-GitHub `ExternalProject_Add` fallback stays separate, as its
/// resolved form does (research R3). Guard: passes at `main`.
#[test]
fn u16_externalproject_other_repository_stays_separate() {
    let d = check_tree(&[
        ("CMakeLists.txt", &root(&ep("waybill-fixture-ep", OTHER, "v1"))),
        ("sub/CMakeLists.txt", &guarded(&ep("waybill-fixture-ep-fallback", OTHER, UNSET))),
    ]);
    assert_eq!(fetches(&d).len(), 2, "{:?}", fetch_purls(&d));
}

/// U-17: an unresolved declaration lost under first-declaration-wins is
/// recorded with an empty version (FR-013, R5 by name).
#[test]
fn u17_first_declaration_wins_records_an_empty_version() {
    let d = check_tree(&[
        ("CMakeLists.txt", &root(&git("waybill-fixture-dep", GH, "v1"))),
        ("sub/CMakeLists.txt", &git("waybill-fixture-dep", GH, UNSET)),
    ]);
    assert_eq!(fetches(&d).len(), 1, "{:?}", fetch_purls(&d));
    let c = the(&d, &format!("{GH_PURL}@v1"));
    assert_eq!(superseded(c), json!([{"file": "sub/CMakeLists.txt", "version": ""}]));
    assert_eq!(n(&coverage(&d).unwrap(), &["merged", "first-declaration-wins"]), 1);
}

// ---------------------------------------------------------------------
// US3: nothing that is not literal changes (T014)
// ---------------------------------------------------------------------

/// U-9: an unresolved repository is not parsed for an owner (FR-006).
#[test]
fn u9_unresolved_repository_is_unchanged() {
    let repo = "https://github.com/${WAYBILL_FIXTURE_OWNER}/waybill-fixture-ur.git";
    let d = check_tree(&[("CMakeLists.txt", &root(&git("waybill-fixture-ur", repo, UNSET)))]);
    let c = the(&d, "pkg:generic/waybill-fixture-ur");
    assert_eq!(c["purl"], "pkg:generic/waybill-fixture-ur");
    assert_eq!(prop(c, "waybill:unresolved-reason"), Some(C151_REASON));
    assert_eq!(prop(c, "waybill:download-url"), None, "{c}");
}

/// U-10: a URL fetch with an unresolved version is unchanged (FR-005).
#[test]
fn u10_url_fetch_is_unchanged() {
    let d = check_tree(&[(
        "CMakeLists.txt",
        &root("FetchContent_Declare(waybill-fixture-u URL https://example.invalid/waybill-fixture-u-${WAYBILL_FIXTURE_V}.tar.gz)\n"),
    )]);
    let c = the(&d, "pkg:generic/waybill-fixture-u");
    assert_eq!(c["purl"], "pkg:generic/waybill-fixture-u");
    assert_eq!(prop(c, "waybill:unresolved-reason"), Some(C151_REASON));
    assert_eq!(prop(c, "waybill:download-url"), None, "{c}");
}

/// U-xhost (SC-005): the changed shapes, byte-identical to a golden on every
/// CI lane. Generated from the branch binary after US2 (T014).
#[test]
fn u_xhost_changed_shapes_match_the_golden_on_every_host() {
    let fx = GitFixture::new();
    fx.write("waybill-fixture-tree/CMakeLists.txt", &root(&format!("{}{}{}", git("waybill-fixture-target", GH, UNSET), aotriton_shape("https://github.com/waybill-org/waybill-fixture-wrapped.git"), git("waybill-fixture-other", OTHER, UNSET))));
    fx.write("waybill-fixture-tree/x/CMakeLists.txt", &git("waybill-fixture-a", "https://github.com/waybill-org/waybill-fixture-two.git", UNSET));
    fx.write("waybill-fixture-tree/y/CMakeLists.txt", &git("waybill-fixture-b", "https://github.com/waybill-org/waybill-fixture-two.git", UNSET));
    let d = check(&fx, &fx.path("waybill-fixture-tree"));
    against_golden(&d, "changed-golden");
}
