# Contract — milestone 075 credential stripping

The milestone's only contract. The change at the CLI level is "one new boolean opt-out flag on `mikebom sbom scan` and `mikebom trace run`, default false." The change at the library level is "one new `sanitize_userinfo` function plus a new boolean param on the two existing auto-detect entry points."

## CLI surface

### New flag

```
--keep-credentials-in-identifiers
```

- Available on: `mikebom sbom scan`, `mikebom trace run`
- Type: boolean (presence enables; absence default false)
- Effect: when set, the FR-001/FR-002 sanitization step is suppressed and auto-detected URLs emit verbatim

### Help-text shape (clap-derived)

```
--keep-credentials-in-identifiers
        Preserve userinfo (e.g., `USER:TOKEN@host`) in auto-detected git
        remote URLs when constructing `repo:` and `git:` identifiers.
        By default, mikebom strips userinfo to prevent accidental
        credential disclosure in published SBOMs. Use this flag only
        when the credentials are deliberately non-sensitive (e.g., a
        public read-only deploy token, internal-network-only
        credentials). Manual `--repo` / `--git-ref` / `--id` flag
        values are emitted verbatim regardless of this flag.
```

## Library surface (`mikebom-cli` crate)

### New public function

```rust
// In mikebom-cli/src/binding/identifiers/auto_detect.rs

/// Strip RFC 3986 userinfo from a URL before it gets embedded as an
/// identifier value in a published SBOM.
///
/// Returns a `SanitizedUrl` carrying the original, the sanitized
/// form, and a flag indicating whether userinfo was actually
/// removed. Callers use the flag to drive log-line emission and
/// `source_label` augmentation.
///
/// Never panics. Never returns `Result`. All failure modes
/// (parse error, cannot-be-base URLs, missing authority) collapse
/// to a passthrough `SanitizedUrl` where `sanitized == original`
/// and `was_sanitized == false`.
pub fn sanitize_userinfo(url: &str) -> SanitizedUrl;
```

### Updated public signatures

```rust
// (was: pub fn auto_detect_repo_identifier(scan_root: &Path) -> Option<Identifier>)
pub fn auto_detect_repo_identifier(
    scan_root: &Path,
    keep_credentials: bool,
) -> Option<Identifier>;

// (was: pub fn auto_detect_build_tier_identifiers(invocation_cwd: &Path) -> Vec<Identifier>)
pub fn auto_detect_build_tier_identifiers(
    invocation_cwd: &Path,
    keep_credentials: bool,
) -> Vec<Identifier>;
```

Both signatures gain a single trailing `keep_credentials: bool` parameter. The two production call sites (in `scan_cmd.rs` and `run.rs`) pass `args.keep_credentials_in_identifiers` directly. No other callers exist (verified by grep at planning time).

### Internal data shape

```rust
// Private to auto_detect.rs; not exported.
struct SanitizedUrl {
    original: String,
    sanitized: String,
    was_sanitized: bool,
}
```

## Integration boundary

### Where `sanitize_userinfo` is called

```rust
// Inside discover_repo_url's caller (the source-tier wrapper):
let raw_url = git_remote_get_url(scan_root, name)?;
let sanitized = if keep_credentials {
    SanitizedUrl { original: raw_url.clone(), sanitized: raw_url.clone(), was_sanitized: false }
} else {
    sanitize_userinfo(&raw_url)
};

if sanitized.was_sanitized {
    tracing::info!(
        scheme = "repo",
        // host + path only; userinfo redacted in the log
        url_safe = %redact_userinfo_for_log(&sanitized.original),
        "sanitized userinfo from auto-detected identifier"
    );
}

let label = build_source_label(remote_name, fallback_used, sanitized.was_sanitized);
Identifier::from_parts_with_label(scheme, value_of(&sanitized.sanitized), kind, Some(label))
```

The `redact_userinfo_for_log` helper is private and produces a string like `https://<userinfo redacted>@github.com/foo.git` — host and path preserved, secret portion replaced.

The build-tier flow at `auto_detect_build_tier_identifiers` mirrors this exactly for both the `repo:` and `git:` identifier construction.

### Opt-out logging

```rust
if keep_credentials {
    tracing::info!(
        "--keep-credentials-in-identifiers set; userinfo in auto-detected identifiers will be preserved verbatim"
    );
}
```

Emitted once per scan invocation, not per identifier. Placed at the top of the auto-detect entry point so it appears before any identifier construction.

## Observable contract from outside the binary

### Default behavior (sanitization on)

