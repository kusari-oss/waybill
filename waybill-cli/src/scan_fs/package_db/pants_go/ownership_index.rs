//! Milestone 226: build `GoOwnershipIndex` from parsed target declarations.
//!
//! Given a stream of `(build_file_path, GoTargetDeclaration)` pairs
//! and the scan root, compute the four routed lookup buckets per
//! data-model.md §"GoOwnershipIndex":
//!
//! - `go_mod` targets → `go_mod_roots` (BUILD file's dir → address)
//! - `go_third_party_package` → `import_path_to_addresses`
//! - `go_binary` → `main_targets` with `main=` resolved to abs path
//! - `go_package` → `package_targets` (BUILD file's dir as pkg dir)
//!
//! Target address computation follows m225's convention:
//! `<subdir>:<name>` for subdirectory BUILDs, bare `<name>` for
//! root BUILDs. When `name=` is omitted, Pants defaults apply
//! (`"mod"` for go_mod, dir basename for go_package).

use std::path::{Path, PathBuf};

use super::{GoOwnershipIndex, GoTargetDeclaration, GoTargetKind, TargetAddress};

/// Compute a target address for a BUILD file at `build_file_dir`
/// relative to `scan_root`, given optional `name=` + fallback name.
fn compute_address(
    build_file_dir: &Path,
    scan_root: &Path,
    name: Option<&str>,
    fallback_name: &str,
) -> TargetAddress {
    let rel = build_file_dir
        .strip_prefix(scan_root)
        .unwrap_or(build_file_dir);
    let dir_str = rel.to_string_lossy().replace('\\', "/");
    let effective_name = name.filter(|s| !s.is_empty()).unwrap_or(fallback_name);
    if dir_str.is_empty() || dir_str == "." {
        TargetAddress(effective_name.to_string())
    } else {
        TargetAddress(format!("{dir_str}:{effective_name}"))
    }
}

/// Normalize a `go_binary(main=...)` path to an absolute directory.
/// - `"."` → BUILD file's own directory
/// - `"./cmd/foo"` or `"cmd/foo"` → `<build_dir>/cmd/foo`
/// - Absolute paths (starting with `/`) → `None` (WARN + skip).
fn normalize_main_path(main: &str, build_file_dir: &Path) -> Option<PathBuf> {
    let trimmed = main.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('/') {
        tracing::warn!(
            main = %trimmed,
            "pants-go reader: go_binary(main=...) with absolute path is not a legal Pants shape; skipping"
        );
        return None;
    }
    let stripped = trimmed.strip_prefix("./").unwrap_or(trimmed);
    if stripped == "." || stripped.is_empty() {
        Some(build_file_dir.to_path_buf())
    } else {
        Some(build_file_dir.join(stripped))
    }
}

