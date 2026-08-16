//! Milestone 235 — Gradle transitive dependency resolution ladder.
//!
//! Tier + fallback-reason enums for the four-tier ladder. Mirrors the
//! `ResolutionStep` pattern from `golang/graph_resolver.rs` (m055 / m160).
//!
//! Kebab-case serialization matches the annotation string that the
//! m235 US4 emitter writes to CDX + SPDX 2.3 + SPDX 3.
//!
//! Spec: `specs/235-gradle-transitive-ladder/spec.md` FR-006 / FR-008.
//!
//! Scaffolding for a multi-user-story milestone: m235 MVP (Phase 3 US1)
//! constructs `Subprocess` and `LockfileOnly` variants; `Cache` and
//! `Static` land with the US2/US3 follow-on milestones. The
//! `#[allow(dead_code)]` gates those variants until they're wired.
#![allow(dead_code)]

use serde::Serialize;

/// Which mechanism produced the emitted Gradle dependency graph.
///
/// The four base tiers form a strict fallback ladder: `Subprocess` (US1)
/// is preferred when the operator opts in and a wrapper is available,
/// then `Cache` (US2), then `Static` (US3). `LockfileOnly` is the m106
/// legacy path that fires when a `gradle.lockfile` exists but no ladder
/// tier ran (or ran and failed).
///
/// The `Mixed` aggregate is NOT a variant here — it's computed at
/// emission time by the annotation writer per Clarifications Q1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GradleResolutionTier {
    /// US1 — `./gradlew :sub:dependencies --no-daemon`.
    Subprocess,
    /// US2 — `${GRADLE_USER_HOME}/caches/modules-2/` walk.
    Cache,
    /// US3 — regex-scoped DSL extraction from `build.gradle(.kts)`.
    Static,
    /// m106 legacy — flat resolved-lockfile emission with no ladder tier.
    LockfileOnly,
}

impl GradleResolutionTier {
    /// Kebab-case annotation string emitted into the SBOM.
    pub fn as_annotation_str(&self) -> &'static str {
        match self {
            Self::Subprocess => "subprocess",
            Self::Cache => "cache",
            Self::Static => "static",
            Self::LockfileOnly => "lockfile-only",
        }
    }
}

/// Reason a ladder tier declined to run OR failed after starting.
///
/// Attached to the `waybill:gradle-fallback-reason` annotation so
/// consumers can distinguish "operator didn't opt in" from "tool
/// missing" from "subprocess timed out".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GradleFallbackReason {
    /// Subprocess killed at the operator-configured timeout.
    Timeout,
    /// No Gradle wrapper OR `gradle` binary found on PATH.
    MissingTool,
    /// Subprocess output couldn't be parsed (unexpected shape).
    ParseError,
    /// Cache didn't contain enough declared deps to be authoritative.
    CacheMiss,
    /// No `build.gradle(.kts)` files found in the scan tree.
    NoSourceFiles,
    /// Operator did not pass the opt-in flag for this tier.
    OperatorOptOut,
    /// Subprocess exited non-zero (build script broken, plugin missing).
    SubprocessError,
}

impl GradleFallbackReason {
    pub fn as_annotation_str(&self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::MissingTool => "missing-tool",
            Self::ParseError => "parse-error",
            Self::CacheMiss => "cache-miss",
            Self::NoSourceFiles => "no-source-files",
            Self::OperatorOptOut => "operator-opt-out",
            Self::SubprocessError => "subprocess-error",
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn tier_annotation_strings_match_spec() {
        assert_eq!(GradleResolutionTier::Subprocess.as_annotation_str(), "subprocess");
        assert_eq!(GradleResolutionTier::Cache.as_annotation_str(), "cache");
        assert_eq!(GradleResolutionTier::Static.as_annotation_str(), "static");
        assert_eq!(GradleResolutionTier::LockfileOnly.as_annotation_str(), "lockfile-only");
    }

    #[test]
    fn fallback_reason_annotation_strings_match_spec() {
        assert_eq!(GradleFallbackReason::Timeout.as_annotation_str(), "timeout");
        assert_eq!(GradleFallbackReason::MissingTool.as_annotation_str(), "missing-tool");
        assert_eq!(GradleFallbackReason::ParseError.as_annotation_str(), "parse-error");
        assert_eq!(GradleFallbackReason::CacheMiss.as_annotation_str(), "cache-miss");
        assert_eq!(GradleFallbackReason::NoSourceFiles.as_annotation_str(), "no-source-files");
        assert_eq!(GradleFallbackReason::OperatorOptOut.as_annotation_str(), "operator-opt-out");
        assert_eq!(GradleFallbackReason::SubprocessError.as_annotation_str(), "subprocess-error");
    }

    #[test]
    fn tier_serialize_roundtrip_kebab_case() {
        let s = serde_json::to_string(&GradleResolutionTier::LockfileOnly).unwrap();
        assert_eq!(s, "\"lockfile-only\"");
    }
}
