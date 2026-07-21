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
