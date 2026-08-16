//! Milestone 235 — Gradle resolution ladder orchestrator.
//!
//! Given a Gradle project directory, tries each ladder tier in order —
//! US1 (subprocess) → US2 (cache) → US3 (static) — and records fallback
//! reasons for tiers that were tried and failed. Returns a
//! `GradleResolvedGraph` naming the winning tier.
//!
//! The lockfile-only fallback (m106) lives in `mod.rs` and runs
//! IN ADDITION to the ladder (per FR-009): whenever a `gradle.lockfile`
//! is present its flat resolved list is emitted unchanged, and the
//! ladder tiers supplement that output with transitive-edge info.
//!
//! MVP (m235 Phase 3 US1 only): `try_subprocess` is implemented;
//! `try_cache` + `try_static` are stubs returning `None` with
//! `NoSourceFiles` / `CacheMiss` reasons. Follow-on milestones fill
//! them in.
//!
//! Some types below (`SubprojectRoot`, `GradleEdge.edge_scope`,
//! `GradleLadderConfig.gradle_resolve_buildscript`, etc.) are
//! scaffolding for future US2/US3/US4 branches.
//! `#[allow(dead_code)]` at the module level suppresses the
//! dead-code lint until those follow-ons wire them.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use super::cache_reader;
use super::static_parser;
use super::subprocess::{self, SubprocessOutcome};
use super::tier::{GradleFallbackReason, GradleResolutionTier};
use crate::scan_fs::package_db::PackageDbEntry;
use waybill_common::types::purl::Purl;

/// A resolved Gradle dependency graph for ONE project directory.
///
/// The `tier` field names which mechanism produced this graph; the
/// `fallback_history` records each tier that was tried and failed
/// BEFORE the winning tier (used by the US4 annotation writer to
/// emit `waybill:gradle-fallback-reason`).
#[derive(Debug, Clone)]
pub struct GradleResolvedGraph {
    pub components: Vec<PackageDbEntry>,
    pub edges: Vec<GradleEdge>,
    pub tier: GradleResolutionTier,
    pub fallback_history: Vec<(GradleResolutionTier, GradleFallbackReason)>,
}

/// A single dependency edge: `source dependsOn target`.
///
/// `edge_scope` maps to CDX `scope` / SPDX 2.3 `RUNTIME/TEST_DEPENDENCY_OF`
/// / SPDX 3 `LifecycleScope` at emission time via the existing
/// milestone-052 / milestone-184 pipeline.
#[derive(Debug, Clone)]
pub struct GradleEdge {
    pub source: Purl,
    pub target: Purl,
    pub edge_scope: EdgeScope,
}

/// Which classpath / configuration this edge belongs to.
///
/// The `From` impl maps to `waybill_common::resolution::LifecycleScope`
/// for CDX/SPDX emission; MVP only produces `Runtime` and `Test`
/// (US1 subprocess resolves runtimeClasspath + testRuntimeClasspath
/// per Clarifications Q1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeScope {
    Runtime,
    Test,
    Buildscript,
    Optional,
}

impl From<EdgeScope> for waybill_common::resolution::LifecycleScope {
    fn from(scope: EdgeScope) -> Self {
        use waybill_common::resolution::LifecycleScope as LS;
        match scope {
            EdgeScope::Runtime => LS::Runtime,
            EdgeScope::Test => LS::Test,
            EdgeScope::Buildscript => LS::Build,
            EdgeScope::Optional => LS::Optional,
        }
    }
}

/// A single Gradle subproject's resolved graph.
///
/// Multi-subproject builds produce one `SubprojectRoot` per included
/// subproject. Single-project builds produce exactly one with
/// `name == ""` (or the root project's declared name).
#[derive(Debug, Clone)]
pub struct SubprojectRoot {
    pub name: String,
    pub path: PathBuf,
    pub graph: GradleResolvedGraph,
}

