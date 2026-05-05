//! Per-built-in-scheme syntactic validators (research.md §1).
//!
//! Validators are best-effort. A failure emits `tracing::warn!` and
//! the caller (`Identifier::parse`) downgrades the identifier's
//! `kind` to `IdentifierKind::UserDefined` (research.md §1 soft-fail
//! / VR-005). The identifier still emits — under the
//! `mikebom:identifiers` annotation rather than the
//! standards-native carrier.
//!
//! Per Constitution Principle X (Transparency): warn-and-emit is more
//! useful than silent-rewrite or hard-fail. Operators see the warning
//! AND can inspect the SBOM to see what got emitted.

use super::{BuiltinScheme, IdentifierError};

/// Dispatch a value through the per-scheme validator. `Ok(())` means
/// the value passed; `Err(BuiltinValidation)` means the caller should
/// soft-fail per research.md §1.
pub fn validate_for_scheme(scheme: BuiltinScheme, value: &str) -> Result<(), IdentifierError> {
    match scheme {
        BuiltinScheme::Repo => validate_repo(value),
        BuiltinScheme::Git => validate_git(value),
        BuiltinScheme::Image => validate_image(value),
        BuiltinScheme::Attestation => validate_attestation(value),
    }
}

/// Validate a `repo:` value. Accepts URL or git-style ssh URL shapes.
///
/// Permissive — git itself accepts many input shapes; we mirror that.
/// Specifically the function accepts:
///
/// - `https://...` / `http://...`
/// - `ssh://...`
/// - `git://...`
/// - `git@host:path` (git-ssh-pseudo)
/// - `<user>@<host>:<path>` (general ssh-pseudo)
pub fn validate_repo(value: &str) -> Result<(), IdentifierError> {
    if value.starts_with("https://")
        || value.starts_with("http://")
        || value.starts_with("ssh://")
        || value.starts_with("git://")
        || value.starts_with("git@")
    {
        return ensure_no_whitespace("repo", value);
    }
    // ssh-pseudo: `<user>@<host>:<path>`. Require an `@`, a `:` after
    // it, AND a non-empty host + path.
    if let Some(at_idx) = value.find('@') {
        let after_at = &value[at_idx + 1..];
        if let Some(colon_idx) = after_at.find(':') {
            let host = &after_at[..colon_idx];
            let path = &after_at[colon_idx + 1..];
            if !host.is_empty() && !path.is_empty() {
                return ensure_no_whitespace("repo", value);
            }
        }
    }
    Err(IdentifierError::BuiltinValidation {
        scheme: "repo".to_string(),
        reason: format!(
            "value {value:?} does not match URL / ssh-URL / git-ssh-pseudo shapes"
        ),
    })
}

/// Validate a `git:` value. Same as `repo:` on the URL portion;
/// optional `#<fragment>` is preserved but unvalidated.
pub fn validate_git(value: &str) -> Result<(), IdentifierError> {
    let url_portion = match value.find('#') {
        Some(i) => &value[..i],
        None => value,
    };
    // Repurpose the `repo:` validator on the URL portion. Wrap the
    // error to attribute it to `git:` rather than `repo:`.
    match validate_repo(url_portion) {
        Ok(()) => Ok(()),
        Err(_) => Err(IdentifierError::BuiltinValidation {
            scheme: "git".to_string(),
            reason: format!(
                "value {value:?} URL portion does not match git URL shapes"
            ),
        }),
    }
}

