//! Spec 1617 (#1623): a package search's conditions do not mark a shipped
//! library non-runtime.
//!
//! Contract `specs/1617-installed-record-scope/contracts/wire.md`, cases
//! I-1..I-11, I-ctl and I-rep. Every case is checked in the three formats
//! through the parity extractors for C42, C62, C122, C235 and C236.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use common::installed_lib::{components, elf_shared_lib, prop, row_all_formats, scan_with, write, Docs};
use corpus_harness_195::layer2_golden::{mask_nondeterministic, masked_difference};
use serde_json::Value;

const SCANS: usize = 20;
const C62: &str = "waybill:lifecycle-scope-derivation";
const C122: &str = "waybill:optional-derivation";
const C235: &str = "waybill:cmake-link-conditions";
const C236: &str = "waybill:cmake-find-conditions";

// ---------------------------------------------------------------------
// Helpers (T002)
// ---------------------------------------------------------------------

fn scan(root: &Path) -> Docs {
    let d = scan_with("--path", root, &[]);
    for row in ["C42", "C62", "C122", "C235", "C236"] {
        row_all_formats(&d, row);
    }
    d
}

fn by_purl<'a>(d: &'a Docs, purl: &str) -> &'a Value {
    let found: Vec<&Value> = components(&d.cdx).into_iter().filter(|c| c["purl"] == purl).collect();
    assert_eq!(found.len(), 1, "{purl}: expected exactly one; all: {:?}", purls(d));
    found[0]
}

fn purls(d: &Docs) -> Vec<&str> {
    components(&d.cdx).into_iter().filter_map(|c| c["purl"].as_str()).collect()
}

fn scope(c: &Value) -> &str {
    c["scope"].as_str().unwrap_or("required")
}

fn conds(c: &Value, key: &str) -> Option<Vec<String>> {
    prop(c, key).map(|v| serde_json::from_str(v).unwrap())
}

fn put_bytes(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// An installed record that ships its shared library: the CMake version
/// file and `lib/lib<name>.so.<version>` under `prefix`.
fn shipped_record(root: &Path, prefix: &str, name: &str, version: &str) {
    write(root, &format!("{prefix}/lib/cmake/{name}/{name}ConfigVersion.cmake"), &format!("set(PACKAGE_VERSION \"{version}\")\n"));
    put_bytes(root, &format!("{prefix}/lib/lib{name}.so.{version}"), &elf_shared_lib(format!("\0{name}\0").as_bytes()));
}

/// An installed record that ships no library file: the version file only.
fn bare_record(root: &Path, prefix: &str, name: &str, version: &str) {
    write(root, &format!("{prefix}/lib/cmake/{name}/{name}ConfigVersion.cmake"), &format!("set(PACKAGE_VERSION \"{version}\")\n"));
}

fn tree() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn assert_required(c: &Value) {
    assert_eq!(scope(c), "required", "{c}");
    assert_eq!(prop(c, C122), None, "{c}");
}

fn assert_repeatable(root: &Path) {
    let docs: Vec<Docs> = (0..SCANS).map(|_| scan_with("--path", root, &[])).collect();
    for (f, name) in ["CycloneDX", "SPDX 2.3", "SPDX 3"].iter().enumerate() {
        let mut variants: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, d) in docs.iter().enumerate() {
            let v = [&d.cdx, &d.spdx23, &d.spdx3][f];
            variants.entry(serde_json::to_string_pretty(&mask_nondeterministic(v)).unwrap()).or_default().push(i);
        }
        if variants.len() > 1 {
            let parsed: Vec<Value> = variants.keys().map(|s| serde_json::from_str(s).unwrap()).collect();
            panic!(
                "{name}: {} variants across {SCANS} scans; {}",
                variants.len(),
                masked_difference(&parsed[0], &parsed[1]).unwrap_or_default()
            );
        }
    }
}

const LIB: &str = "pkg:generic/waybill_fixture_lib@1.2.3";

/// I-1..I-3: a shipped library searched from a shipped consumer config file.
fn consumer_tree(search: &str) -> tempfile::TempDir {
    let t = tree();
    shipped_record(t.path(), "sdk", "waybill_fixture_lib", "1.2.3");
    write(t.path(), "sdk/share/waybill_fixture_consumer/consumer-config.cmake", search);
    t
}

// ---------------------------------------------------------------------
// I-ctl: byte-identity control (T003, FR-007)
// ---------------------------------------------------------------------

const GOLDEN_DIR: &str = "tests/fixtures/installed_record_scope/ctl-golden";
const REGEN: &str = "WAYBILL_UPDATE_INSTALLED_RECORD_SCOPE_GOLDENS";

