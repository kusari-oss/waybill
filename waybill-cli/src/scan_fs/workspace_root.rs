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
    mode: crate::scan_fs::ScanMode,
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
    if let Some(root) = installed_environment_root(&normalized, mode) {
        return Some(root);
    }
    let path = Path::new(&normalized);
    match path.parent() {
        Some(parent) if parent.as_os_str().is_empty() => Some(".".to_string()),
        Some(parent) => Some(to_forward_slash_or_dot(parent)),
        None => Some(".".to_string()),
    }
}

/// The workspace of a component found in an *installed* environment: the
/// root of that environment, not the directory of the file it was read from
/// (#1061).
///
/// A package database or an installed-package directory is not a workspace.
/// Taking its parent made every `pkg-1.0.dist-info` its own workspace — a
/// virtualenv with 200 packages added 200 entries to
/// `waybill:workspaces-detected`. The environment is the unit: the venv or
/// prefix a Python package was installed into, the project a `node_modules`
/// belongs to, the system an OS package database describes.
///
/// The root is what sits above the environment's marker directory, so a
/// rootfs fixture nested inside a repository gets its own root rather than
/// the repository's. Under [`ScanMode::Image`](crate::scan_fs::ScanMode::Image)
/// every installed path maps to `.`: the container is the system, and
/// everything installed in it belongs to it.
///
/// `None` when the path is not inside an installed environment; the caller
/// then uses the file's directory, as for any manifest.
fn installed_environment_root(path: &str, mode: crate::scan_fs::ScanMode) -> Option<String> {
    let p = path.trim_start_matches('/');
    // Marker -> the root is everything before it. Ordered so the outermost
    // environment wins: a `node_modules` inside a venv belongs to the venv's
    // project, never to a nested package.
    const MARKERS: &[&str] = &[
        "var/lib/dpkg/",
        "lib/apk/db/",
        "var/lib/rpm/",
        "usr/lib/sysimage/rpm/",
        "var/lib/pacman/local/",
        "usr/lib/opkg/",
        "var/lib/opkg/",
        "Cellar/",
        "node_modules/",
        "vendor/composer/",
        "Pods/",
    ];
    let mut best: Option<usize> = None;
    for m in MARKERS {
        for (i, _) in p.match_indices(m) {
            // Only at a path-segment boundary: `foo-node_modules/` is not one.
            if i == 0 || p.as_bytes()[i - 1] == b'/' {
                best = Some(best.map_or(i, |b| b.min(i)));
                break;
            }
        }
    }
    let python = python_environment_base(p);
    let at = match (best, python) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }?;
    if mode == crate::scan_fs::ScanMode::Image {
        return Some(".".to_string());
    }
    let prefix = p[..at].trim_end_matches('/');
    Some(if prefix.is_empty() { ".".to_string() } else { prefix.to_string() })
}

