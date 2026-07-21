//! Milestone 209: Debian / Ubuntu apt URL-family resolver.
//!
//! Handles registry download URLs from `deb.debian.org`,
//! `security.debian.org`, `archive.ubuntu.com`, and
//! `security.ubuntu.com`. Matches
//! `/<distro>/pool/main/{letter}/{name}/{name}_{version}_{arch}.deb`.
//!
//! Extracted verbatim from the pre-refactor
//! `mikebom-cli/src/resolve/url_resolver.rs::resolve_deb` per FR-002.
//! Threads `ctx.deb_codename` (the `/etc/os-release`-sampled distro
//! codename) into the extraction as the PURL `distro` qualifier.

use std::future::Future;
use std::pin::Pin;

use waybill_common::resolution::{ResolutionTechnique, ResolvedComponent};
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::common::{build_url_component, hostname_and_path};
use crate::resolve::resolver_trait::{
    ResolveContext, ResolveInput, Resolver, ResolverError,
};

pub(crate) struct DebResolver;

impl Resolver for DebResolver {
    fn name(&self) -> &'static str {
        "deb"
    }

    fn priority(&self) -> u32 {
        94
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
                    "deb.debian.org"
                        | "security.debian.org"
                        | "archive.ubuntu.com"
                        | "security.ubuntu.com"
                )
            }
            ResolveInput::FileOp(_) => false,
        }
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
            let ResolveInput::Connection {
                connection,
                basename_to_file_op,
            } = input
            else {
                return Ok(Vec::new());
            };
            let (hostname, path) = hostname_and_path(connection);
            let Some(purl) = extract_deb_purl(hostname, path, ctx.deb_codename) else {
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

/// Extract a Debian/Ubuntu PURL from a `(hostname, path)` pair. Returns
/// `None` on non-matching inputs. The `codename_hint` argument comes
/// from `/etc/os-release` on the build host and takes precedence over
/// URL-path heuristics.
fn extract_deb_purl(
    hostname: &str,
    path: &str,
    codename_hint: Option<&str>,
) -> Option<Purl> {
    let distro_namespace = match hostname {
        "deb.debian.org" | "security.debian.org" => "debian",
        "archive.ubuntu.com" | "security.ubuntu.com" => "ubuntu",
        _ => return None,
    };

    // Find the pool/ section.
    let pool_idx = path.find("/pool/")?;
    let after_pool = &path[pool_idx + 6..]; // after "/pool/"

    // Split: "main/{letter}/{name}/{name}_{version}_{arch}.deb"
    // or: "main/{letter}/{name}/{filename}.deb"
    let segments: Vec<&str> = after_pool.split('/').collect();
    if segments.len() < 4 {
        return None;
    }

    // The filename is the last segment.
    let filename = segments.last()?;
    let stem = filename
        .strip_suffix(".deb")
        .or_else(|| filename.strip_suffix(".udeb"))?;

    // Split filename stem: "{name}_{version}_{arch}"
    // Note: package names may contain hyphens but not underscores (usually).
    let parts: Vec<&str> = stem.splitn(3, '_').collect();
    if parts.len() < 3 {
        return None;
    }

    let name = parts[0];
    let version = parts[1];
    let arch = parts[2];

    // Codename precedence: explicit hint from the trace host (preferred,
    // because it comes from `/etc/os-release` on the machine the build
    // actually ran on), then a URL-path heuristic for pool URLs that
    // include the codename (`/dists/bookworm/` etc.), then nothing.
    let codename = codename_hint
        .map(|s| s.to_string())
        .or_else(|| guess_deb_codename(distro_namespace, path).map(|s| s.to_string()));

    // Percent-encode special characters in the version for PURL qualifiers.
    // The PURL spec requires proper encoding in the canonical form.
    let encoded_version = percent_encode_deb_version(version);
    // Encode `+` in name too (`libstdc++6` → `libstdc%2B%2B6`).
    let encoded_name = encode_purl_segment(name);

    // PURL deb spec: `distro` qualifier value is the codename alone
    // (`bookworm`, `jammy`), not `<namespace>-<codename>`. Matching the
    // spec here lets downstream tools (deps.dev, osv.dev, vex feeds) use
    // the PURL as a stable lookup key.
    let mut purl_str = format!(
        "pkg:deb/{distro_namespace}/{encoded_name}@{encoded_version}?arch={arch}"
    );
    if let Some(cn) = codename {
        purl_str.push_str(&format!("&distro={cn}"));
    }

    let purl = Purl::new(&purl_str).ok()?;
    tracing::debug!("deb URL match: {purl_str}");
    Some(purl)
}

/// Percent-encode a Debian version string to match the packageurl
/// reference implementation's canonical form. Delegates to the shared
/// helper so scan-mode and trace-mode produce byte-identical PURLs.
///
/// Note the asymmetry: only `+` is encoded; `:` (epoch) and `~`
/// (pre-release marker) stay literal per the reference impl.
fn percent_encode_deb_version(version: &str) -> String {
    waybill_common::types::purl::encode_purl_version(version)
}

/// Attempt to guess the distribution codename from the URL path.
/// This is a best-effort heuristic; the codename is not always present.
fn guess_deb_codename(namespace: &str, path: &str) -> Option<&'static str> {
    // Check for known codenames in the path.
    let debian_codenames = ["trixie", "bookworm", "bullseye", "buster", "stretch"];
    let ubuntu_codenames = [
        "noble", "mantic", "lunar", "kinetic", "jammy", "focal", "bionic",
    ];

    let codenames: &[&str] = match namespace {
        "debian" => &debian_codenames,
        "ubuntu" => &ubuntu_codenames,
        _ => return None,
    };

    let path_lower = path.to_ascii_lowercase();
    codenames
        .iter()
        .find(|&&cn| path_lower.contains(cn))
        .copied()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn extracts_debian_pattern() {
        let purl = extract_deb_purl(
            "deb.debian.org",
            "/debian/pool/main/c/curl/curl_8.5.0-2_amd64.deb",
            None,
        )
        .unwrap();
        assert_eq!(purl.ecosystem(), "deb");
        assert_eq!(purl.namespace(), Some("debian"));
        assert_eq!(purl.name(), "curl");
    }

    #[test]
    fn extracts_ubuntu_pattern() {
        let purl = extract_deb_purl(
            "archive.ubuntu.com",
            "/ubuntu/pool/main/o/openssl/openssl_3.0.13-0ubuntu3.4_amd64.deb",
            None,
        )
        .unwrap();
        assert_eq!(purl.ecosystem(), "deb");
        assert_eq!(purl.namespace(), Some("ubuntu"));
        assert_eq!(purl.name(), "openssl");
    }

    #[test]
    fn codename_hint_lands_in_purl() {
        let purl = extract_deb_purl(
            "deb.debian.org",
            "/debian/pool/main/j/jq/jq_1.6-2.1+deb12u1_arm64.deb",
            Some("bookworm"),
        )
        .unwrap();
        let canonical = purl.as_str();
        assert!(
            canonical.contains("distro=bookworm"),
            "expected distro=bookworm in {canonical}"
        );
    }

    #[test]
    fn rejects_non_deb_hostname() {
        assert!(extract_deb_purl(
            "crates.io",
            "/debian/pool/main/c/curl/curl_8.5.0-2_amd64.deb",
            None
        )
        .is_none());
    }

    #[test]
    fn rejects_malformed_path() {
        assert!(extract_deb_purl("deb.debian.org", "/no/pool/section", None).is_none());
    }

    #[test]
    fn resolver_metadata_matches_contract() {
        let r = DebResolver;
        assert_eq!(r.name(), "deb");
        assert_eq!(r.priority(), 94);
        assert_eq!(r.technique(), ResolutionTechnique::UrlPattern);
        assert!((r.confidence() - 0.95).abs() < f64::EPSILON);
    }
}
