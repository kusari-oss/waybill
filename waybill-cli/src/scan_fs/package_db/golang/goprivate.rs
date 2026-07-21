// Milestone 055 — Parsers for `$GOPROXY` and `$GOPRIVATE` env vars.
//
// Module-level `#[allow(dead_code)]`: the foundational scaffold (T005)
// lands these parsers ahead of the US1 wiring tasks (T021–T025) that
// actually consume them. The allow is removed in T025 once
// `WorkspaceContext::from_workspace()` calls `parse_proxy_chain()` /
// `parse_private_patterns()`.
#![allow(dead_code)]

//
// We honor these two variables to match Go's own behavior (FR-004):
//
//   - `$GOPROXY`: comma- or pipe-separated chain of proxy URLs. Comma
//     means "fall through on HTTP 404/410 only"; pipe means "fall
//     through on any error." Special values: `off` (no fetching),
//     `direct` (source-VCS fetch — out of scope for 055; treated as a
//     terminator). Default when unset: `https://proxy.golang.org,direct`.
//
//   - `$GOPRIVATE`: comma-separated glob patterns. A module path
//     matched by any pattern MUST NOT be fetched via the public proxy
//     (the orchestrator skips step 3 for those modules). Pattern
//     semantics replicate Go's `cmd/go/internal/module/module.go::
//     MatchPrefixPatterns`: segment-wise globbing, `*` matches any chars
//     except `/`.
//
// See specs/055-go-transitive-edges/research.md R5 (GOPRIVATE) and R7
// (GOPROXY) for the algorithm derivations.

use reqwest::Url;

// --------------------------------------------------------------------
// Errors
// --------------------------------------------------------------------

#[derive(thiserror::Error, Debug)]
pub enum ProxyParseError {
    // `reqwest::Url::parse` returns `url::ParseError`, but the `url`
    // crate isn't a direct dep of mikebom-cli. We stringify the error
    // message at the parse site instead of carrying the typed source.
    #[error("invalid proxy URL `{url}`: {detail}")]
    InvalidUrl { url: String, detail: String },

    #[error("`off` and `direct` cannot be chained with other entries")]
    InvalidSpecialChain,
}

// --------------------------------------------------------------------
// $GOPROXY
// --------------------------------------------------------------------

/// One entry in a parsed `$GOPROXY` chain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProxyEntry {
    /// An HTTP/HTTPS proxy URL. `fall_through_on_404_only` is `true`
    /// when this entry was followed by `,` (fall through on 404/410
    /// only) and `false` when followed by `|` (fall through on any
    /// error).
    Url {
        url: Url,
        fall_through_on_404_only: bool,
    },

    /// `direct` — out of scope for 055. Treated as a chain terminator
    /// that signals "no more proxies; fall through to ladder step 4."
    Direct,

    /// `off` — disables step 3 entirely.
    Off,
}

#[derive(Clone, Debug, Default)]
pub struct ProxyChain {
    pub entries: Vec<ProxyEntry>,
}

