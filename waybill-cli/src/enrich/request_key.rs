//! Milestone 839 (#766) — the identity of one deps.dev enrichment
//! lookup.
//!
//! Three call sites need to agree on exactly what identifies a
//! package version: the per-component path, the batch path, and the
//! on-disk cache. If any two of them build the identity differently,
//! the cache silently splits — one path writes entries the other
//! never finds, and the only symptom is a cache that appears not to
//! work. This type is the single construction site.
//!
//! It deliberately does **not** reimplement ecosystem name mapping.
//! `deps_dev_system::deps_dev_package_name` already handles Maven's
//! `group:artifact`, Go's full module path and npm's `@scope/name`,
//! and is tested there; this wraps it so the wrapping is what callers
//! share.

use super::deps_dev_system::{deps_dev_package_name, deps_dev_system_for};

/// Milestone 1067 (#1058, US2) — a version string waybill synthesises when
/// it has none, which no registry publishes.
///
/// Its own set, not the distribution-URL guard's in `scan_fs`: that one
/// guards derived URLs, a different output, and moving it would move
/// goldens this feature must not touch. `0.0.0` is excluded because it
/// is a real published version of some packages (npm).
pub(crate) fn is_placeholder_version(v: &str) -> bool {
    const PLACEHOLDERS: [&str; 7] = [
        "",
        "unknown",
        "0.0.0-unknown",
        "v0.0.0-unknown",
        "noassertion",
        "none",
        "latest",
    ];
    PLACEHOLDERS.iter().any(|p| p.eq_ignore_ascii_case(v))
}

/// One package version to look up: the deps.dev system, the package
/// name as deps.dev expects it for that ecosystem, and the version.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EnrichmentKey {
    pub system: &'static str,
    pub name: String,
    pub version: String,
}

impl EnrichmentKey {
    /// Build a key from PURL parts, or `None` when deps.dev does not
    /// index the ecosystem or the coordinates are incomplete.
    ///
    /// Returning `None` rather than a key that cannot succeed keeps
    /// doomed requests out of every path at once — including out of
    /// batches, where one unusable entry would otherwise consume a
    /// slot and come back empty, indistinguishable from a package
    /// deps.dev genuinely does not carry.
    pub fn from_purl_parts(
        ecosystem: &str,
        namespace: Option<&str>,
        name: &str,
        version: &str,
    ) -> Option<Self> {
        if name.is_empty() || version.is_empty() {
            return None;
        }
        let system = deps_dev_system_for(ecosystem)?;
        Some(Self {
            system,
            name: deps_dev_package_name(ecosystem, namespace, name),
            version: version.to_string(),
        })
    }


    /// Stable string form, used as the on-disk cache key input.
    ///
    /// `/` and `:` are deliberately NOT the separator: Go names
    /// contain slashes and Maven names contain colons, so a separator
    /// drawn from either alphabet could let a name/version boundary
    /// merge and two distinct packages share an entry. ASCII unit
    /// separator appears in none of the three fields.
    pub fn cache_key(&self) -> String {
        format!("{}\u{1f}{}\u{1f}{}", self.system, self.name, self.version)
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn maven_uses_group_colon_artifact() {
        let k = EnrichmentKey::from_purl_parts("maven", Some("com.google.guava"), "guava", "33.0")
            .unwrap();
        assert_eq!(k.system, "maven");
        assert_eq!(k.name, "com.google.guava:guava");
    }

    #[test]
    fn go_keeps_the_full_module_path() {
        let k = EnrichmentKey::from_purl_parts(
            "golang",
            Some("github.com/sirupsen"),
            "github.com/sirupsen/logrus",
            "v1.9.3",
        )
        .unwrap();
        assert_eq!(k.system, "go");
        // Not doubled — the namespace must not be prepended to a name
        // that already carries it.
        assert_eq!(k.name, "github.com/sirupsen/logrus");
    }

    #[test]
    fn npm_scope_is_restored() {
        let k = EnrichmentKey::from_purl_parts("npm", Some("babel"), "core", "7.0.0").unwrap();
        assert_eq!(k.name, "@babel/core");
    }

    #[test]
    fn unindexed_ecosystems_yield_no_key() {
        for eco in ["deb", "apk", "generic", "gem", "docker", "github"] {
            assert!(
                EnrichmentKey::from_purl_parts(eco, None, "x", "1").is_none(),
                "{eco} is not indexed by deps.dev and must not produce a request",
            );
        }
    }

    #[test]
    fn incomplete_coordinates_yield_no_key() {
        assert!(EnrichmentKey::from_purl_parts("cargo", None, "", "1.0").is_none());
        assert!(EnrichmentKey::from_purl_parts("cargo", None, "serde", "").is_none());
    }



    #[test]
    fn placeholder_versions_are_recognised_in_any_case() {
        for v in [
            "", "unknown", "UNKNOWN", "0.0.0-unknown", "v0.0.0-unknown",
            "NOASSERTION", "none", "latest", "Latest",
        ] {
            assert!(is_placeholder_version(v), "{v:?} is a placeholder");
        }
    }

    #[test]
    fn real_versions_are_not_placeholders() {
        // `0.0.0` is published by real npm packages.
        for v in ["0.0.0", "1.0.0", "v1.9.3", "0.0.0-20240101-abcdef"] {
            assert!(!is_placeholder_version(v), "{v:?} is a real version");
        }
    }

    #[test]
    fn cache_key_separates_fields_that_share_an_alphabet() {
        // A separator taken from the name alphabet could let the
        // name/version boundary merge. These two differ only in where
        // that boundary falls.
        let a = EnrichmentKey::from_purl_parts("golang", None, "example.com/a/b", "v1").unwrap();
        let b = EnrichmentKey::from_purl_parts("golang", None, "example.com/a", "b/v1").unwrap();
        assert_ne!(a.cache_key(), b.cache_key());
    }

    #[test]
    fn equal_coordinates_produce_equal_keys() {
        // The property the whole type exists for: every path builds
        // the same key for the same package, so the cache does not
        // silently split.
        let a = EnrichmentKey::from_purl_parts("cargo", None, "serde", "1.0.197").unwrap();
        let b = EnrichmentKey::from_purl_parts("cargo", None, "serde", "1.0.197").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.cache_key(), b.cache_key());
    }
}
