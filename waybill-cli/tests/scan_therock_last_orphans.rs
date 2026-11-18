//! Spec 1543 (#1601, #1602): TheRock's last two unexplained components.
//!
//! Contract `specs/1543-therock-last-orphans/contracts/wire.md`.
//! - **W-1..W-8:** a synthesized workspace root (uv, bun, Kotlin DSL) is
//!   reachable wherever it is not the subject, and is the subject at the
//!   scan root.
//! - **L-1..L-12:** a vendored installed-library record linked by link
//!   directory is a dependency of the project that links it, carrying the
//!   conditions it is linked under.
//!
//! Every case is checked in the three formats and repeated [`SCANS`] times.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::collections::BTreeMap;
use std::path::Path;

use common::installed_lib::{component, depends_on, prop, row_all_formats, scan_with, write, Docs};
use corpus_harness_195::invariants::checks::cdx_component_ref;
use corpus_harness_195::invariants::checks::orphan_explained::{
    cdx_components_with_containers, is_explained, reachable_with_containment,
};
use corpus_harness_195::layer2_golden::{mask_nondeterministic, masked_difference};
use serde_json::Value;

const SCANS: usize = 20;
const OP: &[&str] = &["--root-name", "waybill-fixture-op", "--root-version", "1"];

// ---------------------------------------------------------------------
// Helpers (T001)
// ---------------------------------------------------------------------

fn scan(root: &Path, extra: &[&str]) -> Docs {
    scan_with("--path", root, extra)
}

fn subject(d: &Docs) -> String {
    d.cdx["metadata"]["component"]["bom-ref"].as_str().unwrap_or_default().to_string()
}

/// I7 in one scan: every component the subject cannot reach (through
/// dependencies or containment) is explained, and none that it reaches
/// carries an orphan reason. The formats agree on C45, C104 and C105.
fn assert_explained(d: &Docs) {
    let reachable = reachable_with_containment(&d.cdx);
    let subject = subject(d);
    for (c, _) in cdx_components_with_containers(&d.cdx) {
        let Some(r) = cdx_component_ref(c) else { continue };
        if r == subject {
            continue;
        }
        if reachable.contains(r) {
            assert_eq!(prop(c, "waybill:orphan-reason"), None, "reachable {r} carries C45");
        } else {
            assert!(is_explained(c), "unreachable {r} is unexplained");
        }
    }
    for row in ["C45", "C104", "C105"] {
        row_all_formats(d, row);
    }
}

/// The refs `bom_ref` depends on. `depends_on` finds only components, and
/// the subject is in `metadata`.
fn targets_of(d: &Docs, bom_ref: &str) -> Vec<String> {
    d.cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| e["ref"] == bom_ref)
        .and_then(|e| e["dependsOn"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|t| t.as_str().map(str::to_string))
        .collect()
}

fn reachable(d: &Docs, purl: &str) -> bool {
    reachable_with_containment(&d.cdx).contains(purl)
}

/// [`SCANS`] scans of `root` give one masked document per format.
fn assert_repeatable(root: &Path, extra: &[&str]) {
    let docs: Vec<Docs> = (0..SCANS).map(|_| scan(root, extra)).collect();
    for (f, name) in ["CycloneDX", "SPDX 2.3", "SPDX 3"].iter().enumerate() {
        let mut variants: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, d) in docs.iter().enumerate() {
            let v = [&d.cdx, &d.spdx23, &d.spdx3][f];
            variants.entry(serde_json::to_string_pretty(&mask_nondeterministic(v)).unwrap()).or_default().push(i);
        }
        if variants.len() > 1 {
            let parsed: Vec<Value> = variants.keys().map(|s| serde_json::from_str(s).unwrap()).collect();
            panic!(
                "{name}: {} variants across {SCANS} scans: {:?}; {}",
                variants.len(),
                variants.values().collect::<Vec<_>>(),
                masked_difference(&parsed[0], &parsed[1]).unwrap_or_default()
            );
        }
    }
}

// ---------------------------------------------------------------------
// Workspace fixtures (T002)
// ---------------------------------------------------------------------

const UV_WS: &str = "pkg:generic/waybill-fixture-tensile";
const UV_MEMBERS: [&str; 2] = ["pkg:pypi/waybill-fixture-rocisa@0.1.0", "pkg:pypi/waybill-fixture-tensile@5.0.0"];
const BUN_WS: &str = "pkg:generic/waybill-fixture-mono";
const KTS_WS: &str = "pkg:generic/waybill-fixture-kts@0.0.0";

