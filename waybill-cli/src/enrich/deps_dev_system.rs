//! PURL ecosystem → deps.dev system identifier mapping.
//!
//! deps.dev indexes six package ecosystems: cargo, npm, pypi, go,
//! maven, nuget. PURL's ecosystem field uses slightly different names
//! (`golang` vs `go`), and many common ecosystems (deb, apk, generic,
//! github) aren't covered at all. This module is the single source of
//! truth for the mapping so the enrichment pass can skip unsupported
//! ecosystems silently instead of making doomed API calls.

/// Return the deps.dev `system` identifier for a PURL ecosystem, or
/// `None` if deps.dev doesn't index that ecosystem.
pub fn deps_dev_system_for(ecosystem: &str) -> Option<&'static str> {
    match ecosystem {
        "cargo" => Some("cargo"),
        "npm" => Some("npm"),
        "pypi" => Some("pypi"),
        // PURL spec uses "golang"; deps.dev uses "go".
        "golang" | "go" => Some("go"),
        "maven" => Some("maven"),
        "nuget" => Some("nuget"),
        // Deliberately unsupported (deps.dev has no data):
        // "deb", "apk", "generic", "github", "gem", "docker"
        _ => None,
    }
}

/// Format the deps.dev `package name` field for a PURL-described
/// component. Different ecosystems compose the name from the PURL's
/// `namespace` and `name` fields differently:
///
/// - **Maven**: `"{groupId}:{artifactId}"`. The earlier license-lookup
///   path used just `name` (the artifactId), which produced
///   `com.google.guava:guava` → `guava` and consistently missed.
/// - **Go**: `"{namespace}/{name}"` — the full module path.
/// - **npm scoped**: `"@{namespace}/{name}"`.
/// - **Everything else**: `name` alone.
pub fn deps_dev_package_name(ecosystem: &str, namespace: Option<&str>, name: &str) -> String {
    match ecosystem {
        "maven" => match namespace {
            Some(g) if !g.is_empty() => format!("{g}:{name}"),
            _ => name.to_string(),
        },
        "golang" | "go" => {
            // `ResolvedComponent.name` for Go is the FULL module path
            // (e.g. `github.com/sirupsen/logrus`), not the short name.
            // Prepending the namespace would double the host+org prefix
            // and produce 404s against deps.dev. If the caller happened
            // to pass a short name, fall back to the old behaviour.
            if name.contains('/') {
                name.to_string()
            } else {
                match namespace {
                    Some(ns) if !ns.is_empty() => format!("{ns}/{name}"),
                    _ => name.to_string(),
                }
            }
        }
        "npm" => {
            // `ResolvedComponent.name` for a scoped npm package is
            // ALREADY the full `@scope/name` — the same shape the Go
            // arm above guards against. Prepending the namespace
            // doubled it, and because the namespace arrives
            // percent-encoded from the PURL (`%40types`, not
            // `@types`), `trim_start_matches('@')` stripped nothing.
            // The result was `@%40types/@types/node`, which 404s on
            // every scoped package — silently, since a 404 is
            // indistinguishable from "deps.dev has no data".
            if name.contains('/') {
                name.to_string()
            } else {
                match namespace {
                    Some(ns) if !ns.is_empty() => {
                        // Decode before trimming: the scope marker may
                        // be `@` or `%40` depending on whether the
                        // caller passes a decoded name or a raw PURL
                        // segment.
                        let decoded = ns.replace("%40", "@");
                        let trimmed = decoded.trim_start_matches('@');
                        format!("@{trimmed}/{name}")
                    }
                    _ => name.to_string(),
                }
            }
        }
        _ => name.to_string(),
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn supported_ecosystems_map_to_deps_dev_systems() {
        assert_eq!(deps_dev_system_for("cargo"), Some("cargo"));
        assert_eq!(deps_dev_system_for("npm"), Some("npm"));
        assert_eq!(deps_dev_system_for("pypi"), Some("pypi"));
        assert_eq!(deps_dev_system_for("golang"), Some("go"));
        assert_eq!(deps_dev_system_for("go"), Some("go"));
        assert_eq!(deps_dev_system_for("maven"), Some("maven"));
        assert_eq!(deps_dev_system_for("nuget"), Some("nuget"));
    }

    #[test]
    fn unsupported_ecosystems_return_none() {
        assert_eq!(deps_dev_system_for("deb"), None);
        assert_eq!(deps_dev_system_for("apk"), None);
        assert_eq!(deps_dev_system_for("generic"), None);
        assert_eq!(deps_dev_system_for("github"), None);
        assert_eq!(deps_dev_system_for("gem"), None);
        assert_eq!(deps_dev_system_for("docker"), None);
        assert_eq!(deps_dev_system_for(""), None);
    }

    #[test]
    fn maven_name_is_group_artifact() {
        assert_eq!(
            deps_dev_package_name("maven", Some("com.google.guava"), "guava"),
            "com.google.guava:guava",
        );
    }

    #[test]
    fn go_name_is_module_path() {
        assert_eq!(
            deps_dev_package_name("golang", Some("github.com/spf13"), "cobra"),
            "github.com/spf13/cobra",
        );
    }

    #[test]
    fn go_name_does_not_double_when_caller_passes_full_module_path() {
        // Production path: `ResolvedComponent.name` for Go is already
        // the full module path, not the short name. Prepending the
        // namespace would produce `github.com/sirupsen/github.com/sirupsen/logrus`
        // and 404 at deps.dev. The helper detects this via `/` in name.
        assert_eq!(
            deps_dev_package_name(
                "golang",
                Some("github.com/sirupsen"),
                "github.com/sirupsen/logrus",
            ),
            "github.com/sirupsen/logrus",
        );
    }

    #[test]
    fn npm_scoped_name_includes_at() {
        assert_eq!(
            deps_dev_package_name("npm", Some("angular"), "core"),
            "@angular/core",
        );
        assert_eq!(
            deps_dev_package_name("npm", Some("@types"), "node"),
            "@types/node",
        );
    }

    /// Milestone 841 (#841). The test above passes a SHORT name, which
    /// is why it never caught this: the scanner passes the FULL scoped
    /// name, because `ResolvedComponent.name` for npm already carries
    /// the scope. These are the shapes that actually reach the
    /// function during a scan.
    #[test]
    fn npm_scope_is_not_applied_twice() {
        // What the scanner really passes: percent-encoded namespace
        // from the PURL, full scoped name from the component.
        assert_eq!(
            deps_dev_package_name("npm", Some("%40types"), "@types/node"),
            "@types/node",
            "pre-fix this produced `@%40types/@types/node`, which 404s",
        );
        assert_eq!(
            deps_dev_package_name("npm", Some("@babel"), "@babel/core"),
            "@babel/core",
        );
    }

    /// The scope marker may arrive as `@` or as `%40` depending on
    /// whether the caller decoded the PURL segment. Both must land on
    /// the same name, or enrichment succeeds or fails depending on the
    /// call site.
    #[test]
    fn npm_percent_encoded_scope_is_decoded() {
        assert_eq!(
            deps_dev_package_name("npm", Some("%40types"), "node"),
            "@types/node",
        );
        assert_eq!(
            deps_dev_package_name("npm", Some("%40types"), "node"),
            deps_dev_package_name("npm", Some("@types"), "node"),
        );
    }

    #[test]
    fn npm_unscoped_names_are_untouched() {
        assert_eq!(deps_dev_package_name("npm", None, "lodash"), "lodash");
        assert_eq!(deps_dev_package_name("npm", Some(""), "lodash"), "lodash");
    }

    #[test]
    fn unscoped_ecosystems_use_bare_name() {
        assert_eq!(deps_dev_package_name("cargo", None, "serde"), "serde");
        assert_eq!(deps_dev_package_name("pypi", None, "requests"), "requests");
        assert_eq!(deps_dev_package_name("npm", None, "lodash"), "lodash");
    }

    #[test]
    fn missing_namespace_falls_back_to_bare_name() {
        assert_eq!(deps_dev_package_name("maven", None, "artifactOnly"), "artifactOnly");
        assert_eq!(deps_dev_package_name("maven", Some(""), "artifactOnly"), "artifactOnly");
    }
}
