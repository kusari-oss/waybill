//! Milestone 209 (#601): the `Resolver` trait every ecosystem or
//! technique resolver implements. Plus its supporting types —
//! [`ResolveInput`] (the input event-type discriminant),
//! [`ResolveContext`] (read-only pipeline-wide context), and
//! [`ResolverError`] (the non-panicking failure surface returned
//! from `resolve`).
//!
//! Public API contract locked in
//! `specs/209-resolver-trait-chain/contracts/resolver-trait.md` (C-1
//! + C-2). Data-model + validation rules at
//! `specs/209-resolver-trait-chain/data-model.md` (E1..E4).
//!
//! Design decisions:
//! - **R1 (revised)**: `resolve` returns `Pin<Box<dyn Future + Send + 'a>>`
//!   rather than RPITIT `impl Future`. This is required for
//!   trait-object dispatch (`Box<dyn Resolver>` in `ResolverChain`
//!   per data-model E5), which stable-Rust RPITIT doesn't currently
//!   support (RPITIT traits aren't object-safe). Cost: one boxed
//!   allocation per `resolve` invocation (~50 ns) — well under the
//!   SC-004 5% perf ceiling. No new Cargo deps (avoids `async-trait`).
//! - **Q1**: `resolve` returns `Result<Vec<ResolvedComponent>, ResolverError>`
//!   — `Ok(vec![])` denotes clean no-match, `Err(...)` denotes
//!   transient/internal failure.
//! - **FR-013**: pipeline catches both `Err` and panics at dispatch;
//!   individual resolver failures do NOT abort the pipeline.
//! - **FR-014**: resolvers are stateless — no mutable fields, no
//!   interior mutability.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use mikebom_common::attestation::file::FileOperation;
use mikebom_common::attestation::network::Connection;
use mikebom_common::resolution::{ResolutionTechnique, ResolvedComponent};
use mikebom_common::types::hash::ContentHash;

/// The non-panicking failure surface a resolver returns when its
/// `resolve` call cannot complete cleanly. See data-model E4.
///
/// The pipeline layer catches every `Err` variant, logs a WARN
/// naming the resolver + the variant, and continues to the next
/// resolver in the chain (per FR-013). Panics are also caught
/// and treated identically — a single resolver's failure NEVER
/// aborts the pipeline.
#[derive(Debug, thiserror::Error)]
pub enum ResolverError {
    /// A transient network error occurred (deps.dev timeout, TCP
    /// reset, etc.). The pipeline logs WARN + continues.
    #[error("resolver `{resolver}` hit a transient network error: {source}")]
    Transient {
        resolver: &'static str,
        #[source]
        source: anyhow::Error,
    },

    /// The resolver's internal invariant was violated by the input
    /// (malformed URL, unexpected hash algorithm, etc.). The
    /// pipeline logs WARN + continues.
    #[error("resolver `{resolver}` rejected input as malformed: {reason}")]
    MalformedInput {
        resolver: &'static str,
        reason: String,
    },

    /// The resolver's dependency was unavailable (deps.dev
    /// unreachable). The pipeline logs WARN + continues; the
    /// operator sees enough context to know online-mode is degraded.
    #[error("resolver `{resolver}` dependency unavailable: {reason}")]
    Unavailable {
        resolver: &'static str,
        reason: String,
    },
}

/// Input discriminant passed to `Resolver::resolve`. See data-model E2.
///
/// The pipeline iterates over both connection events + file-operation
/// events, dispatching each to the full chain. Each resolver's
/// `handles()` method filters to the variants it cares about; the
/// pipeline skips `.await`-invocation for non-matching variants.
pub enum ResolveInput<'a> {
    /// A traced network connection. URL-family resolvers + the
    /// hash resolver + the hostname-fallback resolver consume this.
    Connection {
        connection: &'a Connection,
        /// Basename-to-content-hash correlation table built once
        /// per pipeline invocation from file-operation events.
        /// Passed by reference so resolvers don't need to rebuild it.
        basename_to_hash: &'a HashMap<&'a str, &'a ContentHash>,
    },
    /// A traced file-operation event. The path resolver consumes
    /// this (only ecosystem-neutral file-path resolution — the
    /// URL-family resolvers only see `Connection` inputs).
    FileOp(&'a FileOperation),
}

/// Read-only pipeline-wide context passed to every resolver. See
/// data-model E3.
pub struct ResolveContext<'a> {
    /// Debian distro codename sampled from `/etc/os-release`.
    /// Threaded to the Deb resolver as the PURL's `distro`
    /// qualifier; other resolvers ignore it. `None` on non-Debian
    /// hosts.
    pub deb_codename: Option<&'a str>,

    /// Whether the operator passed `--skip-purl-validation`.
    /// Consumed by the deps.dev-hash resolver's `handles()` — when
    /// true, that resolver returns `false` from `handles()` and is
    /// silently skipped for every input, preserving FR-011.
    pub skip_online_validation: bool,
}

/// The common shape every ecosystem or technique resolver
/// implements. See data-model E1 + contract C-1.
///
/// Signature is LOCKED per contract C-1 — adding a required method
/// is a breaking change; new capabilities MUST be added as
/// default-implemented methods to preserve trait-object
/// compatibility with `Box<dyn Resolver>` in `ResolverChain`.
///
/// The `resolve` method returns a boxed future (`Pin<Box<dyn Future
/// + Send>>`) rather than RPITIT `impl Future` because trait objects
/// require object-safety, which RPITIT traits don't have on stable
/// Rust today. Implementations typically wrap their async body in
/// `Box::pin(async move { ... })`.
pub trait Resolver: Send + Sync {
    /// Stable identifier for logging + panic diagnostics.
    /// Snake-case; matches the entry in `RESOLVER_REGISTRY` at
    /// `resolver_chain.rs`.
    fn name(&self) -> &'static str;