fn uv_workspace(root: &Path, at: &str) {
    write(root, &format!("{at}pyproject.toml"), "[project]\nname = \"waybill-fixture-tensile\"\nversion = \"5.0.0\"\n\n[tool.uv.workspace]\nmembers = [\"rocisa\"]\n");
    write(root, &format!("{at}rocisa/pyproject.toml"), "[project]\nname = \"waybill-fixture-rocisa\"\nversion = \"0.1.0\"\n");
    write(
        root,
        &format!("{at}uv.lock"),
        "version = 1\n\n[manifest]\nmembers = [\"waybill-fixture-rocisa\", \"waybill-fixture-tensile\"]\n\n\
         [[package]]\nname = \"waybill-fixture-rocisa\"\nversion = \"0.1.0\"\nsource = { editable = \"rocisa\" }\n\n\
         [[package]]\nname = \"waybill-fixture-tensile\"\nversion = \"5.0.0\"\nsource = { editable = \".\" }\n\
         dependencies = [{ name = \"waybill-fixture-rocisa\" }]\n",
    );
}

fn bun_workspace(root: &Path, at: &str) {
    write(root, &format!("{at}package.json"), r#"{"name":"waybill-fixture-mono","version":"1.0.0","workspaces":["packages/*"]}"#);
    write(root, &format!("{at}packages/a/package.json"), r#"{"name":"waybill-fixture-a","version":"1.0.0"}"#);
    write(
        root,
        &format!("{at}bun.lock"),
        "{\n  \"lockfileVersion\": 1,\n  \"workspaces\": {\n    \"\": { \"name\": \"waybill-fixture-mono\" },\n    \
         \"packages/a\": { \"name\": \"waybill-fixture-a\", \"version\": \"1.0.0\" }\n  },\n  \"packages\": {}\n}\n",
    );
}

fn kts_workspace(root: &Path, at: &str) {
    write(root, &format!("{at}settings.gradle.kts"), "rootProject.name = \"waybill-fixture-kts\"\ninclude(\":app\")\n");
    write(
        root,
        &format!("{at}app/build.gradle.kts"),
        "plugins { kotlin(\"jvm\") }\ndependencies {\n    implementation(\"org.waybill-fixture:lib:1.0.0\")\n}\n",
    );
}

/// The workspace in `proj/`, under a Cargo main module at the scan root.
fn in_subproject(build: fn(&Path, &str)) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path(), "proj/");
    write(dir.path(), "Cargo.toml", "[package]\nname = \"waybill-fixture-top\"\nversion = \"1.0.0\"\n");
    dir
}

fn at_root(build: fn(&Path, &str)) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path(), "");
    dir
}

/// The workspace root is reachable, carries no orphan reason, and reaches
/// every one of `members`; nothing is unexplained.
fn assert_attached(d: &Docs, ws: &str, members: &[&str]) {
    assert!(component(&d.cdx, ws).is_some(), "control: {ws} is emitted");
    assert!(reachable(d, ws), "{ws} is unreachable from {}", subject(d));
    for m in members {
        assert!(depends_on(&d.cdx, ws).iter().any(|t| t == m), "{ws} -> {m} is missing");
    }
    assert_explained(d);
}

// ---------------------------------------------------------------------
// W-1..W-3: a subproject's workspace under --root-name (T005)
// ---------------------------------------------------------------------

#[test]
fn w1_uv_subproject_workspace_root_is_attached() {
    let dir = in_subproject(uv_workspace);
    let d = scan(dir.path(), OP);
    assert_attached(&d, UV_WS, &UV_MEMBERS);
    let root_targets = targets_of(&d, &subject(&d));
    assert!(root_targets.iter().any(|t| t == UV_WS), "the root depends on the workspace root");
    for m in UV_MEMBERS {
        assert!(root_targets.iter().any(|t| t == m), "FR-003: the root's edge to member {m} is kept");
    }
    assert_repeatable(dir.path(), OP);
}

#[test]
fn w2_bun_subproject_workspace_root_is_attached() {
    let dir = in_subproject(bun_workspace);
    let d = scan(dir.path(), OP);
    assert_attached(&d, BUN_WS, &["pkg:npm/waybill-fixture-a@1.0.0"]);
    assert_repeatable(dir.path(), OP);
}

#[test]
fn w3_kotlin_subproject_workspace_root_is_attached() {
    let dir = in_subproject(kts_workspace);
    let d = scan(dir.path(), OP);
    assert_attached(&d, KTS_WS, &[]);
    assert_repeatable(dir.path(), OP);
}