/// Validate an `image:` value per the Q3 canonical regex.
///
/// Accepts:
/// - Full form: `<registry>/<name>:<tag>@sha256:<digest>`
/// - Tarball-only (no registry): `<name>:<tag>@sha256:<digest>` or
///   `<name>@sha256:<digest>`
/// - Pre-distribution-spec (no digest): `<registry>/<name>:<tag>` or
///   `<registry>/<name>` or `<name>:<tag>` or `<name>`
///
/// Per research.md §1: regex
/// `^([a-zA-Z0-9.\-_/]+/)?[a-zA-Z0-9.\-_/]+(:[a-zA-Z0-9.\-_]+)?(@sha256:[a-fA-F0-9]{64})?$`.
/// The actual implementation is a hand-rolled state machine since
/// `regex` is in the workspace deps but we want zero allocations on
/// the hot path.
pub fn validate_image(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(err_image(value, "value is empty"));
    }
    // Split on `@sha256:` if present.
    let (head, digest_part) = match value.find("@sha256:") {
        Some(i) => (&value[..i], Some(&value[i + "@sha256:".len()..])),
        None => (value, None),
    };
    if let Some(d) = digest_part {
        if d.len() != 64 || !d.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(err_image(
                value,
                "digest portion must be 64 hex chars after @sha256:",
            ));
        }
    }
    // Now split head on the LAST `:` (tag). Note: registries can have
    // ports like `localhost:5000/foo`. We need to detect "is this `:`
    // a port colon or a tag colon?". Tag is `[a-zA-Z0-9.\-_]+` only —
    // no `/` allowed. If the `:` is followed by chars containing a
    // `/`, it's a port colon, not a tag.
    let (name_part, tag_part) = split_name_and_tag(head);
    if let Some(tag) = tag_part {
        if tag.is_empty() {
            return Err(err_image(value, "empty tag after `:`"));
        }
        if !tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        {
            return Err(err_image(
                value,
                "tag contains invalid chars (allowed: [a-zA-Z0-9._-])",
            ));
        }
    }
    if name_part.is_empty() {
        return Err(err_image(value, "empty name portion"));
    }
    // The `name_part` may include a registry/host prefix. Allowed chars
    // across the whole thing: `[a-zA-Z0-9.\-_/]` plus the `:` between
    // host and port if any. We approximate by allowing `:` only if the
    // surrounding chars look like a port spec (digits after).
    for (i, c) in name_part.char_indices() {
        let ok = c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '/');
        if ok {
            continue;
        }
        if c == ':' {
            // Allow `:` only if it's a registry port colon (followed by
            // digits up to the next `/` or end).
            let rest = &name_part[i + 1..];
            let mut digits_seen = 0usize;
            for cc in rest.chars() {
                if cc.is_ascii_digit() {
                    digits_seen += 1;
                } else if cc == '/' {
                    break;
                } else {
                    return Err(err_image(value, "invalid char in name portion"));
                }
            }
            if digits_seen == 0 {
                return Err(err_image(
                    value,
                    "`:` in name portion must be followed by a port number",
                ));
            }
            continue;
        }
        return Err(err_image(value, "invalid char in name portion"));
    }
    Ok(())
}

/// Hand-rolled split: find the LAST `:` in `head` that isn't inside a
/// host:port segment (i.e., that has no `/` after it). Returns
/// `(name_with_optional_registry, optional_tag)`.
fn split_name_and_tag(head: &str) -> (&str, Option<&str>) {
    if let Some(last_colon) = head.rfind(':') {
        let after = &head[last_colon + 1..];
        if !after.contains('/') {
            // It's a tag-shape: chars after the `:` are not a path.
            // Distinguish from `host:port` by checking that the char
            // BEFORE the colon doesn't end a host (heuristic: if the
            // post-colon contains only `[0-9]+` and there's no `.`
            // before the colon in the segment, it's a port — but
            // realistically `:5000` followed by no `/` is a malformed
            // registry. We accept the simpler heuristic that "no `/`
            // after the `:`" means it's a tag.).
            return (&head[..last_colon], Some(after));
        }
    }
    (head, None)
}

fn err_image(value: &str, reason: &str) -> IdentifierError {
    IdentifierError::BuiltinValidation {
        scheme: "image".to_string(),
        reason: format!("value {value:?}: {reason}"),
    }
}

/// Validate an `attestation:` value. Permissive — any RFC 3986 URI
/// shape accepted. We require at minimum a scheme followed by `:`
/// (which means the value itself must look like `<inner-scheme>:...`,
/// e.g., `https://example.org/...`). The outer `attestation:` already
/// contains the wrapping; the validator inspects what comes after.
pub fn validate_attestation(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::BuiltinValidation {
            scheme: "attestation".to_string(),
            reason: "value is empty".to_string(),
        });
    }
    // Require an inner-scheme separator: `<scheme>:...` or `urn:...`,
    // ssh-pseudo style is also permitted (operators may use
    // `attestation:git@host:path` for in-toto over git URLs).
    if !value.contains(':') && !value.starts_with('/') {
        return Err(IdentifierError::BuiltinValidation {
            scheme: "attestation".to_string(),
            reason: format!(
                "value {value:?} does not look like a URI (no `:`, not absolute path)"
            ),
        });
    }
    ensure_no_whitespace("attestation", value)
}

