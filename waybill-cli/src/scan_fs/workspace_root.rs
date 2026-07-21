//! Derive workspace-root path from a per-component source path
//! (milestone 176).
//!
//! Every package-DB reader populates `PackageDbEntry.source_path` — the
//! manifest / lockfile / DB file that produced the entry — which threads
//! into `ResolutionEvidence.source_file_paths`. The workspace root for a
//! component is simply the *parent directory* of its source path.
//!
//! Two source-path shapes exist in the codebase today:
//!
//! 1. **Root-relative filesystem path** — the common shape produced by
//!    every lockfile / manifest reader. Examples: `official/requirements.txt`,
//!    `src/frontend/package.json`, `Cargo.toml`. For a root-level
//!    manifest (no parent directory), the workspace root is the sentinel
//!    `"."` — matches the m068 pip precedent.
//!
//! 2. **`path+file://<absolute>` URI** — used by pip main-modules where
//!    a project root becomes `path+file:///abs/to/project`. The absolute
//!    path must first have the URI prefix stripped, then the
//!    `scan_root_abs` prefix stripped, to yield a root-relative
//!    representation. If the absolute path is NOT under `scan_root_abs`
//!    (malformed evidence — should not happen but the derivation is
//!    defensive), the caller receives `None` and omits the annotation
//!    per FR-002.
//!
//! Forward-slash normalization is applied on all platforms per FR-010.

use std::path::Path;

/// Derive a workspace root path from a `source_file_paths` entry.
///
/// Returns `None` when the input is malformed (empty string) or
/// unattributable (a `path+file://` URI whose absolute path is not
/// under `scan_root_abs`). The caller then omits the
/// `waybill:workspace-member` annotation per FR-002.
///
/// See module docs for the two source-path shapes handled.
pub(crate) fn derive_workspace_root(
    source_file_path: &str,
    scan_root_abs: &Path,
) -> Option<String> {
    if source_file_path.is_empty() {
        return None;
    }

    if let Some(abs_str) = source_file_path.strip_prefix("path+file://") {
        let abs_path = Path::new(abs_str);
        let rel = abs_path.strip_prefix(scan_root_abs).ok()?;
        return Some(to_forward_slash_or_dot(rel));
    }

    let normalized = source_file_path.replace('\\', "/");
    let path = Path::new(&normalized);
    match path.parent() {
        Some(parent) if parent.as_os_str().is_empty() => Some(".".to_string()),
        Some(parent) => Some(to_forward_slash_or_dot(parent)),
        None => Some(".".to_string()),
    }
}

fn to_forward_slash_or_dot(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if s.is_empty() {
        ".".to_string()
    } else {
        s
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        PathBuf::from("/abs/to/scan-root")
    }

    #[test]
    fn derive_root_level_manifest_returns_dot() {
        assert_eq!(
            derive_workspace_root("Cargo.toml", &root()),
            Some(".".to_string())
        );
    }

    #[test]
    fn derive_subdir_manifest_returns_dir_path() {
        assert_eq!(
            derive_workspace_root("src/frontend/package.json", &root()),
            Some("src/frontend".to_string())
        );
    }

    #[test]
    fn derive_pip_uri_main_module_returns_relative() {
        assert_eq!(
            derive_workspace_root("path+file:///abs/to/scan-root/src/lfx", &root()),
            Some("src/lfx".to_string())
        );
    }

    #[test]
    fn derive_pip_uri_outside_scan_root_returns_none() {
        assert_eq!(
            derive_workspace_root("path+file:///unrelated/path", &root()),
            None,
        );
    }

    #[test]
    fn derive_empty_string_returns_none() {
        assert_eq!(derive_workspace_root("", &root()), None);
    }

    #[test]
    fn derive_backslash_windows_normalized() {
        assert_eq!(
            derive_workspace_root("src\\frontend\\package.json", &root()),
            Some("src/frontend".to_string())
        );
    }
}
