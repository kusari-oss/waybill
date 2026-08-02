//! Milestone 225: resolve target `source=` / `sources=[...]` → `Vec<PathBuf>`.
//!
//! Given a `TargetDeclaration` and the BUILD file's path, compute:
//!   1. The canonical Pants target address (`<dir>:<name>` or bare
//!      `<name>` for root-BUILD-file targets; dir-basename fallback
//!      when `name=` was omitted for `shell_sources` / `shunit2_tests`).
//!   2. The Vec<PathBuf> of on-disk `.sh` files the target resolves to.
//!      Missing files are dropped with a WARN naming the target
//!      address + missing path (per FR-009 fail-open at target grain).
//!
//! Glob patterns:
//!   - `*.sh` — non-recursive glob within the BUILD file's directory
//!   - `**/*.sh` — recursive glob under the BUILD file's directory
//!   - `subdir/*.sh` — glob within a specific subdirectory
//!
//! Uses `globset` (workspace dep from m113).

use std::path::{Path, PathBuf};

use globset::{Glob, GlobSetBuilder};

use super::{ResolvedTarget, TargetDeclaration, TargetSource};

/// Compute the target address for a BUILD file at `build_file_dir`
/// relative to `scan_root`, given an optional explicit `name=`.
/// Dir-basename fallback when `name` is None.
fn compute_address(build_file_dir: &Path, scan_root: &Path, name: Option<&str>) -> String {
    let rel = build_file_dir
        .strip_prefix(scan_root)
        .unwrap_or(build_file_dir);
    let dir_str = rel.to_string_lossy().replace('\\', "/");
    // Fallback name = dir basename (matches Pants default-target semantic).
    let dir_basename = build_file_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let name = name
        .filter(|s| !s.is_empty())
        .unwrap_or(dir_basename);
    if dir_str.is_empty() || dir_str == "." {
        // Root-level BUILD file → bare name.
        name.to_string()
    } else {
        format!("{dir_str}:{name}")
    }
}

/// Resolve a single `TargetSource::Single` path relative to the BUILD
/// file's directory. Returns the joined path, existence-checked.
fn resolve_single(build_file_dir: &Path, rel_path: &str) -> PathBuf {
    build_file_dir.join(rel_path)
}

/// Resolve a `TargetSource::Globs` list of patterns. Non-existent
/// matches are dropped; recursive `**` patterns are honored by
/// `globset`'s default `MatchOptions`.
fn resolve_globs(build_file_dir: &Path, patterns: &[String]) -> Vec<PathBuf> {
    if patterns.is_empty() {
        return Vec::new();
    }
    let mut builder = GlobSetBuilder::new();
    let mut any_valid = false;
    for p in patterns {
        match Glob::new(p) {
            Ok(g) => {
                builder.add(g);
                any_valid = true;
            }
            Err(e) => {
                tracing::warn!(
                    pattern = %p,
                    error = %e,
                    "pants-shell reader: invalid glob pattern in sources=[]; skipping"
                );
            }
        }
    }
    if !any_valid {
        return Vec::new();
    }
    let set = match builder.build() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "pants-shell reader: failed to build glob set; skipping target"
            );
            return Vec::new();
        }
    };
    // Determine whether any pattern uses `**` (recursive) — if so, we
    // need to walk subdirectories.
    let recursive = patterns.iter().any(|p| p.contains("**"));
    let mut out = Vec::new();
    walk_and_match(build_file_dir, build_file_dir, &set, recursive, &mut out);
    out.sort();
    out
}

fn walk_and_match(
    dir: &Path,
    base: &Path,
    set: &globset::GlobSet,
    recursive: bool,
    out: &mut Vec<PathBuf>,
) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                walk_and_match(&path, base, set, recursive, out);
            }
            continue;
        }
        // Match against the path RELATIVE to the base (BUILD file's dir).
        let rel = match path.strip_prefix(base) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if set.is_match(rel) {
            out.push(path);
        }
    }
}

