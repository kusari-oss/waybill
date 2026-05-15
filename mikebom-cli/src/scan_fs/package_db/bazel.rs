//! Bazel source-tree reader (milestone 102 US1) — STUB.
//!
//! Real implementation lands in PR-B per the milestone-102 incremental-
//! delivery plan (PR-A ships US3 vcpkg + Conan + foundational
//! architecture; PR-B ships US1 Bazel + US2 CMake parsers).
//!
//! Per spec FR-001..FR-004. Cross-platform (no `#[cfg(unix)]` per FR-013).

use std::path::Path;

use super::PackageDbEntry;

/// Walk `scan_root` for `MODULE.bazel` + `WORKSPACE.bazel` + `WORKSPACE`
/// and emit one `PackageDbEntry` per declared dependency.
///
/// STUB — returns empty until PR-B lands the regex parsers per
/// research §2 + §3.
pub fn read(_scan_root: &Path) -> Vec<PackageDbEntry> {
    Vec::new()
}
