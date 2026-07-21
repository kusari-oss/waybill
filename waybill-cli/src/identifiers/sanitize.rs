//! URL credential redaction helpers (FR-016, milestone 105;
//! originally milestone 075).
//!
//! Two public helpers:
//!
//! * [`sanitize_userinfo`] — strips RFC 3986 userinfo from a candidate
//!   URL. Returns `Cow::Borrowed` when the input was credential-free
//!   (the overwhelming common case), `Cow::Owned` when credentials were
//!   stripped. Never panics, never returns `Result`. All parse failures
//!   and edge cases (cannot-be-base, SSH-form) collapse to passthrough.
//!
//! * [`redact_userinfo_for_log`] — produces a log-safe representation
//!   of a URL with userinfo replaced by `<userinfo redacted>` for
//!   inclusion in `tracing::warn!` events. Preserves
//!   scheme/host/port/path so operators can identify which remote was
//!   sanitized without leaking the credential value.
//!
//! ## Logging convention (FR-016)
//!
//! Callers MUST emit a `tracing::warn!` event whenever
//! `sanitize_userinfo` returns `Cow::Owned`. The canonical event shape
//! (from `contracts/credential-redaction.md`):
//!
//! ```text
//! WARN target=waybill::identifiers::sanitize
//!   manifest_file=<path-to-manifest>
//!   url_redacted=<output-of-redact_userinfo_for_log>
//!   "stripped credentials from URL"
//! ```
//!
//! Each call site is responsible for supplying its own `manifest_file`
//! context (e.g., `.gitmodules`, `west.yml`, etc.) so the audit trail
//! is operator-actionable.

use std::borrow::Cow;

/// Strip RFC 3986 userinfo from a candidate URL.
///
/// Returns:
/// - `Cow::Borrowed(url)` when the URL has no userinfo, fails to parse
///   (SSH-form, malformed), or hits a cannot-be-base parser-setter
///   rejection. The original input is passed through unchanged.
/// - `Cow::Owned(sanitized)` when userinfo was successfully stripped.
///   The returned string is the URL re-canonicalized through the
///   `url` crate's serializer with the username cleared and password
///   removed.
///
/// Behavior matches the milestone-075 `sanitize_userinfo` helper
/// exactly (the original lived as a module-private fn in
/// `binding/identifiers/auto_detect.rs`); milestone 105 promoted it to
/// a public utility for the new C/C++ readers per FR-016 and collapsed
/// the previous `SanitizedUrl { original, sanitized, was_sanitized }`
/// return struct into `Cow<'_, str>`. Callers derive
/// "was_sanitized" from `matches!(result, Cow::Owned(_))`.
///
/// Detection: a `Cow::Owned` return indicates the caller MUST emit
/// the FR-016 `tracing::warn!` event per the module-level doc.
pub fn sanitize_userinfo(url: &str) -> Cow<'_, str> {
    let mut parsed = match url::Url::parse(url) {
        Ok(p) => p,
        Err(_) => {
            // Parse failure: SSH-form URLs (`git@host:foo/bar.git`)
            // and any other non-RFC-3986 input. Pass through unchanged.
            return Cow::Borrowed(url);
        }
    };
    // Real credentials = non-empty username OR present password.
    // Empty userinfo (`https://@host/...`) is syntactically userinfo
    // per RFC 3986 but carries no secret to strip — treat as
    // passthrough so the `Cow::Owned` return signal cleanly
    // indicates "credentials were actually stripped" for FR-016
    // log gating.
    let has_credentials =
        !parsed.username().is_empty() || parsed.password().is_some();
    if !has_credentials {
        return Cow::Borrowed(url);
    }
    // `set_username("")` and `set_password(None)` reject cannot-be-
    // base URLs (e.g., `mailto:`). Vanishingly rare for the
    // git-remote input domain, but the safe fallback is passthrough.
    if parsed.set_username("").is_err() {
        return Cow::Borrowed(url);
    }
    if parsed.set_password(None).is_err() {
        return Cow::Borrowed(url);
    }
    Cow::Owned(parsed.to_string())
}

/// Build a redacted form of a URL for log output. Replaces userinfo
/// with the literal string `<userinfo redacted>` while preserving
/// scheme/host/port/path/query/fragment so operators can identify
/// which remote was sanitized without leaking the credential value.
///
/// Behavior:
/// - Parse-success with userinfo present: emit
///   `<scheme>://<userinfo redacted>@<host>[:<port>]<path>...`.
/// - Parse-success without userinfo: pass the input through
///   unchanged (no redaction marker).
/// - Parse-failure (SSH-form, malformed): pass the input through
///   unchanged. Used in code paths gated on `Cow::Owned`-return from
///   `sanitize_userinfo`, so SSH-form never reaches this helper in
///   production.
///
/// The literal credential value MUST NOT appear in the output (FR-016).
/// Callers route the result through `tracing::warn!` as a `Display`
/// field.
pub fn redact_userinfo_for_log(url_str: &str) -> String {
    let parsed = match url::Url::parse(url_str) {
        Ok(p) => p,
        Err(_) => return url_str.to_string(),
    };
    let has_userinfo = !parsed.username().is_empty() || parsed.password().is_some();
    if !has_userinfo {
        return url_str.to_string();
    }
    // Reconstruct: <scheme>://<userinfo redacted>@<host>[:<port>]<path>?<query>#<fragment>
    let mut out = String::new();
    out.push_str(parsed.scheme());
    out.push_str("://<userinfo redacted>@");
    if let Some(host) = parsed.host_str() {
        out.push_str(host);
    }
    if let Some(port) = parsed.port() {
        out.push(':');
        out.push_str(&port.to_string());
    }
    out.push_str(parsed.path());
    if let Some(q) = parsed.query() {
        out.push('?');
        out.push_str(q);
    }
    if let Some(f) = parsed.fragment() {
        out.push('#');
        out.push_str(f);
    }
    out
}

