//! CMake source-tree reader (milestone 102 US2) — STUB.
//!
//! Real implementation lands in PR-B per the milestone-102 incremental-
//! delivery plan. The `include_vendored` parameter is reserved here so
//! the dispatch wiring in `read_all` can pass through the
//! `--include-vendored` CLI flag without a signature break when PR-B
//! lands.
//!
//! Per spec FR-005, FR-006, FR-011, FR-016. Cross-platform.

use std::path::Path;

use super::PackageDbEntry;

/// Walk `scan_root` for `CMakeLists.txt` + `cmake/*.cmake` +
/// `Modules/*.cmake` + `third_party/*.cmake` and emit one
/// `PackageDbEntry` per `FetchContent_Declare` / `ExternalProject_Add`
/// declaration. When `include_vendored` is true, also emits components
/// for `add_subdirectory(third_party/...)` / `add_subdirectory(vendor/...)`
/// with the version backfilled from a co-located `version.txt` per
/// FR-016. `find_package` is NOT parsed per FR-011 (would double-count
/// against OS-package readers + vcpkg + Conan).
///
/// STUB — returns empty until PR-B lands the regex parsers per
/// research §4.
pub fn read(_scan_root: &Path, _include_vendored: bool) -> Vec<PackageDbEntry> {
    Vec::new()
}
