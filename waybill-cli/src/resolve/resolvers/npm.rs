//! Milestone 209: npm URL-family resolver.
//!
//! Handles registry download URLs from `registry.npmjs.org`. Two URL
//! patterns:
//! - `/{name}/-/{name}-{version}.tgz` (unscoped)
//! - `/{@scope}/{name}/-/{name}-{version}.tgz` (scoped)
//!
//! Extracted verbatim from the pre-refactor
//! `waybill-cli/src/resolve/url_resolver.rs::resolve_npm` per FR-002.

use std::future::Future;
use std::pin::Pin;

use waybill_common::resolution::{ResolutionTechnique, ResolvedComponent};
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::common::{build_url_component, hostname_and_path};
use crate::resolve::resolver_trait::{
    ResolveContext, ResolveInput, Resolver, ResolverError,
};

pub(crate) struct NpmResolver;

impl Resolver for NpmResolver {
    fn name(&self) -> &'static str {
        "npm"
    }

    fn priority(&self) -> u32 {
        98
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
                hostname == "registry.npmjs.org"
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
            let Some(purl) = extract_npm_purl(hostname, path) else {
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

/// Extract an npm PURL from a `(hostname, path)` pair. Returns
/// `None` on non-matching inputs.
fn extract_npm_purl(hostname: &str, path: &str) -> Option<Purl> {
    if hostname != "registry.npmjs.org" {
        return None;
    }

    // Remove leading slash.
    let path = path.strip_prefix('/')?;

    // Check for scoped package: @scope/name/-/name-version.tgz
    if path.starts_with('@') {
        let parts: Vec<&str> = path.splitn(4, '/').collect();
        // parts: ["@scope", "name", "-", "name-version.tgz"]
        if parts.len() == 4 && parts[2] == "-" {
            let scope = parts[0]; // includes '@'
            let name = parts[1];
            let filename = parts[3];
            let version = extract_npm_version(filename, name)?;
            // PURL spec: scope is percent-encoded '@' → '%40'
            let encoded_scope = scope.replace('@', "%40");
            // purl-spec § Character encoding: name + version are
            // percent-encoded strings. Scope keeps its `%40<scope>/`
            // literal form so consumers see the npm canonical shape.
            let purl_str = format!(
                "pkg:npm/{encoded_scope}/{}@{}",
                encode_purl_segment(name),
                encode_purl_segment(version),
            );
            let purl = Purl::new(&purl_str).ok()?;
            tracing::debug!("npm scoped URL match: {purl_str}");
            return Some(purl);
        }
    }

    // Unscoped: name/-/name-version.tgz
    let parts: Vec<&str> = path.splitn(3, '/').collect();
    if parts.len() == 3 && parts[1] == "-" {
        let name = parts[0];
        let filename = parts[2];
        let version = extract_npm_version(filename, name)?;
        let purl_str = format!(
            "pkg:npm/{}@{}",
            encode_purl_segment(name),
            encode_purl_segment(version),
        );
        let purl = Purl::new(&purl_str).ok()?;
        tracing::debug!("npm URL match: {purl_str}");
        return Some(purl);
    }

    None
}

/// Extract version from npm tarball filename: "{name}-{version}.tgz"
fn extract_npm_version<'a>(filename: &'a str, name: &str) -> Option<&'a str> {
    let stem = filename.strip_suffix(".tgz")?;
    let version = stem.strip_prefix(name)?.strip_prefix('-')?;
    Some(version)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn extracts_unscoped_pattern() {
        let purl =
            extract_npm_purl("registry.npmjs.org", "/lodash/-/lodash-4.17.21.tgz")
                .unwrap();
        assert_eq!(purl.ecosystem(), "npm");
        assert_eq!(purl.name(), "lodash");
        assert_eq!(purl.version(), Some("4.17.21"));
    }

    #[test]
    fn extracts_scoped_pattern() {
        let purl =
            extract_npm_purl("registry.npmjs.org", "/@angular/core/-/core-16.0.0.tgz")
                .unwrap();
        assert_eq!(purl.ecosystem(), "npm");
        assert_eq!(purl.namespace(), Some("@angular"));
        assert_eq!(purl.name(), "core");
        assert_eq!(purl.version(), Some("16.0.0"));
    }

    #[test]
    fn rejects_non_npm_hostname() {
        assert!(
            extract_npm_purl("registry.yarnpkg.com", "/lodash/-/lodash-4.17.21.tgz")
                .is_none()
        );
    }

    #[test]
    fn rejects_malformed_path() {
        assert!(extract_npm_purl("registry.npmjs.org", "/some/bad/path").is_none());
    }

    #[test]
    fn resolver_metadata_matches_contract() {
        let r = NpmResolver;
        assert_eq!(r.name(), "npm");
        assert_eq!(r.priority(), 98);
        assert_eq!(r.technique(), ResolutionTechnique::UrlPattern);
        assert!((r.confidence() - 0.95).abs() < f64::EPSILON);
    }
}
