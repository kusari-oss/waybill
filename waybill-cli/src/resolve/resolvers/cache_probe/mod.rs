//! Milestone 663 — local-cache-probe resolver + per-ecosystem probes.
//!
//! Slots into `RESOLVER_REGISTRY` between the URL-pattern resolvers
//! (94-100) and the deps.dev-hash resolver (90). Runs before deps.dev
//! (which needs network) and produces high-confidence identity for
//! paths under ecosystem-authoritative local cache roots.
//!
//! **Q1 clarification**: metadata-read failure → decline, log warn,
//! next resolver takes over. Never emits at reduced confidence.
//!
//! **Q2 clarification**: the universal `waybill:resolver-tier`
//! per-component annotation is wired at the emit path (see the
//! existing emit-time hook that consults `evidence.technique`).

pub(super) mod cargo;
pub(super) mod golang;
pub(super) mod maven;
pub(super) mod npm;
pub(super) mod pypi;
pub(super) mod rubygems;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

/// Cross-platform home-dir lookup. `$HOME` (POSIX) or `$USERPROFILE`
/// (Windows). Returns `None` on locked-down systems where neither is
/// set; callers fall back to env-var-derived paths only.
pub(super) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

use waybill_common::resolution::{
    ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
};
use waybill_common::types::purl::Purl;

use crate::resolve::resolver_trait::{
    ResolveContext, ResolveInput, Resolver, ResolverError,
};

/// Cache-probe resolver — routes path-shaped inputs through the
/// per-ecosystem probes in dispatch order. First match wins.
pub(crate) struct CacheProbeResolver {
    probes: Vec<EcosystemProbe>,
}

impl CacheProbeResolver {
    pub(crate) fn new() -> Self {
        Self {
            probes: vec![
                EcosystemProbe::Maven,
                EcosystemProbe::Golang,
                EcosystemProbe::Cargo,
                EcosystemProbe::RubyGems,
                EcosystemProbe::NpmPnpm,
                EcosystemProbe::PyPi,
            ],
        }
    }
}

impl Resolver for CacheProbeResolver {
    fn name(&self) -> &'static str {
        "cache_probe"
    }

    fn priority(&self) -> u32 {
        92
    }

    fn technique(&self) -> ResolutionTechnique {
        ResolutionTechnique::LocalCacheHit
    }

    fn confidence(&self) -> f64 {
        0.92
    }

    fn handles(&self, input: &ResolveInput<'_>, _ctx: &ResolveContext<'_>) -> bool {
        matches!(input, ResolveInput::FileOp(_))
    }

    fn resolve<'a>(
        &'a self,
        input: &'a ResolveInput<'a>,
        _ctx: &'a ResolveContext<'a>,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Vec<ResolvedComponent>, ResolverError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let ResolveInput::FileOp(file_op) = input else {
                return Ok(Vec::new());
            };
            let path = Path::new(&file_op.path);
            for probe in &self.probes {
                if let Some(purl) = probe.try_match(path) {
                    let component = ResolvedComponent {
                        name: purl.name().to_string(),
                        version: purl.version().unwrap_or("").to_string(),
                        purl,
                        evidence: ResolutionEvidence {
                            technique: ResolutionTechnique::LocalCacheHit,
                            confidence: 0.92,
                            source_connection_ids: vec![],
                            source_file_paths: vec![file_op.path.clone()],
                            deps_dev_match: None,
                        },
                        licenses: vec![],
                        concluded_licenses: Vec::new(),
                        hashes: file_op
                            .content_hash
                            .as_ref()
                            .cloned()
                            .into_iter()
                            .collect(),
                        supplier: None,
                        cpes: vec![],
                        advisories: vec![],
                        occurrences: vec![],
                        lifecycle_scope: None,
                        build_inclusion: None,
                        requirement_ranges: Vec::new(),
                        source_type: None,
                        sbom_tier: None,
                        buildinfo_status: None,
                        evidence_kind: None,
                        binary_class: None,
                        binary_stripped: None,
                        linkage_kind: None,
                        detected_go: None,
                        confidence: None,
                        binary_packed: None,
                        npm_role: None,
                        raw_version: None,
                        parent_purl: None,
                        co_owned_by: None,
                        shade_relocation: None,
                        external_references: Vec::new(),
                        extra_annotations: {
                            // Milestone 663 (C152): tag every cache-probe-
                            // emitted component with the resolver-tier
                            // annotation. Universal emission across all
                            // resolvers is planned as a follow-on.
                            let mut m: std::collections::BTreeMap<String, serde_json::Value> =
                                Default::default();
                            m.insert(
                                "waybill:resolver-tier".to_string(),
                                serde_json::Value::String("local_cache_hit".to_string()),
                            );
                            m
                        },
                        binary_role: None,
                    };
                    return Ok(vec![component]);
                }
            }
            Ok(Vec::new())
        })
    }
}

/// Per-ecosystem probe. First-match-wins dispatch order locked in
/// `CacheProbeResolver::new()`. Reorder = spec change.
#[derive(Debug, Clone, Copy)]
pub(super) enum EcosystemProbe {
    Maven,
    Golang,
    Cargo,
    RubyGems,
    NpmPnpm,
    PyPi,
}

impl EcosystemProbe {
    pub(super) fn try_match(&self, path: &Path) -> Option<Purl> {
        match self {
            Self::Maven => maven::try_match_maven(path),
            Self::Golang => golang::try_match_golang(path),
            Self::Cargo => cargo::try_match_cargo(path),
            Self::RubyGems => rubygems::try_match_rubygems(path),
            Self::NpmPnpm => npm::try_match_npm_pnpm(path),
            Self::PyPi => pypi::try_match_pypi(path),
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn resolver_metadata_matches_contract() {
        let r = CacheProbeResolver::new();
        assert_eq!(r.name(), "cache_probe");
        assert_eq!(r.priority(), 92);
        assert_eq!(r.technique(), ResolutionTechnique::LocalCacheHit);
        assert!((r.confidence() - 0.92).abs() < f64::EPSILON);
    }
}
