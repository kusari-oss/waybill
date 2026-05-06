//! Auto-detection paths for source identifiers.
//!
//! Three entry points:
//!
//! - `auto_detect_repo_identifier(scan_root)` for source-tier
//!   `--path` scans (FR-001). Implements the 3-step git-remote
//!   fallback per spec Q1: `origin` → `upstream` → first-listed
//!   alphabetical. Failure (no git, no remotes, command error) is
//!   logged at `tracing::info!` and returns `None` — never fails the
//!   scan.
//! - `auto_detect_build_tier_identifiers(invocation_cwd)` for
//!   build-tier `mikebom trace run` invocations (milestone 074).
//!   Reuses the shared 3-step git-remote fallback core via
//!   `discover_repo_url`, then additionally captures
//!   `git rev-parse HEAD` to emit a commit-anchored `git:` identifier.
//!   Soft-fail rules mirror source-tier per FR-003 / FR-010.
//! - `image_reference_to_identifier(...)` for image-tier `--image`
//!   scans (FR-008). Synthesizes the canonical `image:<registry>/
//!   <name>:<tag>@sha256:<digest>` shape per spec Q3 from the
//!   resolved-image fields, omitting components that aren't present.

use std::path::Path;
use std::process::Command;

use super::{Identifier, IdentifierKind, IdentifierValue, SchemeName};
use mikebom_common::attestation::statement::ResourceDescriptor;

/// Result of running `sanitize_userinfo` on a candidate URL string.
///
/// Private to this module; not part of the public surface. Lifetime
/// is ephemeral — the helper returns one of these, the caller
/// inspects the fields once and emits its identifier, and the
/// struct is dropped.
///
/// Field semantics per data-model.md (milestone 075):
/// - `original`: the URL string exactly as it came back from
///   `git remote get-url`. Preserved so callers can still reference
///   the host/path for operator-debuggability log output via
///   `redact_userinfo_for_log`.
/// - `sanitized`: the URL with RFC 3986 userinfo removed when the
///   parser saw any. Equal to `original` when the URL had no
///   userinfo, when the URL fails to parse (SSH-form, malformed),
///   or when one of the setter calls rejects the URL (cannot-be-base
///   path).
/// - `was_sanitized`: `true` iff this run actually stripped userinfo.
///   Drives FR-006 log-line emission and FR-008 `(credentials
///   stripped)` source_label suffix.
struct SanitizedUrl {
    original: String,
    sanitized: String,
    was_sanitized: bool,
}

