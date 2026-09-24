//! Retrieval of a pinned revision's files (milestone 926, #947).
//!
//! # The target comes from the lockfile, never from an assumption
//!
//! FR-016: a `flake.lock` may pin a fork, an internal mirror, or a
//! self-hosted forge. The URL is built from the locked entry's own `type`,
//! `owner`/`repo`, `host` and `url`. Assuming `NixOS/nixpkgs` would work on
//! the public case and silently retrieve the *wrong repository's* package set
//! for anyone using a mirror — which is worse than failing.
//!
//! # Failure is a first-class outcome, not an error path
//!
//! An unreachable, refused, unauthorized or slow source degrades the scan to
//! today's versionless behaviour (FR-008). It never prompts for credentials
//! (FR-018) and never blocks (FR-019). A network-restricted build environment
//! is a supported configuration, not a fault.

use std::time::Duration;

use super::super::lockfile::LockedRef;

/// Where a pinned revision's files can be retrieved from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Location {
    /// A GitHub-shaped forge: `raw.<host>/<owner>/<repo>/<rev>/<path>`.
    /// `host` is `None` for github.com and `Some` for an enterprise or
    /// self-hosted mirror pinned via `github:owner/repo?host=…`.
    GitHub {
        host: Option<String>,
        owner: String,
        repo: String,
    },
    /// A GitLab-shaped forge: `<host>/<owner>/<repo>/-/raw/<rev>/<path>`.
    GitLab {
        host: Option<String>,
        owner: String,
        repo: String,
    },
}

impl Location {
    /// Derive a location from a locked entry, or `None` when the entry's
    /// shape is one this reader cannot retrieve a single file from.
    ///
    /// `type = "git"` with an arbitrary URL is deliberately unsupported:
    /// there is no general way to fetch one file from a bare git remote over
    /// HTTPS without cloning, and cloning nixpkgs to read one file is not a
    /// trade this feature makes. Such a pin degrades with a distinct reason
    /// rather than being reported as a network failure.
    pub(crate) fn from_locked(locked: &LockedRef) -> Option<Self> {
        let owner = locked.owner.clone()?;
        let repo = locked.repo.clone()?;
        match locked.kind.as_str() {
            "github" => Some(Location::GitHub {
                host: locked.host.clone(),
                owner,
                repo,
            }),
            "gitlab" => Some(Location::GitLab {
                host: locked.host.clone(),
                owner,
                repo,
            }),
            _ => None,
        }
    }

    /// Build the URL for one file at one revision.
    pub(crate) fn raw_url(&self, rev: &str, path: &str) -> String {
        match self {
            Location::GitHub { host, owner, repo } => match host {
                // GitHub Enterprise serves raw content under /raw/ on the
                // same host rather than from a raw.* subdomain.
                Some(h) => format!("https://{h}/{owner}/{repo}/raw/{rev}/{path}"),
                None => format!("https://raw.githubusercontent.com/{owner}/{repo}/{rev}/{path}"),
            },
            Location::GitLab { host, owner, repo } => {
                let h = host.as_deref().unwrap_or("gitlab.com");
                format!("https://{h}/{owner}/{repo}/-/raw/{rev}/{path}")
            }
        }
    }
}

/// Why a retrieval did not produce content.
///
/// Every variant degrades the scan rather than failing it. They are kept
/// distinct because they call for different operator action: a 404 means the
/// revision does not carry a Haskell package set, whereas a 403 means the
/// scanner cannot see a repository that may well have one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FetchError {
    /// Reached the source; it does not have that path at that revision.
    NotFound,
    /// Refused or requires credentials waybill will not supply (FR-018).
    Unauthorized,
    /// Could not connect, or the transport failed.
    Unreachable(String),
    /// Exceeded the operator's budget (FR-019).
    TimedOut,
    /// `--offline` is set and the artifact is not in the local cache (#975).
    ///
    /// Distinct from the transport failures above because it is not a failure
    /// of the source: nothing was attempted. It is the one variant that must
    /// abort a *partial* retrieval rather than being tolerated, since
    /// tolerating it silently under-classifies boot libraries.
    OfflineCacheMiss,
}

// There is deliberately no `UnsupportedSource` variant. An unsupported pin
// shape is recognised by `Location::from_locked` returning `None`, before any
// retrieval is attempted, so it can never arrive as a fetch failure.

/// The retrieval boundary.
///
/// A trait so tests inject a source with no network (the hermetic posture the
/// existing reader suites use) while production uses HTTPS.
pub(crate) trait RevisionSource {
    fn fetch(&self, location: &Location, rev: &str, path: &str) -> Result<String, FetchError>;
}

/// Production source: one bounded HTTPS GET.
pub(crate) struct HttpSource {
    timeout: Duration,
}

impl HttpSource {
    pub(crate) fn new(timeout_secs: u64) -> Self {
        Self {
            timeout: Duration::from_secs(timeout_secs),
        }
    }
}