impl ProxyChain {
    pub fn is_off(&self) -> bool {
        self.entries.iter().any(|e| matches!(e, ProxyEntry::Off))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &ProxyEntry> {
        self.entries.iter()
    }
}

/// Parse a `$GOPROXY` env value per Go's documented semantics.
///
/// `None` or empty input → default chain `[Url(proxy.golang.org, ,), Direct]`.
/// `"off"` → `Off`. `"direct"` → `Direct`. Otherwise: split on `,` and `|`,
/// each token is a URL or one of the special values. The separator
/// preceding each URL determines its `fall_through_on_404_only` flag
/// (true after `,`, false after `|`). The first entry's flag defaults to
/// `true` (Go's convention).
pub fn parse_proxy_chain(env_value: Option<&str>) -> Result<ProxyChain, ProxyParseError> {
    let raw = env_value.unwrap_or("").trim();
    if raw.is_empty() {
        return Ok(default_chain());
    }
    if raw == "off" {
        return Ok(ProxyChain {
            entries: vec![ProxyEntry::Off],
        });
    }
    if raw == "direct" {
        return Ok(ProxyChain {
            entries: vec![ProxyEntry::Direct],
        });
    }

    // Walk the string char-by-char tracking which separator preceded
    // each token. Go allows mixed `,` and `|` separators.
    let mut entries = Vec::new();
    let mut current = String::new();
    // The first entry has no preceding separator; Go treats this as `,`
    // semantics (fall through on 404 only).
    let mut next_fall_through_on_404_only = true;
    for ch in raw.chars() {
        match ch {
            ',' | '|' => {
                push_token(
                    &mut entries,
                    std::mem::take(&mut current),
                    next_fall_through_on_404_only,
                )?;
                next_fall_through_on_404_only = ch == ',';
            }
            _ => current.push(ch),
        }
    }
    push_token(&mut entries, current, next_fall_through_on_404_only)?;

    Ok(ProxyChain { entries })
}

fn push_token(
    entries: &mut Vec<ProxyEntry>,
    token: String,
    fall_through_on_404_only: bool,
) -> Result<(), ProxyParseError> {
    let token = token.trim();
    if token.is_empty() {
        return Ok(());
    }
    match token {
        "off" | "direct" => {
            // `off`/`direct` chained with other entries is technically
            // accepted by Go (it means "stop here"), so we keep it but
            // mark the chain. Pure `off` alone is handled at the top of
            // parse_proxy_chain; if `off` appears mid-chain, treat as a
            // terminator.
            if token == "off" {
                entries.push(ProxyEntry::Off);
            } else {
                entries.push(ProxyEntry::Direct);
            }
            Ok(())
        }
        _ => {
            let url = Url::parse(token).map_err(|e| ProxyParseError::InvalidUrl {
                url: token.to_string(),
                detail: e.to_string(),
            })?;
            if url.scheme() == "http" {
                tracing::warn!(
                    proxy_url = %url,
                    "$GOPROXY entry uses http://; module list is sent to the proxy in cleartext"
                );
            }
            entries.push(ProxyEntry::Url {
                url,
                fall_through_on_404_only,
            });
            Ok(())
        }
    }
}

fn default_chain() -> ProxyChain {
    let url = Url::parse("https://proxy.golang.org").expect("hardcoded URL parses");
    ProxyChain {
        entries: vec![
            ProxyEntry::Url {
                url,
                fall_through_on_404_only: true,
            },
            ProxyEntry::Direct,
        ],
    }
}

// --------------------------------------------------------------------
// $GOPRIVATE
// --------------------------------------------------------------------

/// One pattern from a `$GOPRIVATE` value. Each pattern is split into
/// segments (on `/`); each segment is either a literal or a glob with
/// `*` matching any chars except `/`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivatePattern {
    segments: Vec<PatternSegment>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatternSegment {
    Literal(String),
    Glob(String),
}

#[derive(Clone, Debug, Default)]
pub struct PrivatePatterns {
    patterns: Vec<PrivatePattern>,
}

impl PrivatePatterns {
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Returns true if `module_path` matches any pattern.
    pub fn matches(&self, module_path: &str) -> bool {
        self.patterns.iter().any(|p| pattern_matches(p, module_path))
    }
}

/// Parse a comma-separated `$GOPRIVATE` value. Empty/None input →
/// empty `PrivatePatterns` (matches no module). Bad patterns are
/// logged at `tracing::warn` and skipped (fail-open: the consequence
/// of a misparsed pattern is "module fetched from public proxy when
/// user wanted privacy" — the warn lets the user fix their env).
pub fn parse_private_patterns(env_value: &str) -> PrivatePatterns {
    let mut patterns = Vec::new();
    for raw in env_value.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let segments: Vec<PatternSegment> = raw
            .split('/')
            .map(|seg| {
                if seg.contains('*') {
                    PatternSegment::Glob(seg.to_string())
                } else {
                    PatternSegment::Literal(seg.to_string())
                }
            })
            .collect();
        if segments.is_empty() {
            tracing::warn!(pattern = raw, "$GOPRIVATE entry parses to zero segments; skipped");
            continue;
        }
        patterns.push(PrivatePattern { segments });
    }
    PrivatePatterns { patterns }
}