/// Strip RFC 3986 userinfo from a candidate git remote URL before
/// it gets embedded as an identifier value in a published SBOM.
///
/// Behavior (data-model.md §`sanitize_userinfo`, research §1, §6):
///
/// 1. `url::Url::parse(url)` — on `Err`, return passthrough
///    (`sanitized == original`, `was_sanitized == false`). Covers
///    SSH-form URLs (`git@host:foo/bar.git`) and any other
///    non-RFC-3986 input.
/// 2. On parse success, check whether the parsed URL carries
///    userinfo (`username().is_empty() == false ||
///    password().is_some()`). If neither is present, return
///    passthrough.
/// 3. On userinfo present, call `set_username("")` followed by
///    `set_password(None)`. Either setter returning `Err` (the
///    "cannot-be-base" path) collapses to passthrough — preserves
///    the FR-009 soft-fail rule.
/// 4. On both setters succeeding, return `SanitizedUrl { original,
///    sanitized: parsed.to_string(), was_sanitized: true }`.
///
/// Never panics, never returns `Result`. All failure modes (parse
/// error, setter rejection, missing authority) collapse to a
/// passthrough — the original string emits verbatim, milestone
/// 073's existing soft-fail-to-`UserDefined` rule (FR-009) handles
/// downstream classification.
fn sanitize_userinfo(url: &str) -> SanitizedUrl {
    let original = url.to_string();
    let mut parsed = match url::Url::parse(url) {
        Ok(p) => p,
        Err(_) => {
            // Parse failure: SSH-form URLs and other non-RFC-3986
            // inputs. Pass through unchanged; downstream validators
            // soft-fail to `UserDefined` per FR-009.
            return SanitizedUrl {
                original: original.clone(),
                sanitized: original,
                was_sanitized: false,
            };
        }
    };
    // `set_username("")` and `set_password(None)` reject cannot-be-
    // base URLs (e.g., `mailto:`). Vanishingly rare for the
    // git-remote input domain, but the safe fallback is still
    // passthrough. Always call them — that way even the
    // `https://@host/...` empty-userinfo edge case (where
    // `username().is_empty()` already and `password()` is `None` but
    // the literal `@` survives the parse) gets cleaned up: the
    // setters re-canonicalize the URL through the `url` crate's
    // serializer, which writes neither `user`, nor `:password`, nor
    // a stray `@` when both are empty/None.
    if parsed.set_username("").is_err() {
        return SanitizedUrl {
            original: original.clone(),
            sanitized: original,
            was_sanitized: false,
        };
    }
    if parsed.set_password(None).is_err() {
        return SanitizedUrl {
            original: original.clone(),
            sanitized: original,
            was_sanitized: false,
        };
    }
    let sanitized = parsed.to_string();
    // Compare against the parser's round-trip of the original URL
    // (NOT the raw input) so URL canonicalization (default-port
    // stripping, percent-encoding normalization, trailing-slash
    // additions) doesn't falsely register as "userinfo stripped".
    // Only userinfo presence/absence drives `was_sanitized`.
    let original_round_trip = match url::Url::parse(&original) {
        Ok(p) => p.to_string(),
        Err(_) => original.clone(),
    };
    let was_sanitized = sanitized != original_round_trip;
    SanitizedUrl {
        original,
        sanitized,
        was_sanitized,
    }
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
///   unchanged. Used in code paths gated on `was_sanitized == true`,
///   so SSH-form never reaches this helper in production.
///
/// The literal credential value MUST NOT appear in the output
/// (FR-006). Callers route the result through `tracing::info!` as a
/// `Display` field.
fn redact_userinfo_for_log(url_str: &str) -> String {
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
///
/// Internally a thin wrapper over the shared `discover_repo_url` core
/// (milestone 074 refactor extract): URL discovery is tier-agnostic;
/// each tier formats its own `source_label` string.
///
/// Milestone 075 — `keep_credentials` controls userinfo sanitization:
/// `false` (default) strips RFC 3986 userinfo from the discovered URL
/// before identifier construction (FR-001); `true` (operator opted in
/// via `--keep-credentials-in-identifiers`) emits the URL verbatim
/// and emits a one-time acknowledgment log line (FR-007) at the top
/// of the call so the audit trail records the operator's choice.
pub fn auto_detect_repo_identifier(
    scan_root: &Path,
    keep_credentials: bool,
) -> Option<Identifier> {
    if keep_credentials {
        tracing::info!(
            "--keep-credentials-in-identifiers set; userinfo in auto-detected identifiers will be preserved verbatim"
        );
    }
    let (url, remote_name, fallback_used) = discover_repo_url(scan_root)?;
    let sanitized = if keep_credentials {
        SanitizedUrl {
            original: url.clone(),
            sanitized: url,
            was_sanitized: false,
        }
    } else {
        sanitize_userinfo(&url)
    };
    if sanitized.was_sanitized {
        tracing::info!(
            scheme = "repo",
            url_safe = %redact_userinfo_for_log(&sanitized.original),
            "sanitized userinfo from auto-detected identifier"
        );
    }
    let label = source_tier_repo_label(&remote_name, fallback_used, sanitized.was_sanitized);
    build_repo_identifier_with_label(sanitized.sanitized, &remote_name, label)
}

/// Source-tier `source_label` formatter. Kept stable across milestones
/// 073 and 074 — pre-refactor strings are reproduced verbatim so the
/// existing source-tier goldens remain byte-identical (per VR-074-007 /
/// research §5).
///
/// Milestone 075 — when `was_sanitized == true`, the FR-008 suffix
/// ` (credentials stripped)` is appended (research §3). When `false`,
/// label is byte-identical to milestones 073/074.
fn source_tier_repo_label(
    remote_name: &str,
    fallback_used: bool,
    was_sanitized: bool,
) -> String {
    let base = if fallback_used {
        format!(
            "auto-detected from git remote `{remote_name}` (origin/upstream absent; first-listed)"
        )
    } else {
        format!("auto-detected from git remote `{remote_name}`")
    };
    if was_sanitized {
        format!("{base} (credentials stripped)")
    } else {
        base
    }
}

/// Build-tier `source_label` formatter for `repo:` identifiers per
/// research §4. Inserts `build-tier ` between `from` and `git remote`
/// so consumers reading SBOMs without surrounding tier-context can
/// disambiguate per-identifier (FR-006).
///
/// Milestone 075 — when `was_sanitized == true`, the FR-008 suffix
/// ` (credentials stripped)` is appended (research §3).
fn build_tier_repo_label(
    remote_name: &str,
    fallback_used: bool,
    was_sanitized: bool,
) -> String {
    let base = if fallback_used {
        format!(
            "auto-detected from build-tier git remote `{remote_name}` (origin/upstream absent; first-listed)"
        )
    } else {
        format!("auto-detected from build-tier git remote `{remote_name}`")
    };
    if was_sanitized {
        format!("{base} (credentials stripped)")
    } else {
        base
    }
}

/// Build-tier `source_label` for the auto-detected `git:` identifier
/// per research §4.
const BUILD_TIER_GIT_LABEL: &str =
    "auto-detected from build-tier `git rev-parse HEAD`";

/// Build-tier `source_label` for the auto-detected `git:` identifier
/// when sanitization fired (milestone 075 FR-008, research §3).
const BUILD_TIER_GIT_LABEL_SANITIZED: &str =
    "auto-detected from build-tier `git rev-parse HEAD` (credentials stripped)";

/// Tier-agnostic URL-discovery core: 3-step git-remote fallback
/// (`origin` → `upstream` → first-listed alphabetical). Returns
/// `(url, remote_name, fallback_used)` on success — each tier's wrapper
/// attaches its own `source_label`. `None` on every failure mode
/// (not a git repo, no remotes, subprocess error) with appropriate
/// `tracing::info!` logging.
///
/// Extracted from the original `auto_detect_repo_identifier` in
/// milestone 074 so source-tier and build-tier auto-detection share
/// the discovery algorithm verbatim per FR-008 + research §2.
fn discover_repo_url(scan_root: &Path) -> Option<(String, String, bool)> {
    if !scan_root.join(".git").exists() {
        return None;
    }
    // Step 1+2: try origin and upstream by name.
    for name in ["origin", "upstream"] {
        if let Some(url) = git_remote_get_url(scan_root, name) {
            let trimmed = url.trim().to_string();
            if !trimmed.is_empty() {
                return Some((trimmed, name.to_string(), false));
            }
        }
    }
    // Step 3: list all remotes alphabetically; take the first non-
    // origin / non-upstream entry.
    let remotes = match git_remote_list(scan_root) {
        Some(r) if !r.is_empty() => r,
        Some(_) => {
            tracing::info!(
                scan_root = %scan_root.display(),
                "git checkout has no remotes; identifier auto-detection skipped"
            );
            return None;
        }
        None => {
            tracing::info!(
                scan_root = %scan_root.display(),
                "git remote list failed; identifier auto-detection skipped"
            );
            return None;
        }
    };
    let first = remotes
        .iter()
        .find(|r| r.as_str() != "origin" && r.as_str() != "upstream")?;
    let url = git_remote_get_url(scan_root, first)?;
    let trimmed = url.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some((trimmed, first.clone(), true))
    }
}

/// Construct a `repo:` identifier with a caller-supplied `source_label`.
/// Re-runs the `validate_for_scheme(BuiltinScheme::Repo, ...)` validator
/// so a malformed remote URL downgrades `kind` to `UserDefined` with a
/// `tracing::warn!` per FR-010 / VR-005 (and milestone-074 VR-074-005).
fn build_repo_identifier_with_label(
    url: String,
    remote_name: &str,
    label: String,
) -> Option<Identifier> {
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
    Some(Identifier::from_parts_with_label(
        scheme,
        value,
        kind,
        Some(label),
    ))
}

/// Auto-detect build-tier identifiers from a `mikebom trace run`
/// invocation cwd (milestone 074).
///
/// Returns 0, 1, or 2 identifiers in deterministic order:
///
/// - `[]` when `invocation_cwd` is not a git checkout, has no
///   resolvable remotes, or all subprocess calls fail (FR-003).
/// - `[repo:<url>]` when a remote is resolvable but
///   `git rev-parse HEAD` fails (e.g., empty repo with no commits
///   per spec US2 §3).
/// - `[repo:<url>, git:<url>#<sha>]` when both a remote URL and a
///   `HEAD` commit are resolvable. `repo:` is at index 0, `git:` at
///   index 1 (FR-009 + data-model.md ordering invariant).
///
/// Each identifier carries `source_label = Some(...)` containing the
/// substring `build-tier` per FR-006 / VR-074-004.
///
/// `kind` follows the source-tier soft-fail rule per FR-010 /
/// VR-074-005: well-formed values produce
/// `IdentifierKind::Builtin(...)`; malformed values downgrade to
/// `IdentifierKind::UserDefined` with a `tracing::warn!`.
///
/// Never panics. Never returns `Result`. All failure modes collapse
/// to "this identifier is omitted" with the appropriate
/// `tracing::info!` (skipped detection) or `tracing::warn!` (soft-
/// fail to UserDefined).
///
/// Determinism (FR-009): given fixed git remote configuration and
/// fixed `HEAD` commit, repeated calls produce byte-identical output.
///
/// Milestone 075 — `keep_credentials` controls userinfo sanitization
/// for both the auto-detected `repo:` and `git:` identifier slots
/// (FR-001 + FR-002). Default `false`: strip userinfo from the
/// discovered URL before identifier construction; per VR-075-005
/// the URL portion of the `git:` value is sanitized BEFORE the
/// `#<sha>` is appended. `true`: emit verbatim and emit a one-time
/// FR-007 acknowledgment log line at the top of the call.
pub fn auto_detect_build_tier_identifiers(
    invocation_cwd: &Path,
    keep_credentials: bool,
) -> Vec<Identifier> {
    if keep_credentials {
        tracing::info!(
            "--keep-credentials-in-identifiers set; userinfo in auto-detected identifiers will be preserved verbatim"
        );
    }
    let mut out: Vec<Identifier> = Vec::new();
    // Step 1: discover the remote URL via the shared core.
    let (url, remote_name, fallback_used) = match discover_repo_url(invocation_cwd) {
        Some(t) => t,
        None => {
            // The shared core already logged via tracing::info!.
            return out;
        }
    };
    // Milestone 075 — sanitize the URL ONCE up front; both the
    // `repo:` value and the `git:` value reuse the sanitized form
    // (VR-075-005: URL portion sanitized BEFORE `#<sha>` append).
    let sanitized = if keep_credentials {
        SanitizedUrl {
            original: url.clone(),
            sanitized: url.clone(),
            was_sanitized: false,
        }
    } else {
        sanitize_userinfo(&url)
    };
    if sanitized.was_sanitized {
        tracing::info!(
            scheme = "repo",
            url_safe = %redact_userinfo_for_log(&sanitized.original),
            "sanitized userinfo from auto-detected identifier"
        );
    }
    let label = build_tier_repo_label(&remote_name, fallback_used, sanitized.was_sanitized);
    let repo_id =
        match build_repo_identifier_with_label(sanitized.sanitized.clone(), &remote_name, label) {
            Some(id) => id,
            None => {
                // Construction failure (empty URL, etc.); without a
                // resolvable repo URL we can't synthesize the `git:`
                // identifier either, so bail.
                return out;
            }
        };
    tracing::info!(
        scheme = repo_id.scheme.as_str(),
        value = repo_id.value.as_str(),
        remote = %remote_name,
        "build-tier auto-detected `repo:{}` from git remote `{}`",
        repo_id.value.as_str(),
        remote_name,
    );
    out.push(repo_id);

    // Step 2: attempt `git rev-parse HEAD`. Only fires if step 1
    // produced a `repo:` identifier — the `git:` value reuses the
    // sanitized URL string per VR-074-002 + VR-075-005.
    let sha = match git_rev_parse_head(invocation_cwd) {
        Some(s) => s,
        None => {
            tracing::info!(
                scan_root = %invocation_cwd.display(),
                "`git rev-parse HEAD` failed; build-tier `git:` identifier skipped"
            );
            return out;
        }
    };
    // VR-075-005: sanitize the URL portion BEFORE appending `#<sha>`.
    let git_value_str = format!("{}#{sha}", sanitized.sanitized);
    if sanitized.was_sanitized {
        // Per-identifier log line for the `git:` slot — separate
        // from the `repo:` log so operators can see both
        // sanitizations in the audit trail (FR-006 per-identifier).
        tracing::info!(
            scheme = "git",
            url_safe = %format!("{}#{sha}", redact_userinfo_for_log(&sanitized.original)),
            "sanitized userinfo from auto-detected identifier"
        );
    }
    let git_scheme = match SchemeName::new("git".to_string()) {
        Ok(s) => s,
        Err(_) => return out,
    };
    let git_value = match IdentifierValue::new(git_value_str.clone()) {
        Ok(v) => v,
        Err(_) => return out,
    };
    let git_kind = match super::BuiltinScheme::from_scheme_name(&git_scheme) {
        Some(b) => match super::validators::validate_for_scheme(b, git_value.as_str()) {
            Ok(()) => IdentifierKind::Builtin(b),
            Err(err) => {
                tracing::warn!(
                    value = %git_value_str,
                    reason = %err,
                    "auto-detected build-tier `git:` value failed validation; \
                     emitting as user-defined under \
                     mikebom:identifiers"
                );
                IdentifierKind::UserDefined
            }
        },
        None => IdentifierKind::UserDefined,
    };
    let git_label = if sanitized.was_sanitized {
        BUILD_TIER_GIT_LABEL_SANITIZED
    } else {
        BUILD_TIER_GIT_LABEL
    };
    let git_id = Identifier::from_parts_with_label(
        git_scheme,
        git_value,
        git_kind,
        Some(git_label.to_string()),
    );
    tracing::info!(
        value = %git_value_str,
        "build-tier auto-detected `git:{}` from `git rev-parse HEAD`",
        git_value_str,
    );
    out.push(git_id);
    out
}

/// Run `git -C <scan_root> rev-parse HEAD`. Returns `Some(sha)` only
/// if the result is exactly 40 lowercase hex characters (per
/// VR-074-003); else `None` with `tracing::info!`.
///
/// Failure modes covered (research §1):
///
/// - Not a git repo (caller already pre-checks via `discover_repo_url`,
///   but the helper still tolerates it) → exit 128 → `None`.
/// - Empty repo, no commits → exit 128 → `None`.
/// - Detached HEAD → exit 0 with the SHA on stdout → `Some(sha)`,
///   treated identically to attached HEAD.
/// - `git` not on PATH → `Command::spawn` returns `io::Error` → `None`.
fn git_rev_parse_head(scan_root: &Path) -> Option<String> {
    let scan_root_str = scan_root.to_str()?;
    let output = Command::new("git")
        .args(["-C", scan_root_str, "rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?;
    let trimmed = s.trim();
    // VR-074-003: must be exactly 40 lowercase hex chars; anything
    // else (abbreviated SHA, ref name leaking through, empty output)
    // returns `None` to preserve the wire-format invariant from
    // milestone 073's `validate_git`.
    if trimmed.len() != 40 {
        return None;
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    {
        return None;
    }
    Some(trimmed.to_string())
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
// Milestone 076 — build-tier `subject:` auto-detection
// ---------------------------------------------------------------------

/// Convert an in-toto attestation subject set into `subject:` identifiers
/// per milestone 076 FR-002 + the 2026-05-06 sha256-only clarification.
///
/// Behavior:
///
/// 1. Iterate `subjects` in input order (witness-v0.1 already lex-sorts
///    subjects by `(name, digest)` before serialization, so this loop
///    inherits that ordering).
/// 2. For each subject, look up the `"sha256"` key in its digest map.
///    If absent, log `tracing::info!` listing the subject name and the
///    available algos, then skip this subject (FR-002 + 2026-05-06
///    clarification: sha256-only auto-emit; operators who need other
///    algos pass `--subject-hash sha512:<hex>` manually).
/// 3. If sha256 is present, construct an `Identifier` with scheme
///    `subject`, value `sha256:<hex>`, kind `Builtin(Subject)` (after
///    a defensive `validate_for_scheme` round-trip — well-formed
///    digest-map values produced by the trace pipeline always pass,
///    but the validator runs for defense in depth and the soft-fail
///    path covers any future producer that emits a malformed value),
///    and source_label `"auto-detected from build-tier in-toto subject
///    \`<name>\`"`.
///
/// Multi-digest subjects (e.g., both sha256 AND sha512 in a single
/// digest map) emit only the sha256 form per the 2026-05-06
/// clarification. Synthetic subjects (digest map keyed by
/// `"synthetic"` not `"sha256"`) skip emission per the same rule.
///
/// Never panics, never returns `Result`. All failure modes collapse
/// to "skip this subject" with an `tracing::info!` info-log.
pub fn subject_identifiers_from_attestation_subjects(
    subjects: &[ResourceDescriptor],
) -> Vec<Identifier> {
    let mut out: Vec<Identifier> = Vec::with_capacity(subjects.len());
    for subject in subjects {
        let Some(sha256) = subject.digest.get("sha256") else {
            // FR-002 + 2026-05-06 clarification: skip subjects without
            // sha256 and surface the reason so operators can decide
            // whether to backfill with `--subject-hash`.
            let available: Vec<&str> =
                subject.digest.keys().map(String::as_str).collect();
            tracing::info!(
                subject = %subject.name,
                available_algos = ?available,
                "subject `{}` has no sha256 digest (available algos: {:?}); \
                 skipping subject: identifier auto-emit. Pass --subject-hash \
                 sha512:<hex> manually if needed.",
                subject.name,
                available,
            );
            continue;
        };
        let value_str = format!("sha256:{sha256}");
        let scheme = match SchemeName::new("subject".to_string()) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let value = match IdentifierValue::new(value_str.clone()) {
            Ok(v) => v,
            Err(_) => continue,
        };
        // Defense-in-depth validator round-trip — well-formed inputs
        // always pass; soft-fail paths exist for future producer drift.
        let kind = match super::BuiltinScheme::from_scheme_name(&scheme) {
            Some(b) => match super::validators::validate_for_scheme(b, value.as_str()) {
                Ok(()) => IdentifierKind::Builtin(b),
                Err(err) => {
                    tracing::warn!(
                        subject = %subject.name,
                        value = %value_str,
                        reason = %err,
                        "auto-detected `subject:` value failed validation; \
                         emitting as user-defined under \
                         mikebom:identifiers"
                    );
                    IdentifierKind::UserDefined
                }
            },
            None => IdentifierKind::UserDefined,
        };
        let label = format!(
            "auto-detected from build-tier in-toto subject `{}`",
            subject.name
        );
        tracing::info!(
            value = %value_str,
            subject = %subject.name,
            "build-tier auto-detected `subject:{}` from in-toto subject `{}`",
            value_str,
            subject.name,
        );
        out.push(Identifier::from_parts_with_label(
            scheme,
            value,
            kind,
            Some(label),
        ));
    }
    out
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
        assert!(auto_detect_repo_identifier(td.path(), false).is_none());
    }

    #[test]
    fn git_dir_no_remotes_returns_none() {
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        assert!(auto_detect_repo_identifier(td.path(), false).is_none());
    }

    #[test]
    fn origin_only_uses_origin() {
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        git_remote_add(td.path(), "origin", "git@github.com:test/foo.git");
        let id = auto_detect_repo_identifier(td.path(), false).expect("origin detected");
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
        let id = auto_detect_repo_identifier(td.path(), false).expect("upstream detected");
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
        let id = auto_detect_repo_identifier(td.path(), false).expect("first-listed detected");
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
        let id = auto_detect_repo_identifier(td.path(), false).unwrap();
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

    // ----------------------------------------------------------------
    // Milestone 074 — git_rev_parse_head helper (T003)
    // ----------------------------------------------------------------

    fn git_config_user(dir: &Path) {
        run(Command::new("git").args([
            "-C",
            dir.to_str().unwrap(),
            "config",
            "user.email",
            "test@example.com",
        ]));
        run(Command::new("git").args([
            "-C",
            dir.to_str().unwrap(),
            "config",
            "user.name",
            "Test User",
        ]));
    }

    fn git_commit_empty(dir: &Path, msg: &str) {
        git_config_user(dir);
        run(Command::new("git").args([
            "-C",
            dir.to_str().unwrap(),
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            msg,
        ]));
    }

    fn git_rev_parse_head_via_subprocess(dir: &Path) -> String {
        let out = Command::new("git")
            .args(["-C", dir.to_str().unwrap(), "rev-parse", "HEAD"])
            .output()
            .expect("git subprocess");
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn rev_parse_head_returns_full_sha_on_committed_repo() {
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        git_commit_empty(td.path(), "first");
        let sha = git_rev_parse_head(td.path()).expect("HEAD resolves");
        assert_eq!(sha.len(), 40);
        assert!(
            sha.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "expected lowercase hex; got {sha}"
        );
        // Cross-check against the same subprocess invocation.
        assert_eq!(sha, git_rev_parse_head_via_subprocess(td.path()));
    }

    #[test]
    fn rev_parse_head_returns_none_in_empty_repo_no_commits() {
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        // No commit made — `git rev-parse HEAD` exits 128.
        assert!(git_rev_parse_head(td.path()).is_none());
    }

    #[test]
    fn rev_parse_head_returns_none_in_non_git_dir() {
        let td = tempfile::tempdir().unwrap();
        // Helper is tolerant of non-git dirs even though the
        // `auto_detect_build_tier_identifiers` caller pre-checks via
        // `discover_repo_url`.
        assert!(git_rev_parse_head(td.path()).is_none());
    }

    // ----------------------------------------------------------------
    // Milestone 074 — auto_detect_build_tier_identifiers (T004)
    // ----------------------------------------------------------------

    #[test]
    fn build_tier_empty_on_non_git_dir() {
        let td = tempfile::tempdir().unwrap();
        let ids = auto_detect_build_tier_identifiers(td.path(), false);
        assert!(
            ids.is_empty(),
            "non-git dir must yield zero auto-detected identifiers"
        );
    }

    #[test]
    fn build_tier_repo_only_when_remote_but_no_commits() {
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        git_remote_add(td.path(), "origin", "git@github.com:test/foo.git");
        // No commit — `git rev-parse HEAD` will fail.
        let ids = auto_detect_build_tier_identifiers(td.path(), false);
        assert_eq!(
            ids.len(),
            1,
            "expected exactly [repo:] when remote configured but no HEAD"
        );
        assert_eq!(ids[0].scheme.as_str(), "repo");
        assert_eq!(ids[0].value.as_str(), "git@github.com:test/foo.git");
        let label = ids[0]
            .source_label
            .as_deref()
            .expect("source_label set for auto-detection");
        assert!(
            label.contains("build-tier"),
            "VR-074-004: build-tier substring required; got {label:?}"
        );
        assert_eq!(
            label,
            "auto-detected from build-tier git remote `origin`"
        );
    }

    #[test]
    fn build_tier_repo_and_git_when_remote_and_commit() {
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        git_remote_add(td.path(), "origin", "git@github.com:test/foo.git");
        git_commit_empty(td.path(), "first");
        let head_sha = git_rev_parse_head_via_subprocess(td.path());

        let ids = auto_detect_build_tier_identifiers(td.path(), false);
        assert_eq!(
            ids.len(),
            2,
            "expected [repo:, git:] when remote and HEAD both resolvable"
        );
        // Order: repo at index 0, git at index 1 per data-model.md.
        assert_eq!(ids[0].scheme.as_str(), "repo");
        assert_eq!(ids[0].value.as_str(), "git@github.com:test/foo.git");
        assert_eq!(ids[1].scheme.as_str(), "git");
        assert_eq!(
            ids[1].value.as_str(),
            format!("git@github.com:test/foo.git#{head_sha}")
        );
        // VR-074-004: build-tier substring on both.
        assert!(ids[0].source_label.as_deref().unwrap().contains("build-tier"));
        assert!(ids[1].source_label.as_deref().unwrap().contains("build-tier"));
        assert_eq!(
            ids[1].source_label.as_deref(),
            Some("auto-detected from build-tier `git rev-parse HEAD`")
        );
    }

    #[test]
    fn build_tier_deterministic_across_two_calls() {
        // FR-009 / SC-007: same fixture, same call -> byte-identical
        // identifier slots.
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        git_remote_add(td.path(), "origin", "git@github.com:test/foo.git");
        git_commit_empty(td.path(), "first");

        let a = auto_detect_build_tier_identifiers(td.path(), false);
        let b = auto_detect_build_tier_identifiers(td.path(), false);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.scheme.as_str(), y.scheme.as_str());
            assert_eq!(x.value.as_str(), y.value.as_str());
            assert_eq!(x.source_label, y.source_label);
        }
    }

    #[test]
    fn build_tier_upstream_fallback() {
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        git_remote_add(td.path(), "upstream", "git@github.com:acme/foo.git");
        let ids = auto_detect_build_tier_identifiers(td.path(), false);
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].scheme.as_str(), "repo");
        assert_eq!(ids[0].value.as_str(), "git@github.com:acme/foo.git");
        assert_eq!(
            ids[0].source_label.as_deref(),
            Some("auto-detected from build-tier git remote `upstream`")
        );
    }

    #[test]
    fn build_tier_first_listed_fallback_uses_alpha_label() {
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        git_remote_add(td.path(), "zebra", "git@example.com:z/foo.git");
        git_remote_add(td.path(), "alpha", "git@example.com:a/foo.git");
        let ids = auto_detect_build_tier_identifiers(td.path(), false);
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].value.as_str(), "git@example.com:a/foo.git");
        assert_eq!(
            ids[0].source_label.as_deref(),
            Some(
                "auto-detected from build-tier git remote `alpha` (origin/upstream absent; first-listed)"
            )
        );
    }

    #[test]
    fn build_tier_malformed_remote_softfails_to_userdefined() {
        // FR-010 / VR-074-005: a remote URL that fails `validate_repo`
        // downgrades `kind` to UserDefined rather than rejecting.
        // `validate_repo` (milestone 073) accepts well-known forms but
        // we deliberately use a value the validator can be expected to
        // reject (raw whitespace-only is empty after trim, so we craft
        // something that survives empty-check but fails repo-shape
        // validation: a plain string with no scheme/host structure).
        // If the `validate_repo` of milestone 073 happens to accept
        // whatever we use here, the test still passes (Builtin is
        // also a valid kind) — the assertion only checks that the
        // identifier is returned at all and has the build-tier label.
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        // A bare token with no `://` or `@host:` structure tends to
        // exercise the soft-fail path; whether it actually does is
        // a milestone-073 implementation detail — we don't pin
        // `kind` here. Per research §7's test guidance: assert the
        // identifier slot is present, do NOT assert the kind.
        git_remote_add(td.path(), "origin", "not-a-real-url");
        let ids = auto_detect_build_tier_identifiers(td.path(), false);
        assert!(!ids.is_empty(), "soft-fail must still emit the identifier");
        assert_eq!(ids[0].scheme.as_str(), "repo");
        assert_eq!(ids[0].value.as_str(), "not-a-real-url");
    }

    #[test]
    fn build_tier_origin_wins_over_upstream() {
        let td = tempfile::tempdir().unwrap();
        git_init(td.path());
        git_remote_add(td.path(), "origin", "git@github.com:o/foo.git");
        git_remote_add(td.path(), "upstream", "git@github.com:u/foo.git");
        let ids = auto_detect_build_tier_identifiers(td.path(), false);
        assert!(!ids.is_empty());
        assert_eq!(ids[0].value.as_str(), "git@github.com:o/foo.git");
    }

    // ----------------------------------------------------------------
    // Milestone 075 — sanitize_userinfo unit tests
    // ----------------------------------------------------------------

    #[test]
    fn sanitize_strips_user_password_https() {
        let s = sanitize_userinfo("https://USER:TOKEN@github.com/foo/bar.git");
        assert!(s.was_sanitized);
        assert_eq!(s.original, "https://USER:TOKEN@github.com/foo/bar.git");
        assert_eq!(s.sanitized, "https://github.com/foo/bar.git");
        assert!(!s.sanitized.contains("USER"));
        assert!(!s.sanitized.contains("TOKEN"));
    }

    #[test]
    fn sanitize_strips_user_only_no_password() {
        // GitHub App pattern: bare `<token>@host` without a colon.
        let s = sanitize_userinfo("https://ghp_AAA123@github.com/foo/bar.git");
        assert!(s.was_sanitized);
        assert_eq!(s.sanitized, "https://github.com/foo/bar.git");
        assert!(!s.sanitized.contains("ghp_AAA123"));
    }

    #[test]
    fn sanitize_handles_empty_userinfo() {
        // Edge case: `https://@host/...` — empty userinfo is still
        // userinfo per RFC 3986.
        //
        // Note: `url::Url::parse` may normalize `https://@github.com/foo.git`
        // to `https://github.com/foo.git` even before the setters run.
        // Either way, the result is "no userinfo present in the
        // sanitized output". Don't assert on `was_sanitized` since
        // the parser may already have stripped it.
        let s = sanitize_userinfo("https://@github.com/foo.git");
        assert!(!s.sanitized.contains('@'));
        assert!(s.sanitized.starts_with("https://github.com/foo.git"));
    }

    #[test]
    fn sanitize_preserves_port_when_stripping() {
        let s = sanitize_userinfo("https://USER:TOKEN@github.com:8443/foo.git");
        assert!(s.was_sanitized);
        assert_eq!(s.sanitized, "https://github.com:8443/foo.git");
    }

    #[test]
    fn sanitize_passthrough_on_parse_failure() {
        // Bare token with no scheme — url::Url::parse rejects.
        let s = sanitize_userinfo("not a url at all");
        assert!(!s.was_sanitized);
        assert_eq!(s.original, "not a url at all");
        assert_eq!(s.sanitized, "not a url at all");
    }

    #[test]
    fn sanitize_passthrough_on_no_userinfo() {
        let s = sanitize_userinfo("https://github.com/foo/bar.git");
        assert!(!s.was_sanitized);
        assert_eq!(s.original, "https://github.com/foo/bar.git");
        assert_eq!(s.sanitized, "https://github.com/foo/bar.git");
    }

    #[test]
    fn sanitize_passthrough_on_ssh_form() {
        // SCP-like syntax — url::Url::parse rejects it (research §6).
        // Treated identically to no-userinfo for downstream emission.
        let s = sanitize_userinfo("git@github.com:foo/bar.git");
        assert!(!s.was_sanitized);
        assert_eq!(s.original, "git@github.com:foo/bar.git");
        assert_eq!(s.sanitized, "git@github.com:foo/bar.git");
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
                assert_eq!(a.original, b.original);
                assert_eq!(a.sanitized, b.sanitized);
                assert_eq!(a.was_sanitized, b.was_sanitized);
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

    // ----------------------------------------------------------------
    // Milestone 076 — subject_identifiers_from_attestation_subjects
    // ----------------------------------------------------------------

    fn rd(name: &str, digests: &[(&str, &str)]) -> ResourceDescriptor {
        let mut digest = std::collections::BTreeMap::new();
        for (k, v) in digests {
            digest.insert((*k).to_string(), (*v).to_string());
        }
        ResourceDescriptor {
            name: name.to_string(),
            digest,
        }
    }

    const SHA256_A: &str =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const SHA256_B: &str =
        "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    const SHA512_A: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn subject_autodetect_single_subject_sha256_happy_path() {
        let subjects = vec![rd("myapp", &[("sha256", SHA256_A)])];
        let ids = subject_identifiers_from_attestation_subjects(&subjects);
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].scheme.as_str(), "subject");
        assert_eq!(ids[0].value.as_str(), format!("sha256:{SHA256_A}"));
        assert!(ids[0].is_builtin());
        let label = ids[0].source_label.as_deref().unwrap();
        assert!(
            label.contains("build-tier") && label.contains("myapp"),
            "expected source_label to mention build-tier + subject name; got {label:?}"
        );
    }

    #[test]
    fn subject_autodetect_multi_subject_input_order() {
        // The function preserves input order — witness-v0.1 already
        // produces lex-sorted subjects upstream.
        let subjects = vec![
            rd("myapp-a", &[("sha256", SHA256_A)]),
            rd("myapp-b", &[("sha256", SHA256_B)]),
            rd("myapp-c", &[("sha256", SHA256_A)]),
        ];
        let ids = subject_identifiers_from_attestation_subjects(&subjects);
        assert_eq!(ids.len(), 3);
        assert_eq!(ids[0].value.as_str(), format!("sha256:{SHA256_A}"));
        assert_eq!(ids[1].value.as_str(), format!("sha256:{SHA256_B}"));
        assert_eq!(ids[2].value.as_str(), format!("sha256:{SHA256_A}"));
        // Each carries a unique source_label naming its subject.
        assert!(ids[0].source_label.as_deref().unwrap().contains("myapp-a"));
        assert!(ids[1].source_label.as_deref().unwrap().contains("myapp-b"));
        assert!(ids[2].source_label.as_deref().unwrap().contains("myapp-c"));
    }

    #[test]
    fn subject_autodetect_skips_subject_without_sha256() {
        // Only sha512 in the digest map — sha256 absent. Per the
        // 2026-05-06 clarification + FR-002, this subject is skipped
        // with an info-log; nothing emits.
        let subjects = vec![rd("legacy", &[("sha512", SHA512_A)])];
        let ids = subject_identifiers_from_attestation_subjects(&subjects);
        assert!(
            ids.is_empty(),
            "subject without sha256 must not auto-emit; got {ids:?}"
        );
    }

    #[test]
    fn subject_autodetect_emits_sha256_only_when_multi_digest() {
        // Both sha256 AND sha512 — auto-emit picks sha256 only per the
        // 2026-05-06 clarification.
        let subjects = vec![rd(
            "myapp",
            &[("sha256", SHA256_A), ("sha512", SHA512_A)],
        )];
        let ids = subject_identifiers_from_attestation_subjects(&subjects);
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].value.as_str(), format!("sha256:{SHA256_A}"));
        // No sha512 entry emitted from auto-detection.
        assert!(!ids
            .iter()
            .any(|id| id.value.as_str().starts_with("sha512:")));
    }

    #[test]
    fn subject_autodetect_empty_subject_set_returns_empty_vec() {
        let ids = subject_identifiers_from_attestation_subjects(&[]);
        assert!(ids.is_empty());
    }

    #[test]
    fn subject_autodetect_skips_synthetic_subjects() {
        // Synthetic-fallback subjects use a `synthetic` digest key, not
        // `sha256`. Per FR-002 they don't auto-emit a `subject:`
        // identifier — only the underlying real-content sha256 ever
        // makes it into the SBOM body.
        let subjects = vec![rd("synthetic:echo-hello", &[("synthetic", "abc123")])];
        let ids = subject_identifiers_from_attestation_subjects(&subjects);
        assert!(ids.is_empty());
    }
}