/// Route every parsed target declaration into the appropriate
/// index bucket.
pub(crate) fn build_index(
    decls: &[(PathBuf, GoTargetDeclaration)],
    scan_root: &Path,
) -> GoOwnershipIndex {
    let mut index = GoOwnershipIndex::default();
    for (build_file, decl) in decls {
        let build_file_dir = match build_file.parent() {
            Some(d) => d,
            None => continue,
        };
        match decl.kind {
            GoTargetKind::GoMod => {
                let address = compute_address(
                    build_file_dir,
                    scan_root,
                    decl.name.as_deref(),
                    "mod",
                );
                index
                    .go_mod_roots
                    .insert(build_file_dir.to_path_buf(), address);
            }
            GoTargetKind::GoThirdPartyPackage => {
                let Some(import_path) = decl.import_path.as_deref().filter(|s| !s.is_empty())
                else {
                    continue;
                };
                let Some(name) = decl.name.as_deref().filter(|s| !s.is_empty()) else {
                    continue;
                };
                let address = compute_address(
                    build_file_dir,
                    scan_root,
                    Some(name),
                    name,
                );
                index
                    .import_path_to_addresses
                    .entry(import_path.to_string())
                    .or_default()
                    .push(address);
            }
            GoTargetKind::GoBinary => {
                let Some(main) = decl.main.as_deref().filter(|s| !s.is_empty()) else {
                    continue;
                };
                let Some(name) = decl.name.as_deref().filter(|s| !s.is_empty()) else {
                    continue;
                };
                let Some(resolved_main) = normalize_main_path(main, build_file_dir) else {
                    continue;
                };
                let address = compute_address(
                    build_file_dir,
                    scan_root,
                    Some(name),
                    name,
                );
                index.main_targets.push((resolved_main, address));
            }
            GoTargetKind::GoPackage => {
                let fallback = build_file_dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("pkg");
                let address = compute_address(
                    build_file_dir,
                    scan_root,
                    decl.name.as_deref(),
                    fallback,
                );
                index
                    .package_targets
                    .push((build_file_dir.to_path_buf(), address));
            }
        }
    }
    index
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::scan_fs::package_db::pants_go::GoTargetKind;

    fn decl(
        kind: GoTargetKind,
        name: Option<&str>,
        import_path: Option<&str>,
        main: Option<&str>,
    ) -> GoTargetDeclaration {
        GoTargetDeclaration {
            kind,
            name: name.map(String::from),
            import_path: import_path.map(String::from),
            main: main.map(String::from),
            start_line: 1,
        }
    }

    #[test]
    fn single_go_mod_at_3rdparty_go() {
        let root = PathBuf::from("/repo");
        let build = root.join("3rdparty/go/BUILD");
        let index = build_index(
            &[(build.clone(), decl(GoTargetKind::GoMod, Some("mod"), None, None))],
            &root,
        );
        let addr = index.go_mod_roots.get(&PathBuf::from("/repo/3rdparty/go"));
        assert_eq!(addr.map(|a| a.0.as_str()), Some("3rdparty/go:mod"));
    }

    #[test]
    fn multi_go_mod_deep_and_shallow_both_indexed() {
        let root = PathBuf::from("/repo");
        let shallow = root.join("3rdparty/go/BUILD");
        let deep = root.join("services/api/3rdparty/go/BUILD");
        let index = build_index(
            &[
                (shallow, decl(GoTargetKind::GoMod, Some("root"), None, None)),
                (deep, decl(GoTargetKind::GoMod, Some("api"), None, None)),
            ],
            &root,
        );
        assert_eq!(index.go_mod_roots.len(), 2);
        assert!(index
            .go_mod_roots
            .contains_key(&PathBuf::from("/repo/3rdparty/go")));
        assert!(index
            .go_mod_roots
            .contains_key(&PathBuf::from("/repo/services/api/3rdparty/go")));
    }

    #[test]
    fn go_third_party_package_distinct_import_paths() {
        let root = PathBuf::from("/repo");
        let build = root.join("3rdparty/go/BUILD");
        let index = build_index(
            &[
                (
                    build.clone(),
                    decl(
                        GoTargetKind::GoThirdPartyPackage,
                        Some("cobra"),
                        Some("github.com/spf13/cobra"),
                        None,
                    ),
                ),
                (
                    build,
                    decl(
                        GoTargetKind::GoThirdPartyPackage,
                        Some("viper"),
                        Some("github.com/spf13/viper"),
                        None,
                    ),
                ),
            ],
            &root,
        );
        assert_eq!(index.import_path_to_addresses.len(), 2);
        assert_eq!(
            index
                .import_path_to_addresses
                .get("github.com/spf13/cobra")
                .and_then(|v| v.first())
                .map(|a| a.0.as_str()),
            Some("3rdparty/go:cobra"),
        );
    }

    #[test]
    fn go_binary_main_dot_resolves_to_build_dir() {
        let root = PathBuf::from("/repo");
        let build = root.join("cmd/frontend/BUILD");
        let index = build_index(
            &[(
                build,
                decl(GoTargetKind::GoBinary, Some("frontend"), None, Some(".")),
            )],
            &root,
        );
        assert_eq!(index.main_targets.len(), 1);
        assert_eq!(index.main_targets[0].0, PathBuf::from("/repo/cmd/frontend"));
        assert_eq!(index.main_targets[0].1 .0, "cmd/frontend:frontend");
    }

    #[test]
    fn go_binary_main_subdir_resolves_relative_to_build_dir() {
        let root = PathBuf::from("/repo");
        let build = root.join("BUILD");
        let index = build_index(
            &[(
                build,
                decl(GoTargetKind::GoBinary, Some("cli"), None, Some("./cmd/foo")),
            )],
            &root,
        );
        assert_eq!(index.main_targets[0].0, PathBuf::from("/repo/cmd/foo"));
    }

    #[test]
    fn go_binary_absolute_path_main_is_skipped() {
        let root = PathBuf::from("/repo");
        let build = root.join("cmd/BUILD");
        let index = build_index(
            &[(
                build,
                decl(
                    GoTargetKind::GoBinary,
                    Some("cli"),
                    None,
                    Some("/etc/foo"),
                ),
            )],
            &root,
        );
        assert!(index.main_targets.is_empty());
    }

    #[test]
    fn go_package_default_name_uses_dir_basename() {
        let root = PathBuf::from("/repo");
        let build = root.join("services/api/BUILD");
        let index = build_index(
            &[(build, decl(GoTargetKind::GoPackage, None, None, None))],
            &root,
        );
        assert_eq!(index.package_targets.len(), 1);
        assert_eq!(index.package_targets[0].1 .0, "services/api:api");
    }

    #[test]
    fn empty_declarations_returns_empty_index() {
        let root = PathBuf::from("/repo");
        let index = build_index(&[], &root);
        assert!(index.go_mod_roots.is_empty());
        assert!(index.import_path_to_addresses.is_empty());
        assert!(index.main_targets.is_empty());
        assert!(index.package_targets.is_empty());
    }
}