fn masked(v: &Value) -> Value {
    let s = serde_json::to_string(&mask_nondeterministic(v)).unwrap();
    let re = regex::Regex::new(r"/[^\x22]*?/waybill-fixture-tree").unwrap();
    serde_json::from_str(&re.replace_all(&s, "/TMP/waybill-fixture-tree")).unwrap()
}

/// A shipped record searched unconditionally: `main`'s output. The golden
/// was generated with `main`'s binary before any source change (T003).
#[test]
fn i_ctl_is_byte_identical_to_main() {
    let parent = tree();
    let root: PathBuf = parent.path().join("waybill-fixture-tree");
    shipped_record(&root, "sdk", "waybill_fixture_lib", "1.2.3");
    write(&root, "sdk/share/waybill_fixture_consumer/consumer-config.cmake", "find_package(waybill_fixture_lib)\n");
    let d = scan(&root);
    assert_required(by_purl(&d, LIB));
    let golden_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_DIR);
    for (name, doc) in [("cdx", &d.cdx), ("spdx23", &d.spdx23), ("spdx3", &d.spdx3)] {
        let got = masked(doc);
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
// User story 1: a library an SDK ships is a runtime component (T005)
// ---------------------------------------------------------------------

/// I-1: searched only under `if(NOT TARGET …)`: required, C236 kept, and the
/// search's file in its evidence.
#[test]
fn i01_shipped_library_searched_under_not_target_is_required() {
    let t = consumer_tree("if(NOT TARGET waybill_fixture_lib)\n  find_package(waybill_fixture_lib)\nendif()\n");
    let d = scan(t.path());
    let c = by_purl(&d, LIB);
    assert_required(c);
    assert_eq!(conds(c, C236), Some(vec!["NOT TARGET waybill_fixture_lib".to_string()]), "{c}");
    let files: Vec<String> = serde_json::from_str(prop(c, "waybill:source-files").unwrap()).unwrap();
    assert!(files.iter().any(|f| f.ends_with("consumer-config.cmake")), "{files:?}");
}

/// I-2: searched only in an `else()` branch.
#[test]
fn i02_shipped_library_searched_in_else_is_required() {
    let t = consumer_tree("if(TARGET waybill_fixture_lib)\n  message(STATUS found)\nelse()\n  find_package(waybill_fixture_lib)\nendif()\n");
    let d = scan(t.path());
    let c = by_purl(&d, LIB);
    assert_required(c);
    assert_eq!(conds(c, C236), Some(vec!["else: TARGET waybill_fixture_lib".to_string()]), "{c}");
}

/// I-3: searched unconditionally: required, no C236 (as on `main`).
#[test]
fn i03_shipped_library_searched_unconditionally_is_required() {
    let t = consumer_tree("find_package(waybill_fixture_lib)\n");
    let d = scan(t.path());
    let c = by_purl(&d, LIB);
    assert_required(c);
    assert_eq!(prop(c, C236), None);
}

/// I-9: a record that ships only a static archive, named by its
/// exported-targets file, searched under a condition: required.
#[test]
fn i09_static_archive_named_by_exported_targets_counts() {
    let t = tree();
    let r = t.path();
    write(r, "sdk/lib/cmake/WaybillFixtureE/WaybillFixtureEConfigVersion.cmake", "set(PACKAGE_VERSION \"3.0\")\n");
    put_bytes(r, "sdk/lib/libwaybill-fixture-e-core.a", b"!<arch>\n");
    write(
        r,
        "sdk/lib/cmake/WaybillFixtureE/WaybillFixtureEExports.cmake",
        "get_filename_component(_IMPORT_PREFIX \"${CMAKE_CURRENT_LIST_FILE}\" PATH)\n\
         get_filename_component(_IMPORT_PREFIX \"${_IMPORT_PREFIX}\" PATH)\n\
         get_filename_component(_IMPORT_PREFIX \"${_IMPORT_PREFIX}\" PATH)\n\
         get_filename_component(_IMPORT_PREFIX \"${_IMPORT_PREFIX}\" PATH)\n\
         include(\"${CMAKE_CURRENT_LIST_DIR}/WaybillFixtureEExports-release.cmake\")\n",
    );
    write(
        r,
        "sdk/lib/cmake/WaybillFixtureE/WaybillFixtureEExports-release.cmake",
        "set_target_properties(wfe::core PROPERTIES\n  IMPORTED_LOCATION_RELEASE \"${_IMPORT_PREFIX}/lib/libwaybill-fixture-e-core.a\"\n  )\n",
    );
    write(r, "sdk/share/waybill_fixture_consumer/consumer-config.cmake", "if(NOT TARGET wfe::core)\n  find_package(WaybillFixtureE)\nendif()\n");
    let d = scan(r);
    let c = by_purl(&d, "pkg:generic/waybillfixturee@3.0");
    let files: Vec<String> = serde_json::from_str(prop(c, "waybill:source-files").unwrap()).unwrap();
    assert!(files.iter().any(|f| f.ends_with("libwaybill-fixture-e-core.a")), "control: the archive is the record's evidence: {files:?}");
    assert_required(c);
    assert_eq!(conds(c, C236), Some(vec!["NOT TARGET wfe::core".to_string()]));
}

/// I-10: a shipped library linked only under a condition and searched under
/// one: Optional through the link half, C122 `cmake-conditional-link`, C235
/// and C236 both recorded (FR-005, FR-006).
#[test]
fn i10_link_half_still_applies_to_a_shipped_record() {
    let t = tree();
    let r = t.path();
    write(r, "shared/sdk/lnx/lib/cmake/waybill-fixture-sdk/waybill-fixture-sdkConfigVersion.cmake", "set(PACKAGE_VERSION \"1.0.0\")\n");
    put_bytes(r, "shared/sdk/lnx/lib/libwaybill-fixture-sdk.so", &elf_shared_lib(b"\0wfsdk\0"));
    write(
        r,
        "CMakeLists.txt",
        "project(top)\noption(FLAG \"\" OFF)\nif(FLAG)\n  find_package(waybill-fixture-sdk)\n  target_link_directories(t PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/shared/sdk/lnx/lib)\n  target_link_libraries(t PRIVATE waybill-fixture-sdk)\nendif()\n",
    );
    let d = scan(r);
    let c = by_purl(&d, "pkg:generic/waybill-fixture-sdk@1.0.0");
    let files: Vec<String> = serde_json::from_str(prop(c, "waybill:source-files").unwrap()).unwrap();
    assert!(files.iter().any(|f| f.ends_with("libwaybill-fixture-sdk.so")), "control: it ships its library: {files:?}");
    assert_eq!(scope(c), "excluded", "{c}");
    assert_eq!(prop(c, C122), Some("cmake-conditional-link"), "{c}");
    assert_eq!(conds(c, C235), Some(vec!["FLAG".to_string()]));
    assert_eq!(conds(c, C236), Some(vec!["FLAG".to_string()]));
}

// ---------------------------------------------------------------------
// User story 2: test-only searches (T010)
// ---------------------------------------------------------------------

/// I-4: a shipped library searched only from a shipped test project.
#[test]
fn i04_shipped_library_searched_only_from_tests_is_required() {
    let t = tree();
    shipped_record(t.path(), "sdk", "waybill_fixture_lib", "1.2.3");
    write(t.path(), "sdk/share/waybill_fixture/tests/a/CMakeLists.txt", "find_package(waybill_fixture_lib)\n");
    let d = scan(t.path());
    assert_required(by_purl(&d, LIB));
}

/// I-5: a record shipping no library, searched only from a tests directory:
/// Test with C62 `source-test-directory`, no C122.
#[test]
fn i05_bare_record_searched_only_from_tests_is_test() {
    let t = tree();
    bare_record(t.path(), "sdk", "waybill_fixture_bare", "2.0.0");
    write(t.path(), "sdk/share/waybill_fixture/tests/a/CMakeLists.txt", "find_package(waybill_fixture_bare)\n");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill_fixture_bare@2.0.0");
    assert_eq!(scope(c), "excluded", "{c}");
    assert_eq!(prop(c, "waybill:lifecycle-scope"), Some("test"), "{c}");
    assert_eq!(prop(c, C62), Some("source-test-directory"), "{c}");
    assert_eq!(prop(c, C122), None, "{c}");
}

/// I-7: a bare record searched once from tests and once under `if(WITH_X)`.
/// Since #1624 the name rule follows spec 1269's order, runtime > Optional >
/// Test: the conditional product search makes the record Optional, with
/// C122 `cmake-conditional`, keeping C236.
#[test]
fn i07_mixed_test_and_conditional_searches_follow_the_name_rule() {
    let t = tree();
    bare_record(t.path(), "sdk", "waybill_fixture_bare", "2.0.0");
    write(t.path(), "sdk/share/waybill_fixture/tests/a/CMakeLists.txt", "find_package(waybill_fixture_bare)\n");
    write(t.path(), "sdk/share/waybill_fixture_consumer/consumer-config.cmake", "option(WITH_X \"\" OFF)\nif(WITH_X)\n  find_package(waybill_fixture_bare)\nendif()\n");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill_fixture_bare@2.0.0");
    assert_eq!(scope(c), "excluded", "{c}");
    assert_eq!(prop(c, C62), None, "{c}");
    assert_eq!(prop(c, C122), Some("cmake-conditional"), "{c}");
    assert_eq!(conds(c, C236), Some(vec!["WITH_X".to_string()]));
}

// ---------------------------------------------------------------------
// User story 3: what the tree does not ship stays optional (T013, T014)
// ---------------------------------------------------------------------

/// I-6: a bare record searched under a condition: Optional, as on `main`.
#[test]
/// Spec 1641: each condition below has a visible default that does not
/// hold, so the search stays conditional, as these cases intend.
fn i06_bare_record_searched_conditionally_stays_optional() {
    let t = tree();
    bare_record(t.path(), "sdk", "waybill_fixture_bare", "2.0.0");
    write(t.path(), "sdk/share/waybill_fixture_consumer/consumer-config.cmake", "option(GPU_TARGETS \"\" ON)\nif(NOT GPU_TARGETS)\n  find_package(waybill_fixture_bare)\nendif()\n");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill_fixture_bare@2.0.0");
    assert_eq!(scope(c), "excluded", "{c}");
    assert_eq!(prop(c, C122), Some("cmake-conditional"), "{c}");
    assert_eq!(conds(c, C236), Some(vec!["NOT GPU_TARGETS".to_string()]));
}

/// I-8: a conditional search for a library the tree does not have.
#[test]
fn i08_unpaired_conditional_search_stays_optional() {
    let t = tree();
    write(t.path(), "CMakeLists.txt", "project(top)\noption(WITH_X \"\" OFF)\nif(WITH_X)\n  find_package(waybill_fixture_absent)\nendif()\n");
    let d = scan(t.path());
    let c = by_purl(&d, "pkg:generic/waybill_fixture_absent");
    assert_eq!(scope(c), "excluded", "{c}");
    assert_eq!(conds(c, C236), Some(vec!["WITH_X".to_string()]));
}

/// I-11: a test package's record (exported targets of a `*_tests`
/// package) is Test through spec 1533 (`exported-targets-test-package`),
/// on `main` and here. Spec 1533 scopes installed records by declaration,
/// not by directory (trace.md, T014).
#[test]
fn i11_test_package_keeps_spec_1533_scope() {
    let t = tree();
    let r = t.path();
    write(r, "sdk/lib/cmake/waybill_fixture_p_tests/waybill_fixture_p_testsConfigVersion.cmake", "set(PACKAGE_VERSION \"1.0.0\")\n");
    put_bytes(r, "sdk/lib/libwaybill_fixture_p_tests.so.1.0.0", &elf_shared_lib(b"\0wfpt\0"));
    write(
        r,
        "sdk/lib/cmake/waybill_fixture_p_tests/waybill_fixture_p_tests-targets.cmake",
        "get_filename_component(_IMPORT_PREFIX \"${CMAKE_CURRENT_LIST_FILE}\" PATH)\n\
         get_filename_component(_IMPORT_PREFIX \"${_IMPORT_PREFIX}\" PATH)\n\
         get_filename_component(_IMPORT_PREFIX \"${_IMPORT_PREFIX}\" PATH)\n\
         get_filename_component(_IMPORT_PREFIX \"${_IMPORT_PREFIX}\" PATH)\n",
    );
    write(
        r,
        "sdk/lib/cmake/waybill_fixture_p_tests/waybill_fixture_p_tests-targets-release.cmake",
        "set_target_properties(wfp::tests PROPERTIES\n  IMPORTED_LOCATION_RELEASE \"${_IMPORT_PREFIX}/lib/libwaybill_fixture_p_tests.so.1.0.0\"\n  )\n",
    );
    let d = scan(r);
    let c = by_purl(&d, "pkg:generic/waybill_fixture_p_tests@1.0.0");
    assert_eq!(prop(c, "waybill:lifecycle-scope"), Some("test"), "{c}");
    assert_eq!(prop(c, C62), Some("exported-targets-test-package"), "{c}");
}

// ---------------------------------------------------------------------
// Polish: I-rep (T015)
// ---------------------------------------------------------------------

#[test]
fn i_rep_repeatable() {
    let t = tree();
    let r = t.path();
    shipped_record(r, "sdk", "waybill_fixture_lib", "1.2.3");
    bare_record(r, "sdk", "waybill_fixture_bare", "2.0.0");
    bare_record(r, "sdk", "waybill_fixture_opt", "3.0.0");
    write(r, "sdk/share/waybill_fixture_consumer/consumer-config.cmake", "if(NOT TARGET waybill_fixture_lib)\n  find_package(waybill_fixture_lib)\nendif()\nif(WITH_X)\n  find_package(waybill_fixture_opt)\nendif()\n");
    write(r, "sdk/share/waybill_fixture/tests/a/CMakeLists.txt", "find_package(waybill_fixture_bare)\n");
    assert_repeatable(r);
}