// ----------------------------------------------------------------
// Milestone 075 — sanitize_userinfo unit tests
// (moved from binding/identifiers/auto_detect.rs by milestone 105)
// ----------------------------------------------------------------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_user_password_https() {
        let input = "https://USER:TOKEN@github.com/foo/bar.git";
        let s = sanitize_userinfo(input);
        assert!(matches!(s, Cow::Owned(_)), "expected sanitization");
        assert_eq!(s.as_ref(), "https://github.com/foo/bar.git");
        assert!(!s.contains("USER"));
        assert!(!s.contains("TOKEN"));
    }

    #[test]
    fn sanitize_strips_user_only_no_password() {
        // GitHub App pattern: bare `<token>@host` without a colon.
        let input = "https://ghp_AAA123@github.com/foo/bar.git";
        let s = sanitize_userinfo(input);
        assert!(matches!(s, Cow::Owned(_)));
        assert_eq!(s.as_ref(), "https://github.com/foo/bar.git");
        assert!(!s.contains("ghp_AAA123"));
    }

    #[test]
    fn sanitize_handles_empty_userinfo() {
        // Edge case: `https://@host/...` — syntactically userinfo
        // per RFC 3986 but no credential value to strip. The
        // milestone-105 helper treats this as "no credentials" and
        // returns `Cow::Borrowed` (passthrough). The `Cow::Owned`
        // return signal stays meaningful as "real credentials were
        // stripped" for FR-016 log gating.
        let s = sanitize_userinfo("https://@github.com/foo.git");
        assert!(
            matches!(s, Cow::Borrowed(_)),
            "empty-userinfo input has no credential to strip; expected Borrowed"
        );
        // The literal `@` survives in the passthrough — that's fine
        // because no secret was leaked (there was nothing to leak).
    }

    #[test]
    fn sanitize_preserves_port_when_stripping() {
        let s = sanitize_userinfo("https://USER:TOKEN@github.com:8443/foo.git");
        assert!(matches!(s, Cow::Owned(_)));
        assert_eq!(s.as_ref(), "https://github.com:8443/foo.git");
    }

    #[test]
    fn sanitize_passthrough_on_parse_failure() {
        // Bare token with no scheme — url::Url::parse rejects.
        let input = "not a url at all";
        let s = sanitize_userinfo(input);
        assert!(matches!(s, Cow::Borrowed(_)));
        assert_eq!(s.as_ref(), input);
    }

    #[test]
    fn sanitize_passthrough_on_no_userinfo() {
        let input = "https://github.com/foo/bar.git";
        let s = sanitize_userinfo(input);
        assert!(matches!(s, Cow::Borrowed(_)));
        assert_eq!(s.as_ref(), input);
    }

    #[test]
    fn sanitize_passthrough_on_ssh_form() {
        // SCP-like syntax — url::Url::parse rejects it (research §6).
        // Treated identically to no-userinfo for downstream emission.
        let input = "git@github.com:foo/bar.git";
        let s = sanitize_userinfo(input);
        assert!(matches!(s, Cow::Borrowed(_)));
        assert_eq!(s.as_ref(), input);
    }

    #[test]
    fn sanitize_is_deterministic() {
        // VR-075-002: same input → byte-identical sanitized output
        // across runs.
        let inputs = [
            "https://USER:TOKEN@github.com/foo.git",
            "https://github.com/foo.git",
            "git@github.com:foo/bar.git",
            "https://USER@github.com:443/foo.git",
        ];
        for input in &inputs {
            let a = sanitize_userinfo(input);
            for _ in 0..10 {
                let b = sanitize_userinfo(input);
                assert_eq!(a.as_ref(), b.as_ref());
                assert_eq!(
                    matches!(a, Cow::Owned(_)),
                    matches!(b, Cow::Owned(_)),
                    "Owned/Borrowed status differs across runs"
                );
            }
        }
    }

    // ----------------------------------------------------------------
    // Milestone 075 — redact_userinfo_for_log unit tests
    // ----------------------------------------------------------------

    #[test]
    fn redact_substitutes_userinfo_marker() {
        let r = redact_userinfo_for_log("https://USER:TOKEN@github.com/foo.git");
        assert_eq!(r, "https://<userinfo redacted>@github.com/foo.git");
        assert!(!r.contains("USER"));
        assert!(!r.contains("TOKEN"));
    }

    #[test]
    fn redact_passes_through_no_userinfo() {
        let r = redact_userinfo_for_log("https://github.com/foo.git");
        assert_eq!(r, "https://github.com/foo.git");
    }

    #[test]
    fn redact_passes_through_parse_failure() {
        let r = redact_userinfo_for_log("git@github.com:foo/bar.git");
        assert_eq!(r, "git@github.com:foo/bar.git");
    }

    #[test]
    fn redact_preserves_port_path_query_fragment() {
        // Use port 8443 — `url::Url` normalizes away default scheme
        // ports (`:443` for `https://`) at parse time, which is
        // expected URL-canonicalization behavior, not a redaction
        // bug. A non-default port survives intact.
        let r = redact_userinfo_for_log(
            "https://USER:TOKEN@github.com:8443/foo/bar.git?a=1#frag",
        );
        assert_eq!(
            r,
            "https://<userinfo redacted>@github.com:8443/foo/bar.git?a=1#frag"
        );
    }
}
