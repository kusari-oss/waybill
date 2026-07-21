//! Orchestrate all resolvers in priority order to produce resolved components.
//!
//! Post-milestone-209 refactor: dispatch is delegated to
//! [`super::resolver_chain::ResolverChain`] — this file is now a
//! thin adapter that (a) builds the per-invocation `basename_to_file_op`
//! correlation table, (b) extracts the `deb_codename` from
//! attestation host metadata, (c) iterates the chain over each
//! connection + each file-op, (d) deduplicates the results.
//!
//! The extraction logic that used to live in `url_resolver.rs`,
//! `hash_resolver.rs`, `path_resolver.rs`, and `hostname_resolver.rs`
//! is now split across per-ecosystem + per-technique resolvers under
//! `resolve/resolvers/`. See
//! `specs/209-resolver-trait-chain/plan.md` for the refactor plan.

use std::collections::HashMap;
use std::time::Duration;

use waybill_common::attestation::file::FileOperation;
use waybill_common::attestation::statement::InTotoStatement;
use waybill_common::resolution::ResolvedComponent;

use super::deduplicator::deduplicate;
use super::resolver_chain::ResolverChain;
use super::resolver_trait::{ResolveContext, ResolveInput};

/// Configuration for the resolution pipeline.
#[derive(Clone, Debug)]
pub struct ResolutionConfig {
    /// Timeout for deps.dev API requests.
    pub deps_dev_timeout: Duration,
    /// Skip online API calls (hash resolution via deps.dev).
    pub skip_online_validation: bool,
}

impl Default for ResolutionConfig {
    fn default() -> Self {
        Self {
            deps_dev_timeout: Duration::from_secs(10),
            skip_online_validation: false,
        }
    }
}

/// The resolution pipeline orchestrates the milestone-209 resolver
/// chain against an in-toto attestation.
pub struct ResolutionPipeline {
    config: ResolutionConfig,
    chain: ResolverChain,
}

impl ResolutionPipeline {
    /// Create a new pipeline with the given configuration. Builds
    /// the default resolver chain per `RESOLVER_REGISTRY`.
    pub fn new(config: ResolutionConfig) -> Self {
        let chain = ResolverChain::new_default(config.deps_dev_timeout);
        Self { config, chain }
    }

    /// Resolve all connections and file operations from an
    /// attestation into identified software components.
    ///
    /// Dispatch order preserved from pre-refactor pipeline:
    /// per-connection chain iteration first (URL → hash → hostname),
    /// then per-file-op chain iteration (path resolver), then
    /// deduplication. See `super::resolver_chain::ResolverChain::run`
    /// for the first-match-wins semantics.
    pub async fn resolve(
        &self,
        attestation: &InTotoStatement,
    ) -> anyhow::Result<Vec<ResolvedComponent>> {
        let mut components = Vec::new();

        // deb-codename from attestation host metadata (per
        // pre-refactor pipeline.rs:74-79). Threaded to the DebResolver
        // as `ResolveContext.deb_codename`.
        let deb_codename: Option<&str> = attestation
            .predicate
            .metadata
            .host
            .distro_codename
            .as_deref();

        // Basename → FileOperation correlation table built once per
        // invocation from `file_access.operations`. Consumed by the
        // URL-family resolvers via `ResolveInput::Connection.
        // basename_to_file_op` to attach observed content hashes +
        // source file paths (matches pre-refactor pipeline.rs:89-101).
        let basename_to_file_op: HashMap<&str, &FileOperation> = attestation
            .predicate
            .file_access
            .operations
            .iter()
            .filter_map(|op| {
                let base = std::path::Path::new(&op.path)
                    .file_name()
                    .and_then(|s| s.to_str())?;
                Some((base, op))
            })
            .collect();

        let ctx = ResolveContext {
            deb_codename,
            skip_online_validation: self.config.skip_online_validation,
        };

        // Per-connection chain dispatch. First-match-wins semantic
        // per research R4 preserves pre-refactor behavior.
        for conn in &attestation.predicate.network_trace.connections {
            let input = ResolveInput::Connection {
                connection: conn,
                basename_to_file_op: &basename_to_file_op,
            };
            let mut resolved = self.chain.run(input, &ctx).await;
            components.append(&mut resolved);
        }

        // Per-file-op chain dispatch. Only the PathResolver's
        // handles() returns true for FileOp inputs; other resolvers
        // no-op.
        for file_op in &attestation.predicate.file_access.operations {
            let input = ResolveInput::FileOp(file_op);
            let mut resolved = self.chain.run(input, &ctx).await;
            components.append(&mut resolved);
        }

        // Deduplicate across all resolution techniques.
        let deduped = deduplicate(components);
        Ok(deduped)
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// SC-001 byte-identity smoke test: load the sample attestation
    /// fixture, resolve it through the new chain-based pipeline +
    /// through the preserved-legacy oracle, and assert the resulting
    /// components match exactly.
    #[tokio::test]
    async fn sample_attestation_byte_identity_vs_legacy_oracle() {
        let fixture = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tests/fixtures/sample-attestation.json"
        ))
        .expect("should read sample attestation fixture");

        let attestation: InTotoStatement =
            serde_json::from_str(&fixture).expect("should parse attestation");

        // Both paths use skip_online_validation=true to keep the
        // test hermetic (no deps.dev network calls).
        let config = ResolutionConfig {
            deps_dev_timeout: Duration::from_secs(1),
            skip_online_validation: true,
        };

        // Post-refactor chain path.
        let new_pipeline = ResolutionPipeline::new(config.clone());
        let new_output = new_pipeline
            .resolve(&attestation)
            .await
            .expect("chain resolution should succeed");

        // Pre-refactor legacy oracle path.
        let legacy_pipeline =
            super::super::pipeline_legacy_reference::ResolutionPipeline::new(
                super::super::pipeline_legacy_reference::ResolutionConfig {
                    deps_dev_timeout: config.deps_dev_timeout,
                    skip_online_validation: config.skip_online_validation,
                },
            );
        let legacy_output = legacy_pipeline
            .resolve(&attestation)
            .await
            .expect("legacy oracle resolution should succeed");

        // Assert byte-identity — sorted by PURL for stable comparison.
        let mut new_sorted = new_output.clone();
        new_sorted.sort_by(|a, b| a.purl.as_str().cmp(b.purl.as_str()));
        let mut legacy_sorted = legacy_output.clone();
        legacy_sorted.sort_by(|a, b| a.purl.as_str().cmp(b.purl.as_str()));

        assert_eq!(
            new_sorted.len(),
            legacy_sorted.len(),
            "SC-001 violation: component count differs — new={}, legacy={}. \
             new PURLs: {:?} — legacy PURLs: {:?}",
            new_sorted.len(),
            legacy_sorted.len(),
            new_sorted.iter().map(|c| c.purl.as_str()).collect::<Vec<_>>(),
            legacy_sorted.iter().map(|c| c.purl.as_str()).collect::<Vec<_>>(),
        );

        for (n, l) in new_sorted.iter().zip(legacy_sorted.iter()) {
            assert_eq!(
                n.purl.as_str(),
                l.purl.as_str(),
                "SC-001 violation: PURL diverges"
            );
            assert_eq!(
                n.evidence.technique, l.evidence.technique,
                "SC-005 violation: technique diverges for {}",
                n.purl.as_str()
            );
        }
    }
}
