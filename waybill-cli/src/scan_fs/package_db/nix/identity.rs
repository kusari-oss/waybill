//! Milestone 925 — identifier construction for flake inputs (FR-013a/b).
//!
//! The purl specification has no `nix` type, and whether a canonical one is
//! even expressible is unresolved upstream: several flake references can
//! denote the same package, which is in tension with the version-range
//! matching vulnerability scanners perform. So none is invented here
//! (FR-013c).
//!
//! Instead this follows milestone-128 FR-002a, which settled the same question
//! for Yocto `SRC_URI` + `SRCREV`: emit a host-typed PURL when the source is a
//! recognised forge and a revision is known, and fall back to `pkg:generic`
//! otherwise, **because OSV's commit and ecosystem queries return advisories
//! directly against host-typed PURLs**. That helper parses `SRC_URI` strings
//! whereas a lockfile supplies structured fields, so this is the same decision
//! reimplemented on a different input, not a call into it.

use waybill_common::types::purl::Purl;

use super::lockfile::LockedRef;

/// Map a flake input `type` to its purl type, when one exists.
///
/// Exhaustive on the types that have a purl equivalent; everything else takes
/// the `pkg:generic` path. Written as a match rather than a lookup table so a
/// new arm is a deliberate edit.
fn host_typed_purl_type(flake_type: &str) -> Option<&'static str> {
    match flake_type {
        "github" => Some("github"),
        "gitlab" => Some("gitlab"),
        "sourcehut" => Some("sourcehut"),
        _ => None,
    }
}

/// Whether the purl-spec type definition declares namespace and name
/// case-insensitive, and therefore requires lowercasing.
///
/// Checked against the published type definitions rather than assumed:
///
/// - `github-definition.json`    — `case_sensitive: false`, "It is not case
///   sensitive and shall be lowercased." (both namespace and name)
/// - `bitbucket-definition.json` — identical wording
/// - `gitlab`, `sourcehut`       — **no definition file exists**, so there is
///   no rule to follow and the declared spelling is preserved
///
/// This is the opposite of the #943 decision for Hackage, and deliberately so.
/// The rule is not "preserve case" or "fold case" — it is "follow the registry's
/// own rule", which has to be checked per ecosystem. Hackage names are
/// case-sensitive (`Diff` and `diff` are different packages); GitHub namespaces
/// are not, and two SBOMs of the same repository must join on PURL.
fn requires_lowercasing(purl_type: &str) -> bool {
    matches!(purl_type, "github" | "bitbucket")
}

/// Input types that name something local or registry-indirect, with no
/// published upstream identity to put in a document (FR-003).
pub(crate) fn is_unpublishable(flake_type: &str) -> bool {
    matches!(flake_type, "path" | "indirect")
}

/// A constructed identity for one locked input.
pub(crate) struct InputIdentity {
    pub(crate) purl: Purl,
    pub(crate) name: String,
    pub(crate) version: String,
    /// The upstream this was fetched from, for the native source-location
    /// field (FR-005).
    pub(crate) source_url: Option<String>,
    /// `waybill:source-type` value, mirroring the Pants non-registry-artifact
    /// precedent for `pkg:generic` entries (FR-013b).
    pub(crate) source_type: &'static str,
}

