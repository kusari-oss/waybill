//! Milestone 209: hostname-fallback resolver.
//!
//! Last-resort ecosystem tagging: when no URL-family or hash
//! resolver matches, the hostname alone can hint at the ecosystem
//! (e.g., `pypi.org` → pypi). The pre-refactor pipeline logs the
//! ecosystem hint but does NOT emit a component because we lack
//! name + version (see `pipeline.rs:262-272`). To preserve
//! SC-001 byte-identity, this resolver mirrors that: it logs at
//! `debug!` when a hostname matches a known ecosystem, but always
//! returns `Ok(vec![])`.
//!
//! Wraps `waybill-cli/src/resolve/hostname_resolver.rs::resolve_hostname`
//! per FR-005.

use std::future::Future;
use std::pin::Pin;

use waybill_common::resolution::{ResolutionTechnique, ResolvedComponent};

use super::common::hostname_and_path;
use crate::resolve::hostname_resolver::resolve_hostname;
use crate::resolve::resolver_trait::{
    ResolveContext, ResolveInput, Resolver, ResolverError,
};

pub(crate) struct HostnameFallbackResolver;

impl Resolver for HostnameFallbackResolver {
    fn name(&self) -> &'static str {
        "hostname_fallback"
    }

    fn priority(&self) -> u32 {
        40
    }

    fn technique(&self) -> ResolutionTechnique {
        ResolutionTechnique::HostnameHeuristic
    }

    fn confidence(&self) -> f64 {
        0.40
    }

    fn handles(&self, input: &ResolveInput<'_>, _ctx: &ResolveContext<'_>) -> bool {
        matches!(input, ResolveInput::Connection { .. })
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
            let (hostname, _) = hostname_and_path(connection);
            if hostname.is_empty() {
                return Ok(Vec::new());
            }
            if let Some(ecosystem) = resolve_hostname(hostname) {
                tracing::debug!(
                    "hostname heuristic for {}: ecosystem={ecosystem} \
                     (no PURL created, insufficient info)",
                    connection.id,
                );
            }
            // Pre-refactor behavior: never emit a component (name +
            // version unknown from hostname alone). Preserved verbatim
            // per SC-001.
            Ok(Vec::new())
        })
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn resolver_metadata_matches_contract() {
        let r = HostnameFallbackResolver;
        assert_eq!(r.name(), "hostname_fallback");
        assert_eq!(r.priority(), 40);
        assert_eq!(r.technique(), ResolutionTechnique::HostnameHeuristic);
        assert!((r.confidence() - 0.40).abs() < f64::EPSILON);
    }
}
