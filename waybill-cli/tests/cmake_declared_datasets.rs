//! Spec 1535 (#1565): datasets declared by CMake `file(DOWNLOAD)` lists
//! become data components. Synthetic CMake trees; nothing is fetched.

#![allow(clippy::unwrap_used)]

mod common;

use common::installed_lib::{components, doc_field_all_formats, prop, row_all_formats, scan, write, Docs};
use serde_json::Value;

/// rocSPARSE's measured shape (research R1) with synthetic names: an index
/// loop over a matrix list and a paired MD5 list, a `$ENV` mirror, a primary
/// and a fallback URL, and an `file(MD5)` check.
const MATRICES: &str = r#"
set(TEST_MATRICES
  WaybillFixtureA/waybill-fixture-m1
  WaybillFixtureB/waybill-fixture-m2
  waybill-fixture-m3
)
set(TEST_MD5HASH
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
  cccccccccccccccccccccccccccccccc
)
list(LENGTH TEST_MATRICES len)
math(EXPR len1 "${len} - 1")
foreach(i RANGE 0 ${len1})
  list(GET TEST_MATRICES ${i} m)
  list(GET TEST_MD5HASH ${i} md5)
  if(DEFINED ENV{WAYBILL_FIXTURE_MIRROR})
    file(DOWNLOAD $ENV{WAYBILL_FIXTURE_MIRROR}/${m}.tar.gz ${CMAKE_MATRICES_DIR}/m.tar.gz)
  else()
    file(DOWNLOAD https://example.invalid/MM/${m}.tar.gz ${CMAKE_MATRICES_DIR}/m.tar.gz STATUS DL)
    file(DOWNLOAD https://mirror.example.invalid/MM/${m}.tar.gz ${CMAKE_MATRICES_DIR}/m.tar.gz)
  endif()
  file(MD5 ${CMAKE_MATRICES_DIR}/m.tar.gz hash)
endforeach()
"#;

fn tree(root: &std::path::Path, second_file: bool) {
    write(root, "CMakeLists.txt", "cmake_minimum_required(VERSION 3.20)\nproject(waybill-fixture-app)\ninclude(cmake/ClientMatrices.cmake)\n");
    write(root, "cmake/ClientMatrices.cmake", MATRICES);
    // A tool installer: a literal URL, resolved, not a dataset (US1 scenario 1).
    write(root, "cmake/tools.cmake", "file(DOWNLOAD https://example.invalid/waybill-fixture-tool-installer.sh ${CMAKE_BINARY_DIR}/i.sh)\n");
    if second_file {
        // The same declaration in a second file (rocSPARSE has two).
        write(root, "cmake/matrices_copy.cmake", MATRICES);
    }
}

fn datasets(docs: &Docs) -> Vec<&Value> {
    components(&docs.cdx).into_iter().filter(|c| prop(c, "waybill:component-kind") == Some("dataset")).collect()
}

/// US1 scenarios 1-5 and C229.
#[test]
fn us1_declared_download_lists_become_datasets() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("src");
    tree(&root, true);
    let docs = scan(&root);

    let ds = datasets(&docs);
    assert_eq!(ds.len(), 3, "one per element: the fallback and the second file join it");
    let m1 = ds.iter().find(|c| c["name"] == "waybill-fixture-m1").unwrap();
    assert_eq!(
        m1["purl"],
        "pkg:generic/WaybillFixtureA/waybill-fixture-m1?checksum=md5:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa&download_url=https://example.invalid/MM/WaybillFixtureA/waybill-fixture-m1.tar.gz"
    );
    assert!(m1.get("version").is_none() || m1["version"] == "");
    assert_eq!(m1["hashes"][0]["alg"], "MD5");
    // Both declaring files are evidence.
    let files = prop(m1, "waybill:source-files").unwrap();
    assert!(files.contains("ClientMatrices.cmake") && files.contains("matrices_copy.cmake"), "{files}");
    // An element with no namespace.
    assert!(ds.iter().any(|c| c["purl"].as_str().unwrap().starts_with("pkg:generic/waybill-fixture-m3?")));
    // The tool installer emits nothing.
    assert!(!components(&docs.cdx).iter().any(|c| c["name"].as_str().unwrap().contains("tool-installer")));

    // Per file: 1 mirror (unresolved) + primary + fallback, twice; plus the tool.
    assert_eq!(
        doc_field_all_formats(&docs, "waybill:declared-downloads").expect("C229"),
        serde_json::json!({"datasets": 3, "digest-mismatches": 0, "not-classified": 1, "read": 7, "unresolved": 2})
    );
}

/// Lists of unequal length pair no digest, emit nothing, and are counted.
#[test]
fn unequal_lists_are_counted_not_emitted() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("src");
    write(&root, "CMakeLists.txt", "project(waybill-fixture-app)\n");
    write(&root, "cmake/m.cmake", &MATRICES.replace("  cccccccccccccccccccccccccccccccc\n", ""));
    let docs = scan(&root);
    assert!(datasets(&docs).is_empty());
    let c229 = doc_field_all_formats(&docs, "waybill:declared-downloads").unwrap();
    assert_eq!(c229["digest-mismatches"], 2);
    assert_eq!(c229["not-classified"], 2);
}

