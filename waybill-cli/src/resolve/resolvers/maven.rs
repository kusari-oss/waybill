//! Milestone 209: Maven Central URL-family resolver.
//!
//! Handles registry download URLs from `repo1.maven.org`,
//! `repo.maven.apache.org`, and `central.maven.org`. Matches
//! `/{group/path}/{artifact}/{version}/{artifact}-{version}.(jar|pom|aar)`.
//!
//! Extracted verbatim from the pre-refactor
//! `mikebom-cli/src/resolve/url_resolver.rs::resolve_maven` per FR-002.

use std::future::Future;
use std::pin::Pin;

use waybill_common::resolution::{ResolutionTechnique, ResolvedComponent};
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::common::{build_url_component, hostname_and_path};
use crate::resolve::resolver_trait::{
    ResolveContext, ResolveInput, Resolver, ResolverError,
};

pub(crate) struct MavenResolver;

impl Resolver for MavenResolver {
    fn name(&self) -> &'static str {
        "maven"
    }

    fn priority(&self) -> u32 {
        96
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
                matches!(
                    hostname,
                    "repo1.maven.org" | "repo.maven.apache.org" | "central.maven.org"
                )
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
            let Some(purl) = extract_maven_purl(hostname, path) else {
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

/// Extract a Maven PURL from a `(hostname, path)` pair. Returns
/// `None` on non-matching inputs.
fn extract_maven_purl(hostname: &str, path: &str) -> Option<Purl> {
    match hostname {
        "repo1.maven.org" | "repo.maven.apache.org" | "central.maven.org" => {}
        _ => return None,
    }

    // Strip common prefix: /maven2/ or /maven/
    let rest = path
        .strip_prefix("/maven2/")
        .or_else(|| path.strip_prefix("/maven/"))
        .unwrap_or(path.strip_prefix('/').unwrap_or(path));

    // Split into segments.
    let segments: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();

    // Need at least 3 segments: group(1+), artifact, version, filename
    if segments.len() < 4 {
        return None;
    }

    let filename = segments[segments.len() - 1];
    let version = segments[segments.len() - 2];
    let artifact = segments[segments.len() - 3];
    let group_parts = &segments[..segments.len() - 3];

    // Validate filename starts with "{artifact}-{version}"
    let expected_prefix = format!("{artifact}-{version}");
    if !filename.starts_with(&expected_prefix) {
        return None;
    }

    let group = group_parts.join(".");

    let purl_str = format!(
        "pkg:maven/{}/{}@{}",
        encode_purl_segment(&group),
        encode_purl_segment(artifact),
        encode_purl_segment(version),
    );
    let purl = Purl::new(&purl_str).ok()?;
    tracing::debug!("maven URL match: {purl_str}");
    Some(purl)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn extracts_jar_pattern() {
        let purl = extract_maven_purl(
            "repo1.maven.org",
            "/maven2/org/apache/commons/commons-lang3/3.12.0/commons-lang3-3.12.0.jar",
        )
        .unwrap();
        assert_eq!(purl.ecosystem(), "maven");
        assert_eq!(purl.namespace(), Some("org.apache.commons"));
        assert_eq!(purl.name(), "commons-lang3");
        assert_eq!(purl.version(), Some("3.12.0"));
    }

    #[test]
    fn extracts_pom_pattern() {
        let purl = extract_maven_purl(
            "repo1.maven.org",
            "/maven2/com/google/guava/guava/33.0.0-jre/guava-33.0.0-jre.pom",
        )
        .unwrap();
        assert_eq!(purl.ecosystem(), "maven");
        assert_eq!(purl.namespace(), Some("com.google.guava"));
        assert_eq!(purl.name(), "guava");
        assert_eq!(purl.version(), Some("33.0.0-jre"));
    }

    #[test]
    fn rejects_non_maven_hostname() {
        assert!(extract_maven_purl(
            "crates.io",
            "/maven2/org/apache/commons/commons-lang3/3.12.0/commons-lang3-3.12.0.jar"
        )
        .is_none());
    }

    #[test]
    fn rejects_malformed_path() {
        assert!(extract_maven_purl("repo1.maven.org", "/maven2/too/short").is_none());
    }

    #[test]
    fn resolver_metadata_matches_contract() {
        let r = MavenResolver;
        assert_eq!(r.name(), "maven");
        assert_eq!(r.priority(), 96);
        assert_eq!(r.technique(), ResolutionTechnique::UrlPattern);
        assert!((r.confidence() - 0.95).abs() < f64::EPSILON);
    }
}
