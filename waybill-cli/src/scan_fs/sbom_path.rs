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

/// Strip the scan-rootfs prefix from a single path AND strip any leading `/`,
/// so `ResolutionEvidence.source_file_paths` carries rootfs-relative values
/// from the moment they are recorded by each reader. All three SBOM formats
/// (CDX `mikebom:source-files` property, SPDX 2.3/3 `mikebom:source-files`
/// annotation, CDX `evidence.occurrences[].location`) read the same field, so
/// normalizing once at source guarantees cross-format identity (the
/// `holistic_parity` test verifies this — C18 is `SymmetricEqual`).
///
/// Per milestone 133 US2.1 (FR-012):
/// - **Defect A**: when `rootfs_root` is `Some`, a path starting with the
///   rootfs root prefix has the prefix stripped (avoids leaking the scanner
///   host's tempdir, e.g. `/private/var/folders/.../mikebom-image-XXXXXX/rootfs/
///   usr/bin/curl` becomes `usr/bin/curl`).
/// - **Defect C**: leading `/` is stripped so the no-leading-`/` convention
///   matches FR-007 and FR-014.
///
/// Defect B (CDX-only comma-string vs JSON-array) is fixed at the CDX
/// emission site by [`source_files_as_json_array`] which serializes the
/// already-normalized `source_file_paths` as a JSON array.
pub fn normalize_sbom_path_relative(s: &str, rootfs_root: Option<&std::path::Path>) -> String {
    // Step 1: Windows separator normalization.
    let mut out = if cfg!(windows) {
        s.replace('\\', "/")
    } else {
        s.to_string()
    };
    // Step 2: strip rootfs prefix when set.
    if let Some(root) = rootfs_root {
        let mut prefix = root.to_string_lossy().into_owned();
        if cfg!(windows) {
            prefix = prefix.replace('\\', "/");
        }
        if !prefix.ends_with('/') {
            prefix.push('/');
        }
        if let Some(rest) = out.strip_prefix(prefix.as_str()) {
            out = rest.to_string();
        }
    }
    // Step 3: strip leading `/` (covers non-image scans + paths that didn't
    // match the prefix).
    while out.starts_with('/') {
        out.remove(0);
    }
    out
}

/// Serialize a `source_file_paths` Vec as a JSON-encoded array string for
/// CycloneDX `mikebom:source-files` property emission. Fixes milestone-133
/// FR-012 Defect B (pre-133 emission was a comma-separated string).
///
/// Assumes the input is already path-normalized via
/// [`normalize_sbom_path_relative`] at source-population time.
///
/// Returns `None` when the input is empty so callers can suppress the
/// property entirely.
pub fn source_files_as_json_array(paths: &[String]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    Some(serde_json::to_string(paths).expect("Vec<String> serializes"))
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

    #[test]
    fn normalize_path_relative_no_rootfs_strips_leading_slash() {
        // FR-012 Defect C: even without a rootfs prefix, leading `/` is
        // stripped.
        assert_eq!(normalize_sbom_path_relative("/usr/bin/curl", None), "usr/bin/curl");
    }

    #[test]
    fn normalize_path_relative_strips_rootfs_tempdir_prefix() {
        // FR-012 Defect A: the macOS tempdir + mikebom-image-XXX/rootfs/
        // prefix the resolver records during image scans is stripped so
        // emitted paths are rootfs-relative.
        let rootfs = PathBuf::from("/private/var/folders/dz/abc/T/mikebom-image-XYZ/rootfs");
        assert_eq!(
            normalize_sbom_path_relative(
                "/private/var/folders/dz/abc/T/mikebom-image-XYZ/rootfs/usr/bin/curl",
                Some(&rootfs),
            ),
            "usr/bin/curl",
        );
    }

    #[test]
    fn normalize_path_relative_no_match_falls_back_to_leading_slash_strip() {
        // When a path doesn't share the rootfs prefix (e.g. a stray
        // absolute path from a different scan context), the prefix-strip
        // is a no-op and only the leading-`/` strip applies.
        let rootfs = PathBuf::from("/private/var/folders/dz/abc/T/mikebom-image-XYZ/rootfs");
        assert_eq!(normalize_sbom_path_relative("/etc/passwd", Some(&rootfs)), "etc/passwd");
    }

    #[test]
    fn normalize_path_relative_no_false_match_with_similar_prefix() {
        // Trailing `/` added internally makes prefix-matching unambiguous:
        // `/tmp/rootfs` matches `/tmp/rootfs/foo` but NOT `/tmp/rootfsX/foo`.
        let rootfs = PathBuf::from("/tmp/rootfs");
        assert_eq!(normalize_sbom_path_relative("/tmp/rootfs/foo", Some(&rootfs)), "foo");
        assert_eq!(
            normalize_sbom_path_relative("/tmp/rootfsX/bar", Some(&rootfs)),
            "tmp/rootfsX/bar",
        );
    }

    #[test]
    fn source_files_as_json_array_empty_returns_none() {
        // FR-012 Defect B: empty path list → caller suppresses the
        // property entirely (no `"value": "[]"` noise).
        assert_eq!(source_files_as_json_array(&[]), None);
    }

    #[test]
    fn source_files_as_json_array_single() {
        let paths = vec!["usr/bin/curl".to_string()];
        assert_eq!(source_files_as_json_array(&paths).unwrap(), r#"["usr/bin/curl"]"#);
    }

    #[test]
    fn source_files_as_json_array_multi() {
        let paths = vec!["usr/bin/curl".to_string(), "usr/lib/libc.so.6".to_string()];
        assert_eq!(
            source_files_as_json_array(&paths).unwrap(),
            r#"["usr/bin/curl","usr/lib/libc.so.6"]"#,
        );
    }
}
