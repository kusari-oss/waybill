//! Milestone 209: Cargo (crates.io) URL-family resolver.
//!
//! Handles registry download URLs from `crates.io` and its CDN
//! `static.crates.io`. Two URL patterns:
//! - `/api/v1/crates/{name}/{version}/download` (API)
//! - `/crates/{name}/{name}-{version}.crate` (CDN)
//!
//! Extracted verbatim from the pre-refactor
//! `mikebom-cli/src/resolve/url_resolver.rs::resolve_cargo` per FR-002.

use std::future::Future;
use std::pin::Pin;

use waybill_common::resolution::{ResolutionTechnique, ResolvedComponent};
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::common::{build_url_component, hostname_and_path};
use crate::resolve::resolver_trait::{
    ResolveContext, ResolveInput, Resolver, ResolverError,
};

pub(crate) struct CargoResolver;

impl Resolver for CargoResolver {
    fn name(&self) -> &'static str {
        "cargo"
    }

    fn priority(&self) -> u32 {
        100
    }

    fn technique(&self) -> ResolutionTechnique {
        ResolutionTechnique::UrlPattern
    }

    fn confidence(&self) -> f64 {
        0.95
    }

    fn handles(&self, input: &ResolveInput<'_>, _ctx: &ResolveContext<'_>) -> bool {
        match input {
            ResolveInput::Connection { connection, .. } => {
                let (hostname, _) = hostname_and_path(connection);
                matches!(hostname, "crates.io" | "static.crates.io")
            }
            ResolveInput::FileOp(_) => false,
        }
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
            let ResolveInput::Connection {
                connection,
                basename_to_file_op,
            } = input
            else {
                return Ok(Vec::new());
            };
            let (hostname, path) = hostname_and_path(connection);
            let Some(purl) = extract_cargo_purl(hostname, path) else {
                return Ok(Vec::new());
            };
            let component = build_url_component(
                purl,
                connection,
                path,
                basename_to_file_op,
                self.technique(),
                self.confidence(),
            );
            Ok(vec![component])
        })
    }
}

/// Extract a cargo PURL from a `(hostname, path)` pair. Returns
/// `None` on non-matching inputs.
fn extract_cargo_purl(hostname: &str, path: &str) -> Option<Purl> {
    match hostname {
        "crates.io" | "static.crates.io" => {}
        _ => return None,
    }

    // /api/v1/crates/{name}/{version}/download
    if let Some(rest) = path.strip_prefix("/api/v1/crates/") {
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() >= 2 {
            let name = parts[0];
            let version = parts[1];
            let purl_str = format!(
                "pkg:cargo/{}@{}",
                encode_purl_segment(name),
                encode_purl_segment(version),
            );
            let purl = Purl::new(&purl_str).ok()?;
            tracing::debug!("cargo URL match: {purl_str}");
            return Some(purl);
        }
    }

    // /crates/{name}/{name}-{version}.crate  (CDN pattern)
    if let Some(rest) = path.strip_prefix("/crates/") {
        let parts: Vec<&str> = rest.splitn(2, '/').collect();
        if parts.len() == 2 {
            let name = parts[0];
            let filename = parts[1];
            if let Some(stem) = filename.strip_suffix(".crate") {
                if let Some(version) =
                    stem.strip_prefix(name).and_then(|s| s.strip_prefix('-'))
                {
                    let purl_str = format!(
                        "pkg:cargo/{}@{}",
                        encode_purl_segment(name),
                        encode_purl_segment(version),
                    );
                    let purl = Purl::new(&purl_str).ok()?;
                    tracing::debug!("cargo CDN URL match: {purl_str}");
                    return Some(purl);
                }
            }
        }
    }

    None
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn extracts_api_v1_pattern() {
        let purl =
            extract_cargo_purl("crates.io", "/api/v1/crates/serde/1.0.219/download")
                .unwrap();
        assert_eq!(purl.as_str(), "pkg:cargo/serde@1.0.219");
    }

    #[test]
    fn extracts_cdn_pattern() {
        let purl = extract_cargo_purl(
            "static.crates.io",
            "/crates/tokio/tokio-1.35.0.crate",
        )
        .unwrap();
        assert_eq!(purl.as_str(), "pkg:cargo/tokio@1.35.0");
    }

    #[test]
    fn rejects_non_cargo_hostname() {
        assert!(extract_cargo_purl("pypi.org", "/api/v1/crates/x/1/download").is_none());
    }

    #[test]
    fn rejects_malformed_path() {
        assert!(extract_cargo_purl("crates.io", "/foo/bar/baz").is_none());
    }

    #[test]
    fn resolver_metadata_matches_contract() {
        let r = CargoResolver;
        assert_eq!(r.name(), "cargo");
        assert_eq!(r.priority(), 100);
        assert_eq!(r.technique(), ResolutionTechnique::UrlPattern);
        assert!((r.confidence() - 0.95).abs() < f64::EPSILON);
    }
}