    /// Priority for chain ordering. Higher = runs earlier. MUST
    /// be unique across all registered resolvers per FR-017
    /// (enforced at compile time via `RESOLVER_REGISTRY` const
    /// check).
    fn priority(&self) -> u32;

    /// Which technique this resolver reports via
    /// `ResolutionEvidence.technique` on emitted components.
    /// Preserves SC-005's downstream signal.
    fn technique(&self) -> ResolutionTechnique;

    /// Confidence attached to every component this resolver emits.
    fn confidence(&self) -> f32;

    /// Cheap filter — returns `true` if this resolver applies to
    /// the given input type / shape. Called before each
    /// `.await`-invocation of `resolve` to skip clearly-inapplicable
    /// resolvers. Sync + O(1) or O(few).
    fn handles(&self, input: &ResolveInput<'_>, ctx: &ResolveContext<'_>) -> bool;

    /// The actual resolution logic. The boxed future dispatch
    /// preserves trait-object compatibility for `Box<dyn Resolver>`
    /// in `ResolverChain`.
    ///
    /// Per Q1 clarification: `Ok(Vec::new())` denotes a clean
    /// no-match (chain continues to the next resolver);
    /// `Err(...)` denotes a transient / internal failure (pipeline
    /// logs WARN + continues to the next resolver per FR-013).
    fn resolve<'a>(
        &'a self,
        input: &'a ResolveInput<'a>,
        ctx: &'a ResolveContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ResolvedComponent>, ResolverError>> + Send + 'a>>;
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use mikebom_common::resolution::ResolutionTechnique;

    /// Trivial `Resolver` impl used to smoke-test the trait signature.
    struct TestResolver;

    impl Resolver for TestResolver {
        fn name(&self) -> &'static str {
            "test"
        }

        fn priority(&self) -> u32 {
            50
        }

        fn technique(&self) -> ResolutionTechnique {
            ResolutionTechnique::UrlPattern
        }

        fn confidence(&self) -> f32 {
            0.5
        }

        fn handles(
            &self,
            _input: &ResolveInput<'_>,
            _ctx: &ResolveContext<'_>,
        ) -> bool {
            true
        }

        fn resolve<'a>(
            &'a self,
            _input: &'a ResolveInput<'a>,
            _ctx: &'a ResolveContext<'a>,
        ) -> Pin<
            Box<dyn Future<Output = Result<Vec<ResolvedComponent>, ResolverError>> + Send + 'a>,
        > {
            Box::pin(async move { Ok(Vec::new()) })
        }
    }

    #[test]
    fn test_resolver_metadata_accessors() {
        let r = TestResolver;
        assert_eq!(r.name(), "test");
        assert_eq!(r.priority(), 50);
        assert_eq!(r.technique(), ResolutionTechnique::UrlPattern);
        assert!((r.confidence() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_resolver_is_object_safe() {
        // Compile-time check: `Box<dyn Resolver>` MUST work — this
        // is the whole point of the Pin<Box<dyn Future>>-return
        // choice over RPITIT.
        let boxed: Box<dyn Resolver> = Box::new(TestResolver);
        assert_eq!(boxed.name(), "test");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_resolver_resolve_returns_ok_empty() {
        let r = TestResolver;
        let ctx = ResolveContext {
            deb_codename: None,
            skip_online_validation: false,
        };
        let file_op = placeholder_file_op();
        let input = ResolveInput::FileOp(&file_op);
        let result = r.resolve(&input, &ctx).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    fn placeholder_file_op() -> FileOperation {
        use mikebom_common::attestation::network::ProcessRef;
        use mikebom_common::types::timestamp::Timestamp;

        FileOperation {
            path: "/tmp/test".to_string(),
            operation: mikebom_common::attestation::file::FileOpType::Read,
            process: ProcessRef {
                pid: 1,
                tid: 1,
                comm: "test".to_string(),
            },
            content_hash: None,
            size: 0,
            timestamp: Timestamp::now(),
        }
    }

    #[test]
    fn resolver_error_transient_display() {
        let e = ResolverError::Transient {
            resolver: "test",
            source: anyhow::anyhow!("connection reset"),
        };
        let s = format!("{e}");
        assert!(s.contains("test"), "{s}");
        assert!(s.contains("connection reset"), "{s}");
    }

    #[test]
    fn resolver_error_malformed_input_display() {
        let e = ResolverError::MalformedInput {
            resolver: "cargo",
            reason: "expected /api/v1/crates/... prefix".to_string(),
        };
        let s = format!("{e}");
        assert!(s.contains("cargo"), "{s}");
        assert!(s.contains("prefix"), "{s}");
    }

    #[test]
    fn resolver_error_unavailable_display() {
        let e = ResolverError::Unavailable {
            resolver: "deps_dev_hash",
            reason: "api.deps.dev unreachable".to_string(),
        };
        let s = format!("{e}");
        assert!(s.contains("deps_dev_hash"), "{s}");
        assert!(s.contains("unreachable"), "{s}");
    }
}