/// Byte offset of a Python installation's base: the directory holding
/// `lib/pythonX.Y/{site,dist}-packages` (or Windows `Lib/site-packages`),
/// with a system prefix (`usr/`, `usr/local/`, `local/`) folded into the
/// root it sits under. A venv's base is the venv directory itself.
fn python_environment_base(p: &str) -> Option<usize> {
    let packages = ["/site-packages/", "/dist-packages/"]
        .iter()
        .filter_map(|m| p.find(m))
        .min()?;
    let head = &p[..packages];
    // `<base>/lib/python3.12` or `<base>/Lib` (Windows venvs).
    let lib = head
        .rfind("lib/python")
        .or_else(|| head.rfind("lib64/python"))
        .or_else(|| head.strip_suffix("Lib").map(|h| h.len()))?;
    if lib != 0 && p.as_bytes()[lib - 1] != b'/' {
        return None;
    }
    let mut base = &p[..lib];
    for system_prefix in ["usr/local/", "usr/", "local/"] {
        if let Some(rest) = base.strip_suffix(system_prefix) {
            if rest.is_empty() || rest.ends_with('/') {
                base = rest;
                break;
            }
        }
    }
    Some(base.len())
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

    const PATH: crate::scan_fs::ScanMode = crate::scan_fs::ScanMode::Path;
    const IMAGE: crate::scan_fs::ScanMode = crate::scan_fs::ScanMode::Image;

    fn root() -> PathBuf {
        PathBuf::from("/abs/to/scan-root")
    }

    fn ws(path: &str, mode: crate::scan_fs::ScanMode) -> String {
        derive_workspace_root(path, &root(), mode).unwrap()
    }

    /// #1061: an installed environment's packages share its root, rather
    /// than each `dist-info` / `node_modules/<pkg>` / package-db directory
    /// being its own workspace.
    #[test]
    fn installed_packages_take_their_environment_root_on_a_directory_scan() {
        // A venv's packages belong to the venv.
        assert_eq!(ws(".venv/lib/python3.14/site-packages/pip-26.2.1.dist-info/METADATA", PATH), ".venv");
        assert_eq!(ws("svc/.venv/lib/python3.12/site-packages/a-1.0.dist-info/RECORD", PATH), "svc/.venv");
        assert_eq!(ws("venv/Lib/site-packages/a-1.0.dist-info/METADATA", PATH), "venv");
        // node_modules belongs to the project above it — the OUTERMOST one.
        assert_eq!(ws("node_modules/express/package.json", PATH), ".");
        assert_eq!(ws("web/node_modules/a/node_modules/b/package.json", PATH), "web");
        assert_eq!(ws("php/vendor/composer/installed.json", PATH), "php");
        assert_eq!(ws("ios/Pods/Manifest.lock", PATH), "ios");
        // An OS package database describes the system it sits in, which for
        // a rootfs fixture nested in a repo is that rootfs.
        assert_eq!(ws("var/lib/dpkg/status", PATH), ".");
        assert_eq!(ws("tests/rootfs/var/lib/dpkg/status", PATH), "tests/rootfs");
        assert_eq!(ws("lib/apk/db/installed", PATH), ".");
        assert_eq!(ws("var/lib/pacman/local/zlib-1.3-1/desc", PATH), ".");
        // System Python folds its `usr/` prefix into the system root.
        assert_eq!(ws("usr/lib/python3/dist-packages/requests-2.31.0.dist-info/METADATA", PATH), ".");
        assert_eq!(ws("usr/local/lib/python3.11/site-packages/x-1.dist-info/METADATA", PATH), ".");
        // Homebrew's root is its prefix.
        assert_eq!(ws("opt/homebrew/Cellar/jq/1.7/INSTALL_RECEIPT.json", PATH), "opt/homebrew");
    }

    /// #1061: in a container everything installed belongs to the system.
    #[test]
    fn every_installed_package_in_an_image_belongs_to_the_system_root() {
        for p in [
            "var/lib/dpkg/status",
            "usr/lib/python3/dist-packages/requests-2.31.0.dist-info/METADATA",
            "app/.venv/lib/python3.12/site-packages/a-1.0.dist-info/METADATA",
            "usr/src/app/node_modules/express/package.json",
            "opt/homebrew/Cellar/jq/1.7/INSTALL_RECEIPT.json",
        ] {
            assert_eq!(ws(p, IMAGE), ".", "{p}");
        }
        // A manifest in an image is still a manifest: its directory.
        assert_eq!(ws("app/package-lock.json", IMAGE), "app");
    }

    /// Markers only at a segment boundary, and only real installed shapes.
    #[test]
    fn manifests_and_lookalikes_keep_their_own_directory() {
        assert_eq!(ws("src/frontend/package.json", PATH), "src/frontend");
        assert_eq!(ws("my-node_modules/package.json", PATH), "my-node_modules");
        assert_eq!(ws("docs/site-packages-notes/README.md", PATH), "docs/site-packages-notes");
        assert_eq!(ws("pyproject.toml", PATH), ".");
    }

    #[test]
    fn derive_root_level_manifest_returns_dot() {
        assert_eq!(
            derive_workspace_root("Cargo.toml", &root(), PATH),
            Some(".".to_string())
        );
    }

    #[test]
    fn derive_subdir_manifest_returns_dir_path() {
        assert_eq!(
            derive_workspace_root("src/frontend/package.json", &root(), PATH),
            Some("src/frontend".to_string())
        );
    }

    #[test]
    fn derive_pip_uri_main_module_returns_relative() {
        assert_eq!(
            derive_workspace_root("path+file:///abs/to/scan-root/src/lfx", &root(), PATH),
            Some("src/lfx".to_string())
        );
    }

    #[test]
    fn derive_pip_uri_outside_scan_root_returns_none() {
        assert_eq!(
            derive_workspace_root("path+file:///unrelated/path", &root(), PATH),
            None,
        );
    }

    #[test]
    fn derive_empty_string_returns_none() {
        assert_eq!(derive_workspace_root("", &root(), PATH), None);
    }

    #[test]
    fn derive_backslash_windows_normalized() {
        assert_eq!(
            derive_workspace_root("src\\frontend\\package.json", &root(), PATH),
            Some("src/frontend".to_string())
        );
    }
}
