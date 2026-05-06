# Data Model — milestone 075 credential stripping

The milestone introduces zero new public types. One small private return-shape struct (`SanitizedUrl`) lives in `auto_detect.rs`; one boolean parameter threads through the call chain. Both compose existing milestone-073/074 types.

## Entities

### `SanitizedUrl` (new, private to `auto_detect.rs`)

Return shape of the new `sanitize_userinfo` helper.

```rust
struct SanitizedUrl {
    /// The URL as returned by `git remote get-url`, unchanged.
    /// Preserved so callers can still log the host/path for
    /// operator debuggability.
    original: String,
    /// The URL with RFC 3986 userinfo removed. Equal to `original`
    /// when no userinfo was present, when the URL fails to parse,
    /// or when the operator passed `--keep-credentials-in-identifiers`.
    sanitized: String,
    /// True iff `sanitize_userinfo` actually removed userinfo.
    /// Drives log-line emission and `source_label` augmentation.
    was_sanitized: bool,
}
```

Lifetime: ephemeral — created by `sanitize_userinfo`, consumed immediately by the caller, never stored.

### `CredentialOptOut` (conceptual; materialized as a `bool`)

The boolean state flowing from the new `--keep-credentials-in-identifiers` flag through `ScanArgs` / `RunArgs` into the auto-detect call sites. When `true`, the sanitize function is bypassed and the original URL is used verbatim. Default `false`.

## Functions (public surface added by this milestone)

### `sanitize_userinfo` (new, public to the `auto_detect` module)

```rust
pub fn sanitize_userinfo(url: &str) -> SanitizedUrl;
```

**Behavior**:
1. Call `url::Url::parse(url)`. On `Err`, return `SanitizedUrl { original: url.to_string(), sanitized: url.to_string(), was_sanitized: false }` (passthrough; covers SSH-form URLs and other non-RFC-3986 inputs per research §6).
2. On parse success, check whether the parsed URL has userinfo (`url.username().is_empty() == false || url.password().is_some()`). If neither is present, return passthrough.
3. If userinfo is present, call `url.set_username("")` and `url.set_password(None)`. On `Err` from either setter (cannot-be-base path), return passthrough (preserves the FR-009 soft-fail rule).
4. On both setters succeeding, return `SanitizedUrl { original: url.to_string(), sanitized: url_modified.to_string(), was_sanitized: true }`.

**Error model**: never panics, never returns `Result`. All failure modes (parse error, setter error, cannot-be-base URLs) collapse to passthrough — the original string emits verbatim.

### Updated signatures (existing functions, opt-out param added)

```rust
// Source-tier (was: scan_root → Option<Identifier>)
pub fn auto_detect_repo_identifier(
    scan_root: &Path,
    keep_credentials: bool,  // NEW
) -> Option<Identifier>;

// Build-tier (was: invocation_cwd → Vec<Identifier>)
pub fn auto_detect_build_tier_identifiers(
    invocation_cwd: &Path,
    keep_credentials: bool,  // NEW
) -> Vec<Identifier>;
```

The new `keep_credentials` parameter is forwarded to internal logic that decides whether to call `sanitize_userinfo` on the discovered URL. When `true`, `sanitize_userinfo` is bypassed.

Existing callers in `scan_cmd.rs` and `run.rs` get a one-line update each: pass `args.keep_credentials_in_identifiers` instead of nothing.

## Validation rules

- **VR-075-001**: `sanitize_userinfo` MUST never panic. All error paths collapse to passthrough.
- **VR-075-002**: `sanitize_userinfo` MUST be deterministic — same input → byte-identical `SanitizedUrl` content (FR-010).
- **VR-075-003**: When `was_sanitized == true`, the caller MUST emit exactly one info-level log line and append `(credentials stripped)` to the constructed identifier's `source_label`. When `was_sanitized == false`, the caller MUST NOT log and MUST NOT augment.
- **VR-075-004**: When `keep_credentials == true`, `sanitize_userinfo` MUST NOT be called. The original URL flows through unchanged. The caller MUST emit one info-level log line acknowledging the suppression (FR-007).
- **VR-075-005**: The `git:` identifier value `git:<url>#<sha>` MUST have `<url>` produced from `sanitize_userinfo(url).sanitized` (when not opted out), then the `#<sha>` appended. Sanitization happens on the URL part only — not on the SHA.

## Relationships

```text
mikebom sbom scan / mikebom trace run
    │
    ├── ScanArgs.keep_credentials_in_identifiers : bool
    │   RunArgs.keep_credentials_in_identifiers  : bool
    │
    └── auto_detect_repo_identifier(scan_root, keep_credentials)
        auto_detect_build_tier_identifiers(invocation_cwd, keep_credentials)
            │
            ├── discover_repo_url(scan_root) → Option<(url, remote_name, fallback_used)>
            │
            ├── if !keep_credentials: sanitize_userinfo(&url) → SanitizedUrl
            │   else:                 SanitizedUrl { original=url, sanitized=url, was_sanitized=false }
            │
            ├── If was_sanitized: tracing::info!("sanitized userinfo from `<scheme>`; URL: <userinfo redacted>@<host><path>")
            │   If keep_credentials: tracing::info!("--keep-credentials-in-identifiers set; userinfo preserved verbatim")
            │
            └── Identifier::from_parts_with_label(scheme, value=sanitized_value, kind, source_label_with_suffix)
```

## Backward compatibility

- No `Cargo.toml` deps removed; `url = "2"` promoted from transitive to direct (lockfile churn: zero).
- Existing CLI surface is unchanged for operators who don't pass the new flag. Default behavior changes from "emit verbatim" to "strip credentials" — this is the intended security fix and the only operator-visible change.
- Operators who relied on credentials being preserved (rare, internal-network case) get a one-flag opt-out path: `--keep-credentials-in-identifiers`. Their workflow continues unchanged once they pass the flag.
- Existing milestone-073/074 byte-identity goldens stay byte-identical because, per milestone 074's T001 audit, no fixture has a credentialed remote (FR-012 + SC-008).
- Cross-tier correlation contract (milestones 072–074) is preserved because the sanitized URL is the canonical correlation key — downstream tools matching SBOMs by `repo:` value get identical canonical forms regardless of which side sanitized.
