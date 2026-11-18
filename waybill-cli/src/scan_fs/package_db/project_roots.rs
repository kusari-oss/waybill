//! Shared `should_skip_default_descent` helper used by every
//! ecosystem reader's `scan_fs::walk::WalkConfig` skip predicate.
//!
//! Pre-milestone-114, this file also hosted a `walk_for_project_roots`
//! recursive descent function + a per-call `WalkConfig` adapter
//! struct that mediated between per-ecosystem `Fn(&str) -> bool`
//! closures and the descent loop. Post-milestone-114 (PR 5) all of
//! that machinery moved to `crate::scan_fs::walk::safe_walk` + its
//! own `WalkConfig`; this file shrinks to the shared skip-set
//! helper. Issue #108.
//!
//! The skip predicates across pip / npm / gradle / nuget / yocto / the
//! per-walker `should_skip_descent` callsites overlap almost entirely —
//! they all want to skip installed-tree subtrees, hidden / VCS /
//! tooling dirs, and common build/cache outputs. The
//! [`should_skip_default_descent`] helper centralises that set; each
//! reader's `should_skip` closure composes it with ecosystem-specific
//! additions (pip's `site-packages` for example).

/// Default skip-set: directory names that no ecosystem should
/// descend into when looking for project roots. Three reasons:
///
/// 1. **Installed-tree subtrees** — `node_modules/`, `vendor/`,
///    `bower_components/`. Their own manifests are already handled
///    by their parent project's installed-tree walker; descending
///    would produce N² false-positive "project roots".
/// 2. **Hidden / VCS / tooling dirs** — anything starting with `.`
///    (`.git/`, `.next/`, `.venv/`, `.cache/`, …). Never a project
///    root; always just noise.
/// 3. **Build outputs and language caches** — `target/` (Rust +
///    Maven), `dist/`, `build/`, `out/`, `coverage/`,
///    `__pycache__/`, `venv/`. Won't contain upstream-project
///    metadata worth re-reading.
///
/// Ecosystem-specific additions compose with this — e.g., pip
/// additionally skips `site-packages/` because its venv-walker
/// handles dist-info on a separate pass.
pub(crate) fn should_skip_default_descent(name: &str) -> bool {
    if name.starts_with('.') {
        return true;
    }
    matches!(
        name,
        "node_modules"
            | "bower_components"
            | "vendor"
            | "target"
            | "dist"
            | "build"
            | "out"
            | "coverage"
            | "__pycache__"
            | "venv"
    )
}

/// Spec 1613 (#1610): the dot-directories the manifest readers descend into
/// despite [`should_skip_default_descent`]. They hold CI and docs build
/// configuration; every other dot-directory (VCS metadata, virtualenvs,
/// tool caches, editor state, upstream test suites) stays skipped. Matched
/// against a directory's name, case-insensitively, at any depth.
pub(crate) const LISTED_DOT_DIRECTORIES: [&str; 7] =
    [".github", ".ci", ".gitlab", ".circleci", ".buildkite", ".devcontainer", ".sphinx"];

/// What a listed dot-directory holds: CI configuration, or a docs build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DotDirClass {
    Ci,
    Docs,
}

impl DotDirClass {
    /// The C155 `waybill:python-req-file-scope` value for this class.
    pub(crate) fn requirements_scope(self) -> &'static str {
        match self {
            Self::Ci => "ci",
            Self::Docs => "docs",
        }
    }
}

/// The class of `name` if it is a listed dot-directory.
pub(crate) fn listed_dot_dir_class(name: &str) -> Option<DotDirClass> {
    let hit = LISTED_DOT_DIRECTORIES.iter().find(|d| d.eq_ignore_ascii_case(name))?;
    Some(if *hit == ".sphinx" { DotDirClass::Docs } else { DotDirClass::Ci })
}

/// The class of the listed dot-directory nearest to `path` among its
/// ancestors below `root`. A file outside `root` is compared from its own
/// ancestors, without the root boundary.
pub(crate) fn listed_ancestor_class(path: &std::path::Path, root: &std::path::Path) -> Option<DotDirClass> {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.parent()?
        .components()
        .rev()
        .filter_map(|c| c.as_os_str().to_str())
        .find_map(listed_dot_dir_class)
}

/// The `descend_into` glob a registration uses to opt into
/// [`LISTED_DOT_DIRECTORIES`] (shared walker contract C10).
pub(crate) fn listed_dot_dir_globset() -> anyhow::Result<globset::GlobSet> {
    crate::scan_fs::walk_registry::globset_from_patterns_case_insensitive(&LISTED_DOT_DIRECTORIES)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn listed_names_match_in_any_case_and_others_do_not() {
        for n in [".github", ".GitHub", ".CI", ".ci", ".devcontainer", ".gitlab"] {
            assert_eq!(listed_dot_dir_class(n), Some(DotDirClass::Ci), "{n}");
        }
        assert_eq!(listed_dot_dir_class(".SPHINX"), Some(DotDirClass::Docs));
        for n in [".venv", ".tox", ".cache", ".upstream-tests", "._upstream", ".git", "ci", "github"] {
            assert_eq!(listed_dot_dir_class(n), None, "{n}");
        }
    }

    #[test]
    fn the_nearest_listed_ancestor_decides_and_the_root_bounds_it() {
        let root = Path::new("/r");
        assert_eq!(listed_ancestor_class(Path::new("/r/.ci/metrics/requirements.txt"), root), Some(DotDirClass::Ci));
        assert_eq!(listed_ancestor_class(Path::new("/r/docs/.sphinx/requirements.txt"), root), Some(DotDirClass::Docs));
        assert_eq!(listed_ancestor_class(Path::new("/r/.github/x/.sphinx/requirements.txt"), root), Some(DotDirClass::Docs));
        assert_eq!(listed_ancestor_class(Path::new("/r/.sphinx/x/.ci/requirements.txt"), root), Some(DotDirClass::Ci));
        assert_eq!(listed_ancestor_class(Path::new("/r/src/requirements.txt"), root), None);
        assert_eq!(listed_ancestor_class(Path::new("/.ci/r/requirements.txt"), Path::new("/.ci/r")), None);
    }

    #[test]
    fn the_glob_matches_listed_names_only() {
        let g = listed_dot_dir_globset().unwrap();
        for n in [".github", ".CI", ".sphinx", ".DevContainer"] {
            assert!(g.is_match(n), "{n}");
        }
        for n in [".venv", ".upstream-tests", "node_modules", "build", ".git"] {
            assert!(!g.is_match(n), "{n}");
        }
    }

    #[test]
    fn the_default_skip_is_unchanged_for_listed_names() {
        for n in LISTED_DOT_DIRECTORIES {
            assert!(should_skip_default_descent(n), "{n} is still skipped by default");
        }
    }
}
