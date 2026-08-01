//! Milestone 223: resolve-name → `LifecycleScope` allowlist classifier.
//!
//! Pants supports multiple named "resolves" (default plus mypy, pytest,
//! and so on), each with its own lockfile. Per Q1 answer B and per
//! research.md §R2, we tag components from resolves whose name matches
//! a known dev-tool allowlist as `LifecycleScope::Development`. Every
//! other resolve (including the `default` resolve) tags as
//! `LifecycleScope::Runtime` (the safe default).
//!
//! Every emitted component also carries a `waybill:pants-resolve`
//! annotation with the resolve name verbatim, so operators can
//! spot-check and re-tag downstream if the heuristic misfires on a
//! custom resolve name.

use waybill_common::resolution::LifecycleScope;

/// Allowlist of resolve names that should tag as `Development`.
/// Case-insensitive match against the lockfile filename stem
/// (`3rdparty/python/mypy.lock` → `mypy`). Widened per R2 to cover
/// common Pants community usage across public repos.
const DEV_RESOLVE_NAMES: &[&str] = &[
    // Formatters + linters
    "black",
    "ruff",
    "isort",
    "yapf",
    "autopep8",
    "flake8",
    // Type checkers
    "mypy",
    "pyright",
    "pyre",
    // Test runners
    "pytest",
    "unittest",
    "nose",
    // Coverage
    "coverage",
    "coveragepy",
    // Security scanners
    "bandit",
    "safety",
    // Docs / packaging
    "sphinx",
    "docs",
    // Generic dev-scope names Pants users commonly pick
    "lint",
    "test",
    "dev",
    "ci",
    "check",
    "tools",
];

/// Return `LifecycleScope::Development` if the resolve name is in the
/// dev-allowlist (case-insensitive), else `LifecycleScope::Runtime`.
/// The `default` resolve always returns `Runtime`.
pub(crate) fn classify_resolve(resolve_name: &str) -> LifecycleScope {
    let lowered = resolve_name.to_lowercase();
    if DEV_RESOLVE_NAMES.iter().any(|n| *n == lowered) {
        LifecycleScope::Development
    } else {
        LifecycleScope::Runtime
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn default_resolve_tags_runtime() {
        assert_eq!(classify_resolve("default"), LifecycleScope::Runtime);
    }

    #[test]
    fn mypy_resolve_tags_development() {
        assert_eq!(classify_resolve("mypy"), LifecycleScope::Development);
    }

    #[test]
    fn case_insensitive_match() {
        assert_eq!(classify_resolve("MyPy"), LifecycleScope::Development);
        assert_eq!(classify_resolve("PYTEST"), LifecycleScope::Development);
    }

    #[test]
    fn unknown_resolve_name_defaults_runtime() {
        assert_eq!(
            classify_resolve("my-custom-resolve"),
            LifecycleScope::Runtime
        );
        assert_eq!(
            classify_resolve("payment_processor"),
            LifecycleScope::Runtime
        );
    }
}