```
$ git remote get-url origin
https://x-access-token:ghs_AAA123@github.com/acme/private-repo.git

$ mikebom sbom scan --path . --output out.cdx.json
INFO sanitized userinfo from auto-detected identifier; scheme=`repo`; url_safe=`https://<userinfo redacted>@github.com/acme/private-repo.git`
... (rest of scan flow unchanged)

$ jq '.metadata.component.externalReferences[] | select(.type == "vcs") | .url' out.cdx.json
"https://github.com/acme/private-repo.git"

$ jq '.metadata.component.externalReferences[] | select(.type == "vcs") | .comment' out.cdx.json
"auto-detected from git remote `origin` (credentials stripped)"

$ grep -c "ghs_AAA123" out.cdx.json
0
```

### Opt-out behavior (sanitization off)

```
$ mikebom sbom scan --path . --keep-credentials-in-identifiers --output out.cdx.json
INFO --keep-credentials-in-identifiers set; userinfo in auto-detected identifiers will be preserved verbatim
... (no per-identifier sanitization log lines)

$ jq '.metadata.component.externalReferences[] | select(.type == "vcs") | .url' out.cdx.json
"https://x-access-token:ghs_AAA123@github.com/acme/private-repo.git"

$ jq '.metadata.component.externalReferences[] | select(.type == "vcs") | .comment' out.cdx.json
"auto-detected from git remote `origin`"
```

### SSH-form passthrough

```
$ git remote get-url origin
git@github.com:acme/foo.git

$ mikebom sbom scan --path . --output out.cdx.json
... (no sanitization log line — Url::parse rejects SSH form, sanitize_userinfo returns passthrough)

$ jq '.metadata.component.externalReferences[] | select(.type == "vcs") | .url' out.cdx.json
"git@github.com:acme/foo.git"

$ jq '.metadata.component.externalReferences[] | select(.type == "vcs") | .comment' out.cdx.json
"auto-detected from git remote `origin`"
```

## Test contract (extends `mikebom-cli/tests/identifiers_*` patterns)

A new integration-test file `mikebom-cli/tests/identifiers_credential_strip.rs` MUST cover:

| Test | Acceptance scenario | Validates |
|------|--------------------|-----------|
| `source_tier_strips_credentials_from_https_origin` | US1 §1, SC-001 | FR-001 |
| `source_tier_source_label_carries_stripped_suffix` | US1 §2, SC-006 | FR-008 |
| `source_tier_emits_redacted_info_log` | US1 §3 | FR-006 |
| `source_tier_ssh_form_unchanged` | US1 §4, SC-007 | FR-003 (SSH passthrough) |
| `build_tier_strips_credentials_from_repo_and_git` | US2 §1, SC-002 | FR-001 + FR-002 |
| `manual_repo_emits_verbatim_with_credentials` | US3 §1, SC-003 | FR-004 |
| `manual_repo_overrides_strip_with_credentials_in_value` | US3 §2 | FR-004 + 074's manual-wins |
| `keep_credentials_flag_preserves_userinfo_source_tier` | US4 §1, SC-004 | FR-005 + FR-007 |
| `keep_credentials_flag_preserves_userinfo_build_tier` | US4 §2 | FR-005 + FR-002 |
| `parse_failure_falls_through_to_user_defined` | edge case | FR-009 |

Plus unit tests for `sanitize_userinfo` directly:

| Test | Validates |
|------|-----------|
| `sanitize_strips_user_password_https` | FR-001 base case |
| `sanitize_strips_user_only_no_password` | edge case (token-only userinfo) |
| `sanitize_handles_empty_userinfo` | edge case (`https://@host/...`) |
| `sanitize_preserves_port_when_stripping` | FR-001 + port |
| `sanitize_passthrough_on_parse_failure` | FR-009 + research §6 |
| `sanitize_passthrough_on_no_userinfo` | no-op identity |
| `sanitize_passthrough_on_ssh_form` | research §6 |

Tests use the project's standard `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention.

## Performance contract (per SC-005)

- One `Url::parse` per auto-detected URL. Bounded by `url` crate's parser (microseconds).
- Two setter calls when userinfo is present. Bounded by string allocation cost.
- Total: well under 1ms per identifier on a typical input.
- No new subprocess invocations. No new I/O.

## Determinism contract (per FR-010)

- Same input URL → byte-identical `SanitizedUrl` content. Verified by unit test running the same input 100× and asserting identical results.
- Sanitization happens once per `(scheme, value)` at construction time and is cached in the `Identifier` value. Per-format emission consumes the cached value without re-running sanitization.
- The opt-out flag does not change the sanitization function — it changes whether the function is called. Determinism holds in both paths.
