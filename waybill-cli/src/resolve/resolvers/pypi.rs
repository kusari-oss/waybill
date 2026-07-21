//! Milestone 209: PyPI URL-family resolver.
//!
//! Handles registry download URLs from `pypi.org` and its CDN
//! `files.pythonhosted.org`. Matches `/packages/{hash_prefix}/{hash}/
//! {name}-{version}.(tar.gz|whl|zip|tar.bz2)`.
//!
//! Extracted verbatim from the pre-refactor
//! `mikebom-cli/src/resolve/url_resolver.rs::resolve_pypi` per FR-002.

use std::future::Future;
use std::pin::Pin;

use waybill_common::resolution::{ResolutionTechnique, ResolvedComponent};
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::common::{build_url_component, hostname_and_path};
use crate::resolve::resolver_trait::{
    ResolveContext, ResolveInput, Resolver, ResolverError,
};

pub(crate) struct PypiResolver;

impl Resolver for PypiResolver {
    fn name(&self) -> &'static str {
        "pypi"
    }

    fn priority(&self) -> u32 {
        99
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
                matches!(hostname, "pypi.org" | "files.pythonhosted.org")
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
            let Some(purl) = extract_pypi_purl(hostname, path) else {
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

/// Extract a PyPI PURL from a `(hostname, path)` pair. Returns
/// `None` on non-matching inputs.
fn extract_pypi_purl(hostname: &str, path: &str) -> Option<Purl> {
    match hostname {
        "pypi.org" | "files.pythonhosted.org" => {}
        _ => return None,
    }

    if !path.starts_with("/packages/") {
        return None;
    }

    // Extract the filename from the last path segment.
    let filename = path.rsplit('/').next()?;

    // Strip known extensions to get "{name}-{version}" stem.
    let stem = strip_pypi_extension(filename)?;

    // Split on the last '-' that separates name from version.
    // PyPI filenames: {distribution}-{version}(-{build})?(-{python}(-{abi}(-{platform})))?.whl
    // For .tar.gz: {name}-{version}.tar.gz
    // We split on '-' and try to find where the version starts (first segment starting with digit).
    let (name, version) = split_pypi_name_version(stem)?;

    // Normalize: PEP 503 — replace hyphens/dots with underscores, lowercase.
    let normalized_name = name.replace(['-', '.'], "_").to_lowercase();

    let purl_str = format!(
        "pkg:pypi/{}@{}",
        encode_purl_segment(&normalized_name),
        encode_purl_segment(version),
    );
    let purl = Purl::new(&purl_str).ok()?;
    tracing::debug!("pypi URL match: {purl_str}");
    Some(purl)
}

fn strip_pypi_extension(filename: &str) -> Option<&str> {
    if let Some(s) = filename.strip_suffix(".tar.gz") {
        return Some(s);
    }
    if let Some(s) = filename.strip_suffix(".whl") {
        return Some(s);
    }
    if let Some(s) = filename.strip_suffix(".zip") {
        return Some(s);
    }
    if let Some(s) = filename.strip_suffix(".tar.bz2") {
        return Some(s);
    }
    None
}

/// Split a PyPI filename stem into (name, version).
/// The version starts at the first '-' followed by a digit.
fn split_pypi_name_version(stem: &str) -> Option<(&str, &str)> {
    // For wheel files the format is: {distribution}-{version}(-...)?
    // We need the first '-' where the next char is a digit.
    let bytes = stem.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'-' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            let name = &stem[..i];
            // Version goes until the next '-' (if wheel) or end (if sdist).
            let rest = &stem[i + 1..];
            // For sdist: rest IS the version.
            // For wheel: rest is "version-python-abi-platform"
            let version = rest.split('-').next()?;
            return Some((name, version));
        }
    }
    None
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn extracts_sdist_pattern() {
        let purl = extract_pypi_purl(
            "files.pythonhosted.org",
            "/packages/70/8e/ef012345/requests-2.31.0.tar.gz",
        )
        .unwrap();
        assert_eq!(purl.ecosystem(), "pypi");
        assert_eq!(purl.name(), "requests");
        assert_eq!(purl.version(), Some("2.31.0"));
    }

    #[test]
    fn extracts_wheel_pattern() {
        let purl = extract_pypi_purl(
            "files.pythonhosted.org",
            "/packages/ab/cd/ef/cryptography-42.0.5-cp39-abi3-manylinux_2_28_x86_64.whl",
        )
        .unwrap();
        assert_eq!(purl.ecosystem(), "pypi");
        assert_eq!(purl.name(), "cryptography");
        assert_eq!(purl.version(), Some("42.0.5"));
    }

    #[test]
    fn rejects_non_pypi_hostname() {
        assert!(
            extract_pypi_purl("crates.io", "/packages/ab/cd/ef/requests-2.31.0.tar.gz")
                .is_none()
        );
    }

    #[test]
    fn rejects_malformed_path() {
        assert!(extract_pypi_purl("pypi.org", "/simple/requests/").is_none());
    }

    #[test]
    fn resolver_metadata_matches_contract() {
        let r = PypiResolver;
        assert_eq!(r.name(), "pypi");
        assert_eq!(r.priority(), 99);
        assert_eq!(r.technique(), ResolutionTechnique::UrlPattern);
        assert!((r.confidence() - 0.95).abs() < f64::EPSILON);
    }
}
