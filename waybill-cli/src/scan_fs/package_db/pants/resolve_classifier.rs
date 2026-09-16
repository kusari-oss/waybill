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

/// How strong the evidence behind a resolve's lifecycle classification is.
///
/// Milestone 868 (#887). The distinction is the point: a name allowlist is a
/// guess about a convention, and a `pants.toml` tool section saying
/// `install_from_resolve = "<name>"` is the project stating what the resolve
/// is for. Both can produce the same answer; only one of them is evidence.
/// The count of resolves decided by the weaker kind is emitted document-scope
/// so an auditor can see how much of the classification rests on guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClassificationSource {
    /// A tool section back-referenced this resolve via `install_from_resolve`.
    Declared,
    /// Nothing declared it; the name allowlist or the runtime default decided.
    HeuristicOrDefault,
}

impl ClassificationSource {
    /// Wire form for the `waybill:resolve-classification-source` annotation.
    pub(crate) fn as_wire_str(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::HeuristicOrDefault => "heuristic-or-default",
        }
    }
}

/// Classify a resolve, preferring what the project declares over what its
/// name suggests (FR-003a, contract A-4).
///
/// A tool section carrying `install_from_resolve = "<name>"` says that
/// resolve exists to provide that tool, which makes it build-time. That
/// beats the allowlist in both directions — including the case the allowlist
/// gets wrong on the measured target, where `coverage-py` and `setuptools`
/// are declared by tool sections but match no allowlist entry (`coverage`
/// and `coveragepy` are listed; `coverage-py` is not) and so classify as
/// runtime today.
///
/// The allowlist is deliberately NOT widened to cover them. Research R1
/// records why: patching those two names in would paper over the reason a
/// name allowlist is the wrong instrument, and would still be wrong for the
/// next project that names its coverage resolve something else.
///
/// Where nothing declares, the allowlist still applies and the result is
/// reported as weaker-than-declaration evidence. Removing the fallback
/// outright — as FR-003a originally required — would regress every
/// repository whose tools do not declare, and no measurement supports that
/// trade.
pub(crate) fn classify_resolve_with_source(
    resolve_name: &str,
    declared_by_tool: bool,
) -> (LifecycleScope, ClassificationSource) {
    if declared_by_tool {
        (LifecycleScope::Development, ClassificationSource::Declared)
    } else {
        (
            classify_resolve(resolve_name),
            ClassificationSource::HeuristicOrDefault,
        )
    }
}

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

    // ---------------------------------------------------------------
    // Milestone 868 T024-T025 (US3): declaration beats the allowlist.
    // Contract A-4 (declaration over name) + A-5 (undeclared => runtime,
    // reported).
    // ---------------------------------------------------------------

    #[test]
    fn t024_declaration_beats_allowlist_when_they_disagree() {
        // The live misclassification on the measured target: `coverage-py`
        // matches no allowlist entry (`coverage` and `coveragepy` are
        // listed, `coverage-py` is not), so the name says runtime. A
        // `[coverage-py]` section declares `install_from_resolve`, so the
        // project says build-time. The project wins.
        assert_eq!(
            classify_resolve("coverage-py"),
            LifecycleScope::Runtime,
            "precondition: the allowlist alone gets this wrong",
        );
        assert_eq!(
            classify_resolve_with_source("coverage-py", true),
            (LifecycleScope::Development, ClassificationSource::Declared),
        );

        // Second live case, same shape: `setuptools` is in no category of
        // the allowlist at all.
        assert_eq!(classify_resolve("setuptools"), LifecycleScope::Runtime);
        assert_eq!(
            classify_resolve_with_source("setuptools", true),
            (LifecycleScope::Development, ClassificationSource::Declared),
        );
    }

    #[test]
    fn t024b_declaration_is_recorded_even_when_it_agrees_with_the_allowlist() {
        // `mypy` and `pytest` are BOTH allowlisted and declared on the
        // measured target. The scope is the same either way, so a test that
        // only checked the scope would pass on a broken implementation that
        // ignored the declaration entirely. The source is what differs, and
        // it is what the weak-evidence count is computed from.
        for name in ["mypy", "pytest", "black"] {
            assert_eq!(
                classify_resolve(name),
                LifecycleScope::Development,
                "precondition: {name} is allowlisted",
            );
            assert_eq!(
                classify_resolve_with_source(name, true),
                (LifecycleScope::Development, ClassificationSource::Declared),
                "{name}: a declaration must be recorded as one even when the \
                 allowlist would have reached the same scope",
            );
        }
    }

    #[test]
    fn t025_undeclared_falls_back_and_says_so() {
        // Contract A-5. Undeclared resolves keep the allowlist result and
        // are marked as decided by weaker evidence. Asymmetric on purpose:
        // mis-marking a runtime resolve as build-time hides packages from a
        // consumer filtering for runtime risk, so the default takes the
        // loud failure.
        assert_eq!(
            classify_resolve_with_source("python-default", false),
            (
                LifecycleScope::Runtime,
                ClassificationSource::HeuristicOrDefault
            ),
        );
        // Allowlist still applies where nothing declares — it is a fallback,
        // not dead code.
        assert_eq!(
            classify_resolve_with_source("mypy", false),
            (
                LifecycleScope::Development,
                ClassificationSource::HeuristicOrDefault
            ),
        );
    }

    #[test]
    fn t025b_wire_strings_are_stable() {
        // These land in emitted documents; they are not free to drift.
        assert_eq!(ClassificationSource::Declared.as_wire_str(), "declared");
        assert_eq!(
            ClassificationSource::HeuristicOrDefault.as_wire_str(),
            "heuristic-or-default",
        );
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
