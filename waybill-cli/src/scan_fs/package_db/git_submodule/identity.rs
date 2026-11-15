//! Submodule URL to PURL (spec 1530 FR-004, research R8, R9, R16).

use std::path::Path;

use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::super::forge;

/// The PURL for a gitlink, and whether credentials were stripped from its
/// URL (the caller logs that, FR-005).
pub(crate) struct Identity {
    pub(crate) purl: Purl,
    pub(crate) name: String,
    pub(crate) credentials_stripped: bool,
    /// The sanitized URL, for the redacted WARN and for `waybill:download-url`.
    pub(crate) url: Option<String>,
}

/// Build the identity of a gitlink at `path` pinned to `commit`.
///
/// `url` is the declaration's URL, if any; `origin` is the super-project's
/// `remote.origin.url`, used to resolve a relative URL (research R8).
pub(crate) fn identity(url: Option<&str>, origin: Option<&str>, path: &Path, commit: &str) -> Option<Identity> {
    let Some(raw) = url else {
        let name = path.file_name()?.to_string_lossy().to_string();
        let purl = Purl::new(&format!("pkg:generic/{}@{}", encode_purl_segment(&name), commit)).ok()?;
        return Some(Identity { purl, name, credentials_stripped: false, url: None });
    };
    let resolved = resolve_relative(raw, origin);
    let sanitized = waybill::identifiers::sanitize::sanitize_userinfo(&resolved);
    let credentials_stripped = matches!(sanitized, std::borrow::Cow::Owned(_));
    let clean = sanitized.into_owned();

    if let Some((host, owner, repo)) = host_owner_repo(&clean) {
        if let Some(kind) = forge::forge_type(&host.to_ascii_lowercase()) {
            let fold = forge::lowercases_owner_and_name(kind);
            let case = |s: &str| if fold { s.to_lowercase() } else { s.to_string() };
            let (owner, repo) = (case(&owner), case(&repo));
            let purl = Purl::new(&format!(
                "pkg:{kind}/{}/{}@{}",
                encode_purl_segment(&owner),
                encode_purl_segment(&repo),
                commit
            ))
            .ok()?;
            return Some(Identity { purl, name: repo, credentials_stripped, url: Some(clean) });
        }
    }
    let name = last_segment(&clean)?;
    let purl = Purl::new(&format!(
        "pkg:generic/{}@{}?vcs_url=git+{}",
        encode_purl_segment(&name),
        commit,
        clean
    ))
    .ok()?;
    Some(Identity { purl, name, credentials_stripped, url: Some(clean) })
}

/// Resolve `./x` and `../x` against the super-project's remote URL the way
/// git does: each leading `../` removes one trailing component of the remote
/// URL. With no remote, the URL stays relative.
fn resolve_relative(url: &str, origin: Option<&str>) -> String {
    if !(url.starts_with("./") || url.starts_with("../")) {
        return url.to_string();
    }
    let Some(origin) = origin else { return url.to_string() };
    let mut base = origin.trim_end_matches('/').to_string();
    let mut rest = url;
    loop {
        if let Some(r) = rest.strip_prefix("./") {
            rest = r;
        } else if let Some(r) = rest.strip_prefix("../") {
            rest = r;
            // Drop the last path component; for an scp-style base the `:`
            // separates host from path.
            match base.rfind(['/', ':']) {
                Some(i) if !base[..i].ends_with('/') => base.truncate(i),
                _ => return url.to_string(),
            }
        } else {
            break;
        }
    }
    let sep = if base.ends_with(':') { "" } else { "/" };
    format!("{base}{sep}{rest}")
}

