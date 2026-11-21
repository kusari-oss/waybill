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
///   The returned string is the input with its userinfo span and `@`
///   deleted and every other byte kept (spec 1640 research R3). The
///   `url` crate's serializer would also lowercase an `https` host, drop
///   a default port and resolve dot segments, so a credentialled and a
///   clean spelling of one URL would differ. It is used only as a
///   fall-back when the span cannot be found.
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
    if let Some(spliced) = without_userinfo_span(url) {
        let clean = url::Url::parse(&spliced).is_ok_and(|p| p.username().is_empty() && p.password().is_none());
        if clean {
            return Cow::Owned(spliced);
        }
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

/// `url` without the authority's userinfo and its `@`: the authority runs
/// from after `://` to the first `/`, `?` or `#`, and the userinfo up to its
/// last `@`. `None` when there is no such span.
fn without_userinfo_span(url: &str) -> Option<String> {
    let start = url.find("://")? + 3;
    let rest = &url[start..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let at = rest[..end].rfind('@')?;
    Some(format!("{}{}", &url[..start], &rest[at + 1..]))
}

/// A URL a scanned project declares as a place to fetch from, with its
/// userinfo removed ([`sanitize_userinfo`]). Each removal emits the FR-016
/// warning naming `manifest`, except a bare `git` user of an ssh URL, which
/// carries no secret (spec 1640 planning amendment A1).
pub fn sanitize_declared_url<'a>(raw: &'a str, manifest: &str) -> Cow<'a, str> {
    let out = sanitize_userinfo(raw);
    if matches!(out, Cow::Owned(_)) && !is_ssh_git_user(raw) {
        tracing::warn!(
            target: "waybill::identifiers::sanitize",
            manifest_file = manifest,
            url_redacted = %redact_userinfo_for_log(raw),
            "stripped credentials from URL"
        );
    }
    out
}

fn is_ssh_git_user(raw: &str) -> bool {
    url::Url::parse(raw).is_ok_and(|p| {
        matches!(p.scheme(), "ssh" | "git+ssh") && p.username() == "git" && p.password().is_none()
    })
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

    // ----------------------------------------------------------------
    // Spec 1640 (#1647) research R3: stripping removes the userinfo span
    // and keeps every other byte, so a credentialled and a clean spelling
    // of one URL sanitise to the same string.
    // ----------------------------------------------------------------

    fn stripped(input: &str) -> String {
        let s = sanitize_userinfo(input);
        assert!(matches!(s, Cow::Owned(_)), "expected {input} to be stripped");
        s.into_owned()
    }

    #[test]
    fn s1640_stripping_keeps_every_other_byte() {
        for (input, want) in [
            ("https://u:p@GitHub.com/Org/Repo.git", "https://GitHub.com/Org/Repo.git"),
            ("https://u:p@github.com:443/org/repo.git", "https://github.com:443/org/repo.git"),
            ("https://u:p@github.com/a/../org/repo.git", "https://github.com/a/../org/repo.git"),
            ("https://u:p@host/path%7Erepo with space.git", "https://host/path%7Erepo with space.git"),
            ("git+https://u:p@GitHub.com/Org/Repo.git", "git+https://GitHub.com/Org/Repo.git"),
            ("git://u:p@Host.Example/repo;protocol=https;branch=main", "git://Host.Example/repo;protocol=https;branch=main"),
            ("ssh://git@Host.Example:22/org/repo.git", "ssh://Host.Example:22/org/repo.git"),
            ("https://u:p@host/x?b=2&a=1#frag", "https://host/x?b=2&a=1#frag"),
            ("https://ghp_x@host/o/r.git", "https://host/o/r.git"),
            ("https://u:p%40ss@host/o/r.git", "https://host/o/r.git"),
            ("https://u:p@host/o/r@v1.git", "https://host/o/r@v1.git"),
        ] {
            assert_eq!(stripped(input), want, "{input}");
        }
    }

    #[test]
    fn s1640_a_credentialled_and_a_clean_spelling_agree() {
        let clean = "https://Git.Example:443/Org/a/../Repo.git";
        assert_eq!(sanitize_userinfo(clean).as_ref(), clean);
        assert_eq!(stripped("https://wbfxtoken7c41@Git.Example:443/Org/a/../Repo.git"), clean);
    }

    #[test]
    fn s1640_an_at_sign_in_the_path_is_not_userinfo() {
        let input = "https://host/o/r@v1.git";
        assert!(matches!(sanitize_userinfo(input), Cow::Borrowed(_)));
    }

    #[derive(Clone, Default)]
    struct Capture(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Capture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            match self.0.lock() {
                Ok(mut g) => g.extend_from_slice(buf),
                Err(p) => p.into_inner().extend_from_slice(buf),
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
        type Writer = Capture;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// Everything `f` logs, at every level.
    fn logs_from<T>(f: impl FnOnce() -> T) -> (T, String) {
        let cap = Capture::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(cap.clone())
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .finish();
        let out = tracing::subscriber::with_default(subscriber, f);
        let text = match cap.0.lock() {
            Ok(g) => String::from_utf8_lossy(&g).into_owned(),
            Err(p) => String::from_utf8_lossy(&p.into_inner()).into_owned(),
        };
        (out, text)
    }

    const WARNING: &str = "stripped credentials from URL";

    #[test]
    fn s1640_a_declared_url_warns_once_without_the_credential() {
        let (out, logs) =
            logs_from(|| sanitize_declared_url("https://wbfxuser:wbfxtoken7c41@git.example/o/r.git", "dir/CMakeLists.txt").into_owned());
        assert_eq!(out, "https://git.example/o/r.git");
        assert_eq!(logs.matches(WARNING).count(), 1, "{logs}");
        assert!(logs.contains("dir/CMakeLists.txt"), "{logs}");
        assert!(logs.contains("<userinfo redacted>"), "{logs}");
        assert!(!logs.contains("wbfxuser") && !logs.contains("wbfxtoken7c41"), "{logs}");
    }

    #[test]
    fn s1640_a_clean_declared_url_does_not_warn() {
        let (out, logs) = logs_from(|| sanitize_declared_url("https://git.example/o/r.git", "m").into_owned());
        assert_eq!(out, "https://git.example/o/r.git");
        assert!(!logs.contains(WARNING), "{logs}");
    }

    /// Planning amendment A1: the bare `git` user of an ssh URL is stripped
    /// (Clarification Q1) but is not a credential to warn about.
    #[test]
    fn s1640_the_ssh_git_user_is_stripped_without_a_warning() {
        for raw in ["ssh://git@git.example/o/r.git", "git+ssh://git@git.example/o/r.git"] {
            let (out, logs) = logs_from(|| sanitize_declared_url(raw, "m").into_owned());
            assert!(!out.contains("git@"), "{out}");
            assert!(!logs.contains(WARNING), "{raw}: {logs}");
        }
        let (_, logs) = logs_from(|| sanitize_declared_url("ssh://alice@git.example/o/r.git", "m").into_owned());
        assert_eq!(logs.matches(WARNING).count(), 1, "{logs}");
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
