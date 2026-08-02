//! Milestone 224: JVM resolve-name → `LifecycleScope` allowlist classifier.
//!
//! Pants JVM supports multiple named "resolves" (default, junit,
//! scalatest, ktlint, and so on), each with its own coursier lockfile.
//! Per FR-008, we tag components from resolves whose name matches a
//! JVM-specific dev-tool allowlist as `LifecycleScope::Development`.
//! Every other resolve (including `default`) tags as
//! `LifecycleScope::Runtime` (the safe default).
//!
//! The classifier is deliberately less aggressive than a broad
//! "anything test-shaped" heuristic — false positives here downgrade
//! prod dependencies out of vuln-scan scope, which is worse than
//! false negatives.
//!
//! Every emitted component also carries a `waybill:pants-resolve`
//! annotation with the resolve name verbatim (see contracts §"Output
//! contract"), so operators can spot-check and re-tag downstream.

use waybill_common::resolution::LifecycleScope;

/// Allowlist of JVM resolve names that should tag as `Development`.
/// Case-insensitive match against the lockfile filename stem
/// (`3rdparty/jvm/junit.lock` → `junit`) OR the `[jvm.resolves]`
/// config-declared name.
const DEV_RESOLVE_NAMES: &[&str] = &[
    // Test frameworks
    "scalatest",
    "junit",
    "testng",
    "mockito",
    "assertj",
    "hamcrest",
    // Formatters
    "scalafmt",
    "scalastyle",
    "scalafix",
    "checkstyle",
    // Static analyzers
    "spotbugs",
    "pmd",
    "errorprone",
    // Coverage
    "jacoco",
    // Docs
    "dokka",
    // Kotlin dev-tools
    "ktlint",
    "detekt",
    // Generic dev-scope names Pants users commonly pick
    "lint",
    "test",
    "dev",
    "ci",
    "check",
    "tools",
    "docs",
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
    fn junit_resolve_tags_development() {
        assert_eq!(classify_resolve("junit"), LifecycleScope::Development);
    }

    #[test]
    fn ktlint_uppercase_case_insensitive_match() {
        assert_eq!(classify_resolve("KTLINT"), LifecycleScope::Development);
    }

    #[test]
    fn scalatest_resolve_tags_development() {
        assert_eq!(classify_resolve("scalatest"), LifecycleScope::Development);
    }

    #[test]
    fn custom_runtime_name_tags_runtime() {
        assert_eq!(
            classify_resolve("my-service-runtime"),
            LifecycleScope::Runtime,
        );
    }
}