/// FR-011-style control: a CMake tree with no `file(DOWNLOAD)` has no C229.
#[test]
fn no_downloads_no_record() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("src");
    write(&root, "CMakeLists.txt", "project(waybill-fixture-app)\nfind_package(Threads)\n");
    let docs = scan(&root);
    assert_eq!(doc_field_all_formats(&docs, "waybill:declared-downloads"), None);
    assert!(datasets(&docs).is_empty());
}

/// US2: native kinds in each format, and C160 agreeing.
#[test]
fn us2_datasets_are_native_data_kinds() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("src");
    tree(&root, false);
    let docs = scan(&root);

    for c in datasets(&docs) {
        assert_eq!(c["type"], "data", "{}", c["name"]);
    }
    let spdx = &docs.spdx23;
    for p in spdx["packages"].as_array().unwrap().iter().filter(|p| p["name"].as_str().unwrap().starts_with("waybill-fixture-m")) {
        assert_eq!(p["primaryPackagePurpose"], "OTHER", "{}", p["name"]);
    }
    let g = docs.spdx3["@graph"].as_array().unwrap();
    let ds3: Vec<&Value> = g.iter().filter(|e| e["name"].as_str().is_some_and(|n| n.starts_with("waybill-fixture-m"))).collect();
    assert_eq!(ds3.len(), 3);
    for e in ds3 {
        assert_eq!(e["type"], "dataset_DatasetPackage", "{e}");
        assert_eq!(e["software_primaryPurpose"], "data");
        assert_eq!(e["dataset_datasetType"], serde_json::json!(["noAssertion"]));
    }
    assert!(row_all_formats(&docs, "C160").iter().any(|v| v.contains("dataset")));

    // SC-002: the CycloneDX schema and the SPDX 3 conformance validator.
    let errors = common::cdx_schema::cdx_validation_errors(&docs.cdx);
    assert!(errors.is_empty(), "CycloneDX schema: {errors:?}");
    let validator = common::workspace_root().join(".venv/spdx3-validate/bin/spdx3-validate");
    if !validator.exists() {
        assert!(
            std::env::var("WAYBILL_REQUIRE_SPDX3_VALIDATOR").ok().as_deref() != Some("1"),
            "spdx3-validate not found at {} and WAYBILL_REQUIRE_SPDX3_VALIDATOR=1",
            validator.display()
        );
        eprintln!("[cmake_declared_datasets] spdx3-validate not installed; skipping (CI requires it)");
        return;
    }
    let out = tempfile::tempdir().unwrap();
    let doc = out.path().join("d.spdx3.json");
    std::fs::write(&doc, serde_json::to_vec(&docs.spdx3).unwrap()).unwrap();
    let o = common::spdx3_validate::command(&validator).arg("--quiet").arg("-j").arg(&doc).output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert!(o.status.success() && !text.contains("Violation of type"), "spdx3-validate:\n{text}");
}
