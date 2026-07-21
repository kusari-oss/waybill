//! Milestone 209: deps.dev hash-lookup resolver.
//!
//! Wraps `mikebom-cli/src/resolve/hash_resolver.rs::HashResolver`
//! per FR-003. Handles ONLY `ResolveInput::Connection` variants
//! whose response carries a `content_hash`. `handles()` also
//! short-circuits on `ctx.skip_online_validation = true` (FR-011
//! preservation — the `--skip-purl-validation` CLI flag threads
//! through here).
//!
//! On successful deps.dev match, emits one `ResolvedComponent` per
//! match with `technique: HashMatch`, `confidence: 0.90`, and the
//! `deps_dev_match` provenance field populated.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use waybill_common::resolution::{
    DepsDevMatch, ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
};

use super::common::collect_connection_hashes;
use crate::resolve::hash_resolver::HashResolver;
use crate::resolve::resolver_trait::{
    ResolveContext, ResolveInput, Resolver, ResolverError,
};

pub(crate) struct DepsDevHashResolver {
    inner: HashResolver,
}

impl DepsDevHashResolver {
    /// Construct with the specified deps.dev API timeout. Matches
    /// the pre-refactor `ResolutionPipeline::new` construction of
    /// `HashResolver::new(config.deps_dev_timeout)`.
    pub(crate) fn new(deps_dev_timeout: Duration) -> Self {
        Self {
            inner: HashResolver::new(deps_dev_timeout),
        }
    }
}

impl Resolver for DepsDevHashResolver {
    fn name(&self) -> &'static str {
        "deps_dev_hash"
    }

    fn priority(&self) -> u32 {
        90
    }

    fn technique(&self) -> ResolutionTechnique {
        ResolutionTechnique::HashMatch
    }

    fn confidence(&self) -> f64 {
        0.90
    }

    fn handles(&self, input: &ResolveInput<'_>, ctx: &ResolveContext<'_>) -> bool {
        if ctx.skip_online_validation {
            return false;
        }
        let ResolveInput::Connection { connection, .. } = input else {
            return false;
        };
        connection
            .response
            .as_ref()
            .and_then(|r| r.content_hash.as_ref())
            .is_some()
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
            let ResolveInput::Connection { connection, .. } = input else {
                return Ok(Vec::new());
            };
            let Some(content_hash) = connection
                .response
                .as_ref()
                .and_then(|r| r.content_hash.as_ref())
            else {
                return Ok(Vec::new());
            };

            let matches = self.inner.resolve(content_hash).await.map_err(|e| {
                ResolverError::Transient {
                    resolver: "deps_dev_hash",
                    source: e,
                }
            })?;

            let components = matches
                .into_iter()
                .map(|m| ResolvedComponent {
                    name: m.name.clone(),
                    version: m.version.clone(),
                    purl: m.purl,
                    evidence: ResolutionEvidence {
                        technique: ResolutionTechnique::HashMatch,
                        confidence: 0.90,
                        source_connection_ids: vec![connection.id.clone()],
                        source_file_paths: vec![],
                        deps_dev_match: Some(DepsDevMatch {
                            system: m.system,
                            name: m.name,
                            version: m.version,
                        }),
                    },
                    licenses: vec![],
                    concluded_licenses: Vec::new(),
                    hashes: collect_connection_hashes(connection),
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
                    extra_annotations: Default::default(),
                    binary_role: None,
                })
                .collect();

            Ok(components)
        })
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::attestation::file::{FileOpType, FileOperation};
    use waybill_common::attestation::network::ProcessRef;
    use waybill_common::types::timestamp::Timestamp;

    #[test]
    fn resolver_metadata_matches_contract() {
        let r = DepsDevHashResolver::new(Duration::from_secs(1));
        assert_eq!(r.name(), "deps_dev_hash");
        assert_eq!(r.priority(), 90);
        assert_eq!(r.technique(), ResolutionTechnique::HashMatch);
        assert!((r.confidence() - 0.90).abs() < f64::EPSILON);
    }

    /// Unit-level FR-011 check: handles() returns false for
    /// FileOp inputs regardless of ctx. The full end-to-end
    /// verification with a real Connection lives in T030's
    /// integration harness.
    #[test]
    fn handles_returns_false_for_file_op_inputs() {
        let r = DepsDevHashResolver::new(Duration::from_secs(1));
        let file_op = FileOperation {
            path: "/tmp/x".into(),
            operation: FileOpType::Read,
            process: ProcessRef {
                pid: 1,
                tid: 1,
                comm: "test".into(),
            },
            content_hash: None,
            size: 0,
            timestamp: Timestamp::now(),
        };
        let input = ResolveInput::FileOp(&file_op);
        let ctx = ResolveContext {
            deb_codename: None,
            skip_online_validation: false,
        };
        assert!(!r.handles(&input, &ctx));
    }
}
