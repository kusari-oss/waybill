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
        // `cmake-package-version` is the installed-library reader's own
        // mechanism, not the CMake reader's.
        .filter(|c| {
            prop(c, "waybill:source-mechanism")
                .is_some_and(|m| m.starts_with("cmake-") && m != "cmake-package-version")
        })
        .filter(|c| !c["purl"].as_str().is_some_and(|p| p.contains("waybill-fixture-fetched")))
        .filter_map(|c| c["purl"].as_str().map(str::to_string))
        .collect();
    assert!(from_cmake.is_empty(), "the CMake reader emitted {from_cmake:?}");
}

/// Spec 1532 research R2 (T001): an exported-targets file also matches the
/// CMake reader's `*.cmake` glob. It must emit nothing from one, in
/// `cmake/`, where it does parse.
#[test]
fn the_cmake_reader_emits_nothing_from_an_exported_targets_file() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    write(
        &root,
        "cmake/WaybillFixturePTargets.cmake",
        "get_filename_component(_IMPORT_PREFIX \"${CMAKE_CURRENT_LIST_FILE}\" PATH)\n\
         add_library(wfp::ops STATIC IMPORTED)\n",
    );
    write(
        &root,
        "cmake/waybill-fixture-q-targets-release.cmake",
        "set_target_properties(wfq::q PROPERTIES\n  IMPORTED_LOCATION_RELEASE \"${_IMPORT_PREFIX}/lib/libwaybill-fixture-q.so\")\n",
    );
    write(
        &root,
        "cmake/fetch.cmake",
        "include(FetchContent)\nFetchContent_Declare(waybill-fixture-fetched\n  URL https://example.invalid/waybill-fixture-fetched-1.0.0.tar.gz)\n",
    );
    let docs = scan(&root);
    let cs = components(&docs.cdx);
    assert!(
        cs.iter().any(|c| c["purl"].as_str().is_some_and(|p| p.contains("waybill-fixture-fetched"))),
        "control: the CMake reader is live in cmake/"
    );
    let other: Vec<String> = cs
        .iter()
        .filter(|c| prop(c, "waybill:source-mechanism").is_some_and(|m| m.starts_with("cmake-") && m != "cmake-package-version"))
        .filter_map(|c| c["purl"].as_str().map(str::to_string))
        .filter(|p| !p.contains("waybill-fixture-fetched"))
        .collect();
    assert!(other.is_empty(), "the CMake reader emitted {other:?} from targets files");
}