fn pattern_matches(pattern: &PrivatePattern, module_path: &str) -> bool {
    let module_segments: Vec<&str> = module_path.split('/').collect();
    if module_segments.len() < pattern.segments.len() {
        return false;
    }
    for (idx, seg) in pattern.segments.iter().enumerate() {
        let m = module_segments[idx];
        match seg {
            PatternSegment::Literal(lit) => {
                if lit != m {
                    return false;
                }
            }
            PatternSegment::Glob(glob) => {
                if !glob_segment_matches(glob, m) {
                    return false;
                }
            }
        }
    }
    // The pattern's segments matched as a prefix; the module path may
    // have additional segments (per Go's prefix-match semantics).
    true
}

/// Match a single segment against a glob pattern where `*` matches any
/// chars except `/`. Since we operate one segment at a time, `/` cannot
/// appear in `s`, so `*` effectively matches any chars.
fn glob_segment_matches(pattern: &str, s: &str) -> bool {
    // Implement a small backtracking matcher. Go's `path.Match` does
    // similar; we don't support `?` or `[..]` because GOPRIVATE patterns
    // in the wild don't use them.
    glob_match_inner(pattern.as_bytes(), s.as_bytes())
}

fn glob_match_inner(pat: &[u8], s: &[u8]) -> bool {
    let mut pi = 0;
    let mut si = 0;
    let mut star_pat: Option<usize> = None;
    let mut star_s: usize = 0;
    while si < s.len() {
        if pi < pat.len() && pat[pi] == b'*' {
            star_pat = Some(pi);
            star_s = si;
            pi += 1;
        } else if pi < pat.len() && pat[pi] == s[si] {
            pi += 1;
            si += 1;
        } else if let Some(sp) = star_pat {
            pi = sp + 1;
            star_s += 1;
            si = star_s;
        } else {
            return false;
        }
    }
    while pi < pat.len() && pat[pi] == b'*' {
        pi += 1;
    }
    pi == pat.len()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    // --------- $GOPROXY ----------

    #[test]
    fn goproxy_default_when_unset() {
        let c = parse_proxy_chain(None).unwrap();
        assert_eq!(c.entries.len(), 2);
        match &c.entries[0] {
            ProxyEntry::Url { url, fall_through_on_404_only } => {
                assert_eq!(url.as_str(), "https://proxy.golang.org/");
                assert!(*fall_through_on_404_only);
            }
            other => panic!("expected first entry to be a URL, got {other:?}"),
        }
        assert_eq!(c.entries[1], ProxyEntry::Direct);
    }

    #[test]
    fn goproxy_default_when_empty() {
        let c = parse_proxy_chain(Some("")).unwrap();
        assert_eq!(c.entries.len(), 2);
    }

    #[test]
    fn goproxy_off() {
        let c = parse_proxy_chain(Some("off")).unwrap();
        assert_eq!(c.entries, vec![ProxyEntry::Off]);
        assert!(c.is_off());
    }

    #[test]
    fn goproxy_direct() {
        let c = parse_proxy_chain(Some("direct")).unwrap();
        assert_eq!(c.entries, vec![ProxyEntry::Direct]);
    }

    #[test]
    fn goproxy_url_then_direct_with_comma() {
        let c = parse_proxy_chain(Some("https://internal.example.com,direct")).unwrap();
        assert_eq!(c.entries.len(), 2);
        match &c.entries[0] {
            ProxyEntry::Url { url, fall_through_on_404_only } => {
                assert_eq!(url.as_str(), "https://internal.example.com/");
                assert!(*fall_through_on_404_only);
            }
            other => panic!("got {other:?}"),
        }
        assert_eq!(c.entries[1], ProxyEntry::Direct);
    }

    #[test]
    fn goproxy_pipe_separator_means_fall_through_on_any_error() {
        let c = parse_proxy_chain(Some("https://a.example.com|https://b.example.com")).unwrap();
        assert_eq!(c.entries.len(), 2);
        // First entry: no preceding separator → defaults to comma semantics.
        match &c.entries[0] {
            ProxyEntry::Url { fall_through_on_404_only, .. } => {
                assert!(*fall_through_on_404_only);
            }
            other => panic!("{other:?}"),
        }
        // Second entry: preceded by `|` → fall_through_on_404_only = false.
        match &c.entries[1] {
            ProxyEntry::Url { fall_through_on_404_only, .. } => {
                assert!(!*fall_through_on_404_only);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn goproxy_invalid_url_returns_err() {
        let err = parse_proxy_chain(Some("not a url")).unwrap_err();
        assert!(matches!(err, ProxyParseError::InvalidUrl { .. }));
    }

    #[test]
    fn goproxy_http_scheme_parses_but_warns() {
        // The warn is emitted via `tracing::warn`; we don't assert on
        // log capture here (covered by integration tests). We do assert
        // that http://foo parses successfully.
        let c = parse_proxy_chain(Some("http://insecure.example.com")).unwrap();
        assert_eq!(c.entries.len(), 1);
    }

    // --------- $GOPRIVATE ----------

    #[test]
    fn goprivate_empty_matches_nothing() {
        let p = parse_private_patterns("");
        assert!(p.is_empty());
        assert!(!p.matches("github.com/foo/bar"));
    }

    #[test]
    fn goprivate_exact_prefix_matches() {
        let p = parse_private_patterns("github.com/our-org");
        assert!(p.matches("github.com/our-org/foo"));
        assert!(p.matches("github.com/our-org/foo/bar"));
        assert!(!p.matches("github.com/other-org/foo"));
    }

    #[test]
    fn goprivate_glob_segment_matches_within_segment_only() {
        let p = parse_private_patterns("github.com/our-org/*");
        assert!(p.matches("github.com/our-org/foo"));
        assert!(p.matches("github.com/our-org/foo/bar"));
        assert!(!p.matches("github.com/other-org/foo"));
    }

    #[test]
    fn goprivate_subdomain_glob() {
        let p = parse_private_patterns("*.corp.example.com");
        assert!(p.matches("internal.corp.example.com"));
        assert!(p.matches("a.corp.example.com/some/path"));
        assert!(!p.matches("corp.example.com"));
        // Leading subdomain MUST be present; bare host doesn't match.
        assert!(!p.matches("evil.com/x.corp.example.com/path"));
    }

    #[test]
    fn goprivate_multi_pattern_either_match() {
        let p = parse_private_patterns("github.com/a/*,github.com/b/*");
        assert!(p.matches("github.com/a/foo"));
        assert!(p.matches("github.com/b/bar"));
        assert!(!p.matches("github.com/c/baz"));
    }

    #[test]
    fn goprivate_glob_with_partial_segment() {
        let p = parse_private_patterns("github.com/foo*-private/*");
        assert!(p.matches("github.com/foo-private/x"));
        assert!(p.matches("github.com/foobar-private/x"));
        assert!(!p.matches("github.com/bar-private/x"));
    }

    #[test]
    fn goprivate_module_path_shorter_than_pattern_does_not_match() {
        let p = parse_private_patterns("github.com/our-org/repo/sub");
        assert!(!p.matches("github.com/our-org/repo"));
    }

    #[test]
    fn glob_inner_matches_simple_cases() {
        assert!(glob_match_inner(b"*", b""));
        assert!(glob_match_inner(b"*", b"anything"));
        assert!(glob_match_inner(b"a*b", b"axxxb"));
        assert!(glob_match_inner(b"a*b", b"ab"));
        assert!(!glob_match_inner(b"a*b", b"axxxc"));
        assert!(glob_match_inner(b"foo", b"foo"));
        assert!(!glob_match_inner(b"foo", b"bar"));
    }
}
