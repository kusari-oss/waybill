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
        let purl_str = format!("pkg:{purl_type}/{owner}/{repo}@{rev}");
        if let Ok(purl) = Purl::new(&purl_str) {
            return Some(InputIdentity {
                purl,
                name: repo.to_string(),
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