/// `(host, owner, repo)` for a URL whose path is exactly `<owner>/<repo>`,
/// in `scheme://[user@]host[:port]/owner/repo[.git]` or scp form
/// `[user@]host:owner/repo[.git]`.
fn host_owner_repo(url: &str) -> Option<(String, String, String)> {
    let (host, path) = if let Some((_, rest)) = url.split_once("://") {
        let rest = rest.rsplit_once('@').map(|(_, h)| h).unwrap_or(rest);
        let (hostport, path) = rest.split_once('/')?;
        (hostport.split(':').next()?.to_string(), path.to_string())
    } else {
        // scp form: no `://`, and a `:` before the first `/`.
        let colon = url.find(':')?;
        if url[..colon].contains('/') {
            return None;
        }
        let hostpart = &url[..colon];
        let host = hostpart.rsplit_once('@').map(|(_, h)| h).unwrap_or(hostpart);
        (host.to_string(), url[colon + 1..].to_string())
    };
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut segs = path.split('/').filter(|s| !s.is_empty());
    let (owner, repo) = (segs.next()?, segs.next()?);
    if segs.next().is_some() || host.is_empty() {
        return None;
    }
    Some((host, owner.to_string(), repo.to_string()))
}

fn last_segment(url: &str) -> Option<String> {
    let trimmed = url.trim_end_matches('/');
    let seg = trimmed.rsplit(['/', ':']).next()?;
    let seg = seg.strip_suffix(".git").unwrap_or(seg);
    (!seg.is_empty()).then(|| seg.to_string())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    const C: &str = "9ada6e3b6f74fa7770f46bf8ab6dc4e2a9d59564";

    fn purl(url: Option<&str>, origin: Option<&str>) -> String {
        identity(url, origin, Path::new("third_party/dep"), C).unwrap().purl.as_str().to_string()
    }

    #[test]
    fn s1530_github_forms() {
        for u in [
            "https://github.com/o/r",
            "https://github.com/o/r.git",
            "https://github.com/o/r.git/",
            "git@github.com:o/r.git",
            "ssh://git@github.com/o/r",
            "ssh://git@github.com:22/o/r.git",
        ] {
            assert_eq!(purl(Some(u), None), format!("pkg:github/o/r@{C}"), "{u}");
        }
    }

    #[test]
    fn s1530_forge_casing_follows_the_type_definitions() {
        assert_eq!(purl(Some("https://github.com/ROCm/Foo.git"), None), format!("pkg:github/rocm/foo@{C}"));
        assert_eq!(purl(Some("git@bitbucket.org:Org/Repo.git"), None), format!("pkg:bitbucket/org/repo@{C}"));
        assert_eq!(purl(Some("https://gitlab.com/Org/Repo.git"), None), format!("pkg:gitlab/Org/Repo@{C}"));
        assert_eq!(purl(Some("https://codeberg.org/Org/Repo"), None), format!("pkg:codeberg/Org/Repo@{C}"));
    }

    #[test]
    fn s1530_nested_gitlab_group_and_other_hosts_are_generic() {
        let p = purl(Some("https://gitlab.com/g/sub/p.git"), None);
        assert!(p.starts_with(&format!("pkg:generic/p@{C}?vcs_url=")), "{p}");
        let p = purl(Some("https://git.example.invalid/team/thing.git"), None);
        assert!(p.starts_with(&format!("pkg:generic/thing@{C}?vcs_url=")), "{p}");
        assert!(p.contains("git.example.invalid"), "{p}");
    }

    #[test]
    fn s1530_credentials_are_stripped() {
        let id = identity(
            Some("https://waybill-fixture-user:waybill-fixture-token@example.invalid/x.git"),
            None,
            Path::new("x"),
            C,
        )
        .unwrap();
        assert!(id.credentials_stripped);
        assert!(!id.purl.as_str().contains("waybill-fixture-token"), "{}", id.purl.as_str());
        assert!(!id.purl.as_str().contains("waybill-fixture-user"), "{}", id.purl.as_str());
    }

    #[test]
    fn s1530_relative_urls() {
        assert_eq!(purl(Some("../dep.git"), Some("https://github.com/o/app.git")), format!("pkg:github/o/dep@{C}"));
        assert_eq!(purl(Some("../dep.git"), Some("git@github.com:o/app.git")), format!("pkg:github/o/dep@{C}"));
        let p = purl(Some("../dep.git"), None);
        assert!(p.starts_with(&format!("pkg:generic/dep@{C}?vcs_url=")), "{p}");
    }

    #[test]
    fn s1530_no_url_uses_the_path_basename() {
        assert_eq!(purl(None, None), format!("pkg:generic/dep@{C}"));
    }
}
