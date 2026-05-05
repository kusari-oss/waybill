//! Auto-detection paths for source identifiers.
//!
//! Two entry points:
//!
//! - `auto_detect_repo_identifier(scan_root)` for source-tier
//!   `--path` scans (FR-001). Implements the 3-step git-remote
//!   fallback per spec Q1: `origin` → `upstream` → first-listed
//!   alphabetical. Failure (no git, no remotes, command error) is
//!   logged at `tracing::info!` and returns `None` — never fails the
//!   scan.
//! - `image_reference_to_identifier(...)` for image-tier `--image`
//!   scans (FR-008). Synthesizes the canonical `image:<registry>/
//!   <name>:<tag>@sha256:<digest>` shape per spec Q3 from the
//!   resolved-image fields, omitting components that aren't present.

use std::path::Path;
use std::process::Command;

use super::{Identifier, IdentifierKind, IdentifierValue, SchemeName};

/// Auto-detect a `repo:` identifier from a git checkout. Returns `None`
/// when the scan root isn't a git checkout, has no remotes, or any git
/// subprocess errors out — all such conditions log at `tracing::info!`
/// and never fail the scan (FR-001).
///
/// Three-step fallback per Q1 clarification: try `origin` first; fall
/// back to `upstream`; fall back to first-listed remote per `git
/// remote` output (alphabetical). The chosen remote name is recorded
/// in the resulting identifier's `source_label` for transparency
/// (FR-007).
pub fn auto_detect_repo_identifier(scan_root: &Path) -> Option<Identifier> {
    if !scan_root.join(".git").exists() {
        return None;
    }
    // Step 1+2: try origin and upstream by name.
    for name in ["origin", "upstream"] {
        if let Some(url) = git_remote_get_url(scan_root, name) {
            return build_repo_identifier(url, name, false);
        }
    }
    // Step 3: list all remotes alphabetically; take the first.
    let remotes = match git_remote_list(scan_root) {
        Some(r) if !r.is_empty() => r,
        Some(_) => {
            tracing::info!(
                scan_root = %scan_root.display(),
                "git checkout has no remotes; source identifier auto-detection skipped"
            );
            return None;
        }
        None => {
            tracing::info!(
                scan_root = %scan_root.display(),
                "git remote list failed; source identifier auto-detection skipped"
            );
            return None;
        }
    };
    // Skip origin/upstream because they were already tried above.
    let first = remotes
        .iter()
        .find(|r| r.as_str() != "origin" && r.as_str() != "upstream")?;
    let url = git_remote_get_url(scan_root, first)?;
    build_repo_identifier(url, first, true)
}

/// Construct the `repo:` identifier with a properly-labeled
/// `source_label`. `fallback_used` indicates whether we fell through
/// to the first-listed (non-origin, non-upstream) remote.
fn build_repo_identifier(
    url: String,
    remote_name: &str,
    fallback_used: bool,
) -> Option<Identifier> {
    // Trim trailing newline introduced by the `git` subprocess.
    let url = url.trim().to_string();
    if url.is_empty() {
        return None;
    }
    let scheme = match SchemeName::new("repo".to_string()) {
        Ok(s) => s,
        Err(_) => return None,
    };
    let value = match IdentifierValue::new(url.clone()) {
        Ok(v) => v,
        Err(_) => return None,
    };
    // Re-run the value validator so the resulting kind reflects
    // whether the URL is well-formed. Auto-detected values from `git
    // remote get-url` are well-formed unless the operator explicitly
    // configured a malformed remote — preserve the soft-fail path.
    let kind = match super::BuiltinScheme::from_scheme_name(&scheme) {
        Some(b) => match super::validators::validate_for_scheme(b, value.as_str()) {
            Ok(()) => IdentifierKind::Builtin(b),
            Err(err) => {
                tracing::warn!(
                    remote = remote_name,
                    url = %url,
                    reason = %err,
                    "auto-detected repo URL failed `repo:` validation; \
                     emitting as user-defined under \
                     mikebom:identifiers"
                );
                IdentifierKind::UserDefined
            }
        },
        None => IdentifierKind::UserDefined,
    };
    let label = if fallback_used {
        format!(
            "auto-detected from git remote `{remote_name}` (origin/upstream absent; first-listed)"
        )
    } else {
        format!("auto-detected from git remote `{remote_name}`")
    };
    Some(Identifier::from_parts_with_label(
        scheme,
        value,
        kind,
        Some(label),
    ))
}