/// Aggregate summary across every Gradle project touched by a scan.
///
/// Used by the US4 annotation writer to decide between homogeneous
/// tier emission vs `mixed` doc-scope with per-subproject annotations.
/// MVP (Phase 3): US4 not yet wired; this struct is produced but not
/// consumed until US4 lands.
#[derive(Debug, Clone)]
pub struct GradleScanSummary {
    pub subprojects: Vec<SubprojectRoot>,
    /// If `aggregate_mixed == false`, all subprojects have the same
    /// tier and this is that tier. If `true`, subprojects differ and
    /// the annotation writer emits `"mixed"` at doc scope.
    pub aggregate_tier: GradleResolutionTier,
    pub aggregate_mixed: bool,
    /// Sorted, deduplicated `<tier>:<reason>` pairs of tiers that
    /// were TRIED and FAILED across all Gradle projects in this scan
    /// before the winning tier won. Comma-joined for wire emission.
    /// `None` when no tier was tried (e.g., pure `lockfile-only` scan)
    /// or when every attempt succeeded on first try. Drives the
    /// document-scope `waybill:gradle-fallback-reason` annotation
    /// (C147). Pure `operator-opt-out` reasons are excluded — that's
    /// the default no-flag path, not a failure the operator needs
    /// visibility into.
    pub fallback_summary: Option<String>,
}

impl GradleScanSummary {
    pub fn empty() -> Self {
        Self {
            subprojects: Vec::new(),
            aggregate_tier: GradleResolutionTier::LockfileOnly,
            aggregate_mixed: false,
            fallback_summary: None,
        }
    }

    /// Compute aggregate_tier + aggregate_mixed from a Vec of
    /// per-subproject roots. Left public for the US4 emitter.
    pub fn aggregate(mut roots: Vec<SubprojectRoot>) -> Self {
        if roots.is_empty() {
            return Self::empty();
        }
        let first_tier = roots[0].graph.tier;
        let all_same = roots.iter().all(|r| r.graph.tier == first_tier);
        Self {
            aggregate_mixed: !all_same,
            aggregate_tier: first_tier,
            subprojects: std::mem::take(&mut roots),
            fallback_summary: None,
        }
    }
}

/// Per-scan flags passed from the CLI layer to the ladder.
///
/// Mirrors the `GradleCliFlags` clap-derived struct at
/// `waybill-cli/src/cli/scan_cmd.rs` but decoupled so the reader
/// doesn't depend on clap.
#[derive(Debug, Clone, Default)]
pub struct GradleLadderConfig {
    pub gradle_resolve: bool,
    pub gradle_resolve_buildscript: bool,
    pub gradle_daemon: bool,
    pub gradle_timeout_secs: u64,
    pub gradle_extra_configurations: Vec<String>,
}

impl GradleLadderConfig {
    /// Default matches the spec's Q1/Q2/Q3 clarified defaults: US1
    /// is opt-in (false), daemon off (false), 5-min timeout, no
    /// extra configurations.
    pub fn opt_out() -> Self {
        Self {
            gradle_resolve: false,
            gradle_resolve_buildscript: false,
            gradle_daemon: false,
            gradle_timeout_secs: 300,
            gradle_extra_configurations: Vec::new(),
        }
    }
}

/// Try the US1 subprocess tier for one project directory.
///
/// `Ok(graph)` on success. `Err(reason)` when this tier declined or
/// failed — the caller records the reason in the fallback history
/// and moves to the next tier.
pub(super) fn try_subprocess(
    project_dir: &Path,
    config: &GradleLadderConfig,
) -> Result<GradleResolvedGraph, GradleFallbackReason> {
    if !config.gradle_resolve {
        return Err(GradleFallbackReason::OperatorOptOut);
    }
    match subprocess::resolve_via_subprocess(project_dir, config) {
        Ok(graph) => Ok(graph),
        Err(outcome) => {
            let reason = match outcome {
                SubprocessOutcome::Timeout => GradleFallbackReason::Timeout,
                SubprocessOutcome::ToolMissing => GradleFallbackReason::MissingTool,
                SubprocessOutcome::ParseError { .. } => GradleFallbackReason::ParseError,
                SubprocessOutcome::NonZeroExit { .. } => GradleFallbackReason::SubprocessError,
            };
            tracing::warn!(
                target: "waybill::gradle",
                "US1 subprocess resolution failed at {} — reason={} — degrading to next tier",
                project_dir.display(),
                reason.as_annotation_str()
            );
            Err(reason)
        }
    }
}