impl RevisionSource for HttpSource {
    /// Retrieve one file, on a dedicated OS thread.
    ///
    /// The thread is not an optimisation — it is required for correctness.
    /// `reqwest::blocking` builds its own Tokio runtime, and dropping that
    /// runtime inside another one panics in
    /// `tokio::runtime::blocking::shutdown`. The scan CLI's `execute` is an
    /// `async fn`, so every call here is already inside a runtime. Spawning a
    /// plain `std::thread` gives the blocking client a context with no
    /// ambient runtime, which is what it requires.
    ///
    /// This is the same posture m173's Go cache-warmer takes: synchronous
    /// work on its own thread rather than woven into the async runtime.
    ///
    /// It was missed locally because a warm cache short-circuits before any
    /// client is built, and the fixture that exercises it happens to pin the
    /// same revision a real scan had already cached. CI, starting cold, hit
    /// it on the first run.
    fn fetch(&self, location: &Location, rev: &str, path: &str) -> Result<String, FetchError> {
        let url = location.raw_url(rev, path);
        let timeout = self.timeout;

        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    let client = reqwest::blocking::Client::builder()
                        .timeout(timeout)
                        // No credential resolution by design (FR-018): a
                        // private mirror degrades rather than prompting or
                        // reading ambient tokens.
                        .build()
                        .map_err(|e| FetchError::Unreachable(e.to_string()))?;

                    let resp = client.get(&url).send().map_err(|e| {
                        if e.is_timeout() {
                            FetchError::TimedOut
                        } else {
                            FetchError::Unreachable(e.to_string())
                        }
                    })?;

                    match resp.status().as_u16() {
                        200 => resp
                            .text()
                            .map_err(|e| FetchError::Unreachable(e.to_string())),
                        401 | 403 => Err(FetchError::Unauthorized),
                        404 => Err(FetchError::NotFound),
                        other => Err(FetchError::Unreachable(format!("HTTP {other}"))),
                    }
                })
                .join()
                // A panic inside the worker degrades like any other failure
                // rather than propagating and aborting the scan (FR-008).
                .unwrap_or_else(|_| {
                    Err(FetchError::Unreachable("retrieval thread panicked".into()))
                })
        })
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn locked(kind: &str, owner: &str, repo: &str, host: Option<&str>) -> LockedRef {
        LockedRef {
            kind: kind.to_string(),
            owner: Some(owner.to_string()),
            repo: Some(repo.to_string()),
            url: None,
            host: host.map(String::from),
            rev: Some("deadbeef".into()),
            nar_hash: None,
            last_modified: None,
        }
    }

    #[test]
    fn m926_public_github_builds_the_raw_url() {
        let l = Location::from_locked(&locked("github", "NixOS", "nixpkgs", None)).unwrap();
        assert_eq!(
            l.raw_url("abc123", "pkgs/x.nix"),
            "https://raw.githubusercontent.com/NixOS/nixpkgs/abc123/pkgs/x.nix"
        );
    }

    /// FR-016: a fork is pinned with a different owner and must be retrieved
    /// from that owner, not from upstream.
    #[test]
    fn m926_a_fork_is_retrieved_from_the_fork() {
        let l = Location::from_locked(&locked("github", "some-org", "nixpkgs", None)).unwrap();
        assert!(l.raw_url("abc123", "p").contains("/some-org/nixpkgs/"));
        assert!(!l.raw_url("abc123", "p").contains("/NixOS/"));
    }

    /// FR-016: an internal mirror differs from the public forge only by host.
    #[test]
    fn m926_an_internal_mirror_is_retrieved_from_its_own_host() {
        let l = Location::from_locked(&locked(
            "github",
            "platform",
            "nixpkgs",
            Some("git.internal.example"),
        ))
        .unwrap();
        assert_eq!(
            l.raw_url("abc123", "pkgs/x.nix"),
            "https://git.internal.example/platform/nixpkgs/raw/abc123/pkgs/x.nix"
        );
    }

    #[test]
    fn m926_gitlab_uses_its_own_raw_path_shape() {
        let l = Location::from_locked(&locked("gitlab", "grp", "nixpkgs", None)).unwrap();
        assert_eq!(
            l.raw_url("abc123", "pkgs/x.nix"),
            "https://gitlab.com/grp/nixpkgs/-/raw/abc123/pkgs/x.nix"
        );
    }

    /// A shape this reader cannot retrieve one file from is recognised as
    /// such, so it can degrade with a reason distinct from a network failure.
    #[test]
    fn m926_unsupported_shapes_yield_no_location() {
        assert!(Location::from_locked(&locked("git", "o", "r", None)).is_none());
        assert!(Location::from_locked(&locked("tarball", "o", "r", None)).is_none());
        let mut no_owner = locked("github", "o", "r", None);
        no_owner.owner = None;
        assert!(Location::from_locked(&no_owner).is_none());
    }
}