/// Public: fully resolve a target declaration into address + files.
/// Callers pass the BUILD file's absolute path and the scan root.
pub(crate) fn resolve_target(
    decl: &TargetDeclaration,
    build_file: &Path,
    scan_root: &Path,
) -> ResolvedTarget {
    let build_file_dir = build_file.parent().unwrap_or(scan_root);
    let address = compute_address(build_file_dir, scan_root, decl.name.as_deref());

    let files: Vec<PathBuf> = match &decl.source {
        TargetSource::Single(rel_path) => {
            let candidate = resolve_single(build_file_dir, rel_path);
            if candidate.exists() {
                vec![candidate]
            } else {
                tracing::warn!(
                    target = %address,
                    missing_path = %candidate.display(),
                    "pants-shell reader: source= references a file that does not exist on disk; skipping"
                );
                Vec::new()
            }
        }
        TargetSource::Globs(patterns) => {
            let resolved = resolve_globs(build_file_dir, patterns);
            if patterns.is_empty() {
                // Operator omitted `sources=` — Pants would apply the
                // default (`["*.sh", "*.bash"]`). We do NOT emulate that
                // in v1 per spec Assumptions (explicit sources only);
                // log an INFO diagnostic naming the target so operators
                // can spot silent omissions.
                tracing::info!(
                    target = %address,
                    "pants-shell reader: target omitted sources=[]; emitting nothing (explicit-glob-only mode)"
                );
            } else if resolved.is_empty() {
                tracing::info!(
                    target = %address,
                    patterns = ?patterns,
                    "pants-shell reader: sources=[] glob matched zero files; emitting nothing"
                );
            }
            resolved
        }
    };

    ResolvedTarget {
        address,
        kind: decl.kind,
        files,
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::scan_fs::package_db::pants_shell::ShellTargetKind;
    use std::io::Write;
    use tempfile::tempdir;

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        writeln!(f, "#!/bin/sh").unwrap();
        p
    }

    #[test]
    fn single_source_resolves_to_one_file() {
        let root = tempdir().unwrap();
        let scripts = root.path().join("scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        let build_file = scripts.join("BUILD");
        std::fs::write(&build_file, "").unwrap();
        touch(&scripts, "deploy.sh");
        let decl = TargetDeclaration {
            kind: ShellTargetKind::ShellSource,
            name: Some("deploy".to_string()),
            source: TargetSource::Single("deploy.sh".to_string()),
            start_line: 1,
        };
        let rt = resolve_target(&decl, &build_file, root.path());
        assert_eq!(rt.address, "scripts:deploy");
        assert_eq!(rt.files.len(), 1);
        assert_eq!(rt.files[0].file_name().unwrap(), "deploy.sh");
    }

    #[test]
    fn glob_sources_matches_three_files() {
        let root = tempdir().unwrap();
        let helpers = root.path().join("helpers");
        std::fs::create_dir_all(&helpers).unwrap();
        let build_file = helpers.join("BUILD");
        std::fs::write(&build_file, "").unwrap();
        touch(&helpers, "a.sh");
        touch(&helpers, "b.sh");
        touch(&helpers, "c.sh");
        touch(&helpers, "d.txt"); // should NOT match
        let decl = TargetDeclaration {
            kind: ShellTargetKind::ShellSources,
            name: Some("utils".to_string()),
            source: TargetSource::Globs(vec!["*.sh".to_string()]),
            start_line: 1,
        };
        let rt = resolve_target(&decl, &build_file, root.path());
        assert_eq!(rt.address, "helpers:utils");
        assert_eq!(rt.files.len(), 3);
    }

    #[test]
    fn recursive_glob_matches_nested() {
        let root = tempdir().unwrap();
        let helpers = root.path().join("helpers");
        let nested = helpers.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let build_file = helpers.join("BUILD");
        std::fs::write(&build_file, "").unwrap();
        touch(&helpers, "top.sh");
        touch(&nested, "inner.sh");
        let decl = TargetDeclaration {
            kind: ShellTargetKind::ShellSources,
            name: Some("all".to_string()),
            source: TargetSource::Globs(vec!["**/*.sh".to_string()]),
            start_line: 1,
        };
        let rt = resolve_target(&decl, &build_file, root.path());
        assert_eq!(rt.files.len(), 2);
    }

    #[test]
    fn missing_source_file_returns_empty_files() {
        let root = tempdir().unwrap();
        let scripts = root.path().join("scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        let build_file = scripts.join("BUILD");
        std::fs::write(&build_file, "").unwrap();
        // Do NOT create the .sh file.
        let decl = TargetDeclaration {
            kind: ShellTargetKind::ShellSource,
            name: Some("gone".to_string()),
            source: TargetSource::Single("gone.sh".to_string()),
            start_line: 1,
        };
        let rt = resolve_target(&decl, &build_file, root.path());
        assert_eq!(rt.files.len(), 0);
    }

    #[test]
    fn empty_glob_match_returns_empty_files() {
        let root = tempdir().unwrap();
        let dir = root.path().join("empty");
        std::fs::create_dir_all(&dir).unwrap();
        let build_file = dir.join("BUILD");
        std::fs::write(&build_file, "").unwrap();
        let decl = TargetDeclaration {
            kind: ShellTargetKind::ShellSources,
            name: Some("nothing".to_string()),
            source: TargetSource::Globs(vec!["*.sh".to_string()]),
            start_line: 1,
        };
        let rt = resolve_target(&decl, &build_file, root.path());
        assert_eq!(rt.files.len(), 0);
    }

    #[test]
    fn root_level_build_address_is_bare_name() {
        let root = tempdir().unwrap();
        let build_file = root.path().join("BUILD");
        std::fs::write(&build_file, "").unwrap();
        touch(root.path(), "top.sh");
        let decl = TargetDeclaration {
            kind: ShellTargetKind::ShellSource,
            name: Some("root-script".to_string()),
            source: TargetSource::Single("top.sh".to_string()),
            start_line: 1,
        };
        let rt = resolve_target(&decl, &build_file, root.path());
        assert_eq!(rt.address, "root-script");
    }

    #[test]
    fn subdir_build_address_is_prefixed() {
        let root = tempdir().unwrap();
        let sub = root.path().join("a/b/c");
        std::fs::create_dir_all(&sub).unwrap();
        let build_file = sub.join("BUILD");
        std::fs::write(&build_file, "").unwrap();
        touch(&sub, "x.sh");
        let decl = TargetDeclaration {
            kind: ShellTargetKind::ShellSource,
            name: Some("nested".to_string()),
            source: TargetSource::Single("x.sh".to_string()),
            start_line: 1,
        };
        let rt = resolve_target(&decl, &build_file, root.path());
        assert_eq!(rt.address, "a/b/c:nested");
    }

    #[test]
    fn missing_name_falls_back_to_dir_basename() {
        let root = tempdir().unwrap();
        let helpers = root.path().join("helpers");
        std::fs::create_dir_all(&helpers).unwrap();
        let build_file = helpers.join("BUILD");
        std::fs::write(&build_file, "").unwrap();
        touch(&helpers, "a.sh");
        let decl = TargetDeclaration {
            kind: ShellTargetKind::ShellSources,
            name: None,
            source: TargetSource::Globs(vec!["*.sh".to_string()]),
            start_line: 1,
        };
        let rt = resolve_target(&decl, &build_file, root.path());
        assert_eq!(rt.address, "helpers:helpers");
    }
}
