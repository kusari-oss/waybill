//! Milestone 209: Go modules URL-family resolver.
//!
//! Handles registry download URLs from `proxy.golang.org` and
//! `sum.golang.org`. Matches `/{module}/@v/{version}.(zip|mod|info|ziphash)`.
//!
//! Extracted verbatim from the pre-refactor
//! `waybill-cli/src/resolve/url_resolver.rs::resolve_golang` per FR-002.

use std::future::Future;
use std::pin::Pin;

use waybill_common::resolution::{ResolutionTechnique, ResolvedComponent};
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::common::{build_url_component, hostname_and_path};
use crate::resolve::resolver_trait::{
    ResolveContext, ResolveInput, Resolver, ResolverError,
};

pub(crate) struct GolangResolver;

impl Resolver for GolangResolver {
    fn name(&self) -> &'static str {
        "golang"
    }

    fn priority(&self) -> u32 {
        97
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
                matches!(hostname, "proxy.golang.org" | "sum.golang.org")
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
            let Some(purl) = extract_golang_purl(hostname, path) else {
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

/// Extract a Go module PURL from a `(hostname, path)` pair. Returns
/// `None` on non-matching inputs.
fn extract_golang_purl(hostname: &str, path: &str) -> Option<Purl> {
    if hostname != "proxy.golang.org" && hostname != "sum.golang.org" {
        return None;
    }

    // Find "/@v/" separator.
    let at_v_idx = path.find("/@v/")?;
    let module = path[1..at_v_idx].to_string(); // strip leading '/'
    let version_file = &path[at_v_idx + 4..]; // after "/@v/"

    // Strip known extensions.
    let version = version_file
        .strip_suffix(".zip")
        .or_else(|| version_file.strip_suffix(".mod"))
        .or_else(|| version_file.strip_suffix(".info"))
        .or_else(|| version_file.strip_suffix(".ziphash"))?;

    if module.is_empty() || version.is_empty() {
        return None;
    }

    // purl-spec: Go versions like `v1.2.3+incompatible` must encode.
    let purl_str = format!(
        "pkg:golang/{}@{}",
        encode_purl_segment(&module),
        encode_purl_segment(version),
    );
    let purl = Purl::new(&purl_str).ok()?;
    tracing::debug!("golang URL match: {purl_str}");
    Some(purl)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn extracts_zip_pattern() {
        let purl =
            extract_golang_purl("proxy.golang.org", "/golang.org/x/net/@v/v0.24.0.zip")
                .unwrap();
        assert_eq!(purl.ecosystem(), "golang");
        assert_eq!(purl.version(), Some("v0.24.0"));
    }

    #[test]
    fn extracts_mod_pattern() {
        let purl = extract_golang_purl(
            "proxy.golang.org",
            "/github.com/stretchr/testify/@v/v1.9.0.mod",
        )
        .unwrap();
        assert_eq!(purl.ecosystem(), "golang");
        assert_eq!(purl.version(), Some("v1.9.0"));
    }

    #[test]
    fn rejects_non_golang_hostname() {
        assert!(
            extract_golang_purl("crates.io", "/golang.org/x/net/@v/v0.24.0.zip")
                .is_none()
        );
    }

    #[test]
    fn rejects_malformed_path() {
        assert!(extract_golang_purl("proxy.golang.org", "/no/at-v/separator").is_none());
    }

    #[test]
    fn resolver_metadata_matches_contract() {
        let r = GolangResolver;
        assert_eq!(r.name(), "golang");
        assert_eq!(r.priority(), 97);
        assert_eq!(r.technique(), ResolutionTechnique::UrlPattern);
        assert!((r.confidence() - 0.95).abs() < f64::EPSILON);
    }
}
