//! Milestone 209: file-path resolver.
//!
//! Wraps `waybill-cli/src/resolve/path_resolver.rs::resolve_path_with_context`
//! per FR-004. Handles ONLY `ResolveInput::FileOp` variants — the
//! path resolver operates on file-access events, not connection
//! events (matches pre-refactor pipeline.rs:275-325 semantics).
//!
//! Constructs a `ResolvedComponent` from every matching path with
//! `technique: FilePathPattern`, `confidence: 0.70`, and the file
//! op's content hash attached when present.

use std::future::Future;
use std::pin::Pin;

use waybill_common::resolution::{
    ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
};

use crate::resolve::path_resolver::resolve_path_with_context;
use crate::resolve::resolver_trait::{
    ResolveContext, ResolveInput, Resolver, ResolverError,
};

pub(crate) struct PathResolver;

impl Resolver for PathResolver {
    fn name(&self) -> &'static str {
        "path"
    }

    fn priority(&self) -> u32 {
        70
    }

    fn technique(&self) -> ResolutionTechnique {
        ResolutionTechnique::FilePathPattern
    }

    fn confidence(&self) -> f64 {
        0.70
    }

    fn handles(&self, input: &ResolveInput<'_>, _ctx: &ResolveContext<'_>) -> bool {
        // Pre-refactor pipeline runs path resolution ONLY over
        // file-access events (pipeline.rs:276). Connection inputs
        // are not path-resolved. Preserve that boundary.
        matches!(input, ResolveInput::FileOp(_))
    }

    fn resolve<'a>(
        &'a self,
        input: &'a ResolveInput<'a>,
        ctx: &'a ResolveContext<'a>,
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
            let Some(purl) = resolve_path_with_context(&file_op.path, ctx.deb_codename)
            else {
                return Ok(Vec::new());
            };
            let component = ResolvedComponent {
                name: purl.name().to_string(),
                version: purl.version().unwrap_or("").to_string(),
                purl,
                evidence: ResolutionEvidence {
                    technique: ResolutionTechnique::FilePathPattern,
                    confidence: 0.70,
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
                extra_annotations: Default::default(),
                binary_role: None,
            };
            Ok(vec![component])
        })
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn resolver_metadata_matches_contract() {
        let r = PathResolver;
        assert_eq!(r.name(), "path");
        assert_eq!(r.priority(), 70);
        assert_eq!(r.technique(), ResolutionTechnique::FilePathPattern);
        assert!((r.confidence() - 0.70).abs() < f64::EPSILON);
    }
}