/// Run `git -C <scan_root> remote get-url <name>`. Returns `None` on
/// any failure (subprocess error, exit status non-zero, empty output).
fn git_remote_get_url(scan_root: &Path, name: &str) -> Option<String> {
    let scan_root_str = scan_root.to_str()?;
    let output = Command::new("git")
        .args(["-C", scan_root_str, "remote", "get-url", name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Run `git -C <scan_root> remote`. Returns the list of configured
/// remote names, sorted alphabetically (which is `git remote`'s
/// natural output). `None` on subprocess error.
fn git_remote_list(scan_root: &Path) -> Option<Vec<String>> {
    let scan_root_str = scan_root.to_str()?;
    let output = Command::new("git")
        .args(["-C", scan_root_str, "remote"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?;
    let mut names: Vec<String> = s
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    names.sort();
    Some(names)
}

// ---------------------------------------------------------------------
// Image-tier auto-detection
// ---------------------------------------------------------------------

/// Synthesize an `image:` identifier from resolved-image fields.
///
/// Per the Q3 clarification, the canonical shape is:
///
/// ```text
/// image:<registry>/<name>:<tag>@sha256:<digest>
/// ```
///
/// with these documented omissions:
///
/// - tarball-loaded images without a registry context omit the
///   registry portion: `image:<name>@sha256:<digest>` or
///   `image:<name>:<tag>@sha256:<digest>`.
/// - pre-distribution-spec images without a digest omit the digest:
///   `image:<registry>/<name>:<tag>` etc.
///
/// Returns `None` when there's not enough information to synthesize
/// any meaningful identifier (no name).
pub fn image_reference_to_identifier(
    registry: Option<&str>,
    name: &str,
    tag: Option<&str>,
    digest: Option<&str>,
) -> Option<Identifier> {
    if name.is_empty() {
        return None;
    }
    let mut wire = String::new();
    if let Some(r) = registry {
        if !r.is_empty() {
            wire.push_str(r);
            wire.push('/');
        }
    }
    wire.push_str(name);
    if let Some(t) = tag {
        if !t.is_empty() {
            wire.push(':');
            wire.push_str(t);
        }
    }
    if let Some(d) = digest {
        if !d.is_empty() {
            wire.push_str("@sha256:");
            wire.push_str(d);
        }
    }
    let scheme = SchemeName::new("image".to_string()).ok()?;
    let value = IdentifierValue::new(wire).ok()?;
    let kind = match super::BuiltinScheme::from_scheme_name(&scheme) {
        Some(b) => match super::validators::validate_for_scheme(b, value.as_str()) {
            Ok(()) => IdentifierKind::Builtin(b),
            Err(err) => {
                tracing::warn!(
                    value = value.as_str(),
                    reason = %err,
                    "auto-synthesized `image:` value failed validation; \
                     emitting as user-defined under \
                     mikebom:identifiers"
                );
                IdentifierKind::UserDefined
            }
        },
        None => IdentifierKind::UserDefined,
    };
    Some(Identifier::from_parts_with_label(
        scheme,
        value,
        kind,
        Some("auto-detected from resolved image reference".to_string()),
    ))
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::process::Command;

    fn run(cmd: &mut Command) {
        let status = cmd.status().expect("git subprocess");
        assert!(status.success(), "git command failed: {cmd:?}");
    }

    fn git_init(dir: &Path) {
        run(Command::new("git")
            .args(["-C", dir.to_str().unwrap(), "init", "-q"]));
        // Some CI git installs require user.email/user.name; not
        // strictly required for the read-only `remote` commands we
        // exercise, but harmless.
    }

    fn git_remote_add(dir: &Path, name: &str, url: &str) {
        run(Command::new("git")
            .args(["-C", dir.to_str().unwrap(), "remote", "add", name, url]));
    }

    #[test]
    fn no_git_dir_returns_none() {
        let td = tempfile::tempdir().unwrap();
        assert!(auto_detect_repo_identifier(td.path()).is_none());
    }

    #[test]
    fn git_dir_no_remotes_returns_none() {
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        assert!(auto_detect_repo_identifier(td.path()).is_none());
    }

    #[test]
    fn origin_only_uses_origin() {
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        git_remote_add(td.path(), "origin", "git@github.com:test/foo.git");
        let id = auto_detect_repo_identifier(td.path()).expect("origin detected");
        assert_eq!(id.scheme.as_str(), "repo");
        assert_eq!(id.value.as_str(), "git@github.com:test/foo.git");
        assert_eq!(
            id.source_label.as_deref(),
            Some("auto-detected from git remote `origin`")
        );
        assert!(id.is_builtin());
    }

    #[test]
    fn upstream_only_uses_upstream() {
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        git_remote_add(td.path(), "upstream", "git@github.com:acme/foo.git");
        let id = auto_detect_repo_identifier(td.path()).expect("upstream detected");
        assert_eq!(id.scheme.as_str(), "repo");
        assert_eq!(id.value.as_str(), "git@github.com:acme/foo.git");
        assert_eq!(
            id.source_label.as_deref(),
            Some("auto-detected from git remote `upstream`")
        );
    }

    #[test]
    fn third_remote_only_uses_first_alphabetical_with_fallback_label() {
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        // Add only third-named remotes (no origin/upstream).
        git_remote_add(td.path(), "zebra", "git@example.com:z/foo.git");
        git_remote_add(td.path(), "alpha", "git@example.com:a/foo.git");
        let id = auto_detect_repo_identifier(td.path()).expect("first-listed detected");
        // Alphabetical first → alpha.
        assert_eq!(id.value.as_str(), "git@example.com:a/foo.git");
        assert_eq!(
            id.source_label.as_deref(),
            Some(
                "auto-detected from git remote `alpha` (origin/upstream absent; first-listed)"
            )
        );
    }

    #[test]
    fn origin_wins_over_upstream() {
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        git_remote_add(td.path(), "origin", "git@github.com:o/foo.git");
        git_remote_add(td.path(), "upstream", "git@github.com:u/foo.git");
        let id = auto_detect_repo_identifier(td.path()).unwrap();
        assert_eq!(id.value.as_str(), "git@github.com:o/foo.git");
    }

    #[test]
    fn image_full_form_synthesis() {
        let id = image_reference_to_identifier(
            Some("docker.io"),
            "acme/foo",
            Some("v1"),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"),
        )
        .unwrap();
        assert_eq!(id.scheme.as_str(), "image");
        assert_eq!(
            id.value.as_str(),
            "docker.io/acme/foo:v1@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
        assert!(id.is_builtin());
    }

    #[test]
    fn image_tarball_no_registry_synthesis() {
        let id = image_reference_to_identifier(
            None,
            "acme/foo",
            None,
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"),
        )
        .unwrap();
        assert_eq!(
            id.value.as_str(),
            "acme/foo@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    fn image_pre_distribution_spec_no_digest_synthesis() {
        let id =
            image_reference_to_identifier(Some("docker.io"), "acme/foo", Some("v1"), None)
                .unwrap();
        assert_eq!(id.value.as_str(), "docker.io/acme/foo:v1");
    }

    #[test]
    fn image_empty_name_returns_none() {
        assert!(image_reference_to_identifier(Some("docker.io"), "", None, None).is_none());
    }
}
