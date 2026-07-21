//! OCI image reference parser (milestone 032).
//!
//! Parses image-ref strings into a typed `ImageReference` carrying
//! registry / repository / tag / digest. Replaces the
//! `oci_client::Reference` parser as part of dropping the
//! oci-client dep (#65). Behavior parity is the success criterion:
//! every ref shape the milestone-031 implementation accepted MUST
//! parse the same way here.
//!
//! Grammar (per OCI distribution-spec + Docker conventions):
//!
//! - The first path segment is a **registry** iff it contains `.`
//!   or `:` OR equals `localhost`. Otherwise the whole prefix is a
//!   repository path under `docker.io`.
//! - When the docker.io repository path has no `/`, prepend
//!   `library/` (Docker Hub's official-images convention —
//!   `alpine` becomes `library/alpine`).
//! - Tag defaults to `latest` if neither tag nor digest is
//!   present.
//! - Digest takes precedence over tag when both are present.
//!
//! Accepted shapes (verified by `tests::parses_typical_refs`):
//!
//! | Input                                                       | registry          | repository           | tag        | digest   |
//! |-------------------------------------------------------------|-------------------|----------------------|------------|----------|
//! | `alpine`                                                    | `docker.io`       | `library/alpine`     | `latest`   | None     |
//! | `alpine:3.19`                                               | `docker.io`       | `library/alpine`     | `3.19`     | None     |
//! | `library/alpine:3.19`                                       | `docker.io`       | `library/alpine`     | `3.19`     | None     |
//! | `docker.io/library/alpine:3.19`                             | `docker.io`       | `library/alpine`     | `3.19`     | None     |
//! | `gcr.io/foo/bar:tag`                                        | `gcr.io`          | `foo/bar`            | `tag`      | None     |
//! | `localhost:5000/foo/bar:tag`                                | `localhost:5000`  | `foo/bar`            | `tag`      | None     |
//! | `ghcr.io/foo/bar@sha256:0123…`                              | `ghcr.io`         | `foo/bar`            | None       | `sha256:0123…` |

use anyhow::{anyhow, bail, Result};

/// Default registry for refs that don't specify one (per Docker
/// Hub convention).
pub(super) const DEFAULT_REGISTRY: &str = "docker.io";

/// Default tag when neither tag nor digest is specified (per the
/// OCI distribution-spec / Docker convention).
pub(super) const DEFAULT_TAG: &str = "latest";

/// Parsed image reference. One of `tag` or `digest` is always
/// `Some`; both can be present (digest takes precedence at
/// resolution time).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ImageReference {
    pub registry: String,
    pub repository: String,
    pub tag: Option<String>,
    pub digest: Option<String>,
}

impl ImageReference {
    /// Whichever of digest or tag should be used to fetch the
    /// manifest — digest wins when both are set, else tag, else
    /// the OCI default `latest`.
    #[allow(dead_code)] // wired by registry.rs in commit 2 (032/migration)
    pub(super) fn resolved_reference(&self) -> &str {
        if let Some(d) = self.digest.as_ref() {
            return d.as_str();
        }
        self.tag.as_deref().unwrap_or(DEFAULT_TAG)
    }
}

/// Parse an image-ref string. Returns the typed
/// [`ImageReference`].
pub(super) fn parse_reference(input: &str) -> Result<ImageReference> {
    if input.is_empty() {
        bail!("empty image reference");
    }

    // Step 1: split off digest (everything after `@`).
    let (head, digest) = match input.split_once('@') {
        Some((h, d)) => (h, Some(d.to_string())),
        None => (input, None),
    };
    if let Some(d) = digest.as_ref() {
        if d.is_empty() {
            bail!("digest is empty after `@`");
        }
        if !d.contains(':') {
            bail!("digest must be `<algorithm>:<hex>` shape, got `{d}`");
        }
    }

    // Step 2: split head into registry/repository + tag.
    // The tag is the last `:`-separated segment IFF it doesn't
    // contain a `/` (which would mean it's actually a registry
    // port like `localhost:5000`). Find the LAST `/` first; the
    // tag separator must come after it.
    let last_slash = head.rfind('/');
    let last_colon = head.rfind(':');
    let (path_part, tag) = match (last_slash, last_colon) {
        (Some(s), Some(c)) if c > s => (&head[..c], Some(head[c + 1..].to_string())),
        (None, Some(c)) => (&head[..c], Some(head[c + 1..].to_string())),
        _ => (head, None),
    };
    if let Some(t) = tag.as_ref() {
        if t.is_empty() {
            bail!("tag is empty after `:`");
        }
    }

    if path_part.is_empty() {
        bail!("repository portion is empty");
    }

    // Step 3: split path_part into registry + repository.
    // The first `/`-separated segment is a registry iff it
    // contains `.` or `:` OR equals `localhost`.
    let (registry, repository) = match path_part.split_once('/') {
        Some((first, rest)) if is_registry_segment(first) => {
            if rest.is_empty() {
                bail!("repository portion is empty after registry `{first}`");
            }
            (first.to_string(), rest.to_string())
        }
        _ => (DEFAULT_REGISTRY.to_string(), path_part.to_string()),
    };

    // Step 4: docker.io's `library/` convention for bare official
    // images. Applies only when the registry is the default and
    // the repo has no `/` separator.
    let repository = if registry == DEFAULT_REGISTRY && !repository.contains('/') {
        format!("library/{repository}")
    } else {
        repository
    };

    // Step 5: enforce that we have at least a tag or a digest.
    let resolved = ImageReference {
        registry,
        repository,
        tag: tag.clone().or_else(|| {
            // No tag, no digest → default to "latest".
            if digest.is_none() {
                Some(DEFAULT_TAG.to_string())
            } else {
                None
            }
        }),
        digest,
    };

    if resolved.repository.is_empty() {
        return Err(anyhow!("repository is empty after parsing `{input}`"));
    }

    Ok(resolved)
}