// ---------------------------------------------------------------------
// W-4..W-6: a workspace at the scan root is the subject (T006)
// ---------------------------------------------------------------------

/// C69 is emitted only when the subject displaced a main module
/// (milestone 127 FR-006), so `displaced` says whether to expect it.
fn assert_subject_is_workspace_root(d: &Docs, ws: &str, displaced: bool) {
    assert_eq!(subject(d), ws);
    let heuristic = row_all_formats(d, "C69");
    if displaced {
        assert!(
            heuristic.iter().any(|h| h.contains("workspace-root")),
            "C69 names the workspace-root rung: {heuristic:?}"
        );
    } else {
        assert!(heuristic.is_empty(), "no main module lost, so no C69: {heuristic:?}");
    }
    assert_explained(d);
}

#[test]
fn w4_uv_workspace_at_the_root_is_the_subject() {
    let dir = at_root(uv_workspace);
    let d = scan(dir.path(), &[]);
    assert_subject_is_workspace_root(&d, UV_WS, true);
    for m in UV_MEMBERS {
        assert!(reachable(&d, m), "member {m} is reached through the workspace root");
    }
    assert_repeatable(dir.path(), &[]);
}

#[test]
fn w5_bun_workspace_at_the_root_is_the_subject() {
    let dir = at_root(bun_workspace);
    let d = scan(dir.path(), &[]);
    assert_subject_is_workspace_root(&d, BUN_WS, true);
    assert!(reachable(&d, "pkg:npm/waybill-fixture-a@1.0.0"));
    assert_repeatable(dir.path(), &[]);
}

#[test]
fn w6_kotlin_workspace_at_the_root_is_the_subject() {
    let dir = at_root(kts_workspace);
    let d = scan(dir.path(), &[]);
    assert_subject_is_workspace_root(&d, KTS_WS, false);
    assert_repeatable(dir.path(), &[]);
}

// ---------------------------------------------------------------------
// W-7, W-8 (T007)
// ---------------------------------------------------------------------

/// W-7: two workspace roots at the scan root. Neither is promoted; both are
/// reachable from whatever the ladder picks.
#[test]
fn w7_two_workspace_roots_at_the_root_are_both_anchored() {
    let dir = tempfile::tempdir().unwrap();
    uv_workspace(dir.path(), "");
    bun_workspace(dir.path(), "");
    let d = scan(dir.path(), &[]);
    let s = subject(&d);
    assert!(s != UV_WS && s != BUN_WS, "neither workspace root is promoted: {s}");
    assert!(reachable(&d, UV_WS) && reachable(&d, BUN_WS), "both are reachable from {s}");
    assert_explained(&d);
    assert_repeatable(dir.path(), &[]);
}