/// Build the identifier for a locked input.
///
/// `node_key` is the lockfile's own name for the input, used as the component
/// name when the pin carries no repo name of its own.
///
/// Returns `None` when the input cannot be identified — an unpublishable type,
/// or no revision. Per FR-002 identity IS the locked revision, so a pin
/// without one is not something to emit under a guessed identity.
pub(crate) fn identify(node_key: &str, locked: &LockedRef) -> Option<InputIdentity> {
    if is_unpublishable(&locked.kind) {
        return None;
    }
    let rev = locked.rev.as_deref()?;
    if rev.is_empty() {
        return None;
    }

    if let (Some(purl_type), Some(owner), Some(repo)) = (
        host_typed_purl_type(&locked.kind),
        locked.owner.as_deref(),
        locked.repo.as_deref(),
    ) {
        // Canonical form per the type's own definition. Without this the same
        // upstream yields two identifiers — a lockfile writing `NixOS/nixpkgs`
        // and one writing `nixos/nixpkgs` produce components that will not join
        // across two SBOMs of the same dependency.
        let (owner, repo) = if requires_lowercasing(purl_type) {
            (owner.to_ascii_lowercase(), repo.to_ascii_lowercase())
        } else {
            (owner.to_string(), repo.to_string())
        };
        let purl_str = format!("pkg:{purl_type}/{owner}/{repo}@{rev}");
        if let Ok(purl) = Purl::new(&purl_str) {
            return Some(InputIdentity {
                purl,
                name: repo.clone(),
                version: rev.to_string(),
                source_url: locked.url.clone(),
                source_type: "nix-flake-input",
            });
        }
    }

    // FR-013b — no host-typed equivalent, or a host-typed input missing
    // owner/repo. A `tarball` input reaches here and is still identifiable:
    // Phase 0 measured `rev` present on every `tarball` sample.
    let purl_str = format!("pkg:generic/{node_key}@{rev}");
    let purl = Purl::new(&purl_str).ok()?;
    Some(InputIdentity {
        purl,
        name: node_key.to_string(),
        version: rev.to_string(),
        source_url: locked.url.clone(),
        source_type: "nix-flake-input",
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn locked(kind: &str, owner: Option<&str>, repo: Option<&str>, rev: Option<&str>) -> LockedRef {
        LockedRef {
            kind: kind.to_string(),
            owner: owner.map(String::from),
            repo: repo.map(String::from),
            url: None,
            host: None,
            rev: rev.map(String::from),
            nar_hash: None,
            last_modified: None,
        }
    }

    #[test]
    fn github_input_is_host_typed() {
        let l = locked("github", Some("waybill-fixture-org"), Some("waybill-fixture-repo"), Some("abc123"));
        let id = identify("nixpkgs", &l).unwrap();
        assert_eq!(
            id.purl.as_str(),
            "pkg:github/waybill-fixture-org/waybill-fixture-repo@abc123",
            "a github input with a rev must be host-typed per FR-013a — OSV returns \
             advisories against host-typed PURLs, which is why m128 chose them"
        );
    }

    #[test]
    fn tarball_input_falls_back_to_generic_but_is_still_identified() {
        let mut l = locked("tarball", None, None, Some("def456"));
        l.url = Some("https://example.invalid/x.tar.gz".to_string());
        let id = identify("nixpkgs", &l).unwrap();
        assert_eq!(id.purl.as_str(), "pkg:generic/nixpkgs@def456");
        assert_eq!(
            id.source_url.as_deref(),
            Some("https://example.invalid/x.tar.gz"),
            "FR-013b requires the upstream URL be carried for generic entries"
        );
    }

    #[test]
    fn path_and_indirect_inputs_are_not_identified() {
        for kind in ["path", "indirect"] {
            assert!(
                identify("local", &locked(kind, None, None, Some("abc"))).is_none(),
                "FR-003: `{kind}` has no published upstream identity"
            );
        }
    }

    #[test]
    fn an_input_without_a_revision_is_not_identified() {
        assert!(
            identify("x", &locked("github", Some("o"), Some("r"), None)).is_none(),
            "identity IS the locked revision (FR-002); without one there is nothing to emit"
        );
    }

    #[test]
    fn github_namespace_and_name_are_lowercased_to_canonical_form() {
        // purl-spec `github-definition.json`: namespace and name are
        // `case_sensitive: false` — "It is not case sensitive and shall be
        // lowercased." Without this, one lockfile writing `NixOS/nixpkgs` and
        // another writing `nixos/nixpkgs` produce two identifiers for one
        // upstream, and two SBOMs of the same dependency will not join.
        let l = locked("github", Some("NixOS"), Some("NixPkgs"), Some("abc123"));
        let id = identify("nixpkgs", &l).unwrap();
        assert_eq!(id.purl.as_str(), "pkg:github/nixos/nixpkgs@abc123");
        assert_eq!(id.name, "nixpkgs", "the component name follows the canonical repo name");
    }

    #[test]
    fn a_type_with_no_purl_definition_keeps_its_declared_case() {
        // `gitlab` and `sourcehut` have NO definition file in purl-spec, so
        // there is no lowercasing rule to follow and inventing one would be a
        // guess. Checked, not assumed — the opposite of the github case.
        let l = locked("gitlab", Some("MixedCase"), Some("RepoName"), Some("abc123"));
        let id = identify("x", &l).unwrap();
        assert_eq!(id.purl.as_str(), "pkg:gitlab/MixedCase/RepoName@abc123");
    }

    #[test]
    fn the_revision_is_never_case_folded() {
        // A git revision is a hex digest; lowercasing it is harmless today but
        // the rule being applied is about NAMESPACE and NAME, not version.
        let l = locked("github", Some("NixOS"), Some("nixpkgs"), Some("ABCDEF123"));
        let id = identify("x", &l).unwrap();
        assert!(id.purl.as_str().ends_with("@ABCDEF123"), "got {}", id.purl.as_str());
    }

    #[test]
    fn no_pkg_nix_identifier_is_ever_constructed() {
        // FR-013c. Exercised across every input type this reader can meet.
        for kind in ["github", "gitlab", "sourcehut", "tarball", "git", "mercurial"] {
            if let Some(id) = identify("n", &locked(kind, Some("o"), Some("r"), Some("rev1"))) {
                assert!(
                    !id.purl.as_str().starts_with("pkg:nix"),
                    "FR-013c forbids inventing a pkg:nix identifier; `{kind}` produced {}",
                    id.purl.as_str()
                );
            }
        }
    }
}
