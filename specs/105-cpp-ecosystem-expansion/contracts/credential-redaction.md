# Contract: URL credential redaction (FR-016)

**Maps to**: FR-016 | **Helper**: `sanitize_userinfo` (existing milestone-075 code; promoted to public during this milestone — see research.md R4)

## Helper signature (post-refactor)

```rust
/// Strips `user:password@` from `https://` URLs and normalizes
/// `ssh://`-with-credential URLs. Returns the original string unchanged
/// when no credentials are present. Logs `tracing::warn!` when sanitization
/// occurred (operator-actionable; the manifest likely has secrets in it).
pub fn sanitize_userinfo(url: &str) -> Cow<'_, str> {
    // ...
}
```

Location: `mikebom-cli/src/identifiers/sanitize.rs` (moved from
`mikebom-cli/src/binding/identifiers/auto_detect.rs` by the
refactor step in research.md R4).

## Invocation points (every new reader)

| Reader | Invocation site | Field(s) sanitized |
|---|---|---|
| `cpm-cmake` | `cpm_cmake::derive_purl` | `GIT_REPOSITORY` URL |
| `west` | `west::resolve_remote_url` | `remotes[].url-base` + final composed URL |
| `idf-component` | `idf_component::resolve_git_dep` | git-dep `url:` field |
| `git-submodule` | `git_submodule::parse_entry` | `.gitmodules` `url` field |
| Existing readers (no change) | — | already handle their own URL sources |

## Audit gate

A new test `tests/credential_redaction_audit.rs` greps every emitted SBOM
component in the golden fixture suite for known credential patterns
(`https://[^/]*:[^@]*@`, `ssh://[^@]*@[^/]*`). Any match fails the test —
acts as a regression-prevention safety net independent of per-reader unit
tests.

## Logging

Per FR-016, the redaction log is at `tracing::warn!` level (the existing
helper emits `tracing::info!`; milestone 105 bumps it to `warn`). The
log message format:

```
{"timestamp": "...", "level": "WARN", "target": "mikebom::identifiers::sanitize",
 "message": "stripped credentials from URL",
 "manifest_file": "/path/to/west.yml",
 "url_redacted": "https://***@github.com/org/repo.git"}
```

The redacted form (`https://***@<host>/<path>`) keeps host + path visible so
operators can locate the offending manifest while never echoing the secret.
