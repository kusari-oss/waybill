//! Forge host to PURL type, shared by the Yocto and git-submodule readers
//! (spec 1530 research R9) so one repository has one PURL type across readers.

/// The PURL type for a forge host, or `None` for any other host.
///
/// `github` and `bitbucket` have purl-spec type definitions; `gitlab` and
/// `codeberg` do not (checked 2026-11-15, spec 1530 research R16) and are
/// kept for consistency with the readers that already emit them.
pub(crate) fn forge_type(host: &str) -> Option<&'static str> {
    match host {
        "github.com" => Some("github"),
        "gitlab.com" => Some("gitlab"),
        "bitbucket.org" => Some("bitbucket"),
        "codeberg.org" => Some("codeberg"),
        _ => None,
    }
}

/// Whether a forge PURL type's purl-spec definition requires its namespace
/// and name to be lowercased (spec 1530 FR-004, research R16). The same
/// rule as `nix/identity.rs`, which records the definitions' wording.
pub(crate) fn lowercases_owner_and_name(purl_type: &str) -> bool {
    matches!(purl_type, "github" | "bitbucket")
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn s1530_forge_hosts() {
        assert_eq!(forge_type("github.com"), Some("github"));
        assert_eq!(forge_type("gitlab.com"), Some("gitlab"));
        assert_eq!(forge_type("bitbucket.org"), Some("bitbucket"));
        assert_eq!(forge_type("codeberg.org"), Some("codeberg"));
        assert_eq!(forge_type("example.invalid"), None);
        assert_eq!(forge_type("GitHub.com"), None, "hosts are matched as written; callers lowercase them");
    }

    #[test]
    fn s1530_casing_rule_follows_the_type_definitions() {
        assert!(lowercases_owner_and_name("github"));
        assert!(lowercases_owner_and_name("bitbucket"));
        assert!(!lowercases_owner_and_name("gitlab"));
        assert!(!lowercases_owner_and_name("codeberg"));
    }
}
