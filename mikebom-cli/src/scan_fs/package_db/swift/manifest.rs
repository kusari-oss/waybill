//! Swift Package Manager manifest detection.
//!
//! v0.1 detects `Package.swift` PRESENCE only — the file content is NEVER
//! parsed because `Package.swift` is executable Swift code, and a safe
//! parser requires either a Swift parser dependency (Constitution Principle
//! I violation) or shelling out to the host's `swift` toolchain (Strict
//! Boundary 3 + scan-time dependency violation). Per FR-002 + clarification
//! Q3, dependency discovery flows exclusively through the sibling
//! `Package.resolved` lockfile.

use std::path::Path;

/// Returns `true` iff a regular file exists at `path`. Used by the reader
/// to detect `Package.swift` so we can emit a `tracing::warn!` when the
/// manifest is present but its sibling `Package.resolved` is missing
/// (FR-002 fail-closed signal: "run `swift package resolve` first").
pub(super) fn detect(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn detects_present_package_swift() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Package.swift");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"// swift-tools-version:5.9\n").unwrap();
        assert!(detect(&path));
    }

    #[test]
    fn returns_false_when_absent() {
        let dir = tempdir().unwrap();
        assert!(!detect(&dir.path().join("Package.swift")));
    }

    #[test]
    fn returns_false_when_path_is_directory() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("Package.swift");
        std::fs::create_dir_all(&nested).unwrap();
        // A directory named `Package.swift` (pathological case) is NOT a file.
        assert!(!detect(&nested));
    }
}