fn ensure_no_whitespace(scheme: &str, value: &str) -> Result<(), IdentifierError> {
    if value.chars().any(|c| c.is_whitespace()) {
        return Err(IdentifierError::BuiltinValidation {
            scheme: scheme.to_string(),
            reason: format!("value {value:?} contains whitespace"),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn validate_repo_accepts_https_url() {
        validate_repo("https://github.com/foo/bar.git").unwrap();
        validate_repo("http://localhost/foo/bar").unwrap();
    }

    #[test]
    fn validate_repo_accepts_ssh_url() {
        validate_repo("ssh://git@github.com/foo/bar.git").unwrap();
        validate_repo("git://github.com/foo/bar.git").unwrap();
    }

    #[test]
    fn validate_repo_accepts_git_ssh_pseudo() {
        validate_repo("git@github.com:foo/bar.git").unwrap();
        validate_repo("user@example.com:repo/path.git").unwrap();
    }

    #[test]
    fn validate_repo_rejects_garbage() {
        assert!(validate_repo("not_a_url").is_err());
        assert!(validate_repo("just text").is_err());
        assert!(validate_repo("").is_err());
    }

    #[test]
    fn validate_repo_rejects_whitespace_in_url() {
        // url-shape detected by prefix but contains whitespace.
        assert!(validate_repo("https://github.com/foo bar").is_err());
    }

    #[test]
    fn validate_git_accepts_url_with_fragment() {
        validate_git("https://github.com/foo/bar.git#abc1234567890").unwrap();
        validate_git("git@github.com:foo/bar.git#main").unwrap();
    }

    #[test]
    fn validate_git_accepts_url_without_fragment() {
        validate_git("https://github.com/foo/bar.git").unwrap();
    }

    #[test]
    fn validate_git_rejects_garbage() {
        assert!(validate_git("not_a_url").is_err());
    }

    #[test]
    fn validate_image_accepts_full_form() {
        validate_image(
            "docker.io/foo/bar:v1@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .unwrap();
    }

    #[test]
    fn validate_image_accepts_no_registry_with_digest() {
        validate_image(
            "foo/bar@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .unwrap();
    }

    #[test]
    fn validate_image_accepts_no_digest() {
        validate_image("docker.io/foo/bar:v1").unwrap();
        validate_image("foo/bar:v1").unwrap();
        validate_image("foo").unwrap();
    }

    #[test]
    fn validate_image_accepts_registry_with_port() {
        validate_image("localhost:5000/foo:v1").unwrap();
        validate_image(
            "registry.example.com:8443/team/img:1.0@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .unwrap();
    }

    #[test]
    fn validate_image_rejects_short_digest() {
        assert!(validate_image("foo/bar@sha256:abc").is_err());
    }

    #[test]
    fn validate_image_rejects_non_hex_digest() {
        assert!(validate_image(
            "foo/bar@sha256:zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
        )
        .is_err());
    }

    #[test]
    fn validate_image_rejects_garbage() {
        assert!(validate_image("").is_err());
        assert!(validate_image("foo bar").is_err());
        assert!(validate_image("foo:tag/with/slash").is_err());
    }

    #[test]
    fn validate_attestation_accepts_https_url() {
        validate_attestation("https://example.org/att/build-42").unwrap();
    }

    #[test]
    fn validate_attestation_accepts_urn() {
        validate_attestation("urn:in-toto:42").unwrap();
    }

    #[test]
    fn validate_attestation_rejects_no_scheme() {
        assert!(validate_attestation("plain_text_no_scheme").is_err());
    }

    #[test]
    fn validate_attestation_rejects_empty() {
        assert!(validate_attestation("").is_err());
    }

    #[test]
    fn validate_attestation_rejects_whitespace() {
        assert!(validate_attestation("https://example.org/att 42").is_err());
    }

    #[test]
    fn dispatch_routes_to_correct_validator() {
        validate_for_scheme(BuiltinScheme::Repo, "git@github.com:a/b.git").unwrap();
        validate_for_scheme(BuiltinScheme::Git, "https://x/y#c").unwrap();
        validate_for_scheme(BuiltinScheme::Image, "foo/bar").unwrap();
        validate_for_scheme(BuiltinScheme::Attestation, "https://x").unwrap();

        assert!(validate_for_scheme(BuiltinScheme::Repo, "garbage").is_err());
        assert!(validate_for_scheme(BuiltinScheme::Git, "garbage").is_err());
        assert!(validate_for_scheme(BuiltinScheme::Image, "").is_err());
        assert!(validate_for_scheme(BuiltinScheme::Attestation, "").is_err());
    }
}
