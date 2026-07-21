//! Milestone 209: RubyGems URL-family resolver.
//!
//! Handles registry download URLs from `rubygems.org`. Matches
//! `/downloads/{name}-{version}.gem` and `/gems/{name}-{version}.gem`.
//!
//! Extracted verbatim from the pre-refactor
//! `waybill-cli/src/resolve/url_resolver.rs::resolve_rubygems` per FR-002.

use std::future::Future;
use std::pin::Pin;

use waybill_common::resolution::{ResolutionTechnique, ResolvedComponent};
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::common::{build_url_component, hostname_and_path};
use crate::resolve::resolver_trait::{
    ResolveContext, ResolveInput, Resolver, ResolverError,
};

pub(crate) struct RubyGemsResolver;

impl Resolver for RubyGemsResolver {
    fn name(&self) -> &'static str {
        "rubygems"
    }

    fn priority(&self) -> u32 {
        95
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
                hostname == "rubygems.org"
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
            let Some(purl) = extract_rubygems_purl(hostname, path) else {
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

/// Extract a RubyGems PURL from a `(hostname, path)` pair. Returns
/// `None` on non-matching inputs.
fn extract_rubygems_purl(hostname: &str, path: &str) -> Option<Purl> {
    if hostname != "rubygems.org" {
        return None;
    }

    let filename = path
        .strip_prefix("/downloads/")
        .or_else(|| path.strip_prefix("/gems/"))?;

    let stem = filename.strip_suffix(".gem")?;

    // The version starts after the last '-' that is followed by a digit.
    let (name, version) = split_gem_name_version(stem)?;

    let purl_str = format!(
        "pkg:gem/{}@{}",
        encode_purl_segment(name),
        encode_purl_segment(version),
    );
    let purl = Purl::new(&purl_str).ok()?;
    tracing::debug!("rubygems URL match: {purl_str}");
    Some(purl)
}

/// Split a gem filename stem into (name, version).
/// The version starts at the last '-' followed by a digit.
fn split_gem_name_version(stem: &str) -> Option<(&str, &str)> {
    let bytes = stem.as_bytes();
    // Search from the end for the last '-' followed by a digit.
    for i in (0..bytes.len()).rev() {
        if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            return Some((&stem[..i], &stem[i + 1..]));
        }
    }
    None
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn extracts_downloads_pattern() {
        let purl =
            extract_rubygems_purl("rubygems.org", "/downloads/rails-7.1.3.gem").unwrap();
        assert_eq!(purl.ecosystem(), "gem");
        assert_eq!(purl.name(), "rails");
        assert_eq!(purl.version(), Some("7.1.3"));
    }

    #[test]
    fn extracts_gems_pattern() {
        let purl =
            extract_rubygems_purl("rubygems.org", "/gems/nokogiri-1.16.5.gem").unwrap();
        assert_eq!(purl.ecosystem(), "gem");
        assert_eq!(purl.name(), "nokogiri");
        assert_eq!(purl.version(), Some("1.16.5"));
    }

    #[test]
    fn rejects_non_rubygems_hostname() {
        assert!(extract_rubygems_purl("crates.io", "/gems/rails-7.1.3.gem").is_none());
    }

    #[test]
    fn rejects_malformed_path() {
        assert!(extract_rubygems_purl("rubygems.org", "/api/rails.gem").is_none());
    }

    #[test]
    fn resolver_metadata_matches_contract() {
        let r = RubyGemsResolver;
        assert_eq!(r.name(), "rubygems");
        assert_eq!(r.priority(), 95);
        assert_eq!(r.technique(), ResolutionTechnique::UrlPattern);
        assert!((r.confidence() - 0.95).abs() < f64::EPSILON);
    }
}
