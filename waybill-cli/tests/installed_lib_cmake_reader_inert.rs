//! Spec 1531, research R1 (task T005): a CMake package version file also
//! matches the CMake reader's `*.cmake` glob. The installed-library reader
//! assumes the CMake reader emits nothing from it; this pins that.

#![allow(clippy::unwrap_used)]

mod common;

use common::installed_lib::{components, prop, scan, write};

const FIXTURE: &str =
    include_str!("../src/scan_fs/package_db/installed_lib/testdata/BasicConfigVersion-SameMajor.cmake.txt");

#[test]
fn the_cmake_reader_emits_nothing_from_a_package_version_file() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    write(&root, "lib/cmake/WaybillFixtureB/WaybillFixtureBConfigVersion.cmake", FIXTURE);
    // `cmake/` is a directory the CMake reader does parse, so the version
    // file there is the case that could go wrong.
    write(&root, "cmake/WaybillFixtureBConfigVersion.cmake", FIXTURE);
    // Control: the CMake reader is live in `cmake/` and emits a fetch.
    write(
        &root,
        "cmake/fetch.cmake",
        "include(FetchContent)\nFetchContent_Declare(waybill-fixture-fetched\n  URL https://example.invalid/waybill-fixture-fetched-1.0.0.tar.gz)\n",
    );
    let docs = scan(&root);
    let fetched = components(&docs.cdx)
        .into_iter()
        .any(|c| c["purl"].as_str().is_some_and(|p| p.contains("waybill-fixture-fetched")));
    assert!(fetched, "control: the CMake reader must read cmake/fetch.cmake");
    let from_cmake: Vec<String> = components(&docs.cdx)
        .into_iter()
        .filter(|c| prop(c, "waybill:source-mechanism").is_some_and(|m| m.starts_with("cmake-")))
        .filter(|c| !c["purl"].as_str().is_some_and(|p| p.contains("waybill-fixture-fetched")))
        .filter_map(|c| c["purl"].as_str().map(str::to_string))
        .collect();
    assert!(from_cmake.is_empty(), "the CMake reader emitted {from_cmake:?}");
}
