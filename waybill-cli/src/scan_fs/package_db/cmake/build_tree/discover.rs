//! Spec 1652 T009: what a build tree holds, found by name.
//!
//! One `safe_walk` over each named tree, which does NOT apply the source
//! scan's default-descent skip: a build tree is `build/`, and the file API
//! lives under `.cmake/`. It records every `CMakeCache.txt`, every
//! FetchContent population (`_deps/<lc>-src/`), and every ExternalProject
//! stamp (`<name>-download`, `<name>-mkdir` files and `<name>-stamp/`
//! directories), wherever `PREFIX`/`STAMP_DIR` put them (research R3, R7).
//! Contents are not read here; stamps are never read at all (R9).

use std::path::{Path, PathBuf};

use super::super::super::exclude_path::ExclusionSet;
use crate::scan_fs::walk::{safe_walk, WalkConfig};

/// Deep enough for TheRock's `<subproject>/build/_deps/x-subbuild/…` and
/// `<target>_fetch/src/<target>-stamp/` layouts.
const MAX_DEPTH: usize = 16;

#[derive(Debug, Default)]
pub(crate) struct Found {
    pub(crate) caches: Vec<PathBuf>,
    /// `_deps/<lc>-src` directories.
    pub(crate) populations: Vec<PathBuf>,
    /// `<name>-download` files inside a `*-stamp` directory.
    pub(crate) downloads: Vec<PathBuf>,
    /// `*-stamp` directories.
    pub(crate) stamp_dirs: Vec<PathBuf>,
}

fn name(p: &Path) -> &str {
    p.file_name().and_then(|n| n.to_str()).unwrap_or("")
}

fn parent_name(p: &Path) -> &str {
    p.parent().map(name).unwrap_or("")
}

/// Descent stops at object trees, VCS metadata and FetchContent's populated
/// sources: what a population holds is the dependency's own code.
fn skip(candidate: &Path, _root: &Path) -> bool {
    let n = name(candidate);
    n == "CMakeFiles" || n == ".git" || (parent_name(candidate) == "_deps" && n.ends_with("-src"))
}

pub(crate) fn discover(tree: &Path) -> Found {
    let mut found = Found::default();
    let exclude = ExclusionSet::new_empty();
    let cfg = WalkConfig { max_depth: MAX_DEPTH, should_skip: &skip, exclude_set: &exclude };
    // A skipped directory is never visited, so populations are listed when
    // their `_deps` parent is.
    safe_walk(tree, &cfg, |p| {
        let n = name(p);
        if p.is_dir() {
            if n == "_deps" {
                for e in std::fs::read_dir(p).into_iter().flatten().flatten() {
                    let child = e.path();
                    if child.is_dir() && name(&child).ends_with("-src") {
                        found.populations.push(child);
                    }
                }
            } else if n.ends_with("-stamp") {
                found.stamp_dirs.push(p.to_path_buf());
            }
        } else if n == "CMakeCache.txt" {
            found.caches.push(p.to_path_buf());
        } else if n.ends_with("-download") && parent_name(p).ends_with("-stamp") {
            found.downloads.push(p.to_path_buf());
        }
    });
    found.caches.sort();
    found.populations.sort();
    found.downloads.sort();
    found.stamp_dirs.sort();
    found
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
    }

    #[test]
    fn finds_caches_populations_and_moved_stamps() {
        let d = tempfile::tempdir().unwrap();
        let b = d.path();
        touch(&b.join("CMakeCache.txt"));
        touch(&b.join("_deps/zlib-subbuild/CMakeCache.txt"));
        std::fs::create_dir_all(b.join("_deps/zlib-src/inner/_deps/nested-src")).unwrap();
        touch(&b.join("sub/build/CMakeCache.txt"));
        // TheRock's therock_subproject_fetch: PREFIX <target>_fetch.
        touch(&b.join("third-party/fmt/fmt_fetch/src/fmt-stamp/fmt-download"));
        std::fs::create_dir_all(b.join("x-prefix/src/x-stamp")).unwrap();
        touch(&b.join("CMakeFiles/junk-stamp/junk-download"));
        let f = discover(b);
        let rel = |v: &[PathBuf]| v.iter().map(|p| p.strip_prefix(b).unwrap().to_string_lossy().replace('\\', "/")).collect::<Vec<_>>();
        assert_eq!(rel(&f.caches), ["CMakeCache.txt", "_deps/zlib-subbuild/CMakeCache.txt", "sub/build/CMakeCache.txt"]);
        assert_eq!(rel(&f.populations), ["_deps/zlib-src"], "a population's own tree is not descended");
        assert_eq!(rel(&f.downloads), ["third-party/fmt/fmt_fetch/src/fmt-stamp/fmt-download"]);
        assert_eq!(rel(&f.stamp_dirs), ["third-party/fmt/fmt_fetch/src/fmt-stamp", "x-prefix/src/x-stamp"]);
    }

    #[test]
    fn hidden_and_build_named_directories_are_descended() {
        let d = tempfile::tempdir().unwrap();
        touch(&d.path().join("build/.cmake/api/v1/reply/out/CMakeCache.txt"));
        assert_eq!(discover(d.path()).caches.len(), 1);
    }
}
