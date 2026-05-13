//! Path normalization for SBOM JSON emission per milestone-100
//! Clarifications + research §2.
//!
//! On Windows, replaces backslash separators with forward-slash; on
//! Unix, returns the native string unchanged. SBOM JSON is a
//! cross-platform artifact; forward-slash everywhere matches the de
//! facto industry convention (syft + trivy) and the CDX 1.6 / SPDX 2.3
//! / SPDX 3 schema example conventions.
//!
//! Only the *separator character* is normalized. Drive-letter prefixes
//! (`C:`) are preserved verbatim — a Windows path like
//! `C:\Users\dev\Cargo.toml` becomes `C:/Users/dev/Cargo.toml`.
//!
//! Internal path operations (file opens, canonical-path lookups,
//! walker traversal) continue to use native OS paths — only strings
//! emitted into SBOM JSON output go through this helper.

use std::path::Path;

/// Normalize a filesystem path for SBOM JSON emission.
///
/// On Windows: `to_string_lossy()` then replace `\` with `/`.
/// On Unix: `to_string_lossy()` only (no separator change needed).
#[allow(dead_code)]
pub fn normalize_sbom_path(path: &Path) -> String {
    let raw = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        raw.replace('\\', "/")
    } else {
        raw
    }
}

/// Convenience variant for `&str` callers where the path has already
/// been converted to a `String` (e.g., `PackageDbEntry.source_path`).
pub fn normalize_sbom_path_str(s: &str) -> String {
    if cfg!(windows) {
        s.replace('\\', "/")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn unix_path_unchanged() {
        // Forward-slash already; should pass through on every host
        // (Unix branch: no-op `to_string()`; Windows branch: replace
        // is a no-op when no `\` present).
        let p = PathBuf::from("/home/dev/project/Cargo.toml");
        assert_eq!(normalize_sbom_path(&p), "/home/dev/project/Cargo.toml");
    }

    #[test]
    fn windows_backslash_normalized_on_windows() {
        // This test only meaningfully exercises on Windows hosts. On
        // Unix the path stays backslash-separated (Unix doesn't treat
        // `\` as a path separator at the Path level, so the raw
        // string contains literal backslashes). On Windows the
        // normalization fires.
        let p = PathBuf::from(r"C:\Users\dev\project\Cargo.toml");
        let out = normalize_sbom_path(&p);
        #[cfg(windows)]
        assert_eq!(out, "C:/Users/dev/project/Cargo.toml");
        #[cfg(not(windows))]
        assert_eq!(out, r"C:\Users\dev\project\Cargo.toml");
    }

    #[test]
    fn str_variant_normalizes_str_input() {
        // The `_str` variant operates on `&str` directly. Same cfg
        // branching as the `Path` variant.
        #[cfg(windows)]
        assert_eq!(normalize_sbom_path_str(r"C:\a\b"), "C:/a/b");
        #[cfg(not(windows))]
        assert_eq!(normalize_sbom_path_str("/a/b"), "/a/b");
    }

    #[test]
    fn drive_letter_colon_preserved() {
        // The colon after the drive letter is NOT a separator — it
        // stays. Only `\` becomes `/`.
        #[cfg(windows)]
        assert_eq!(normalize_sbom_path_str(r"D:\projects\foo"), "D:/projects/foo");
        // On Unix the equivalent doesn't apply.
    }
}