/// Try the US2 cache tier for one project directory.
///
/// `Ok(graph)` when the Gradle cache is warm for this project's
/// declared coords; `Err(reason)` when the cache is absent, empty,
/// or missing all declared coords.
pub(super) fn try_cache(
    project_dir: &Path,
) -> Result<GradleResolvedGraph, GradleFallbackReason> {
    match cache_reader::resolve_via_cache(project_dir) {
        Ok(graph) => Ok(graph),
        Err(err) => {
            let reason = match err {
                cache_reader::GradleCacheError::CacheAbsent => {
                    GradleFallbackReason::MissingTool
                }
                cache_reader::GradleCacheError::NoSeedsHit => {
                    GradleFallbackReason::CacheMiss
                }
            };
            tracing::debug!(
                target: "waybill::gradle",
                "US2 cache resolution failed at {} — reason={} — degrading to next tier",
                project_dir.display(),
                reason.as_annotation_str()
            );
            Err(reason)
        }
    }
}

/// Ladder entry point — attempts US1 (subprocess) → US2 (cache) →
/// US3 (static, stub).
///
/// When all tiers fail to produce components, returns an empty
/// `LockfileOnly` graph with the fallback history recorded — the
/// caller (`mod.rs::read`) then relies on m106 lockfile output (if
/// any) to populate the SBOM.
pub fn resolve(
    project_dir: &Path,
    config: &GradleLadderConfig,
) -> GradleResolvedGraph {
    let mut history: Vec<(GradleResolutionTier, GradleFallbackReason)> = Vec::new();

    // US1 subprocess (m235 Phase 3).
    match try_subprocess(project_dir, config) {
        Ok(graph) => return graph,
        Err(reason) => history.push((GradleResolutionTier::Subprocess, reason)),
    }

    // US2 cache (m235 Phase 4).
    match try_cache(project_dir) {
        Ok(mut graph) => {
            graph.fallback_history = history;
            return graph;
        }
        Err(reason) => history.push((GradleResolutionTier::Cache, reason)),
    }

    // US3 static parser (m235 Phase 5).
    match try_static(project_dir) {
        Ok(mut graph) => {
            graph.fallback_history = history;
            return graph;
        }
        Err(reason) => history.push((GradleResolutionTier::Static, reason)),
    }

    GradleResolvedGraph {
        components: Vec::new(),
        edges: Vec::new(),
        tier: GradleResolutionTier::LockfileOnly,
        fallback_history: history,
    }
}

/// Try the US3 static-parser tier for one project directory.
///
/// `Ok(graph)` when at least one `build.gradle(.kts)` was parsed;
/// `Err(NoSourceFiles)` when no build files exist in the project
/// tree (ladder degrades to lockfile-only).
pub(super) fn try_static(
    project_dir: &Path,
) -> Result<GradleResolvedGraph, GradleFallbackReason> {
    match static_parser::resolve_via_static_parse(project_dir) {
        Ok(graph) => Ok(graph),
        Err(err) => {
            let reason = match err {
                static_parser::GradleStaticError::NoSourceFiles => {
                    GradleFallbackReason::NoSourceFiles
                }
            };
            tracing::debug!(
                target: "waybill::gradle",
                "US3 static parse failed at {} — reason={} — degrading to next tier",
                project_dir.display(),
                reason.as_annotation_str()
            );
            Err(reason)
        }
    }
}