/// First-segment-of-path test for "is this a registry?". A
/// segment is a registry iff:
/// 1. It contains a `.` (e.g., `gcr.io`, `ghcr.io`,
///    `myregistry.example.com`), OR
/// 2. It contains a `:` (e.g., `localhost:5000`,
///    `127.0.0.1:5000`), OR
/// 3. It equals `localhost`.
fn is_registry_segment(s: &str) -> bool {
    s.contains('.') || s.contains(':') || s == "localhost"
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// (input, registry, repository, tag, digest)
    type ParseCase = (&'static str, &'static str, &'static str, Option<&'static str>, Option<&'static str>);

    #[test]
    fn parses_typical_refs() {
        let cases: &[ParseCase] = &[
            ("alpine", "docker.io", "library/alpine", Some("latest"), None),
            ("alpine:3.19", "docker.io", "library/alpine", Some("3.19"), None),
            ("library/alpine:3.19", "docker.io", "library/alpine", Some("3.19"), None),
            (
                "docker.io/library/alpine:3.19",
                "docker.io",
                "library/alpine",
                Some("3.19"),
                None,
            ),
            ("gcr.io/foo/bar:tag", "gcr.io", "foo/bar", Some("tag"), None),
            (
                "localhost:5000/foo/bar:tag",
                "localhost:5000",
                "foo/bar",
                Some("tag"),
                None,
            ),
            (
                "ghcr.io/foo/bar@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "ghcr.io",
                "foo/bar",
                None,
                Some("sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"),
            ),
            (
                "gcr.io/distroless/static-debian12:latest",
                "gcr.io",
                "distroless/static-debian12",
                Some("latest"),
                None,
            ),
        ];
        for (input, exp_reg, exp_repo, exp_tag, exp_digest) in cases {
            let parsed = parse_reference(input).unwrap_or_else(|e| {
                panic!("parse_reference(`{input}`) failed: {e}")
            });
            assert_eq!(parsed.registry, *exp_reg, "input={input}");
            assert_eq!(parsed.repository, *exp_repo, "input={input}");
            assert_eq!(parsed.tag.as_deref(), *exp_tag, "input={input}");
            assert_eq!(parsed.digest.as_deref(), *exp_digest, "input={input}");
        }
    }

    #[test]
    fn parses_tag_and_digest_together() {
        // `name:tag@sha256:digest` — both present. digest wins
        // for resolution but tag is preserved on the struct.
        let parsed =
            parse_reference("alpine:3.19@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef").unwrap();
        assert_eq!(parsed.repository, "library/alpine");
        assert_eq!(parsed.tag.as_deref(), Some("3.19"));
        assert_eq!(
            parsed.digest.as_deref(),
            Some("sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        assert_eq!(
            parsed.resolved_reference(),
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "digest should win over tag for resolution"
        );
    }

    #[test]
    fn resolved_reference_falls_back_through_tag_to_default() {
        let parsed = parse_reference("alpine").unwrap();
        assert_eq!(parsed.resolved_reference(), "latest");
        let parsed = parse_reference("alpine:3.19").unwrap();
        assert_eq!(parsed.resolved_reference(), "3.19");
    }

    #[test]
    fn rejects_empty_input() {
        assert!(parse_reference("").is_err());
    }

    #[test]
    fn rejects_empty_tag() {
        assert!(parse_reference("alpine:").is_err());
    }

    #[test]
    fn rejects_empty_digest() {
        assert!(parse_reference("alpine@").is_err());
    }

    #[test]
    fn rejects_malformed_digest() {
        // No `<alg>:<hex>` separator.
        assert!(parse_reference("alpine@deadbeef").is_err());
    }

    #[test]
    fn rejects_registry_only_no_repository() {
        // `docker.io/` — registry but no repository segment.
        assert!(parse_reference("docker.io/").is_err());
    }

    #[test]
    fn is_registry_segment_logic() {
        assert!(is_registry_segment("gcr.io"));
        assert!(is_registry_segment("ghcr.io"));
        assert!(is_registry_segment("myregistry.example.com"));
        assert!(is_registry_segment("localhost:5000"));
        assert!(is_registry_segment("127.0.0.1:5000"));
        assert!(is_registry_segment("localhost"));
        assert!(!is_registry_segment("library"));
        assert!(!is_registry_segment("foo"));
        assert!(!is_registry_segment("alpine"));
    }
}