/// W-8: a workspace root something already depends on gets no root edge.
/// Kotlin's workspace root at a subdirectory is reached from the Cargo main
/// module through the fallback, which is open without --root-name.
#[test]
fn w8_a_targeted_workspace_root_gets_no_extra_edge() {
    let dir = in_subproject(kts_workspace);
    let d = scan(dir.path(), &[]);
    let s = subject(&d);
    let incoming: Vec<&str> = d.cdx["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["dependsOn"].as_array().into_iter().flatten().any(|t| t == KTS_WS))
        .map(|e| e["ref"].as_str().unwrap())
        .collect();
    assert_eq!(incoming, [s.as_str()], "exactly one edge into the workspace root");
    assert_explained(&d);
    assert_repeatable(dir.path(), &[]);
}

// ---------------------------------------------------------------------
// Link fixtures (T002)
// ---------------------------------------------------------------------

const SDK: &str = "pkg:generic/waybill-fixture-sdk@1.0.0";

#[derive(Clone, Copy)]
enum Lib {
    Present,
    Absent,
    Dvc,
}

/// A vendored prebuilt package at `dir`: a CMake version file under
/// `lnx/lib/cmake/<name>/`, and the library (or its DVC pointer) in
/// `lnx/lib`. Its prefix is `dir/lnx`.
fn sdk_record(root: &Path, dir: &str, name: &str, lib: Lib) {
    write(root, &format!("{dir}/lnx/lib/cmake/{name}/{name}ConfigVersion.cmake"), "set(PACKAGE_VERSION \"1.0.0\")\n");
    let file = format!("{dir}/lnx/lib/lib{name}.a");
    match lib {
        Lib::Present => write(root, &file, "!<arch>\n"),
        Lib::Absent => {}
        Lib::Dvc => write(
            root,
            &format!("{file}.dvc"),
            &format!("outs:\n- md5: 3c217a9769930ece24c1d54c2342989d\n  size: 8\n  path: lib{name}.a\n"),
        ),
    }
}

/// A CMake tree with `body` in the root `CMakeLists.txt`.
fn cmake_tree(sdk_name: &str, lib: Lib, body: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    sdk_record(dir.path(), "shared/sdk", sdk_name, lib);
    write(dir.path(), "CMakeLists.txt", &format!("project(top)\n{body}"));
    dir
}

const LINK_SDK: &str = "target_link_directories(t PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/shared/sdk/lnx/lib)\n\
                        target_link_libraries(t PRIVATE waybill-fixture-sdk)\n";

fn conditional(body: &str) -> String {
    format!("if(FLAG)\n{body}endif()\n")
}

/// The record is reachable, Optional, with C122 `cmake-conditional-link`
/// and C235 `conditions`, agreeing across formats.
fn assert_conditional(d: &Docs, purl: &str, conditions: &str) {
    let c = component(&d.cdx, purl).unwrap_or_else(|| panic!("control: {purl} is emitted"));
    assert!(reachable(d, purl), "{purl} is unreachable");
    assert_eq!(c["scope"], "excluded", "{purl} is optional");
    let c122 = row_all_formats(d, "C122");
    assert!(c122.iter().any(|v| v.contains("cmake-conditional-link")), "C122: {c122:?}");
    // The extractor yields each element, JSON-encoded.
    let c235 = row_all_formats(d, "C235");
    let expected: std::collections::BTreeSet<String> = serde_json::from_str::<Vec<String>>(conditions)
        .unwrap()
        .iter()
        .map(|c| serde_json::to_string(c).unwrap())
        .collect();
    assert_eq!(c235, expected, "C235 holds {conditions}");
    assert_eq!(prop(c, "waybill:cmake-link-conditions"), Some(conditions));
    assert_explained(d);
}

/// The record is reachable and keeps its scope; no C122 value of this
/// feature, no C235.
fn assert_required(d: &Docs, purl: &str) {
    let c = component(&d.cdx, purl).unwrap_or_else(|| panic!("control: {purl} is emitted"));
    assert!(reachable(d, purl), "{purl} is unreachable");
    assert_ne!(c["scope"], "excluded");
    assert_eq!(prop(c, "waybill:cmake-link-conditions"), None);
    assert!(row_all_formats(d, "C235").is_empty());
    assert_explained(d);
}

// ---------------------------------------------------------------------
// L-1..L-4 (T013)
// ---------------------------------------------------------------------

#[test]
fn l1_a_conditional_link_makes_the_record_an_optional_dependency() {
    let dir = cmake_tree("waybill-fixture-sdk", Lib::Present, &conditional(LINK_SDK));
    assert_conditional(&scan(dir.path(), &[]), SDK, r#"["FLAG"]"#);
    assert_repeatable(dir.path(), &[]);
}

#[test]
fn l2_an_unconditional_link_keeps_the_scope() {
    let dir = cmake_tree("waybill-fixture-sdk", Lib::Present, LINK_SDK);
    assert_required(&scan(dir.path(), &[]), SDK);
    assert_repeatable(dir.path(), &[]);
}

#[test]
fn l3_the_library_need_not_be_present() {
    let dir = cmake_tree("waybill-fixture-sdk", Lib::Absent, &conditional(LINK_SDK));
    assert_conditional(&scan(dir.path(), &[]), SDK, r#"["FLAG"]"#);
    assert_repeatable(dir.path(), &[]);
}

/// L-4: the record is named differently from the linked library, so only
/// the DVC pointer in the link directory connects them.
#[test]
fn l4_a_dvc_pointer_matches_by_file() {
    let dir = tempfile::tempdir().unwrap();
    sdk_record(dir.path(), "shared/sdk", "waybill-fixture-sdk", Lib::Absent);
    write(dir.path(), "shared/sdk/lnx/lib/libwaybill-fixture-core.a.dvc", "outs:\n- md5: 00\n  path: libwaybill-fixture-core.a\n");
    write(
        dir.path(),
        "CMakeLists.txt",
        "project(top)\nif(FLAG)\n  target_link_directories(t PRIVATE shared/sdk/lnx/lib)\n  target_link_libraries(t PRIVATE waybill-fixture-core)\nendif()\n",
    );
    assert_conditional(&scan(dir.path(), &[]), SDK, r#"["FLAG"]"#);
    assert_repeatable(dir.path(), &[]);
}

// ---------------------------------------------------------------------
// L-5, L-6, L-11, L-12 (T014)
// ---------------------------------------------------------------------

/// The component and relationship sets of `with` and `without` are equal.
fn assert_same_graph(with: &Docs, without: &Docs) {
    let comps = |d: &Docs| -> Vec<String> {
        let mut v: Vec<String> = cdx_components_with_containers(&d.cdx)
            .iter()
            .map(|(c, _)| serde_json::to_string(&mask_nondeterministic(c)).unwrap())
            .collect();
        v.sort();
        v
    };
    assert_eq!(comps(with), comps(without), "FR-006: no component is changed or invented");
    // The subject is named after the scanned directory, which differs.
    let deps = |d: &Docs| -> String {
        serde_json::to_string(&d.cdx["dependencies"]).unwrap().replace(&subject(d), "SUBJECT")
    };
    assert_eq!(deps(with), deps(without), "FR-006: no edge is invented");
}

#[test]
fn l5_a_directory_outside_every_prefix_links_nothing() {
    let body = conditional(
        "target_link_directories(t PRIVATE elsewhere/lib)\ntarget_link_libraries(t PRIVATE waybill-fixture-sdk)\n",
    );
    let with = cmake_tree("waybill-fixture-sdk", Lib::Present, &body);
    let without = cmake_tree("waybill-fixture-sdk", Lib::Present, "");
    assert_same_graph(&scan(with.path(), &[]), &scan(without.path(), &[]));
    assert_repeatable(with.path(), &[]);
}

#[test]
fn l6_a_name_matching_nothing_links_nothing() {
    let body = conditional(
        "target_link_directories(t PRIVATE shared/sdk/lnx/lib)\ntarget_link_libraries(t PRIVATE waybill-fixture-other)\n",
    );
    let with = cmake_tree("waybill-fixture-sdk", Lib::Present, &body);
    let without = cmake_tree("waybill-fixture-sdk", Lib::Present, "");
    assert_same_graph(&scan(with.path(), &[]), &scan(without.path(), &[]));
    assert_repeatable(with.path(), &[]);
}

#[test]
fn l11_link_directories_apply_to_every_later_target() {
    let body = conditional("link_directories(shared/sdk/lnx/lib)\ntarget_link_libraries(other PRIVATE waybill-fixture-sdk)\n");
    let dir = cmake_tree("waybill-fixture-sdk", Lib::Present, &body);
    assert_conditional(&scan(dir.path(), &[]), SDK, r#"["FLAG"]"#);
    assert_repeatable(dir.path(), &[]);
}

/// L-12: an inner record's prefix lies inside an outer record's. Only the
/// inner one is linked.
#[test]
fn l12_the_innermost_prefix_wins() {
    let dir = tempfile::tempdir().unwrap();
    sdk_record(dir.path(), "shared/outer", "waybill-fixture-sdk", Lib::Present);
    sdk_record(dir.path(), "shared/outer/lnx/inner", "waybill-fixture-sdk", Lib::Present);
    write(
        dir.path(),
        "shared/outer/lnx/inner/lnx/lib/cmake/waybill-fixture-sdk/waybill-fixture-sdkConfigVersion.cmake",
        "set(PACKAGE_VERSION \"2.0.0\")\n",
    );
    write(
        dir.path(),
        "CMakeLists.txt",
        "project(top)\nif(FLAG)\n  link_directories(shared/outer/lnx/inner/lnx/lib)\n  link_libraries(waybill-fixture-sdk)\nendif()\n",
    );
    let d = scan(dir.path(), &[]);
    let inner = "pkg:generic/waybill-fixture-sdk@2.0.0";
    assert_eq!(prop(component(&d.cdx, inner).unwrap(), "waybill:cmake-link-conditions"), Some(r#"["FLAG"]"#));
    assert_eq!(prop(component(&d.cdx, SDK).unwrap(), "waybill:cmake-link-conditions"), None, "the outer record is not linked");
    assert_repeatable(dir.path(), &[]);
}

// ---------------------------------------------------------------------
// L-7, L-10 (T015)
// ---------------------------------------------------------------------

/// L-7: TheRock's shape. The link is unconditional inside a file the reader's
/// patterns never match, included under `if (WIN_SDK)`.
#[test]
fn l7_an_included_file_carries_the_include_site_condition() {
    let dir = tempfile::tempdir().unwrap();
    sdk_record(dir.path(), "shared/sdk", "waybill-fixture-sdk", Lib::Dvc);
    write(dir.path(), "CMakeLists.txt", "project(top)\nadd_subdirectory(proj)\n");
    write(dir.path(), "proj/CMakeLists.txt", "if (WIN_SDK)\n  include(${CMAKE_CURRENT_SOURCE_DIR}/CMakeLists_wsl.txt)\nendif()\n");
    write(
        dir.path(),
        "proj/CMakeLists_wsl.txt",
        "target_link_directories(${T} PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../shared/sdk/lnx/lib)\n\
         target_link_libraries(${T} PRIVATE waybill-fixture-sdk)\n",
    );
    assert_conditional(&scan(dir.path(), &[]), SDK, r#"["WIN_SDK"]"#);
    assert_repeatable(dir.path(), &[]);
}

#[test]
fn l10_module_includes_are_ignored_and_include_cycles_terminate() {
    let dir = cmake_tree("waybill-fixture-sdk", Lib::Present, "include(FetchContent)\ninclude(a.cmake)\n");
    write(dir.path(), "a.cmake", &format!("include(b.cmake)\n{}", conditional(LINK_SDK.replace("${CMAKE_CURRENT_SOURCE_DIR}/", "").as_str())));
    write(dir.path(), "b.cmake", "include(a.cmake)\n");
    assert_conditional(&scan(dir.path(), &[]), SDK, r#"["FLAG"]"#);
    assert_repeatable(dir.path(), &[]);
}

// ---------------------------------------------------------------------
// L-8, L-9 (T016)
// ---------------------------------------------------------------------

/// L-8: the linking file inside a git submodule. The submodule depends on
/// the record, typed optional in SPDX 2.3; the root does not.
#[test]
fn l8_the_linking_projects_submodule_owns_the_edge() {
    use common::git_fixture::GitFixture;
    let fx = GitFixture::new();
    fx.repo("x");
    fx.write("x/CMakeLists.txt", &format!("project(x)\n{}", conditional(&LINK_SDK.replace("shared/sdk", "../../shared/sdk"))));
    fx.git(&fx.path("x"), &["add", "-A"]);
    fx.git(&fx.path("x"), &["commit", "-q", "-m", "files"]);
    fx.repo("app");
    fx.add_submodule("app", "x", "third-party/x");
    fx.write("app/CMakeLists.txt", "project(top)\nadd_subdirectory(third-party/x)\n");
    sdk_record(&fx.path("app"), "shared/sdk", "waybill-fixture-sdk", Lib::Present);
    let docs = common::cmake_fetch::scan(&fx, &fx.path("app"), &[]);
    let deps = docs.cdx["dependencies"].as_array().unwrap();
    let owners: Vec<&str> = deps
        .iter()
        .filter(|e| e["dependsOn"].as_array().into_iter().flatten().any(|t| t == SDK))
        .map(|e| e["ref"].as_str().unwrap())
        .collect();
    assert_eq!(owners.len(), 1, "one edge into the record: {owners:?}");
    // Not `contains("x")`: the root's ref, `waybill-fixture-root`, has an x
    // too, and mutation M8 (owner forced to the root) survived that check.
    let root = docs.cdx["metadata"]["component"]["bom-ref"].as_str().unwrap();
    assert_ne!(owners[0], root, "the root does not own the link");
    let owner = docs.cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["bom-ref"] == owners[0])
        .unwrap_or_else(|| panic!("the owner {} is a component", owners[0]));
    assert_eq!(prop(owner, "waybill:source-mechanism"), Some("git-submodule"), "owned by the submodule");
    let optional = docs.spdx23["relationships"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["relationshipType"] == "OPTIONAL_DEPENDENCY_OF")
        .count();
    assert!(optional >= 1, "SPDX 2.3 types the edge OPTIONAL_DEPENDENCY_OF");
}

#[test]
fn l9_conditions_union_and_one_unconditional_link_makes_it_required() {
    let two = format!("if(A)\n{LINK_SDK}endif()\nif(B)\n{LINK_SDK}endif()\n");
    let dir = cmake_tree("waybill-fixture-sdk", Lib::Present, &two);
    assert_conditional(&scan(dir.path(), &[]), SDK, r#"["A","B"]"#);
    assert_repeatable(dir.path(), &[]);
    let three = format!("{two}{LINK_SDK}");
    let dir = cmake_tree("waybill-fixture-sdk", Lib::Present, &three);
    assert_required(&scan(dir.path(), &[]), SDK);
}
